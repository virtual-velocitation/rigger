//! Periphery (API / integration) tests for spec 92 criterion 2, THE WHOLE PRODUCT IS COVERED:
//! the workflow-definition indexer (`src/grounder/workflowdef.rs`, new pub `extract_events` /
//! `project_events` / `project_batches`, plus new pub `KIND_STAGE` / `REL_NEEDS` / `REL_RUNS` /
//! `REL_REVIEWS`) and the JavaScript plain-constant tags fix (`src/grounder/symbols/registry.rs`).
//!
//! Every existing test of this criterion's new code (`workflowdef::tests`, the new
//! `contextgraph::sqlite::tests::workflow_definition_events_fold_into_...` fold test, and
//! `registry.rs`'s / `grounder.rs`'s new JS-constant tests) calls the crate's INTERNAL functions
//! directly - `extract()`, `p.subgraph(...)` after hand-built events, `Symbols::open(..).ground(..)`.
//! None of them drive the SHIPPED `rigger` binary, so none of them can see:
//!
//! 1. That `extract_events` / `project_events` / `project_batches` - now PUBLIC API reachable as
//!    `rigger::grounder::workflowdef::...` - actually get WIRED to the cold `rigger graph build`
//!    path (`cmd_graph_build` -> `ingest::ingest_project_batched` -> the shared `walk_batches`,
//!    src/ingest.rs) and to `rigger graph --show` / `--around` (`Projector::locate` / `subgraph`,
//!    src/contextgraph/sqlite.rs) reading the SAME `graph.db` the build wrote - an in-process call
//!    to `extract_events` proves the pure computation, never that the CLI a reader actually runs
//!    reaches it.
//! 2. That the fold arm's new `KIND_STAGE`/`KIND_GATE`/`KIND_AGENT` and
//!    `REL_NEEDS`/`REL_RUNS`/`REL_REVIEWS` match arms (contextgraph/sqlite.rs `fold()`) apply to
//!    events MINTED BY THE REAL EXTRACTION PASS off a REAL `.rigger/workflow.yml` file - the
//!    existing fold test constructs `DocConceptExtracted`/`DocLinkExtracted` events by hand, which
//!    cannot prove `workflowdef::extract` ever emits a payload the fold arm actually accepts.
//! 3. That the workflow-definition half and the JavaScript half ride the SAME shared walk
//!    together, through the ONE wired entry point an operator or the conductor actually calls -
//!    the existing `ingest.rs` unit test proves the wiring in-process; this proves it through the
//!    binary.
//! 4. That the review-panel fallback rule, now extracted to `config::Workflow::effective_review_panel`
//!    (`pub(crate)`, shared by a live run's `conductor::RunCtx::effective_review_panel` and this
//!    indexer's `workflowdef::reviewers_of`), produces the CORRECT `REVIEWS` edges - and only those
//!    edges - end to end off a real YAML file, including the negative case (a gate-less,
//!    review-less stage gets no edge at all).
//! 5. That a plain top-level JS/`.mjs` constant - the exact `docs/audit/2026-09-graph-vs-grep.md`
//!    questions 5 and 8 gap - surfaces as a `kind constant` entity through `rigger graph --show`
//!    AND appears as a node under `rigger graph --around <file>` (the neighborhood surface a reader
//!    actually uses to answer "what does this file define"), and that a function-valued const is
//!    never double-tagged, through the SAME shared JS tags query the code-entity extraction (not
//!    just the `ground` index) now uses.
//! 6. That the CONSTRAINTS WALK's degraded-parse promise (specs/92, line 62-63: "a JavaScript file
//!    with syntax the grammar cannot parse - indexed as far as the parse reaches, with a `partial`
//!    marker on the file node, never dropped silently") holds through a REAL cold `rigger graph
//!    build` - review round 3's adversary caught this as entirely UNIMPLEMENTED
//!    (`adv-u2c2-partial-marker-unimplemented`) and round 4 closed it at the data-model/fold layer
//!    (`FileSymbols::partial`, threaded through `CodeEntityExtracted`/`EdgeInferred`, stamped onto
//!    the file node's `partial` attr by `contextgraph::sqlite`'s fold). Round 4's own tests
//!    (`extract.rs`, `events.rs`) prove this by calling `extract()` / `build_index()` /
//!    `index_events()` directly and folding into an in-memory `Projector::open(":memory:", ..)` -
//!    never through the CLI's actual cold-build entry point (`cmd_graph_build`) writing the REAL
//!    persisted `graph.db` a reader's `--around` then queries. Proven here instead: a real
//!    malformed `workflows/*.js` file run through a real `rigger graph build`, read back from the
//!    SAME persisted store via the crate's own public `Projection::subgraph` - the identical read
//!    `--around` performs - carries the marker; a well-formed sibling never does; and the malformed
//!    file's well-formed prefix is still indexed, never dropped.
//!
//! Scope is strictly criterion 2 (the JavaScript and definition indexers). Criterion 1 (reindex
//! freshness / the lag advisory), criterion 3 (`ground`'s ranking) and criterion 4 (`rigger setup`
//! / the lookup hook / the shipped skill) are owned by sibling units and are deliberately not
//! exercised here.

#[cfg(feature = "symbols")]
use rigger::contextgraph::sqlite::Projector;
#[cfg(feature = "symbols")]
use rigger::contextgraph::{Projection, KIND_CODE_ENTITY, KIND_FILE};
#[cfg(feature = "symbols")]
use std::path::Path;
#[cfg(feature = "symbols")]
use std::process::Command;

// The compiled `rigger` binary under test is located at RUNTIME by the shared authority in
// `tests/common`: a path baked in at compile time goes stale the moment the target dir moves, and
// every suite that spawns the product then dies with a bare NotFound.
mod common;

/// A throwaway project dir that is its own git repo, so `cmd_graph_build`'s root resolution (the
/// git top-level) and `project_identity()` (which scopes the graph read `--show`/`--around` use)
/// are both stable and match between the seed and the binary's own reads.
#[cfg(feature = "symbols")]
fn temp_project() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let _ = Command::new("git")
        .args(["init", "-q"])
        .current_dir(dir.path())
        .status();
    dir
}

/// Run `rigger <args...>` in `cwd` through the COMPILED binary, returning (stdout, stderr,
/// success). Opts out of the auto-started dashboard and points the instance registry at a
/// throwaway state dir, exactly as the other CLI integration tests do, so a short-lived
/// invocation spawns nothing that outlives the test.
#[cfg(feature = "symbols")]
fn run_rigger(cwd: &Path, args: &[&str]) -> (String, String, bool) {
    let state = tempfile::tempdir().expect("a temp XDG_STATE_HOME for the rigger invocation");
    let out = Command::new(common::rigger_bin())
        .args(args)
        .current_dir(cwd)
        .env("RIGGER_NO_DASH", "1")
        .env("XDG_STATE_HOME", state.path())
        .output()
        .expect("failed to spawn the rigger binary");
    (
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
        out.status.success(),
    )
}

/// The project identity `cmd_graph_build`/`cmd_graph`'s `--show`/`--around` scope their graph read
/// under (the basename of the git top-level; `project_identity_at` in `src/main.rs`, not itself
/// exported) - a fresh `temp_project()` mints no `.rigger/project.id`, so this is the pre-spec-09
/// legacy basename identity both the CLI and this direct-open read must agree on for item 6's
/// `Projector::open` to see the SAME store the CLI just wrote.
#[cfg(feature = "symbols")]
fn project_identity_of(root: &Path) -> String {
    let toplevel = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["rev-parse", "--show-toplevel"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .filter(|s| !s.is_empty());
    let base = toplevel.as_deref().map(Path::new).unwrap_or(root);
    base.file_name()
        .and_then(|n| n.to_str())
        .filter(|s| !s.is_empty())
        .map(String::from)
        .unwrap_or_else(|| "rigger".to_string())
}

/// How many events `rigger graph build` reported ingesting, parsed from the line it prints - the
/// shipped observable for "this build did something".
#[cfg(feature = "symbols")]
fn ingested_count(stdout: &str) -> usize {
    stdout
        .split_once("ingested ")
        .and_then(|(_, rest)| rest.split_whitespace().next())
        .and_then(|n| n.parse().ok())
        .unwrap_or_else(|| panic!("graph build must report its ingested count; got:\n{stdout}"))
}

/// Write a `.rigger/workflow.yml` mirroring the Design text's own example (`stage:implement`,
/// `gate:mutation`, `agent:rust-engineer`) closely enough to exercise every relation source this
/// criterion owns: `plan` (agent only, no gates - proves NO wrongful REVIEWS fallback),
/// `plan-critique` (no agent, direct `adversary`/`adjudicator` - REVIEWS from those two ALONE),
/// `implement` (agent + a gate, no direct review fields - REVIEWS falls back to
/// `defaults.review`), and `checkin` (needs implement, its own two gates).
#[cfg(feature = "symbols")]
fn seed_workflow_yml(root: &Path) {
    std::fs::create_dir_all(root.join(".rigger")).unwrap();
    std::fs::write(
        root.join(".rigger").join("workflow.yml"),
        "defaults:\n\
         \x20\x20review:\n\
         \x20\x20\x20\x20lenses: [architecture-reviewer, sdet]\n\
         \x20\x20\x20\x20adversary: adversary\n\
         \x20\x20\x20\x20adjudicator: adjudicator\n\
         \n\
         gates:\n\
         \x20\x20fmt:\n\
         \x20\x20\x20\x20run: \"cargo fmt --check\"\n\
         \x20\x20mutation:\n\
         \x20\x20\x20\x20run: \"cargo mutants\"\n\
         \n\
         stages:\n\
         \x20\x20plan:\n\
         \x20\x20\x20\x20agent: planner\n\
         \x20\x20plan-critique:\n\
         \x20\x20\x20\x20needs: [plan]\n\
         \x20\x20\x20\x20adversary: adversary\n\
         \x20\x20\x20\x20adjudicator: adjudicator\n\
         \x20\x20implement:\n\
         \x20\x20\x20\x20needs: [plan-critique]\n\
         \x20\x20\x20\x20agent: rust-engineer\n\
         \x20\x20\x20\x20gates: [fmt]\n\
         \x20\x20checkin:\n\
         \x20\x20\x20\x20needs: [implement]\n\
         \x20\x20\x20\x20agent: rust-engineer\n\
         \x20\x20\x20\x20gates: [fmt, mutation]\n",
    )
    .unwrap();
}

/// Write `workflows/rigger.js` with two plain top-level constants shaped exactly like the audit's
/// own examples (a ternary and a boolean coercion - neither is an arrow/function expression, the
/// upstream `tags.scm` shape) plus an arrow-valued const, so the double-tag guard is provable
/// through the CLI too; and `shim/relay.mjs` with a plain constant, proving the `.mjs` extension is
/// wired through the identical shared tags query (the Design text's other named path).
#[cfg(feature = "symbols")]
fn seed_js_files(root: &Path) {
    std::fs::create_dir_all(root.join("workflows")).unwrap();
    std::fs::write(
        root.join("workflows").join("rigger.js"),
        "const OUTER_WALL_CLOCK_SEC = Number(process.env.TIMEOUT) > 0 ? Number(process.env.TIMEOUT) : 900\n\
         const FRESH = !!process.env.IS_FRESH\n\
         const greet = () => 1\n",
    )
    .unwrap();
    std::fs::create_dir_all(root.join("shim")).unwrap();
    std::fs::write(
        root.join("shim").join("relay.mjs"),
        "const RELAY_TIMEOUT_MS = 30 * 1000\n",
    )
    .unwrap();
}

/// WIRING, at the shared-walk seam (`src/ingest.rs` `walk_batches` calling
/// `grounder::workflowdef::project_batches`, alongside the code half's JS extraction) and at the
/// CLI entry point (`cmd_graph_build`) both this criterion's halves are reached through: ONE
/// `rigger graph build` over a project carrying BOTH a real `.rigger/workflow.yml` and a real
/// `workflows/rigger.js` must make BOTH a workflow-definition entity and a JS-constant entity
/// resolvable afterward - proving the whole product is covered in one pass, not two accidentally
/// disjoint indexers a reader would have to invoke separately.
#[cfg(feature = "symbols")]
#[test]
fn graph_build_wires_the_workflow_definition_and_javascript_indexers_into_one_shared_walk() {
    let dir = temp_project();
    let root = dir.path();
    seed_workflow_yml(root);
    seed_js_files(root);

    let (out, err, ok) = run_rigger(root, &["graph", "build"]);
    assert!(ok, "graph build must succeed; stderr: {err}; stdout: {out}");
    assert!(
        ingested_count(&out) > 0,
        "sanity: a project with a real workflow.yml and a real .js file must ingest something; \
         got:\n{out}"
    );

    let (stage_out, stage_err, stage_ok) =
        run_rigger(root, &["graph", "--show", "stage:implement"]);
    assert!(
        stage_ok,
        "graph --show stage:implement must succeed; stderr: {stage_err}"
    );
    assert!(
        stage_out.contains("kind stage"),
        "the workflow-definition half must be resolvable after ONE shared-walk build; got:\n{stage_out}"
    );

    let (js_out, js_err, js_ok) = run_rigger(root, &["graph", "--show", "OUTER_WALL_CLOCK_SEC"]);
    assert!(
        js_ok,
        "graph --show OUTER_WALL_CLOCK_SEC must succeed; stderr: {js_err}"
    );
    assert!(
        js_out.contains("kind constant"),
        "the JavaScript half must ALSO be resolvable after the SAME shared-walk build (the two \
         indexers must not silently exclude one another); got:\n{js_out}"
    );
}

/// API + FOLD, off a REAL `.rigger/workflow.yml`: `rigger graph --show`/`--around`, reading the
/// `graph.db` a real `rigger graph build` wrote, must expose `stage:implement`/`gate:mutation`/
/// `agent:rust-engineer` - the Design text's own example triple - at their documented kinds, and
/// the `NEEDS`/`RUNS` edges must match the YAML's `needs:`/`gates:`/`agent:` lists exactly.
#[cfg(feature = "symbols")]
#[test]
fn graph_show_and_around_expose_stage_gate_and_agent_entities_with_needs_and_runs_edges_from_a_real_workflow_yml(
) {
    let dir = temp_project();
    let root = dir.path();
    seed_workflow_yml(root);

    let (build_out, build_err, build_ok) = run_rigger(root, &["graph", "build"]);
    assert!(
        build_ok,
        "graph build must succeed; stderr: {build_err}; stdout: {build_out}"
    );

    for (query, want_kind) in [
        ("stage:implement", "kind stage"),
        ("stage:checkin", "kind stage"),
        ("stage:plan", "kind stage"),
        ("gate:mutation", "kind gate"),
        ("gate:fmt", "kind gate"),
        ("agent:rust-engineer", "kind agent"),
        ("agent:planner", "kind agent"),
    ] {
        let (out, err, ok) = run_rigger(root, &["graph", "--show", query]);
        assert!(ok, "graph --show {query} must succeed; stderr: {err}");
        assert!(
            out.contains(want_kind),
            "graph --show {query} must report {want_kind:?}; got:\n{out}"
        );
    }

    let (around, err, ok) = run_rigger(
        root,
        &["graph", "--around", "stage:implement", "--depth", "1"],
    );
    assert!(
        ok,
        "graph --around stage:implement must succeed; stderr: {err}"
    );
    for edge in [
        "edge stage:implement -NEEDS-> stage:plan-critique",
        "edge stage:implement -RUNS-> gate:fmt",
        "edge stage:implement -RUNS-> agent:rust-engineer",
    ] {
        assert!(
            around.contains(edge),
            "the NEEDS/RUNS edges must match implement's own needs/gates/agent YAML fields \
             exactly; expected {edge:?} in:\n{around}"
        );
    }

    let (checkin_around, err2, ok2) = run_rigger(
        root,
        &["graph", "--around", "stage:checkin", "--depth", "1"],
    );
    assert!(
        ok2,
        "graph --around stage:checkin must succeed; stderr: {err2}"
    );
    assert!(
        checkin_around.contains("edge stage:checkin -NEEDS-> stage:implement")
            && checkin_around.contains("edge stage:checkin -RUNS-> gate:fmt")
            && checkin_around.contains("edge stage:checkin -RUNS-> gate:mutation"),
        "checkin's own needs/gates YAML fields must all appear as edges; got:\n{checkin_around}"
    );
}

/// REVIEWS fallback rule, off a REAL `.rigger/workflow.yml`, end to end through the CLI: a stage
/// naming its OWN `adversary`/`adjudicator` (`plan-critique`) gets REVIEWS from exactly those two,
/// NEVER the workflow-wide panel; a gated stage naming NEITHER (`implement`, `checkin`) falls back
/// to `defaults.review` (its lenses AND its adversary/adjudicator); and a gate-less, review-less
/// stage (`plan`) gets NO REVIEWS edge at all - the false-positive `workflowdef::reviewers_of`'s
/// own doc names as the reason for the `&& !stage.gates.is_empty()` guard. This exercises
/// `config::Workflow::effective_review_panel` - extracted specifically so the live run and this
/// indexer never disagree - through the indexer's real caller, not a hand-built `Workflow`.
#[cfg(feature = "symbols")]
#[test]
fn graph_around_reflects_the_review_panel_fallback_rule_for_a_real_workflow_yml() {
    let dir = temp_project();
    let root = dir.path();
    seed_workflow_yml(root);

    let (build_out, build_err, build_ok) = run_rigger(root, &["graph", "build"]);
    assert!(
        build_ok,
        "graph build must succeed; stderr: {build_err}; stdout: {build_out}"
    );

    // plan-critique: its OWN adversary/adjudicator, and ONLY those - never the workflow panel's
    // lenses, which it never named.
    let (pc, err, ok) = run_rigger(
        root,
        &["graph", "--around", "stage:plan-critique", "--depth", "1"],
    );
    assert!(
        ok,
        "graph --around stage:plan-critique must succeed; stderr: {err}"
    );
    assert!(
        pc.contains("edge agent:adversary -REVIEWS-> stage:plan-critique")
            && pc.contains("edge agent:adjudicator -REVIEWS-> stage:plan-critique"),
        "plan-critique's own adversary/adjudicator fields must produce REVIEWS edges; got:\n{pc}"
    );
    assert!(
        !pc.contains("agent:architecture-reviewer") && !pc.contains("agent:sdet"),
        "a stage naming its own adversary/adjudicator must NOT also inherit the workflow-wide \
         review panel's lenses; got:\n{pc}"
    );

    // implement: no adversary/adjudicator/review of its own, but it DOES run a gate - falls back
    // to defaults.review in full (lenses AND adversary AND adjudicator).
    let (imp, err2, ok2) = run_rigger(
        root,
        &["graph", "--around", "stage:implement", "--depth", "1"],
    );
    assert!(
        ok2,
        "graph --around stage:implement must succeed; stderr: {err2}"
    );
    for reviewer_edge in [
        "edge agent:architecture-reviewer -REVIEWS-> stage:implement",
        "edge agent:sdet -REVIEWS-> stage:implement",
        "edge agent:adversary -REVIEWS-> stage:implement",
        "edge agent:adjudicator -REVIEWS-> stage:implement",
    ] {
        assert!(
            imp.contains(reviewer_edge),
            "a gated stage declaring no review fields of its own must inherit the FULL \
             defaults.review panel; expected {reviewer_edge:?} in:\n{imp}"
        );
    }

    // plan: no review fields AND no gates - must get NO REVIEWS edge at all (the false-positive
    // this guard exists to prevent: a stage with nothing to review must not silently inherit the
    // workflow panel just because SOME other stage happens to).
    let (plan, err3, ok3) = run_rigger(root, &["graph", "--around", "stage:plan", "--depth", "1"]);
    assert!(
        ok3,
        "graph --around stage:plan must succeed; stderr: {err3}"
    );
    assert!(
        plan.contains("edge stage:plan -RUNS-> agent:planner"),
        "plan must still RUN its own assigned agent; got:\n{plan}"
    );
    assert!(
        !plan.contains("-REVIEWS-> stage:plan"),
        "a gate-less, review-less stage must get NO REVIEWS edge at all (not even from \
         defaults.review); got:\n{plan}"
    );
}

/// API + FOLD, off REAL `workflows/rigger.js` and `shim/relay.mjs` files: a plain top-level
/// constant (never an arrow/function expression - the ONLY shape the upstream `tags.scm` tagged
/// before this criterion) must surface as a `kind constant` entity through `rigger graph --show`
/// AND as a node under `rigger graph --around <file>` - the neighborhood surface a reader actually
/// uses to answer "what does this file define" (`docs/audit/2026-09-graph-vs-grep.md` questions 5
/// and 8's own framing). An arrow-valued const must still resolve as `kind function`, never ALSO
/// as a constant (no double-tagging), through the same CLI surface.
#[cfg(feature = "symbols")]
#[test]
fn graph_show_and_around_expose_plain_top_level_js_and_mjs_constants_without_double_tagging_functions(
) {
    let dir = temp_project();
    let root = dir.path();
    seed_js_files(root);

    let (build_out, build_err, build_ok) = run_rigger(root, &["graph", "build"]);
    assert!(
        build_ok,
        "graph build must succeed; stderr: {build_err}; stdout: {build_out}"
    );
    assert!(
        ingested_count(&build_out) > 0,
        "sanity: the JS/mjs fixtures must actually get ingested; got:\n{build_out}"
    );

    // Q8: OUTER_WALL_CLOCK_SEC (a ternary-valued constant).
    let (outer, err, ok) = run_rigger(root, &["graph", "--show", "OUTER_WALL_CLOCK_SEC"]);
    assert!(
        ok,
        "graph --show OUTER_WALL_CLOCK_SEC must succeed; stderr: {err}"
    );
    assert!(
        outer.contains("kind constant") && outer.contains("workflows/rigger.js"),
        "a ternary-valued top-level const must ground to workflows/rigger.js as kind constant; \
         got:\n{outer}"
    );

    // Q5: FRESH (a boolean-coercion-valued constant).
    let (fresh, err2, ok2) = run_rigger(root, &["graph", "--show", "FRESH"]);
    assert!(ok2, "graph --show FRESH must succeed; stderr: {err2}");
    assert!(
        fresh.contains("kind constant"),
        "a boolean-coercion-valued top-level const must be kind constant; got:\n{fresh}"
    );

    // The .mjs extension: wired through the identical shared tags query.
    let (relay, err3, ok3) = run_rigger(root, &["graph", "--show", "RELAY_TIMEOUT_MS"]);
    assert!(
        ok3,
        "graph --show RELAY_TIMEOUT_MS must succeed; stderr: {err3}"
    );
    assert!(
        relay.contains("kind constant") && relay.contains("shim/relay.mjs"),
        "a plain top-level const in a .mjs file must ALSO ground as kind constant (the shared \
         tags query, not a .js-only special case); got:\n{relay}"
    );

    // No double-tag: an arrow-valued const is still kind function, never ALSO a constant.
    let (greet, err4, ok4) = run_rigger(root, &["graph", "--show", "greet"]);
    assert!(ok4, "graph --show greet must succeed; stderr: {err4}");
    assert!(
        greet.contains("kind function"),
        "an arrow-valued const must extract as kind function through the CLI too; got:\n{greet}"
    );
    assert!(
        !greet.contains("candidates"),
        "an arrow-valued const must resolve to exactly ONE entity, never a second (ambiguous) \
         constant-kind definition of the same name; got:\n{greet}"
    );

    // The neighborhood surface: both constants appear as NODES when a reader asks what
    // workflows/rigger.js defines - not merely findable by name in isolation.
    let (around, err5, ok5) = run_rigger(
        root,
        &["graph", "--around", "workflows/rigger.js", "--depth", "1"],
    );
    assert!(
        ok5,
        "graph --around workflows/rigger.js must succeed; stderr: {err5}"
    );
    assert!(
        around.contains("workflows/rigger.js::OUTER_WALL_CLOCK_SEC")
            && around.contains("workflows/rigger.js::FRESH"),
        "both plain top-level constants must appear as nodes in the file's own neighborhood, \
         answering \"what does this file define\" directly - the exact gap questions 5 and 8 of \
         the audit measured (\"JS not indexed\"); got:\n{around}"
    );
}

/// CONSTRAINTS WALK, through the REAL pipeline end to end (item 6 above): a JavaScript file the
/// grammar cannot fully parse must still be indexed as far as the parse reaches, with a `partial`
/// marker stamped on its file node - proven here off a REAL `rigger graph build` (not a hand-built
/// event fed to an in-memory store), reading back the SAME persisted `graph.db` the CLI wrote
/// through the crate's own public `Projection::subgraph`, the identical read `--around` performs.
/// A well-formed sibling file must never carry the marker, and the malformed file's well-formed
/// prefix (a function ahead of its syntax error) must still be indexed, never dropped silently.
#[cfg(feature = "symbols")]
#[test]
fn a_real_malformed_js_file_carries_the_partial_marker_through_a_real_graph_build_and_a_well_formed_one_never_does(
) {
    let dir = temp_project();
    let root = dir.path();
    std::fs::create_dir_all(root.join("workflows")).unwrap();
    std::fs::write(
        root.join("workflows").join("broken.js"),
        "function greet() { return 1; }\nfunction broken(\n",
    )
    .unwrap();
    std::fs::write(
        root.join("workflows").join("whole.js"),
        "function greet2() { return 2; }\n",
    )
    .unwrap();

    let (out, err, ok) = run_rigger(root, &["graph", "build"]);
    assert!(
        ok,
        "graph build must succeed even over a malformed JS file; stderr: {err}; stdout: {out}"
    );
    assert!(
        ingested_count(&out) > 0,
        "sanity: the malformed and well-formed fixtures must actually get ingested; got:\n{out}"
    );

    let id = project_identity_of(root);
    let gp = Projector::open(root.join(".rigger").join("graph.db").to_str().unwrap(), &id)
        .expect("the real graph.db a real `rigger graph build` just wrote must open");
    let g = gp
        .subgraph(
            &[
                "workflows/broken.js".to_string(),
                "workflows/whole.js".to_string(),
            ],
            1,
        )
        .expect("a subgraph read over the real persisted store must succeed");

    let broken = g
        .nodes
        .iter()
        .find(|n| n.kind == KIND_FILE && n.id == "workflows/broken.js")
        .expect(
            "the malformed file must still get a KIND_FILE container node - indexed as far as \
             the parse reaches, never dropped entirely",
        );
    assert_eq!(
        broken.attrs.get("partial").map(String::as_str),
        Some("true"),
        "a real malformed JS file run through a REAL `rigger graph build` must carry the partial \
         marker on its file node, read back through the same public Projection::subgraph the \
         CLI's own --around uses; got attrs {:?}",
        broken.attrs
    );
    assert!(
        g.nodes
            .iter()
            .any(|n| n.kind == KIND_CODE_ENTITY && n.id == "workflows/broken.js::greet"),
        "the well-formed function ahead of the parse error must still be indexed through the \
         real pipeline, never dropped silently; got {:?}",
        g.nodes
    );

    let whole = g
        .nodes
        .iter()
        .find(|n| n.kind == KIND_FILE && n.id == "workflows/whole.js")
        .expect("the well-formed file's own container node must fold");
    assert_eq!(
        whole.attrs.get("partial"),
        None,
        "a well-formed JS file run through the real pipeline must never carry the partial \
         marker; got {:?}",
        whole.attrs
    );
}
