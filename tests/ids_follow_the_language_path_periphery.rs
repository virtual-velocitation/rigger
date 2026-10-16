//! Periphery (contract and integration) tests for spec 119, IDS FOLLOW THE LANGUAGE'S PATH: each
//! definition's node is `<file>::<path>`, its path the language's own. The crate's own tests pin
//! the extraction and the fold; these run OUTSIDE the crate and guard the two wire forms the path
//! crosses on its way to an operator:
//!
//!  - the `CodeEntityExtracted` payload the production ingest emits: `path` is carried only when
//!    it differs from `name`, so a file-level definition's payload keeps the form recorded before
//!    the field existed, and every payload round-trips through the public payload type, whose
//!    `id_path` is the id's text after the file;
//!  - the persisted symbol index the production ingest lowers from: one persisted before
//!    definitions carried `path` is never served, so the ingest's payloads come from a rebuild over
//!    the tree, while the same index carrying `path` is served as persisted.

mod common;
#[path = "common/symbol_index.rs"]
mod symbol_index;

/// A payload's `name` and its `path` key's JSON value, `None` when it has no such key.
#[cfg(feature = "symbols")]
type NameAndPath = common::NameAndKey;

/// A fresh tree whose one file `a.rs` holds a file-level fn `run` and type `T`, the impl `impl T`
/// and the method `T::new` under it.
#[cfg(feature = "symbols")]
fn source_tree() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("a.rs"),
        "fn run() {}\nstruct T;\nimpl T {\n    fn new() {}\n}\n",
    )
    .unwrap();
    dir
}

/// Each of `payloads` as its [`NameAndPath`].
#[cfg(feature = "symbols")]
fn names_and_paths(payloads: &[serde_json::Value]) -> Vec<NameAndPath> {
    common::names_and_key(payloads, "path")
}

/// The [`NameAndPath`] of the method `new`'s payload: its `path`, `T::new`.
#[cfg(feature = "symbols")]
fn new_at_its_path() -> NameAndPath {
    ("new".to_string(), Some("T::new".into()))
}

/// The [`NameAndPath`] of each payload [`source_tree`]'s own extraction emits, in emit order, each
/// definition after every definition containing it - `new` after `T` and `impl T`, though its name
/// sorts before `run`'s: the method `new` alone carries a `path` ([`new_at_its_path`]).
#[cfg(feature = "symbols")]
fn source_names_and_paths() -> Vec<NameAndPath> {
    vec![
        ("T".to_string(), None),
        ("impl T".to_string(), None),
        ("run".to_string(), None),
        new_at_its_path(),
    ]
}

/// GIVEN a tree whose `a.rs` defines the file-level `run`, `T` and `impl T` and the method
/// `T::new`, WHEN the production ingest emits it, THEN the method's `CodeEntityExtracted` payload
/// is emitted after `T`'s and `impl T`'s, each payload carries `path` exactly when its path differs
/// from its name (`T::new` alone), and each deserializes to
/// the public payload type, whose `id_path` is that path, and serializes back to itself.
#[cfg(feature = "symbols")]
#[test]
fn the_extracted_event_carries_path_only_off_file_level_and_round_trips() {
    let tree = source_tree();
    let payloads = common::code_entity_payloads(tree.path());
    assert_eq!(
        names_and_paths(&payloads),
        source_names_and_paths(),
        "the method's payload after its type's and impl's, and only the payload whose path differs from its name carries a path key"
    );
    let round_trips: Vec<(String, bool)> = payloads
        .iter()
        .map(|payload| {
            let (typed, back) = common::code_entity_round_trip(payload);
            (typed.id_path().to_string(), back)
        })
        .collect();
    assert_eq!(
        round_trips,
        [
            ("T".to_string(), true),
            ("impl T".to_string(), true),
            ("run".to_string(), true),
            ("T::new".to_string(), true),
        ],
        "each payload's id path, and whether it serializes back to itself"
    );
}

/// The [`NameAndPath`] of each `CodeEntityExtracted` payload the production ingest emits over
/// [`source_tree`] once its persisted index, stamped with the current extraction generation,
/// holds `a.rs` as the one method `new` at line 4, carrying `path` as given or no `path` key when
/// `None`.
#[cfg(feature = "symbols")]
fn emitted_over_an_index_whose_method_carries(path: Option<&str>) -> Vec<NameAndPath> {
    let tree = source_tree();
    let method = common::fixtures::with_path(
        serde_json::json!({ "kind": "Method", "name": "new", "line": 4, "end_line": 4 }),
        path,
    );
    let index =
        symbol_index::rust_index_json("a.rs", serde_json::json!([method]), serde_json::json!([]));
    symbol_index::persist_current(tree.path().to_str().unwrap(), &index);
    names_and_paths(&common::code_entity_payloads(tree.path()))
}

/// GIVEN a tree whose persisted index was written before definitions carried `path` (its one
/// method lacks the key), WHEN the production ingest emits the tree, THEN that index is not
/// served: the payloads are the tree's own, rebuilt, the method's carrying `T::new`, never the
/// method alone with the empty path a path-less definition would fold at its leaf by. The same
/// index carrying `path` is served as persisted, the tree unread for it: its one method alone.
#[cfg(feature = "symbols")]
#[test]
fn the_ingest_rebuilds_an_index_persisted_before_path_and_emits_its_path() {
    assert_eq!(
        emitted_over_an_index_whose_method_carries(None),
        source_names_and_paths(),
        "an index lacking path is rebuilt from the tree, the method at its path"
    );
    assert_eq!(
        emitted_over_an_index_whose_method_carries(Some("T::new")),
        [new_at_its_path()],
        "an index carrying path is served as persisted"
    );
}
