//! Periphery for the headless host's spawn configuration: a headless Claude Code session reads
//! its hooks, MCP servers and subagents from its working directory, and a fresh unit worktree
//! carries none of them (`.claude/settings.json` is machine-local and a consumer's `.gitignore`
//! ignores `.claude`). So the host hands every one of them to the session on its command line,
//! built from rigger's own single sources, and writes nothing into the worktree.
//!
//! Each launch below runs the checked-in `claude-code-argv-capture-agent.sh` in place of the
//! real `claude`, which records the exact argv it was started with to a file this test owns
//! (outside the worktree) - so every assertion reads what really reached the child process.
//! The expected values are rebuilt here from the same `hooks` installers `rigger setup` uses and
//! from the committed `.claude/agents/*.md`, never a hand copy.
//!
//! Also pinned here, because both are what an operator's own session sees of the same concern:
//! `rigger prime` (the SessionStart hook's command) exits quietly when its reader closes the
//! pipe, and `rigger validate` advises when the checkout carries none of these hooks.

mod common;
use common::cli::{run_rigger, run_rigger_envs, temp_project};
use common::fixtures::{git_commit_all, git_out, implementer_opts, temp_git_project_with_commit};
use common::repo::repo_root;

use std::path::Path;
use std::process::Stdio;

use rigger::config::{split_frontmatter, AgentDef};
use rigger::driver::claude_code::Driver;
use rigger::eventstore::sqlite::Store;
use rigger::hooks;
use serde_json::Value;

/// The spawn id every launch here uses; the spawn-bound MCP server names it.
const SPAWN_ID: &str = "u-config/implementer#0";
/// The `rigger` binary the host names for its spawn-bound MCP server in these launches.
const RIGGER_BIN: &str = "/fixture/bin/rigger";

/// Launch one headless spawn in `dir` through the argv-capture fixture, with `settings_json` as
/// the spawn's own settings, and return the argv the child process actually received.
fn launch_capturing(dir: &Path, settings_json: &str) -> Vec<String> {
    let capture_dir = tempfile::tempdir().expect("a throwaway dir for the argv capture");
    let capture = capture_dir.path().join("argv");
    let progress = Store::open(":memory:").expect("in-memory progress store");
    let run = Store::open(":memory:").expect("in-memory run store");
    let driver = Driver {
        bin: repo_root()
            .join("tests/fixtures/claude-code-argv-capture-agent.sh")
            .to_string_lossy()
            .into_owned(),
        rigger_bin: RIGGER_BIN.to_string(),
        progress_store: &progress,
        run_store: &run,
        scratch_root: String::new(),
        stop_grace: std::time::Duration::from_secs(30),
    };
    let mut opts = implementer_opts(SPAWN_ID);
    opts.dir = dir.to_string_lossy().into_owned();
    opts.settings_json = settings_json.to_string();
    opts.env = vec![(
        "RIGGER_ARGV_CAPTURE".to_string(),
        capture.to_string_lossy().into_owned(),
    )];
    let mut launch = driver
        .launch(&AgentDef::default(), "the task", &opts, &progress)
        .expect("the headless spawn launches");
    let status = launch
        .child
        .wait()
        .expect("the fixture agent exits on its own");
    assert!(
        status.success(),
        "the argv-capture fixture failed: {status}"
    );
    let raw = std::fs::read(&capture).expect("the fixture recorded its argv");
    raw.split(|b| *b == 0)
        .filter(|field| !field.is_empty())
        .map(|field| String::from_utf8(field.to_vec()).expect("a utf-8 argument"))
        .collect()
}

/// The JSON value that follows `flag` in `argv`, failing loudly (naming the argv) when the flag
/// is absent or its value is not JSON.
fn flag_json(argv: &[String], flag: &str) -> Value {
    let at = argv
        .iter()
        .position(|a| a == flag)
        .unwrap_or_else(|| panic!("the spawn argv carries no {flag}: {argv:?}"));
    let value = argv
        .get(at + 1)
        .unwrap_or_else(|| panic!("{flag} has no value: {argv:?}"));
    serde_json::from_str(value).unwrap_or_else(|e| panic!("{flag} is not JSON ({e}): {value}"))
}

/// The session settings `rigger setup` installs into an empty `.claude/settings.json`, merged
/// onto `base` by the same `hooks` installers: the SessionStart prime hook, the PreToolUse
/// grep-guard and the status line.
fn installed_session_settings(base: &[u8]) -> Value {
    let primed = hooks::install_session_start(base, "rigger prime").unwrap();
    let guarded =
        hooks::install_pretooluse_hook(&primed, "Grep|Bash", "rigger grep-guard").unwrap();
    let with_line = hooks::install_status_line(&guarded, "rigger status --line").unwrap();
    serde_json::from_slice(&with_line).unwrap()
}

/// `argv` carries the session settings, the spawn-bound MCP server alone, and both helper agents.
fn assert_spawn_is_configured(argv: &[String]) {
    assert_settings_are_the_installed_ones(argv);
    assert_mcp_config_is_the_installed_server(argv);
    assert_helper_agents_reach_the_spawn(argv);
}

/// `argv`'s `--settings` is exactly what the hooks installers build: the SessionStart prime
/// hook, the PreToolUse grep-guard and the status line.
fn assert_settings_are_the_installed_ones(argv: &[String]) {
    assert_eq!(
        flag_json(argv, "--settings"),
        installed_session_settings(b""),
        "the spawn's settings are exactly the hooks and status line `rigger setup` installs"
    );
}

/// `argv`'s `--mcp-config` is the one `rigger` server the MCP installer builds for this spawn,
/// and `--strict-mcp-config` keeps any other configuration out.
fn assert_mcp_config_is_the_installed_server(argv: &[String]) {
    let installed =
        hooks::install_mcp_server(b"", "rigger", RIGGER_BIN, &["mcp", "--spawn", SPAWN_ID])
            .unwrap();
    assert_eq!(
        flag_json(argv, "--mcp-config"),
        serde_json::from_slice::<Value>(&installed).unwrap(),
        "the spawn's MCP config is the installer-built spawn-bound server"
    );
    assert!(
        argv.iter().any(|a| a == "--strict-mcp-config"),
        "only the host's MCP config applies: {argv:?}"
    );
}

/// `argv`'s `--agents` defines `lookup` and `verify` carrying every byte of the committed
/// `.claude/agents/{name}.md`: each frontmatter field (the name as the key, `tools` as the list
/// its comma-separated value spells) and the body as the prompt, verbatim, with nothing else.
fn assert_helper_agents_reach_the_spawn(argv: &[String]) {
    let agents = flag_json(argv, "--agents");
    for name in ["lookup", "verify"] {
        let text = std::fs::read_to_string(repo_root().join(format!(".claude/agents/{name}.md")))
            .expect("the committed helper agent");
        let (front, body) = split_frontmatter(&text).expect("the helper has frontmatter");
        let def = agents
            .get(name)
            .unwrap_or_else(|| panic!("--agents defines no {name}: {agents}"));
        assert_eq!(
            def["prompt"].as_str(),
            Some(body),
            "{name}'s prompt is its committed body, byte for byte"
        );
        let mut fields = 1; // the prompt
        for line in front.lines() {
            let (key, value) = line
                .split_once(": ")
                .unwrap_or_else(|| panic!("{name} frontmatter line {line:?}"));
            if key == "name" {
                assert_eq!(value, name, "the helper is keyed by its own name");
                continue;
            }
            fields += 1;
            let carried = match key {
                "tools" => def[key]
                    .as_array()
                    .unwrap_or_else(|| panic!("{name}.tools is a list: {def}"))
                    .iter()
                    .map(|t| t.as_str().expect("a tool name"))
                    .collect::<Vec<_>>()
                    .join(", "),
                _ => def[key]
                    .as_str()
                    .unwrap_or_else(|| panic!("{name}.{key} is carried: {def}"))
                    .to_string(),
            };
            assert_eq!(carried, value, "{name}.{key} is the committed value");
        }
        assert_eq!(
            def.as_object().map(|o| o.len()),
            Some(fields),
            "{name} carries its committed fields and nothing else: {def}"
        );
    }
}

/// A committed git repository standing in for a fresh unit worktree, holding one tracked file.
fn committed_worktree() -> tempfile::TempDir {
    let repo = temp_git_project_with_commit();
    std::fs::write(repo.path().join("README.md"), "a unit worktree\n").unwrap();
    git_commit_all(repo.path(), "seed");
    repo
}

/// Every path `git status` reports in `dir`, untracked files included.
fn porcelain(dir: &Path) -> String {
    git_out(
        dir,
        &[
            "status",
            "--porcelain",
            "--untracked-files=all",
            "--ignored",
        ],
    )
}

/// Launch one spawn in a fresh committed worktree and hand the argv it received to `check`.
fn spawn_in_a_fresh_worktree(check: fn(&[String])) {
    let dir = committed_worktree();
    check(&launch_capturing(dir.path(), ""));
}

rigger::test_cases! {
    a_headless_spawn_carries_the_hooks_and_status_line_rigger_setup_installs:
        spawn_in_a_fresh_worktree(assert_settings_are_the_installed_ones);
    a_headless_spawn_carries_the_installer_built_mcp_server:
        spawn_in_a_fresh_worktree(assert_mcp_config_is_the_installed_server);
    a_headless_spawn_carries_both_helper_agents_as_committed:
        spawn_in_a_fresh_worktree(assert_helper_agents_reach_the_spawn);
}

#[test]
fn a_headless_spawn_merges_the_hooks_onto_its_own_settings() {
    let dir = committed_worktree();
    let own = r#"{"model":"opus","permissions":{"allow":["Bash(cargo:*)"]}}"#;
    let argv = launch_capturing(dir.path(), own);
    assert_eq!(
        flag_json(&argv, "--settings"),
        installed_session_settings(own.as_bytes()),
        "the spawn's own settings survive and the hooks are merged onto them"
    );
}

#[test]
fn a_headless_spawn_writes_nothing_into_its_worktree() {
    let dir = committed_worktree();
    let before = porcelain(dir.path());
    let argv = launch_capturing(dir.path(), "");
    assert_spawn_is_configured(&argv);
    assert_eq!(
        porcelain(dir.path()),
        before,
        "configuring the spawn left no file, tracked, untracked or ignored, in its worktree"
    );
    assert!(
        !dir.path().join(".claude").exists(),
        "no .claude directory was materialized in the worktree"
    );
}

#[test]
fn a_worktree_that_ignores_dot_claude_still_gets_the_full_configuration() {
    let dir = committed_worktree();
    std::fs::write(dir.path().join(".gitignore"), ".claude/\n").unwrap();
    git_commit_all(dir.path(), "ignore the machine-local installs");
    let argv = launch_capturing(dir.path(), "");
    assert_spawn_is_configured(&argv);
}

#[test]
fn rigger_prime_exits_quietly_when_its_reader_closes_the_pipe() {
    // `rigger prime | head -1`: the reader goes away before prime finishes writing. Closing the
    // read end before the child writes anything makes the first write hit the closed pipe, every
    // time, rather than only when the output outgrows the pipe buffer.
    let proj = temp_project();
    let state = tempfile::tempdir().expect("a throwaway XDG_STATE_HOME");
    let mut child = common::rigger_courier()
        .arg("prime")
        .current_dir(proj.path())
        .env("RIGGER_NO_DASH", "1")
        .env("XDG_STATE_HOME", state.path())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn rigger prime");
    drop(child.stdout.take());
    let out = child.wait_with_output().expect("rigger prime exits");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        out.status.success(),
        "rigger prime must exit 0 when its reader closes the pipe; {}; stderr:\n{stderr}",
        out.status
    );
    assert!(
        !stderr.contains("panicked"),
        "a closed stdout is not a panic; stderr:\n{stderr}"
    );
}

/// The words `rigger validate`'s missing-hooks advisory opens with.
const HOOKS_ADVISORY: &str = "does not carry rigger's session hooks";

#[test]
fn validate_advises_when_the_checkout_has_no_hooks_installed() {
    let proj = temp_project();
    let root = proj.path();
    let (_out, err, ok) = run_rigger_envs(root, &["setup"], &[("RIGGER_NPM", "true")]);
    assert!(ok, "rigger setup must succeed; stderr:\n{err}");

    let (_out, err, ok) = run_rigger(root, &["validate"]);
    assert!(ok, "validate succeeds on a set-up project; stderr:\n{err}");
    assert!(
        !err.contains(HOOKS_ADVISORY),
        "with the hooks installed validate is silent about them; stderr:\n{err}"
    );

    std::fs::remove_file(root.join(".claude/settings.json")).unwrap();
    let (_out, err, ok) = run_rigger(root, &["validate"]);
    assert!(
        ok,
        "the advisory never changes the exit status; stderr:\n{err}"
    );
    assert!(
        err.contains(HOOKS_ADVISORY) && err.contains("rigger setup"),
        "without the hooks validate advises and names the fix; stderr:\n{err}"
    );
}
