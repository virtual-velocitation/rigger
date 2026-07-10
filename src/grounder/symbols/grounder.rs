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

impl Grounder for Symbols {
    fn ground(&self, query: &str, k: usize) -> Vec<Ref> {
        if query.is_empty() || k == 0 {
            return Vec::new();
        }
        // The query's alphanumeric/underscore terms are the symbol candidates (so `apply_damage`
        // stays one term). Single-character terms are dropped as noise.
        let terms: Vec<&str> = query
            .split(|c: char| !c.is_alphanumeric() && c != '_')
            .filter(|t| t.len() >= 2)
            .collect();
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
    /// files in sorted path order, and grep walks the shared guarded skeleton. With an empty query
    /// or no match this returns empty views (the empty-radius fail-safe unit 3 routes to the full
    /// panel), never a partial or a panic.
    fn blast_radius(&self, query: &str, k: usize) -> BlastRadius {
        // The query's symbol candidates: the SAME alphanumeric/underscore terms `ground` extracts
        // (so `apply_damage` stays one term and single-char noise is dropped).
        let terms: Vec<&str> = query
            .split(|c: char| !c.is_alphanumeric() && c != '_')
            .filter(|t| t.len() >= 2)
            .collect();

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
        for r in grep.ground(query, usize::MAX) {
            if seen.insert(r.file.clone()) {
                safe.push(r.file);
            }
        }

        // The precise view is the ranked structural set capped at `k`; the safe view stays uncapped.
        let mut precise = structural;
        precise.truncate(k);
        BlastRadius {
            precise,
            safe,
            serialize,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::grounder::Grounder;

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
