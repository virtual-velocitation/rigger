//! [`DirTreeSource`]: the tree of a directory with no git. A file's content id is the FNV-1a hash
//! of its bytes, and the tree id is the hash over the sorted (path, content id) pairs, so the same
//! content always names the same tree.

use crate::walk::walk_scoped;
use rigger_domain::code_facts::Tip;
use rigger_domain::code_view::{Entry, TreeSource, WorktreeDiff, ALWAYS_IN_SCOPE};
use rigger_domain::playbooks::fnv1a_64;
use std::collections::HashMap;
use std::ops::ControlFlow;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// The head a directory source names: a directory has no ref.
pub const DIRECTORY_HEAD: &str = "directory";

/// The content id of `bytes`.
pub fn content_id(bytes: &[u8]) -> String {
    format!("{:016x}", fnv1a_64(bytes))
}

/// The tree id of `entries`, which must be sorted by path.
pub fn tree_id(entries: &[Entry]) -> String {
    let mut listing = Vec::new();
    for e in entries {
        listing.extend_from_slice(e.path.as_bytes());
        listing.push(0);
        listing.extend_from_slice(e.blob.as_bytes());
        listing.push(b'\n');
    }
    content_id(&listing)
}

/// Every file under `root` a tree's facts cover, path-sorted, with its content id: the files
/// the one scoped walk visits (committed `.gitignore` honoured, hidden entries and symlinks
/// skipped), plus each [`ALWAYS_IN_SCOPE`] path that is a regular file. A path that is not UTF-8
/// is skipped, since the facts hold paths as text, and so is a file that cannot be read (a
/// permissions race, a file removed mid-walk), as the walk skips an unreadable entry.
fn walk(root: &Path) -> Vec<Entry> {
    let mut files: Vec<(String, PathBuf)> = Vec::new();
    let _ = walk_scoped(root, None, &mut |file| {
        if let Some(path) = tree_path(root, file) {
            files.push((path, file.to_path_buf()));
        }
        ControlFlow::Continue(())
    });
    for path in ALWAYS_IN_SCOPE {
        let file = root.join(path);
        if std::fs::symlink_metadata(&file).is_ok_and(|m| m.is_file()) {
            files.push(((*path).to_owned(), file));
        }
    }
    let mut entries: Vec<Entry> = files
        .into_iter()
        .filter_map(|(path, file)| {
            let bytes = std::fs::read(file).ok()?;
            Some(Entry {
                path,
                blob: content_id(&bytes),
            })
        })
        .collect();
    entries.sort_by(|a, b| a.path.cmp(&b.path));
    entries.dedup_by(|a, b| a.path == b.path);
    entries
}

/// The `/`-separated path of `file` under `root`, or none when a component is not UTF-8.
fn tree_path(root: &Path, file: &Path) -> Option<String> {
    let parts: Option<Vec<&str>> = file
        .strip_prefix(root)
        .ok()?
        .components()
        .map(|c| c.as_os_str().to_str())
        .collect();
    Some(parts?.join("/"))
}

/// The tree of a directory with no git. A tree id names a listing this source has seen, since a
/// directory keeps no history: `head` lists the directory and remembers the listing, and the other
/// operations answer for trees it remembers.
pub struct DirTreeSource {
    root: PathBuf,
    /// Tree id to its listing.
    listings: Mutex<HashMap<String, Vec<Entry>>>,
}

impl DirTreeSource {
    pub fn new(root: &Path) -> Self {
        Self {
            root: root.to_path_buf(),
            listings: Mutex::new(HashMap::new()),
        }
    }

    fn remembered(&self, tree: &str) -> Result<Vec<Entry>, String> {
        let listings = self.listings.lock().map_err(|e| e.to_string())?;
        listings
            .get(tree)
            .cloned()
            .ok_or_else(|| format!("tree {tree} is not a listing of {}", self.root.display()))
    }
}

impl TreeSource for DirTreeSource {
    fn head(&self) -> Result<Tip, String> {
        let entries = walk(&self.root);
        let tree = tree_id(&entries);
        self.listings
            .lock()
            .map_err(|e| e.to_string())?
            .insert(tree.clone(), entries);
        Ok(Tip {
            head: DIRECTORY_HEAD.into(),
            commit: None,
            tree,
        })
    }

    fn list(&self, tree: &str) -> Result<Vec<Entry>, String> {
        self.remembered(tree)
    }

    fn changed_against_worktree(
        &self,
        tree: &str,
        worktree: &Path,
    ) -> Result<WorktreeDiff, String> {
        let held: HashMap<String, String> = self
            .remembered(tree)?
            .into_iter()
            .map(|e| (e.path, e.blob))
            .collect();
        let now: HashMap<String, String> = walk(worktree)
            .into_iter()
            .map(|e| (e.path, e.blob))
            .collect();
        let mut out = WorktreeDiff {
            changed: held
                .iter()
                .filter(|(path, blob)| now.get(*path) != Some(*blob))
                .map(|(path, _)| path.clone())
                .collect(),
            untracked: now
                .keys()
                .filter(|path| !held.contains_key(*path))
                .cloned()
                .collect(),
        };
        out.changed.sort();
        out.untracked.sort();
        Ok(out)
    }

    /// Reads each blob from a path a remembered listing gives it, and refuses bytes that no
    /// longer hash to it: the directory changed since it was listed.
    fn read_blobs(
        &self,
        blobs: &[String],
        sink: &mut dyn FnMut(&str, &[u8]),
    ) -> Result<(), String> {
        let paths: HashMap<String, String> = {
            let listings = self.listings.lock().map_err(|e| e.to_string())?;
            listings
                .values()
                .flatten()
                .map(|e| (e.blob.clone(), e.path.clone()))
                .collect()
        };
        for blob in blobs {
            let path = paths
                .get(blob)
                .ok_or_else(|| format!("blob {blob} is in no listing"))?;
            let file = self.root.join(path);
            let bytes =
                std::fs::read(&file).map_err(|e| format!("read {}: {e}", file.display()))?;
            if content_id(&bytes) != *blob {
                return Err(format!("{path} changed since it was listed"));
            }
            sink(blob, &bytes);
        }
        Ok(())
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::fs::{symlink, PermissionsExt};
    use std::sync::mpsc;
    use std::time::Duration;

    /// A directory holding one regular file, `src/lib.rs`, and an empty `.rigger`, so a corner
    /// can sit both in the walked tree and at the always-in-scope workflow path.
    fn tree_with_one_file() -> tempfile::TempDir {
        let root = tempfile::tempdir().expect("tempdir");
        std::fs::create_dir_all(root.path().join("src")).expect("create src");
        std::fs::create_dir_all(root.path().join(".rigger")).expect("create .rigger");
        std::fs::write(root.path().join("src/lib.rs"), b"fn real() {}\n").expect("write lib.rs");
        root
    }

    fn listed_paths(source: &DirTreeSource, tree: &str) -> Vec<String> {
        let listing = source.list(tree).expect("the head's tree is remembered");
        listing.into_iter().map(|e| e.path).collect()
    }

    fn mkfifo(path: &Path) {
        let status = std::process::Command::new("mkfifo")
            .arg(path)
            .status()
            .expect("run mkfifo");
        assert!(status.success(), "test setup: mkfifo must succeed");
    }

    #[test]
    fn a_file_symlink_pointing_outside_the_root_is_never_listed_or_read() {
        let outside = tempfile::tempdir().expect("tempdir");
        let secret = outside.path().join("secret.rs");
        std::fs::write(&secret, b"bytes outside the root\n").expect("write the outside file");
        let root = tree_with_one_file();
        symlink(&secret, root.path().join("src/escape.rs")).expect("link in the tree");
        symlink(&secret, root.path().join(".rigger/workflow.yml")).expect("link the workflow");

        let source = DirTreeSource::new(root.path());
        let tip = source.head().expect("a tree with outside links lists");
        assert_eq!(listed_paths(&source, &tip.tree), ["src/lib.rs"]);

        let mut read = Vec::new();
        let outside_blob = content_id(b"bytes outside the root\n");
        let outcome = source.read_blobs(&[outside_blob], &mut |blob, _| read.push(blob.to_owned()));
        assert!(
            outcome.is_err(),
            "the outside file's content is in no listing"
        );
        assert!(
            read.is_empty(),
            "no byte outside the root is read: {read:?}"
        );

        let diff = source
            .changed_against_worktree(&tip.tree, root.path())
            .expect("the worktree diff lists");
        assert!(
            diff.changed.is_empty() && diff.untracked.is_empty(),
            "an outside link is neither changed nor untracked: {diff:?}"
        );
    }

    #[test]
    fn a_fifo_in_the_tree_is_skipped_and_never_blocks_the_listing_or_the_read() {
        let root = tree_with_one_file();
        mkfifo(&root.path().join("src/pipe.rs"));
        mkfifo(&root.path().join(".rigger/workflow.yml"));

        let at = root.path().to_path_buf();
        let (done, outcome) = mpsc::channel();
        std::thread::spawn(move || {
            let source = DirTreeSource::new(&at);
            let run = || -> Result<_, String> {
                let tip = source.head()?;
                let listing = source.list(&tip.tree)?;
                let blobs: Vec<String> = listing.iter().map(|e| e.blob.clone()).collect();
                let mut read = Vec::new();
                source.read_blobs(&blobs, &mut |blob, _| read.push(blob.to_owned()))?;
                let diff = source.changed_against_worktree(&tip.tree, &at)?;
                let paths: Vec<String> = listing.into_iter().map(|e| e.path).collect();
                Ok((paths, blobs, read, diff))
            };
            let _ = done.send(run());
        });
        let (paths, blobs, read, diff) = outcome
            .recv_timeout(Duration::from_secs(10))
            .expect("the listing and the read return; a FIFO blocked them")
            .expect("a tree holding FIFOs lists and reads");
        assert_eq!(paths, ["src/lib.rs"]);
        assert_eq!(read, blobs, "every listed blob is read");
        assert!(
            diff.changed.is_empty() && diff.untracked.is_empty(),
            "a FIFO is neither changed nor untracked: {diff:?}"
        );
    }

    #[test]
    fn an_unreadable_file_is_skipped_never_a_failed_listing() {
        let root = tree_with_one_file();
        let locked = root.path().join("src/locked.rs");
        let locked_workflow = root.path().join(".rigger/workflow.yml");
        let mut locked_out = true;
        for file in [&locked, &locked_workflow] {
            std::fs::write(file, b"locked\n").expect("write the locked file");
            std::fs::set_permissions(file, std::fs::Permissions::from_mode(0o000))
                .expect("strip the file's permission bits");
            locked_out &= std::fs::read(file).is_err();
        }

        let source = DirTreeSource::new(root.path());
        let tip = source
            .head()
            .expect("an unreadable file never fails the listing");
        // A user who reads every file regardless of its bits (root) still reads the locked ones,
        // and then they are listed like any other file.
        let expected: &[&str] = if locked_out {
            &["src/lib.rs"]
        } else {
            &[".rigger/workflow.yml", "src/lib.rs", "src/locked.rs"]
        };
        assert_eq!(listed_paths(&source, &tip.tree), expected);

        let diff = source
            .changed_against_worktree(&tip.tree, root.path())
            .expect("an unreadable file never fails the worktree diff");
        assert!(
            diff.changed.is_empty() && diff.untracked.is_empty(),
            "an unreadable file is neither changed nor untracked: {diff:?}"
        );
    }
}
