//! [`GitTreeSource`]: the code store's tree source over a git repository. Every answer is read
//! from git's objects - the checked-out commit's tree, its listing, two trees' differences and
//! the blobs themselves - so a tree's facts never depend on what the worktree holds.

use rigger_domain::code_facts::Tip;
use rigger_domain::code_view::{Change, Entry, TreeSource, WorktreeDiff};
use std::path::{Path, PathBuf};

/// The file modes a listing keeps: regular files, executable or not. Symlinks and submodules
/// hold no bytes of their own to read facts from.
const FILE_MODES: [&str; 2] = ["100644", "100755"];

/// The tree source of the git repository holding `root`.
pub struct GitTreeSource {
    root: PathBuf,
}

impl GitTreeSource {
    pub fn new(root: &Path) -> Self {
        Self {
            root: root.to_path_buf(),
        }
    }

    /// The standard output of `git <args>` run in `dir`, or the failure naming the command and
    /// what git said.
    fn git(dir: &Path, args: &[&str]) -> Result<Vec<u8>, String> {
        let out = crate::subprocess::git_in(dir)
            .args(args)
            .output()
            .map_err(|e| format!("git {}: {e}", args.join(" ")))?;
        if out.status.success() {
            Ok(out.stdout)
        } else {
            Err(format!(
                "git {} in {}: {}",
                args.join(" "),
                dir.display(),
                String::from_utf8_lossy(&out.stderr).trim()
            ))
        }
    }

    /// The trimmed answer of `git <args>` in the repository, or none when git refuses it.
    fn answer(&self, args: &[&str]) -> Option<String> {
        let out = Self::git(&self.root, args).ok()?;
        Some(String::from_utf8_lossy(&out).trim().to_owned())
    }
}

/// The fields of a `-z` output: NUL-separated, the last one empty.
fn fields(out: &[u8]) -> impl Iterator<Item = &[u8]> {
    out.split(|b| *b == 0).filter(|f| !f.is_empty())
}

/// The blob a side of a raw diff names, when that side is a file a listing keeps.
fn file_side(mode: &str, blob: &str) -> Option<String> {
    FILE_MODES.contains(&mode).then(|| blob.to_owned())
}

impl TreeSource for GitTreeSource {
    /// The checked-out commit and its tree; a branch with no commit yet names the empty tree.
    fn head(&self) -> Result<Tip, String> {
        let branch = self.answer(&["symbolic-ref", "--short", "-q", "HEAD"]);
        let commit = self.answer(&["rev-parse", "--verify", "-q", "HEAD^{commit}"]);
        let tree = match (&commit, &branch) {
            (Some(commit), _) => self.answer(&["rev-parse", &format!("{commit}^{{tree}}")]),
            (None, Some(_)) => self.answer(&["hash-object", "-t", "tree", "--stdin"]),
            (None, None) => None,
        };
        let tree =
            tree.ok_or_else(|| format!("{} has no HEAD git can resolve", self.root.display()))?;
        Ok(Tip {
            head: branch.unwrap_or_else(|| "HEAD".into()),
            commit,
            tree,
        })
    }

    fn list(&self, tree: &str) -> Result<Vec<Entry>, String> {
        let out = Self::git(&self.root, &["ls-tree", "-r", "-z", "--full-tree", tree])?;
        let mut entries: Vec<Entry> = fields(&out)
            .filter_map(|line| {
                // `<mode> <type> <object>\t<path>`
                let line = std::str::from_utf8(line).ok()?;
                let (meta, path) = line.split_once('\t')?;
                let mut meta = meta.split(' ');
                let (mode, _, blob) = (meta.next()?, meta.next()?, meta.next()?);
                Some(Entry {
                    path: path.to_owned(),
                    blob: file_side(mode, blob)?,
                })
            })
            .collect();
        entries.sort_by(|a, b| a.path.cmp(&b.path));
        Ok(entries)
    }

    /// A rename reads as a deletion and an addition, and a change of mode alone, which leaves
    /// the bytes as they were, is no change.
    fn changed(&self, from: &str, to: &str) -> Result<Vec<Change>, String> {
        let out = Self::git(
            &self.root,
            &["diff-tree", "-r", "-z", "--raw", "--no-renames", from, to],
        )?;
        let mut changes = Vec::new();
        let mut raw = fields(&out);
        // `:<old mode> <new mode> <old object> <new object> <status>`, then the path.
        while let (Some(meta), Some(path)) = (raw.next(), raw.next()) {
            let (Ok(meta), Ok(path)) = (std::str::from_utf8(meta), std::str::from_utf8(path))
            else {
                continue;
            };
            let meta: Vec<&str> = meta.trim_start_matches(':').split(' ').collect();
            let [old_mode, new_mode, old, new, ..] = meta.as_slice() else {
                return Err(format!("git diff-tree answered {meta:?}"));
            };
            let (old, new) = (file_side(old_mode, old), file_side(new_mode, new));
            if old != new {
                changes.push(Change {
                    path: path.to_owned(),
                    old,
                    new,
                });
            }
        }
        changes.sort_by(|a, b| a.path.cmp(&b.path));
        Ok(changes)
    }

    /// Staged and unstaged edits both count: a path the tree holds is changed when the
    /// worktree's bytes differ or are gone, and a path it does not hold is untracked unless
    /// git ignores it.
    fn changed_against_worktree(
        &self,
        tree: &str,
        worktree: &Path,
    ) -> Result<WorktreeDiff, String> {
        let status = Self::git(
            worktree,
            &[
                "diff",
                "--name-status",
                "-z",
                "--no-renames",
                "--no-relative",
                tree,
            ],
        )?;
        let mut out = WorktreeDiff::default();
        let mut status = fields(&status);
        while let (Some(kind), Some(path)) = (status.next(), status.next()) {
            let Ok(path) = std::str::from_utf8(path) else {
                continue;
            };
            if kind == b"A" {
                out.untracked.push(path.to_owned());
            } else {
                out.changed.push(path.to_owned());
            }
        }
        let others = Self::git(
            worktree,
            &[
                "ls-files",
                "--others",
                "--exclude-standard",
                "-z",
                "--full-name",
            ],
        )?;
        for path in fields(&others).filter_map(|p| std::str::from_utf8(p).ok()) {
            if !out.changed.iter().any(|c| c == path) {
                out.untracked.push(path.to_owned());
            }
        }
        out.changed.sort();
        out.untracked.sort();
        out.untracked.dedup();
        Ok(out)
    }

    /// Reads every blob through one `git cat-file --batch`; a blob the repository does not hold
    /// fails the read.
    fn read_blobs(
        &self,
        blobs: &[String],
        sink: &mut dyn FnMut(&str, &[u8]),
    ) -> Result<(), String> {
        let mut batch = crate::worktree::BlobBatch::start(&self.root)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("{} is in no git repository", self.root.display()))?;
        for blob in blobs {
            let bytes = batch
                .blob(blob)
                .map_err(|e| e.to_string())?
                .ok_or_else(|| format!("the repository holds no blob {blob}"))?;
            sink(blob, &bytes);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{git_commit_all, git_ok, git_out, init_repo};
    use rigger_store::code::doubles::{CountingGroup, ToyExtract};
    use rigger_store::code::{content_id, DirTreeSource, Store};

    type Files = &'static [(&'static str, &'static str)];

    fn write(root: &Path, files: Files) {
        for (path, text) in files {
            let file = root.join(path);
            std::fs::create_dir_all(file.parent().unwrap()).unwrap();
            std::fs::write(file, text).unwrap();
        }
    }

    fn remove(root: &Path, paths: &[&str]) {
        for path in paths {
            std::fs::remove_file(root.join(path)).unwrap();
        }
    }

    /// A repository whose head commit holds `files`.
    fn repo(files: Files) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        init_repo(dir.path());
        write(dir.path(), files);
        git_commit_all(dir.path(), "base");
        dir
    }

    fn paths(entries: &[Entry]) -> Vec<String> {
        entries.iter().map(|e| e.path.clone()).collect()
    }

    /// A change as its path and whether it has an old and a new side, since the two sources
    /// name blobs differently.
    fn shape(changes: &[Change]) -> Vec<(String, bool, bool)> {
        changes
            .iter()
            .map(|c| (c.path.clone(), c.old.is_some(), c.new.is_some()))
            .collect()
    }

    #[test]
    fn head_names_the_branch_its_commit_and_tree() {
        struct Row {
            name: &'static str,
            /// Readies a fresh `git init` repository.
            setup: fn(&Path),
            head: &'static str,
            commit: bool,
            files: &'static [&'static str],
        }
        fn committed(root: &Path) {
            git_ok(root, &["checkout", "-q", "-b", "work"]);
            write(root, &[("a.txt", "a\n")]);
            git_commit_all(root, "a");
        }
        let rows = [
            Row {
                name: "a branch with a commit",
                setup: committed,
                head: "work",
                commit: true,
                files: &["a.txt"],
            },
            Row {
                name: "a detached head",
                setup: |root| {
                    committed(root);
                    git_ok(root, &["checkout", "-q", "--detach"]);
                },
                head: "HEAD",
                commit: true,
                files: &["a.txt"],
            },
            Row {
                name: "a branch with no commit yet",
                setup: |root| {
                    git_ok(root, &["symbolic-ref", "HEAD", "refs/heads/unborn"]);
                },
                head: "unborn",
                commit: false,
                files: &[],
            },
        ];
        for row in rows {
            let dir = tempfile::tempdir().unwrap();
            git_ok(dir.path(), &["init", "-q"]);
            git_ok(dir.path(), &["config", "user.email", "t@example.com"]);
            git_ok(dir.path(), &["config", "user.name", "t"]);
            (row.setup)(dir.path());
            let source = GitTreeSource::new(dir.path());
            let tip = source.head().expect(row.name);
            assert_eq!(tip.head, row.head, "{}", row.name);
            assert_eq!(tip.commit.is_some(), row.commit, "{}", row.name);
            if let Some(commit) = &tip.commit {
                assert_eq!(
                    *commit,
                    git_out(dir.path(), &["rev-parse", "HEAD"]),
                    "{}",
                    row.name
                );
            }
            let listed = source.list(&tip.tree).expect(row.name);
            assert_eq!(paths(&listed), row.files, "{}", row.name);
        }
    }

    #[test]
    fn advance_reads_git_objects_never_worktree_bytes() {
        struct Row {
            name: &'static str,
            committed: Files,
            edit: Files,
            delete: &'static [&'static str],
        }
        let rows = [
            Row {
                name: "a modified file is read as committed",
                committed: &[("a.toy", "def a\n")],
                edit: &[("a.toy", "def edited\n")],
                delete: &[],
            },
            Row {
                name: "a deleted file is read as committed",
                committed: &[("a.toy", "def a\n"), ("b.toy", "def b\n")],
                edit: &[],
                delete: &["b.toy"],
            },
            Row {
                name: "an untracked file is never read",
                committed: &[("a.toy", "def a\n")],
                edit: &[("new.toy", "def new\n")],
                delete: &[],
            },
        ];
        for row in rows {
            let dir = repo(row.committed);
            write(dir.path(), row.edit);
            remove(dir.path(), row.delete);
            let source = GitTreeSource::new(dir.path());
            let extract = ToyExtract::new(1);
            let head = source.head().expect(row.name);
            Store::new(dir.path())
                .advance(&source, &extract, &CountingGroup::default(), &head)
                .expect(row.name);
            let committed: Vec<String> = row.committed.iter().map(|(p, _)| (*p).into()).collect();
            assert_eq!(extract.paths(), committed, "{}", row.name);
            for (path, text) in row.committed {
                assert_eq!(
                    extract.bytes_of(path),
                    Some(text.as_bytes().to_vec()),
                    "{}: {path}",
                    row.name
                );
            }
        }
    }

    #[test]
    fn git_and_dir_sources_list_and_diff_the_same_tree() {
        struct Row {
            name: &'static str,
            write: Files,
            delete: &'static [&'static str],
            /// Marks a path executable in the index only.
            executable: Option<&'static str>,
        }
        let base: Files = &[("a.txt", "a\n"), ("src/b.txt", "b\n")];
        let rows = [
            Row {
                name: "add",
                write: &[("c.txt", "c\n")],
                delete: &[],
                executable: None,
            },
            Row {
                name: "modify",
                write: &[("a.txt", "a2\n")],
                delete: &[],
                executable: None,
            },
            Row {
                name: "delete",
                write: &[],
                delete: &["a.txt"],
                executable: None,
            },
            Row {
                name: "rename",
                write: &[("renamed.txt", "a\n")],
                delete: &["a.txt"],
                executable: None,
            },
            Row {
                name: "mode",
                write: &[],
                delete: &[],
                executable: Some("a.txt"),
            },
            Row {
                name: "nested dir",
                write: &[("src/deep/er/d.txt", "d\n")],
                delete: &[],
                executable: None,
            },
        ];
        for row in rows {
            let dir = repo(base);
            let git = GitTreeSource::new(dir.path());
            let plain = DirTreeSource::new(dir.path());
            let (git_old, dir_old) = (git.head().unwrap(), plain.head().unwrap());
            write(dir.path(), row.write);
            remove(dir.path(), row.delete);
            git_ok(dir.path(), &["add", "-A"]);
            if let Some(path) = row.executable {
                git_ok(dir.path(), &["update-index", "--chmod=+x", path]);
            }
            git_ok(dir.path(), &["commit", "-q", "-m", row.name]);
            let (git_new, dir_new) = (git.head().unwrap(), plain.head().unwrap());

            for (g, d) in [(&git_old, &dir_old), (&git_new, &dir_new)] {
                let (listed, walked) = (git.list(&g.tree).unwrap(), plain.list(&d.tree).unwrap());
                assert_eq!(paths(&listed), paths(&walked), "{}: list", row.name);
                let ids: Vec<String> = listed.iter().map(|e| e.blob.clone()).collect();
                let mut read = Vec::new();
                git.read_blobs(&ids, &mut |id, bytes| {
                    read.push((id.to_owned(), content_id(bytes)))
                })
                .unwrap();
                let hashed: Vec<String> = read.into_iter().map(|(_, h)| h).collect();
                let expected: Vec<String> = walked.iter().map(|e| e.blob.clone()).collect();
                assert_eq!(hashed, expected, "{}: blob bytes", row.name);
            }
            assert_eq!(
                shape(&git.changed(&git_old.tree, &git_new.tree).unwrap()),
                shape(&plain.changed(&dir_old.tree, &dir_new.tree).unwrap()),
                "{}: changed",
                row.name
            );
        }
    }

    #[test]
    fn changed_against_worktree_lists_modified_deleted_and_untracked() {
        struct Row {
            name: &'static str,
            write: Files,
            delete: &'static [&'static str],
            /// Paths staged with `git add` before the diff.
            stage: &'static [&'static str],
            changed: &'static [&'static str],
            untracked: &'static [&'static str],
        }
        let base: Files = &[
            ("a.txt", "a\n"),
            ("b.txt", "b\n"),
            (".gitignore", "ignored.txt\n"),
        ];
        let rows = [
            Row {
                name: "clean",
                write: &[],
                delete: &[],
                stage: &[],
                changed: &[],
                untracked: &[],
            },
            Row {
                name: "modified",
                write: &[("a.txt", "a2\n")],
                delete: &[],
                stage: &[],
                changed: &["a.txt"],
                untracked: &[],
            },
            Row {
                name: "modified and staged",
                write: &[("a.txt", "a2\n")],
                delete: &[],
                stage: &["a.txt"],
                changed: &["a.txt"],
                untracked: &[],
            },
            Row {
                name: "deleted",
                write: &[],
                delete: &["b.txt"],
                stage: &[],
                changed: &["b.txt"],
                untracked: &[],
            },
            Row {
                name: "untracked",
                write: &[("new.txt", "n\n")],
                delete: &[],
                stage: &[],
                changed: &[],
                untracked: &["new.txt"],
            },
            Row {
                name: "added and staged",
                write: &[("new.txt", "n\n")],
                delete: &[],
                stage: &["new.txt"],
                changed: &[],
                untracked: &["new.txt"],
            },
            Row {
                name: "ignored",
                write: &[("ignored.txt", "i\n")],
                delete: &[],
                stage: &[],
                changed: &[],
                untracked: &[],
            },
            Row {
                name: "every kind at once",
                write: &[("a.txt", "a2\n"), ("z/new.txt", "n\n")],
                delete: &["b.txt"],
                stage: &[],
                changed: &["a.txt", "b.txt"],
                untracked: &["z/new.txt"],
            },
        ];
        for row in rows {
            let dir = repo(base);
            let source = GitTreeSource::new(dir.path());
            let head = source.head().expect(row.name);
            write(dir.path(), row.write);
            remove(dir.path(), row.delete);
            for path in row.stage {
                git_ok(dir.path(), &["add", path]);
            }
            let diff = source
                .changed_against_worktree(&head.tree, dir.path())
                .expect(row.name);
            let owned = |paths: &[&str]| paths.iter().map(|p| (*p).to_owned()).collect();
            assert_eq!(
                diff,
                WorktreeDiff {
                    changed: owned(row.changed),
                    untracked: owned(row.untracked),
                },
                "{}",
                row.name
            );
        }
    }
}
