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
    /// with the process still possibly running behind it; dropping it - explicitly, right
    /// after the loop, on every exit alike - ends it: `try_wait` first (a clean EOF means
    /// the child has typically already exited, so this alone reaps it with no signal
    /// sent), falling back to a best-effort end-then-collect through the same handle
    /// otherwise. That mirrors `launch()`'s own "never returns `Err` with an
    /// unaccounted-for child still running behind it" contract without this function
    /// needing to track which shape of exit it hit.
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
    fn read_stream(&self, launch: Launch, opts: &SpawnOpts) -> Result<AgentResult, Error> {
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

        let marker = liveness::marker_path(&self.scratch_root, &opts.run_id, &opts.id);
        let mut stream_file = open_stream_file(
            stream_path(&self.scratch_root, &opts.run_id, &opts.id, opts.launch).as_deref(),
        );

        let mut resolved_model = String::new();
        let mut permission_denials: u64 = 0;
        let mut result: Option<AgentResult> = None;

        for line in BufReader::new(stdout).lines() {
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

        // Reap through the ONE canonical mechanism, on every exit path alike (a clean
        // EOF, or the `break` above on a post-result read error): `ReapedChild::drop`
        // tries a non-blocking `try_wait` first - on a clean EOF the child has typically
        // already exited, so this alone reaps it with no signal sent - and falls back to
        // a best-effort end-then-collect through the same handle only when the process
        // is not yet known to have exited. Explicit, right here, rather than left to run
        // at the function's end: the join just below depends on the child's stderr pipe
        // having already closed.
        drop(reaper);
        // Best-effort join: diagnostic-only drain (see the doc above), and the reap just
        // above guarantees the child's stderr pipe has closed by now, so the thread has
        // already hit EOF or is about to - never a caller-visible wait for its own sake.
        if let Some(handle) = stderr_drain {
            let _ = handle.join();
        }

        result.ok_or_else(|| {
            Error(format!(
                "claude_code driver: {:?}: the agent stream ended with no result",
                opts.id
            ))
        })
    }

    /// Best-effort progress line (spec 14's mechanism, spec 104's own writer): a lost
    /// progress line is a smaller loss than a lost launch record or a lost result, so
    /// this never fails the spawn the way [`Driver::launch`]'s `SpawnLaunched` write does.
    fn record_progress(&self, opts: &SpawnOpts, activity: &str) {
        let _ = progress_store::record(self.progress_store, &opts.run_id, &opts.id, activity);
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
        self.read_stream(launch, opts)
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
}
