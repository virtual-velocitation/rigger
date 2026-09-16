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

/// The degree percentile at or above which a symbol is treated as a HUB and its blast radius
/// SERIALIZES (architecture 5.5.2). Drawn from the repo's OWN per-language reference-degree
/// distribution (not an absolute constant a monorepo would blow past): the 90th percentile flags
/// only the top decile of highest-degree names, so a hub serializes conservatively rather than
/// truncating. Unit 2's eval measures the parallelism this knob retains; unit 3 owns any retune.
const HUB_DEGREE_PERCENTILE: f64 = 0.90;

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

/// Every distinct `(name, language)` pair's occurrence count across the WHOLE index, counting
/// DEFINITIONS always and REFERENCES only when `count_refs` is set - the ONE counting pass
/// [`commonness_map`] and [`ambiguity_map`] both share (spec 92 criterion 3 remediation,
/// arch-u92c3-cutoff-formula-duplicated-not-shared's DRY finding applied to this pair too: two
/// near-identical scan loops over the same data, one function body now covers both). Keyed by
/// `(name, Lang)`, never a bare name (spec 92 criterion 3 remediation round 5,
/// adj-u92c3-r4-verdict-reject / adv-u92c3r4-ambiguity-map-bleeds-across-languages): a bare-name
/// key sums an entity's popularity/ambiguity across every language sharing that name, exactly
/// the cross-language collision [`SymbolIndex::reference_degree`] and [`SymbolIndex::is_hub`]
/// already guard against (5.5.2) - a `run` over-defined in Python must never inflate the same
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
/// own popularity with tree-wide name ambiguity (the defect this map exists to fix). Shares its
/// percentile-cutoff formula with [`SymbolIndex::is_hub`] via
/// [`crate::grounder::symbols::model::percentile_cutoff`] rather than re-deriving it, per
/// arch-u92c3-cutoff-formula-duplicated-not-shared.
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
struct ScoredHit<'a> {
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
}

/// Score every (file, line) definition/reference location in `idx` against `terms`, ranked
/// tier-first, then rarest-commonness-first, then definition-over-reference, then by file/line
/// (a total, deterministic order) - the ONE scored, sorted pass both [`Symbols::ground`] and
/// [`Symbols::ground_ranked`] consume, so the two views can never disagree on ranking. A
/// location that matches more than one term (or both an EXACT and a CONTAINS candidate) keeps
/// its BEST tier - commonness is a property of the matched entity's own name, so it never varies
/// by which term matched - mirroring the def-and-ref-at-one-line collapse the prior single-pass
/// scorer already made.
fn scored_hits<'a>(idx: &'a SymbolIndex, terms: &[&str]) -> Vec<ScoredHit<'a>> {
    let commonness = commonness_map(idx);
    let mut best: BTreeMap<(&'a str, u32), ScoredHit<'a>> = BTreeMap::new();
    for (path, fs) in idx.files() {
        let mut score_one = |name: &'a str, line: u32, kind: HitKind, lexical: u8| {
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
            // `SymbolIndex::reference_degree`/`is_hub` already scope the fan-out signal (5.5.2).
            let mut hit_tier = 0u8;
            for t in terms {
                let tier = if name == *t {
                    2u8
                } else if name.contains(t) {
                    1u8
                } else {
                    0u8
                };
                if tier > hit_tier {
                    hit_tier = tier;
                }
            }
            if hit_tier == 0 {
                return;
            }
            let hit_commonness = commonness.get(&(name, fs.lang)).copied().unwrap_or(0);
            best.entry((path.as_str(), line))
                .and_modify(|slot| {
                    let better = hit_tier > slot.tier
                        || (hit_tier == slot.tier && hit_commonness < slot.commonness)
                        || (hit_tier == slot.tier
                            && hit_commonness == slot.commonness
                            && lexical > slot.lexical);
                    if better {
                        *slot = ScoredHit {
                            tier: hit_tier,
                            commonness: hit_commonness,
                            lexical,
                            file: path.as_str(),
                            line,
                            name,
                            lang: fs.lang,
                            kind,
                        };
                    }
                })
                .or_insert(ScoredHit {
                    tier: hit_tier,
                    commonness: hit_commonness,
                    lexical,
                    file: path.as_str(),
                    line,
                    name,
                    lang: fs.lang,
                    kind,
                });
        };
        for d in &fs.defs {
            score_one(d.name.as_str(), d.line, HitKind::Def, 3);
        }
        for r in &fs.refs {
            score_one(r.name.as_str(), r.line, HitKind::Ref, 2);
        }
    }
    let mut hits: Vec<ScoredHit<'a>> = best.into_values().collect();
    hits.sort_by(|a, b| {
        b.tier
            .cmp(&a.tier)
            .then(a.commonness.cmp(&b.commonness))
            .then(b.lexical.cmp(&a.lexical))
            .then(a.file.cmp(b.file))
            .then(a.line.cmp(&b.line))
    });
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
    /// 1) - the `symbols` override of the grep-only trait default:
    ///
    /// - `precise` (the grounding contract) is the STRUCTURAL view - the files that DEFINE the
    ///   queried symbol ranked ABOVE the files that REFERENCE it - capped at `k`. It is what seeds
    ///   an agent's prompt, so it favors precision.
    /// - `safe` (the safety contract) is the UNION of the structural view and grep, UNCAPPED. It
    ///   runs BOTH engines - the structural graph AND the EXISTING [`Grep`] grounder over the same
    ///   root - so it is never narrower than today's grep radius (5.5.9). Name-level linking MISSES
    ///   references (macros, dynamic dispatch, re-exports, a mention the tags query never indexes as
    ///   a symbol); the grep union recovers them, so the partitioning consumer can never
    ///   under-partition.
    /// - `serialize` is set when ANY query term is a HUB in ANY present language (its per-language
    ///   reference degree clears [`HUB_DEGREE_PERCENTILE`] of that language's OWN degree
    ///   distribution). A hub's radius fails SAFE by conflict-with-everything - the consumer gives
    ///   the unit its own batch - NEVER by truncating `safe` (which still carries every file).
    ///
    /// Determinism is by construction: `files()` is a `BTreeMap`, so both structural passes visit
    /// files in sorted path order (the ranked `precise` / structural head of `safe`), and the
    /// grep-recovered tail of `safe` is explicitly SORTED before it is appended - grep itself walks
    /// the tree in unsorted `read_dir` order, so without the sort `safe` would be set-deterministic
    /// but not order-deterministic. Sorting the tail makes the whole `safe` ordering reproducible
    /// across processes, which is what unit 3 needs to hash the seed-file list into a stable
    /// `BlastRadiusComputed` audit event. An empty query, or a degenerate query whose every term is
    /// dropped by the character-count filter (a single character - ASCII or multibyte - or all
    /// punctuation, the same guard `ground` applies), returns empty views (the empty-radius
    /// fail-safe unit 3 routes to the full panel) WITHOUT running a whole-repo grep, never a partial
    /// or a panic. A query WITH real terms that simply matches nothing is ALSO empty, but that case
    /// does run the uncapped grep - it just comes back empty; only the empty/degenerate-terms cases
    /// short-circuit before grep.
    fn blast_radius(&self, query: &str, k: usize) -> BlastRadius {
        // The query's symbol candidates: the SAME alphanumeric/underscore terms `ground` extracts
        // through the shared `query_terms` authority (so `apply_damage` stays one term and
        // single-char noise is dropped).
        let terms = query_terms(query);
        // The empty-terms fail-safe, applied BEFORE any index read or grep: a degenerate query (an
        // empty query, a single character, or all punctuation) that drops EVERY term grounds to
        // nothing, so its blast radius is the empty radius too - matching `ground`, which
        // early-returns on the same condition. Without this short-circuit `blast_radius` would fall
        // through to the UNCAPPED `grep.ground(query, usize::MAX)` below, and a one-char query would
        // match nearly every line in the tree - an unbounded whole-repo grep that leaves `precise`
        // empty but `safe` covering almost the entire repo, forcing the full panel and corrupting
        // the retention metric. `BlastRadius::default()` is empty precise, empty safe, not-serialize
        // - the same empty fail-safe unit 3 routes to the full, unpartitioned panel.
        if terms.is_empty() {
            return BlastRadius::default();
        }

        // The STRUCTURAL view, ranked (definer files, then referencer files not already a definer),
        // plus the hub verdict - all computed under ONE read lock over the index, returned as a
        // tuple so neither binding needs a dead pre-initialization before the locked block.
        let (structural, serialize): (Vec<String>, bool) = {
            let idx = self.idx.lock().unwrap();
            // Iterate `files()` directly to KEEP each hit's owning file: `definitions_named` drops
            // it (the arch-u15-1-defsnamed-drops-file cohesion note), which would force a rescan.
            // `files()` is a BTreeMap, so this is sorted-path-order and deterministic.
            let mut definers: Vec<&str> = Vec::new();
            let mut referencers: Vec<&str> = Vec::new();
            for (path, fs) in idx.files() {
                if fs.defs.iter().any(|d| terms.contains(&d.name.as_str())) {
                    definers.push(path.as_str());
                }
                if fs.refs.iter().any(|r| terms.contains(&r.name.as_str())) {
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
            // Hub composition: serialize if ANY term is a hub WITHIN ANY language the index holds.
            // The per-language scope is drawn from the languages actually present, so a name that
            // over-links in another language never flags this one (the 5.5.2 cross-language fix).
            let langs: BTreeSet<Lang> = idx.files().values().map(|f| f.lang).collect();
            let hub = terms.iter().any(|t| {
                langs
                    .iter()
                    .any(|&l| idx.is_hub(t, l, HUB_DEGREE_PERCENTILE))
            });
            (ranked, hub)
        };

        // The SAFE-SUPERSET view: the FULL (untruncated) structural set UNIONed with an UNCAPPED
        // grep over the same root - the honest "both engines" cost (5.5.9). This clone happens
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
        for r in grep.ground(query, usize::MAX) {
            if seen.insert(r.file.clone()) {
                grep_tail.push(r.file);
            }
        }
        grep_tail.sort();
        safe.extend(grep_tail);

        // The precise view is the ranked structural set capped at `k`; the safe view stays uncapped.
        let mut precise = structural;
        precise.truncate(k);
        BlastRadius {
            precise,
            safe,
            serialize,
        }
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
            crate::grounder::symbols::registry::GRAMMAR_TAGS_VERSION
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
    /// language - the same primitive [`SymbolIndex::is_hub`] already uses, not an approximate
    /// count of what happened to be visible in this page.
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
            let key = match h.kind {
                HitKind::Def => EntityKey::Def(h.name, h.file, h.line),
                HitKind::Ref => match def_sites.get(&h.entity()).map(Vec::as_slice) {
                    Some([(file, line)]) => EntityKey::Def(h.name, file, *line),
                    _ => EntityKey::Standalone(h.name, h.lang),
                },
            };
            if !seen.insert(key) {
                continue;
            }
            out.push(RankedRef {
                loc: Ref {
                    file: h.file.to_string(),
                    line: h.line,
                    text: h.name.to_string(),
                },
                degree: idx.reference_degree(h.name, h.lang),
            });
            if out.len() >= k {
                break;
            }
        }
        out
    }

    /// Whether `query` has at least one match whose MATCHED ENTITY sits AT OR BELOW the repo's
    /// own name-ambiguity distribution's [`HUB_DEGREE_PERCENTILE`] cutoff (spec 92 criterion 3:
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
    ///    mirroring [`SymbolIndex::is_hub`]'s own per-language cutoff exactly: a name defined
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
        // present, exactly as `SymbolIndex::is_hub` draws its degree cutoff from only the
        // queried language's own reference-degree distribution (5.5.2). A language absent from
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
                percentile_cutoff(&mut degrees, HUB_DEGREE_PERCENTILE)
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
                && stamp.ends_with(crate::grounder::symbols::registry::GRAMMAR_TAGS_VERSION),
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
        // A higher-degree symbol (`helper`, referenced twice) so the per-language degree
        // distribution is non-degenerate: `apply_damage` (degree 1) then sits BELOW the hub
        // percentile, making the `!serialize` assertion below meaningful rather than a single-name
        // artifact (a lone referenced name would trivially be its own 100th percentile).
        std::fs::write(dir.path().join("h1.rs"), "fn a() { helper(); }\n").unwrap();
        std::fs::write(dir.path().join("h2.rs"), "fn b() { helper(); }\n").unwrap();
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
        assert!(
            !br.serialize,
            "apply_damage is not a hub, so this radius does not serialize; got {br:?}"
        );
    }

    /// Spec 16 unit 1, the criterion-1 hub fixture: a HUB symbol (degree at or above the repo's
    /// per-language degree percentile) fails SAFE by SERIALIZING (flagged conflict-with-everything)
    /// rather than TRUNCATING its large file set. The safe view still carries EVERY file (never
    /// dropped, even past the `k` cap); `serialize` tells the partitioning consumer to give the
    /// unit its own batch. A degree-1 symbol in the same repo does not serialize.
    #[test]
    fn a_hub_symbol_serializes_and_its_safe_view_is_not_truncated() {
        let dir = tempfile::tempdir().unwrap();
        // `spawn` is referenced across many files (a hub); `rare_call` in exactly one, so the
        // per-language degree distribution has a genuine high-degree name to clear the percentile.
        std::fs::write(dir.path().join("def.rs"), "fn spawn() {}\n").unwrap();
        let mut expected: Vec<String> = vec!["def.rs".to_string()];
        for i in 0..12 {
            let name = format!("f{i}.rs");
            std::fs::write(dir.path().join(&name), "fn c() { spawn(); }\n").unwrap();
            expected.push(name);
        }
        std::fs::write(dir.path().join("rare.rs"), "fn r() { rare_call(); }\n").unwrap();
        let g = Symbols::open(dir.path().to_str().unwrap(), None);

        // A SMALL cap proves the safe view is uncapped: there are 13 `spawn` files, more than k=8.
        let br = g.blast_radius("spawn", 8);
        assert!(
            br.serialize,
            "a hub symbol must serialize (conflict-with-everything), never truncate; got {br:?}"
        );
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
        // A degree-1 symbol in the SAME repo is NOT a hub and does not serialize.
        let rare = g.blast_radius("rare_call", 8);
        assert!(
            !rare.serialize,
            "a degree-1 symbol is not a hub; got {rare:?}"
        );
    }

    /// The blast-radius fail-safe paths (spec 16 unit 1): an empty query and a no-match query each
    /// return EMPTY views and never serialize (unit 3 routes an empty radius to the full,
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

        // Empty query -> no terms -> empty views, never serialize.
        let empty = g.blast_radius("", 8);
        assert!(
            empty.precise.is_empty() && empty.safe.is_empty() && !empty.serialize,
            "an empty query is the empty fail-safe: both views empty, no serialize; got {empty:?}"
        );

        // A name that appears nowhere -> nothing structural AND nothing grep -> empty views.
        let none = g.blast_radius("nonexistent_symbol_zzz", 8);
        assert!(
            none.precise.is_empty() && none.safe.is_empty() && !none.serialize,
            "a no-match query is the empty fail-safe: both views empty, no serialize; got {none:?}"
        );

        // k=0 caps the PRECISE view to empty; the SAFE view is uncapped and still carries the radius.
        let k0 = g.blast_radius("parse", 0);
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
             safe, no serialize), not an unbounded whole-repo grep; got {one_char:?}"
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

        let br = g.blast_radius("render", 8);
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

    #[test]
    fn a_reference_ranks_below_a_definition_of_the_same_name() {
        let dir = tempfile::tempdir().unwrap();
        // def.rs DEFINES parse; call.rs REFERENCES it (a call site is a real symbol reference).
        std::fs::write(dir.path().join("def.rs"), "fn parse() {}\n").unwrap();
        std::fs::write(dir.path().join("call.rs"), "fn run() { parse(); }\n").unwrap();
        let g = Symbols::open(dir.path().to_str().unwrap(), None);
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
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.rs"), "fn parse() {}\n").unwrap();
        let g = Symbols::open(dir.path().to_str().unwrap(), None);
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
        let dir = tempfile::tempdir().unwrap();
        // Named so a plain file-order tiebreak (with tier ignored) would put the CONTAINS
        // match first - only real tier precedence can make the exact match win here.
        std::fs::write(dir.path().join("zzz_exact.rs"), "fn run() {}\n").unwrap();
        std::fs::write(dir.path().join("aaa_contains.rs"), "fn run_all() {}\n").unwrap();
        let g = Symbols::open(dir.path().to_str().unwrap(), None);

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
    /// shape at a scale that clears `HUB_DEGREE_PERCENTILE` under the OLD (broken) raw
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
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("combat.rs"), "fn apply_damage() {}\n").unwrap();
        let g = Symbols::open(dir.path().to_str().unwrap(), None);

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
}
