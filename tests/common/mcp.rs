//! A live rigger stdio session over a project (`rigger mcp`, `rigger serve`, `rigger run
//! --driver workflow`), shared by every suite that drives an MCP server through the real
//! subprocess.

use std::path::Path;
use std::time::{Duration, Instant};

/// A live rigger stdio session over `root`: one request in, one JSON-RPC response line out - a
/// live round trip through the real subprocess, not a batch of requests read back after the
/// process exits.
pub struct McpSession {
    child: std::process::Child,
    stdin: Option<std::process::ChildStdin>,
    stdout: std::io::BufReader<std::process::ChildStdout>,
    next_id: i64,
}

impl McpSession {
    pub fn start(root: &Path) -> Self {
        McpSession::start_with(root, &["mcp"])
    }

    /// A rigger stdio session over `rigger <args>` in `root`, built as every short-lived
    /// integration invocation is ([`super::cli::rigger_command`]): no dash, and the instance
    /// registry isolated under `root`.
    pub fn start_with(root: &Path, args: &[&str]) -> Self {
        use std::process::Stdio;

        let mut child = super::cli::rigger_command(root, args, &[], root)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn a rigger stdio session");
        let stdin = child.stdin.take();
        let stdout = std::io::BufReader::new(child.stdout.take().unwrap());
        McpSession {
            child,
            stdin,
            stdout,
            next_id: 0,
        }
    }

    /// Write one raw `line` and read back the one response line it gets (`why` names the
    /// expectation when none comes).
    pub fn exchange(&mut self, line: &str, why: &str) -> String {
        use std::io::{BufRead, Write};

        let stdin = self
            .stdin
            .as_mut()
            .expect("the session's stdin is still open");
        writeln!(stdin, "{line}").unwrap();
        stdin.flush().unwrap();
        let mut response = String::new();
        self.stdout.read_line(&mut response).expect(why);
        response
    }

    /// One JSON-RPC `method` call with `params`, answered by exactly one JSON response line.
    pub fn call(&mut self, method: &str, params: serde_json::Value) -> serde_json::Value {
        self.next_id += 1;
        let req = serde_json::json!({
            "jsonrpc": "2.0", "id": self.next_id, "method": method, "params": params
        });
        let line = self.exchange(&req.to_string(), "the rigger stdio session must answer");
        serde_json::from_str(&line)
            .unwrap_or_else(|e| panic!("not one JSON-RPC response line ({e}): {line:?}"))
    }

    /// The advertised tool names, from `tools/list`.
    pub fn tool_names(&mut self) -> Vec<String> {
        let list = self.call("tools/list", serde_json::json!({}));
        list["result"]["tools"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["name"].as_str().unwrap().to_string())
            .collect()
    }

    /// `rigger_peers`: each call reads the store afresh from the run's boundary (spec 101), so
    /// the first call already sees every decision recorded before it.
    pub fn peers(&mut self) -> serde_json::Value {
        self.tool_call("rigger_peers", serde_json::json!({}))
    }

    /// One `tools/call` of tool `name` with `arguments`: the whole JSON-RPC response.
    pub fn tool_call(&mut self, name: &str, arguments: serde_json::Value) -> serde_json::Value {
        self.call(
            "tools/call",
            serde_json::json!({"name": name, "arguments": arguments}),
        )
    }

    /// The `initialize` handshake a well-behaved MCP client (the shim's SDK client included)
    /// performs first, once per session; it must succeed.
    pub fn initialize(&mut self) {
        let init = self.call(
            "initialize",
            serde_json::json!({
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": {"name": "rigger-stdio-session-test", "version": "0.0.0"},
            }),
        );
        assert!(
            init.get("result").is_some(),
            "initialize must succeed; got {init}"
        );
    }

    /// Poll `rigger_next` 20 ms apart: `Some(id)` once it hands out a spawn, `None` once it
    /// answers `done: true`. An empty or absent id is no handout and the run still going (the
    /// conductor grounds and enqueues on its own thread), so it polls on; past `deadline` the
    /// session [`fail`](Self::fail)s.
    pub fn next_spawn(&mut self, deadline: Instant) -> Option<String> {
        loop {
            let next = self.tool_call("rigger_next", serde_json::json!({}));
            if let Some(content) = next["result"].get("structuredContent") {
                let id = content["id"].as_str().unwrap_or_default();
                if !id.is_empty() {
                    return Some(id.to_string());
                }
                if content["done"] == serde_json::Value::Bool(true) {
                    return None;
                }
            }
            if Instant::now() >= deadline {
                self.fail(
                    "rigger_next handed out no spawn and never answered done by the deadline",
                );
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    /// End the session's own child by its handle, drain its stderr, wait it, and panic naming
    /// `why` and that stderr.
    pub fn fail(&mut self, why: &str) -> ! {
        use std::io::Read;

        let _ = self.child.kill();
        let mut err = String::new();
        if let Some(mut stderr) = self.child.stderr.take() {
            let _ = stderr.read_to_string(&mut err);
        }
        let _ = self.child.wait();
        panic!("{why}; stderr:\n{err}");
    }

    /// Close stdin - the EOF that lets `mcpserver::Server::run`'s read loop finish and the
    /// process exit, exactly like the shim closing its side of the pipe - and collect the exit.
    pub fn finish(mut self) -> std::process::Output {
        drop(self.stdin.take());
        self.child
            .wait_with_output()
            .expect("the rigger stdio session must exit")
    }
}
