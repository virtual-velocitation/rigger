//! Grounding gives each agent only the context it needs: the locations relevant
//! to its task. `Grounder` is the port. `Grep` is the self-contained literal
//! default; the real turbovec engine (semantic vector search) plugs in behind the
//! same trait under the `turbovec` feature.

#[cfg(feature = "turbovec")]
pub mod turbovec;

// The structural grounding axis (spec 15): a symbol index projected from the code tree.
// Declared UNGATED on purpose - the parser-free data model (`symbols::model`) must compile
// in the light (`--no-default-features`) lane, where tree-sitter is not even linked, which
// is the compile-time proof that no tree-sitter type crosses into the model API. Only the
// tree-sitter-touching submodules inside `symbols` carry `#[cfg(feature = "symbols")]`.
pub mod symbols;

use std::collections::HashSet;
use std::ops::ControlFlow;
use std::path::{Path, PathBuf};

/// The single source of truth for which directories BOTH grounders skip: VCS / build /
/// dependency dirs plus non-code tooling dotdirs that hold no first-party source.
///
/// This lives in `mod.rs` (ALWAYS compiled) rather than in the feature-gated
/// `turbovec.rs`, so grep's `walk()` and turbovec's `collect_files` consume ONE list
/// and can never drift. This change ADDS the model-cache + tooling-dotdir denies to BOTH
/// walks: previously both shared the same narrower 5-entry list (`.git`, `.rigger`,
/// `vendor`, `target`, `node_modules`), so NEITHER denied the ~128 MB `.fastembed_cache`
/// model blobs - both grep (the `defaults.grounder: grep` fallback) and turbovec would
/// index them.
///
/// - `.git` / `vendor` / `target` / `node_modules` - VCS + build + dependency trees.
/// - `.rigger` - our own event store / grounding index / config, not source.
/// - `.fastembed_cache` - the ~128 MB embedding-model cache fastembed writes at the
///   repo root (default, or at `FASTEMBED_CACHE_DIR`); indexing it makes every walk
///   hash 128 MB and surfaces the cache's JSON blobs as grounding hits.
/// - `.github` / `.cargo` / `.claude` - non-code dotdirs (CI config, cargo config,
///   agent config) that pollute the index without grounding value.
pub(crate) const SKIP_DIRS: &[&str] = &[
    ".git",
    ".rigger",
    ".fastembed_cache",
    ".github",
    ".cargo",
    ".claude",
    "vendor",
    "target",
    "node_modules",
];

/// Whether a directory named `name` is one BOTH grounders skip (see [`SKIP_DIRS`]).
pub(crate) fn is_skipped_dir(name: &str) -> bool {
    SKIP_DIRS.contains(&name)
}

/// The ONE guarded directory-walk skeleton BOTH grounders share: grep's `walk` (this
/// module) and turbovec's `collect_files` (the `turbovec` feature). The two used to
/// each reimplement the identical canonicalize + visited-canonical-path cycle guard +
/// [`SKIP_DIRS`] check; they now differ ONLY in the per-file LEAF ACTION they pass as
/// `on_file` - grep searches the file's lines, turbovec collects `(rel_path, content)`.
/// Factoring the skeleton here (always compiled, in `mod.rs`) means the cycle guard and
/// the skip-list can never drift between the two walks.
///
/// A directory symlink is descended at most once per canonical target: `visited` holds
/// the canonicalized path of every directory already entered, so a symlink CYCLE
/// (`a -> b`, `b -> a`, or a link back to an ancestor) terminates instead of looping
/// forever / blowing the stack. A target that cannot be canonicalized (a broken link,
/// a permissions race) falls back to the literal path so a real directory is still read.
///
/// `on_file` returns [`ControlFlow`]: `Break(())` stops the whole walk immediately
/// (grep uses this to stop once it has collected its `k` hits), `Continue(())` walks on.
/// The return value propagates up the recursion, so a `Break` unwinds the entire walk.
pub(crate) fn walk_guarded<F>(
    dir: &Path,
    visited: &mut HashSet<PathBuf>,
    on_file: &mut F,
) -> ControlFlow<()>
where
    F: FnMut(&Path) -> ControlFlow<()>,
{
    // Canonicalize this dir and record it BEFORE descending. If canonicalization fails
    // (permissions, a race) fall back to the literal path so a real dir is still read;
    // if the canonical path was already visited, a symlink pointed us back into a
    // subtree we are already walking - stop, or we would loop forever.
    let canonical = std::fs::canonicalize(dir).unwrap_or_else(|_| dir.to_path_buf());
    if !visited.insert(canonical) {
        return ControlFlow::Continue(());
    }
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return ControlFlow::Continue(()),
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if path.is_dir() {
            // Skip the shared VCS / build / dependency / tooling dirs (see `SKIP_DIRS`),
            // the same set both grounders deny, so the two walks never diverge.
            if !is_skipped_dir(&name) {
                walk_guarded(&path, visited, on_file)?;
            }
        } else {
            on_file(&path)?;
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
    /// on the accepted code (turbovec reindexDelta). The default is a no-op - grep
    /// re-reads the tree each time and needs no index.
    fn reindex(&self, _src_dir: &str, _files: &[String]) {}

    /// The two-view blast radius of `query` (architecture 5.5.1, spec 16). The DEFAULT impl - the
    /// one a grep / turbovec / nop grounder inherits - returns this grounder's OWN top-`k` radius
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
}

/// Nop grounds nothing.
pub struct Nop;

impl Grounder for Nop {
    fn ground(&self, _query: &str, _k: usize) -> Vec<Ref> {
        Vec::new()
    }
}

/// Whether a configured grounder name resolves to the turbovec (semantic) engine:
/// the explicit `"turbovec"` / `"vector"` aliases, OR an UNSET / empty name - because
/// turbovec is the default grounder (§3.2, R4). Grep is reachable ONLY when the user
/// writes `grounder: grep` explicitly; it is never the silent default.
pub fn resolves_to_turbovec(name: &str) -> bool {
    matches!(
        name.trim().to_lowercase().as_str(),
        "" | "turbovec" | "vector"
    )
}

/// The loud error returned when the configured / default grounder is turbovec but
/// this binary was built WITHOUT the `turbovec` feature. Selecting a grounder must
/// NEVER silently degrade to grep - that is exactly what hid turbovec being absent
/// for a whole session - so this is surfaced to the caller, which fails the process.
pub fn turbovec_feature_missing_error(name: &str) -> String {
    let shown = if name.trim().is_empty() {
        "<unset, defaults to turbovec>".to_string()
    } else {
        format!("{name:?}")
    };
    format!(
        "grounder {shown} is configured/default but this binary was built without the \
         turbovec feature; rebuild with the default features (and install OpenBLAS), or \
         set `defaults.grounder: grep` explicitly to use the literal grep grounder"
    )
}

/// The loud error returned when `defaults.grounder: symbols` is configured but this binary was
/// built WITHOUT the `symbols` feature (the structural index and its grammars). Selecting a
/// grounder must NEVER silently degrade to grep - the same no-silent-degrade rule as turbovec -
/// so this is surfaced to the caller, which fails the process. When the feature IS built,
/// `main::select_grounder` resolves `symbols` to the real `Symbols` grounder BEFORE delegating
/// here, so this arm is reached only by a feature-off binary (or a direct call).
pub fn symbols_feature_missing_error() -> String {
    "grounder \"symbols\" is configured but this binary was built without the symbols feature; \
     rebuild with the default features, or set `defaults.grounder: grep` explicitly to use the \
     literal grep grounder"
        .to_string()
}

/// Select a grounder by the configured `defaults.grounder` name, rooted at `root`
/// (§3.2, §5.4, R4). This is the FEATURE-INDEPENDENT part of the choice and the
/// grep-only build's resolver:
/// - `"nop"` -> [`Nop`];
/// - `"grep"` -> [`Grep`] (the literal grounder, reachable ONLY when named explicitly);
/// - the turbovec names (`"turbovec"` / `"vector"`) AND the UNSET / empty default
///   resolve to turbovec, which is the default grounder. When the `turbovec` feature
///   is built, `src/main.rs::select_grounder` handles these names before delegating
///   here; when it is NOT built, this function returns a LOUD error rather than
///   silently degrading to grep.
/// - any other (unknown) name is a hard error - never a silent grep fallback.
pub fn grounder_for(name: &str, root: &str) -> Result<Box<dyn Grounder>, String> {
    match name.trim().to_lowercase().as_str() {
        "nop" => Ok(Box::new(Nop)),
        "grep" => Ok(Box::new(Grep { root: root.into() })),
        _ if resolves_to_turbovec(name) => Err(turbovec_feature_missing_error(name)),
        // `symbols` resolves to the real grounder in `select_grounder` when the feature is built;
        // here (the feature-independent resolver) it is a LOUD error, never a silent grep degrade.
        "symbols" => Err(symbols_feature_missing_error()),
        other => Err(format!(
            "unknown grounder {other:?}; valid names are turbovec (default), symbols, grep, nop"
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
        // Carry a canonical-path visited set so a directory symlink CYCLE (`a -> b`,
        // `b -> a`, or a link back to an ancestor) terminates instead of looping
        // forever / blowing the stack. The canonicalize + visited-set cycle guard and the
        // SKIP_DIRS check live in the SHARED `walk_guarded` skeleton - the same one
        // turbovec's `collect_files` uses - so the two walks can never drift; this walk's
        // ONLY leaf action is to search each file's lines, stopping once it has `k` hits.
        let mut visited = HashSet::new();
        let _ = walk_guarded(Path::new(&self.root), &mut visited, &mut |path| {
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

        // The default view is the DISTINCT files of `ground(query, k)`, in ground order.
        let want: Vec<String> = {
            let mut files: Vec<String> = Vec::new();
            for r in g.ground("apply_damage", 8) {
                if !files.contains(&r.file) {
                    files.push(r.file);
                }
            }
            files
        };
        assert!(want.len() >= 2, "the fixture should ground both files");

        let br = g.blast_radius("apply_damage", 8);
        // precise == safe == the grep radius, and no hub composition on the default path.
        assert_eq!(
            br.precise, want,
            "precise view is the grounder's top-k radius"
        );
        assert_eq!(
            br.safe, want,
            "the default safe view equals the precise view (grep radius, a trivial superset)"
        );
        assert!(!br.serialize, "the default path never serializes");

        // An empty query / k=0 grounds nothing, so both views are empty (the empty fail-safe).
        let empty = g.blast_radius("apply_damage", 0);
        assert!(empty.precise.is_empty() && empty.safe.is_empty() && !empty.serialize);
    }

    /// The grep grounder's walk must SKIP the shared denied dirs - in particular
    /// `.fastembed_cache` (the ~128 MB model cache): the documented
    /// `defaults.grounder: grep` fallback must not index the model blobs. Before the
    /// shared `SKIP_DIRS` list, grep's walk had a narrower 5-entry skip-list that let
    /// the cache through. We seed a source file plus a match inside each denied dir and
    /// assert the grep hits come ONLY from the source file.
    #[test]
    fn grep_walk_skips_the_shared_denied_dirs() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        // A genuine source file whose line MUST be found.
        std::fs::write(root.join("lib.rs"), "fn find_me() {}\n").unwrap();
        // A file inside each denied dir containing the SAME needle - it must NOT be
        // walked. `.fastembed_cache` stands in for the model cache.
        for denied in SKIP_DIRS {
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
                !SKIP_DIRS.iter().any(|d| r.file.starts_with(d)),
                "grep must not descend into a denied dir; leaked {r:?}"
            );
        }
        // Exactly the one source file matched, once.
        assert_eq!(
            refs.iter().map(|r| r.file.as_str()).collect::<Vec<_>>(),
            vec!["lib.rs"],
            "only the first-party source file should match; got {refs:?}"
        );
    }

    /// The grep grounder's walk must TERMINATE on a directory symlink CYCLE rather than
    /// loop forever / blow the stack. We build a real cycle - `sub/loop -> root` (a link
    /// back to an ancestor) - and assert the walk returns, finds the real match, and
    /// does not re-enter through the link. A hang here fails the test by timeout.
    #[test]
    fn grep_walk_terminates_on_a_symlink_cycle() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::write(root.join("real.rs"), "fn only_once() {}\n").unwrap();
        let sub = root.join("sub");
        std::fs::create_dir(&sub).unwrap();
        std::fs::write(sub.join("nested.rs"), "fn nested_once() {}\n").unwrap();
        // A directory symlink pointing back up at the root: walking into `sub/loop`
        // re-enters the whole tree, which without the cycle guard recurses forever.
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

    #[test]
    fn unset_and_turbovec_names_resolve_to_turbovec_not_grep() {
        // The empty / unset default and the turbovec aliases all resolve to turbovec
        // - grep is NEVER the silent default. In a grep-only build (this crate test
        // runs without the turbovec feature in the lib's own context), grounder_for
        // FAILS LOUDLY for them instead of degrading to grep.
        for name in ["", "  ", "turbovec", "vector", "TurboVec", "VECTOR"] {
            assert!(
                resolves_to_turbovec(name),
                "{name:?} must resolve to turbovec (the default grounder)"
            );
        }
        // grep / nop are NOT turbovec; they are explicit-only opt-ins.
        assert!(!resolves_to_turbovec("grep"));
        assert!(!resolves_to_turbovec("nop"));
    }

    #[test]
    fn grounder_for_fails_loudly_when_turbovec_is_unavailable() {
        // grounder_for is the grep-only resolver: the unset default and the turbovec
        // names must be a LOUD error here (the feature is not compiled into this
        // resolver), never a silent grep. The message must name turbovec, the missing
        // feature, and the explicit grep escape hatch.
        for name in ["", "turbovec", "vector"] {
            let err = grounder_for(name, "/tmp")
                .err()
                .unwrap_or_else(|| panic!("{name:?} must be a loud error without the feature"));
            assert!(
                err.contains("turbovec") && err.contains("feature") && err.contains("grep"),
                "the loud error must name turbovec, the feature, and the grep opt-out; got: {err}"
            );
        }
        // An unknown name is ALSO a hard error, not a silent grep fallback.
        assert!(grounder_for("bogus-grounder", "/tmp").is_err());
        // grep / nop still resolve fine.
        assert!(grounder_for("grep", "/tmp").is_ok());
        assert!(grounder_for("nop", "/tmp").is_ok());
    }

    /// The feature-INDEPENDENT resolver never returns a `Symbols` grounder: `symbols` is a LOUD
    /// error here (naming the feature), never a silent grep degrade - the same rule as turbovec.
    /// When the `symbols` feature IS built, `main::select_grounder` intercepts the name first; this
    /// arm is the feature-off behavior. It holds identically in BOTH feature lanes (this resolver
    /// is feature-independent), so the test is ungated.
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
}
