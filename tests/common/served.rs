//! Fixtures for suites that exercise the served dash: in-process routes, a real loopback
//! socket, raw HTTP responses, and the served page's script run under node.

use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::process::Command;
use std::time::{Duration, Instant};

use rigger::contextgraph::{CallGraph, Direction, Graph};
use rigger::dash::{self, route, DashInputs, InstanceView, Response};

/// `GET target` through the in-process dash router over `graph` (no events, no liveness),
/// asserting it is served 200.
pub fn served(graph: &Graph, target: &str) -> Response {
    let liveness: HashMap<String, u64> = HashMap::new();
    let resp = route(
        "GET",
        target,
        &[],
        graph,
        &[],
        &liveness,
        0,
        "rigger-run",
        "origin/main",
        &[],
    );
    assert_eq!(resp.status, 200, "GET {target} must be served 200");
    resp
}

/// [`served`]'s body, parsed as JSON.
pub fn served_json(graph: &Graph, target: &str) -> serde_json::Value {
    let resp = served(graph, target);
    serde_json::from_slice(&resp.body)
        .unwrap_or_else(|e| panic!("the served {target} body must be valid JSON: {e}"))
}

/// The raw bytes of `GET target` against a real dash served on an ephemeral loopback port
/// whose providers all panic - the routes under test read no store, graph or registry.
pub fn get_raw(target: &str) -> Vec<u8> {
    let provider = |_instance: Option<&str>| -> Result<DashInputs, String> {
        panic!("the served route reads no run-scoped store input")
    };
    let graph_provider = |_instance: Option<&str>| -> Graph {
        panic!("the served route opens no whole-graph projection")
    };
    let calls_provider = |_instance: Option<&str>,
                          _seeds: &[String],
                          _dir: Direction,
                          _depth: i64,
                          _floor: &str|
     -> CallGraph { panic!("the served route opens no calls projection") };
    let instances_provider =
        || -> Vec<InstanceView> { panic!("the served route reads no instance registry") };

    let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind an ephemeral loopback port");
    let addr = listener.local_addr().expect("learn the bound port");
    // Never joined: `serve_on` loops over `listener.incoming()` for the life of the
    // process, exactly like every other real-socket dash test in this tree drives it.
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

    let deadline = Instant::now() + Duration::from_secs(3);
    let mut client = loop {
        match TcpStream::connect(addr) {
            Ok(s) => break s,
            Err(_) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(5)),
            Err(e) => panic!("never connected to the served dash on {addr}: {e}"),
        }
    };
    client
        .set_read_timeout(Some(Duration::from_secs(10)))
        .expect("set a read timeout on the client");
    let req = format!("GET {target} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n");
    client.write_all(req.as_bytes()).expect("write the request");
    let mut raw = Vec::new();
    client
        .read_to_end(&mut raw)
        .expect("read the served response to EOF (Connection: close)");
    raw
}

/// Start the dash on a FRESH ephemeral loopback port - `graph_provider` behind its lazy
/// whole-graph provider (`/api/graph` reads it), `poll_graph` behind its state-poll provider -
/// and fetch `GET path` once, returning the raw HTTP response, or `None` on a genuine
/// socket-level failure.
///
/// The listener this attempt binds is HANDED to `serve_on`, never dropped and re-bound. That is
/// load-bearing: releasing it first would leave the port free for the whole handoff window, so a
/// sibling test's `bind(0)` in the same binary could be handed it; one `serve` then wins the
/// re-bind and the loser's client CONNECTS SUCCESSFULLY to it and reads the OTHER test's fixture -
/// a content failure no connect-error retry can see, reddening only on a loaded machine. Owning
/// the port from `bind` through `serve_on` closes that window by construction: a response returned
/// here is always this attempt's own server's.
pub fn try_fetch_over<G>(path: &str, graph_provider: G, poll_graph: Graph) -> Option<String>
where
    G: Fn(Option<&str>) -> Graph + Send + 'static,
{
    let listener = TcpListener::bind(("127.0.0.1", 0)).ok()?;
    let addr = listener.local_addr().ok()?;

    let provider = move |_instance: Option<&str>| -> Result<DashInputs, String> {
        Ok((Vec::new(), poll_graph.clone(), Vec::new(), HashMap::new()))
    };
    let calls_provider =
        |_: Option<&str>, _: &[String], _: Direction, _: i64, _: &str| CallGraph::default();
    let instances_provider = Vec::new;
    // A detached server thread: `serve_on` loops until the process ends; we drive one request.
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

    let deadline = Instant::now() + Duration::from_millis(1500);
    let mut client = loop {
        match TcpStream::connect(addr) {
            Ok(s) => break s,
            Err(_) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(10)),
            Err(_) => return None,
        }
    };
    let req = format!("GET {path} HTTP/1.1\r\nHost: localhost\r\n\r\n");
    if client.write_all(req.as_bytes()).is_err() {
        return None;
    }
    let mut resp = String::new();
    match client.read_to_string(&mut resp) {
        Ok(_) => Some(resp),
        Err(_) => None,
    }
}

/// Fetch through `attempt` over a real loopback socket, RETRYING the whole port handoff on a
/// socket-level transient. Each attempt owns its own port (see [`try_fetch_over`]), so a retry
/// never returns another server's response; `what` names the request in the failure.
pub fn fetch_with_retry(what: &str, attempt: impl Fn() -> Option<String>) -> String {
    for _ in 0..200 {
        if let Some(resp) = attempt() {
            return resp;
        }
    }
    panic!(
        "the dash server never served {what} over the real socket after many fresh-port attempts"
    );
}

/// A whole-graph provider serving `graph` for every instance.
pub fn graph_provider_of(graph: Graph) -> impl Fn(Option<&str>) -> Graph + Send + 'static {
    move |_instance: Option<&str>| graph.clone()
}

/// `GET path` over a real loopback socket against a dash whose whole-graph and state-poll
/// providers both serve `graph`.
pub fn fetch_served(path: &str, graph: &Graph) -> String {
    fetch_with_retry(path, || {
        try_fetch_over(path, graph_provider_of(graph.clone()), graph.clone())
    })
}

/// A raw HTTP response split into its header block and its body bytes.
pub fn split_response(raw: &[u8]) -> (&str, &[u8]) {
    let sep = b"\r\n\r\n";
    let idx = raw
        .windows(sep.len())
        .position(|w| w == sep)
        .expect("a served HTTP response has a header/body terminator");
    let header = std::str::from_utf8(&raw[..idx]).expect("HTTP headers are ASCII/UTF-8");
    let body = &raw[idx + sep.len()..];
    (header, body)
}

/// The value of header `name` (case-insensitive) in a raw header block.
pub fn header_value<'a>(headers: &'a str, name: &str) -> Option<&'a str> {
    headers.lines().find_map(|line| {
        let (k, v) = line.split_once(':')?;
        k.trim().eq_ignore_ascii_case(name).then(|| v.trim())
    })
}

/// The served console shell's HTML.
pub fn served_console_body() -> String {
    let raw = get_raw("/console");
    let (_, body) = split_response(&raw);
    std::str::from_utf8(body)
        .expect("the console shell is UTF-8 HTML")
        .to_string()
}

/// The served console page carries every one of `needles`.
pub fn assert_served_console_page_carries(needles: &[&str]) {
    assert_console_body_carries(&served_console_body(), needles);
}

/// The served console page `body` carries every one of `needles`.
pub fn assert_console_body_carries(body: &str, needles: &[&str]) {
    for needle in needles {
        assert!(
            body.contains(needle),
            "missing {needle:?} in the served console page: {body}"
        );
    }
}

/// The body of a raw HTTP response read as text.
pub fn body_of(resp: &str) -> &str {
    resp.split_once("\r\n\r\n")
        .map(|(_, body)| body)
        .expect("a served response body")
}

/// Connect to a dash server that is still starting, retrying for up to four seconds.
pub fn connect_with_retry(addr: SocketAddr) -> TcpStream {
    for _ in 0..200 {
        if let Ok(s) = TcpStream::connect(addr) {
            return s;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    panic!("the dash server never became reachable on {addr}");
}

/// The inline `<script>` of a served page.
pub fn page_script(page: &str) -> &str {
    let open = page
        .find("<script>")
        .expect("the served page carries a <script>")
        + "<script>".len();
    let close = page
        .find("</script>")
        .expect("the served page closes its <script>");
    &page[open..close]
}

/// A node `vm` harness that runs `shim`, then the page script it is handed as its first argument,
/// then `driver`, in one sandbox exposing only `console` and `process`; `filename` names the
/// harness in node's stack traces.
pub fn vm_harness(shim: &str, driver: &str, filename: &str) -> String {
    const TEMPLATE: &str = r##""use strict";
const vm = require("vm");
const fs = require("fs");
const pageScript = fs.readFileSync(process.argv[2], "utf8");
const SHIM = String.raw`__SHIM__`;
const DRIVER = String.raw`__DRIVER__`;
const sandbox = { console: console, process: process };
vm.createContext(sandbox);
vm.runInContext(SHIM + "\n" + pageScript + "\n" + DRIVER, sandbox, { filename: "__FILENAME__" });
"##;
    TEMPLATE
        .replace("__SHIM__", shim)
        .replace("__DRIVER__", driver)
        .replace("__FILENAME__", filename)
}

/// Run `harness_src` under node against `page`'s script and return (success, stdout, stderr).
pub fn run_page_harness(page: &str, harness_src: &str) -> (bool, String, String) {
    let script = page_script(page);

    let dir = tempfile::tempdir().expect("a scratch dir for the runtime harness");
    let harness_path = dir.path().join("harness.js");
    let script_path = dir.path().join("page-script.js");
    std::fs::write(&harness_path, harness_src).expect("write the runtime harness");
    std::fs::write(&script_path, script).expect("write the served page script");

    let out = Command::new("node")
        .arg(&harness_path)
        .arg(&script_path)
        .output()
        .expect("spawn node to drive the served page");
    (
        out.status.success(),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

/// [`run_node_harness`], skipped (loudly) when there is no `node` runtime on PATH - node is
/// present on dev machines and ubuntu-latest CI, so only a machine without it skips.
pub fn node_harness_passes(harness_src: &str, ok_token: &str) {
    node_harness_claims(harness_src, ok_token, SERVED_CLIENT_SEAM);
}

/// [`node_harness_passes`] whose failure reports `claim` - what the harness proves.
pub fn node_harness_claims(harness_src: &str, ok_token: &str, claim: &str) {
    if !super::fixtures::tool_available("node", "--version") {
        eprintln!(
            "SKIP: no `node` runtime on PATH. This runtime guard needs node (present on dev \
             machines and on ubuntu-latest CI); install node to run it."
        );
        return;
    }
    run_node_harness_claiming(harness_src, ok_token, claim);
}

/// What a served-page runtime harness proves unless its caller names more.
const SERVED_CLIENT_SEAM: &str = "the runtime harness must drive the served client seam";

/// Run `harness_src` against the live served page, asserting node succeeds and prints
/// `ok_token` - the sentinel that proves the driver ran to its end.
pub fn run_node_harness(harness_src: &str, ok_token: &str) {
    run_node_harness_claiming(harness_src, ok_token, SERVED_CLIENT_SEAM);
}

/// [`run_node_harness`] whose failure reports `claim`.
fn run_node_harness_claiming(harness_src: &str, ok_token: &str, claim: &str) {
    let (ok, stdout, stderr) = run_page_harness(&dash::live_page(), harness_src);
    assert!(
        ok,
        "{claim}, but node failed:\n--- stdout ---\n{stdout}\n--- stderr ---\n{stderr}"
    );
    assert!(
        stdout.contains(ok_token),
        "the runtime harness must confirm '{ok_token}':\n--- stdout ---\n{stdout}\n--- stderr ---\n{stderr}"
    );
}
