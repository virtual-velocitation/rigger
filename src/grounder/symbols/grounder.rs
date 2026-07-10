//! The `symbols` grounder (spec 15, unit 4): the `Grounder` port over the persisted symbol
//! index, serving the PRECISE grounding contract (architecture 5.5.6). It ranks a DEFINITION
//! whose name matches the query above a mere REFERENCE, above an incidental prose mention
//! (which is not indexed as a symbol at all, so it never appears). Selection is wired in
//! `grounder_for` / `main::select_grounder`; this module is the consumer of units 1-3's model,
//! extraction, registry, and store.

use crate::grounder::symbols::model::{Lang, SymbolIndex};
use crate::grounder::symbols::{build_index, reindex_files, store};
use crate::grounder::{BlastRadius, Grep, Grounder, Ref};
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
/// fingerprint dropped, so the shared `index_one_file` authority still runs for it (and leaves its
/// existing entry, exactly as it does on the whole-tree walk).
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
        // Score each (file, line) in ONE pass over the files, keyed so a location that is BOTH a
        // definition and a reference of the query (e.g. a recursive `fn parse() { parse(); }`, whose
        // def name and self-call share a line) collapses to its HIGHEST score - not two rows. A
        // DEFINITION whose name is a term ranks 3, a REFERENCE 2; incidental prose is not indexed as
        // a symbol, so it never appears (it ranks 0 by absence). We iterate `files()` directly -
        // keeping each hit's file and line - rather than `definitions_named`/`references_named`,
        // which drop the file and would force a rescan to recover it (the
        // arch-u15-1-defsnamed-drops-file cohesion note); grounding needs file+line on every hit, so
        // the file-keeping pass is both correct and cheaper. The `BTreeMap` also makes the fold
        // order deterministic regardless of insertion order.
        let mut best: BTreeMap<(&str, u32), (u8, &str)> = BTreeMap::new();
        for (path, fs) in idx.files() {
            for d in &fs.defs {
                if terms.contains(&d.name.as_str()) {
                    let slot = best
                        .entry((path.as_str(), d.line))
                        .or_insert((0, d.name.as_str()));
                    if 3 > slot.0 {
                        *slot = (3, d.name.as_str());
                    }
                }
            }
            for r in &fs.refs {
                if terms.contains(&r.name.as_str()) {
                    let slot = best
                        .entry((path.as_str(), r.line))
                        .or_insert((0, r.name.as_str()));
                    if 2 > slot.0 {
                        *slot = (2, r.name.as_str());
                    }
                }
            }
        }
        // Rank: higher score first, then file, then line (a total, deterministic order).
        let mut scored: Vec<(u8, &str, u32, &str)> = best
            .into_iter()
            .map(|((file, line), (score, name))| (score, file, line, name))
            .collect();
        scored.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(b.1)).then(a.2.cmp(&b.2)));
        scored
            .into_iter()
            .take(k)
            .map(|(_, file, line, name)| Ref {
                file: file.to_string(),
                line,
                text: name.to_string(),
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
    /// off (a grep / turbovec / nop grounder inherits the empty default and emits neither). The
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
        std::fs::write(dir.path().join("a.rs"), "fn one() {}\n").unwrap();
        std::fs::write(dir.path().join("b.rs"), "fn two() {}\n").unwrap();
        // Both open over the SAME initial on-disk index {a: one, b: two}, each into its own memory.
        let conductor = Symbols::open(root, None);
        let other_process = Symbols::open(root, None);

        // The "other process" reindexes b.rs -> twoprime and PERSISTS it. Disk is now {a, b'}.
        std::fs::write(dir.path().join("b.rs"), "fn twoprime() {}\n").unwrap();
        other_process.reindex(root, &["b.rs".to_string()]);

        // The conductor still holds the STALE in-memory index {a: one, b: two}. It reindexes a.rs.
        // Its persist MUST fold in the peer's twoprime rather than clobber it back to two.
        std::fs::write(dir.path().join("a.rs"), "fn oneprime() {}\n").unwrap();
        conductor.reindex(root, &["a.rs".to_string()]);

        // A fresh reader (a third process) sees BOTH changes on disk.
        let reader = Symbols::open(root, None);
        assert!(
            !reader.ground("oneprime", 5).is_empty(),
            "the conductor's own reindex must be persisted"
        );
        assert!(
            !reader.ground("twoprime", 5).is_empty(),
            "the peer's concurrent reindex must NOT be clobbered by the conductor's stale snapshot"
        );
        assert!(
            reader.ground("two", 5).is_empty(),
            "the superseded symbol must be gone"
        );
    }

    #[test]
    fn reindex_replaces_only_a_changed_files_symbols() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_str().unwrap();
        std::fs::write(dir.path().join("a.rs"), "fn one() {}\n").unwrap();
        std::fs::write(dir.path().join("b.rs"), "fn two() {}\n").unwrap();
        let g = Symbols::open(root, None);
        assert!(!g.ground("one", 5).is_empty());

        // Change a.rs, reindex just it: the new symbol grounds, the old one is gone, b.rs stands.
        std::fs::write(dir.path().join("a.rs"), "fn oneprime() {}\n").unwrap();
        g.reindex(root, &["a.rs".to_string()]);
        assert!(
            !g.ground("oneprime", 5).is_empty(),
            "the freshened symbol must ground"
        );
        assert!(
            g.ground("one", 5).is_empty(),
            "the replaced symbol must be gone"
        );
        assert!(
            !g.ground("two", 5).is_empty(),
            "the untouched file must stand"
        );
    }
}
