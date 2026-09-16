//! The workflow-definition-to-events emit pass (spec 92 criterion 2, THE WHOLE PRODUCT IS
//! COVERED): lowers `.rigger/workflow.yml`'s stages, gates and agent roles into
//! `DocConceptExtracted` / `DocLinkExtracted` events - REUSING the exact events and fold the
//! design-intent pass (spec 29b) uses, never a second entity/edge-fold authority (see
//! `crate::contextgraph`'s fold-arm doc) - so the workflow definition becomes graph entities
//! (`stage:<name>`, `gate:<name>`, `agent:<name>`, matching the Design text's own
//! `stage:implement` / `gate:mutation` / `agent:rust-engineer` example) with `needs` / `runs` /
//! `reviews` relations - the last split into a plain edge for a panel's full-only roster and a
//! distinctly-tagged one for its opt-in `tiers.light` roster (see `reviewers_of`'s own doc for
//! why the two are never unioned). This is the emit half; the fold half lives in
//! `contextgraph::sqlite` and stays compiled in both lanes.

use crate::config::{self, ReviewPanel, Stage, Workflow};
use crate::contextgraph::{
    DocConceptExtracted, DocLinkExtracted, KIND_AGENT, KIND_GATE, KIND_STAGE, REL_NEEDS,
    REL_REVIEWS, REL_REVIEWS_LIGHT, REL_RUNS, TYPE_DOC_CONCEPT_EXTRACTED, TYPE_DOC_LINK_EXTRACTED,
};
use crate::eventstore::Event;
use std::collections::BTreeSet;
use std::path::Path;

/// The one relative path every entity/edge this pass extracts is attributed to (the `doc` attr on
/// every folded node): `.rigger/workflow.yml` is ALWAYS the source, so this is a constant, never a
/// parameter threaded through every call.
const WORKFLOW_DOC: &str = ".rigger/workflow.yml";

fn stage_id(name: &str) -> String {
    format!("stage:{name}")
}

fn gate_id(name: &str) -> String {
    format!("gate:{name}")
}

fn agent_id(name: &str) -> String {
    format!("agent:{name}")
}

/// One `(kind, id, title)` concept tuple, before it is lowered into a `DocConceptExtracted`
/// payload - see [`extract`]'s own doc.
type ConceptTuple = (&'static str, String, String);

/// One `(from, rel, to)` link tuple, before it is lowered into a `DocLinkExtracted` payload - see
/// [`extract`]'s own doc.
type LinkTuple = (String, &'static str, String);

/// Lower `workflow`'s stages, gates and agent roles into `(kind, id, title)` concepts and
/// `(from, rel, to)` links - the pure computation [`extract_events`] serializes. Kept separate
/// from serialization so a test can assert on the plain tuples without decoding JSON.
///
/// Per stage: `stage --NEEDS--> stage` for each `stage.needs` entry; `stage --RUNS--> gate` for
/// each `stage.gates` entry; `stage --RUNS--> agent` for `stage.agent` / each of `stage.agents`
/// (the stage runs under this implementer). `agent --REVIEWS--> stage` comes from whichever
/// source the stage actually declares: a standalone stage's own `adversary:` / `adjudicator:`
/// fields when set (`plan-critique`'s shape), plus any `review:` override's roster; and ONLY when
/// neither of those names anyone AND the stage runs at least one gate (the signal its units go
/// through review) does it fall back to [`Workflow::effective_review_panel`] - the workflow's
/// `defaults.review`, unless the stage overrides it - so a gate-less, review-less stage like
/// `plan` never wrongly inherits the workflow-wide panel.
///
/// Every declared gate (`workflow.gates`, whether or not any stage runs it) and every agent role
/// named anywhere above becomes its own concept too, so "gates... become graph entities" holds
/// for the whole definition, not merely the ones a stage happens to reference.
fn extract(workflow: &Workflow) -> (Vec<ConceptTuple>, Vec<LinkTuple>) {
    let mut concepts: Vec<ConceptTuple> = Vec::new();
    let mut links: Vec<LinkTuple> = Vec::new();
    let mut gate_names: BTreeSet<String> = workflow.gates.keys().cloned().collect();
    let mut agent_names: BTreeSet<String> = BTreeSet::new();

    for (name, stage) in &workflow.stages {
        concepts.push((KIND_STAGE, stage_id(name), name.clone()));

        for need in &stage.needs {
            links.push((stage_id(name), REL_NEEDS, stage_id(need)));
        }
        for gate in &stage.gates {
            links.push((stage_id(name), REL_RUNS, gate_id(gate)));
            gate_names.insert(gate.clone());
        }
        if !stage.agent.is_empty() {
            links.push((stage_id(name), REL_RUNS, agent_id(&stage.agent)));
            agent_names.insert(stage.agent.clone());
        }
        for a in &stage.agents {
            links.push((stage_id(name), REL_RUNS, agent_id(a)));
            agent_names.insert(a.clone());
        }

        let (full_reviewers, light_reviewers) = reviewers_of(workflow, stage);
        for r in full_reviewers {
            links.push((agent_id(&r), REL_REVIEWS, stage_id(name)));
            agent_names.insert(r);
        }
        for r in light_reviewers {
            links.push((agent_id(&r), REL_REVIEWS_LIGHT, stage_id(name)));
            agent_names.insert(r);
        }
    }

    for name in &gate_names {
        concepts.push((KIND_GATE, gate_id(name), name.clone()));
    }
    for name in &agent_names {
        concepts.push((KIND_AGENT, agent_id(name), name.clone()));
    }

    concepts.sort_by(|a, b| a.0.cmp(b.0).then_with(|| a.1.cmp(&b.1)));
    concepts.dedup_by(|a, b| a.0 == b.0 && a.1 == b.1);
    links.sort_by(|a, b| {
        a.1.cmp(b.1)
            .then_with(|| a.0.cmp(&b.0))
            .then_with(|| a.2.cmp(&b.2))
    });
    links.dedup();

    (concepts, links)
}

/// The FULL-panel-only reviewer agent ids a review panel names on its own top-level roster
/// (lenses, adversary, adjudicator) - the roster a unit at this panel's stage reviews itself with
/// whenever it is NOT routed to the panel's opt-in `tiers.light` reduced roster instead.
/// Deliberately excludes that light roster (see [`light_reviewers_of`]): unlike
/// [`config::ReviewPanel::agent_ids`], which unions both rosters for REFERENTIAL VALIDATION (a
/// different question - "does this id resolve to a real agent"), this asks "which agent reviews a
/// unit that stays on the full panel", and a light-only agent is never that.
fn full_reviewers_of(panel: &ReviewPanel) -> Vec<String> {
    let mut ids = panel.lenses.clone();
    if !panel.adversary.is_empty() {
        ids.push(panel.adversary.clone());
    }
    if !panel.adjudicator.is_empty() {
        ids.push(panel.adjudicator.clone());
    }
    ids
}

/// The reviewer agent ids a review panel's OPT-IN `tiers.light` reduced roster names, if
/// configured - the roster a LOW-risk unit routes to INSTEAD of [`full_reviewers_of`]'s roster,
/// never alongside it. Empty when the panel names no depth policy (the shipped default),
/// mirroring [`config::ReviewPanel::depth`].
fn light_reviewers_of(panel: &ReviewPanel) -> Vec<String> {
    panel
        .depth()
        .map(|depth| full_reviewers_of(&depth.light))
        .unwrap_or_default()
}

/// The reviewer agent ids a stage's `REVIEWS` / `REVIEWS_LIGHT` edges are drawn from, split by
/// which relation each belongs on - see [`extract`]'s own doc for the source-selection rule (a
/// stage's own direct fields/review panel, else - when gated - the workflow's `defaults.review`).
/// `.0` feeds the plain [`REL_REVIEWS`] edge: a stage's own direct `adversary:`/`adjudicator:`
/// fields (which carry no tiers concept of their own) plus the resolved panel's FULL-only roster
/// ([`full_reviewers_of`]). `.1` feeds the distinctly-tagged [`REL_REVIEWS_LIGHT`] edge: the SAME
/// resolved panel's `tiers.light` roster ([`light_reviewers_of`]). Kept apart rather than folded
/// into one `.agent_ids()` union because a real run routes each unit to light XOR full exclusively
/// by its observable risk (`config::Workflow::tiers` doc): an undistinguished edge would assert
/// that a light-tier-only agent reviews a HIGH-risk unit at this stage (and the reverse for a
/// full-panel-only agent under a LOW-risk routing) - a real accuracy defect, not mere
/// incompleteness. Never empty-string entries (an unset `adversary:` / `adjudicator:` field
/// defaults to `""`, filtered here rather than by every caller).
fn reviewers_of(workflow: &Workflow, stage: &Stage) -> (Vec<String>, Vec<String>) {
    let mut full: Vec<String> = Vec::new();
    let mut light: Vec<String> = Vec::new();
    if !stage.adversary.is_empty() {
        full.push(stage.adversary.clone());
    }
    if !stage.adjudicator.is_empty() {
        full.push(stage.adjudicator.clone());
    }
    full.extend(full_reviewers_of(&stage.review));
    light.extend(light_reviewers_of(&stage.review));
    if full.is_empty() && light.is_empty() && !stage.gates.is_empty() {
        let panel = workflow.effective_review_panel(stage);
        full.extend(full_reviewers_of(panel));
        light.extend(light_reviewers_of(panel));
    }
    full.retain(|r| !r.is_empty());
    light.retain(|r| !r.is_empty());
    (full, light)
}

/// Lower `workflow`'s stages, gates and agent roles into events: one `DocConceptExtracted` per
/// entity (sorted by kind then id, mirroring
/// [`crate::grounder::design::events::concept_events`]'s own ordering), THEN one
/// `DocLinkExtracted` per relation (sorted by rel then from then to) - concepts before links so a
/// link's endpoint is ensured at its SPECIFIC kind when its own concept event folds first
/// (`ensure_node` never demotes an already-specific kind back to the generic role a link alone
/// would ensure), mirroring the design-intent pass's own concept-before-link ordering. Pure and
/// deterministic: identical input yields byte-identical output, entirely from `workflow`'s own
/// `BTreeMap`-ordered stages/gates - no filesystem, no clock, no randomness.
pub fn extract_events(workflow: &Workflow) -> Vec<Event> {
    let (concepts, links) = extract(workflow);
    let mut events: Vec<Event> = concepts
        .into_iter()
        .map(|(kind, id, title)| {
            let payload = DocConceptExtracted {
                kind: kind.to_string(),
                id,
                title,
                doc: WORKFLOW_DOC.to_string(),
            };
            Event::new(
                TYPE_DOC_CONCEPT_EXTRACTED,
                serde_json::to_vec(&payload)
                    .expect("workflow-definition concept payload serializes"),
            )
        })
        .collect();
    events.extend(links.into_iter().map(|(from, rel, to)| {
        let payload = DocLinkExtracted {
            from,
            to,
            rel: rel.to_string(),
        };
        Event::new(
            TYPE_DOC_LINK_EXTRACTED,
            serde_json::to_vec(&payload).expect("workflow-definition link payload serializes"),
        )
    }));
    events
}

/// Load `.rigger/workflow.yml` at `root` and lower it into events - the production entry point a
/// live run's [`crate::ingest`] pipeline and a cold `rigger graph build` both use. Reads through
/// the LIGHTWEIGHT [`config::load_workflow`] (no agents-dir read, no referential validation) - see
/// that function's own doc for why indexing the definition must not fail on an unrelated agent
/// frontmatter concern. Absent or unparseable yields NO events (never a crash), mirroring how a
/// design doc with no design intent yields nothing to the design extraction pass.
pub fn project_events(root: &str) -> Vec<Event> {
    let path = Path::new(root).join(".rigger").join("workflow.yml");
    match config::load_workflow(&path) {
        Ok(wf) => extract_events(&wf),
        Err(_) => Vec::new(),
    }
}

/// [`project_events`] as ONE keyed file batch (`.rigger/workflow.yml`, its events) - the shape
/// [`crate::ingest`]'s walk folds alongside the code (`gc`) and design (`gd`) halves, under its
/// own `gw` prefix. Empty when there is nothing to extract ([`project_events`] returned no
/// events), so an absent/unparseable workflow contributes no batch at all - never an
/// empty-but-present one the caller would key and append pointlessly.
pub fn project_batches(root: &str) -> Vec<(String, Vec<Event>)> {
    let events = project_events(root);
    if events.is_empty() {
        Vec::new()
    } else {
        vec![(WORKFLOW_DOC.to_string(), events)]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Gate;
    use std::collections::BTreeMap;

    /// A small fixture workflow mirroring this project's OWN `.rigger/workflow.yml` shape closely
    /// enough to exercise every relation source: `plan` (agent only, no gates - no REVIEWS),
    /// `plan-critique` (no agent, direct adversary/adjudicator - REVIEWS from those), `implement`
    /// (agent + gates, no direct review fields - REVIEWS falls back to `defaults.review`), and
    /// `checkin` (needs implement, its own gate).
    fn fixture() -> Workflow {
        let mut gates = BTreeMap::new();
        gates.insert(
            "fmt".to_string(),
            Gate {
                run: "cargo fmt --check".to_string(),
                kind: "core".to_string(),
                inputs: Vec::new(),
            },
        );
        gates.insert(
            "mutation".to_string(),
            Gate {
                run: "cargo mutants".to_string(),
                kind: "core".to_string(),
                inputs: Vec::new(),
            },
        );

        let mut stages = BTreeMap::new();
        stages.insert(
            "plan".to_string(),
            Stage {
                name: "plan".to_string(),
                agent: "planner".to_string(),
                ..Default::default()
            },
        );
        stages.insert(
            "plan-critique".to_string(),
            Stage {
                name: "plan-critique".to_string(),
                needs: vec!["plan".to_string()],
                adversary: "adversary".to_string(),
                adjudicator: "adjudicator".to_string(),
                ..Default::default()
            },
        );
        stages.insert(
            "implement".to_string(),
            Stage {
                name: "implement".to_string(),
                needs: vec!["plan-critique".to_string()],
                agent: "rust-engineer".to_string(),
                gates: vec!["fmt".to_string()],
                ..Default::default()
            },
        );
        stages.insert(
            "checkin".to_string(),
            Stage {
                name: "checkin".to_string(),
                needs: vec!["implement".to_string()],
                agent: "rust-engineer".to_string(),
                gates: vec!["fmt".to_string(), "mutation".to_string()],
                ..Default::default()
            },
        );

        Workflow {
            gates,
            stages,
            defaults: config::Defaults {
                review: ReviewPanel {
                    lenses: vec!["architecture-reviewer".to_string(), "sdet".to_string()],
                    adversary: "adversary".to_string(),
                    adjudicator: "adjudicator".to_string(),
                    ..Default::default()
                },
                ..Default::default()
            },
            ..Default::default()
        }
    }

    #[test]
    fn stages_gates_and_agents_all_become_concepts() {
        let (concepts, _links) = extract(&fixture());
        let has = |kind: &str, id: &str| concepts.iter().any(|(k, i, _)| *k == kind && i == id);
        // The Design text's own example triple, matching this workflow's real shape.
        assert!(has(KIND_STAGE, "stage:implement"), "got {concepts:?}");
        assert!(has(KIND_GATE, "gate:mutation"), "got {concepts:?}");
        assert!(has(KIND_AGENT, "agent:rust-engineer"), "got {concepts:?}");
        // Every declared gate becomes an entity, even `fmt` which every stage runs - and a gate
        // NO stage runs would too (the loop covers `workflow.gates` independent of stage refs).
        assert!(has(KIND_GATE, "gate:fmt"), "got {concepts:?}");
        // Every stage becomes an entity, including the review-only `plan-critique`.
        assert!(has(KIND_STAGE, "stage:plan-critique"), "got {concepts:?}");
        assert!(has(KIND_STAGE, "stage:plan"), "got {concepts:?}");
        // The review-panel agents (inherited via defaults.review) become entities too.
        assert!(
            has(KIND_AGENT, "agent:architecture-reviewer"),
            "got {concepts:?}"
        );
        assert!(has(KIND_AGENT, "agent:adversary"), "got {concepts:?}");
        assert!(has(KIND_AGENT, "agent:adjudicator"), "got {concepts:?}");
        // No duplicate (kind, id) pair despite `rust-engineer` and `adversary`/`adjudicator`
        // appearing across multiple stages.
        let mut ids: Vec<&String> = concepts.iter().map(|(_, id, _)| id).collect();
        let before = ids.len();
        ids.sort();
        ids.dedup();
        assert_eq!(
            ids.len(),
            before,
            "a concept id must appear at most once, got {concepts:?}"
        );
    }

    #[test]
    fn needs_and_runs_edges_match_the_yaml_lists() {
        let (_concepts, links) = extract(&fixture());
        let has = |from: &str, rel: &str, to: &str| {
            links
                .iter()
                .any(|(f, r, t)| f == from && *r == rel && t == to)
        };
        assert!(
            has("stage:implement", REL_NEEDS, "stage:plan-critique"),
            "got {links:?}"
        );
        assert!(
            has("stage:checkin", REL_NEEDS, "stage:implement"),
            "got {links:?}"
        );
        assert!(
            has("stage:checkin", REL_RUNS, "gate:mutation"),
            "got {links:?}"
        );
        assert!(has("stage:checkin", REL_RUNS, "gate:fmt"), "got {links:?}");
        assert!(
            has("stage:implement", REL_RUNS, "agent:rust-engineer"),
            "the stage RUNS its own assigned implementer agent; got {links:?}"
        );
        assert!(
            has("stage:plan", REL_RUNS, "agent:planner"),
            "got {links:?}"
        );
    }

    #[test]
    fn reviews_edges_prefer_a_stages_own_fields_and_fall_back_to_defaults_review_only_when_gated() {
        let (_concepts, links) = extract(&fixture());
        let has = |from: &str, to: &str| {
            links
                .iter()
                .any(|(f, r, t)| f == from && *r == REL_REVIEWS && t == to)
        };
        // plan-critique names its own adversary/adjudicator directly - those, and ONLY those.
        assert!(
            has("agent:adversary", "stage:plan-critique"),
            "got {links:?}"
        );
        assert!(
            has("agent:adjudicator", "stage:plan-critique"),
            "got {links:?}"
        );
        // implement/checkin declare no adversary/adjudicator/review of their own but DO run
        // gates, so they inherit the workflow's defaults.review panel (lenses + adversary +
        // adjudicator).
        for stage in ["stage:implement", "stage:checkin"] {
            assert!(has("agent:architecture-reviewer", stage), "got {links:?}");
            assert!(has("agent:sdet", stage), "got {links:?}");
            assert!(has("agent:adversary", stage), "got {links:?}");
            assert!(has("agent:adjudicator", stage), "got {links:?}");
        }
        // plan runs no gates and declares no review of its own: it must NOT wrongly inherit
        // defaults.review - the false-positive this guard exists to prevent.
        assert!(
            !links
                .iter()
                .any(|(_, r, t)| *r == REL_REVIEWS && t == "stage:plan"),
            "a gate-less, review-less stage must get no REVIEWS edge at all, got {links:?}"
        );
    }

    #[test]
    fn a_tiers_light_only_reviewer_never_shows_up_as_an_indistinguishable_full_panel_reviewer() {
        // A real run routes each unit to the light OR the full panel EXCLUSIVELY by observable
        // risk (`Workflow::tiers` doc) - never both. So a light-only agent (never on the full
        // panel's own lenses/adversary/adjudicator) must land on a DISTINCTLY-TAGGED edge, never
        // the plain REL_REVIEWS edge the full panel's own roster gets - and vice versa.
        let mut wf = fixture();
        wf.defaults.review.tiers = Some(Box::new(config::ReviewDepth {
            light: ReviewPanel {
                lenses: vec!["fast-lens".to_string()],
                adjudicator: "light-adjudicator".to_string(),
                ..Default::default()
            },
            threshold: 4,
            ..Default::default()
        }));

        let (_concepts, links) = extract(&wf);
        let has = |from: &str, rel: &str, to: &str| {
            links
                .iter()
                .any(|(f, r, t)| f == from && *r == rel && t == to)
        };

        for stage in ["stage:implement", "stage:checkin"] {
            // The light-only roster lands on its own, distinctly-tagged edge...
            assert!(
                has("agent:fast-lens", REL_REVIEWS_LIGHT, stage),
                "got {links:?}"
            );
            assert!(
                has("agent:light-adjudicator", REL_REVIEWS_LIGHT, stage),
                "got {links:?}"
            );
            // ...and NEVER on the plain REVIEWS edge the full panel's own roster gets - that
            // would make it indistinguishable from a full-panel reviewer.
            assert!(
                !has("agent:fast-lens", REL_REVIEWS, stage),
                "a light-only reviewer must never be indistinguishable from a full-panel \
                 reviewer; got {links:?}"
            );
            assert!(
                !has("agent:light-adjudicator", REL_REVIEWS, stage),
                "got {links:?}"
            );
            // The full panel's own roster is untouched by tiering: still the plain edge.
            assert!(has("agent:architecture-reviewer", REL_REVIEWS, stage));
            assert!(has("agent:sdet", REL_REVIEWS, stage));
            assert!(has("agent:adversary", REL_REVIEWS, stage));
            assert!(has("agent:adjudicator", REL_REVIEWS, stage));
            // ...and the full panel's own roster never doubles onto the light edge either.
            assert!(!has("agent:adjudicator", REL_REVIEWS_LIGHT, stage));
        }
    }

    #[test]
    fn a_real_on_disk_workflow_yml_with_tiers_still_tags_the_light_roster_distinctly() {
        // Same guarantee as the in-process fixture above, but through a REAL on-disk
        // `.rigger/workflow.yml` parsed by `config::load_workflow` (via `project_events`) - not a
        // hand-built `Workflow` struct - proving the split survives real YAML parsing end to end.
        let dir = tempfile::tempdir().unwrap();
        let rigger_dir = dir.path().join(".rigger");
        std::fs::create_dir_all(&rigger_dir).unwrap();
        let yaml = "name: w\n\
defaults:\n  \
review:\n    \
lenses: [archlens]\n    \
adversary: adv\n    \
adjudicator: adj\n    \
tiers:\n      \
threshold: 1\n      \
light:\n        \
lenses: [fast-lens]\n        \
adjudicator: light-adjudicator\n\
stages:\n  \
implement:\n    \
agent: worker\n    \
gates: [fmt]\n\
gates:\n  \
fmt:\n    \
run: cargo fmt --check\n";
        std::fs::write(rigger_dir.join("workflow.yml"), yaml).unwrap();

        let events = project_events(dir.path().to_str().unwrap());
        let has_link = |from: &str, rel: &str, to: &str| {
            events.iter().any(|e| {
                if e.type_ != TYPE_DOC_LINK_EXTRACTED {
                    return false;
                }
                let l: DocLinkExtracted = serde_json::from_slice(&e.data).unwrap();
                l.from == from && l.rel == rel && l.to == to
            })
        };
        assert!(
            has_link("agent:fast-lens", REL_REVIEWS_LIGHT, "stage:implement"),
            "got {events:?}"
        );
        assert!(
            has_link(
                "agent:light-adjudicator",
                REL_REVIEWS_LIGHT,
                "stage:implement"
            ),
            "got {events:?}"
        );
        assert!(
            !has_link("agent:fast-lens", REL_REVIEWS, "stage:implement"),
            "the light-only lens must never be indistinguishable from a full-panel reviewer; \
             got {events:?}"
        );
        assert!(has_link("agent:archlens", REL_REVIEWS, "stage:implement"));
        assert!(has_link("agent:adv", REL_REVIEWS, "stage:implement"));
        assert!(has_link("agent:adj", REL_REVIEWS, "stage:implement"));
    }

    #[test]
    fn extract_events_is_deterministic_across_repeated_calls() {
        let wf = fixture();
        let a = extract_events(&wf);
        let b = extract_events(&wf);
        let ser = |evs: &[Event]| -> Vec<(String, Vec<u8>)> {
            evs.iter()
                .map(|e| (e.type_.clone(), e.data.clone()))
                .collect()
        };
        assert_eq!(
            ser(&a),
            ser(&b),
            "identical input must yield byte-identical events on every call"
        );
        assert!(!a.is_empty(), "the fixture must actually extract something");
    }

    #[test]
    fn project_events_on_a_missing_workflow_yields_nothing_never_a_crash() {
        let dir = tempfile::tempdir().unwrap();
        assert!(project_events(dir.path().to_str().unwrap()).is_empty());
        assert!(project_batches(dir.path().to_str().unwrap()).is_empty());
    }

    #[test]
    fn project_events_reads_this_projects_own_real_workflow_yml() {
        // The strongest proof of THE WHOLE PRODUCT IS COVERED: indexing the REAL, committed
        // `.rigger/workflow.yml` (not a fixture) yields the Design text's own example triple.
        // `CARGO_MANIFEST_DIR` (not `.`), so this resolves the crate root regardless of the
        // process's own working directory.
        let events = project_events(env!("CARGO_MANIFEST_DIR"));
        assert!(
            !events.is_empty(),
            "this project's own .rigger/workflow.yml must extract at least one event"
        );
        let has_concept = |kind: &str, id: &str| {
            events.iter().any(|e| {
                if e.type_ != TYPE_DOC_CONCEPT_EXTRACTED {
                    return false;
                }
                let c: DocConceptExtracted = serde_json::from_slice(&e.data).unwrap();
                c.kind == kind && c.id == id
            })
        };
        assert!(has_concept(KIND_STAGE, "stage:implement"));
        assert!(has_concept(KIND_GATE, "gate:mutation"));
        assert!(has_concept(KIND_AGENT, "agent:rust-engineer"));
    }
}
