//! Exposes the conductor's workflow bridge over MCP (JSON-RPC 2.0 on stdio) so a
//! Claude Code workflow shim can drive it: rigger_next picks up the next queued
//! agent spawn, rigger_result reports its outcome, rigger_emit records a decision
//! live to the event store, and rigger_peers lists peers' decisions. A plain
//! newline-delimited stdio loop - no async runtime needed.

use std::io::{BufRead, Write};

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
            "rigger_peers" => Ok(self.tool_peers()),
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
        self.store
            .append(
                &self.stream,
                ExpectedRevision::Any,
                &[Event::new(typ, bytes)],
            )
            .map_err(|e| e.to_string())?;
        Ok(json!({}))
    }

    fn tool_peers(&self) -> Value {
        let decisions: Vec<Value> = self
            .peers
            .decisions()
            .iter()
            .map(|d| json!({"id": d.id, "summary": d.summary, "governs": d.governs}))
            .collect();
        json!({"decisions": decisions})
    }
}

fn tool_list() -> Value {
    json!([
        {"name": "rigger_next", "description": "Pick up the next queued agent spawn. The id is empty when nothing is waiting.", "inputSchema": {"type": "object", "properties": {}}},
        {"name": "rigger_result", "description": "Report an agent's final result by spawn id.", "inputSchema": {"type": "object", "properties": {"id": {"type": "string"}, "output": {"type": "string"}, "error": {"type": "string"}}, "required": ["id"]}},
        {"name": "rigger_emit", "description": "Record a decision on the shared event log, live, so other agents see it immediately.", "inputSchema": {"type": "object", "properties": {"type": {"type": "string"}, "data": {"type": "object"}}, "required": ["type", "data"]}},
        {"name": "rigger_peers", "description": "List the decisions other agents have made so far this run, so you do not work blind to them.", "inputSchema": {"type": "object", "properties": {}}},
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
