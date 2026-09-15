//! Spec 90 criterion 1, THE AUDIT TEST: test git is hermetic.
//!
//! The operator's global git has `commit.gpgsign=true`, and the unit tests in
//! `src/worktree.rs` and `src/conductor.rs` (about 43 `git commit` sites) never disabled it
//! themselves, so every test commit ran gpg against the operator's real keyring - keyring-lock
//! contention and `git worktree add` races whenever several agents ran the suite at once. The
//! fix is centralized in ONE place, `.cargo/pidns-runner.sh` (already the target runner for
//! every test binary, via `.cargo/config.toml`): it exports a fixed author/committer identity
//! (`rigger-test <rigger-test@localhost>`), disables interactive credential/passphrase prompts
//! (`GIT_TERMINAL_PROMPT=0`), and - landed ahead of this spec - suppresses commit/tag signing
//! through git's environment-config channel (`GIT_CONFIG_COUNT`/`GIT_CONFIG_KEY_n`/
//! `GIT_CONFIG_VALUE_n`), all regardless of the operator's own global config. CI opts the
//! namespace-containment half of the runner out (`RIGGER_PIDNS: off`, a throwaway VM has no
//! unprivileged user namespaces) but must carry the IDENTICAL git-hermeticity env block at
//! workflow level so the two never drift apart silently.
//!
//! This file proves all four pieces of the Done-when checkbox:
//! 1. [`runner_exports_the_fixed_hermetic_git_identity_block`] - the runner's own exported set
//!    is exactly the fixed identity + no-signing + no-prompt block, and it is actually
//!    `export`ed (not merely assigned).
//! 2. [`ci_workflow_env_block_matches_the_runners_exported_git_identity_set`] - the workflow's
//!    top-level `env:` carries the SAME keys and values.
//! 3. [`a_hostile_global_config_would_break_an_unsandboxed_commit`] and
//!    [`a_test_commit_succeeds_with_the_fixed_identity_and_no_gpg_invocation_under_the_runners_override`]
//!    together prove: a real `git commit` against a deliberately hostile global config
//!    (signing on, a `gpg.program` pointing at a binary that does not exist) fails WITHOUT
//!    the runner's override and succeeds, with the fixed identity, WITH it - a
//!    control/treatment pair, not just a "no error" assertion, so a regression that silently
//!    drops the override (rather than merely reordering it) still fails this test.
//! 4. [`no_test_source_sets_the_signing_hermetic_variables_itself`] - walks every `.rs` file
//!    under `src/` and `tests/` (this file excluded - see its own doc comment) for the two
//!    literal tokens the runner's signing suppression owns (`commit.gpgsign`, `GIT_CONFIG_COUNT`)
//!    - the runner is their single authority.
//!
//! SCOPE, decided (see decision `u90c1-audit-scope-signing-only`): criterion 4's ban list is
//! exactly the two tokens Design's own audit-test sentence names for THIS mechanism (signing
//! suppression) - `commit.gpgsign` and `GIT_CONFIG_COUNT` - not `GIT_AUTHOR_*`/`GIT_COMMITTER_*`
//! (three existing sites already `.env()` their own identity on purpose, and Design says tests
//! that do this "keep working"), and not `GIT_CONFIG_GLOBAL` (`tests/cli.rs` already points it at
//! a hostile config for an unrelated `rigger setup` .gitignore test; `GIT_CONFIG_COUNT` provably
//! outranks whatever a `GIT_CONFIG_GLOBAL`-pointed file says, verified by this file's own
//! control/treatment pair, so that pre-existing use cannot defeat the runner's hermeticity).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

/// The ten git-hermeticity keys the runner owns end to end: exported (assigned + named on an
/// `export` line) in `.cargo/pidns-runner.sh` and mirrored verbatim in the CI workflow's
/// top-level `env:`. Order matches the runner's own `export` line.
const HERMETIC_GIT_KEYS: [&str; 10] = [
    "GIT_CONFIG_COUNT",
    "GIT_CONFIG_KEY_0",
    "GIT_CONFIG_VALUE_0",
    "GIT_CONFIG_KEY_1",
    "GIT_CONFIG_VALUE_1",
    "GIT_AUTHOR_NAME",
    "GIT_AUTHOR_EMAIL",
    "GIT_COMMITTER_NAME",
    "GIT_COMMITTER_EMAIL",
    "GIT_TERMINAL_PROMPT",
];

fn expected_hermetic_git_block() -> BTreeMap<&'static str, &'static str> {
    [
        ("GIT_CONFIG_COUNT", "2"),
        ("GIT_CONFIG_KEY_0", "commit.gpgsign"),
        ("GIT_CONFIG_VALUE_0", "false"),
        ("GIT_CONFIG_KEY_1", "tag.gpgsign"),
        ("GIT_CONFIG_VALUE_1", "false"),
        ("GIT_AUTHOR_NAME", "rigger-test"),
        ("GIT_AUTHOR_EMAIL", "rigger-test@localhost"),
        ("GIT_COMMITTER_NAME", "rigger-test"),
        ("GIT_COMMITTER_EMAIL", "rigger-test@localhost"),
        ("GIT_TERMINAL_PROMPT", "0"),
    ]
    .into_iter()
    .collect()
}

/// The committed test runner's raw text, resolved from the crate manifest dir (never the
/// process CWD, so this is stable regardless of where `cargo test` is invoked from).
fn runner_script_text() -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join(".cargo")
        .join("pidns-runner.sh");
    std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("cannot read test runner at {}: {e}", path.display()))
}

/// Parse every simple, unquoted, non-interpolated `KEY=value` shell assignment line in `script`
/// (a comment, a conditional, or an interpolated default like `TMPDIR="${X:-...}"` never matches
/// the plain uppercase-identifier key or the no-space, non-empty value this requires) into a
/// map keyed by name - "last assignment wins", matching shell semantics.
fn parse_shell_assignments(script: &str) -> BTreeMap<String, String> {
    let mut map = BTreeMap::new();
    for raw in script.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let key_is_plain_ident = !key.is_empty()
            && !key.starts_with(|c: char| c.is_ascii_digit())
            && key
                .chars()
                .all(|c| c.is_ascii_uppercase() || c == '_' || c.is_ascii_digit());
        let value = value.trim_matches('"');
        if key_is_plain_ident && !value.is_empty() && !value.contains(char::is_whitespace) {
            map.insert(key.to_string(), value.to_string());
        }
    }
    map
}

/// Every name listed on an `export ...` statement in `script` (a trailing `\` line-continuation
/// joined first, so a wrapped multi-line export list is seen whole).
fn exported_names(script: &str) -> Vec<String> {
    let joined = script.replace("\\\n", " ");
    let mut names = Vec::new();
    for line in joined.lines() {
        if let Some(rest) = line.trim().strip_prefix("export ") {
            names.extend(rest.split_whitespace().map(str::to_string));
        }
    }
    names
}

/// Criterion 1, first half: the runner assigns EXACTLY the fixed identity/no-signing/no-prompt
/// values (not merely "some" values - a future edit that renames `rigger-test@localhost` to
/// something else, or flips a `false` to `true`, fails here) and genuinely exports every one of
/// them (an assignment alone does not reach a child process the runner then `exec`s).
#[test]
fn runner_exports_the_fixed_hermetic_git_identity_block() {
    let script = runner_script_text();
    let assignments = parse_shell_assignments(&script);
    let exported = exported_names(&script);
    let expected = expected_hermetic_git_block();

    for key in HERMETIC_GIT_KEYS {
        let got = assignments.get(key).map(String::as_str);
        let want = expected.get(key).copied();
        assert_eq!(
            got, want,
            ".cargo/pidns-runner.sh must assign {key}={want:?}, found {got:?}"
        );
        assert!(
            exported.iter().any(|n| n == key),
            "{key} is assigned in .cargo/pidns-runner.sh but never appears on an `export` line - \
             an assignment alone never reaches the test binary this runner `exec`s"
        );
    }
}

/// Criterion 1, second half: CI (`RIGGER_PIDNS: off`, a throwaway VM) must carry the IDENTICAL
/// block at workflow level, so the two can never silently drift apart. Parses the committed
/// workflow rather than running it, like `tests/ci_lanes.rs`.
#[test]
fn ci_workflow_env_block_matches_the_runners_exported_git_identity_set() {
    let runner_script = runner_script_text();
    let runner_block = parse_shell_assignments(&runner_script);

    let workflow_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join(".github")
        .join("workflows")
        .join("rust.yml");
    let workflow_text = std::fs::read_to_string(&workflow_path).unwrap_or_else(|e| {
        panic!(
            "cannot read CI workflow at {}: {e}",
            workflow_path.display()
        )
    });
    let workflow: serde_yaml::Value = serde_yaml::from_str(&workflow_text).unwrap_or_else(|e| {
        panic!(
            "CI workflow at {} is not valid YAML: {e}",
            workflow_path.display()
        )
    });
    let workflow_env = workflow
        .get("env")
        .and_then(|e| e.as_mapping())
        .unwrap_or_else(|| panic!("CI workflow has no top-level `env:` mapping"));

    for key in HERMETIC_GIT_KEYS {
        let runner_value = runner_block.get(key).unwrap_or_else(|| {
            panic!(
                "{key} missing from .cargo/pidns-runner.sh's own parsed assignments - see \
                 runner_exports_the_fixed_hermetic_git_identity_block for that half of the proof"
            )
        });
        let workflow_value = workflow_env
            .get(serde_yaml::Value::String(key.to_string()))
            .and_then(|v| v.as_str().map(str::to_string).or_else(|| v.as_i64().map(|n| n.to_string())))
            .unwrap_or_else(|| {
                panic!(
                    "CI workflow's top-level env: is missing {key} (runner exports {key}={runner_value}) - \
                     the two must carry the identical git-hermeticity block"
                )
            });
        assert_eq!(
            &workflow_value, runner_value,
            "CI workflow's {key} ({workflow_value:?}) must match the runner's ({runner_value:?}) - \
             they must never drift apart"
        );
    }
}

/// A git repo at `dir`, with `global_config` as `GIT_CONFIG_GLOBAL` and every OTHER env var
/// inherited unchanged from this test binary's own process (crucially, NOT cleared) - proving
/// whatever this test asserts is a fact about the REAL environment `cargo test` actually runs
/// test commits under, not a hermetically-sealed fixture that begs its own question. `unset`
/// additionally strips the named vars from the child, for the control run.
fn commit_under(
    dir: &Path,
    global_config: &Path,
    unset: &[&str],
    message: &str,
) -> std::process::Output {
    let mut cmd = Command::new("git");
    cmd.current_dir(dir)
        .args(["commit", "--allow-empty", "-q", "-m", message])
        .env("GIT_CONFIG_GLOBAL", global_config);
    for var in unset {
        cmd.env_remove(var);
    }
    cmd.output().expect("git must be runnable")
}

/// A global config that signs both commits and tags AND names a `gpg.program` that does not
/// exist on this or any machine - so a commit that actually reaches gpg fails LOUDLY (`cannot
/// exec`), rather than silently succeeding because the host happens to have no working gpg
/// setup either way. Also sets a DIFFERENT author/committer identity, so a commit that reaches
/// this file for identity (rather than the runner's override) is caught too.
fn write_hostile_global_config(path: &Path) {
    std::fs::write(
        path,
        "[user]\n\
         \tname = hostile-user\n\
         \temail = hostile@example.com\n\
         [commit]\n\
         \tgpgsign = true\n\
         [tag]\n\
         \tgpgsign = true\n\
         [gpg]\n\
         \tprogram = /nonexistent/definitely-not-a-real-gpg-binary-90c1\n",
    )
    .expect("write hostile global config");
}

fn init_repo(dir: &Path) {
    assert!(Command::new("git")
        .current_dir(dir)
        .args(["init", "-q"])
        .status()
        .unwrap()
        .success());
}

/// The CONTROL: without the runner's `GIT_CONFIG_COUNT`-channel override (stripped for this one
/// child only - every other inherited var, including the fixed identity, is left alone), the
/// hostile global config's `gpgsign = true` really does reach `gpg.program`, and the commit
/// really does fail - proving the fixture is a genuine trap, not an inert file nobody reads. Runs
/// FIRST so criterion 3's real assertion has this as its baseline: a regression that silently
/// drops the runner's override entirely would make BOTH commits behave identically, which the
/// treatment test's own success assertion alone would not distinguish from "gpg was never a
/// threat here" without this control.
#[test]
fn a_hostile_global_config_would_break_an_unsandboxed_commit() {
    let repo = tempfile::tempdir().unwrap();
    init_repo(repo.path());
    let global = tempfile::NamedTempFile::new().unwrap();
    write_hostile_global_config(global.path());

    let out = commit_under(
        repo.path(),
        global.path(),
        &[
            "GIT_CONFIG_COUNT",
            "GIT_CONFIG_KEY_0",
            "GIT_CONFIG_VALUE_0",
            "GIT_CONFIG_KEY_1",
            "GIT_CONFIG_VALUE_1",
        ],
        "control",
    );
    assert!(
        !out.status.success(),
        "control: a commit under the hostile global config, with the runner's signing-suppression \
         vars stripped, must FAIL (git must attempt the nonexistent gpg.program) - if it succeeds, \
         this fixture proves nothing and the treatment test below is not a real regression guard. \
         stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("gpg") || stderr.contains("sign"),
        "control commit failed, but not visibly because of gpg/signing as expected; stderr:\n{stderr}"
    );
}

/// Criterion 1's central proof: under the SAME hostile global config, WITH the current process's
/// real (runner-provided) environment left untouched - this is what an actual `cargo test`-run
/// test commit sees - the commit succeeds, gpg is never invoked, and the committed identity is
/// the fixed `rigger-test <rigger-test@localhost>` rather than the hostile file's `hostile-user`.
#[test]
fn a_test_commit_succeeds_with_the_fixed_identity_and_no_gpg_invocation_under_the_runners_override()
{
    let repo = tempfile::tempdir().unwrap();
    init_repo(repo.path());
    let global = tempfile::NamedTempFile::new().unwrap();
    write_hostile_global_config(global.path());

    let out = commit_under(repo.path(), global.path(), &[], "treatment");
    assert!(
        out.status.success(),
        "a test commit under a hostile global config (signing on, gpg.program nonexistent) must \
         succeed under the runner's real environment; stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );

    let log = Command::new("git")
        .current_dir(repo.path())
        .args(["log", "-1", "--format=%an|%ae|%cn|%ce"])
        .env("GIT_CONFIG_GLOBAL", global.path())
        .output()
        .expect("git log must run");
    assert!(log.status.success());
    let ident = String::from_utf8_lossy(&log.stdout).trim().to_string();
    assert_eq!(
        ident, "rigger-test|rigger-test@localhost|rigger-test|rigger-test@localhost",
        "the committed author/committer must be the runner's fixed identity, not the hostile \
         global config's"
    );
}

/// Every `.rs` file strictly under `dir`, recursively, appended to `out`, deterministically
/// ordered.
fn collect_rs_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<_> = entries.flatten().map(|e| e.path()).collect();
    entries.sort();
    for path in entries {
        if path.is_dir() {
            collect_rs_files(&path, out);
        } else if path.extension().and_then(|e| e.to_str()) == Some("rs") {
            out.push(path);
        }
    }
}

/// Criterion 4: the two literal tokens the runner's OWN signing-suppression mechanism owns
/// (decision `u90c1-audit-scope-signing-only` explains why the list stops there) must never
/// appear in `src/` or `tests/` outside this file itself - which is excluded because its own
/// job is to name them (in this doc comment, in [`HERMETIC_GIT_KEYS`]'s sibling
/// [`expected_hermetic_git_block`], and in this very check) and it sets neither for a real git
/// invocation.
#[test]
fn no_test_source_sets_the_signing_hermetic_variables_itself() {
    const BANNED: [&str; 2] = ["commit.gpgsign", "GIT_CONFIG_COUNT"];
    let self_path = PathBuf::from(file!())
        .file_name()
        .unwrap()
        .to_string_lossy()
        .into_owned();

    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mut files = Vec::new();
    for top in ["src", "tests"] {
        collect_rs_files(&root.join(top), &mut files);
    }

    let mut findings = Vec::new();
    for path in &files {
        let rel = path
            .strip_prefix(&root)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/");
        if rel == format!("tests/{self_path}") {
            continue;
        }
        let Ok(content) = std::fs::read_to_string(path) else {
            continue;
        };
        for (i, line) in content.lines().enumerate() {
            for needle in BANNED {
                if line.contains(needle) {
                    findings.push(format!("{rel}:{}: contains {needle:?}", i + 1));
                }
            }
        }
    }
    assert!(
        findings.is_empty(),
        "the runner (.cargo/pidns-runner.sh) is the single authority for git commit signing \
         suppression; no other test source may set it:\n{}",
        findings.join("\n")
    );
}
