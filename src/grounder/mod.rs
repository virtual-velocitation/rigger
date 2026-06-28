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

/// Select a grounder by the configured `defaults.grounder` name, rooted at `root`
/// (§3.2, §5.4, R4). This is the feature-independent part of the choice:
/// `"nop"` -> [`Nop`]; `"grep"` or empty (the default) -> [`Grep`]. The semantic
/// names (`"vector"` / `"turbovec"`) resolve to the turbovec engine only when the
/// `turbovec` feature is built; this function therefore falls back to grep for
/// them with a stderr note, and `src/main.rs` overrides that path when the feature
/// is on. An unknown name also falls back to grep with a note.
pub fn grounder_for(name: &str, root: &str) -> Box<dyn Grounder> {
    match name.to_lowercase().as_str() {
        "nop" => Box::new(Nop),
        "grep" | "" => Box::new(Grep { root: root.into() }),
        "vector" | "turbovec" => {
            eprintln!(
                "rigger: grounder {name:?} needs the turbovec feature (not built); falling back to grep"
            );
            Box::new(Grep { root: root.into() })
        }
        other => {
            eprintln!("rigger: unknown grounder {other:?}; falling back to grep");
            Box::new(Grep { root: root.into() })
        }
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
            .ground("apply_damage", 5)
            .is_empty());

        // grep and the empty default both ground for real.
        for name in ["grep", ""] {
            let refs = grounder_for(name, &root).ground("apply_damage", 5);
            assert!(
                refs.iter().any(|r| r.text.contains("apply_damage")),
                "grounder {name:?} should find the line"
            );
        }
    }
}
