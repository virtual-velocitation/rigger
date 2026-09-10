//! Periphery (integration) tests for spec 86 criterion 2 (PROOF LANDS ON THE CARD), independent of
//! both the implementer's own fixtures and the existing client-seam test in
//! `proof_row_renders_on_the_card.rs`.
//!
//! Two boundaries neither of those reaches:
//!
//!  - The Done-when, driven end to end through the crate's PUBLIC API (`build_index` ->
//!    `index_events` -> `Projector` -> `dash::card`), from OUTSIDE the crate, over a fixture of its
//!    own. The implementer's own `events.rs` unit test
//!    (`a_product_entity_referenced_by_two_tests_carries_proven_by_2_and_renders_on_its_card`)
//!    already proves this same pipeline, but IN-CRATE and over the implementer's OWN fixture - that
//!    proof rests on the implementer's own understanding of the boundary, exactly the class of gap
//!    this layer exists to close (mirroring `code_entity_test_exclusion_periphery.rs`'s own
//!    precedent for criterion 1's Done-when). A second test here drives the `pending_proof`
//!    forward-reference/reconciliation fold arm (`fold_test_evidence` -> `stage_pending_proof` ->
//!    `reconcile_pending_proof`) through this SAME real pipeline: the implementer's own coverage of
//!    that arm (`sqlite.rs::proof_evidence_c2::an_unresolvable_test_reference_is_staged_and_
//!    reconciled_once_its_definition_later_folds`) hand-builds the two events directly against the
//!    fold, which proves the fold logic is correct IF handed such a sequence but never proves the
//!    real sorted-path pipeline (`project_batches_paced`/`index_events` over `BTreeMap<String,
//!    FileSymbols>`) ever PRODUCES one - a genuinely different fact this test pins by naming its
//!    fixture files so a `tests/`-dir file sorts alphabetically BEFORE the product file it
//!    references. Both tests live in the `symbols` lane only (they drive the real tree-sitter
//!    extraction pass).
//!  - The SERVED `/api/graph?card=<id>` wire contract for the two new `Card` fields. No existing
//!    test crosses the real socket for `card=` at all: the implementer's own `dash.rs` unit tests
//!    call the pure `route`/`card` functions IN-PROCESS (structurally blind to HTTP framing and JSON
//!    serialization), and `proof_row_renders_on_the_card.rs`'s client-seam driver hands `renderCard`
//!    a hand-built JS object directly, never fetching from a server at all - so the serde
//!    `skip_serializing_if` contract (a real JSON KEY present/absent over the wire, not a Rust
//!    struct field's value) and the store's string-encoded `proof_evidence` correctly round-tripping
//!    into a genuine JSON ARRAY (not a double-encoded string) have never been exercised at the true
//!    HTTP boundary until this file. Runs in BOTH lanes (`dash`/`contextgraph` are not feature-gated,
//!    mirroring `dash_kg_graph_route.rs`'s own scope note).

// ---- the Done-when, end to end via the public API (symbols lane only) ----------------------

/// A fixture independent of the implementer's own (`product_fn`/`unused_fn`/`helper`): a product
/// file defining `strike` (referenced by a same-file `#[cfg(test)] mod tests`) and `parry` (defined
/// but never referenced by anything), alongside a separate `tests/`-dir file that also references
/// `strike`.
#[cfg(feature = "symbols")]
const STRIKE_PRODUCT_SRC: &str = "\
fn strike() {}

fn parry() {}

#[cfg(test)]
mod tests {
    use super::strike;

    #[test]
    fn strike_lands() {
        strike();
    }
}
";

#[cfg(feature = "symbols")]
const STRIKE_OUTSIDE_TEST_SRC: &str = "\
#[test]
fn an_outside_combat_test() {
    strike();
}
";

#[cfg(feature = "symbols")]
#[test]
fn proof_lands_through_the_public_pipeline_independent_of_the_implementers_own_fixture() {
    use rigger::contextgraph::sqlite::Projector;
    use rigger::contextgraph::{Projection, KIND_FILE};
    use rigger::dash::card;
    use rigger::grounder::symbols::build_index;
    use rigger::grounder::symbols::events::index_events;

    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("combat.rs"), STRIKE_PRODUCT_SRC).unwrap();
    let tests_dir = root.path().join("tests");
    std::fs::create_dir(&tests_dir).unwrap();
    std::fs::write(
        tests_dir.join("combat_periphery.rs"),
        STRIKE_OUTSIDE_TEST_SRC,
    )
    .unwrap();

    let idx = build_index(root.path().to_str().unwrap(), None);
    let mut events = index_events(&idx);
    let p = Projector::open(":memory:", "test").unwrap();
    for (zero_based, event) in events.iter_mut().enumerate() {
        event.position = zero_based as u64 + 1;
        p.apply(event).unwrap();
    }

    let seed_files = [
        "combat.rs".to_string(),
        "tests/combat_periphery.rs".to_string(),
    ];
    let g = p.subgraph(&seed_files, 3).unwrap();

    // PROVEN: strike is referenced by the in-file test (line 11) and the tests/-dir file (line 3) -
    // proven_by: 2, both file:lines, in fold (sorted-path) order.
    let proven = card(&g, "combat.rs::strike").expect("combat.rs::strike is a graph node");
    assert_eq!(
        proven.proven_by, 2,
        "one same-file and one cross-file test-origin reference both prove strike; card: {proven:?}"
    );
    assert_eq!(
        proven.proof_evidence,
        vec![
            "combat.rs:11".to_string(),
            "tests/combat_periphery.rs:3".to_string()
        ],
        "both evidence file:lines land on the card, in fold order; card: {proven:?}"
    );

    // UNREFERENCED: parry, defined in the same file, is never called by any test - the explicit
    // no-test state, never a made-up value.
    let unproven = card(&g, "combat.rs::parry").expect("combat.rs::parry is a graph node");
    assert_eq!(
        unproven.proven_by, 0,
        "parry is never referenced by a test; card: {unproven:?}"
    );
    assert!(unproven.proof_evidence.is_empty());

    // Criterion 1's own promise still holds alongside the new evidence: neither test item ever
    // became a node, and the tests/-dir file still carries no file container node despite
    // contributing evidence.
    let node_ids: std::collections::BTreeSet<&str> =
        g.nodes.iter().map(|n| n.id.as_str()).collect();
    for excluded in [
        "combat.rs::tests",
        "combat.rs::strike_lands",
        "tests/combat_periphery.rs::an_outside_combat_test",
        "tests/combat_periphery.rs",
    ] {
        assert!(
            !node_ids.contains(excluded),
            "{excluded:?} is test code and must never become a graph node; nodes: {node_ids:?}"
        );
    }
    assert!(
        !g.nodes
            .iter()
            .any(|n| n.kind == KIND_FILE && n.id == "tests/combat_periphery.rs"),
        "an all-test file still carries no file container node even though it contributes \
         evidence; nodes: {:?}",
        g.nodes
    );
    assert!(
        !g.edges
            .iter()
            .any(|e| e.from == "tests/combat_periphery.rs"),
        "a test-origin reference never folds an edge of any kind; edges: {:?}",
        g.edges
    );
}

/// A `tests/`-dir file whose path sorts ALPHABETICALLY BEFORE the product file it references
/// (`tests/aaa_check.rs` < `zzz_product.rs`), so the real sorted-path pipeline
/// (`project_batches_paced`/`index_events` over `BTreeMap<String, FileSymbols>`) folds this
/// evidence event BEFORE `finisher`'s own definition exists - the forward-reference case
/// `pending_proof`/`reconcile_pending_proof` exists for, produced here by the REAL pipeline rather
/// than a hand-built event sequence.
#[cfg(feature = "symbols")]
const FINISHER_PRODUCT_SRC: &str = "fn finisher() {}\n";

#[cfg(feature = "symbols")]
const FORWARD_CHECK_SRC: &str = "\
#[test]
fn checks_finisher() {
    finisher();
}
";

#[cfg(feature = "symbols")]
#[test]
fn forward_referenced_evidence_through_the_public_pipeline_is_reconciled_once_its_definition_later_folds(
) {
    use rigger::contextgraph::sqlite::Projector;
    use rigger::contextgraph::Projection;
    use rigger::dash::card;
    use rigger::grounder::symbols::build_index;
    use rigger::grounder::symbols::events::index_events;

    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("zzz_product.rs"), FINISHER_PRODUCT_SRC).unwrap();
    let tests_dir = root.path().join("tests");
    std::fs::create_dir(&tests_dir).unwrap();
    std::fs::write(tests_dir.join("aaa_check.rs"), FORWARD_CHECK_SRC).unwrap();

    let idx = build_index(root.path().to_str().unwrap(), None);
    assert!(
        idx.files().keys().next().map(String::as_str) == Some("tests/aaa_check.rs"),
        "fixture precondition: the tests/-dir file must sort before the product file so this test \
         actually exercises the forward-reference path, not the same-order case round 1 already \
         covers; files: {:?}",
        idx.files().keys().collect::<Vec<_>>()
    );

    let mut events = index_events(&idx);
    let p = Projector::open(":memory:", "test").unwrap();
    for (zero_based, event) in events.iter_mut().enumerate() {
        event.position = zero_based as u64 + 1;
        p.apply(event).unwrap();
    }

    let g = p
        .subgraph(
            &[
                "zzz_product.rs".to_string(),
                "tests/aaa_check.rs".to_string(),
            ],
            3,
        )
        .unwrap();
    let finisher =
        card(&g, "zzz_product.rs::finisher").expect("zzz_product.rs::finisher is a graph node");
    assert_eq!(
        finisher.proven_by, 1,
        "the forward-referenced evidence is reconciled onto the definition once it folds, even \
         though it arrived first through the real pipeline; card: {finisher:?}"
    );
    assert_eq!(
        finisher.proof_evidence,
        vec!["tests/aaa_check.rs:3".to_string()]
    );
}

// ---- the SERVED /api/graph?card= wire contract (both lanes) --------------------------------

/// Start `serve` on a fresh ephemeral loopback port, fetch `GET <path>` once against a fixture-graph
/// provider, and return the raw HTTP response - or `None` on a genuine socket-level failure. Mirrors
/// `dash_kg_graph_route.rs::try_fetch_served` verbatim - the established per-file duplication
/// convention for this class of served-route periphery test in this codebase (each `tests/*.rs`
/// integration test compiles as its own independent crate, so the harness plumbing cannot be shared
/// via a plain `use`). See that file's own doc for why the listener is handed to `serve_on` rather
/// than dropped and re-bound.
fn try_fetch_served(path: &str, graph: rigger::contextgraph::Graph) -> Option<String> {
    use std::io::{Read, Write};
    use std::net::{TcpListener, TcpStream};
    use std::time::{Duration, Instant};

    let listener = TcpListener::bind(("127.0.0.1", 0)).ok()?;
    let addr = listener.local_addr().ok()?;

    let graph_provider = {
        let graph = graph.clone();
        move |_instance: Option<&str>| -> rigger::contextgraph::Graph { graph.clone() }
    };
    let provider = move |_instance: Option<&str>| -> Result<rigger::dash::DashInputs, String> {
        Ok((
            Vec::new(),
            graph.clone(),
            Vec::new(),
            std::collections::HashMap::new(),
        ))
    };
    let calls_provider =
        |_: Option<&str>, _: &[String], _: rigger::contextgraph::Direction, _: i64, _: &str| {
            rigger::contextgraph::CallGraph::default()
        };
    let instances_provider = Vec::new;
    std::thread::spawn(move || {
        let _ = rigger::dash::serve_on(
            listener,
            provider,
            graph_provider,
            calls_provider,
            instances_provider,
            3,
            "rigger-run",
            "origin/main",
        );
    });

    let deadline = Instant::now() + Duration::from_millis(1500);
    let mut client = loop {
        match TcpStream::connect(addr) {
            Ok(s) => break s,
            Err(_) if Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(10));
            }
            Err(_) => return None,
        }
    };

    let req = format!("GET {path} HTTP/1.1\r\nHost: localhost\r\n\r\n");
    if client.write_all(req.as_bytes()).is_err() {
        return None;
    }
    let mut resp = String::new();
    match client.read_to_string(&mut resp) {
        Ok(_) => Some(resp),
        Err(_) => None,
    }
}

/// Drive the hand-rolled dash server over a REAL loopback socket and fetch `GET <path>`, retrying on
/// a socket-level transient. Mirrors `dash_kg_graph_route.rs::fetch_served` verbatim.
fn fetch_served(path: &str, graph: &rigger::contextgraph::Graph) -> String {
    for _ in 0..200 {
        if let Some(resp) = try_fetch_served(path, graph.clone()) {
            return resp;
        }
    }
    panic!(
        "the dash server never served {path} over the real socket after many fresh-port attempts"
    );
}

/// Split a raw HTTP response into its body (everything past the header terminator). Mirrors
/// `dash_kg_graph_route.rs::body_of` verbatim.
fn body_of(resp: &str) -> &str {
    resp.split_once("\r\n\r\n")
        .map(|(_, body)| body)
        .expect("a served response body")
}

/// A minimal graph with one PROVEN code entity (`proven_by`/`proof_evidence` attrs already folded)
/// and one UNPROVEN one (neither attr present) - built by hand, mirroring how the real fold leaves
/// them (a decimal-digit string and a JSON-array-shaped string respectively; see
/// `contextgraph::sqlite::record_proof`'s own doc for why), so this test is scoped to the SERVED
/// wire contract, independent of whether the fold itself is correct (round 1/2 above already prove
/// that through the real pipeline).
fn proof_fixture_graph() -> rigger::contextgraph::Graph {
    use rigger::contextgraph::{Graph, Node, KIND_CODE_ENTITY};
    let node = |id: &str, attrs: &[(&str, &str)]| Node {
        id: id.to_string(),
        kind: KIND_CODE_ENTITY.to_string(),
        attrs: attrs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect(),
    };
    Graph {
        nodes: vec![
            node(
                "widget.rs::sturdy",
                &[
                    ("name", "sturdy"),
                    ("proven_by", "2"),
                    (
                        "proof_evidence",
                        r#"["widget.rs:5","tests/widget_periphery.rs:2"]"#,
                    ),
                ],
            ),
            node("widget.rs::fragile", &[("name", "fragile")]),
        ],
        edges: Vec::new(),
    }
}

/// The SERVED `/api/graph?card=<id>` route carries `proven_by`/`proof_evidence` over the REAL socket
/// as genuine JSON - `proof_evidence` a real JSON ARRAY (not the double-encoded string the store
/// holds it as internally), `proven_by` a real JSON number. Guards the serve/route/serialize seam
/// the in-process `dash.rs` unit tests and the JS-mocked client-seam test are both blind to (neither
/// ever sends this struct through `serde_json` onto a real socket).
#[test]
fn the_served_graph_card_route_carries_proven_by_and_proof_evidence_as_a_real_json_array() {
    let graph = proof_fixture_graph();
    let resp = fetch_served("/api/graph?card=widget.rs::sturdy", &graph);
    assert!(
        resp.starts_with("HTTP/1.1 200 OK"),
        "GET /api/graph?card= returns 200 over the real serve socket:\n{resp}"
    );
    let json: serde_json::Value =
        serde_json::from_str(body_of(&resp)).expect("the served card body is valid JSON");
    assert_eq!(
        json["card"]["proven_by"], 2,
        "proven_by crosses the real socket as a JSON number: {json}"
    );
    assert_eq!(
        json["card"]["proof_evidence"],
        serde_json::json!(["widget.rs:5", "tests/widget_periphery.rs:2"]),
        "proof_evidence crosses the real socket as a genuine JSON array, not a double-encoded \
         string: {json}"
    );
}

/// The SERVED `/api/graph?card=<id>` route OMITS the `proof_evidence` key entirely for an unproven
/// entity (the real `skip_serializing_if` contract firing over the wire, never observable from a
/// Rust struct field's value alone) while still serving `proven_by: 0` - so the client can render
/// the explicit "no test reaches this entity" state from a REAL present zero, never an absent field.
#[test]
fn the_served_graph_card_route_omits_proof_evidence_but_still_serves_proven_by_zero_for_an_unproven_entity(
) {
    let graph = proof_fixture_graph();
    let resp = fetch_served("/api/graph?card=widget.rs::fragile", &graph);
    assert!(
        resp.starts_with("HTTP/1.1 200 OK"),
        "GET /api/graph?card= returns 200 over the real serve socket:\n{resp}"
    );
    let body = body_of(&resp);
    assert!(
        !body.contains("proof_evidence"),
        "an unproven entity's proof_evidence is OMITTED from the wire (skip_serializing_if), not \
         sent as an empty array: {body}"
    );
    let json: serde_json::Value =
        serde_json::from_str(body).expect("the served card body is valid JSON");
    assert_eq!(
        json["card"]["proven_by"], 0,
        "proven_by is ALWAYS present (never omitted), so the client can render the explicit \
         no-test state from a real 0, not an absent field: {json}"
    );
}
