//! Periphery for spec 94, criterion 5 ("THE STATUSLINE COMMAND"): `rigger status --line`
//! prints exactly the core's `console::statusline` text - the SAME line `rigger status`
//! itself prints as its own first line - and nothing else, and `rigger setup` registers that
//! command as the editor's status line in `.claude/settings.json`.
//!
//! WHY THIS FILE. `cmd_status`/`cmd_setup` are PRIVATE free functions in the `rigger` BINARY
//! crate (`src/main.rs`), unreachable from an integration-test crate under `tests/` other than
//! by spawning the compiled binary (mirrors `tests/console_status_periphery.rs`'s identical
//! situation for the same command). `crates/rigger-driver/src/hooks.rs`'s own colocated `mod tests` already proves
//! the settings-merge logic (`install_status_line`) in isolation; this file proves the OTHER
//! half - that the real compiled `rigger status --line` and `rigger setup` wire it through a
//! real `argv` -> `main()` dispatch and a real on-disk store/settings file.

mod common;

use common::cli::read_run_events;
use common::cli::run_rigger;
use common::cli::seed_run_events;
use common::cli::temp_project;
use common::cli::temp_store_project;

/// `rigger status --line` prints EXACTLY ONE line, and it is `console::fold`'s own
/// `statusline` computed independently by this test from a real store round trip - not a
/// second, hand-composed rendering, and no other `rigger status` furniture (needs-you,
/// current blockers, the run-id header) leaks into it.
#[test]
fn status_line_prints_exactly_the_consoles_own_statusline_and_nothing_else() {
    let proj = temp_store_project();
    let root = proj.path();
    seed_run_events(
        root,
        &[
            ("UnitStarted", r#"{"id":"u-esc"}"#),
            ("UnitEscalated", r#"{"id":"u-esc"}"#),
        ],
    );

    let (out, err, ok) = run_rigger(root, &["status", "--line"]);
    assert!(ok, "rigger status --line must succeed: {err}");

    let events = read_run_events(root);
    let want = rigger::console::fold(&events, 3).expect("console::fold over the real read-back");

    assert_eq!(
        out.trim_end_matches('\n'),
        want.statusline,
        "rigger status --line must print exactly console::fold's own statusline; got:\n{out}"
    );
    assert_eq!(
        out.lines().count(),
        1,
        "rigger status --line must print exactly one line; got:\n{out}"
    );
}

/// The line `rigger status --line` prints is IDENTICAL to `rigger status`'s own first line
/// for the same recorded stream - "the console's bottom line for the same position" - proven
/// by running both against the same seeded store rather than trusting two independent
/// derivations to agree.
#[test]
fn status_line_matches_rigger_status_first_line_for_the_same_position() {
    let proj = temp_store_project();
    let root = proj.path();
    seed_run_events(
        root,
        &[
            ("UnitStarted", r#"{"id":"u-a"}"#),
            ("UnitStarted", r#"{"id":"u-b"}"#),
            ("UnitIntegrated", r#"{"id":"u-a"}"#),
        ],
    );

    let (full_out, full_err, full_ok) = run_rigger(root, &["status"]);
    assert!(full_ok, "rigger status must succeed: {full_err}");
    let first_line = full_out.lines().next().unwrap_or("");

    let (line_out, line_err, line_ok) = run_rigger(root, &["status", "--line"]);
    assert!(line_ok, "rigger status --line must succeed: {line_err}");

    assert_eq!(
        line_out.trim_end_matches('\n'),
        first_line,
        "rigger status --line must equal rigger status's own first line; full:\n{full_out}\nline:\n{line_out}"
    );
}

/// A clean run (no units at all) prints the console's `healthy` statusline alone - the same
/// bare answer `rigger status`'s own first line gives on an empty store.
#[test]
fn status_line_on_a_clean_run() {
    let proj = temp_store_project();
    let root = proj.path();

    let (out, err, ok) = run_rigger(root, &["status", "--line"]);
    assert!(
        ok,
        "rigger status --line must succeed on an empty store: {err}"
    );
    assert_eq!(out.trim_end_matches('\n'), "- . 0/0 units . healthy");
}

/// The literal state `rigger setup` itself leaves every project in, until the first `rigger
/// run`: no `.rigger/events.db` exists yet anywhere, only the settings `rigger setup` just
/// wrote. The editor polls `--line` unconditionally, with no human to read a hard error, so
/// it must render the SAME graceful text the console's own empty-store shell renders (spec 94
/// CONSTRAINTS WALK: "no run recorded; start one with `rigger run <spec>`"), exit 0 - never
/// the CLI's "no rigger store found" refusal every other store-opening courier still gives
/// (adjudication reject on the prior round: adv-u94c5-setup-wires-a-command-that-hard-fails-
/// with-no-run-yet).
#[test]
fn status_line_on_a_project_with_no_store_yet_renders_a_graceful_placeholder() {
    let dir = temp_project();
    let root = dir.path();

    let (_out, err, ok) = run_rigger(root, &["setup"]);
    assert!(ok, "rigger setup must succeed; stderr:\n{err}");
    assert!(
        !root.join(".rigger").join("events.db").exists(),
        "this test only means something in the literal post-setup, no-store-yet state"
    );

    let (out, err, ok) = run_rigger(root, &["status", "--line"]);
    assert!(
        ok,
        "rigger status --line must succeed even before the first `rigger run`; stderr:\n{err}"
    );
    assert_eq!(
        out.trim_end_matches('\n'),
        "no run recorded; start one with `rigger run <spec>`",
        "must mirror the console's own empty-store text (spec 94 CONSTRAINTS WALK); got:\n{out}"
    );
}

/// `rigger status --line --json` (or any other combination with `--json`) is rejected: the
/// two are different output modes for the same command and cannot both apply to one call.
#[test]
fn status_line_and_json_are_mutually_exclusive() {
    let proj = temp_store_project();
    let root = proj.path();

    let (_out, err, ok) = run_rigger(root, &["status", "--line", "--json"]);
    assert!(!ok, "status --line --json must be rejected");
    assert!(
        err.contains("--line") && err.contains("--json"),
        "the error should name both conflicting flags; got:\n{err}"
    );
}

/// `rigger setup` registers `rigger status --line` as the editor's status line command in
/// `.claude/settings.json`'s `statusLine` key, drift-aware like every other `rigger setup`
/// install step: a fresh install reports it, a rerun on an up-to-date repo is silent.
#[test]
fn setup_registers_the_statusline_command() {
    let dir = temp_project();
    let root = dir.path();

    let (out, err, ok) = run_rigger(root, &["setup"]);
    assert!(ok, "rigger setup must succeed; stderr:\n{err}");
    assert!(
        out.contains("registered") && out.contains("status line"),
        "setup must report registering the status line command; got:\n{out}"
    );

    let settings: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(root.join(".claude").join("settings.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(settings["statusLine"]["type"], "command");
    assert_eq!(settings["statusLine"]["command"], "rigger status --line");

    // The pre-existing SessionStart hook (installed by the same setup run) must survive
    // untouched - the settings.json merges must not clobber each other.
    assert_eq!(
        settings["hooks"]["SessionStart"][0]["hooks"][0]["command"],
        "rigger prime"
    );

    // A rerun on an up-to-date repo is a silent no-op for this artifact.
    let (out2, err2, ok2) = run_rigger(root, &["setup"]);
    assert!(ok2, "the rerun must succeed; stderr:\n{err2}");
    assert!(
        !out2.contains("status line"),
        "a rerun on an up-to-date repo must not re-report the status line registration; got:\n{out2}"
    );
}

/// `rigger setup`'s statusLine registration is drift-aware like every other install step
/// (mirrors `install_operator_mcp`'s identical Installed/Refreshed/AlreadyCurrent contract,
/// proven for that artifact by
/// `setup_reports_a_drifted_operator_mcp_server_as_refreshed_through_the_full_composition` in
/// `tests/cli.rs`): a `statusLine` command drifted from an older build (or a hand edit) is
/// self-healed AND reported as refreshed - distinct from a fresh install, and never silently
/// repaired - and no sibling artifact `setup` already installed re-reports merely because this
/// one drifted.
#[test]
fn setup_reports_a_drifted_statusline_command_as_refreshed() {
    let dir = temp_project();
    let root = dir.path();

    let (_out, err, ok) = run_rigger(root, &["setup"]);
    assert!(ok, "the first setup must succeed; stderr:\n{err}");

    // Hand-corrupt ONLY the statusLine entry (an older build's command, or a hand edit) -
    // every other artifact `setup` just installed stays exactly as it is.
    let settings_path = root.join(".claude").join("settings.json");
    let mut settings: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&settings_path).unwrap()).unwrap();
    settings["statusLine"] = serde_json::json!({
        "type": "command",
        "command": "/old/stale/statusline.sh",
    });
    std::fs::write(
        &settings_path,
        serde_json::to_vec_pretty(&settings).unwrap(),
    )
    .unwrap();

    let (out2, err2, ok2) = run_rigger(root, &["setup"]);
    assert!(ok2, "the repair rerun must succeed; stderr:\n{err2}");
    assert!(
        out2.contains("refreshed the drifted rigger status line command"),
        "a drifted statusLine entry must be reported as refreshed, not silently repaired; got:\n{out2}"
    );
    assert!(
        !out2.contains("registered the rigger status line command"),
        "a refresh must never be misreported as a fresh install; got:\n{out2}"
    );
    // Nothing ELSE drifted - the already-installed SessionStart hook must not re-report,
    // proving the combined gate isolates the one artifact that actually changed.
    assert!(
        !out2.contains("registered the rigger MCP server")
            && !out2.contains("installed the graph-first lookup hook"),
        "an untouched artifact must not re-report merely because a sibling drifted; got:\n{out2}"
    );

    let settings: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&settings_path).unwrap()).unwrap();
    assert_eq!(
        settings["statusLine"]["command"], "rigger status --line",
        "the drifted command must self-heal to the current build's own invocation"
    );
}
