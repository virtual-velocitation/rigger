//! Periphery for `rigger gate <id>`: an agent's request that the run's driver gate its committed
//! worktree. These run OUTSIDE the crates, over the compiled binary and the on-disk store.
//!
//! OWNED HERE: the verb's CLI face with no driver serving the project (it runs the gate for
//! information only, prints the result, says it recorded nothing and records nothing, a stale
//! socket file included); its refusals of an uncommitted tree and of an undeclared gate; and
//! `rigger emit`'s refusal of a GateVerdict, the forge a worker would otherwise reach for.
//!
//! NOT OWNED HERE: the driver's answer to a request (the gate run, the signed verdict, its reuse
//! and the refusals the driver makes), which `rigger-conductor`'s own tests drive through the run's
//! serving entry point over an in-process channel, and the socket adapter, which
//! `rigger-process`'s own tests drive.

use crate::common;

use std::path::Path;

use common::cli::{
    emit_refused, identified_git_project, run_rigger_envs, seed_store, with_run_store,
    write_workflow,
};
use common::fixtures::git_ok;
use rigger::eventstore::{Direction, Event};
use rigger::run::STREAM;

/// The workflow block declaring the one gate `g`, under no build budget, so the verb's run takes
/// no slot from a machine-wide pool.
const GATE_G: &str =
    "build:\n  max_concurrent: 0\ngates:\n  g:\n    run: \"true\"\n    kind: core\n";

/// A committed project declaring gate `g`, its store seeded and its rigger state ignored, and the
/// scratch directory its gate runs under.
fn committed_project() -> (tempfile::TempDir, tempfile::TempDir) {
    let root = identified_git_project();
    seed_store(root.path());
    write_workflow(root.path(), GATE_G);
    std::fs::write(root.path().join(".gitignore"), ".rigger/\n").unwrap();
    std::fs::write(root.path().join("README"), "fixture\n").unwrap();
    git_ok(root.path(), &["add", "-A"]);
    git_ok(root.path(), &["commit", "-q", "-m", "fixture"]);
    (root, tempfile::tempdir().expect("a scratch dir"))
}

/// `rigger <args>` in `root` with its scratch under `scratch`: (stdout, stderr, success).
fn rigger_in(root: &Path, scratch: &Path, args: &[&str]) -> (String, String, bool) {
    run_rigger_envs(root, args, &[("RIGGER_TMPDIR", scratch.to_str().unwrap())])
}

/// Every GateVerdict in `root`'s store.
fn gate_verdicts(root: &Path) -> Vec<Event> {
    with_run_store(root, |store| {
        store
            .read_stream(STREAM, 0, Direction::Forward)
            .unwrap()
            .into_iter()
            .filter(|e| e.type_ == "GateVerdict")
            .collect()
    })
}

/// WITH NO DRIVER THE VERB INFORMS AND RECORDS NOTHING: GIVEN a committed project no driver
/// serves, with no socket file or a stale one, WHEN an agent runs `rigger gate g`, THEN the gate
/// runs, the verb prints its result and says it recorded nothing, and the store holds no verdict.
#[test]
fn with_no_driver_the_gate_verb_runs_the_gate_for_information_only_and_records_nothing() {
    for stale in [false, true] {
        let (root, scratch) = committed_project();
        if stale {
            // A listener dropped without unlinking leaves a socket file nothing answers.
            #[cfg(unix)]
            drop(
                std::os::unix::net::UnixListener::bind(root.path().join(".rigger/gate.sock"))
                    .unwrap(),
            );
        }
        let (out, err, ok) = rigger_in(root.path(), scratch.path(), &["gate", "g"]);
        assert!(
            ok,
            "stale {stale}: rigger gate g exits 0 on a green gate; stderr:\n{err}"
        );
        assert!(
            out.contains("PASS gate g"),
            "stale {stale}: the verb prints the result: {out}"
        );
        assert!(
            err.contains("no driver is serving this project") && err.contains("recorded nothing"),
            "stale {stale}: the verb says it recorded nothing: {err}"
        );
        assert!(
            gate_verdicts(root.path()).is_empty(),
            "stale {stale}: nothing was recorded"
        );
    }
}

#[test]
fn the_gate_verb_refuses_an_uncommitted_tree_and_an_undeclared_gate_recording_nothing() {
    struct Case {
        why: &'static str,
        dirty: bool,
        gate: &'static str,
        says: &'static str,
    }
    let cases = [
        Case {
            why: "an uncommitted file",
            dirty: true,
            gate: "g",
            says: "uncommitted changes",
        },
        Case {
            why: "an undeclared gate",
            dirty: false,
            gate: "nope",
            says: "the declared gates are: g",
        },
    ];
    for case in cases {
        let why = case.why;
        let (root, scratch) = committed_project();
        if case.dirty {
            std::fs::write(root.path().join("loose.rs"), "loose\n").unwrap();
        }
        let (_out, err, ok) = rigger_in(root.path(), scratch.path(), &["gate", case.gate]);
        assert!(!ok, "{why}: the verb exits non-zero");
        assert!(
            err.contains(case.says),
            "{why}: stderr says {:?}:\n{err}",
            case.says
        );
        assert!(
            gate_verdicts(root.path()).is_empty(),
            "{why}: nothing was recorded"
        );
    }
}

#[test]
fn rigger_emit_refuses_a_gate_verdict_so_no_worker_can_forge_one() {
    let (root, scratch) = committed_project();
    let body = r#"{"gate":"g","pass":true,"flaky":false,"evidence":"PASS"}"#;
    let (_out, err, ok) = rigger_in(root.path(), scratch.path(), &["emit", "GateVerdict", body]);
    assert!(!ok, "rigger emit GateVerdict exits non-zero");
    assert!(err.contains(&emit_refused("GateVerdict")), "stderr:\n{err}");
    assert!(
        gate_verdicts(root.path()).is_empty(),
        "nothing was recorded"
    );
}
