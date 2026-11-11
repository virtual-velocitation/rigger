//! Where a directory's git repository lies and what its checkout holds, read from the files git
//! keeps rather than asked of a git process. Two plain shapes are read: a checkout holding a
//! `.git` directory, and a linked worktree whose `.git` file names its administrative directory
//! by an absolute path. Anything else is left to git, so every answer is the one git gives: a
//! bare repository, a relative `gitdir`, a configured work tree or worktree configuration, an
//! environment that narrows git's walk, a walk crossing a device, a repository this user does not
//! own, an OS whose ownership is not read here, or refs held in a reftable.

use crate::worktree::{git_output, GitOutput};
use rigger_process::file_trust::{device_of, owned_by_this_user};
use std::io::ErrorKind;
use std::path::{Component, Path, PathBuf};

/// The variables that narrow or widen git's walk up from a directory, which this reading does
/// not follow.
const WALK_VARIABLES: [&str; 2] = ["GIT_CEILING_DIRECTORIES", "GIT_DISCOVERY_ACROSS_FILESYSTEM"];

/// The repository a directory lies in.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Repository {
    /// The physical top of the worktree the directory lies in.
    pub toplevel: PathBuf,
    /// The worktree's administrative directory: the `.git` directory, or the one a `.git` file
    /// names.
    pub git_dir: PathBuf,
    /// The directory every worktree of the repository shares.
    pub common_dir: PathBuf,
    /// How many directories below `toplevel` the physical directory lies.
    depth: usize,
}

/// What `git rev-parse` answers about where a directory's repository lies.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Location {
    /// `--show-toplevel`: the physical top of the worktree.
    Toplevel,
    /// `--git-common-dir`: the common directory, relative to the directory asked from in a main
    /// checkout and absolute in a linked worktree.
    CommonDir,
}

impl Location {
    fn flag(self) -> &'static str {
        match self {
            Location::Toplevel => "--show-toplevel",
            Location::CommonDir => "--git-common-dir",
        }
    }
}

impl Repository {
    /// What `git rev-parse` answers for `ask` from the directory this repository was read from.
    pub fn answer(&self, ask: Location) -> String {
        match ask {
            Location::Toplevel => self.toplevel.to_string_lossy().into_owned(),
            Location::CommonDir if self.git_dir == self.common_dir => {
                format!("{}.git", "../".repeat(self.depth))
            }
            Location::CommonDir => self.common_dir.to_string_lossy().into_owned(),
        }
    }

    /// Whether a ref named `name` exists, loose in the worktree's or the common directory, or
    /// packed in `packed`.
    fn has_ref(&self, name: &str, packed: &str) -> bool {
        self.git_dir.join(name).is_file()
            || self.common_dir.join(name).is_file()
            || packed_ref(packed, name).is_some()
    }
}

/// The repository `start` lies in, read from git's files: `Some(Some(_))` when found,
/// `Some(None)` when `start` lies in none, and `None` when the files leave the answer to git.
pub fn read_repository(start: &Path) -> Option<Option<Repository>> {
    if WALK_VARIABLES
        .iter()
        .any(|variable| std::env::var_os(variable).is_some())
    {
        return None;
    }
    let physical = match std::fs::canonicalize(start) {
        Ok(physical) => physical,
        Err(e) if e.kind() == ErrorKind::NotFound => return Some(None),
        Err(_) => return None,
    };
    let device = device_of(&physical)?;
    for (depth, dir) in physical.ancestors().enumerate() {
        if device_of(dir)? != device {
            return None;
        }
        let dot = dir.join(".git");
        match std::fs::symlink_metadata(&dot) {
            Ok(meta) if meta.is_dir() => return main_checkout(dir, dot, depth).map(Some),
            Ok(meta) if meta.is_file() => return linked_worktree(dir, &dot, depth).map(Some),
            Ok(_) => return None,
            Err(e) if e.kind() == ErrorKind::NotFound => {}
            Err(_) => return None,
        }
        if dir.join("HEAD").exists() && dir.join("objects").is_dir() {
            return None;
        }
    }
    Some(None)
}

/// The main checkout at `toplevel`, whose `.git` directory is `dot`.
fn main_checkout(toplevel: &Path, dot: PathBuf, depth: usize) -> Option<Repository> {
    let plain = dot.join("HEAD").is_file()
        && dot.join("objects").is_dir()
        && !dot.join("commondir").exists()
        && plain_config(&dot, &dot)
        && owned(&[toplevel, &dot]);
    plain.then(|| Repository {
        toplevel: toplevel.to_path_buf(),
        git_dir: dot.clone(),
        common_dir: dot,
        depth,
    })
}

/// The linked worktree at `toplevel`, whose `.git` file is `dot`.
fn linked_worktree(toplevel: &Path, dot: &Path, depth: usize) -> Option<Repository> {
    let text = std::fs::read_to_string(dot).ok()?;
    let git_dir = PathBuf::from(
        text.strip_prefix("gitdir: ")?
            .trim_end_matches(['\n', '\r']),
    );
    if !git_dir.is_absolute() || !git_dir.join("HEAD").is_file() {
        return None;
    }
    let common = std::fs::read_to_string(git_dir.join("commondir")).ok()?;
    let common_dir = normalized(&git_dir.join(common.trim_end_matches(['\n', '\r'])));
    let plain = common_dir.join("objects").is_dir()
        && plain_config(&git_dir, &common_dir)
        && owned(&[toplevel, dot, &git_dir, &common_dir]);
    plain.then(|| Repository {
        toplevel: toplevel.to_path_buf(),
        git_dir,
        common_dir,
        depth,
    })
}

/// Whether the repository's configuration leaves the worktree where its `.git` lies: no
/// per-worktree configuration, and in the shared one no included file, no line naming a work
/// tree and no `bare` key set other than false. A configuration that cannot be read is not plain.
fn plain_config(git_dir: &Path, common_dir: &Path) -> bool {
    if git_dir.join("config.worktree").exists() {
        return false;
    }
    std::fs::read_to_string(common_dir.join("config")).is_ok_and(|config| {
        config.lines().all(|line| {
            let line = line.trim().to_ascii_lowercase();
            let (key, value) = line.split_once('=').unwrap_or((&line, "true"));
            !line.contains("worktree")
                && !line.contains("include")
                && (key.trim() != "bare" || matches!(value.trim(), "false" | "no" | "off" | "0"))
        })
    })
}

/// Whether this user owns every one of `paths`, as git requires before it trusts a repository.
fn owned(paths: &[&Path]) -> bool {
    paths
        .iter()
        .all(|path| owned_by_this_user(path) == Some(true))
}

/// `path` with its `.` and `..` components resolved lexically.
fn normalized(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for part in path.components() {
        match part {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            other => out.push(other),
        }
    }
    out
}

/// The object id `packed` (a `packed-refs` file's text) holds for the ref `name`.
fn packed_ref<'p>(packed: &'p str, name: &str) -> Option<&'p str> {
    packed
        .lines()
        .filter(|line| !line.starts_with(['#', '^']))
        .find_map(|line| {
            let (id, rest) = line.split_once(' ')?;
            (rest == name).then_some(id)
        })
}

/// Whether `text` is a full object id: forty (SHA-1) or sixty-four (SHA-256) lowercase hex
/// digits.
fn is_object_id(text: &str) -> bool {
    matches!(text.len(), 40 | 64)
        && text
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// `git -C <start> rev-parse <ask>`'s trimmed answer, none when git refuses: read from git's
/// files for a plain repository ([`read_repository`]), else asked of git.
pub fn rev_parse(start: &Path, ask: Location) -> Option<String> {
    match read_repository(start) {
        Some(found) => found.map(|repo| repo.answer(ask)),
        None => git_output(start, &["rev-parse", ask.flag()], GitOutput::Stdout)
            .ok()
            .map(|out| String::from_utf8_lossy(&out).trim().to_owned()),
    }
}

/// The checkout `start` lies in, read from git's files: the short name of the branch its `HEAD`
/// names (`HEAD` when detached) and the commit it is at (none on a branch with no commit yet), as
/// `git symbolic-ref --short -q HEAD` and `git rev-parse --verify -q HEAD^{commit}` answer them.
/// None when the files leave it to git: a repository that is not plain, refs in a reftable, a
/// `HEAD` naming a ref outside the branches, a branch name another ref also spells (which git
/// would not shorten), or a symbolic branch ref.
pub fn read_head(start: &Path) -> Option<(String, Option<String>)> {
    let repo = read_repository(start)??;
    if repo.common_dir.join("reftable").exists() {
        return None;
    }
    let head = std::fs::read_to_string(repo.git_dir.join("HEAD")).ok()?;
    let head = head.trim_end_matches(['\n', '\r']);
    let Some(full) = head.strip_prefix("ref: ") else {
        return is_object_id(head).then(|| ("HEAD".to_owned(), Some(head.to_owned())));
    };
    let name = full.strip_prefix("refs/heads/")?;
    let packed = match std::fs::read_to_string(repo.common_dir.join("packed-refs")) {
        Ok(packed) => packed,
        Err(e) if e.kind() == ErrorKind::NotFound => String::new(),
        Err(_) => return None,
    };
    let spelled_elsewhere = [
        name.to_owned(),
        format!("refs/{name}"),
        format!("refs/tags/{name}"),
        format!("refs/remotes/{name}"),
        format!("refs/remotes/{name}/HEAD"),
    ]
    .iter()
    .any(|other| repo.has_ref(other, &packed));
    if spelled_elsewhere {
        return None;
    }
    let commit = match std::fs::read_to_string(repo.common_dir.join(full)) {
        Ok(loose) => {
            let loose = loose.trim_end_matches(['\n', '\r']);
            Some(is_object_id(loose).then(|| loose.to_owned())?)
        }
        Err(e) if e.kind() == ErrorKind::NotFound => packed_ref(&packed, full).map(str::to_owned),
        Err(_) => return None,
    };
    Some((name.to_owned(), commit))
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use crate::test_support::{git_ok, git_out, init_repo, run_git};

    /// git's own answer to `rev-parse <ask>` from `dir`, none when it refuses.
    fn git_says(dir: &Path, ask: Location) -> Option<String> {
        let out = run_git(dir, &["rev-parse", ask.flag()]);
        out.status
            .success()
            .then(|| String::from_utf8_lossy(&out.stdout).trim().to_owned())
    }

    /// git's own answer to which branch and commit `dir`'s checkout is at.
    fn git_head(dir: &Path) -> (String, Option<String>) {
        let answer = |args: &[&str]| {
            let out = run_git(dir, args);
            out.status
                .success()
                .then(|| String::from_utf8_lossy(&out.stdout).trim().to_owned())
        };
        (
            answer(&["symbolic-ref", "--short", "-q", "HEAD"]).unwrap_or_else(|| "HEAD".into()),
            answer(&["rev-parse", "--verify", "-q", "HEAD^{commit}"]),
        )
    }

    /// A main checkout `main` with a nested and an outside linked worktree, a branch with no
    /// commit, a detached worktree, packed refs, and symlinks into each; every path the table
    /// below names lies under the returned directory.
    fn shapes() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let main = root.join("main");
        std::fs::create_dir_all(main.join("a/b")).unwrap();
        init_repo(&main);
        std::fs::create_dir_all(main.join(".rigger/tmp")).unwrap();
        git_ok(
            &main,
            &["worktree", "add", "-q", ".rigger/tmp/wt", "-b", "nested"],
        );
        git_ok(
            &main,
            &["worktree", "add", "-q", "../linked", "-b", "outside"],
        );
        std::fs::create_dir_all(root.join("linked/s/t")).unwrap();
        git_ok(&main, &["worktree", "add", "-q", "--detach", "../detached"]);
        git_ok(&main, &["branch", "packed"]);
        git_ok(&main, &["pack-refs", "--all"]);
        git_ok(&main, &["worktree", "add", "-q", "../on-packed", "packed"]);
        git_ok(
            &main,
            &[
                "worktree",
                "add",
                "-q",
                "--orphan",
                "-b",
                "unborn",
                "../unborn",
            ],
        );
        git_ok(&main, &["tag", "outside"]);
        std::fs::create_dir_all(root.join("plain")).unwrap();
        for (link, target) in [
            ("link-a", "main/a"),
            ("link-ls", "linked/s"),
            ("mainlink", "main"),
        ] {
            std::os::unix::fs::symlink(root.join(target), root.join(link)).unwrap();
        }
        dir
    }

    /// Each row: a directory under the shapes, and whether its answers are read from git's files
    /// rather than left to git. Every answer read must be the one git gives.
    #[test]
    fn the_repository_read_from_files_is_the_one_git_names() {
        let dir = shapes();
        let cases: &[(&str, bool)] = &[
            ("main", true),
            ("main/a", true),
            ("main/a/b", true),
            ("main/.rigger/tmp/wt", true),
            ("linked", true),
            ("linked/s/t", true),
            ("detached", true),
            ("on-packed", true),
            ("unborn", true),
            ("link-a", true),
            ("link-ls", true),
            ("mainlink/a", true),
            ("mainlink/.rigger/tmp/wt", true),
            ("main/.git", false),
            ("main/.git/refs", false),
            ("missing", true),
        ];
        for (path, read) in cases {
            let start = dir.path().join(path);
            let found = read_repository(&start);
            assert_eq!(found.is_some(), *read, "{path}: read from files");
            for ask in [Location::Toplevel, Location::CommonDir] {
                let from_files = found.as_ref().map(|r| r.as_ref().map(|r| r.answer(ask)));
                if let Some(answer) = from_files {
                    assert_eq!(answer, git_says(&start, ask), "{path}: {ask:?}");
                }
                assert_eq!(
                    rev_parse(&start, ask),
                    git_says(&start, ask),
                    "{path}: {ask:?}"
                );
            }
        }
        let outside = tempfile::tempdir().unwrap();
        if git_says(outside.path(), Location::Toplevel).is_none() {
            let found = read_repository(outside.path());
            assert!(matches!(found, None | Some(None)), "{found:?}");
        }
    }

    /// Each row: a checkout, and whether its branch and commit are read from git's files. Every
    /// one read is the one git names; `outside`, which a tag also spells, is left to git.
    #[test]
    fn the_head_read_from_files_is_the_one_git_names() {
        let dir = shapes();
        let cases: &[(&str, bool)] = &[
            ("main", true),
            ("main/a", true),
            ("main/.rigger/tmp/wt", true),
            ("linked/s/t", false),
            ("detached", true),
            ("on-packed", true),
            ("unborn", true),
            ("main/.git", false),
        ];
        for (path, read) in cases {
            let start = dir.path().join(path);
            let head = read_head(&start);
            assert_eq!(head.is_some(), *read, "{path}: read from files");
            if let Some(head) = head {
                assert_eq!(head, git_head(&start), "{path}");
            }
        }
        let unborn = read_head(&dir.path().join("unborn")).unwrap();
        assert_eq!(unborn, ("unborn".to_owned(), None));
        let packed = read_head(&dir.path().join("on-packed")).unwrap();
        assert_eq!(
            packed.1,
            Some(git_out(dir.path().join("main"), &["rev-parse", "packed"]))
        );
    }
}
