//! Periphery (integration) tests for spec 86 criterion 2 (PROOF LANDS ON THE CARD), independent of
//! both the implementer's own fixtures and the client-seam test in
//! `proof_row_renders_on_the_card.rs`.
//!
//! Two boundaries neither of those reaches:
//!
//!  - The Done-when end to end through the crate's PUBLIC API: the code store's view of a real
//!    tree, read by the real extractor and composed beside the record half
//!    ([`common::fixtures::files_subgraph`]), down to `query::card`. Proof is a pure reading of
//!    one tree, so an edit is the next tree read whole: each test walks one or more tree
//!    generations and pins the proof every generation shows - an edit to the defining file keeps a
//!    cross-file proof, an edit to a test file moves its proof whole, a deleted reference retracts
//!    it, and an ambiguous same-named pair is never credited. These tests live in the `symbols`
//!    lane only (they drive the real tree-sitter extractor).
//!  - The SERVED `/api/graph?card=<id>` wire contract for the two `Card` proof fields, across the
//!    real socket: the serde `skip_serializing_if` contract (a JSON KEY present or absent over the
//!    wire) and `proof_evidence` arriving as a genuine JSON ARRAY, not a double-encoded string.
//!    Runs in BOTH lanes (`dash`/`contextgraph` are not feature-gated).

use crate::common;

use common::served::body_of;
use common::served::fetch_served;

// ---- the Done-when, end to end via the public API (symbols lane only) ----------------------

/// The subgraph `depth` hops around `seeds` of a tree holding exactly `files`, read by the real
/// extractor and composed beside an empty record half.
#[cfg(feature = "symbols")]
fn tree_subgraph(
    files: &[(&str, &str)],
    seeds: &[&str],
    depth: i64,
) -> rigger::contextgraph::Graph {
    let records = rigger::contextgraph::sqlite::Projector::open(":memory:", "test").unwrap();
    common::fixtures::files_subgraph(
        &records,
        &rigger_grounder::facts::FactsExtractor,
        files,
        seeds,
        depth,
    )
}

/// One tree generation and the proof it must show: the tree holding exactly `files`, and for each
/// `(entity id, proven_by, evidence)` of `want`, that entity's card read two hops around the
/// file that defines it.
#[cfg(feature = "symbols")]
struct Generation {
    files: &'static [(&'static str, &'static str)],
    want: &'static [(&'static str, usize, &'static [&'static str])],
}

/// Holds every generation of `walk` in turn (`why` names the walk).
#[cfg(feature = "symbols")]
fn assert_generations(why: &str, walk: &[Generation]) {
    for (step, generation) in walk.iter().enumerate() {
        for (id, proven_by, evidence) in generation.want {
            let file = id.split("::").next().unwrap();
            let g = tree_subgraph(generation.files, &[file], 2);
            let card = rigger::contextgraph::query::card(&g, id)
                .unwrap_or_else(|| panic!("{why}, generation {step}: {id} is a graph node"));
            assert_eq!(
                (card.proven_by, card.proof_evidence.clone()),
                (
                    *proven_by,
                    evidence.iter().map(|e| e.to_string()).collect::<Vec<_>>()
                ),
                "{why}, generation {step}: {id}; card: {card:?}"
            );
        }
    }
}

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
    use rigger::contextgraph::query::card;
    use rigger::contextgraph::KIND_FILE;

    assert_generations(
        "a same-file and a cross-file test both prove strike; parry is never referenced",
        &[Generation {
            files: &[
                ("combat.rs", STRIKE_PRODUCT_SRC),
                ("tests/combat_periphery.rs", STRIKE_OUTSIDE_TEST_SRC),
            ],
            want: &[
                (
                    "combat.rs::strike",
                    2,
                    &["combat.rs:11", "tests/combat_periphery.rs:3"],
                ),
                ("combat.rs::parry", 0, &[]),
            ],
        }],
    );

    // Criterion 1's own promise still holds alongside the evidence: neither test item ever became
    // a node, and the tests/-dir file still carries no file container node despite contributing
    // evidence.
    let g = tree_subgraph(
        &[
            ("combat.rs", STRIKE_PRODUCT_SRC),
            ("tests/combat_periphery.rs", STRIKE_OUTSIDE_TEST_SRC),
        ],
        &["combat.rs", "tests/combat_periphery.rs"],
        3,
    );
    assert!(
        card(&g, "combat.rs::strike").is_some(),
        "sanity: strike is graphed"
    );
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
        "a test-origin reference never makes an edge of any kind; edges: {:?}",
        g.edges
    );
}

/// A `tests/`-dir file whose path sorts ALPHABETICALLY BEFORE the product file it references
/// (`tests/aaa_check.rs` < `zzz_product.rs`): proof reads the whole tree, so no path order can
/// leave a reference waiting on a definition read after it.
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
fn a_test_file_sorting_before_the_product_file_it_references_still_proves_it() {
    assert_generations(
        "a reference whose file sorts before the definition's proves it",
        &[Generation {
            files: &[
                ("zzz_product.rs", FINISHER_PRODUCT_SRC),
                ("tests/aaa_check.rs", FORWARD_CHECK_SRC),
            ],
            want: &[("zzz_product.rs::finisher", 1, &["tests/aaa_check.rs:3"])],
        }],
    );
}

// ---- proof follows the tree across edits ---------------------------------------------------

/// `watch` (proven by a cross-file test) and `idle` (never referenced).
#[cfg(feature = "symbols")]
const SENTINEL_V1_SRC: &str = "fn watch() {}\n\nfn idle() {}\n";

/// `sentinel.rs` after an edit UNRELATED to `watch`: a new function is appended while `watch`'s
/// own line (1) is untouched.
#[cfg(feature = "symbols")]
const SENTINEL_V2_SRC: &str = "fn watch() {}\n\nfn idle() {}\n\nfn alert() {}\n";

#[cfg(feature = "symbols")]
const SENTINEL_TEST_SRC: &str = "\
#[test]
fn checks_watch() {
    watch();
}
";

/// A cross-file test proves `watch`; an edit to `sentinel.rs` unrelated to `watch` must keep the
/// proof, and its one evidence entry, that the untouched test file establishes.
#[cfg(feature = "symbols")]
#[test]
fn cross_file_proof_survives_an_edit_of_the_defining_file() {
    assert_generations(
        "an unrelated edit to the defining file keeps a cross-file proof",
        &[
            Generation {
                files: &[
                    ("sentinel.rs", SENTINEL_V1_SRC),
                    ("tests/sentinel_periphery.rs", SENTINEL_TEST_SRC),
                ],
                want: &[
                    ("sentinel.rs::watch", 1, &["tests/sentinel_periphery.rs:3"]),
                    ("sentinel.rs::idle", 0, &[]),
                ],
            },
            Generation {
                files: &[
                    ("sentinel.rs", SENTINEL_V2_SRC),
                    ("tests/sentinel_periphery.rs", SENTINEL_TEST_SRC),
                ],
                want: &[
                    ("sentinel.rs::watch", 1, &["tests/sentinel_periphery.rs:3"]),
                    ("sentinel.rs::idle", 0, &[]),
                    ("sentinel.rs::alert", 0, &[]),
                ],
            },
        ],
    );
}

#[cfg(feature = "symbols")]
const DUO_SRC: &str = "fn left() {}\n\nfn right() {}\n";

#[cfg(feature = "symbols")]
const DUO_TEST_V1_SRC: &str = "\
#[test]
fn checks_left() {
    left();
}
";

/// An unrelated line is added ABOVE the test (shifts the reference's own line from 3 to 4) while
/// still referencing `left`.
#[cfg(feature = "symbols")]
const DUO_TEST_V2_SRC: &str = "\
// housekeeping
#[test]
fn checks_left() {
    left();
}
";

/// The test is edited to reference `right` instead of `left` entirely.
#[cfg(feature = "symbols")]
const DUO_TEST_V3_SRC: &str = "\
#[test]
fn checks_right() {
    right();
}
";

/// Three edits to the SAME test file: an edit that only shifts the reference's own line must net
/// to ONE entry at the new line, never a second, stale one at the old; an edit that swaps which
/// product entity it reaches must move the proof whole, never strand an entry on the old target.
#[cfg(feature = "symbols")]
#[test]
fn an_edit_of_a_test_file_moves_its_proof_whole_rather_than_accreting_or_stranding_it() {
    assert_generations(
        "edits to one test file",
        &[
            Generation {
                files: &[
                    ("duo.rs", DUO_SRC),
                    ("tests/duo_periphery.rs", DUO_TEST_V1_SRC),
                ],
                want: &[
                    ("duo.rs::left", 1, &["tests/duo_periphery.rs:3"]),
                    ("duo.rs::right", 0, &[]),
                ],
            },
            Generation {
                files: &[
                    ("duo.rs", DUO_SRC),
                    ("tests/duo_periphery.rs", DUO_TEST_V2_SRC),
                ],
                want: &[
                    ("duo.rs::left", 1, &["tests/duo_periphery.rs:4"]),
                    ("duo.rs::right", 0, &[]),
                ],
            },
            Generation {
                files: &[
                    ("duo.rs", DUO_SRC),
                    ("tests/duo_periphery.rs", DUO_TEST_V3_SRC),
                ],
                want: &[
                    ("duo.rs::left", 0, &[]),
                    ("duo.rs::right", 1, &["tests/duo_periphery.rs:3"]),
                ],
            },
        ],
    );
}

#[cfg(feature = "symbols")]
const ALPHA_V1_SRC: &str = "fn shared() {}\n";

/// `alpha.rs` after an edit UNRELATED to `shared`: a second function is added.
#[cfg(feature = "symbols")]
const ALPHA_V2_SRC: &str = "fn shared() {}\n\nfn alpha_only() {}\n";

#[cfg(feature = "symbols")]
const BETA_SRC: &str = "fn shared() {}\n";

#[cfg(feature = "symbols")]
const AMBIGUOUS_TEST_SRC: &str = "\
#[test]
fn checks_shared() {
    shared();
}
";

/// `alpha.rs` and `beta.rs` both define `shared`; a test file references it with no local
/// definition of its own. Neither definition is credited, before or after an edit to `alpha.rs`
/// elsewhere.
#[cfg(feature = "symbols")]
#[test]
fn an_ambiguous_same_named_pair_never_gets_confident_credit_across_an_edit() {
    assert_generations(
        "an ambiguous name proves neither definition",
        &[
            Generation {
                files: &[
                    ("alpha.rs", ALPHA_V1_SRC),
                    ("beta.rs", BETA_SRC),
                    ("tests/ambiguous_periphery.rs", AMBIGUOUS_TEST_SRC),
                ],
                want: &[("alpha.rs::shared", 0, &[]), ("beta.rs::shared", 0, &[])],
            },
            Generation {
                files: &[
                    ("alpha.rs", ALPHA_V2_SRC),
                    ("beta.rs", BETA_SRC),
                    ("tests/ambiguous_periphery.rs", AMBIGUOUS_TEST_SRC),
                ],
                want: &[("alpha.rs::shared", 0, &[]), ("beta.rs::shared", 0, &[])],
            },
        ],
    );
}

#[cfg(feature = "symbols")]
const GONE_PRODUCT_SRC: &str = "fn vanish() {}\n";

#[cfg(feature = "symbols")]
const GONE_TEST_V1_SRC: &str = "\
#[test]
fn checks_vanish() {
    vanish();
}
";

/// The test file after its ONLY reference is deleted - replaced with a plain comment, ZERO
/// references left in the file.
#[cfg(feature = "symbols")]
const GONE_TEST_V2_SRC: &str = "// the test that proved vanish() was deleted\n";

/// A product function proven by exactly one test, then that test's ONLY reference deleted: the
/// proof and its file:line entry are retracted, never stranded.
#[cfg(feature = "symbols")]
#[test]
fn a_deleted_test_reference_retracts_its_proof() {
    assert_generations(
        "deleting the one proving reference",
        &[
            Generation {
                files: &[
                    ("gone.rs", GONE_PRODUCT_SRC),
                    ("tests/gone_periphery.rs", GONE_TEST_V1_SRC),
                ],
                want: &[("gone.rs::vanish", 1, &["tests/gone_periphery.rs:3"])],
            },
            Generation {
                files: &[
                    ("gone.rs", GONE_PRODUCT_SRC),
                    ("tests/gone_periphery.rs", GONE_TEST_V2_SRC),
                ],
                want: &[("gone.rs::vanish", 0, &[])],
            },
        ],
    );
}

/// A `tests/`-dir file that defines something but calls nothing: a definition is never evidence,
/// only a reference is.
#[cfg(feature = "symbols")]
const BLANK_TESTS_DIR_SRC: &str = "fn noop_helper() {}\n";

#[cfg(feature = "symbols")]
const QUIET_PRODUCT_SRC: &str = "fn quiet() {}\n\nfn heard() {}\n";

#[cfg(feature = "symbols")]
const QUIET_TEST_SRC: &str = "\
#[test]
fn checks_heard() {
    heard();
}
";

/// A `tests/`-dir file with an empty reference set reads to no node and no edge at all - never a
/// stray `""`-named entity - and leaves an unrelated pair beside it in the same tree to prove
/// normally.
#[cfg(feature = "symbols")]
#[test]
fn a_reference_free_tests_dir_file_creates_nothing_and_disturbs_no_other_proof() {
    let blank_only = tree_subgraph(
        &[("tests/blank_helper.rs", BLANK_TESTS_DIR_SRC)],
        &["tests/blank_helper.rs"],
        2,
    );
    assert!(
        blank_only.nodes.is_empty() && blank_only.edges.is_empty(),
        "a reference-free tests/-dir file creates nothing at all; nodes: {:?}, edges: {:?}",
        blank_only.nodes,
        blank_only.edges
    );

    assert_generations(
        "an unrelated pair beside a reference-free tests/-dir file",
        &[Generation {
            files: &[
                ("tests/blank_helper.rs", BLANK_TESTS_DIR_SRC),
                ("quiet.rs", QUIET_PRODUCT_SRC),
                ("tests/quiet_check.rs", QUIET_TEST_SRC),
            ],
            want: &[
                ("quiet.rs::heard", 1, &["tests/quiet_check.rs:3"]),
                ("quiet.rs::quiet", 0, &[]),
            ],
        }],
    );
}

// ---- the SERVED /api/graph?card= wire contract (both lanes) --------------------------------

/// A minimal graph with one PROVEN code entity (`proven_by`/`proof_evidence` attrs present) and one
/// UNPROVEN one (neither attr present) - built by hand in the attr shapes a proven entity carries (a
/// decimal-digit string and a JSON-array-shaped string respectively), so this test is scoped to the
/// SERVED wire contract, independent of how proof is derived (the tests above prove that through
/// the public API).
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
