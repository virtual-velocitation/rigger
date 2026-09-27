//! Git fixtures: throwaway repositories and the git commands a test runs against them - shared by
//! the integration suites (through `tests/common/git.rs`) and the crate's own unit tests.

use std::path::Path;
use std::process::{Command, Output};

/// `git init` a repository at `path` with a committer identity and one empty commit, so a base
/// ref like `HEAD` resolves.
pub fn init_repo(path: &Path) {
    for args in [
        &["init", "-q"][..],
        &["config", "user.email", "t@example.com"],
        &["config", "user.name", "t"],
        &["commit", "--allow-empty", "-q", "-m", "init"],
    ] {
        assert!(run_git(path, args).status.success());
    }
}

/// Run `git -C <dir> <args...>`, returning its output whatever its exit status.
pub fn run_git(dir: impl AsRef<Path>, args: &[&str]) -> Output {
    Command::new("git")
        .arg("-C")
        .arg(dir.as_ref())
        .args(args)
        .output()
        .unwrap_or_else(|e| panic!("spawn git {args:?}: {e}"))
}

/// Run `git -C <dir> <args...>`, asserting it succeeds (naming its stderr when it does not),
/// and return its output.
pub fn git_ok(dir: impl AsRef<Path>, args: &[&str]) -> Output {
    let out = run_git(dir, args);
    assert!(
        out.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    out
}

/// The trimmed stdout of a `git -C <dir> <args...>` that must succeed.
pub fn git_out(dir: impl AsRef<Path>, args: &[&str]) -> String {
    String::from_utf8_lossy(&git_ok(dir, args).stdout)
        .trim()
        .to_string()
}

/// The trimmed stdout of `git -C <dir> <args...>`, whatever its exit status.
pub fn git_stdout(dir: impl AsRef<Path>, args: &[&str]) -> String {
    String::from_utf8_lossy(&run_git(dir, args).stdout)
        .trim()
        .to_string()
}

/// `git -C <dir> commit -q -m <message>` stamped with a fixed, deliberately old author and
/// committer date instead of the wall-clock "now" - so a later cherry-pick of the commit (which
/// stamps its own "now") can never reproduce a byte-identical commit object - asserting it
/// succeeds.
pub fn commit_at_fixed_date(dir: impl AsRef<Path>, message: &str) -> Output {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir.as_ref())
        .args(["commit", "-q", "-m", message])
        .env("GIT_AUTHOR_DATE", "2000-01-01T00:00:00")
        .env("GIT_COMMITTER_DATE", "2000-01-01T00:00:00")
        .output()
        .unwrap_or_else(|e| panic!("spawn git commit: {e}"));
    assert!(out.status.success(), "fixed-date commit failed");
    out
}

/// `git init -q` at `root` - a bare repository with no commit, for a test that only needs `root`
/// to sit inside a git work tree.
pub fn git_init_quiet(root: &Path) {
    run_git(root, &["init", "-q"]);
}
