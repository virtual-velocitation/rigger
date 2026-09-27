//! Git fixtures: throwaway repositories and the git commands a suite runs against them.

use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::{Command, Output};

/// The fixed committer identity a fixture commit carries on a machine with no global git
/// identity (a CI runner), passed through the environment so no repository config is written.
const FIXTURE_IDENTITY: [(&str, &str); 4] = [
    ("GIT_AUTHOR_NAME", "t"),
    ("GIT_AUTHOR_EMAIL", "t@e"),
    ("GIT_COMMITTER_NAME", "t"),
    ("GIT_COMMITTER_EMAIL", "t@e"),
];

/// `git init` a repository at `path` with a committer identity and one empty commit, so a base
/// ref like `HEAD` resolves.
pub fn init_repo(path: impl AsRef<Path>) {
    for args in [
        &["init", "-q"][..],
        &["config", "user.email", "t@example.com"],
        &["config", "user.name", "t"],
        &["commit", "--allow-empty", "-q", "-m", "init"],
    ] {
        assert!(Command::new("git")
            .arg("-C")
            .arg(path.as_ref())
            .args(args)
            .status()
            .unwrap()
            .success());
    }
}

/// A throwaway directory holding an [`init_repo`] repository - its own git repo with one empty
/// commit, so a base ref like `HEAD` resolves and a real unit worktree can branch from it.
pub fn temp_git_project_with_commit() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("create a throwaway git project");
    init_repo(dir.path());
    dir
}

/// A `git -C <dir> <args...>` command, not yet run.
fn git_command(dir: &Path, args: &[&str]) -> Command {
    let mut cmd = Command::new("git");
    cmd.arg("-C").arg(dir).args(args);
    cmd
}

/// The output of `command` (a `git <args...>`), panicking only when git cannot be spawned.
fn output_of(mut command: Command, args: &[&str]) -> Output {
    command
        .output()
        .unwrap_or_else(|e| panic!("spawn git {args:?}: {e}"))
}

/// `out` (the output of `git <args...>`), asserting it succeeded - naming its stderr when not.
fn succeeded(out: Output, args: &[&str]) -> Output {
    assert!(
        out.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    out
}

/// `out`'s stdout, lossily decoded and trimmed.
fn trimmed_stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

/// Run `git -C <dir> <args...>`, returning its output whatever its exit status.
pub fn run_git(dir: impl AsRef<Path>, args: &[&str]) -> Output {
    output_of(git_command(dir.as_ref(), args), args)
}

/// Run `git -C <dir> <args...>`, asserting it succeeds (naming its stderr when it does not),
/// and return its output.
pub fn git_ok(dir: impl AsRef<Path>, args: &[&str]) -> Output {
    succeeded(run_git(dir, args), args)
}

/// [`git_ok`] under the fixed [`FIXTURE_IDENTITY`], for a command that creates a commit
/// (`commit`, `commit-tree`, ...) on a machine that may have no identity configured.
pub fn git_ok_with_identity(dir: impl AsRef<Path>, args: &[&str]) -> Output {
    let mut command = git_command(dir.as_ref(), args);
    command.envs(FIXTURE_IDENTITY);
    succeeded(output_of(command, args), args)
}

/// The trimmed stdout of a `git -C <dir> <args...>` that must succeed.
pub fn git_out(dir: impl AsRef<Path>, args: &[&str]) -> String {
    trimmed_stdout(&git_ok(dir, args))
}

/// The trimmed stdout of a [`git_ok_with_identity`] run.
pub fn git_out_with_identity(dir: impl AsRef<Path>, args: &[&str]) -> String {
    trimmed_stdout(&git_ok_with_identity(dir, args))
}

/// The trimmed stdout of `git -C <dir> <args...>`, whatever its exit status.
pub fn git_stdout(dir: impl AsRef<Path>, args: &[&str]) -> String {
    trimmed_stdout(&run_git(dir, args))
}

/// The trimmed stdout of `git -C <dir> <args...>` when it succeeds with a non-empty answer, or
/// `None` - for a read-only query whose absence is itself the answer (an unborn branch, a ref
/// that does not exist).
pub fn git_answer(dir: impl AsRef<Path>, args: &[&str]) -> Option<String> {
    let out = run_git(dir, args);
    out.status
        .success()
        .then(|| trimmed_stdout(&out))
        .filter(|s| !s.is_empty())
}

/// Stage everything in the repository at `dir` and commit it as `msg`, asserting both succeed.
pub fn git_commit_all(dir: impl AsRef<Path>, msg: &str) {
    for args in [&["add", "-A"][..], &["commit", "-q", "-m", msg]] {
        git_ok(dir.as_ref(), args);
    }
}

/// Install a `pre-commit` hook in the repository at `repo_path` that refuses every commit.
pub fn install_refusing_hook(repo_path: impl AsRef<Path>) {
    let hooks = repo_path.as_ref().join(".git").join("hooks");
    std::fs::create_dir_all(&hooks).unwrap();
    let hook = hooks.join("pre-commit");
    std::fs::write(&hook, "#!/bin/sh\necho 'hook: refusing' >&2\nexit 1\n").unwrap();
    std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o755)).unwrap();
}

/// `git -C <dir> commit -q -m <message>` stamped with a fixed, deliberately old author and
/// committer date instead of the wall-clock "now" - so a later cherry-pick of the commit (which
/// stamps its own "now") can never reproduce a byte-identical commit object - asserting it
/// succeeds.
pub fn commit_at_fixed_date(dir: impl AsRef<Path>, message: &str) -> Output {
    let args = ["commit", "-q", "-m", message];
    let mut command = git_command(dir.as_ref(), &args);
    command
        .env("GIT_AUTHOR_DATE", "2000-01-01T00:00:00")
        .env("GIT_COMMITTER_DATE", "2000-01-01T00:00:00");
    succeeded(output_of(command, &args), &args)
}

/// `git init -q` at `root` - a bare repository with no commit, for a test that only needs `root`
/// to sit inside a git work tree.
pub fn git_init_quiet(root: &Path) {
    run_git(root, &["init", "-q"]);
}
