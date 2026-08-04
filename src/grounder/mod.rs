//! Grounding gives each agent only the context it needs: the locations relevant
//! to its task. `Grounder` is the port. `Grep` is the self-contained literal
//! grounder; the structural `symbols` grounder (spec 15) is the default, plugging
//! in behind the same trait.

// The structural grounding axis (spec 15): a symbol index projected from the code tree.
// Declared UNGATED on purpose - the parser-free data model (`symbols::model`) must compile
// in the light (`--no-default-features`) lane, where tree-sitter is not even linked, which
// is the compile-time proof that no tree-sitter type crosses into the model API. Only the
// tree-sitter-touching submodules inside `symbols` carry `#[cfg(feature = "symbols")]`.
pub mod symbols;

// The design-intent grounding axis (spec 29b): a doc-extraction pass that lowers the reference
// architecture, addenda, load-bearing decisions, handbook rules, and inline rationale into
// DocConceptExtracted events. Gated behind `symbols` exactly like the code extractor - it is the
// EMIT half; the always-compiled fold (contextgraph::sqlite) folds a design-intent log with this
// pass absent, so the light lane never links the doc extractor.
#[cfg(feature = "symbols")]
pub mod design;

use std::ops::ControlFlow;
use std::path::Path;

/// The ONE scoped directory-walk skeleton EVERY walk shares: grep's `ground` (this module), the
/// code-ingest `build_index`, and the design-ingest `project_batches`. They differ ONLY in the
/// per-file LEAF ACTION they pass as `on_file` - grep searches lines, the ingests extract
/// symbols / design intent. Factoring the walk here (always compiled, in `mod.rs`) means the
/// scope can never drift between them, so ingest and grounding cover exactly the same file set.
///
/// The walk is SCOPED to the project's own sources (spec 49, section 3), three ways:
///
/// - **The project's own ignore rules.** It honors the repository's committed `.gitignore`
///   files (root and nested), so whatever the project already declared as not-source - build
///   outputs, caches, vendored artifacts - is skipped. Only committed `.gitignore` is honored:
///   the user's GLOBAL gitignore and the per-clone `.git/info/exclude` are DISABLED, so the
///   scope is a pure function of the tree (the same tree yields a byte-identical graph on any
///   machine). `require_git(false)` applies those rules even when the tree is not a checked-out
///   repository (a cold-checkout build), and `parents(false)` confines rule-reading to the root
///   so a `.gitignore` ABOVE the project never bleeds in.
/// - **The always-excluded set.** Hidden entries are skipped, which covers the two the spec
///   names as never-source regardless of any ignore file - the VCS metadata directory `.git`
///   and rigger's own runtime directory `.rigger` (both dotdirs) - and additionally de-noises
///   the other tooling dotdirs (`.github`/`.cargo`/`.claude`/`.fastembed_cache`, none of which
///   is first-party source), replacing the old hardcoded skip-list with the actual declared
///   rules plus this convention.
/// - **Root confinement.** Symlinks are NOT followed (`follow_links(false)`), so a link that
///   escapes the root - or a link cycle - is simply never traversed: the walk can never grow a
///   cluster for paths outside the canonicalized project, and a cycle cannot form (there is no
///   visited-set to maintain because there is nothing to loop).
///
/// Entries are visited in sorted file-name order, so the walk is deterministic; an unreadable
/// entry (a permissions race, a broken link) is skipped rather than crashing the walk.
///
/// `on_file` receives each regular file (never a directory, never an unfollowed symlink) and
/// returns [`ControlFlow`]: `Break(())` stops the whole walk immediately (grep uses this to stop
/// once it has collected its `k` hits), `Continue(())` walks on.
pub(crate) fn walk_guarded<F>(root: &Path, on_file: &mut F) -> ControlFlow<()>
where
    F: FnMut(&Path) -> ControlFlow<()>,
{
    let walker = ignore::WalkBuilder::new(root)
        // Skip hidden entries: this is the always-excluded set - the VCS metadata dir `.git` and
        // rigger's runtime dir `.rigger` (both dotdirs) - plus the other never-source tooling
        // dotdirs, independent of any ignore file.
        .hidden(true)
        // Honor the project's OWN committed ignore rules only, so the scope is a pure function of
        // the tree (machine-independent, byte-identical rebuilds).
        .git_ignore(true)
        .git_global(false)
        .git_exclude(false)
        .ignore(false)
        // Apply committed `.gitignore` even when the tree is not a git checkout (cold build), and
        // never read ignore files above the root.
        .require_git(false)
        .parents(false)
        // Root confinement: never follow a symlink, so nothing escapes the root and no cycle forms.
        .follow_links(false)
        // Deterministic traversal order.
        .sort_by_file_name(|a, b| a.cmp(b))
        .build();
    for dent in walker {
        // An entry we cannot read (a permissions race, a broken link) is skipped, never a panic.
        let Ok(entry) = dent else { continue };
        // Only regular files reach the leaf action: directories are descended by the walker, and
        // an unfollowed symlink (its `file_type` is the link, not a file) is skipped - confinement.
        if entry.file_type().is_some_and(|ft| ft.is_file()) {
            on_file(entry.path())?;
        }
    }
    ControlFlow::Continue(())
}

/// A relevant location: a file, a line, and a snippet.
#[derive(Clone, Debug)]
pub struct Ref {
    pub file: String,
    pub line: u32,
    pub text: String,
}

/// The two-view blast radius of a query (architecture 5.5.1, spec 16 unit 1). Blast-radius has
/// OPPOSITE error costs for its two consumers, so it delivers TWO views over the same query:
///
/// - `precise` - the ranked, capped view (definers ranked above referencers) that seeds an
///   agent's prompt. A spurious extra file here merely wastes a little context, so precision
///   is what it optimizes for.
/// - `safe` - the SAFE-SUPERSET view (the union of the structural view and grep, uncapped) that
///   the conductor partitions and routes review tiers by. `partition_by_blast_radius` co-schedules
///   two units only when their file sets are DISJOINT, so a MISSED reference could co-schedule two
///   conflicting units in one parallel batch. Over-inclusion is the safe error; this view is
///   therefore never narrower than the grep radius it augments and is never capped.
///
/// `serialize` is the fail-safe for a HUB symbol - one whose per-language reference degree
/// exceeds the repo's degree-distribution percentile (5.5.2). Rather than truncating its huge
/// (often whole-repo) file set, a hub radius is flagged conflict-with-everything: the partitioning
/// consumer (unit 3) places such a unit in its own batch instead of co-scheduling it. Correctness
/// is kept, parallelism reduced - never the reverse. `safe` still carries the real files (never
/// truncated); `serialize` only tells the consumer to conflict this radius against all others.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BlastRadius {
    /// The precise / ranked view: definer files first, then referencer files, capped at `k`.
    pub precise: Vec<String>,
    /// The safe-superset view: the union of `precise` and grep, uncapped - always a superset of the
    /// grep radius.
    pub safe: Vec<String>,
    /// Whether this radius must serialize (conflict-with-everything) because the queried symbol
    /// is a hub. The partitioning consumer never co-schedules a serialize radius; the files in
    /// `safe` are NOT truncated when this is set.
    pub serialize: bool,
}

/// Grounder returns up to k locations relevant to a query.
pub trait Grounder: Send + Sync {
    fn ground(&self, query: &str, k: usize) -> Vec<Ref>;

    /// Re-index the given files after a unit integrates, so the next agent grounds
    /// on the accepted code (the `symbols` grounder freshens the changed files). The
    /// default is a no-op - grep re-reads the tree each time and needs no index.
    fn reindex(&self, _src_dir: &str, _files: &[String]) {}

    /// The two-view blast radius of `query` (architecture 5.5.1, spec 16). The DEFAULT impl - the
    /// one a grep / nop grounder inherits - returns this grounder's OWN top-`k` radius
    /// (the distinct files it grounds, in ground order) as BOTH views and never serializes. So a
    /// non-symbols grounder's blast radius is EXACTLY its grep/top-k radius: `precise == safe`, no
    /// hub composition, no extra work. This is what keeps unit 3's symbols-inactive `grounded_seed`
    /// (which reads `precise`) byte-for-byte unchanged - it is the same `ground(query, k)` file set
    /// it produces today. Only the `symbols` grounder overrides this to union the structural
    /// cross-reference graph with an uncapped grep and to flag hub symbols as serialize.
    fn blast_radius(&self, query: &str, k: usize) -> BlastRadius {
        let mut files: Vec<String> = Vec::new();
        for r in self.ground(query, k) {
            if !files.contains(&r.file) {
                files.push(r.file);
            }
        }
        BlastRadius {
            precise: files.clone(),
            safe: files,
            serialize: false,
        }
    }

    /// A provenance stamp for the `BlastRadiusComputed` audit event (spec 16 unit 3,
    /// architecture 5.5.9): the index content-hash + grammar / tag-query version that
    /// produced this grounder's radii, so a recorded radius reconstructs which index state
    /// grounded it and staleness is answerable ("why the full panel?"). It is ALSO the
    /// structural-active signal unit 3's conductor keys the audit off: the DEFAULT (grep /
    /// nop - no structural cross-reference index) returns an EMPTY stamp, so the
    /// conductor emits NO audit event and drives NO retention metric on that path, keeping the
    /// shipped default byte-for-byte unchanged. Only the `symbols` grounder overrides this to a
    /// non-empty `<index-content-hash>/<grammar-tags-version>` stamp.
    fn index_stamp(&self) -> String {
        String::new()
    }
}

/// Nop grounds nothing.
pub struct Nop;

impl Grounder for Nop {
    fn ground(&self, _query: &str, _k: usize) -> Vec<Ref> {
        Vec::new()
    }
}

/// Whether a configured grounder name is a RETIRED engine (spec 57): the vector-embedding engine
/// (`"turbovec"` and its `"vector"` alias) and the `"hybrid"` mode whose whole purpose was
/// composing those vectors onto the structural symbols index. The retention question was settled by
/// measurement - the vector index's retrieval hit-set was identical to the knowledge graph's own
/// structural retrieval, and it was invoked zero times across an A/B workload - so the engine left
/// the tree. These names are no longer accepted: [`grounder_for`] rejects them with a migration
/// error ([`retired_grounder_error`]), never a silent degrade to grep. An UNSET / empty name is NOT
/// retired - it resolves to the `symbols` default.
pub fn is_retired_grounder(name: &str) -> bool {
    matches!(
        name.trim().to_lowercase().as_str(),
        "turbovec" | "vector" | "hybrid"
    )
}

/// The loud MIGRATION error returned when a RETIRED grounder name (`turbovec` / `vector` /
/// `hybrid`) is configured (spec 57). Selecting a grounder must NEVER silently degrade, so a
/// request for a retired engine is REJECTED with a message that names the retirement AND the
/// surviving default (`symbols`) - so an operator carrying an old config is pointed straight at the
/// new truth instead of guessing. The structural `symbols` grounder is now the default and the
/// lookup surface; `grep` and `nop` remain explicit opt-ins.
pub fn retired_grounder_error(name: &str) -> String {
    format!(
        "grounder {name:?} was retired: the structural `symbols` grounder is now the default and \
         the lookup surface. Set `defaults.grounder: symbols` (or leave it unset), or choose \
         `grep` / `nop` explicitly."
    )
}

/// The loud error returned when the `symbols` grounder is selected - by the explicit name OR the
/// UNSET / empty default, since `symbols` is the default - but this binary was built WITHOUT the
/// `symbols` feature (the structural index and its grammars). Selecting a grounder must NEVER
/// silently degrade to grep (a configured grounder never quietly becomes something else), so this
/// is surfaced to the caller, which fails the process. When the feature IS built,
/// `main::select_grounder` resolves the default / `symbols` to the real `Symbols` grounder BEFORE
/// delegating here, so this arm is reached only by a feature-off binary (or a direct call).
pub fn symbols_feature_missing_error() -> String {
    "grounder \"symbols\" (the default) is selected but this binary was built without the symbols \
     feature; rebuild with the default features, or set `defaults.grounder: grep` explicitly to \
     use the literal grep grounder"
        .to_string()
}

/// Select a grounder by the configured `defaults.grounder` name, rooted at `root`
/// (§3.2, §5.4, R4). This is the FEATURE-INDEPENDENT part of the choice and the single
/// name-contract authority (spec 57): the accepted-name set is EXACTLY `symbols` / `grep` / `nop`.
/// - `"nop"` -> [`Nop`];
/// - `"grep"` -> [`Grep`] (the literal grounder, reachable ONLY when named explicitly);
/// - the UNSET / empty default AND the explicit name `"symbols"` resolve to the structural
///   `symbols` grounder, which is the DEFAULT and the lookup surface. When the `symbols` feature is
///   built, `src/main.rs::select_grounder` wires the real grounder before delegating here; here
///   (feature-independent) they are a LOUD feature-missing error, never a silent grep degrade.
/// - the RETIRED names (`"turbovec"` / `"vector"` / `"hybrid"`, spec 57) are REJECTED with a
///   migration error naming the retirement and the `symbols` default - never a silent grep fallback
///   and never the generic `unknown grounder` message (which would misread a retired config as a
///   typo).
/// - any other (unknown) name is a hard error advertising the exact accepted set - never a silent
///   grep fallback.
pub fn grounder_for(name: &str, root: &str) -> Result<Box<dyn Grounder>, String> {
    match name.trim().to_lowercase().as_str() {
        "nop" => Ok(Box::new(Nop)),
        "grep" => Ok(Box::new(Grep { root: root.into() })),
        // The UNSET / empty default and the explicit `symbols` name resolve to the structural
        // default; `select_grounder` wires the real grounder when the feature is built, so here (the
        // feature-independent resolver) they are the loud feature-missing error, never a silent grep.
        "" | "symbols" => Err(symbols_feature_missing_error()),
        // The retired vector engine and its symbols-composite are rejected with a migration error
        // (retirement + symbols default), never a silent grep degrade and never `unknown grounder`.
        _ if is_retired_grounder(name) => Err(retired_grounder_error(name)),
        other => Err(format!(
            "unknown grounder {other:?}; valid names are symbols (default), grep, nop"
        )),
    }
}

/// Grep is the self-contained literal grounder: a case-insensitive substring
/// search over the tree, skipping VCS and build dirs.
pub struct Grep {
    pub root: String,
}

impl Grounder for Grep {
    fn ground(&self, query: &str, k: usize) -> Vec<Ref> {
        if query.is_empty() || k == 0 {
            return Vec::new();
        }
        let needle = query.to_lowercase();
        let mut refs = Vec::new();
        // The scope (the project's own ignore rules, the always-excluded dotdirs, and root
        // confinement) lives in the SHARED `walk_guarded` skeleton - the same one the ingests
        // use - so no walk can drift from another; this walk's ONLY leaf action is to search
        // each file's lines, stopping once it has `k` hits.
        let _ = walk_guarded(Path::new(&self.root), &mut |path| {
            search_file(path, &self.root, &needle, k, &mut refs);
            // Stop the whole walk once we have collected the requested k hits - the
            // early-out that keeps grep from scanning the rest of the tree once full.
            if refs.len() >= k {
                ControlFlow::Break(())
            } else {
                ControlFlow::Continue(())
            }
        });
        refs
    }
}

fn search_file(path: &Path, root: &str, needle: &str, k: usize, refs: &mut Vec<Ref>) {
    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(_) => return, // binary or unreadable
    };
    let rel = path
        .strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .into_owned();
    for (i, line) in content.lines().enumerate() {
        if refs.len() >= k {
            return;
        }
        if line.to_lowercase().contains(needle) {
            refs.push(Ref {
                file: rel.clone(),
                line: (i + 1) as u32,
                text: line.trim().to_string(),
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grep_finds_matching_lines() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("combat.rs"),
            "fn apply_damage() {}\nfn render() {}\n",
        )
        .unwrap();
        let g = Grep {
            root: dir.path().to_string_lossy().into_owned(),
        };
        let refs = g.ground("apply_damage", 5);
        assert!(refs.iter().any(|r| r.text.contains("apply_damage")));
        assert!(g.ground("apply_damage", 0).is_empty());
    }

    /// The DEFAULT `blast_radius` (the one a non-symbols grounder inherits) is EXACTLY the
    /// grounder's own top-`k` radius: `precise == safe` = the distinct files it grounds, and it
    /// NEVER serializes. This is the contract that keeps unit 3's symbols-inactive `grounded_seed`
    /// (which reads `precise`) byte-for-byte unchanged - it is the same `ground(query, k)` file set.
    /// This test is ungated: it holds identically in both feature lanes because the default impl
    /// touches no structural index.
    #[test]
    fn default_blast_radius_is_the_grounders_own_top_k_radius_both_views_never_serialize() {
        let dir = tempfile::tempdir().unwrap();
        // Two files both matching the needle so the radius has more than one file.
        std::fs::write(dir.path().join("combat.rs"), "fn apply_damage() {}\n").unwrap();
        std::fs::write(
            dir.path().join("notes.rs"),
            "// apply_damage is called here\n",
        )
        .unwrap();
        let g = Grep {
            root: dir.path().to_string_lossy().into_owned(),
        };

        let br = g.blast_radius("apply_damage", 8);
        // The default radius is EXACTLY these two grep-matched files - a CONCRETE expected list, not
        // a re-run of the impl's own dedup loop (which would pass tautologically for any impl). Grep
        // walks the tree in unsorted `read_dir` order, so compare the SET (sorted); the two views'
        // element-for-element ORDER equality is pinned separately just below.
        let mut got = br.precise.clone();
        got.sort();
        assert_eq!(
            got,
            vec!["combat.rs".to_string(), "notes.rs".to_string()],
            "the default radius is exactly the two files grep matches; got {br:?}"
        );
        // Both views are the SAME grep radius - equal element-for-element, in the same order (safe is
        // the trivial superset of precise on the default path) - and it never serializes.
        assert_eq!(
            br.precise, br.safe,
            "the default safe view equals the precise view (grep radius, a trivial superset)"
        );
        assert!(!br.serialize, "the default path never serializes");

        // An empty query / k=0 grounds nothing, so both views are empty (the empty fail-safe).
        let empty = g.blast_radius("apply_damage", 0);
        assert!(empty.precise.is_empty() && empty.safe.is_empty() && !empty.serialize);
    }

    /// The shared walk (here via the grep grounder, the ungated default) scopes to the project's
    /// own sources. It skips the ALWAYS-EXCLUDED hidden dotdirs - the VCS metadata `.git`, rigger's
    /// runtime `.rigger`, and the tooling dotdirs (`.fastembed_cache` the ~128 MB model cache,
    /// `.github`/`.cargo`/`.claude`) - AND everything the repository's OWN `.gitignore` excludes
    /// (build outputs, dependency trees), while still finding every in-root source file. We seed a
    /// source file plus the SAME needle inside each denied dir and assert the hits come ONLY from
    /// the source file. Ungated: it holds identically in BOTH feature lanes (the walk is ungated).
    #[test]
    fn grep_walk_scopes_to_the_project_via_gitignore_and_the_always_excluded_dotdirs() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        // A genuine in-root source file whose line MUST be found.
        std::fs::write(root.join("lib.rs"), "fn find_me() {}\n").unwrap();
        // The project's OWN version-control ignore rules exclude these build / dependency trees.
        std::fs::write(
            root.join(".gitignore"),
            "target/\nnode_modules/\nvendor/\nbuild/\n",
        )
        .unwrap();
        // Two classes of denied dir, each seeded with the SAME needle so any leak surfaces:
        //  - hidden dotdirs, always excluded regardless of any ignore file (`.git`/`.rigger` are
        //    the two the spec names; the rest are tooling dotdirs);
        //  - directories the repository's own `.gitignore` names.
        let hidden_dotdirs = [
            ".git",
            ".rigger",
            ".fastembed_cache",
            ".github",
            ".cargo",
            ".claude",
        ];
        let gitignored = ["target", "node_modules", "vendor", "build"];
        for denied in hidden_dotdirs.iter().chain(gitignored.iter()) {
            let sub = root.join(denied);
            std::fs::create_dir_all(&sub).unwrap();
            std::fs::write(sub.join("blob.txt"), "fn find_me() {}\n").unwrap();
        }

        let g = Grep {
            root: root.to_string_lossy().into_owned(),
        };
        // Ask for many hits so nothing is dropped by the k cap - if a denied dir were
        // walked, its match would appear here.
        let refs = g.ground("find_me", 100);
        assert!(
            !refs.is_empty(),
            "the real source file's match must be found"
        );
        for r in &refs {
            assert!(
                hidden_dotdirs
                    .iter()
                    .chain(gitignored.iter())
                    .all(|d| !r.file.starts_with(d)),
                "grep must not descend into a denied dir; leaked {r:?}"
            );
        }
        // Exactly the one in-root source file matched, once.
        assert_eq!(
            refs.iter().map(|r| r.file.as_str()).collect::<Vec<_>>(),
            vec!["lib.rs"],
            "only the first-party in-root source should match; got {refs:?}"
        );
    }

    /// The grep grounder's walk must TERMINATE on a directory symlink CYCLE rather than
    /// loop forever / blow the stack. We build a real cycle - `sub/loop -> root` (a link
    /// back to an ancestor) - and assert the walk returns, finds the real match, and
    /// does not re-enter through the link. Root confinement makes this hold BY CONSTRUCTION:
    /// the walk never follows a symlink, so `sub/loop` is simply not traversed and a cycle can
    /// never form. A hang here (a regression that started following links) fails the test by
    /// timeout.
    #[test]
    fn grep_walk_terminates_on_a_symlink_cycle() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::write(root.join("real.rs"), "fn only_once() {}\n").unwrap();
        let sub = root.join("sub");
        std::fs::create_dir(&sub).unwrap();
        std::fs::write(sub.join("nested.rs"), "fn nested_once() {}\n").unwrap();
        // A directory symlink pointing back up at the root: following `sub/loop` would re-enter
        // the whole tree and recurse forever. The scoped walk does not follow it.
        std::os::unix::fs::symlink(root, sub.join("loop")).unwrap();

        let g = Grep {
            root: root.to_string_lossy().into_owned(),
        };
        // The walk must RETURN (a hang here fails the test by timeout) and find each
        // real match exactly once, never re-collecting it through the cycle.
        let only_once = g.ground("only_once", 100);
        assert_eq!(
            only_once.iter().filter(|r| r.file == "real.rs").count(),
            1,
            "the top-level match must be found exactly once, not re-entered via the cycle"
        );
        let nested = g.ground("nested_once", 100);
        assert_eq!(
            nested.iter().filter(|r| r.file == "sub/nested.rs").count(),
            1,
            "the nested match must be found exactly once, not re-entered via the cycle"
        );
    }

    /// A non-structural grounder (grep / nop / the trait default) has NO cross-reference index,
    /// so its `index_stamp` is EMPTY. That empty stamp is the signal unit 3's conductor reads to
    /// emit NO `BlastRadiusComputed` audit and drive NO retention metric on this path - the exact
    /// mechanism that keeps the shipped (non-symbols) default byte-for-byte unchanged. Ungated: it
    /// holds identically in both feature lanes (the default impl touches no structural index).
    #[test]
    fn default_index_stamp_is_empty_so_the_default_path_drives_no_audit() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.rs"), "fn foo() {}\n").unwrap();
        let g = Grep {
            root: dir.path().to_string_lossy().into_owned(),
        };
        assert!(
            g.index_stamp().is_empty(),
            "grep is not structural; an empty stamp keeps the default path byte-for-byte unchanged"
        );
        assert!(
            Nop.index_stamp().is_empty(),
            "nop grounds nothing and stamps nothing"
        );
    }

    #[test]
    fn grounder_for_selects_by_name() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("combat.rs"),
            "fn apply_damage() {}\nfn render() {}\n",
        )
        .unwrap();
        let root = dir.path().to_string_lossy().into_owned();

        // nop grounds nothing.
        assert!(grounder_for("nop", &root)
            .expect("nop is always available")
            .ground("apply_damage", 5)
            .is_empty());

        // grep grounds for real, but ONLY when named explicitly.
        let refs = grounder_for("grep", &root)
            .expect("grep is always available")
            .ground("apply_damage", 5);
        assert!(
            refs.iter().any(|r| r.text.contains("apply_damage")),
            "the explicit grep grounder should find the line"
        );
    }

    /// `is_retired_grounder` matches EXACTLY the retired names (`turbovec` / `vector` / `hybrid`),
    /// case-insensitively and whitespace-trimmed - and NOTHING else. The UNSET / empty default is
    /// NOT retired (it is the `symbols` default), and neither are the accepted `symbols` / `grep` /
    /// `nop` names. This is the single predicate `grounder_for` keys its migration-error arm off.
    #[test]
    fn is_retired_grounder_matches_only_the_retired_names() {
        for name in [
            "turbovec",
            "vector",
            "hybrid",
            "TurboVec",
            "VECTOR",
            "  Hybrid ",
        ] {
            assert!(
                is_retired_grounder(name),
                "{name:?} is a retired grounder name"
            );
        }
        // The unset default and every accepted name are NOT retired.
        for name in ["", "  ", "symbols", "SYMBOLS", "grep", "nop"] {
            assert!(
                !is_retired_grounder(name),
                "{name:?} is the default or an accepted name, not retired"
            );
        }
    }

    /// A retired name (`turbovec` / `vector` / `hybrid`) yields the MIGRATION error, not the old
    /// "rebuild with the feature" message and not a silent grep degrade: it names the retirement AND
    /// the `symbols` default, so an operator on an old config is pointed at the new truth. Holds in
    /// BOTH feature lanes (the resolver is feature-independent), so it is ungated.
    #[test]
    fn retired_names_give_the_migration_error_naming_the_symbols_default() {
        for name in ["turbovec", "vector", "hybrid"] {
            let err = grounder_for(name, "/tmp")
                .err()
                .unwrap_or_else(|| panic!("{name:?} must be a loud migration error, never grep"));
            let low = err.to_lowercase();
            assert!(
                low.contains("retire") && low.contains("symbols"),
                "the migration error must name the retirement and the symbols default; got: {err}"
            );
            assert!(
                !low.contains("feature"),
                "a retired name is gone for good - not a missing feature to rebuild; got: {err}"
            );
            assert!(
                !err.contains("unknown grounder"),
                "a retired name must not read as a typo; got: {err}"
            );
        }
        // An unknown name is ALSO a hard error, not a silent grep fallback.
        assert!(grounder_for("bogus-grounder", "/tmp").is_err());
        // grep / nop still resolve fine.
        assert!(grounder_for("grep", "/tmp").is_ok());
        assert!(grounder_for("nop", "/tmp").is_ok());
    }

    /// The feature-INDEPENDENT resolver never returns a `Symbols` grounder: `symbols` (and the unset
    /// default) is a LOUD error here (naming the feature), never a silent grep degrade. When the
    /// `symbols` feature IS built, `main::select_grounder` intercepts it first; this arm is the
    /// feature-off behavior. It holds identically in BOTH feature lanes (this resolver is
    /// feature-independent), so the test is ungated.
    #[test]
    fn symbols_without_the_feature_is_a_loud_error_not_a_grep_fallback() {
        let err = grounder_for("symbols", ".")
            .err()
            .expect("symbols must be a loud error in the feature-independent resolver");
        assert!(
            err.to_lowercase().contains("symbols")
                && err.contains("feature")
                && err.contains("grep"),
            "the loud error must name symbols, the feature, and the grep opt-out; got: {err}"
        );
    }

    /// `defaults.grounder: hybrid` is RETIRED (spec 57): hybrid's whole purpose was composing the
    /// vector engine onto symbols, and the vector engine is gone. So it must yield the MIGRATION
    /// error (retirement + symbols default), NOT the old "rebuild with the symbols feature" message
    /// and NOT the generic `unknown grounder` message (which would misread a retired config as a
    /// typo). Case-insensitive and whitespace-trimmed like every other resolver name. Holds in BOTH
    /// feature lanes (the resolver is feature-independent), so it is ungated.
    #[test]
    fn hybrid_is_retired_and_gives_the_migration_error_not_a_feature_or_typo_error() {
        for name in ["hybrid", "  Hybrid "] {
            let err = grounder_for(name, ".")
                .err()
                .expect("hybrid is retired - a loud migration error, never grep");
            let low = err.to_lowercase();
            assert!(
                low.contains("retire") && low.contains("symbols"),
                "hybrid's error must name the retirement and the symbols default; got: {err}"
            );
            assert!(
                !low.contains("feature"),
                "hybrid is retired, not a missing feature to rebuild; got: {err}"
            );
            assert!(
                !err.contains("unknown grounder"),
                "hybrid must NOT read as a typo; got: {err}"
            );
        }
    }

    /// Spec 57 (DEFAULT GROUNDER, criterion 1) - the selection surface contract, driven through the
    /// single feature-independent resolver `grounder_for`:
    /// - the DEFAULT is the structural `symbols` grounder: an UNSET / empty `defaults.grounder` and
    ///   the explicit `symbols` name resolve to the SAME symbols branch, never the retired turbovec
    ///   engine and never a silent grep degrade;
    /// - the retired names (`turbovec` / `vector` / `hybrid`) are REJECTED with a migration error
    ///   naming the retirement AND the symbols default, never the generic `unknown grounder` message;
    /// - the accepted-name set is EXACTLY `symbols` / `grep` / `nop`, and the unknown-name message
    ///   advertises exactly that set (no retired name).
    ///
    /// It is ungated: `grounder_for` is feature-independent, so it holds identically in both lanes.
    #[test]
    fn default_grounder_selection_and_retired_names_contract() {
        // The default IS symbols: unset and the explicit name resolve to the same symbols branch
        // (a loud feature-missing error in this feature-independent resolver, wired to the real
        // grounder in `select_grounder`) - never turbovec, never a silent grep degrade.
        for name in ["", "  ", "symbols", "SYMBOLS", " Symbols "] {
            let err = grounder_for(name, ".").err().unwrap_or_else(|| {
                panic!("{name:?} resolves to the symbols default (loud without the feature)")
            });
            assert!(
                err.to_lowercase().contains("symbols"),
                "the unset/symbols default must name symbols; got: {err}"
            );
            assert!(
                !err.to_lowercase().contains("turbovec"),
                "the default is symbols, not the retired turbovec engine; got: {err}"
            );
        }

        // The retired names error with a MIGRATION message (retirement + symbols default), never a
        // silent grep degrade and never the generic unknown-grounder message.
        for name in ["turbovec", "vector", "hybrid", "TurboVec", " Hybrid "] {
            let err = grounder_for(name, ".").err().unwrap_or_else(|| {
                panic!("{name:?} must be rejected, never silently degrade to grep")
            });
            let low = err.to_lowercase();
            assert!(
                low.contains("retire"),
                "a retired name's error must name the retirement; got: {err}"
            );
            assert!(
                low.contains("symbols"),
                "a retired name's error must name the symbols default; got: {err}"
            );
            assert!(
                !err.contains("unknown grounder"),
                "a retired name must not hit the generic unknown-grounder arm; got: {err}"
            );
        }

        // The accepted-name set is EXACTLY symbols / grep / nop.
        assert!(grounder_for("grep", ".").is_ok());
        assert!(grounder_for("nop", ".").is_ok());
        for rejected in ["turbovec", "hybrid", "vector", "bogus", "sem", "embed"] {
            assert!(
                grounder_for(rejected, ".").is_err(),
                "{rejected:?} is not an accepted name"
            );
        }
        let unknown = grounder_for("bogus-grounder", ".").err().unwrap();
        assert!(
            unknown.contains("unknown grounder"),
            "a typo hits the unknown arm; got: {unknown}"
        );
        assert!(
            unknown.contains("symbols") && unknown.contains("grep") && unknown.contains("nop"),
            "the unknown-name message lists the accepted set; got: {unknown}"
        );
        assert!(
            !unknown.contains("turbovec") && !unknown.contains("hybrid"),
            "the unknown-name message must not advertise retired names; got: {unknown}"
        );
    }
}
