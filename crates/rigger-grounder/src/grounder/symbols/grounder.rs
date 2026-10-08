//! The `symbols` grounder (spec 15, unit 4): the `Grounder` port over the persisted symbol
//! index, serving the PRECISE grounding contract (architecture 5.5.6). It ranks a DEFINITION
//! whose name matches the query above a mere REFERENCE, above an incidental prose mention
//! (which is not indexed as a symbol at all, so it never appears). Selection is wired in
//! `grounder_for` / `main::select_grounder`; this module is the consumer of units 1-3's model,
//! extraction, registry, and store.

use crate::grounder::symbols::model::{percentile_cutoff, Lang, SymbolIndex};
use crate::grounder::symbols::{build_index, reindex_files, store};
use crate::grounder::{BlastRadius, Grep, Grounder, RankedRef, Ref};
use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::path::Path;
use std::sync::Mutex;

/// The percentile of the tree-wide name-ambiguity distribution above which a name counts as
/// AMBIGUOUS for the ranked-by-intent page (spec 92 criterion 3): drawn from the repo's OWN
/// distribution, not an absolute constant a monorepo would blow past, so only the top decile of
/// most-defined names is down-weighted.
const AMBIGUITY_PERCENTILE: f64 = 0.90;

/// The `symbols` grounder over the persisted index. `open` loads the persisted index (building and
/// persisting it on a cold start); `ground` ranks name matches by the precise contract; `reindex`
/// re-parses ONLY the files whose content actually changed, keyed on the line-ending-normalized
/// [`store::content_hash`] (the freshening gate). The in-memory index is behind a `Mutex` so the
/// grounder is `Send + Sync` (the trait bound) and concurrent `ground`s serialize their read.
pub struct Symbols {
    /// The project root the index was built over - the same root `store::save`/`load` key on, and
    /// the root `reindex` resolves each relative file against.
    root: String,
    /// An explicit `--language` override applied to every file, or `None` to auto-detect by
    /// extension (threaded into `index_one_file`, exactly as `build_index` does).
    override_lang: Option<Lang>,
    /// The in-memory index served by `ground` and freshened by `reindex`.
    idx: Mutex<SymbolIndex>,
    /// Per-file content fingerprints (`rel_path -> content_hash`) the reindex freshening gate keys
    /// on to skip a named-but-unchanged file. Seeded lazily (empty at `open`): a file's first
    /// reindex always re-parses (fingerprint absent -> treated as changed, the safe default), and
    /// a later reindex whose content hashes equal is skipped. Process-local by design - it gates
    /// only redundant work, never correctness, so it need not survive a process (the persisted
    /// index already does).
    fingerprints: Mutex<BTreeMap<String, String>>,
}

impl Symbols {
    /// Open the grounder over `root`: load the persisted index, or - on a cold start - build it
    /// over the tree and persist it so the next process loads instead of rebuilding. A corrupt or
    /// absent artifact loads as `None` and is transparently rebuilt (never a crash).
    pub fn open(root: &str, override_lang: Option<Lang>) -> Symbols {
        let idx = store::load(root).unwrap_or_else(|| {
            let built = build_index(root, override_lang);
            // Best-effort persist: a write failure (e.g. a read-only tree) must not stop grounding
            // from the in-memory index we just built; the next open simply rebuilds.
            let _ = store::save(&built, root);
            built
        });
        Symbols {
            root: root.to_string(),
            override_lang,
            idx: Mutex::new(idx),
            fingerprints: Mutex::new(BTreeMap::new()),
        }
    }
}

/// The reindex freshening gate: given the remembered per-file `fingerprints`, return the subset of
/// `files` whose CURRENT content differs from the remembered [`store::content_hash`] - the files a
/// reindex must actually re-parse - and update `fingerprints` to the fresh hashes. A file whose
/// content is unchanged (including one that changed ONLY its line endings, which `content_hash`
/// normalizes away) is filtered out, so reindex re-parses and re-persists ONLY genuinely-changed
/// files. A file that cannot be read (deleted/unreadable) is returned as changed with its stale
/// fingerprint dropped, so the shared `index_one_file` authority still runs for it and REMOVES its
/// entry - the deleted file's stale symbols are purged, leaving the index equal to a fresh
/// whole-tree build that never visits it.
fn changed_files(
    root: &str,
    files: &[String],
    fingerprints: &mut BTreeMap<String, String>,
) -> Vec<String> {
    let mut changed = Vec::new();
    for rel in files {
        let abs = Path::new(root).join(rel);
        match std::fs::read_to_string(&abs) {
            Ok(src) => {
                let hash = store::content_hash(&src);
                if fingerprints.get(rel) != Some(&hash) {
                    fingerprints.insert(rel.clone(), hash);
                    changed.push(rel.clone());
                }
            }
            Err(_) => {
                fingerprints.remove(rel);
                changed.push(rel.clone());
            }
        }
    }
    changed
}

/// The query's symbol-candidate terms: the alphanumeric/underscore runs of at least TWO Unicode
/// characters, so `apply_damage` stays ONE term and single-character noise is dropped. The filter
/// counts CHARACTERS (`chars().count()`), not bytes, so a single multibyte alphanumeric character
/// (an accented letter, a CJK ideograph) is dropped exactly like an ASCII single char rather than
/// surviving on its 2-3 byte length. This is the ONE authority both [`Symbols::ground`] and
/// [`Symbols::blast_radius`] extract terms with, so the two can never disagree on what a query
/// means - a query that grounds to nothing (no terms) also has an empty blast radius. Keeping the
/// extraction shared is exactly what lets `blast_radius` short-circuit to the empty fail-safe on a
/// degenerate query (a one-character or all-punctuation query whose every token is dropped by the
/// character-count filter) BEFORE it ever runs grep, rather than falling through to an unbounded
/// whole-repo grep.
fn query_terms(query: &str) -> Vec<&str> {
    query
        .split(|c: char| !c.is_alphanumeric() && c != '_')
        // Count CHARACTERS, not bytes: `t.len()` is the byte length, so a single MULTIBYTE
        // alphanumeric char (an accented letter, a CJK ideograph, the micro sign) is 2-3 bytes
        // and a `len() >= 2` filter would keep it as a term - letting a degenerate single-char
        // query slip past the empty-terms fail-safe and fall through to an uncapped whole-repo
        // grep. `chars().count() >= 2` drops EVERY one-character token uniformly, ASCII or not.
        .filter(|t| t.chars().count() >= 2)
        .collect()
}

/// The query terms a criterion's BLAST RADIUS grounds on: the code it names, never its prose.
/// Each whitespace-separated piece of a code span (backticked text) is a term, and so is every
/// bare token that is identifier-shaped - it contains `_` or `::`, or is camelCase / CamelCase.
/// A span like `a::b` or `path/file.rs` stays ONE term. A plain prose word is never a term, so a
/// criterion that names no code has none. A term needs two characters, one alphanumeric.
fn code_terms(query: &str) -> Vec<String> {
    let mut terms: Vec<String> = Vec::new();
    for (i, part) in query.split('`').enumerate() {
        let in_span = i % 2 == 1;
        for word in part.split_whitespace() {
            let term = if in_span {
                word.trim_end_matches("()")
                    .trim_matches(|c: char| matches!(c, ',' | ';' | '.' | '(' | ')' | '&' | '*'))
            } else {
                word.trim_matches(|c: char| !(c.is_alphanumeric() || c == '_'))
            };
            let keep = term.chars().count() >= 2
                && term.chars().any(char::is_alphanumeric)
                && (in_span || identifier_shaped(term));
            if keep && !terms.iter().any(|t| t == term) {
                terms.push(term.to_string());
            }
        }
    }
    terms
}

/// Whether a bare prose token reads as code: it contains `_` or `::`, or mixes case with an
/// uppercase letter past its first character (`camelCase`, `CamelCase`) - never an ALL-CAPS or
/// Capitalized prose word.
fn identifier_shaped(token: &str) -> bool {
    token.contains('_')
        || token.contains("::")
        || (token.chars().any(char::is_lowercase) && token.chars().skip(1).any(char::is_uppercase))
}

/// Every distinct `(name, language)` pair's occurrence count across the WHOLE index, counting
/// DEFINITIONS always and REFERENCES only when `count_refs` is set - the ONE counting pass
/// [`commonness_map`] and [`ambiguity_map`] both share (spec 92 criterion 3 remediation,
/// arch-u92c3-cutoff-formula-duplicated-not-shared's DRY finding applied to this pair too: two
/// near-identical scan loops over the same data, one function body now covers both). Keyed by
/// `(name, Lang)`, never a bare name (spec 92 criterion 3 remediation round 5,
/// adj-u92c3-r4-verdict-reject / adv-u92c3r4-ambiguity-map-bleeds-across-languages): a bare-name
/// key sums an entity's popularity/ambiguity across every language sharing that name, exactly
/// the cross-language collision [`SymbolIndex::reference_degree`] already guards against (5.5.2) - a `run` over-defined in Python must never inflate the same
/// bare name's Rust count, in either direction.
fn name_occurrence_map(idx: &SymbolIndex, count_refs: bool) -> BTreeMap<(&str, Lang), usize> {
    let mut counts: BTreeMap<(&str, Lang), usize> = BTreeMap::new();
    for fs in idx.files().values() {
        for d in &fs.defs {
            *counts.entry((d.name.as_str(), fs.lang)).or_insert(0) += 1;
        }
        if count_refs {
            for r in &fs.refs {
                *counts.entry((r.name.as_str(), fs.lang)).or_insert(0) += 1;
            }
        }
    }
    counts
}

/// Every distinct `(name, language)` pair's total DEFINITION+REFERENCE occurrence count (spec 92
/// criterion 3, RANKED BY INTENT): the inverse-document-frequency proxy Design names - "a token
/// in hundreds of files - `run`, `new`, `tests` - carries near-zero weight." The ONE authority
/// [`scored_hits`] draws a matched ENTITY's own commonness from (looked up by the entity's OWN
/// resolved `(name, Lang)` pair, never the raw query term - a CONTAINS-tier term is by
/// definition a substring and so is almost never itself a key here), for ranking (rarer wins a
/// tie). NOT the signal [`Symbols::has_strong_match`] gates on - a popular but UNAMBIGUOUS
/// entity (one definition, many call sites, e.g. `criterion_stable_id`) must rank low here (it
/// is genuinely the specific thing a caller meant when they typed its exact name) without being
/// misread as "too common to be a confident match" - that second, distinct question is
/// [`ambiguity_map`]'s (spec 92 criterion 3 remediation, adj-u92c3-verdict-reject: the two were
/// wrongly conflated by an earlier round of this unit).
fn commonness_map(idx: &SymbolIndex) -> BTreeMap<(&str, Lang), usize> {
    name_occurrence_map(idx, true)
}

/// Every distinct `(name, language)` pair's DISTINCT-DEFINITION count (spec 92 criterion 3
/// remediation, adj-u92c3-verdict-reject): the genuine tree-wide AMBIGUITY signal - "how many
/// different things could this name mean, WITHIN this language" - independent of how often any
/// ONE of those definitions is called. A name defined exactly once (within its own language) is
/// entirely unambiguous no matter how many places reference it (`criterion_stable_id`: 1
/// definition, 37 references in this very repo, must read as a confident match); a name defined
/// many times over in unrelated places (`run`, `new`, `parse`) is genuinely ambiguous regardless
/// of reference volume. This is the ONE authority [`Symbols::has_strong_match`] gates its cutoff
/// on - never [`commonness_map`]'s raw def+ref occurrence volume, which conflates one entity's
/// own popularity with tree-wide name ambiguity (the defect this map exists to fix). Its cutoff
/// is [`crate::grounder::symbols::model::percentile_cutoff`].
fn ambiguity_map(idx: &SymbolIndex) -> BTreeMap<(&str, Lang), usize> {
    name_occurrence_map(idx, false)
}

/// Whether a location's match is a DEFINITION or a REFERENCE of the query - the existing
/// lexical tier ([`ScoredHit::lexical`]: 3 for a definition, 2 for a reference) is derived from
/// this, and [`Symbols::ground_ranked`] uses it to decide how a hit's entity should be keyed
/// (a definition is its own entity by construction; a reference is absorbed into its sole
/// definer, when unambiguous).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum HitKind {
    Def,
    Ref,
}

/// One (file, line) location scored against the query's terms (spec 92 criterion 3, the
/// scorer): `tier` - EXACT (2, a term equals the entity's name) beats CONTAINS (1, a term
/// merely occurs within a longer name), the Design decision "a query token that equals an
/// entity's name beats a token that merely occurs in it"; `commonness` - the MATCHED ENTITY's
/// OWN tree-wide occurrence count from [`commonness_map`], looked up by the entity's OWN
/// resolved name rather than whichever query term matched it (ascending: rarer ranks first, the
/// inverse-document-frequency ordering, without needing floating point - spec 92 criterion 3
/// remediation round 2, adv-u92c3r2-contains-tier-commonness-collapses-to-spurious-zero: a
/// CONTAINS-tier term is by definition a substring, so keying this off the term itself missed
/// `commonness_map` for nearly every CONTAINS hit and silently handed it the artificial rarest
/// score); `lexical` - the EXISTING definition-over-reference tier, the Design's final
/// tiebreaker ("then by the existing lexical score").
///
/// Gap 104 puts three keys ahead of that order, counting DISTINCT query terms case-insensitively.
/// `name_coverage` - the terms the entity's name holds - ranks first, so a query that names an
/// entity by its shape (`impl EventStore for` -> the impl blocks named `impl EventStore for
/// Store`) finds it above a name that holds one incidental word (`for`). `test` - the location is
/// test code ([`Def::is_test`]/[`SymRef::is_test`], or a file under a `tests` directory) - ranks
/// next, so a test double never outranks the product code it stands in for. `coverage` - the
/// terms the name OR the file path holds - ranks third, so the words that name the module narrow
/// the match (`sqlite store open` -> `open_connection` in `store/sqlite.rs` above `reopen_window`
/// elsewhere).
///
/// [`Def::is_test`]: crate::grounder::symbols::model::Def::is_test
/// [`SymRef::is_test`]: crate::grounder::symbols::model::SymRef::is_test
struct ScoredHit<'a> {
    name_coverage: usize,
    test: bool,
    coverage: usize,
    tier: u8,
    commonness: usize,
    lexical: u8,
    file: &'a str,
    line: u32,
    name: &'a str,
    lang: Lang,
    kind: HitKind,
}

impl ScoredHit<'_> {
    /// The `(name, Lang)` key that identifies this hit's matched entity within
    /// [`commonness_map`]/[`ambiguity_map`]/`def_sites` - the ONE pairing every cross-language
    /// scoping lookup in this module uses, so a same-named entity in an unrelated language can
    /// never be looked up, absorbed into, or pooled with this one (spec 92 criterion 3
    /// remediation round 5, adj-u92c3-r4-verdict-reject).
    fn entity(&self) -> (&str, Lang) {
        (self.name, self.lang)
    }

    /// The ONE ranking order, best first: name coverage, then product before test code, then
    /// name-and-path coverage, then tier, then rarest commonness, then definition over reference,
    /// then file and line (a total, deterministic order). Both the per-location collapse and the
    /// final sort in [`scored_hits`] compare by it, so the two can never disagree.
    fn rank(&self) -> impl Ord + '_ {
        (
            std::cmp::Reverse(self.name_coverage),
            self.test,
            std::cmp::Reverse(self.coverage),
            std::cmp::Reverse(self.tier),
            self.commonness,
            std::cmp::Reverse(self.lexical),
            self.file,
            self.line,
        )
    }
}

/// Score every (file, line) definition/reference location in `idx` against `terms`, ranked by
/// [`ScoredHit::rank`] - the ONE scored, sorted pass both [`Symbols::ground`] and
/// [`Symbols::ground_ranked`] consume, so the two views can never disagree on ranking. A
/// location that matches more than one term (or both an EXACT and a CONTAINS candidate) keeps
/// its BEST tier - commonness is a property of the matched entity's own name, so it never varies
/// by which term matched - mirroring the def-and-ref-at-one-line collapse the prior single-pass
/// scorer already made.
fn scored_hits<'a>(idx: &'a SymbolIndex, terms: &[&str]) -> Vec<ScoredHit<'a>> {
    let commonness = commonness_map(idx);
    let lower_terms: Vec<String> = terms.iter().map(|t| t.to_lowercase()).collect();
    let mut best: BTreeMap<(&'a str, u32), ScoredHit<'a>> = BTreeMap::new();
    for (path, fs) in idx.files() {
        let lower_path = path.to_lowercase();
        let test_file = crate::grounder::symbols::events::is_under_tests_dir(path);
        let mut score_one = |name: &'a str, line: u32, kind: HitKind, lexical: u8, test: bool| {
            // The best TIER any query term gives this name: an EXACT match (some term equals the
            // name) always wins over a CONTAINS match (some term merely occurs within it).
            // `commonness` is the MATCHED ENTITY's own tree-wide occurrence count - `(name,
            // fs.lang)`, never the raw query term `t` - so it is the same value regardless of
            // which term matched. A CONTAINS-tier term is by definition a substring, so it is
            // almost never itself an indexed name; keying the lookup on `t` instead of `name`
            // (round-2 defect, adv-u92c3r2-contains-tier-commonness-collapses-to-spurious-zero)
            // silently missed `commonness_map` for nearly every CONTAINS hit and handed it the
            // artificial rarest score (`unwrap_or(0)`), drowning a genuinely rare entity under
            // unrelated ones that merely share a common substring. Keying on `name` ALONE,
            // ignoring `fs.lang` (round-4 defect, adv-u92c3r4-ambiguity-map-bleeds-across-
            // languages), pooled a same-named entity's commonness across every language sharing
            // it; `(name, fs.lang)` scopes the lookup to this hit's OWN language, exactly as
            // `SymbolIndex::reference_degree` already scopes the fan-out signal (5.5.2).
            let mut hit_tier = 0u8;
            for t in terms {
                let tier = if name == *t {
                    2u8
                } else if name.contains(t) {
                    1u8
                } else {
                    0u8
                };
                hit_tier = hit_tier.max(tier);
            }
            if hit_tier == 0 {
                return;
            }
            let lower_name = name.to_lowercase();
            let in_name = |t: &&String| lower_name.contains(t.as_str());
            let hit = ScoredHit {
                name_coverage: lower_terms.iter().filter(in_name).count(),
                test: test || test_file,
                coverage: lower_terms
                    .iter()
                    .filter(|t| in_name(t) || lower_path.contains(t.as_str()))
                    .count(),
                tier: hit_tier,
                commonness: commonness.get(&(name, fs.lang)).copied().unwrap_or(0),
                lexical,
                file: path.as_str(),
                line,
                name,
                lang: fs.lang,
                kind,
            };
            // One row per (file, line): the better-ranked candidate keeps the slot - an impl
            // block's definition, matching every word of an impl-shaped query, over the trait
            // reference on its own header line.
            match best.entry((path.as_str(), line)) {
                std::collections::btree_map::Entry::Occupied(mut slot) => {
                    if hit.rank() < slot.get().rank() {
                        slot.insert(hit);
                    }
                }
                std::collections::btree_map::Entry::Vacant(slot) => {
                    slot.insert(hit);
                }
            }
        };
        for d in &fs.defs {
            score_one(d.name.as_str(), d.line, HitKind::Def, 3, d.is_test);
        }
        for r in &fs.refs {
            score_one(r.name.as_str(), r.line, HitKind::Ref, 2, r.is_test);
        }
    }
    let mut hits: Vec<ScoredHit<'a>> = best.into_values().collect();
    hits.sort_by(|a, b| a.rank().cmp(&b.rank()));
    hits
}

impl Grounder for Symbols {
    fn ground(&self, query: &str, k: usize) -> Vec<Ref> {
        if query.is_empty() || k == 0 {
            return Vec::new();
        }
        // The query's alphanumeric/underscore terms are the symbol candidates (so `apply_damage`
        // stays one term). Single-character terms are dropped as noise. `blast_radius` extracts
        // terms through the SAME `query_terms` authority, so the two views agree on emptiness.
        let terms = query_terms(query);
        if terms.is_empty() {
            return Vec::new();
        }
        let idx = self.idx.lock().unwrap();
        // Ranked by intent (spec 92 criterion 3): exact-name match first, then the rarest
        // matching token, then definition-over-reference - NOT deduplicated by entity (that is
        // `ground_ranked`'s page-shape concern), so this stays the SAME row-per-location
        // contract `grounded_seed` (conductor.rs) relies on for prompt-seed file diversity.
        scored_hits(&idx, &terms)
            .into_iter()
            .take(k)
            .map(|h| Ref {
                file: h.file.to_string(),
                line: h.line,
                text: h.name.to_string(),
            })
            .collect()
    }

    fn reindex(&self, _src_dir: &str, files: &[String]) {
        // Decide which named files ACTUALLY changed (the content-hash freshening gate). Persist only
        // when something changed, so a reindex of unchanged files touches neither the parser nor the
        // disk. The fingerprints are process-local and gate only redundant work, so this check is
        // deliberately OUTSIDE the write lock; `index_one_file` re-reads each file at parse time.
        let changed = {
            let mut fps = self.fingerprints.lock().unwrap();
            changed_files(&self.root, files, &mut fps)
        };
        if changed.is_empty() {
            return;
        }
        // ONE `idx` lock across the whole freshen, and INSIDE `store::reindex_under_lock` the
        // cross-process write lock across reload -> apply -> persist - the single mutation
        // authority. The reload (under the held lock) folds in any change a concurrent `rigger
        // reindex` process persisted since this grounder loaded its in-memory copy, so our stale
        // snapshot cannot clobber that write (a cross-process lost update). We then re-parse ONLY
        // the changed files through the shared `reindex_files` authority - never a second
        // extraction path - on top of the reloaded base, and it publishes atomically.
        let mut idx = self.idx.lock().unwrap();
        let _ = store::reindex_under_lock(&mut idx, &self.root, |base| {
            reindex_files(&self.root, base, &changed, self.override_lang);
        });
    }

    /// The two-view blast radius over the cross-reference graph (architecture 5.5.1, spec 16 unit
    /// 1) - the `symbols` override of the grep-only trait default.
    ///
    /// The query is a criterion, and its terms are the CODE it names ([`code_terms`]: its code
    /// spans and identifier-shaped tokens), never its prose words - a prose word that happens to
    /// be a symbol name would pull that symbol's whole neighborhood in:
    ///
    /// - `precise` (the grounding contract) is the STRUCTURAL view - the files that DEFINE the
    ///   queried symbols (and any file a path-like term names) ranked ABOVE the files that
    ///   REFERENCE them - capped at `k`. It is what seeds
    ///   an agent's prompt, so it favors precision.
    /// - `safe` (the safety contract) is the UNION of the structural view and grep, UNCAPPED. It
    ///   runs BOTH engines - the structural graph AND the EXISTING [`Grep`] grounder over the same
    ///   root, once per term - so it is never narrower than the grep radius of what the criterion
    ///   names (5.5.9). Name-level linking MISSES
    ///   references (macros, dynamic dispatch, re-exports, a mention the tags query never indexes as
    ///   a symbol); the grep union recovers them, so the partitioning consumer can never
    ///   under-partition.
    /// - A HUB term (a name referenced across much of the tree) fails SAFE through `safe` itself:
    ///   every file that defines or references it is in the view, never truncated, so the
    ///   partitioning consumer's overlap test keeps the unit apart from exactly the units that
    ///   share one of those files - and pairs it with every unit that shares none.
    ///
    /// Determinism is by construction: `files()` is a `BTreeMap`, so both structural passes visit
    /// files in sorted path order (the ranked `precise` / structural head of `safe`), and the
    /// grep-recovered tail of `safe` is explicitly SORTED before it is appended - grep itself walks
    /// the tree in unsorted `read_dir` order, so without the sort `safe` would be set-deterministic
    /// but not order-deterministic. Sorting the tail makes the whole `safe` ordering reproducible
    /// across processes, which is what unit 3 needs to hash the seed-file list into a stable
    /// `BlastRadiusComputed` audit event. A query naming no code - empty, prose only, a single
    /// character (ASCII or multibyte) or all punctuation - returns empty views (the empty-radius
    /// fail-safe the scheduler runs alone and unit 3 routes to the full panel) WITHOUT running a
    /// whole-repo grep, never a partial or a panic. A query WITH code terms that simply matches
    /// nothing is ALSO empty, but that case does run the uncapped grep per term - it just comes
    /// back empty.
    fn blast_radius(&self, query: &str, k: usize) -> BlastRadius {
        // The query's terms are the CODE it names ([`code_terms`]), never its prose: a criterion's
        // prose words that happen to be symbol names (`tests`, `run`, `parse`) would otherwise pull
        // every file defining or referencing them into the radius.
        let terms = code_terms(query);
        // The empty-terms fail-safe, applied BEFORE any index read or text search: a query naming
        // no code (an empty query, prose only, a single character, all punctuation) grounds to the
        // empty radius - empty precise and empty safe - which the scheduler runs alone and unit 3
        // routes to the full, unpartitioned panel.
        if terms.is_empty() {
            return BlastRadius::default();
        }

        // The STRUCTURAL view, ranked (definer files, then referencer files not already a definer),
        // computed under ONE read lock over the index.
        let structural: Vec<String> = {
            let idx = self.idx.lock().unwrap();
            // Iterate `files()` directly to KEEP each hit's owning file.
            // `files()` is a BTreeMap, so this is sorted-path-order and deterministic.
            // A path-like term (`dir/file.rs`) names a FILE, which counts as a definer; any other
            // term names a symbol by its last `::` segment (`grounder::tree_bytes` -> `tree_bytes`).
            let (paths, names): (Vec<&str>, Vec<&str>) = terms
                .iter()
                .map(String::as_str)
                .partition(|t| t.contains('/') || t.contains('.'));
            let names: Vec<&str> = names
                .iter()
                .map(|t| t.rsplit("::").next().unwrap_or(t))
                .collect();
            let mut definers: Vec<&str> = Vec::new();
            let mut referencers: Vec<&str> = Vec::new();
            for (path, fs) in idx.files() {
                let named = paths
                    .iter()
                    .any(|t| path == t || path.ends_with(&format!("/{t}")));
                if named || fs.defs.iter().any(|d| names.contains(&d.name.as_str())) {
                    definers.push(path.as_str());
                }
                if fs.refs.iter().any(|r| names.contains(&r.name.as_str())) {
                    referencers.push(path.as_str());
                }
            }
            // Ranked: every definer file first, then each referencer that is not also a definer.
            let mut ranked: Vec<String> = Vec::new();
            for f in &definers {
                ranked.push((*f).to_string());
            }
            for f in &referencers {
                if !definers.contains(f) {
                    ranked.push((*f).to_string());
                }
            }
            ranked
        };

        // The SAFE-SUPERSET view: the FULL (untruncated) structural set UNIONed with an UNCAPPED
        // grep for each term over the same root - the honest "both engines" cost (5.5.9). This clone happens
        // BEFORE `precise` is capped, so the safe view is never bounded by `k`; do not reorder the
        // truncation above it or the uncapped-superset contract breaks. `usize::MAX` makes grep
        // collect every matching file, not a top-`k` slice. A `seen` set keeps the dedup O(lines)
        // rather than O(lines * files): grep yields one hit per matching LINE and the safe view is
        // uncapped, so a per-file linear scan would be quadratic on a wide radius.
        let mut safe = structural.clone();
        let mut seen: HashSet<String> = safe.iter().cloned().collect();
        let grep = Grep {
            root: self.root.clone(),
        };
        // The grep-recovered tail: the files grep matched that the structural view missed. Grep
        // walks the tree in unsorted `read_dir` order, so collect the tail and SORT it before
        // appending - the structural head is already `BTreeMap`-sorted, so this makes the whole
        // `safe` ordering reproducible across processes (unit 3 hashes the seed-file list into a
        // `BlastRadiusComputed` event that must be cross-process byte-identical). Sorting a set of
        // distinct paths (not the raw grep hits) keeps this O(tail log tail), not per-line.
        let mut grep_tail: Vec<String> = Vec::new();
        for r in terms.iter().flat_map(|t| grep.ground(t, usize::MAX)) {
            if seen.insert(r.file.clone()) {
                grep_tail.push(r.file);
            }
        }
        grep_tail.sort();
        safe.extend(grep_tail);

        // The precise view is the ranked structural set capped at `k`; the safe view stays uncapped.
        let mut precise = structural;
        precise.truncate(k);
        BlastRadius { precise, safe }
    }

    /// The provenance stamp for unit 3's `BlastRadiusComputed` audit event: the content-hash of
    /// the CURRENT in-memory index unioned with the grammar / tag-query version, as
    /// `<index-content-hash>/<grammar-tags-version>`. The `symbols` grounder is STRUCTURAL, so it
    /// returns a NON-EMPTY stamp - the signal unit 3's conductor keys the audit + retention metric
    /// off (a grep / nop grounder inherits the empty default and emits neither). The
    /// hash is over the COMPACT `serde_json::to_string` of the `BTreeMap`-backed index. Because the
    /// index is `BTreeMap`-backed it serializes in sorted-key order, so this is DETERMINISTIC: the
    /// SAME index state yields the SAME stamp across processes, and a reindex that changes the graph
    /// changes the stamp - which is exactly what makes a recorded radius reconstruct which index
    /// generation grounded it. (This is a DIFFERENT byte stream than `store::save`, which pretty-
    /// prints via `to_vec_pretty`; both are deterministic, but the stamp is not the on-disk bytes -
    /// it need only be stable across processes for the same index, which the compact form is.)
    fn index_stamp(&self) -> String {
        let idx = self.idx.lock().unwrap();
        let serialized = serde_json::to_string(&*idx).unwrap_or_default();
        format!(
            "{}/{}",
            store::content_hash(&serialized),
            crate::grounder::symbols::model::GRAMMAR_TAGS_VERSION
        )
    }

    /// The RANKED-BY-INTENT page (spec 92 criterion 3): [`scored_hits`]'s already-ranked list,
    /// deduplicated to one row per ENTITY - "six call sites of one function occupy one row with
    /// its degree." A DEFINITION is its own entity, keyed by `(name, file, line)` - always
    /// unique by construction, so two DIFFERENT definitions sharing a name (the constraints
    /// walk: "a name defined in two files - `ground` returns both entities as separate rows")
    /// never collapse into one, even when they share a file (six same-named methods on six
    /// structs in one file stay six rows). A REFERENCE is absorbed into its SOLE definer's row
    /// when the name has EXACTLY one definition anywhere in the tree; a name with ZERO or
    /// AMBIGUOUS (multiple) definitions is never guessed at - its references stand alone as
    /// their own (unattributed) entity rather than being pinned to one of several candidates.
    /// `degree` is the real [`SymbolIndex::reference_degree`] for the entity's name and
    /// language, not an approximate
    /// count of what happened to be visible in this page - EXCEPT on an ambiguous name's own
    /// Def row, which reports 0 rather than the tree-wide count (spec 92 criterion 3
    /// remediation round 6, adj-u92c3-r5-verdict-reject /
    /// adv-u92c3r5-ambiguous-definition-degree-inflated-and-triplicated): that count is exactly
    /// the unattributed reference pool the Standalone row exists to carry, so a Def row that
    /// cannot be pinned to any given reference must never claim it too - see the degree
    /// computation below for the full reasoning.
    fn ground_ranked(&self, query: &str, k: usize) -> Vec<RankedRef> {
        if query.is_empty() || k == 0 {
            return Vec::new();
        }
        let terms = query_terms(query);
        if terms.is_empty() {
            return Vec::new();
        }
        let idx = self.idx.lock().unwrap();
        let hits = scored_hits(&idx, &terms);

        // Every matched (name, language) pair's definition sites, so a REFERENCE hit can look up
        // whether it has a SOLE, SAME-LANGUAGE definer to absorb into. Built over the WHOLE
        // index (not just the hits), since a name's definition is only a "hit" itself when it
        // also matches a term - the absorption question ("how many definitions does this name
        // have, within this language") is independent of that. Keyed by `(name, Lang)`, never a
        // bare name (spec 92 criterion 3 remediation round 5, adj-u92c3-r4-verdict-reject /
        // adv-u92c3r4-ground-ranked-def-sites-cross-language-absorption): a bare-name key let a
        // Rust reference with NO Rust definition (e.g. a `.unwrap()` call site, matching only
        // `SymRef::name`) resolve through a same-named definition in an unrelated language (a JS
        // `unwrap` helper) as if it were that entity's own sole definer - silently absorbing and
        // deduplicating the entire cross-language reference population into ONE foreign row,
        // erasing rather than down-weighting the Design's own poster-child tree-wide-common case.
        let mut def_sites: BTreeMap<(&str, Lang), Vec<(&str, u32)>> = BTreeMap::new();
        for (path, fs) in idx.files() {
            for d in &fs.defs {
                def_sites
                    .entry((d.name.as_str(), fs.lang))
                    .or_default()
                    .push((path.as_str(), d.line));
            }
        }

        #[derive(PartialEq, Eq, Hash, Clone)]
        enum EntityKey<'a> {
            /// A specific definition site - always a distinct entity.
            Def(&'a str, &'a str, u32),
            /// A `(name, Lang)` with zero or ambiguous (multiple SAME-LANGUAGE) definitions -
            /// its references stand alone, keyed by name AND language (never a bare name: a
            /// standalone Rust `unwrap` and a standalone Python `unwrap` are different entities,
            /// there is only ever one such row per `(name, Lang)` pair).
            Standalone(&'a str, Lang),
        }

        let mut seen: HashSet<EntityKey> = HashSet::new();
        let mut out: Vec<RankedRef> = Vec::new();
        for h in hits {
            // How many SAME-LANGUAGE definitions this hit's own name has - independent of
            // which hit kind matched, since a Def hit's own definition is always counted in
            // `def_sites` and a Ref hit's sole-definer lookup below uses the identical key.
            let def_count = def_sites.get(&h.entity()).map_or(0, Vec::len);
            let key = match h.kind {
                HitKind::Def => EntityKey::Def(h.name, h.file, h.line),
                HitKind::Ref => match def_sites.get(&h.entity()).map(Vec::as_slice) {
                    Some([(file, line)]) => EntityKey::Def(h.name, file, *line),
                    _ => EntityKey::Standalone(h.name, h.lang),
                },
            };
            if !seen.insert(key.clone()) {
                continue;
            }
            // An AMBIGUOUS (more than one same-language definition) entity's own Def row
            // reports an attributable degree of 0, never the tree-wide reference count (spec
            // 92 criterion 3 remediation round 6, adj-u92c3-r5-verdict-reject /
            // adv-u92c3r5-ambiguous-definition-degree-inflated-and-triplicated): the
            // Standalone row for this same `(name, Lang)` exists SPECIFICALLY because a
            // reference cannot be pinned to any one of its several candidates, so a Def row
            // must never turn around and claim that identical unattributable pool as its own
            // - the opposite of what creating the Standalone row already decided. An
            // UNAMBIGUOUS (exactly one definer) Def row keeps the real
            // `reference_degree` - every reference to that name genuinely IS this entity's
            // own, nothing is pooled. A Standalone row always keeps the real
            // `reference_degree` too - it IS the pooled, unattributed reference count, by
            // construction of why it exists at all.
            let degree = match key {
                EntityKey::Def(..) if def_count > 1 => 0,
                _ => idx.reference_degree(h.name, h.lang),
            };
            out.push(RankedRef {
                loc: Ref {
                    file: h.file.to_string(),
                    line: h.line,
                    text: h.name.to_string(),
                },
                degree,
            });
            if out.len() >= k {
                break;
            }
        }
        out
    }

    /// Whether `query` has at least one match whose MATCHED ENTITY sits AT OR BELOW the repo's
    /// own name-ambiguity distribution's [`AMBIGUITY_PERCENTILE`] cutoff (spec 92 criterion 3:
    /// "a query with no strong token returns the honest 'no entity matches strongly' line
    /// instead of noise"). Two fixes over the first round of this unit (spec 92 criterion 3
    /// remediation, adj-u92c3-verdict-reject):
    ///
    /// 1. Judges the SAME hits [`scored_hits`] (the actual ranker `ground`/`ground_ranked` use)
    ///    produces, rather than re-deriving a second raw `counts.get(term)` lookup keyed on the
    ///    QUERY TERM. A CONTAINS-tier term (`"damage"` matching `apply_damage`) is never itself
    ///    an indexed name, so that second lookup always missed and silently read as "not
    ///    strong" - an absent-key sentinel inversion against `scored_hits`' own
    ///    `unwrap_or(0)` = rarest convention. Judging `scored_hits`' resolved
    ///    [`ScoredHit::name`] (the entity actually matched) instead closes that gap.
    /// 2. Draws its cutoff from [`ambiguity_map`] (distinct-DEFINITION count per `(name, Lang)`)
    ///    rather than [`commonness_map`] (raw def+ref occurrence count): a single-definition,
    ///    heavily-referenced entity (`criterion_stable_id`, `sweep_terminal` - this repo's own
    ///    Done-when audit fixtures) is completely unambiguous and must never be misclassified
    ///    as tree-wide-common merely because it is called often.
    /// 3. Scopes BOTH the per-hit ambiguity lookup AND the cutoff distribution itself by the
    ///    hit's OWN language (spec 92 criterion 3 remediation round 5,
    ///    adj-u92c3-r4-verdict-reject / adv-u92c3r4-ambiguity-map-bleeds-across-languages),
    ///    as [`SymbolIndex::reference_degree`] scopes its count: a name defined
    ///    once in Rust and, separately, once in an unrelated language is genuinely unambiguous
    ///    in EACH language alone; pooling the two definition counts into one bare-name bucket
    ///    manufactures a tree-wide-ambiguous verdict neither language's own distribution
    ///    supports.
    ///
    /// A query with no matches at all is not strong either - it has no candidate to be
    /// confident about.
    fn has_strong_match(&self, query: &str, _k: usize) -> bool {
        let terms = query_terms(query);
        if terms.is_empty() {
            return false;
        }
        let idx = self.idx.lock().unwrap();
        let hits = scored_hits(&idx, &terms);
        if hits.is_empty() {
            return false;
        }
        let ambiguity = ambiguity_map(&idx);
        // The ambiguity cutoff, drawn SEPARATELY per language from that language's OWN
        // distinct-definition distribution - never one pooled cutoff across every language
        // present (5.5.2). A language absent from
        // `ambiguity` (no definition anywhere in it) falls back to 0: nothing has EVER been
        // observed as ambiguous there, so every hit in that language trivially clears the
        // cutoff, matching `ambiguity.get(..).unwrap_or(0)` below for every such hit regardless.
        let mut cutoffs: BTreeMap<Lang, usize> = BTreeMap::new();
        for lang in hits.iter().map(|h| h.lang).collect::<BTreeSet<Lang>>() {
            let mut degrees: Vec<usize> = ambiguity
                .iter()
                .filter_map(|(&(_, l), &count)| (l == lang).then_some(count))
                .collect();
            let cutoff = if degrees.is_empty() {
                0
            } else {
                percentile_cutoff(&mut degrees, AMBIGUITY_PERCENTILE)
            };
            cutoffs.insert(lang, cutoff);
        }
        hits.iter().any(|h| {
            let cutoff = cutoffs.get(&h.lang).copied().unwrap_or(0);
            ambiguity.get(&h.entity()).copied().unwrap_or(0) <= cutoff
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::grounder::symbols::model::{Def, FileSymbols, Kind, SymRef};
    use crate::grounder::Grounder;

    /// The `symbols` grounder is STRUCTURAL, so its `index_stamp` (unit 3's audit provenance +
    /// the structural-active signal the conductor keys the `BlastRadiusComputed` audit off) is
    /// NON-EMPTY, shaped `<index-content-hash>/<grammar-tags-version>`, and CHANGES with the
    /// index content - so a recorded radius reconstructs which index generation grounded it.
    #[test]
    fn index_stamp_is_nonempty_and_tracks_index_content() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.rs"), "fn foo() {}\n").unwrap();
        let g = Symbols::open(dir.path().to_str().unwrap(), None);
        let stamp = g.index_stamp();
        assert!(
            !stamp.is_empty(),
            "symbols is structural: the audit stamp must be non-empty so the conductor emits the audit"
        );
        assert!(
            stamp.contains('/')
                && stamp.ends_with(crate::grounder::symbols::model::GRAMMAR_TAGS_VERSION),
            "the stamp is <index-content-hash>/<grammar-tags-version>; got {stamp:?}"
        );
        // A DIFFERENT index (different symbols) yields a different content-hash half, so a radius
        // recorded under one index generation is distinguishable from one under another.
        let dir2 = tempfile::tempdir().unwrap();
        std::fs::write(dir2.path().join("b.rs"), "fn bar() {}\nfn baz() {}\n").unwrap();
        let g2 = Symbols::open(dir2.path().to_str().unwrap(), None);
        assert_ne!(
            g.index_stamp(),
            g2.index_stamp(),
            "distinct index content must yield a distinct provenance stamp"
        );
        // DETERMINISM (the load-bearing provenance/replay property): the SAME tree opened by a
        // SECOND grounder must yield the SAME stamp - the compact `BTreeMap` serialization is
        // sorted-key stable, so a radius replayed in another process reconstructs to the same
        // index generation. A `to_string` -> `to_vec` drift, or a non-deterministic index order,
        // would trip this.
        let dir3 = tempfile::tempdir().unwrap();
        std::fs::write(dir3.path().join("a.rs"), "fn foo() {}\n").unwrap();
        let g3a = Symbols::open(dir3.path().to_str().unwrap(), None);
        let g3b = Symbols::open(dir3.path().to_str().unwrap(), None);
        assert_eq!(
            g3a.index_stamp(),
            g3b.index_stamp(),
            "the same tree opened twice must yield the SAME stamp (cross-process determinism)"
        );
    }

    #[test]
    fn ranks_a_definition_above_an_incidental_prose_mention() {
        let dir = tempfile::tempdir().unwrap();
        // combat.rs DEFINES apply_damage; notes.rs only MENTIONS it in a comment (prose).
        std::fs::write(
            dir.path().join("combat.rs"),
            "fn apply_damage(x: u8) -> u8 { x }\n",
        )
        .unwrap();
        std::fs::write(
            dir.path().join("notes.rs"),
            "// TODO: think about apply_damage someday\nfn unrelated() {}\n",
        )
        .unwrap();
        let g = Symbols::open(dir.path().to_str().unwrap(), None);
        let refs = g.ground("apply_damage", 5);
        assert!(!refs.is_empty(), "the definition must be grounded");
        // The DEFINITION site outranks the prose mention: combat.rs is first, and the prose
        // mention (a comment, not a symbol) never appears at all.
        assert_eq!(refs[0].file, "combat.rs");
        assert!(
            !refs.iter().any(|r| r.file == "notes.rs"),
            "an incidental prose mention is not indexed as a symbol and must not be grounded; got {refs:?}"
        );
    }

    /// Spec 16 unit 1, the criterion-1 recall fixture: the SAFE-SUPERSET view recovers a reference
    /// the name-level structural graph alone MISSES. `apply_damage` is defined in one file, called
    /// (a real symbol reference the graph links) in another, and mentioned ONLY in a COMMENT in a
    /// third - a comment is not a symbol, so the tags query never indexes it and the structural
    /// graph misses that file, but a literal grep matches the substring. `structural ∪ grep`
    /// recovers it, so the safe view is strictly a superset of the structural (precise) view - the
    /// recall the partitioning consumer needs, safe by construction.
    #[test]
    fn safe_superset_recovers_a_grep_only_reference_the_structural_graph_misses() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("combat.rs"),
            "fn apply_damage(x: u8) -> u8 { x }\n",
        )
        .unwrap();
        std::fs::write(
            dir.path().join("caller.rs"),
            "fn go() { apply_damage(1); }\n",
        )
        .unwrap();
        std::fs::write(
            dir.path().join("notes.rs"),
            "// apply_damage is discussed but never called here\n",
        )
        .unwrap();
        let g = Symbols::open(dir.path().to_str().unwrap(), None);

        let br = g.blast_radius("apply_damage", 8);
        // The precise view IS the structural cross-reference graph: the definer and the real call
        // site, ranked - and NOT the comment-only mention (which is no symbol).
        assert!(
            br.precise.contains(&"combat.rs".to_string()),
            "the definer must be in the precise view; got {br:?}"
        );
        assert!(
            br.precise.contains(&"caller.rs".to_string()),
            "the real call site must be in the precise view; got {br:?}"
        );
        assert!(
            !br.precise.contains(&"notes.rs".to_string()),
            "a comment-only mention is not a symbol; the precise view must exclude it; got {br:?}"
        );
        // The definer ranks ABOVE the referencer in the precise view.
        let combat_at = br.precise.iter().position(|f| f == "combat.rs").unwrap();
        let caller_at = br.precise.iter().position(|f| f == "caller.rs").unwrap();
        assert!(
            combat_at < caller_at,
            "the definer must rank above the referencer; got {br:?}"
        );
        // The safe-superset view UNIONs an uncapped grep, so it RECOVERS the comment mention the
        // structural graph missed - the miss the safety contract exists to backstop.
        assert!(
            br.safe.contains(&"notes.rs".to_string()),
            "the safe view must recover the grep-only reference the structural graph misses; got {br:?}"
        );
        // And it is a strict superset of the precise (structural) view.
        for f in &br.precise {
            assert!(
                br.safe.contains(f),
                "safe must be a superset of precise; missing {f} in {br:?}"
            );
        }
    }

    /// A criterion grounds its blast radius on the CODE it names, never on its prose: the query
    /// terms are its code spans plus identifier-shaped bare tokens, so the prose words `tests`,
    /// `run` and `parse` - which happen to be symbol names here - pull in nothing, and the radius
    /// is `reclaim_space`'s alone. A criterion naming no code at all grounds on nothing (the empty
    /// radius, which the scheduler serializes).
    #[test]
    fn blast_radius_grounds_a_criterion_on_its_code_spans_not_its_prose() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("reclaim.rs"), "fn reclaim_space() {}\n").unwrap();
        std::fs::write(
            dir.path().join("caller.rs"),
            "fn go() { reclaim_space(); }\n",
        )
        .unwrap();
        std::fs::write(dir.path().join("tests.rs"), "fn tests() {}\n").unwrap();
        std::fs::write(
            dir.path().join("run.rs"),
            "fn run() { parse(); tests(); }\n",
        )
        .unwrap();
        std::fs::write(dir.path().join("parse.rs"), "fn parse() {}\n").unwrap();
        let g = Symbols::open(dir.path().to_str().unwrap(), None);

        let br = g.blast_radius(
            "a test proves the run reclaims space: `reclaim_space` frees what tests and parse \
             leave behind, asserted in the run's tests",
            8,
        );
        let mut safe = br.safe.clone();
        safe.sort();
        assert_eq!(
            safe,
            vec!["caller.rs".to_string(), "reclaim.rs".to_string()],
            "the radius is the named span's alone, never the prose words'; got {br:?}"
        );

        let prose = g.blast_radius("the run parses its tests and reclaims one of them", 8);
        assert_eq!(
            prose,
            BlastRadius::default(),
            "a criterion naming no code grounds on nothing; got {prose:?}"
        );
    }

    /// A span of several words (`rigger validate`) is searched for as ONE phrase: the radius is the
    /// files holding the phrase, never every file holding one of its words, and a plain word inside
    /// it (`rigger`, `validate`) matches no symbol. A span naming only language keywords names no
    /// code at all.
    #[test]
    fn blast_radius_searches_a_multi_word_span_as_one_phrase_and_never_grounds_on_keywords() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("verb.rs"), "// run rigger validate first\n").unwrap();
        std::fs::write(dir.path().join("word.rs"), "// rigger is everywhere\n").unwrap();
        std::fs::write(
            dir.path().join("check.rs"),
            "pub fn validate(&mut self) {}\n",
        )
        .unwrap();
        let g = Symbols::open(dir.path().to_str().unwrap(), None);

        let br = g.blast_radius("`rigger validate` refuses a drifted spec", 8);
        assert_eq!(
            br.safe,
            vec!["verb.rs".to_string()],
            "a multi-word span grounds on its phrase hits alone; got {br:?}"
        );

        let keywords = g.blast_radius("the method takes `&mut self` and is `pub fn`", 8);
        assert_eq!(
            keywords,
            BlastRadius::default(),
            "language keywords are never terms; got {keywords:?}"
        );
    }

    /// A span keeps the name it carries through generic and call arguments, trailing marks and
    /// member access: `spawn_unit<T>`, `finish(x)`, `parked:` and `store.open()` each match the
    /// symbol they name, and a dotted term is a path only when it ends in a file extension.
    #[test]
    fn blast_radius_reads_the_name_inside_generics_calls_marks_and_member_access() {
        let dir = tempfile::tempdir().unwrap();
        for (file, def) in [
            ("generic.rs", "spawn_unit"),
            ("call.rs", "finish"),
            ("mark.rs", "parked"),
            ("member.rs", "open"),
        ] {
            std::fs::write(dir.path().join(file), format!("fn {def}() {{}}\n")).unwrap();
        }
        let g = Symbols::open(dir.path().to_str().unwrap(), None);
        for (query, file) in [
            ("`spawn_unit<T>`", "generic.rs"),
            ("`finish(x)`", "call.rs"),
            ("`parked:`", "mark.rs"),
            ("`store.open()`", "member.rs"),
            ("`call.rs`", "call.rs"),
        ] {
            let br = g.blast_radius(query, 8);
            assert!(
                br.safe.contains(&file.to_string()),
                "{query} names the code in {file}; got {br:?}"
            );
        }
    }

    /// A HUB symbol (a name referenced across many files) fails SAFE through its radius, never by
    /// truncating it and never by a conflict-with-everything flag: the safe view carries EVERY file
    /// of the hub's neighborhood (even past the `k` cap), so the overlap test keeps it apart from
    /// exactly the units that share one of those files and pairs it with every unit that shares
    /// none.
    #[test]
    fn a_hub_symbol_s_safe_view_carries_its_whole_neighborhood_untruncated() {
        let dir = tempfile::tempdir().unwrap();
        // `spawn` is referenced across many files (a hub).
        std::fs::write(dir.path().join("def.rs"), "fn spawn() {}\n").unwrap();
        let mut expected: Vec<String> = vec!["def.rs".to_string()];
        for i in 0..12 {
            let name = format!("f{i}.rs");
            std::fs::write(dir.path().join(&name), "fn c() { spawn(); }\n").unwrap();
            expected.push(name);
        }
        let g = Symbols::open(dir.path().to_str().unwrap(), None);

        // A SMALL cap proves the safe view is uncapped: there are 13 `spawn` files, more than k=8.
        let br = g.blast_radius("`spawn`", 8);
        for f in &expected {
            assert!(
                br.safe.contains(f),
                "the hub safe view must not truncate {f}; got {br:?}"
            );
        }
        assert!(
            br.safe.len() >= expected.len(),
            "the safe view is uncapped: all {} hub files must be present, not capped at 8; got {} in {br:?}",
            expected.len(),
            br.safe.len()
        );
    }

    /// The blast-radius fail-safe paths (spec 16 unit 1): an empty query and a no-match query each
    /// return EMPTY views (unit 3 routes an empty radius to the full,
    /// unpartitioned panel). A `k=0` cap collapses the PRECISE view to empty, but the SAFE view is
    /// UNCAPPED by design - it still carries the full structural-union-grep radius so the
    /// partitioning consumer can never under-include just because the prompt budget was zero.
    #[test]
    fn blast_radius_empty_and_no_match_are_the_empty_failsafe_and_k0_keeps_safe_uncapped() {
        let dir = tempfile::tempdir().unwrap();
        // A definer and a real referencer of `parse`, so the radius is non-empty for a real query.
        std::fs::write(dir.path().join("def.rs"), "fn parse() {}\n").unwrap();
        std::fs::write(dir.path().join("call.rs"), "fn run() { parse(); }\n").unwrap();
        let g = Symbols::open(dir.path().to_str().unwrap(), None);

        // Empty query -> no terms -> empty views.
        let empty = g.blast_radius("", 8);
        assert!(
            empty.precise.is_empty() && empty.safe.is_empty(),
            "an empty query is the empty fail-safe: both views empty; got {empty:?}"
        );

        // A name that appears nowhere -> nothing structural AND nothing grep -> empty views.
        let none = g.blast_radius("nonexistent_symbol_zzz", 8);
        assert!(
            none.precise.is_empty() && none.safe.is_empty(),
            "a no-match query is the empty fail-safe: both views empty; got {none:?}"
        );

        // k=0 caps the PRECISE view to empty; the SAFE view is uncapped and still carries the radius.
        let k0 = g.blast_radius("`parse`", 0);
        assert!(
            k0.precise.is_empty(),
            "k=0 caps the precise view to empty; got {k0:?}"
        );
        assert!(
            k0.safe.contains(&"def.rs".to_string()) && k0.safe.contains(&"call.rs".to_string()),
            "the safe view is uncapped even at k=0 - it carries the full radius; got {k0:?}"
        );
    }

    /// Spec 17 criterion 2 (plan17-c2-blast-radius-empty-terms-guard): a DEGENERATE query - one
    /// whose only tokens are dropped by the character-count term filter (a single character, ASCII
    /// OR multibyte, or all punctuation) - must yield an EMPTY blast radius, IDENTICAL in emptiness
    /// to `ground` on the same query. The multibyte arm is the teeth: a byte-length filter keeps a
    /// 2-byte character as a term and greps the whole tree, so counting CHARACTERS is what closes
    /// the fail-safe for the full Unicode single-character class.
    /// This is distinct from the empty-QUERY case the sibling test already covers: the
    /// query here is non-empty, but every term is filtered out. Before the guard, `ground`
    /// early-returned on empty terms while `blast_radius` fell through to an UNBOUNDED whole-repo
    /// grep (`grep.ground(query, usize::MAX)`), so `precise` was empty while `safe` covered almost
    /// the entire tree - forcing the full review panel and corrupting the retention metric. The
    /// single shared `query_terms` authority makes the two agree: no terms -> the empty fail-safe
    /// radius, BEFORE grep is ever run.
    #[test]
    fn blast_radius_on_a_degenerate_terms_query_is_empty_not_a_whole_repo_grep() {
        let dir = tempfile::tempdir().unwrap();
        // Every file contains the letter `a` AND the multibyte char U+00E9 (`\u{e9}`, an accented
        // `e`, TWO UTF-8 bytes but ONE Unicode character), so an UNGUARDED single-character grep
        // for either would match every file - the whole-repo blow-up the guard exists to prevent.
        // The `\u{e9}` escape keeps the source ASCII while the runtime string is the real 2-byte
        // character, which is exactly the class the byte-length filter used to miss.
        std::fs::write(dir.path().join("alpha.rs"), "fn parse() {} // \u{e9}\n").unwrap();
        std::fs::write(
            dir.path().join("beta.rs"),
            "fn draw() { parse(); } // \u{e9}\n",
        )
        .unwrap();
        let g = Symbols::open(dir.path().to_str().unwrap(), None);

        // A single-character query: its only token is one CHARACTER, dropped by the char-count term
        // filter, so it has NO symbol terms. `ground` returns nothing...
        assert!(
            g.ground("a", 8).is_empty(),
            "a degenerate single-char query grounds to nothing"
        );
        // ...and `blast_radius` must be the IDENTICAL empty radius - not a whole-repo grep for "a".
        let one_char = g.blast_radius("a", 8);
        assert_eq!(
            one_char,
            BlastRadius::default(),
            "a degenerate single-char query is the empty fail-safe radius (empty precise, empty \
             safe), not an unbounded whole-repo grep; got {one_char:?}"
        );

        // A single MULTIBYTE character (U+00E9: 2 UTF-8 bytes, but still ONE Unicode character).
        // The term filter must drop it by CHARACTER count, not byte length - a byte-length `>= 2`
        // filter would keep this 2-byte token, miss the empty-terms guard, and fall through to the
        // uncapped whole-repo grep (which matches BOTH files here since each contains the char).
        // `ground` returns nothing (no symbol named `\u{e9}`)...
        let multibyte = "\u{e9}";
        assert!(
            g.ground(multibyte, 8).is_empty(),
            "a degenerate single-MULTIBYTE-char query grounds to nothing"
        );
        // ...and `blast_radius` must be the IDENTICAL empty radius, NOT a whole-repo grep for the
        // 2-byte character. This arm is RED under a byte-length filter (safe = both files) and
        // GREEN once the filter counts CHARACTERS.
        let one_multibyte = g.blast_radius(multibyte, 8);
        assert_eq!(
            one_multibyte,
            BlastRadius::default(),
            "a single multibyte char (2 bytes, ONE char) is the empty fail-safe radius, not an \
             unbounded whole-repo grep for the substring; got {one_multibyte:?}"
        );

        // An all-punctuation query splits into only empty/dropped tokens - the same empty radius,
        // again matching `ground`.
        assert!(
            g.ground("!!!", 8).is_empty(),
            "an all-punctuation query grounds to nothing"
        );
        let punct = g.blast_radius("!!!", 8);
        assert_eq!(
            punct,
            BlastRadius::default(),
            "an all-punctuation query is the empty fail-safe radius, not a grep set; got {punct:?}"
        );
    }

    /// Spec 16 unit 1, the cross-language over-inclusion fixture (sdet-u16-1-structural-view-cross
    /// -language): the structural referencer scan iterates `files()` and matches refs BY NAME across
    /// ALL languages, mirroring `ground`'s own cross-language matching. So a query for a Rust symbol
    /// pulls in a Python file that references the same name - the OVER-inclusion (safe) direction,
    /// which is correct by construction (definers/referencers are deliberately cross-language for
    /// grounding recall; only the fan-out HUB verdict is per-language). This pins that a Python
    /// referencer of a Rust-defined name lands in the precise AND safe views, and that safe stays a
    /// superset of precise. Gated behind the `symbols` feature like every test here (real parsing).
    #[test]
    fn structural_view_is_cross_language_a_python_referencer_of_a_rust_symbol_is_included() {
        let dir = tempfile::tempdir().unwrap();
        // Rust DEFINES `render`; Python CALLS `render` (a real symbol reference, in another
        // language) and a comment-only mention grep alone recovers.
        std::fs::write(dir.path().join("view.rs"), "fn render() {}\n").unwrap();
        std::fs::write(dir.path().join("client.py"), "def draw():\n    render()\n").unwrap();
        std::fs::write(
            dir.path().join("notes.py"),
            "# render is described in prose only, never called\n",
        )
        .unwrap();
        let g = Symbols::open(dir.path().to_str().unwrap(), None);

        let br = g.blast_radius("`render`", 8);
        // The Rust definer is present (the precise structural view).
        assert!(
            br.precise.contains(&"view.rs".to_string()),
            "the Rust definer must be in the precise view; got {br:?}"
        );
        // The Python CALL SITE is a cross-language structural referencer, pulled into precise.
        assert!(
            br.precise.contains(&"client.py".to_string()),
            "the cross-language Python referencer must be in the precise structural view (over-inclusion); got {br:?}"
        );
        // The comment-only Python mention is no symbol; grep recovers it into the safe superset.
        assert!(
            br.safe.contains(&"notes.py".to_string()),
            "the safe view must recover the comment-only cross-language grep reference; got {br:?}"
        );
        // Safe is a superset of precise.
        for f in &br.precise {
            assert!(
                br.safe.contains(f),
                "safe must be a superset of precise; missing {f} in {br:?}"
            );
        }
    }

    /// A `Symbols` grounder over a fresh root holding `files` (`(name, source)` pairs); the
    /// root lives as long as the returned dir.
    fn grounder_over(files: &[(&str, &str)]) -> (tempfile::TempDir, Symbols) {
        let dir = tempfile::tempdir().unwrap();
        for (name, source) in files {
            let path = dir.path().join(name);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, source).unwrap();
        }
        let g = Symbols::open(dir.path().to_str().unwrap(), None);
        (dir, g)
    }

    /// The tree behind the gap-104 ranking tests: a trait, two product impls of it, two test doubles
    /// implementing it (one in a `#[cfg(test)]` module, one under a `tests/` directory), the one
    /// store opener every impl calls, an unrelated impl in the trait's own file, and unrelated
    /// names that merely contain a query word (`for`, `open`) in files that sort first.
    fn store_tree() -> (tempfile::TempDir, Symbols) {
        grounder_over(&[
            (
                "domain/eventstore.rs",
                "pub trait EventStore { fn append(&self); }\nstruct Guard;\nimpl Drop for Guard { fn drop(&mut self) {} }\n",
            ),
            (
                "tests/doubles.rs",
                "struct Double;\nimpl EventStore for Double { fn append(&self) {} }\n",
            ),
            (
                "store/sqlite.rs",
                "pub struct Store;\npub fn open_connection() {}\nimpl EventStore for Store { fn append(&self) { open_connection(); } }\n",
            ),
            (
                "store/kurrentdb.rs",
                "pub struct Store;\nimpl EventStore for Store { fn append(&self) { open_connection(); } }\n",
            ),
            (
                "conductor.rs",
                "#[cfg(test)]\nmod tests {\n    struct Fake;\n    impl EventStore for Fake { fn append(&self) {} }\n}\n",
            ),
            (
                "a_console/lib.rs",
                "fn malformed_input_for_op() {}\nfn formatter_for_display() {}\nfn reopen_window() {}\n",
            ),
        ])
    }

    /// The row position of the first ranked entity named `text` in `file`, failing loudly when
    /// the page does not carry it.
    fn position_of(ranked: &[RankedRef], file: &str, text: &str) -> usize {
        ranked
            .iter()
            .position(|r| r.loc.file == file && r.loc.text == text)
            .unwrap_or_else(|| panic!("no row {file}: {text} in {ranked:?}"))
    }

    /// Gap 104: `impl EventStore for` names the impl sites - each impl block ranks above names
    /// that merely contain `for`, and a product impl ranks above a test double.
    #[test]
    fn ground_ranked_puts_the_impl_sites_of_an_impl_shaped_query_above_incidental_word_matches() {
        let (_dir, g) = store_tree();
        let ranked = g.ground_ranked("impl EventStore for", 10);
        let sqlite = position_of(&ranked, "store/sqlite.rs", "impl EventStore for Store");
        let kurrent = position_of(&ranked, "store/kurrentdb.rs", "impl EventStore for Store");
        let double = position_of(&ranked, "conductor.rs", "impl EventStore for Fake");
        let tests_dir_double =
            position_of(&ranked, "tests/doubles.rs", "impl EventStore for Double");
        assert!(
            sqlite.max(kurrent) < double.min(tests_dir_double),
            "a product impl ranks above a test double, in a test module or a tests directory; \
             got {ranked:?}"
        );
        let other_impl = position_of(&ranked, "domain/eventstore.rs", "impl Drop for Guard");
        assert!(
            other_impl > double.max(tests_dir_double),
            "an impl of another trait matches the query's words only through its file path, so \
             it ranks below every impl the query names; got {ranked:?}"
        );
        for incidental in ["malformed_input_for_op", "formatter_for_display"] {
            if let Some(p) = ranked.iter().position(|r| r.loc.text == incidental) {
                assert!(
                    p > double,
                    "{incidental} merely contains `for`, so it ranks below every impl site; got {ranked:?}"
                );
            }
        }
    }

    /// Gap 104: a query naming an entity by its name AND its module path (`sqlite store open`)
    /// finds the entity in that module first, above a rarer name that matches one word alone.
    #[test]
    fn ground_ranked_puts_the_entity_matching_the_most_query_words_across_name_and_path_first() {
        let (_dir, g) = store_tree();
        let ranked = g.ground_ranked("sqlite store open", 3);
        assert_eq!(
            (ranked[0].loc.file.as_str(), ranked[0].loc.text.as_str()),
            ("store/sqlite.rs", "open_connection"),
            "got {ranked:?}"
        );
    }

    #[test]
    fn a_reference_ranks_below_a_definition_of_the_same_name() {
        // def.rs DEFINES parse; call.rs REFERENCES it (a call site is a real symbol reference).
        let (_dir, g) = grounder_over(&[
            ("def.rs", "fn parse() {}\n"),
            ("call.rs", "fn run() { parse(); }\n"),
        ]);
        let refs = g.ground("parse", 5);
        assert_eq!(
            refs[0].file, "def.rs",
            "the definition outranks the reference; got {refs:?}"
        );
    }

    #[test]
    fn a_self_referential_definition_grounds_once_at_its_highest_score() {
        // A recursive `fn parse() { parse(); }` is BOTH a definition and a reference of `parse`,
        // and the def name and the self-call share a line. The location must ground ONCE (at the
        // definition score), not as a duplicate def-row plus ref-row.
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("r.rs"), "fn parse() { parse(); }\n").unwrap();
        let g = Symbols::open(dir.path().to_str().unwrap(), None);
        let refs = g.ground("parse", 8);
        let at_line_1: Vec<_> = refs
            .iter()
            .filter(|r| r.file == "r.rs" && r.line == 1)
            .collect();
        assert_eq!(
            at_line_1.len(),
            1,
            "the def+ref at one location must collapse to a single grounded ref; got {refs:?}"
        );
    }

    /// Spec 92 criterion 3 remediation round 5 (adj-u92c3-r4-verdict-reject,
    /// adv-u92c3r4-ground-ranked-def-sites-cross-language-absorption): `ground_ranked`'s
    /// entity-resolution (`def_sites`) must scope by `(name, Lang)`, never a bare name. JS
    /// defines the ONLY tree-wide definition of `unwrap`; five UNRELATED Rust files each
    /// reference that same bare name (mirroring Rust's own `.unwrap()` call sites, which are
    /// pure references with no local Rust definition). Before this fix, every Rust reference
    /// absorbed into the JS definition's entity (the sole bare-name def site) and collapsed to
    /// ONE row - erasing the entire Rust reference population rather than surfacing it as its
    /// own (unattributed) in-language entity, exactly the live `rigger ground unwrap 50` repro
    /// against this repo's own tree.
    #[test]
    fn ground_ranked_keeps_a_cross_language_reference_as_its_own_in_language_entity_not_absorbed_into_a_foreign_definition(
    ) {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("shim.mjs"), "function unwrap() {}\n").unwrap();
        for i in 0..5 {
            std::fs::write(
                dir.path().join(format!("r{i}.rs")),
                "fn go() { unwrap(); }\n",
            )
            .unwrap();
        }
        let g = Symbols::open(dir.path().to_str().unwrap(), None);

        let ranked = g.ground_ranked("unwrap", 50);
        assert!(
            ranked.iter().any(|r| r.loc.file == "shim.mjs"),
            "the JS definition must still surface its own row; got {ranked:?}"
        );
        let rust_row = ranked
            .iter()
            .find(|r| r.loc.file.ends_with(".rs"))
            .unwrap_or_else(|| {
                panic!(
                    "the Rust reference population must surface its OWN row, never be absorbed \
                 into a same-named foreign-language definition; got {ranked:?}"
                )
            });
        assert_eq!(
            rust_row.degree, 5,
            "the Rust entity's degree must be its own in-language reference count (5), never \
             the JS definition's; got {ranked:?}"
        );
        assert_eq!(
            ranked.len(),
            2,
            "a same-named foreign-language definition must never collapse the Rust reference \
             population into the JS row - exactly 2 rows (the JS def, the Rust standalone \
             entity); got {ranked:?}"
        );
    }

    /// Spec 92 criterion 3 remediation round 5 (adj-u92c3-r4-verdict-reject,
    /// adv-u92c3r4-ambiguity-map-bleeds-across-languages): `ambiguity_map` (and therefore
    /// `has_strong_match`'s cutoff) must scope by `(name, Lang)`, never a bare name.
    /// `collide` is defined EXACTLY ONCE in Rust (genuinely unambiguous within Rust alone) and,
    /// separately, EXACTLY ONCE in Python (also genuinely unambiguous within Python alone).
    /// Pooling the two languages' definition counts into one bare-name bucket manufactures a
    /// tree-wide ambiguity of 2 - an outlier against the nine Rust filler names' baseline of 1
    /// each - so the old bare-name-keyed cutoff wrongly read Rust's own unambiguous `collide` as
    /// tree-wide-common and answered "no entity matches strongly". Scoped per-language, Rust's
    /// own ambiguity distribution is nine names at 1 plus `collide` at 1: no outlier, so
    /// `collide` correctly reads as a confident, unambiguous match.
    #[test]
    fn has_strong_match_scopes_definition_ambiguity_by_language_a_cross_language_pool_never_taints_an_unambiguous_in_language_definition(
    ) {
        let dir = tempfile::tempdir().unwrap();
        let filler: String = (0..9).map(|i| format!("fn filler{i}() {{}}\n")).collect();
        std::fs::write(dir.path().join("filler.rs"), filler).unwrap();
        std::fs::write(dir.path().join("combat.rs"), "fn collide() {}\n").unwrap();
        std::fs::write(dir.path().join("other.py"), "def collide():\n    pass\n").unwrap();
        let g = Symbols::open(dir.path().to_str().unwrap(), None);

        assert!(
            g.has_strong_match("collide", 8),
            "collide has exactly ONE definition within Rust and, separately, exactly one \
             within Python - each language's OWN ambiguity is 1, genuinely unambiguous. \
             Pooling both languages' definition counts into one bare-name bucket must never \
             manufacture a false tree-wide-ambiguous verdict for either language's own \
             genuinely unambiguous entity."
        );
    }

    #[test]
    fn empty_query_or_zero_k_grounds_nothing() {
        let (_dir, g) = grounder_over(&[("a.rs", "fn parse() {}\n")]);
        assert!(g.ground("", 5).is_empty());
        assert!(g.ground("parse", 0).is_empty());
    }

    #[test]
    fn open_persists_the_index_on_a_cold_start() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.rs"), "fn parse() {}\n").unwrap();
        let _g = Symbols::open(dir.path().to_str().unwrap(), None);
        // A cold open builds + persists, so a second process (here, `store::load`) finds it.
        assert!(
            store::load(dir.path().to_str().unwrap()).is_some(),
            "a cold open must persist the index for the next process"
        );
    }

    #[test]
    fn reindex_freshening_gate_skips_a_content_unchanged_file() {
        // The content-hash freshening gate consumes `store::content_hash`: a file whose content is
        // unchanged - INCLUDING a change to line endings only, which the hash normalizes away - is
        // NOT re-parsed. A genuinely different content IS.
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_str().unwrap();
        let rel = "a.rs".to_string();
        std::fs::write(dir.path().join("a.rs"), "fn one() {}\n").unwrap();
        let mut fps = BTreeMap::new();

        // First sight of the file: no fingerprint yet -> it is "changed" (must be parsed).
        assert_eq!(
            changed_files(root, std::slice::from_ref(&rel), &mut fps),
            vec![rel.clone()]
        );
        // Rewrite the SAME logical content with CRLF line endings: content_hash normalizes CRLF to
        // LF, so the fingerprint is unchanged -> the gate SKIPS it (no needless re-parse/persist).
        std::fs::write(dir.path().join("a.rs"), "fn one() {}\r\n").unwrap();
        assert!(
            changed_files(root, std::slice::from_ref(&rel), &mut fps).is_empty(),
            "a line-ending-only change must be skipped by the content-hash gate"
        );
        // A genuine content change bumps the hash -> the gate returns it.
        std::fs::write(dir.path().join("a.rs"), "fn oneprime() {}\n").unwrap();
        assert_eq!(
            changed_files(root, std::slice::from_ref(&rel), &mut fps),
            vec![rel.clone()]
        );
    }

    #[test]
    fn a_concurrent_reindex_from_a_second_grounder_is_not_clobbered() {
        // Two `Symbols` grounders over the SAME root model two long-lived processes: a conductor
        // holding one for a whole `rigger run` while a separate `rigger reindex` process opens
        // another. Each reindexes a DIFFERENT file. `Symbols::open` loads the index ONCE, so the
        // conductor's in-memory copy goes STALE the moment the other process persists. Without
        // reload-under-lock, the conductor's later reindex would write its stale snapshot over the
        // peer's freshly-persisted change - a cross-process LOST UPDATE. Reloading the persisted
        // base under the held write lock before applying its own delta folds the peer's write in,
        // so BOTH changes survive.
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_str().unwrap();
        // Named so the OLD and NEW symbol at each site share no substring relationship (spec 92
        // criterion 3 added CONTAINS-tier matching, so e.g. "one" would still weakly ground to a
        // renamed "oneprime" - a real, intended recall improvement, but it would wrongly pass a
        // "the superseded symbol must be gone" assertion here for the wrong reason).
        std::fs::write(dir.path().join("a.rs"), "fn alpha_orig() {}\n").unwrap();
        std::fs::write(dir.path().join("b.rs"), "fn gamma_orig() {}\n").unwrap();
        // Both open over the SAME initial on-disk index {a: alpha_orig, b: gamma_orig}, each
        // into its own memory.
        let conductor = Symbols::open(root, None);
        let other_process = Symbols::open(root, None);

        // The "other process" reindexes b.rs -> delta_fresh and PERSISTS it. Disk is now {a, b'}.
        std::fs::write(dir.path().join("b.rs"), "fn delta_fresh() {}\n").unwrap();
        other_process.reindex(root, &["b.rs".to_string()]);

        // The conductor still holds the STALE in-memory index {a: alpha_orig, b: gamma_orig}. It
        // reindexes a.rs. Its persist MUST fold in the peer's delta_fresh rather than clobber it
        // back to gamma_orig.
        std::fs::write(dir.path().join("a.rs"), "fn beta_fresh() {}\n").unwrap();
        conductor.reindex(root, &["a.rs".to_string()]);

        // A fresh reader (a third process) sees BOTH changes on disk.
        let reader = Symbols::open(root, None);
        assert!(
            !reader.ground("beta_fresh", 5).is_empty(),
            "the conductor's own reindex must be persisted"
        );
        assert!(
            !reader.ground("delta_fresh", 5).is_empty(),
            "the peer's concurrent reindex must NOT be clobbered by the conductor's stale snapshot"
        );
        assert!(
            reader.ground("gamma_orig", 5).is_empty(),
            "the superseded symbol must be gone"
        );
    }

    #[test]
    fn reindex_replaces_only_a_changed_files_symbols() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_str().unwrap();
        // Named so the OLD and NEW symbol share no substring relationship (spec 92 criterion 3's
        // CONTAINS-tier matching would otherwise still weakly ground "one" to a renamed
        // "oneprime" - correct recall, but the wrong reason for THIS assertion to pass).
        std::fs::write(dir.path().join("a.rs"), "fn alpha_orig() {}\n").unwrap();
        std::fs::write(dir.path().join("b.rs"), "fn gamma_orig() {}\n").unwrap();
        let g = Symbols::open(root, None);
        assert!(!g.ground("alpha_orig", 5).is_empty());

        // Change a.rs, reindex just it: the new symbol grounds, the old one is gone, b.rs stands.
        std::fs::write(dir.path().join("a.rs"), "fn beta_fresh() {}\n").unwrap();
        g.reindex(root, &["a.rs".to_string()]);
        assert!(
            !g.ground("beta_fresh", 5).is_empty(),
            "the freshened symbol must ground"
        );
        assert!(
            g.ground("alpha_orig", 5).is_empty(),
            "the replaced symbol must be gone"
        );
        assert!(
            !g.ground("gamma_orig", 5).is_empty(),
            "the untouched file must stand"
        );
    }

    /// Spec 92 criterion 3 (RANKED BY INTENT), the tier decision: "a query token that equals
    /// an entity's name beats a token that merely occurs in it." `run` EXACTLY names one
    /// definition; `run_all` only CONTAINS the token. Both are equally rare (each defined
    /// exactly once, nowhere else), so tier is the only thing that can decide the order - not
    /// commonness, not the def/ref lexical tier (both hits are definitions).
    #[test]
    fn ground_ranks_an_exact_name_match_above_a_name_that_merely_contains_the_token() {
        // Named so a plain file-order tiebreak (with tier ignored) would put the CONTAINS
        // match first - only real tier precedence can make the exact match win here.
        let (_dir, g) = grounder_over(&[
            ("zzz_exact.rs", "fn run() {}\n"),
            ("aaa_contains.rs", "fn run_all() {}\n"),
        ]);

        let refs = g.ground("run", 5);
        assert_eq!(
            refs[0].file, "zzz_exact.rs",
            "an exact-name match must outrank a name that merely contains the token; got {refs:?}"
        );
        assert_eq!(refs[0].text, "run");
    }

    /// Spec 92 criterion 3, the audit's own failure shape (docs/audit/2026-09-graph-vs-grep.md
    /// question 3: "dash run-awareness API routes" - `ground` returned "six `run` definitions
    /// in conductor.rs", ranking failed). SIX tree-wide `run` definitions must not drown out a
    /// rare, specific `dash` match, even though both share the SAME exact-match tier and the
    /// SAME definition lexical tier - only the inverse-document-frequency-style commonness
    /// weighting can tell them apart.
    #[test]
    fn ground_gives_a_rare_exact_match_priority_over_six_tree_wide_definitions_of_another_term() {
        let dir = tempfile::tempdir().unwrap();
        for i in 0..6 {
            std::fs::write(dir.path().join(format!("r{i}.rs")), "fn run() {}\n").unwrap();
        }
        // Named so a plain file-order tiebreak (with commonness ignored) would put a `run`
        // hit first - only real inverse-document-frequency weighting can make `dash` win.
        std::fs::write(dir.path().join("zzz_dash.rs"), "fn dash() {}\n").unwrap();
        let g = Symbols::open(dir.path().to_str().unwrap(), None);

        let refs = g.ground("dash run", 10);
        assert_eq!(
            refs[0].text, "dash",
            "the rare token must rank above the six tree-wide `run` hits; got {refs:?}"
        );
        assert_eq!(refs[0].file, "zzz_dash.rs");
    }

    /// Spec 92 criterion 3 remediation round 2 (adv-u92c3r2-contains-tier-commonness-collapses
    /// -to-spurious-zero, UPHELD by review): `scored_hits`' CONTAINS-tier commonness must be the
    /// MATCHED ENTITY's OWN tree-wide occurrence count, never the raw query term's. A
    /// CONTAINS-tier term is by definition a substring, so it is almost never itself an indexed
    /// name; keying `commonness_map` on the term (instead of the entity it resolved to) silently
    /// collapsed every CONTAINS hit to the artificial rarest score (0), drowning a genuinely rare
    /// entity under unrelated ones that merely happen to share a common substring.
    ///
    /// `cfg_one`..`cfg_four` are four unrelated entities (each defined once, referenced twice -
    /// own commonness 3) sharing the substring "cfg", which is never itself a standalone name.
    /// `zorble_alone` (defined once, referenced nowhere - own commonness 1) is genuinely rarer.
    /// `zorble_alone`'s definition is placed LAST (the highest line number) in the fixture file
    /// deliberately: under the old bug every CONTAINS hit ties at commonness 0, so the sort falls
    /// through to line order and puts `zorble_alone` LAST, not first - only scoring commonness
    /// off the matched entity's own name can put the genuinely rare one first.
    #[test]
    fn ground_ranked_puts_a_genuinely_rare_contains_tier_entity_above_common_ones_sharing_its_substring(
    ) {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("entities.rs"),
            "fn cfg_one() {}\nfn cfg_two() {}\nfn cfg_three() {}\nfn cfg_four() {}\nfn zorble_alone() {}\n",
        )
        .unwrap();
        std::fs::write(
            dir.path().join("callers.rs"),
            "fn go() {\n    cfg_one();\n    cfg_one();\n    cfg_two();\n    cfg_two();\n    cfg_three();\n    cfg_three();\n    cfg_four();\n    cfg_four();\n}\n",
        )
        .unwrap();
        let g = Symbols::open(dir.path().to_str().unwrap(), None);

        // Neither "cfg" nor "zorble" is ever itself a standalone name in this fixture - every
        // match below is CONTAINS-tier, never EXACT, squarely exercising the scorer's
        // CONTAINS-tier commonness lookup.
        let ranked = g.ground_ranked("cfg zorble", 10);
        assert_eq!(
            ranked.len(),
            5,
            "five distinct entities must each collapse to one row; got {ranked:?}"
        );
        assert_eq!(
            ranked[0].loc.text, "zorble_alone",
            "the genuinely rare entity (own commonness 1) must outrank four entities that share \
             its query substring but are each themselves more common (own commonness 3) - CONTAINS \
             -tier commonness keyed on the raw query term ties all five at an artificial zero and \
             falls through to line order (which would rank zorble_alone LAST, by construction of \
             this fixture); got {ranked:?}"
        );
    }

    /// Spec 92 criterion 3, "the top page is deduplicated by entity, so six call sites of one
    /// function occupy one row with its degree." A definition plus every one of its call
    /// sites (scattered across DIFFERENT files) must collapse to a SINGLE [`RankedRef`],
    /// carrying the real reference degree.
    #[test]
    fn ground_ranked_dedupes_a_functions_many_call_sites_into_one_row_with_its_degree() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("def.rs"), "fn apply_damage() {}\n").unwrap();
        for i in 0..3 {
            std::fs::write(
                dir.path().join(format!("caller{i}.rs")),
                "fn go() { apply_damage(); }\n",
            )
            .unwrap();
        }
        let g = Symbols::open(dir.path().to_str().unwrap(), None);

        let ranked = g.ground_ranked("apply_damage", 5);
        assert_eq!(
            ranked.len(),
            1,
            "the definition and its three call sites must collapse into ONE entity row; got {ranked:?}"
        );
        assert_eq!(ranked[0].loc.file, "def.rs");
        assert_eq!(
            ranked[0].degree, 3,
            "the row's degree must be the reference count across every call site; got {ranked:?}"
        );
    }

    /// Spec 92 constraints walk: "a name defined in two files - `ground` returns both entities
    /// as separate rows." The entity dedup is per DEFINITION, never per bare name - two
    /// distinct `run` definitions must not collapse into one just because they share a name.
    #[test]
    fn ground_ranked_keeps_two_definitions_of_the_same_name_as_separate_rows() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.rs"), "fn run() {}\n").unwrap();
        std::fs::write(dir.path().join("b.rs"), "fn run() {}\n").unwrap();
        let g = Symbols::open(dir.path().to_str().unwrap(), None);

        let ranked = g.ground_ranked("run", 5);
        let files: Vec<&str> = ranked.iter().map(|r| r.loc.file.as_str()).collect();
        assert!(
            files.contains(&"a.rs") && files.contains(&"b.rs"),
            "two distinct definitions of the same name must both appear as separate rows; got {ranked:?}"
        );
        assert_eq!(
            ranked.len(),
            2,
            "no more than the two real definitions - never collapsed, never duplicated; got {ranked:?}"
        );
    }

    /// Spec 92 criterion 3 remediation round 6 (adj-u92c3-r5-verdict-reject /
    /// adv-u92c3r5-ambiguous-definition-degree-inflated-and-triplicated): an AMBIGUOUS name's
    /// own Def rows must never claim the whole unattributed reference pool as their own
    /// degree. Two same-language definitions of one name, PLUS real call sites, reproduces the
    /// reject's own empirical repro (two Rust defs of `run` + five Rust refs -> three rows,
    /// every row printing the same pooled degree) - here with a fixture name so the assertion
    /// is exact and stable rather than depending on this crate's own real `run` population.
    /// Every prior test either used zero-reference ambiguous defs
    /// (`ground_ranked_keeps_two_definitions_of_the_same_name_as_separate_rows`) or an
    /// unambiguous single def WITH references
    /// (`ground_ranked_dedupes_a_functions_many_call_sites_into_one_row_with_its_degree`) -
    /// never an ambiguous def combined with real references, the exact combination the reject
    /// named as untested.
    #[test]
    fn ground_ranked_gives_ambiguous_definitions_zero_degree_and_pools_the_real_count_on_the_standalone_row(
    ) {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.rs"), "fn dup_name() {}\n").unwrap();
        std::fs::write(dir.path().join("b.rs"), "fn dup_name() {}\n").unwrap();
        for i in 0..5 {
            std::fs::write(
                dir.path().join(format!("caller{i}.rs")),
                "fn go() { dup_name(); }\n",
            )
            .unwrap();
        }
        let g = Symbols::open(dir.path().to_str().unwrap(), None);

        let ranked = g.ground_ranked("dup_name", 10);
        assert_eq!(
            ranked.len(),
            3,
            "two ambiguous Def rows plus one pooled Standalone reference row; got {ranked:?}"
        );
        let def_rows: Vec<_> = ranked
            .iter()
            .filter(|r| r.loc.file == "a.rs" || r.loc.file == "b.rs")
            .collect();
        assert_eq!(
            def_rows.len(),
            2,
            "both ambiguous definitions must still appear as separate rows; got {ranked:?}"
        );
        for row in &def_rows {
            assert_eq!(
                row.degree, 0,
                "an ambiguous definition's own attributable degree is unknown - the code has \
                 already decided (by creating a Standalone row at all) that its references \
                 cannot be pinned to one candidate, so a Def row must never claim the whole \
                 unattributed pool as its own; got {ranked:?}"
            );
        }
        let standalone = ranked
            .iter()
            .find(|r| r.loc.file != "a.rs" && r.loc.file != "b.rs")
            .unwrap_or_else(|| {
                panic!("expected a pooled Standalone reference row; got {ranked:?}")
            });
        assert_eq!(
            standalone.degree, 5,
            "the Standalone row alone carries the real aggregate unattributed reference count - \
             genuinely different from the ambiguous Def rows' degree, never a shared pooled \
             number; got {ranked:?}"
        );
    }

    /// Spec 92 criterion 3, "a query with no strong token returns the honest 'no entity
    /// matches strongly' line instead of noise." A query that only matches a tree-wide-common
    /// (AMBIGUOUS - many distinct definitions) token is not strong; a rare, specific match is;
    /// a query matching nothing at all is not strong either (it has no data to be noisy about,
    /// but it is not a confident hit). `run` is genuinely ambiguous - twelve UNRELATED
    /// definitions, so a match could mean any of them; `apply_damage` is defined exactly once.
    /// Reference VOLUME plays no part here (spec 92 criterion 3 remediation, the
    /// adj-u92c3-verdict-reject fix): a name referenced constantly but defined exactly once is
    /// NOT ambiguous, only a name with multiple candidate definitions is - see
    /// `has_strong_match_is_true_for_a_single_definition_referenced_many_times` below for that
    /// half of the contract.
    #[test]
    fn has_strong_match_is_false_when_every_matching_token_is_tree_wide_common() {
        let dir = tempfile::tempdir().unwrap();
        for i in 0..12 {
            std::fs::write(dir.path().join(format!("f{i}.rs")), "fn run() {}\n").unwrap();
        }
        std::fs::write(dir.path().join("combat.rs"), "fn apply_damage() {}\n").unwrap();
        let g = Symbols::open(dir.path().to_str().unwrap(), None);

        assert!(
            !g.has_strong_match("run", 8),
            "a query that only matches a name with many distinct (ambiguous) definitions must \
             not be a strong match"
        );
        assert!(
            g.has_strong_match("apply_damage", 8),
            "a rare, specific match is strong"
        );
        assert!(
            !g.has_strong_match("nonexistent_symbol_zzz", 8),
            "a query with no matches at all is not a strong match either"
        );
    }

    /// Spec 92 criterion 3 remediation (adj-u92c3-verdict-reject): the reject's own live repro
    /// against the real project tree - `criterion_stable_id` (1 definition, 37 references) and
    /// `sweep_terminal` (1 definition, 93 references) both wrongly printed "no entity matches
    /// strongly". A single-definition entity is UNAMBIGUOUS no matter how many places call it;
    /// the old cutoff conflated one entity's own reference VOLUME with tree-wide name
    /// AMBIGUITY (how many DISTINCT definitions share the name). This fixture mirrors that
    /// shape at a scale that clears `AMBIGUITY_PERCENTILE` under the OLD (broken) raw
    /// occurrence-count cutoff, proving the fix measures ambiguity, not popularity.
    #[test]
    fn has_strong_match_is_true_for_a_single_definition_referenced_many_times() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("def.rs"), "fn widely_called() {}\n").unwrap();
        for i in 0..40 {
            std::fs::write(
                dir.path().join(format!("caller{i}.rs")),
                "fn go() { widely_called(); }\n",
            )
            .unwrap();
        }
        let g = Symbols::open(dir.path().to_str().unwrap(), None);

        assert!(
            g.has_strong_match("widely_called", 8),
            "a single-definition entity must read as strong regardless of how many places \
             reference it - reference volume is not ambiguity"
        );
    }

    /// Spec 92 criterion 3 remediation (adj-u92c3-verdict-reject, the CONTAINS-tier
    /// sentinel-inversion gap): a query term that is never ITSELF an indexed name (only a
    /// substring of one) must still read as strong when it resolves to a real, unambiguous
    /// entity - `has_strong_match` must judge the MATCHED ENTITY's ambiguity
    /// (`scored_hits`' resolved `name`), not do a second raw exact-key lookup on the query
    /// term itself, which finds nothing and used to silently read as "not strong".
    #[test]
    fn has_strong_match_is_true_for_a_contains_tier_match_of_an_unambiguous_entity() {
        let (_dir, g) = grounder_over(&[("combat.rs", "fn apply_damage() {}\n")]);

        assert!(
            g.has_strong_match("damage", 8),
            "\"damage\" only CONTAINS-matches the single-definition apply_damage; it must read \
             as strong, not silently fail through the honest no-match line"
        );
    }

    #[test]
    fn reindex_over_a_deleted_file_stops_grounding_it() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_str().unwrap();
        std::fs::write(dir.path().join("a.rs"), "fn gone_symbol() {}\n").unwrap();
        std::fs::write(dir.path().join("b.rs"), "fn kept_symbol() {}\n").unwrap();
        let g = Symbols::open(root, None);
        assert!(!g.ground("gone_symbol", 5).is_empty());

        // Delete a.rs and reindex it: the grounder must stop surfacing the gone file as a seed
        // (a deleted file's defs must not keep grounding), while the surviving file still grounds.
        std::fs::remove_file(dir.path().join("a.rs")).unwrap();
        g.reindex(root, &["a.rs".to_string()]);
        assert!(
            g.ground("gone_symbol", 5).is_empty(),
            "a deleted file's symbol must not keep grounding"
        );
        assert!(
            !g.ground("kept_symbol", 5).is_empty(),
            "the surviving file must still ground"
        );

        // The removal is PERSISTED: a fresh reader (a separate process) never grounds it either.
        let reader = Symbols::open(root, None);
        assert!(
            reader.ground("gone_symbol", 5).is_empty(),
            "the deleted file's symbols must be purged from the persisted index"
        );
    }

    /// Spec 92 criterion 2: `docs/audit/2026-09-graph-vs-grep.md` questions 5 (the driver
    /// `fresh` flag's semantics) and 8 (`OUTER_WALL_CLOCK_SEC`) both failed with "JS not
    /// indexed" - `ground` surfaced only generic `new`/`driver` module noise. Proven here
    /// against this project's OWN, REAL `workflows/rigger.js` (its exact bytes, copied into an
    /// isolated fixture root at the same relative path - not a synthetic snippet), through the
    /// production `Symbols` grounder: both constants must now ground to it.
    #[test]
    fn ground_answers_audit_questions_5_and_8_against_the_real_rigger_js() {
        let real = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../workflows")
                .join("rigger.js"),
        )
        .expect("this project's own workflows/rigger.js must be readable");
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("workflows")).unwrap();
        std::fs::write(dir.path().join("workflows").join("rigger.js"), &real).unwrap();
        let g = Symbols::open(dir.path().to_str().unwrap(), None);

        // Q8: `OUTER_WALL_CLOCK_SEC` (a ternary-valued constant).
        let outer = g.ground("OUTER_WALL_CLOCK_SEC", 6);
        assert!(
            outer.iter().any(|r| r.file == "workflows/rigger.js"),
            "OUTER_WALL_CLOCK_SEC must ground to workflows/rigger.js; got {outer:?}"
        );
        // Q5: the driver `fresh` flag - its constant is named `FRESH` (`!!A.fresh`).
        let fresh = g.ground("FRESH", 6);
        assert!(
            fresh.iter().any(|r| r.file == "workflows/rigger.js"),
            "FRESH must ground to workflows/rigger.js; got {fresh:?}"
        );
    }

    /// Test-only fixture: a definition at `line`, real fields elsewhere immaterial to
    /// [`scored_hits`]'s tie-break (`is_test`/`is_out_of_line_module`/`path_override`/
    /// `enclosing_inline_module_path` all left at their inert defaults).
    fn hand_def(name: &str, line: u32) -> Def {
        Def {
            kind: Kind::Function,
            name: name.to_string(),
            line,
            is_test: false,
            is_out_of_line_module: false,
            path_override: None,
            enclosing_inline_module_path: None,
        }
    }

    /// Test-only fixture: a reference at `line`, uncalled/unattributed (`enclosing: None`) and
    /// not test evidence - the fields [`scored_hits`] itself never reads.
    fn hand_ref(name: &str, line: u32) -> SymRef {
        SymRef {
            name: name.to_string(),
            line,
            enclosing: None,
            is_test: false,
        }
    }

    /// A one-file `fixture.rs` Rust index holding `defs` and `refs`.
    fn fixture_index(defs: Vec<Def>, refs: Vec<SymRef>) -> SymbolIndex {
        let mut idx = SymbolIndex::default();
        idx.insert_file(
            "fixture.rs".to_string(),
            FileSymbols {
                lang: Lang::Rust,
                defs,
                refs,
                partial: false,
            },
        );
        idx
    }

    /// The name `hits` slots at `fixture.rs`'s `line`.
    fn slot_at<'a>(hits: &[ScoredHit<'a>], line: u32) -> Option<&'a str> {
        hits.iter()
            .find(|h| h.line == line && h.file == "fixture.rs")
            .map(|h| h.name)
    }

    /// Over a `fixture.rs` index of `defs` and `refs`, scoring `terms` slots each `(line, name,
    /// why)` case's `name` at its `line`.
    fn assert_slots(
        defs: Vec<Def>,
        refs: Vec<SymRef>,
        terms: &[&str],
        expected: &[(u32, &str, &str)],
    ) {
        let idx = fixture_index(defs, refs);
        let hits = scored_hits(&idx, terms);
        for (line, name, why) in expected {
            let slot = slot_at(&hits, *line);
            assert_eq!(slot, Some(*name), "{why}; got {slot:?}");
        }
    }

    crate::test_cases! {
        /// [`scored_hits`]' own tie-break, `let better = ..` (grounder.rs, three clauses over one
        /// (file, line) location's accumulated best candidate): clause 1 is `hit_tier >
        /// slot.tier` - a STRICTLY higher tier always wins, no matter how the two candidates compare
        /// on commonness or lexical kind. Two hand-built candidates collide at the SAME (file, line)
        /// in each cluster below (the only way [`scored_hits`]' `best.entry(..).and_modify(..)`
        /// tie-break ever runs at all): a definition is always scored before any reference in the
        /// SAME file ([`scored_hits`]'s own `for d in &fs.defs { .. } for r in &fs.refs { .. }`), so
        /// the definition is always the initial slot (`or_insert`) and the reference is always the
        /// challenger (`and_modify`) whenever both match at one location.
        scored_hits_a_strictly_higher_tier_always_wins_over_a_worse_commonness_and_lexical: assert_slots(
            vec![
                // Cluster A (line 10): the SLOT is a CONTAINS-tier (1) definition that is
                // both COMMON (occurs 4x tree-wide, via the 3 extra refs below) and, being a
                // definition, lexically favored (3) - every OTHER clause would keep it. Only
                // the strictly-higher-tier challenger below can dislodge it.
                hand_def("walrus_partial_match_hook", 10),
                // Cluster B (line 20): the SLOT is an EXACT-tier (2) definition that is RARE
                // (occurs once). The challenger below matches the SAME tier but is common
                // (4x) - equal tier must NOT let a worse-commonness challenger win.
                hand_def("penguin_rare_def", 20),
            ],
            vec![
                // Cluster A's challenger: EXACT tier (2, via the term below), strictly
                // higher than the slot's CONTAINS tier (1) - must win despite its worse
                // (rarer-favoring-the-slot) commonness and worse (reference) lexical kind.
                hand_ref("narwhal_full_match_hook", 10),
                // Cluster B's challenger: SAME tier (2) as the slot, but common (4x) -
                // clause 1 alone must never promote it; the slot must stay.
                hand_ref("octopus_common_ref", 20),
                // Inflate "walrus_partial_match_hook"'s tree-wide commonness to 4 (the
                // definition above plus these three), so cluster A's slot is common, not
                // rare - proving the win is from tier alone, not incidental rarity.
                hand_ref("walrus_partial_match_hook", 200),
                hand_ref("walrus_partial_match_hook", 201),
                hand_ref("walrus_partial_match_hook", 202),
                // Inflate "octopus_common_ref" to 4 occurrences, so cluster B's challenger
                // is common (worse), not merely tied on commonness too.
                hand_ref("octopus_common_ref", 210),
                hand_ref("octopus_common_ref", 211),
                hand_ref("octopus_common_ref", 212),
            ],
            &[
                "partial_match",           // CONTAINS-tier term for the cluster-A slot
                "narwhal_full_match_hook", // EXACT-tier term for the cluster-A challenger
                "penguin_rare_def",        // EXACT-tier term for the cluster-B slot
                "octopus_common_ref",      // EXACT-tier term for the cluster-B challenger
            ],
            &[
                (
                    10,
                    "narwhal_full_match_hook",
                    "a strictly higher tier must win regardless of commonness/lexical",
                ),
                (
                    20,
                    "penguin_rare_def",
                    "an EQUAL tier must never be promoted by clause 1 alone, however common the \
             challenger; the rarer-and-already-slotted definition must stay",
                ),
            ],
        );
        /// Clause 2, `hit_tier == slot.tier && hit_commonness < slot.commonness`: at a TIED tier,
        /// the STRICTLY rarer candidate wins outright - even against a worse (reference) lexical
        /// kind - and a merely EQUAL commonness must never itself promote: there is no third clause
        /// falling back to [`ScoredHit::lexical`] within a slot (proven separately - the definition
        /// wins a same-tier, same-commonness tie by scoring order alone).
        scored_hits_breaks_a_tier_tie_by_strict_rarity_never_by_an_equal_commonness: assert_slots(
            vec![
                // Cluster C (line 30): the SLOT is a common (4x) definition; the challenger
                // below is a strictly RARER (1x) reference at the same tier - it must win
                // despite its worse lexical kind, proving commonness outranks lexical.
                hand_def("quokka_common_def", 30),
                // Cluster D (line 40): SLOT and challenger have EQUAL commonness (1x each)
                // at the same tier - clause 2's `<` must stay false on a tie, never `<=`.
                hand_def("echidna_equal_def", 40),
            ],
            vec![
                hand_ref("wombat_rare_ref", 30),
                hand_ref("platypus_equal_ref", 40),
                // Inflate "quokka_common_def" to 4 occurrences (common, worse).
                hand_ref("quokka_common_def", 220),
                hand_ref("quokka_common_def", 221),
                hand_ref("quokka_common_def", 222),
            ],
            &[
                "quokka_common_def",
                "wombat_rare_ref",
                "echidna_equal_def",
                "platypus_equal_ref",
            ],
            &[
                (
                    30,
                    "wombat_rare_ref",
                    "a strictly rarer same-tier challenger must win over a common slot, even as a \
             reference against a definition",
                ),
                (
                    40,
                    "echidna_equal_def",
                    "an EQUAL commonness must never itself promote a reference over an already-slotted, \
             same-tier definition",
                ),
            ],
        );
        /// A tied tier AND tied commonness never promotes the challenger, however their
        /// [`ScoredHit::lexical`] kinds compare - `better`'s two clauses stop at commonness, with no
        /// third clause falling back to `lexical` within a slot. Since every definition in a file is
        /// always scored before any reference in it, a definition can never challenge an
        /// already-slotted reference; only a same-tier, same-commonness REFERENCE can ever challenge
        /// an already-slotted DEFINITION (and does not promote it), or a second definition can
        /// challenge a first one at the SAME line (and does not promote it either) - the
        /// FIRST-inserted candidate always stays, by scoring order alone.
        scored_hits_lexical_never_promotes_a_tied_reference_or_a_tied_second_definition: assert_slots(
            vec![
                // Cluster E (line 50): TWO definitions at the same tier and commonness (1x
                // each) - "bandicoot" is scored first (Vec order) and must stay the slot;
                // `better`'s commonness clause must stay `<`, never `<=`, so an equal
                // second definition never promotes over the first.
                hand_def("bandicoot_alpha_def", 50),
                hand_def("dingo_beta_def", 50),
            ],
            vec![],
            &    ["bandicoot_alpha_def", "dingo_beta_def"],
            &[
                (
                    50,
                    "bandicoot_alpha_def",
                    "tied tier, tied commonness, tied (definition) lexical: the first-scored definition \
             must stay the slot",
                ),
            ],
        );
    }
}
