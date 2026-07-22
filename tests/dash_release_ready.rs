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

use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
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
    let provider = move || -> Result<DashInputs, String> {
        Ok((events.clone(), Graph::default(), Vec::new(), HashMap::new()))
    };
    // A free loopback port: bind an ephemeral listener, learn its port, release it, serve there.
    let port = TcpListener::bind(("127.0.0.1", 0))
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    // A detached server thread: `serve` loops until the process ends; we drive one request.
    // `run_branch`/`base` are the same values `cmd_dash` threads from `resolve_run_base`.
    std::thread::spawn(move || {
        let _ = dash::serve(addr, provider, 3, "rigger-run", "origin/main");
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
#[test]
fn release_ready_crosses_the_api_state_socket_on_a_done_run() {
    let done = positioned(&[
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
    assert_eq!(
        rr["pr_command"],
        "gh pr create --base main --head rigger-run"
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
