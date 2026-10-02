//! Fixtures over the served dash page's source.

/// The served page's decisions render region discloses PROGRESSIVELY: bound to the region from
/// the `el("decisions")` assignment to its empty-state sentinel (so a `<details>` another panel
/// emits cannot satisfy it), each decision is a native `<details>` whose `<summary>` previews
/// `id + a one-line summary` and whose expandable body carries the FULL reasoning - never the old
/// flat `<table>` dump - superseded entries stay struck, and the `preview()` helper collapses a
/// summary to ONE truncated line.
pub fn assert_decisions_region_discloses_progressively(page: &str) {
    // Bind to the decisions render region: from the `el("decisions")` assignment to its
    // empty-state sentinel, so a `<details>` another panel emits cannot satisfy the guard.
    let start = page
        .find("el(\"decisions\")")
        .expect("the decisions render region must exist");
    let end = page[start..]
        .find("no decisions recorded")
        .map(|i| start + i)
        .expect("the decisions render must keep its empty-state sentinel");
    let region = &page[start..end];

    // Native progressive disclosure: each decision is a `<details>` with a `<summary>` line -
    // NOT the old flat `<table>` that dumped every (possibly multi-KB) summary inline.
    assert!(
        region.contains("<details"),
        "each decision must render as a native <details> element: {region}"
    );
    assert!(
        region.contains("<summary>"),
        "each decision's <details> needs a one-line <summary> preview: {region}"
    );
    assert!(
        !region.contains("<table"),
        "the decisions must no longer render as a flat <table> dump: {region}"
    );

    // The `<summary>` previews id + a ONE-LINE summary; the expandable body carries the FULL
    // reasoning. Both the id and the truncated preview feed the summary line, and the full
    // `summary` text feeds the body, so a long decision collapses to one line but expands whole.
    assert!(
        region.contains("esc(d.id)"),
        "the summary line must show the decision id: {region}"
    );
    assert!(
        region.contains("preview(d.summary)"),
        "the summary line must show a one-line preview of the decision summary: {region}"
    );
    assert!(
        region.contains("esc(d.summary)"),
        "the expandable body must carry the full decision reasoning (esc(d.summary)): {region}"
    );
    // Superseded decisions stay visually struck through in the collapsed line.
    assert!(
        region.contains("d.superseded"),
        "superseded decisions must still be distinguished (struck): {region}"
    );

    // The `preview()` helper collapses the summary to a SINGLE line (whitespace runs collapsed)
    // and truncates it with an ellipsis, so the always-visible line is never a multi-KB dump.
    let p = page
        .find("function preview(")
        .expect("a preview() helper must collapse a summary to one line");
    let body = &page[p..(p + 320).min(page.len())];
    assert!(
        body.contains("replace(/\\s+/"),
        "preview() must collapse whitespace runs so the preview is one line: {body}"
    );
    assert!(
        body.contains(".slice(") && body.contains("..."),
        "preview() must truncate a long summary with an ellipsis: {body}"
    );
}
