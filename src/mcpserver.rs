//! Exposes the conductor's workflow bridge over MCP (JSON-RPC 2.0 on stdio) so a
//! Claude Code workflow shim can drive it: rigger_next picks up the next queued
//! agent spawn, rigger_result reports its outcome, rigger_emit records a decision
//! live to the event store, and rigger_peers lists peers' decisions. A plain
//! newline-delimited stdio loop - no async runtime needed.

use std::io::{BufRead, Write};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde_json::{json, Value};

use crate::contextgraph::Projection;
use crate::driver::workflow::Driver;
use crate::eventstore::{Event, EventStore, ExpectedRevision};
use crate::sidecar::Sidecar;

/// A tool's failure, carrying the JSON-RPC error code to report it with. Most
/// failures are internal (`-32603`); a bad argument (e.g. an unknown spawn id)
/// is invalid-params (`-32602`).
struct ToolError {
    code: i64,
    message: String,
}

impl ToolError {
    /// An internal error (`-32603`): something went wrong server-side.
    fn internal(message: impl Into<String>) -> Self {
        ToolError {
            code: -32603,
            message: message.into(),
        }
    }

    /// An invalid-params error (`-32602`): the caller's arguments were wrong
    /// (e.g. a stale/unknown spawn id, a missing required field).
    fn invalid_params(message: impl Into<String>) -> Self {
        ToolError {
            code: -32602,
            message: message.into(),
        }
    }
}

impl From<String> for ToolError {
    fn from(message: String) -> Self {
        ToolError::internal(message)
    }
}

impl From<&str> for ToolError {
    fn from(message: &str) -> Self {
        ToolError::internal(message)
    }
}

/// The MCP bridge over the workflow driver, event store, side-car, and (optionally)
/// the context-graph projector.
pub struct Server<'a> {
    driver: &'a Driver,
    store: &'a dyn EventStore,
    stream: String,
    peers: &'a Sidecar,
    /// The live context-graph projector. When set, an emitted event is folded into
    /// the graph the moment it is appended - so a ReviewFinding (or DecisionMade) an
    /// agent emits via rigger_emit becomes retrievable through `graph_context` by the
    /// agents that ground afterwards (the adversary / adjudicator). Without this, the
    /// workflow-driver path would write findings only to the log and the side-car, and
    /// the graph - the system's cross-agent memory - would never see them.
    graph: Option<&'a dyn Projection>,
}

impl<'a> Server<'a> {
    pub fn new(
        driver: &'a Driver,
        store: &'a dyn EventStore,
        stream: &str,
        peers: &'a Sidecar,
    ) -> Self {
        Server {
            driver,
            store,
            stream: stream.to_string(),
            peers,
            graph: None,
        }
    }

    /// Wire the live context-graph projector so emitted events fold into the graph as
    /// they are appended (the workflow-driver path's bridge from rigger_emit to the
    /// graph, mirroring the conductor's own `emit_with_actor`).
    pub fn with_graph(mut self, graph: &'a dyn Projection) -> Self {
        self.graph = Some(graph);
        self
    }

    /// Serve MCP over the given streams until the input closes (the shim's stdin).
    pub fn run(&self, input: impl BufRead, mut output: impl Write) -> std::io::Result<()> {
        for line in input.lines() {
            let line = line?;
            if line.trim().is_empty() {
                continue;
            }
            // Unparseable input is a JSON-RPC parse error (-32700): reply rather
            // than `continue`, which would silently drop the message and hang a
            // client that is waiting for a response. The id is null because we
            // could not parse the message to recover it (spec §5.1).
            let response = match serde_json::from_str::<Value>(&line) {
                Ok(msg) => self.handle(&msg),
                Err(_) => Some(err(Value::Null, -32700, "parse error")),
            };
            if let Some(response) = response {
                writeln!(output, "{response}")?;
                output.flush()?;
            }
        }
        Ok(())
    }

    fn handle(&self, msg: &Value) -> Option<String> {
        // A JSON-RPC request MUST carry a string `method`. A notification is a
        // request with no `id` and needs no response; a request with an `id` but
        // no usable `method` is malformed and gets an Invalid Request error
        // (-32600), echoing the id when present (spec §4 / §5.1).
        let id = msg.get("id").cloned();
        let method = match msg.get("method").and_then(Value::as_str) {
            Some(m) => m,
            None => {
                return Some(err(
                    id.unwrap_or(Value::Null),
                    -32600,
                    "invalid request: missing method",
                ));
            }
        };
        match method {
            "initialize" => id.map(|id| {
                ok(
                    id,
                    json!({
                        "protocolVersion": "2024-11-05",
                        "capabilities": {"tools": {}},
                        "serverInfo": {"name": "rigger", "version": "0.1.0"},
                    }),
                )
            }),
            "tools/list" => id.map(|id| ok(id, json!({"tools": tool_list()}))),
            "tools/call" => {
                // tools/call is a request, so it must carry an id; without one it
                // is treated as a malformed notification and dropped.
                let id = id?;
                // A tools/call missing params or the tool name is an Invalid
                // Params error (-32602): reply rather than drop, so the client is
                // not left hanging on a request it can never get an answer to.
                let name = match msg
                    .get("params")
                    .and_then(|p| p.get("name"))
                    .and_then(Value::as_str)
                {
                    Some(n) => n,
                    None => {
                        return Some(err(
                            id,
                            -32602,
                            "invalid params: tools/call requires params.name",
                        ));
                    }
                };
                let args = msg
                    .get("params")
                    .and_then(|p| p.get("arguments"))
                    .cloned()
                    .unwrap_or_else(|| json!({}));
                Some(self.call_tool(id, name, &args))
            }
            // Other methods with an id are unknown requests (-32601); methods
            // without an id are notifications (initialized, etc.) and stay silent.
            _ => id.map(|id| err(id, -32601, &format!("method not found: {method}"))),
        }
    }

    fn call_tool(&self, id: Value, name: &str, args: &Value) -> String {
        let result = match name {
            "rigger_next" => self.tool_next(),
            "rigger_result" => self.tool_result(args),
            "rigger_emit" => self.tool_emit(args),
            "rigger_peers" => Ok(self.tool_peers(args)),
            _ => return err(id, -32602, &format!("unknown tool {name}")),
        };
        match result {
            Ok(structured) => ok(
                id,
                json!({
                    "content": [{"type": "text", "text": structured.to_string()}],
                    "structuredContent": structured,
                }),
            ),
            // A tool may request a specific JSON-RPC code (e.g. -32602 for an
            // unknown spawn id); otherwise an internal error is -32603.
            Err(e) => err(id, e.code, &e.message),
        }
    }

    fn tool_next(&self) -> Result<Value, ToolError> {
        match self.driver.next() {
            Some(req) => serde_json::to_value(req).map_err(|e| ToolError::internal(e.to_string())),
            // An empty id means "no spawn right now". `done` disambiguates the two
            // cases the shim cannot otherwise tell apart: `done:true` means the
            // conductor has finished and the shim should exit; `done:false` means the
            // conductor is still running (grounding, or between waves) and the shim
            // must poll again rather than exit. Without `done`, an early empty `next`
            // looks identical to a finished run and the shim exits before the first
            // spawn is even enqueued.
            None => Ok(json!({"id": "", "done": self.driver.is_finished()})),
        }
    }

    fn tool_result(&self, args: &Value) -> Result<Value, ToolError> {
        let id = args.get("id").and_then(Value::as_str).unwrap_or_default();
        let output = args
            .get("output")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let error = args
            .get("error")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        // An unknown / stale id means no spawn is waiting on this result. Report
        // it as invalid-params rather than swallowing it: silent success here
        // would leave the real spawn blocked forever (the shim thinks it
        // delivered a result it never did).
        if self.driver.result(id, output, error) {
            Ok(json!({}))
        } else {
            Err(ToolError::invalid_params(format!(
                "unknown spawn id {id:?}"
            )))
        }
    }

    fn tool_emit(&self, args: &Value) -> Result<Value, ToolError> {
        let typ = args
            .get("type")
            .and_then(Value::as_str)
            .ok_or("rigger_emit: missing type")?;
        let data = args.get("data").cloned().unwrap_or_else(|| json!({}));
        let bytes = serde_json::to_vec(&data).map_err(|e| e.to_string())?;

        // The actor metadata stamps the DECIDED edge; valid_from sets the
        // bi-temporal validity (§6). Both are optional builder overrides.
        let mut event = Event::new(typ, bytes);
        if let Some(meta) = args.get("meta").and_then(Value::as_object) {
            for (k, v) in meta {
                let v = v
                    .as_str()
                    .ok_or_else(|| format!("rigger_emit: meta value for {k:?} must be a string"))?;
                event = event.with_meta(k, v);
            }
        }
        if let Some(vf) = args.get("valid_from") {
            event = event.with_valid_from(parse_valid_from(vf)?);
        }

        let pos = self
            .store
            .append(
                &self.stream,
                ExpectedRevision::Any,
                std::slice::from_ref(&event),
            )
            .map_err(|e| e.to_string())?;
        // Fold the appended event into the live graph (when wired), so a ReviewFinding
        // or DecisionMade an agent emits becomes retrievable through `graph_context` by
        // the agents that ground afterwards - the graph is the cross-agent memory the
        // review tiers communicate through. Best-effort: a fold failure must not fail
        // the emit, which already landed durably in the log.
        if let Some(g) = self.graph {
            let mut folded = event;
            folded.position = pos;
            let _ = g.apply(&folded);
        }
        Ok(json!({}))
    }

    /// List peers' decisions AND review findings, optionally scoped to a blast-radius
    /// (§5.3). When the caller passes a `files` array (the agent's blast-radius), only
    /// decisions whose `governs` intersects it and findings whose `about` intersects
    /// it come back; absent or empty, every decision and finding does. The findings
    /// are how concurrent review lenses see each other's findings LIVE, before any of
    /// them grounds again - the same side-car channel that surfaces peer decisions.
    fn tool_peers(&self, args: &Value) -> Value {
        let files: Vec<String> = args
            .get("files")
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default();
        let decisions: Vec<Value> = self
            .peers
            .decisions_for(&files)
            .iter()
            .map(|d| json!({"id": d.id, "summary": d.summary, "governs": d.governs}))
            .collect();
        let findings: Vec<Value> = self
            .peers
            .findings_for(&files)
            .iter()
            .map(|f| json!({"id": f.id, "by": f.by, "summary": f.summary, "about": f.about}))
            .collect();
        json!({"decisions": decisions, "findings": findings})
    }
}

/// Parse a `valid_from` argument into a [`SystemTime`]: a JSON integer of unix
/// nanoseconds, or an RFC3339 string (the common `YYYY-MM-DDTHH:MM:SS[.fff][Z|±HH:MM]`
/// forms). The integer-nanos form is the canonical one (§6).
fn parse_valid_from(v: &Value) -> Result<SystemTime, String> {
    if let Some(nanos) = v.as_i64() {
        return nanos_to_time(nanos);
    }
    if let Some(s) = v.as_str() {
        // Allow a bare integer-as-string too, then fall through to RFC3339.
        if let Ok(nanos) = s.parse::<i64>() {
            return nanos_to_time(nanos);
        }
        return rfc3339_to_time(s);
    }
    Err("rigger_emit: valid_from must be unix-nanos (integer) or an RFC3339 string".into())
}

/// Convert unix nanoseconds (which may be negative, i.e. before the epoch) to a
/// [`SystemTime`].
fn nanos_to_time(nanos: i64) -> Result<SystemTime, String> {
    if nanos >= 0 {
        Ok(UNIX_EPOCH + Duration::from_nanos(nanos as u64))
    } else {
        Ok(UNIX_EPOCH - Duration::from_nanos((-nanos) as u64))
    }
}

/// A dependency-free RFC3339 parser for the common forms: a `YYYY-MM-DD` date, a
/// `T`/space separator, an `HH:MM:SS` time, optional fractional seconds, and a
/// `Z` or `±HH:MM` offset.
fn rfc3339_to_time(s: &str) -> Result<SystemTime, String> {
    let bad = || format!("rigger_emit: invalid RFC3339 valid_from {s:?}");
    let bytes = s.as_bytes();
    if bytes.len() < 19 {
        return Err(bad());
    }
    let num = |a: usize, b: usize| -> Result<i64, String> {
        s.get(a..b)
            .and_then(|p| p.parse::<i64>().ok())
            .ok_or_else(bad)
    };
    let (year, month, day) = (num(0, 4)?, num(5, 7)?, num(8, 10)?);
    let (hour, min, sec) = (num(11, 13)?, num(14, 16)?, num(17, 19)?);
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return Err(bad());
    }

    let rest = &s[19..];
    // Optional fractional seconds.
    let mut idx = 0;
    let rest_bytes = rest.as_bytes();
    let mut nanos_frac: u64 = 0;
    if rest_bytes.first() == Some(&b'.') {
        idx = 1;
        let frac_start = idx;
        while idx < rest_bytes.len() && rest_bytes[idx].is_ascii_digit() {
            idx += 1;
        }
        let mut frac = rest[frac_start..idx].to_string();
        if frac.is_empty() {
            return Err(bad());
        }
        frac.truncate(9);
        while frac.len() < 9 {
            frac.push('0');
        }
        nanos_frac = frac.parse::<u64>().map_err(|_| bad())?;
    }

    // Timezone offset: Z, +HH:MM, or -HH:MM.
    let tz = &rest[idx..];
    let offset_secs: i64 = match tz {
        "Z" | "z" => 0,
        _ => {
            let sign = match tz.as_bytes().first() {
                Some(b'+') => 1,
                Some(b'-') => -1,
                _ => return Err(bad()),
            };
            if tz.len() < 6 {
                return Err(bad());
            }
            let oh: i64 = tz.get(1..3).and_then(|p| p.parse().ok()).ok_or_else(bad)?;
            let om: i64 = tz.get(4..6).and_then(|p| p.parse().ok()).ok_or_else(bad)?;
            sign * (oh * 3600 + om * 60)
        }
    };

    let days = days_from_civil(year, month as u32, day as u32);
    let secs = days * 86_400 + hour * 3600 + min * 60 + sec - offset_secs;
    let total_nanos = secs * 1_000_000_000 + nanos_frac as i64;
    nanos_to_time(total_nanos)
}

/// Days since the unix epoch (1970-01-01) for a civil (proleptic Gregorian) date,
/// per Howard Hinnant's `days_from_civil` algorithm. Works for dates before the
/// epoch (negative result).
fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400; // [0, 399]
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) as i64 + 2) / 5 + d as i64 - 1; // [0, 365]
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy; // [0, 146096]
    era * 146_097 + doe - 719_468
}

fn tool_list() -> Value {
    json!([
        {"name": "rigger_next", "description": "Pick up the next queued agent spawn. The id is empty when nothing is waiting.", "inputSchema": {"type": "object", "properties": {}}},
        {"name": "rigger_result", "description": "Report an agent's final result by spawn id.", "inputSchema": {"type": "object", "properties": {"id": {"type": "string"}, "output": {"type": "string"}, "error": {"type": "string"}}, "required": ["id"]}},
        {"name": "rigger_emit", "description": "Record a decision on the shared event log, live, so other agents see it immediately. Optionally set meta (e.g. the acting agent, which stamps the graph's DECIDED edge) and valid_from (the bi-temporal time the fact became true).", "inputSchema": {"type": "object", "properties": {"type": {"type": "string"}, "data": {"type": "object"}, "meta": {"type": "object", "description": "Metadata entries (string->string), e.g. {\"actor\": \"<agent-id>\"}.", "additionalProperties": {"type": "string"}}, "valid_from": {"description": "When the fact became true: unix nanoseconds (integer) or an RFC3339 timestamp string.", "type": ["integer", "string"]}}, "required": ["type", "data"]}},
        {"name": "rigger_peers", "description": "List the decisions AND review findings other agents have raised so far this run, so you do not work blind to them (concurrent reviewers see each other's findings live). Pass `files` (your blast-radius) to scope the result to decisions and findings that touch those files; omit it to see every one.", "inputSchema": {"type": "object", "properties": {"files": {"type": "array", "items": {"type": "string"}, "description": "The agent's blast-radius: only decisions whose `governs`, and findings whose `about`, intersect these files are returned. Omit for all."}}}},
    ])
}

fn ok(id: Value, result: Value) -> String {
    json!({"jsonrpc": "2.0", "id": id, "result": result}).to_string()
}

fn err(id: Value, code: i64, message: &str) -> String {
    json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": message}}).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eventstore::sqlite::Store;
    use crate::eventstore::{Direction, Filter};
    use std::io::Cursor;

    #[test]
    fn emit_tool_appends_to_the_store() {
        let store = Store::open(":memory:").unwrap();
        let driver = Driver::new();
        let peers = Sidecar::start(&store, 0, Filter::default()).unwrap();
        let server = Server::new(&driver, &store, "run", &peers);

        let input = r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"rigger_emit","arguments":{"type":"DecisionMade","data":{"id":"d1","summary":"x"}}}}"#;
        let mut output = Vec::new();
        server.run(Cursor::new(input), &mut output).unwrap();

        let events = store
            .read_all(0, Direction::Forward, &Filter::default())
            .unwrap();
        assert!(events.iter().any(|e| e.type_ == "DecisionMade"));
        let resp: Value = serde_json::from_str(String::from_utf8(output).unwrap().trim()).unwrap();
        assert_eq!(resp["id"], 1);
        assert!(resp.get("result").is_some());
    }

    #[test]
    fn emit_tool_folds_a_review_finding_into_the_wired_graph() {
        // The workflow-driver path's bridge from rigger_emit to the graph: when a
        // graph is wired, a ReviewFinding an agent emits folds into a KIND_FINDING node
        // the moment it lands, so an agent that grounds afterwards retrieves it via
        // graph_context (not via the conductor hand-threading prompts).
        use crate::contextgraph::{self, sqlite::Projector, Projection};

        let store = Store::open(":memory:").unwrap();
        let driver = Driver::new();
        let peers = Sidecar::start(&store, 0, Filter::default()).unwrap();
        let graph = Projector::open(":memory:").unwrap();
        let server = Server::new(&driver, &store, "run", &peers).with_graph(&graph);

        let input = r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"rigger_emit","arguments":{"type":"ReviewFinding","data":{"id":"f1","summary":"skips the buffer authority","about":["combat.rs"]},"meta":{"actor":"tech-lens"}}}}"#;
        let mut output = Vec::new();
        server.run(Cursor::new(input), &mut output).unwrap();

        // The finding folded into the graph, reachable from the file it is ABOUT.
        let g = graph.subgraph(&["combat.rs".to_string()], 2).unwrap();
        let n = g
            .nodes
            .iter()
            .find(|n| n.id == "f1")
            .expect("the emitted ReviewFinding must fold into the wired graph");
        assert_eq!(n.kind, contextgraph::KIND_FINDING);
        assert_eq!(
            n.attrs.get("summary").map(String::as_str),
            Some("skips the buffer authority")
        );
        assert!(
            g.edges
                .iter()
                .any(|e| e.rel == contextgraph::REL_RAISED && e.from == "tech-lens"),
            "the actor must be the RAISED source of the folded finding"
        );
    }

    #[test]
    fn emit_tool_carries_meta_actor() {
        let store = Store::open(":memory:").unwrap();
        let driver = Driver::new();
        let peers = Sidecar::start(&store, 0, Filter::default()).unwrap();
        let server = Server::new(&driver, &store, "run", &peers);

        let input = r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"rigger_emit","arguments":{"type":"DecisionMade","data":{"id":"d1"},"meta":{"actor":"a7"}}}}"#;
        let mut output = Vec::new();
        server.run(Cursor::new(input), &mut output).unwrap();

        let events = store
            .read_all(0, Direction::Forward, &Filter::default())
            .unwrap();
        let e = events
            .iter()
            .find(|e| e.type_ == "DecisionMade")
            .expect("stored the emitted event");
        assert_eq!(e.meta.get("actor").map(String::as_str), Some("a7"));
    }

    #[test]
    fn emit_tool_sets_valid_from_from_nanos() {
        let store = Store::open(":memory:").unwrap();
        let driver = Driver::new();
        let peers = Sidecar::start(&store, 0, Filter::default()).unwrap();
        let server = Server::new(&driver, &store, "run", &peers);

        // 2_000_000_000 ns = 2 seconds after the unix epoch.
        let input = r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"rigger_emit","arguments":{"type":"DecisionMade","data":{},"valid_from":2000000000}}}"#;
        let mut output = Vec::new();
        server.run(Cursor::new(input), &mut output).unwrap();

        let events = store
            .read_all(0, Direction::Forward, &Filter::default())
            .unwrap();
        let e = events
            .iter()
            .find(|e| e.type_ == "DecisionMade")
            .expect("stored the emitted event");
        assert_eq!(
            e.valid_from,
            UNIX_EPOCH + Duration::from_nanos(2_000_000_000)
        );
    }

    #[test]
    fn rfc3339_valid_from_parses_to_epoch_seconds() {
        // 1970-01-01T00:00:02Z is two seconds after the epoch.
        let t = parse_valid_from(&json!("1970-01-01T00:00:02Z")).unwrap();
        assert_eq!(t, UNIX_EPOCH + Duration::from_secs(2));
        // A real-world timestamp with an offset.
        let z = parse_valid_from(&json!("2021-01-01T00:00:00Z")).unwrap();
        let off = parse_valid_from(&json!("2021-01-01T01:00:00+01:00")).unwrap();
        assert_eq!(z, off, "the offset is applied back to UTC");
    }

    #[test]
    fn peers_tool_scopes_to_the_files_arg() {
        use std::time::Instant;

        let store = Store::open(":memory:").unwrap();
        // Two decisions, one touching a.rs, one touching b.rs, on the run stream.
        for (id, governs) in [("da", "a.rs"), ("db", "b.rs")] {
            let data = serde_json::to_vec(&serde_json::json!({
                "id": id, "summary": "x", "governs": [governs],
            }))
            .unwrap();
            store
                .append(
                    "run",
                    ExpectedRevision::Any,
                    &[Event::new(crate::contextgraph::TYPE_DECISION_MADE, data)],
                )
                .unwrap();
        }

        let driver = Driver::new();
        let peers = Sidecar::start(&store, 0, Filter::default()).unwrap();
        // Wait for the side-car to catch up on both decisions.
        let deadline = Instant::now() + Duration::from_secs(2);
        while peers.decisions().len() < 2 {
            assert!(Instant::now() < deadline, "side-car never caught up");
            std::thread::sleep(Duration::from_millis(10));
        }
        let server = Server::new(&driver, &store, "run", &peers);

        let input = r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"rigger_peers","arguments":{"files":["a.rs"]}}}"#;
        let mut output = Vec::new();
        server.run(Cursor::new(input), &mut output).unwrap();

        let resp: Value = serde_json::from_str(String::from_utf8(output).unwrap().trim()).unwrap();
        let decisions = &resp["result"]["structuredContent"]["decisions"];
        let arr = decisions.as_array().expect("decisions array");
        assert_eq!(arr.len(), 1, "files=[a.rs] returns only the a.rs decision");
        assert_eq!(arr[0]["id"], "da");
    }

    #[test]
    fn peers_tool_surfaces_findings_scoped_to_the_files_arg() {
        // Item 4: rigger_peers surfaces peer review FINDINGS as well as decisions, so a
        // concurrent reviewer scoped to its files sees a finding about one of them.
        use std::time::Instant;

        let store = Store::open(":memory:").unwrap();
        // Two review findings, one about a.rs, one about b.rs, on the run stream.
        for (id, about) in [("fa", "a.rs"), ("fb", "b.rs")] {
            let data = serde_json::to_vec(&serde_json::json!({
                "id": id, "by": "lensA", "summary": "x", "about": [about],
            }))
            .unwrap();
            store
                .append(
                    "run",
                    ExpectedRevision::Any,
                    &[Event::new(crate::contextgraph::TYPE_REVIEW_FINDING, data)],
                )
                .unwrap();
        }

        let driver = Driver::new();
        let peers = Sidecar::start(&store, 0, Filter::default()).unwrap();
        // Wait for the side-car to catch up on both findings.
        let deadline = Instant::now() + Duration::from_secs(2);
        while peers.findings().len() < 2 {
            assert!(Instant::now() < deadline, "side-car never caught up");
            std::thread::sleep(Duration::from_millis(10));
        }
        let server = Server::new(&driver, &store, "run", &peers);

        let input = r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"rigger_peers","arguments":{"files":["a.rs"]}}}"#;
        let mut output = Vec::new();
        server.run(Cursor::new(input), &mut output).unwrap();

        let resp: Value = serde_json::from_str(String::from_utf8(output).unwrap().trim()).unwrap();
        let findings = &resp["result"]["structuredContent"]["findings"];
        let arr = findings.as_array().expect("findings array");
        assert_eq!(
            arr.len(),
            1,
            "files=[a.rs] returns only the a.rs finding: {resp}"
        );
        assert_eq!(arr[0]["id"], "fa");
        assert_eq!(arr[0]["by"], "lensA");
    }

    #[test]
    fn next_reports_done_only_after_the_conductor_finishes() {
        // rigger_next must distinguish "nothing queued yet" (done:false, keep
        // polling) from "the run is over" (done:true, exit). Before finish() an
        // empty next is done:false; after finish() it is done:true. This is what
        // stops the shim exiting before the first spawn is even enqueued.
        let store = Store::open(":memory:").unwrap();
        let driver = Driver::new();
        let peers = Sidecar::start(&store, 0, Filter::default()).unwrap();
        let server = Server::new(&driver, &store, "run", &peers);

        let call = r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"rigger_next","arguments":{}}}"#;

        // Before finish: empty id, done:false.
        let mut out = Vec::new();
        server.run(Cursor::new(call), &mut out).unwrap();
        let resp: Value = serde_json::from_str(String::from_utf8(out).unwrap().trim()).unwrap();
        let sc = &resp["result"]["structuredContent"];
        assert_eq!(sc["id"], "", "no spawn queued yet");
        assert_eq!(sc["done"], false, "a running conductor is not done: {resp}");

        // After finish: empty id, done:true.
        driver.finish();
        let mut out = Vec::new();
        server.run(Cursor::new(call), &mut out).unwrap();
        let resp: Value = serde_json::from_str(String::from_utf8(out).unwrap().trim()).unwrap();
        let sc = &resp["result"]["structuredContent"];
        assert_eq!(sc["id"], "", "still no spawn");
        assert_eq!(
            sc["done"], true,
            "a finished conductor reports done so the shim exits: {resp}"
        );
    }

    #[test]
    fn initialize_advertises_tools() {
        let store = Store::open(":memory:").unwrap();
        let driver = Driver::new();
        let peers = Sidecar::start(&store, 0, Filter::default()).unwrap();
        let server = Server::new(&driver, &store, "run", &peers);

        let input = "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"initialize\"}\n\
                     {\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"tools/list\"}";
        let mut output = Vec::new();
        server.run(Cursor::new(input), &mut output).unwrap();
        let text = String::from_utf8(output).unwrap();
        assert!(text.contains("rigger_next") && text.contains("rigger_emit"));
    }

    #[test]
    fn rigger_result_for_an_unknown_id_is_an_error() {
        let store = Store::open(":memory:").unwrap();
        let driver = Driver::new();
        let peers = Sidecar::start(&store, 0, Filter::default()).unwrap();
        let server = Server::new(&driver, &store, "run", &peers);

        // No spawn is pending, so id "999" is unknown. The shim must get an
        // error, not a silent success that would block the conductor forever.
        let input = r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"rigger_result","arguments":{"id":"999","output":"done"}}}"#;
        let mut output = Vec::new();
        server.run(Cursor::new(input), &mut output).unwrap();

        let resp: Value = serde_json::from_str(String::from_utf8(output).unwrap().trim()).unwrap();
        assert_eq!(resp["id"], 1);
        assert_eq!(
            resp["error"]["code"], -32602,
            "an unknown spawn id must be an invalid-params error: {resp}"
        );
        assert!(resp.get("result").is_none(), "no success result: {resp}");
    }

    #[test]
    fn malformed_json_gets_a_parse_error() {
        let store = Store::open(":memory:").unwrap();
        let driver = Driver::new();
        let peers = Sidecar::start(&store, 0, Filter::default()).unwrap();
        let server = Server::new(&driver, &store, "run", &peers);

        // Unparseable input must not be silently dropped (which hangs the client):
        // it gets a -32700 parse error with a null id.
        let input = "{not valid json";
        let mut output = Vec::new();
        server.run(Cursor::new(input), &mut output).unwrap();

        let resp: Value = serde_json::from_str(String::from_utf8(output).unwrap().trim()).unwrap();
        assert_eq!(
            resp["error"]["code"], -32700,
            "unparseable input must be a parse error: {resp}"
        );
        assert_eq!(
            resp["id"],
            Value::Null,
            "parse error echoes a null id: {resp}"
        );
    }

    #[test]
    fn request_missing_method_gets_an_invalid_request_error() {
        let store = Store::open(":memory:").unwrap();
        let driver = Driver::new();
        let peers = Sidecar::start(&store, 0, Filter::default()).unwrap();
        let server = Server::new(&driver, &store, "run", &peers);

        // A well-formed JSON object that is not a valid JSON-RPC request (no
        // method) must get an Invalid Request error echoing its id, not silence.
        let input = r#"{"jsonrpc":"2.0","id":7,"params":{}}"#;
        let mut output = Vec::new();
        server.run(Cursor::new(input), &mut output).unwrap();

        let resp: Value = serde_json::from_str(String::from_utf8(output).unwrap().trim()).unwrap();
        assert_eq!(resp["id"], 7, "the error echoes the request id: {resp}");
        assert_eq!(
            resp["error"]["code"], -32600,
            "a request with no method is an invalid request: {resp}"
        );
    }

    #[test]
    fn tools_call_missing_name_gets_an_invalid_params_error() {
        let store = Store::open(":memory:").unwrap();
        let driver = Driver::new();
        let peers = Sidecar::start(&store, 0, Filter::default()).unwrap();
        let server = Server::new(&driver, &store, "run", &peers);

        // A tools/call missing params.name must get an invalid-params error, not
        // be dropped (which would hang the client awaiting a response).
        let input = r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{}}"#;
        let mut output = Vec::new();
        server.run(Cursor::new(input), &mut output).unwrap();

        let resp: Value = serde_json::from_str(String::from_utf8(output).unwrap().trim()).unwrap();
        assert_eq!(resp["id"], 3);
        assert_eq!(
            resp["error"]["code"], -32602,
            "tools/call without params.name is invalid params: {resp}"
        );
    }
}
