//! Periphery (contract / API) test for spec 29b's design-intent link pass, run OUTSIDE the
//! crate over the library's public surface: the link extraction reads a doc's inline-code paths
//! through the domain's one backtick code-span reader (spec 112). It extracts real Markdown, so it
//! runs on the `symbols` lane alone.

#![cfg(feature = "symbols")]

#[test]
fn the_design_link_pass_reads_inline_code_paths_through_the_one_backtick_run_reader() {
    use rigger::grounder::design::extract::extract_links;
    use rigger::grounder::design::model::LinkRel;

    // Spec 112 (*The one masker*, as the operator decided it in `d112-op-backtick-run-pairing`):
    // the design-intent link pass reads a doc's inline-code paths through the domain's one
    // backtick code-span reader, which pairs backtick RUNS as Markdown delimits a code span.
    // Given a design doc naming one path in a double-backtick span, one in a single-backtick
    // span and one after an unterminated backtick, when its links are extracted, then exactly
    // the two delimited paths are SPECIFIES targets: the double-backtick span reads as one span
    // whose text is its path, and an unterminated backtick names nothing.
    let md = "# Reference architecture\n\n\
              The projector ``src/store/log.rs`` folds the log, `src/store/fold.rs` reads it, \
              and an unterminated `src/store/tail.rs names nothing.\n";
    let got: Vec<(String, LinkRel, String)> = extract_links("docs/architecture.md", md)
        .into_iter()
        .map(|l| (l.from, l.rel, l.to))
        .collect();
    let from = "docs/architecture.md".to_string();
    assert_eq!(
        got,
        vec![
            (
                from.clone(),
                LinkRel::Specifies,
                "src/store/fold.rs".to_string()
            ),
            (from, LinkRel::Specifies, "src/store/log.rs".to_string()),
        ],
        "both delimited code paths, and only they, link from the design doc"
    );
}
