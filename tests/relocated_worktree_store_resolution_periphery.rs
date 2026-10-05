//! Periphery (cross-module, real-binary) test for spec 89, criterion 2 (SCRATCH IS OUTSIDE
//! THE STORE TREE) at the store-RESOLUTION boundary a real courier crosses.
//!
//! `find_store_dir_from`'s own new unit tests in `src/main.rs`
//! (`find_store_dir_from_resolves_the_owning_repo_even_when_the_worktree_lives_outside_it`,
//! `find_store_dir_from_never_climbs_a_relocated_worktrees_own_unrelated_ancestors_into_a_
//! foreign_store`) prove the PURE walk-classification logic returns the right directory when
//! called directly with the right arguments - but that function (and `require_store_dir`,
//! which wraps it) is private to the `rigger` BINARY crate, reachable only from its own
//! in-process unit tests. Nothing proves the real, compiled COURIER commands spec 89's own
//! Problem statement names (`emit`/`result`/`peers`/`reported` - a worker's self-report from
//! inside its unit worktree) actually WIRE this logic correctly: that `cmd_emit` reaches
//! `require_store_dir` with the process's genuine cwd, that a relocated worktree's real
//! subprocess lands its write in the owning repo's real store rather than refusing or
//! fabricating a stray one, and - the class of bug a white-box call-the-function-directly
//! test structurally cannot see - that it does not silently bind a FOREIGN store an unrelated
//! ancestor of the relocated worktree happens to carry. This file closes that gap end to end,
//! driving the real compiled binary exactly as a worker's own `rigger emit` self-report would,
//! from inside a REAL git-linked worktree living outside the repo's own directory tree - the
//! shape every real spawn's worktree takes once this criterion's relocated scratch default is
//! in effect (spec 89 Design, SCRATCH LIVES OUTSIDE THE STORE TREE).
//!
//! The context-graph reads an agent runs from that same worktree (`rigger graph --show`,
//! `rigger graph --around` and the spawn MCP server's `rigger_graph`) cross the same boundary:
//! they answer from the owning repository's graph, never a graph opened in the worktree.

use std::path::Path;

mod common;
use common::cli::open_graph;
use common::cli::run_rigger;
use common::cli::run_stream_identity;
use common::fixtures::apply_code_entity;
use common::git::run_git;
use common::mcp::McpSession;

use rigger::eventstore::namespace::Namespaced;
use rigger::eventstore::sqlite::Store;
use rigger::eventstore::{Direction, EventStore};

/// `git init` plus one real commit under `root` - a bare `git init` has no commit for `git
/// worktree add -b <new-branch>` to branch from.
fn git_init_committed(root: &Path) {
    let run = |args: &[&str]| {
        assert!(
            run_git(root, args).status.success(),
            "git {args:?} must succeed"
        );
    };
    run(&["init", "-q"]);
    run(&["config", "user.email", "test@example.invalid"]);
    run(&["config", "user.name", "test"]);
    std::fs::write(root.join("README"), "x").expect("write README");
    run(&["add", "-A"]);
    run(&["commit", "-q", "-m", "init"]);
}

/// Seed an initialized, EMPTY `.rigger/events.db` under `root` - `rigger emit` refuses to
/// fabricate a fresh store from the wrong cwd (spec 05), mirroring
/// `tests/cli.rs::seed_store`/`tests/spawn_scratch_reap_authorized_root_periphery.rs::seed_store`.
fn seed_store(root: &Path) {
    let rigger = root.join(".rigger");
    std::fs::create_dir_all(&rigger).expect("create .rigger");
    std::fs::File::create(rigger.join("events.db")).expect("create events.db");
}

/// Add a git-linked worktree of `root` at `worktree` on the new branch `branch` - placed by the
/// caller wholly outside `root`'s own directory tree, the shape a real spawn's worktree takes.
fn add_relocated_worktree(root: &Path, worktree: &Path, branch: &str) {
    assert!(
        run_git(
            root,
            &[
                "worktree",
                "add",
                "-q",
                worktree.to_str().unwrap(),
                "-b",
                branch
            ],
        )
        .status
        .success(),
        "git worktree add must succeed for the fixture"
    );
}

/// A committed repository at `root` with a store and a graph holding one code entity,
/// `a.rs::alpha`, folded under the identity the binary resolves for `root` - the owning graph a
/// lookup from any of its worktrees must answer from - and a relocated worktree of it at
/// `worktree` on `branch`, carrying the `.rigger/` a unit worktree checks out (its tracked
/// workflow) with no graph of its own. Answers nothing; the caller owns both directories.
fn owning_graph_with_relocated_worktree(root: &Path, worktree: &Path, branch: &str) {
    git_init_committed(root);
    seed_store(root);
    // Seeded at a position the store below never reaches, so an emit the test folds never
    // collides with it on the graph's per-position applied ledger.
    apply_code_entity(
        &open_graph(root),
        100_001,
        "a.rs",
        "alpha",
        "function",
        1,
        "rust",
    );
    add_relocated_worktree(root, worktree, branch);
    std::fs::create_dir_all(worktree.join(".rigger")).expect("create the worktree's .rigger");
    std::fs::write(
        worktree.join(".rigger").join("workflow.yml"),
        "stages: []\n",
    )
    .expect("write the worktree's workflow");
}

/// The structured answer of one `rigger_graph` tool call with `arguments` over `mcp`.
fn graph_tool(mcp: &mut McpSession, arguments: serde_json::Value) -> serde_json::Value {
    let resp = mcp.call(
        "tools/call",
        serde_json::json!({"name": "rigger_graph", "arguments": arguments}),
    );
    resp["result"]["structuredContent"].clone()
}

/// The node ids of a `rigger_graph` `around` answer.
fn node_ids(around: &serde_json::Value) -> Vec<String> {
    around["nodes"]
        .as_array()
        .map(|nodes| {
            nodes
                .iter()
                .filter_map(|n| n["id"].as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default()
}

/// Every `DecisionMade` payload recorded in `root`'s real store, read back through a FRESH
/// `Store::open` independent of the courier subprocess that wrote it - so a passing assertion
/// proves the subprocess's write genuinely landed on disk at `root`, not merely that the
/// child process exited 0.
fn decision_summaries_in(root: &Path) -> Vec<String> {
    let backend = Store::open(root.join(".rigger").join("events.db").to_str().unwrap())
        .expect("open the real store for readback");
    let store = Namespaced::new(&backend, &run_stream_identity(root));
    store
        .read_stream(rigger::conductor::STREAM, 0, Direction::Forward)
        .expect("read the real store's stream")
        .into_iter()
        .filter(|e| e.type_ == "DecisionMade")
        .map(|e| String::from_utf8_lossy(&e.data).into_owned())
        .collect()
}

/// Spec 89, criterion 2: a real `rigger emit`, run from inside a git-linked worktree that
/// lives WHOLLY OUTSIDE the repo's own directory tree (`elsewhere/rigger-wt-x`, never a
/// descendant of `root` - the shape a real spawn's worktree takes once the relocated scratch
/// default is in effect), must still resolve and write to `root`'s real store: never refuse
/// with "no rigger store found", and never fabricate a stray store inside the worktree itself.
///
/// Non-vacuous against the real binary: before this criterion's fix, `find_store_dir_from`'s
/// plain `.parent()` climb from `start` never physically passes through `root` for a worktree
/// living outside it, so this test fails closed (emit refuses) on the pre-fix code path - it
/// is not merely re-asserting a tautology the current binary always satisfies.
#[test]
fn rigger_emit_from_a_relocated_worktree_resolves_the_owning_repos_real_store() {
    let dir = tempfile::tempdir().expect("create the fixture repo dir");
    let root = dir.path();
    git_init_committed(root);
    seed_store(root);

    let elsewhere = tempfile::tempdir().expect("create the relocated-scratch sibling dir");
    let worktree = elsewhere.path().join("rigger-wt-x");
    add_relocated_worktree(root, &worktree, "rigger/u/x");

    let marker = "probe-outside-the-repo-tree";
    let (stdout, stderr, ok) = run_rigger(
        &worktree,
        &[
            "emit",
            "DecisionMade",
            &format!(r#"{{"id":"{marker}","summary":"s"}}"#),
        ],
    );
    assert!(
        ok,
        "rigger emit from a relocated (outside-the-repo) worktree must succeed; stderr: {stderr}"
    );
    assert!(
        stdout.contains("emitted DecisionMade"),
        "must report the emit landing, not a silent no-op; stdout: {stdout:?}"
    );

    assert!(
        !worktree.join(".rigger").join("events.db").is_file(),
        "the relocated worktree must never grow its OWN stray store - the write must land on \
         the real repo's store, never fabricate a local one at the worktree"
    );
    let decisions = decision_summaries_in(root);
    assert!(
        decisions.iter().any(|d| d.contains(marker)),
        "the emit from the relocated worktree must land in the REAL repo's own store at \
         {root:?}; DecisionMade payloads found there: {decisions:?}"
    );
}

/// A graph read run from a relocated worktree - `rigger graph --show` and `rigger graph --around`,
/// the lookups every spawned agent is told to run from its worktree - answers from the OWNING
/// repository's graph, and never opens a graph of its own in the worktree: a worktree carries no
/// graph, so a read rooted there answers every lookup empty and the agent loses the graph.
#[test]
fn rigger_graph_show_and_around_from_a_relocated_worktree_answer_from_the_owning_repos_graph() {
    let dir = tempfile::tempdir().expect("create the fixture repo dir");
    let root = dir.path();
    let elsewhere = tempfile::tempdir().expect("create the relocated-scratch sibling dir");
    let worktree = elsewhere.path().join("rigger-wt-g");
    owning_graph_with_relocated_worktree(root, &worktree, "rigger/u/g");

    let (show, stderr, ok) = run_rigger(&worktree, &["graph", "--show", "alpha"]);
    assert!(
        ok,
        "graph --show from the worktree must succeed; stderr: {stderr}"
    );
    assert!(
        show.contains("show a.rs::alpha"),
        "graph --show from the worktree must find the entity in the owning graph; got:\n{show}"
    );
    let (around, stderr, ok) = run_rigger(&worktree, &["graph", "--around", "a.rs"]);
    assert!(
        ok,
        "graph --around from the worktree must succeed; stderr: {stderr}"
    );
    assert!(
        around.contains("a.rs::alpha"),
        "graph --around from the worktree must answer the owning graph's neighbourhood; got:\n{around}"
    );
    assert!(
        !worktree.join(".rigger").join("graph.db").exists(),
        "a graph read from the worktree must never open a graph.db of its own there"
    );
}

/// The spawn MCP server a spawn's host starts in its worktree (`rigger mcp --spawn <id>`) serves
/// `rigger_graph` from the OWNING repository's graph, and folds an emit it serves into that same
/// graph - never a graph.db of its own in the worktree, which would answer every lookup empty and
/// swallow every fold.
#[test]
fn rigger_mcp_spawn_from_a_relocated_worktree_serves_and_folds_into_the_owning_repos_graph() {
    let dir = tempfile::tempdir().expect("create the fixture repo dir");
    let root = dir.path();
    let elsewhere = tempfile::tempdir().expect("create the relocated-scratch sibling dir");
    let worktree = elsewhere.path().join("rigger-wt-m");
    owning_graph_with_relocated_worktree(root, &worktree, "rigger/u/m");

    let mut mcp = McpSession::start_with(&worktree, &["mcp", "--spawn", "u/implementer#0"]);
    let show = graph_tool(&mut mcp, serde_json::json!({"show": "alpha"}));
    assert_eq!(
        show["site"]["id"], "a.rs::alpha",
        "rigger_graph show from the worktree must find the owning graph's entity; got:\n{show}"
    );
    let around = graph_tool(&mut mcp, serde_json::json!({"around": "a.rs"}));
    assert!(
        node_ids(&around).contains(&"a.rs::alpha".to_string()),
        "rigger_graph around from the worktree must answer the owning graph's neighbourhood; \
         got:\n{around}"
    );
    let emitted = mcp.call(
        "tools/call",
        serde_json::json!({"name": "rigger_emit", "arguments": {"type": "DecisionMade",
            "data": {"id": "d-probe", "summary": "s", "governs": ["a.rs"]}}}),
    );
    assert_eq!(
        emitted["result"]["structuredContent"]["folded"], true,
        "the spawn server's emit must fold; got:\n{emitted}"
    );
    let _ = mcp.finish();

    let (owning, stderr, ok) = run_rigger(root, &["graph", "--around", "a.rs"]);
    assert!(
        ok,
        "graph --around at the owning root must succeed; stderr: {stderr}"
    );
    assert!(
        owning.contains("d-probe"),
        "the spawn server's emit must be folded into the owning graph; got:\n{owning}"
    );
    assert!(
        !worktree.join(".rigger").join("graph.db").exists(),
        "the spawn server must never open a graph.db of its own in the worktree"
    );
}

/// Spec 89, criterion 2: when a git-linked worktree lives outside the repo tree AND a FOREIGN
/// project's store sits at one of the worktree's own filesystem ancestors (an unrelated
/// project, or a leftover fixture, parked under the same cache-home mount the relocated
/// default now shares with every OTHER project on the machine), a real `rigger emit` run from
/// inside that worktree must still resolve the REAL owning repo's store - never silently climb
/// into and bind the foreign ancestor's, the exact cross-project escape
/// `adv9-walkup-cross-project` (cited by this criterion's own production doc comment) names.
///
/// Non-vacuous against the real binary: an unbounded ancestor climb from a relocated worktree
/// (the naive fix) would find the FOREIGN store first (nearer to `worktree` than `root`) and
/// bind to it instead - this test's negative assertion on the foreign store fails on that
/// shape, distinguishing the real fix from the naive one, not merely from no fix at all.
#[test]
fn rigger_emit_from_a_relocated_worktree_never_climbs_into_a_foreign_ancestors_store() {
    let dir = tempfile::tempdir().expect("create the fixture repo dir");
    let root = dir.path();
    git_init_committed(root);
    seed_store(root);

    // A FOREIGN store sitting at an ancestor of the relocated worktree - the exact shape a
    // plain unbounded climb would wrongly bind to (mirrors the implementer's own
    // `find_store_dir_from_never_climbs_a_relocated_worktrees_own_unrelated_ancestors_into_a_
    // foreign_store` fixture shape).
    let elsewhere = tempfile::tempdir().expect("create the foreign-ancestor sibling dir");
    seed_store(elsewhere.path());
    let worktree = elsewhere.path().join("nested").join("rigger-wt-y");
    std::fs::create_dir_all(worktree.parent().unwrap()).expect("create the worktree's parent");
    add_relocated_worktree(root, &worktree, "rigger/u/y");

    let marker = "probe-past-a-foreign-ancestor";
    let (_stdout, stderr, ok) = run_rigger(
        &worktree,
        &[
            "emit",
            "DecisionMade",
            &format!(r#"{{"id":"{marker}","summary":"s"}}"#),
        ],
    );
    assert!(
        ok,
        "rigger emit must succeed, resolving the real owning repo past the foreign ancestor; \
         stderr: {stderr}"
    );

    let foreign_decisions = decision_summaries_in(elsewhere.path());
    assert!(
        foreign_decisions.is_empty(),
        "the foreign ancestor's store must receive NOTHING from this emit - a climb that binds \
         it would be the exact adv9-walkup-cross-project hazard reopened; found: \
         {foreign_decisions:?}"
    );
    let real_decisions = decision_summaries_in(root);
    assert!(
        real_decisions.iter().any(|d| d.contains(marker)),
        "the emit must land in the REAL owning repo's store at {root:?}, never the foreign \
         ancestor's; DecisionMade payloads found there: {real_decisions:?}"
    );
}
