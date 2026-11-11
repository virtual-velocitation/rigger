//! Periphery (contract and integration) tests for spec 119, IDS FOLLOW THE LANGUAGE'S PATH: each
//! definition's node is `<file>::<path>`, its path the language's own. The crate's own tests pin
//! the extraction and the fold; these run OUTSIDE the crate and guard the wire form the path
//! crosses on its way to an operator: the `CodeEntityExtracted` payload the production ingest
//! emits. `path` is carried only when it differs from `name`, so a file-level definition's payload
//! keeps the form recorded before the field existed, and every payload round-trips through the
//! public payload type, whose `id_path` is the id's text after the file.

#[cfg(feature = "symbols")]
use crate::common;

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
