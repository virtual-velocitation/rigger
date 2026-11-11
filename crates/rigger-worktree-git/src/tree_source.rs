//! [`GitTreeSource`]: the code store's tree source over a git repository. Every answer is read
//! from git's objects - the checked-out commit's tree, its listing and the blobs themselves - so
//! a tree's facts never depend on what the worktree holds, and no read writes a worktree's index.

use crate::discover::{read_head, rev_parse, Location};
use crate::subprocess::git_in;
use crate::worktree::{finish_git, finish_git_query, start_command, GitOutput};
use rigger_domain::code_facts::Tip;
use rigger_domain::code_view::{Entry, TreeSource, WorktreeDiff};
use std::fs::File;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::TempDir;

/// The file modes a listing keeps: regular files, executable or not. Symlinks and submodules
/// hold no bytes of their own to read facts from.
const FILE_MODES: [&str; 2] = ["100644", "100755"];

/// The variable naming the index a git run reads in place of its worktree's.
const INDEX_FILE: &str = "GIT_INDEX_FILE";

/// The file name of a git index, in a worktree's git dir and in an [`IndexCopy`]'s directory.
pub(crate) const INDEX: &str = "index";

/// The one git command every git the source starts itself is built from: git in `dir`
/// ([`git_in`]) with optional locks off, so a read takes no lock it can do without. A diff
/// refreshes the index it reads all the same, so the diff reads an [`IndexCopy`]. Discovery's
/// `rev-parse` ([`rev_parse`]), which finds the index to copy, reads no index.
fn read_git(dir: &Path) -> Command {
    let mut git = git_in(dir);
    git.arg("--no-optional-locks");
    git
}

/// A private copy of a worktree's index, in a directory of its own in the OS temp directory,
/// which git reads, and refreshes, in place of the worktree's own. It keeps the index's
/// modification time, by which git judges an entry racily clean. Dropping it removes the
/// directory, with any lock git left beside the copy; one a hard stop leaves is the OS temp
/// cleaner's.
struct IndexCopy {
    dir: TempDir,
}

impl IndexCopy {
    /// The copy of the index of `worktree`, found where git finds it ([`Location::GitDir`]): a
    /// linked worktree's is in its own administrative directory. A worktree with no index has no
    /// copy, so git, reading none, answers as it answers in the worktree; the worktree's own index
    /// is never read in its place.
    fn of(worktree: &Path) -> Result<Self, String> {
        let git_dir = rev_parse(worktree, Location::GitDir)?
            .ok_or_else(|| format!("{} is in no git repository", worktree.display()))?;
        let index = Path::new(&git_dir).join(INDEX);
        let copy = Self {
            dir: tempfile::tempdir().map_err(|e| format!("make a temp directory: {e}"))?,
        };
        let mut original = match File::open(&index) {
            Ok(file) => file,
            Err(e) if e.kind() == ErrorKind::NotFound => return Ok(copy),
            Err(e) => return Err(format!("read {}: {e}", index.display())),
        };
        let mut write = |path: &Path| {
            let modified = original.metadata()?.modified()?;
            let mut file = File::create_new(path)?;
            std::io::copy(&mut original, &mut file)?;
            file.set_modified(modified)
        };
        write(&copy.path())
            .map_err(|e| format!("copy {} to {}: {e}", index.display(), copy.path().display()))?;
        Ok(copy)
    }

    /// Where the copy is, or, for a worktree with no index, where git finds none.
    fn path(&self) -> PathBuf {
        self.dir.path().join(INDEX)
    }
}

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
        Self::finish(dir, args, start_command(read_git(dir), args))
    }

    /// [`Self::git`]'s answer for the run of `git <args>` in `dir` that `started` began.
    fn finish(
        dir: &Path,
        args: &[&str],
        started: std::io::Result<std::process::Child>,
    ) -> Result<Vec<u8>, String> {
        finish_git(started, GitOutput::Stdout).map_err(|said| failed(dir, args, &said))
    }

    /// The trimmed answer of `git <args>` in the repository, or the failure naming the command
    /// and what git said.
    fn answer(&self, args: &[&str]) -> Result<String, String> {
        Self::git(&self.root, args).map(|out| trimmed(&out))
    }

    /// The trimmed answer of the git query `args` in the repository ([`finish_git_query`]): none
    /// for git's quiet no, and the failure naming the command and what git said for any other
    /// failure.
    fn query(&self, args: &[&str]) -> Result<Option<String>, String> {
        finish_git_query(start_command(read_git(&self.root), args))
            .map(|out| out.as_deref().map(trimmed))
            .map_err(|said| failed(&self.root, args, &said))
    }
}

/// The failure of `git <args>` in `dir`, naming the command and what git said.
fn failed(dir: &Path, args: &[&str], said: &[u8]) -> String {
    format!(
        "git {} in {}: {}",
        args.join(" "),
        dir.display(),
        String::from_utf8_lossy(said).trim()
    )
}

/// A git answer as trimmed text.
fn trimmed(out: &[u8]) -> String {
    String::from_utf8_lossy(out).trim().to_owned()
}

/// The fields of a `-z` output: NUL-separated, the last one empty.
fn fields(out: &[u8]) -> impl Iterator<Item = &[u8]> {
    out.split(|b| *b == 0).filter(|f| !f.is_empty())
}

/// The blob a listing line names, when the line is a file a listing keeps.
fn file_side(mode: &str, blob: &str) -> Option<String> {
    FILE_MODES.contains(&mode).then(|| blob.to_owned())
}

impl TreeSource for GitTreeSource {
    /// The checked-out commit and its tree. A branch with no commit yet, which git answers by
    /// its quiet no that `HEAD` names no object, names the empty tree; a head git fails to read
    /// any other way, a commit it cannot read included, is an error.
    fn head(&self) -> Result<Tip, String> {
        let branch = self.query(&["symbolic-ref", "--short", "-q", "HEAD"])?;
        let named = self.query(&["rev-parse", "--verify", "-q", "HEAD"])?;
        let (commit, tree) = match (named, &branch) {
            (Some(named), _) => {
                let commit =
                    self.answer(&["rev-parse", "--verify", &format!("{named}^{{commit}}")])?;
                let tree = self.answer(&["rev-parse", &format!("{commit}^{{tree}}")])?;
                (Some(commit), tree)
            }
            (None, Some(_)) => (
                None,
                self.answer(&["hash-object", "-t", "tree", "--stdin"])?,
            ),
            (None, None) => {
                return Err(format!(
                    "{} has no HEAD git can resolve",
                    self.root.display()
                ))
            }
        };
        Ok(Tip {
            head: branch.unwrap_or_else(|| "HEAD".into()),
            commit,
            tree,
        })
    }

    /// Read from git's files ([`read_head`]): the checkout is at `tip` when its branch and
    /// commit are `tip`'s, since a commit names one tree.
    fn still_at(&self, tip: &Tip) -> bool {
        read_head(&self.root).is_some_and(|(head, commit)| head == tip.head && commit == tip.commit)
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

    /// Staged and unstaged edits both count: a path the tree holds is changed when the
    /// worktree's bytes differ or are gone, and a path it does not hold is untracked unless
    /// git ignores it. A change of the worktree's mode alone leaves the bytes as they were, so
    /// it is no change. The diff and the untracked listing run at once, the diff over a copy of
    /// the worktree's index ([`IndexCopy`]).
    fn changed_against_worktree(
        &self,
        tree: &str,
        worktree: &Path,
    ) -> Result<WorktreeDiff, String> {
        let diff = [
            "-c",
            "core.fileMode=false",
            "diff",
            "--name-status",
            "-z",
            "--no-renames",
            "--no-relative",
            tree,
        ];
        let listing = [
            "ls-files",
            "--others",
            "--exclude-standard",
            "-z",
            "--full-name",
        ];
        let listing_started = start_command(read_git(worktree), &listing);
        let status = IndexCopy::of(worktree).and_then(|copy| {
            let mut git = read_git(worktree);
            git.env(INDEX_FILE, copy.path());
            Self::finish(worktree, &diff, start_command(git, &diff))
        });
        let others = Self::finish(worktree, &listing, listing_started);
        let (status, others) = (status?, others?);
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
        let mut batch = crate::worktree::BlobBatch::start(&self.root, read_git(&self.root))
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

    /// Sets the executable bits of `file`, on an OS whose files carry them.
    #[cfg(unix)]
    fn make_executable(file: &Path) {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(file, std::fs::Permissions::from_mode(0o755)).unwrap();
    }

    /// A file on this OS carries no executable bit to set.
    #[cfg(not(unix))]
    fn make_executable(_: &Path) {}

    /// Moves `file`'s modification time an hour back, its bytes as they were: a stat change git's
    /// diff refreshes the index it reads over.
    fn restamp(file: &Path) {
        let an_hour_ago = std::time::SystemTime::now() - std::time::Duration::from_secs(3600);
        std::fs::File::options()
            .write(true)
            .open(file)
            .and_then(|file| file.set_modified(an_hour_ago))
            .unwrap();
    }

    fn paths(entries: &[Entry]) -> Vec<String> {
        entries.iter().map(|e| e.path.clone()).collect()
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
            #[cfg(unix)]
            assert!(source.still_at(&tip), "{}: still at its head", row.name);
            write(dir.path(), &[("moved.txt", "m\n")]);
            git_commit_all(dir.path(), "moved");
            assert!(!source.still_at(&tip), "{}: moved past its head", row.name);
        }
    }

    /// A branch whose ref names a commit git cannot read is a head that cannot be read, never a
    /// branch with no commit: only git's own answer that the ref does not exist names the empty
    /// tree.
    #[test]
    fn a_head_git_cannot_read_is_an_error_never_the_empty_tree() {
        fn objects_file(root: &Path) -> PathBuf {
            let commit = git_out(root, &["rev-parse", "HEAD"]);
            let (fan, rest) = commit.split_at(2);
            root.join(".git").join("objects").join(fan).join(rest)
        }
        /// Leaves the head of the repository at its root unreadable.
        type BreakHead = fn(&Path);
        let rows: [(&str, BreakHead); 2] = [
            ("a commit whose object is corrupt", |root| {
                let file = objects_file(root);
                std::fs::remove_file(&file).unwrap();
                std::fs::write(&file, b"junk").unwrap();
            }),
            ("a branch naming a commit the repository lacks", |root| {
                let missing = "1".repeat(git_out(root, &["rev-parse", "HEAD"]).len());
                std::fs::write(
                    root.join(".git").join("refs").join("heads").join("work"),
                    format!("{missing}\n"),
                )
                .unwrap();
            }),
        ];
        for (name, break_head) in rows {
            let dir = repo(&[("a.txt", "a\n")]);
            git_ok(dir.path(), &["checkout", "-q", "-b", "work"]);
            break_head(dir.path());
            let head = GitTreeSource::new(dir.path()).head();
            assert!(head.is_err(), "{name}: read as {head:?}");
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
    fn git_and_dir_sources_list_the_same_files_and_bytes_across_commits() {
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
            // Each source is asked about its head as it reads it, before the commit after it: the
            // directory source answers only for the tree it read last.
            let agree_at_their_heads = || {
                let (g, d) = (git.head().unwrap(), plain.head().unwrap());
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
            };
            agree_at_their_heads();
            write(dir.path(), row.write);
            remove(dir.path(), row.delete);
            git_ok(dir.path(), &["add", "-A"]);
            if let Some(path) = row.executable {
                git_ok(dir.path(), &["update-index", "--chmod=+x", path]);
            }
            git_ok(dir.path(), &["commit", "-q", "-m", row.name]);
            agree_at_their_heads();
        }
    }

    #[test]
    fn the_git_and_directory_sources_list_the_same_paths_for_one_tree() {
        let tree: Files = &[
            (".gitignore", "target/\n"),
            (".hidden/h.toy", "def hidden\n"),
            (".rigger/workflow.yml", "stages: []\n"),
            ("target/out.toy", "def built\n"),
            ("src/a.toy", "def a\n"),
        ];
        let checkout = repo(tree);
        let copy = tempfile::tempdir().unwrap();
        write(copy.path(), tree);
        type Opens = fn(&Path) -> Box<dyn TreeSource>;
        let rows: [(&str, &Path, Opens); 3] = [
            (
                "the git source over the checkout",
                checkout.path(),
                |root| Box::new(GitTreeSource::new(root)),
            ),
            (
                "the directory source over the checkout",
                checkout.path(),
                |root| Box::new(DirTreeSource::new(root)),
            ),
            (
                "the directory source over a plain copy",
                copy.path(),
                |root| Box::new(DirTreeSource::new(root)),
            ),
        ];
        for (name, root, source) in rows {
            let source = source(root);
            let extract = ToyExtract::new(1);
            let store = tempfile::tempdir().unwrap();
            Store::new(store.path())
                .advance(
                    source.as_ref(),
                    &extract,
                    &CountingGroup::default(),
                    &source.head().expect(name),
                )
                .expect(name);
            assert_eq!(
                extract.paths(),
                [".rigger/workflow.yml", "src/a.toy"],
                "{name}"
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
            /// Paths made executable in the worktree before the diff.
            chmod: &'static [&'static str],
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
                chmod: &[],
                changed: &[],
                untracked: &[],
            },
            Row {
                name: "modified",
                write: &[("a.txt", "a2\n")],
                delete: &[],
                stage: &[],
                chmod: &[],
                changed: &["a.txt"],
                untracked: &[],
            },
            Row {
                name: "modified and staged",
                write: &[("a.txt", "a2\n")],
                delete: &[],
                stage: &["a.txt"],
                chmod: &[],
                changed: &["a.txt"],
                untracked: &[],
            },
            Row {
                name: "deleted",
                write: &[],
                delete: &["b.txt"],
                stage: &[],
                chmod: &[],
                changed: &["b.txt"],
                untracked: &[],
            },
            Row {
                name: "untracked",
                write: &[("new.txt", "n\n")],
                delete: &[],
                stage: &[],
                chmod: &[],
                changed: &[],
                untracked: &["new.txt"],
            },
            Row {
                name: "added and staged",
                write: &[("new.txt", "n\n")],
                delete: &[],
                stage: &["new.txt"],
                chmod: &[],
                changed: &[],
                untracked: &["new.txt"],
            },
            Row {
                name: "ignored",
                write: &[("ignored.txt", "i\n")],
                delete: &[],
                stage: &[],
                chmod: &[],
                changed: &[],
                untracked: &[],
            },
            Row {
                name: "a mode change alone is no change",
                write: &[],
                delete: &[],
                stage: &[],
                chmod: &["a.txt"],
                changed: &[],
                untracked: &[],
            },
            Row {
                name: "every kind at once",
                write: &[("a.txt", "a2\n"), ("z/new.txt", "n\n")],
                delete: &["b.txt"],
                stage: &[],
                chmod: &[],
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
            for path in row.chmod {
                make_executable(&dir.path().join(path));
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

    /// A read leaves the index of the worktree it reads as it was, and takes no lock on it, even
    /// where a file's stat changed while its bytes did not, over which git's diff refreshes the
    /// index it reads.
    #[test]
    fn a_read_leaves_the_worktrees_index_as_it_was() {
        struct Row {
            name: &'static str,
            /// The worktree to read, readied beside the checkout `main`, whose head commit holds
            /// `a.txt` and `b.txt`.
            worktree: fn(&Path) -> PathBuf,
            changed: &'static [&'static str],
        }
        let rows = [
            Row {
                name: "the main checkout",
                worktree: Path::to_path_buf,
                changed: &[],
            },
            Row {
                name: "a linked worktree, whose index is in its own administrative directory",
                worktree: |main| {
                    let linked = main.with_file_name("linked");
                    let at = linked.to_str().unwrap();
                    git_ok(main, &["worktree", "add", "-q", "--detach", at]);
                    linked
                },
                changed: &[],
            },
            Row {
                name: "a checkout with no index, which git reads as every file deleted",
                worktree: |main| {
                    std::fs::remove_file(main.join(".git").join("index")).unwrap();
                    main.to_path_buf()
                },
                changed: &["a.txt", "b.txt"],
            },
        ];
        for row in rows {
            let dir = tempfile::tempdir().unwrap();
            let main = dir.path().join("main");
            std::fs::create_dir(&main).unwrap();
            init_repo(&main);
            write(&main, &[("a.txt", "a\n"), ("b.txt", "b\n")]);
            git_commit_all(&main, "base");
            let worktree = (row.worktree)(&main);
            restamp(&worktree.join("a.txt"));
            let git_dir = git_out(&worktree, &["rev-parse", "--absolute-git-dir"]);
            let index = Path::new(&git_dir).join("index");
            let before = std::fs::read(&index).ok();
            let source = GitTreeSource::new(&main);
            let tree = source.head().expect(row.name).tree;
            let diff = source
                .changed_against_worktree(&tree, &worktree)
                .expect(row.name);
            assert_eq!(
                std::fs::read(&index).ok(),
                before,
                "{}: the worktree's index",
                row.name
            );
            let changed = row.changed.iter().map(|p| (*p).to_owned()).collect();
            assert_eq!(
                diff,
                WorktreeDiff {
                    changed,
                    untracked: Vec::new(),
                },
                "{}",
                row.name
            );
            assert!(
                !index.with_extension("lock").exists(),
                "{}: the worktree's index lock",
                row.name
            );
        }
    }

    /// An index copy holds the worktree's index as it is, its modification time included, or
    /// nothing for a worktree with no index; dropping it removes its directory.
    #[test]
    fn an_index_copy_holds_the_index_as_it_is_until_dropped() {
        let modified = |file: &Path| std::fs::metadata(file).and_then(|m| m.modified()).ok();
        for (name, indexed) in [("a checkout with an index", true), ("one with none", false)] {
            let checkout = repo(&[("a.txt", "a\n")]);
            let index = checkout.path().join(".git").join(INDEX);
            restamp(&index);
            if !indexed {
                std::fs::remove_file(&index).unwrap();
            }
            let copy = IndexCopy::of(checkout.path()).expect(name);
            let held = std::fs::read(copy.path()).ok();
            assert_eq!(held, std::fs::read(&index).ok(), "{name}: the bytes");
            assert_eq!(held.is_some(), indexed, "{name}: a copy");
            assert_eq!(modified(&copy.path()), modified(&index), "{name}: the time");
            let dir = copy.dir.path().to_path_buf();
            drop(copy);
            assert!(!dir.exists(), "{name}: the copy's directory");
        }
    }

    /// Every git the source starts itself is built from [`read_git`], which turns optional locks
    /// off.
    #[test]
    fn the_sources_git_runs_with_optional_locks_off() {
        let git = read_git(Path::new("/r"));
        let args: Vec<_> = git.get_args().collect();
        assert_eq!(args, ["-C", "/r", "--no-optional-locks"]);
    }
}
