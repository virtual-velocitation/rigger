//! Git fixtures: throwaway repositories and the git commands a suite runs against them.

use std::os::unix::fs::PermissionsExt;
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
        assert!(Command::new("git")
            .arg("-C")
            .arg(path)
            .args(args)
            .status()
            .unwrap()
            .success());
    }
}

/// Run `git -C <dir> <args...>`, returning its output whatever its exit status.
pub fn run_git(dir: &Path, args: &[&str]) -> Output {
    Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .unwrap_or_else(|e| panic!("spawn git {args:?}: {e}"))
}

/// Run `git -C <dir> <args...>`, asserting it succeeds (naming its stderr when it does not),
/// and return its output.
pub fn git_ok(dir: &Path, args: &[&str]) -> Output {
    let out = run_git(dir, args);
    assert!(
        out.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    out
}

/// The trimmed stdout of a `git -C <dir> <args...>` that must succeed.
pub fn git_out(dir: &Path, args: &[&str]) -> String {
    String::from_utf8_lossy(&git_ok(dir, args).stdout)
        .trim()
        .to_string()
}

/// The trimmed stdout of `git -C <dir> <args...>`, whatever its exit status.
pub fn git_stdout(dir: &str, args: &[&str]) -> String {
    String::from_utf8_lossy(&run_git(Path::new(dir), args).stdout)
        .trim()
        .to_string()
}

/// Install a `pre-commit` hook in the repository at `repo_path` that refuses every commit.
pub fn install_refusing_hook(repo_path: &str) {
    let hooks = Path::new(repo_path).join(".git").join("hooks");
    std::fs::create_dir_all(&hooks).unwrap();
    let hook = hooks.join("pre-commit");
    std::fs::write(&hook, "#!/bin/sh\necho 'hook: refusing' >&2\nexit 1\n").unwrap();
    std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o755)).unwrap();
}

/// A go-gitsemver fixture repository at `root`: `go-gitsemver.yml` matching this repo's own
/// (`mode: Mainline`, `tag-prefix: v`), an initial commit tagged `v1.0.0`, then one more commit
/// with `second_commit_message` - enough history for a real version derivation.
pub fn tagged_gitsemver_repo(root: &Path, second_commit_message: &str) {
    git_ok(root, &["init", "-q"]);
    git_ok(root, &["config", "user.email", "t@example.com"]);
    git_ok(root, &["config", "user.name", "t"]);
    std::fs::write(
        root.join("go-gitsemver.yml"),
        "mode: Mainline\ntag-prefix: v\n",
    )
    .expect("write fixture go-gitsemver.yml");
    git_ok(root, &["add", "go-gitsemver.yml"]);
    git_ok(root, &["commit", "-q", "-m", "chore: initial"]);
    git_ok(root, &["tag", "v1.0.0"]);
    std::fs::write(root.join("file.txt"), "second\n").expect("write fixture file");
    git_ok(root, &["add", "file.txt"]);
    git_ok(root, &["commit", "-q", "-m", second_commit_message]);
}
