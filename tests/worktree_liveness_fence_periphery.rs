//! Spec 83, criterion 1 (THE FENCE), at the periphery: the compiled binary's `rigger run` over a
//! real git repository and its on-disk store.
//!
//! `gc_integrated_branches`, run at the top of every `conductor::run`, reclaims the worktree and
//! branch of a unit whose ledger reads `Integrated` once `worktree::spawn_fence` finds none of its
//! spawns in flight, and logs each removal with its evidence. The crate's own tests drive its
//! logging seam (`gc_integrated_branches_logged`) through an injected sink; this file proves the
//! production wrapper is the one `rigger run` calls and that its evidence reaches real stderr. It
//! takes a unit's worktree only from under the run's scratch root: a worktree outside it, such as
//! a person's tree on a handed-back unit's branch, stays with its branch, and the log says so.

use crate::common;

use common::cli::run_recording;
use common::cli::run_stream_identity;
use common::cli::{write_workflow_fixture, REVIEWLESS_GIT_UNIT_WORKFLOW};
use common::git::git_answer;
use common::git::git_ok;
use common::git::git_ok_with_identity;
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

/// A unit whose ledger reads `Integrated` and whose one spawn was answered is reclaimed by
/// `gc_integrated_branches` when `rigger run` starts, from a worktree under the run's scratch
/// root only. There the worktree and its branch are gone, and real stderr carries the `branch-gc`
/// removal naming the branch and its terminal evidence. A worktree outside the root stays with
/// its work and its branch, and real stderr says the branch was kept and where it is checked out.
#[test]
fn a_run_reclaims_an_integrated_units_worktree_only_under_its_scratch_root_logging_its_evidence() {
    // Each row: where the unit's branch is checked out, and whether that is under the root.
    let rows = [
        ("under the scratch root", true),
        ("outside the scratch root", false),
    ];
    for (row, under) in rows {
        let dir = temp_git_project_with_commit();
        let root = dir.path();
        write_workflow_fixture(root, &REVIEWLESS_GIT_UNIT_WORKFLOW);
        git_ok(root, &["branch", "rigger-run"]);
        let scratch = tempfile::tempdir().unwrap();
        let scratch = scratch.path().canonicalize().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let place = if under {
            scratch.clone()
        } else {
            outside.path().canonicalize().unwrap()
        };
        let settled_dir = place.join("settled");
        let settled = settled_dir.to_str().unwrap();
        git_ok(
            root,
            &[
                "worktree",
                "add",
                settled,
                "-b",
                "rigger/u/settled",
                "rigger-run",
            ],
        );
        // Work committed on the branch, so the run's sweep of merged worktrees leaves the
        // worktree to the collection.
        std::fs::write(settled_dir.join("settled.rs"), "fn settled() {}\n").unwrap();
        git_ok(&settled_dir, &["add", "settled.rs"]);
        git_ok_with_identity(&settled_dir, &["commit", "-q", "-m", "settled work"]);

        // Started, then integrated, as a crash before the teardown recorded the removal leaves
        // it, with its one spawn answered ordinarily, so its evidence reads "terminal".
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

        let (out, err, ok, _fake) = run_recording(root, &["run"], &scratch);
        assert!(ok, "{row}: the run succeeds; stdout: {out}\nstderr: {err}");

        let branch_kept = git_answer(
            root,
            &[
                "rev-parse",
                "--verify",
                "--quiet",
                "refs/heads/rigger/u/settled",
            ],
        )
        .is_some();
        if under {
            assert!(
                !settled_dir.exists(),
                "{row}: the run reclaims the integrated unit's worktree; stderr:\n{err}"
            );
            assert!(
                err.contains("branch-gc")
                    && err.contains("removing branch")
                    && err.contains(r#""rigger/u/settled""#)
                    && err.contains("terminal"),
                "{row}: the removal is attributable from real stderr with its evidence: {err}"
            );
            assert!(
                !branch_kept,
                "{row}: the branch ref is reclaimed with the worktree"
            );
        } else {
            assert_eq!(
                std::fs::read_to_string(settled_dir.join("settled.rs"))
                    .ok()
                    .as_deref(),
                Some("fn settled() {}\n"),
                "{row}: the worktree and its work stay; stderr:\n{err}"
            );
            assert!(
                err.contains("branch-gc")
                    && err.contains("kept branch")
                    && err.contains(r#""rigger/u/settled""#)
                    && err.contains(settled),
                "{row}: real stderr says the branch was kept and where it is checked out: {err}"
            );
            assert!(branch_kept, "{row}: the branch stays");
        }
    }
}
