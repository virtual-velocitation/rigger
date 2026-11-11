//! [`DirTreeSource`]: the tree of a directory with no git. A file's content id is the FNV-1a hash
//! of its bytes, and the tree id is the hash over the sorted (path, content id) pairs, so the same
//! content always names the same tree.

use crate::walk::walk_scoped;
use rigger_domain::code_facts::Tip;
use rigger_domain::code_view::{Entry, TreeSource, WorktreeDiff, ALWAYS_IN_SCOPE};
use rigger_domain::playbooks::fnv1a_64;
use std::collections::{BTreeSet, HashMap};
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

/// What one walk of a directory found.
struct Walked {
    /// Every file a tree's facts cover, path-sorted, with its content id.
    entries: Vec<Entry>,
    /// The in-scope files the walk could not read (a permissions race, a file removed
    /// mid-walk), path-sorted: no listing holds them, since they have no bytes to hash.
    unreadable: Vec<String>,
}

/// Every file under `root` a tree's facts cover: the files the one scoped walk visits (committed
/// `.gitignore` honoured, hidden entries and symlinks skipped), plus each [`ALWAYS_IN_SCOPE`]
/// path that is a regular file. A path that is not UTF-8 is skipped, since the facts hold paths
/// as text; a file that cannot be read is kept apart from the entries.
fn walk(root: &Path) -> Walked {
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
    files.sort_by(|a, b| a.0.cmp(&b.0));
    files.dedup_by(|a, b| a.0 == b.0);
    let mut walked = Walked {
        entries: Vec::new(),
        unreadable: Vec::new(),
    };
    for (path, file) in files {
        match std::fs::read(file) {
            Ok(bytes) => walked.entries.push(Entry {
                path,
                blob: content_id(&bytes),
            }),
            Err(_) => walked.unreadable.push(path),
        }
    }
    walked
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
        let entries = walk(&self.root).entries;
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
        let now = walk(worktree);
        let blobs: HashMap<&str, &str> = now
            .entries
            .iter()
            .map(|e| (e.path.as_str(), e.blob.as_str()))
            .collect();
        let mut changed: BTreeSet<String> = held
            .iter()
            .filter(|(path, blob)| blobs.get(path.as_str()) != Some(&blob.as_str()))
            .map(|(path, _)| path.clone())
            .collect();
        // A file the worktree holds but cannot read is changed whatever the tree holds there,
        // since its content is unknown.
        changed.extend(now.unreadable);
        let mut untracked: Vec<String> = blobs
            .keys()
            .filter(|path| !held.contains_key(**path))
            .map(|path| (*path).to_owned())
            .collect();
        untracked.sort();
        Ok(WorktreeDiff {
            changed: changed.into_iter().collect(),
            untracked,
        })
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

    /// The bytes of a file outside the tree, which no answer may hold.
    const OUTSIDE: &[u8] = b"bytes outside the root\n";

    fn mkfifo(path: &Path) {
        let status = std::process::Command::new("mkfifo")
            .arg(path)
            .status()
            .expect("run mkfifo");
        assert!(status.success(), "test setup: mkfifo must succeed");
    }

    /// What the directory source answers over a tree.
    struct Answers {
        /// The head's listing.
        paths: Vec<String>,
        /// The listed blobs, and the blobs a read of them gave.
        blobs: Vec<String>,
        read: Vec<String>,
        /// Whether a read of the outside file's content was refused, and what it gave.
        outside_refused: bool,
        outside_read: Vec<String>,
        diff: WorktreeDiff,
    }

    /// The source's answers over `root`, run off the test's thread so a corner that blocks a read
    /// fails the test instead of hanging it.
    fn answers(root: &Path, outside_blob: String) -> Answers {
        let at = root.to_path_buf();
        let (done, outcome) = mpsc::channel();
        std::thread::spawn(move || {
            let source = DirTreeSource::new(&at);
            let run = || -> Result<Answers, String> {
                let tip = source.head()?;
                let listing = source.list(&tip.tree)?;
                let blobs: Vec<String> = listing.iter().map(|e| e.blob.clone()).collect();
                let mut read = Vec::new();
                source.read_blobs(&blobs, &mut |blob, _| read.push(blob.to_owned()))?;
                let mut outside_read = Vec::new();
                let outside_refused = source
                    .read_blobs(&[outside_blob], &mut |blob, _| {
                        outside_read.push(blob.to_owned())
                    })
                    .is_err();
                let diff = source.changed_against_worktree(&tip.tree, &at)?;
                Ok(Answers {
                    paths: listing.into_iter().map(|e| e.path).collect(),
                    blobs,
                    read,
                    outside_refused,
                    outside_read,
                    diff,
                })
            };
            let _ = done.send(run());
        });
        outcome
            .recv_timeout(Duration::from_secs(10))
            .expect("the listing, the read and the diff return; a corner blocked them")
            .expect("a tree holding a corner lists, reads and diffs")
    }

    #[test]
    fn each_walk_corner_is_kept_out_of_the_listing_and_the_read_and_diffs_as_its_row_says() {
        /// A corner planted at `src/corner.rs` and at the always-in-scope workflow path.
        struct Corner {
            name: &'static str,
            /// Plants the corner at `file`, given a regular file outside the root; answers
            /// whether it took for this user (root reads a file whatever its bits).
            plant: fn(&Path, &Path) -> bool,
            /// The paths the worktree diff calls changed when the corner took.
            changed: &'static [&'static str],
        }
        let corners = [
            Corner {
                name: "a file symlink pointing outside the root",
                plant: |file, outside| {
                    symlink(outside, file).expect("link the corner outside the root");
                    true
                },
                changed: &[],
            },
            Corner {
                name: "a FIFO",
                plant: |file, _| {
                    mkfifo(file);
                    true
                },
                changed: &[],
            },
            Corner {
                name: "an unreadable file: withheld as changed, since its content is unknown",
                plant: |file, _| {
                    std::fs::write(file, b"locked\n").expect("write the locked file");
                    std::fs::set_permissions(file, std::fs::Permissions::from_mode(0o000))
                        .expect("strip the file's permission bits");
                    std::fs::read(file).is_err()
                },
                changed: &[".rigger/workflow.yml", "src/corner.rs"],
            },
        ];
        for corner in corners {
            let outside = tempfile::tempdir().expect("tempdir");
            let secret = outside.path().join("secret.rs");
            std::fs::write(&secret, OUTSIDE).expect("write the outside file");
            let root = tree_with_one_file();
            let mut took = true;
            for file in ["src/corner.rs", ".rigger/workflow.yml"] {
                took &= (corner.plant)(&root.path().join(file), &secret);
            }

            let got = answers(root.path(), content_id(OUTSIDE));
            // A corner that did not take is a plain file, listed and unchanged like any other.
            let (listed, changed): (&[&str], &[&str]) = if took {
                (&["src/lib.rs"], corner.changed)
            } else {
                (
                    &[".rigger/workflow.yml", "src/corner.rs", "src/lib.rs"],
                    &[],
                )
            };
            assert_eq!(got.paths, listed, "{}: listing", corner.name);
            assert_eq!(
                got.read, got.blobs,
                "{}: every listed blob is read",
                corner.name
            );
            assert!(
                got.outside_refused && got.outside_read.is_empty(),
                "{}: no byte outside the root is read: {:?}",
                corner.name,
                got.outside_read
            );
            assert_eq!(
                got.diff,
                WorktreeDiff {
                    changed: changed.iter().map(|p| (*p).to_owned()).collect(),
                    untracked: Vec::new(),
                },
                "{}: worktree diff",
                corner.name
            );
        }
    }
}
