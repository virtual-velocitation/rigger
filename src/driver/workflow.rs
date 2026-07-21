//! The in-Claude-Code agent driver: the conductor stays the orchestrator, its
//! `spawn` calls enqueue spawn requests here, and an in-process MCP server drains
//! them - the workflow shim calls rigger_next to pick up a request, runs the
//! agent via the Workflow tool's `agent()`, and calls rigger_result when done.
//! Agents emit decisions live via the MCP rigger_emit tool (handled by the
//! server, not here), so the emit callback is unused. The spawn's wire id IS its
//! deterministic `opts.id`, so the server can stamp those live emits with the id of
//! the spawn it is serving (the per-spawn correlation the verdict-channel-mismatch
//! backstop keys on; the serial shim makes "the spawn being served" unambiguous).

use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicBool, Ordering};
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
    /// The agent's PERSONA - its role instructions (`AgentDef::prompt`), threaded
    /// from the conductor's single persona source (`SpawnOpts::system_prompt`). The
    /// shim passes it to the Agent SDK `query()` as `options.systemPrompt`, so a
    /// workflow agent gets its role exactly as a cli agent does (cli passes the same
    /// persona via `--system-prompt`). Omitted from the wire when empty.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub system_prompt: String,
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
    /// Set once the conductor's `run` has returned. It is the ONLY signal that an
    /// empty `next()` means "the run is over" rather than "nothing is queued yet":
    /// the conductor enqueues spawns asynchronously, so an empty queue early in the
    /// run is transient, not terminal. Without this flag the shim cannot tell a
    /// not-yet-grounded conductor from a finished one and exits prematurely (the
    /// pga race the shim's poll loop hit on the first real e2e run).
    finished: AtomicBool,
}

impl Default for Driver {
    fn default() -> Self {
        Driver {
            inner: Mutex::new(Inner {
                queue: VecDeque::new(),
                pending: HashMap::new(),
                next_id: 0,
            }),
            finished: AtomicBool::new(false),
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

    /// Mark the run finished - the conductor's `run` has returned, so no further
    /// spawns will be enqueued. Called by the composition root after `conductor::run`
    /// completes; flips an empty `next()` from "wait, more may come" to "done".
    pub fn finish(&self) {
        self.finished.store(true, Ordering::SeqCst);
    }

    /// Whether the conductor has finished AND no spawn is left to drain. Only then is
    /// it safe for the shim to exit: a pending or queued spawn after `finish()` (e.g.
    /// a spawn still in flight when the run wound down) must still be served.
    pub fn is_finished(&self) -> bool {
        if !self.finished.load(Ordering::SeqCst) {
            return false;
        }
        let inner = self.inner.lock().unwrap();
        inner.queue.is_empty() && inner.pending.is_empty()
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
                // The in-process MCP result carries no resolved model id (spec 05 line 52
                // sources it from the stepwise `rigger result --meta` path), so it is empty.
                Ok(AgentResult {
                    output,
                    resolved_model: String::new(),
                })
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
            // Use the conductor's DETERMINISTIC spawn id (`opts.id`) as the wire id, so the
            // MCP server can stamp this spawn's live `rigger_emit` calls with it - the
            // per-spawn [`META_SPAWN`](crate::conductor::META_SPAWN) correlation the
            // verdict-channel-mismatch backstop keys on (spec 18, unit 3). The shim serves
            // agents serially and echoes this id back on `rigger_result`, so it doubles as
            // the pending-map key. Fall back to a monotonic counter only when a caller left
            // `opts.id` empty (test-only), preserving the map's unique-key invariant.
            let id = if opts.id.is_empty() {
                inner.next_id += 1;
                inner.next_id.to_string()
            } else {
                opts.id.clone()
            };
            let req = SpawnRequest {
                id: id.clone(),
                prompt: prompt.to_string(),
                // The persona (role) the conductor threaded in via SpawnOpts; the shim
                // passes it to query() as the system prompt, so a workflow agent gets
                // its role exactly as the cli path does.
                system_prompt: opts.system_prompt.clone(),
                // The cascade rung this attempt resolves (spec 10 unit 4): a `model_ladder`
                // agent escalates one rung per remediation attempt, clamped at the last.
                model: agent.model_for_attempt(opts.attempt),
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
                    system_prompt: "You are the rust engineer. Implement findings.".into(),
                    dir: String::new(),
                    isolation: false,
                    parallel: false,
                    blast_radius: vec!["a.rs".into()],
                    ..Default::default()
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
        // The persona (the agent's role) threaded through SpawnOpts reaches the spawn
        // request, so the shim can pass it to query() as the system prompt - a workflow
        // agent gets its role exactly as the cli path does.
        assert_eq!(
            req.system_prompt, "You are the rust engineer. Implement findings.",
            "the spawn request must carry the agent persona (role) to the shim"
        );
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
    fn is_finished_only_after_finish_and_drained() {
        // A fresh driver is not finished (the conductor is still running).
        let driver = Driver::new();
        assert!(
            !driver.is_finished(),
            "a running conductor is never finished"
        );

        // After finish() with nothing pending/queued, it is finished.
        driver.finish();
        assert!(
            driver.is_finished(),
            "after finish() with an empty queue the run is over"
        );
    }

    #[test]
    fn finish_does_not_strand_an_in_flight_spawn() {
        // A spawn that is still pending when finish() is called must keep
        // is_finished() false until it is drained: the shim must still pick it up.
        let driver = Arc::new(Driver::new());
        let d2 = driver.clone();
        let handle = std::thread::spawn(move || {
            let emit = |_: &str, _: Value| Ok(());
            d2.spawn(
                &AgentDef {
                    id: "a".into(),
                    ..Default::default()
                },
                "do it",
                &SpawnOpts {
                    system_prompt: String::new(),
                    dir: String::new(),
                    isolation: false,
                    parallel: false,
                    blast_radius: Vec::new(),
                    ..Default::default()
                },
                &emit,
            )
        });

        // Wait until the spawn is queued.
        let deadline = Instant::now() + Duration::from_secs(2);
        while {
            let inner = driver.inner.lock().unwrap();
            inner.pending.is_empty()
        } {
            assert!(Instant::now() < deadline, "spawn never queued");
            std::thread::sleep(Duration::from_millis(1));
        }

        // finish() fires while the spawn is still pending: NOT finished yet.
        driver.finish();
        assert!(
            !driver.is_finished(),
            "an in-flight spawn after finish() must keep the run from reporting done"
        );

        // Drain it; now the run is finished.
        let req = driver.next().expect("the pending spawn is still served");
        assert!(driver.result(&req.id, "done".into(), String::new()));
        handle.join().unwrap().unwrap();
        assert!(
            driver.is_finished(),
            "once the last spawn drains, the finished run reports done"
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
                    system_prompt: String::new(),
                    dir: String::new(),
                    isolation: true,
                    parallel: false,
                    blast_radius: Vec::new(),
                    ..Default::default()
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
