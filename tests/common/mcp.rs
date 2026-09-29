//! A live `rigger mcp` stdio session over a project, shared by every suite that drives the MCP
//! server through the real subprocess.

use std::path::Path;

/// A live `rigger mcp` stdio session over `root`: one request in, one JSON-RPC response line
/// out - a live round trip through the real subprocess, not a batch of requests read back
/// after the process exits.
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

    /// A session over `rigger <args>` (a `rigger mcp` invocation) in `root`.
    pub fn start_with(root: &Path, args: &[&str]) -> Self {
        use std::process::Stdio;

        let mut child = super::rigger_courier()
            .args(args)
            .current_dir(root)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn rigger mcp");
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
        let line = self.exchange(&req.to_string(), "rigger mcp must answer");
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
        self.call(
            "tools/call",
            serde_json::json!({"name": "rigger_peers", "arguments": {}}),
        )
    }

    /// Close stdin - the EOF that lets `mcpserver::Server::run`'s read loop finish and the
    /// process exit, exactly like the shim closing its side of the pipe - and collect the exit.
    pub fn finish(mut self) -> std::process::Output {
        drop(self.stdin.take());
        self.child.wait_with_output().expect("rigger mcp must exit")
    }
}
