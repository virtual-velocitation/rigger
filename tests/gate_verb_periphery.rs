//! Periphery for `rigger gate <id>`: an agent's own run of a declared gate, recorded where the
//! driver reuses it. These run OUTSIDE the crates, over the compiled binary and the on-disk store.
//!
//! OWNED HERE: the verb's CLI face over a committed project (it runs the gate, prints the result
//! and leaves one GateVerdict carrying the verb's provenance and the input digest of the commit it
//! gated); its refusals of an uncommitted tree and of an undeclared gate; and `rigger emit`'s
//! refusal of a GateVerdict, the forge a worker would otherwise reach for.
//!
//! NOT OWNED HERE: the driver's reuse of the verb's verdict, the verb's resolution of the gate's
//! inputs and the tree-changing gate (`rigger-conductor`'s own tests, which drive both the verb's
//! core and the driver over one store).

use crate::common;

use std::path::Path;

use common::cli::{
    emit_refused, identified_git_project, run_rigger_envs, seed_store, with_run_store,
    write_workflow,
};
use common::fixtures::{git_ok, git_out};
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

#[test]
fn the_gate_verb_runs_a_declared_gate_on_a_committed_tree_and_records_an_attested_verdict() {
    let (root, scratch) = committed_project();
    let sha = git_out(root.path(), &["rev-parse", "HEAD"]);
    let sha = sha.trim();
    let (out, err, ok) = rigger_in(root.path(), scratch.path(), &["gate", "g"]);
    assert!(ok, "rigger gate g exits 0 on a green gate; stderr:\n{err}");
    assert!(out.contains("PASS"), "the verb prints the result: {out}");
    let verdicts = gate_verdicts(root.path());
    assert_eq!(verdicts.len(), 1, "the verb records one verdict");
    let v = &verdicts[0];
    assert_eq!(
        v.meta.get("gate_origin").map(String::as_str),
        Some("rigger gate"),
        "the verdict carries the verb's provenance"
    );
    let digest = v
        .meta
        .get("input_digest")
        .expect("the verdict carries a digest");
    assert!(
        digest.ends_with(&format!(":{sha}")),
        "the digest addresses the gated commit {sha}: {digest}"
    );
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
