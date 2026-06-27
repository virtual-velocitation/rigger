//! The in-Claude-Code agent driver: the conductor stays the orchestrator, its
//! `spawn` calls enqueue spawn requests here, and an in-process MCP server drains
//! them - the workflow shim calls rigger_next to pick up a request, runs the
//! agent via the Workflow tool's `agent()`, and calls rigger_result when done.
//! Agents emit decisions live via the MCP rigger_emit tool (handled by the
//! server, not here), so the emit callback is unused.

use std::collections::{HashMap, VecDeque};
use std::sync::mpsc::{channel, Sender};
use std::sync::Mutex;

use serde::Serialize;
use serde_json::Value;

use crate::conductor::{AgentDriver, AgentResult, Error, SpawnOpts};
use crate::config::AgentDef;

/// What the shim picks up via rigger_next.
#[derive(Clone, Serialize)]
pub struct SpawnRequest {
    pub id: String,
    pub prompt: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub model: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub tools: Vec<String>,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub dir: String,
}

struct Call {
    req: SpawnRequest,
    tx: Sender<Result<AgentResult, Error>>,
}

struct Inner {
    queue: VecDeque<String>,
    pending: HashMap<String, Call>,
    next_id: i64,
}

/// Driver bridges the conductor to a polling MCP server.
pub struct Driver {
    inner: Mutex<Inner>,
}

impl Default for Driver {
    fn default() -> Self {
        Driver {
            inner: Mutex::new(Inner {
                queue: VecDeque::new(),
                pending: HashMap::new(),
                next_id: 0,
            }),
        }
    }
}

impl Driver {
    pub fn new() -> Self {
        Self::default()
    }

    /// Return the next queued spawn request for the shim, or None if none waits.
    pub fn next(&self) -> Option<SpawnRequest> {
        let mut inner = self.inner.lock().unwrap();
        while let Some(id) = inner.queue.pop_front() {
            if let Some(call) = inner.pending.get(&id) {
                return Some(call.req.clone());
            }
        }
        None
    }

    /// Deliver an agent's result to the waiting spawn. A blank `err` means success.
    pub fn result(&self, id: &str, output: String, err: String) {
        let inner = self.inner.lock().unwrap();
        if let Some(call) = inner.pending.get(id) {
            let r = if err.is_empty() {
                Ok(AgentResult { output })
            } else {
                Err(Error(err))
            };
            let _ = call.tx.send(r);
        }
    }
}

impl AgentDriver for Driver {
    fn spawn(
        &self,
        agent: &AgentDef,
        prompt: &str,
        opts: &SpawnOpts,
        _emit: &dyn Fn(&str, Value) -> Result<(), Error>,
    ) -> Result<AgentResult, Error> {
        let (tx, rx) = channel();
        let id = {
            let mut inner = self.inner.lock().unwrap();
            inner.next_id += 1;
            let id = inner.next_id.to_string();
            let req = SpawnRequest {
                id: id.clone(),
                prompt: prompt.to_string(),
                model: agent.model.clone(),
                tools: agent.tools.clone(),
                dir: opts.dir.clone(),
            };
            inner.pending.insert(id.clone(), Call { req, tx });
            inner.queue.push_back(id.clone());
            id
        };
        let result = rx
            .recv()
            .map_err(|_| Error("workflow driver: spawn channel closed".into()));
        self.inner.lock().unwrap().pending.remove(&id);
        result?
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    #[test]
    fn next_and_result_drive_spawn() {
        let driver = Arc::new(Driver::new());
        let d2 = driver.clone();
        let handle = std::thread::spawn(move || {
            let emit = |_: &str, _: Value| Ok(());
            d2.spawn(
                &AgentDef {
                    id: "a".into(),
                    model: "sonnet".into(),
                    ..Default::default()
                },
                "do it",
                &SpawnOpts { dir: String::new() },
                &emit,
            )
        });

        let deadline = Instant::now() + Duration::from_secs(2);
        let req = loop {
            if let Some(r) = driver.next() {
                break r;
            }
            assert!(
                Instant::now() < deadline,
                "rigger_next never returned the spawn"
            );
            std::thread::sleep(Duration::from_millis(1));
        };
        assert_eq!(req.prompt, "do it");
        assert_eq!(req.model, "sonnet");

        driver.result(&req.id, "done".into(), String::new());
        let res = handle.join().unwrap().unwrap();
        assert_eq!(res.output, "done");
    }
}
