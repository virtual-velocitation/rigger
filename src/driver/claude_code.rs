//! The native Claude Code agent host (spec 104: rigger hosts its agents as headless
//! Claude Code sessions; `docs/architecture-addendum-claude-code-integration.md` §4).
//! Replaces the blocking `cli` driver's `Command::output()` (one shot, learn nothing
//! until exit, infer the rest) with a typed, streaming launch: every fact about an agent,
//! its session, its model, its tools, its permissions, travels as an explicit argv or
//! stream field, never inferred from a prompt sentence or parsed stdout.
//!
//! This file lands incrementally, criterion by criterion (spec 104's own "one host, ship
//! it whole" intent still holds - the criteria are a delivery split, not a design split):
//! criterion 1 (THE LAUNCH IS TYPED) owns argv, cwd, environment and the open half of the
//! launch record. THE STREAM (criterion 2, this module's addition below `Launch`) reads
//! what criterion 1 starts: one reader per child, line by line, closing
//! `impl AgentDriver for Driver`. The composition-root swap - `rigger run` (`src/main.rs`)
//! launching its agents through this host instead of `cli::Driver` - is DEFERRED to spec
//! 105 (`d-u104-stream-defer-composition-swap`): `tests/cli.rs`'s existing fixtures assume
//! `cli::Driver`'s argv/stdio contract, and this host has no failure-class relaunch or hold
//! yet (criterion 5) to run unattended against a real `api_retry`.

use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStderr, Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::Value;

use crate::conductor::{AgentDriver, AgentResult, Error, SpawnOpts};
use crate::config::AgentDef;
use crate::eventstore::EventStore;
use crate::hooks;
use crate::liveness;
use crate::progress::SpawnLaunched;
use crate::progress_store;
use crate::spawn::SpawnResult;
use crate::spawn_store;

/// The permission mode this host always passes (architecture addendum §4.1): paired with
/// `--permission-prompts none`, whatever would otherwise prompt is DENIED and reported on
/// the stream, rather than silently blocking on a prompt nobody can answer. Not
/// configurable per agent - every persona runs unattended through the same path (§2,
/// invariant 6: one host).
const PERMISSION_MODE: &str = "default";

/// How often [`Driver::read_stream`]'s loop wakes to re-check elapsed time when
/// `max_wall_clock` is 0 (unbounded): a "wake up occasionally" budget, never itself a
/// wall-clock bound - an unbounded spawn still polls rather than blocking forever on the
/// channel, so a future caller with another reason to want the loop responsive is never
/// shut out by this recv.
const UNBOUNDED_POLL: std::time::Duration = std::time::Duration::from_secs(5);

/// Spawns agents as headless Claude Code sessions.
pub struct Driver<'a> {
    /// The `claude` binary to run. Empty resolves to `"claude"` on `$PATH`, same
    /// fallback convention as [`crate::driver::cli::Driver`].
    pub bin: String,
    /// How this host invokes ITSELF as the spawn's bound MCP server
    /// (`<rigger_bin> mcp --spawn <id>`, §4.3). Empty resolves to `"rigger"` on `$PATH` -
    /// a test points this at a fixture binary instead of a real `rigger` install.
    pub rigger_bin: String,
    /// The progress store `launch` records `SpawnLaunched` to and THE STREAM (criterion
    /// 2) records every waiting/denied/unparseable line to (`crate::progress_store`,
    /// `.rigger/progress.db` at the composition root) - injected, never opened by this
    /// driver itself, mirroring how every other store-touching port in this codebase
    /// takes its store by reference rather than a path it resolves on its own.
    pub progress_store: &'a dyn EventStore,
    /// The run's own event store (`.rigger/events.db`). THE STREAM records the `result`
    /// message here as a [`SpawnResult`] (spec 104 criterion 2) - the host does this
    /// FOR the agent, since a host-launched prompt carries no self-report instruction.
    pub run_store: &'a dyn EventStore,
    /// The run's scratch root (architecture addendum §4.2): the raw stream-json
    /// transcript lands at `<scratch_root>/agent-stream/<run>/<spawn>.<launch>.jsonl`
    /// (see [`stream_path`]) and the liveness marker at
    /// `<scratch_root>/agent-live/<run>/<spawn>` (see [`crate::liveness::marker_path`]).
    /// Empty disables both (no scratch root configured / a test that does not care).
    pub scratch_root: String,
    /// THE STOP's own grace period (spec 104 criterion 6, architecture addendum §4.6):
    /// how long [`Driver::stop_for_wall_clock_silence`] waits, after closing the
    /// session's input, for the child to exit on its own before forcing it via
    /// [`crate::reap::end_child`] - the literal 30 seconds the Design section names in
    /// production. An INJECTED field, never a bare module constant - the same "the real
    /// duration is a value a test overrides on the instance" seam this codebase already
    /// uses for a waited duration elsewhere (`d-clock-seam`), so a test proving the FULL
    /// stop sequence never has to sit out a real 30 seconds. `Driver::default()`
    /// (test-only) resolves it to 30s; production sets it explicitly (the composition
    /// root, spec 105).
    pub stop_grace: std::time::Duration,
}

// `Default` is a TEST convenience only: production always constructs `Driver` with real
// injected stores at the composition root (`src/main.rs`), so a value that reads through
// `self.progress_store`/`self.run_store` without one is a test bug, never a real launch -
// the same contract `crate::eventstore::SilentStore` (also `#[cfg(test)]`) documents.
#[cfg(test)]
impl Default for Driver<'static> {
    fn default() -> Self {
        // An inline literal reference (rvalue static promotion), not a named `static`:
        // `SilentStore` is a unit struct with no `Drop`, so `&SilentStore` promotes to
        // `'static` on its own - matching every other call site in the crate (e.g. this
        // same file's `launch_records_a_lost_write_loudly_rather_than_spawning_blind`
        // test) rather than a second, unnecessary spelling of the same thing.
        Driver {
            bin: "claude".to_string(),
            rigger_bin: "rigger".to_string(),
            progress_store: &crate::eventstore::SilentStore,
            run_store: &crate::eventstore::SilentStore,
            scratch_root: String::new(),
            stop_grace: std::time::Duration::from_secs(30),
        }
    }
}

/// A started launch: the live child (stdin still open - THE STREAM, criterion 2, writes
/// any later operator note and closes it after the first `result`), the session id this
/// host minted for it, and the exact argv it ran with (kept for the caller's own
/// diagnostics/tests; not re-derivable from `Child`).
pub struct Launch {
    pub child: Child,
    pub session_id: String,
    pub args: Vec<String>,
}

impl Driver<'_> {
    /// THE LAUNCH (spec 104 criterion 1): start one child process for this spawn launch.
    ///
    /// Ordering is the criterion: `SpawnLaunched` is appended to the progress store
    /// BEFORE the child is started (so a launch that never even started a process because
    /// `bin` does not exist still cannot be re-attempted silently - the record proves the
    /// attempt), and it is the ONLY event this call writes - closing it belongs to a
    /// later criterion. The environment the child sees is the operator's ambient
    /// environment (inherited unchanged, credential included) plus `opts.env`; this
    /// function neither reads nor sets a credential variable of its own. Once the record
    /// is written the launch never returns `Err` with an unaccounted-for child still
    /// running behind it: a failure writing the first message ends the child through its
    /// own handle (`kill()` + `wait()`) before the error propagates.
    pub fn launch(
        &self,
        agent: &AgentDef,
        task: &str,
        opts: &SpawnOpts,
        store: &dyn EventStore,
    ) -> Result<Launch, Error> {
        let session_id = uuid::Uuid::new_v4().to_string();
        let args = build_args(agent, opts, &session_id, self.rigger_bin());

        let started = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        progress_store::record_launch(
            store,
            &opts.run_id,
            &SpawnLaunched {
                spawn: opts.id.clone(),
                launch: opts.launch,
                session_id: session_id.clone(),
                resumed_from: if opts.resumed_from.is_empty() {
                    None
                } else {
                    Some(opts.resumed_from.clone())
                },
                started,
                ended: None,
                class: None,
            },
        )?;

        let bin = self.bin();
        let mut cmd = Command::new(bin);
        cmd.args(&args);
        if !opts.dir.is_empty() {
            cmd.current_dir(&opts.dir);
        }
        // The ONE build-environment authority's injection site for this driver (spec 65),
        // exactly like the cli driver applies it: every var the resolver derived, on top
        // of the inherited ambient environment (`Command` never clears it) - so the
        // operator's own credential rides through untouched and unread.
        for (k, v) in &opts.env {
            cmd.env(k, v);
        }
        cmd.stdin(Stdio::piped());
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());
        let mut child = cmd
            .spawn()
            .map_err(|e| Error(format!("claude_code driver: launch {:?}: {e}", opts.id)))?;

        // The task is the first user message on the input stream (§4.1); stdin stays
        // open afterward (§4.2's "the host closes the input after the first `result`" is
        // the reader's call, not this one's).
        let write_result = child
            .stdin
            .as_mut()
            .map(|stdin| writeln!(stdin, "{}", first_user_message(task)));
        if let Some(Err(e)) = write_result {
            // The durable `SpawnLaunched` record above already claims this launch
            // happened - a write failure here must not also leak the OS process behind
            // it. End it through the handle that spawned it (process lifecycle
            // discipline: kill() + wait(), never a computed-pid signal) before
            // reporting the error, so no unaccounted-for child outlives this `Err`.
            let _ = child.kill();
            let _ = child.wait();
            return Err(Error(format!(
                "claude_code driver: write task to {:?}: {e}",
                opts.id
            )));
        }

        Ok(Launch {
            child,
            session_id,
            args,
        })
    }

    fn bin(&self) -> &str {
        if self.bin.is_empty() {
            "claude"
        } else {
            &self.bin
        }
    }

    fn rigger_bin(&self) -> &str {
        if self.rigger_bin.is_empty() {
            "rigger"
        } else {
            &self.rigger_bin
        }
    }

    /// THE STREAM (spec 104 criterion 2): read `launch`'s child stdout line by line to
    /// completion, close the input after the first `result`, and reap the child. Every
    /// line touches the spawn's liveness marker and is persisted to the raw stream file
    /// (architecture addendum §4.2) before this function looks at its content, so a line
    /// this reader cannot parse is never silently lost - it still lands liveness, the
    /// transcript file, AND a progress line (below), just no structured record.
    ///
    /// Every exit reaps the child through ONE mechanism, [`crate::dash::ReapedChild`] -
    /// the SAME guard the dashboard's own supervised child uses (adj-u104-stream round-3
    /// REQUIRED FIX 2: a second, hand-rolled Drop-based reap guard duplicated that exact
    /// concern; deleted in favor of reusing the canonical one, per this codebase's
    /// standing one-reaping-mechanism precedent). `launch.child` is moved into it before
    /// the stdout loop below, so the missing-stdout-pipe guard, a read error before any
    /// result, and a `record_result_if_absent` write failure all leave `reaper` in scope
    /// with the process still possibly running behind it; dropping it ends it either way:
    /// `try_wait` first, falling back to a best-effort end-then-collect through the same
    /// handle when the process is not yet known to have exited. That mirrors `launch()`'s
    /// own "never returns `Err` with an unaccounted-for child still running behind it"
    /// contract.
    ///
    /// A CLEAN EOF is different (adj-u104-stream round-5 REQUIRED FIX,
    /// op-104-stream-clean-eof-waits-before-the-reap): the child closing its OWN stdout
    /// pipe is not proof it has exited - real post-output teardown (flushing telemetry,
    /// releasing a lock) can still be running - so THIS function tracks that one exit
    /// shape (`ended_via_break` below) and, on a clean EOF ONLY, blocks on the child's own
    /// handle (`reaper.child_mut().wait()`, no signal) BEFORE the trailing `drop(reaper)`,
    /// so that teardown always finishes on its own first. Every other exit path (missing
    /// stdout pipe, a read error before any result, the post-result-read-error `break`
    /// below, a `record_result_if_absent` failure) is unchanged: straight to the
    /// signal-capable drop, since none of those is evidence the child is mid-teardown.
    ///
    /// A read error AFTER a result is already captured (adj-u104-stream round-2 REQUIRED
    /// FIX) is handled the same uniform way: the loop below just stops reading (`break`)
    /// rather than propagating `Err`, so this function still returns the
    /// already-recorded `Ok(AgentResult)` rather than discarding it, since
    /// `record_result_if_absent` already wrote the real result to the run store one line
    /// earlier and only the FIRST result ever acts (round-3 REQUIRED FIX 1 pins the
    /// in-memory result to that same first line too - see the `result.is_none()` guard
    /// below).
    ///
    /// stderr is drained on its own thread, started before the stdout loop below and
    /// running the whole time this function blocks reading stdout (REQUIRED FIX 2): the
    /// `Stdio::piped()` pipe `launch()` gave stderr has a bounded kernel buffer (64KiB on
    /// Linux) that nobody else reads, so a real agent that writes more than that before
    /// its first stdout line would otherwise block the CHILD's write() forever while this
    /// host sits blocked reading stdout - a genuine two-sided deadlock, not merely a slow
    /// path. The drained bytes are diagnostic only (never this criterion's record of
    /// truth), so they are discarded.
    ///
    /// STOP (spec 104 criterion 6): stdout is likewise read on its OWN thread (the SAME
    /// stderr-deadlock reasoning applies to it too - this function must never itself sit
    /// blocked in `BufReader::lines()` with no way to notice silence), forwarding each
    /// line (or `None` on a clean EOF) over a channel this function drains with
    /// `recv_timeout`, so `max_wall_clock` (0 = unbounded, [`crate::config::AgentDef`]'s
    /// own resolved bound) can be enforced against STREAM SILENCE - the wall time since
    /// the last line, not total run time, mirroring `liveness::is_stale`'s own semantics
    /// for the external stepwise driver, but enforced HERE because this host has no
    /// external stepper polling a marker file. A genuine expiry before any result exists
    /// runs THE STOP sequence (close the session's input, wait [`Driver::stop_grace`], end
    /// the child via [`crate::reap::end_child`] - the handle-bound production helper, never a
    /// computed pid) and records the EXISTING [`SpawnResult::liveness_fault`] shape, same
    /// as `liveness::sweep`'s own fault for the stepwise driver, just recorded HERE rather
    /// than by an external sweep. An expiry AFTER a result already landed is not a stop at
    /// all - it degrades to the SAME soft-break the post-result read-error path already
    /// takes (round-2 precedent), letting the existing reap decide the child's fate.
    fn read_stream(
        &self,
        launch: Launch,
        opts: &SpawnOpts,
        max_wall_clock: u64,
    ) -> Result<AgentResult, Error> {
        let session_id = launch.session_id.clone();
        let mut reaper = crate::dash::ReapedChild::new(launch.child);

        let stdout = reaper.child_mut().stdout.take().ok_or_else(|| {
            Error(format!(
                "claude_code driver: {:?}: launch carried no stdout pipe",
                opts.id
            ))
        })?;
        let stderr_drain = reaper
            .child_mut()
            .stderr
            .take()
            .map(|stderr| std::thread::spawn(move || drain_child_stderr(stderr)));
        // THE STOP watchdog needs a way to notice silence while blocked reading stdout,
        // so stdout is read on its own thread too (see the doc above) and forwarded here
        // over a channel this loop drains with a timeout instead of `BufRead::lines`'s
        // own unbounded blocking iterator.
        let (line_tx, line_rx) = std::sync::mpsc::channel::<Option<std::io::Result<String>>>();
        let stdout_reader = std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                if line_tx.send(Some(line)).is_err() {
                    return; // the reader (this function) is gone; nothing left to feed.
                }
            }
            let _ = line_tx.send(None); // clean EOF
        });

        let marker = liveness::marker_path(&self.scratch_root, &opts.run_id, &opts.id);
        let mut stream_file = open_stream_file(
            stream_path(&self.scratch_root, &opts.run_id, &opts.id, opts.launch).as_deref(),
        );

        let mut resolved_model = String::new();
        let mut permission_denials: u64 = 0;
        let mut result: Option<AgentResult> = None;
        // adj-u104-stream round-5 REQUIRED FIX (op-104-stream-clean-eof-waits-before-the-reap):
        // distinguishes the loop's two exit shapes below. `false` when the loop instead runs
        // to real exhaustion (a clean EOF: the child closed its stdout pipe on its own,
        // with no read error) - see the `wait()` this flag gates, just past the loop.
        let mut ended_via_break = false;
        let mut last_activity = std::time::Instant::now();

        loop {
            let recv_timeout = if max_wall_clock == 0 {
                // Unbounded: still poll periodically rather than an infinite wait, so a
                // future caller that DOES want to observe long-run liveness some other
                // way is never blocked out by this recv - purely a "wake up occasionally"
                // budget, never itself a wall-clock bound.
                UNBOUNDED_POLL
            } else {
                let elapsed = last_activity.elapsed();
                let bound = std::time::Duration::from_secs(max_wall_clock);
                match bound.checked_sub(elapsed) {
                    Some(remaining) if !remaining.is_zero() => remaining,
                    _ => {
                        // Already past the bound with no line since the last check -
                        // expire without blocking on the channel at all.
                        std::time::Duration::ZERO
                    }
                }
            };

            let received = if recv_timeout.is_zero() {
                Err(std::sync::mpsc::RecvTimeoutError::Timeout)
            } else {
                line_rx.recv_timeout(recv_timeout)
            };

            let line = match received {
                Ok(None) => break, // clean EOF; ended_via_break stays false
                Ok(Some(line)) => {
                    last_activity = std::time::Instant::now();
                    line
                }
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break, // reader thread gone; treat as EOF
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) if result.is_none() => {
                    // THE STOP (spec 104 criterion 6): genuine wall-clock SILENCE with no
                    // result yet. Runs the whole sequence inline (mirrors the pre-result
                    // read-error arm below, which also returns directly) so the caller
                    // sees exactly one outcome for "this launch never produced a result".
                    return self.stop_for_wall_clock_silence(
                        reaper,
                        stderr_drain,
                        stdout_reader,
                        opts,
                        &session_id,
                        max_wall_clock,
                    );
                }
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                    // result.is_some(): silence AFTER a real result already landed is not
                    // a stop - the SAME soft-break the post-result read-error path below
                    // already takes (round-2 precedent): stop reading, let the existing
                    // reap decide the child's fate.
                    self.record_progress(opts, "stream (silent after result, stopping the read)");
                    ended_via_break = true;
                    break;
                }
            };

            let line = match line {
                Ok(line) => line,
                // adj-u104-stream round-2 REQUIRED FIX: `record_result_if_absent` may
                // already have durably written the real `SpawnResult` one line ago (the
                // loop keeps reading after the FIRST result by design - a duplicate
                // result, or more transcript, is anticipated, not an error). A read
                // error on one of those LATER lines must never overturn that already-
                // recorded success by turning it into an `Err` here - record it as a
                // best-effort progress line instead and stop reading; the child is
                // reaped below exactly like any other exit (the process has not
                // necessarily exited merely because reading its stdout failed - the
                // `ReapedChild` drop just below decides that on its own).
                // Before any result exists, an unreadable line is still the genuine
                // failure it always was.
                Err(e) if result.is_some() => {
                    self.record_progress(
                        opts,
                        &format!("stream (read error after result, ignored): {e}"),
                    );
                    ended_via_break = true;
                    break;
                }
                Err(e) => {
                    return Err(Error(format!(
                        "claude_code driver: {:?}: read agent stream: {e}",
                        opts.id
                    )));
                }
            };

            // "every line touches the spawn's liveness marker - the host proves life,
            // the agent is never asked to" (architecture addendum §4.2).
            touch_liveness_marker(marker.as_deref());
            if let Some(f) = stream_file.as_mut() {
                // Best-effort: the transcript is a diagnostic/audit artifact, never the
                // record of truth (the progress store and the run store are), so a full
                // disk here must not fail an otherwise-healthy spawn.
                let _ = writeln!(f, "{line}");
            }

            let Ok(v) = serde_json::from_str::<Value>(&line) else {
                // "a line that is not JSON is recorded as a progress line and skipped."
                self.record_progress(opts, &format!("stream (unparseable): {line}"));
                continue;
            };

            match (
                v.get("type").and_then(Value::as_str),
                v.get("subtype").and_then(Value::as_str),
            ) {
                (Some("system"), Some("init")) => {
                    resolved_model = v
                        .get("model")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string();
                    // The MCP connection status is noted for the record but does not
                    // gate this criterion - "a rigger server that did not connect fails
                    // the launch as a fault" is failure-class territory (criterion 5's,
                    // NOT this one's).
                    if let Some(servers) = v.get("mcp_servers").and_then(Value::as_array) {
                        self.record_progress(opts, &format!("mcp servers: {servers:?}"));
                    }
                }
                (Some("system"), Some("api_retry")) => {
                    self.record_progress(opts, &api_retry_line(&v));
                }
                (Some("system"), Some("permission_denied")) => {
                    permission_denials += 1;
                    self.record_progress(opts, "permission denied");
                }
                // adj-u104-stream round-3 REQUIRED FIX 1: only the FIRST result-type line
                // acts - a genuine SECOND one (the comment above this match's `Err(e) if
                // result.is_some()` arm calls it anticipated, not an error) must never
                // overwrite the already-captured `result`, nor re-derive it from a
                // `resolved_model`/`permission_denials` state that may have drifted since
                // (more `system/init` or `permission_denied` lines between the two
                // results). The `result.is_none()` GUARD on this arm - rather than an
                // `if` inside a catch-all `(Some("result"), _)` arm - means a later
                // duplicate falls straight through to the `_` arm below: no
                // `spawn_result_from`/`record_result_if_absent` round trip for it either,
                // matching "the log holds one result" (CONSTRAINTS WALK) instead of
                // merely relying on that call's own idempotence to paper over it.
                (Some("result"), _) if result.is_none() => {
                    let res = spawn_result_from(&v, opts, &resolved_model, permission_denials);
                    spawn_store::record_result_if_absent(self.run_store, &res)?;
                    result = Some(AgentResult {
                        output: res.output,
                        resolved_model: resolved_model.clone(),
                    });
                    // "the host closes the input after the first `result`" - dropping the
                    // handle closes the pipe; a session that would otherwise wait on more
                    // input can now exit.
                    drop(reaper.child_mut().stdin.take());
                }
                _ => {
                    // assistant/user turns, hook events, every other subtype, AND a
                    // duplicate result-type line once `result` is already `Some` (falls
                    // through here past the guarded arm above): raw persistence +
                    // liveness already happened above; no structured record for this
                    // criterion (out of THE STREAM's Done-when scope).
                }
            }
        }

        // adj-u104-stream round-5 REQUIRED FIX (op-104-stream-clean-eof-waits-before-the-reap):
        // a clean EOF means the child closed ITS OWN stdout pipe, not that it has exited -
        // legitimate post-output work (flushing telemetry, releasing a lock) can still be
        // running. round-4's consolidation onto `ReapedChild::drop`'s non-blocking
        // `try_wait` treated "not yet known to have exited" the same as "gone missing",
        // so a child still finishing that teardown got force-ended mid-flight.
        // A direct BLOCKING wait - no signal - here on the clean-EOF path ONLY lets that
        // teardown finish on its own first; `ReapedChild::drop` just below then finds
        // `Ok(Some(_))` via its own `try_wait` and sends nothing.
        //
        // The `break` path (a post-result read error, or post-result silence) is
        // UNCHANGED: neither is evidence of in-progress teardown, so it keeps going
        // straight to the signal-capable drop exactly as before this fix.
        if !ended_via_break {
            let _ = reaper.child_mut().wait();
        }

        // Reap through the ONE canonical mechanism, on every exit path alike (a clean
        // EOF, or the `break` above on a post-result read error): `ReapedChild::drop`
        // tries a non-blocking `try_wait` first - on a clean EOF the explicit `wait()`
        // just above already collected it, so this alone reaps it with no signal sent -
        // and falls back to a best-effort end-then-collect through the same handle only
        // when the process is not yet known to have exited (the break path above, which
        // never waited). Explicit, right here, rather than left to run at the function's
        // end: the join just below depends on the child's stderr pipe having already
        // closed.
        drop(reaper);
        // Best-effort join: diagnostic-only drain (see the doc above), and the reap just
        // above guarantees the child's stderr pipe has closed by now, so the thread has
        // already hit EOF or is about to - never a caller-visible wait for its own sake.
        if let Some(handle) = stderr_drain {
            let _ = handle.join();
        }
        // The stdout reader thread's own pipe closed the moment the child (now reaped)
        // exited, so its loop has already hit EOF or is about to - the same "never a
        // caller-visible wait for its own sake" guarantee as the stderr join above.
        let _ = stdout_reader.join();

        result.ok_or_else(|| {
            Error(format!(
                "claude_code driver: {:?}: the agent stream ended with no result",
                opts.id
            ))
        })
    }

    /// THE STOP sequence itself (spec 104 criterion 6), factored out of [`Driver::read_stream`]'s
    /// loop so its one call site there reads as a single named step: "closes the session's
    /// input stream, waits a grace period, then ends the child through the sanctioned
    /// lifecycle helper on the child's own handle" (architecture addendum §4.6), then
    /// records the EXISTING liveness-fault shape and closes the launch record `stopped`.
    /// Always returns `Err` - a launch that never produced a result stays a failure from
    /// this driver's own return value, exactly like the "agent stream ended with no
    /// result" ending just above; a caller wanting the no-attempt-charged semantics reads
    /// [`SpawnResult::is_liveness_fault`] off the run store this durably wrote, the SAME
    /// way `liveness::sweep`'s callers already do for the stepwise driver's own fault.
    fn stop_for_wall_clock_silence(
        &self,
        mut reaper: crate::dash::ReapedChild,
        stderr_drain: Option<std::thread::JoinHandle<()>>,
        stdout_reader: std::thread::JoinHandle<()>,
        opts: &SpawnOpts,
        session_id: &str,
        max_wall_clock: u64,
    ) -> Result<AgentResult, Error> {
        let message = stop_message(&opts.id, max_wall_clock);
        self.record_progress(opts, &format!("stream: {message}"));

        // "closes the session's input stream" - the SAME handle-drop THE STREAM's own
        // success path uses to end a session's turn (dropping the handle closes the pipe).
        drop(reaper.child_mut().stdin.take());

        // "waits a grace period" (self.stop_grace - 30s in production; an injected
        // shorter one in a test proving the full sequence): poll for the child exiting on
        // its own, without blocking the full grace when it already has.
        let deadline = std::time::Instant::now() + self.stop_grace;
        while std::time::Instant::now() < deadline {
            if matches!(reaper.child_mut().try_wait(), Ok(Some(_))) {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }

        // "then ends the child through the sanctioned lifecycle helper on the child's own
        // handle" - the NEW handle-bound production helper (reap.rs, spec 104 criterion
        // 6's own decision `u104-stop-uses-reap-child-handle`): a no-op if the grace wait
        // above already collected it.
        crate::reap::end_child(reaper.child_mut());

        // "the existing liveness-fault result is recorded" - the SAME [`SpawnResult`]
        // shape `liveness::sweep` already records for the stepwise driver's own hung
        // agent, so every downstream reader (the not-yet-recovered surface, last-write-
        // wins recovery) treats a wall-clock STOP identically to a marker-staleness fault.
        let fault = SpawnResult::liveness_fault(
            opts.id.clone(),
            message.clone(),
            crate::failure::FailureClass::Infra.as_str(),
        );
        spawn_store::record_result_if_absent(self.run_store, &fault)?;

        // "the launch ends `stopped`" - best-effort (spec 104: closing a record is a
        // courtesy for reconciliation/observability, never the source of truth the
        // liveness-fault result above already is; a lost close write only means a LATER
        // supervisor sweep redundantly re-closes and re-reaps an already-gone process,
        // never a correctness gap - the same convention [`Driver::record_progress`]
        // documents for its own writes).
        let _ = progress_store::record_launch(
            self.progress_store,
            &opts.run_id,
            &SpawnLaunched::closed(opts.id.clone(), opts.launch, session_id, "stopped", ""),
        );

        drop(reaper);
        if let Some(handle) = stderr_drain {
            let _ = handle.join();
        }
        // The stdout reader's pipe closes the moment `end_child` above finishes the
        // child (SIGTERM already sent it EOF; the SIGKILL fallback guarantees it), so
        // this join never waits for its own sake either.
        let _ = stdout_reader.join();

        Err(Error(format!(
            "claude_code driver: {:?}: {message}",
            opts.id
        )))
    }

    /// Best-effort progress line (spec 14's mechanism, spec 104's own writer): a lost
    /// progress line is a smaller loss than a lost launch record or a lost result, so
    /// this never fails the spawn the way [`Driver::launch`]'s `SpawnLaunched` write does.
    fn record_progress(&self, opts: &SpawnOpts, activity: &str) {
        let _ = progress_store::record(self.progress_store, &opts.run_id, &opts.id, activity);
    }

    /// SUPERVISOR START-UP RECONCILIATION (spec 104 criterion 6, STOP's other half): "on
    /// start the supervisor closes any `SpawnLaunched` left open as `interrupted` and
    /// reaps processes still rooted in that spawn's worktree before it relaunches." Called
    /// once, before any relaunch, with `progress_events` already scoped to `run_id` (the
    /// SAME slice shape [`crate::progress::consolidate`]'s own caller already assembles) -
    /// this function reads no store itself for that half, only writes the closures.
    ///
    /// CONSTRAINTS WALK ("cold start - the log and the progress store are the only
    /// state"): a spawn's worktree dir needs NO event field of its own - it is the same
    /// PURE function of the run's scratch root and the spawn's unit
    /// ([`crate::conductor::unit_worktree_dir`]) every other worktree caller already uses,
    /// via [`crate::spawn::unit_of`] on the open record's own spawn id. An empty
    /// `scratch_root` (no scratch configured) or a spawn id `unit_of` cannot parse
    /// degrades to "close the record, reap nothing" - the same conservative shape
    /// [`crate::liveness`]'s marker functions use for a degenerate id, never a guess at a
    /// path to reap.
    ///
    /// Returns the spawn ids reconciled (closed), in [`crate::progress::open_launches`]'s
    /// deterministic order - callers with nothing to log can ignore it.
    pub fn reconcile_on_start(
        &self,
        progress_events: &[crate::eventstore::Event],
        run_id: &str,
    ) -> Result<Vec<String>, Error> {
        let open = crate::progress::open_launches(progress_events).map_err(|e| {
            Error(format!(
                "claude_code driver: reconcile: decode an open SpawnLaunched: {e}"
            ))
        })?;
        let authorized_root = Path::new(&self.scratch_root);
        let mut reconciled = Vec::with_capacity(open.len());
        for sl in &open {
            progress_store::record_launch(
                self.progress_store,
                run_id,
                &SpawnLaunched::closed(
                    sl.spawn.clone(),
                    sl.launch,
                    sl.session_id.clone(),
                    "interrupted",
                    "",
                ),
            )?;
            if !self.scratch_root.is_empty() {
                if let Some(unit) = crate::spawn::unit_of(&sl.spawn).filter(|u| !u.is_empty()) {
                    let dir = crate::conductor::unit_worktree_dir(&self.scratch_root, unit);
                    crate::reap::reap_processes_rooted_under(Path::new(&dir), authorized_root);
                }
            }
            reconciled.push(sl.spawn.clone());
        }
        Ok(reconciled)
    }
}

/// Read `stderr` to EOF, discarding every byte - the concurrent drain [`Driver::read_stream`]
/// starts before its stdout loop so a chatty agent's stderr volume can never deadlock the
/// host (REQUIRED FIX 2). Raw bytes, not lines: stderr carries no promise of being valid
/// UTF-8 or line-delimited the way the stdout stream-json protocol is, and a `BufRead::lines`
/// read that hit invalid UTF-8 would stop draining at exactly the moment the pipe still
/// needs a reader. Diagnostic-only for this criterion (never a record of truth), so a read
/// error just ends the drain - there is nothing for this background thread to report to.
fn drain_child_stderr(mut stderr: ChildStderr) {
    let mut buf = [0u8; 8192];
    while matches!(stderr.read(&mut buf), Ok(n) if n > 0) {}
}

impl AgentDriver for Driver<'_> {
    /// Completes THE LAUNCH (criterion 1) with THE STREAM (criterion 2): start the child
    /// and read it to its `result`, exactly as `docs/architecture-addendum-claude-code-integration.md`
    /// §4 describes. `emit` is unused: unlike the cli driver (a subprocess with no live
    /// channel, so its decisions are bridged from stdout after the fact), this host's
    /// agent records its own decisions LIVE through its bound MCP server
    /// (`rigger mcp --spawn <id>`, criterion 3) - there is nothing left to bridge here.
    fn spawn(
        &self,
        agent: &AgentDef,
        prompt: &str,
        opts: &SpawnOpts,
        _emit: &dyn Fn(&str, Value) -> Result<(), Error>,
    ) -> Result<AgentResult, Error> {
        let launch = self.launch(agent, prompt, opts, self.progress_store)?;
        // THE STOP (spec 104 criterion 6): the agent's own resolved bound (spec 10 unit
        // 3, `AgentDef::max_wall_clock`, already folded from `defaults.max_wall_clock` at
        // config-load time) - 0 stays unbounded, the established convention this field's
        // own doc already sets.
        self.read_stream(launch, opts, agent.max_wall_clock.unwrap_or(0))
    }
}

/// Touch `marker` (create it, or bump its mtime if it already exists) so a fresh liveness
/// read from `rigger step`/the dash sees this spawn as alive - the pure filesystem side
/// of `liveness::marker_path`'s contract. `None` (no scratch root, or a degenerate id
/// `marker_path` itself declines - see its doc) is a silent no-op, mirroring every other
/// caller's "no marker: leave it alone" convention. Best-effort throughout: a liveness
/// touch that failed is a slightly-staler marker, never a reason to abort the spawn whose
/// aliveness it exists to prove.
fn touch_liveness_marker(marker: Option<&Path>) {
    let Some(marker) = marker else { return };
    if let Some(parent) = marker.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::File::create(marker);
}

/// The scratch subdirectory a spawn's raw stream-json transcript lives under (spec 104
/// criterion 2, architecture addendum §4.2) - a sibling of `agent-live`
/// ([`crate::liveness::MARKER_SUBDIR`]) and `agent-scratch`
/// ([`crate::driver::replay::spawn_scratch_path`]).
const AGENT_STREAM_SUBDIR: &str = "agent-stream";

/// The raw stream-json transcript path for one launch:
/// `<scratch_root>/agent-stream/<sanitized run_id>/<sanitized spawn_id>.<launch>.jsonl`
/// (an EMPTY `run_id` omits the run subdir). Built on [`liveness::scratch_subpath`] - the
/// ONE shared layout [`liveness::marker_path`] and
/// [`crate::driver::replay::spawn_scratch_path`] build on too (spec 104 round-4 REQUIRED
/// FIX 3), just with this function's own `.<launch>.jsonl` suffix appended onto the
/// leaf it returns - so a spawn's stream, scratch and liveness marker can never alias a
/// sibling's path, and the empty-`scratch_root`/degenerate-id degrade (see that
/// function's own doc) lives in exactly one place rather than a copy per call site.
fn stream_path(scratch_root: &str, run_id: &str, spawn_id: &str, launch: u32) -> Option<PathBuf> {
    let leaf = liveness::scratch_subpath(scratch_root, AGENT_STREAM_SUBDIR, run_id, spawn_id)?;
    let name = leaf.file_name()?.to_str()?;
    Some(leaf.with_file_name(format!("{name}.{launch}.jsonl")))
}

/// Open (creating parent directories) the raw stream file at `path` for a fresh write -
/// one file per launch, matching [`stream_path`]'s one-launch-one-file naming. `None`
/// (an empty scratch root or spawn id) or an open failure both degrade to "do not persist
/// the transcript" rather than failing the spawn: see [`Driver::read_stream`]'s doc on why
/// this artifact is best-effort.
fn open_stream_file(path: Option<&Path>) -> Option<std::fs::File> {
    let path = path?;
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    std::fs::File::create(path).ok()
}

/// The waiting progress line for a `system/api_retry` message (architecture addendum
/// §4.2: "carrying category, attempt and delay"), grounded on the exact shape a probe of
/// the real CLI showed (spec 104 Problem section):
/// `{"type":"system","subtype":"api_retry","attempt":1,"max_retries":10,
/// "retry_delay_ms":618,"error_status":401,"error":"authentication_failed"}`. The
/// CATEGORY is `error` (Claude Code's own error-category string, e.g.
/// `authentication_failed`) - failure CLASSIFICATION from it is criterion 5's job; this
/// is a human-readable line only.
fn api_retry_line(v: &Value) -> String {
    let category = v.get("error").and_then(Value::as_str).unwrap_or("unknown");
    let attempt = v.get("attempt").and_then(Value::as_u64).unwrap_or(0);
    let delay_ms = v.get("retry_delay_ms").and_then(Value::as_u64).unwrap_or(0);
    format!("waiting: {category} (attempt {attempt}, retrying in {delay_ms}ms)")
}

/// THE STOP's human-readable message (spec 104 criterion 6), recorded both as the
/// [`SpawnResult::liveness_fault`]'s `error` text and this driver's own returned `Err` -
/// the ONE place that wording is assembled, mirroring [`crate::liveness::stale_result_message`]'s
/// role for the stepwise driver's own hung-agent fault, but naming THIS host's own
/// mechanism (stream silence against `max_wall_clock`) rather than a stale marker file,
/// since this driver enforces it in-process rather than through an external sweep.
fn stop_message(spawn_id: &str, max_wall_clock: u64) -> String {
    format!(
        "spawn {spawn_id:?} stopped: its agent stream stayed silent for {max_wall_clock}s \
         (its max_wall_clock bound) with no result - no remediation attempt is charged (the \
         unit's code is not at fault). Re-drive it once the agent/driver is healthy: record \
         a real result with `rigger result {spawn_id}` (last-write-wins supersedes this \
         liveness fault)."
    )
}

/// Map a `result` stream message to the [`SpawnResult`] THE STREAM records (Design:
/// "the `result` message becomes the `SpawnResult`: `output`, and `meta` with
/// `resolved_model`, `session_id`, `usage` (`input`, `output`, `cache_creation`,
/// `cache_read`), `turns`, `cost_usd`, `permission_denials`"). `permission_denials` is the
/// count THIS reader accumulated from `system/permission_denied` lines seen earlier in
/// THIS launch's own stream - "the count rides on the result" - rather than trusting a
/// same-named field on the result message itself, so the count is correct even against a
/// Claude Code version whose result message omits or names that field differently.
/// Every numeric field defaults to its zero value when the real message omits it, so a
/// result line missing a field it usually carries still yields a well-formed record
/// rather than failing the whole spawn over one absent number.
fn spawn_result_from(
    v: &Value,
    opts: &SpawnOpts,
    resolved_model: &str,
    permission_denials: u64,
) -> SpawnResult {
    let output = v
        .get("result")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let session_id = v
        .get("session_id")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let usage = v.get("usage").cloned().unwrap_or(Value::Null);
    let get_u64 = |key: &str| usage.get(key).and_then(Value::as_u64).unwrap_or(0);
    let meta = serde_json::json!({
        "resolved_model": resolved_model,
        "session_id": session_id,
        "usage": {
            "input": get_u64("input_tokens"),
            "output": get_u64("output_tokens"),
            "cache_creation": get_u64("cache_creation_input_tokens"),
            "cache_read": get_u64("cache_read_input_tokens"),
        },
        "turns": v.get("num_turns").and_then(Value::as_u64).unwrap_or(0),
        "cost_usd": v.get("total_cost_usd").and_then(Value::as_f64).unwrap_or(0.0),
        "permission_denials": permission_denials,
    });
    SpawnResult::ok(opts.id.clone(), output).with_meta(meta)
}

/// The typed stream-json first input message: `{"type":"user","message":{"role":"user",
/// "content":task}}`, one compact line (stream-json is line-delimited).
fn first_user_message(task: &str) -> String {
    serde_json::json!({
        "type": "user",
        "message": { "role": "user", "content": task },
    })
    .to_string()
}

/// The `--mcp-config` value naming exactly one server, this spawn's own bound MCP server
/// (§4.3): `<rigger_bin> mcp --spawn <spawn_id>`. Passed as a JSON string, never a file
/// (§4.1).
fn mcp_config_json(spawn_id: &str, rigger_bin: &str) -> String {
    serde_json::json!({
        "mcpServers": {
            "rigger": {
                "command": rigger_bin,
                "args": ["mcp", "--spawn", spawn_id],
            },
        },
    })
    .to_string()
}

/// Single-quote `s` for embedding in a POSIX shell command line: wraps it in `'...'` and
/// replaces every embedded `'` with `'\''` (close the quote, an escaped literal quote, reopen
/// it) - the standard shell-safe encoding, so a root containing a space or a shell
/// metacharacter can never split an argument or be reinterpreted. A hook's `command` (see
/// [`install_write_guard_hook`]) is a shell command line Claude Code runs through the
/// operator's shell, never a bare argv array, so every value threaded into one needs this,
/// not just a join with spaces.
fn shell_single_quote(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('\'');
    for ch in s.chars() {
        if ch == '\'' {
            out.push_str("'\\''");
        } else {
            out.push(ch);
        }
    }
    out.push('\'');
    out
}

/// The exact `rigger guard-write --root <dir> [--root <dir> ...]` shell command line THE
/// WRITE GUARD's installed hook runs (spec 104 criterion 4, Design's "THE WRITE GUARD"):
/// one `--root` per root, each single-quoted, in the given order - so the printed command
/// is deterministic and matches `rigger guard-write`'s own "the first root" deny wording
/// (`--root` given first is the root a reader of this exact command line sees first too).
fn write_guard_command(roots: &[String], rigger_bin: &str) -> String {
    let mut cmd = shell_single_quote(rigger_bin);
    cmd.push_str(" guard-write");
    for root in roots {
        cmd.push_str(" --root ");
        cmd.push_str(&shell_single_quote(root));
    }
    cmd
}

/// THE WRITE GUARD's injection half (spec 104 criterion 4: "criterion 4's, command and
/// injection both"): merges the `PreToolUse` hook entry that runs
/// `rigger guard-write --root <dir>...` for `Edit|Write|NotebookEdit` into a spawn's
/// settings JSON. Reuses [`hooks::install_pretooluse_hook`] - the SAME merge authority
/// `rigger setup`'s own PreToolUse installs use (spec 92's graph-first lookup hook), never
/// a second, parallel hook-merging implementation - so this composes cleanly with whatever
/// `existing` already carries: empty (nothing yet), or criterion 5's `StopFailure` family
/// already merged in under its OWN top-level event key, left untouched by this call (THE
/// HOOKS: "the per-spawn settings JSON carries exactly two hook families ... assembled ...
/// from their two owners"). `roots` is the spawn's `dir` and its scratch container (THE
/// WRITE GUARD's own stated roots); `existing` may be empty.
pub fn install_write_guard_hook(
    existing: &[u8],
    roots: &[String],
    rigger_bin: &str,
) -> Result<Vec<u8>, hooks::Error> {
    let command = write_guard_command(roots, rigger_bin);
    hooks::install_pretooluse_hook(existing, "Edit|Write|NotebookEdit", &command)
}

/// Build the typed `claude` headless invocation (architecture addendum §4.1 table): the
/// ONE argv authority for this driver, exactly as `cli::build_args` is for the cli driver
/// - every field below is a fact, never inferred at read time.
///
/// `--json-schema` (verdict personas, §4.5) is deliberately out of this criterion's scope
/// (the Done-when text names session id, stream-json, persona, model, tools, permission
/// flags, MCP config and settings only) and is not built here; a later criterion adds it
/// alongside whichever caller knows a persona is a verdict role, through this same
/// function rather than a second argv authority.
pub fn build_args(
    agent: &AgentDef,
    opts: &SpawnOpts,
    session_id: &str,
    rigger_bin: &str,
) -> Vec<String> {
    let mut args = vec![
        "-p".to_string(),
        "--output-format".to_string(),
        "stream-json".to_string(),
        "--input-format".to_string(),
        "stream-json".to_string(),
        "--verbose".to_string(),
        "--session-id".to_string(),
        session_id.to_string(),
    ];
    if !opts.system_prompt.is_empty() {
        args.push("--system-prompt".to_string());
        args.push(opts.system_prompt.clone());
    }
    let model = agent.model_for_attempt(opts.attempt);
    if !model.is_empty() {
        args.push("--model".to_string());
        args.push(model);
    }
    if !agent.fallback_model.is_empty() {
        args.push("--fallback-model".to_string());
        args.push(agent.fallback_model.clone());
    }
    let tools = agent.allowed_tools();
    if !tools.is_empty() {
        args.push("--allowed-tools".to_string());
        args.push(tools.join(","));
    }
    args.push("--permission-mode".to_string());
    args.push(PERMISSION_MODE.to_string());
    args.push("--permission-prompts".to_string());
    args.push("none".to_string());
    args.push("--mcp-config".to_string());
    args.push(mcp_config_json(&opts.id, rigger_bin));
    args.push("--strict-mcp-config".to_string());
    if !opts.settings_json.is_empty() {
        args.push("--settings".to_string());
        args.push(opts.settings_json.clone());
    }
    args
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eventstore::sqlite::Store;
    use crate::eventstore::{Direction, SilentStore};
    use std::io::Read;

    fn opts(id: &str) -> SpawnOpts {
        SpawnOpts {
            id: id.to_string(),
            attempt: 0,
            system_prompt: "You implement findings.".to_string(),
            ..Default::default()
        }
    }

    // ---- build_args: pure, no process ----

    #[test]
    fn build_args_types_the_full_launch() {
        let a = AgentDef {
            id: "impl".into(),
            model: "sonnet".into(),
            fallback_model: "haiku".into(),
            tools: vec!["Read".into(), "Bash".into()],
            ..Default::default()
        };
        let mut o = opts("u1/implementer#0");
        o.settings_json = "{\"hooks\":{}}".to_string();
        let args = build_args(&a, &o, "sess-123", "rigger");

        assert_eq!(args[0], "-p");
        let get_val = |flag: &str| -> String {
            let i = args
                .iter()
                .position(|x| x == flag)
                .unwrap_or_else(|| panic!("missing flag {flag}: {args:?}"));
            args[i + 1].clone()
        };
        assert_eq!(get_val("--output-format"), "stream-json");
        assert_eq!(get_val("--input-format"), "stream-json");
        assert!(args.iter().any(|x| x == "--verbose"));
        assert_eq!(get_val("--session-id"), "sess-123");
        assert_eq!(get_val("--system-prompt"), "You implement findings.");
        assert_eq!(get_val("--model"), "sonnet");
        assert_eq!(get_val("--fallback-model"), "haiku");
        assert_eq!(get_val("--allowed-tools"), "Read,Bash");
        assert_eq!(get_val("--permission-mode"), "default");
        assert_eq!(get_val("--permission-prompts"), "none");
        assert!(args.iter().any(|x| x == "--strict-mcp-config"));
        assert_eq!(get_val("--settings"), "{\"hooks\":{}}");
    }

    // ---- THE WRITE GUARD's injection half (spec 104 criterion 4) ----

    #[test]
    fn write_guard_command_shapes_one_root_flag_per_root_in_order() {
        let roots = vec!["/spawn/dir".to_string(), "/spawn/scratch".to_string()];
        assert_eq!(
            write_guard_command(&roots, "rigger"),
            "'rigger' guard-write --root '/spawn/dir' --root '/spawn/scratch'"
        );
    }

    #[test]
    fn write_guard_command_escapes_an_embedded_single_quote() {
        let roots = vec!["/a b/it's/weird".to_string()];
        let cmd = write_guard_command(&roots, "rigger");
        // Round-trip: a POSIX shell splitting this exact string must recover the ORIGINAL
        // root text, embedded quote and space both - not a mis-split argument.
        assert_eq!(cmd, "'rigger' guard-write --root '/a b/it'\\''s/weird'");
    }

    #[test]
    fn install_write_guard_hook_merges_the_pretooluse_entry_into_empty_settings() {
        let roots = vec!["/spawn/dir".to_string()];
        let out = install_write_guard_hook(b"", &roots, "rigger").unwrap();
        let v: serde_json::Value = serde_json::from_slice(&out).unwrap();
        assert_eq!(
            v["hooks"]["PreToolUse"][0]["matcher"],
            "Edit|Write|NotebookEdit"
        );
        assert_eq!(
            v["hooks"]["PreToolUse"][0]["hooks"][0]["command"],
            "'rigger' guard-write --root '/spawn/dir'"
        );
    }

    #[test]
    fn install_write_guard_hook_passes_both_the_dir_and_the_scratch_container_as_roots() {
        let roots = vec!["/spawn/dir".to_string(), "/spawn/scratch".to_string()];
        let out = install_write_guard_hook(b"", &roots, "rigger").unwrap();
        let v: serde_json::Value = serde_json::from_slice(&out).unwrap();
        let command = v["hooks"]["PreToolUse"][0]["hooks"][0]["command"]
            .as_str()
            .unwrap();
        assert!(command.contains("--root '/spawn/dir'"));
        assert!(command.contains("--root '/spawn/scratch'"));
    }

    #[test]
    fn install_write_guard_hook_composes_with_an_existing_stopfailure_family() {
        // THE HOOKS: exactly two hook families, assembled from their two owners. Simulate
        // criterion 5's StopFailure entries already present under their OWN event key, and
        // prove this call adds PreToolUse alongside it without touching StopFailure.
        let existing = br#"{
            "hooks": {
                "StopFailure": [
                    {"matcher": "", "hooks": [{"type": "command", "command": "rigger hook stop-failure --spawn u1/implementer#0 --class rate_limit"}]}
                ]
            }
        }"#;
        let roots = vec!["/spawn/dir".to_string()];
        let out = install_write_guard_hook(existing, &roots, "rigger").unwrap();
        let v: serde_json::Value = serde_json::from_slice(&out).unwrap();
        assert_eq!(
            v["hooks"]["StopFailure"][0]["hooks"][0]["command"],
            "rigger hook stop-failure --spawn u1/implementer#0 --class rate_limit",
            "the StopFailure family must survive untouched"
        );
        assert_eq!(
            v["hooks"]["PreToolUse"][0]["matcher"],
            "Edit|Write|NotebookEdit"
        );
    }

    #[test]
    fn install_write_guard_hook_is_idempotent() {
        let roots = vec!["/spawn/dir".to_string()];
        let first = install_write_guard_hook(b"", &roots, "rigger").unwrap();
        let second = install_write_guard_hook(&first, &roots, "rigger").unwrap();
        assert_eq!(
            first, second,
            "installing the same roots twice must not duplicate the hook entry"
        );
    }

    #[test]
    fn build_args_mcp_config_names_the_spawn_bound_server() {
        let a = AgentDef::default();
        let o = opts("u7-launch/implementer#2");
        let args = build_args(&a, &o, "sess", "rigger");
        let i = args.iter().position(|x| x == "--mcp-config").unwrap();
        let cfg: serde_json::Value = serde_json::from_str(&args[i + 1]).unwrap();
        assert_eq!(cfg["mcpServers"]["rigger"]["command"], "rigger");
        assert_eq!(
            cfg["mcpServers"]["rigger"]["args"],
            serde_json::json!(["mcp", "--spawn", "u7-launch/implementer#2"])
        );
        // Exactly one server named.
        assert_eq!(cfg["mcpServers"].as_object().unwrap().len(), 1);
    }

    #[test]
    fn build_args_mcp_config_uses_the_configured_rigger_bin() {
        let a = AgentDef::default();
        let o = opts("u/implementer#0");
        let args = build_args(&a, &o, "sess", "/custom/path/rigger");
        let i = args.iter().position(|x| x == "--mcp-config").unwrap();
        let cfg: serde_json::Value = serde_json::from_str(&args[i + 1]).unwrap();
        assert_eq!(
            cfg["mcpServers"]["rigger"]["command"],
            "/custom/path/rigger"
        );
    }

    #[test]
    fn build_args_omits_empty_optional_flags() {
        let a = AgentDef::default();
        let o = opts("u/implementer#0"); // settings_json left empty by opts()
        let mut bare = o;
        bare.system_prompt = String::new();
        let args = build_args(&a, &bare, "sess", "rigger");
        assert!(!args.iter().any(|x| x == "--system-prompt"));
        assert!(!args.iter().any(|x| x == "--model"));
        assert!(!args.iter().any(|x| x == "--fallback-model"));
        assert!(!args.iter().any(|x| x == "--allowed-tools"));
        assert!(!args.iter().any(|x| x == "--settings"));
        // The always-on flags are still present.
        assert!(args.iter().any(|x| x == "--strict-mcp-config"));
        assert!(args.iter().any(|x| x == "--session-id"));
    }

    // ---- launch(): real subprocess via the checked-in fixture ----

    fn fixture_bin() -> String {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/claude-code-echo-agent.sh")
            .to_string_lossy()
            .into_owned()
    }

    fn read_fixture_lines(child: &mut Child) -> Vec<String> {
        child.wait().expect("fixture agent exits");
        let mut out = String::new();
        child
            .stdout
            .take()
            .expect("piped stdout")
            .read_to_string(&mut out)
            .unwrap();
        out.lines().map(str::to_string).collect()
    }

    #[test]
    fn launch_records_before_the_child_starts_even_when_spawn_fails() {
        // spec 104 criterion 1: the ordering IS the criterion. Point `bin` at a path
        // that cannot possibly exist, so `Command::spawn` fails - and prove the
        // SpawnLaunched record was still written, because it happens before the spawn
        // attempt, not after a successful one.
        let driver = Driver {
            bin: "/definitely/does/not/exist/claude-xyz".to_string(),
            rigger_bin: "rigger".to_string(),
            ..Driver::default()
        };
        let store = Store::open(":memory:").unwrap();
        let mut o = opts("u104-launch/implementer#0");
        o.run_id = "run-1".to_string();

        let err = match driver.launch(&AgentDef::default(), "do the thing", &o, &store) {
            Ok(_) => panic!("spawning a nonexistent binary must fail"),
            Err(e) => e,
        };
        assert!(err.0.contains("u104-launch/implementer#0"), "{}", err.0);

        let recorded = store
            .read_stream(crate::progress::STREAM, 0, Direction::Forward)
            .unwrap();
        assert_eq!(
            recorded.len(),
            1,
            "the launch record lands even though the process never started"
        );
        assert_eq!(recorded[0].type_, crate::progress::TYPE_SPAWN_LAUNCHED);
        let sl: SpawnLaunched = serde_json::from_slice(&recorded[0].data).unwrap();
        assert_eq!(sl.spawn, "u104-launch/implementer#0");
        assert!(!sl.session_id.is_empty(), "a session id was still minted");
    }

    #[test]
    fn launch_records_a_lost_write_loudly_rather_than_spawning_blind() {
        // The store write itself failing (e.g. down) must also stop the launch before
        // any process exists - never silently spawn against an unrecorded launch.
        let driver = Driver::default();
        let o = opts("u/implementer#0");
        let err = match driver.launch(&AgentDef::default(), "task", &o, &SilentStore) {
            Ok(_) => panic!("a launch nobody can record must not proceed"),
            Err(e) => e,
        };
        assert!(
            err.0.to_lowercase().contains("nothing") || err.0.contains("u/implementer#0"),
            "{}",
            err.0
        );
    }

    #[test]
    fn launch_starts_the_child_in_the_spawns_dir_with_a_minted_session_id() {
        let dir = std::env::temp_dir();
        let driver = Driver {
            bin: fixture_bin(),
            rigger_bin: "rigger".to_string(),
            ..Driver::default()
        };
        let store = Store::open(":memory:").unwrap();
        let mut o = opts("u/implementer#0");
        o.dir = dir.to_string_lossy().into_owned();

        let mut launch = driver
            .launch(&AgentDef::default(), "the task", &o, &store)
            .unwrap();

        // A real UUID v4 was minted and threaded onto argv as --session-id.
        assert_eq!(
            launch
                .args
                .iter()
                .position(|x| x == "--session-id")
                .map(|i| &launch.args[i + 1]),
            Some(&launch.session_id)
        );
        assert_eq!(
            launch.session_id.len(),
            36,
            "session id: {}",
            launch.session_id
        );

        let lines = read_fixture_lines(&mut launch.child);
        let cwd = std::fs::canonicalize(&lines[0]).unwrap();
        let want = std::fs::canonicalize(&dir).unwrap();
        assert_eq!(cwd, want, "child ran in the spawn's dir");
    }

    #[test]
    #[serial_test::serial(claude_code_ambient_env)]
    fn launch_applies_opts_env_and_inherits_the_ambient_credential_untouched() {
        // spec 104 criterion 1: "the environment is the operator's plus opts.env; the
        // host sets and reads no credential variable" - proven two ways in one test: a
        // pre-existing ambient var (standing in for the operator's credential) reaches
        // the child UNCHANGED with no entry in opts.env for it, and an opts.env pair
        // reaches the child too. `#[serial]` (docs/architecture-addendum, Cargo.toml's
        // own comment on `serial_test`): this test mutates the process-global
        // environment, a resource `cargo test`'s default parallelism must not interleave.
        std::env::set_var("ANTHROPIC_API_KEY", "operators-own-credential");
        let driver = Driver {
            bin: fixture_bin(),
            rigger_bin: "rigger".to_string(),
            ..Driver::default()
        };
        let store = Store::open(":memory:").unwrap();
        let mut o = opts("u/implementer#0");
        o.env = vec![("RIGGER_TEST_VAR".to_string(), "from-opts-env".to_string())];
        assert!(
            !o.env.iter().any(|(k, _)| k == "ANTHROPIC_API_KEY"),
            "the credential is never IN opts.env - it must ride the inherited environment"
        );

        let mut launch = driver
            .launch(&AgentDef::default(), "task", &o, &store)
            .unwrap();
        let lines = read_fixture_lines(&mut launch.child);
        assert!(
            lines.contains(&"ANTHROPIC_API_KEY=operators-own-credential".to_string()),
            "lines: {lines:?}"
        );
        assert!(
            lines.contains(&"RIGGER_TEST_VAR=from-opts-env".to_string()),
            "lines: {lines:?}"
        );
        std::env::remove_var("ANTHROPIC_API_KEY");
    }

    #[test]
    fn launch_writes_the_task_as_the_first_stream_json_user_message() {
        let driver = Driver {
            bin: fixture_bin(),
            rigger_bin: "rigger".to_string(),
            ..Driver::default()
        };
        let store = Store::open(":memory:").unwrap();
        let o = opts("u/implementer#0");

        let mut launch = driver
            .launch(&AgentDef::default(), "implement the thing", &o, &store)
            .unwrap();
        let lines = read_fixture_lines(&mut launch.child);
        let stdin_line = lines
            .iter()
            .find(|l| l.starts_with("STDIN="))
            .expect("fixture echoed the first stdin line");
        let json_text = stdin_line.trim_start_matches("STDIN=");
        let v: serde_json::Value = serde_json::from_str(json_text).unwrap();
        assert_eq!(v["type"], "user");
        assert_eq!(v["message"]["role"], "user");
        assert_eq!(v["message"]["content"], "implement the thing");
    }

    #[test]
    fn launch_leaves_stdin_open_for_the_reader_to_close() {
        // THE STREAM (criterion 2) closes input after the first `result`, not this
        // criterion - so `launch` must hand back a child whose stdin handle is still
        // present (not dropped/closed) after writing the first message.
        let driver = Driver {
            bin: fixture_bin(),
            rigger_bin: "rigger".to_string(),
            ..Driver::default()
        };
        let store = Store::open(":memory:").unwrap();
        let o = opts("u/implementer#0");
        let mut launch = driver
            .launch(&AgentDef::default(), "task", &o, &store)
            .unwrap();
        assert!(
            launch.child.stdin.is_some(),
            "stdin must stay open after the first message"
        );
        // Reap: the fixture reads its one line and exits on its own, so this never
        // blocks - matching every sibling test in this file (`read_fixture_lines`'s
        // own `.wait()`), never leaving a zombie behind.
        launch.child.wait().unwrap();
    }

    #[test]
    fn a_launch_relaunches_at_a_new_ordinal_with_no_resumed_from_this_spec() {
        // CONSTRAINTS WALK: "Relaunch - a new session id and the next launch ordinal."
        // Every launch spec 104 itself performs is fresh (resume is spec 105's), so two
        // successive launches for the same spawn get two DIFFERENT session ids even when
        // the caller advances only `launch`.
        let driver = Driver {
            bin: fixture_bin(),
            rigger_bin: "rigger".to_string(),
            ..Driver::default()
        };
        let store = Store::open(":memory:").unwrap();
        let mut o = opts("u/implementer#0");
        o.launch = 0;
        let mut first = driver
            .launch(&AgentDef::default(), "t", &o, &store)
            .unwrap();
        first.child.wait().unwrap();

        o.launch = 1;
        let mut second = driver
            .launch(&AgentDef::default(), "t", &o, &store)
            .unwrap();
        second.child.wait().unwrap();

        assert_ne!(first.session_id, second.session_id);
        let recorded = store
            .read_stream(crate::progress::STREAM, 0, Direction::Forward)
            .unwrap();
        assert_eq!(recorded.len(), 2);
        let launches: Vec<u32> = recorded
            .iter()
            .map(|e| {
                serde_json::from_slice::<SpawnLaunched>(&e.data)
                    .unwrap()
                    .launch
            })
            .collect();
        assert_eq!(launches, vec![0, 1]);
    }

    // ---- THE STREAM (spec 104 criterion 2): the pure per-line mappings ----

    #[test]
    fn stream_path_mirrors_spawn_scratch_paths_layout() {
        let p = stream_path("/scratch", "run-1", "u1/implementer#0", 2).unwrap();
        assert_eq!(
            p,
            std::path::PathBuf::from("/scratch/agent-stream/run-1/u1_2fimplementer_230.2.jsonl")
        );
    }

    #[test]
    fn stream_path_omits_the_run_subdir_for_an_empty_run_id() {
        let p = stream_path("/scratch", "", "u1/implementer#0", 0).unwrap();
        assert_eq!(
            p,
            std::path::PathBuf::from("/scratch/agent-stream/u1_2fimplementer_230.0.jsonl")
        );
    }

    #[test]
    fn stream_path_is_none_for_an_empty_spawn_id() {
        assert_eq!(stream_path("/scratch", "run-1", "", 0), None);
    }

    #[test]
    fn stream_path_is_none_rather_than_relative_for_an_empty_scratch_root() {
        // Spec 104 round-4 REQUIRED FIX 3 regression, pinned directly at this call site
        // (the periphery suite's `spawn_with_an_empty_scratch_root_never_writes_relative_
        // to_cwd` already proves it end to end through `spawn()`; this is the cheap, pure
        // unit-level proof of the same guard, now delegated to `liveness::scratch_subpath`).
        assert_eq!(stream_path("", "run-1", "u1/implementer#0", 0), None);
    }

    #[test]
    fn api_retry_line_carries_category_attempt_and_delay() {
        // The exact shape a probe of the real CLI showed (spec 104 Problem section).
        let v: Value = serde_json::from_str(
            r#"{"type":"system","subtype":"api_retry","attempt":1,"max_retries":10,
                "retry_delay_ms":618,"error_status":401,"error":"authentication_failed"}"#,
        )
        .unwrap();
        let line = api_retry_line(&v);
        assert!(line.contains("authentication_failed"), "{line}");
        assert!(line.contains('1'), "{line}");
        assert!(line.contains("618"), "{line}");
    }

    #[test]
    fn api_retry_line_degrades_gracefully_on_missing_fields() {
        let v: Value = serde_json::from_str(r#"{"type":"system","subtype":"api_retry"}"#).unwrap();
        let line = api_retry_line(&v);
        assert!(line.contains("unknown"), "{line}");
    }

    #[test]
    fn spawn_result_from_maps_the_real_results_shape() {
        // Grounded on a real `claude -p --output-format stream-json` result line.
        let v: Value = serde_json::from_str(
            r#"{"type":"result","subtype":"success","is_error":false,"num_turns":3,
                "result":"done: the answer is 42","session_id":"sess-42",
                "total_cost_usd":0.0456,
                "usage":{"input_tokens":100,"output_tokens":50,
                         "cache_creation_input_tokens":20,"cache_read_input_tokens":10},
                "permission_denials":[]}"#,
        )
        .unwrap();
        let o = opts("u1/implementer#0");
        let res = spawn_result_from(&v, &o, "claude-sonnet-4-5-20250929", 2);

        assert_eq!(res.id, "u1/implementer#0");
        assert_eq!(res.output, "done: the answer is 42");
        assert!(!res.is_error());
        assert_eq!(res.resolved_model(), "claude-sonnet-4-5-20250929");
        assert_eq!(res.meta["session_id"], "sess-42");
        assert_eq!(res.meta["usage"]["input"], 100);
        assert_eq!(res.meta["usage"]["output"], 50);
        assert_eq!(res.meta["usage"]["cache_creation"], 20);
        assert_eq!(res.meta["usage"]["cache_read"], 10);
        assert_eq!(res.meta["turns"], 3);
        assert_eq!(res.meta["cost_usd"], 0.0456);
        // The reader's OWN accumulated count wins over whatever the result message
        // itself carries (here an empty array) - see spawn_result_from's doc.
        assert_eq!(res.meta["permission_denials"], 2);
    }

    #[test]
    fn spawn_result_from_degrades_gracefully_on_missing_fields() {
        let v: Value = serde_json::from_str(r#"{"type":"result","result":"ok"}"#).unwrap();
        let o = opts("u/implementer#0");
        let res = spawn_result_from(&v, &o, "", 0);
        assert_eq!(res.output, "ok");
        assert_eq!(res.meta["usage"]["input"], 0);
        assert_eq!(res.meta["turns"], 0);
        assert_eq!(res.meta["cost_usd"], 0.0);
    }

    // ---- reconcile_on_start (spec 104 criterion 6, supervisor start-up reconciliation) ----

    /// A long-lived process rooted at `dir`, standing in for a hand-off-left-behind agent
    /// process - mirrors `reap::tests::sleeper_in`'s exact shape (that helper is private
    /// to its own module, so this is the local copy this module's own tests need).
    fn sleeper_in(dir: &Path) -> Child {
        Command::new("sleep")
            .arg("300")
            .current_dir(dir)
            .spawn()
            .expect("spawn sleep")
    }

    /// Poll until `pred` holds or a generous timeout elapses; returns whether it held.
    fn wait_until(mut pred: impl FnMut() -> bool) -> bool {
        for _ in 0..200 {
            if pred() {
                return true;
            }
            std::thread::sleep(std::time::Duration::from_millis(25));
        }
        false
    }

    fn open_launch(spawn: &str, launch: u32, session_id: &str) -> SpawnLaunched {
        SpawnLaunched {
            spawn: spawn.to_string(),
            launch,
            session_id: session_id.to_string(),
            resumed_from: None,
            started: 1,
            ended: None,
            class: None,
        }
    }

    #[test]
    fn reconcile_on_start_closes_every_open_launch_as_interrupted() {
        let progress = Store::open(":memory:").unwrap();
        progress_store::record_launch(
            &progress,
            "run-1",
            &open_launch("u1/implementer#0", 0, "sess-a"),
        )
        .unwrap();

        let driver = Driver {
            progress_store: &progress,
            ..Driver::default()
        };
        let events = progress
            .read_stream(crate::progress::STREAM, 0, Direction::Forward)
            .unwrap();
        let reconciled = driver.reconcile_on_start(&events, "run-1").unwrap();
        assert_eq!(reconciled, vec!["u1/implementer#0".to_string()]);

        let after = progress
            .read_stream(crate::progress::STREAM, 0, Direction::Forward)
            .unwrap();
        assert_eq!(
            after.len(),
            2,
            "the open record plus its new closing record"
        );
        assert!(
            crate::progress::open_launches(&after).unwrap().is_empty(),
            "the launch must no longer read as open"
        );
        let closing: SpawnLaunched = serde_json::from_slice(&after[1].data).unwrap();
        assert_eq!(closing.ended.as_deref(), Some("interrupted"));
        assert_eq!(closing.spawn, "u1/implementer#0");
        assert_eq!(closing.launch, 0);
    }

    #[test]
    fn reconcile_on_start_is_a_noop_when_nothing_is_open() {
        let progress = Store::open(":memory:").unwrap();
        let driver = Driver {
            progress_store: &progress,
            ..Driver::default()
        };
        let reconciled = driver.reconcile_on_start(&[], "run-1").unwrap();
        assert!(reconciled.is_empty());
        assert!(
            progress
                .read_stream(crate::progress::STREAM, 0, Direction::Forward)
                .unwrap()
                .is_empty(),
            "nothing open means nothing written"
        );
    }

    #[test]
    fn reconcile_on_start_leaves_an_already_closed_launch_alone() {
        let progress = Store::open(":memory:").unwrap();
        progress_store::record_launch(
            &progress,
            "run-1",
            &open_launch("u1/implementer#0", 0, "sess-a"),
        )
        .unwrap();
        progress_store::record_launch(
            &progress,
            "run-1",
            &SpawnLaunched::closed("u1/implementer#0", 0, "sess-a", "completed", ""),
        )
        .unwrap();

        let driver = Driver {
            progress_store: &progress,
            ..Driver::default()
        };
        let events = progress
            .read_stream(crate::progress::STREAM, 0, Direction::Forward)
            .unwrap();
        let reconciled = driver.reconcile_on_start(&events, "run-1").unwrap();
        assert!(
            reconciled.is_empty(),
            "an already-closed launch is not re-closed"
        );
        assert_eq!(
            progress
                .read_stream(crate::progress::STREAM, 0, Direction::Forward)
                .unwrap()
                .len(),
            2,
            "no third event was appended"
        );
    }

    #[test]
    fn reconcile_on_start_skips_reaping_a_malformed_spawn_id_but_still_closes_it() {
        let progress = Store::open(":memory:").unwrap();
        progress_store::record_launch(&progress, "run-1", &open_launch("bare-id", 0, "sess-a"))
            .unwrap();

        let scratch = tempfile::tempdir().unwrap();
        let driver = Driver {
            progress_store: &progress,
            scratch_root: scratch.path().to_string_lossy().into_owned(),
            ..Driver::default()
        };
        let events = progress
            .read_stream(crate::progress::STREAM, 0, Direction::Forward)
            .unwrap();
        // Must not panic on an id with no `/unit` half (spawn::unit_of returns None).
        let reconciled = driver.reconcile_on_start(&events, "run-1").unwrap();
        assert_eq!(reconciled, vec!["bare-id".to_string()]);
    }

    #[test]
    fn reconcile_on_start_reaps_a_process_still_rooted_in_the_spawns_worktree() {
        let scratch = tempfile::tempdir().unwrap();
        let scratch_root = scratch.path().to_string_lossy().into_owned();
        // The SAME deterministic path every other worktree caller derives (spec 104
        // criterion 6's own decision: no new `dir` field, this pure fn is the authority).
        let unit_dir = crate::conductor::unit_worktree_dir(&scratch_root, "u1");
        std::fs::create_dir_all(&unit_dir).unwrap();
        let mut left_behind = sleeper_in(Path::new(&unit_dir));

        let progress = Store::open(":memory:").unwrap();
        progress_store::record_launch(
            &progress,
            "run-1",
            &open_launch("u1/implementer#0", 0, "sess-a"),
        )
        .unwrap();

        let driver = Driver {
            progress_store: &progress,
            scratch_root: scratch_root.clone(),
            ..Driver::default()
        };
        let events = progress
            .read_stream(crate::progress::STREAM, 0, Direction::Forward)
            .unwrap();
        driver.reconcile_on_start(&events, "run-1").unwrap();

        assert!(
            wait_until(|| matches!(left_behind.try_wait(), Ok(Some(_)))),
            "a process still rooted in the spawn's worktree must be reaped before relaunch"
        );
    }

    #[test]
    fn reconcile_on_start_reaps_nothing_when_no_scratch_root_is_configured() {
        // An empty scratch_root (no scratch configured, or a test that does not care) must
        // degrade to "close the record, reap nothing" - never guess a path to reap.
        let progress = Store::open(":memory:").unwrap();
        progress_store::record_launch(
            &progress,
            "run-1",
            &open_launch("u1/implementer#0", 0, "sess-a"),
        )
        .unwrap();
        let driver = Driver {
            progress_store: &progress,
            scratch_root: String::new(),
            ..Driver::default()
        };
        let events = progress
            .read_stream(crate::progress::STREAM, 0, Direction::Forward)
            .unwrap();
        // Must not panic reaping "/rigger-wt-u1" or any other guessed absolute path.
        let reconciled = driver.reconcile_on_start(&events, "run-1").unwrap();
        assert_eq!(reconciled, vec!["u1/implementer#0".to_string()]);
    }
}
