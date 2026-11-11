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
/// is skipped, since the facts hold paths as text.
fn walk(root: &Path) -> Result<Vec<Entry>, String> {
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
    let mut entries = files
        .into_iter()
        .map(|(path, file)| {
            let bytes =
                std::fs::read(&file).map_err(|e| format!("read {}: {e}", file.display()))?;
            Ok(Entry {
                path,
                blob: content_id(&bytes),
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    entries.sort_by(|a, b| a.path.cmp(&b.path));
    entries.dedup_by(|a, b| a.path == b.path);
    Ok(entries)
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
        let entries = walk(&self.root)?;
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
        let now: HashMap<String, String> = walk(worktree)?
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
