//! Periphery (contract / integration) tests for spec 37 criterion 1: the extractor attributes
//! each reference to its INNERMOST enclosing definition (the caller), recorded on the new
//! `SymRef.enclosing` field.
//!
//! These run OUTSIDE the crate, over the library's public surface, and guard the boundary the
//! inside-out unit tests are structurally blind to. The inside-out `extract` test asserts only the
//! in-memory attribution and never serializes; the store's `save_load_roundtrips` uses a
//! reference-LESS sample, so it never exercises `SymRef`'s serialized form at all. Two boundaries
//! are therefore untested at the base tree:
//!
//!  - the SERIALIZED-FORM back-compat contract of the new field. `SymRef` derives
//!    Serialize/Deserialize and the persisted symbol index is a projection of it onto disk
//!    (`store::save` / `store::load` write `.rigger/symbols/index.json`). The field carries
//!    `#[serde(default, skip_serializing_if = "Option::is_none")]`, so (a) a caller-less reference
//!    must serialize BYTE-IDENTICALLY to the pre-37 form - no `enclosing` key at all - and (b) an
//!    index persisted by a pre-37 binary (references with no `enclosing` key) must still LOAD,
//!    folding every reference caller-less rather than failing on a missing field. This drives the
//!    real persistence boundary and pins the raw on-disk JSON a rebuild actually replays. Parser-
//!    free, so it runs in BOTH feature lanes.
//!  - the extractor's attribution SURVIVING that persistence boundary end-to-end: a caller
//!    attributed in-memory while building the index must still be `Some(caller)` after a save/load
//!    round-trip, and a top-level reference must stay `None`. This half drives the public
//!    `build_index` entry point through the real tree-sitter extraction pass, so it lives in the
//!    `symbols` lane only.

use rigger::grounder::symbols::model::{Def, FileSymbols, Kind, Lang, SymRef, SymbolIndex};
use rigger::grounder::symbols::store;

// ---- serialized-form / back-compat contract (parser-free model + store: BOTH feature lanes) ----

#[test]
fn a_caller_less_reference_serializes_byte_identically_to_the_pre37_form() {
    // `skip_serializing_if = "Option::is_none"`: a reference with no enclosing caller must serialize
    // with NO `enclosing` key, so an index of caller-less references is byte-identical to what a
    // pre-37 binary wrote. A regression dropping `skip_serializing_if` would emit `"enclosing":
    // null` on every reference and rewrite every historical index's bytes - defeating the field's
    // whole "wholly additive, no re-serialization churn" contract (spec 37).
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
            }],
            refs: vec![
                SymRef {
                    name: "G".into(),
                    line: 2,
                    enclosing: None,
                },
                SymRef {
                    name: "H".into(),
                    line: 3,
                    enclosing: None,
                },
            ],
        },
    );
    store::save(&idx, root).unwrap();
    let bytes = std::fs::read_to_string(store::index_path(root)).unwrap();
    assert!(
        !bytes.contains("enclosing"),
        "a caller-less index must omit the enclosing key entirely (byte-identical to the pre-37 \
         on-disk form); got:\n{bytes}"
    );
}

#[test]
fn a_caller_attributed_reference_serializes_and_reloads_its_enclosing_name() {
    // The opposite direction of the same contract: a reference WITH a caller serializes the
    // enclosing name and round-trips through the real persistence boundary preserving it. Together
    // with the caller-less test above this pins BOTH serde attributes on the field: drop
    // `skip_serializing_if` and the caller-less test reds; drop the field's persistence and this
    // one reds.
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
            }],
            refs: vec![SymRef {
                name: "G".into(),
                line: 2,
                enclosing: Some("f".into()),
            }],
        },
    );
    store::save(&idx, root).unwrap();
    let bytes = std::fs::read_to_string(store::index_path(root)).unwrap();
    assert!(
        bytes.contains("enclosing"),
        "a caller-attributed reference must serialize its enclosing key; got:\n{bytes}"
    );
    let loaded = store::load(root).expect("the persisted index loads");
    let refs = &loaded.files()["a.rs"].refs;
    assert_eq!(
        refs.iter()
            .find(|r| r.name == "G")
            .unwrap()
            .enclosing
            .as_deref(),
        Some("f"),
        "the enclosing caller survives a save/load round-trip"
    );
}

#[test]
fn a_pre37_persisted_index_loads_folding_references_caller_less() {
    // `#[serde(default)]`: an `index.json` written by a pre-37 binary has references with NO
    // `enclosing` key at all (the shape verified against the live serializer: a caller-less ref is
    // exactly `{ "name", "line" }`). `store::load` must still deserialize it - NOT return `None`
    // from a missing-field error - folding every reference caller-less. This pins the raw on-disk
    // JSON contract a rebuild replays, deliberately as bytes rather than through the Rust type.
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_str().unwrap();
    let path = store::index_path(root);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    // A pre-37 index: two references carrying only `name` + `line`, no `enclosing` key.
    let legacy = r#"{
  "files": {
    "legacy.rs": {
      "lang": "Rust",
      "defs": [
        { "kind": "Function", "name": "caller_fn", "line": 1 }
      ],
      "refs": [
        { "name": "G", "line": 2 },
        { "name": "H", "line": 5 }
      ]
    }
  }
}"#;
    std::fs::write(&path, legacy).unwrap();

    let loaded = store::load(root)
        .expect("a pre-37 index (references without an enclosing key) must still load, not error");
    let file = loaded
        .files()
        .get("legacy.rs")
        .expect("the legacy file entry is present");
    // Every reference folds caller-less - the missing key defaults to `None`.
    assert_eq!(
        file.refs.len(),
        2,
        "both legacy references survive the load"
    );
    for r in &file.refs {
        assert_eq!(
            r.enclosing, None,
            "a reference persisted before the enclosing field existed folds caller-less, not an error"
        );
    }
    // The rest of the entry deserialized intact, so a wrong-shape fixture reds loudly here rather
    // than passing vacuously on an empty load.
    assert_eq!(file.lang, Lang::Rust);
    assert_eq!(file.defs.len(), 1);
    assert_eq!(file.defs[0].name, "caller_fn");
}

// ---- extractor attribution survives the persistence boundary (symbols lane only) ----

#[cfg(feature = "symbols")]
#[test]
fn extractor_attribution_survives_the_build_index_save_load_pipeline() {
    // End-to-end across the extract -> model -> store seam through the PUBLIC `build_index` entry
    // point (which the inside-out `extract` unit test bypasses): a call inside a function body must
    // attribute to that function AND survive serialization, and a top-level reference must stay
    // caller-less across the round-trip. A regression that lost the attribution, or a serde change
    // that dropped the field on save/load, reds here.
    use rigger::grounder::symbols::build_index;
    let dir = tempfile::tempdir().unwrap();
    // `fn f` calls `G()`; the `impl Draw for Widget` header's `Draw` bound is a top-level reference
    // belonging to no function body. (The Rust tags query captures an impl-header trait bound as a
    // reference but not a plain `use` import, so the header bound is the faithful top-level
    // no-caller case - the same shape the inside-out extract test relies on.)
    std::fs::write(
        dir.path().join("lib.rs"),
        "trait Draw {}\nstruct Widget;\nimpl Draw for Widget {}\nfn f() {\n    G();\n}\n",
    )
    .unwrap();
    let root = dir.path().to_str().unwrap();

    let idx = build_index(root, None);
    store::save(&idx, root).unwrap();
    let loaded = store::load(root).expect("the built index persists and reloads");

    let refs = &loaded
        .files()
        .get("lib.rs")
        .expect("the walked file is indexed")
        .refs;
    let g = refs
        .iter()
        .find(|r| r.name == "G")
        .expect("the call G() is extracted as a reference");
    assert_eq!(
        g.enclosing.as_deref(),
        Some("f"),
        "a call inside fn f attributes to f and the caller survives save/load"
    );
    let draw = refs
        .iter()
        .find(|r| r.name == "Draw")
        .expect("the impl-header Draw bound is extracted as a reference");
    assert_eq!(
        draw.enclosing, None,
        "a top-level reference outside every function body stays caller-less across save/load"
    );
}
