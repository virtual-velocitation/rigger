//! The code store as the composition root wires it: the store at a project's root, read from the
//! repository's objects (or from the directory, for a project with no git), extracted by
//! [`FactsExtractor`] and grouped by [`ProductionGroupings`]. Every caller opens it through
//! [`CodeStore::open`], which is `open_current`: the conductor after a landed merge, `graph build`
//! and `setup` as writers, and every reader.

use rigger_domain::code_facts::Stamp;
use rigger_domain::code_view::{AdvanceCodeStore, CodeView, Extract, TreeSource, WorktreeDiff};
use rigger_domain::composed::{Honesty, Label};
use rigger_domain::groupings::ProductionGroupings;
use rigger_grounder::facts::FactsExtractor;
use rigger_store::code::{open_current, DirTreeSource, Mode, Opened, Store};
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
        open_current(
            &self.root,
            self.source.as_ref(),
            &FactsExtractor,
            &ProductionGroupings,
            mode,
        )
    }

    /// The stamp of the snapshot in force, read without advancing; none for an empty store.
    pub fn in_force(&self) -> Option<Stamp> {
        Store::new(&self.root)
            .current(FactsExtractor.fingerprint())
            .map(|view| view.stamp().clone())
    }

    /// How `worktree` differs from the tree `opened` describes.
    pub fn differing(&self, opened: &Opened, worktree: &Path) -> Result<WorktreeDiff, String> {
        self.source
            .changed_against_worktree(&opened.view.stamp().tip.tree, worktree)
    }

    /// The honesty label ([`Label`]) of `opened` read from `worktree`: the tip it describes, how
    /// many files `worktree` differs from it by, and why it is behind the head when it is.
    pub fn label(&self, opened: &Opened, worktree: &Path) -> Label {
        let honesty = Honesty {
            worktree: worktree.display().to_string(),
            diff: self.differing(opened, worktree),
            behind: opened.behind.as_ref().map(ToString::to_string),
        };
        Label::of(&opened.view.stamp().tip, honesty).0
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
