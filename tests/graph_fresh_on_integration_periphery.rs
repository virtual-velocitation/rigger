//! Periphery (real-git, real-worktree, cross-module) test for spec 92 criterion 1, FRESH ON
//! EVERY INTEGRATION: the cross-module seam `RunCtx::integrate_and_emit` -> `RunCtx::
//! ingest_files_into_graph` -> `rigger::ingest::ingest_files_batched` (src/conductor.rs), added
//! right alongside the grounder's own pre-existing post-integrate reindex.
//!
//! WHAT THE IMPLEMENTER'S OWN TESTS ALREADY COVER (not re-proven here). `src/ingest.rs`'s own
//! `scoped_reindex_tests` proves `ingest_files_batched`/`graph_index_lag`'s extraction, keying,
//! and bounded-scope filter directly (a hand-built root + file list, no conductor involved).
//! `src/conductor.rs`'s own `mod tests` proves `RunCtx::ingest_files_into_graph`'s bounded-scope
//! property THE SAME WAY - calling the private method directly against a hand-built `RunCtx`,
//! never through a real `run()`. `tests/validate_advisories.rs` proves the OBSERVABLE advisory
//! `rigger validate` draws from a lagging graph, driving the real binary.
//!
//! THE GAP those three leave. Nothing anywhere drives a real, git-isolated `conductor::run()` -
//! the actual production seam, the thing an operator's run actually does - and checks that the
//! CONTEXT GRAPH is fresh for a unit's files immediately after it lands, with the test itself
//! calling no reindex/ingest function of its own. `ingest_files_into_graph` is private
//! (`RunCtx` is not `pub`), so an external test cannot call it directly the way the inside-out
//! tests do; the only way to observe it from outside the crate is to drive the real seam it is
//! wired into and read the graph back afterward. Without this, a regression that deleted the
//! `self.ingest_files_into_graph(&files);` call from `integrate_and_emit` (leaving the callee
//! itself, and every inside-out test of it, untouched and green) would pass every other test in
//! the suite.
//!
//! NON-VACUOUS BY CONSTRUCTION. `RunCtx::ingest_project_into_graph` (the whole-project walk)
//! runs at most ONCE per process, triggered by the FIRST prompt this run builds - before the
//! implementer's spawn ever runs, so before `live_feature.rs` exists anywhere the walk can see
//! it (it lives only inside the unit's own throwaway worktree until a real merge lands it on the
//! repo root). With only one stage/one spawn in this run, that walk fires exactly once, over a
//! tree that does not yet contain the file. So nothing but the integrate-time scoped reindex
//! this criterion adds can be the reason `live_after_landing` is ever live in the graph by the
//! time this test reads it back - the property a revert of spec 92 criterion 1 would falsify.
//!
//! Whole-file `symbols` gate: `ingest_files_into_graph` is an unconditional no-op in the light
//! lane (mirroring `ingest_project_into_graph`'s own guard), so nothing in this file has a
//! light-lane counterpart to prove - a bare `--no-default-features` build with zero tests here,
//! same as the light lane draws no GRAPH INDEX LAG warning at all in `validate_advisories.rs`.

#![cfg(feature = "symbols")]

mod common;

use rigger::conductor::{run, AgentDriver, AgentResult, Deps, Error, SpawnOpts};
use rigger::config::{AgentDef, Config, Gate, Stage};
use rigger::contextgraph::sqlite::Projector;
use rigger::contextgraph::{Projection, KIND_CODE_ENTITY};
use rigger::eventstore::sqlite::Store;
use rigger::gate::ExecRunner;
use rigger::ledger::Status;
use serde_json::Value;
use std::process::Command;

/// `git init` a throwaway repo with one empty commit - the committed HEAD a real, isolated unit
/// worktree branches from and merges back into (mirrors the conductor's own scratch repo, and
/// `tests/unified_traversal_grounding.rs`'s `init_seam_repo`).
fn init_repo() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path();
    for args in [
        &["init", "-q"][..],
        &["config", "user.email", "t@example.com"],
        &["config", "user.name", "t"],
        &["commit", "--allow-empty", "-q", "-m", "init"],
    ] {
        Command::new("git")
            .arg("-C")
            .arg(p)
            .args(args)
            .output()
            .unwrap();
    }
    dir
}

/// Writes ONE new Rust file carrying a distinctive `pub fn` into the implementer's own worktree,
/// so the unit has a real, non-empty diff and carries the run cleanly through to a real,
/// git-merged integration - the file itself is the only observable this test needs; nothing else
/// about the spawn matters.
struct LandingDriver {
    implementer: String,
}

impl AgentDriver for LandingDriver {
    fn spawn(
        &self,
        agent: &AgentDef,
        _prompt: &str,
        opts: &SpawnOpts,
        _emit: &dyn Fn(&str, Value) -> Result<(), Error>,
    ) -> Result<AgentResult, Error> {
        if agent.id == self.implementer && !opts.dir.is_empty() {
            std::fs::write(
                format!("{}/live_feature.rs", opts.dir),
                "pub fn live_after_landing() {}\n",
            )
            .unwrap();
        }
        Ok(AgentResult {
            output: String::new(),
            resolved_model: String::new(),
        })
    }
}

/// Spec 92 criterion 1 (FRESH ON EVERY INTEGRATION), the cross-module seam itself: a real,
/// worktree-isolated `conductor::run()` over a single implement stage that lands cleanly leaves
/// the CONTEXT GRAPH reflecting the landed file's real definition - immediately, as a side
/// effect of the integration seam, never because this test asked for a reindex.
///
/// Only the `symbols` lane compiles the extraction pass at all (`ingest_files_into_graph` is an
/// unconditional no-op in the light lane, mirroring `ingest_project_into_graph`'s own guard), so
/// this is `symbols`-only - the whole-file gate above (the light lane has nothing to prove here).
#[test]
fn a_landed_units_file_is_fresh_in_the_graph_immediately_after_integration() {
    let repo = init_repo();
    let mut cfg = Config::default();
    // Spec 89 criterion 2 ruling item 2: this real, worktree-creating `conductor::run()` must
    // never reach the real ambient XDG_CACHE_HOME/HOME cache-home default.
    cfg.workflow.defaults.workdir = common::isolated_workdir(repo.path());
    cfg.agents.insert(
        "worker".into(),
        AgentDef {
            id: "worker".into(),
            ..Default::default()
        },
    );
    cfg.workflow.gates.insert(
        "ok".into(),
        Gate {
            run: "true".into(),
            kind: "core".into(),
            inputs: Vec::new(),
        },
    );
    cfg.workflow.stages.insert(
        "s".into(),
        Stage {
            name: "s".into(),
            agent: "worker".into(),
            coverage: "core".into(),
            gates: vec!["ok".into()],
            on_pass: "merge".into(),
            ..Default::default()
        },
    );

    let graph = Projector::open(":memory:", "test").unwrap();
    let store = Store::open(":memory:").unwrap();
    let driver = LandingDriver {
        implementer: "worker".into(),
    };
    let deps = Deps {
        store: &store,
        driver: &driver,
        gates: &ExecRunner,
        repo: repo.path().to_str().unwrap().to_string(),
        grounder: None,
        graph: Some(&graph),
        criteria: Vec::new(),
    };

    let rs = run(&cfg, &deps).unwrap();
    assert_eq!(
        rs.units["s"].status,
        Status::Integrated,
        "sanity: the unit must actually land for the seam under test to ever run at all - {:?}",
        rs.units["s"]
    );

    // No reindex/ingest call of any kind happens here - `is_live` only READS the graph the real
    // seam already folded during `run()` above.
    let is_live = |name: &str| -> bool {
        graph
            .subgraph(&["live_feature.rs".to_string()], 1)
            .unwrap()
            .nodes
            .iter()
            .any(|n| {
                n.kind == KIND_CODE_ENTITY && n.attrs.get("name").map(String::as_str) == Some(name)
            })
    };
    assert!(
        is_live("live_after_landing"),
        "a unit's own file must be fresh in the context graph immediately after its real \
         integration lands, with no separate reindex ever asked for by this test - the graph \
         index would otherwise stay stale until some later, unrelated prompt happened to \
         re-walk the whole project (docs/audit/2026-09-graph-vs-grep.md findings 4/9/11/12)"
    );
}
