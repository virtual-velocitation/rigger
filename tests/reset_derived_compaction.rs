//! Periphery (CLI) test of `rigger reset --derived` on a backend the migration does not run on.
//!
//! The migration rewrites and deletes rows of the embedded `.rigger/events.db` and vacuums the
//! file, which no server-backed store offers. This test drives the COMPILED binary against a
//! project configured for the server-backed store and holds the refusal to its wording and to
//! leaving the local log as it stands, then the same project without that configuration to
//! migrating.

mod common;

use common::cli::log_and_graph_files;
use common::cli::migrated_lines;
use common::cli::read_run_events;
use common::cli::run_rigger;
use common::cli::run_rigger_envs;
use common::cli::seed_derived_duplicates;
use common::cli::temp_rigger_project;
use common::cli::{LOG_LEFT_AS_IT_STANDS_LINE, SERVER_BACKED_DERIVED_REFUSAL};
use rigger::retention::TYPE_GENERATION_INGESTED;

#[test]
fn reset_derived_on_a_backend_that_cannot_compact_fails_loudly_naming_the_backend_it_needs() {
    let dir = temp_rigger_project();
    let root = dir.path();
    seed_derived_duplicates(root);
    let db_before = read_run_events(root).len();
    let found = log_and_graph_files(root);

    // Rewriting and deleting rows and reclaiming the file is a mechanic of the embedded log.
    // Configured for the server-backed store, `--derived` must FAIL - never silently report a
    // migration that did not happen, which is the one outcome an operator cannot detect.
    let (out, err, ok) = run_rigger_envs(
        root,
        &["reset", "--derived"],
        &[("KURRENTDB_CONN", "esdb://127.0.0.1:2113?tls=false")],
    );
    assert_eq!(
        (
            ok,
            out.as_str(),
            err.as_str(),
            log_and_graph_files(root) == found
        ),
        (false, "", SERVER_BACKED_DERIVED_REFUSAL, true),
        "the refusal names the mode, the backend the migration needs, the backend the project is \
         configured for and what to do instead, advises no pruning of the server store, and \
         leaves the local log and graph files byte for byte"
    );

    // And it must be a REFUSAL, not a half-done migration: the local log is untouched.
    assert_eq!(
        (read_run_events(root).len(), db_before),
        (common::cli::DUP_ROUNDS, common::cli::DUP_ROUNDS),
        "a refused migration must leave the log exactly as it was"
    );

    // The refusal is decided BEFORE any pruning, so a composed invocation does not half-succeed.
    let (out, _, ok) = run_rigger_envs(
        root,
        &["reset", "--runs", "--derived"],
        &[("KURRENTDB_CONN", "esdb://127.0.0.1:2113?tls=false")],
    );
    assert!(
        !ok,
        "a composed reset naming an unsupported --derived must fail before pruning; stdout: {out}"
    );
    assert!(
        !out.contains("reset --runs:"),
        "the composed reset must refuse BEFORE running the --runs prune; got: {out:?}"
    );

    // WITHOUT THE VARIABLE the same project is on the sqlite backend, and the same log migrates:
    // the one batch's latest recording, the stream's last row, becomes its entry.
    let (out, err, ok) = run_rigger(root, &["reset", "--derived"]);
    assert_eq!(
        (
            ok,
            out,
            read_run_events(root)
                .into_iter()
                .map(|event| (event.position, event.revision, event.type_))
                .collect::<Vec<_>>(),
        ),
        (
            true,
            migrated_lines(1, common::cli::DUP_ROUNDS, 0) + LOG_LEFT_AS_IT_STANDS_LINE,
            vec![(3, 2, TYPE_GENERATION_INGESTED.to_string())],
        ),
        "on the sqlite backend the same log migrates; its stderr: {err}"
    );
}
