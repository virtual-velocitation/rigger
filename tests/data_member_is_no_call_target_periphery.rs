//! Periphery (API / contract / integration) tests for spec 116 criterion 4, A DATA MEMBER IS NO
//! CALL TARGET: a named code entity whose kind is `field`, `property` or `declaration` is read
//! by every call-side reader as a bare placeholder is, and by `locate` as the definition it is.
//! The crates' own tests pin each reader from inside; these run OUTSIDE the crates and guard
//! the boundaries the rule crosses on its way to an operator:
//!
//!  - the wire form: the kind strings a code entity event carries, which a graph folded before
//!    the constants existed already holds, and the three of them that are no call target;
//!  - the public projection: `Projector::apply` of hand-built entity events, then
//!    `Projection::calls` in both directions, `Projector::locate`, the tiers the whole graph
//!    serves and the nodes a test proves, over each of the three kinds;
//!  - the binary: `rigger graph --show` over a project graph holding a field, and a second
//!    `rigger graph build` retiring a fn beside a member of its leaf;
//!  - the files lens, whole-graph and re-projected, in process and through the served route;
//!  - the coupling layer the code lens detects communities over.
//!
//! No grammar yields one of the three kinds yet, so each enters as the entity event the fold
//! reads. The rebuild test extracts real source, so it runs on the `symbols` lane alone; both
//! lanes run every other test.

mod common;
#[cfg(feature = "symbols")]
#[path = "common/symbol_index.rs"]
mod symbol_index;

use common::cli::{
    ambiguous_show, open_graph, run_rigger_ok, seed_rigger_dir, show_head, temp_project,
};
use common::fixtures::{
    apply_call, apply_code_entity, apply_def, apply_edge_inferred_evidence, call_edge_pairs,
    call_frontiers, call_layer, call_node_ids, call_walk, edge, kinded_def_node, located_many,
    node_with_optional_attrs, plain,
};
use common::lens::{coupling_rows, files_lens_rows, SUB_C as CONCEPT, SUB_C_FILES_ROUTE};
use common::served::served_json;
use rigger::contextgraph::sqlite::Projector;
use rigger::contextgraph::{
    is_call_target, CallGraph, Direction, Graph, Projection, CODE_KIND_CONSTANT, CODE_KIND_FIELD,
    CODE_KIND_FUNCTION, CODE_KIND_IMPL, CODE_KIND_METHOD, CODE_KIND_MODULE, CODE_KIND_OTHER,
    CODE_KIND_PROPERTY, CODE_KIND_TRAIT, CODE_KIND_TYPE, KIND_CODE_ENTITY, KIND_CONCEPT,
    NON_TARGET_KINDS, REL_CALLS, REL_CONTAINS, REL_REALIZES, TIER_AMBIGUOUS, TIER_EXTRACTED,
    TIER_INFERRED,
};

/// The three kind strings no call resolves onto, as an entity event spells them.
const NON_TARGETS: [&str; 3] = ["declaration", "field", "property"];

/// The ten kind constants this criterion names carry the strings a folded graph already holds,
/// and the non-target kinds are exactly `field`, `property` and `declaration`.
#[test]
fn the_kind_constants_carry_the_strings_an_entity_event_spells() {
    assert_eq!(
        [
            CODE_KIND_FUNCTION,
            CODE_KIND_METHOD,
            CODE_KIND_TYPE,
            CODE_KIND_TRAIT,
            CODE_KIND_IMPL,
            CODE_KIND_MODULE,
            CODE_KIND_CONSTANT,
            CODE_KIND_OTHER,
            CODE_KIND_FIELD,
            CODE_KIND_PROPERTY,
        ],
        [
            "function", "method", "type", "trait", "impl", "module", "constant", "other", "field",
            "property",
        ],
        "each kind constant is the kind string its entity event carries"
    );
    let mut non_targets = NON_TARGET_KINDS;
    non_targets.sort_unstable();
    assert_eq!(
        non_targets, NON_TARGETS,
        "the non-target kinds are a declaration and the two data members"
    );
}

/// `is_call_target` is false for exactly the three non-target kind strings: true for every
/// other kind an entity event carries, for the empty string and for an absent kind.
#[test]
fn is_call_target_refuses_exactly_the_three_non_target_kind_strings() {
    let answers: Vec<(Option<&str>, bool)> = [
        None,
        Some(""),
        Some("constant"),
        Some("constructor"),
        Some("declaration"),
        Some("field"),
        Some("function"),
        Some("impl"),
        Some("macro"),
        Some("method"),
        Some("module"),
        Some("other"),
        Some("property"),
        Some("trait"),
        Some("type"),
    ]
    .into_iter()
    .map(|kind| (kind, is_call_target(kind)))
    .collect();
    assert_eq!(
        answers,
        [
            (None, true),
            (Some(""), true),
            (Some("constant"), true),
            (Some("constructor"), true),
            (Some("declaration"), false),
            (Some("field"), false),
            (Some("function"), true),
            (Some("impl"), true),
            (Some("macro"), true),
            (Some("method"), true),
            (Some("module"), true),
            (Some("other"), true),
            (Some("property"), false),
            (Some("trait"), true),
            (Some("type"), true),
        ],
        "a declaration, a field and a property are the only kinds a call does not resolve onto"
    );
}

/// The `<rel> <tier>` of each live edge of `p` onto `target`, sorted.
fn tiers_onto(p: &Projector, target: &str) -> Vec<String> {
    let mut tiers: Vec<String> = p
        .whole()
        .unwrap()
        .edges
        .iter()
        .filter(|e| e.to == target)
        .map(|e| format!("{} {}", e.rel, e.tier))
        .collect();
    tiers.sort();
    tiers
}

/// A projection where `src/a.rs` holds `fn caller` calling `open`: each `(file, kind)` of
/// `early` is a definition of `open` folded before the call, each of `late` one folded after.
fn caller_of_open(early: &[(&str, &str)], late: &[(&str, &str)]) -> Projector {
    let p = Projector::open(":memory:", "test").unwrap();
    apply_def(&p, 1, "src/a.rs", "caller", 1, true);
    let mut pos = 1;
    for (file, kind) in early {
        pos += 1;
        apply_code_entity(&p, pos, file, "open", kind, 2, "rust");
    }
    pos += 1;
    apply_call(&p, pos, "src/a.rs", "open", "caller");
    for (file, kind) in late {
        pos += 1;
        apply_code_entity(&p, pos, file, "open", kind, 2, "rust");
    }
    p
}

/// The one call of [`caller_of_open`] landed on `src/b.rs`'s fn.
fn the_call_onto_the_fn() -> Vec<(String, String)> {
    vec![("src/a.rs::caller".to_string(), "src/b.rs::open".to_string())]
}

/// GIVEN a same-file call naming the leaf of a field, a property or a declaration, with the one
/// fn of that leaf in another file, WHEN the public walks run, THEN the DOWN walk lands the call
/// on that fn, the UP walk from the fn reaches the caller, the UP walk seeded at the member has
/// no caller, and `locate` answers the leaf with the member among its candidates.
#[test]
fn a_call_naming_a_members_leaf_walks_to_the_other_files_fn_in_both_directions() {
    for kind in NON_TARGETS {
        let p = caller_of_open(&[("src/b.rs", "function"), ("src/a.rs", kind)], &[]);

        let down = call_walk(&p, "src/a.rs::caller", Direction::Down);
        assert_eq!(
            call_node_ids(&down),
            ["src/a.rs::caller", "src/b.rs::open"],
            "{kind}: the DOWN walk reaches the fn and never the member"
        );
        assert_eq!(
            call_edge_pairs(&down),
            the_call_onto_the_fn(),
            "{kind}: the call resolves down to the other file's fn"
        );
        assert_eq!(
            call_layer(&down, "src/b.rs::open"),
            Some(1),
            "{kind}: the fn sits one hop below the caller"
        );
        assert_eq!(
            call_frontiers(&down),
            Vec::<(String, Vec<String>)>::new(),
            "{kind}: one call target of the leaf is no frontier"
        );

        let up = call_walk(&p, "src/b.rs::open", Direction::Up);
        assert_eq!(
            call_node_ids(&up),
            ["src/a.rs::caller", "src/b.rs::open"],
            "{kind}: the UP walk from the fn reaches the same-file call's caller"
        );
        assert_eq!(
            call_edge_pairs(&up),
            the_call_onto_the_fn(),
            "{kind}: the caller's edge lands on the fn"
        );
        assert_eq!(
            call_layer(&up, "src/a.rs::caller"),
            Some(1),
            "{kind}: the caller sits one hop above the fn"
        );

        let at_member = call_walk(&p, "src/a.rs::open", Direction::Up);
        assert_eq!(
            call_node_ids(&at_member),
            ["src/a.rs::open"],
            "{kind}: the member seeded UP is alone"
        );
        assert_eq!(
            call_edge_pairs(&at_member),
            Vec::<(String, String)>::new(),
            "{kind}: the member seeded UP has no caller"
        );

        assert_eq!(
            p.locate("open").unwrap(),
            located_many(&[
                ("src/a.rs::open", "src/a.rs"),
                ("src/b.rs::open", "src/b.rs")
            ]),
            "{kind}: a lookup by name lists the member beside the fn"
        );
    }
}

/// The call and its reference are tiered beside a member of the called leaf as with no member
/// present: inferred, onto the caller's placeholder `src/a.rs::%open`, the member's own node
/// keeping its `CONTAINS` edge alone. A fn of the leaf in the caller's own file is what tiers
/// them extracted, onto that fn.
#[test]
fn a_member_of_the_called_leaf_changes_no_tier_of_the_call() {
    assert_eq!(
        tiers_onto(
            &caller_of_open(&[("src/a.rs", "function")], &[]),
            "src/a.rs::open"
        ),
        [
            "CALLS extracted",
            "CONTAINS extracted",
            "REFERENCES extracted"
        ],
        "a same-file fn of the leaf is the resolved local call"
    );
    assert_eq!(
        tiers_onto(
            &caller_of_open(&[("src/b.rs", "function")], &[]),
            "src/a.rs::%open"
        ),
        ["CALLS inferred", "REFERENCES inferred"],
        "with no member the call is the inferred cross-file call"
    );
    for kind in NON_TARGETS {
        let p = caller_of_open(&[("src/b.rs", "function"), ("src/a.rs", kind)], &[]);
        assert_eq!(
            tiers_onto(&p, "src/a.rs::%open"),
            ["CALLS inferred", "REFERENCES inferred"],
            "{kind}: the call keeps the tier it has with no member"
        );
        assert_eq!(
            tiers_onto(&p, "src/a.rs::open"),
            ["CONTAINS extracted"],
            "{kind}: the member keeps its own CONTAINS edge alone"
        );
    }
}

/// The node ids the DOWN walk from the caller reaches at the ambiguous floor.
fn reached_at_the_ambiguous_floor(p: &Projector) -> Vec<String> {
    call_node_ids(
        &p.calls(
            &["src/a.rs::caller".to_string()],
            Direction::Down,
            5,
            TIER_AMBIGUOUS,
        )
        .unwrap(),
    )
}

/// GIVEN a call already folded onto a name nothing defines, WHEN a field, a property or a
/// declaration of that name folds in another file, THEN the call keeps the tier it had and the
/// walk still ends at the placeholder; a fn folding there instead upgrades it and is reached.
#[test]
fn a_member_folding_after_a_call_upgrades_no_tier_and_adds_no_callee() {
    assert_eq!(
        tiers_onto(&caller_of_open(&[], &[]), "src/a.rs::%open"),
        ["CALLS ambiguous", "REFERENCES ambiguous"],
        "a call onto a name nothing defines is ambiguous"
    );
    let defined = caller_of_open(&[], &[("src/c.rs", "function")]);
    assert_eq!(
        tiers_onto(&defined, "src/a.rs::%open"),
        ["CALLS inferred", "REFERENCES inferred"],
        "a fn of the name folding later upgrades the call"
    );
    assert_eq!(
        reached_at_the_ambiguous_floor(&defined),
        ["src/a.rs::caller", "src/c.rs::open"],
        "a fn of the name is the callee"
    );
    for kind in NON_TARGETS {
        let p = caller_of_open(&[], &[("src/c.rs", kind)]);
        assert_eq!(
            tiers_onto(&p, "src/a.rs::%open"),
            ["CALLS ambiguous", "REFERENCES ambiguous"],
            "{kind}: the member upgrades no tier"
        );
        assert_eq!(
            reached_at_the_ambiguous_floor(&p),
            ["src/a.rs::%open", "src/a.rs::caller"],
            "{kind}: the walk ends at the placeholder, never at the member"
        );
    }
}

/// GIVEN two call targets of a leaf in two files, a fn and a constant, WHEN a call names the
/// leaf beside members of it in the caller's file and in a fourth file, THEN the frontier holds
/// the fn and the constant, sorted, and no member.
#[test]
fn a_member_adds_no_candidate_to_a_frontier_a_constant_of_the_leaf_stays_in() {
    for kind in NON_TARGETS {
        let p = caller_of_open(
            &[("src/b.rs", "function"), ("src/a.rs", kind)],
            &[("src/c.rs", "constant"), ("src/d.rs", kind)],
        );

        let down = call_walk(&p, "src/a.rs::caller", Direction::Down);
        assert_eq!(
            call_frontiers(&down),
            [(
                "src/a.rs::%open".to_string(),
                vec!["src/b.rs::open".to_string(), "src/c.rs::open".to_string()],
            )],
            "{kind}: the frontier's candidates are the fn and the constant"
        );
        assert_eq!(
            call_node_ids(&down),
            ["src/a.rs::%open", "src/a.rs::caller"],
            "{kind}: the walk stops at the frontier"
        );
    }
}

/// The UP walk from `src/a.rs::helper` over a projection where `src/a.rs::slot`, of `kind`,
/// calls `helper`, and `src/d.rs::user` calls `slot` across the file boundary.
fn up_through_a_caller_of(kind: &str) -> CallGraph {
    let p = Projector::open(":memory:", "test").unwrap();
    apply_def(&p, 1, "src/a.rs", "helper", 1, true);
    apply_code_entity(&p, 2, "src/a.rs", "slot", kind, 2, "rust");
    apply_call(&p, 3, "src/a.rs", "helper", "slot");
    apply_def(&p, 4, "src/d.rs", "user", 1, true);
    apply_call(&p, 5, "src/d.rs", "slot", "user");
    call_walk(&p, "src/a.rs::helper", Direction::Up)
}

/// A member the UP walk reaches as a caller has no caller of its own: the walk stops there,
/// where a fn in its place carries the walk on to the fn's caller.
#[test]
fn a_member_the_up_walk_reaches_has_no_caller() {
    let through_a_fn = up_through_a_caller_of("function");
    assert_eq!(
        call_node_ids(&through_a_fn),
        ["src/a.rs::helper", "src/a.rs::slot", "src/d.rs::user"],
        "a fn the walk reaches carries it on to its own caller"
    );
    assert_eq!(
        call_layer(&through_a_fn, "src/d.rs::user"),
        Some(2),
        "the fn's caller sits two hops above the seed"
    );
    for kind in NON_TARGETS {
        let up = up_through_a_caller_of(kind);
        assert_eq!(
            call_node_ids(&up),
            ["src/a.rs::helper", "src/a.rs::slot"],
            "{kind}: the walk reaches the member and stops"
        );
        assert_eq!(
            call_edge_pairs(&up),
            [("src/a.rs::slot".to_string(), "src/a.rs::helper".to_string())],
            "{kind}: the member's own call is the one edge"
        );
    }
}

/// The `<id> <proven_by>` of each node of `p` a test proves, sorted.
fn proven(p: &Projector) -> Vec<String> {
    let mut proven: Vec<String> = p
        .whole()
        .unwrap()
        .nodes
        .iter()
        .filter_map(|n| Some(format!("{} {}", n.id, n.attrs.get("proven_by")?)))
        .collect();
    proven.sort();
    proven
}

/// Fold one test-origin reference to `open` from `tests/t.rs` line 3 at `pos`.
fn apply_test_reference(p: &Projector, pos: u64) {
    apply_edge_inferred_evidence(p, pos, "tests/t.rs", "open", 3);
}

/// GIVEN a test referencing `open`, WHEN the one fn `open` stands beside a field, a property or
/// a declaration of that leaf, THEN the proof lands on the fn as its one definition; and WHEN
/// the member is the only `open`, folded before or after the reference, THEN no node is proven.
#[test]
fn a_test_proof_lands_on_the_fn_and_never_on_a_member_of_its_leaf() {
    for kind in NON_TARGETS {
        let beside_a_fn = Projector::open(":memory:", "test").unwrap();
        apply_def(&beside_a_fn, 1, "src/b.rs", "open", 1, true);
        apply_code_entity(&beside_a_fn, 2, "src/a.rs", "open", kind, 2, "rust");
        apply_test_reference(&beside_a_fn, 3);
        assert_eq!(
            proven(&beside_a_fn),
            ["src/b.rs::open 1"],
            "{kind}: the fn is the one definition the proof resolves onto"
        );

        let member_first = Projector::open(":memory:", "test").unwrap();
        apply_code_entity(&member_first, 1, "src/a.rs", "open", kind, 2, "rust");
        apply_test_reference(&member_first, 2);
        assert_eq!(
            proven(&member_first),
            Vec::<String>::new(),
            "{kind}: a reference folding after the member proves nothing"
        );

        let member_last = Projector::open(":memory:", "test").unwrap();
        apply_test_reference(&member_last, 1);
        apply_code_entity(&member_last, 2, "src/a.rs", "open", kind, 2, "rust");
        assert_eq!(
            proven(&member_last),
            Vec::<String>::new(),
            "{kind}: a member folding after the reference takes no waiting proof"
        );
    }
    let fn_last = Projector::open(":memory:", "test").unwrap();
    apply_test_reference(&fn_last, 1);
    apply_def(&fn_last, 2, "src/a.rs", "open", 2, true);
    assert_eq!(
        proven(&fn_last),
        ["src/a.rs::open 1"],
        "a fn folding after the reference takes the waiting proof"
    );
}

/// GIVEN a project graph where `src/a.rs` holds a field `open` and `src/b.rs` a fn `open`, and
/// a field `state` no fn shares, WHEN the operator runs `rigger graph --show` on each leaf, THEN
/// the shared leaf lists the field beside the fn and the lone field is shown at its own site.
#[test]
fn graph_show_answers_a_fields_leaf() {
    let dir = temp_project();
    let root = dir.path();
    seed_rigger_dir(root);
    {
        let p = open_graph(root);
        apply_def(&p, 1, "src/b.rs", "open", 1, true);
        apply_code_entity(&p, 2, "src/a.rs", "open", "field", 2, "rust");
        apply_code_entity(&p, 3, "src/a.rs", "state", "field", 3, "rust");
    }

    assert_eq!(
        run_rigger_ok(root, &["graph", "--show", "open"]),
        ambiguous_show(
            "open",
            &[
                ("src/a.rs::open", "src/a.rs"),
                ("src/b.rs::open", "src/b.rs")
            ]
        ),
        "a leaf a field and a fn share lists both"
    );
    assert_eq!(
        show_head(root, "state"),
        [
            "show src/a.rs::state",
            "  site: src/a.rs:3   kind field   degree 1"
        ],
        "a lone field's leaf is located at the field's own site"
    );
}

/// A graph where the concept realizes three members: `src/a.rs::x`, a definition of `kind`;
/// `src/c.rs::caller`, a fn; and `src/c.rs::%x`, the bare placeholder `caller` calls. With
/// `with_fn`, `src/b.rs::x`, a fn outside the concept, defines the leaf too.
fn lens_graph(kind: &str, with_fn: bool) -> Graph {
    let mut nodes = vec![
        node_with_optional_attrs(CONCEPT, KIND_CONCEPT, &[("label", Some("the pipeline"))]),
        kinded_def_node("src/a.rs::x", kind),
        kinded_def_node("src/c.rs::caller", "function"),
        plain("src/c.rs::%x", KIND_CODE_ENTITY),
    ];
    if with_fn {
        nodes.push(kinded_def_node("src/b.rs::x", "function"));
    }
    Graph {
        nodes,
        edges: vec![
            edge("src/a.rs::x", CONCEPT, REL_REALIZES, TIER_EXTRACTED),
            edge("src/c.rs::caller", CONCEPT, REL_REALIZES, TIER_EXTRACTED),
            edge("src/c.rs::%x", CONCEPT, REL_REALIZES, TIER_EXTRACTED),
            edge("src/c.rs::caller", "src/c.rs::%x", REL_CALLS, TIER_INFERRED),
        ],
    }
}

/// GIVEN a bare placeholder whose leaf a field, a property or a declaration in another file
/// shares, WHEN the files lens folds the whole graph and re-grains the concept all three
/// realize, THEN the member sits under its own file; with no fn of the leaf the placeholder
/// folds nowhere and is unresolved with no candidate; with one fn of the leaf it resolves onto
/// that fn's file alone. A fn in the member's place takes the placeholder.
#[test]
fn the_files_lens_folds_a_member_under_its_own_file_and_resolves_no_placeholder_onto_it() {
    for kind in NON_TARGETS {
        assert_eq!(
            files_lens_rows(&lens_graph(kind, false), CONCEPT, 3),
            [
                "whole src/a.rs 1",
                "whole src/c.rs 1",
                "concept src/a.rs 1",
                "concept src/c.rs 1",
                "concept src/c.rs::%x <- []",
            ],
            "{kind}: the member is its file's one entity and the placeholder has no candidate"
        );
        assert_eq!(
            files_lens_rows(&lens_graph(kind, true), CONCEPT, 3),
            [
                "whole src/a.rs 1",
                "whole src/b.rs 2",
                "whole src/c.rs 1",
                "whole src/b.rs -- src/c.rs 1",
                "concept src/a.rs 1",
                "concept src/b.rs 1",
                "concept src/c.rs 1",
            ],
            "{kind}: the placeholder resolves onto the fn's file, the member no second candidate"
        );
    }
    assert_eq!(
        files_lens_rows(&lens_graph("function", false), CONCEPT, 3),
        [
            "whole src/a.rs 2",
            "whole src/c.rs 1",
            "whole src/a.rs -- src/c.rs 1",
            "concept src/a.rs 2",
            "concept src/c.rs 1",
        ],
        "a fn in the member's place takes the placeholder"
    );
}

/// The served `/api/graph` route carries the same re-grain: the field under its own file and
/// the placeholder in the unresolved sidecar with no candidate.
#[test]
fn the_served_files_regrain_keeps_a_placeholder_off_a_field() {
    let body = served_json(&lens_graph("field", false), SUB_C_FILES_ROUTE);
    assert_eq!(
        body["clusters"]
            .as_array()
            .expect("a clusters array")
            .iter()
            .map(|c| (c["key"].as_str().unwrap(), c["count"].as_u64().unwrap()))
            .collect::<Vec<_>>(),
        [("src/a.rs", 1), ("src/c.rs", 1)],
        "the served re-grain folds the field under its own file: {body}"
    );
    assert_eq!(
        body["unresolved"],
        serde_json::json!([{ "id": "src/c.rs::%x", "candidates": [] }]),
        "the served placeholder names no candidate: {body}"
    );
}

/// The coupling layer's `<node> <degree>` rows over a graph where `src/a.rs` contains
/// `src/a.rs::x`, a definition of `kind`, and `src/c.rs::caller` calls the bare `src/c.rs::%x`.
fn coupling_beside(kind: &str) -> Vec<String> {
    let graph = Graph {
        nodes: vec![
            kinded_def_node("src/a.rs::x", kind),
            kinded_def_node("src/c.rs::caller", "function"),
            plain("src/c.rs::%x", KIND_CODE_ENTITY),
        ],
        edges: vec![
            edge("src/a.rs", "src/a.rs::x", REL_CONTAINS, TIER_EXTRACTED),
            edge("src/c.rs::caller", "src/c.rs::%x", REL_CALLS, TIER_INFERRED),
        ],
    };
    coupling_rows(&graph)
}

/// GIVEN a bare placeholder whose leaf one field, property or declaration shares, WHEN the
/// coupling layer is built, THEN the placeholder stays its own node and the member keeps its
/// one edge; a fn in the member's place takes the placeholder's edge.
#[test]
fn the_coupling_layer_redirects_no_placeholder_onto_a_member_and_keeps_the_members_edges() {
    for kind in NON_TARGETS {
        assert_eq!(
            coupling_beside(kind),
            [
                "src/a.rs 1",
                "src/a.rs::x 1",
                "src/c.rs::%x 1",
                "src/c.rs::caller 1"
            ],
            "{kind}: the placeholder is its own node and the member keeps its CONTAINS edge"
        );
    }
    assert_eq!(
        coupling_beside("function"),
        ["src/a.rs 1", "src/a.rs::x 2", "src/c.rs::caller 1"],
        "a fn of the leaf takes the placeholder's edge"
    );
}

/// The tiers of the live edges onto `b.rs::%x` ([`tiers_onto`]), and whether `a.rs::x` is still
/// a node, after `rigger graph build` folded a tree where `f.rs` defines `fn x` and `b.rs`'s
/// `fn g` calls it, an `a.rs::x` of kind `beside` was folded into the project graph when one is
/// named, and a second build folded the tree with `f.rs` defining `fn y` instead.
#[cfg(feature = "symbols")]
fn tiers_after_the_fn_retires(beside: Option<&str>) -> (Vec<String>, bool) {
    let (dir, graph) =
        common::cli::built_graph(&[("f.rs", "fn x() {}\n"), ("b.rs", "fn g() { x(); }\n")]);
    assert_eq!(
        tiers_onto(&graph, "b.rs::%x"),
        ["CALLS inferred", "REFERENCES inferred"],
        "while the fn stands the cross-file call is inferred"
    );
    if let Some(kind) = beside {
        apply_code_entity(&graph, 1_000_000, "a.rs", "x", kind, 1, "rust");
    }
    drop(graph);

    let graph = common::cli::rebuilt_graph(dir.path(), &[("f.rs", symbol_index::FN_Y)]);
    let member_stands = graph
        .whole()
        .unwrap()
        .nodes
        .iter()
        .any(|n| n.id == "a.rs::x");
    (tiers_onto(&graph, "b.rs::%x"), member_stands)
}

/// GIVEN a built project whose one fn `x` a cross-file call reaches, WHEN the fn leaves its
/// file and the operator runs `rigger graph build` again, THEN the call is demoted to ambiguous
/// beside a field, a property or a declaration of `x` in another file exactly as with nothing
/// beside it, and stays inferred beside a constant of `x`.
#[cfg(feature = "symbols")]
#[test]
fn a_rebuild_retiring_a_fn_demotes_its_cross_file_call_beside_a_member_of_its_leaf() {
    let demoted = || {
        vec![
            "CALLS ambiguous".to_string(),
            "REFERENCES ambiguous".to_string(),
        ]
    };
    assert_eq!(
        tiers_after_the_fn_retires(None),
        (demoted(), false),
        "with nothing beside it the retired fn's call is ambiguous"
    );
    for kind in NON_TARGETS {
        assert_eq!(
            tiers_after_the_fn_retires(Some(kind)),
            (demoted(), true),
            "{kind}: the member still stands and the call is ambiguous all the same"
        );
    }
    assert_eq!(
        tiers_after_the_fn_retires(Some("constant")),
        (
            vec![
                "CALLS inferred".to_string(),
                "REFERENCES inferred".to_string()
            ],
            true
        ),
        "a constant of the leaf still answers the call"
    );
}
