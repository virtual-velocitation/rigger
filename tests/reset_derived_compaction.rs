//! Periphery (CLI) test of `rigger reset --derived` on a backend the migration does not run on.
//!
//! The migration rewrites and deletes rows of the embedded `.rigger/events.db` and vacuums the
//! file, which no server-backed store offers. This test drives the COMPILED binary against a
//! project configured for the server-backed store and holds the refusal to its wording and to
//! leaving the local log as it stands.

mod common;

use common::cli::read_run_events;
use common::cli::run_rigger_envs;
use common::cli::seed_derived_duplicates;
use common::cli::temp_rigger_project;

#[test]
fn reset_derived_on_a_backend_that_cannot_compact_fails_loudly_naming_the_backend_it_needs() {
    let dir = temp_rigger_project();
    let root = dir.path();
    seed_derived_duplicates(root);
    let db_before = read_run_events(root).len();

    // Rewriting and deleting rows and reclaiming the file is a mechanic of the embedded log.
    // Configured for the server-backed store, `--derived` must FAIL - never silently report a
    // migration that did not happen, which is the one outcome an operator cannot detect.
    let (out, err, ok) = run_rigger_envs(
        root,
        &["reset", "--derived"],
        &[("KURRENTDB_CONN", "esdb://127.0.0.1:2113?tls=false")],
    );
    assert_eq!(
        (ok, out.as_str(), err.as_str()),
        (
            false,
            "",
            "rigger: reset --derived: the migration rewrites and deletes rows of the event log \
             and vacuums the file, which is a mechanic of the embedded .rigger/events.db store; \
             this project is configured for the server-backed store, where the migration does \
             not run. Re-run it against a project on the sqlite backend. Refusing rather than \
             reporting a migration that did not happen.\n"
        ),
        "the refusal names the mode, the backend the migration needs, the backend the project is \
         configured for and what to do instead, and advises no pruning of the server store"
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
}
