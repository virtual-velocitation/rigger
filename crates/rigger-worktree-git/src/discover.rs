//! Where a directory's git repository lies and what its checkout holds, read from the files git
//! keeps rather than asked of a git process. Two plain shapes are read: a checkout holding a
//! `.git` directory, and a linked worktree whose `.git` file names its administrative directory
//! by an absolute path. Anything else is left to git, so every answer is the one git gives: a
//! bare repository, a relative `gitdir`, a configured work tree or worktree configuration, an
//! environment that narrows or widens git's walk, a repository this user does not own, an OS
//! whose ownership is not read here, or refs held in a reftable. A directory lies in no
//! repository only when a walk up from it, stopping where git's own walk stops, meets neither a
//! `.git` entry nor a bare repository; git refusing anywhere else is a failure, never that none
//! is there.

use crate::worktree::{git_failure, git_output, GitOutput};
use rigger_process::file_trust::{device_of, owned_by_this_user};
use std::ffi::OsString;
use std::io::ErrorKind;
use std::path::{Component, Path, PathBuf};

/// The directories git's walk up from a directory never enters.
const CEILINGS: &str = "GIT_CEILING_DIRECTORIES";

/// Whether git's walk up from a directory crosses devices.
const ACROSS_DEVICES: &str = "GIT_DISCOVERY_ACROSS_FILESYSTEM";

/// The variables that narrow or widen git's walk up from a directory, under which no answer is
/// read from the files.
const WALK_VARIABLES: [&str; 2] = [CEILINGS, ACROSS_DEVICES];

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
    /// The physical directory's path below `toplevel`.
    prefix: PathBuf,
}

/// What `git rev-parse` answers about where a directory's repository lies.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Location {
    /// `--show-toplevel`: the physical top of the worktree.
    Toplevel,
    /// `--git-common-dir`: the common directory, relative to the directory asked from in a main
    /// checkout and absolute in a linked worktree.
    CommonDir,
    /// `--show-prefix`: the directory's path below the top of its worktree, each part followed
    /// by `/`; empty at the top.
    Prefix,
}

impl Location {
    fn flag(self) -> &'static str {
        match self {
            Location::Toplevel => "--show-toplevel",
            Location::CommonDir => "--git-common-dir",
            Location::Prefix => "--show-prefix",
        }
    }
}

impl Repository {
    /// What `git rev-parse` answers for `ask` from the directory this repository was read from.
    pub fn answer(&self, ask: Location) -> String {
        match ask {
            Location::Toplevel => self.toplevel.to_string_lossy().into_owned(),
            Location::CommonDir if self.git_dir == self.common_dir => {
                format!("{}.git", "../".repeat(self.prefix.components().count()))
            }
            Location::CommonDir => self.common_dir.to_string_lossy().into_owned(),
            Location::Prefix => self
                .prefix
                .iter()
                .map(|part| format!("{}/", part.to_string_lossy()))
                .collect(),
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

/// Where git's walk up from a directory stops, as its environment sets it.
#[derive(Debug, Default)]
struct Reach {
    /// The directories the walk never enters ([`CEILINGS`]): its absolute entries, each resolved
    /// to its physical path up to the first empty entry and taken as written after it, as git
    /// takes them. An entry that cannot be resolved is none.
    ceilings: Vec<PathBuf>,
    /// Whether the walk crosses devices ([`ACROSS_DEVICES`]). Any value set is taken to let it,
    /// which can only lengthen the walk.
    across_devices: bool,
}

impl Reach {
    /// The reach this process's environment sets.
    fn from_env() -> Self {
        Self::of(
            std::env::var_os(CEILINGS),
            std::env::var_os(ACROSS_DEVICES).is_some(),
        )
    }

    /// The reach a [`CEILINGS`] value and an [`ACROSS_DEVICES`] setting give.
    fn of(ceilings: Option<OsString>, across_devices: bool) -> Self {
        let mut resolved = true;
        let ceilings = ceilings
            .iter()
            .flat_map(std::env::split_paths)
            .filter_map(|entry| {
                if entry.as_os_str().is_empty() {
                    resolved = false;
                    None
                } else if !entry.is_absolute() {
                    None
                } else if resolved {
                    std::fs::canonicalize(entry).ok()
                } else {
                    Some(entry)
                }
            })
            .collect();
        Self {
            ceilings,
            across_devices,
        }
    }
}

/// What a walk up from a directory, stopping where git's own walk stops, meets first.
#[derive(Debug, PartialEq, Eq)]
enum Met {
    /// A `.git` directory, or a `.git` file when `file`, in `toplevel`, `prefix` below which
    /// the start lies.
    Dot {
        toplevel: PathBuf,
        prefix: PathBuf,
        file: bool,
    },
    /// What git may take for a repository and this reading does not follow: a `.git` that is
    /// neither a file nor a directory, a bare repository's shape, or an entry it cannot read.
    Unread,
    /// Nothing: the walk reached a ceiling, a device boundary or the root, or the start does
    /// not exist.
    Nothing,
}

/// Walks up from `start` as git does: from its physical path, never into a ceiling of `reach`
/// that is a proper ancestor, and never onto another device unless `reach` crosses devices
/// (a device this OS does not report is never a boundary, which can only lengthen the walk).
fn walk(start: &Path, reach: &Reach) -> Met {
    // An empty `start` is the current directory, as `git -C ""` takes it.
    let start = if start.as_os_str().is_empty() {
        Path::new(".")
    } else {
        start
    };
    let physical = match std::fs::canonicalize(start) {
        Ok(physical) => physical,
        Err(e) if e.kind() == ErrorKind::NotFound => return Met::Nothing,
        Err(_) => return Met::Unread,
    };
    let ceiling = reach
        .ceilings
        .iter()
        .filter(|ceiling| physical.starts_with(ceiling) && physical != **ceiling)
        .map(|ceiling| ceiling.components().count())
        .max();
    let device = if reach.across_devices {
        None
    } else {
        device_of(&physical)
    };
    for dir in physical.ancestors() {
        if ceiling.is_some_and(|depth| dir.components().count() <= depth) {
            return Met::Nothing;
        }
        if let Some(device) = device {
            match device_of(dir) {
                Some(here) if here == device => {}
                Some(_) => return Met::Nothing,
                None => return Met::Unread,
            }
        }
        let dot = dir.join(".git");
        match std::fs::symlink_metadata(&dot) {
            Ok(meta) if meta.is_dir() || meta.is_file() => {
                let Ok(prefix) = physical.strip_prefix(dir) else {
                    return Met::Unread;
                };
                return Met::Dot {
                    toplevel: dir.to_path_buf(),
                    prefix: prefix.to_path_buf(),
                    file: meta.is_file(),
                };
            }
            Ok(_) => return Met::Unread,
            Err(e) if e.kind() == ErrorKind::NotFound => {}
            Err(_) => return Met::Unread,
        }
        if dir.join("HEAD").exists() && dir.join("objects").is_dir() {
            return Met::Unread;
        }
    }
    Met::Nothing
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
    match walk(start, &Reach::default()) {
        Met::Dot {
            toplevel,
            prefix,
            file: false,
        } => main_checkout(&toplevel, prefix).map(Some),
        Met::Dot {
            toplevel,
            prefix,
            file: true,
        } => linked_worktree(&toplevel, prefix).map(Some),
        Met::Unread => None,
        Met::Nothing => Some(None),
    }
}

/// The main checkout at `toplevel`, `prefix` below which the start lies.
fn main_checkout(toplevel: &Path, prefix: PathBuf) -> Option<Repository> {
    let dot = toplevel.join(".git");
    let plain = dot.join("HEAD").is_file()
        && dot.join("objects").is_dir()
        && !dot.join("commondir").exists()
        && plain_config(&dot, &dot)
        && owned(&[toplevel, &dot]);
    plain.then(|| Repository {
        toplevel: toplevel.to_path_buf(),
        git_dir: dot.clone(),
        common_dir: dot,
        prefix,
    })
}

/// The linked worktree at `toplevel`, whose `.git` file names its administrative directory,
/// `prefix` below which the start lies.
fn linked_worktree(toplevel: &Path, prefix: PathBuf) -> Option<Repository> {
    let dot = toplevel.join(".git");
    let text = std::fs::read_to_string(&dot).ok()?;
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
        && owned(&[toplevel, &dot, &git_dir, &common_dir]);
    plain.then(|| Repository {
        toplevel: toplevel.to_path_buf(),
        git_dir,
        common_dir,
        prefix,
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

/// `git -C <start> rev-parse <ask>`'s trimmed answer: read from git's files for a plain
/// repository ([`read_repository`]), else asked of git. None when `start` lies in no repository;
/// a failure naming git's refusal when git refuses where a walk up from `start` meets a `.git`
/// entry or a bare repository ([`walk`]), as in a worktree whose administrative directory is
/// gone or a repository this user does not own.
pub fn rev_parse(start: &Path, ask: Location) -> Result<Option<String>, String> {
    if let Some(found) = read_repository(start) {
        return Ok(found.map(|repo| repo.answer(ask)));
    }
    let args = ["rev-parse", ask.flag()];
    asked(
        start,
        &args,
        git_output(start, &args, GitOutput::Stdout),
        || walk(start, &Reach::from_env()),
    )
}

/// What git's run of `args` from `start` answers ([`rev_parse`]): its trimmed output when it
/// ran and succeeded; when it refused or could not start, none where a walk up from `start`
/// (`met`) meets nothing, else a failure naming the command, `start` and git's cause.
fn asked(
    start: &Path,
    args: &[&str],
    ran: Result<Vec<u8>, Vec<u8>>,
    met: impl FnOnce() -> Met,
) -> Result<Option<String>, String> {
    match ran {
        Ok(out) => Ok(Some(String::from_utf8_lossy(&out).trim().to_owned())),
        Err(_) if met() == Met::Nothing => Ok(None),
        Err(why) => Err(git_failure(args, start, String::from_utf8_lossy(&why).trim()).0),
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
    /// commit, a detached worktree, packed refs, symlinks into each, a linked worktree whose
    /// administrative directory is gone (`pruned`) and a `.git` directory holding nothing
    /// (`headless`); every path the tables below name lies under the returned directory.
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
        git_ok(
            &main,
            &["worktree", "add", "-q", "../pruned", "-b", "pruned"],
        );
        std::fs::remove_dir_all(main.join(".git/worktrees/pruned")).unwrap();
        std::fs::create_dir_all(root.join("headless/.git")).unwrap();
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

    /// [`rev_parse`]'s answer from `start` for `ask`, checked against git's: git's own answer
    /// when it gives one, and when it refuses, a failure where a repository may lie (`inside`)
    /// and none where none does.
    fn assert_rev_parse_agrees(start: &Path, ask: Location, inside: bool, what: &str) {
        let asked = rev_parse(start, ask);
        match git_says(start, ask) {
            Some(answer) => assert_eq!(asked, Ok(Some(answer)), "{what}: {ask:?}"),
            None if inside => assert!(asked.is_err(), "{what}: {ask:?}: {asked:?}"),
            None => assert_eq!(asked, Ok(None), "{what}: {ask:?}"),
        }
    }

    /// Each row: a directory under the shapes, whether its answers are read from git's files
    /// rather than left to git, and whether a walk up from it meets a repository. Every answer
    /// read must be the one git gives, and where git refuses, a refusal inside a repository is a
    /// failure, never none.
    #[test]
    fn the_repository_read_from_files_is_the_one_git_names() {
        let dir = shapes();
        let cases: &[(&str, bool, bool)] = &[
            ("main", true, true),
            ("main/a", true, true),
            ("main/a/b", true, true),
            ("main/.rigger/tmp/wt", true, true),
            ("linked", true, true),
            ("linked/s/t", true, true),
            ("detached", true, true),
            ("on-packed", true, true),
            ("unborn", true, true),
            ("link-a", true, true),
            ("link-ls", true, true),
            ("mainlink/a", true, true),
            ("mainlink/.rigger/tmp/wt", true, true),
            ("main/.git", false, true),
            ("main/.git/refs", false, true),
            ("missing", true, false),
            ("pruned", false, true),
            ("headless", false, true),
        ];
        let asks = [Location::Toplevel, Location::CommonDir, Location::Prefix];
        for (path, read, inside) in cases {
            let start = dir.path().join(path);
            let found = read_repository(&start);
            assert_eq!(found.is_some(), *read, "{path}: read from files");
            let met = walk(&start, &Reach::default());
            assert_eq!(met != Met::Nothing, *inside, "{path}: {met:?}");
            for ask in asks {
                let from_files = found.as_ref().map(|r| r.as_ref().map(|r| r.answer(ask)));
                if let Some(answer) = from_files {
                    assert_eq!(answer, git_says(&start, ask), "{path}: {ask:?}");
                }
                assert_rev_parse_agrees(&start, ask, *inside, path);
            }
        }
        // An empty start is the directory this process runs in, which git takes `-C ""` to mean.
        let (here, empty) = (
            read_repository(Path::new(".")),
            read_repository(Path::new("")),
        );
        for ask in asks {
            let answer = |found: &Option<Option<Repository>>| {
                found.as_ref().map(|r| r.as_ref().map(|r| r.answer(ask)))
            };
            assert_eq!(answer(&empty), answer(&here), "an empty start: {ask:?}");
            assert_eq!(
                rev_parse(Path::new(""), ask),
                rev_parse(Path::new("."), ask),
                "an empty start: {ask:?}"
            );
            let inside = walk(Path::new(""), &Reach::default()) != Met::Nothing;
            assert_rev_parse_agrees(Path::new(""), ask, inside, "an empty start");
        }
        let outside = tempfile::tempdir().unwrap();
        if git_says(outside.path(), Location::Toplevel).is_none() {
            assert_eq!(read_repository(outside.path()), Some(None));
            for ask in asks {
                assert_eq!(rev_parse(outside.path(), ask), Ok(None), "outside: {ask:?}");
            }
        }
    }

    /// Each row: what git's run answered, what a walk up from the directory met, and what that
    /// answers: git's output where it ran, and where it refused or could not start, none only
    /// where the walk met nothing, else a failure naming the command, the directory and the cause.
    #[test]
    fn a_refusal_is_none_only_where_the_walk_meets_nothing() {
        let start = Path::new("/r");
        let args = ["rev-parse", "--git-common-dir"];
        let linked = Met::Dot {
            toplevel: start.to_path_buf(),
            prefix: PathBuf::new(),
            file: true,
        };
        let failure = |why: &str| Err(format!("git rev-parse --git-common-dir in /r: {why}"));
        type Ran = Result<Vec<u8>, Vec<u8>>;
        type Answer = Result<Option<String>, String>;
        let cases: [(Ran, Met, Answer); 5] = [
            (Ok(b".git\n".to_vec()), Met::Unread, Ok(Some(".git".into()))),
            (
                Err(b"fatal: not a git repository\n".to_vec()),
                Met::Nothing,
                Ok(None),
            ),
            (Err(b"could not start".to_vec()), Met::Nothing, Ok(None)),
            (
                Err(b"fatal: not a git repository\n".to_vec()),
                linked,
                failure("fatal: not a git repository"),
            ),
            (
                Err(b"could not start".to_vec()),
                Met::Unread,
                failure("could not start"),
            ),
        ];
        for (ran, met, answer) in cases {
            assert_eq!(asked(start, &args, ran, || met), answer);
        }
    }

    /// Each row: a directory under the shapes, the entries of `GIT_CEILING_DIRECTORIES` (each
    /// under the shapes, but for an empty entry and a relative one), and whether a repository is
    /// found from it under them. A walk up from the directory meets one exactly when git does:
    /// git never enters a ceiling that is a proper ancestor, resolves an entry to its physical
    /// path up to the first empty entry, and ignores a relative entry.
    #[test]
    fn the_walk_stops_at_the_ceilings_git_stops_at() {
        let dir = shapes();
        let at = |path: &str| dir.path().join(path).into_os_string();
        let cases: Vec<(&str, Vec<OsString>, bool)> = vec![
            ("main/a/b", vec![at("main")], false),
            ("main/a/b", vec![at("main/a")], false),
            ("linked/s/t", vec![at("linked/s")], false),
            ("main", vec![at("main")], true),
            ("main/a", vec![at("")], true),
            ("main/a", vec![at("mainlink")], false),
            ("main/a", vec![OsString::new(), at("mainlink")], true),
            ("main/a/b", vec![OsString::from("main")], true),
        ];
        for (path, entries, found) in cases {
            let start = dir.path().join(path);
            let value = std::env::join_paths(&entries).unwrap();
            let git_finds = crate::subprocess::git_in(&start)
                .env(CEILINGS, &value)
                .args(["rev-parse", "--git-dir"])
                .output()
                .unwrap()
                .status
                .success();
            let met = walk(&start, &Reach::of(Some(value.clone()), false));
            assert_eq!(
                (met != Met::Nothing, git_finds),
                (found, found),
                "{path} under {value:?}: {met:?}"
            );
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
        // An empty start is the directory this process runs in, which git takes `-C ""` to mean.
        let empty = read_head(Path::new(""));
        assert_eq!(empty, read_head(Path::new(".")), "an empty start");
        if let Some(head) = empty {
            assert_eq!(head, git_head(Path::new("")), "an empty start");
        }
    }
}
