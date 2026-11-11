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
