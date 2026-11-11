//! The code store as the composition root wires it: the store at a project's root, read from the
//! repository's objects (or from the directory, for a project with no git), extracted by
//! [`FactsExtractor`] and grouped by the production groupings. Every caller opens it through
//! [`CodeStore::open`], which is `open_current`: the conductor after a landed merge, `graph build`
//! and `setup` as writers, and every reader.

use rigger_domain::code_facts::Stamp;
use rigger_domain::code_view::{
    AdvanceCodeStore, CodeView, Extract, Head, OpenCodeView, TreeSource, Withheld, WorktreeDiff,
};
use rigger_domain::composed::Label;
use rigger_grounder::facts::{open_store, FactsExtractor};
use rigger_store::code::{DirTreeSource, Mode, Opened, Store};
use rigger_worktree_git::tree_source::GitTreeSource;
use std::path::{Path, PathBuf};

/// The code store of one project, bound to its root: the operator checkout that owns `.rigger/`.
pub struct CodeStore {
    root: PathBuf,
    source: Box<dyn TreeSource + Send + Sync>,
}

impl CodeStore {
    /// The store of the project whose root is `root`, read from the git repository there: its
    /// head is `root`'s checked-out commit, so `root` must be the operator checkout, never a unit
    /// worktree.
    pub fn in_repository(root: &Path) -> Self {
        Self {
            root: root.to_path_buf(),
            source: Box::new(GitTreeSource::new(root)),
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
        let current = Store::new(&self.root).current(FactsExtractor.fingerprint())?;
        Ok(current.map(|view| view.stamp().clone()))
    }

    /// How `worktree` differs from the tree `opened` describes.
    pub fn differing(&self, opened: &Opened, worktree: &Path) -> Result<WorktreeDiff, String> {
        self.source
            .changed_against_worktree(&opened.view.stamp().tip.tree, worktree)
    }

    /// The honesty label ([`Label`]) of `opened` read from `worktree`: the tip it describes, how
    /// many files `worktree` differs from it by, and why it is behind the head when it is.
    pub fn label(&self, opened: &Opened, worktree: &Path) -> Label {
        Label::describing(
            &opened.view.stamp().tip,
            worktree.display().to_string(),
            &Withheld::of(self.differing(opened, worktree)),
            opened.behind.as_ref().map(ToString::to_string),
        )
    }
}

/// A reader in `worktree` reads the head's view without waiting on a writer, compared with its
/// own tree.
impl OpenCodeView for CodeStore {
    fn head(&self, worktree: &Path) -> Result<Head, String> {
        let opened = self.open(Mode::Read)?;
        let diff = self.differing(&opened, worktree);
        Ok(Head {
            diff,
            behind: opened.behind.as_ref().map(ToString::to_string),
            view: Box::new(opened.view),
        })
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
