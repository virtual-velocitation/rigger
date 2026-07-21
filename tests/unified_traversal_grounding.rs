//! Periphery (integration) tests for spec 29c: structural grounding is ONE seeded traversal over
//! the unified graph, whose result is rendered into the spawn prompt. These run OUTSIDE the crate,
//! over the library's PUBLIC surface, so they guard the boundary the inside-out unit tests are
//! structurally blind to. They cover both halves the one traversal renders into the prompt: the
//! CODE neighborhood (criterion 1) and the DESIGN INTENT that governs the touched files (criterion
//! 3) - two projections of the SAME `subgraph` result reaching the same spawn through the same path.
//!
//! The whole change lives in `build_prompt_with_failure` / `graph_context` /
//! `write_code_neighborhood` / `write_design_intent` - all PRIVATE to `conductor`. The inside-out
//! unit tests reach those private functions directly (a hand-built `RunCtx`), so nothing pins the
//! behavior at the PUBLIC edge where it actually matters: an agent, during a real run, receives its
//! prompt through the `AgentDriver` port, and that prompt must carry the code neighborhood AND the
//! design intent the ONE unified-graph traversal surfaces - never the separate "Relevant locations"
//! structural stitch the spec retires, and never a design-intent title fetched by vector similarity
//! rather than by graph traversal.
//!
//! So these tests drive the public `conductor::run` entry with a capturing `AgentDriver`, populate
//! the unified graph through the public `contextgraph` event API (the same serialized events a real
//! run folds), and assert on the prompt the driver received. They exercise the new cross-module
//! render seam end-to-end: `conductor` reading `code-entity` nodes AND design-intent nodes
//! (`design-doc` / `arch-decision` / `handbook-rule` / `rationale`) out of the ONE `subgraph`
//! traversal and delivering them to a spawn. The render fold + those node/edge kinds are always
//! compiled, so these guard the boundary in BOTH feature lanes.

use std::sync::Mutex;

use rigger::conductor::{run, AgentDriver, AgentResult, Deps, Error, SpawnOpts};
use rigger::config::{AgentDef, Config, Stage};
use rigger::contextgraph::sqlite::Projector;
use rigger::contextgraph::{
    Projection, KIND_ARCH_DECISION, KIND_DESIGN_DOC, KIND_HANDBOOK_RULE, KIND_RATIONALE,
    REL_CONSTRAINS, REL_DOC_REFERENCES, REL_EXPLAINS, REL_GOVERNS, REL_SPECIFIES,
    TYPE_CODE_ENTITY_EXTRACTED, TYPE_DECISION_MADE, TYPE_DOC_CONCEPT_EXTRACTED,
    TYPE_DOC_LINK_EXTRACTED, TYPE_REVIEW_FINDING,
};
use rigger::eventstore::sqlite::Store;
use rigger::eventstore::Event;
use rigger::gate::ExecRunner;
use rigger::grounder::{Grounder, Ref};
use serde_json::{json, Value};

/// A driver that captures every prompt it is asked to spawn, then returns an empty result. It is
/// the observation channel for the periphery boundary: the prompt a spawn actually receives.
#[derive(Default)]
struct CapturingDriver {
    prompts: Mutex<Vec<String>>,
}

impl AgentDriver for CapturingDriver {
    fn spawn(
        &self,
        _agent: &AgentDef,
        prompt: &str,
        _opts: &SpawnOpts,
        _emit: &dyn Fn(&str, Value) -> Result<(), Error>,
    ) -> Result<AgentResult, Error> {
        self.prompts.lock().unwrap().push(prompt.to_string());
        Ok(AgentResult {
            output: String::new(),
            resolved_model: String::new(),
        })
    }
}

/// A grounder that resolves any query to a single file - the NL/turbovec seeding the spec retains,
/// stubbed so the seed is deterministic and the file's code neighborhood comes ONLY from the graph
/// traversal (its ref carries no text, so nothing the grounder returns is itself rendered).
struct SeedGrounder {
    file: String,
}

impl Grounder for SeedGrounder {
    fn ground(&self, _query: &str, _k: usize) -> Vec<Ref> {
        vec![Ref {
            file: self.file.clone(),
            line: 0,
            text: String::new(),
        }]
    }
}

/// Fold one event, built from its serialized JSON payload, into the graph at `pos` - the public
/// event API a real run folds through.
fn fold(g: &Projector, pos: &mut u64, type_: &str, payload: Value) {
    *pos += 1;
    let mut e = Event::new(type_, serde_json::to_vec(&payload).unwrap());
    e.position = *pos;
    g.apply(&e).unwrap();
}

/// Drive `conductor::run` over a single stage whose grounding query is `coverage`, grounded by
/// `grounder`, and return every prompt the driver was asked to spawn. The prompt is composed by the
/// same `build_prompt_with_failure` path a real run uses, so the code neighborhood in it comes from
/// the ONE seeded traversal - seeded by whatever files `grounder` resolves `coverage` to. This is the
/// parameterized core the criterion helpers share: criterion 1/3's tests seed a FIXED file via the
/// stub `SeedGrounder`; criterion 4 drives the REAL `Turbovec` so the seed is what the vector index
/// resolves an NL query to.
fn run_and_capture_prompts_grounded(
    graph: &Projector,
    grounder: &dyn Grounder,
    coverage: &str,
) -> Vec<String> {
    let mut cfg = Config::default();
    cfg.agents.insert(
        "impl".into(),
        AgentDef {
            id: "impl".into(),
            ..Default::default()
        },
    );
    cfg.workflow.stages.insert(
        "s".into(),
        Stage {
            name: "s".into(),
            agent: "impl".into(),
            coverage: coverage.into(),
            ..Default::default()
        },
    );

    let store = Store::open(":memory:").unwrap();
    let driver = CapturingDriver::default();
    let deps = Deps {
        store: &store,
        driver: &driver,
        gates: &ExecRunner,
        repo: String::new(),
        grounder: Some(grounder),
        graph: Some(graph),
        criteria: Vec::new(),
    };
    // The prompt is captured before the spawn returns, so the run's terminal disposition
    // (integrate / not) is irrelevant to what this periphery layer observes.
    let _ = run(&cfg, &deps);
    let prompts = driver.prompts.lock().unwrap().clone();
    prompts
}

/// Drive `conductor::run` over a single stage whose grounding seeds on `core.rs` (via the stub
/// `SeedGrounder`), and return every prompt the driver was asked to spawn. The fixed-seed form the
/// criterion 1/3 tests use; criterion 4 uses [`run_and_capture_prompts_grounded`] with the real
/// `Turbovec` so the seed is what the vector index resolves.
fn run_and_capture_prompts(graph: &Projector) -> Vec<String> {
    let grounder = SeedGrounder {
        file: "core.rs".into(),
    };
    run_and_capture_prompts_grounded(graph, &grounder, "core")
}

/// The load-bearing periphery contract of criterion 1: an agent's prompt, composed through the
/// public `run` path, carries the code neighborhood the ONE unified-graph traversal surfaces for the
/// touched file - together with the decisions and findings about it - and NO LONGER carries the
/// separate structural-grounder "Relevant locations" stitch the spec retires.
///
/// This is non-vacuous against the pre-collapse behavior: before criterion 1, the prompt rendered a
/// "Relevant locations to read first" block (the negative assertion would fail) and rendered NO code
/// neighborhood from the graph (the `run_unit` assertion would fail). Both flip only because the one
/// seeded traversal now sources the code neighborhood.
#[test]
fn a_spawn_prompt_carries_the_unified_traversal_code_neighborhood_not_the_old_structural_stitch() {
    let graph = Projector::open(":memory:", "test").unwrap();
    let mut pos = 0u64;

    // CODE NEIGHBORHOOD (29a): a definition the run extracted from the touched file. Its name is a
    // string that appears NOWHERE the grounder returns, so its presence in the prompt proves it was
    // sourced from the graph traversal, not stitched from the grounder's refs.
    fold(
        &graph,
        &mut pos,
        TYPE_CODE_ENTITY_EXTRACTED,
        json!({ "file": "core.rs", "name": "run_unit", "kind": "function", "line": 42, "lang": "rust", "fresh": true }),
    );
    // A DECISION and a FINDING about the SAME file: the one traversal must still return these
    // alongside the code neighborhood, so the collapse onto a single traversal drops nothing.
    fold(
        &graph,
        &mut pos,
        TYPE_DECISION_MADE,
        json!({ "id": "d_core", "summary": "the decision that governs the core file", "governs": ["core.rs"] }),
    );
    fold(
        &graph,
        &mut pos,
        TYPE_REVIEW_FINDING,
        json!({ "id": "f_core", "by": "arch", "unit": "u1", "summary": "the finding about the core file", "about": ["core.rs"] }),
    );

    let prompts = run_and_capture_prompts(&graph);
    assert!(
        !prompts.is_empty(),
        "the stage's agent must have been spawned with a prompt"
    );
    let prompt = &prompts[0];

    // The code neighborhood the ONE traversal surfaces reaches the prompt, as a "read first"
    // location line derived from the graph node (its file + line + name), not from the grounder.
    assert!(
        prompt.contains("run_unit") && prompt.contains("core.rs:42"),
        "the prompt must surface the file's code neighborhood (core.rs:42 run_unit) from the \
         unified traversal; prompt was:\n{prompt}"
    );
    // The separate structural-grounder stitch is GONE: the code neighborhood now comes from the ONE
    // traversal, so the old "Relevant locations" block must not be rendered.
    assert!(
        !prompt.contains("Relevant locations to read first"),
        "the separate structural-grounder 'Relevant locations' stitch must be collapsed away; \
         prompt was:\n{prompt}"
    );
    // The same single traversal still returns the decisions and findings about the file.
    assert!(
        prompt.contains("the decision that governs the core file"),
        "the one traversal must still surface the decision about the file; prompt was:\n{prompt}"
    );
    assert!(
        prompt.contains("the finding about the core file"),
        "the one traversal must still surface the finding about the file; prompt was:\n{prompt}"
    );
}

/// The code-neighborhood section is prompt-budgeted: a broad neighborhood renders the most-recent
/// definitions verbatim and collapses the remainder into ONE visible elision note, so a large file's
/// extracted definitions can never blow the prompt. This guards a load-bearing render behavior the
/// spec's done-when leaves implicit - an implementer could silently drop the cap and every existing
/// test would stay green - at the same public prompt boundary.
///
/// It is non-vacuous: the definitions sort deterministically by (file, line, id), so the earliest
/// lines render and the latest are elided. Removing the cap would render every definition, flipping
/// both the "later definition is absent" and the "elision note present" assertions.
#[test]
fn the_code_neighborhood_section_is_budget_capped_with_a_visible_elision_note() {
    let graph = Projector::open(":memory:", "test").unwrap();
    let mut pos = 0u64;

    // Fold more definitions than the verbatim cap keeps (the cap is well under this), all in the one
    // touched file so the seeded traversal reaches every one of them. Only the FIRST event of the
    // file's extraction batch carries `fresh` (29a's supersede-on-re-extract retires the file's prior
    // edges once, at the batch head); the rest accrete, exactly as a real extraction pass emits them.
    let count = 40u32;
    for i in 1..=count {
        fold(
            &graph,
            &mut pos,
            TYPE_CODE_ENTITY_EXTRACTED,
            json!({ "file": "core.rs", "name": format!("definition_{i:03}"), "kind": "function", "line": i, "lang": "rust", "fresh": i == 1 }),
        );
    }

    let prompts = run_and_capture_prompts(&graph);
    assert!(
        !prompts.is_empty(),
        "the stage's agent must have been spawned"
    );
    let prompt = &prompts[0];

    // The earliest definition (smallest line) renders verbatim.
    assert!(
        prompt.contains("definition_001"),
        "the earliest-sorted definition must render verbatim in the code neighborhood; \
         prompt was:\n{prompt}"
    );
    // The latest definition (largest line) is past the cap, so it is elided, not rendered.
    assert!(
        !prompt.contains(&format!("definition_{count:03}")),
        "a definition past the verbatim cap must be elided from the prompt, not rendered; \
         prompt was:\n{prompt}"
    );
    // The remainder collapses into ONE visible elision note (the store keeps the full set).
    assert!(
        prompt.contains("more definition(s) elided"),
        "the over-budget remainder must collapse into a visible elision note; \
         prompt was:\n{prompt}"
    );
}

/// The elision note is an INSTRUCTION to the agent - "recover the full set with X" - so the command
/// it names must actually recover the elided code definitions. The `rigger peers` command prints
/// only decisions / lessons / findings and can never return a code entity; the honest recovery for
/// an elided definition is `rigger graph --around <file>`, whose subgraph nodes include code
/// entities. This pins that honest command AT THE PUBLIC PROMPT BOUNDARY - the exact bytes an agent
/// receives through the `AgentDriver` port during a real `run`. The inside-out unit test reaches the
/// private render function directly; a wiring regression (the wrong section composed, the note
/// transformed during prompt assembly, or a refactor reverting the command) could keep that private
/// test green while shipping a false instruction to a real spawn - which only a test at this boundary
/// catches.
///
/// Non-vacuous: the definition note is uniquely identified by "more definition(s)" (the decisions /
/// lessons / findings notes say "older <noun>(s)"), so the assertions isolate the code section from
/// the legitimate `rigger peers` those other notes name. Rendering the recovery command as `rigger
/// peers` (the pre-fix behavior) flips BOTH assertions: the positive (honest command absent) and the
/// negative (dishonest command present).
#[test]
fn the_spawn_prompt_code_neighborhood_elision_note_names_the_honest_graph_around_recovery() {
    let graph = Projector::open(":memory:", "test").unwrap();
    let mut pos = 0u64;

    // Over-cap the code neighborhood so its remainder elides into the recovery note. All in the one
    // seeded file (core.rs), so the traversal reaches every definition and the note names that file.
    let count = 40u32;
    for i in 1..=count {
        fold(
            &graph,
            &mut pos,
            TYPE_CODE_ENTITY_EXTRACTED,
            json!({ "file": "core.rs", "name": format!("definition_{i:03}"), "kind": "function", "line": i, "lang": "rust", "fresh": i == 1 }),
        );
    }

    let prompts = run_and_capture_prompts(&graph);
    assert!(
        !prompts.is_empty(),
        "the stage's agent must have been spawned"
    );
    let prompt = &prompts[0];

    // The code-neighborhood elision note names the HONEST recovery command, scoped to the touched
    // file the single-seed traversal ran over (`rigger graph --around core.rs`).
    assert!(
        prompt.contains(
            "more definition(s) elided to keep this prompt under budget - recover the full set with `rigger graph --around core.rs`"
        ),
        "the code-neighborhood elision note must name the honest `rigger graph --around <file>` \
         recovery (whose subgraph returns code entities); prompt was:\n{prompt}"
    );
    // It must NOT name `rigger peers` for a code definition - that command never prints a code
    // entity, so it could never recover the elided remainder (the boundary defect the fix closed).
    assert!(
        !prompt.contains(
            "more definition(s) elided to keep this prompt under budget - recover the full set with `rigger peers"
        ),
        "the code-neighborhood elision note must not name `rigger peers` (which cannot recover a \
         code definition) as the recovery command; prompt was:\n{prompt}"
    );
}

/// The code-neighborhood section renders ONLY when the traversal actually surfaces code definitions
/// for the touched files. A run whose graph carries decisions / findings about the file but no
/// extracted definition for it must not emit a bare, dangling "Code neighborhood" header with
/// nothing under it - empty scaffolding is noise in an agent's prompt. This guards the empty-section
/// suppression at the same public boundary: the inside-out unit test asserts the private renderer
/// writes no header; this asserts the composed prompt an agent receives carries none either, while
/// still surfacing the decision - so the one traversal ran and reached the file, it simply had no
/// code neighborhood to render.
///
/// Non-vacuous: the decision about `core.rs` proves the traversal reached the file, so a renderer
/// that emitted the header unconditionally (before the empty-candidate early return) would flip the
/// header-absence assertion while the decision assertion still held.
#[test]
fn a_spawn_prompt_with_no_extracted_definitions_renders_no_code_neighborhood_header() {
    let graph = Projector::open(":memory:", "test").unwrap();
    let mut pos = 0u64;

    // A DECISION about the touched file but NO code entity for it: the seeded traversal reaches the
    // file (the decision governs it) yet has zero definitions to render as a code neighborhood.
    fold(
        &graph,
        &mut pos,
        TYPE_DECISION_MADE,
        json!({ "id": "d_core", "summary": "the decision that governs the core file", "governs": ["core.rs"] }),
    );

    let prompts = run_and_capture_prompts(&graph);
    assert!(
        !prompts.is_empty(),
        "the stage's agent must have been spawned"
    );
    let prompt = &prompts[0];

    // The one traversal ran and reached the file: its decision is surfaced.
    assert!(
        prompt.contains("the decision that governs the core file"),
        "the one traversal must still surface the decision about the file; prompt was:\n{prompt}"
    );
    // With no extracted definition, the code-neighborhood section is suppressed entirely - no bare
    // header renders over an empty body.
    assert!(
        !prompt.contains("Code neighborhood of these files"),
        "an empty code neighborhood must render no bare header; prompt was:\n{prompt}"
    );
}

/// Spec 29c criterion 4: `turbovec` NL retrieval is RETAINED and COMPLEMENTARY to the unified graph.
/// The 29c collapse (criterion 1) retired the separate structural-grounder "Relevant locations"
/// stitch, but it KEPT the vector grounder as the NL SEEDER: a symbol-free query - English prose with
/// no code identifier to seed a graph node id on - still resolves through the REAL vector index, and
/// the files it resolves SEED the ONE unified-graph traversal that composes the prompt. Graph and
/// vectors are layers, not competitors.
///
/// This pins BOTH halves at the PUBLIC boundary with the REAL `Turbovec` engine (never a stub - so
/// "resolves via the vector index" is proven by the vector index actually resolving it, not assumed):
///
///  (1) RETAINED / via the vector index: `Turbovec::ground` resolves the English query
///      "how is damage dealt to an enemy" - natural-language prose, not a code identifier - to
///      `combat.rs`, ranking the damage code above the rendering code by embedding similarity. (This
///      is the exact corpus + query the turbovec unit test `grounds_semantically` pins, so the
///      ranking is deterministic here too.)
///
///  (2) COMPLEMENTARY / its result SEEDS the unified traversal: driving the SAME real `Turbovec`
///      through the public `conductor::run` grounding path, with the stage's grounding query being
///      that symbol-free NL query, the spawn's prompt carries the graph neighborhood the ONE seeded
///      traversal surfaces for `combat.rs` - a decision the run folded ABOUT that file. The decision
///      text appears NOWHERE in the query or the repo source, so its presence in the prompt can only
///      be the seeded traversal reaching `combat.rs` - which happened because turbovec's NL result is
///      what seeded it.
///
/// Non-vacuous (mutation-checked): had the collapse dropped turbovec as the seeder (an empty seed),
/// `graph_context` would traverse nothing and the neighborhood marker would be absent - flipping
/// assertion (2); if turbovec did not resolve the symbol-free query via the vector index, assertion
/// (1) would fail. This does NOT re-prove the traversal itself (criterion 1) or the tier filters
/// (criterion 2) - it owns only vector-retention and the vector->traversal seam.
///
/// `turbovec`-gated (the vector index IS the `turbovec` feature; the light `--no-default-features`
/// lane has no vector index to retain) and `#[file_serial(turbovec_model)]` on the shared model key,
/// so no two embedding-model constructions ever overlap across test binaries (the documented
/// heap-corruption trigger).
#[cfg(feature = "turbovec")]
#[test]
#[serial_test::file_serial(turbovec_model)]
fn turbovec_nl_retrieval_is_retained_and_seeds_the_unified_traversal() {
    use rigger::grounder::turbovec::Turbovec;

    // A tiny repo whose two files differ only in MEANING: `combat.rs` deals damage, `render.rs`
    // draws. A symbol-free English query must pick `combat.rs` semantically, through the vector index.
    let repo = tempfile::tempdir().unwrap();
    std::fs::write(
        repo.path().join("combat.rs"),
        "fn apply_damage(target: &mut Entity, amount: f32) {\n    target.health -= amount;\n}\n",
    )
    .unwrap();
    std::fs::write(
        repo.path().join("render.rs"),
        "fn draw_sprite(sprite: &Sprite, x: f32, y: f32) {\n    // upload to the gpu\n}\n",
    )
    .unwrap();

    let tv = Turbovec::new(repo.path().to_str().unwrap())
        .expect("the real turbovec engine must construct over the tiny repo");

    // A symbol-free NL query: English prose with no code identifier to seed a graph node id on. The
    // conductor grounds a plain unit stage on its `coverage`, so this same query drives production.
    let query = "how is damage dealt to an enemy";

    // (1) RETAINED, via the vector index: the query resolves to `combat.rs`, ranked above
    // `render.rs`. This is the vector grounder answering an NL query - the retrieval 29c retains.
    let refs = tv.ground(query, 8);
    assert_eq!(
        refs.first().map(|r| r.file.as_str()),
        Some("combat.rs"),
        "the symbol-free NL query must resolve via the vector index, ranking the damage code \
         above the rendering code; got {refs:?}"
    );

    // The unified graph the run populates: a DECISION folded ABOUT `combat.rs` - the file turbovec
    // resolves the query to. Its summary is a marker that appears NOWHERE in the query or the repo
    // source, so surfacing it in the prompt can ONLY be the seeded traversal reaching `combat.rs`.
    let graph = Projector::open(":memory:", "test").unwrap();
    let mut pos = 0u64;
    fold(
        &graph,
        &mut pos,
        TYPE_DECISION_MADE,
        json!({ "id": "d_combat", "summary": "the design note that governs the combat file", "governs": ["combat.rs"] }),
    );

    // (2) COMPLEMENTARY, seeds the unified traversal: drive the SAME real turbovec through the public
    // `conductor::run` grounding path. `grounded_seed` resolves `query` through turbovec to
    // `combat.rs`, `graph_context` seeds the ONE `subgraph` traversal on it, and the prompt carries
    // that file's graph neighborhood - proving turbovec's NL result seeded the unified traversal.
    let prompts = run_and_capture_prompts_grounded(&graph, &tv, query);
    assert!(
        !prompts.is_empty(),
        "the stage's agent must have been spawned with a prompt"
    );
    let prompt = &prompts[0];
    assert!(
        prompt.contains("the design note that governs the combat file"),
        "turbovec's NL result must SEED the unified traversal: the prompt must surface the graph \
         neighborhood (the decision about combat.rs) the one seeded traversal reaches via the \
         turbovec-resolved file; prompt was:\n{prompt}"
    );
}

/// Fold a design-intent NODE plus a typed code-binding EDGE to `to` - the two public `contextgraph`
/// events (a `DocConceptExtracted` node of `kind`, then a `DocLinkExtracted` edge of `rel`) a real
/// 29b ingestion pass emits - so the one seeded traversal reaches the node and the design-intent
/// section can render it.
fn fold_design_intent(
    g: &Projector,
    pos: &mut u64,
    kind: &str,
    id: &str,
    title: &str,
    rel: &str,
    to: &str,
) {
    fold(
        g,
        pos,
        TYPE_DOC_CONCEPT_EXTRACTED,
        json!({ "kind": kind, "id": id, "title": title, "doc": id }),
    );
    fold(
        g,
        pos,
        TYPE_DOC_LINK_EXTRACTED,
        json!({ "from": id, "to": to, "rel": rel }),
    );
}

/// The load-bearing periphery contract of criterion 3 (the highest-value new capability c3 owns):
/// an agent whose blast radius touches file `F` receives, IN THE PROMPT IT IS SPAWNED WITH and BY
/// GRAPH TRAVERSAL (never by vector similarity), the DESIGN INTENT that governs `F` - the handbook
/// rule that GOVERNS it, the RA / architecture section that SPECIFIES it, the load-bearing decision
/// that CONSTRAINS it, and the local rationale that explains it (spec 29b's node / edge kinds). The
/// inside-out unit test reaches the PRIVATE `build_prompt_with_failure` directly; this pins the SAME
/// render at the PUBLIC boundary - the exact bytes a spawn receives through the `AgentDriver` port
/// during a real `run`, sourced from the ONE unified `subgraph` traversal the design half of the
/// graph shares with the code neighborhood and the decisions / findings.
///
/// BY TRAVERSAL, NOT VECTOR: the seeding grounder resolves the query to `core.rs` but its ref carries
/// EMPTY text, so any design-intent title in the prompt can ONLY have come from the graph traversal.
///
/// TIGHT-SCOPED ("of these files"): a design-intent node reachable in the subgraph whose binding edge
/// targets a NON-seed file, and a doc that only CITES the file (a `references` edge, not a code-
/// binding SPECIFIES / GOVERNS), are NOT surfaced - so the section renders design intent that BINDS
/// the touched files, not everything the traversal reaches.
///
/// Non-vacuous against the pre-c3 tree: before criterion 3 the prompt rendered NO design-intent
/// section, so every positive assertion fails on the base; they flip green only because
/// `write_design_intent` now renders the design half of the one traversal into the prompt. Dropping
/// the seed scoping would surface decoy A, flipping the negative assertion.
#[test]
fn a_spawn_prompt_carries_the_design_intent_that_governs_the_touched_files_by_traversal() {
    let graph = Projector::open(":memory:", "test").unwrap();
    let mut pos = 0u64;

    // DESIGN INTENT that BINDS the touched file `core.rs` (29b): one design-intent node per code-
    // binding relation the section surfaces. Each title is a string the grounder never returns, so
    // its presence in the prompt proves it was sourced from the graph traversal, not the seed.
    fold_design_intent(
        &graph,
        &mut pos,
        KIND_HANDBOOK_RULE,
        "docs/handbook/loops.md",
        "the loop discipline rule governing core",
        REL_GOVERNS,
        "core.rs",
    );
    fold_design_intent(
        &graph,
        &mut pos,
        KIND_DESIGN_DOC,
        "specs/29c.md#unified-traversal",
        "the RA section specifying the unified traversal",
        REL_SPECIFIES,
        "core.rs",
    );
    fold_design_intent(
        &graph,
        &mut pos,
        KIND_ARCH_DECISION,
        "docs/adr/0007.md",
        "the load-bearing decision constraining core",
        REL_CONSTRAINS,
        "core.rs",
    );
    fold_design_intent(
        &graph,
        &mut pos,
        KIND_RATIONALE,
        "core.rs#L42",
        "the local rationale explaining run_unit",
        REL_EXPLAINS,
        "core.rs",
    );

    // DECOY A (file-scope): a design-doc whose code-binding SPECIFIES edge targets a NON-seed file
    // (`other.rs`), yet is reachable in the subgraph because it also CITES `core.rs` (a `references`
    // edge). "Of these files" scoping must keep it out.
    fold_design_intent(
        &graph,
        &mut pos,
        KIND_DESIGN_DOC,
        "specs/other.md#other",
        "the RA section specifying OTHER not core",
        REL_SPECIFIES,
        "other.rs",
    );
    fold(
        &graph,
        &mut pos,
        TYPE_DOC_LINK_EXTRACTED,
        json!({ "from": "specs/other.md#other", "to": "core.rs", "rel": REL_DOC_REFERENCES }),
    );
    // DECOY B (relation-scope): a design-doc that only CITES `core.rs` (a `references` edge) with NO
    // code-binding SPECIFIES / GOVERNS edge. A mere citation is not intent that governs the file.
    fold(
        &graph,
        &mut pos,
        TYPE_DOC_CONCEPT_EXTRACTED,
        json!({ "kind": KIND_DESIGN_DOC, "id": "docs/misc.md", "title": "a mere doc citation of core", "doc": "docs/misc.md" }),
    );
    fold(
        &graph,
        &mut pos,
        TYPE_DOC_LINK_EXTRACTED,
        json!({ "from": "docs/misc.md", "to": "core.rs", "rel": REL_DOC_REFERENCES }),
    );

    let prompts = run_and_capture_prompts(&graph);
    assert!(
        !prompts.is_empty(),
        "the stage's agent must have been spawned with a prompt"
    );
    let prompt = &prompts[0];

    // RENDERED by traversal: all four design-intent titles reach the prompt (the grounder returns
    // empty text, so a title can only have come from the graph half of the one traversal).
    assert!(
        prompt.contains("the loop discipline rule governing core"),
        "the design-intent section must surface the handbook rule that GOVERNS the touched file; \
         prompt was:\n{prompt}"
    );
    assert!(
        prompt.contains("the RA section specifying the unified traversal"),
        "the design-intent section must surface the RA section that SPECIFIES the touched file; \
         prompt was:\n{prompt}"
    );
    assert!(
        prompt.contains("the load-bearing decision constraining core"),
        "the design-intent section must surface the arch-decision that CONSTRAINS the touched \
         file; prompt was:\n{prompt}"
    );
    assert!(
        prompt.contains("the local rationale explaining run_unit"),
        "the design-intent section must surface the rationale that explains the touched file; \
         prompt was:\n{prompt}"
    );
    // Each rendered line names its binding relation and the touched file it governs.
    assert!(
        prompt.contains(&format!("{REL_GOVERNS} core.rs")),
        "the design-intent line must name the GOVERNS relation and the touched file; prompt \
         was:\n{prompt}"
    );
    assert!(
        prompt.contains(&format!("{REL_SPECIFIES} core.rs")),
        "the design-intent line must name the SPECIFIES relation and the touched file; prompt \
         was:\n{prompt}"
    );
    // TIGHT-SCOPED: neither decoy surfaces. Decoy A binds a non-seed file; decoy B only cites the
    // file. Both are reachable in the subgraph, so their absence proves the render scopes to design
    // intent that BINDS the touched files, not everything the traversal reaches.
    assert!(
        !prompt.contains("the RA section specifying OTHER not core"),
        "a design-intent node whose binding edge targets a NON-seed file must NOT surface \
         (of-these-files scoping); prompt was:\n{prompt}"
    );
    assert!(
        !prompt.contains("a mere doc citation of core"),
        "a doc that only CITES the file (a `references` edge, not a code-binding SPECIFIES / \
         GOVERNS) is not intent that governs it and must NOT surface; prompt was:\n{prompt}"
    );
}

/// The design-intent section is prompt-budgeted exactly like the code neighborhood: a file with a
/// broad governing footprint renders the most-recent design-intent nodes verbatim and collapses the
/// remainder into ONE visible elision note. That note is an INSTRUCTION to the agent - "recover the
/// full set with X" - so the command it names must actually recover an elided design-intent node.
/// `rigger peers` prints only decisions / lessons / findings and can NEVER return a design-intent
/// node; the honest recovery is `rigger graph --around <file>`, whose subgraph nodes INCLUDE the
/// design-intent nodes. This pins that honest command AT THE PUBLIC PROMPT BOUNDARY - the exact bytes
/// an agent receives through the `AgentDriver` port during a real `run`.
///
/// The inside-out unit test never over-caps this section (it folds a handful of nodes, well under
/// the verbatim cap), so nothing there exercises the elision note; a wiring regression, a refactor
/// reverting the recovery command, or a dropped cap could keep that private test green while shipping
/// a false instruction - or an unbounded prompt - to a real spawn, which only a test at this boundary
/// catches.
///
/// Non-vacuous: on the pre-c3 tree there is no design-intent section, so the note is absent and the
/// positive assertion fails; reverting the recovery command to `rigger peers` flips BOTH the positive
/// (honest command absent) and the negative (dishonest command present); dropping the verbatim cap
/// renders every node, so the "elided" note never appears.
#[test]
fn the_design_intent_section_is_budget_capped_and_its_elision_note_names_the_honest_graph_around_recovery(
) {
    let graph = Projector::open(":memory:", "test").unwrap();
    let mut pos = 0u64;

    // Fold MORE design-intent nodes bound to the one seed file than the verbatim cap keeps (the cap
    // is well under this count), so the remainder collapses into the recovery note. Each is a
    // handbook rule that GOVERNS `core.rs`, so the single-seed traversal reaches every one of them.
    let count = 40u32;
    for i in 1..=count {
        let id = format!("docs/handbook/rule_{i:03}.md");
        fold_design_intent(
            &graph,
            &mut pos,
            KIND_HANDBOOK_RULE,
            &id,
            &format!("the governing rule {i:03}"),
            REL_GOVERNS,
            "core.rs",
        );
    }

    let prompts = run_and_capture_prompts(&graph);
    assert!(
        !prompts.is_empty(),
        "the stage's agent must have been spawned"
    );
    let prompt = &prompts[0];

    // The over-budget remainder collapses into ONE visible note that names the HONEST recovery
    // command, scoped to the touched file the single-seed traversal ran over.
    assert!(
        prompt.contains(
            "more design-intent node(s) elided to keep this prompt under budget - recover the full set with `rigger graph --around core.rs`"
        ),
        "the design-intent elision note must name the honest `rigger graph --around <file>` recovery \
         (whose subgraph returns design-intent nodes); prompt was:\n{prompt}"
    );
    // It must NOT name `rigger peers` for a design-intent node - that command never prints one, so it
    // could never recover the elided remainder.
    assert!(
        !prompt.contains(
            "more design-intent node(s) elided to keep this prompt under budget - recover the full set with `rigger peers"
        ),
        "the design-intent elision note must not name `rigger peers` (which cannot recover a \
         design-intent node) as the recovery command; prompt was:\n{prompt}"
    );
}

/// A governing DECISION must NEVER leak into the design-intent section of the spawn prompt. A
/// decision is dated by a `decision --GOVERNS--> file` edge - the SAME REL_GOVERNS relation a
/// handbook rule uses to govern a file - so the ONLY thing keeping a decision out of the
/// design-intent section is `write_design_intent`'s kind guard (a candidate must be a
/// design-intent KIND, not merely reached through a design-intent relation). This is a COMMON-PATH
/// guard: with rigger self-hosting, decisions govern the touched files on essentially every real
/// spawn, so a silent regression would render every governing decision as a design-intent line with
/// an EMPTY title (a decision carries `summary`, not `title`) in every prompt.
///
/// The inside-out unit test reaches `write_design_intent` directly; this pins the same guard at the
/// PUBLIC prompt boundary. Non-vacuous: the decision reaches the prompt through the DECISIONS section
/// (its summary is asserted present), proving the traversal reached it, while it is ABSENT from the
/// design-intent section. Mutation-proven: replacing the kind check with `true` leaks the decision
/// as `- GOVERNS core.rs  d_leak_decision: ` (empty title), flipping the negative assertion.
#[test]
fn a_governing_decision_never_leaks_into_the_spawn_prompt_design_intent_section() {
    let graph = Projector::open(":memory:", "test").unwrap();
    let mut pos = 0u64;

    // A genuine handbook rule that GOVERNS the file - a design-intent node bound by the shared
    // GOVERNS relation, so the design-intent section renders and the discriminator is KIND, not
    // relation. Its title is a string the grounder never returns, so it can only reach the prompt by
    // graph traversal.
    fold_design_intent(
        &graph,
        &mut pos,
        KIND_HANDBOOK_RULE,
        "docs/handbook/loops.md",
        "the loop rule that governs core",
        REL_GOVERNS,
        "core.rs",
    );
    // A DECISION that GOVERNS the SAME file through the SHARED GOVERNS relation - the leak vector.
    fold(
        &graph,
        &mut pos,
        TYPE_DECISION_MADE,
        json!({ "id": "d_leak_decision", "summary": "a governing decision that is not design intent", "governs": ["core.rs"] }),
    );

    let prompts = run_and_capture_prompts(&graph);
    assert!(
        !prompts.is_empty(),
        "the stage's agent must have been spawned with a prompt"
    );
    let prompt = &prompts[0];

    // The genuine handbook rule (design-intent kind, GOVERNS relation) renders in the section.
    assert!(
        prompt.contains("the loop rule that governs core"),
        "the handbook rule bound by GOVERNS must render in the design-intent section; prompt \
         was:\n{prompt}"
    );
    assert!(
        prompt.contains(&format!("{REL_GOVERNS} core.rs")),
        "the design-intent line must name the GOVERNS relation and the touched file; prompt \
         was:\n{prompt}"
    );
    // NON-VACUITY: the decision DID reach the prompt - through the decisions section - so it was in
    // the traversal; the kind guard, not reachability, is what keeps it out of the design-intent
    // section.
    assert!(
        prompt.contains("a governing decision that is not design intent"),
        "the governing decision must still surface in the decisions section (proving the traversal \
         reached it); prompt was:\n{prompt}"
    );
    // THE GUARD: the governing decision must NOT render as a design-intent line. Its design-intent
    // form would be `- GOVERNS core.rs  d_leak_decision: ` (rel, file, then the id with an empty
    // title); the decisions section renders it as `- d_leak_decision: <summary>` instead, so this
    // exact substring can only appear if the decision leaked through the shared relation.
    assert!(
        !prompt.contains(&format!("{REL_GOVERNS} core.rs  d_leak_decision")),
        "a governing DECISION must be excluded from the design-intent section by the kind guard - it \
         belongs in the decisions section, not here; prompt was:\n{prompt}"
    );
}

/// The design-intent section renders the most-recently-recorded bindings verbatim and elides the
/// oldest past the cap - the newest-renders / oldest-elided pair the sibling c1 code-neighborhood cap
/// test set the standard for. `write_design_intent` orders candidates by each node's highest
/// binding-edge event-log position (a deterministic total order, NOT authored priority), so over a
/// footprint larger than the verbatim cap the latest-recorded binding survives and the earliest is
/// collapsed into the elision note.
///
/// The existing budget-cap test asserts only that the note appears, never WHICH nodes survive; this
/// pins the ordering itself at the public prompt boundary. Mutation-proven: reversing the comparator
/// (`rc.cmp(&ra)` -> `ra.cmp(&rc)`) ships the OLDEST binding verbatim and elides the NEWEST, flipping
/// both the "newest renders" and "oldest elided" assertions.
#[test]
fn the_spawn_prompt_design_intent_section_renders_the_newest_binding_and_elides_the_oldest() {
    let graph = Projector::open(":memory:", "test").unwrap();
    let mut pos = 0u64;

    // Fold MORE handbook rules bound to the one file than the verbatim cap keeps, in ascending order,
    // so rule_040's binding edge carries the HIGHEST log position (newest recorded) and rule_001's
    // the lowest (oldest). Each GOVERNS `core.rs`, so the single-seed traversal reaches every one.
    let count = 40u32;
    for i in 1..=count {
        let id = format!("docs/handbook/rule_{i:03}.md");
        fold_design_intent(
            &graph,
            &mut pos,
            KIND_HANDBOOK_RULE,
            &id,
            &format!("the governing rule {i:03}"),
            REL_GOVERNS,
            "core.rs",
        );
    }

    let prompts = run_and_capture_prompts(&graph);
    assert!(
        !prompts.is_empty(),
        "the stage's agent must have been spawned"
    );
    let prompt = &prompts[0];

    // The newest-recorded binding renders verbatim.
    assert!(
        prompt.contains("the governing rule 040"),
        "the newest-recorded design-intent binding must render verbatim; prompt was:\n{prompt}"
    );
    // The oldest-recorded binding is past the verbatim cap, so it is elided, not rendered.
    assert!(
        !prompt.contains("the governing rule 001"),
        "the oldest-recorded design-intent binding must be elided past the cap, not rendered; \
         prompt was:\n{prompt}"
    );
    // The over-budget remainder collapses into ONE visible elision note.
    assert!(
        prompt.contains("more design-intent node(s) elided"),
        "the over-budget remainder must collapse into a visible elision note; prompt was:\n{prompt}"
    );
}

/// The design-intent section is suppressed entirely when nothing governs the touched files: a run
/// whose graph carries a code neighborhood and a decision about the file but NO design-intent node
/// bound to it must not emit a bare "Design intent..." header over an empty body - empty scaffolding
/// is noise in an agent's prompt. This is the design-intent counterpart to
/// `a_spawn_prompt_with_no_extracted_definitions_renders_no_code_neighborhood_header`, guarding the
/// same empty-section suppression at the design half of the one traversal: the inside-out unit test
/// asserts the private renderer writes no header; this asserts the composed prompt an agent receives
/// carries none either, while still surfacing the code neighborhood - so the one traversal ran and
/// reached the file, it simply had no design intent to render.
///
/// Non-vacuous: the code definition of `core.rs` renders, proving the traversal reached the file, so
/// a renderer that emitted the design-intent header unconditionally (before the empty-candidate early
/// return) would flip the header-absence assertion while the code-neighborhood assertion still held.
/// The section is empty because NO design-intent edge binds the seed at all (not because a guard
/// filtered one out), so this isolates the empty-candidate suppression itself: it stays green under
/// the kind-guard and recency-order mutations the sibling design-intent tests pin.
#[test]
fn a_spawn_prompt_with_no_governing_design_intent_renders_no_design_intent_header() {
    let graph = Projector::open(":memory:", "test").unwrap();
    let mut pos = 0u64;

    // A code definition of the touched file - the seeded traversal reaches `core.rs` and renders its
    // code neighborhood - but NO design-intent node (handbook rule / RA section / arch decision /
    // rationale) and NO design-intent edge bound to it, so the design-intent section has zero
    // candidates for a reason independent of the kind guard.
    fold(
        &graph,
        &mut pos,
        TYPE_CODE_ENTITY_EXTRACTED,
        json!({ "file": "core.rs", "name": "run_unit", "kind": "function", "line": 42, "lang": "rust", "fresh": true }),
    );

    let prompts = run_and_capture_prompts(&graph);
    assert!(
        !prompts.is_empty(),
        "the stage's agent must have been spawned"
    );
    let prompt = &prompts[0];

    // The one traversal ran and reached the file: its code neighborhood is surfaced.
    assert!(
        prompt.contains("run_unit") && prompt.contains("core.rs:42"),
        "the one traversal must still surface the file's code neighborhood; prompt was:\n{prompt}"
    );
    // With no governing design intent, the design-intent section is suppressed entirely - no bare
    // header renders over an empty body.
    assert!(
        !prompt.contains("Design intent that governs these files"),
        "an empty design-intent section must render no bare header; prompt was:\n{prompt}"
    );
}

/// A grounder that seeds MORE THAN ONE file - a real blast radius touches several files at once. Its
/// refs carry EMPTY text (like `SeedGrounder`), so anything in the prompt was sourced by the graph
/// traversal, not by the seed; the files are returned in a fixed order so the derived seed - and thus
/// the multi-seed recovery command - is deterministic.
struct MultiSeedGrounder {
    files: Vec<String>,
}

impl Grounder for MultiSeedGrounder {
    fn ground(&self, _query: &str, _k: usize) -> Vec<Ref> {
        self.files
            .iter()
            .map(|f| Ref {
                file: f.clone(),
                line: 0,
                text: String::new(),
            })
            .collect()
    }
}

/// The design-intent section honors a MULTI-FILE blast radius - the common shape of a real run, which
/// almost always touches several files at once. Two boundary behaviors that the single-seed sibling
/// tests structurally cannot reach are pinned here, both at the PUBLIC prompt boundary an agent
/// receives through the `AgentDriver` port:
///  (1) A single design-intent node that GOVERNS more than one of the touched files must name EVERY
///      touched file it binds on its rendered line (`write_design_intent` joins the distinct bound
///      seed files) - not just the first. A run whose rule governs two touched files but rendered
///      only one would silently under-state which files the intent covers.
///  (2) The over-budget elision note must name the MULTI-SEED recovery `rigger graph --around <file>`
///      on each touched file (`graph_around_recovery`'s multi-seed branch), since `--around` takes a
///      single id and a multi-file blast radius needs one invocation per file to recover the elided
///      remainder.
///
/// Non-vacuous: rendering only the first bound file (dropping the per-node file join) drops `util.rs`
/// from the governing rule's line, flipping (1); collapsing the recovery to the single-seed form
/// (`rigger graph --around core.rs`) with no "on each of" list flips (2). Neither path is exercised by
/// any single-seed test in either layer, so this is the only guard over the multi-file render seam.
#[test]
fn the_design_intent_section_renders_every_touched_file_a_node_binds_with_a_multi_seed_recovery() {
    let graph = Projector::open(":memory:", "test").unwrap();
    let mut pos = 0u64;

    // Enough governing rules bound across the two touched files to overflow the verbatim cap, folded
    // FIRST so they carry the OLDEST bindings - the over-budget remainder that collapses into the
    // recovery note. Each GOVERNS one seed file, so every one is a candidate the two-file traversal
    // reaches.
    let count = 40u32;
    for i in 1..=count {
        let id = format!("docs/handbook/extra_{i:03}.md");
        let to = if i % 2 == 0 { "core.rs" } else { "util.rs" };
        fold_design_intent(
            &graph,
            &mut pos,
            KIND_HANDBOOK_RULE,
            &id,
            &format!("extra governing rule {i:03}"),
            REL_GOVERNS,
            to,
        );
    }
    // ONE handbook rule that GOVERNS BOTH touched files, folded LAST so its binding is the
    // NEWEST-recorded and it survives the verbatim cap - its rendered line must name BOTH files.
    fold_design_intent(
        &graph,
        &mut pos,
        KIND_HANDBOOK_RULE,
        "docs/handbook/loops.md",
        "the loop rule governing both core and util",
        REL_GOVERNS,
        "core.rs",
    );
    fold(
        &graph,
        &mut pos,
        TYPE_DOC_LINK_EXTRACTED,
        json!({ "from": "docs/handbook/loops.md", "to": "util.rs", "rel": REL_GOVERNS }),
    );

    // A real blast radius that touches TWO files, in a fixed order so the derived seed is
    // `core.rs util.rs`.
    let grounder = MultiSeedGrounder {
        files: vec!["core.rs".into(), "util.rs".into()],
    };
    let prompts = run_and_capture_prompts_grounded(&graph, &grounder, "core");
    assert!(
        !prompts.is_empty(),
        "the stage's agent must have been spawned"
    );
    let prompt = &prompts[0];

    // (1) The rule that governs BOTH touched files names BOTH on its line (the distinct bound seed
    // files joined, sorted) - proving the render honors every touched file, not just the first.
    assert!(
        prompt.contains(&format!("{REL_GOVERNS} core.rs util.rs")),
        "a design-intent node that binds MORE THAN ONE touched file must name every file it binds on \
         its line (the distinct seed files joined), not just the first; prompt was:\n{prompt}"
    );
    assert!(
        prompt.contains("the loop rule governing both core and util"),
        "the newest-recorded governing rule must render verbatim in the design-intent section; \
         prompt was:\n{prompt}"
    );
    // (2) The over-budget remainder collapses into a note naming the MULTI-SEED recovery: `rigger
    // graph --around <file>` on EACH touched file, since --around takes a single id.
    assert!(
        prompt.contains(
            "more design-intent node(s) elided to keep this prompt under budget - recover the full set with `rigger graph --around <file>` on each of: core.rs util.rs"
        ),
        "the multi-seed design-intent elision note must name `rigger graph --around <file>` on each \
         touched file (--around takes a single id); prompt was:\n{prompt}"
    );
}
