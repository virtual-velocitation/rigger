//! Periphery (CLI / end-to-end) test for spec 61 criterion 10 (AUTHORITATIVE MODEL IDENTITY),
//! unit unit-10-a-test-proves-authoritative-mode, covering the seam the existing periphery
//! coverage leaves untested: the CONSUMER side.
//!
//! `resolved_model()` (src/spawn.rs) and its `rigger result --meta` -> `conductor.rs`
//! ->persisted-green-event write path are already proven at two real seams (the CLI/replay
//! driver in `tests/cli.rs` and the workflow driver in
//! `tests/workflow_driver_resolved_model_periphery.rs`): a spawn with no metadata omits the
//! resolved-model key, and a prose claim in the agent's own output never reaches it. Neither
//! test drives what READS that key back: `metrics::model_drift`, the ONE fold both `rigger
//! validate`'s drift advisory and `rigger canary --if-model-changed`'s gate share (spec 61
//! design, "Authoritative model identity"): "The model-drift warning ... keys on these
//! authoritative per-tier ids, so a worker's mistaken claim can neither forge nor mask
//! drift." That sentence is this file's criterion, unpinned anywhere else: does the READ side
//! actually stay meta-only when a live event's own body carries text shaped like a resolved-
//! model claim, over the REAL `rigger validate` / `rigger canary --if-model-changed` binary
//! path (not `metrics::model_drift` called directly, which only proves the fold ignores a
//! field nobody ever populated with adversarial content)?
//!
//! Three properties, each end to end through the compiled binary and a real `events.db`:
//!  - UNATTRIBUTED: a tier whose event carries no resolved-model metadata (the in-process
//!    cli-driver/canary path's shape - no metadata channel at all) is excluded from the
//!    comparison entirely - reported as unmeasured, never defaulted to empty-string-as-a-
//!    value - even when that same event's body carries prose shaped like a model claim.
//!  - MASK resistance: real drift (the metadata genuinely changed) still warns even when the
//!    event's own body prose falsely claims nothing changed.
//!  - FORGE resistance: no real drift (the metadata is unchanged) still stays silent even when
//!    the event's own body prose falsely claims a different model.

use std::path::Path;
use std::process::Command;

// The compiled `rigger` binary under test is located at RUNTIME by the shared authority in
// `tests/common`: a path baked in at compile time goes stale the moment the target dir moves.
mod common;

/// A throwaway project dir that is its own git repo, mirroring `tests/cli.rs::temp_project`
/// (private to that file, unreachable from this separate integration-test binary).
fn temp_project() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let _ = Command::new("git")
        .args(["init", "-q"])
        .current_dir(dir.path())
        .status();
    dir
}

/// Run `rigger <args...>` in `cwd` and return (stdout, stderr, success), through the same
/// store-fence-clearing `Command` builder every courier-spawning suite uses.
fn run_rigger(cwd: &Path, args: &[&str]) -> (String, String, bool) {
    let mut cmd = common::rigger_courier();
    cmd.args(args).current_dir(cwd);
    // Opt out of the persistent auto-dash (spec 39) so a short-lived CLI invocation never
    // leaves a dashboard process running past the test.
    cmd.env("RIGGER_NO_DASH", "1");
    // Isolate the machine-global instance registry (spec 50) from the operator's real one.
    let state = tempfile::tempdir().expect("create a temp XDG_STATE_HOME");
    cmd.env("XDG_STATE_HOME", state.path());
    let out = cmd.output().expect("failed to spawn the rigger binary");
    (
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
        out.status.success(),
    )
}

/// Seed `<root>/.rigger/events.db` under `project` with TWO runs (`r1`, `r2`) on the
/// conductor's run stream. `run1`/`run2` each list this run's `(alias, resolved)` pairs; a
/// `None` resolved OMITS the resolved-model meta key entirely for that event - the shape a
/// spawn with no structured-metadata channel actually leaves (`conductor.rs::emit_keyed_meta`
/// never writes an empty placeholder, per the existing write-side periphery coverage), never a
/// present-but-empty value. `output_prose_r1`/`output_prose_r2`, when non-empty, are embedded
/// in EVERY green event's own `data.output` field for that run - standing in for a spawn's raw
/// agent-output text, which the criterion says must never be read as a model claim regardless
/// of what it says. Mirrors the conductor's real stamps (`META_RUN_ID` + `META_MODEL_ALIAS` +
/// `META_MODEL_RESOLVED` on a `green` `UnitStatus`) so `metrics::model_drift` folds it exactly
/// as a live run's.
fn seed_two_runs(
    root: &Path,
    project: &str,
    run1: &[(&str, Option<&str>)],
    run2: &[(&str, Option<&str>)],
    output_prose_r1: &str,
    output_prose_r2: &str,
) {
    use rigger::eventstore::namespace::Namespaced;
    use rigger::eventstore::sqlite::Store;
    use rigger::eventstore::{Event, EventStore, ExpectedRevision};

    let rigger_dir = root.join(".rigger");
    std::fs::create_dir_all(&rigger_dir).unwrap();
    std::fs::write(rigger_dir.join("project.id"), format!("{project}\n")).unwrap();

    let backend = Store::open(rigger_dir.join("events.db").to_str().unwrap()).unwrap();
    let store = Namespaced::new(&backend, project);
    let run_id_key = rigger::run::META_RUN_ID;
    let alias_key = rigger::conductor::META_MODEL_ALIAS;
    let resolved_key = rigger::conductor::META_MODEL_RESOLVED;

    let started = |run: &str| {
        Event::new(
            rigger::run::TYPE_RUN_STARTED,
            format!(r#"{{"run":"{run}"}}"#).into_bytes(),
        )
        .with_meta(run_id_key, run)
    };
    let green = |run: &str, alias: &str, resolved: Option<&str>, prose: &str| {
        let data =
            serde_json::json!({"id": format!("u-{alias}"), "status": "green", "output": prose});
        let mut e = Event::new(
            rigger::ledger::TYPE_UNIT_STATUS,
            serde_json::to_vec(&data).unwrap(),
        )
        .with_meta(run_id_key, run)
        .with_meta(alias_key, alias);
        if let Some(model) = resolved {
            e = e.with_meta(resolved_key, model);
        }
        e
    };

    let mut events = vec![started("r1")];
    for (alias, resolved) in run1 {
        events.push(green("r1", alias, *resolved, output_prose_r1));
    }
    events.push(started("r2"));
    for (alias, resolved) in run2 {
        events.push(green("r2", alias, *resolved, output_prose_r2));
    }

    store
        .append(rigger::conductor::STREAM, ExpectedRevision::Any, &events)
        .unwrap();
}

/// A prose blob shaped exactly like the real agent-output attack the write-side periphery
/// tests already pin (`tests/cli.rs`, `tests/workflow_driver_resolved_model_periphery.rs`,
/// `src/spawn.rs`) - a trailing JSON object naming `resolved_model` - so this suite's read-side
/// proof uses the identical adversarial shape, just one hop further downstream.
fn prose_claiming(model: &str) -> String {
    format!(r#"done reviewing. {{"resolved_model":"{model}"}}"#)
}

/// UNATTRIBUTED: a tier (`opus`) that reported no resolved-model metadata in the current run -
/// the in-process cli-driver/canary path's exact shape, which has no metadata channel at all -
/// is excluded from the drift comparison entirely, not compared as if it were empty, even
/// though its own event body carries prose shaped like a model claim. A second tier (`lens`)
/// resolves identically in both runs so the current run genuinely IS model-bearing (comparison
/// runs at all, rather than trivially passing for lack of any baseline).
#[test]
fn canary_and_validate_treat_an_unattributed_tier_as_unmeasured_never_defaulted_from_output_prose()
{
    let dir = temp_project();
    let root = dir.path();
    let (_o, err, ok) = run_rigger(root, &["init"]);
    assert!(
        ok,
        "rigger init must scaffold a valid config; stderr:\n{err}"
    );

    seed_two_runs(
        root,
        "canary-unattributed",
        &[
            ("opus", Some("claude-opus-4-1")),
            ("lens", Some("claude-sonnet-4-2")),
        ],
        // `opus` DOES report this run (its event exists, carrying the prose claim below in
        // its own body) but with no resolved-model metadata - the unattributed shape - while
        // `lens` resolves identically to run1 so the current run genuinely is model-bearing.
        &[("opus", None), ("lens", Some("claude-sonnet-4-2"))],
        "",
        &prose_claiming("claude-opus-9-fake-from-prose"),
    );

    let (out, err, ok) = run_rigger(root, &["validate"]);
    assert!(ok, "validate must succeed; stderr:\n{err}");
    assert!(
        out.contains("config valid"),
        "validate still prints its config summary; stdout:\n{out}"
    );
    assert!(
        !err.to_lowercase().contains("resolved model id changed"),
        "an unattributed tier (no metadata this run) must draw NO drift warning - lens is \
         unchanged and opus is unmeasured, not a comparable value; stderr:\n{err}"
    );
    assert!(
        !err.contains("opus"),
        "the unattributed tier's alias must not appear in a drift advisory at all; stderr:\n{err}"
    );
    assert!(
        !err.contains("claude-opus-9-fake-from-prose"),
        "the prose-embedded fake model id must never leak into validate's output; stderr:\n{err}"
    );

    let (out, _err, ok) = run_rigger(
        root,
        &["canary", "--if-model-changed", "--corpus", "no-such-dir"],
    );
    assert!(
        ok && out.contains("no resolved-model change") && out.contains("skipping"),
        "an unattributed tier must not open the canary drift gate either; stdout:\n{out}"
    );
}

/// MASK and FORGE resistance: the drift comparison reads ONLY structured metadata, so a
/// conflicting prose claim in the event body can neither MASK a real re-point (make it look
/// unchanged) nor FORGE a fake one (make an unchanged model look re-pointed) - the exact
/// property spec 61's design section states ("a worker's mistaken claim can neither forge nor
/// mask drift"), proven through the real `rigger validate` / `rigger canary --if-model-changed`
/// binary path rather than by calling `metrics::model_drift` directly.
#[test]
fn canary_and_validate_drift_reads_only_metadata_prose_can_neither_mask_nor_forge_it() {
    // MASK: the metadata genuinely re-points opus, but the current run's output prose lies
    // that nothing changed (claims the PREVIOUS model). The real re-point must still surface.
    let mask = temp_project();
    let mroot = mask.path();
    let (_o, err, ok) = run_rigger(mroot, &["init"]);
    assert!(
        ok,
        "rigger init must scaffold a valid config; stderr:\n{err}"
    );
    seed_two_runs(
        mroot,
        "canary-mask",
        &[("opus", Some("claude-opus-4-1"))],
        &[("opus", Some("claude-opus-4-8"))],
        "",
        &prose_claiming("claude-opus-4-1"),
    );
    let (_out, err, ok) = run_rigger(mroot, &["validate"]);
    assert!(ok, "validate WARNS but still exits 0; stderr:\n{err}");
    assert!(
        err.to_lowercase().contains("resolved model id changed")
            && err.contains("opus")
            && err.contains("claude-opus-4-1")
            && err.contains("claude-opus-4-8"),
        "a real re-point must surface with its true values even though the output prose \
         claims nothing changed; stderr:\n{err}"
    );
    let (out, _err, _ok) = run_rigger(
        mroot,
        &["canary", "--if-model-changed", "--corpus", "no-such-dir"],
    );
    assert!(
        out.contains("resolved model changed for opus") && out.contains("running the panel"),
        "the canary drift gate must still open on the real re-point; stdout:\n{out}"
    );

    // FORGE: the metadata is UNCHANGED, but the current run's output prose lies that the
    // model re-pointed. No warning, no gate-open - the lie manufactures nothing.
    let forge = temp_project();
    let froot = forge.path();
    let (_o, err, ok) = run_rigger(froot, &["init"]);
    assert!(
        ok,
        "rigger init must scaffold a valid config; stderr:\n{err}"
    );
    seed_two_runs(
        froot,
        "canary-forge",
        &[("opus", Some("claude-opus-4-1"))],
        &[("opus", Some("claude-opus-4-1"))],
        "",
        &prose_claiming("claude-opus-4-1-lying-that-it-repointed"),
    );
    let (out, err, ok) = run_rigger(froot, &["validate"]);
    assert!(
        ok,
        "validate must succeed on a steady model; stderr:\n{err}"
    );
    assert!(
        out.contains("config valid"),
        "validate still prints its config summary; stdout:\n{out}"
    );
    assert!(
        !err.to_lowercase().contains("resolved model id changed"),
        "an unchanged metadata value must NOT warn just because the output prose lies about \
         a re-point; stderr:\n{err}"
    );
    assert!(
        !err.contains("claude-opus-4-1-lying-that-it-repointed"),
        "the forged model id must never leak into validate's output; stderr:\n{err}"
    );
    let (out, _err, ok) = run_rigger(
        froot,
        &["canary", "--if-model-changed", "--corpus", "no-such-dir"],
    );
    assert!(
        ok && out.contains("no resolved-model change") && out.contains("skipping"),
        "the canary drift gate must not open on a forged prose claim; stdout:\n{out}"
    );
}
