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

use std::process::Command;
use std::sync::Mutex;

use rigger::conductor::{run, AgentDriver, AgentResult, Deps, Error, SpawnOpts};
use rigger::config::{AgentDef, Config, Gate, Stage};
use rigger::contextgraph::sqlite::Projector;
use rigger::contextgraph::{
    Projection, KIND_ARCH_DECISION, KIND_DESIGN_DOC, KIND_HANDBOOK_RULE, KIND_RATIONALE,
    REL_CONSTRAINS, REL_DOC_REFERENCES, REL_EXPLAINS, REL_GOVERNS, REL_SPECIFIES,
    TYPE_CODE_ENTITY_EXTRACTED, TYPE_DECISION_MADE, TYPE_DOC_CONCEPT_EXTRACTED,
    TYPE_DOC_LINK_EXTRACTED, TYPE_LESSON_LEARNED, TYPE_REVIEW_FINDING,
};
use rigger::eventstore::sqlite::Store;
use rigger::eventstore::Event;
use rigger::gate::ExecRunner;
use rigger::grounder::{Grounder, Ref};
use rigger::spawn::ROLE_SDET_AUTHOR;
use serde_json::{json, Value};
use tempfile::TempDir;

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

/// A driver that records every spawn's `(agent id, prompt)` pair, so a test can isolate the prompt a
/// SPECIFIC role received. The `sdet-author`'s build-seam spawn is a DIFFERENT call site than the
/// implementer's (`spawn_sdet_author`) and threads its grounding slice INDEPENDENTLY, so keying the
/// capture by agent id is what lets a test pin that one call site (and reddens only for a regression
/// there, not for one at the implementer's call site). The implementer spawn also authors a one-line
/// file into its worktree, so the unit has a non-empty diff and the run carries cleanly THROUGH the
/// build seam - past the pre-gate commit and the gate - the sdet-author fires at.
struct SeamDriver {
    /// The implementer agent's id - the one spawn that authors the unit's feature file.
    implementer: String,
    /// `(agent id, prompt)` for every spawn, in spawn order.
    prompts: Mutex<Vec<(String, String)>>,
}

impl AgentDriver for SeamDriver {
    fn spawn(
        &self,
        agent: &AgentDef,
        prompt: &str,
        opts: &SpawnOpts,
        _emit: &dyn Fn(&str, Value) -> Result<(), Error>,
    ) -> Result<AgentResult, Error> {
        self.prompts
            .lock()
            .unwrap()
            .push((agent.id.clone(), prompt.to_string()));
        // The implementer authors the unit's feature into its worktree, so the pre-gate commit has a
        // non-empty diff and the unit integrates - carrying the run cleanly THROUGH the build seam
        // where the sdet-author spawn (captured above) fires.
        if agent.id == self.implementer && !opts.dir.is_empty() {
            std::fs::write(format!("{}/feature.rs", opts.dir), "// unit feature\n").unwrap();
        }
        Ok(AgentResult {
            output: String::new(),
            resolved_model: String::new(),
        })
    }
}

/// A driver for a fan-out REVIEW stage (lens -> adversary -> adjudicator). It records every spawn's
/// `(agent id, prompt)` pair so a test can isolate a SPECIFIC tier's prompt (the adversary's / the
/// adjudicator's), and it emits a ReviewFinding on the LENS's behalf - the lens's REAL work channel
/// (the review_protocol) - so the finding folds into the graph BEFORE the later tiers ground and
/// their `graph_context` can surface it. The lens id it emits for, and the finding payload, are
/// injected so a test controls exactly what a later tier must retrieve. The adjudicator returns an
/// approve verdict (its stdout IS the gating verdict); every OTHER tier returns a substantive stdout
/// so `run_reviewer` reads it as non-degenerate (Gap 18) and the run proceeds to the next tier
/// instead of respawning/halting before the adversary and adjudicator are reached.
struct ReviewDriver {
    /// The lens agent id - the one spawn that emits the ReviewFinding.
    lens: String,
    /// The adjudicator agent id - the one spawn whose stdout is the gating verdict.
    adjudicator: String,
    /// The ReviewFinding payload the lens emits (folded ABOUT the seed file).
    finding: Value,
    /// `(agent id, prompt)` for every spawn, in spawn order.
    prompts: Mutex<Vec<(String, String)>>,
}

impl AgentDriver for ReviewDriver {
    fn spawn(
        &self,
        agent: &AgentDef,
        prompt: &str,
        _opts: &SpawnOpts,
        emit: &dyn Fn(&str, Value) -> Result<(), Error>,
    ) -> Result<AgentResult, Error> {
        self.prompts
            .lock()
            .unwrap()
            .push((agent.id.clone(), prompt.to_string()));
        // The lens raises its finding to the graph (the review_protocol channel), so the adversary
        // and the adjudicator - which ground AFTER it - retrieve it through `graph_context`, never a
        // hand-threaded stdout block. This is the exact cross-tier path the implement-only trim must
        // not sever: emitting here (not pre-folding) drives the REAL fan-out threading end to end.
        if agent.id == self.lens {
            emit(TYPE_REVIEW_FINDING, self.finding.clone())?;
        }
        // The adjudicator's stdout IS its gating verdict; every other tier's stdout is discarded but
        // must be non-empty so `run_reviewer` does not read the spawn as a degenerate infrastructure
        // fault (Gap 18) and halt the run before the next tier is spawned and captured.
        let output = if agent.id == self.adjudicator {
            r#"{"verdict":"approve"}"#.to_string()
        } else {
            "reviewed".to_string()
        };
        Ok(AgentResult {
            output,
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

/// Drive `conductor::run` over a single PRODUCER (planner) stage - grounding seeded on `core.rs` via
/// the stub `SeedGrounder` - and return every prompt the driver was asked to spawn. A producer SHARES
/// the implementer spawn block but is NOT an implement stage, so spec 36 keeps its FULL grounding
/// context (the trim is implement-only): this is the observation channel for the producer-not-trimmed
/// guarantee, the exact regression a naive "trim everything that is not review" predicate would break.
fn run_and_capture_producer_prompts(graph: &Projector) -> Vec<String> {
    let grounder = SeedGrounder {
        file: "core.rs".into(),
    };
    let mut cfg = Config::default();
    cfg.agents.insert(
        "plan".into(),
        AgentDef {
            id: "plan".into(),
            ..Default::default()
        },
    );
    cfg.workflow.stages.insert(
        "p".into(),
        Stage {
            name: "p".into(),
            agent: "plan".into(),
            // A non-empty `produces` marks this a PRODUCER (planner) stage: it emits a DAG, not code.
            produces: "unit".into(),
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
        grounder: Some(&grounder),
        graph: Some(graph),
        criteria: Vec::new(),
    };
    let _ = run(&cfg, &deps);
    let prompts = driver.prompts.lock().unwrap().clone();
    prompts
}

/// The load-bearing periphery contract of criterion 1 (spec 29c): an agent's prompt, composed
/// through the public `run` path, carries the code neighborhood the ONE unified-graph traversal
/// surfaces for the touched file, and NO LONGER carries the separate structural-grounder "Relevant
/// locations" stitch the spec retires. (Spec 36 trims the capped decisions/lessons/findings bulk from
/// this implement prompt - that is covered by
/// `the_implement_prompt_is_trimmed_to_the_intent_layer_with_a_rigger_peers_pointer`, and the FULL
/// slice still carrying them by `the_producer_prompt_keeps_the_full_grounding_context_...`; this test
/// pins only the code-neighborhood-from-traversal half, which BOTH slices keep.)
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
}

/// Criterion 1 (this unit OWNS it): the IMPLEMENT prompt is TRIMMED to the deterministic intent
/// layer. For an implement-stage spawn whose seed carries decisions / lessons / findings in its
/// depth-2 neighborhood, the assembled prompt KEEPS the design-intent and code-neighborhood
/// sections and ADDS a one-line pointer naming the pull tools (`rigger_peers` for prior
/// decisions / findings, `rigger graph --around` for code navigation), and DROPS the capped
/// decisions / lessons / findings sections - the push-then-truncate bulk spec 36 replaces with
/// precise on-demand pulls. The intent layer (design intent + code neighborhood) is delivered
/// by traversal, not by retrieval luck, so the deterministic-delivery guarantee is preserved.
///
/// Non-vacuous against the pre-trim tree: before the trim the implement prompt rendered the
/// decisions / lessons / findings sections and NO pointer, so every "must OMIT" assertion and the
/// pointer assertions fail on the base; they flip green only because the implement slice now
/// renders the intent layer plus the pointer and omits the capped bulk. Mutation-isolating: the
/// same seed drives a FULL spawn (the producer / review path) unchanged, pinned by
/// `the_producer_prompt_keeps_the_full_grounding_context_not_the_implement_trim`.
#[test]
fn the_implement_prompt_is_trimmed_to_the_intent_layer_with_a_rigger_peers_pointer() {
    let graph = Projector::open(":memory:", "test").unwrap();
    let mut pos = 0u64;

    // CODE NEIGHBORHOOD (stays): a definition the run extracted from the touched file.
    fold(
        &graph,
        &mut pos,
        TYPE_CODE_ENTITY_EXTRACTED,
        json!({ "file": "core.rs", "name": "run_unit", "kind": "function", "line": 42, "lang": "rust", "fresh": true }),
    );
    // DESIGN INTENT (stays): a handbook rule that GOVERNS the touched file.
    fold_design_intent(
        &graph,
        &mut pos,
        KIND_HANDBOOK_RULE,
        "docs/handbook/loops.md",
        "the loop discipline rule governing core",
        REL_GOVERNS,
        "core.rs",
    );
    // The capped dev-loop bulk the trim DROPS from the implement prompt: a decision, a lesson, and a
    // finding, all about the SAME seed file, so the one traversal reaches every one of them and their
    // absence is the trim's doing, not a mis-seeded edge.
    fold(
        &graph,
        &mut pos,
        TYPE_DECISION_MADE,
        json!({ "id": "d_core", "summary": "TRIMMED_DECISION_MARKER the decision governing core", "governs": ["core.rs"] }),
    );
    fold(
        &graph,
        &mut pos,
        TYPE_LESSON_LEARNED,
        json!({ "id": "l_core", "summary": "TRIMMED_LESSON_MARKER the lesson about core", "about": ["core.rs"] }),
    );
    fold(
        &graph,
        &mut pos,
        TYPE_REVIEW_FINDING,
        json!({ "id": "f_core", "by": "arch", "unit": "u1", "summary": "TRIMMED_FINDING_MARKER the finding about core", "about": ["core.rs"] }),
    );

    let prompts = run_and_capture_prompts(&graph);
    assert!(
        !prompts.is_empty(),
        "the stage's agent must have been spawned"
    );
    let prompt = &prompts[0];

    // KEPT: the code neighborhood the one traversal surfaces (delivered by traversal, not retrieval).
    assert!(
        prompt.contains("run_unit") && prompt.contains("core.rs:42"),
        "the trimmed implement prompt must KEEP the code-neighborhood section; prompt was:\n{prompt}"
    );
    // KEPT: the design intent bound to the touched file.
    assert!(
        prompt.contains("the loop discipline rule governing core"),
        "the trimmed implement prompt must KEEP the design-intent section; prompt was:\n{prompt}"
    );
    // ADDED: a one-line pointer naming BOTH pull tools the reference bulk is now retrievable through.
    assert!(
        prompt.contains("rigger_peers"),
        "the trimmed implement prompt must point at `rigger_peers` for prior decisions / lessons / \
         findings; prompt was:\n{prompt}"
    );
    assert!(
        prompt.contains("rigger graph --around"),
        "the trimmed implement prompt must point at `rigger graph --around` for code navigation; \
         prompt was:\n{prompt}"
    );
    // DROPPED: the capped decisions / lessons / findings sections - proven by section header AND by
    // the unique per-section marker, so neither the header nor the bulk body can slip through.
    assert!(
        !prompt.contains("Decisions that govern these files"),
        "the trimmed implement prompt must OMIT the decisions section header; prompt was:\n{prompt}"
    );
    assert!(
        !prompt.contains("TRIMMED_DECISION_MARKER"),
        "the trimmed implement prompt must OMIT the capped decisions bulk; prompt was:\n{prompt}"
    );
    assert!(
        !prompt.contains("Lessons already learned about these files"),
        "the trimmed implement prompt must OMIT the lessons section header; prompt was:\n{prompt}"
    );
    assert!(
        !prompt.contains("TRIMMED_LESSON_MARKER"),
        "the trimmed implement prompt must OMIT the capped lessons bulk; prompt was:\n{prompt}"
    );
    assert!(
        !prompt.contains("Findings other reviewers have already raised"),
        "the trimmed implement prompt must OMIT the findings section header; prompt was:\n{prompt}"
    );
    assert!(
        !prompt.contains("TRIMMED_FINDING_MARKER"),
        "the trimmed implement prompt must OMIT the capped findings bulk; prompt was:\n{prompt}"
    );
}

/// Criterion 1 (this unit OWNS the implement-stage trim): the trim is keyed on the IMPLEMENT stage
/// SPECIFICALLY, never on "not review". A PRODUCER (the planner) SHARES the implementer spawn block
/// but is not an implement stage, so its first-wave prompt must keep the FULL grounding context - the
/// decisions/lessons/findings bulk the implement slice drops - so the planner is not blinded to the
/// prior-decomposition decisions it must not re-litigate (and is consistent with its own re-spawn,
/// which grounds through the full slice). This pins that at the PUBLIC boundary through a producer run.
///
/// Non-vacuous / mutation-isolating: the SAME seed carries a decision and a finding that the sibling
/// `the_implement_prompt_is_trimmed_...` proves are DROPPED from the implement prompt; here they must
/// be PRESENT. Regressing the shared spawn block to hand the producer the implement slice (a naive
/// not-review trim) drops both and reddens this test while leaving the implement-trim test green.
#[test]
fn the_producer_prompt_keeps_the_full_grounding_context_not_the_implement_trim() {
    let graph = Projector::open(":memory:", "test").unwrap();
    let mut pos = 0u64;

    // A decision and a finding about the seed file the producer grounds to: on the FULL slice both
    // render; on the (wrong) implement slice both would be dropped for a pointer.
    fold(
        &graph,
        &mut pos,
        TYPE_DECISION_MADE,
        json!({ "id": "d_core", "summary": "PRODUCER_DECISION_MARKER the decomposition decision governing core", "governs": ["core.rs"] }),
    );
    fold(
        &graph,
        &mut pos,
        TYPE_REVIEW_FINDING,
        json!({ "id": "f_core", "by": "arch", "unit": "u1", "summary": "PRODUCER_FINDING_MARKER the finding about core", "about": ["core.rs"] }),
    );

    let prompts = run_and_capture_producer_prompts(&graph);
    assert!(
        !prompts.is_empty(),
        "the producer stage's agent must have been spawned with a prompt"
    );
    let prompt = &prompts[0];

    // FULL: the decisions and findings bulk reaches the planner, so it is not blinded to the prior
    // decomposition decisions it must not re-litigate.
    assert!(
        prompt.contains("Decisions that govern these files")
            && prompt.contains("PRODUCER_DECISION_MARKER"),
        "the producer prompt must keep the FULL decisions section (the trim is implement-only); \
         prompt was:\n{prompt}"
    );
    assert!(
        prompt.contains("Findings other reviewers have already raised")
            && prompt.contains("PRODUCER_FINDING_MARKER"),
        "the producer prompt must keep the FULL findings section (the trim is implement-only); \
         prompt was:\n{prompt}"
    );
}

/// `git init` a throwaway repo with one empty commit - the committed HEAD an isolated unit worktree
/// branches from (mirrors the conductor's own scratch repo). A REAL repo is required for the seam
/// test: the sdet-author spawn only fires for a unit that HAS a worktree (`spawn_sdet_author` skips an
/// empty `dir`), so an `isolation: none` / repo-less run can never reach the build seam it observes.
fn init_seam_repo() -> TempDir {
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

/// Drive a real, WORKTREE-ISOLATED `conductor::run` of a single non-producer (implement) stage that
/// also has an `sdet-author` agent configured, and return every prompt the `sdet-author` spawn
/// received. The sdet-author runs at the BUILD SEAM - after the implementer emits green, in the
/// implementer's OWN worktree - so this is the only PUBLIC path that exercises the sdet-author call
/// site's grounding slice: the inside-out lifecycle tests run `graph: None` and cannot observe a
/// slice at all. Seeded on `core.rs` via the stub `SeedGrounder`, exactly like the implement/producer
/// trim tests, so the sdet-author's slice is compared against the SAME neighborhood.
fn run_and_capture_sdet_author_prompts(graph: &Projector) -> Vec<String> {
    let repo = init_seam_repo();
    let mut cfg = Config::default();
    cfg.agents.insert(
        "worker".into(),
        AgentDef {
            id: "worker".into(),
            ..Default::default()
        },
    );
    cfg.agents.insert(
        ROLE_SDET_AUTHOR.into(),
        AgentDef {
            id: ROLE_SDET_AUTHOR.into(),
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

    let grounder = SeedGrounder {
        file: "core.rs".into(),
    };
    let store = Store::open(":memory:").unwrap();
    let driver = SeamDriver {
        implementer: "worker".into(),
        prompts: Mutex::new(Vec::new()),
    };
    let deps = Deps {
        store: &store,
        driver: &driver,
        gates: &ExecRunner,
        repo: repo.path().to_str().unwrap().to_string(),
        grounder: Some(&grounder),
        graph: Some(graph),
        criteria: Vec::new(),
    };
    // The prompt is captured before the spawn returns, so the run's terminal disposition is
    // irrelevant to what this periphery layer observes.
    let _ = run(&cfg, &deps);
    let all = driver.prompts.lock().unwrap().clone();
    all.into_iter()
        .filter(|(id, _)| id == ROLE_SDET_AUTHOR)
        .map(|(_, prompt)| prompt)
        .collect()
}

/// Criterion 1 (this unit OWNS the implement-stage trim), the SDET-author call site: the build-seam
/// `sdet-author` spawn - a DIFFERENT call site than the implementer, threading its grounding slice
/// INDEPENDENTLY - also receives the TRIMMED implement slice. The sdet-author authors periphery tests
/// ALONGSIDE the implementer in the SAME worktree, so it gets the same trimmed intent layer: code
/// neighborhood + design intent + the one-line pull-tools pointer, with the capped
/// decisions/lessons/findings bulk OMITTED.
///
/// This closes a boundary the sibling tests leave open. `the_implement_prompt_is_trimmed_...` drives
/// the IMPLEMENTER call site and `the_producer_prompt_keeps_the_full_...` the producer call site, but
/// NEITHER reaches `spawn_sdet_author`: a regression flipping ONLY the sdet-author call site to the
/// full slice leaves both of them green while silently un-trimming this spawn. The inside-out
/// lifecycle tests run `graph: None`, so they cannot observe the slice at all; only a real
/// worktree-isolated run with a seeded graph does.
///
/// Non-vacuous / mutation-isolating: seeded with a decision, a lesson, and a finding about the seed
/// file (unique markers) that the FULL slice renders and the trimmed slice drops. Flipping the
/// sdet-author call site (`spawn_sdet_author`'s `GroundingSlice::Implement`) to `Full` reddens the
/// OMIT assertions here while leaving the implementer and producer tests untouched.
#[test]
fn the_sdet_author_build_seam_spawn_receives_the_trimmed_implement_slice() {
    let graph = Projector::open(":memory:", "test").unwrap();
    let mut pos = 0u64;

    // CODE NEIGHBORHOOD (stays on both slices): a definition the run extracted from the touched file.
    fold(
        &graph,
        &mut pos,
        TYPE_CODE_ENTITY_EXTRACTED,
        json!({ "file": "core.rs", "name": "run_unit", "kind": "function", "line": 42, "lang": "rust", "fresh": true }),
    );
    // DESIGN INTENT (stays on both slices): a handbook rule that GOVERNS the touched file.
    fold_design_intent(
        &graph,
        &mut pos,
        KIND_HANDBOOK_RULE,
        "docs/handbook/loops.md",
        "the loop discipline rule governing core",
        REL_GOVERNS,
        "core.rs",
    );
    // The capped dev-loop bulk the implement slice DROPS: a decision, a lesson, and a finding, all
    // about the SAME seed file, so the one traversal reaches every one and their absence is the trim's
    // doing, not a mis-seeded edge.
    fold(
        &graph,
        &mut pos,
        TYPE_DECISION_MADE,
        json!({ "id": "d_core", "summary": "SDET_TRIM_DECISION_MARKER the decision governing core", "governs": ["core.rs"] }),
    );
    fold(
        &graph,
        &mut pos,
        TYPE_LESSON_LEARNED,
        json!({ "id": "l_core", "summary": "SDET_TRIM_LESSON_MARKER the lesson about core", "about": ["core.rs"] }),
    );
    fold(
        &graph,
        &mut pos,
        TYPE_REVIEW_FINDING,
        json!({ "id": "f_core", "by": "arch", "unit": "u1", "summary": "SDET_TRIM_FINDING_MARKER the finding about core", "about": ["core.rs"] }),
    );

    let prompts = run_and_capture_sdet_author_prompts(&graph);
    assert!(
        !prompts.is_empty(),
        "the sdet-author must be spawned at the build seam so its grounding slice can be observed"
    );
    let prompt = &prompts[0];

    // KEPT: the code neighborhood the one traversal surfaces (the deterministic intent layer).
    assert!(
        prompt.contains("run_unit") && prompt.contains("core.rs:42"),
        "the sdet-author's trimmed prompt must KEEP the code-neighborhood section; prompt was:\n{prompt}"
    );
    // KEPT: the design intent bound to the touched file.
    assert!(
        prompt.contains("the loop discipline rule governing core"),
        "the sdet-author's trimmed prompt must KEEP the design-intent section; prompt was:\n{prompt}"
    );
    // ADDED: the one-line pointer naming BOTH pull tools the reference bulk is retrievable through.
    assert!(
        prompt.contains("rigger_peers"),
        "the sdet-author's trimmed prompt must point at `rigger_peers` for prior decisions / lessons \
         / findings; prompt was:\n{prompt}"
    );
    assert!(
        prompt.contains("rigger graph --around"),
        "the sdet-author's trimmed prompt must point at `rigger graph --around` for code navigation; \
         prompt was:\n{prompt}"
    );
    // DROPPED: the capped decisions / lessons / findings sections - proven by section header AND by
    // the unique per-section marker, so neither the header nor the bulk body can slip through.
    assert!(
        !prompt.contains("Decisions that govern these files"),
        "the sdet-author's trimmed prompt must OMIT the decisions section header; prompt was:\n{prompt}"
    );
    assert!(
        !prompt.contains("SDET_TRIM_DECISION_MARKER"),
        "the sdet-author's trimmed prompt must OMIT the capped decisions bulk; prompt was:\n{prompt}"
    );
    assert!(
        !prompt.contains("Lessons already learned about these files"),
        "the sdet-author's trimmed prompt must OMIT the lessons section header; prompt was:\n{prompt}"
    );
    assert!(
        !prompt.contains("SDET_TRIM_LESSON_MARKER"),
        "the sdet-author's trimmed prompt must OMIT the capped lessons bulk; prompt was:\n{prompt}"
    );
    assert!(
        !prompt.contains("Findings other reviewers have already raised"),
        "the sdet-author's trimmed prompt must OMIT the findings section header; prompt was:\n{prompt}"
    );
    assert!(
        !prompt.contains("SDET_TRIM_FINDING_MARKER"),
        "the sdet-author's trimmed prompt must OMIT the capped findings bulk; prompt was:\n{prompt}"
    );
}

/// Drive a real fan-out REVIEW stage (lens then adversary then adjudicator) through the public `run`
/// path, grounding every tier on `core.rs` via the stub `SeedGrounder`, with the lens emitting one
/// ReviewFinding about that file, and return every spawn's `(agent id, prompt)` pair. This is the
/// observation channel for the review-not-blinded guardrail: the adversary and the adjudicator ground
/// AFTER the lens, so their prompts are where a finding the lens raised must still appear. A review
/// tier's prompt is assembled by `build_review_prompt` then `build_prompt` at `GroundingSlice::Full`,
/// the SAME full slice the producer keeps and the implement slice drops, so a regression that hands
/// review the implement slice surfaces HERE, at the review call site.
fn run_and_capture_review_prompts(graph: &Projector, finding: Value) -> Vec<(String, String)> {
    let grounder = SeedGrounder {
        file: "core.rs".into(),
    };
    let mut cfg = Config::default();
    for id in ["lens", "adversary", "adj"] {
        cfg.agents.insert(
            id.into(),
            AgentDef {
                id: id.into(),
                ..Default::default()
            },
        );
    }
    cfg.workflow.stages.insert(
        "review".into(),
        Stage {
            name: "review".into(),
            // An empty `agent` with a non-empty `agents` lens list is what marks this a fan-out REVIEW
            // stage (`is_fan_out`), so the three tiers run and communicate THROUGH the graph: the lens
            // emits, then the adversary and adjudicator ground and retrieve it.
            agents: vec!["lens".into()],
            adversary: "adversary".into(),
            adjudicator: "adj".into(),
            coverage: "core".into(),
            ..Default::default()
        },
    );

    let store = Store::open(":memory:").unwrap();
    let driver = ReviewDriver {
        lens: "lens".into(),
        adjudicator: "adj".into(),
        finding,
        prompts: Mutex::new(Vec::new()),
    };
    let deps = Deps {
        store: &store,
        driver: &driver,
        gates: &ExecRunner,
        // A repo-less run: a standalone review stage owns no code to integrate, so it needs no
        // worktree, and its reviewers run in the project cwd (`assert_isolated_cwd` is a no-op with no
        // repo). This mirrors the lib lens-finding test's setup (both drive the public `run` path);
        // only the DRIVER differs.
        repo: String::new(),
        grounder: Some(&grounder),
        graph: Some(graph),
        criteria: Vec::new(),
    };
    // Prompts are captured before each spawn returns, so the run's terminal disposition is irrelevant
    // to what this periphery layer observes.
    let _ = run(&cfg, &deps);
    let all = driver.prompts.lock().unwrap().clone();
    all
}

/// Criterion 3 (this unit OWNS the review-path-preservation guardrail): the spec-36 trim is
/// IMPLEMENT-ONLY, so it must NOT weaken review. Driven through a REAL fan-out review over the public
/// `run` path, a finding the LENS raises must STILL reach the ADVERSARY and the ADJUDICATOR (which
/// ground after it) under `graph_context`'s findings header, so neither review tier is blinded.
///
/// The review-not-blinded SEAM is ALSO regression-covered upstream by the pre-existing lib test
/// `conductor::tests::lens_finding_reaches_later_tiers_through_the_graph`, which drives the SAME
/// public `run` fan-out, emits a lens ReviewFinding, asserts the adversary and adjudicator prompts
/// carry it under the same findings header, and reddens on the SAME `build_prompt`
/// `GroundingSlice::Full`-to-`::Implement` flip. This co-located test does NOT claim unique regression
/// coverage over it. Its value is (a) sibling-family cohesion: this file already holds the co-located
/// slice-policy family - `the_implement_prompt_is_trimmed_` and `the_sdet_author_build_seam_` prove
/// their call sites DROP the findings and `the_producer_prompt_keeps_the_full_` proves the producer
/// KEEPS them - and this completes it so all four slice call sites are legible and guarded in ONE
/// periphery file; and (b) a named, co-located guard on a correctness seam (blinding review is a
/// regression per spec-36) that survives future edits to that lib test: if its review assertions are
/// later refactored, this guard still reddens on the `build_prompt` Full-slice flip at the review
/// call site.
///
/// Non-vacuous / mutation-isolating: the lens's finding carries a unique marker the FULL slice renders
/// and the implement slice would drop. Flipping the review call site (`build_prompt`'s
/// `GroundingSlice::Full` to `::Implement`) reddens BOTH assertions here - the adversary and
/// adjudicator would then receive the trimmed slice and lose the findings section - while leaving the
/// implement/producer/sdet-author trim tests (which exercise their OWN call sites) untouched.
#[test]
fn the_review_prompt_keeps_the_full_findings_so_a_lens_finding_reaches_the_adversary_and_adjudicator(
) {
    let graph = Projector::open(":memory:", "test").unwrap();

    // A lens raises one finding about the seed file. On the FULL review slice the adversary and the
    // adjudicator retrieve it through `graph_context`; on the (wrong) implement slice it would be
    // dropped for a pull-tool pointer and review would be blinded to the reviewer that raised it.
    let finding = json!({
        "id": "f_review",
        "by": "lens:lens",
        "unit": "u1",
        "summary": "REVIEW_FINDING_MARKER the lens finding the later review tiers must retrieve",
        "about": ["core.rs"],
    });
    let prompts = run_and_capture_review_prompts(&graph, finding);

    let prompt_for = |role: &str| -> String {
        prompts
            .iter()
            .find(|(id, _)| id == role)
            .unwrap_or_else(|| {
                panic!(
                    "the {role:?} review tier must have been spawned; spawns were:\n{prompts:#?}"
                )
            })
            .1
            .clone()
    };

    // The ADVERSARY grounds after the lens, so the lens's finding must reach it under the
    // `graph_context` findings header - proven by the section header AND the unique finding marker, so
    // neither an empty header nor an incidental match can pass for the retrieved finding.
    let adv = prompt_for("adversary");
    assert!(
        adv.contains("Findings other reviewers have already raised")
            && adv.contains("REVIEW_FINDING_MARKER"),
        "the adversary's review prompt must keep the FULL findings section so a lens finding reaches \
         it (the trim is implement-only; review is not blinded); prompt was:\n{adv}"
    );

    // The ADJUDICATOR grounds last and gates the stage; it too must retrieve the lens's finding, or
    // its verdict would be blind to what the lens raised.
    let adj = prompt_for("adj");
    assert!(
        adj.contains("Findings other reviewers have already raised")
            && adj.contains("REVIEW_FINDING_MARKER"),
        "the adjudicator's review prompt must keep the FULL findings section so a lens finding reaches \
         it (the trim is implement-only; review is not blinded); prompt was:\n{adj}"
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
/// for the touched files. A run whose graph carries design intent about the file but no extracted
/// definition for it must not emit a bare, dangling "Code neighborhood" header with nothing under it,
/// since empty scaffolding is noise in an agent's prompt. This guards the empty-section suppression at
/// the same public boundary: the inside-out unit test asserts the private renderer writes no header;
/// this
/// asserts the composed prompt an agent receives carries none either, while still surfacing the design
/// intent - so the one traversal ran and reached the file, it simply had no code neighborhood to
/// render.
///
/// Proof of reach is the DESIGN INTENT bound to the file, not a decision: spec 36 keeps the design
/// intent on the implement prompt but trims the decisions bulk, so the design-intent binding is the
/// implement-slice signal that the traversal reached `core.rs`. Non-vacuous: that binding renders, so
/// a renderer that emitted the code-neighborhood header unconditionally (before the empty-candidate
/// early return) would flip the header-absence assertion while the design-intent assertion still held.
#[test]
fn a_spawn_prompt_with_no_extracted_definitions_renders_no_code_neighborhood_header() {
    let graph = Projector::open(":memory:", "test").unwrap();
    let mut pos = 0u64;

    // A DESIGN-INTENT node bound to the touched file but NO code entity for it: the seeded traversal
    // reaches the file (a handbook rule GOVERNS it) yet has zero definitions to render as a code
    // neighborhood. Design intent stays on the trimmed implement prompt, so it is the reach signal.
    fold_design_intent(
        &graph,
        &mut pos,
        KIND_HANDBOOK_RULE,
        "docs/handbook/loops.md",
        "the loop rule that governs core",
        REL_GOVERNS,
        "core.rs",
    );

    let prompts = run_and_capture_prompts(&graph);
    assert!(
        !prompts.is_empty(),
        "the stage's agent must have been spawned"
    );
    let prompt = &prompts[0];

    // The one traversal ran and reached the file: its design intent is surfaced.
    assert!(
        prompt.contains("the loop rule that governs core"),
        "the one traversal must still surface the design intent bound to the file; prompt \
         was:\n{prompt}"
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

    // The unified graph the run populates: a DESIGN-INTENT node bound ABOUT `combat.rs` - the file
    // turbovec resolves the query to. Its title is a marker that appears NOWHERE in the query or the
    // repo source, so surfacing it in the prompt can ONLY be the seeded traversal reaching
    // `combat.rs`. Design intent (not a decision) is the marker because spec 36 keeps the design
    // intent on the trimmed implement prompt this run assembles, while it trims the decisions bulk.
    let graph = Projector::open(":memory:", "test").unwrap();
    let mut pos = 0u64;
    fold_design_intent(
        &graph,
        &mut pos,
        KIND_HANDBOOK_RULE,
        "docs/handbook/combat.md",
        "the design note that governs the combat file",
        REL_GOVERNS,
        "combat.rs",
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
         neighborhood (the design intent about combat.rs) the one seeded traversal reaches via the \
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
/// PUBLIC prompt boundary of the trimmed implement prompt (design intent stays on the implement slice;
/// spec 36 trims the decisions section from it, so the guard is that a decision governing the file
/// never LEAKS into the design-intent section that DOES render). Non-vacuous: the decision GOVERNS the
/// SAME file at the SAME depth as the handbook rule that DOES render, so the kind guard - not
/// reachability - is what keeps it out. Mutation-proven: replacing the kind check with `true` leaks
/// the decision as `- GOVERNS core.rs  d_leak_decision: ` (empty title) into the rendered
/// design-intent section, flipping the negative assertion.
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
    // NON-VACUITY: the decision GOVERNS the SAME file at the SAME depth-1 as the handbook rule that
    // DID render above, so it is unquestionably in the seeded subgraph; the kind guard, not
    // reachability, is what keeps it out of the design-intent section. (The trimmed implement prompt
    // no longer renders a decisions section at all - `the_implement_prompt_is_trimmed_...` owns that -
    // so the guard here is purely that the decision does not LEAK into the design-intent section.)
    // THE GUARD: the governing decision must NOT render as a design-intent line. Its leaked
    // design-intent form would be `- GOVERNS core.rs  d_leak_decision: ` (rel, file, then the id with
    // an empty title, since a decision carries `summary`, not `title`), so this exact substring can
    // only appear if the decision leaked through the shared GOVERNS relation past the kind guard.
    assert!(
        !prompt.contains(&format!("{REL_GOVERNS} core.rs  d_leak_decision")),
        "a governing DECISION must be excluded from the design-intent section by the kind guard - it \
         is retrievable via `rigger_peers`, not rendered here; prompt was:\n{prompt}"
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
