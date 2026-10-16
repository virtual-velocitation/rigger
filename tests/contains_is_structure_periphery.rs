//! Periphery (contract, API and integration) tests for spec 119, CONTAINS IS STRUCTURE: each
//! definition's `CONTAINS` comes from its parent, the file holding only a definition with none.
//! The crate's own tests pin the extraction and the fold; these run OUTSIDE the crate and guard
//! the wire form the parent crosses on its way to an operator, and what the operator sees:
//!
//!  - the `CodeEntityExtracted` payload the production ingest emits carries `parent` only when the
//!    definition has one, so a file-level definition's payload keeps the form recorded before the
//!    field existed, and every payload round-trips through the public payload type, whose
//!    `parent_path` - the parent the fold hangs its `CONTAINS` off - is that `parent`;
//!  - a nested payload recorded before the field existed, its `path` already its own, reads as
//!    having no parent, `parent_path` none, and replays contained by its file;
//!  - `rigger graph build` folds each variant's `CONTAINS` from its enum, and a build after a
//!    variant is removed retires that variant's, as `rigger graph --around` prints them.

mod common;

use rigger::contextgraph::REL_CONTAINS;

/// `payload`, a `CodeEntityExtracted` payload's JSON, read as the public payload type
/// ([`common::code_entity_round_trip`]): its `parent_path`, its `parent`, and whether it
/// serializes back to `payload` itself.
fn parent_read(payload: &serde_json::Value) -> (Option<String>, Option<String>, bool) {
    let (typed, back) = common::code_entity_round_trip(payload);
    (typed.parent_path().map(str::to_owned), typed.parent, back)
}

/// GIVEN a tree whose `a.rs` defines the enum `E` holding the variants `A` and `B`, WHEN the
/// production ingest emits it, THEN `E`'s `CodeEntityExtracted` payload is emitted before its
/// variants', though each variant's name sorts before `E`, each variant's payload carries `parent`
/// `E` and the file-level `E`'s carries no `parent` key, and each payload deserializes to the
/// public payload type, whose `parent` and `parent_path` are that key's value, and serializes back
/// to itself.
#[cfg(feature = "symbols")]
#[test]
fn the_extracted_event_carries_parent_only_off_file_level_and_round_trips() {
    let tree = common::cli::temp_project();
    common::fixtures::write_text(
        tree.path(),
        "a.rs",
        common::fixtures::ENUM_E_HOLDING_A_AND_B,
    );
    let payloads = common::code_entity_payloads(tree.path());
    let under_e = |variant: &str| (variant.to_string(), Some("E".into()));
    assert_eq!(
        common::names_and_key(&payloads, "parent"),
        [("E".to_string(), None), under_e("A"), under_e("B")],
        "the enum's payload before its variants', and only a variant's carries a parent key, its enum's path"
    );
    let read: Vec<_> = payloads.iter().map(parent_read).collect();
    let under_e = (Some("E".to_string()), Some("E".to_string()), true);
    assert_eq!(
        read,
        [(None, None, true), under_e.clone(), under_e],
        "each payload's parent_path, its parent, and whether it serializes back to itself"
    );
}

/// GIVEN the payload of the method `T::new` as recorded before definitions carried `parent` - its
/// `path` `T::new`, no `parent` key - WHEN it is read as the public payload type and replayed
/// through the fold, THEN it has no parent and no `parent_path`, serializes back to itself, and its
/// file contains it.
#[test]
fn a_nested_payload_recorded_before_parent_replays_contained_by_its_file() {
    use common::fixtures::{
        assert_live, event_of, fold_events, path_entity_json, NESTED_METHOD, T_NEW,
    };

    let (file, leaf, path, kind) = T_NEW;
    let payload = path_entity_json(file, leaf, path, kind, 1, "rust");
    assert_eq!(
        parent_read(&payload),
        (None, None, true),
        "a payload with no parent key reads as having none and serializes back without one"
    );
    assert_live(
        &fold_events([event_of(
            rigger::contextgraph::TYPE_CODE_ENTITY_EXTRACTED,
            payload,
        )]),
        REL_CONTAINS,
        &[(file, NESTED_METHOD)],
        "a payload with no parent key is contained by its file",
    );
}

/// The `CONTAINS` lines `rigger graph --around` prints for `edges`, each `(parent, child)`.
#[cfg(feature = "symbols")]
fn contains_lines(edges: &[(&str, &str)]) -> Vec<(String, String, String)> {
    edges
        .iter()
        .map(|&(parent, child)| (parent.into(), REL_CONTAINS.into(), child.into()))
        .collect()
}

/// GIVEN a project whose `a.rs` defines the enum `E` holding the variants `A` and `B`, WHEN the
/// operator runs `rigger graph build` and `rigger graph --around a.rs::E`, THEN the file contains
/// `E` and `E` contains each variant; AND WHEN `B` is removed from the file and the operator
/// builds again, THEN `--around` prints `E` containing `A` alone, `B`'s `CONTAINS` retired.
#[cfg(feature = "symbols")]
#[test]
fn graph_build_hangs_each_variant_off_its_enum_and_retires_a_removed_ones() {
    use common::cli::{edge_lines, run_around, run_rigger_ok, temp_project};
    use common::fixtures::{
        ENUM_E, ENUM_E_HOLDING_A, ENUM_E_HOLDING_A_AND_B, E_HOLDS_A, VARIANT_E_B,
    };

    let dir = temp_project();
    let root = dir.path();
    let mut printed = Vec::new();
    for source in [ENUM_E_HOLDING_A_AND_B, ENUM_E_HOLDING_A] {
        common::fixtures::write_text(root, "a.rs", source);
        run_rigger_ok(root, &["graph", "build"]);
        let (out, err, ok) = run_around(root, ENUM_E, 1);
        assert!(
            ok,
            "graph --around must succeed after a build; stderr: {err}"
        );
        printed.push(edge_lines(&out));
    }
    assert_eq!(
        printed,
        [
            contains_lines(&[E_HOLDS_A, &[(ENUM_E, VARIANT_E_B)]].concat()),
            contains_lines(E_HOLDS_A),
        ],
        "each build's edges around the enum: each variant contained by it, the removed one retired"
    );
}
