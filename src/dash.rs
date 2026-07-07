//! `rigger dash` - an embedded, read-only observability page over the existing
//! projections (spec 11, unit 2).
//!
//! This module owns ALL of the dash's HTTP serving and rendering. It is a THIN
//! adapter: every number it shows is folded by an existing read-model
//! ([`crate::ledger::project`], [`crate::metrics::project`],
//! [`crate::spawn::step_result`], and the [`crate::contextgraph`] subgraph). There is
//! no new business logic here and, in particular, review verdicts are NOT re-derived -
//! they come straight from [`crate::metrics`]'s classification (there is no verdict
//! event type; it is inferred from `UnitStatus` transitions), so the dash and
//! `rigger stats` can never disagree.
//!
//! Two hard lines the spec draws, enforced structurally:
//!   - **No async runtime.** The HTTP layer is hand-rolled and synchronous over
//!     [`std::net::TcpListener`] (one request at a time, loopback only). The default
//!     build gains no tokio/axum and no new dependency at all.
//!   - **No write/control surface.** [`route`] answers only `GET`; every other method,
//!     on every path, is refused with `405`. The conductor stays the sole mutation
//!     authority - control goes through the CLI, never the dash.

use std::collections::BTreeMap;
use std::io::{self, BufRead, BufReader, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::Serialize;

use crate::contextgraph::{Graph, KIND_DECISION, KIND_FINDING, REL_SUPERSEDES};
use crate::eventstore::{Event, Position};
use crate::{ledger, metrics, spawn};

/// The single-file page, embedded at compile time (vanilla HTML/CSS/JS, no build step).
/// [`STATE_PLACEHOLDER`] is substituted with `null` for live serving (the page polls the
/// JSON endpoints) or with an inlined snapshot for `--export` (a static, shareable file).
const PAGE_TEMPLATE: &str = include_str!("dash.html");

/// The token in [`PAGE_TEMPLATE`] replaced with the embedded state. It sits on the right
/// of a JS assignment, so substituting `null` (live) or a JSON object literal (export)
/// both yield valid JavaScript.
const STATE_PLACEHOLDER: &str = "__RIGGER_STATE__";

/// The default loopback port for `rigger dash` when `--port` is not given.
pub const DEFAULT_PORT: u16 = 7420;

// ---------------------------------------------------------------------------
// View DTOs. These live HERE, not on the projection types: adding `Serialize` to
// `metrics::Metrics` / `ledger::RunState` / `contextgraph::Graph` would make the dash a
// co-owner of modules it only reads. Translating their public fields into these plain
// serde structs keeps the dash a thin adapter and the projections' blast radius clean.
// ---------------------------------------------------------------------------

/// The whole `/api/state` payload: one snapshot of the run, assembled from the four
/// projections. `events` is populated only for `--export` (a static page cannot fetch);
/// the live `/api/state` leaves it absent and the page tails [`events_json`] separately.
#[derive(Debug, Serialize)]
pub struct StateView {
    /// Unix seconds when this snapshot was built (client shows it as the freshness clock).
    pub generated_at: u64,
    /// The highest global event position folded into this snapshot - the cursor a live
    /// client can poll `/api/events?since=` from.
    pub position: Position,
    pub run: RunView,
    pub metrics: MetricsView,
    /// The live pending frontier + fixpoint/halt, reused verbatim from
    /// [`spawn::step_result`] (already `Serialize`).
    pub step: spawn::Step,
    pub graph: GraphView,
    /// Present only in an exported snapshot, so the static page can render its event feed
    /// without a network fetch.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub events: Option<Vec<EventView>>,
}

/// The ledger projection, flattened for the wire.
#[derive(Debug, Serialize)]
pub struct RunView {
    pub spec_defect: bool,
    pub deferred_gate_failed: bool,
    pub units: Vec<UnitView>,
    /// Unit ids currently awaiting a human (a `ManualReview` with the unit not yet
    /// terminal) - the other half of the action-needed inbox alongside escalations. Read
    /// verbatim from [`ledger::RunState::manual_review`]; the dash does not fold it.
    pub manual_review: Vec<String>,
}

/// One unit's lifecycle, from [`ledger::Unit`].
#[derive(Debug, Serialize)]
pub struct UnitView {
    pub id: String,
    pub spec_criterion: String,
    pub status: String,
    pub depends_on: Vec<String>,
    pub attempts: u32,
    pub commit: String,
    pub branch: String,
    pub evidence: BTreeMap<String, String>,
}

/// The metrics projection, with the two derived ratios materialized for the client.
#[derive(Debug, Serialize)]
pub struct MetricsView {
    pub units_started: u64,
    pub first_pass_clean: u64,
    pub units_escalated: u64,
    /// Reviews classified as APPROVE by [`metrics::project`] (a `reviewed` transition).
    pub review_approve: u64,
    /// Reviews classified as REJECT by [`metrics::project`] (a loop-back `UnitFailed`).
    pub review_reject: u64,
    pub first_pass_yield: f64,
    pub escalation_rate: f64,
    pub gates: Vec<GateView>,
}

/// One gate's remediation tally (fail is the remediation signal).
#[derive(Debug, Serialize)]
pub struct GateView {
    pub gate: String,
    pub pass: u64,
    pub fail: u64,
    pub total: u64,
}

/// The decisions and findings reachable in the context subgraph around the run.
#[derive(Debug, Serialize)]
pub struct GraphView {
    pub decisions: Vec<DecisionView>,
    pub findings: Vec<FindingView>,
}

/// A decision node; `superseded` is true when a currently-valid `SUPERSEDES` edge points
/// at it (so the page strikes it through), read straight from the context graph rather
/// than re-folding supersession here.
#[derive(Debug, Serialize)]
pub struct DecisionView {
    pub id: String,
    pub summary: String,
    pub superseded: bool,
}

/// A review-finding node from the context graph.
#[derive(Debug, Serialize)]
pub struct FindingView {
    pub id: String,
    pub summary: String,
    pub by: String,
    pub unit: String,
}

/// One event on the `/api/events` feed: a generic, per-type-agnostic view (position,
/// type, and a truncated payload) so the feed adapts over the raw log with no
/// event-specific logic.
#[derive(Debug, Serialize)]
pub struct EventView {
    pub position: Position,
    #[serde(rename = "type")]
    pub type_: String,
    pub summary: String,
}

// ---------------------------------------------------------------------------
// Builders: projections -> view DTOs.
// ---------------------------------------------------------------------------

/// Assemble the `/api/state` snapshot from an ordered slice of run events and a
/// pre-fetched context [`Graph`]. Pure and side-effect free, so it is unit-testable
/// against a seeded slice with no socket, store, or repo.
///
/// `include_events` inlines the event feed into the snapshot (for `--export`); the live
/// endpoint passes `false` and serves the feed from [`events_json`] instead.
pub fn build_state(
    events: &[Event],
    graph: &Graph,
    include_events: bool,
) -> Result<StateView, serde_json::Error> {
    let run = ledger::project(events)?;
    let m = metrics::project(events);
    let step = spawn::step_result(events)?;

    let units = run
        .units
        .values()
        .map(|u| UnitView {
            id: u.id.clone(),
            spec_criterion: u.spec_criterion.clone(),
            status: u.status.as_str().to_string(),
            depends_on: u.depends_on.clone(),
            attempts: u.attempts,
            commit: u.commit.clone(),
            branch: u.branch.clone(),
            evidence: u.evidence.clone(),
        })
        .collect();

    let gates = m
        .gates
        .iter()
        .map(|(gate, c)| GateView {
            gate: gate.clone(),
            pass: c.pass,
            fail: c.fail,
            total: c.total(),
        })
        .collect();

    let metrics_view = MetricsView {
        units_started: m.units_started,
        first_pass_clean: m.first_pass_clean,
        units_escalated: m.units_escalated,
        review_approve: m.review_approve,
        review_reject: m.review_reject,
        first_pass_yield: m.first_pass_yield(),
        escalation_rate: m.escalation_rate(),
        gates,
    };

    let events_view = if include_events {
        Some(events.iter().map(event_view).collect())
    } else {
        None
    };

    Ok(StateView {
        generated_at: now_unix(),
        position: events.iter().map(|e| e.position).max().unwrap_or(0),
        run: RunView {
            spec_defect: run.spec_defect,
            deferred_gate_failed: run.deferred_gate_failed,
            units,
            // Read straight from the ledger projection (folded by `ledger::project`); the
            // dash does not re-derive the inbox, keeping this a thin adapter.
            manual_review: run.manual_review,
        },
        metrics: metrics_view,
        step,
        graph: build_graph_view(graph),
        events: events_view,
    })
}

/// Translate a context [`Graph`] into the decisions/findings the page renders. A decision
/// is marked `superseded` when a currently-valid `SUPERSEDES` edge targets it - the graph
/// keeps such edges valid (only the superseded decision's GOVERNS edges are invalidated),
/// so this is a faithful read of the graph's own supersession, not a re-derivation.
fn build_graph_view(graph: &Graph) -> GraphView {
    let superseded: std::collections::BTreeSet<&str> = graph
        .edges
        .iter()
        .filter(|e| e.rel == REL_SUPERSEDES)
        .map(|e| e.to.as_str())
        .collect();

    let mut decisions = Vec::new();
    let mut findings = Vec::new();
    for n in &graph.nodes {
        match n.kind.as_str() {
            KIND_DECISION => decisions.push(DecisionView {
                id: n.id.clone(),
                summary: n.attrs.get("summary").cloned().unwrap_or_default(),
                superseded: superseded.contains(n.id.as_str()),
            }),
            KIND_FINDING => findings.push(FindingView {
                id: n.id.clone(),
                summary: n.attrs.get("summary").cloned().unwrap_or_default(),
                by: n.attrs.get("by").cloned().unwrap_or_default(),
                unit: n.attrs.get("unit").cloned().unwrap_or_default(),
            }),
            _ => {}
        }
    }
    decisions.sort_by(|a, b| a.id.cmp(&b.id));
    findings.sort_by(|a, b| a.id.cmp(&b.id));
    GraphView {
        decisions,
        findings,
    }
}

/// The node ids to seed the context subgraph with: every unit, decision, and finding id
/// named in the run's own events. Seeding by the ids the run actually produced (rather
/// than a blast-radius file walk) lets the subgraph return their authoritative nodes and
/// the valid SUPERSEDES edges among them at a shallow depth, independent of whether the
/// run emitted the file-touch edges that would otherwise connect them.
pub fn graph_seeds(events: &[Event]) -> Vec<String> {
    use crate::contextgraph::{TYPE_DECISION_MADE, TYPE_REVIEW_FINDING, TYPE_UNIT_STARTED};
    let mut seeds: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    for e in events {
        let key = match e.type_.as_str() {
            TYPE_DECISION_MADE | TYPE_REVIEW_FINDING => "id",
            TYPE_UNIT_STARTED => "unit",
            _ => continue,
        };
        if let Some(id) = field_str(e, key) {
            if !id.is_empty() {
                seeds.insert(id);
            }
        }
    }
    seeds.into_iter().collect()
}

/// A generic feed view of one event: position, type, and a bounded, per-type-agnostic
/// preview of the payload.
fn event_view(e: &Event) -> EventView {
    let raw = String::from_utf8_lossy(&e.data);
    let mut summary: String = raw.chars().take(160).collect();
    if raw.chars().count() > 160 {
        summary.push_str("...");
    }
    EventView {
        position: e.position,
        type_: e.type_.clone(),
        summary,
    }
}

/// Read a top-level string field from an event's JSON payload (best-effort).
fn field_str(e: &Event, key: &str) -> Option<String> {
    serde_json::from_slice::<serde_json::Value>(&e.data)
        .ok()?
        .get(key)?
        .as_str()
        .map(str::to_string)
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

// ---------------------------------------------------------------------------
// JSON endpoint bodies.
// ---------------------------------------------------------------------------

/// The `/api/state` body: the full projected snapshot as JSON.
pub fn state_json(events: &[Event], graph: &Graph) -> Result<String, serde_json::Error> {
    serde_json::to_string(&build_state(events, graph, false)?)
}

/// The `/api/events?since=<position>` body: every event whose global position is strictly
/// greater than `since` (the same exclusive convention as `EventStore::read_all`), so a
/// client polls forward from its last-seen cursor. `since = 0` returns the whole feed
/// (positions are 1-based).
pub fn events_json(events: &[Event], since: Position) -> String {
    let feed: Vec<EventView> = events
        .iter()
        .filter(|e| e.position > since)
        .map(event_view)
        .collect();
    // A tiny hand-built object so the endpoint has no dedicated wrapper DTO.
    serde_json::json!({ "events": feed }).to_string()
}

/// The live page: the template with the state placeholder resolved to `null`, so the
/// browser polls the JSON endpoints.
pub fn live_page() -> String {
    PAGE_TEMPLATE.replace(STATE_PLACEHOLDER, "null")
}

/// The `--export` page: the template with the snapshot (including its event feed) inlined,
/// yielding a self-contained static file that renders offline and never fetches.
///
/// The serialized snapshot is neutralized ([`escape_for_script`]) before it is spliced into
/// the `<script>` element, so no string field it carries can break out of that container.
pub fn render_export(events: &[Event], graph: &Graph) -> Result<String, serde_json::Error> {
    let json = serde_json::to_string(&build_state(events, graph, true)?)?;
    Ok(PAGE_TEMPLATE.replace(STATE_PLACEHOLDER, &escape_for_script(&json)))
}

/// Neutralize a serialized-JSON payload for safe inlining inside an HTML `<script>` element.
///
/// `serde_json` escapes none of `<`, `>`, `&`, so a string field carrying `</script>` - an
/// agent-authored `DecisionMade`/`ReviewFinding` summary, a unit `spec_criterion`, or a raw
/// event payload, all of which flow verbatim into an exported snapshot's inlined feed - would
/// close the script element and inject executing markup into the shared file. Rewriting each to
/// its `\uXXXX` JSON escape - plus the U+2028/U+2029 line separators, which are valid inside a
/// JSON string but terminate a JavaScript statement - keeps the value byte-identical once the
/// browser parses the object literal while making a `</script>` breakout impossible. These five
/// characters only ever occur inside JSON string content (structural JSON uses none of them), so
/// a blanket rewrite of the serialized form stays valid JSON.
fn escape_for_script(json: &str) -> String {
    let mut out = String::with_capacity(json.len());
    for c in json.chars() {
        match c {
            '<' => out.push_str("\\u003c"),
            '>' => out.push_str("\\u003e"),
            '&' => out.push_str("\\u0026"),
            '\u{2028}' => out.push_str("\\u2028"),
            '\u{2029}' => out.push_str("\\u2029"),
            _ => out.push(c),
        }
    }
    out
}

// ---------------------------------------------------------------------------
// HTTP: a hand-rolled synchronous response + router. No async runtime, no dependency.
// ---------------------------------------------------------------------------

/// A minimal HTTP response the router returns and the server writes.
#[derive(Debug, PartialEq, Eq)]
pub struct Response {
    pub status: u16,
    pub content_type: &'static str,
    pub body: Vec<u8>,
}

impl Response {
    fn html(status: u16, body: String) -> Self {
        Response {
            status,
            content_type: "text/html; charset=utf-8",
            body: body.into_bytes(),
        }
    }
    fn json(status: u16, body: String) -> Self {
        Response {
            status,
            content_type: "application/json",
            body: body.into_bytes(),
        }
    }
    fn text(status: u16, body: &str) -> Self {
        Response {
            status,
            content_type: "text/plain; charset=utf-8",
            body: body.as_bytes().to_vec(),
        }
    }

    fn reason(&self) -> &'static str {
        match self.status {
            200 => "OK",
            400 => "Bad Request",
            404 => "Not Found",
            405 => "Method Not Allowed",
            500 => "Internal Server Error",
            _ => "OK",
        }
    }

    /// Write this response as HTTP/1.1 with `Connection: close`, so a bare client knows
    /// the body ends at the connection close (no keep-alive bookkeeping).
    fn write_to(&self, w: &mut impl Write) -> io::Result<()> {
        let header = format!(
            "HTTP/1.1 {} {}\r\nContent-Type: {}\r\nContent-Length: {}\r\n\
             Cache-Control: no-store\r\nConnection: close\r\n\r\n",
            self.status,
            self.reason(),
            self.content_type,
            self.body.len(),
        );
        w.write_all(header.as_bytes())?;
        w.write_all(&self.body)?;
        w.flush()
    }
}

/// The single routing authority. Answers only `GET`; every other method - on every path -
/// is a `405`, which is the structural guarantee that the dash exposes NO mutating
/// endpoint. Pure over the projected inputs, so it is unit-testable without a socket.
pub fn route(method: &str, target: &str, events: &[Event], graph: &Graph) -> Response {
    if method != "GET" {
        return Response::text(
            405,
            "rigger dash is read-only: it serves GET requests only and has no write or \
             control endpoint (the conductor is the sole mutation authority).",
        );
    }
    let path = target.split('?').next().unwrap_or(target);
    match path {
        "/" | "/index.html" => Response::html(200, live_page()),
        "/api/state" => match state_json(events, graph) {
            Ok(body) => Response::json(200, body),
            Err(e) => Response::text(500, &format!("dash: state projection failed: {e}")),
        },
        "/api/events" => {
            let since = query_param(target, "since")
                .and_then(|v| v.parse::<Position>().ok())
                .unwrap_or(0);
            Response::json(200, events_json(events, since))
        }
        _ => Response::text(404, "not found"),
    }
}

/// The first value of query parameter `key` in a request target (`/path?a=1&b=2`).
fn query_param<'a>(target: &'a str, key: &str) -> Option<&'a str> {
    let q = target.split_once('?')?.1;
    q.split('&').find_map(|pair| {
        let (k, v) = pair.split_once('=')?;
        (k == key).then_some(v)
    })
}

/// Parse the method and target out of an HTTP request line (`GET /path HTTP/1.1`).
/// Returns `None` for a malformed line, which the server answers with `400`.
fn parse_request_line(line: &str) -> Option<(String, String)> {
    let mut parts = line.split_whitespace();
    let method = parts.next()?.to_string();
    let target = parts.next()?.to_string();
    Some((method, target))
}

// ---------------------------------------------------------------------------
// The blocking server loop.
// ---------------------------------------------------------------------------

/// Serve the dash on `addr` until the process is stopped, re-reading fresh projection
/// inputs from `provider` on each request (the run advances while the dash watches).
///
/// One connection at a time, synchronously: loopback single-operator traffic needs no
/// concurrency, and a serial loop keeps the sqlite reads and the whole server free of any
/// async runtime. Only the `/api/*` paths consult `provider`; the static page and the
/// method/not-found guards need no store read, so the page still serves before a run has
/// created the store.
pub fn serve<F>(addr: SocketAddr, provider: F) -> io::Result<()>
where
    F: Fn() -> Result<(Vec<Event>, Graph), String>,
{
    let listener = TcpListener::bind(addr)?;
    let bound = listener.local_addr()?;
    eprintln!("rigger dash: serving on http://{bound}/ (read-only; Ctrl-C to stop)");
    for stream in listener.incoming() {
        match stream {
            Ok(s) => {
                if let Err(e) = handle_conn(s, &provider) {
                    eprintln!("rigger dash: connection error: {e}");
                }
            }
            Err(e) => eprintln!("rigger dash: accept error: {e}"),
        }
    }
    Ok(())
}

/// Read one request, route it, and write the response. Splits the store read from the
/// pure [`route`] so a `provider` failure degrades only the `/api/*` paths (to `500`),
/// never the static page.
fn handle_conn<F>(stream: TcpStream, provider: &F) -> io::Result<()>
where
    F: Fn() -> Result<(Vec<Event>, Graph), String>,
{
    // Bound how long a slow or broken client can hold the single serving slot.
    stream.set_read_timeout(Some(Duration::from_secs(10)))?;
    let mut reader = BufReader::new(stream);

    let mut request_line = String::new();
    if reader.read_line(&mut request_line)? == 0 {
        return Ok(()); // client closed before sending anything
    }
    // Drain the remaining request headers (bounded) so the client's write completes before
    // we reply; we route on the request line alone (GET has no body).
    let mut header = String::new();
    while reader.read_line(&mut header)? > 0 {
        if header == "\r\n" || header == "\n" {
            break;
        }
        header.clear();
    }

    let mut stream = reader.into_inner();
    let response = match parse_request_line(request_line.trim_end()) {
        None => Response::text(400, "bad request"),
        Some((method, target)) => {
            let needs_data = method == "GET" && target.starts_with("/api/");
            if needs_data {
                match provider() {
                    Ok((events, graph)) => route(&method, &target, &events, &graph),
                    Err(e) => Response::text(500, &format!("dash: reading the store failed: {e}")),
                }
            } else {
                // The page, 404, and the 405 read-only guard need no projection input.
                route(&method, &target, &[], &Graph::default())
            }
        }
    };
    response.write_to(&mut stream)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contextgraph::{Edge, Node};
    use crate::eventstore::Event;

    fn ev(type_: &str, json: &str) -> Event {
        Event::new(type_, json.as_bytes().to_vec())
    }

    /// Give a slice of events 1-based positions, as the store would on append, so
    /// position-sensitive reads (`/api/events?since=`) are exercised realistically.
    fn positioned(mut events: Vec<Event>) -> Vec<Event> {
        for (i, e) in events.iter_mut().enumerate() {
            e.position = (i + 1) as Position;
        }
        events
    }

    fn seeded_run() -> Vec<Event> {
        positioned(vec![
            ev(
                "UnitStarted",
                r#"{"id":"u1","spec_criterion":"do the thing"}"#,
            ),
            ev("UnitStatus", r#"{"id":"u1","status":"green"}"#),
            ev("GateVerdict", r#"{"gate":"cargo test","pass":true}"#),
            ev("GateVerdict", r#"{"gate":"cargo test","pass":false}"#),
            ev("UnitStatus", r#"{"id":"u1","status":"reviewed"}"#),
            ev("UnitIntegrated", r#"{"id":"u1","commit":"abc123"}"#),
        ])
    }

    #[test]
    fn root_serves_the_embedded_page_with_the_placeholder_resolved() {
        let r = route("GET", "/", &[], &Graph::default());
        assert_eq!(r.status, 200);
        assert_eq!(r.content_type, "text/html; charset=utf-8");
        let body = String::from_utf8(r.body).unwrap();
        assert!(body.contains("rigger dash"), "serves the page");
        assert!(
            !body.contains(STATE_PLACEHOLDER),
            "the live page must resolve the state placeholder (to null), not leak the token"
        );
        assert!(
            body.contains("EMBEDDED_STATE = null"),
            "live serving inlines a null state so the page polls"
        );
    }

    #[test]
    fn state_endpoint_projects_the_seeded_run() {
        let events = seeded_run();
        let r = route("GET", "/api/state", &events, &Graph::default());
        assert_eq!(r.status, 200);
        assert_eq!(r.content_type, "application/json");
        let v: serde_json::Value = serde_json::from_slice(&r.body).unwrap();

        assert_eq!(v["run"]["units"][0]["id"], "u1");
        assert_eq!(v["run"]["units"][0]["status"], "integrated");
        // Metrics folds are present and reflect the seeded gate verdicts.
        assert_eq!(v["metrics"]["units_started"], 1);
        let gates = v["metrics"]["gates"].as_array().unwrap();
        assert_eq!(gates[0]["gate"], "cargo test");
        assert_eq!(gates[0]["pass"], 1);
        assert_eq!(gates[0]["fail"], 1);
        // The live /api/state does not inline the event feed (the page tails it separately).
        assert!(v.get("events").is_none() || v["events"].is_null());
    }

    /// Review verdicts on the wire are exactly `metrics::project`'s classification, never a
    /// second derivation in the dash. Locks the reuse the spec mandates.
    #[test]
    fn review_verdicts_come_straight_from_the_metrics_classification() {
        // A per-unit review reject: a `verified` transition then a loop-back UnitFailed.
        // And a separate approve: a `reviewed` transition.
        let events = positioned(vec![
            ev("UnitStarted", r#"{"id":"a","agent":"impl"}"#),
            ev("UnitStatus", r#"{"id":"a","status":"verified"}"#),
            ev("UnitFailed", r#"{"id":"a"}"#),
            ev("UnitStarted", r#"{"id":"b","agent":"impl"}"#),
            ev("UnitStatus", r#"{"id":"b","status":"reviewed"}"#),
        ]);
        let m = metrics::project(&events);
        let state = build_state(&events, &Graph::default(), false).unwrap();
        assert_eq!(state.metrics.review_reject, m.review_reject);
        assert_eq!(state.metrics.review_approve, m.review_approve);
        assert_eq!(
            state.metrics.review_reject, 1,
            "the verified-then-failed loop-back classifies as one reject"
        );
        assert_eq!(state.metrics.review_approve, 1);
    }

    #[test]
    fn events_endpoint_is_since_exclusive() {
        let events = seeded_run();
        let all: serde_json::Value = serde_json::from_str(&events_json(&events, 0)).unwrap();
        assert_eq!(all["events"].as_array().unwrap().len(), events.len());

        let tail: serde_json::Value = serde_json::from_str(&events_json(&events, 4)).unwrap();
        let tail = tail["events"].as_array().unwrap();
        assert_eq!(tail.len(), 2, "since=4 returns only positions 5 and 6");
        assert_eq!(tail[0]["position"], 5);
        assert_eq!(tail[0]["type"], "UnitStatus");
    }

    /// The structural read-only pin: NO mutating endpoint exists. Every write-shaped method,
    /// on every path (including ones that look like write targets), is refused with 405 and
    /// mutates nothing.
    #[test]
    fn no_mutating_endpoint_exists() {
        let events = seeded_run();
        for method in ["POST", "PUT", "DELETE", "PATCH", "HEAD", "OPTIONS"] {
            for path in [
                "/",
                "/api/state",
                "/api/events",
                "/api/units/u1",
                "/api/run",
                "/anything",
            ] {
                let r = route(method, path, &events, &Graph::default());
                assert_eq!(
                    r.status, 405,
                    "{method} {path} must be refused: the dash has no write surface"
                );
            }
        }
    }

    #[test]
    fn unknown_get_path_is_404() {
        let r = route("GET", "/does/not/exist", &[], &Graph::default());
        assert_eq!(r.status, 404);
    }

    #[test]
    fn export_inlines_the_snapshot_as_a_static_page() {
        let events = seeded_run();
        let html = render_export(&events, &Graph::default()).unwrap();
        assert!(
            !html.contains(STATE_PLACEHOLDER),
            "export must resolve the placeholder"
        );
        assert!(
            !html.contains("EMBEDDED_STATE = null"),
            "an export is NOT the live/null page - it carries the snapshot"
        );
        assert!(
            html.contains("\"id\":\"u1\""),
            "the snapshot's unit is inlined into the static page"
        );
        // The static page renders offline: its state carries the event feed.
        assert!(
            html.contains("UnitIntegrated"),
            "the exported feed is inlined so the static page renders without fetching"
        );
    }

    /// Regression (adjudicator-blocked stored XSS): an agent-authored string field - a
    /// finding/decision summary or a raw event payload, all of which flow verbatim into the
    /// exported snapshot's inlined event feed - must never break out of the `<script>`
    /// container. serde_json escapes none of `< > /`, so a payload carrying `</script>` would
    /// close the script element and inject executing markup into the shared export file.
    #[test]
    fn export_neutralizes_a_script_breakout_in_the_inlined_state() {
        // A realistic malicious payload: it inlines verbatim into the feed summary.
        let payload = r#"{"id":"u1","note":"</script><img src=x onerror=alert(1)>"}"#;
        let events = positioned(vec![ev("DecisionMade", payload)]);
        let html = render_export(&events, &Graph::default()).unwrap();

        // The template carries exactly ONE real `</script>` (its own script close). Were the
        // inlined snapshot left raw, the payload's `</script>` would add a second and break the
        // container; neutralization keeps the count at one.
        assert_eq!(
            html.matches("</script>").count(),
            1,
            "the inlined snapshot must carry no raw </script> that escapes the script container"
        );
        // The breakout markup must not survive verbatim anywhere in the file.
        assert!(
            !html.contains("</script><img"),
            "the </script>-prefixed injection must be neutralized, not inlined raw"
        );
        // Neutralized, not dropped: the `<` is escaped to its < JSON form, so the browser
        // still parses the state back to the original string value.
        assert!(
            html.contains(r"\u003c/script\u003e"),
            "the payload's < is escaped to its \\u003c JSON form, preserving the value while defanging the tag"
        );
        // The escaped state is still valid JSON that round-trips to the original string.
        let start = html.find("EMBEDDED_STATE = ").unwrap() + "EMBEDDED_STATE = ".len();
        let rest = &html[start..];
        let end = rest.find(";\n").unwrap();
        let state: serde_json::Value = serde_json::from_str(&rest[..end]).unwrap();
        let feed = state["events"].as_array().unwrap();
        assert!(
            feed.iter().any(|e| e["summary"]
                .as_str()
                .unwrap_or("")
                .contains("</script><img")),
            "the round-tripped value is the original payload, unharmed by the transport escaping"
        );
    }

    #[test]
    fn decision_view_strikes_through_superseded_entries() {
        let node = |id: &str, kind: &str, summary: &str| Node {
            id: id.to_string(),
            kind: kind.to_string(),
            attrs: BTreeMap::from([("summary".to_string(), summary.to_string())]),
        };
        let graph = Graph {
            nodes: vec![
                node("d-new", KIND_DECISION, "the new call"),
                node("d-old", KIND_DECISION, "the old call"),
            ],
            edges: vec![Edge {
                from: "d-new".to_string(),
                to: "d-old".to_string(),
                rel: REL_SUPERSEDES.to_string(),
                valid_from: 0,
                valid_to: None,
                source: 0,
            }],
        };
        let view = build_graph_view(&graph);
        let old = view.decisions.iter().find(|d| d.id == "d-old").unwrap();
        let new = view.decisions.iter().find(|d| d.id == "d-new").unwrap();
        assert!(old.superseded, "a SUPERSEDES target is struck through");
        assert!(!new.superseded, "the superseding decision is not");
    }

    #[test]
    fn graph_seeds_enumerate_unit_decision_and_finding_ids() {
        let events = vec![
            ev("UnitStarted", r#"{"unit":"u1"}"#),
            ev("DecisionMade", r#"{"id":"d1","summary":"x"}"#),
            ev("ReviewFinding", r#"{"id":"f1","by":"sdet"}"#),
            ev("GateVerdict", r#"{"gate":"g","pass":true}"#),
        ];
        let seeds = graph_seeds(&events);
        assert_eq!(
            seeds,
            vec!["d1".to_string(), "f1".to_string(), "u1".to_string()]
        );
    }

    #[test]
    fn build_state_on_an_empty_run_is_empty_not_a_panic() {
        let state = build_state(&[], &Graph::default(), false).unwrap();
        assert!(state.run.units.is_empty());
        assert_eq!(state.metrics.units_started, 0);
        assert_eq!(state.position, 0);
        assert!(state.step.wave.is_empty());
    }

    #[test]
    fn request_line_parsing_extracts_method_and_target() {
        assert_eq!(
            parse_request_line("GET /api/state?since=3 HTTP/1.1"),
            Some(("GET".to_string(), "/api/state?since=3".to_string()))
        );
        assert_eq!(parse_request_line(""), None);
        assert_eq!(parse_request_line("GET"), None);
    }

    #[test]
    fn query_param_reads_since() {
        assert_eq!(query_param("/api/events?since=42", "since"), Some("42"));
        assert_eq!(
            query_param("/api/events?a=1&since=7&b=2", "since"),
            Some("7")
        );
        assert_eq!(query_param("/api/events", "since"), None);
    }

    /// The whole HTTP stack, end to end, against a REAL seeded sqlite store: seed a run,
    /// bind the hand-rolled server on an ephemeral loopback port, drive a real GET over a
    /// TCP socket, and assert the projected JSON comes back. Exercises [`handle_conn`], the
    /// store-reading provider, [`route`], and the response writer together - the literal
    /// "a test drives the JSON endpoints against a seeded store" the done-when calls for.
    #[test]
    fn endpoints_serve_over_a_real_socket_against_a_seeded_store() {
        use crate::conductor;
        use crate::eventstore::namespace::Namespaced;
        use crate::eventstore::sqlite::Store;
        use crate::eventstore::{Direction, EventStore, ExpectedRevision};
        use std::io::{Read, Write};
        use std::net::{TcpListener, TcpStream};

        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("events.db");
        let db_str = db.to_str().unwrap().to_string();
        {
            let backend = Store::open(&db_str).unwrap();
            let store = Namespaced::new(&backend, "proj-dash");
            // Append unpositioned events; the store stamps the real 1-based positions.
            let seed = vec![
                ev("UnitStarted", r#"{"id":"u1","unit":"u1","agent":"impl"}"#),
                ev("UnitStatus", r#"{"id":"u1","status":"reviewed"}"#),
                ev("UnitIntegrated", r#"{"id":"u1","commit":"deadbee"}"#),
            ];
            store
                .append(conductor::STREAM, ExpectedRevision::Any, &seed)
                .unwrap();
        }

        // The same shape of read cmd_dash's provider performs (store -> run events).
        let db_for_provider = db_str.clone();
        let provider = move || -> Result<(Vec<Event>, Graph), String> {
            let backend = Store::open(&db_for_provider).map_err(|e| e.to_string())?;
            let store = Namespaced::new(&backend, "proj-dash");
            let events = store
                .read_stream(conductor::STREAM, 0, Direction::Forward)
                .map_err(|e| e.to_string())?;
            Ok((events, Graph::default()))
        };

        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let addr = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let (conn, _) = listener.accept().unwrap();
            handle_conn(conn, &provider).unwrap();
        });

        let mut client = TcpStream::connect(addr).unwrap();
        client
            .write_all(b"GET /api/state HTTP/1.1\r\nHost: localhost\r\n\r\n")
            .unwrap();
        let mut resp = String::new();
        client.read_to_string(&mut resp).unwrap();
        server.join().unwrap();

        assert!(
            resp.starts_with("HTTP/1.1 200 OK"),
            "state endpoint returns 200:\n{resp}"
        );
        assert!(resp.contains("application/json"), "content type is JSON");
        let body = resp.split("\r\n\r\n").nth(1).expect("a response body");
        let v: serde_json::Value = serde_json::from_str(body).expect("body is JSON");
        assert_eq!(v["run"]["units"][0]["id"], "u1");
        assert_eq!(v["run"]["units"][0]["status"], "integrated");
        assert_eq!(v["metrics"]["review_approve"], 1);
    }

    /// The read-only guard also holds over a real socket: a POST is refused 405 and the
    /// provider is never even consulted (it would panic if called), proving no request can
    /// reach a mutation path.
    #[test]
    fn a_post_over_a_real_socket_is_refused_without_touching_the_store() {
        use std::io::{Read, Write};
        use std::net::{TcpListener, TcpStream};

        let provider = || -> Result<(Vec<Event>, Graph), String> {
            panic!("a non-GET request must never read the store");
        };
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let addr = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let (conn, _) = listener.accept().unwrap();
            handle_conn(conn, &provider).unwrap();
        });

        let mut client = TcpStream::connect(addr).unwrap();
        client
            .write_all(b"POST /api/state HTTP/1.1\r\nHost: localhost\r\nContent-Length: 0\r\n\r\n")
            .unwrap();
        let mut resp = String::new();
        client.read_to_string(&mut resp).unwrap();
        server.join().unwrap();

        assert!(
            resp.starts_with("HTTP/1.1 405"),
            "a write method is refused read-only:\n{resp}"
        );
    }
}
