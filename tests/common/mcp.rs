//! A live rigger stdio session over a project, shared by every suite that drives the MCP
//! server through the real subprocess.

use std::path::Path;
use std::time::{Duration, Instant};

/// A live rigger stdio session over `root`: one request in, one JSON-RPC response line
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

    /// A session over `rigger <args>` (a rigger stdio session) in `root`, with no dash and
    /// its discovery registry isolated under `root`.
    pub fn start_with(root: &Path, args: &[&str]) -> Self {
        use std::process::Stdio;

        let mut child = super::cli::rigger_command(root, args, &[], root)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn the rigger stdio session");
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

    /// One `tools/call` of `name` with `arguments`: the whole JSON-RPC response, as [`call`]
    /// returns it.
    ///
    /// [`call`]: McpSession::call
    pub fn tool_call(&mut self, name: &str, arguments: serde_json::Value) -> serde_json::Value {
        self.call(
            "tools/call",
            serde_json::json!({"name": name, "arguments": arguments}),
        )
    }

    /// The `initialize` handshake a well-behaved MCP client performs first, once per session.
    pub fn initialize(&mut self) {
        let init = self.call(
            "initialize",
            serde_json::json!({
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": {"name": "rigger-test-session", "version": "0.0.0"},
            }),
        );
        assert!(
            init.get("result").is_some(),
            "initialize must succeed; got {init}"
        );
    }

    /// Poll `rigger_next` 20 ms apart until it hands out a spawn id (`Some`) or reports the run
    /// done (`None`). An answer doing neither is transient - the conductor grounds and enqueues
    /// on its own thread - so it polls on, and at `deadline` the session ends through [`fail`].
    ///
    /// [`fail`]: McpSession::fail
    pub fn next_spawn(&mut self, deadline: Instant) -> Option<String> {
        loop {
            let next = self.tool_call("rigger_next", serde_json::json!({}));
            let answer = &next["result"]["structuredContent"];
            let id = answer["id"].as_str().unwrap_or_default();
            if !id.is_empty() {
                return Some(id.to_string());
            }
            if answer["done"] == true {
                return None;
            }
            if Instant::now() >= deadline {
                self.fail("rigger_next neither handed out a spawn nor reported done in time");
            }
            std::thread::sleep(Duration::from_millis(20));
        }
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

    /// End the session's own child through its handle, collect what it wrote to stderr, and
    /// panic naming `why` and that stderr - the exit for a session whose run can no longer
    /// finish, since a run with a spawn pending never exits on stdin closing.
    ///
    /// The stderr read is bounded, never to end of file: a workflow-driver session auto-starts a
    /// dash that inherits the pipe and outlives the child, so the pipe may never close. The
    /// child is waited first, so everything it wrote is already buffered, and the read stops at
    /// the first quarter second with nothing new.
    pub fn fail(&mut self, why: &str) -> ! {
        use std::io::Read;

        let _ = self.child.kill();
        let _ = self.child.wait();
        let (chunks, drained) = std::sync::mpsc::channel();
        if let Some(mut stderr) = self.child.stderr.take() {
            std::thread::spawn(move || {
                let mut chunk = [0u8; 4096];
                while let Ok(read) = stderr.read(&mut chunk) {
                    if read == 0 || chunks.send(chunk[..read].to_vec()).is_err() {
                        break;
                    }
                }
            });
        }
        let mut err = Vec::new();
        while let Ok(chunk) = drained.recv_timeout(Duration::from_millis(250)) {
            err.extend(chunk);
        }
        panic!("{why}; stderr:\n{}", String::from_utf8_lossy(&err))
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
