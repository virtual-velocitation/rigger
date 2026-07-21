//! Periphery (integration) tests for spec 29c criterion 5: the unified graph is POPULATED from the
//! LIVE project. These run OUTSIDE the crate, over the library's PUBLIC surface, so they guard the
//! boundary the inside-out unit tests are structurally blind to.
//!
//! Criterion 5's production ingestion has ONE public entry point per graph half: the whole-tree
//! extraction functions `grounder::symbols::events::project_batches` (code, spec 29a) and
//! `grounder::design::events::project_batches` (design intent, spec 29b). A live run drives BOTH to
//! lower the real project tree into the per-file event batches the always-compiled fold ingests. The
//! conductor's fold / keyed-emit / once-per-process wiring around them is PRIVATE, and the inside-out
//! unit test drives that wiring through `build_prompt_with_failure` directly - so nothing there pins
//! the PUBLIC extraction contract these two functions carry. The happy-path integration suites feed
//! only files that DO extract, so they never exercise:
//!
//! - the documented SKIP-EMPTY contract (a file that extracts to nothing yields NO batch) and the
//!   barren-root no-op (a source-less tree yields NO batches, never a panic) - the very property that
//!   lets the conductor's ingest stay a byte-for-byte no-op on a tree with nothing to extract;
//! - the design walk's UNREADABLE / binary-file robustness. A live run ingests the WHOLE project
//!   tree, and a real tree carries binary files (images, databases) the design walk `read_to_string`s;
//!   a file that is not valid UTF-8 must be skipped, never crash the walk that populates the graph.
//!
//! These tests pin those boundaries at the crate edge.
//!
//! The extraction path compiles only in the `symbols` lane (the design module and the symbols
//! extraction pass both carry `#[cfg(feature = "symbols")]`), so - exactly like the 29a/29b
//! integration suites - these tests are symbols-gated and compile to nothing in the light lane,
//! keeping BOTH feature lanes green.

/// The CODE half's public production entry SKIPS a file that extracts to no symbols, and yields an
/// empty batch set on a barren root. The 29a/29c happy-path integration tests feed only files that DO
/// extract, so nothing there pins the skip: deleting the `(!events.is_empty())` filter would leave
/// their assertions (the real files present) green while silently emitting an empty batch for every
/// symbol-less file a real tree carries. This pins the contract at the crate boundary.
#[cfg(feature = "symbols")]
#[test]
fn code_project_batches_skips_a_symbol_less_file_and_yields_nothing_on_a_barren_root() {
    use rigger::grounder::symbols::events::project_batches;

    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("src")).unwrap();
    // A file with a real definition (yields a batch) beside a parseable file with NO symbols. The
    // symbol-less file IS indexed - a recovered parse inserts an empty entry - but extracts to no
    // events, so it must carry no batch.
    std::fs::write(dir.path().join("src/real.rs"), "pub fn real_symbol() {}\n").unwrap();
    std::fs::write(
        dir.path().join("src/blank.rs"),
        "// only a comment, no symbols\n",
    )
    .unwrap();

    let files: Vec<String> = project_batches(dir.path().to_str().unwrap())
        .into_iter()
        .map(|(f, _)| f)
        .collect();
    assert_eq!(
        files,
        vec!["src/real.rs"],
        "a parseable file that extracts to no symbols yields no batch; only the file with real \
         definitions is lowered. got {files:?}"
    );

    // Barren root: a directory with no extractable source yields an EMPTY batch set (never a panic) -
    // the contract that lets the conductor's ingest stay a byte-for-byte no-op on a source-less tree.
    let barren = tempfile::tempdir().unwrap();
    assert!(
        project_batches(barren.path().to_str().unwrap()).is_empty(),
        "a source-less root yields no code batches"
    );
}

/// The DESIGN half's public production entry SKIPS a file with no design intent, and yields an empty
/// batch set on a barren root. The 29b/29c happy-path integration tests feed only files that DO carry
/// intent (a design doc and a `# WHY:` source), so nothing there pins the skip: deleting the
/// `if !events.is_empty()` guard would leave their assertions green while emitting an empty batch for
/// every plain source file a real tree carries. This pins the contract at the crate boundary.
#[cfg(feature = "symbols")]
#[test]
fn design_project_batches_skips_a_no_intent_file_and_yields_nothing_on_a_barren_root() {
    use rigger::grounder::design::events::project_batches;

    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("docs")).unwrap();
    std::fs::create_dir_all(dir.path().join("src")).unwrap();
    // A design doc (yields concept + link intent) beside a plain source file carrying NO `# WHY:` /
    // `# NOTE:` rationale - and so no design intent at all.
    std::fs::write(
        dir.path().join("docs/architecture.md"),
        "# Reference architecture\n\n## Node taxonomy\n\nThe `src/combat.rs` module folds nodes.\n",
    )
    .unwrap();
    std::fs::write(dir.path().join("src/plain.rs"), "pub fn plain() {}\n").unwrap();

    let files: Vec<String> = project_batches(dir.path().to_str().unwrap())
        .into_iter()
        .map(|(f, _)| f)
        .collect();
    assert_eq!(
        files,
        vec!["docs/architecture.md"],
        "a source file with no # WHY:/# NOTE: rationale carries no design intent and yields no \
         batch; only the design doc is lowered. got {files:?}"
    );

    let barren = tempfile::tempdir().unwrap();
    assert!(
        project_batches(barren.path().to_str().unwrap()).is_empty(),
        "a design-intent-less root yields no design batches"
    );
}

/// The DESIGN half walks the WHOLE tree and `read_to_string`s every file, so a real project's binary
/// files (images, databases) reach the walk. A file that is not valid UTF-8 must be SKIPPED, never
/// crash the walk that populates the graph: the `if let Ok(contents) = read_to_string(..)` guard is
/// what keeps `read_to_string`'s Err arm from ever panicking a live ingest. The happy-path suites
/// feed only UTF-8 files, so nothing there exercises the Err arm; a refactor to `.expect("readable")`
/// would panic every live run whose tree carries one binary file. This pins the boundary: the binary
/// file yields no batch and the readable design doc beside it still ingests.
#[cfg(feature = "symbols")]
#[test]
fn design_project_batches_skips_a_binary_file_without_crashing_the_walk() {
    use rigger::grounder::design::events::project_batches;

    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("docs")).unwrap();
    std::fs::create_dir_all(dir.path().join("assets")).unwrap();
    // A genuine design doc beside a non-UTF-8 binary file, the pair a real tree presents to the walk.
    std::fs::write(
        dir.path().join("docs/architecture.md"),
        "# Reference architecture\n\n## Node taxonomy\n\nThe `src/combat.rs` module folds nodes.\n",
    )
    .unwrap();
    std::fs::write(
        dir.path().join("assets/logo.png"),
        [0x89u8, 0x50, 0x4E, 0x47, 0x00, 0xFF, 0xFE, 0x01, 0x80, 0xC0],
    )
    .unwrap();

    // The walk must not panic on the binary file, and the binary file must contribute no batch while
    // the readable design doc beside it still ingests.
    let files: Vec<String> = project_batches(dir.path().to_str().unwrap())
        .into_iter()
        .map(|(f, _)| f)
        .collect();
    assert_eq!(
        files,
        vec!["docs/architecture.md"],
        "a binary (non-UTF-8) file is skipped without crashing the walk; only the readable design \
         doc is lowered. got {files:?}"
    );
}
