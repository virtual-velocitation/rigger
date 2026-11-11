//! Periphery (API / contract / integration) tests for spec 116 criterion 2, A REFERENCE'S CALLER
//! IS ITS FUNCTION: a reference's caller is its innermost enclosing function-like definition,
//! else its innermost enclosing definition, else its file. The crate's own tests pin the rule on
//! the extracted references; these run OUTSIDE the crate and guard the three boundaries the rule
//! crosses on its way to an operator:
//!
//!  - the graph `rigger graph build` folds: each call hangs off the function the rule names, on
//!    source where that function and the innermost enclosing definition are different entities;
//!  - the wire forms of the three code kinds the rule reads, `constructor`, `macro` and
//!    `declaration`: the kind string a code entity event carries.
//!
//! It also guards, on the graph `rigger graph build` folds, spec 119 criterion 8, A CALLER IS
//! NAMED BY ITS PATH: a call in a Rust method of `impl T` and one in a Go receiver method each
//! hang off the method's own definition node at its path, never a nameless node at its leaf.
//!
//! It also holds the wire form of the eight older kinds (spec 116 criterion 4 moved their kind
//! strings onto shared constants): the emit pass still lowers each to the string it always did.

#[cfg(feature = "symbols")]
use crate::common;
#[cfg(feature = "symbols")]
use crate::symbol_index;

#[cfg(feature = "symbols")]
use rigger::grounder::symbols::model::Kind;
#[cfg(feature = "symbols")]
use symbol_index::{def, rust_file};

/// GIVEN a project holding the two caller sources: a Rust file nesting an impl, a trait and an
/// inline mod in `fn host` beside a file-level impl, mod and trait, and a JS file binding calls
/// to consts inside and outside functions,
/// WHEN the operator runs `rigger graph build`,
/// THEN the graph's `CALLS` edges are exactly: a fn-local impl header's, trait body's and inline
/// mod's references from `host`; a call inside a method or fn nested in any of them from that
/// method or fn; a file-level impl header's from the impl, a file-level mod's and trait's from
/// themselves; a function-local JS const's call from its function and a top-level const's from
/// the const; and no edge at all for a top-level reference, whose caller is the file.
#[cfg(feature = "symbols")]
#[test]
fn graph_build_hangs_each_call_off_its_innermost_enclosing_function() {
    let (_project, graph) = common::cli::built_graph(&[
        ("lib.rs", common::fixtures::RUST_CALLER_SOURCE),
        ("a.js", common::fixtures::JS_CALLER_SOURCE),
    ]);

    common::fixtures::assert_live_calls(
        &graph,
        &[
            ("a.js::h", "a.js::%k"),
            ("a.js::outer", "a.js::%f"),
            ("a.js::y", "a.js::%g"),
            ("lib.rs::<Widget as Draw>::draw", "lib.rs::%paint"),
            ("lib.rs::Top", "lib.rs::%announce"),
            ("lib.rs::host", "lib.rs::%declare"),
            ("lib.rs::host", "lib.rs::%setup"),
            ("lib.rs::host", "lib.rs::Draw"),
            ("lib.rs::host", "lib.rs::host::Local"),
            ("lib.rs::host::<Local as Draw>::draw", "lib.rs::%brush"),
            ("lib.rs::host::Shape::area", "lib.rs::%measure"),
            ("lib.rs::host::inner::deep", "lib.rs::%dig"),
            ("lib.rs::impl Draw for Widget", "lib.rs::Draw"),
            ("lib.rs::impl Draw for Widget", "lib.rs::Widget"),
            ("lib.rs::outer", "lib.rs::%boot"),
        ],
        "every CALLS edge of the built graph, each from the caller the function-first rule names",
    );
}

/// The `CALLS` edges of [`common::fixtures::RUST_METHOD_CALLER_SOURCE`] folded as `a.rs`, each `(from, to)`: the
/// call in `new` from the method's path `T::new`, and the impl header's self type from the impl.
#[cfg(feature = "symbols")]
const RUST_METHOD_CALLER_CALLS: &[(&str, &str)] = &[
    ("a.rs::T::new", "a.rs::helper"),
    ("a.rs::impl T", "a.rs::T"),
];

/// The `CALLS` edges of [`common::fixtures::GO_RECEIVER_CALLER_SOURCE`] folded as `g.go`, each `(from, to)`: the
/// receiver type and the call to `helper`, both from the method's path `Recv::name`.
#[cfg(feature = "symbols")]
const GO_RECEIVER_CALLER_CALLS: &[(&str, &str)] = &[
    ("g.go::Recv::name", "g.go::Recv"),
    ("g.go::Recv::name", "g.go::helper"),
];

/// GIVEN a project whose one file calls `helper` from a method: a Rust `new` of `impl T`, or a Go
/// receiver method `name` of `*Recv`,
/// WHEN the operator runs `rigger graph build`,
/// THEN every `CALLS` edge of the method hangs off its own definition node at its path,
/// `a.rs::T::new` or `g.go::Recv::name`, the node carrying its leaf, kind and line, never a
/// nameless node at its leaf `a.rs::new` or `g.go::name` (spec 119 criterion 8, A CALLER IS
/// NAMED BY ITS PATH).
#[cfg(feature = "symbols")]
#[test]
fn graph_build_hangs_a_method_s_calls_off_the_method_s_path() {
    let cases = [
        (
            ("a.rs", "rust", common::fixtures::RUST_METHOD_CALLER_SOURCE),
            &[
                ("T", "T", "type", 1),
                ("T::new", "new", "method", 3),
                ("helper", "helper", "function", 5),
                ("impl T", "impl T", "impl", 2),
            ][..],
            RUST_METHOD_CALLER_CALLS,
        ),
        (
            ("g.go", "go", common::fixtures::GO_RECEIVER_CALLER_SOURCE),
            &[
                ("Recv", "Recv", "type", 3),
                ("Recv::name", "name", "method", 5),
                ("helper", "helper", "function", 7),
            ][..],
            GO_RECEIVER_CALLER_CALLS,
        ),
    ];
    for (file, definitions, calls) in cases {
        common::cli::assert_built_graph_holds(
            file,
            definitions,
            &[],
            calls,
            "every code entity of the built graph is a definition at its path, the method's caller \
             node among them",
        );
    }
}

/// A file holding one definition of each of the three kinds, each on line 1.
#[cfg(feature = "symbols")]
fn one_of_each_new_kind() -> rigger::grounder::symbols::model::FileSymbols {
    rust_file(
        vec![
            def(Kind::Constructor, "c"),
            def(Kind::Declaration, "d"),
            def(Kind::Macro, "m"),
        ],
        vec![],
    )
}

/// The `(name, kind string)` of each code entity event the emit pass lowers `file` to, sorted.
#[cfg(feature = "symbols")]
fn emitted_kinds(file: &rigger::grounder::symbols::model::FileSymbols) -> Vec<(String, String)> {
    use rigger::contextgraph::{CodeEntityExtracted, TYPE_CODE_ENTITY_EXTRACTED};
    use rigger::grounder::symbols::events::extract_events;

    let mut kinds: Vec<(String, String)> = extract_events("a.rs", file)
        .iter()
        .filter(|e| e.type_ == TYPE_CODE_ENTITY_EXTRACTED)
        .map(|e| serde_json::from_slice::<CodeEntityExtracted>(&e.data).unwrap())
        .map(|entity| (entity.name, entity.kind))
        .collect();
    kinds.sort();
    kinds
}

/// The emit pass lowers a constructor, a declaration and a macro definition to a code entity
/// event carrying the kind string `constructor`, `declaration` and `macro`.
#[cfg(feature = "symbols")]
#[test]
fn the_emit_pass_lowers_the_three_code_kinds_to_their_kind_strings() {
    assert_eq!(
        emitted_kinds(&one_of_each_new_kind()),
        [("c", "constructor"), ("d", "declaration"), ("m", "macro")]
            .map(|(name, kind)| (name.to_string(), kind.to_string()))
    );
}

/// The emit pass lowers each of the eight other definition kinds to the kind string a graph
/// folded before the kind constants existed already holds: none is renamed by reading it from
/// a constant, and none is swapped for another.
#[cfg(feature = "symbols")]
#[test]
fn the_emit_pass_lowers_the_eight_older_kinds_to_the_kind_strings_a_folded_graph_holds() {
    let file = rust_file(
        vec![
            def(Kind::Constant, "k"),
            def(Kind::Function, "f"),
            def(Kind::Impl, "i"),
            def(Kind::Method, "e"),
            def(Kind::Module, "o"),
            def(Kind::Other, "x"),
            def(Kind::Trait, "t"),
            def(Kind::Type, "y"),
        ],
        vec![],
    );
    assert_eq!(
        emitted_kinds(&file),
        [
            ("e", "method"),
            ("f", "function"),
            ("i", "impl"),
            ("k", "constant"),
            ("o", "module"),
            ("t", "trait"),
            ("x", "other"),
            ("y", "type"),
        ]
        .map(|(name, kind)| (name.to_string(), kind.to_string()))
    );
}
