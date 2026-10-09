//! PERIPHERY (contract / API / integration) tests for the edges of `rigger reset --derived`, the
//! one-time migration of the event log, that the migration's own behavior test in `tests/cli.rs`
//! does not walk. Each drives the built binary and reads back what an operator can read.
//!
//!   1. **The backend the migration needs.** On a project configured for the server-backed store
//!      the command refuses in the migration's own words, before it reaches for the server, and
//!      leaves a local log lying beside it as it found it; the same project without that
//!      configuration migrates.
//!   2. **The mode list.** A `rigger reset` handed flags but no mode names every mode, and names
//!      `--derived` as the migration it is.
//!   3. **The next writer.** A migration that deleted the tail of the run stream leaves a log the
//!      next append lands on: above every position the log ever held, at the revision after the
//!      highest the stream still holds, and folded into the graph.

mod common;

use common::cli::{
    applied_positions, emit, keyed, log_and_graph_files, migrated_lines, read_run_events,
    rigger_file, run_rigger, run_rigger_envs, temp_rigger_project, with_run_store,
    LOG_LEFT_AS_IT_STANDS_LINE,
};
use rigger::contextgraph::{TYPE_CODE_ENTITY_EXTRACTED, TYPE_DECISION_MADE, TYPE_EDGE_INFERRED};
use rigger::eventstore::ExpectedRevision;
use rigger::retention::TYPE_GENERATION_INGESTED;
use std::path::Path;

/// The generation every seeded batch below is keyed under.
const BATCH: &str = "gc/src/a.rs@h1";

/// Record, in `root`'s run stream, one pre-ledger batch of two derived events - an entity at
/// position 1 and an edge at position 2 - so the migration rewrites the first into the batch's
/// ledger entry and deletes the second, the stream's tail.
fn seed_a_two_event_batch(root: &Path) {
    let edge = br#"{"file":"src/a.rs","name":"alpha","lang":"rust"}"#.to_vec();
    with_run_store(root, |store| {
        store
            .append(
                rigger::conductor::STREAM,
                ExpectedRevision::Any,
                &[
                    keyed(
                        TYPE_CODE_ENTITY_EXTRACTED,
                        common::cli::code_entity(),
                        &format!("{BATCH}#0"),
                        10,
                    ),
                    keyed(TYPE_EDGE_INFERRED, edge, &format!("{BATCH}#1"), 11),
                ],
            )
            .expect("seed the batch");
    });
}

/// `root`'s run stream as `(position, revision, type)`, oldest first.
fn stream_shape(root: &Path) -> Vec<(u64, i64, String)> {
    read_run_events(root)
        .into_iter()
        .map(|event| (event.position, event.revision, event.type_))
        .collect()
}

// ---------------------------------------------------------------------------------------
// 1. The backend the migration needs
// ---------------------------------------------------------------------------------------

/// Given a project holding a local log with a pre-ledger batch, when the operator runs `rigger
/// reset --derived` with the project configured for the server-backed store, then the command
/// fails with the migration's refusal and nothing else, prints no report, and the local log and
/// graph files stand byte for byte; run again without that configuration, the same log migrates.
#[test]
fn reset_derived_on_a_server_backed_project_refuses_the_migration_and_leaves_the_local_log() {
    let dir = temp_rigger_project();
    let root = dir.path();
    seed_a_two_event_batch(root);
    let found = log_and_graph_files(root);

    // A well-formed address nothing listens on: the refusal is decided on the selection alone.
    let (out, err, ok) = run_rigger_envs(
        root,
        &["reset", "--derived"],
        &[("KURRENTDB_CONN", "kurrentdb://127.0.0.1:65533?tls=false")],
    );

    assert_eq!(
        (
            ok,
            out.as_str(),
            err.as_str(),
            log_and_graph_files(root) == found
        ),
        (
            false,
            "",
            "rigger: reset --derived: the migration rewrites and deletes rows of the event log \
             and vacuums the file, which is a mechanic of the embedded .rigger/events.db store; \
             this project is configured for the server-backed store, where the migration does \
             not run. Re-run it against a project on the sqlite backend. Refusing rather than \
             reporting a migration that did not happen.\n",
            true,
        ),
        "a server-backed project refuses the migration and changes nothing"
    );

    let (out, err, ok) = run_rigger(root, &["reset", "--derived"]);
    assert_eq!(
        (ok, out, stream_shape(root)),
        (
            true,
            migrated_lines(1, 2, 0) + LOG_LEFT_AS_IT_STANDS_LINE,
            vec![(1, 0, TYPE_GENERATION_INGESTED.to_string())],
        ),
        "on the sqlite backend the same log migrates; its stderr: {err}"
    );
}

// ---------------------------------------------------------------------------------------
// 2. The mode list
// ---------------------------------------------------------------------------------------

/// Given any project, when the operator runs `rigger reset --force-live`, a flag that is no
/// mode, then the command fails naming every mode, `--derived` as the migration of the event
/// log's derived events into ledger entries, and the log holding a pre-ledger batch is left as
/// it stands.
#[test]
fn reset_handed_no_mode_names_the_derived_mode_as_the_migration() {
    let dir = temp_rigger_project();
    let root = dir.path();
    seed_a_two_event_batch(root);
    let found = log_and_graph_files(root);

    let (out, err, ok) = run_rigger(root, &["reset", "--force-live"]);

    assert_eq!(
        (
            ok,
            out.as_str(),
            err.as_str(),
            log_and_graph_files(root) == found
        ),
        (
            false,
            "",
            "rigger: reset: expected at least one mode: rigger reset --runs (prune the context \
             graph), rigger reset --derived (migrate the event log's derived events into ledger \
             entries), rigger reset --build-cache (reclaim the shared gate build cache), and/or \
             rigger reset --scratch-orphans (reclaim cache-home scratch roots whose repo is \
             gone)\n",
            true,
        ),
        "a reset handed no mode names the modes and changes nothing"
    );
}

// ---------------------------------------------------------------------------------------
// 3. The next writer
// ---------------------------------------------------------------------------------------

/// Given a run stream whose last event was a derived row the migration deleted, when an agent
/// then records a decision, then the decision stands above every position the log ever held (3,
/// never the deleted row's 2), at the revision after the highest the stream still holds (1, the
/// deleted row's own), the stream reads back as the ledger entry then the decision, and the
/// graph folded the decision at its position.
#[test]
fn the_append_after_a_migration_that_deleted_the_streams_tail_lands_above_it_and_folds() {
    let dir = temp_rigger_project();
    let root = dir.path();
    seed_a_two_event_batch(root);
    assert_eq!(
        stream_shape(root),
        vec![
            (1, 0, TYPE_CODE_ENTITY_EXTRACTED.to_string()),
            (2, 1, TYPE_EDGE_INFERRED.to_string()),
        ],
        "premise: the stream's tail is a derived row"
    );
    let (out, err, ok) = run_rigger(root, &["reset", "--derived"]);
    assert_eq!(
        (ok, out, stream_shape(root)),
        (
            true,
            migrated_lines(1, 2, 0) + LOG_LEFT_AS_IT_STANDS_LINE,
            vec![(1, 0, TYPE_GENERATION_INGESTED.to_string())],
        ),
        "premise: the migration deleted the stream's tail; its stderr: {err}"
    );

    emit(
        root,
        TYPE_DECISION_MADE,
        r#"{"id":"d-after","summary":"s","governs":["src/a.rs"],"supersedes":""}"#,
    );

    assert_eq!(
        (
            stream_shape(root),
            applied_positions(&rigger_file(root, "graph.db")),
        ),
        (
            vec![
                (1, 0, TYPE_GENERATION_INGESTED.to_string()),
                (3, 1, TYPE_DECISION_MADE.to_string()),
            ],
            vec![3],
        ),
        "the next append lands above the deleted tail and is folded"
    );
}
