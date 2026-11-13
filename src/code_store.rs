//! The code store as the composition root wires it: the store at a project's root, read from the
//! repository's objects (or from the directory, for a project with no git), extracted by
//! [`FactsExtractor`](rigger_grounder::facts::FactsExtractor) and grouped by the production groupings. Every caller opens it through
//! [`CodeStore::open`], which is `open_current`: the conductor after a landed merge, `graph build`
//! and `setup` as writers, and every reader.

use rigger_domain::code_facts::Stamp;
use rigger_domain::code_view::{
    AdvanceCodeStore, CodeView, Compared, Head, OpenCodeView, TreeSource, WorktreeDiff,
};
use rigger_domain::composed::Label;
use rigger_grounder::facts::{open_store, read_head, store_fingerprint};
use rigger_store::code::{DirTreeSource, Mode, Opened, Store};
use rigger_worktree_git::discover::Repository;
use rigger_worktree_git::tree_source::GitTreeSource;
use std::path::{Path, PathBuf};

/// The code store of one project, bound to its root: the operator checkout that owns `.rigger/`.
pub struct CodeStore {
    root: PathBuf,
    source: Box<dyn TreeSource + Send + Sync>,
}

impl CodeStore {
    /// The store of the git repository `checkout` was discovered in, rooted where every worktree
    /// of it shares ([`Repository::main_root`]) and read at the main checkout's head, whichever
    /// worktree it was discovered from.
    pub fn in_repository(checkout: Repository) -> Self {
        Self {
            root: checkout.main_root(),
            source: Box::new(GitTreeSource::new(checkout)),
        }
    }

    /// The store of the project at `root`, a directory with no git.
    pub fn in_directory(root: &Path) -> Self {
        Self {
            root: root.to_path_buf(),
            source: Box::new(DirTreeSource::new(root)),
        }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// The view of the head, advancing the store first when it describes another tip or none.
    pub fn open(&self, mode: Mode) -> Result<Opened, String> {
        open_store(&self.root, self.source.as_ref(), mode)
    }

    /// The stamp of the snapshot in force, read without advancing; none for an empty store, and
    /// an error for a store that cannot be read.
    pub fn in_force(&self) -> Result<Option<Stamp>, String> {
        let current = Store::new(&self.root).current(store_fingerprint())?;
        Ok(current.map(|view| view.stamp().clone()))
    }

    /// How `worktree` differs from the tree `opened` describes.
    pub fn differing(&self, opened: &Opened, worktree: &Path) -> Result<WorktreeDiff, String> {
        self.source
            .changed_against_worktree(&opened.view.stamp().tip.tree, worktree)
    }

    /// The honesty label ([`Label`]) of `opened` read from `worktree`: the tip it describes, how
    /// many of `worktree`'s files a reader there takes over it, and why it is behind the head when
    /// it is.
    pub fn label(&self, opened: &Opened, worktree: &Path) -> Label {
        let diff = self.differing(opened, worktree);
        Label::describing(
            &opened.view.stamp().tip,
            worktree.display().to_string(),
            Compared::of(diff.as_ref().map_err(String::as_str)),
            opened.behind.as_ref().map(ToString::to_string),
        )
    }
}

/// A reader in `worktree` reads the head's view without waiting on a writer, compared with its
/// own tree.
impl OpenCodeView for CodeStore {
    fn head(&self, worktree: &Path) -> Result<Head, String> {
        read_head(&self.root, self.source.as_ref(), worktree)
    }
}

impl AdvanceCodeStore for CodeStore {
    fn advance_to_head(&self) -> Result<(), String> {
        match self.open(Mode::Write)?.behind {
            None => Ok(()),
            Some(behind) => Err(behind.to_string()),
        }
    }
}
