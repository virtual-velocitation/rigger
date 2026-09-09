//! Periphery (contract / integration) tests for spec 86 criterion 1: TESTS ARE NOT NODES. The
//! code-entity emit pass excludes test code - a whole file under a `tests/` directory, and any
//! `#[test]`/`#[cfg(test)]` region inside an otherwise-included file - from graph NODE and EDGE
//! creation, carried on the new `Def.is_test` / `SymRef.is_test` fields.
//!
//! These run OUTSIDE the crate, over the library's public surface, and guard two boundaries:
//!
//!  - the SERIALIZED-FORM back-compat contract of the two new fields - the same shape spec 37's
//!    `enclosing` field carries (see `symbol_ref_caller_attribution.rs`). Both derive
//!    `#[serde(default, skip_serializing_if = "std::ops::Not::not")]`, so (a) `is_test: false`
//!    must serialize BYTE-IDENTICALLY to the pre-86 form - no `is_test` key at all, (b) `is_test:
//!    true` must serialize the key and round-trip it through a real save/load, and (c) an index
//!    persisted by a pre-86 binary (no `is_test` key on any definition or reference) must still
//!    LOAD, defaulting every one to `is_test: false` rather than erroring on a missing field. The
//!    inside-out unit tests never pin any of this: `store.rs`'s own round-trip test only proves
//!    RE-serializing the SAME in-memory index twice is byte-STABLE, never that a false-throughout
//!    index matches the pre-86 wire shape, and no unit test anywhere loads a hand-authored legacy
//!    fixture missing the key. Parser-free, so this section runs in BOTH feature lanes.
//!  - the exclusion rule's Done-when, driven end to end through the crate's PUBLIC API
//!    (`build_index` -> `index_events` -> `Projector`) from OUTSIDE the crate: after ingesting a
//!    fixture with product code, a `tests/` file, a `#[cfg(test)]` module and `#[test]`
//!    functions, the graph holds a code-entity node for every product item and none for any test
//!    item, and the `tests/` file carries no file container node at all - the concrete mechanism
//!    behind "the files lens lists no test file as a subject" (the files lens folds by existing
//!    file nodes; a file with none can never be one of its subjects). The implementer's own
//!    `events.rs` unit tests already prove this same contract in-crate, but that proof rests on
//!    the implementer's own understanding of the boundary - the periphery layer exists precisely
//!    so that guarantee does not rest on one role's judgment alone, so this test drives the same
//!    Done-when independently, over a fixture of its own, through the public surface only. Lives
//!    in the `symbols` lane only (it drives the real tree-sitter extraction pass).
//!  - the round-2 cfg-predicate fix (review REJECT `adj-u86c1-verdict-reject` /
//!    `adv-u86c1-cfg-predicate-negation-and-cfg-attr-inverted`), independently, through the SAME
//!    public API: `#[cfg(not(test))]` (the production-only half of a dual-cfg mock construct) and
//!    `#[cfg_attr(test, ..)]` (an always-compiled item) must both reach the graph as ordinary
//!    product code-entity nodes. The implementer's own `extract.rs` unit test
//!    (`negated_and_cfg_attr_predicates_naming_test_do_not_mark_the_item_test`) proves this at the
//!    `FileSymbols` level, inside the crate; this test proves the SAME contract at the periphery,
//!    end to end through `build_index` -> `index_events` -> `Projector`, so the guarantee that a
//!    dual-cfg mock's product half is never dropped does not rest on the author's own judgment.
//!  - the round-3 fix for round 2's own recurrence (review REJECT `adj-u86c1-verdict-reject`
//!    round 2, findings `arch-u86c1-r2-compound-not-predicate-still-marks-product-code-test` /
//!    `sdet-u86c1-r2-cfg-predicate-fix-does-not-generalize-to-nested-negation-or-any` /
//!    `adv-u86c1-r2-trailing-comment-severs-the-attribute-stack-scan`), independently, through the
//!    SAME public API: a `not(test)` nested inside `all(..)`, a `test` disjunct sitting alongside a
//!    non-test one inside `any(..)`, and a `#[cfg(test)]` attribute carrying a trailing same-line
//!    `//` comment, must each resolve exactly as their un-nested/un-commented counterparts already
//!    do - the two compound predicates graphing their item as ordinary product code, and the
//!    trailing-commented module (and everything nested inside it) staying excluded end to end.
//!  - a round-3 sdet-author finding the round-3 fix's own two fixtures do not reach
//!    (`sdet-u86c1-r3-embedded-slash-attribute-plus-trailing-comment-severs-scan`): round 3's
//!    trailing-comment remedy truncates a line at its FIRST `//`, which is the comment marker
//!    only when the attribute's OWN text contains no `//` of its own. An ordinary
//!    `#[doc = "https://..."]` attribute, carrying a genuine trailing comment, sitting between a
//!    `#[test]` attribute and the item it tags, truncates at the URL's `//` instead, fails the
//!    `#[...]` shape check, and severs the upward scan before it ever reaches `#[test]` - the
//!    tagged item leaks into the graph as ordinary product code, the same failure direction as
//!    the round-2 finding this file already guards above.
//!  - the round-4 recurrence (review REJECT `adj-u86c1-verdict-reject` round 4, finding
//!    `adv-u86c1-r4-inner-cfg-test-attribute-not-recognized`), independently, through the SAME
//!    public API: the INNER-attribute form of a test module, `mod tests { #![cfg(test)] .. }`
//!    (the attribute the FIRST node inside the mod's own body, rather than a sibling before the
//!    `mod` keyword), must exclude the module and everything nested inside it exactly as the
//!    already-covered OUTER-attribute form `#[cfg(test)] mod tests { .. }` does. The
//!    implementer's own `extract.rs` unit test
//!    (`an_inner_cfg_test_attribute_marks_its_enclosing_module_and_the_module_marks_its_children`)
//!    proves this at the `FileSymbols` level; this test proves the SAME contract at the
//!    periphery, end to end through `build_index` -> `index_events` -> `Projector`.
//!  - the rest of `op-u86c1-r5-close-every-remaining-test-shape` (amending
//!    `op-u86c1-r4-structural-attribute-walk-is-the-only-remedy`), which mandates round 5 close
//!    every remaining test-shape item in one round, not one per round. Item 1 (the inner-attribute
//!    module form) is covered above; item 4 (`#[cfg_attr(test, ..)]` staying product) was already
//!    covered since round 1/2. The remaining two are periphery-tested here, empirically confirmed
//!    (probe-then-revert, `sdet-u86c1-r5-two-confirmed-live-gaps`) as LIVE, currently-open gaps -
//!    these two tests are EXPECTED TO FAIL until a future round's fix lands, exactly like the
//!    round-3 sdet commit's own URL-bearing-attribute case did before round 4 closed it:
//!    - item 2, an OUT-OF-LINE `#[cfg(test)] mod name;` declaration whose declared FILE carries no
//!      attribute of its own (the attribute lives in a different file's tree entirely) - LIVE in
//!      this repo today (`src/eventstore/mod.rs`'s `mod contract`, `src/lib.rs`'s `mod
//!      blast_radius_eval`) - must exclude the declared file in full
//!      (`an_out_of_line_cfg_test_module_declaration_excludes_its_declared_file_through_the_public_api`).
//!    - item 3's `impl_item` case: `tags.scm` never tags an `impl_item` as a definition (only
//!      `@reference.implementation`), so a `#[cfg(test)]`-attributed impl block never becomes a
//!      test-region container the way an attributed `mod` already is, and an unattributed method
//!      inside it leaks as product code
//!      (`a_cfg_test_impl_block_excludes_its_methods_through_the_public_api`).
//!
//!    Item 3's remaining kinds (`struct_item`, `enum_item`, `trait_item`, `type_item`,
//!    `macro_definition`, plus the non-tagged `use_declaration`/`const_item`/`static_item`) are
//!    verified ALREADY correct - the sibling walk is kind-agnostic by construction - and PASS
//!    (`cfg_test_on_every_other_item_kind_excludes_or_stays_scoped_through_the_public_api`).

use rigger::grounder::symbols::model::{Def, FileSymbols, Kind, Lang, SymRef, SymbolIndex};
use rigger::grounder::symbols::store;

// ---- serialized-form / back-compat contract (parser-free model + store: BOTH feature lanes) ----

#[test]
fn is_test_false_serializes_byte_identically_to_the_pre86_form() {
    // `skip_serializing_if = "std::ops::Not::not"`: a definition/reference with `is_test: false`
    // must serialize with NO `is_test` key, so an index of product-only code (the overwhelmingly
    // common case) is byte-identical to what a pre-86 binary wrote. A regression dropping
    // `skip_serializing_if` would emit `"is_test":false` on every definition and reference in the
    // project, rewriting every historical index's bytes - defeating the field's whole "wholly
    // additive, no re-serialization churn" contract.
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_str().unwrap();
    let mut idx = SymbolIndex::default();
    idx.insert_file(
        "a.rs".into(),
        FileSymbols {
            lang: Lang::Rust,
            defs: vec![Def {
                kind: Kind::Function,
                name: "f".into(),
                line: 1,
                is_test: false,
                is_out_of_line_module: false,
            }],
            refs: vec![SymRef {
                name: "g".into(),
                line: 2,
                enclosing: None,
                is_test: false,
            }],
        },
    );
    store::save(&idx, root).unwrap();
    let bytes = std::fs::read_to_string(store::index_path(root)).unwrap();
    assert!(
        !bytes.contains("is_test"),
        "product-only code (is_test: false throughout) must omit the is_test key entirely \
         (byte-identical to the pre-86 on-disk form); got:\n{bytes}"
    );
}

#[test]
fn is_test_true_serializes_the_key_and_round_trips() {
    // The opposite direction of the same contract: a definition/reference WITH `is_test: true`
    // serializes the key and round-trips through the real persistence boundary preserving it.
    // Together with the false-case test above this pins both serde attributes on each field: drop
    // `skip_serializing_if` and the false-case test reds; drop the field's persistence and this
    // one reds.
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_str().unwrap();
    let mut idx = SymbolIndex::default();
    idx.insert_file(
        "tests_mod.rs".into(),
        FileSymbols {
            lang: Lang::Rust,
            defs: vec![Def {
                kind: Kind::Function,
                name: "it_works".into(),
                line: 1,
                is_test: true,
                is_out_of_line_module: false,
            }],
            refs: vec![SymRef {
                name: "helper".into(),
                line: 2,
                enclosing: Some("it_works".into()),
                is_test: true,
            }],
        },
    );
    store::save(&idx, root).unwrap();
    let bytes = std::fs::read_to_string(store::index_path(root)).unwrap();
    assert!(
        bytes.contains("is_test"),
        "an is_test: true definition/reference must serialize the is_test key; got:\n{bytes}"
    );
    let loaded = store::load(root).expect("the persisted index loads");
    let file = &loaded.files()["tests_mod.rs"];
    assert!(
        file.defs
            .iter()
            .find(|d| d.name == "it_works")
            .unwrap()
            .is_test,
        "is_test: true survives a save/load round-trip on a definition"
    );
    assert!(
        file.refs
            .iter()
            .find(|r| r.name == "helper")
            .unwrap()
            .is_test,
        "is_test: true survives a save/load round-trip on a reference"
    );
}

#[test]
fn a_pre86_persisted_index_with_no_is_test_key_loads_defaulting_every_item_to_false() {
    // `#[serde(default)]`: an `index.json` written by a pre-86 binary has definitions and
    // references with NO `is_test` key at all (the shape pinned by the false-case test above: a
    // product-only entry is exactly `{ "kind", "name", "line" }` / `{ "name", "line" }`).
    // `store::load` must still deserialize it - not error on a missing field - folding every item
    // `is_test: false`, never manufacturing a false exclusion of pre-86 data.
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_str().unwrap();
    let path = store::index_path(root);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    // A pre-86 index: one definition and one reference, neither carrying an `is_test` key.
    let legacy = r#"{
  "files": {
    "legacy.rs": {
      "lang": "Rust",
      "defs": [
        { "kind": "Function", "name": "legacy_fn", "line": 1 }
      ],
      "refs": [
        { "name": "callee", "line": 2 }
      ]
    }
  }
}"#;
    std::fs::write(&path, legacy).unwrap();

    let loaded =
        store::load(root).expect("a pre-86 index (no is_test key) must still load, not error");
    let file = loaded
        .files()
        .get("legacy.rs")
        .expect("the legacy file entry is present");
    assert_eq!(
        file.defs.len(),
        1,
        "the legacy definition survives the load"
    );
    assert!(
        !file.defs[0].is_test,
        "a definition persisted before is_test existed defaults to false, not an error and not \
         true"
    );
    assert_eq!(file.refs.len(), 1, "the legacy reference survives the load");
    assert!(
        !file.refs[0].is_test,
        "a reference persisted before is_test existed defaults to false"
    );
    // The rest of the entry deserialized intact, so a wrong-shape fixture reds loudly here rather
    // than passing vacuously on an empty load.
    assert_eq!(file.lang, Lang::Rust);
    assert_eq!(file.defs[0].name, "legacy_fn");
    assert_eq!(file.refs[0].name, "callee");
}

// ---- the round-6 is_out_of_line_module field's own wire-format / back-compat contract -----
// (`Def::is_out_of_line_module`, `op-u86c1-r5-close-every-remaining-test-shape` item 2). The
// implementer's own round-6 fixtures set this field on every literal they touch (mechanical
// ripple), but no test anywhere - implementer or periphery - independently pins ITS OWN
// serde(default, skip_serializing_if) contract the way `is_test`'s trio above does; the three
// tests above only assert on the `is_test` key's presence/absence, never `is_out_of_line_module`'s,
// so a regression dropping this field's own `skip_serializing_if` (rewriting every historical
// index's bytes) or its own `#[serde(default)]` (erroring a pre-round-6 index dead instead of
// loading it) would pass every existing test in this file undetected. Mirrors the `is_test` trio
// exactly, one field later.

#[test]
fn is_out_of_line_module_false_serializes_byte_identically_to_the_pre_round6_form() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_str().unwrap();
    let mut idx = SymbolIndex::default();
    idx.insert_file(
        "a.rs".into(),
        FileSymbols {
            lang: Lang::Rust,
            defs: vec![Def {
                kind: Kind::Module,
                name: "inline_mod".into(),
                line: 1,
                is_test: false,
                is_out_of_line_module: false,
            }],
            refs: vec![],
        },
    );
    store::save(&idx, root).unwrap();
    let bytes = std::fs::read_to_string(store::index_path(root)).unwrap();
    assert!(
        !bytes.contains("is_out_of_line_module"),
        "an inline (has-a-body) module - is_out_of_line_module: false - must omit the key \
         entirely, byte-identical to the pre-round-6 on-disk form; got:\n{bytes}"
    );
}

#[test]
fn is_out_of_line_module_true_serializes_the_key_and_round_trips() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_str().unwrap();
    let mut idx = SymbolIndex::default();
    idx.insert_file(
        "parent.rs".into(),
        FileSymbols {
            lang: Lang::Rust,
            defs: vec![Def {
                kind: Kind::Module,
                name: "contract".into(),
                line: 1,
                is_test: true,
                is_out_of_line_module: true,
            }],
            refs: vec![],
        },
    );
    store::save(&idx, root).unwrap();
    let bytes = std::fs::read_to_string(store::index_path(root)).unwrap();
    assert!(
        bytes.contains("is_out_of_line_module"),
        "an out-of-line `mod name;` declaration - is_out_of_line_module: true - must serialize \
         the key; got:\n{bytes}"
    );
    let loaded = store::load(root).expect("the persisted index loads");
    let file = &loaded.files()["parent.rs"];
    assert!(
        file.defs
            .iter()
            .find(|d| d.name == "contract")
            .unwrap()
            .is_out_of_line_module,
        "is_out_of_line_module: true survives a save/load round-trip"
    );
}

#[test]
fn a_pre_round6_persisted_index_with_no_is_out_of_line_module_key_loads_defaulting_to_false() {
    // Simulates an index written by the round-1..5 binary: `is_test` already exists on the wire
    // (round 1 shipped it), but `is_out_of_line_module` (round 6) does not - the narrower,
    // more-realistic back-compat gap than the full pre-86 fixture above, which has NEITHER key.
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_str().unwrap();
    let path = store::index_path(root);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let legacy = r#"{
  "files": {
    "legacy_mod.rs": {
      "lang": "Rust",
      "defs": [
        { "kind": "Module", "name": "legacy_child", "line": 1, "is_test": true }
      ],
      "refs": []
    }
  }
}"#;
    std::fs::write(&path, legacy).unwrap();

    let loaded = store::load(root)
        .expect("a round-1..5 index (is_test present, is_out_of_line_module absent) must load");
    let file = loaded
        .files()
        .get("legacy_mod.rs")
        .expect("the legacy file entry is present");
    assert!(
        file.defs[0].is_test,
        "the pre-existing is_test key still loads true, unaffected by the new field's absence"
    );
    assert!(
        !file.defs[0].is_out_of_line_module,
        "a definition persisted before is_out_of_line_module existed defaults to false, not an \
         error and not true - never manufacturing a false cross-file exclusion of old data"
    );
}

// ---- the exclusion rule's Done-when, end to end via the public API (symbols lane only) ----

/// A fixture independent of the implementer's own in-crate one: a product file defining
/// `widget` (called from a `#[cfg(test)]` module's plain, UNATTRIBUTED helper - test code by
/// CONTAINMENT, not its own attribute) and a `#[test]` function, alongside a wholly separate
/// `tests/` directory file.
#[cfg(feature = "symbols")]
const WIDGET_SRC: &str = "\
fn widget() {}

#[cfg(test)]
mod tests {
    use super::widget;

    fn drive() {
        widget();
    }

    #[test]
    fn widget_works() {
        drive();
    }
}
";

#[cfg(feature = "symbols")]
const OUTSIDE_IN_TEST_SRC: &str =
    "#[test]\nfn an_outside_in_test() {\n    calls_widget_from_outside();\n}\n";

#[cfg(feature = "symbols")]
#[test]
fn ingesting_product_and_test_code_through_the_public_api_graphs_only_the_product() {
    use rigger::contextgraph::sqlite::Projector;
    use rigger::contextgraph::{Projection, KIND_CODE_ENTITY, REL_CONTAINS, REL_REFERENCES};
    use rigger::grounder::symbols::build_index;
    use rigger::grounder::symbols::events::index_events;
    use std::collections::BTreeSet;

    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("widget.rs"), WIDGET_SRC).unwrap();
    let tests_dir = root.path().join("tests");
    std::fs::create_dir(&tests_dir).unwrap();
    std::fs::write(tests_dir.join("widget_periphery.rs"), OUTSIDE_IN_TEST_SRC).unwrap();

    let idx = build_index(root.path().to_str().unwrap(), None);
    let mut events = index_events(&idx);
    let p = Projector::open(":memory:", "test").unwrap();
    for (zero_based, event) in events.iter_mut().enumerate() {
        event.position = zero_based as u64 + 1;
        p.apply(event).unwrap();
    }

    let seed_files = [
        "widget.rs".to_string(),
        "tests/widget_periphery.rs".to_string(),
    ];
    let g = p.subgraph(&seed_files, 3).unwrap();
    let node_ids: BTreeSet<&str> = g.nodes.iter().map(|n| n.id.as_str()).collect();

    // The product definition became a code-entity node, contained by its own file node.
    let product_node = g
        .nodes
        .iter()
        .find(|n| n.id == "widget.rs::widget")
        .expect("the product definition folds into a code-entity node");
    assert_eq!(product_node.kind, KIND_CODE_ENTITY);
    assert!(
        node_ids.contains("widget.rs"),
        "the product file carries its own file container node; nodes: {node_ids:?}"
    );
    let contains_hits = g
        .edges
        .iter()
        .filter(|e| e.rel == REL_CONTAINS && e.to == "widget.rs::widget")
        .count();
    assert_eq!(
        contains_hits, 1,
        "exactly one CONTAINS edge ties the product file to its product definition"
    );

    // None of the test items - the cfg(test) module, its unattributed helper (test by
    // containment), the #[test] fn, or the tests/ file's own #[test] fn - ever became a node, and
    // the whole tests/ file carries no file container node either (whole-file exclusion, so it
    // contributed no batch - the concrete reason the files lens can never list it as a subject).
    let must_be_absent: BTreeSet<&str> = BTreeSet::from([
        "widget.rs::tests",
        "widget.rs::drive",
        "widget.rs::widget_works",
        "tests/widget_periphery.rs::an_outside_in_test",
        "tests/widget_periphery.rs",
    ]);
    let leaked: BTreeSet<&str> = node_ids.intersection(&must_be_absent).copied().collect();
    assert!(
        leaked.is_empty(),
        "test-scoped items must never reach the graph as nodes; leaked: {leaked:?}"
    );

    // Neither did any structural edge originate from test-only content: the call made only from
    // `drive` (test code) into `widget` never became a REFERENCES edge, and the excluded tests/
    // file's own reference never became an edge of any kind.
    let edges_from_test_content = g
        .edges
        .iter()
        .filter(|e| e.from == "tests/widget_periphery.rs")
        .count();
    assert_eq!(
        edges_from_test_content, 0,
        "an excluded file's references never become structural edges"
    );
    let references_from_test_scoped_call = g
        .edges
        .iter()
        .filter(|e| e.rel == REL_REFERENCES && e.to == "widget.rs::widget")
        .count();
    assert_eq!(
        references_from_test_scoped_call, 0,
        "a call made only from test code must never become a REFERENCES edge"
    );
}

/// A round-2 regression fixture, independent of the implementer's own `extract.rs` fixture: the
/// two shapes `adv-u86c1-cfg-predicate-negation-and-cfg-attr-inverted` proved were wrongly folded
/// to `is_test: true` and dropped from the graph entirely. Neither is test code - `real_client` is
/// the PRODUCTION half of a dual-cfg mock (it compiles whenever `test` is NOT set, so it is
/// definitionally the item that ships), and `RealConfig` is compiled unconditionally (`cfg_attr`
/// only conditionally attaches its trailing `derive`, it never gates the tagged item's own
/// compilation).
#[cfg(feature = "symbols")]
const CFG_PREDICATE_SRC: &str = "\
#[cfg(not(test))]
fn real_client() {}

#[cfg_attr(test, derive(Debug))]
struct RealConfig;
";

#[cfg(feature = "symbols")]
#[test]
fn cfg_not_test_and_cfg_attr_predicates_graph_as_product_through_the_public_api() {
    use rigger::contextgraph::sqlite::Projector;
    use rigger::contextgraph::{Projection, KIND_CODE_ENTITY};

    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("cfgpred.rs"), CFG_PREDICATE_SRC).unwrap();

    let idx = rigger::grounder::symbols::build_index(root.path().to_str().unwrap(), None);
    let mut events = rigger::grounder::symbols::events::index_events(&idx);
    let p = Projector::open(":memory:", "test").unwrap();
    for (zero_based, event) in events.iter_mut().enumerate() {
        event.position = zero_based as u64 + 1;
        p.apply(event).unwrap();
    }

    let g = p.subgraph(&["cfgpred.rs".to_string()], 3).unwrap();

    // `#[cfg(not(test))]` guards the production half of a dual-cfg construct: it must reach the
    // graph as a code-entity node like any other product function, never be folded to test-only
    // and silently dropped.
    let real_client = g
        .nodes
        .iter()
        .find(|n| n.id == "cfgpred.rs::real_client")
        .unwrap_or_else(|| {
            panic!(
                "#[cfg(not(test))] fn real_client must graph as a product code-entity node; \
                 nodes: {:?}",
                g.nodes.iter().map(|n| &n.id).collect::<Vec<_>>()
            )
        });
    assert_eq!(real_client.kind, KIND_CODE_ENTITY);

    // `cfg_attr`'s predicate governs only the inner `derive`, never the tagged item's own
    // compilation, so `RealConfig` is compiled unconditionally and must graph too.
    let real_config = g
        .nodes
        .iter()
        .find(|n| n.id == "cfgpred.rs::RealConfig")
        .unwrap_or_else(|| {
            panic!(
                "#[cfg_attr(test, ..)] struct RealConfig must graph as a product code-entity \
                 node; nodes: {:?}",
                g.nodes.iter().map(|n| &n.id).collect::<Vec<_>>()
            )
        });
    assert_eq!(real_config.kind, KIND_CODE_ENTITY);
}

/// Mutation-efficacy pin (round 6 `cargo mutants` finding, `predicate_group_names_test`'s `not`
/// arm): `not(P)`'s test-only-ness is only answerable by inverting P's answer when P is built
/// PURELY from `test` - inverting a MIXED predicate like `feature = "x"` (independent of `test`
/// altogether) is unsound and wrongly marked this ordinary product function as test code,
/// independently proved here through the SAME public API as the sibling `not(test)` fixture above.
#[cfg(feature = "symbols")]
#[test]
fn a_not_wrapping_a_non_test_atom_graphs_as_product_through_the_public_api() {
    use rigger::contextgraph::sqlite::Projector;
    use rigger::contextgraph::{Projection, KIND_CODE_ENTITY};

    let root = tempfile::tempdir().unwrap();
    std::fs::write(
        root.path().join("notfeature.rs"),
        "#[cfg(not(feature = \"x\"))]\nfn real_client() {}\n",
    )
    .unwrap();

    let idx = rigger::grounder::symbols::build_index(root.path().to_str().unwrap(), None);
    let mut events = rigger::grounder::symbols::events::index_events(&idx);
    let p = Projector::open(":memory:", "test").unwrap();
    for (zero_based, event) in events.iter_mut().enumerate() {
        event.position = zero_based as u64 + 1;
        p.apply(event).unwrap();
    }

    let g = p.subgraph(&["notfeature.rs".to_string()], 3).unwrap();
    let real_client = g
        .nodes
        .iter()
        .find(|n| n.id == "notfeature.rs::real_client")
        .unwrap_or_else(|| {
            panic!(
                "#[cfg(not(feature = \"x\"))] fn real_client is independent of test altogether \
                 and must graph as a product code-entity node; nodes: {:?}",
                g.nodes.iter().map(|n| &n.id).collect::<Vec<_>>()
            )
        });
    assert_eq!(real_client.kind, KIND_CODE_ENTITY);
}

/// sdet-author gap: the round-6 fix's own `double_negation_is_test_only` (`not(not(test))` cancels
/// back to test-only, since the inner `not(test)` IS pure) is pinned only at the `extract.rs`
/// `FileSymbols` level by the implementer's own unit test - never independently through the public
/// API the way its sibling `not(feature = "x")` shape is above. Proving it here closes that gap:
/// unlike the mixed-predicate case, a doubly-negated `test` is PURE test algebra throughout, so
/// the item must be EXCLUDED, the opposite assertion direction from the test above.
#[cfg(feature = "symbols")]
#[test]
fn a_double_negation_of_test_excludes_the_item_through_the_public_api() {
    use rigger::contextgraph::sqlite::Projector;
    use rigger::contextgraph::Projection;

    let root = tempfile::tempdir().unwrap();
    std::fs::write(
        root.path().join("doublenot.rs"),
        "#[cfg(not(not(test)))]\nfn double_negation_is_test_only() {}\n",
    )
    .unwrap();

    let idx = rigger::grounder::symbols::build_index(root.path().to_str().unwrap(), None);
    let mut events = rigger::grounder::symbols::events::index_events(&idx);
    let p = Projector::open(":memory:", "test").unwrap();
    for (zero_based, event) in events.iter_mut().enumerate() {
        event.position = zero_based as u64 + 1;
        p.apply(event).unwrap();
    }

    let g = p.subgraph(&["doublenot.rs".to_string()], 3).unwrap();
    assert!(
        !g.nodes
            .iter()
            .any(|n| n.id == "doublenot.rs::double_negation_is_test_only"),
        "#[cfg(not(not(test)))] cancels back to a PURE test-only predicate (unlike the mixed \
         not(feature = \"x\") case above) and must be excluded exactly like a bare #[cfg(test)]; \
         nodes: {:?}",
        g.nodes
    );
}

/// A round-3 regression fixture, independent of the implementer's own `extract.rs` fixture: the
/// two compound-predicate shapes `arch-u86c1-r2-compound-not-predicate-still-marks-product-code-
/// test` / `sdet-u86c1-r2-cfg-predicate-fix-does-not-generalize-to-nested-negation-or-any` proved
/// round 2's top-level-only `not(..)` special case did not generalize to. Neither item is test
/// code: `dual_cfg_mock_production_half` compiles whenever `test` is NOT set (a `not(test)`
/// conjunct nested inside `all(..)` - the production half of a dual-cfg construct), and
/// `debug_only_helper` compiles whenever `debug_assertions` is set regardless of `test` (a `test`
/// disjunct inside `any(..)` alongside a non-test one - a debug-only helper that ships in every
/// non-release build).
#[cfg(feature = "symbols")]
const COMPOUND_CFG_PREDICATE_SRC: &str = "\
#[cfg(all(not(test), feature = \"x\"))]
fn dual_cfg_mock_production_half() {}

#[cfg(any(debug_assertions, test))]
fn debug_only_helper() {}
";

#[cfg(feature = "symbols")]
#[test]
fn compound_cfg_predicates_graph_as_product_through_the_public_api() {
    use rigger::contextgraph::sqlite::Projector;
    use rigger::contextgraph::{Projection, KIND_CODE_ENTITY};

    let root = tempfile::tempdir().unwrap();
    std::fs::write(
        root.path().join("compoundcfg.rs"),
        COMPOUND_CFG_PREDICATE_SRC,
    )
    .unwrap();

    let idx = rigger::grounder::symbols::build_index(root.path().to_str().unwrap(), None);
    let mut events = rigger::grounder::symbols::events::index_events(&idx);
    let p = Projector::open(":memory:", "test").unwrap();
    for (zero_based, event) in events.iter_mut().enumerate() {
        event.position = zero_based as u64 + 1;
        p.apply(event).unwrap();
    }

    let g = p.subgraph(&["compoundcfg.rs".to_string()], 3).unwrap();

    for name in ["dual_cfg_mock_production_half", "debug_only_helper"] {
        let id = format!("compoundcfg.rs::{name}");
        assert!(
            g.nodes
                .iter()
                .any(|n| n.id == id && n.kind == KIND_CODE_ENTITY),
            "{name} (a compound cfg predicate naming test only as a non-controlling sub-clause) \
             must graph as a product code-entity node; nodes: {:?}",
            g.nodes.iter().map(|n| &n.id).collect::<Vec<_>>()
        );
    }
}

/// A round-3 regression fixture for `adv-u86c1-r2-trailing-comment-severs-the-attribute-stack-
/// scan`: a same-line trailing `//` comment on a `#[cfg(test)]` attribute must not stop the
/// module - or anything nested inside it - from being recognized as test-scoped and excluded
/// from the graph, exactly as the un-commented shape already is
/// (`test_annotated_definitions_and_everything_nested_inside_them_are_marked_is_test` in
/// `extract.rs`, and `ingesting_product_and_test_code_through_the_public_api_graphs_only_the_
/// product` above).
#[cfg(feature = "symbols")]
const TRAILING_COMMENT_CFG_TEST_SRC: &str = "\
fn product() {}

#[cfg(test)] // module gate, trailing comment
mod tests {
    fn helper() {}

    #[test]
    fn it_works() {}
}
";

#[cfg(feature = "symbols")]
#[test]
fn a_trailing_comment_on_cfg_test_still_excludes_the_module_through_the_public_api() {
    use rigger::contextgraph::sqlite::Projector;
    use rigger::contextgraph::{Projection, KIND_CODE_ENTITY};
    use std::collections::BTreeSet;

    let root = tempfile::tempdir().unwrap();
    std::fs::write(
        root.path().join("trailingcomment.rs"),
        TRAILING_COMMENT_CFG_TEST_SRC,
    )
    .unwrap();

    let idx = rigger::grounder::symbols::build_index(root.path().to_str().unwrap(), None);
    let mut events = rigger::grounder::symbols::events::index_events(&idx);
    let p = Projector::open(":memory:", "test").unwrap();
    for (zero_based, event) in events.iter_mut().enumerate() {
        event.position = zero_based as u64 + 1;
        p.apply(event).unwrap();
    }

    let g = p.subgraph(&["trailingcomment.rs".to_string()], 3).unwrap();
    let node_ids: BTreeSet<&str> = g.nodes.iter().map(|n| n.id.as_str()).collect();

    let product = g
        .nodes
        .iter()
        .find(|n| n.id == "trailingcomment.rs::product")
        .expect("product graphs normally, unaffected by the fix");
    assert_eq!(product.kind, KIND_CODE_ENTITY);

    let must_be_absent: BTreeSet<&str> = BTreeSet::from([
        "trailingcomment.rs::tests",
        "trailingcomment.rs::helper",
        "trailingcomment.rs::it_works",
    ]);
    let leaked: BTreeSet<&str> = node_ids.intersection(&must_be_absent).copied().collect();
    assert!(
        leaked.is_empty(),
        "a #[cfg(test)] module with a trailing same-line comment on its own attribute, and \
         everything nested inside it, must still be excluded from the graph; leaked: {leaked:?}"
    );
}

/// A round-3 sdet-author finding (`sdet-u86c1-r3-embedded-slash-attribute-plus-trailing-comment-
/// severs-scan`), independent of the round-3 fix's own two fixtures above: round 3's remedy for
/// `adv-u86c1-r2-trailing-comment-severs-the-attribute-stack-scan` truncates a line at its FIRST
/// `//`, assuming that is always the trailing comment's own marker. It is not, whenever an
/// attribute's OWN text legitimately contains `//` before a genuine trailing comment - the
/// ordinary `#[doc = "https://..."]` idiom is exactly this shape. `it_works` below has `#[test]`
/// directly in its attribute stack, but the closer `#[doc = "..."]` line's first `//` sits INSIDE
/// the URL, so the naive truncation cuts the line to `#[doc = "see https:` (no longer ending in
/// `]`), the shape check fails, and the scan hits the same `break` that severed
/// `adv-u86c1-r2-trailing-comment-severs-the-attribute-stack-scan` - stopping before it ever
/// reaches `#[test]` above. `it_works` leaks into the graph as an ordinary product code-entity
/// node: the same "test code IN" failure direction as the round-2 finding, one attribute-value
/// shape further than round 3's fix reaches.
#[cfg(feature = "symbols")]
const URL_BEARING_ATTRIBUTE_SRC: &str = "\
fn product() {}

#[test]
#[doc = \"see https://example.com for context\"] // kept for reference
fn it_works() {}
";

#[cfg(feature = "symbols")]
#[test]
fn a_url_bearing_attribute_between_test_and_the_item_does_not_leak_the_item_into_the_graph() {
    use rigger::contextgraph::sqlite::Projector;
    use rigger::contextgraph::{Projection, KIND_CODE_ENTITY};

    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("urlattr.rs"), URL_BEARING_ATTRIBUTE_SRC).unwrap();

    let idx = rigger::grounder::symbols::build_index(root.path().to_str().unwrap(), None);
    let mut events = rigger::grounder::symbols::events::index_events(&idx);
    let p = Projector::open(":memory:", "test").unwrap();
    for (zero_based, event) in events.iter_mut().enumerate() {
        event.position = zero_based as u64 + 1;
        p.apply(event).unwrap();
    }

    let g = p.subgraph(&["urlattr.rs".to_string()], 3).unwrap();

    let product = g
        .nodes
        .iter()
        .find(|n| n.id == "urlattr.rs::product")
        .expect("product graphs normally, unaffected by this scenario");
    assert_eq!(product.kind, KIND_CODE_ENTITY);

    let it_works_leaked = g
        .nodes
        .iter()
        .any(|n| n.id == "urlattr.rs::it_works" && n.kind == KIND_CODE_ENTITY);
    assert!(
        !it_works_leaked,
        "it_works has #[test] in its own attribute stack (one line further up than a \
         url-bearing #[doc] attribute carrying a genuine trailing comment); it must never graph \
         as a product code-entity node, but it did: nodes: {:?}",
        g.nodes.iter().map(|n| &n.id).collect::<Vec<_>>()
    );
}

/// Round 4 (`op-u86c1-r4-structural-attribute-walk-is-the-only-remedy`): the operator ruling
/// mandates a periphery case for a multi-line attribute on top of every rounds 1-3 shape above -
/// a case the LINE-based scan those rounds patched could never pass in general (it walked upward
/// one physical line at a time, so an attribute wrapped across several lines was never one
/// contiguous unit to it). Reading the grammar's own `attribute_item` node - whatever its own
/// text spans - makes this fall out for free rather than needing its own fix.
#[cfg(feature = "symbols")]
const MULTILINE_CFG_TEST_SRC: &str = "\
fn product() {}

#[cfg(
    test
)]
mod tests {
    fn helper() {}

    #[test]
    fn it_works() {}
}
";

#[cfg(feature = "symbols")]
#[test]
fn a_multiline_cfg_test_attribute_still_excludes_the_module_through_the_public_api() {
    use rigger::contextgraph::sqlite::Projector;
    use rigger::contextgraph::{Projection, KIND_CODE_ENTITY};
    use std::collections::BTreeSet;

    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("multiline.rs"), MULTILINE_CFG_TEST_SRC).unwrap();

    let idx = rigger::grounder::symbols::build_index(root.path().to_str().unwrap(), None);
    let mut events = rigger::grounder::symbols::events::index_events(&idx);
    let p = Projector::open(":memory:", "test").unwrap();
    for (zero_based, event) in events.iter_mut().enumerate() {
        event.position = zero_based as u64 + 1;
        p.apply(event).unwrap();
    }

    let g = p.subgraph(&["multiline.rs".to_string()], 3).unwrap();
    let node_ids: BTreeSet<&str> = g.nodes.iter().map(|n| n.id.as_str()).collect();

    let product = g
        .nodes
        .iter()
        .find(|n| n.id == "multiline.rs::product")
        .expect("product graphs normally, unaffected by a multi-line attribute elsewhere");
    assert_eq!(product.kind, KIND_CODE_ENTITY);

    let must_be_absent: BTreeSet<&str> = BTreeSet::from([
        "multiline.rs::tests",
        "multiline.rs::helper",
        "multiline.rs::it_works",
    ]);
    let leaked: BTreeSet<&str> = node_ids.intersection(&must_be_absent).copied().collect();
    assert!(
        leaked.is_empty(),
        "a #[cfg(\\n test\\n)] attribute wrapped across three physical lines must still gate the \
         module it decorates, and everything nested inside it, out of the graph; leaked: {leaked:?}"
    );
}

/// Round 4 (`op-u86c1-r4-structural-attribute-walk-is-the-only-remedy`): the operator ruling's
/// second mandated case - a COMMENT whose own text happens to contain the literal characters
/// `#[test]` must never be mistaken for a real attribute. A line-based text scan that looked for
/// `#[...]`-shaped lines above an item, stripping only a RECOGNIZED comment prefix, could in
/// principle be tempted to pattern-match inside a comment's text too; the structural walk cannot
/// make that mistake even in principle, because a `line_comment` node is inspected only for ITS
/// OWN kind (to skip over it) and its text is never parsed as an attribute - only a real
/// `attribute_item` node, produced by the grammar for actual `#[...]` syntax, is ever handed to
/// the attribute-name/predicate reader.
#[cfg(feature = "symbols")]
const COMMENT_MENTIONING_TEST_ATTRIBUTE_SRC: &str = "\
// #[test] this comment just talks about the #[test] attribute, it does not apply one
fn not_actually_a_test() {}
";

#[cfg(feature = "symbols")]
#[test]
fn a_comment_mentioning_test_attribute_text_does_not_exclude_the_item_through_the_public_api() {
    use rigger::contextgraph::sqlite::Projector;
    use rigger::contextgraph::{Projection, KIND_CODE_ENTITY};

    let root = tempfile::tempdir().unwrap();
    std::fs::write(
        root.path().join("commentmention.rs"),
        COMMENT_MENTIONING_TEST_ATTRIBUTE_SRC,
    )
    .unwrap();

    let idx = rigger::grounder::symbols::build_index(root.path().to_str().unwrap(), None);
    let mut events = rigger::grounder::symbols::events::index_events(&idx);
    let p = Projector::open(":memory:", "test").unwrap();
    for (zero_based, event) in events.iter_mut().enumerate() {
        event.position = zero_based as u64 + 1;
        p.apply(event).unwrap();
    }

    let g = p.subgraph(&["commentmention.rs".to_string()], 3).unwrap();

    let not_a_test = g
        .nodes
        .iter()
        .find(|n| n.id == "commentmention.rs::not_actually_a_test")
        .unwrap_or_else(|| {
            panic!(
                "a plain comment that merely mentions #[test] as text must never exclude the \
                 item beneath it; nodes: {:?}",
                g.nodes.iter().map(|n| &n.id).collect::<Vec<_>>()
            )
        });
    assert_eq!(not_a_test.kind, KIND_CODE_ENTITY);
}

/// A round-4 sdet-author case, independent of round 4's own two mandated fixtures above: round
/// 4's BRAND NEW `comma_separated_groups` (extract.rs) - which replaces rounds 1-3's text-based
/// `split_top_level_args` entirely - splits a `cfg` combinator's argument `token_tree` into its
/// top-level comma-separated groups by pushing `current` via `std::mem::take` at each `,` and
/// only pushing one FINAL group past the loop when `current` is non-empty. A TRAILING comma -
/// exactly the shape `rustfmt` itself produces whenever it wraps a comma-separated argument list,
/// attribute lists included, across multiple lines - is the one input this guard exists for: a
/// mis-handled trailing comma would leave a spurious EMPTY final group, and `any`'s own semantics
/// ("names test iff EVERY group does") would then fold the whole predicate to `false` on that
/// vacuous empty group (`group.first()` -> `None`) even though the sole real predicate is bare
/// `test` - silently UN-EXCLUDING a `#[test]`-bearing module the moment `rustfmt` wraps its own
/// `cfg` predicate. Combined here with round 4's own mandated multi-line shape (a single-element
/// `any(\n    test,\n)` is both wrapped across lines AND trailing-comma-terminated - the sharpest
/// discriminator: a spurious empty group flips this fixture's answer, where a two-element list
/// would not, since `any`'s own real element ("test") already answers independently of a
/// trailing empty one in a longer list). This guarantee was correct in the round-4 diff (verified
/// against the real parsed tree before authoring this test) but rested on no test anywhere in the
/// tree - not the implementer's own unit test, not either of round 4's own two mandated periphery
/// cases, both of which use single-line, no-trailing-comma predicates.
#[cfg(feature = "symbols")]
const TRAILING_COMMA_CFG_TEST_SRC: &str = "\
fn product() {}

#[cfg(any(
    test,
))]
mod tests {
    fn helper() {}

    #[test]
    fn it_works() {}
}
";

#[cfg(feature = "symbols")]
#[test]
fn a_trailing_comma_in_a_wrapped_cfg_predicate_still_excludes_the_module_through_the_public_api() {
    use rigger::contextgraph::sqlite::Projector;
    use rigger::contextgraph::{Projection, KIND_CODE_ENTITY};
    use std::collections::BTreeSet;

    let root = tempfile::tempdir().unwrap();
    std::fs::write(
        root.path().join("trailingcomma.rs"),
        TRAILING_COMMA_CFG_TEST_SRC,
    )
    .unwrap();

    let idx = rigger::grounder::symbols::build_index(root.path().to_str().unwrap(), None);
    let mut events = rigger::grounder::symbols::events::index_events(&idx);
    let p = Projector::open(":memory:", "test").unwrap();
    for (zero_based, event) in events.iter_mut().enumerate() {
        event.position = zero_based as u64 + 1;
        p.apply(event).unwrap();
    }

    let g = p.subgraph(&["trailingcomma.rs".to_string()], 3).unwrap();
    let node_ids: BTreeSet<&str> = g.nodes.iter().map(|n| n.id.as_str()).collect();

    let product = g
        .nodes
        .iter()
        .find(|n| n.id == "trailingcomma.rs::product")
        .expect("product graphs normally, unaffected by a trailing comma elsewhere in the file");
    assert_eq!(product.kind, KIND_CODE_ENTITY);

    let must_be_absent: BTreeSet<&str> = BTreeSet::from([
        "trailingcomma.rs::tests",
        "trailingcomma.rs::helper",
        "trailingcomma.rs::it_works",
    ]);
    let leaked: BTreeSet<&str> = node_ids.intersection(&must_be_absent).copied().collect();
    assert!(
        leaked.is_empty(),
        "a #[cfg(any(\\n    test,\\n))] attribute - a single-element predicate with a \
         rustfmt-style trailing comma - must still gate the module it decorates, and everything \
         nested inside it, out of the graph; a mis-handled trailing comma (a spurious empty final \
         group) would silently un-exclude this instead; leaked: {leaked:?}"
    );
}

/// The round-4 mandated regression (review REJECT `adj-u86c1-verdict-reject` round 4, finding
/// `adv-u86c1-r4-inner-cfg-test-attribute-not-recognized`), independently, through the SAME
/// public API: the INNER-attribute form `mod tests { #![cfg(test)] .. }` - the attribute is the
/// FIRST node inside the mod's own body rather than a sibling preceding the `mod` keyword - must
/// exclude the mod itself, an unattributed helper sitting alongside the attribute, AND a
/// `#[test]` function nested inside, exactly as the already-covered OUTER-attribute form does.
#[cfg(feature = "symbols")]
const INNER_CFG_TEST_ATTRIBUTE_SRC: &str = "\
fn product() {}

mod tests {
    #![cfg(test)]

    fn helper() {
        product();
    }

    #[test]
    fn it_works() {
        helper();
    }
}
";

#[cfg(feature = "symbols")]
#[test]
fn an_inner_cfg_test_attribute_excludes_its_module_through_the_public_api() {
    use rigger::contextgraph::sqlite::Projector;
    use rigger::contextgraph::{Projection, KIND_CODE_ENTITY};
    use std::collections::BTreeSet;

    let root = tempfile::tempdir().unwrap();
    std::fs::write(
        root.path().join("innerattr.rs"),
        INNER_CFG_TEST_ATTRIBUTE_SRC,
    )
    .unwrap();

    let idx = rigger::grounder::symbols::build_index(root.path().to_str().unwrap(), None);
    let mut events = rigger::grounder::symbols::events::index_events(&idx);
    let p = Projector::open(":memory:", "test").unwrap();
    for (zero_based, event) in events.iter_mut().enumerate() {
        event.position = zero_based as u64 + 1;
        p.apply(event).unwrap();
    }

    let g = p.subgraph(&["innerattr.rs".to_string()], 3).unwrap();
    let node_ids: BTreeSet<&str> = g.nodes.iter().map(|n| n.id.as_str()).collect();

    let product = g
        .nodes
        .iter()
        .find(|n| n.id == "innerattr.rs::product")
        .expect("product code stays graphed, unaffected by the inner-attribute mod elsewhere");
    assert_eq!(product.kind, KIND_CODE_ENTITY);

    let must_be_absent: BTreeSet<&str> = BTreeSet::from([
        "innerattr.rs::tests",
        "innerattr.rs::helper",
        "innerattr.rs::it_works",
    ]);
    let leaked: BTreeSet<&str> = node_ids.intersection(&must_be_absent).copied().collect();
    assert!(
        leaked.is_empty(),
        "a mod gated by the INNER #![cfg(test)] attribute form (the attribute as the first node \
         inside the mod's own body, not a sibling preceding it) must exclude the mod itself, an \
         unattributed helper beside the attribute, and a nested #[test] function, exactly as the \
         outer #[cfg(test)] mod form already does; leaked: {leaked:?}"
    );
}

/// Round-5 mandated surface (`op-u86c1-r5-close-every-remaining-test-shape` item 2, amending
/// `op-u86c1-r4-structural-attribute-walk-is-the-only-remedy`): an OUT-OF-LINE test module - a
/// parent file's `#[cfg(test)] mod name;` (no body of its own; the declaration merely names a
/// SEPARATE file) - must exclude the declared file IN FULL, the same way a whole file directly
/// under a `tests/` directory already is. The declared file itself carries no attribute at all
/// (Rust's module system, not this repo's convention, is what gates its compilation), so a
/// per-file structural walk over that file's OWN parsed tree ([`preceded_by_test_attribute`] in
/// `extract.rs`) can never see the attribute that excludes it - it lives in a DIFFERENT file. This
/// is LIVE in rigger's own tree today, not a hypothetical: `src/eventstore/mod.rs` declares
/// `#[cfg(test)] pub mod contract;` (`src/eventstore/contract.rs`) and `src/lib.rs` declares
/// `#[cfg(test)] mod blast_radius_eval;` (`src/blast_radius_eval.rs`), and this fixture is the
/// minimal shape of both: a plain, unattributed `pub fn` in the declared file, exactly like
/// `contract.rs`'s own `pub fn assert_contract`.
///
/// SURFACE CONFIRMED STILL OPEN (`sdet-u86c1-r5-two-confirmed-live-gaps`): probed empirically
/// through this same public API before writing this test - `child.rs::helper` emitted a genuine
/// `CodeEntityExtracted` event and reached the graph as ordinary product code, exactly the failure
/// this test pins. The round-5 fix (`fix-u86c1-r5-inner-attribute-item`) closed item 1 of the
/// mandate (the inner-attribute module form) only; this test is expected to FAIL until a future
/// round resolves each test-shaped out-of-line module declaration to its file(s) at the
/// events/index layer (where every file's path is known - `events.rs`'s `is_under_tests_dir` /
/// `project_batches`, per the mandate) and excludes them there, since a per-file extractor
/// structurally cannot see an attribute that lives in a different file.
#[cfg(feature = "symbols")]
const PARENT_SRC: &str = "\
fn product() {}

#[cfg(test)]
pub mod contract;
";

#[cfg(feature = "symbols")]
const OUT_OF_LINE_CHILD_SRC: &str = "pub fn assert_contract() {}\n";

#[cfg(feature = "symbols")]
#[test]
fn an_out_of_line_cfg_test_module_declaration_excludes_its_declared_file_through_the_public_api() {
    use rigger::contextgraph::sqlite::Projector;
    use rigger::contextgraph::Projection;
    use std::collections::BTreeSet;

    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("parent.rs"), PARENT_SRC).unwrap();
    std::fs::write(root.path().join("contract.rs"), OUT_OF_LINE_CHILD_SRC).unwrap();

    let idx = rigger::grounder::symbols::build_index(root.path().to_str().unwrap(), None);
    let mut events = rigger::grounder::symbols::events::index_events(&idx);
    let p = Projector::open(":memory:", "test").unwrap();
    for (zero_based, event) in events.iter_mut().enumerate() {
        event.position = zero_based as u64 + 1;
        p.apply(event).unwrap();
    }

    let g = p
        .subgraph(&["parent.rs".to_string(), "contract.rs".to_string()], 3)
        .unwrap();
    let node_ids: BTreeSet<&str> = g.nodes.iter().map(|n| n.id.as_str()).collect();

    assert!(
        node_ids.contains("parent.rs::product"),
        "the parent file's own product code stays graphed, unaffected by what its out-of-line \
         mod declaration points at; nodes: {node_ids:?}"
    );

    let must_be_absent: BTreeSet<&str> =
        BTreeSet::from(["contract.rs::assert_contract", "contract.rs"]);
    let leaked: BTreeSet<&str> = node_ids.intersection(&must_be_absent).copied().collect();
    assert!(
        leaked.is_empty(),
        "a file named only by an out-of-line `#[cfg(test)] mod name;` declaration in another \
         file must be excluded in FULL - no code-entity node for anything it defines and no file \
         container node for the file itself - exactly as a file directly under a tests/ \
         directory already is; a plain `pub fn` with no attribute of its own here has no way to \
         self-exclude, since the attribute gating it lives in the OTHER file's tree entirely; \
         leaked: {leaked:?}"
    );
}

/// sdet-author gap: the test above and `a_non_test_out_of_line_mod_and_an_inline_test_mod_never_
/// exclude_a_coincidentally_named_sibling_file` below both place the declaring file and its
/// out-of-line target at the tempdir ROOT (`dir` empty), so `out_of_line_test_module_files`'s
/// `dir.is_empty()` branch is the only one any test in this round exercises. This repo's OWN two
/// round-5-disclosed live instances - `src/eventstore/mod.rs` (`#[cfg(test)] pub mod contract;`)
/// resolving to `src/eventstore/contract.rs`, and `src/lib.rs` (`#[cfg(test)] mod
/// blast_radius_eval;`) resolving to `src/blast_radius_eval.rs` - are BOTH the OTHER branch: a
/// declaring file that itself lives in a subdirectory, so the resolved sibling path carries that
/// same directory prefix (`format!("{dir}/{}.rs", d.name)`). Mirrors that exact shape so the
/// guarantee criterion 1 makes about this very tree is proven by a fixture, not merely inferred
/// from the flat-root case generalizing.
#[cfg(feature = "symbols")]
#[test]
fn an_out_of_line_cfg_test_module_declaration_in_a_subdirectory_excludes_its_sibling_through_the_public_api(
) {
    use rigger::contextgraph::sqlite::Projector;
    use rigger::contextgraph::Projection;
    use std::collections::BTreeSet;

    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir(root.path().join("nested")).unwrap();
    std::fs::write(
        root.path().join("nested").join("parent.rs"),
        "fn nested_product() {}\n\n#[cfg(test)]\npub mod child;\n",
    )
    .unwrap();
    std::fs::write(
        root.path().join("nested").join("child.rs"),
        "pub fn nested_assert() {}\n",
    )
    .unwrap();

    let idx = rigger::grounder::symbols::build_index(root.path().to_str().unwrap(), None);
    let mut events = rigger::grounder::symbols::events::index_events(&idx);
    let p = Projector::open(":memory:", "test").unwrap();
    for (zero_based, event) in events.iter_mut().enumerate() {
        event.position = zero_based as u64 + 1;
        p.apply(event).unwrap();
    }

    let g = p
        .subgraph(
            &[
                "nested/parent.rs".to_string(),
                "nested/child.rs".to_string(),
            ],
            3,
        )
        .unwrap();
    let node_ids: BTreeSet<&str> = g.nodes.iter().map(|n| n.id.as_str()).collect();

    assert!(
        node_ids.contains("nested/parent.rs::nested_product"),
        "the declaring file's own product code stays graphed; nodes: {node_ids:?}"
    );
    let must_be_absent: BTreeSet<&str> =
        BTreeSet::from(["nested/child.rs::nested_assert", "nested/child.rs"]);
    let leaked: BTreeSet<&str> = node_ids.intersection(&must_be_absent).copied().collect();
    assert!(
        leaked.is_empty(),
        "a subdirectory sibling named only by an out-of-line `#[cfg(test)] mod name;` \
         declaration must be excluded in FULL exactly like the flat-root-level case, proving the \
         `dir` prefix (never just the bare `<name>.rs` the root-level fixtures alone would leave \
         untested) is threaded correctly into the resolved path; leaked: {leaked:?}"
    );
}

/// sdet-author gap: every out-of-line-resolution fixture in this file names a `mod` that DOES
/// resolve to a real file `idx` holds. `out_of_line_test_module_files`'s own doc comment claims
/// the resolution is matched against paths the index ACTUALLY holds, "never assumed" - a
/// `#[cfg(test)] mod name;` declaration whose named file genuinely does not exist (a stale
/// declaration, or a form this resolver does not yet reach - `#[path = ".."]`,
/// `dec-u86c1-r6-path-and-nested-mod-not-yet-covered`) must not panic the whole ingest and must
/// not exclude some unrelated file by accident; the declaring file's own product code must still
/// graph normally.
#[cfg(feature = "symbols")]
#[test]
fn an_out_of_line_test_mod_declaration_naming_no_real_file_neither_panics_nor_excludes_anything_else(
) {
    use rigger::contextgraph::sqlite::Projector;
    use rigger::contextgraph::{Projection, KIND_CODE_ENTITY};

    let root = tempfile::tempdir().unwrap();
    std::fs::write(
        root.path().join("dangling_parent.rs"),
        "fn still_graphs() {}\n\n#[cfg(test)]\nmod nonexistent_child;\n",
    )
    .unwrap();
    // An unrelated file that must be entirely unaffected by the dangling declaration above.
    std::fs::write(
        root.path().join("unrelated.rs"),
        "pub fn unrelated_fn() {}\n",
    )
    .unwrap();

    let idx = rigger::grounder::symbols::build_index(root.path().to_str().unwrap(), None);
    let mut events = rigger::grounder::symbols::events::index_events(&idx);
    let p = Projector::open(":memory:", "test").unwrap();
    for (zero_based, event) in events.iter_mut().enumerate() {
        event.position = zero_based as u64 + 1;
        p.apply(event).unwrap();
    }

    let g = p
        .subgraph(
            &["dangling_parent.rs".to_string(), "unrelated.rs".to_string()],
            3,
        )
        .unwrap();
    let product = g
        .nodes
        .iter()
        .find(|n| n.id == "dangling_parent.rs::still_graphs")
        .expect(
            "a #[cfg(test)] mod declaration naming no real file must not prevent the declaring \
             file's own product code from graphing",
        );
    assert_eq!(product.kind, KIND_CODE_ENTITY);
    let unrelated = g
        .nodes
        .iter()
        .find(|n| n.id == "unrelated.rs::unrelated_fn")
        .expect(
            "an unrelated file must never be swept into exclusion by a dangling out-of-line \
             declaration elsewhere; nodes: {:?}",
        );
    assert_eq!(unrelated.kind, KIND_CODE_ENTITY);
}

/// sdet-author gap: every out-of-line-resolution test above drives `index_events`. The mandate
/// (`op-u86c1-r5-close-every-remaining-test-shape` item 2, quoted in this file's own module doc)
/// names TWO production entry points needing the fix - `events.rs`'s `is_under_tests_dir` /
/// `project_batches` - and `project_batches` is the ACTUAL entry point a live run drives
/// (`conductor::RunCtx::ingest_project_batches`, spec 29c), not merely a test convenience;
/// `out_of_line_test_module_files` is computed and applied at BOTH call sites independently (two
/// inline `.filter(|(path, _)| !excluded.contains(...))` sites over the SAME shared helper, per
/// `events.rs`'s own source), so nothing so far proves `project_batches`'s OWN copy of that wiring
/// is correct rather than merely the `index_events` one exercised everywhere else in this file.
#[cfg(feature = "symbols")]
#[test]
fn project_batches_also_excludes_an_out_of_line_test_module_declarations_target_file() {
    use rigger::contextgraph::sqlite::Projector;
    use rigger::contextgraph::Projection;
    use rigger::grounder::symbols::events::project_batches;
    use std::collections::BTreeSet;

    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("parent.rs"), PARENT_SRC).unwrap();
    std::fs::write(root.path().join("contract.rs"), OUT_OF_LINE_CHILD_SRC).unwrap();

    let batches = project_batches(root.path().to_str().unwrap());
    let files: BTreeSet<&str> = batches.iter().map(|(f, _)| f.as_str()).collect();
    assert!(
        files.contains("parent.rs"),
        "the declaring file still contributes its own batch; files: {files:?}"
    );
    assert!(
        !files.contains("contract.rs"),
        "project_batches (the entry point a live run actually drives, spec 29c) must exclude \
         the out-of-line test module's declared file's batch entirely too, not merely \
         index_events's own copy of the same filter; files: {files:?}"
    );

    let p = Projector::open(":memory:", "test").unwrap();
    let mut pos = 0u64;
    for (_, events) in &batches {
        for e in events {
            pos += 1;
            let mut ev = e.clone();
            ev.position = pos;
            p.apply(&ev).unwrap();
        }
    }
    let g = p
        .subgraph(&["parent.rs".to_string(), "contract.rs".to_string()], 3)
        .unwrap();
    assert!(
        !g.nodes
            .iter()
            .any(|n| n.id == "contract.rs::assert_contract"),
        "the excluded file's own definition must never reach the graph through this entry \
         point either; nodes: {:?}",
        g.nodes
    );
}

/// Mutation-efficacy pin (round 6 `cargo mutants` finding, events.rs `out_of_line_test_module_files`'s
/// `d.kind != Kind::Module || !d.is_out_of_line_module || !d.is_test` skip guard): BOTH the
/// `is_out_of_line_module` and `is_test` conjuncts are required before a Module-kind definition is
/// even a CANDIDATE for cross-file exclusion - dropping either gate (mutating either `||` to `&&`)
/// only misbehaves observably when a coincidentally-named SIBLING FILE exists to wrongly sweep in,
/// which every earlier fixture in this file never sets up. Two such collisions, in one project:
///
/// - `host_a.rs` declares a PLAIN out-of-line `pub mod sibling;` (no `#[cfg(test)]` at all -
///   `is_out_of_line_module: true`, `is_test: false`) and a REAL `sibling.rs` file happens to
///   exist; the `is_test` gate must keep `sibling.rs` OUT of exclusion (it is ordinary product
///   code merely declared from another file).
/// - `host_b.rs` declares an INLINE `#[cfg(test)] mod contract { .. }` (has its own body -
///   `is_out_of_line_module: false`, `is_test: true`) and a REAL, UNRELATED `contract.rs` file
///   happens to share that name; the `is_out_of_line_module` gate must keep `contract.rs` OUT of
///   exclusion (the inline module governs only its OWN contents by containment, never a
///   same-named sibling file it never declared).
#[cfg(feature = "symbols")]
#[test]
fn a_non_test_out_of_line_mod_and_an_inline_test_mod_never_exclude_a_coincidentally_named_sibling_file(
) {
    use rigger::contextgraph::sqlite::Projector;
    use rigger::contextgraph::Projection;

    let root = tempfile::tempdir().unwrap();
    std::fs::write(
        root.path().join("host_a.rs"),
        "fn product_a() {}\n\npub mod sibling;\n",
    )
    .unwrap();
    std::fs::write(root.path().join("sibling.rs"), "pub fn sibling_fn() {}\n").unwrap();
    std::fs::write(
        root.path().join("host_b.rs"),
        "fn product_b() {}\n\n#[cfg(test)]\nmod contract {\n    #[test]\n    fn it_works() {}\n}\n",
    )
    .unwrap();
    std::fs::write(root.path().join("contract.rs"), "pub fn contract_fn() {}\n").unwrap();

    let idx = rigger::grounder::symbols::build_index(root.path().to_str().unwrap(), None);
    let mut events = rigger::grounder::symbols::events::index_events(&idx);
    let p = Projector::open(":memory:", "test").unwrap();
    for (zero_based, event) in events.iter_mut().enumerate() {
        event.position = zero_based as u64 + 1;
        p.apply(event).unwrap();
    }

    let g = p
        .subgraph(
            &[
                "host_a.rs".to_string(),
                "sibling.rs".to_string(),
                "host_b.rs".to_string(),
                "contract.rs".to_string(),
            ],
            3,
        )
        .unwrap();
    let node_ids: std::collections::BTreeSet<&str> =
        g.nodes.iter().map(|n| n.id.as_str()).collect();

    assert!(
        node_ids.contains("sibling.rs::sibling_fn"),
        "a REAL sibling.rs must stay graphed when the only thing naming it is a NON-test \
         out-of-line `pub mod sibling;` - the is_test gate must block exclusion here; nodes: \
         {node_ids:?}"
    );
    assert!(
        node_ids.contains("contract.rs::contract_fn"),
        "a REAL contract.rs must stay graphed when the only coincidence is an INLINE \
         `#[cfg(test)] mod contract {{ .. }}` elsewhere sharing its name - the \
         is_out_of_line_module gate must block exclusion here; nodes: {node_ids:?}"
    );
}

/// Round-5 mandated surface (`op-u86c1-r5-close-every-remaining-test-shape` item 3): "any item
/// kind" names `impl_item` explicitly among the node kinds an excluding attribute may sit on.
/// Rust's own `tags.scm` (`tree-sitter-rust` 0.24.2) captures an `impl_item` ONLY as
/// `@reference.implementation` - never as a `@definition.*` - so it never enters `def_ranges` and
/// [`test_regions`] (which only ever considers SELF-ATTRIBUTED items from `def_ranges`) can never
/// treat a `#[cfg(test)] impl Widget { .. }` block itself as a test region the way it already does
/// for a `#[cfg(test)] mod tests { .. }` (a `mod_item` IS a `@definition.module`). A method nested
/// inside such an impl - a plain `fn helper()` with no attribute of its own, or even a `#[test] fn
/// it_works()` calling it - is a real `@definition.method`/`function_item` and so IS covered by
/// [`preceded_by_test_attribute`]'s generic, kind-agnostic sibling walk when the attribute sits
/// directly on the method itself; but nothing propagates the ENCLOSING impl's own `#[cfg(test)]`
/// onto a plain sibling method that carries no attribute of its own.
///
/// SURFACE CONFIRMED STILL OPEN (`sdet-u86c1-r5-two-confirmed-live-gaps`): probed empirically
/// through `extract()` directly before writing this test - for this exact fixture shape, `helper`
/// (no attribute of its own) came back `is_test=false` while `it_works` (its own direct `#[test]`)
/// came back `is_test=true`, confirming the leak is specifically the impl-level attribute failing
/// to propagate onto its unattributed sibling, not a defect in the sibling walk itself. This test
/// is expected to FAIL until a future round extends test-region detection to also treat a
/// `#[cfg(test)]`/`#[cfg(any(..test..))]`-attributed `impl_item` as a container whose ENTIRE body
/// is a test region, mirroring what already happens for `mod_item`.
#[cfg(feature = "symbols")]
const CFG_TEST_IMPL_SRC: &str = "\
struct Widget;

#[cfg(test)]
impl Widget {
    fn helper() {
        product();
    }

    #[test]
    fn it_works() {
        helper();
    }
}

fn product() {}

struct Ordinary;

impl Ordinary {
    fn ordinary_method() {}
}
";

#[cfg(feature = "symbols")]
#[test]
fn a_cfg_test_impl_block_excludes_its_methods_through_the_public_api() {
    use rigger::contextgraph::sqlite::Projector;
    use rigger::contextgraph::{Projection, KIND_CODE_ENTITY};
    use std::collections::BTreeSet;

    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("widgetimpl.rs"), CFG_TEST_IMPL_SRC).unwrap();

    let idx = rigger::grounder::symbols::build_index(root.path().to_str().unwrap(), None);
    let mut events = rigger::grounder::symbols::events::index_events(&idx);
    let p = Projector::open(":memory:", "test").unwrap();
    for (zero_based, event) in events.iter_mut().enumerate() {
        event.position = zero_based as u64 + 1;
        p.apply(event).unwrap();
    }

    let g = p.subgraph(&["widgetimpl.rs".to_string()], 3).unwrap();
    let node_ids: BTreeSet<&str> = g.nodes.iter().map(|n| n.id.as_str()).collect();

    let product = g
        .nodes
        .iter()
        .find(|n| n.id == "widgetimpl.rs::product")
        .expect("product code stays graphed, unaffected by the cfg(test) impl block elsewhere");
    assert_eq!(product.kind, KIND_CODE_ENTITY);

    let must_be_absent: BTreeSet<&str> =
        BTreeSet::from(["widgetimpl.rs::helper", "widgetimpl.rs::it_works"]);
    let leaked: BTreeSet<&str> = node_ids.intersection(&must_be_absent).copied().collect();
    assert!(
        leaked.is_empty(),
        "every method inside a #[cfg(test)]-attributed impl block is test code, whether or not \
         it carries an attribute of its own - exactly as a plain helper inside a #[cfg(test)] mod \
         already is by containment - since an impl_item is never itself a tags-query definition, \
         nothing currently treats the attributed impl block as a test-region container the way a \
         mod_item already is; leaked: {leaked:?}"
    );

    // Mutation-efficacy pin (round 6 `cargo mutants` finding, extract.rs `collect_self_attributed_
    // impl_regions`): an ORDINARY (non-`#[cfg(test)]`) impl block's method must NOT be swept up as
    // test code merely because SOME OTHER impl block in the file is self-attributed test - the
    // check is `node.kind() == "impl_item" && node_preceded_by_test_attribute(..)`, both conjuncts
    // required. Mutating that `&&` to `||` would treat EVERY `impl_item` in the file as a test
    // region regardless of its own attribution (the first disjunct alone is enough), which every
    // assertion above is blind to (`product`/`helper`/`it_works` are never themselves inside an
    // ordinary impl block) - only a method inside a genuinely ordinary impl block catches it.
    assert!(
        g.nodes
            .iter()
            .any(|n| n.id == "widgetimpl.rs::ordinary_method"),
        "a method inside an ORDINARY (non-test) impl block must stay graphed even though the same \
         file also has a #[cfg(test)]-attributed impl block elsewhere; got {:?}",
        g.nodes
    );
}

/// sdet-author gap: `a_cfg_test_impl_block_excludes_its_methods_through_the_public_api` above only
/// exercises the OUTER attribute form (`#[cfg(test)] impl Widget { .. }`, a sibling BEFORE the
/// `impl_item`). `collect_self_attributed_impl_regions` calls the SAME shared
/// `node_preceded_by_test_attribute` the round-5 inner-module fix already made check
/// `leading_inner_test_attribute` first - so an `impl` block gated by the INNER form
/// (`impl Widget { #![cfg(test)] .. }`, the attribute as the body's own first child rather than a
/// sibling before the `impl` keyword) should already be caught by construction, but no test
/// anywhere - implementer's own `extract.rs` unit tests or this file - poses that exact
/// node-kind/attribute-direction COMBINATION. Proving it independently rather than inferring it
/// from the two mechanisms each working in isolation.
#[cfg(feature = "symbols")]
const INNER_ATTR_IMPL_SRC: &str = "\
struct Widget;

impl Widget {
    #![cfg(test)]

    fn helper() {
        product();
    }

    #[test]
    fn it_works() {
        helper();
    }
}

fn product() {}
";

#[cfg(feature = "symbols")]
#[test]
fn a_cfg_test_impl_block_using_the_inner_attribute_form_excludes_its_methods_through_the_public_api(
) {
    use rigger::contextgraph::sqlite::Projector;
    use rigger::contextgraph::{Projection, KIND_CODE_ENTITY};
    use std::collections::BTreeSet;

    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("innerimpl.rs"), INNER_ATTR_IMPL_SRC).unwrap();

    let idx = rigger::grounder::symbols::build_index(root.path().to_str().unwrap(), None);
    let mut events = rigger::grounder::symbols::events::index_events(&idx);
    let p = Projector::open(":memory:", "test").unwrap();
    for (zero_based, event) in events.iter_mut().enumerate() {
        event.position = zero_based as u64 + 1;
        p.apply(event).unwrap();
    }

    let g = p.subgraph(&["innerimpl.rs".to_string()], 3).unwrap();
    let node_ids: BTreeSet<&str> = g.nodes.iter().map(|n| n.id.as_str()).collect();

    let product = g
        .nodes
        .iter()
        .find(|n| n.id == "innerimpl.rs::product")
        .expect("product code stays graphed, unaffected by the inner-attribute impl elsewhere");
    assert_eq!(product.kind, KIND_CODE_ENTITY);

    let must_be_absent: BTreeSet<&str> =
        BTreeSet::from(["innerimpl.rs::helper", "innerimpl.rs::it_works"]);
    let leaked: BTreeSet<&str> = node_ids.intersection(&must_be_absent).copied().collect();
    assert!(
        leaked.is_empty(),
        "an impl block gated by its OWN inner #![cfg(test)] attribute must exclude its methods \
         exactly like the outer-attribute form already does; leaked: {leaked:?}"
    );
}

/// Round-5 mandated surface (`op-u86c1-r5-close-every-remaining-test-shape` item 3), the rest of
/// "any item kind" beyond the `mod_item`/`function_item` shapes every earlier round already
/// exercised: [`preceded_by_test_attribute`]'s sibling walk is kind-agnostic by construction (it
/// dispatches on the ATTRIBUTE node's kind, never the attributed item's), so a directly-attributed
/// `struct_item`, `enum_item`, `trait_item`, `type_item` (a type alias) and `macro_definition`
/// should already self-exclude exactly like a `#[cfg(test)] mod`/`fn` does - this proves that
/// generalization holds empirically, through the public API, rather than resting on reading the
/// walk's kind-agnostic shape as sufficient by inspection. Also covers the three item kinds the
/// mandate names that `tags.scm` never tags as a definition or reference AT ALL
/// (`use_declaration`, `const_item`, `static_item`): each is attributed `#[cfg(test)]` here
/// immediately before the plain product `fn` this test pins, proving such an attribute stays
/// scoped to the leaf item it sits on and never leaks onto - or wrongly excludes - an unrelated
/// sibling that carries no attribute of its own.
#[cfg(feature = "symbols")]
const OTHER_ITEM_KINDS_SRC: &str = "\
#[cfg(test)]
struct TestStruct;

#[cfg(test)]
enum TestEnum {
    A,
}

#[cfg(test)]
trait TestTrait {}

#[cfg(test)]
type TestAlias = i32;

#[cfg(test)]
macro_rules! test_macro {
    () => {};
}

#[cfg(test)]
use std::string::String as ImportedForTestOnly;

#[cfg(test)]
const TEST_CONST: i32 = 1;

#[cfg(test)]
static TEST_STATIC: i32 = 1;

fn product() {}
";

#[cfg(feature = "symbols")]
#[test]
fn cfg_test_on_every_other_item_kind_excludes_or_stays_scoped_through_the_public_api() {
    use rigger::contextgraph::sqlite::Projector;
    use rigger::contextgraph::{Projection, KIND_CODE_ENTITY};
    use std::collections::BTreeSet;

    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("kinds.rs"), OTHER_ITEM_KINDS_SRC).unwrap();

    let idx = rigger::grounder::symbols::build_index(root.path().to_str().unwrap(), None);
    let mut events = rigger::grounder::symbols::events::index_events(&idx);
    let p = Projector::open(":memory:", "test").unwrap();
    for (zero_based, event) in events.iter_mut().enumerate() {
        event.position = zero_based as u64 + 1;
        p.apply(event).unwrap();
    }

    let g = p.subgraph(&["kinds.rs".to_string()], 3).unwrap();
    let node_ids: BTreeSet<&str> = g.nodes.iter().map(|n| n.id.as_str()).collect();

    // The plain product fn - the ONLY item here with no #[cfg(test)] of its own - must stay
    // graphed; none of `use`/`const`/`static`'s attributes (kinds tags.scm never tags as a
    // definition at all) may leak onto it.
    let product = g.nodes.iter().find(|n| n.id == "kinds.rs::product").expect(
        "the plain product fn stays graphed - a #[cfg(test)] on a preceding use/const/static \
             (none of which are definitions at all) must never leak onto an unrelated sibling",
    );
    assert_eq!(product.kind, KIND_CODE_ENTITY);

    // Every directly-#[cfg(test)]-attributed definition, whatever its grammar kind, self-excludes
    // exactly like the already-covered fn/mod shapes.
    let must_be_absent: BTreeSet<&str> = BTreeSet::from([
        "kinds.rs::TestStruct",
        "kinds.rs::TestEnum",
        "kinds.rs::TestTrait",
        "kinds.rs::TestAlias",
        "kinds.rs::test_macro",
    ]);
    let leaked: BTreeSet<&str> = node_ids.intersection(&must_be_absent).copied().collect();
    assert!(
        leaked.is_empty(),
        "a directly #[cfg(test)]-attributed struct/enum/trait/type-alias/macro_rules! definition \
         must self-exclude exactly like an attributed fn or mod already does - the sibling walk \
         dispatches on the ATTRIBUTE node's kind, never the attributed item's, so this must hold \
         for every definition-producing grammar kind uniformly; leaked: {leaked:?}"
    );
}
