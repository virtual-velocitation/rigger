//! The facts memo on disk (addendum section 13): the facts extracted from a worktree file, kept
//! under the code store by the store's fingerprint, the file's path and its content, so a
//! one-shot read extracts each content once.

use rigger_domain::code_facts::FileFacts;
use std::path::PathBuf;
use std::time::Duration;

/// The memo's directory under the code store's.
pub(crate) const DIR: &str = "memo";

/// How much the memo on disk keeps: at most `bytes` of entries, none unread for longer than `age`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MemoCap {
    pub bytes: u64,
    pub age: Duration,
}

impl MemoCap {
    /// 64 MiB of entries, none unread for more than three days.
    pub const STATED: MemoCap = MemoCap {
        bytes: 64 << 20,
        age: Duration::from_secs(3 * 24 * 60 * 60),
    };
}

/// The entries of one store fingerprint in the memo directory `dir`.
pub(crate) struct DiskMemo {
    dir: PathBuf,
    fingerprint: u64,
}

impl DiskMemo {
    pub(crate) fn new(dir: PathBuf, fingerprint: u64) -> Self {
        Self { dir, fingerprint }
    }

    /// The facts of the file at `path` holding `bytes`, when an entry holds them.
    pub(crate) fn get(&self, _path: &str, _bytes: &[u8]) -> Option<FileFacts> {
        let _ = (&self.dir, self.fingerprint);
        None
    }

    /// Keeps `facts`, extracted from `bytes`.
    pub(crate) fn put(&self, _bytes: &[u8], _facts: &FileFacts) -> Result<(), String> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::code::dir_source::content_id;
    use rigger_domain::code_facts::Def;

    /// The facts of the file at `path` holding `bytes`, defining one function per line.
    fn facts_of(path: &str, bytes: &[u8]) -> FileFacts {
        let defs = String::from_utf8_lossy(bytes)
            .lines()
            .enumerate()
            .map(|(at, leaf)| Def {
                local: leaf.to_owned(),
                leaf: leaf.to_owned(),
                kind: "fn".into(),
                line: at as u32 + 1,
                end_line: at as u32 + 1,
                parent: None,
                test: false,
            })
            .collect();
        FileFacts {
            path: path.to_owned(),
            blob: content_id(bytes),
            lang: "toy".into(),
            defs,
            ..Default::default()
        }
    }

    /// The names in `dir`, sorted.
    fn names(dir: &std::path::Path) -> Vec<String> {
        let mut names: Vec<String> = (std::fs::read_dir(dir).into_iter().flatten().flatten())
            .filter_map(|e| e.file_name().to_str().map(str::to_owned))
            .collect();
        names.sort();
        names
    }

    /// Two writers of one entry leave one whole entry: GIVEN the same facts put by several
    /// threads at once, THEN the memo holds that one entry and no temp, and it reads back as the
    /// facts.
    #[test]
    fn writers_of_one_entry_leave_it_whole() {
        let root = tempfile::tempdir().unwrap();
        let memo = DiskMemo::new(root.path().join(DIR), 7);
        let bytes = b"alpha\nbeta\n";
        let facts = facts_of("src/a.rs", bytes);
        std::thread::scope(|scope| {
            for _ in 0..8 {
                scope.spawn(|| {
                    for _ in 0..16 {
                        memo.put(bytes, &facts).unwrap();
                    }
                });
            }
        });
        assert_eq!(names(&root.path().join(DIR)).len(), 1);
        assert_eq!(memo.get("src/a.rs", bytes), Some(facts));
    }
}
