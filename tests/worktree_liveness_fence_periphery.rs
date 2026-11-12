//! Spec 83, criterion 1 (THE FENCE), at the periphery: the compiled binary's `rigger run` over a
//! real git repository and its on-disk store.
//!
//! `gc_integrated_branches`, run at the top of every `conductor::run`, reclaims the worktree and
//! branch of a unit whose ledger reads `Integrated` once `worktree::spawn_fence` finds none of its
//! spawns in flight, and logs each removal with its evidence. The crate's own tests drive its
//! logging seam (`gc_integrated_branches_logged`) through an injected sink; this file proves the
//! production wrapper is the one `rigger run` calls and that its evidence reaches real stderr. It
//! logs a removal only for a worktree git still lists, and `registered_worktree_for` reads git's
//! global worktree list, so a worktree outside the scratch root is reclaimed by this path alone.

use crate::common;

use common::cli::run_recording;
use common::cli::run_stream_identity;
use common::cli::{write_workflow_fixture, REVIEWLESS_GIT_UNIT_WORKFLOW};
use common::git::git_answer;
use common::git::git_ok;
use common::git::temp_git_project_with_commit;
use rigger::spawn::SpawnEvent;
use std::path::Path;

/// Mint the no-spec run `rigger run` adopts (no criteria, no pinned definition) in `root`'s
/// on-disk store and append `events`, built through the crate's public
/// `rigger::spawn`/`rigger::eventstore` API, to its run stream: a unit's ledger history and its
/// spawns, as an earlier drive of the run recorded them.
fn seed_events(root: &Path, events: Vec<rigger::eventstore::Event>) {
    use rigger::eventstore::namespace::Namespaced;
    use rigger::eventstore::sqlite::Store;
    use rigger::eventstore::{EventStore, ExpectedRevision};

    let rigger_dir = root.join(".rigger");
    std::fs::create_dir_all(&rigger_dir).unwrap();
    let backend = Store::open(rigger_dir.join("events.db").to_str().unwrap()).unwrap();
    let store = Namespaced::new(&backend, &run_stream_identity(root));
    rigger::run_store::ensure_started(&store, &[]).unwrap();
    for event in events {
        store
            .append(rigger::conductor::STREAM, ExpectedRevision::Any, &[event])
            .unwrap();
    }
}

/// A unit whose ledger reads `Integrated`, whose one spawn was answered, and whose worktree
/// git still lists outside the scratch root is reclaimed by `gc_integrated_branches` when
/// `rigger run` starts: the worktree and its branch are gone, and real stderr carries the
/// `branch-gc` removal naming the branch and its terminal evidence.
#[test]
fn a_run_reclaims_an_integrated_units_registered_worktree_logging_its_evidence() {
    let dir = temp_git_project_with_commit();
    let root = dir.path();
    write_workflow_fixture(root, &REVIEWLESS_GIT_UNIT_WORKFLOW);
    git_ok(root, &["branch", "rigger-run"]);

    let settled_dir = root.join("settled-outside-the-scratch-root");
    git_ok(
        root,
        &[
            "worktree",
            "add",
            settled_dir.to_str().unwrap(),
            "-b",
            "rigger/u/settled",
            "rigger-run",
        ],
    );

    // Started, then integrated, as a crash before the teardown recorded the removal leaves it,
    // with its one spawn answered ordinarily, so its evidence reads "terminal".
    seed_events(
        root,
        vec![
            rigger::eventstore::Event::new(
                rigger::ledger::TYPE_UNIT_STARTED,
                br#"{"id":"settled","branch":"rigger/u/settled"}"#.to_vec(),
            ),
            rigger::eventstore::Event::new(
                rigger::ledger::TYPE_UNIT_INTEGRATED,
                br#"{"id":"settled","commit":"deadbeef"}"#.to_vec(),
            ),
            common::spawn_request("settled", "settled", "adversary", 0, "verify")
                .to_event()
                .unwrap(),
            rigger::spawn::SpawnResult::ok("settled/adversary#0", "approve")
                .to_event()
                .unwrap(),
        ],
    );

    let scratch = tempfile::tempdir().unwrap();
    let (out, err, ok, _fake) = run_recording(root, &["run"], scratch.path());
    assert!(ok, "the run succeeds; stdout: {out}\nstderr: {err}");

    assert!(
        !settled_dir.exists(),
        "the run reclaims the integrated unit's registered worktree; stderr:\n{err}"
    );
    assert!(
        err.contains("branch-gc")
            && err.contains("removing branch")
            && err.contains(r#""rigger/u/settled""#)
            && err.contains("terminal"),
        "the removal is attributable from real stderr with its evidence: {err}"
    );
    assert!(
        git_answer(
            root,
            &[
                "rev-parse",
                "--verify",
                "--quiet",
                "refs/heads/rigger/u/settled"
            ]
        )
        .is_none(),
        "the branch ref is reclaimed with the worktree"
    );
}
