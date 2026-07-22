//! Periphery (integration) test for the dash's DECISION history render (spec 30, criterion c4):
//! the decision history renders as PROGRESSIVE DISCLOSURE - each decision a native `<details>`
//! whose `<summary>` previews `id + a one-line summary` and whose expandable body carries the FULL
//! reasoning, so a multi-KB decision collapses to one line but expands whole (the dash charter: no
//! framework, no inline multi-KB dumps).
//!
//! This runs OUTSIDE the crate, over the library's PUBLIC surface (`rigger::dash::serve`), and
//! crosses the REAL loopback HTTP socket the operator's browser actually hits. The implementer's
//! inside-out unit test in `dash.rs` greps `live_page()` IN-PROCESS: it is structurally blind to
//! the serve path (the `route` dispatch of `GET /` -> `Response::html(200, live_page())` and the
//! HTTP framing the socket delivers). This layer proves the SERVED root page - the bytes a client
//! receives from the public `serve` entrypoint - carries the c4 progressive-disclosure decisions
//! region end-to-end, not merely that the in-process template string does.
//!
//! Interactive expand/collapse and the `preview()` truncation are BROWSER behaviors (rule 4), so
//! this is a STRUCTURAL guard on the render MECHANISMS the served page ships that deliver them; it
//! binds to the decisions render region (`el("decisions")` .. the empty-state sentinel) so a
//! `<details>` some OTHER panel emits cannot satisfy it. This criterion OWNS progressive
//! disclosure; the run-tree section is criterion 3's, so this test does NOT touch the tree render.
//!
//! `dash`, `spawn`, `contextgraph` are compiled on BOTH the default and the `--no-default-features`
//! lane (none feature-gated), so this guards the served boundary in both lanes.

use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::process::Command;
use std::time::{Duration, Instant};

use rigger::contextgraph::Graph;
use rigger::dash::{self, DashInputs};

/// Start `serve` on a FRESH ephemeral loopback port and fetch `GET /` once, returning the raw HTTP
/// response - or `None` when THIS attempt lost the free-port handoff race (see the retry note on
/// [`fetch_served_root_page`]). A `None` is always a transient port-handoff loss, never a content
/// failure: a cleanly-served response is returned whole (200 or not) so the caller's assertions run
/// on it, and only a connect/read that never completes (nobody listening, or a stale holder's reset)
/// yields `None` to be retried with a fresh port.
fn try_fetch_served_root_page() -> Option<String> {
    // Free-port probe: bind port 0, learn the port, release it, then serve there. Releasing before
    // `serve` re-binds opens a TOCTOU window (see [`fetch_served_root_page`]); the caller retries.
    let port = TcpListener::bind(("127.0.0.1", 0))
        .ok()?
        .local_addr()
        .ok()?
        .port();
    let addr = SocketAddr::from(([127, 0, 0, 1], port));

    // The root page never reads the provider; a trivial empty-inputs provider satisfies `serve`'s
    // `Fn() -> Result<DashInputs, String>` bound.
    let provider = || -> Result<DashInputs, String> {
        Ok((Vec::new(), Graph::default(), Vec::new(), HashMap::new()))
    };

    // A detached server thread: `serve` loops until the process ends; we drive one request. If its
    // internal bind lost the race (EADDRINUSE), the thread returns at once and nobody answers here.
    std::thread::spawn(move || {
        let _ = dash::serve(addr, provider, 3, "rigger-run", "origin/main");
    });

    // Connect within a SHORT budget: `serve` binds within a few ms when it wins the port, so a
    // budget miss means the bind lost the race (nobody is listening) - retry a fresh port, do not
    // hang. A connect that succeeds against a stale holder instead surfaces below as a reset.
    let deadline = Instant::now() + Duration::from_millis(1500);
    let mut client = loop {
        match TcpStream::connect(addr) {
            Ok(s) => break s,
            Err(_) if Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(10));
            }
            Err(_) => return None,
        }
    };

    // Drive one request. A write/read error here is the free-port window's stale holder answering
    // and resetting the connection (`Connection reset by peer`); treat it as a handoff loss and
    // retry. The server answers `Connection: close`, so a clean `read_to_string` reads to EOF.
    if client
        .write_all(b"GET / HTTP/1.1\r\nHost: localhost\r\n\r\n")
        .is_err()
    {
        return None;
    }
    let mut resp = String::new();
    match client.read_to_string(&mut resp) {
        Ok(_) => Some(resp),
        Err(_) => None,
    }
}

/// Drive the hand-rolled dash server over a REAL loopback socket through the public `serve`
/// entrypoint and fetch `GET /` (the root page), returning the full raw HTTP response (status line
/// + headers + body).
///
/// This RETRIES the whole port handoff. The free-port probe - bind port 0, learn the port, release
/// it, then let `serve` re-bind it - leaves an unavoidable TOCTOU window: under parallel test load
/// the released port is re-taken before `serve` re-binds it, so `serve`'s internal bind returns
/// EADDRINUSE (and the client then connects to the transient holder and is reset). This is a
/// TEST-harness artifact of learning a free port for a server that binds INTERNALLY - production
/// `rigger dash` binds ONE stable port once (via `free_port_from`) and never drop-rebinds, so it is
/// never exposed to this race. A lost handoff simply retries with a FRESH port; each attempt is
/// independent, so the guard is deterministic without weakening what it proves (the served bytes
/// over the real socket). Only a connection-level transient is retried; a cleanly-served response
/// is returned to the caller's assertions unchanged, so a genuine content regression still fails.
fn fetch_served_root_page() -> String {
    for _ in 0..200 {
        if let Some(resp) = try_fetch_served_root_page() {
            return resp;
        }
    }
    panic!(
        "the dash server never served GET / over the real socket after many fresh-port attempts"
    );
}

/// The SERVED root page carries the c4 progressive-disclosure decisions region over the real HTTP
/// `serve` socket: a well-formed `200 text/html` response whose decisions render region emits a
/// native `<details>`/`<summary>` per decision (id + a one-line `preview`) with the FULL summary in
/// the expandable body - NOT the old flat `<table>` that dumped every summary inline - plus the
/// one-line `preview()` helper the summary line depends on. Guards the serve/route/framing seam the
/// in-process `live_page()` grep is structurally blind to.
#[test]
fn the_served_root_page_ships_the_decisions_progressive_disclosure_region() {
    let resp = fetch_served_root_page();

    // The served response is a well-formed HTML page, not a 404/405/500 - the browser's entrypoint.
    assert!(
        resp.starts_with("HTTP/1.1 200 OK"),
        "GET / returns 200 over the real serve socket:\n{resp}"
    );
    assert!(
        resp.contains("text/html"),
        "the served root page is HTML:\n{resp}"
    );
    let page = resp
        .split_once("\r\n\r\n")
        .map(|(_, body)| body)
        .expect("a served response body");

    // Bind to the decisions render region: from the `el("decisions")` assignment to its empty-state
    // sentinel, so a `<details>` ANOTHER panel emits cannot satisfy the guard.
    let start = page
        .find("el(\"decisions\")")
        .expect("the served page must carry the decisions render region");
    let end = page[start..]
        .find("no decisions recorded")
        .map(|i| start + i)
        .expect("the decisions render must keep its empty-state sentinel");
    let region = &page[start..end];

    // Native progressive disclosure crosses the wire: each decision a `<details>` with a `<summary>`
    // preview line - NOT the old flat `<table>` that dumped every (possibly multi-KB) summary inline.
    assert!(
        region.contains("<details"),
        "the served decisions region must render each decision as a native <details>: {region}"
    );
    assert!(
        region.contains("<summary>"),
        "the served decisions region needs a one-line <summary> preview per decision: {region}"
    );
    assert!(
        !region.contains("<table"),
        "the served decisions region must no longer be a flat <table> dump: {region}"
    );

    // The `<summary>` line previews id + a ONE-LINE summary; the expandable body carries the FULL
    // reasoning. Both the id and the truncated preview feed the summary line, and the full `summary`
    // text feeds the body, so a long decision collapses to one line but expands whole.
    assert!(
        region.contains("esc(d.id)"),
        "the served summary line must show the decision id: {region}"
    );
    assert!(
        region.contains("preview(d.summary)"),
        "the served summary line must show a one-line preview of the summary: {region}"
    );
    assert!(
        region.contains("esc(d.summary)"),
        "the served expandable body must carry the full decision reasoning: {region}"
    );
    assert!(
        region.contains("d.superseded"),
        "the served region must still distinguish superseded decisions (struck): {region}"
    );

    // The `preview()` helper the summary line depends on ships too, collapsing the summary to a
    // SINGLE line (whitespace runs collapsed) and truncating a long one with an ellipsis - so the
    // always-visible line the served page carries is never a multi-KB dump.
    let p = page
        .find("function preview(")
        .expect("the served page must carry the preview() helper");
    let helper = &page[p..(p + 320).min(page.len())];
    assert!(
        helper.contains("replace(/\\s+/"),
        "served preview() must collapse whitespace runs to one line: {helper}"
    );
    assert!(
        helper.contains(".slice(") && helper.contains("..."),
        "served preview() must truncate a long summary with an ellipsis: {helper}"
    );
}

/// A DOM shim + test driver (JavaScript source) that RUNS the served page's own `render()` twice -
/// one live-poll cycle - to prove an operator-expanded decision stays open across the re-render.
///
/// The template-grep tests above are structurally blind to the RUNTIME behavior: `render()` re-runs
/// every `POLL_MS` and wholesale-replaces the decisions region's `innerHTML`, which destroys and
/// recreates the `<details>` subtree; a grep over the served string cannot see that a body the
/// operator expanded snaps shut on the next poll. This harness executes the real page script under
/// node's built-in `vm` (no npm, fully hermetic - `fetch`/`setTimeout` are inert so the live tail
/// never touches the network), so it observes the actual open-state across two renders.
///
/// It shares the page script's lexical scope (appended after it), so it calls `render()`/`el()`
/// directly. The DOM shim covers exactly what the render path touches: `getElementById`,
/// `innerHTML`, `querySelectorAll("details.decision")`, `addEventListener`, and each `<details>`
/// `open`/`data-did` state. Mutation-proven: removing the render-side `open` re-application (or the
/// whole tracking mechanism) re-collapses `d-alpha` and the driver throws.
const RENDER_TWICE_HARNESS: &str = r##"
"use strict";
const vm = require("vm");
const fs = require("fs");
const pageScript = fs.readFileSync(process.argv[2], "utf8");

// Minimal DOM shim (vm-realm, prepended to the page script).
const SHIM = String.raw`
const __els = {};
function __El(id){ this.id=id; this._html=""; this._text=""; this._children=[]; this._listeners={}; this.open=false; this.dataset={}; }
Object.defineProperty(__El.prototype, "innerHTML", {
  get(){ return this._html; },
  set(v){ this._html = String(v); this._children = __parseDecisions(this._html); }
});
Object.defineProperty(__El.prototype, "textContent", {
  get(){ return this._text; },
  set(v){ this._text = String(v); }
});
__El.prototype.querySelectorAll = function(sel){
  return this._children.filter(function(c){ return /(^|\s)decision(\s|$)/.test(c.cls); });
};
__El.prototype.addEventListener = function(t,f){ (this._listeners[t]=this._listeners[t]||[]).push(f); };
function __parseDecisions(html){
  const out = [];
  const blocks = html.match(/<details\b[\s\S]*?<\/details>/g) || [];
  for(const blk of blocks){
    const open = /<details\b[^>]*\sopen(?=[\s>])/.test(blk);
    const clsM = blk.match(/<details\b[^>]*class="([^"]*)"/);
    const cls = clsM ? clsM[1] : "";
    const didM = blk.match(/data-did="([^"]*)"/);
    const codeM = blk.match(/<code class="did">([^<]*)<\/code>/);
    const did = didM ? didM[1] : (codeM ? codeM[1] : "");
    out.push({ open: open, cls: cls, did: did, dataset: { did: didM ? didM[1] : undefined },
      _listeners: {}, addEventListener: function(t,f){ (this._listeners[t]=this._listeners[t]||[]).push(f); } });
  }
  return out;
}
const document = { getElementById: function(id){ return __els[id] || (__els[id] = new __El(id)); } };
const fetch = function(){ return Promise.reject(new Error("no network in the render harness")); };
const setTimeout = function(){ return 0; };
`;

// Test driver (vm-realm, appended after the page script - shares its scope).
const DRIVER = String.raw`
;(function(){
  const long = "d-alpha reasoning line one\nline two spans multiple lines and is long enough that the one-line preview truncates while the expandable body carries the whole thing so the operator can actually read it";
  const state = {
    run: { units: [] }, metrics: {}, step: { wave: [] },
    graph: { decisions: [
      { id: "d-alpha", summary: long, superseded: false },
      { id: "d-beta", summary: "d-beta reasoning", superseded: false }
    ], findings: [] },
    tree: [], blockers: [], events: [], generated_at: 0, position: 1
  };

  // Poll render #1: the operator's browser paints the decision list.
  render(state);
  const dec = el("decisions");
  const a1 = dec._children.find(function(c){ return c.did === "d-alpha"; });
  if(!a1) throw new Error("render#1 produced no <details> for d-alpha");

  // The operator expands d-alpha (native <details> toggle).
  a1.open = true;
  (a1._listeners.toggle || []).forEach(function(fn){ fn({}); });

  // Poll render #2: the live poll re-renders the region (wholesale innerHTML swap).
  render(state);
  const a2 = el("decisions")._children.find(function(c){ return c.did === "d-alpha"; });
  const b2 = el("decisions")._children.find(function(c){ return c.did === "d-beta"; });
  if(!a2) throw new Error("render#2 produced no <details> for d-alpha");
  if(a2.open !== true) throw new Error("REGRESSION: an operator-expanded decision (d-alpha) re-collapsed after the live poll re-render");
  if(b2 && b2.open === true) throw new Error("an untouched decision (d-beta) must stay collapsed across the poll");
  console.log("OK expanded-decision-survives-poll");
})();
`;

const sandbox = { console: console };
vm.createContext(sandbox);
vm.runInContext(SHIM + "\n" + pageScript + "\n" + DRIVER, sandbox, { filename: "dash-render-harness.js" });
"##;

/// Extract the single inline `<script>` body from the served page.
fn page_script(page: &str) -> &str {
    let open = page
        .find("<script>")
        .expect("the served page carries a <script>")
        + "<script>".len();
    let close = page
        .find("</script>")
        .expect("the served page closes its <script>");
    &page[open..close]
}

/// True when a `node` runtime can be spawned (present on dev machines and on GitHub `ubuntu-latest`,
/// which ships Node.js on PATH, so this runtime guard runs in CI).
fn node_available() -> bool {
    Command::new("node")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// RUNTIME guard for spec 30 c4's charter: a decision the operator expands must stay open across the
/// 1.5s live poll so a multi-KB reasoning body can actually be READ in the primary `rigger dash`
/// mode. The live poll re-runs `render()`, which wholesale-replaces the decisions region's
/// `innerHTML` (destroying + recreating the `<details>` subtree); the fix tracks expanded ids and
/// re-applies `open` on every render so the operator's expansion survives.
///
/// This drives the SERVED page's real `render()` twice under a DOM shim (via node's `vm`), expands
/// `d-alpha` between the renders, and asserts it is still open after the second render while an
/// untouched `d-beta` stays collapsed. It is the runtime check the grep tests cannot make: reverting
/// the render-side `open` re-application re-collapses `d-alpha` and this test goes red.
#[test]
fn an_operator_expanded_decision_survives_the_live_poll_re_render() {
    if !node_available() {
        eprintln!(
            "SKIP an_operator_expanded_decision_survives_the_live_poll_re_render: no `node` runtime \
             on PATH. This runtime guard needs node (present on dev machines and on ubuntu-latest \
             CI); install node to run it."
        );
        return;
    }

    let page = dash::live_page();
    let script = page_script(&page);

    let dir = tempfile::tempdir().expect("a scratch dir for the render harness");
    let harness_path = dir.path().join("harness.js");
    let script_path = dir.path().join("page-script.js");
    std::fs::write(&harness_path, RENDER_TWICE_HARNESS).expect("write the render harness");
    std::fs::write(&script_path, script).expect("write the served page script");

    let out = Command::new("node")
        .arg(&harness_path)
        .arg(&script_path)
        .output()
        .expect("spawn node to drive the served render() twice");

    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        out.status.success(),
        "the served render() must keep an operator-expanded decision open across the live poll's \
         re-render, but the runtime harness failed:\n--- stdout ---\n{stdout}\n--- stderr ---\n{stderr}"
    );
    assert!(
        stdout.contains("OK expanded-decision-survives-poll"),
        "the render harness must confirm the expanded decision survived:\n--- stdout ---\n{stdout}\n--- stderr ---\n{stderr}"
    );
}

/// A DOM shim + test driver (JavaScript source) that LOADS the served page's own script and RUNS
/// its `preview()` helper on a multi-line, over-long summary, proving the always-visible `<summary>`
/// line really collapses to ONE truncated line - the core c4 charter that a decision "collapses to
/// one line but expands whole", so a multi-KB reasoning body is NEVER dumped inline (spec 30:36).
///
/// The template-grep tests above only prove the `.slice(`/`...` TOKENS exist in the shipped
/// `preview()`; they are blind to what the function actually COMPUTES. A mutation that guts the
/// truncation (raising the cap so no summary is ever cut) or drops the `/g` flag (so only the first
/// whitespace run collapses and newlines survive) keeps every one of those tokens - so all three
/// grep tests stay green while a multi-KB summary dumps inline on the always-visible line. This
/// harness RUNS the real helper under node's built-in `vm` (no npm, hermetic; `fetch`/`setTimeout`
/// are inert so the page's load-time poll never touches the network) and asserts the OUTPUT: a
/// long, multi-line input yields a single line (no `\n`/`\r`/`\t`, no residual whitespace run)
/// truncated with an ellipsis, and a short one passes through untouched. Mutation-proven: raising
/// the cap or dropping the `/g` flag makes the driver throw.
const PREVIEW_HARNESS: &str = r##"
"use strict";
const vm = require("vm");
const fs = require("fs");
const pageScript = fs.readFileSync(process.argv[2], "utf8");

// Minimal DOM shim (vm-realm, prepended to the page script): just enough for the page script to
// LOAD and run its load-time bootstrap harmlessly. `preview()` itself touches no DOM.
const SHIM = String.raw`
const __els = {};
function __El(id){ this.id=id; this._html=""; this._text=""; }
Object.defineProperty(__El.prototype, "innerHTML", { get(){ return this._html; }, set(v){ this._html = String(v); } });
Object.defineProperty(__El.prototype, "textContent", { get(){ return this._text; }, set(v){ this._text = String(v); } });
__El.prototype.querySelectorAll = function(){ return []; };
__El.prototype.addEventListener = function(){};
const document = { getElementById: function(id){ return __els[id] || (__els[id] = new __El(id)); } };
const fetch = function(){ return Promise.reject(new Error("no network in the preview harness")); };
const setTimeout = function(){ return 0; };
`;

// Test driver (vm-realm, appended after the page script - shares its scope, so `preview` is in scope).
const DRIVER = String.raw`
;(function(){
  // A multi-LINE, over-long summary: hard newlines + doubled internal whitespace + a tab + a long
  // unbroken tail, exactly the multi-KB reasoning a decision carries. The one-line preview MUST
  // collapse every whitespace run to a single space (no hard break survives) and TRUNCATE with an
  // ellipsis, so the always-visible <summary> line is never a multi-KB inline dump.
  const multi = "line one\n\nline two   has   runs\tand a tab\n" + "x".repeat(300);
  const p = preview(multi);
  if(typeof p !== "string") throw new Error("preview() did not return a string: " + p);
  if(/[\n\r\t]/.test(p)) throw new Error("REGRESSION: preview kept a hard line break/tab (not one line): " + JSON.stringify(p));
  if(/\s{2,}/.test(p)) throw new Error("REGRESSION: preview kept a collapsed-whitespace run (not one line): " + JSON.stringify(p));
  if(p.length > 123) throw new Error("REGRESSION: preview did not truncate a long summary (len=" + p.length + "); a multi-KB body would dump inline on the always-visible line");
  if(!p.endsWith("...")) throw new Error("REGRESSION: preview did not mark truncation with an ellipsis: " + JSON.stringify(p));
  // A short, single-line summary passes through unchanged (no spurious truncation/ellipsis).
  const short = preview("a tidy one-liner");
  if(short !== "a tidy one-liner") throw new Error("preview() mangled a short single-line summary: " + JSON.stringify(short));
  console.log("OK preview-collapses-and-truncates");
})();
`;

const sandbox = { console: console };
vm.createContext(sandbox);
vm.runInContext(SHIM + "\n" + pageScript + "\n" + DRIVER, sandbox, { filename: "dash-preview-harness.js" });
"##;

/// RUNTIME guard for spec 30 c4's core charter (the summary is a PREVIEW, never an inline dump):
/// the `preview()` helper the `<summary>` line depends on must collapse a multi-line, multi-KB
/// summary to ONE truncated line. The grep tests above only prove `preview()` CONTAINS the
/// `.slice(`/`...` idiom - they cannot see that raising the truncation cap (so nothing is ever cut)
/// or dropping the `/g` flag (so only the first whitespace run collapses) leaves a multi-KB summary
/// dumped inline on the always-visible line while every grep stays green. This drives the real
/// helper under node's `vm` and asserts its OUTPUT; reverting either behavior makes it go red.
#[test]
fn the_summary_preview_collapses_a_multiline_summary_to_one_truncated_line() {
    if !node_available() {
        eprintln!(
            "SKIP the_summary_preview_collapses_a_multiline_summary_to_one_truncated_line: no \
             `node` runtime on PATH. This runtime guard needs node (present on dev machines and on \
             ubuntu-latest CI); install node to run it."
        );
        return;
    }

    let page = dash::live_page();
    let script = page_script(&page);

    let dir = tempfile::tempdir().expect("a scratch dir for the preview harness");
    let harness_path = dir.path().join("harness.js");
    let script_path = dir.path().join("page-script.js");
    std::fs::write(&harness_path, PREVIEW_HARNESS).expect("write the preview harness");
    std::fs::write(&script_path, script).expect("write the served page script");

    let out = Command::new("node")
        .arg(&harness_path)
        .arg(&script_path)
        .output()
        .expect("spawn node to run the served preview() helper");

    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        out.status.success(),
        "preview() must collapse a multi-line summary to one truncated line so the <summary> is \
         never a multi-KB inline dump, but the runtime harness failed:\n--- stdout ---\n{stdout}\n--- stderr ---\n{stderr}"
    );
    assert!(
        stdout.contains("OK preview-collapses-and-truncates"),
        "the preview harness must confirm the one-line truncation:\n--- stdout ---\n{stdout}\n--- stderr ---\n{stderr}"
    );
}
