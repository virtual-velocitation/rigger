//! Periphery test for spec 122 criterion 5, PROOFS SETTLE ONCE PER TRANSACTION, its timed half:
//! the live fold of `src/cli/mod.rs` into a graph already holding `tests/cli.rs`, both read at
//! main's commit 3dcf9a8c and extracted by the binary's own extraction, takes at most 5 s of the
//! CPU time of the thread that folds it - where resolving every proof row of a name at each of
//! its test references held the write lock for minutes.
//!
//! The clock is the folding thread's own CPU time, read from `/proc/thread-self/schedstat`, so
//! the bound measures the fold and not the load beside it; the window covers the entry's one
//! `Projection::apply_generation` (`fold_asked`) alone, the seeding fold and both extractions
//! outside it. It
//! extracts real source, so it runs on the `symbols` lane alone, and only Linux has the clock.

#![cfg(all(feature = "symbols", target_os = "linux"))]

mod common;

use std::time::Duration;

use common::fixtures::{fold_asked, git_ok, temp_graph_db, GENERATION_BEFORE};
use common::repo::repo_root;
use rigger::contextgraph::sqlite::Projector;
use rigger::grounder::symbols::events::bytes_batch;

/// Main's commit whose two files the fold reads.
const FIXTURE_COMMIT: &str = "3dcf9a8c";

/// The most CPU time the fold of `src/cli/mod.rs` may take.
const BOUND: Duration = Duration::from_secs(5);

/// The batch the extraction answers for `path` as [`FIXTURE_COMMIT`] holds it, read from this
/// checkout's history.
fn batch_at_fixture_commit(path: &str) -> Vec<rigger::eventstore::Event> {
    let blob = git_ok(repo_root(), &["show", &format!("{FIXTURE_COMMIT}:{path}")]).stdout;
    bytes_batch(path, Some(&blob), false)
}

/// The CPU time the calling thread has run for: the first field of its `schedstat`, in
/// nanoseconds.
fn thread_cpu() -> Duration {
    let schedstat = std::fs::read_to_string("/proc/thread-self/schedstat")
        .expect("/proc/thread-self/schedstat names the thread's CPU time");
    let nanos = schedstat
        .split_whitespace()
        .next()
        .and_then(|field| field.parse().ok())
        .expect("schedstat opens with the thread's CPU time in nanoseconds");
    Duration::from_nanos(nanos)
}

#[test]
fn folding_src_cli_mod_rs_over_tests_cli_rs_takes_at_most_five_seconds_of_thread_cpu() {
    let (_dir, graph_db) = temp_graph_db();
    let p = Projector::open(&graph_db, "timed").unwrap();
    fold_asked(
        &p,
        "tests/cli.rs",
        GENERATION_BEFORE,
        1,
        1,
        batch_at_fixture_commit("tests/cli.rs"),
    );
    let batch = batch_at_fixture_commit("src/cli/mod.rs");
    let before = thread_cpu();
    fold_asked(&p, "src/cli/mod.rs", GENERATION_BEFORE, 2, 2, batch);
    let spent = thread_cpu() - before;
    let report = format!("the fold of src/cli/mod.rs took {spent:?} of thread CPU");
    eprintln!("{report}");
    assert!(spent <= BOUND, "{report}, over {BOUND:?}");
}
