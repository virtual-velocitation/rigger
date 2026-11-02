//! The in-Claude-Code agent driver: the conductor stays the orchestrator, its
//! `spawn` calls enqueue spawn requests here, and an in-process MCP server drains
//! them - the workflow shim calls rigger_next to pick up a request, runs the
//! agent via the Workflow tool's `agent()`, and calls rigger_result when done.
//! Agents emit decisions live via the MCP rigger_emit tool (handled by the
//! server, not here), so the emit callback is unused. The spawn's wire id IS its
//! deterministic `opts.id`, so the server can stamp those live emits with the id of
//! the spawn it is serving (the per-spawn correlation the verdict-channel-mismatch
//! backstop keys on; the serial shim makes "the spawn being served" unambiguous).
//!
//! On `rigger serve` the launch is the handout (spec 105): [`Driver::next`] hands out no queued
//! spawn while the run is held, reading the hold through the read the composition root injects
//! ([`Driver::with_hold_read`]), so the shim keeps polling and the request stays pending. Once the
//! session closes ([`Driver::close_session`]) nothing serves a spawn, so every pending spawn
//! returns a park to its caller and every later spawn returns one at once, and no spawn blocks on
//! a session that is gone.

use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Sender};
use std::sync::Mutex;

use serde::Serialize;
use serde_json::Value;

use crate::agent::{parked_spawn, AgentDriver, AgentResult, Error, SpawnOpts};
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
    /// persona through the file `--system-prompt-file` names). Omitted from the wire when
    /// empty.
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
pub struct Driver<'a> {
    inner: Mutex<Inner>,
    /// Set once the conductor's `run` has returned. It is the ONLY signal that an
    /// empty `next()` means "the run is over" rather than "nothing is queued yet":
    /// the conductor enqueues spawns asynchronously, so an empty queue early in the
    /// run is transient, not terminal. Without this flag the shim cannot tell a
    /// not-yet-grounded conductor from a finished one and exits prematurely (the
    /// pga race the shim's poll loop hit on the first real e2e run).
    finished: AtomicBool,
    /// Set only by [`close_session`](Driver::close_session), under `inner`'s lock, so a spawn
    /// reading it there either parks or stands pending for the close to park; public for a wait on
    /// a hold to read.
    pub session_closed: AtomicBool,
    /// Whether the run stands held, read ahead of each handout; `None` on a driver no hold
    /// governs.
    hold_read: Option<&'a (dyn Fn() -> Result<bool, Error> + Sync)>,
}

impl Default for Driver<'_> {
    fn default() -> Self {
        Driver {
            inner: Mutex::new(Inner {
                queue: VecDeque::new(),
                pending: HashMap::new(),
                next_id: 0,
            }),
            finished: AtomicBool::new(false),
            session_closed: AtomicBool::new(false),
            hold_read: None,
        }
    }
}

impl<'a> Driver<'a> {
    pub fn new() -> Self {
        Self::default()
    }

    /// This driver with `read` answering whether the run stands held, the read each handout
    /// decides on.
    pub fn with_hold_read(mut self, read: &'a (dyn Fn() -> Result<bool, Error> + Sync)) -> Self {
        self.hold_read = Some(read);
        self
    }

    /// The session has closed, so nothing will serve a spawn: every pending spawn, queued or
    /// handed out, returns a park to its caller and every later one returns it at once, and the
    /// conductor unwinds uncharged. A result already delivered stays ahead of the park.
    pub fn close_session(&self) {
        let inner = self.inner.lock().unwrap();
        self.session_closed.store(true, Ordering::SeqCst);
        for (id, call) in &inner.pending {
            let _ = call.tx.send(Err(parked_spawn(id)));
        }
    }

    /// Return the next queued spawn request for the shim, or None if none waits or the run is not
    /// proved unheld: a held run or a read that fails hands out nothing and pops nothing, so the
    /// request stays pending for a later poll. The hold is read only with a spawn queued, since
    /// the shim polls at 20 Hz while idle, and outside the lock, which a spawn queues under.
    pub fn next(&self) -> Option<SpawnRequest> {
        if self.inner.lock().unwrap().queue.is_empty() {
            return None;
        }
        if self
            .hold_read
            .is_some_and(|read| !matches!(read(), Ok(false)))
        {
            return None;
        }
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
    /// `resolved_model` is the AUTHORITATIVE model id the shim's `rigger_result` call
    /// carried in `meta.resolved_model` (spec 61 c10 round 2), sourced end to end from
    /// the Agent SDK's own structured terminal metadata
    /// ([`resolvedModelFromUsage`](shim/shim.mjs)) - `mcpserver.rs::tool_result` reads it
    /// off the wire and passes it straight through here, never the agent's own output
    /// text. Empty when the shim observed no single authoritative id (never a guess or
    /// default). The stepwise `rigger step` path already surfaced this field via
    /// [`SpawnResult::meta_str`](crate::spawn::SpawnResult::meta_str); the
    /// in-process workflow path now matches it instead of hardcoding it empty.
    ///
    /// Returns `true` if `id` named a pending spawn and the result was delivered,
    /// `false` if the id was unknown or stale. A `false` must be surfaced to the
    /// caller (not swallowed): a shim reporting a result for a wrong/stale id
    /// otherwise gets silent success while the real spawn blocks forever.
    #[must_use]
    pub fn result(&self, id: &str, output: String, err: String, resolved_model: String) -> bool {
        let inner = self.inner.lock().unwrap();
        if let Some(call) = inner.pending.get(id) {
            let r = if err.is_empty() {
                Ok(AgentResult {
                    output,
                    resolved_model,
                    ..Default::default()
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

impl AgentDriver for Driver<'_> {
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
            if self.session_closed.load(Ordering::SeqCst) {
                return Err(parked_spawn(&id));
            }
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
    use std::sync::atomic::AtomicUsize;
    use std::sync::mpsc::Receiver;
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
            driver.result(
                &req.id,
                "done".into(),
                String::new(),
                "claude-opus-4-8-20260101".into()
            ),
            "a result for a known spawn id must report it was delivered"
        );
        let res = handle.join().unwrap().unwrap();
        assert_eq!(res.output, "done");
        // Reject-fix (spec 61 c10 round 2): the resolved model id the shim reports on
        // `rigger_result` must reach the conductor's AgentResult, not be dropped.
        assert_eq!(
            res.resolved_model, "claude-opus-4-8-20260101",
            "the resolved model id delivered to result() must reach AgentResult"
        );
    }

    #[test]
    fn result_reports_an_unknown_id() {
        let driver = Driver::new();
        assert!(
            !driver.result("does-not-exist", "out".into(), String::new(), String::new()),
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
        assert!(driver.result(&req.id, "done".into(), String::new(), String::new()));
        handle.join().unwrap().unwrap();
        assert!(
            driver.is_finished(),
            "once the last spawn drains, the finished run reports done"
        );
    }

    /// The spawn id the served-hold and close rows launch.
    const ID: &str = "a/implementer#0";

    /// One spawn of [`ID`], the conductor's side of the bridge.
    fn launch(driver: &Driver) -> Result<AgentResult, Error> {
        let emit = |_: &str, _: Value| Ok(());
        driver.spawn(
            &AgentDef::default(),
            "do it",
            &SpawnOpts {
                id: ID.to_string(),
                ..Default::default()
            },
            &emit,
        )
    }

    /// Wait until [`ID`] stands pending on `driver`.
    fn await_pending(driver: &Driver) {
        let deadline = Instant::now() + Duration::from_secs(2);
        while !driver.inner.lock().unwrap().pending.contains_key(ID) {
            assert!(Instant::now() < deadline, "spawn never queued");
            std::thread::sleep(Duration::from_millis(1));
        }
    }

    /// What the spawn a row launched returned, the bound keeping a row from blocking on a spawn
    /// nothing answers.
    fn returned(outcome: &Receiver<Result<AgentResult, Error>>) -> Result<AgentResult, Error> {
        outcome
            .recv_timeout(Duration::from_secs(2))
            .expect("the spawn returns")
    }

    /// Answers [`ID`] when dropped, so a row that fails with its spawn still blocked leaves the
    /// scope a thread to join and reports the failure instead of hanging.
    struct AnswerOnDrop<'a, 'b>(&'a Driver<'b>);

    impl Drop for AnswerOnDrop<'_, '_> {
        fn drop(&mut self) {
            let _ = self
                .0
                .result(ID, String::new(), String::new(), String::new());
        }
    }

    /// Run `row` with a spawn of [`ID`] standing pending on `driver` and its outcome on the
    /// channel `row` is handed.
    fn with_a_pending_spawn(
        driver: &Driver,
        row: impl FnOnce(&Receiver<Result<AgentResult, Error>>),
    ) {
        std::thread::scope(|s| {
            let (tx, outcome) = channel();
            s.spawn(move || tx.send(launch(driver)));
            let _answer = AnswerOnDrop(driver);
            await_pending(driver);
            row(&outcome);
        });
    }

    /// The id `next` hands out, what these rows tell a handout by.
    fn next_id(driver: &Driver) -> Option<String> {
        driver.next().map(|req| req.id)
    }

    /// Spec 105: on `rigger serve` the launch is the handout, so `next` hands out a queued spawn
    /// only on a read proving the run unheld. A hold or a failed read hands out nothing and pops
    /// nothing, so the same spawn goes out once a read proves the run unheld.
    #[test]
    fn next_hands_out_a_queued_spawn_only_on_a_read_proving_the_run_unheld() {
        for (name, answer, handed_out) in [
            ("a run proved unheld", Some(false), Some(ID.to_string())),
            ("a held run", Some(true), None),
            ("a read that failed", None, None),
        ] {
            let held = Mutex::new(answer);
            let read = || {
                held.lock()
                    .unwrap()
                    .ok_or_else(|| Error("the run read failed".to_string()))
            };
            let driver = Driver::new().with_hold_read(&read);
            with_a_pending_spawn(&driver, |outcome| {
                assert_eq!(next_id(&driver), handed_out, "{name}");
                *held.lock().unwrap() = Some(false);
                if handed_out.is_none() {
                    assert_eq!(next_id(&driver), Some(ID.to_string()), "{name}: unpopped");
                }
                assert_eq!(next_id(&driver), None, "{name}: handed out once");
                assert!(driver.result(ID, "done".into(), String::new(), String::new()));
                assert_eq!(returned(outcome).unwrap().output, "done", "{name}");
            });
        }
    }

    /// `next` reads the hold only with a spawn queued, since the shim polls it at 20 Hz while idle.
    #[test]
    fn next_reads_the_hold_only_with_a_spawn_queued() {
        let reads = AtomicUsize::new(0);
        let read = || {
            reads.fetch_add(1, Ordering::SeqCst);
            Ok(true)
        };
        let driver = Driver::new().with_hold_read(&read);
        assert_eq!(next_id(&driver), None);
        assert_eq!(reads.load(Ordering::SeqCst), 0, "nothing queued");
        with_a_pending_spawn(&driver, |_| {
            assert_eq!(next_id(&driver), None);
            assert_eq!(reads.load(Ordering::SeqCst), 1, "one spawn queued");
        });
    }

    /// Spec 105: a close returns a park to every pending spawn, queued or handed out, since nothing
    /// serves it, and a result that landed first stands.
    #[test]
    fn a_close_parks_every_pending_spawn_no_result_answered() {
        let park = parked_spawn(ID).0;
        for (name, handed_out, answered, ended) in [
            ("a queued spawn", false, false, Err(park.clone())),
            ("a spawn handed out", true, false, Err(park.clone())),
            (
                "a spawn whose result landed first",
                true,
                true,
                Ok("done".to_string()),
            ),
        ] {
            let driver = Driver::new();
            with_a_pending_spawn(&driver, |outcome| {
                if handed_out {
                    assert_eq!(next_id(&driver), Some(ID.to_string()), "{name}");
                }
                if answered {
                    assert!(driver.result(ID, "done".into(), String::new(), String::new()));
                }
                driver.close_session();
                assert_eq!(
                    returned(outcome)
                        .map(|result| result.output)
                        .map_err(|e| e.0),
                    ended,
                    "{name}"
                );
            });
        }
    }

    /// Spec 105: a spawn behind a close parks at once and queues nothing, however long the run
    /// goes on, and the flag the hold's wait reads is up.
    #[test]
    fn a_spawn_behind_a_close_parks_at_once_and_queues_nothing() {
        let driver = Arc::new(Driver::new());
        assert!(!driver.session_closed.load(Ordering::SeqCst));
        driver.close_session();
        assert!(driver.session_closed.load(Ordering::SeqCst));
        let d2 = driver.clone();
        let (tx, rx) = channel();
        std::thread::spawn(move || tx.send(launch(&d2)));
        let ended = returned(&rx);
        assert_eq!(ended.map_err(|e| e.0).unwrap_err(), parked_spawn(ID).0);
        assert_eq!(next_id(&driver), None);
        assert!(driver.inner.lock().unwrap().pending.is_empty());
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
        assert!(driver.result(&req.id, "done".into(), String::new(), String::new()));
        handle.join().unwrap().unwrap();
    }
}
