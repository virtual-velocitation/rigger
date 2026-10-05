//! The gate template sets `rigger init` scaffolds from (spec 113): each directory
//! `scaffold/<key>/` holds a `set.yml` (parsed at run time by `src/cli/setup.rs`) and a `files`
//! list of repository-relative paths, one per line, whose bytes `rigger init` copies into a
//! project matching the set. This module enumerates `scaffold/` ONCE and returns both the source
//! of `$OUT_DIR/template_sets.rs` - the `TEMPLATE_SETS` array `src/cli/setup.rs` includes - and
//! the paths `build.rs` watches, so the embedding and its re-run triggers can never disagree.
//!
//! It has no `fn main` and reads only the standard library, so `build.rs` and
//! `tests/template_sets_build.rs` include this same file by `#[path]`, as they do
//! `build/watch.rs`: a build script cannot run under `cargo test`, and a reimplementation in the
//! test would prove nothing about the build.

use std::path::{Component, Path, PathBuf};

/// What one enumeration of `scaffold/` produces: the generated Rust source and every path whose
/// change must re-run the build script.
pub struct GeneratedSets {
    pub source: String,
    pub watch_paths: Vec<PathBuf>,
}

/// Enumerate `root/scaffold/` in name order, skipping every entry that is not a directory, and
/// generate one `TemplateSet` per set directory: its key (the directory name), `include_str!` of
/// its `set.yml` and `include_str!` of each path its `files` lists (blank lines skipped). Each
/// embedded path is spelled relative to `CARGO_MANIFEST_DIR`, so the generated source does not
/// depend on where the repository is checked out. Refuses - naming `scaffold/` - a root with no
/// `scaffold/` or one holding no set directory; naming the set, a set directory missing `set.yml`
/// or `files`; and naming the set and the path, a listed path that is absolute, holds a `.` or
/// `..` segment, names no file or names the same components as an earlier listed path.
pub fn generate_template_sets(root: &Path) -> Result<GeneratedSets, String> {
    let scaffold = root.join("scaffold");
    let entries = std::fs::read_dir(&scaffold)
        .map_err(|e| format!("scaffold/: the gate template sets directory is unreadable: {e}"))?;
    let mut set_dirs = Vec::new();
    for entry in entries {
        let path = entry
            .map_err(|e| format!("scaffold/: an entry is unreadable: {e}"))?
            .path();
        if path.is_dir() {
            set_dirs.push(path);
        }
    }
    set_dirs.sort();
    if set_dirs.is_empty() {
        return Err("scaffold/ holds no gate template set directory".to_string());
    }
    let mut source = String::from("const TEMPLATE_SETS: &[TemplateSet] = &[\n");
    let mut watch_paths = vec![scaffold];
    for dir in set_dirs {
        let key = dir
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| format!("scaffold/: {} is not a UTF-8 set name", dir.display()))?
            .to_string();
        let refuse = |why: String| format!("gate template set {key}: {why}");
        for required in ["set.yml", "files"] {
            if !dir.join(required).is_file() {
                return Err(refuse(format!("missing scaffold/{key}/{required}")));
            }
        }
        let listed = std::fs::read_to_string(dir.join("files"))
            .map_err(|e| refuse(format!("scaffold/{key}/files is unreadable: {e}")))?;
        let mut seen: Vec<&str> = Vec::new();
        let mut files = String::new();
        for rel in listed.lines().map(str::trim).filter(|l| !l.is_empty()) {
            let path = Path::new(rel);
            if let Some(why) = path.components().find_map(not_a_name) {
                return Err(refuse(format!("{rel} {why}")));
            }
            if !root.join(path).is_file() {
                return Err(refuse(format!("{rel} names no file")));
            }
            // By components, not text: `a//b` and `a/b` name one file.
            if seen
                .iter()
                .any(|prior| Path::new(prior).components().eq(path.components()))
            {
                return Err(refuse(format!("{rel} is listed twice")));
            }
            seen.push(rel);
            files.push_str(&format!(
                "            ({rel:?}, {}),\n",
                manifest_include(rel)
            ));
        }
        source.push_str(&format!(
            "    TemplateSet {{\n        key: {key:?},\n        set: {},\n        files: &[\n{files}        ],\n    }},\n",
            manifest_include(&format!("scaffold/{key}/set.yml"))
        ));
        watch_paths.push(dir.join("set.yml"));
        watch_paths.push(dir.join("files"));
        watch_paths.extend(seen.into_iter().map(|rel| root.join(rel)));
    }
    source.push_str("];\n");
    Ok(GeneratedSets {
        source,
        watch_paths,
    })
}

/// Why a listed path's `component` is refused, or `None` for a plain name: a listed path is
/// copied to the same relative path in the scaffolded project, so every component must be a name.
fn not_a_name(component: Component) -> Option<&'static str> {
    match component {
        Component::Normal(_) => None,
        Component::RootDir | Component::Prefix(_) => Some("is an absolute path"),
        Component::CurDir => Some("holds a . segment"),
        Component::ParentDir => Some("holds a .. segment"),
    }
}

/// `include_str!` of the repository-relative `rel`, spelled from `CARGO_MANIFEST_DIR`.
fn manifest_include(rel: &str) -> String {
    format!(
        "include_str!(concat!(env!(\"CARGO_MANIFEST_DIR\"), {:?}))",
        format!("/{rel}")
    )
}
