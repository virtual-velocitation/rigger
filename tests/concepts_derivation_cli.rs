//! Periphery (CLI / end-to-end) tests for spec 54's CONCEPTS lens derivation exercised through the
//! BUILT BINARY: `rigger graph concepts [--resolution <r>]`. The inside-out unit tests in
//! `src/concepts.rs` feed a hand-built `Graph` to `derive` / `events` and hand-apply the resulting
//! events into a `Projector`. So none of them exercises the actual subcommand: its dispatch, its
//! argument parsing, its store bootstrap, the real `whole()` read it derives over, or the real
//! `append_and_fold_batch` seam that appends the `ConceptDerived` / `ConceptRealized` events to the
//! run log AND folds them into live `REALIZES` edges under the local `.rigger/`. This file guards
//! exactly that binary boundary, over the shipped executable and the public projection surface:
//!
//!  - SUBCOMMAND wiring + arg parsing: `graph concepts` dispatches, `--resolution` parses, and a
//!    malformed or unknown argument exits non-zero with its documented message (before any store
//!    side effect).
//!  - the END-TO-END record seam over the REAL store: a seeded INTENT layer (design docs + their
//!    intent edges, ingested as `DocConceptExtracted` / `DocLinkExtracted` events), derived through
//!    the binary, materializes a live concept layer (`KIND_CONCEPT` nodes + `REALIZES` edges) read
//!    back over `Projector::whole`, groups each document WITH the code it governs ACROSS directory
//!    lines, EXCLUDES a dev-loop `decision --GOVERNS--> file` (harness noise, no intent-doc
//!    endpoint), and the summary line reports honest counts.
//!  - the EMPTY no-op: a project with no intent edges records nothing and still exits 0.
//!  - GRAIN coexistence + supersession: two resolutions coexist (distinct `concept/<r>/*` ids, both
//!    live), and re-running one grain REPLACES only that grain's concept set - no stale duplicate
//!    memberships and the other grain untouched.
//!  - DETERMINISM through the binary: re-running a grain reproduces the byte-identical live layer.
//!
//! The intent layer is built from the ALWAYS-COMPILED design-intent folds (spec 29b), and the
//! derivation, the fold, and `append_and_fold_batch` are all always-compiled, so every test here
//! runs identically in BOTH feature lanes.

use std::collections::BTreeSet;
use std::path::Path;
use std::process::Command;

use rigger::conductor;
use rigger::contextgraph::sqlite::Projector;
use rigger::contextgraph::{
    Projection, KIND_CONCEPT, KIND_DESIGN_DOC, KIND_HANDBOOK_RULE, KIND_RATIONALE, REL_EXPLAINS,
    REL_GOVERNS, REL_REALIZES, REL_SPECIFIES, TYPE_DECISION_MADE, TYPE_DOC_CONCEPT_EXTRACTED,
    TYPE_DOC_LINK_EXTRACTED,
};
use rigger::eventstore::namespace::Namespaced;
use rigger::eventstore::sqlite::Store;
use rigger::eventstore::Event;
use rigger::ingest::append_and_fold_batch;

// The compiled `rigger` binary under test is located at RUNTIME by the shared authority in
// `tests/common`: a path baked in at compile time goes stale the moment the target dir moves,
// and every suite that spawns the product then dies with a bare NotFound.
mod common;
use common::rigger_bin;

/// A stable project identity pinned into the fixture, so the in-test SEED store and the BINARY
/// resolve the SAME namespace: the binary reads `.rigger/project.id` at the git top-level, and the
/// seed opens its `Store` / `Projector` under that identity, so a fold the seed lands is the exact
/// projection the binary later derives concepts over and records into.
const IDENTITY: &str = "concepttest";

/// A throwaway git project with `.rigger/project.id` pinned. Its own git repo makes the identity
/// resolution deterministic (the top-level is the fixture), and the pinned id file makes the seed
/// and the binary agree on the store namespace. Kept alive by the caller.
fn project() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let git = |args: &[&str]| {
        let _ = Command::new("git").args(args).current_dir(root).status();
    };
    git(&["init", "-q"]);
    git(&["config", "user.email", "t@t"]);
    git(&["config", "user.name", "t"]);
    std::fs::create_dir_all(root.join(".rigger")).unwrap();
    std::fs::write(root.join(".rigger").join("project.id"), IDENTITY).unwrap();
    dir
}

/// The local `.rigger/<name>` path under the fixture, as the binary's `db_path` resolves it.
fn rigger_db(root: &Path, name: &str) -> String {
    root.join(".rigger")
        .join(name)
        .to_string_lossy()
        .into_owned()
}

/// Run `rigger graph concepts <args>` in `root` over the built binary and return the output.
fn concepts(root: &Path, args: &[&str]) -> std::process::Output {
    let mut argv = vec!["graph", "concepts"];
    argv.extend_from_slice(args);
    Command::new(rigger_bin())
        .args(&argv)
        .current_dir(root)
        .env("RIGGER_NO_DASH", "1")
        .output()
        .expect("spawn rigger graph concepts")
}

/// A `DocConceptExtracted` event (spec 29b): one design-intent node of `kind` at `id`, carrying a
/// `title` (the label source the concept derivation reads). Folds the intent-doc node.
fn doc(kind: &str, id: &str, title: &str) -> Event {
    Event::new(
        TYPE_DOC_CONCEPT_EXTRACTED,
        serde_json::to_vec(&serde_json::json!({
            "kind": kind, "id": id, "title": title, "doc": id
        }))
        .unwrap(),
    )
}

/// A `DocLinkExtracted` event (spec 29b): one design-intent edge `<from> --rel--> <to>` at the
/// extracted tier. `from` is an intent-doc, `to` is the code it attaches to - the coupling the
/// concepts derivation groups over.
fn link(from: &str, to: &str, rel: &str) -> Event {
    Event::new(
        TYPE_DOC_LINK_EXTRACTED,
        serde_json::to_vec(&serde_json::json!({ "from": from, "to": to, "rel": rel })).unwrap(),
    )
}

/// A dev-loop `DecisionMade` GOVERNING a file (spec's harness-noise case): it folds a `decision`
/// node and a `decision --GOVERNS--> file` edge. The relation is an intent relation, but the
/// `decision` endpoint is NOT an intent-doc, so the intent-layer filter must REJECT this edge - a
/// concept is design intent, never a dev-loop artifact.
fn decision_noise(id: &str, file: &str) -> Event {
    Event::new(
        TYPE_DECISION_MADE,
        serde_json::to_vec(&serde_json::json!({
            "id": id, "summary": "noise", "governs": [file], "supersedes": ""
        }))
        .unwrap(),
    )
}

/// Seed the canonical spec-54 INTENT layer into the fixture's REAL store via the exact production
/// seam (`append_and_fold_batch` on the run stream): it appends the design-intent events to
/// `.rigger/events.db` and folds them into `.rigger/graph.db`.
///
/// - Concept A ("The knowledge graph"): a `design-doc` SPECIFIES four files across `src/graph` and
///   `src/db` (a doc grouped with its code ACROSS directory lines).
/// - Concept B ("Review adjudication"): a `handbook-rule` GOVERNS four files across `src/review` and
///   `src/verdict`.
/// - Concept C: a `rationale` EXPLAINS `src/graph/index.rs` - a file in the SAME directory as
///   concept A's `src/graph/store.rs`, yet a DIFFERENT concept, because no design doc governs it
///   (grouping is by INTENT, never by directory).
/// - NOISE: a dev-loop `decision --GOVERNS--> src/graph/store.rs` the intent-layer filter rejects.
///
/// The store / projection handles drop at function end, freeing the sqlite files before the binary
/// opens them.
fn seed_intent(root: &Path) {
    let backend = Store::open(&rigger_db(root, "events.db")).unwrap();
    let store = Namespaced::new(&backend, IDENTITY);
    let graph = Projector::open(&rigger_db(root, "graph.db"), IDENTITY).unwrap();

    let a_files = [
        "src/graph/store.rs",
        "src/graph/fold.rs",
        "src/db/sqlite.rs",
        "src/db/schema.rs",
    ];
    let b_files = [
        "src/review/panel.rs",
        "src/review/lens.rs",
        "src/verdict/judge.rs",
        "src/verdict/tally.rs",
    ];

    let mut events = vec![
        doc(KIND_DESIGN_DOC, "docs/kg.md", "The knowledge graph"),
        doc(KIND_HANDBOOK_RULE, "docs/review.md", "Review adjudication"),
        doc(KIND_RATIONALE, "src/graph/index.rs#L5", "why index by name"),
    ];
    for f in &a_files {
        events.push(link("docs/kg.md", f, REL_SPECIFIES));
    }
    for f in &b_files {
        events.push(link("docs/review.md", f, REL_GOVERNS));
    }
    events.push(link(
        "src/graph/index.rs#L5",
        "src/graph/index.rs",
        REL_EXPLAINS,
    ));
    // NOISE: a dev-loop decision GOVERNS a region-A file. Same rel as an intent edge, but a
    // `decision` node is not an intent-doc, so the layer must EXCLUDE it.
    events.push(decision_noise("d-noise", "src/graph/store.rs"));

    append_and_fold_batch(
        &store,
        Some(&graph as &dyn Projection),
        conductor::STREAM,
        &events,
    )
    .expect("seed the intent layer through the real append-and-fold seam");
}

/// The live concept layer read back over the PUBLIC projection surface: the sorted set of
/// `KIND_CONCEPT` node ids, and every live `<member> --REALIZES--> <concept>` edge as
/// `(member, concept)` pairs.
fn concept_layer(root: &Path) -> (Vec<String>, Vec<(String, String)>) {
    let graph = Projector::open(&rigger_db(root, "graph.db"), IDENTITY).unwrap();
    let whole = graph.whole().unwrap();
    let mut concepts: Vec<String> = whole
        .nodes
        .iter()
        .filter(|n| n.kind == KIND_CONCEPT)
        .map(|n| n.id.clone())
        .collect();
    concepts.sort();
    concepts.dedup();
    let mut edges: Vec<(String, String)> = whole
        .edges
        .iter()
        .filter(|e| e.rel == REL_REALIZES)
        .map(|e| (e.from.clone(), e.to.clone()))
        .collect();
    edges.sort();
    (concepts, edges)
}

/// The live concept a member currently realizes (its single `REALIZES` target), if any.
fn member_of<'a>(edges: &'a [(String, String)], node: &str) -> Option<&'a str> {
    edges
        .iter()
        .find(|(from, _)| from == node)
        .map(|(_, to)| to.as_str())
}

#[test]
fn the_subcommand_records_a_live_concept_layer_over_the_real_store() {
    // Drive the built binary end-to-end: it reads the seeded intent layer via `whole()`, derives
    // concepts, and records them THROUGH `append_and_fold_batch` into live `REALIZES` edges - the
    // seam the inside-out tests (which hand-apply events) never exercise.
    let dir = project();
    let root = dir.path();
    seed_intent(root);

    let out = concepts(root, &[]);
    assert!(
        out.status.success(),
        "graph concepts must succeed over a real seeded store: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("graph concepts: derived"),
        "the summary line reports the derivation outcome: {stdout}"
    );
    assert!(
        stdout.contains("at resolution 1"),
        "the summary reports the default resolution grain: {stdout}"
    );
    assert!(
        stdout.contains("event(s) recorded into"),
        "the summary reports the events recorded into the store: {stdout}"
    );

    let (concept_nodes, edges) = concept_layer(root);
    // Two docs governing disjoint regions plus a rationale on a doc-less file: three connected
    // intent regions, three concepts.
    assert_eq!(
        concept_nodes.len(),
        3,
        "three connected intent regions became three live concepts, got {}: {concept_nodes:?}",
        concept_nodes.len()
    );
    for c in &concept_nodes {
        assert!(
            c.starts_with("concept/1/"),
            "the default-grain pass names concepts `concept/1/<n>`: {c}"
        );
    }

    // Concept A groups the design-doc WITH all four files it specifies, ACROSS src/graph and src/db.
    let ca = member_of(&edges, "docs/kg.md").expect("the design-doc realizes concept A");
    for member in [
        "src/graph/store.rs",
        "src/graph/fold.rs",
        "src/db/sqlite.rs",
        "src/db/schema.rs",
    ] {
        assert_eq!(
            member_of(&edges, member),
            Some(ca),
            "{member} realizes concept A (a doc grouped with its code across directory lines)"
        );
    }
    // Concept B groups the handbook-rule WITH its four files across src/review and src/verdict.
    let cb = member_of(&edges, "docs/review.md").expect("the handbook-rule realizes concept B");
    for member in [
        "src/review/panel.rs",
        "src/review/lens.rs",
        "src/verdict/judge.rs",
        "src/verdict/tally.rs",
    ] {
        assert_eq!(
            member_of(&edges, member),
            Some(cb),
            "{member} realizes concept B across directory lines"
        );
    }
    assert_ne!(ca, cb, "the two doc regions are DISTINCT concepts");

    // Grouping is by INTENT, not directory: `src/graph/index.rs` sits in the SAME directory as
    // concept A's `src/graph/store.rs`, yet lands in a DIFFERENT concept - no design doc governs it.
    let cc =
        member_of(&edges, "src/graph/index.rs").expect("the rationale's file realizes a concept");
    assert_ne!(
        cc, ca,
        "a src/graph file with no design-doc link is NOT forced into concept A by its directory"
    );

    // The dev-loop decision noise never entered the layer: it realizes NO concept, and the `decision`
    // node is not a concept member.
    assert!(
        member_of(&edges, "d-noise").is_none(),
        "a dev-loop decision --GOVERNS--> file is NOT intent and realizes no concept"
    );
}

#[test]
fn an_empty_project_records_no_concept_and_still_succeeds() {
    // A project with no intent edges is a clean no-op end-to-end: the pass derives nothing, records
    // nothing, and exits 0 (never an error). This proves the CLI wiring, the store bootstrap, and
    // the empty-derivation no-op path over the built binary in BOTH feature lanes.
    let dir = project();
    let root = dir.path();

    let out = concepts(root, &[]);
    assert!(
        out.status.success(),
        "an empty intent layer is a no-op, not an error: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("derived 0 concept"),
        "the summary reports zero concepts: {stdout}"
    );
    assert!(
        stdout.contains("0 intent-linked node(s)"),
        "the summary reports zero intent-linked nodes: {stdout}"
    );
    assert!(
        stdout.contains("0 event(s) recorded"),
        "an empty pass records no events: {stdout}"
    );

    let (concept_nodes, edges) = concept_layer(root);
    assert!(
        concept_nodes.is_empty() && edges.is_empty(),
        "no concept layer materialized for an empty project: {} node(s), {} edge(s)",
        concept_nodes.len(),
        edges.len()
    );
}

#[test]
fn resolution_grains_coexist_and_a_rerun_supersedes_only_its_own_grain() {
    // Two grains are recorded from the same intent layer, then the default grain is re-run. The
    // `--resolution` grains coexist (distinct `concept/<r>/*` ids, both live), and a re-run of one
    // grain REPLACES only that grain's concept set (the `fresh` pass boundary) - the other grain is
    // untouched, and the re-run leaves exactly one live membership per member (no duplicates).
    let dir = project();
    let root = dir.path();
    seed_intent(root);

    assert!(
        concepts(root, &[]).status.success(),
        "recording the default grain succeeds"
    );
    assert!(
        concepts(root, &["--resolution", "2"]).status.success(),
        "recording a second grain (r=2) succeeds"
    );

    let (concept_nodes, _edges) = concept_layer(root);
    let grain1: BTreeSet<&String> = concept_nodes
        .iter()
        .filter(|c| c.starts_with("concept/1/"))
        .collect();
    let grain2_before: BTreeSet<String> = concept_nodes
        .iter()
        .filter(|c| c.starts_with("concept/2/"))
        .cloned()
        .collect();
    assert!(!grain1.is_empty(), "the default grain is live");
    assert!(
        !grain2_before.is_empty(),
        "the r=2 grain coexists with the default grain (distinct ids), not destroyed by it"
    );

    // Re-run the default grain: it must supersede ONLY `concept/1/*`, leaving `concept/2/*` whole.
    assert!(
        concepts(root, &[]).status.success(),
        "re-running the default grain succeeds"
    );
    let (concept_nodes2, edges2) = concept_layer(root);
    let grain2_after: BTreeSet<String> = concept_nodes2
        .iter()
        .filter(|c| c.starts_with("concept/2/"))
        .cloned()
        .collect();
    assert_eq!(
        grain2_before, grain2_after,
        "re-running the default grain leaves the r=2 grain's concepts intact"
    );

    // After the re-run each default-grain member carries exactly ONE live membership - the prior
    // pass's memberships were superseded, not left as stale duplicates.
    let mut default_members: Vec<&String> = edges2
        .iter()
        .filter(|(_, to)| to.starts_with("concept/1/"))
        .map(|(from, _)| from)
        .collect();
    let total = default_members.len();
    default_members.sort();
    default_members.dedup();
    assert!(
        total > 0,
        "the default grain still has live memberships after the re-run"
    );
    assert_eq!(
        default_members.len(),
        total,
        "each member has exactly one live default-grain membership after the re-run (supersession, \
         no duplicates)"
    );
}

#[test]
fn a_malformed_resolution_or_unknown_argument_fails_loudly() {
    // Argument validation runs BEFORE any store side effect: a non-numeric resolution, a
    // non-positive resolution, and an unknown argument each exit non-zero with the documented
    // message. This pins the subcommand's parsing contract over the built binary.
    let dir = project();
    let root = dir.path();

    let non_number = concepts(root, &["--resolution", "not-a-number"]);
    assert!(
        !non_number.status.success(),
        "a non-numeric --resolution is rejected"
    );
    assert!(
        String::from_utf8_lossy(&non_number.stderr).contains("--resolution expects a number"),
        "the error names the malformed numeric argument: {}",
        String::from_utf8_lossy(&non_number.stderr)
    );

    let non_positive = concepts(root, &["--resolution", "0"]);
    assert!(
        !non_positive.status.success(),
        "a non-positive --resolution is rejected"
    );
    assert!(
        String::from_utf8_lossy(&non_positive.stderr).contains("positive finite number"),
        "the error demands a positive finite resolution: {}",
        String::from_utf8_lossy(&non_positive.stderr)
    );

    let unknown = concepts(root, &["--bogus"]);
    assert!(!unknown.status.success(), "an unknown argument is rejected");
    assert!(
        String::from_utf8_lossy(&unknown.stderr).contains("unknown argument"),
        "the error names the unknown argument: {}",
        String::from_utf8_lossy(&unknown.stderr)
    );
}

#[test]
fn re_running_a_grain_reproduces_the_byte_identical_live_layer() {
    // Determinism THROUGH THE BINARY, observed on the MATERIALIZED layer: running `graph concepts`
    // twice on the same store re-derives, supersedes the grain's prior memberships, and re-folds -
    // and the live concept layer read back over the public projection is identical to the first
    // pass's (same `KIND_CONCEPT` node ids, same `REALIZES` edges). This guards the
    // derive -> supersede -> fold seam's end-to-end determinism as it lands in the store, which
    // neither the supersession test (which checks only duplicate-freedom + the OTHER grain's
    // survival) nor the inside-out byte-identical-events test (which never drives the binary or the
    // fold) asserts.
    let dir = project();
    let root = dir.path();
    seed_intent(root);

    assert!(
        concepts(root, &[]).status.success(),
        "the first pass records the default grain"
    );
    let (concepts1, edges1) = concept_layer(root);
    assert!(
        !concepts1.is_empty() && !edges1.is_empty(),
        "the first pass materialized a non-empty live concept layer (else the guard is vacuous): \
         {} node(s), {} edge(s)",
        concepts1.len(),
        edges1.len()
    );

    // Re-run the SAME grain over the SAME store: re-derive, supersede this grain's prior
    // memberships, re-fold. A deterministic pass reproduces the exact live layer byte for byte.
    assert!(
        concepts(root, &[]).status.success(),
        "re-running the default grain succeeds"
    );
    let (concepts2, edges2) = concept_layer(root);

    assert_eq!(
        concepts1, concepts2,
        "re-running the same grain reproduces the identical concept nodes"
    );
    assert_eq!(
        edges1, edges2,
        "re-running the same grain reproduces the identical REALIZES membership edges"
    );
}
