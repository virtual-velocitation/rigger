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
use std::process::{Child, ChildStderr, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::Value;

use crate::conductor::{
    classify_failure, no_result_error, AgentDriver, AgentResult, Error, SpawnOpts,
};
use crate::config::AgentDef;
use crate::eventstore::{Direction, EventStore};
use crate::liveness;
use crate::progress::{self, SpawnLaunched};
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

/// Bounds [`Driver::read_stream`]'s ORDINARY (non-STOP) exit joins - the stderr-drain and
/// stdout-reader threads, right after `dash::ReapedChild::drop` reaps the one child this
/// function held - a dedicated diagnostic-drain bound, DELIBERATELY NEVER
/// [`Driver::stop_grace`] (adj-u104stop-r4-verdict-reject REQUIRED FIX,
/// `op-104-stop-ordinary-path-drain-bound`): `stop_grace` is THE STOP's own wait for a
/// graceful exit and means nothing on this path, where the child has already exited
/// cleanly - reusing it here stalled an already-captured, fully successful result by up
/// to 2x its value (60s in production) whenever a descendant inherited a copy of either
/// pipe, and permanently leaked the blocked reader thread if that descendant never exited
/// (independently reproduced three ways: `adv-u104stop-r4-ordinary-path-reuses-stop-grace-
/// for-60s-latency-and-thread-leak`). The common case - the pipe closes the instant the
/// child itself exits - still joins in microseconds; this bound only matters for the rare
/// stranger holding a leaked copy, and a few seconds is enough to distinguish "closing
/// right now" from "never closing", which is all a diagnostic drain needs to prove. Not
/// injected (unlike `stop_grace`): this is a fixed internal implementation bound for a
/// best-effort diagnostic wait, never a caller-configurable timeout for a concern anyone
/// outside this module has a reason to tune.
const ORDINARY_DRAIN_JOIN_BOUND: std::time::Duration = std::time::Duration::from_secs(2);

/// A configured binary, or `default` (resolved on `$PATH`) when none is configured - the
/// ONE empty-means-default rule both [`Driver::bin`] and [`Driver::rigger_bin`] follow.
fn bin_or_path_default<'s>(configured: &'s str, default: &'s str) -> &'s str {
    if configured.is_empty() {
        default
    } else {
        configured
    }
}

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
        let args = build_args(
            agent,
            opts,
            &session_id,
            bin_or_path_default(&self.rigger_bin, "rigger"),
        );

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

        let bin = bin_or_path_default(&self.bin, "claude");
        let mut cmd = crate::subprocess::command_in(bin, &opts.dir);
        cmd.args(&args);
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
    /// path. The drain still discards everything but a bounded TAIL
    /// ([`STDERR_TAIL_CAP`] bytes, [`drain_child_stderr_tail`]) - never this criterion's
    /// record of truth on its own - which criterion 5's no-result path below folds into
    /// the class it reports (CONSTRAINTS WALK: "a fault of class `unknown` carrying the
    /// stderr tail").
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
            .map(|stderr| std::thread::spawn(move || drain_child_stderr_tail(stderr)));
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
        // FAILURE CLASS's second-priority source (spec 104 criterion 5, Design: "the last
        // `api_retry.error`"): the category of the MOST RECENT `system/api_retry` line THIS
        // launch's own reader has seen so far - `None` until the first one arrives, then
        // overwritten by every later one, so a session that ends without a result classifies
        // from whichever category was live right before it stopped.
        let mut last_api_retry_category: Option<String> = None;
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
                // `Some(0)` (elapsed lands exactly on the bound) and `None` (already past
                // it) both mean "expire without blocking" - `unwrap_or_default` folds them
                // into the same `Duration::ZERO` a guarded match once spelled out, with no
                // redundant guard clause a mutation sweep would find equivalent (the
                // guarded arm's `_` fallback already returned this same zero value).
                bound.checked_sub(elapsed).unwrap_or_default()
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
                    // FAILURE CLASS's second-priority source (criterion 5): the LATEST
                    // category wins, a later line overwriting an earlier one exactly like
                    // `resolved_model`/`permission_denials` accumulate as the stream plays.
                    if let Some(category) = v.get("error").and_then(Value::as_str) {
                        last_api_retry_category = Some(category.to_string());
                    }
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
        // The reap just above ends only the ONE child handle it holds (spec 104 criterion 6
        // round-4 fix): a descendant that inherited a copy of either pipe's write end
        // before going silent keeps it open regardless, so both joins below are bounded
        // ([`Driver::join_within`]) rather than a bare `join()` that could wait on such a
        // stranger forever - never a caller-visible wait for its own sake in the ordinary
        // case, where the thread has already hit EOF or is about to. Bounded by
        // [`ORDINARY_DRAIN_JOIN_BOUND`], NEVER `self.stop_grace` - this is not THE STOP,
        // and `stop_grace` has no meaning on a path where the child has already exited
        // cleanly (adj-u104stop-r4-verdict-reject REQUIRED FIX,
        // `op-104-stop-ordinary-path-drain-bound`). The stderr join's own return value IS
        // this criterion's bounded TAIL (never merely diagnostic now that FAILURE CLASS's
        // no-result path folds it into a classification below) - a handle that outruns
        // the bound, or whose thread panicked, degrades to an empty tail, same as no
        // stderr pipe at all.
        let stderr_tail = stderr_drain
            .and_then(|handle| self.join_within(opts, "stderr", handle, ORDINARY_DRAIN_JOIN_BOUND))
            .unwrap_or_default();
        self.join_within(opts, "stdout", stdout_reader, ORDINARY_DRAIN_JOIN_BOUND);

        result.ok_or_else(|| self.classify_no_result(opts, &last_api_retry_category, &stderr_tail))
    }

    /// A FAILURE HAS A CLASS (spec 104 criterion 5): build the `Error` for a session that
    /// ended with no `result` - the natural loop-exhaustion/clean-EOF exit
    /// [`read_stream`](Self::read_stream)'s bottom falls through to, exactly the
    /// CONSTRAINTS WALK shape ("the child exits before `system/init` ... a fault of class
    /// `unknown` carrying the stderr tail, never a hang"). Reads the FIRST-priority source
    /// (the `StopFailure` hook's record, keyed on `opts.id` AND scoped to `opts.run_id` -
    /// adj-u104c5 REQUIRED FIX 1, adv-u104c5-stopfailure-crosses-run-boundary: a stale
    /// record from an old or unrelated run must never outrank the live session's own
    /// `api_retry` category, matching `rigger status`'s own established progress-event
    /// run-scoping convention at `src/main.rs`) from THIS driver's own progress store,
    /// folds it against `last_api_retry_category` via [`classify_failure`], and embeds the
    /// result via [`no_result_error`] ahead of a
    /// human-readable message carrying the class and `stderr_tail` (lossily decoded,
    /// trimmed). A progress-store read failure degrades to "no `StopFailure` record found"
    /// (`None`) rather than failing the whole classification - the second source
    /// (`last_api_retry_category`) and the `unknown` floor both still apply, so a store
    /// hiccup here never turns a classifiable failure into an opaque one
    /// (`classify_no_result_degrades_to_the_api_retry_floor_when_the_progress_store_read_fails`
    /// below proves this branch is reachable).
    fn classify_no_result(
        &self,
        opts: &SpawnOpts,
        last_api_retry_category: &Option<String>,
        stderr_tail: &[u8],
    ) -> Error {
        let stop_failure_class = self
            .progress_store
            .read_stream(progress::STREAM, 0, Direction::Forward)
            .ok()
            .map(|events| {
                // Same convention as `rigger status`'s own progress-event filter
                // (`src/main.rs`): scope to THIS spawn's run before folding, an empty
                // `run_id` (no run started yet - e.g. a bare unit test) matching every
                // event unscoped.
                events
                    .into_iter()
                    .filter(|e| {
                        opts.run_id.is_empty()
                            || e.meta.get(crate::run::META_RUN_ID).map(String::as_str)
                                == Some(opts.run_id.as_str())
                    })
                    .collect::<Vec<_>>()
            })
            .and_then(|events| progress::latest_stop_failure_class(&events, &opts.id));
        let class = classify_failure(
            stop_failure_class.as_deref(),
            last_api_retry_category.as_deref(),
        );
        let tail = String::from_utf8_lossy(stderr_tail);
        no_result_error(
            class,
            format!(
                "claude_code driver: {:?}: the agent stream ended with no result (class \
                 {class}; stderr tail: {})",
                opts.id,
                tail.trim()
            ),
        )
    }

    /// THE STOP sequence itself (spec 104 criterion 6), factored out of [`Driver::read_stream`]'s
    /// loop so its one call site there reads as a single named step: "closes the session's
    /// input stream, waits a grace period, then ends the child through the sanctioned
    /// lifecycle helper on the child's own handle" (architecture addendum §4.6), then records
    /// the EXISTING liveness-fault shape and closes the launch record `stopped`. THE STOP
    /// sweeps nothing beyond that one child's own PID TREE - a running spawn's worktree can be shared concurrently by sibling spawns in the same unit (the review
    /// fan-out's own lenses, `run_review_agents_concurrently`), and a cwd-scanned match has no
    /// notion of which spawn owns a process, so a cwd sweep here would signal a live sibling's
    /// own legitimate process, not just this spawn's leftovers (`op-104-stop-no-sweep-at-wall-
    /// clock-stop`, `adv-u104stop-r2-stop-reaps-concurrent-sibling-worktree`); ending the
    /// child's own descendants BY PROCESS TREE instead (round-4 fix, decision
    /// `op-104-stop-end-the-tree-and-bound-the-joins`) carries no such risk, since a sibling's
    /// process hangs off a different parent and can never appear in this walk regardless of
    /// what it shares on disk. Always returns `Err` - a launch that never produced a result
    /// stays a failure from this driver's own return value, exactly like the "agent stream
    /// ended with no result" ending just above; a caller wanting the no-attempt-charged
    /// semantics reads [`SpawnResult::is_liveness_fault`] off the run store this durably
    /// wrote, the SAME way `liveness::sweep`'s callers already do for the stepwise driver's
    /// own fault.
    fn stop_for_wall_clock_silence(
        &self,
        mut reaper: crate::dash::ReapedChild,
        stderr_drain: Option<std::thread::JoinHandle<Vec<u8>>>,
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

        // Snapshot the child's own descendants RIGHT NOW (spec 104 criterion 6 round-4
        // fix, decision `op-104-stop-end-the-tree-and-bound-the-joins`) - BEFORE the
        // passive grace wait just below, not after, and not inside `end_child` (which is
        // not even called until that wait ends): a descendant a session forked but never
        // `exec`'d reparents to its nearest surviving ancestor the INSTANT the session
        // itself exits, gracefully or not, so a snapshot taken any later would already be
        // too late for exactly the well-behaved case the grace wait below exists for - a
        // session that notices its input closed and winds down entirely on its own,
        // orphaning whatever it forked before reap::end_child ever gets a chance to look.
        let descendants = crate::reap::snapshot_descendants(reaper.child_mut().id());

        // "waits a grace period" (self.stop_grace - 30s in production; an injected
        // shorter one in a test proving the full sequence): poll for the child exiting on
        // its own, without blocking the full grace when it already has. Bounded by
        // `saturating_duration_since` rather than a raw `now() < deadline` comparison: the
        // exact tie between `now()` and a fixed `Instant` a prior addition computed is
        // unobservable to any test, so a boundary-operator mutant there would be
        // equivalent; this shape leaves no such operator for one to mutate.
        let deadline = std::time::Instant::now() + self.stop_grace;
        while !deadline
            .saturating_duration_since(std::time::Instant::now())
            .is_zero()
        {
            if matches!(reaper.child_mut().try_wait(), Ok(Some(_))) {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }

        // "then ends the child through the sanctioned lifecycle helper on the child's own
        // handle" - the handle-bound production helper (reap.rs, spec 104 criterion 6's own
        // decision `u104-stop-uses-reap-child-handle`): a no-op on the child itself if the
        // grace wait above already collected it. It also ends the descendants snapshotted
        // above regardless (round-4 fix) - a process the session forked but never `exec`'d,
        // so it is reaped too rather than merely outliving the one handle this function
        // ever held, whether the session itself needed signalling or wound down on its own.
        crate::reap::end_child(reaper.child_mut(), descendants);

        // That descendant walk is matched by PID-TREE membership alone, deliberately NEVER
        // by cwd - NO worktree-wide sweep here: this spawn's worktree can be shared, right now, by a live
        // SIBLING spawn (the review fan-out's own concurrent lenses,
        // `run_review_agents_concurrently`, all passed the same dir), and a cwd-scanned
        // match has no notion of which spawn owns a process - a sweep here would signal
        // that sibling's own legitimate, in-progress work, not just this spawn's leftovers
        // (round-2 reject `adv-u104stop-r2-stop-reaps-concurrent-sibling-worktree`, upheld
        // against the round-1 fix this replaces). A sibling's process hangs off a DIFFERENT
        // parent, so it can never appear in this child's own descendant tree regardless of
        // what it shares on disk - safe by construction, not by omission.
        //
        // A descendant that had ALREADY escaped this child's process tree before the
        // snapshot above ever ran (reparented to init by a double fork, say) is invisible
        // to this walk and stays untouched - the bounded joins just below are what keep
        // THE STOP returning regardless of what such a stranger still does with the pipe.

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
        // Bounded (spec 104 criterion 6 round-4 fix, [`Driver::join_within`]): the child's
        // own pipe copy closes the moment `end_child` above finishes it and its known
        // descendants, but a descendant that had ALREADY escaped the tree before that
        // snapshot ran still holds its own copy open, and no signal this function sent
        // could ever reach it - this is the backstop that keeps THE STOP returning anyway.
        // Bounded by `self.stop_grace` here (THIS is THE STOP's own wait-for-a-graceful-
        // exit concern, unlike `read_stream`'s ORDINARY-exit joins just below, which use
        // the dedicated `ORDINARY_DRAIN_JOIN_BOUND` instead).
        if let Some(handle) = stderr_drain {
            self.join_within(opts, "stderr", handle, self.stop_grace);
        }
        self.join_within(opts, "stdout", stdout_reader, self.stop_grace);

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

    /// Join `handle` (a stdout-reader or stderr-drain thread), but never past `bound`
    /// (spec 104 criterion 6 round-4 fix, decision
    /// `op-104-stop-end-the-tree-and-bound-the-joins`): a process the driven child forked
    /// but never `exec`'d can inherit a copy of the stdout/stderr pipe's write end, and once
    /// that copy has ALREADY escaped the child's own process tree - reparented before
    /// [`crate::reap::end_child`]'s descendant snapshot ever ran, so no signal this host
    /// sends can reach it - nothing this function does closes it: the thread blocked
    /// reading that pipe would otherwise never see EOF, and neither [`Driver::read_stream`]
    /// nor [`Driver::stop_for_wall_clock_silence`] would ever return. Polling
    /// `JoinHandle::is_finished` against a deadline, rather than calling `join()` directly,
    /// bounds the CALLER's own return time regardless of what such a stranger does with the
    /// pipe - the SAME seam every join of a reader or drain thread in this file goes
    /// through, so a stop (and an ordinary stream end) both return on a bounded clock no
    /// matter who else holds the pipe.
    ///
    /// `bound` is the caller's own concern, never this function's: THE STOP's two joins
    /// pass `self.stop_grace` (its own wait-for-a-graceful-exit duration, injected and
    /// test-overridable), while [`Driver::read_stream`]'s ORDINARY-exit joins pass the
    /// dedicated, un-injected [`ORDINARY_DRAIN_JOIN_BOUND`] - the two concerns share this
    /// mechanism but must never share a magnitude (adj-u104stop-r4-verdict-reject REQUIRED
    /// FIX, `op-104-stop-ordinary-path-drain-bound`: reusing `stop_grace` on the ordinary
    /// path stalled an already-captured successful result by up to 2x its value).
    ///
    /// `label` names which pipe this was, for the progress line recorded when `handle`
    /// outruns the deadline - the caller's only visible trace of a stranger it can neither
    /// identify nor touch. A handle that outruns the deadline is left to run: dropping a
    /// `JoinHandle` detaches its thread rather than cancelling it, so it keeps draining
    /// (harmlessly) until whatever still holds the pipe finally closes it or exits on its
    /// own.
    ///
    /// Generic over the thread's own return type (FAILURE CLASS's stderr-drain thread
    /// returns its captured [`STDERR_TAIL_CAP`]-bounded `Vec<u8>`; the stdout-reader
    /// thread returns nothing) so both share this ONE bounded-join mechanism rather than
    /// a second copy reconciled after the fact. Returns `Some(value)` when `handle`
    /// finished within `bound`, `None` on either a timeout or a join failure (the thread
    /// panicked) - a caller with a value to recover (the stderr tail) degrades that `None`
    /// to its own empty default; a caller with nothing to recover (the stdout reader)
    /// simply discards it.
    fn join_within<T>(
        &self,
        opts: &SpawnOpts,
        label: &str,
        handle: std::thread::JoinHandle<T>,
        bound: std::time::Duration,
    ) -> Option<T> {
        let deadline = std::time::Instant::now() + bound;
        loop {
            if handle.is_finished() {
                return handle.join().ok();
            }
            if std::time::Instant::now() >= deadline {
                self.record_progress(
                    opts,
                    &format!(
                        "stream: a process outside the child's tree still holds the {label} \
                         pipe open - not waiting for it further"
                    ),
                );
                return None;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    }
}

/// The most stderr bytes [`drain_child_stderr_tail`] keeps: enough for a real diagnostic
/// line or two (CONSTRAINTS WALK's "carrying the stderr tail"), small enough that a
/// flooding agent (the stderr-flood fixture writes 1MiB) never grows this thread's own
/// buffer past a bounded size.
const STDERR_TAIL_CAP: usize = 4096;

/// Read `stderr` to EOF, discarding every byte but the LAST [`STDERR_TAIL_CAP`] of them -
/// the concurrent drain [`Driver::read_stream`] starts before its stdout loop so a chatty
/// agent's stderr volume can never deadlock the host (REQUIRED FIX 2), and this criterion
/// (spec 104 criterion 5) is the tail's only reader: a session that ends with no `result`
/// folds it into the class it reports (CONSTRAINTS WALK: "a fault of class `unknown`
/// carrying the stderr tail"). Raw bytes, not lines: stderr carries no promise of being
/// valid UTF-8 or line-delimited the way the stdout stream-json protocol is, and a
/// `BufRead::lines` read that hit invalid UTF-8 would stop draining at exactly the moment
/// the pipe still needs a reader. A read error just ends the drain, returning whatever tail
/// had accumulated so far - there is nothing for this background thread to report to.
fn drain_child_stderr_tail(mut stderr: ChildStderr) -> Vec<u8> {
    let mut tail: Vec<u8> = Vec::with_capacity(STDERR_TAIL_CAP);
    let mut buf = [0u8; 8192];
    while let Ok(n) = stderr.read(&mut buf) {
        if n == 0 {
            break;
        }
        push_bounded_tail(&mut tail, &buf[..n]);
    }
    tail
}

/// Append `chunk` to `tail`, then trim from the FRONT down to at most [`STDERR_TAIL_CAP`]
/// bytes - the pure ring-buffer step [`drain_child_stderr_tail`]'s read loop calls on every
/// chunk, split out so the bounding behavior itself is testable with no process, no pipe,
/// no IO at all.
fn push_bounded_tail(tail: &mut Vec<u8>, chunk: &[u8]) {
    tail.extend_from_slice(chunk);
    // `saturating_sub` rather than a guarded `>` comparison: at the exact boundary
    // `tail.len() == STDERR_TAIL_CAP` a guard would skip the trim, but computing `excess`
    // unconditionally already yields 0 there too (`drain(..0)` is a no-op) - the guard's
    // two branches are byte-identical on every input, so this shape leaves no boundary
    // operator for a mutation sweep to find equivalent.
    let excess = tail.len().saturating_sub(STDERR_TAIL_CAP);
    tail.drain(..excess);
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
    use crate::conductor::{strip_failure_marker, AgentFailure};
    use crate::eventstore::sqlite::Store;
    use crate::eventstore::{Direction, EventStore, SilentStore};
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

    // ---- classify_no_result: run-scoping + degrade-on-store-error ----
    // (adj-u104c5 REQUIRED FIX 1: adv-u104c5-stopfailure-crosses-run-boundary,
    // sdet-u104c5-progress-store-read-failure-branch-untested)

    /// An [`EventStore`] double whose `read_stream` always fails - the counterpart
    /// `SilentStore` above (always succeeds) cannot stand in for, and neither can the
    /// periphery `Fixture` (a real sqlite `:memory:` store, which also always succeeds):
    /// nothing exercised `classify_no_result`'s own `.ok()` degrade-on-error branch until
    /// this double existed. Mirrors `conductor::tests::FailingStore`'s shape (a store
    /// double that fails on demand) narrowed to this module's one call
    /// (`read_stream` only); `append`/`read_all`/`subscribe_*` are never reached by
    /// `classify_no_result`'s read-only path, so they degrade the same inert way
    /// `SilentStore`'s own unused arms do.
    struct FailingReadStore;
    impl EventStore for FailingReadStore {
        fn append(
            &self,
            _stream: &str,
            _expected: crate::eventstore::ExpectedRevision,
            events: &[crate::eventstore::Event],
        ) -> Result<crate::eventstore::Appended, crate::eventstore::Error> {
            Ok(crate::eventstore::Appended::from_placements(vec![
                None;
                events
                    .len()
            ]))
        }
        fn read_stream(
            &self,
            _stream: &str,
            _from: crate::eventstore::Revision,
            _dir: Direction,
        ) -> Result<Vec<crate::eventstore::Event>, crate::eventstore::Error> {
            Err(crate::eventstore::Error::Backend(
                "simulated progress-store read failure".to_string(),
            ))
        }
        fn read_all(
            &self,
            _from: crate::eventstore::Position,
            _dir: Direction,
            _filter: &crate::eventstore::Filter,
        ) -> Result<Vec<crate::eventstore::Event>, crate::eventstore::Error> {
            Ok(Vec::new())
        }
        fn subscribe_all(
            &self,
            _from: crate::eventstore::Position,
            _filter: &crate::eventstore::Filter,
        ) -> Result<crate::eventstore::Subscription, crate::eventstore::Error> {
            Err(crate::eventstore::Error::Backend(
                "the failing double answers reads only".into(),
            ))
        }
        fn subscribe_stream(
            &self,
            _stream: &str,
            _from: crate::eventstore::Revision,
        ) -> Result<crate::eventstore::Subscription, crate::eventstore::Error> {
            Err(crate::eventstore::Error::Backend(
                "the failing double answers reads only".into(),
            ))
        }
    }

    #[test]
    fn classify_no_result_degrades_to_the_api_retry_floor_when_the_progress_store_read_fails() {
        // sdet-u104c5-progress-store-read-failure-branch-untested: `classify_no_result`'s
        // own `.ok()` on the progress-store read (never a `?`) must degrade to "no
        // StopFailure record" rather than failing the whole classification - proven here
        // with a store whose `read_stream` always errs.
        let driver = Driver {
            progress_store: &FailingReadStore,
            ..Driver::default()
        };
        let o = opts("u/implementer#0");
        let e = driver.classify_no_result(&o, &Some("overloaded".to_string()), b"stderr tail");
        assert!(
            strip_failure_marker(&e).contains(&format!("class {}", AgentFailure::Overloaded)),
            "a failed progress-store read must still fall through to the api_retry \
             category, never fail the whole classification: {}",
            e.0
        );
    }

    /// Records a `billing_error` StopFailure for the spawn under `record_run`, classifies a
    /// no-result exit of that spawn in `run-1` whose api_retry category was `overloaded`, and
    /// asserts the classification lands on `expected`.
    fn assert_stop_failure_classification(record_run: &str, expected: AgentFailure) {
        let store = Store::open(":memory:").unwrap();
        crate::progress_store::record_stop_failure(
            &store,
            record_run,
            &crate::progress::StopFailure {
                spawn: "u/implementer#0".to_string(),
                class: "billing_error".to_string(),
            },
        )
        .unwrap();
        let driver = Driver {
            progress_store: &store,
            ..Driver::default()
        };
        let mut o = opts("u/implementer#0");
        o.run_id = "run-1".to_string();
        let e = driver.classify_no_result(&o, &Some("overloaded".to_string()), b"tail");
        assert!(
            strip_failure_marker(&e).contains(&format!("class {expected}")),
            "a StopFailure recorded under run {record_run:?} must classify run-1's no-result \
             exit as {expected}: {}",
            e.0
        );
    }

    crate::test_cases! {
        // adv-u104c5-stopfailure-crosses-run-boundary: a StopFailure record left over from
        // an OLD or UNRELATED run must never outrank the live session's own api_retry
        // category, matching `rigger status`'s established run-scoping convention.
        classify_no_result_ignores_a_stopfailure_record_from_a_different_run:
            assert_stop_failure_classification("some-other-run", AgentFailure::Overloaded);
        classify_no_result_still_honors_a_stopfailure_record_from_the_same_run:
            assert_stop_failure_classification("run-1", AgentFailure::BillingError);
    }

    #[test]
    fn push_bounded_tail_keeps_only_the_last_stderr_tail_cap_bytes() {
        let mut tail: Vec<u8> = Vec::new();
        // More than STDERR_TAIL_CAP across several chunks, so the cap must actually trim
        // across calls - exactly how `drain_child_stderr_tail`'s read loop drives this.
        let chunk = vec![b'x'; 1024];
        for _ in 0..8 {
            push_bounded_tail(&mut tail, &chunk);
        }
        push_bounded_tail(&mut tail, b"TAIL-MARKER");
        assert!(tail.len() <= STDERR_TAIL_CAP);
        assert!(
            String::from_utf8_lossy(&tail).ends_with("TAIL-MARKER"),
            "the LATEST bytes survive, not the earliest"
        );
    }

    #[test]
    fn push_bounded_tail_is_a_no_op_shrink_when_under_the_cap() {
        let mut tail: Vec<u8> = Vec::new();
        push_bounded_tail(&mut tail, b"short");
        assert_eq!(tail, b"short");
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

    crate::test_cases! {
        stream_path_mirrors_spawn_scratch_paths_layout: assert_eq!(
            stream_path("/scratch", "run-1", "u1/implementer#0", 2),
            Some(std::path::PathBuf::from(
                "/scratch/agent-stream/run-1/u1_2fimplementer_230.2.jsonl"
            ))
        );
        stream_path_omits_the_run_subdir_for_an_empty_run_id: assert_eq!(
            stream_path("/scratch", "", "u1/implementer#0", 0),
            Some(std::path::PathBuf::from(
                "/scratch/agent-stream/u1_2fimplementer_230.0.jsonl"
            ))
        );
        stream_path_is_none_for_an_empty_spawn_id:
            assert_eq!(stream_path("/scratch", "run-1", "", 0), None);
        // Spec 104 round-4 REQUIRED FIX 3 regression, pinned directly at this call site
        // (the periphery suite's `spawn_with_an_empty_scratch_root_never_writes_relative_
        // to_cwd` already proves it end to end through `spawn()`; this is the cheap, pure
        // unit-level proof of the same guard, now delegated to `liveness::scratch_subpath`).
        stream_path_is_none_rather_than_relative_for_an_empty_scratch_root:
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
        assert_eq!(
            res.meta_str(crate::spawn::META_RESOLVED_MODEL),
            "claude-sonnet-4-5-20250929"
        );
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
}
