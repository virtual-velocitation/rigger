//! The one scoped walk of a project's files: every directory walk rigger makes (the grounder's
//! grep and ingests, and the directory tree source) goes through [`walk_scoped`], so the scope
//! can never drift between them and every walk covers exactly the same file set.

use std::ops::ControlFlow;
use std::path::{Path, PathBuf};

/// Hands each regular file under `root` to `on_file`, confined to the `named` paths (relative to
/// `root`) when there are any.
///
/// The walk is scoped to the project's own sources, three ways:
///
/// - **The project's own ignore rules.** It honours the repository's committed `.gitignore`
///   files (root and nested), so whatever the project already declared as not-source - build
///   outputs, caches, vendored artifacts - is skipped. Only committed `.gitignore` is honoured:
///   the user's global gitignore and the per-clone `.git/info/exclude` are disabled, so the
///   scope is a pure function of the tree (the same tree yields a byte-identical graph on any
///   machine). `require_git(false)` applies those rules even when the tree is not a checked-out
///   repository, and `parents(false)` confines rule-reading to the root so a `.gitignore` above
///   the project never bleeds in.
/// - **The always-excluded set.** Hidden entries are skipped: the VCS metadata directory `.git`,
///   rigger's own runtime directory `.rigger`, and every other tooling dot-directory, none of
///   which is first-party source.
/// - **Root confinement.** Symlinks are not followed, so a link that escapes the root, or a link
///   cycle, is never traversed.
///
/// A `named` walk descends only the directories on the way to a named path, so it visits exactly
/// the named files the whole walk would visit, at a cost bounded by the names rather than the
/// tree.
///
/// Entries are visited in sorted file-name order, so the walk is deterministic; an unreadable
/// entry (a permissions race, a broken link) is skipped rather than failing the walk. `on_file`
/// receives each regular file, never a directory or an unfollowed symlink, and returns
/// `Break(())` to stop the whole walk or `Continue(())` to walk on.
pub fn walk_scoped<F>(root: &Path, named: Option<&[String]>, on_file: &mut F) -> ControlFlow<()>
where
    F: FnMut(&Path) -> ControlFlow<()>,
{
    let mut walker = ignore::WalkBuilder::new(root);
    walker
        // The always-excluded set, independent of any ignore file.
        .hidden(true)
        // The project's own committed ignore rules only, so the scope is a pure function of the
        // tree.
        .git_ignore(true)
        .git_global(false)
        .git_exclude(false)
        .ignore(false)
        // Committed `.gitignore` applies even outside a git checkout, and no ignore file above
        // the root is read.
        .require_git(false)
        .parents(false)
        // Root confinement: nothing escapes the root and no cycle forms.
        .follow_links(false)
        .sort_by_file_name(|a, b| a.cmp(b));
    if let Some(named) = named {
        let wanted: Vec<PathBuf> = named.iter().map(|n| root.join(n)).collect();
        walker.filter_entry(move |entry| wanted.iter().any(|w| w.starts_with(entry.path())));
    }
    for dent in walker.build() {
        let Ok(entry) = dent else { continue };
        // An unfollowed symlink's file type is the link, so it never reaches `on_file`.
        if entry.file_type().is_some_and(|ft| ft.is_file()) {
            on_file(entry.path())?;
        }
    }
    ControlFlow::Continue(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rigger_domain::code_view::in_scope;

    /// Every file a fixture holds besides its committed `.gitignore`.
    const FILES: &[&str] = &[
        ".git/HEAD",
        ".github/workflows/ci.yml",
        ".rigger/blob.txt",
        ".rigger/cache/x.bin",
        ".rigger/workflow.yml",
        "src/.hidden.rs",
        "src/lib.rs",
        "target/out.bin",
    ];

    /// A tree holding every one of [`FILES`] and, when `gitignore` is not empty, a committed
    /// `.gitignore` holding it.
    fn fixture(gitignore: &str) -> tempfile::TempDir {
        let root = tempfile::tempdir().expect("tempdir");
        for file in FILES {
            let path = root.path().join(file);
            std::fs::create_dir_all(path.parent().expect("a file sits in a directory"))
                .expect("create the file's directory");
            std::fs::write(&path, file.as_bytes()).expect("write the file");
        }
        if !gitignore.is_empty() {
            std::fs::write(root.path().join(".gitignore"), gitignore).expect("write .gitignore");
        }
        root
    }

    /// The files a scoped walk of `root` visits, as sorted `/`-separated paths under it.
    fn visited(root: &Path, named: Option<&[String]>) -> Vec<String> {
        let mut out = Vec::new();
        let _ = walk_scoped(root, named, &mut |file| {
            let rel = file
                .strip_prefix(root)
                .expect("a visited file is under the root");
            let parts: Vec<String> = rel
                .components()
                .map(|c| c.as_os_str().to_string_lossy().into_owned())
                .collect();
            out.push(parts.join("/"));
            ControlFlow::Continue(())
        });
        out.sort();
        out
    }

    #[test]
    fn the_scoped_walk_visits_the_files_in_scope_that_the_committed_gitignore_keeps() {
        /// The committed `.gitignore`, the paths the walk is confined to, the files it visits.
        type Case = (
            &'static str,
            Option<&'static [&'static str]>,
            &'static [&'static str],
        );
        let cases: &[Case] = &[
            (
                "",
                None,
                &[".rigger/workflow.yml", "src/lib.rs", "target/out.bin"],
            ),
            ("target/\n", None, &[".rigger/workflow.yml", "src/lib.rs"]),
            (".rigger/\n", None, &["src/lib.rs", "target/out.bin"]),
            (
                "target/\n",
                Some(&[".rigger/workflow.yml", ".rigger/blob.txt", "target/out.bin"]),
                &[".rigger/workflow.yml"],
            ),
        ];
        for (gitignore, named, expected) in cases {
            let root = fixture(gitignore);
            let named: Option<Vec<String>> =
                named.map(|n| n.iter().map(|p| (*p).to_owned()).collect());
            assert_eq!(
                visited(root.path(), named.as_deref()),
                *expected,
                "gitignore {gitignore:?}, named {named:?}"
            );
        }
        // With nothing ignored, the walk keeps a file exactly when the domain's one scope rule
        // does.
        let root = fixture("");
        let all = visited(root.path(), None);
        for file in FILES {
            assert_eq!(all.contains(&(*file).to_owned()), in_scope(file), "{file}");
        }
    }

    #[cfg(unix)]
    #[test]
    fn a_name_that_is_not_utf8_is_outside_the_scoped_walk() {
        use std::os::unix::ffi::OsStrExt;
        let not_utf8 = std::ffi::OsStr::from_bytes(b"n\xffme");
        let root = fixture("");
        std::fs::write(root.path().join("src").join(not_utf8), b"a file").expect("write the file");
        let dir = root.path().join(not_utf8);
        std::fs::create_dir(&dir).expect("create the directory");
        std::fs::write(dir.join("lib.rs"), b"a file").expect("write the file within");
        assert_eq!(
            visited(root.path(), None),
            [".rigger/workflow.yml", "src/lib.rs", "target/out.bin"]
        );
    }
}
