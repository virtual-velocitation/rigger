//! PERIPHERY (contract / API / integration) tests for the edges of `rigger reset --derived`, the
//! delete of the event log's derived events, that the delete's own behavior test in
//! `tests/cli.rs` does not walk. Each drives the built binary and reads back what an operator can
//! read.
//!
//!   1. **The mode list.** A `rigger reset` handed flags but no mode names every mode, and names
//!      `--derived` as the delete it is.
//!   2. **The next writer.** A delete that emptied the run stream leaves a log the next append
//!      lands on: above every position the log ever held, and folded into the graph.
//!   3. **The shipped store-hygiene skill.** The committed skill an operator reads before a reset
//!      describes `--derived` as the delete, word for word, wherever it names it.

use crate::common;

use common::cli::{
    applied_positions, deleted_line, emit, log_and_graph_files, pre_ledger_batch, rigger_file,
    run_rigger, stream_shape, temp_rigger_project, LOG_LEFT_AS_IT_STANDS_LINE,
};
use common::repo::repo_text;
use rigger::contextgraph::{TYPE_CODE_ENTITY_EXTRACTED, TYPE_DECISION_MADE, TYPE_EDGE_INFERRED};
use std::path::Path;

/// Record, in `root`'s run stream, one pre-ledger batch of two derived events - an entity at
/// position 1 and an edge at position 2 - which the delete removes, the stream's whole content.
fn seed_a_two_event_batch(root: &Path) {
    common::cli::seed_pre_ledger_events(root, &pre_ledger_batch("alpha", 10));
}

// ---------------------------------------------------------------------------------------
// 1. The mode list
// ---------------------------------------------------------------------------------------

/// Given any project, when the operator runs `rigger reset --force-live`, a flag that is no
/// mode, then the command fails naming every mode, `--derived` as the delete of the event log's
/// derived events, and the log holding a pre-ledger batch is left as it stands.
#[test]
fn reset_handed_no_mode_names_the_derived_mode_as_the_delete() {
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
            (common::cli::refusal_line(
                "reset: expected at least one mode: rigger reset --runs (prune the context \
                 graph), rigger reset --derived (delete the event log's derived events), rigger \
                 reset --build-cache (reclaim the shared gate build \
                 cache), and/or rigger reset --scratch-orphans (reclaim cache-home scratch roots \
                 whose repo is gone)"
            ) + "\n")
                .as_str(),
            true,
        ),
        "a reset handed no mode names the modes and changes nothing"
    );
}

// ---------------------------------------------------------------------------------------
// 2. The next writer
// ---------------------------------------------------------------------------------------

/// Given a run stream whose every event was a derived row the delete removed, when an agent then
/// records a decision, then the decision stands above every position the log ever held (3,
/// never the deleted rows' 1 or 2), the stream reads back as that decision alone, and the graph
/// folded the decision at its position.
#[test]
fn the_append_after_a_delete_that_emptied_the_stream_lands_above_it_and_folds() {
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
        (true, deleted_line(2) + LOG_LEFT_AS_IT_STANDS_LINE, vec![],),
        "premise: the delete emptied the stream; its stderr: {err}"
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
        (vec![(3, 0, TYPE_DECISION_MADE.to_string())], vec![3],),
        "the next append lands above the deleted rows and is folded"
    );
}

// ---------------------------------------------------------------------------------------
// 3. The shipped store-hygiene skill
// ---------------------------------------------------------------------------------------

/// Given the committed `rigger-reset-store` skill, when an operator reads what it says of
/// `rigger reset --derived`, then the two lines naming it are the delete's: what it deletes, that
/// setup runs it, that every other event survives, the backend it runs on and its refusal of a
/// live run, and that it removes the code store's facts memo once the delete has run, and nothing
/// when refused; and that a hand-edit is no substitute for it. Read from the bytes on disk, the
/// file an operator opens, with no render in the loop.
#[test]
fn the_committed_reset_store_skill_describes_the_derived_reset_as_the_delete() {
    let shipped = repo_text("skills/rigger-reset-store/SKILL.md");
    let naming_it: Vec<&str> = shipped
        .lines()
        .filter(|line| line.contains("rigger reset --derived"))
        .collect();

    assert_eq!(
        naming_it,
        [
            "- `rigger reset --derived` deletes every derived event of `events.db` in one \
             transaction and vacuums so the file shrinks on disk; `rigger setup` runs the same \
             delete once the graph it rebuilds is in place. Every other event - every decision, \
             finding, lesson, gate verdict, the whole run history - survives byte-for-byte. The \
             delete runs on the embedded sqlite backend only, and it refuses (unless overridden \
             with `--force-live`) while a run is live against the store. Once the delete has run \
             it removes the code store's facts memo (a cache reads refill); a refused \
             `--derived` removes nothing.",
            "Never touch `events.db`, `graph.db`, or `progress.db` with raw SQL, `rm`, or any tool \
             outside `rigger reset`. The event log is append-only truth: a hand-edit or a \
             hand-deleted row can desync the graph from the log in ways `rigger reset \
             --derived`'s own delete is specifically built to avoid. A store file that is \
             genuinely corrupt is an incident to fix at its root, never a reason to reach for a \
             database client.",
        ]
    );
}
