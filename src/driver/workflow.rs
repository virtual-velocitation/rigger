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
    /// The agent's blast-radius (the grounded seed files). The shim passes it to
    /// rigger_peers to scope the tool-boundary injection of peer decisions (§5.3).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub blast_radius: Vec<String>,
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
    ///
    /// Returns `true` if `id` named a pending spawn and the result was delivered,
    /// `false` if the id was unknown or stale. A `false` must be surfaced to the
    /// caller (not swallowed): a shim reporting a result for a wrong/stale id
    /// otherwise gets silent success while the real spawn blocks forever.
    #[must_use]
    pub fn result(&self, id: &str, output: String, err: String) -> bool {
        let inner = self.inner.lock().unwrap();
        if let Some(call) = inner.pending.get(id) {
            let r = if err.is_empty() {
                Ok(AgentResult { output })
            } else {
                Err(Error(err))
            };
            let _ = call.tx.send(r);
            true
        } else {
            false
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
                // recurse: false strips any fan-out (Agent/Task) tool so the agent
                // cannot spawn sub-agents - runaway-proof by construction (§3.1, §6).
                tools: agent.allowed_tools(),
                dir: opts.dir.clone(),
                // Carried to the shim so it fetches blast-radius-filtered peer
                // decisions and injects them at the tool boundary (§5.3).
                blast_radius: opts.blast_radius.clone(),
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
                &SpawnOpts {
                    dir: String::new(),
                    isolation: false,
                    parallel: false,
                    blast_radius: vec!["a.rs".into()],
                },
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
        assert_eq!(
            req.blast_radius,
            ["a.rs"],
            "the spawn request must carry the blast-radius to the shim"
        );

        assert!(
            driver.result(&req.id, "done".into(), String::new()),
            "a result for a known spawn id must report it was delivered"
        );
        let res = handle.join().unwrap().unwrap();
        assert_eq!(res.output, "done");
    }

    #[test]
    fn result_reports_an_unknown_id() {
        let driver = Driver::new();
        assert!(
            !driver.result("does-not-exist", "out".into(), String::new()),
            "a result for an id that names no pending spawn must report unknown"
        );
    }

    #[test]
    fn recurse_false_strips_fan_out_from_the_spawn_request() {
        let driver = Arc::new(Driver::new());
        let d2 = driver.clone();
        let handle = std::thread::spawn(move || {
            let emit = |_: &str, _: Value| Ok(());
            d2.spawn(
                &AgentDef {
                    id: "impl".into(),
                    tools: vec!["Read".into(), "Agent".into()],
                    recurse: false,
                    ..Default::default()
                },
                "do it",
                &SpawnOpts {
                    dir: String::new(),
                    isolation: true,
                    parallel: false,
                    blast_radius: Vec::new(),
                },
                &emit,
            )
        });

        let deadline = Instant::now() + Duration::from_secs(2);
        let req = loop {
            if let Some(r) = driver.next() {
                break r;
            }
            assert!(Instant::now() < deadline, "rigger_next never returned");
            std::thread::sleep(Duration::from_millis(1));
        };
        assert_eq!(req.tools, ["Read"]);
        assert!(!req.tools.contains(&"Agent".to_string()));
        assert!(driver.result(&req.id, "done".into(), String::new()));
        handle.join().unwrap().unwrap();
    }
}
