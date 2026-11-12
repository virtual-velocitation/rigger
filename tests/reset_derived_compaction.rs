//! Periphery (CLI) test of `rigger reset --derived` on a backend the delete does not run on.
//!
//! The delete removes rows of the embedded `.rigger/events.db` and vacuums the file, which no
//! server-backed store offers. This test drives the COMPILED binary against a project configured
//! for the server-backed store and holds the refusal to its wording and to leaving the local log
//! as it stands, then the same project without that configuration to deleting.

use crate::common;

use common::cli::deleted_line;
use common::cli::log_and_graph_files;
use common::cli::read_run_events;
use common::cli::run_rigger;
use common::cli::run_rigger_envs;
use common::cli::seed_derived_duplicates;
use common::cli::stream_shape;
use common::cli::temp_rigger_project;
use common::cli::{server_backed_derived_refusal, LOG_LEFT_AS_IT_STANDS_LINE};

#[test]
fn reset_derived_on_a_backend_that_cannot_compact_fails_loudly_naming_the_backend_it_needs() {
    let dir = temp_rigger_project();
    let root = dir.path();
    seed_derived_duplicates(root);
    let db_before = read_run_events(root).len();
    let found = log_and_graph_files(root);

    // Deleting rows and reclaiming the file is a mechanic of the embedded log. Configured for the
    // server-backed store, `--derived` must FAIL - never silently report a delete that did not
    // happen, which is the one outcome an operator cannot detect.
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
        (false, "", server_backed_derived_refusal().as_str(), true),
        "the refusal names the mode, the backend the delete needs, the backend the project is \
         configured for and what to do instead, advises no pruning of the server store, and \
         leaves the local log and graph files byte for byte"
    );

    // And it must be a REFUSAL, not a half-done delete: the local log is untouched.
    assert_eq!(
        (read_run_events(root).len(), db_before),
        (common::cli::DUP_ROUNDS, common::cli::DUP_ROUNDS),
        "a refused delete must leave the log exactly as it was"
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

    // WITHOUT THE VARIABLE the same project is on the sqlite backend, and the delete empties the
    // stream of its derived rows.
    let (out, err, ok) = run_rigger(root, &["reset", "--derived"]);
    assert_eq!(
        (ok, out, stream_shape(root),),
        (
            true,
            deleted_line(common::cli::DUP_ROUNDS) + LOG_LEFT_AS_IT_STANDS_LINE,
            vec![],
        ),
        "on the sqlite backend the same log's derived rows are deleted; its stderr: {err}"
    );
}

/// `rigger reset --derived` clears the code store's facts memo first, on any backend: GIVEN a
/// project whose memo holds two entries, WHEN `--derived` runs configured for the server-backed
/// store, THEN it names the bytes it removed and leaves no memo before it refuses the delete;
/// and WHEN it runs again on the sqlite backend, THEN with no memo left it says nothing of one.
#[test]
fn reset_derived_clears_the_code_facts_memo_first_on_any_backend() {
    let dir = temp_rigger_project();
    let root = dir.path();
    let memo = rigger_store::code::Store::new(root).memo_dir();
    std::fs::create_dir_all(&memo).unwrap();
    std::fs::write(memo.join("a.facts"), [0u8; 10]).unwrap();
    std::fs::write(memo.join("b.facts"), [0u8; 20]).unwrap();

    let (out, err, ok) = run_rigger_envs(
        root,
        &["reset", "--derived"],
        &[("KURRENTDB_CONN", "esdb://127.0.0.1:2113?tls=false")],
    );
    assert_eq!(
        (ok, out.as_str(), err.as_str(), memo.exists()),
        (
            false,
            "reset --derived: removed the code facts memo (30B)\n",
            server_backed_derived_refusal().as_str(),
            false
        ),
        "the memo is cleared and named before the delete is refused"
    );

    let (out, err, ok) = run_rigger(root, &["reset", "--derived"]);
    assert!(ok, "the sqlite delete runs; stderr: {err}");
    assert!(
        !out.contains("code facts memo"),
        "with no memo left, nothing is said of one; stdout: {out}"
    );
}
