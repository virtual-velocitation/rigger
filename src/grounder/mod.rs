//! Grounding gives each agent only the context it needs: the locations relevant
//! to its task. `Grounder` is the port. `Grep` is the self-contained literal
//! default; the real turbovec engine (semantic vector search) plugs in behind the
//! same trait under the `turbovec` feature.

#[cfg(feature = "turbovec")]
pub mod turbovec;

use std::path::Path;

/// A relevant location: a file, a line, and a snippet.
#[derive(Clone, Debug)]
pub struct Ref {
    pub file: String,
    pub line: u32,
    pub text: String,
}

/// Grounder returns up to k locations relevant to a query.
pub trait Grounder: Send + Sync {
    fn ground(&self, query: &str, k: usize) -> Vec<Ref>;

    /// Re-index the given files after a unit integrates, so the next agent grounds
    /// on the accepted code (turbovec reindexDelta). The default is a no-op - grep
    /// re-reads the tree each time and needs no index.
    fn reindex(&self, _src_dir: &str, _files: &[String]) {}
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
        other => Err(format!(
            "unknown grounder {other:?}; valid names are turbovec (default), grep, nop"
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
        walk(Path::new(&self.root), &self.root, &needle, k, &mut refs);
        refs
    }
}

fn skip_dir(name: &str) -> bool {
    matches!(
        name,
        ".git" | ".rigger" | "vendor" | "target" | "node_modules"
    )
}

fn walk(dir: &Path, root: &str, needle: &str, k: usize, refs: &mut Vec<Ref>) {
    if refs.len() >= k {
        return;
    }
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        if refs.len() >= k {
            return;
        }
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if path.is_dir() {
            if !skip_dir(&name) {
                walk(&path, root, needle, k, refs);
            }
        } else {
            search_file(&path, root, needle, k, refs);
        }
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
}
