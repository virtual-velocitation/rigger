//! [`DirTreeSource`]: the tree of a directory with no git. A file's content id is the FNV-1a hash
//! of its bytes, and the tree id is the hash over the sorted (path, content id) pairs, so the same
//! content always names the same tree.

use rigger_domain::code_facts::Tip;
use rigger_domain::code_view::{Change, Entry, TreeSource, WorktreeDiff};
use rigger_domain::playbooks::fnv1a_64;
use std::collections::{BTreeMap, HashMap};
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

/// The paths that differ between two path-sorted listings.
pub fn diff(old: &[Entry], new: &[Entry]) -> Vec<Change> {
    let mut by_path: BTreeMap<&str, (Option<&str>, Option<&str>)> = BTreeMap::new();
    for e in old {
        by_path.entry(&e.path).or_default().0 = Some(&e.blob);
    }
    for e in new {
        by_path.entry(&e.path).or_default().1 = Some(&e.blob);
    }
    by_path
        .into_iter()
        .filter(|(_, (o, n))| o != n)
        .map(|(path, (old, new))| Change {
            path: path.to_owned(),
            old: old.map(str::to_owned),
            new: new.map(str::to_owned),
        })
        .collect()
}

/// Every regular file under `root`, path-sorted, with its content id. Hidden entries (`.git`,
/// `.rigger` and every other dot-name) and symlinks are skipped; a path that is not UTF-8 is
/// skipped too, since the facts hold paths as text.
fn walk(root: &Path) -> Result<Vec<Entry>, String> {
    let mut entries = Vec::new();
    let mut dirs = vec![(root.to_path_buf(), String::new())];
    while let Some((dir, prefix)) = dirs.pop() {
        let listing =
            std::fs::read_dir(&dir).map_err(|e| format!("read {}: {e}", dir.display()))?;
        for item in listing {
            let item = item.map_err(|e| format!("read {}: {e}", dir.display()))?;
            let Some(name) = item.file_name().to_str().map(str::to_owned) else {
                continue;
            };
            if name.starts_with('.') {
                continue;
            }
            let kind = item
                .file_type()
                .map_err(|e| format!("stat {}: {e}", item.path().display()))?;
            let path = format!("{prefix}{name}");
            if kind.is_dir() {
                dirs.push((item.path(), format!("{path}/")));
            } else if kind.is_file() {
                let bytes = std::fs::read(item.path())
                    .map_err(|e| format!("read {}: {e}", item.path().display()))?;
                entries.push(Entry {
                    path,
                    blob: content_id(&bytes),
                });
            }
        }
    }
    entries.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(entries)
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

    fn changed(&self, from: &str, to: &str) -> Result<Vec<Change>, String> {
        Ok(diff(&self.remembered(from)?, &self.remembered(to)?))
    }

    fn changed_against_worktree(
        &self,
        tree: &str,
        worktree: &Path,
    ) -> Result<WorktreeDiff, String> {
        let mut out = WorktreeDiff::default();
        for change in diff(&self.remembered(tree)?, &walk(worktree)?) {
            if change.old.is_some() {
                out.changed.push(change.path);
            } else {
                out.untracked.push(change.path);
            }
        }
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
