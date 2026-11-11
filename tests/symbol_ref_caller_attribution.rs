//! Periphery (integration) test for spec 37 criterion 1: the extractor attributes each reference
//! to its INNERMOST enclosing function (the caller; a reference no function encloses takes its
//! innermost enclosing definition), recorded on the `SymRef.enclosing` field.
//!
//! It runs OUTSIDE the crate, over the library's public `build_index` entry point and the real
//! tree-sitter extraction pass, so it lives in the `symbols` lane only.

#[cfg(feature = "symbols")]
#[test]
fn extractor_attributes_each_reference_to_its_innermost_enclosing_definition() {
    // Through the PUBLIC `build_index` entry point (which the inside-out `extract` unit test
    // bypasses): a call inside a function body must attribute to that function, and a top-level
    // reference must stay caller-less.
    let dir = tempfile::tempdir().unwrap();
    // `fn f` calls `G()`; the `impl Draw for Widget` header's `Draw` bound attributes to the impl
    // block (a definition of its own); the top-level `setup!()` call belongs to no definition.
    std::fs::write(
        dir.path().join("lib.rs"),
        "trait Draw {}\nstruct Widget;\nimpl Draw for Widget {}\nfn f() {\n    G();\n}\nsetup!();\n",
    )
    .unwrap();
    let root = dir.path().to_str().unwrap();

    let loaded = rigger::grounder::symbols::build_index(root, None);

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
        "a call inside fn f attributes to f"
    );
    let draw = refs
        .iter()
        .find(|r| r.name == "Draw")
        .expect("the impl-header Draw bound is extracted as a reference");
    assert_eq!(
        draw.enclosing.as_deref(),
        Some("impl Draw for Widget"),
        "an impl header's trait bound attributes to its impl"
    );
    let setup = refs
        .iter()
        .find(|r| r.name == "setup")
        .expect("the top-level setup! call is extracted as a reference");
    assert_eq!(
        setup.enclosing, None,
        "a top-level reference outside every definition stays caller-less"
    );
}
