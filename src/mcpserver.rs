//! Exposes the conductor's workflow bridge over MCP (JSON-RPC 2.0 on stdio) so a
//! Claude Code workflow shim can drive it: rigger_next picks up the next queued
//! agent spawn, rigger_result reports its outcome, rigger_emit records a decision
//! live to the event store, and rigger_peers lists peers' decisions. A plain
//! newline-delimited stdio loop - no async runtime needed.

use std::io::{BufRead, Write};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde_json::{json, Value};

use crate::driver::workflow::Driver;
use crate::eventstore::{Event, EventStore, ExpectedRevision};
use crate::sidecar::Sidecar;

/// The MCP bridge over the workflow driver, event store, and side-car.
pub struct Server<'a> {
    driver: &'a Driver,
    store: &'a dyn EventStore,
    stream: String,
    peers: &'a Sidecar,
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
        }
    }

    /// Serve MCP over the given streams until the input closes (the shim's stdin).
    pub fn run(&self, input: impl BufRead, mut output: impl Write) -> std::io::Result<()> {
        for line in input.lines() {
            let line = line?;
            if line.trim().is_empty() {
                continue;
            }
            let msg: Value = match serde_json::from_str(&line) {
                Ok(m) => m,
                Err(_) => continue,
            };
            if let Some(response) = self.handle(&msg) {
                writeln!(output, "{response}")?;
                output.flush()?;
            }
        }
        Ok(())
    }

    fn handle(&self, msg: &Value) -> Option<String> {
        let method = msg.get("method")?.as_str()?;
        let id = msg.get("id").cloned();
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
                let id = id?;
                let params = msg.get("params")?;
                let name = params.get("name")?.as_str()?;
                let args = params
                    .get("arguments")
                    .cloned()
                    .unwrap_or_else(|| json!({}));
                Some(self.call_tool(id, name, &args))
            }
            // notifications (initialized, etc.) carry no id and need no response
            _ => None,
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
            Err(e) => err(id, -32603, &e),
        }
    }

    fn tool_next(&self) -> Result<Value, String> {
        match self.driver.next() {
            Some(req) => serde_json::to_value(req).map_err(|e| e.to_string()),
            None => Ok(json!({"id": ""})),
        }
    }

    fn tool_result(&self, args: &Value) -> Result<Value, String> {
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
        self.driver.result(id, output, error);
        Ok(json!({}))
    }

    fn tool_emit(&self, args: &Value) -> Result<Value, String> {
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

        self.store
            .append(&self.stream, ExpectedRevision::Any, &[event])
            .map_err(|e| e.to_string())?;
        Ok(json!({}))
    }

    /// List peers' decisions, optionally scoped to a blast-radius (§5.3). When the
    /// caller passes a `files` array (the agent's blast-radius), only decisions whose
    /// `governs` intersects it come back; absent or empty, every decision does.
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
        json!({"decisions": decisions})
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
        {"name": "rigger_peers", "description": "List the decisions other agents have made so far this run, so you do not work blind to them. Pass `files` (your blast-radius) to scope the result to decisions that touch those files; omit it to see every decision.", "inputSchema": {"type": "object", "properties": {"files": {"type": "array", "items": {"type": "string"}, "description": "The agent's blast-radius: only decisions whose `governs` intersects these files are returned. Omit for all decisions."}}}},
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
}
