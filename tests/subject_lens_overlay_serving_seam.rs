//! Periphery (integration) test for the two SERVING SEAMS that deliver the spec 55 c4 unified-
//! inspector client seam to a browser: the LIVE ROUTER path (`GET /` -> `route` -> `live_page`) and
//! the EXPORT path (`render_export` -> a self-contained static snapshot). This criterion OWNS the
//! served page that carries the subject-sticky lens control, the rationale-overlay toggle, and the
//! re-projection renderer; this layer proves that page is actually DELIVERED, unregressed, by both of
//! the dash's page-producing seams and that each honors its own delivery contract.
//!
//! The unit's own served-page test (`subject_lens_overlay_served_page.rs`) drives the client seam at
//! the SCRIPT altitude: a structural grep on the bytes of `dash::live_page()`, and a node-`vm` driver
//! that runs the extracted `<script>`. Neither layer crosses the two module seams in `dash.rs` that
//! actually deliver that script to a client:
//!   * the ROUTING AUTHORITY (`route`) - the dispatch a browser's `GET /` hits, which frames the page
//!     as a READ-ONLY `200 text/html` response and refuses every mutating method; and
//!   * the EXPORT SPLICE (`render_export`) - which inlines the run snapshot so the page evaluates
//!     `LIVE = false`, making the seam's `!LIVE` STATIC-EXPORT branch the one that runs.
//!
//! The served-page test calls `live_page()` DIRECTLY (bypassing `route`, always `LIVE = true`), so the
//! router's HTTP contract and the export's `LIVE = false` degradation are boundaries it is
//! structurally blind to. This layer proves the c4 seam survives BOTH deliveries and honors each
//! seam's contract.
//!
//! `dash`, `contextgraph` compile on BOTH the default and the `--no-default-features` lane (neither is
//! feature-gated), so this guards the serving seam in both lanes.

use std::collections::HashMap;

use rigger::contextgraph::Graph;
use rigger::dash;

/// A minimal empty graph. The c4 client seam lives in the page CHROME - the static `#kghead` header
/// and the inline `<script>` - independent of any graph content, so an empty projection is the
/// tightest fixture that still exercises the full serving path (the seam ships whether or not a run
/// has produced a graph yet).
fn empty_graph() -> Graph {
    Graph {
        nodes: Vec::new(),
        edges: Vec::new(),
    }
}

/// The c4 client-seam anchors that must survive EITHER serving seam: the static header the new
/// controls live in (`#kghead`), the subject-sticky lens control (`#kgsubjectlens`), the rationale-
/// overlay toggle (`#kgoverlaytoggle` + its `data-overlay` handle), and the three dispatch functions
/// that make them work (`onLensPick` subject-sticky, `reprojectSubject` re-request, `setOverlay`
/// toggle). Each token is bound to a c4 mechanism, so an unrelated substring cannot satisfy it.
const C4_SEAM_ANCHORS: &[&str] = &[
    "id=\"kghead\"",
    "id=\"kgsubjectlens\"",
    "id=\"kgoverlaytoggle\"",
    "data-overlay",
    "function onLensPick(",
    "function reprojectSubject(",
    "function setOverlay(",
];

/// Assert `html` carries every c4 seam anchor, naming which delivery produced it so a failure points
/// at the seam that dropped the control.
fn assert_carries_c4_seam(html: &str, whence: &str) {
    for anchor in C4_SEAM_ANCHORS {
        assert!(
            html.contains(anchor),
            "the {whence} must carry the spec 55 c4 client-seam anchor `{anchor}`"
        );
    }
}

/// The LIVE ROUTER SEAM (spec 55 c4): the dash's single routing authority - the dispatch a browser's
/// `GET /` actually hits - frames the c4 client seam as a READ-ONLY `200 text/html` response, and
/// serves the byte-for-byte `live_page()`. `/` and `/index.html` are the two aliases that serve it. A
/// NON-GET method is refused (`405`) and never yields the inspector, so the seam is reachable ONLY
/// over the read-only method contract the dash guarantees (it exposes no mutating endpoint). The
/// implementer's served-page test calls `live_page()` DIRECTLY, bypassing `route`; this proves the
/// routing authority delivers the seam and honors the HTTP contract around it. Dropping the c4 seam
/// from the served page, or serving it on a mutating method, reddens this driver.
#[test]
fn the_router_serves_the_c4_client_seam_as_read_only_html() {
    let graph = empty_graph();
    let ages: HashMap<String, u64> = HashMap::new();

    for path in ["/", "/index.html"] {
        let resp = dash::route(
            "GET",
            path,
            &[],
            &graph,
            &[],
            &ages,
            3,
            "rigger-run",
            "origin/main",
            &[],
        );
        assert_eq!(
            resp.status, 200,
            "GET {path} must serve the live inspector page (200)"
        );
        assert_eq!(
            resp.content_type, "text/html; charset=utf-8",
            "GET {path} must be served as html so the browser runs the c4 client seam"
        );
        let body = String::from_utf8(resp.body).expect("the served page is utf-8");
        assert_carries_c4_seam(&body, &format!("GET {path} response body"));
        // The routing authority serves EXACTLY the live page - no divergence between what the
        // script-altitude test greps (`live_page()`) and what a browser receives from `route`.
        assert_eq!(
            body,
            dash::live_page(),
            "GET {path} must serve exactly live_page(), byte for byte"
        );
    }

    // The READ-ONLY contract holds for the seam-bearing page: a mutating method is refused (405) and
    // never serves the inspector - the c4 controls are read-only affordances, reachable only via GET.
    let refused = dash::route(
        "POST",
        "/",
        &[],
        &graph,
        &[],
        &ages,
        3,
        "rigger-run",
        "origin/main",
        &[],
    );
    assert_eq!(
        refused.status, 405,
        "a non-GET method on / must be refused (405), never serve the inspector"
    );
    let refused_body = String::from_utf8(refused.body).expect("the refusal is utf-8");
    assert!(
        !refused_body.contains("id=\"kghead\""),
        "a refused (405) request must NOT serve the c4 inspector seam"
    );
}

/// The EXPORT SEAM (spec 55 c4): `render_export` splices the run snapshot into the SAME template, so
/// the exported static artifact (the file `rigger dash --export` writes, the one an operator opens
/// offline) must ALSO ship the c4 client seam. Crucially the export sets `EMBEDDED_STATE` to the
/// snapshot (NOT `null`), so the page evaluates `LIVE = false` and the seam's `!LIVE` STATIC-EXPORT
/// branch is the one that runs: an exported subject re-projection degrades to a message instead of a
/// dead `/api/graph?seed=&lens=` fetch against a server that is not there. The implementer's served-
/// page test only drives `live_page()` (always `LIVE = true`), so this `LIVE = false` delivery AND its
/// degradation are boundaries that test cannot reach. Dropping the seam from the export template, or
/// removing reprojectSubject's `!LIVE` guard, reddens this driver.
#[test]
fn the_export_page_carries_the_c4_seam_and_degrades_its_live_only_affordance() {
    let graph = empty_graph();
    let ages: HashMap<String, u64> = HashMap::new();
    let html = dash::render_export(&[], &graph, &[], &ages, 3, "rigger-run", "origin/main")
        .expect("render_export serializes a valid snapshot");

    // The c4 seam ships in the EXPORTED artifact, not only the live page.
    assert_carries_c4_seam(&html, "exported snapshot");

    // The export is a self-contained `LIVE = false` page: the state placeholder is fully resolved to
    // the snapshot (never left unresolved, never `null`), so `LIVE = EMBEDDED_STATE === null` is false
    // and the seam's static-export branches are the ones that run.
    assert!(
        !html.contains("__RIGGER_STATE__"),
        "the exported snapshot must resolve the state placeholder (a self-contained offline file)"
    );
    assert!(
        !html.contains("EMBEDDED_STATE = null"),
        "the exported snapshot must NOT be live (EMBEDDED_STATE is the snapshot, so LIVE=false)"
    );

    // The c4 subject re-projection degrades gracefully under `LIVE = false`: reprojectSubject guards
    // on `!LIVE` and shows the static-export message, so an exported page never fires a dead live
    // fetch. Scope the assertion to reprojectSubject's own body (start -> next top-level function) so a
    // `!LIVE` branch elsewhere on the page cannot satisfy it.
    let start = html
        .find("async function reprojectSubject(")
        .expect("the exported page carries reprojectSubject");
    let end = html[start..]
        .find("\nfunction ")
        .map(|i| start + i)
        .unwrap_or(html.len());
    let reproject = &html[start..end];
    assert!(
        reproject.contains("!LIVE"),
        "reprojectSubject must GUARD on !LIVE so an exported (static) page never fires a live fetch"
    );
    assert!(
        reproject.contains("this is a static export"),
        "an exported subject re-projection must degrade to the static-export message, not a dead fetch"
    );
}
