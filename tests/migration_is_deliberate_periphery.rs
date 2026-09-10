//! Periphery (integration) tests for spec 86 criterion 3 (THE MIGRATION IS DELIBERATE),
//! independent of both the implementer's own fixtures and the two test paths that reach this
//! criterion's mechanism only from inside the crate.
//!
//! Two boundaries this file closes:
//!
//!  - The emit+fold seam through the crate's PUBLIC API, over fixtures of its own, for ALL THREE
//!    exclusion shapes criterion 1 established and this criterion's retirement mechanism must
//!    reach: `extract_events`'s own two boundary-sentinel return sites (see that function's own
//!    doc in `grounder/symbols/events.rs`) - the whole-`tests/`-dir exclusion (`is_under_tests_dir`,
//!    the CENTRAL migration case the Design names) and the end-of-function "survived filtering
//!    down to nothing" case (an in-file `#[cfg(test)]` re-wrap of a product item) - PLUS the
//!    OUT-OF-LINE `#[cfg(test)] mod name;` shape (round 5,
//!    adj-u86c3-r4-out-of-line-exclusion-still-unmigrated), which is a distinct, CALLER-level
//!    routing fix (`index_events`/`project_batches_paced`'s own `for_extraction` hollowing) rather
//!    than a third branch inside `extract_events` itself - that exclusion set can only be computed
//!    where every file's path in the project is known together, never from one file's own parse.
//!    The implementer's own coverage of the first two shapes is either in-crate with a HAND-BUILT
//!    event payload
//!    (`sqlite.rs::migration_c3::re_ingesting_a_store_that_already_holds_test_entity_nodes_...`,
//!    which never calls `extract_events` at all) or `conductor.rs`'s own
//!    `re_excluding_the_same_file_twice_in_one_process_retires_its_middle_generation`, which does
//!    drive the real filesystem and the real `extract_events`, but only through the PRIVATE
//!    `RunCtx` machinery - so neither proves the crate's PUBLIC `extract_events` ->
//!    `Projector::apply` composition an external caller (or a future refactor of either side
//!    alone) actually holds; the third (out-of-line) shape had NO coverage anywhere reaching the
//!    real fold before this file's own addition below. `index_events` and `project_batches_paced`
//!    are TWO independently-coded call sites for the identical `for_extraction` hollowing (round 5
//!    fixed the same drop-the-batch defect in both, separately), and `project_batches_paced` -
//!    never `index_events` - is the one `src/ingest.rs` actually calls in production, so this
//!    shape's probe runs against BOTH entry points (one shared body, round-1-review addendum
//!    below) rather than resting on one proving the other by inference. Mirrors
//!    `proof_lands_on_the_card_periphery.rs`'s own stated precedent for criterion 2's identical
//!    class of gap.
//!  - `rigger validate`'s RETIRED CODE-ENTITY advisory, driven end to end through the COMPILED
//!    binary (stdout/stderr/exit code, exactly as an operator sees it) - never exercised at the
//!    CLI boundary before this file: the implementer's own coverage
//!    (`main.rs::retired_entities_advisory_for_reads_the_projectors_own_counting_authority` and
//!    its siblings) calls the gathering functions directly, in-process, never spawning
//!    `rigger validate` itself. Mirrors `tests/validate_advisories.rs`'s own convention for spec
//!    68's two advisories (a DIFFERENT file - that one's own doc scopes itself to criterion 4's
//!    index-staleness and log-bloat checks only, so this criterion's advisory gets its own file
//!    rather than widening that one's stated ownership).
//!
//! NOT OWNED here:
//!  - the fold SQL itself (`ensure_node`'s merge, `supersede_file_edges`, the `r.name.is_empty()`
//!    guard's own correctness given a hand-built event sequence) - the implementer's own
//!    `sqlite.rs::migration_c3` tests own that array of cases directly;
//!  - the in-process double-exclusion replay-key collision fix (`RunCtx::replayed_generations` /
//!    `RunCtx::emit_keyed_batch`'s stale-generation retirement): `RunCtx` is a private, non-`pub`
//!    struct with no public constructor anywhere in the crate's public surface, and
//!    `conductor::run`'s only production call to `ingest_project_batches` (src/conductor.rs:7985)
//!    invokes it exactly ONCE per call - so no sequence of PUBLIC API calls can even reach two
//!    ingests sharing one `RunCtx`, the precondition this fix's own bug required. The
//!    implementer's own `conductor.rs::tests::re_excluding_the_same_file_twice_in_one_process_
//!    retires_its_middle_generation` is the only test that CAN reach this state, and it already
//!    drives the real filesystem and the real fold rather than a hand-built sequence;
//!  - whether a superseded row literally persists on disk ("never a store wipe"): no public
//!    `Projection` method exposes a historical/superseded read - `subgraph`'s own doc says it
//!    follows "only currently valid edges" - so this is provable only in-crate, where
//!    `sqlite.rs::migration_c3` already reads the raw superseded edge directly.

mod common;

use std::path::Path;
use std::process::Command;

// =========================================================================================
// Part 1: the emit+fold seam through the PUBLIC API (symbols lane only)
// =========================================================================================

/// `is_under_tests_dir`'s branch: a WHOLE `tests/`-dir file, the Design's own central migration
/// case ("a whole `tests/*.rs` file - the central case"). `keep.rs` is an unrelated, untouched
/// product file present throughout, so its survival independently proves "the fold yields the
/// same product entities before and after".
#[cfg(feature = "symbols")]
#[test]
fn a_whole_tests_dir_files_legacy_entity_is_retired_through_the_real_extract_events_and_fold_seam()
{
    use rigger::contextgraph::sqlite::Projector;
    use rigger::contextgraph::{Projection, TYPE_CODE_ENTITY_EXTRACTED};
    use rigger::eventstore::Event;
    use rigger::grounder::symbols::build_index;
    use rigger::grounder::symbols::events::extract_events;

    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("keep.rs"), "pub fn keep_me() {}\n").unwrap();
    std::fs::create_dir_all(root.path().join("tests")).unwrap();
    std::fs::write(
        root.path().join("tests/legacy.rs"),
        "fn old_test_helper() { assert!(true); }\n",
    )
    .unwrap();

    let p = Projector::open(":memory:", "test").unwrap();
    let mut pos = 1u64;

    // "Before": a legacy node standing in for what a PRE-spec-86 binary's fold would have
    // created for tests/legacy.rs - the state the Design's MIGRATION paragraph names ("existing
    // test-entity nodes"). The CURRENT `extract_events` can never produce this event for a
    // tests/-dir path (criterion 1 already excludes it structurally, unconditionally), so
    // simulating the prior generation directly is the only way to reproduce a store this
    // criterion predates - the same simulation the implementer's own fold-level test uses.
    let legacy = serde_json::json!({
        "file": "tests/legacy.rs", "name": "old_test_helper", "kind": "function",
        "line": 1, "lang": "rust", "fresh": true,
    });
    let mut legacy_event = Event::new(
        TYPE_CODE_ENTITY_EXTRACTED,
        serde_json::to_vec(&legacy).unwrap(),
    );
    legacy_event.position = pos;
    pos += 1;
    p.apply(&legacy_event).unwrap();

    // The unrelated product file's REAL first ingest, through the public pipeline.
    let idx = build_index(root.path().to_str().unwrap(), None);
    let keep_fs = idx
        .files()
        .get("keep.rs")
        .expect("fixture precondition: keep.rs is indexed");
    for mut ev in extract_events("keep.rs", keep_fs) {
        ev.position = pos;
        pos += 1;
        p.apply(&ev).unwrap();
    }

    assert_eq!(
        p.retired_code_entity_count().unwrap(),
        0,
        "precondition: nothing is retired yet - the legacy node is still live"
    );

    // "After": tests/legacy.rs's REAL re-ingest, driven entirely by the crate's PUBLIC
    // `extract_events` - never a hand-built boundary payload - proving the actual
    // `is_under_tests_dir` -> `empty_structural_boundary_event` emit path composes correctly
    // with the fold's `r.name.is_empty()` retirement arm at the public API boundary.
    let legacy_fs = idx
        .files()
        .get("tests/legacy.rs")
        .expect("fixture precondition: tests/legacy.rs is indexed");
    let mut boundary_events = extract_events("tests/legacy.rs", legacy_fs);
    assert_eq!(
        boundary_events.len(),
        1,
        "a whole tests/-dir file's real re-extraction is exactly one boundary sentinel; got \
         {boundary_events:?}"
    );
    for ev in boundary_events.iter_mut() {
        ev.position = pos;
        pos += 1;
        p.apply(ev).unwrap();
    }

    assert_eq!(
        p.retired_code_entity_count().unwrap(),
        1,
        "the real boundary sentinel retires exactly the legacy entity"
    );

    let g = p.subgraph(&["tests/legacy.rs".to_string()], 2).unwrap();
    assert!(
        !g.nodes
            .iter()
            .any(|n| n.id == "tests/legacy.rs::old_test_helper"),
        "the retired legacy entity is no longer live-reachable; nodes: {:?}",
        g.nodes
    );
    // NOT "no file-container node at all": the LEGACY event above already created one (a
    // pre-spec-86 fold of a tests/-dir file made an ordinary KIND_FILE container, exactly like
    // any other file) - and "never a store wipe" means that row legitimately persists. The
    // sibling test below (`a_lone_empty_structural_sentinel_through_the_real_pipeline_creates_
    // nothing_at_all`) proves the boundary SENTINEL ITSELF creates no container when none
    // already existed, which is criterion 1's actual promise.

    // "the fold yields the same product entities before and after": keep.rs::keep_me, never
    // touched by the migration, is still fully live and reachable.
    let kept = p.subgraph(&["keep.rs".to_string()], 2).unwrap();
    assert!(
        kept.nodes.iter().any(|n| n.id == "keep.rs::keep_me"),
        "an unrelated, untouched product entity survives the migration unchanged; nodes: {:?}",
        kept.nodes
    );
}

/// Criterion 1's own "never a node and never an edge on the canvas" promise, re-proven through
/// the REAL `extract_events` -> `Projector::apply` seam criterion 3 changed: a tests/-dir file
/// with NO prior state at all (unlike the sibling test above, which simulates pre-existing
/// legacy history) must fold its lone boundary sentinel into nothing - no file container, no
/// entity node, no edge. The implementer's own version of this exact claim
/// (`sqlite.rs::migration_c3::the_empty_structural_boundary_sentinel_creates_no_node_and_no_edge_
/// of_its_own`) hand-builds the event directly against the fold; this drives the real, public
/// `extract_events` instead.
#[cfg(feature = "symbols")]
#[test]
fn a_lone_empty_structural_sentinel_through_the_real_pipeline_creates_nothing_at_all() {
    use rigger::contextgraph::sqlite::Projector;
    use rigger::contextgraph::Projection;
    use rigger::grounder::symbols::build_index;
    use rigger::grounder::symbols::events::extract_events;

    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(root.path().join("tests")).unwrap();
    std::fs::write(
        root.path().join("tests/fresh.rs"),
        "fn brand_new_test_helper() { assert!(true); }\n",
    )
    .unwrap();

    let idx = build_index(root.path().to_str().unwrap(), None);
    let fs = idx
        .files()
        .get("tests/fresh.rs")
        .expect("fixture precondition: tests/fresh.rs is indexed");
    let mut events = extract_events("tests/fresh.rs", fs);
    assert_eq!(events.len(), 1, "got {events:?}");

    let p = Projector::open(":memory:", "test").unwrap();
    for ev in events.iter_mut() {
        ev.position = 1;
        p.apply(ev).unwrap();
    }

    let g = p.subgraph(&["tests/fresh.rs".to_string()], 1).unwrap();
    assert!(
        g.nodes.is_empty() && g.edges.is_empty(),
        "a lone boundary sentinel on a file with no prior state creates nothing at all through \
         the real pipeline; got {:?} / {:?}",
        g.nodes,
        g.edges
    );
    assert_eq!(p.retired_code_entity_count().unwrap(), 0);
}

/// `extract_events`'s OTHER boundary-sentinel site: the end-of-function push that fires when a
/// file's surviving definition/reference set drops to empty even though the file itself is NOT
/// under a `tests/` directory - here, every item in a product-path file is re-wrapped entirely
/// inside `#[cfg(test)]`. Distinct code path from `is_under_tests_dir`'s early return above, and
/// distinct from the implementer's own `conductor.rs` regression test for this exact scenario
/// (which drives it through the private `RunCtx`, not the crate's public API).
#[cfg(feature = "symbols")]
#[test]
fn a_product_file_rewrapped_entirely_into_cfg_test_retires_its_prior_entity_through_extract_events_own_end_of_function_boundary(
) {
    use rigger::contextgraph::sqlite::Projector;
    use rigger::contextgraph::Projection;
    use rigger::grounder::symbols::build_index;
    use rigger::grounder::symbols::events::extract_events;

    let root = tempfile::tempdir().unwrap();
    let target = root.path().join("wrapped.rs");

    let p = Projector::open(":memory:", "test").unwrap();
    let mut pos = 1u64;

    // Generation 1: real, defines `folded`, through the real pipeline.
    std::fs::write(&target, "pub fn folded() {}\n").unwrap();
    let idx1 = build_index(root.path().to_str().unwrap(), None);
    let fs1 = idx1.files().get("wrapped.rs").expect("wrapped.rs indexed");
    for mut ev in extract_events("wrapped.rs", fs1) {
        ev.position = pos;
        pos += 1;
        p.apply(&ev).unwrap();
    }
    let live = p.subgraph(&["wrapped.rs".to_string()], 2).unwrap();
    assert!(
        live.nodes.iter().any(|n| n.id == "wrapped.rs::folded"),
        "sanity: generation 1 folds live; nodes: {:?}",
        live.nodes
    );
    assert_eq!(p.retired_code_entity_count().unwrap(), 0);

    // Generation 2: the SAME symbol, now wholly inside `#[cfg(test)]` - not a tests/-dir path,
    // so this exercises extract_events's END-OF-FUNCTION empty-survivor push, never the
    // is_under_tests_dir branch the sibling test above covers.
    std::fs::write(
        &target,
        "#[cfg(test)]\nmod hidden {\n    pub fn folded() {}\n}\n",
    )
    .unwrap();
    let idx2 = build_index(root.path().to_str().unwrap(), None);
    let fs2 = idx2.files().get("wrapped.rs").expect("wrapped.rs indexed");
    let mut boundary_events = extract_events("wrapped.rs", fs2);
    assert_eq!(
        boundary_events.len(),
        1,
        "a product file re-wrapped entirely into #[cfg(test)] extracts to exactly one boundary \
         sentinel; got {boundary_events:?}"
    );
    for ev in boundary_events.iter_mut() {
        ev.position = pos;
        pos += 1;
        p.apply(ev).unwrap();
    }

    assert_eq!(
        p.retired_code_entity_count().unwrap(),
        1,
        "the real end-of-function boundary sentinel retires the now-excluded entity"
    );
    let after = p.subgraph(&["wrapped.rs".to_string()], 2).unwrap();
    assert!(
        !after.nodes.iter().any(|n| n.id == "wrapped.rs::folded"),
        "the re-wrapped entity is no longer live-reachable; nodes: {:?}",
        after.nodes
    );
}

/// The THIRD exclusion shape criterion 1 established, and the one this criterion's mechanism
/// missed on the first attempt (round 5, adj-u86c3-r4-out-of-line-exclusion-still-unmigrated): an
/// OUT-OF-LINE `#[cfg(test)] mod name;` declaration. Unlike the two siblings above, this exclusion
/// is computed one layer above `extract_events` itself (only there is every file's path in the
/// project known together, so only there can the DECLARING file's attribute be resolved against
/// its TARGET file) - by TWO independently-coded call sites, `index_events` and
/// `project_batches_paced`, both of which dropped the excluded file's batch entirely before round
/// 5's fix gave each its OWN `for_extraction` hollowing call. Parameterized over `entry` so one
/// body proves the retirement fires through EITHER site rather than one standing in for the other
/// by inference - see the two `#[test]`s below, reproducing the exact probe the adjudicator's own
/// rejection ran by hand (`tests/_adjudicator_probe_out_of_line.rs`, reverted) through this
/// crate's permanent test suite instead.
#[cfg(feature = "symbols")]
fn assert_out_of_line_declaration_retires_through_the_real_fold_seam(
    entry: impl Fn(&str) -> Vec<rigger::eventstore::Event>,
) {
    use rigger::contextgraph::sqlite::Projector;
    use rigger::contextgraph::Projection;

    let root = tempfile::tempdir().unwrap();
    let root_str = root.path().to_str().unwrap();
    let lib_path = root.path().join("lib.rs");
    // Generation 1: `lib.rs` and `contract.rs` are both ordinary product files - `contract.rs` is
    // not yet named by any `mod` declaration at all, so it indexes and extracts as plain product
    // code, exactly like any other file.
    std::fs::write(&lib_path, "pub fn keep_me() {}\n").unwrap();
    std::fs::write(
        root.path().join("contract.rs"),
        "pub fn assert_contract() {}\n",
    )
    .unwrap();

    let p = Projector::open(":memory:", "test").unwrap();
    let mut pos = 1u64;
    for mut ev in entry(root_str) {
        ev.position = pos;
        pos += 1;
        p.apply(&ev).unwrap();
    }
    let before = p
        .subgraph(&["lib.rs".to_string(), "contract.rs".to_string()], 2)
        .unwrap();
    assert!(
        before
            .nodes
            .iter()
            .any(|n| n.id == "contract.rs::assert_contract"),
        "precondition: contract.rs::assert_contract is ordinary, live product code before any \
         out-of-line declaration names it; nodes: {:?}",
        before.nodes
    );
    assert_eq!(p.retired_code_entity_count().unwrap(), 0);

    // Generation 2: `lib.rs` grows an out-of-line `#[cfg(test)] mod contract;` - `contract.rs`
    // itself is untouched (Rust's own module system, not an attribute on the file itself, is what
    // gates this), so a per-file view of `contract.rs` alone could never see the exclusion; only
    // `entry`'s own cross-file `out_of_line_test_module_files` resolution can.
    std::fs::write(
        &lib_path,
        "pub fn keep_me() {}\n\n#[cfg(test)]\nmod contract;\n",
    )
    .unwrap();
    // Deterministic re-ingest through the real PUBLIC pipeline, never a hand-built payload.
    for mut ev in entry(root_str) {
        ev.position = pos;
        pos += 1;
        p.apply(&ev).unwrap();
    }

    assert_eq!(
        p.retired_code_entity_count().unwrap(),
        1,
        "the out-of-line-excluded file's own hollowed re-extraction (for_extraction) retires its \
         prior structural entity through the SAME boundary-sentinel fold arm the other two \
         shapes already use - the exact defect adj-u86c3-r4-out-of-line-exclusion-still-unmigrated \
         found (retired_code_entity_count stayed 0 forever) is closed"
    );
    let after = p
        .subgraph(&["lib.rs".to_string(), "contract.rs".to_string()], 2)
        .unwrap();
    assert!(
        !after
            .nodes
            .iter()
            .any(|n| n.id == "contract.rs::assert_contract"),
        "the out-of-line-excluded entity is no longer live-reachable; nodes: {:?}",
        after.nodes
    );
    assert!(
        after.nodes.iter().any(|n| n.id == "lib.rs::keep_me"),
        "an unrelated, untouched product entity in the DECLARING file survives the migration \
         unchanged; nodes: {:?}",
        after.nodes
    );
}

#[cfg(feature = "symbols")]
#[test]
fn an_out_of_line_cfg_test_mod_declarations_target_retires_through_the_real_index_events_and_fold_seam(
) {
    use rigger::grounder::symbols::build_index;
    use rigger::grounder::symbols::events::index_events;

    assert_out_of_line_declaration_retires_through_the_real_fold_seam(|root| {
        index_events(&build_index(root, None))
    });
}

/// The SAME shape as its `index_events` sibling above, through `project_batches_paced` instead -
/// the site `src/ingest.rs` actually calls in production (`index_events` has no production
/// caller of its own). Round 5 fixed this exact defect in `index_events` and
/// `project_batches_paced` as two SEPARATE edits (each had its own `.filter(|(path, _)|
/// !excluded.contains(path.as_str()))` line dropping the batch before `extract_events` ever ran),
/// so proving the fix through one is not evidence it holds through the other; `workers: 2`
/// exercises the parallel dispatch path (`crate::parallel::map_ordered`), not merely the `workers
/// <= 1` inline fallback - width-invariance across worker counts is `parallel_ordered_emit.rs`'s
/// own, separate, general-purpose guarantee, so this does not re-prove that, only that THIS
/// fixture's retirement holds at a width that actually engages the pool.
#[cfg(feature = "symbols")]
#[test]
fn an_out_of_line_cfg_test_mod_declarations_target_retires_through_the_real_project_batches_paced_and_fold_seam(
) {
    use rigger::grounder::symbols::events::project_batches_paced;

    assert_out_of_line_declaration_retires_through_the_real_fold_seam(|root| {
        project_batches_paced(root, 2)
            .0
            .into_iter()
            .flat_map(|(_, evs)| evs)
            .collect()
    });
}

// =========================================================================================
// Part 2: `rigger validate`'s RETIRED CODE-ENTITY advisory, through the COMPILED binary
// =========================================================================================

/// A throwaway project: its own git repo, so `project_identity()` resolves as it does for a
/// real project (mirrors `tests/validate_advisories.rs`'s own `temp_project` convention).
fn temp_project() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("create temp project");
    let _ = Command::new("git")
        .args(["init", "-q"])
        .current_dir(dir.path())
        .status();
    std::fs::create_dir_all(dir.path().join(".rigger")).expect("create .rigger");
    dir
}

/// The project identity `rigger validate`'s own `project_identity()` resolves for `root`: the
/// tracked `.rigger/project.id` when present (as `rigger init` mints), else the git top-level's
/// basename, else `root`'s own basename. Mirrors `tests/validate_advisories.rs`'s own
/// `run_stream_identity`, needed here so a directly-seeded `graph.db` lands under the SAME
/// project scope the compiled binary will read it back under.
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
    if let Ok(raw) = std::fs::read_to_string(base.join(".rigger").join("project.id")) {
        let id = raw.trim();
        if !id.is_empty() {
            return id.to_string();
        }
    }
    base.file_name()
        .and_then(|n| n.to_str())
        .filter(|s| !s.is_empty())
        .map(String::from)
        .unwrap_or_else(|| "rigger".to_string())
}

/// Run `rigger <args...>` in `cwd`, returning (stdout, stderr, success). Mirrors
/// `tests/validate_advisories.rs`'s own `run_rigger`: the dashboard and the machine-global
/// instance registry are stubbed out so a short-lived invocation never leaves a live process or
/// a phantom registry entry behind.
fn run_rigger(cwd: &Path, args: &[&str]) -> (String, String, bool) {
    let mut cmd = common::rigger_courier();
    cmd.args(args).current_dir(cwd);
    cmd.env("RIGGER_NO_DASH", "1");
    let state = tempfile::tempdir().expect("create a temp XDG_STATE_HOME");
    cmd.env("XDG_STATE_HOME", state.path());
    let out = cmd.output().expect("failed to spawn the rigger binary");
    (
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
        out.status.success(),
    )
}

/// Seed `root`'s `.rigger/graph.db` directly (bypassing the extraction pass entirely, exactly
/// like `tests/validate_advisories.rs`'s own `seed_duplicated_key`/`seed_key_under_two_covered_
/// types` bypass the extraction pass to seed `events.db`): one legacy `CodeEntityExtracted` node
/// standing in for a pre-spec-86 store, then the empty-name `EdgeInferred` boundary sentinel that
/// retires it - the exact two-event shape `sqlite.rs::migration_c3`'s own fold-level test proves
/// correct. `contextgraph` is not feature-gated (unlike `grounder::symbols`), so this seeding
/// runs in BOTH feature lanes, and Part 2's tests below carry no `#[cfg(feature = "symbols")]`.
fn seed_a_retired_entity(root: &Path) {
    use rigger::contextgraph::sqlite::Projector;
    use rigger::contextgraph::{Projection, TYPE_CODE_ENTITY_EXTRACTED, TYPE_EDGE_INFERRED};
    use rigger::eventstore::Event;

    let project = project_identity_of(root);
    let graph_path = root.join(".rigger").join("graph.db");
    std::fs::create_dir_all(graph_path.parent().unwrap()).unwrap();
    let p = Projector::open(graph_path.to_str().unwrap(), &project).unwrap();

    let legacy = serde_json::json!({
        "file": "tests/legacy.rs", "name": "old_test_helper", "kind": "function",
        "line": 1, "lang": "rust", "fresh": true,
    });
    let mut e1 = Event::new(
        TYPE_CODE_ENTITY_EXTRACTED,
        serde_json::to_vec(&legacy).unwrap(),
    );
    e1.position = 1;
    p.apply(&e1).unwrap();

    let boundary = serde_json::json!({
        "file": "tests/legacy.rs", "name": "", "lang": "rust", "fresh": true,
    });
    let mut e2 = Event::new(TYPE_EDGE_INFERRED, serde_json::to_vec(&boundary).unwrap());
    e2.position = 2;
    p.apply(&e2).unwrap();
}

/// Seed `root`'s `.rigger/graph.db` with ONE live, never-retired code entity - the "graph.db
/// exists but nothing has been retired" case, distinct from no `graph.db` at all.
fn seed_a_live_entity(root: &Path) {
    use rigger::contextgraph::sqlite::Projector;
    use rigger::contextgraph::{Projection, TYPE_CODE_ENTITY_EXTRACTED};
    use rigger::eventstore::Event;

    let project = project_identity_of(root);
    let graph_path = root.join(".rigger").join("graph.db");
    std::fs::create_dir_all(graph_path.parent().unwrap()).unwrap();
    let p = Projector::open(graph_path.to_str().unwrap(), &project).unwrap();

    let def = serde_json::json!({
        "file": "product.rs", "name": "product_fn", "kind": "function",
        "line": 1, "lang": "rust", "fresh": true,
    });
    let mut e = Event::new(
        TYPE_CODE_ENTITY_EXTRACTED,
        serde_json::to_vec(&def).unwrap(),
    );
    e.position = 1;
    p.apply(&e).unwrap();
}

#[test]
fn validate_warns_of_retired_code_entities_with_the_measured_count_and_never_fails() {
    let dir = temp_project();
    let root = dir.path();
    let (_out, err, ok) = run_rigger(root, &["init"]);
    assert!(ok, "rigger init must succeed; stderr:\n{err}");
    seed_a_retired_entity(root);

    let (_out, err, ok) = run_rigger(root, &["validate"]);
    assert!(
        ok,
        "an advisory must never fail validate's exit status; stderr:\n{err}"
    );
    assert!(
        err.to_lowercase().contains("retired"),
        "validate must warn that a code entity was retired; stderr:\n{err}"
    );
    assert!(
        err.contains('1'),
        "the warning must carry the measured retired count (exactly one); stderr:\n{err}"
    );
    assert!(
        err.to_lowercase().contains("never") || err.to_lowercase().contains("log"),
        "the migration is never a store wipe - the advisory must say history stays, per \
         `retired_entities_advisory`'s own wording; stderr:\n{err}"
    );
}

#[test]
fn validate_is_silent_on_retired_code_entities_when_nothing_has_been_retired() {
    let dir = temp_project();
    let root = dir.path();
    let (_out, err, ok) = run_rigger(root, &["init"]);
    assert!(ok, "rigger init must succeed; stderr:\n{err}");
    seed_a_live_entity(root);

    let (_out, err, ok) = run_rigger(root, &["validate"]);
    assert!(ok, "validate must succeed; stderr:\n{err}");
    assert!(
        !err.to_lowercase().contains("retired"),
        "a graph.db with a live, never-retired entity must draw no retirement warning; \
         stderr:\n{err}"
    );
}

#[test]
fn validate_never_fabricates_a_graph_db_and_draws_no_retired_advisory_on_a_fresh_project() {
    let dir = temp_project();
    let root = dir.path();
    let (_out, err, ok) = run_rigger(root, &["init"]);
    assert!(ok, "rigger init must succeed; stderr:\n{err}");
    let graph_path = root.join(".rigger").join("graph.db");
    assert!(
        !graph_path.exists(),
        "precondition: a fresh project has no graph.db yet"
    );

    let (out, err, ok) = run_rigger(root, &["validate"]);
    assert!(
        ok,
        "validate must succeed on a project with no graph.db yet; stderr:\n{err}"
    );
    assert!(
        out.contains("config valid"),
        "validate must still print its config summary; stdout:\n{out}"
    );
    assert!(
        !err.to_lowercase().contains("retired"),
        "a project with no graph.db yet must draw no retirement warning; stderr:\n{err}"
    );
    assert!(
        !graph_path.exists(),
        "a read-only advisory must never fabricate the graph.db file it would have read; the \
         file must stay absent afterward"
    );
}
