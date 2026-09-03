//! Periphery for spec 80, unit c1 (criterion 1 - FULL-TEXT EXTRACTION): `extract_criteria`'s
//! PUBLIC CONTRACT (`rigger::spec::extract_criteria`), proven at the crate boundary against
//! REAL committed spec content rather than an invented fixture.
//!
//! What this file OWNS: two black-box contract checks that only see the `pub fn` surface,
//! never `extract_criteria`'s private helpers (`criterion_block_boundary`,
//! `checkbox_opening_lines`, `checkbox_text`) - the outside-in complement to the
//! implementer's own exhaustive synthetic-fixture unit tests beside `extract_criteria` in
//! `src/spec.rs::tests` (which prove the JOINING RULE's boundary logic - adjacent
//! checkboxes, a blank line, a heading, a nested sub-bullet - on invented text a unit test
//! structurally cannot get wrong by construction, since the author writes both the input
//! and the assertion). Test one below instead replays the EXACT real-world input that
//! exposed this bug (specs/62-dash-marker-lifecycle.md's own criterion 1, named in spec
//! 80's Goal as the fresh run that proved the truncation live), so the fix is pinned
//! against the actual regression, not a paraphrase of it. Test two is a corpus-wide
//! invariant sweep proving the JOINING RULE never merges two DIFFERENT checkbox items into
//! one - the one way a boundary-detection defect could silently corrupt the criterion list
//! across content no synthetic fixture anticipates.
//!
//! NOT owned (spec 80's Design, BLAST RADIUS: "src/spec.rs only, plus tests"; criterion 1's
//! own text: "end-to-end delivery is criterion 2's, NOT this one's"): whether the full text
//! reaches `self.deps.criteria` / `UnitStarted.spec_criterion` through main.rs/conductor.rs
//! (criterion 2, a separate unit's periphery proof) - and NOT owned either: whether the
//! spec-shape lint stays first-physical-line-only (already proven white-box by the
//! implementer's `spec_shape_advisories_ignores_*` tests, and guarded corpus-wide by the
//! pre-existing `tests/spec_lint.rs::spec_lint_self_clean_over_the_committed_corpus`
//! regression snapshot, which this unit's `checkbox_opening_lines` split was designed to
//! leave untouched).

use std::path::Path;

/// The exact real-world input that exposed this bug: specs/62's own criterion 1, a
/// checkbox wrapping across three physical lines with an OWNS sentence on the third
/// (spec 80's Goal section: "Proven live in the spec-62 fresh run"). Before this unit's
/// fix, `extract_criteria` returned only the first physical line here - this test proves
/// the PUBLIC function now returns the whole thing, called exactly as any real consumer
/// (`main.rs::load_criteria`, `conductor.rs`) calls it: by reading the file from disk and
/// passing its text straight through, no synthetic reshaping.
#[test]
fn extract_criteria_recovers_the_real_spec_62_owns_sentence_that_this_bug_used_to_truncate() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("specs/62-dash-marker-lifecycle.md");
    let text =
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));

    let criteria = rigger::spec::extract_criteria(&text);
    assert!(
        !criteria.is_empty(),
        "specs/62 must parse at least one Done-when criterion"
    );

    let first = &criteria[0];
    let expected = "a test proves MARKER FOLLOWS BIND: a dash start whose bind fails writes \
                     no marker and leaves an existing marker untouched, while a successful \
                     bind writes the marker naming the bound port and the serving PID. This \
                     criterion OWNS the write ordering.";
    assert_eq!(
        first, expected,
        "extract_criteria must return specs/62 criterion 1's FULL three-line text, joined \
         single-spaced with indentation stripped - not just its first physical line; got: \
         {first:?}"
    );

    // The literal pre-fix behavior, spelled out as an explicit regression tripwire: the
    // truncated first-physical-line-only reading this bug produced must never come back.
    let truncated_first_line =
        "a test proves MARKER FOLLOWS BIND: a dash start whose bind fails writes no marker and";
    assert_ne!(
        first, truncated_first_line,
        "extract_criteria must not regress to first-physical-line-only truncation on real \
         spec content"
    );
    assert!(
        first.contains("This criterion OWNS the write ordering."),
        "the third-physical-line OWNS sentence must survive extraction; got: {first:?}"
    );
}

/// Corpus-wide boundary invariant: for every committed `specs/*.md` file, `extract_criteria`
/// never joins two DIFFERENT checkbox items into one criterion. A checkbox-marker line
/// (`- [ ]`, `- [x]`, `- [X]`, or the `*` spelling) always starts a NEW criterion under the
/// JOINING RULE's own boundary definition - so a correctly-joined criterion's text can never
/// contain a second, embedded checkbox marker. This is the one way a boundary-detection
/// regression (weakening `criterion_block_boundary`'s `checkbox_text(line).is_some()` arm)
/// would silently corrupt real content that no synthetic fixture happens to cover - proven
/// here across the WHOLE real corpus, not the implementer's own hand-picked examples.
#[test]
fn extract_criteria_never_merges_two_checkbox_items_across_the_committed_corpus() {
    let specs_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("specs");
    let mut entries: Vec<_> = std::fs::read_dir(&specs_dir)
        .expect("read specs/ directory")
        .map(|e| e.expect("read specs/ dir entry").path())
        .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("md"))
        .collect();
    entries.sort();
    assert!(
        entries.len() >= 3,
        "sanity: the committed specs/ corpus must be non-trivial; got {} files",
        entries.len()
    );

    for path in &entries {
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        let text = std::fs::read_to_string(path)
            .unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
        let criteria = rigger::spec::extract_criteria(&text);

        for (i, criterion) in criteria.iter().enumerate() {
            let n = i + 1;
            assert!(
                !criterion.trim().is_empty(),
                "{name} criterion {n}: extract_criteria returned an empty/blank criterion"
            );
            for marker in ["- [ ]", "- [x]", "- [X]", "* [ ]", "* [x]", "* [X]"] {
                assert!(
                    !criterion.contains(marker),
                    "{name} criterion {n} embeds a checkbox marker ({marker:?}) mid-text - \
                     two checkbox items were joined into one, a JOINING RULE boundary \
                     regression; got: {criterion:?}"
                );
            }
        }
    }
}
