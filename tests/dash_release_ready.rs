//! Periphery (integration) test for the ready-to-release handoff on the dash wire
//! (spec 38, criterion 3): the `release_ready` DTO must cross the REAL `/api/state` HTTP
//! socket, from the SAME authority (`ledger::RunState::release_ready`) `rigger status`
//! prints, present ONLY on a done run and OMITTED entirely otherwise.
//!
//! This runs OUTSIDE the crate over the library's public `serve` entrypoint, so it guards
//! the boundary the inside-out `dash.rs` unit test is structurally blind to. That unit test
//! drives `build_state` / `state_json` IN-PROCESS and greps the emitted JSON string; it
//! never crosses the `serve` -> `handle_conn` -> `route` -> `build_state` socket path that
//! threads the newly-added `run_branch`/`base` parameters, and it never proves the
//! `#[serde(skip_serializing_if = "Option::is_none")]` omission survives the wire. This file
//! adds exactly that layer, over the same public surface on BOTH the default and the
//! `--no-default-features` lane (none of it is feature-gated).
//!
//! Spec 82, criterion 2 round 2 adds a SECOND layer at the bottom of this file: the wire DTO
//! crossing the socket correctly (proven above) is necessary but not sufficient - the served
//! page's CLIENT-SIDE `render()` still has to splice that value into the DOM without mangling
//! its embedded newline (round 1 closed on the DTO alone and an adversary caught exactly this
//! gap: a real browser collapses the newline under the page's default `white-space: normal`
//! unless the render pipeline and its CSS preserve it). The `dash.rs` inside-out fix test binds
//! that pipeline with a STATIC grep on the exact JS source line and the CSS rule text; it never
//! EXECUTES either, so a future regression inside `esc()` itself (its body, not this call site)
//! would still grep-match and ship green. `release_ready_pr_command_survives_render_into_the_dom`
//! closes that gap by actually RUNNING the served page's real `render()` under node's built-in
//! `vm` (the same hermetic runtime-harness pattern this suite already uses in
//! `dash_decisions_progressive_disclosure.rs`), fed the SAME JSON this file's socket test above
//! proves crosses the wire, and asserts the resulting DOM node's content - not its source text.

use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::process::Command;
use std::time::Duration;

use rigger::contextgraph::Graph;
use rigger::dash::{self, DashInputs};
use rigger::eventstore::Event;
use serde_json::Value;

/// One event of `type_` with a JSON body, positioned the way the store would stamp it so a
/// position-ordered fold sees a realistic monotonic stream.
fn positioned(pairs: &[(&str, &str)]) -> Vec<Event> {
    pairs
        .iter()
        .enumerate()
        .map(|(i, (ty, json))| {
            let mut e = Event::new(*ty, json.as_bytes().to_vec());
            e.position = (i + 1) as u64;
            e
        })
        .collect()
}

/// Serve the seeded run over a real loopback socket, drive one `GET /api/state`, and return
/// the parsed JSON body - the exact path the operator's browser hits.
fn served_state(events: Vec<Event>) -> Value {
    let provider = move |_instance: Option<&str>| -> Result<DashInputs, String> {
        Ok((events.clone(), Graph::default(), Vec::new(), HashMap::new()))
    };
    // A free loopback port this harness OWNS from `bind` through `serve_on`: the listener is handed
    // to the server, never dropped and re-bound. Releasing it first would leave the port free for
    // the whole handoff window, so a sibling test's `bind(0)` in this same binary could be handed
    // it; one `serve` then wins the re-bind and the loser's client CONNECTS SUCCESSFULLY to it and
    // reads the OTHER test's fixture - a content failure no connect-error retry can see.
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let addr = listener.local_addr().unwrap();
    // The lazy `/api/graph` provider (spec 45, criterion 1): this test drives the state poll, which
    // never consults it, so an empty graph provider satisfies `serve`'s `Fn() -> Graph` bound.
    let graph_provider = |_instance: Option<&str>| Graph::default();
    let calls_provider =
        |_: Option<&str>, _: &[String], _: rigger::contextgraph::Direction, _: i64, _: &str| {
            rigger::contextgraph::CallGraph::default()
        };
    let instances_provider = Vec::new;
    // A detached server thread: `serve_on` loops until the process ends; we drive one request.
    // `run_branch`/`base` are the same values `cmd_dash` threads from `resolve_run_base`.
    std::thread::spawn(move || {
        let _ = dash::serve_on(
            listener,
            provider,
            graph_provider,
            calls_provider,
            instances_provider,
            3,
            "rigger-run",
            "origin/main",
        );
    });

    let mut client = connect_with_retry(addr);
    client
        .write_all(b"GET /api/state HTTP/1.1\r\nHost: localhost\r\n\r\n")
        .unwrap();
    let mut resp = String::new();
    client.read_to_string(&mut resp).unwrap();
    assert!(
        resp.starts_with("HTTP/1.1 200 OK"),
        "the state endpoint returns 200:\n{resp}"
    );
    let body = resp.split("\r\n\r\n").nth(1).expect("a response body");
    serde_json::from_str(body).expect("the /api/state body parses as JSON")
}

fn connect_with_retry(addr: SocketAddr) -> TcpStream {
    for _ in 0..200 {
        if let Ok(s) = TcpStream::connect(addr) {
            return s;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    panic!("the dash server never became reachable on {addr}");
}

/// Spec 38, criterion 3: the ready-to-release handoff crosses the real `/api/state` socket
/// on a DONE run - naming the run branch, the release-target base (with `origin/` stripped),
/// the integrated-unit count, and the exact PR command - so the dash and `rigger status`
/// surface the SAME handoff from the SAME authority.
///
/// Spec 82, criterion 1: the PR command is the two-command unique-head flow, with the head
/// derived from the seeded `RunStarted`'s spec stem and run-short-id - proving that
/// derivation crosses the REAL socket, not just the in-process `build_state` unit test.
#[test]
fn release_ready_crosses_the_api_state_socket_on_a_done_run() {
    let done = positioned(&[
        (
            "RunStarted",
            r#"{"run":"7ad52031-01f1-4d37-aa19-ad48090f84a5","spec":"specs/82-unique-pr-heads.md"}"#,
        ),
        ("UnitStarted", r#"{"id":"u1"}"#),
        ("UnitIntegrated", r#"{"id":"u1","commit":"abc"}"#),
    ]);
    let v = served_state(done);
    let rr = &v["release_ready"];
    assert!(
        rr.is_object(),
        "a done run ships the release_ready DTO over the wire; got:\n{v}"
    );
    assert_eq!(rr["run_branch"], "rigger-run");
    assert_eq!(
        rr["base"], "main",
        "the base crosses the wire with `origin/` stripped to the release-target branch"
    );
    assert_eq!(rr["integrated_units"], 1);
    let head = "pr/82-unique-pr-heads-7ad52031-01f";
    assert_eq!(
        rr["pr_command"],
        format!("git push origin rigger-run:{head}\ngh pr create --base main --head {head}")
    );
}

/// The `Option::is_none` skip contract survives the socket: an unfinished run ships NO
/// `release_ready` key at all (absent, not null), so an unfinished run surfaces no
/// release-ready signal on the dash wire either.
#[test]
fn release_ready_is_absent_from_the_wire_for_an_unfinished_run() {
    // A still-un-integrated unit.
    let running = positioned(&[
        ("UnitStarted", r#"{"id":"u1"}"#),
        ("UnitIntegrated", r#"{"id":"u1","commit":"abc"}"#),
        ("UnitStarted", r#"{"id":"u2"}"#),
    ]);
    let v = served_state(running);
    assert!(
        v.get("release_ready").is_none(),
        "an unfinished run omits release_ready from the wire entirely; got:\n{v}"
    );

    // Every unit integrated, but a failed deferred phase-boundary gate: not releasable, so
    // the handoff is still absent from the wire.
    let deferred_failed = positioned(&[
        ("UnitStarted", r#"{"id":"u1"}"#),
        ("UnitIntegrated", r#"{"id":"u1","commit":"abc"}"#),
        ("DeferredGateFailed", r#"{"gate":"itest"}"#),
    ]);
    let v = served_state(deferred_failed);
    assert!(
        v.get("release_ready").is_none(),
        "a failed deferred gate keeps release_ready off the wire; got:\n{v}"
    );
}

/// The integrated-unit COUNT crosses the wire correctly for a run that integrated MORE THAN
/// ONE unit (spec 38, criterion 3): the served `release_ready.integrated_units` is the value
/// the dash's client-side render pluralizes, and every other release-ready test seeds exactly
/// ONE integrated unit - so a count-of-two never crosses the socket and a miscount would ship
/// green. This seeds two integrated units and asserts the wire carries `integrated_units: 2`,
/// with the run branch and PR command intact, so the dash pluralizes off a truthful count.
#[test]
fn release_ready_carries_a_multi_unit_count_across_the_wire() {
    let done_two = positioned(&[
        ("RunStarted", r#"{"run":"r1"}"#),
        ("UnitStarted", r#"{"id":"u1"}"#),
        ("UnitIntegrated", r#"{"id":"u1","commit":"abc"}"#),
        ("UnitStarted", r#"{"id":"u2"}"#),
        ("UnitIntegrated", r#"{"id":"u2","commit":"def"}"#),
    ]);
    let v = served_state(done_two);
    let rr = &v["release_ready"];
    assert!(
        rr.is_object(),
        "a done two-unit run ships the release_ready DTO over the wire; got:\n{v}"
    );
    assert_eq!(
        rr["integrated_units"], 2,
        "the plural integrated-unit count crosses the wire truthfully (the value the dash \
         pluralizes off); got:\n{v}"
    );
    assert_eq!(rr["run_branch"], "rigger-run");
    // Spec 82, criterion 1: no spec was seeded, so the head degrades to the run-short-id
    // alone (`pr/r1`) - the derivation itself is proven end-to-end by the sibling test above.
    assert_eq!(
        rr["pr_command"],
        "git push origin rigger-run:pr/r1\ngh pr create --base main --head pr/r1"
    );
}

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

/// True when a `node` runtime can be spawned (present on dev machines and on GitHub
/// `ubuntu-latest`, which ships Node.js on PATH, so this runtime guard runs in CI).
fn node_available() -> bool {
    Command::new("node")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// A DOM shim + test driver (JavaScript source) that RUNS the served page's own `render()` over a
/// REAL wire state (the exact JSON this file's socket test proves crosses `/api/state`), then reads
/// back the rendered release banner's `<code class="pr">` content.
///
/// The shim covers exactly what a full `render()` pass touches: every element it addresses via
/// `el(id)` is auto-vivified with settable `innerHTML`/`textContent`/`hidden`, matching the
/// `dash_decisions_progressive_disclosure.rs` harness's established shape. `querySelectorAll`
/// returns an empty list (this driver never inspects the decisions/tree regions, only the release
/// banner), so the toggle re-arm loop in `render()` runs zero iterations harmlessly.
const RENDER_PR_COMMAND_HARNESS: &str = r##"
"use strict";
const vm = require("vm");
const fs = require("fs");
const pageScript = fs.readFileSync(process.argv[2], "utf8");
const state = JSON.parse(fs.readFileSync(process.argv[3], "utf8"));

// Minimal DOM shim (vm-realm, prepended to the page script).
const SHIM = String.raw`
const __els = {};
function __El(id){ this.id=id; this._html=""; this._text=""; this.hidden=false; }
Object.defineProperty(__El.prototype, "innerHTML", { get(){ return this._html; }, set(v){ this._html = String(v); } });
Object.defineProperty(__El.prototype, "textContent", { get(){ return this._text; }, set(v){ this._text = String(v); } });
__El.prototype.querySelectorAll = function(){ return []; };
__El.prototype.addEventListener = function(){};
const document = { getElementById: function(id){ return __els[id] || (__els[id] = new __El(id)); } };
// The page installs its drag-pan handlers on the window global at load; a faithful shim provides
// it (a no-op addEventListener; this harness only drives the release banner).
const window = { addEventListener: function(){} };
const fetch = function(){ return Promise.reject(new Error("no network in the render harness")); };
const setTimeout = function(){ return 0; };
`;

// Test driver (vm-realm, appended after the page script - shares its scope, `render`/`el` in scope).
const DRIVER = String.raw`
;(function(){
  render(state);
  const rel = el("release");
  if (rel.hidden) throw new Error("render() left the release banner hidden for a done run's state");
  const html = rel.innerHTML;
  const m = html.match(/<code class="pr">([\s\S]*?)<\/code>/);
  if (!m) throw new Error("render() produced no <code class=\"pr\"> node: " + JSON.stringify(html));
  const rendered = m[1];
  if (!/\n/.test(rendered)) {
    throw new Error("REGRESSION: the rendered pr command lost its embedded newline: " + JSON.stringify(rendered));
  }
  const commands = rendered.split("\n");
  if (commands.length !== 2) {
    throw new Error("REGRESSION: the rendered pr command is not exactly two lines: " + JSON.stringify(rendered));
  }
  if (!commands[0].startsWith("git push")) {
    throw new Error("the first rendered line must be the git push command: " + JSON.stringify(rendered));
  }
  if (!commands[1].startsWith("gh pr create")) {
    throw new Error("the second rendered line must be the gh pr create command: " + JSON.stringify(rendered));
  }
  console.log("OK pr-command-newline-survives-render");
})();
`;

const sandbox = { console: console, state: state };
vm.createContext(sandbox);
vm.runInContext(SHIM + "\n" + pageScript + "\n" + DRIVER, sandbox, { filename: "dash-release-render-harness.js" });
"##;

/// Spec 82, criterion 2 (DASH HANDOFF MATCHES), round 2: the two-command PR handoff still reads
/// as a real line break - not a collapsed run-on - once the SERVED PAGE'S OWN `render()` actually
/// splices it into the DOM, not merely once its DTO crosses the wire (proven above) or once a
/// grep matches the JS source text that is supposed to do the splicing (the implementer's own
/// inside-out fix, `dash.rs::tests::release_ready_pr_command_newline_renders_as_a_real_line_break_
/// not_a_collapsed_run_on`).
///
/// Round 1 closed this criterion on the DTO alone; the adversary caught that the CLIENT-SIDE
/// `render()` -> `esc()` -> `innerHTML` pipeline was never driven, so a whitespace-collapsing
/// regression anywhere in THAT pipeline could still ship green even with a byte-perfect wire. The
/// round-2 production fix (a `white-space: pre-wrap` CSS rule) is correct, but its own regression
/// test binds ONLY the literal source text of the splice call site and the CSS rule - it never
/// EXECUTES `esc()` or `render()`, so it cannot see a regression introduced inside `esc()`'s own
/// body (e.g. a future "hardening" pass that starts collapsing whitespace there): the call site
/// `esc(rr.pr_command)` would still grep-match while the rendered output silently lost its break.
///
/// This test closes that gap: it feeds `render()` the SAME JSON this file's socket test proves
/// crosses `/api/state` (the real authority, not a hand-typed stand-in), executes the served
/// page's actual `render()` and `esc()` under node's built-in `vm` (hermetic, no npm - the exact
/// runtime-harness pattern already established by `dash_decisions_progressive_disclosure.rs`), and
/// asserts what the DOM node's content ACTUALLY IS: a real two-line split at a raw `\n`, not a
/// pattern match on source. Mutation-proven non-vacuous first-hand: temporarily reverting the
/// production CSS fix (dropping `white-space: pre-wrap` from `.release code.pr`) does NOT redden
/// this test (real CSS layout is unreachable from node's `vm`, so it is legitimately rule-4
/// out-of-gate and stays the implementer's structural CSS-text assertion to guard); but
/// temporarily making `esc()` collapse whitespace (`.replace(/\s+/g, " ")` alongside its existing
/// escapes) DOES redden this test with the exact "lost its embedded newline" message, while the
/// implementer's own grep-based test stays green throughout (its assertion never executes `esc`) -
/// confirming this layer catches a class of regression the existing coverage cannot.
#[test]
fn release_ready_pr_command_survives_render_into_the_dom_with_its_newline_intact() {
    if !node_available() {
        eprintln!(
            "SKIP release_ready_pr_command_survives_render_into_the_dom_with_its_newline_intact: \
             no `node` runtime on PATH. This runtime guard needs node (present on dev machines and \
             on ubuntu-latest CI); install node to run it."
        );
        return;
    }

    // The SAME wire content a browser actually receives: drive the real /api/state socket, exactly
    // as `release_ready_crosses_the_api_state_socket_on_a_done_run` above, then feed that OWN JSON
    // to the served page's OWN render() - proving the wire and the client agree end to end.
    let done = positioned(&[
        (
            "RunStarted",
            r#"{"run":"7ad52031-01f1-4d37-aa19-ad48090f84a5","spec":"specs/82-unique-pr-heads.md"}"#,
        ),
        ("UnitStarted", r#"{"id":"u1"}"#),
        ("UnitIntegrated", r#"{"id":"u1","commit":"abc"}"#),
    ]);
    let state = served_state(done);
    let rr = &state["release_ready"];
    assert!(
        rr["pr_command"]
            .as_str()
            .expect("pr_command is a string on the wire")
            .contains('\n'),
        "the wire payload this test feeds to render() must itself carry the real embedded \
         newline, or this test would vacuously pass regardless of what render() does: {state}"
    );

    let page = dash::live_page();
    let script = page_script(&page);

    let dir = tempfile::tempdir().expect("a scratch dir for the render harness");
    let harness_path = dir.path().join("harness.js");
    let script_path = dir.path().join("page-script.js");
    let state_path = dir.path().join("state.json");
    std::fs::write(&harness_path, RENDER_PR_COMMAND_HARNESS).expect("write the render harness");
    std::fs::write(&script_path, script).expect("write the served page script");
    std::fs::write(&state_path, state.to_string()).expect("write the wire state fixture");

    let out = Command::new("node")
        .arg(&harness_path)
        .arg(&script_path)
        .arg(&state_path)
        .output()
        .expect("spawn node to drive the served render() over the real wire state");

    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        out.status.success(),
        "the served render() must splice pr_command's real newline into the DOM unmangled, but \
         the runtime harness failed:\n--- stdout ---\n{stdout}\n--- stderr ---\n{stderr}"
    );
    assert!(
        stdout.contains("OK pr-command-newline-survives-render"),
        "the render harness must confirm the newline survived render():\n--- stdout ---\n{stdout}\n--- stderr ---\n{stderr}"
    );
}
