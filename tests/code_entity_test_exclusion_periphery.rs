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
