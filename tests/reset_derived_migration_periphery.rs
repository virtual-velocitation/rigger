//! PERIPHERY (contract / API / integration) tests for the edges of `rigger reset --derived`, the
//! one-time migration of the event log, that the migration's own behavior test in `tests/cli.rs`
//! does not walk. Each drives the built binary and reads back what an operator can read.
//!
//!   1. **The mode list.** A `rigger reset` handed flags but no mode names every mode, and names
//!      `--derived` as the migration it is.
//!   2. **The next writer.** A migration that deleted the tail of the run stream leaves a log the
//!      next append lands on: above every position the log ever held, at the revision after the
//!      highest the stream still holds, and folded into the graph.
//!   3. **The shipped store-hygiene skill.** The committed skill an operator reads before a reset
//!      describes `--derived` as the migration, word for word, wherever it names it.

mod common;

use common::cli::{
    applied_positions, emit, log_and_graph_files, migrated_lines, pre_ledger_batch, rigger_file,
    run_rigger, stream_shape, temp_rigger_project, with_run_store, LOG_LEFT_AS_IT_STANDS_LINE,
};
use common::repo::repo_text;
use rigger::contextgraph::{TYPE_CODE_ENTITY_EXTRACTED, TYPE_DECISION_MADE, TYPE_EDGE_INFERRED};
use rigger::eventstore::ExpectedRevision;
use rigger::retention::TYPE_GENERATION_INGESTED;
use std::path::Path;

/// Record, in `root`'s run stream, one pre-ledger batch of two derived events - an entity at
/// position 1 and an edge at position 2 - so the migration rewrites the first into the batch's
/// ledger entry and deletes the second, the stream's tail.
fn seed_a_two_event_batch(root: &Path) {
    with_run_store(root, |store| {
        store
            .append(
                rigger::conductor::STREAM,
                ExpectedRevision::Any,
                &pre_ledger_batch("alpha", 10),
            )
            .expect("seed the batch");
    });
}

// ---------------------------------------------------------------------------------------
// 1. The mode list
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
// 2. The next writer
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

// ---------------------------------------------------------------------------------------
// 3. The shipped store-hygiene skill
// ---------------------------------------------------------------------------------------

/// Given the committed `rigger-reset-store` skill, when an operator reads what it says of
/// `rigger reset --derived`, then the two lines naming it are the migration's: what it rewrites
/// and deletes, that every other event survives, the backend it runs on and its refusal of a
/// live run; and that a hand-edit is no substitute for it. Read from the bytes on disk, the file
/// an operator opens, with no render in the loop.
#[test]
fn the_committed_reset_store_skill_describes_the_derived_reset_as_the_migration() {
    let shipped = repo_text("skills/rigger-reset-store/SKILL.md");
    let naming_it: Vec<&str> = shipped
        .lines()
        .filter(|line| line.contains("rigger reset --derived"))
        .collect();

    assert_eq!(
        naming_it,
        [
            "- `rigger reset --derived` migrates `events.db`, once: for each file it rewrites the \
             first row of the file's latest derived batch into that generation's ledger entry, in \
             place, deletes every other derived event, and vacuums so the file shrinks on disk. \
             Every other event - every decision, finding, lesson, gate verdict, the whole run \
             history - survives byte-for-byte. The migration runs on the embedded sqlite backend \
             only, and it refuses (unless overridden with `--force-live`) while a run is live \
             against the store.",
            "Never touch `events.db`, `graph.db`, or `progress.db` with raw SQL, `rm`, or any tool \
             outside `rigger reset`. The event log is append-only truth: a hand-edit or a \
             hand-deleted row can desync the graph from the log in ways `rigger reset \
             --derived`'s own migration is specifically built to avoid. A store file that is \
             genuinely corrupt is an incident to fix at its root, never a reason to reach for a \
             database client.",
        ]
    );
}
