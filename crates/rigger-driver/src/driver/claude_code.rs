//! The native Claude Code agent host (spec 104: rigger hosts its agents as headless
//! Claude Code sessions; `docs/architecture-addendum-claude-code-integration.md` §4).
//! Replaces the blocking `cli` driver's wait-for-exit launch (one shot, learn nothing
//! until exit, infer the rest) with a typed, streaming launch: every fact about an agent,
//! its session, its model, its tools, its permissions, travels as an explicit argv or
//! stream field, never inferred from a prompt sentence or parsed stdout.
//!
//! THE LAUNCH owns argv, cwd, environment and the open half of the launch record. THE STREAM
//! reads what the launch starts: one reader per child, line by line, closing
//! `impl AgentDriver for Driver`. `rigger run` and `rigger critique` launch their agents through
//! this host, each building [`Driver`] at its own composition root.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStderr, ChildStdin, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::Value;

use crate::agent::{
    classify_failure, launch_interrupted, no_result_error, store_fault, AgentDriver, AgentFailure,
    AgentResult, Disposition, Error, SpawnOpts, API_UNSTABLE, SPAWN_BOUND_TOOLS,
    SPAWN_PROGRESS_TOOL,
};
use crate::config::AgentDef;
use crate::driver::replay::{
    answered, read_or_park, reclaim_spawn_registered_scratch, spawn_scratch_path, Logged,
};
use crate::driver::{
    allowed_tools_args, bin_or_path_default, harness_env, json_arg, progress_framed,
    spawn_config_args, system_prompt_file, LaunchSession, SystemPromptFile,
};
use crate::eventstore::EventStore;
use crate::hold_probe::{HoldProbe, OwedProbe, ProbeOutcome};
use crate::hooks;
use crate::liveness;
use crate::progress::{self, ApiRetry, SpawnLaunched};
use crate::progress_store;
use crate::run;
use crate::run::read::{read_current_hold_at_head, read_current_run, released_past};
use crate::run::STREAM;
use crate::run_store;
use crate::spawn::SpawnResult;
use crate::spawn_store;

/// How often [`Driver::read_stream`]'s loop wakes when `max_wall_clock` is 0 (unbounded): a
/// "wake up occasionally" budget, never itself a wall-clock bound - the wake-up never stops
/// the session, it only re-checks whether the child has exited behind a pipe a descendant
/// still holds open (which would otherwise never deliver EOF). Also the shortest a bounded
/// launch waits between two reads of its silence, so a silence frozen just under the bound by
/// a hold or a dead beat cannot spin on the store.
const UNBOUNDED_POLL: std::time::Duration = std::time::Duration::from_secs(5);

/// The wait a bounded launch's wake under its `bound` re-arms for, having read `silent`: the rest
/// of the bound, at least [`UNBOUNDED_POLL`]; a wake that read no silence (`None`) waits the poll,
/// since its read proved nothing about the bound.
fn next_wait(
    bound: std::time::Duration,
    silent: Option<std::time::Duration>,
) -> std::time::Duration {
    silent.map_or(UNBOUNDED_POLL, |silent| {
        bound.saturating_sub(silent).max(UNBOUNDED_POLL)
    })
}

/// Bounds [`Driver::read_stream`]'s ORDINARY (non-STOP) exit joins - the stderr-drain and
/// stdout-reader threads, right after `reaped_child::ReapedChild::drop` reaps the one child this
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

/// THE STOP's production grace: how long a host waits, once it has closed a silent session's
/// input, for the session to exit on its own before ending it ([`Driver::stop_grace`]).
pub const STOP_GRACE: std::time::Duration = std::time::Duration::from_secs(30);

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
    /// 2) records every waiting/denied/unparseable line and every [`ApiRetry`] to
    /// (`crate::progress_store`, `.rigger/progress.db` at the composition root) - injected,
    /// never opened by this driver itself, mirroring how every other store-touching port in
    /// this codebase takes its store by reference rather than a path it resolves on its own.
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
    /// [`crate::reap::end_child`] - [`STOP_GRACE`] in production. An INJECTED field, never a
    /// read of that constant here - the same "the real duration is a value a test overrides on
    /// the instance" seam this codebase already uses for a waited duration elsewhere
    /// (`d-clock-seam`), so a test proving the FULL stop sequence never has to sit out a real
    /// 30 seconds. `Driver::default()` (test-only) resolves it to [`STOP_GRACE`]; each
    /// composition root sets it explicitly.
    pub stop_grace: std::time::Duration,
    /// The environment the store selection hands every launch: the connection a spawn's bound
    /// `rigger` server opens the run's store with, empty on the local store. Set after
    /// [`SpawnOpts::env`], so the selection wins under the same key.
    pub handed_env: Vec<(String, String)>,
    /// Where THE STOP reads what a session's silence leaves out (spec 105 LIVENESS): the
    /// progress store holding the supervisor's `SupervisorBeat`s and this host's `ApiRetry`
    /// records, with the holds read from [`Driver::run_store`]. `rigger run` hands it in. A
    /// host built without one, as `rigger critique` builds it, has no supervisor to weigh, so
    /// it reads no record and counts silence from the last line.
    pub beat_source: Option<&'a dyn EventStore>,
    /// The run stream this host holds the run on (spec 105): a sighting or a third failed close
    /// appends its `RunHeld` through it, and a close stamps the revision it reads there. `run_cli`
    /// hands it the folding store it hands as `run_store`. `None` on a host that holds nothing,
    /// `rigger critique`'s: it appends no hold and stamps no close.
    pub hold_sink: Option<&'a dyn EventStore>,
}

// `Default` is a TEST convenience only: production always constructs `Driver` with real
// injected stores at a composition root (`src/cli/run.rs`, `src/cli/critique.rs`), so a value
// that reads through `self.progress_store`/`self.run_store` without one is a test bug, never a
// real launch -
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
            bin: crate::claude_auth::DEFAULT_BIN.to_string(),
            rigger_bin: "rigger".to_string(),
            progress_store: &crate::eventstore::SilentStore,
            run_store: &crate::eventstore::SilentStore,
            scratch_root: String::new(),
            stop_grace: STOP_GRACE,
            handed_env: Vec::new(),
            beat_source: None,
            hold_sink: None,
        }
    }
}

/// A started launch: the live child (stdin open, for THE STREAM, criterion 2, to write the first
/// message on and close after the first `result`), the session id this host minted or continued
/// for it, and the exact argv it ran with (kept for the caller's own diagnostics/tests; not
/// re-derivable from `Child`).
pub struct Launch {
    pub child: Child,
    pub session_id: String,
    /// The model this launch runs on, the `--model` it was given, empty for the default model:
    /// what a hold of this launch names.
    pub model: String,
    /// The ordinal this launch took on the record ([`progress_store::next_launch`]), the one
    /// its transcript and its close carry.
    pub ordinal: u32,
    /// The session this launch continues, empty on a fresh launch: what the spawn's result
    /// records.
    pub resumed_from: String,
    /// Whether this launch continues a session ([`LaunchSession::resumed`]): what makes
    /// [`Driver::read_stream`] await the session's `system/init`.
    pub resumed: bool,
    pub args: Vec<String>,
    /// The file the session reads its system prompt from (`--system-prompt-file`), held for
    /// as long as the launch is, so the session can read it whenever it starts.
    pub persona: SystemPromptFile,
    /// The first user message, the task framed with the live-progress instruction (section
    /// 4.1, [`first_user_message`]): what [`Driver::read_stream`] writes to the session's input
    /// on a thread of its own, so a session that never reads it still reaches THE STOP.
    message: String,
}

/// What a spawn's first launch runs as ([`Driver::first_launch`]): its session, the task it is
/// sent and the ordinal it takes on the record, each decided by store reads made before anything
/// launches.
struct FirstLaunch {
    session: LaunchSession,
    task: String,
    ordinal: u32,
}

/// What a spawn's store reads decide before anything launches ([`Driver::before_launch`]).
enum Prelaunch {
    /// The run ended the spawn: it is answered from this result, with no launch.
    Answered(SpawnResult),
    /// The spawn launches as this.
    Launch(FirstLaunch),
}

/// The threads [`Driver::read_stream`] works a launch's pipes on: the writer of its first message
/// ([`write_first_message`]), which holds the session's input until the host closes it
/// ([`Pipes::close_input`]), the stderr drain, whose tail a no-result ending carries, and the
/// stdout reader feeding the stream's lines. Every ending joins them once the child is reaped
/// ([`Driver::join_pipes`]).
struct Pipes {
    /// What the writer waits on to close the session's input: dropping it is the close.
    input_open: Option<std::sync::mpsc::Sender<()>>,
    input: Option<std::thread::JoinHandle<std::io::Result<()>>>,
    stderr_drain: Option<std::thread::JoinHandle<Vec<u8>>>,
    stdout_reader: std::thread::JoinHandle<()>,
}

impl Pipes {
    /// Close the session's input (THE STREAM: "the host closes the input after the first
    /// `result`"): at once when the first message's write has returned, and the moment it
    /// returns when it has not. A write a session never reads returns when the child ends.
    fn close_input(&mut self) {
        self.input_open = None;
    }
}

/// How [`Driver::read_stream`] ended a launch short of an `Err`, which ends this `spawn` call
/// with its scratch reclaimed; on a held run the decorator can launch the same id again.
enum StreamEnd {
    /// The launch's result, which is the spawn's.
    Result(AgentResult),
    /// A `--resume` launch that yielded no `system/init`, closed `interrupted`
    /// ([`Driver::session_gone`]): the spawn lands a fresh launch next.
    NoSession,
    /// A launch closed `interrupted` for its class or a held run ([`Driver::fault`]): the spawn
    /// returns the classed error, marked interrupted ([`launch_interrupted`]) so the held-run
    /// decorator relaunches on the host's decision, and, recording no result and charging no
    /// attempt, keeps its scratch for the same id's next launch.
    Interrupted(Error),
}

/// What a hold read at a launch's ending proved ([`Driver::run_held`]), the one rule its readers
/// share: only a run proved [`HoldRead::Unheld`] takes the resume fallback.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum HoldRead {
    /// The run stands held, at the head of the read that proved it: the stamp an `interrupted`
    /// close that read decided carries ([`Driver::close_at`]).
    Held(Option<crate::eventstore::Revision>),
    /// The run stands unheld, or the host has no hold sink to read.
    Unheld,
    /// The read failed, proving neither: a run not proved unheld launches nothing, as the
    /// decorator's own read rules, and no `interrupted` close stands on a hold no read proved, so
    /// the launch keeps its own ending and the spawn's own failure is never masked.
    Unproved,
}

/// One launch as the record names it: the ordinal it took and the session it ran as, what every
/// close of it carries ([`Driver::close`]), and the model a hold of it names ([`Driver::sight`]).
#[derive(Clone, Copy)]
struct LaunchId<'a> {
    ordinal: u32,
    session_id: &'a str,
    /// The session the launch continues, empty on a fresh launch: what the fallback names.
    resumed_from: &'a str,
    model: &'a str,
}

impl Driver<'_> {
    /// THE LAUNCH (spec 104 criterion 1): start one child process for this spawn launch.
    ///
    /// Ordering is the criterion: `SpawnLaunched` is appended to the progress store
    /// BEFORE the child is started (so a launch that never even started a process because
    /// `bin` does not exist still cannot be re-attempted silently - the record proves the
    /// attempt), and it is the ONLY event this call writes - a close, where an ending writes
    /// one, is [`Driver::read_stream`]'s to write. The environment the child sees is the
    /// operator's ambient
    /// environment (inherited unchanged, credential included) plus the harness environment
    /// ([`harness_env`]) plus `opts.env` plus [`Driver::handed_env`], each later source winning
    /// under a shared key; this function neither reads nor sets a credential variable of its
    /// own. Once the child is started the launch returns it, with the first message for
    /// [`Driver::read_stream`] to write on a thread of its own ([`Launch`]): the launch writes
    /// nothing, so it fails before the child starts or not at all, and a session that never
    /// reads its input still reaches THE STOP.
    ///
    /// The launch takes the ordinal after the highest the record holds for this spawn
    /// ([`progress_store::next_launch`]), and 0 only for an id with no recorded launch, so a
    /// restart's first spawn never truncates a prior transcript. Its
    /// record, its transcript ([`stream_path`]) and its close all carry that
    /// [`Launch::ordinal`]; a failed read of the record starts nothing.
    ///
    /// A spawn naming a session to continue ([`SpawnOpts::resumed_from`]) is launched as a
    /// resume of it and sent [`SpawnOpts::resume_task`]; the record carries that session as
    /// both its id and its `resumed_from`. As on the cli host, a session that is gone falls
    /// back to a fresh launch: a resume that yields no `system/init` closes `interrupted`
    /// ([`Driver::read_stream`]) and the spawn launches again fresh, at the next ordinal, with
    /// the full task, unless its class or a run not proved unheld says to launch nothing
    /// ([`Driver::read_stream`]).
    pub fn launch(
        &self,
        agent: &AgentDef,
        task: &str,
        opts: &SpawnOpts,
        store: &dyn EventStore,
    ) -> Result<Launch, Error> {
        // The session this launch runs as: the one `opts` resumes, or a freshly minted one -
        // the ONE session authority the cli host shares ([`LaunchSession`]).
        let session = LaunchSession::of(opts);
        let task = session.task(task, opts);
        self.launch_as(session, agent, task, opts, store, None)
    }

    /// [`Driver::launch`] as `session`, sending `task` as it stands, at `ordinal` when the caller
    /// already read it off the record and at [`progress_store::next_launch`] when `None`: the way
    /// in of the fallback, a fresh session for a spawn whose `opts` still name the one that is
    /// gone, and of a spawn's first launch ([`Driver::first_launch`]), the relaunch among them, a
    /// fork of the session a launch left `interrupted`.
    fn launch_as(
        &self,
        session: LaunchSession,
        agent: &AgentDef,
        task: &str,
        opts: &SpawnOpts,
        store: &dyn EventStore,
        ordinal: Option<u32>,
    ) -> Result<Launch, Error> {
        let session_id = session.id.clone();
        let container = spawn_scratch_path(&self.scratch_root, &opts.run_id, &opts.id);
        let roots = write_roots(&opts.dir, container.as_deref())?;
        // The persona travels as a file in the spawn's scratch, never as an argv string (see
        // [`SystemPromptFile`]); the spawn's terminus reclaims that scratch.
        let persona = system_prompt_file(&opts.system_prompt, container)?;
        let args = build_args(
            agent,
            &persona,
            opts,
            &session,
            bin_or_path_default(&self.rigger_bin, "rigger"),
            &roots,
        )?;

        let started = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let ordinal = match ordinal {
            Some(ordinal) => ordinal,
            None => progress_store::next_launch(store, &opts.run_id, &opts.id)?,
        };
        progress_store::record_launch(
            store,
            &opts.run_id,
            &SpawnLaunched {
                spawn: opts.id.clone(),
                launch: ordinal,
                session_id: session_id.clone(),
                resumed_from: session.resumed().then(|| session.resumed_from()),
                started,
                ended: None,
                class: None,
                revision: None,
                dir: opts.dir.clone(),
            },
        )?;

        let bin = bin_or_path_default(&self.bin, crate::claude_auth::DEFAULT_BIN);
        let mut cmd = crate::subprocess::command_in(bin, &opts.dir);
        cmd.args(&args);
        // The ONE build-environment authority's injection site for this driver (spec 65),
        // exactly like the cli driver applies it: every var the resolver derived, on top
        // of the inherited ambient environment (`Command` never clears it) - so the
        // operator's own credential rides through untouched and unread. The harness
        // environment every headless worker gets ([`harness_env`]) goes first, so the build
        // environment overrides it under the same key.
        for (k, v) in harness_env(&opts.id) {
            cmd.env(k, v);
        }
        for (k, v) in opts.env.iter().chain(&self.handed_env) {
            cmd.env(k, v);
        }
        cmd.stdin(Stdio::piped());
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());
        let child = cmd
            .spawn()
            .map_err(|e| Error(format!("claude_code driver: launch {:?}: {e}", opts.id)))?;

        Ok(Launch {
            child,
            session_id,
            model: agent.model_for_attempt(opts.attempt).to_string(),
            ordinal,
            resumed_from: session.resumed_from(),
            resumed: session.resumed(),
            args,
            persona,
            // The task, framed with the live-progress instruction, is the first user message on
            // the input stream (section 4.1), which [`Driver::read_stream`] writes. Every launch
            // passes through here, so a resume and a relaunch are framed too.
            message: first_user_message(&framed(task)),
        })
    }

    /// What the spawn's first launch runs as (spec 105, RESUME BY SESSION), decided by reads
    /// alone, so a failed read launches nothing. It takes the next ordinal of `opts.id` among its
    /// records in this run ([`progress_store::launch_records`], [`progress::latest_launch_of`],
    /// [`progress::next_launch_after`]). When that latest launch ended `interrupted`, whichever
    /// site closed it, it is a relaunch ([`LaunchSession::relaunch`]) in `opts.dir`, a fork of
    /// the session that launch's close names under a minted id, charged no attempt, sent the
    /// [`Driver::interruption_note`] in front of what [`LaunchSession::task`] sends, the
    /// [`SpawnOpts::resume_task`] when that launch carried it: its record, its open record unless
    /// it was lost, and the records its chain of sessions leads back through decide. Any other
    /// spawn runs as [`Driver::launch`] would launch it, [`LaunchSession::of`] its `opts`.
    fn first_launch(&self, prompt: &str, opts: &SpawnOpts) -> Result<FirstLaunch, Error> {
        let chain = progress_store::launch_records(self.progress_store, &opts.run_id, &opts.id)?;
        let records = progress::latest_launch_of(&chain);
        let ordinal = progress::next_launch_after(records.last());
        let Some(interrupted) = records
            .last()
            .filter(|latest| latest.ended.as_deref() == Some(progress::ENDED_INTERRUPTED))
        else {
            let session = LaunchSession::of(opts);
            let task = session.task(prompt, opts).to_string();
            return Ok(FirstLaunch {
                session,
                task,
                ordinal,
            });
        };
        let note = self.interruption_note(interrupted)?;
        let launched = records.first().unwrap_or(interrupted);
        let session = LaunchSession::relaunch(launched, interrupted, &chain, opts);
        let task = match session.task(prompt, opts) {
            "" => note,
            carried => format!("{note}\n\n{carried}"),
        };
        Ok(FirstLaunch {
            session,
            task,
            ordinal,
        })
    }

    /// Every store read the spawn makes before anything launches: the current run's log
    /// ([`read_or_park`]), then its launch records and the hold sink ([`Driver::first_launch`]).
    /// Its error is the failed read's, which [`AgentDriver::spawn`] marks a store fault once.
    fn before_launch(
        &self,
        agent: &AgentDef,
        prompt: &str,
        opts: &SpawnOpts,
    ) -> Result<Prelaunch, Error> {
        Ok(
            match read_or_park(self.run_store, agent, prompt, opts, &self.scratch_root)? {
                Logged::Ended(res) => Prelaunch::Answered(res),
                Logged::Parked | Logged::Requested => {
                    Prelaunch::Launch(self.first_launch(prompt, opts)?)
                }
            },
        )
    }

    /// What a relaunch tells the session `interrupted` closed (spec 105): a close with no class,
    /// the supervisor's start-up reconciliation or the fallback's, names the restart; a classed
    /// close, the host's own in a fault place, names the hold the first release on the hold sink
    /// past the close's stamp ended ([`released_past`]) and how long it stood, or the class alone
    /// when the close is unstamped, the host has no hold sink or no such release ended a hold.
    fn interruption_note(&self, interrupted: &SpawnLaunched) -> Result<String, Error> {
        let why = match &interrupted.class {
            None => ": rigger's supervisor restarted".to_string(),
            Some(class) => {
                let released = match self.hold_sink.zip(interrupted.revision) {
                    Some((sink, stamp)) => released_past(sink, STREAM, stamp)?,
                    None => None,
                };
                match released {
                    Some((hold, held_for_s)) => format!(
                        ": the run was held {held_for_s}s ({}) and is released now",
                        hold.reason()
                    ),
                    None => format!(" by {class}"),
                }
            }
        };
        Ok(format!(
            "This session was interrupted{why}. Continue where you left off."
        ))
    }

    /// THE STREAM (spec 104 criterion 2): read `launch`'s child stdout line by line to
    /// completion, close the input after the first `result`, and reap the child. Every
    /// line touches the spawn's liveness marker and is persisted to the raw stream file
    /// (architecture addendum §4.2) before this function looks at its content, so a line
    /// this reader cannot parse is never silently lost - it still lands liveness, the
    /// transcript file, AND a progress line (below), just no structured record.
    ///
    /// Every exit reaps the child through ONE mechanism, [`crate::reaped_child::ReapedChild`] -
    /// the SAME guard the dashboard's own supervised child uses (adj-u104-stream round-3
    /// REQUIRED FIX 2: a second, hand-rolled Drop-based reap guard duplicated that exact
    /// concern; deleted in favor of reusing the canonical one, per this codebase's
    /// standing one-reaping-mechanism precedent). `launch.child` is moved into it before
    /// the stdout loop below, so the missing-stdout-pipe guard, a read error before any
    /// result, and a `record_result_unless_ended` write failure all leave `reaper` in scope
    /// with the process still possibly running behind it; dropping it ends it either way:
    /// `try_wait` first, falling back to a best-effort end-then-collect through the same
    /// handle when the process is not yet known to have exited, so no child outlives this
    /// function's `Err`.
    ///
    /// A CLEAN EOF is different (adj-u104-stream round-5 REQUIRED FIX,
    /// op-104-stream-clean-eof-waits-before-the-reap): the child closing its OWN stdout
    /// pipe is not proof it has exited - real post-output teardown (flushing telemetry,
    /// releasing a lock) can still be running - so THIS function tracks that one exit
    /// shape (`ended_via_break` below) and, on a clean EOF ONLY, blocks on the child's own
    /// handle (`reaper.child_mut().wait()`, no signal) BEFORE the trailing `drop(reaper)`,
    /// so that teardown always finishes on its own first. Every other exit path (missing
    /// stdout pipe, a read error before any result, the post-result-read-error `break`
    /// below, a `record_result_unless_ended` failure) is unchanged: straight to the
    /// signal-capable drop, since none of those is evidence the child is mid-teardown.
    ///
    /// A read error AFTER a result is already captured (adj-u104-stream round-2 REQUIRED
    /// FIX) is handled the same uniform way: the loop below just stops reading (`break`)
    /// rather than propagating `Err`, so this function still returns the
    /// already-recorded `Ok(AgentResult)` rather than discarding it, since
    /// `record_result_unless_ended` already wrote the real result to the run store one line
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
    /// The first message ([`Launch`]) is written on a thread of its own too
    /// ([`write_first_message`]), started with the readers, so a session that never reads its
    /// input, whatever the task's size, still reaches THE STOP below rather than holding this
    /// host in a write. The writer holds the input until the host closes it
    /// ([`Pipes::close_input`]): after the first `result`, before the clean EOF's wait, and first
    /// in THE STOP. A write that fails ends nothing by itself: the session ends as its stream
    /// says, and the writer's join ([`Driver::join_pipes`]) leaves a progress line naming the
    /// error.
    ///
    /// STOP (spec 104 criterion 6): stdout is likewise read on its OWN thread (the SAME
    /// stderr-deadlock reasoning applies to it too - this function must never itself sit
    /// blocked in `BufReader::lines()` with no way to notice silence), forwarding each
    /// line ([`crate::reaped_child::line_channel`], disconnecting on a clean EOF) over a channel
    /// this function drains with
    /// `recv_timeout`, so `max_wall_clock` (0 = unbounded, [`crate::config::AgentDef`]'s
    /// own resolved bound) can be enforced against STREAM SILENCE - the session's silent time
    /// ([`Driver::silence`]), not total run time and not the bare wall time since the last
    /// line, mirroring `liveness::sweep`'s own reading for the external stepwise driver, but
    /// enforced HERE because this host has no external stepper polling a marker file. Silent
    /// time can pass the bound no sooner than the wall time since the last line does, so a
    /// line costs no read: the records are read only on a wake at that point, and a wake under
    /// the bound waits out the rest of it, at least [`UNBOUNDED_POLL`], or the poll when its
    /// records do not read ([`next_wait`]), and re-checks the child. A genuine expiry before any
    /// result exists runs THE STOP sequence (close the session's input ([`Pipes::close_input`]),
    /// wait [`Driver::stop_grace`], end the child via [`crate::reap::end_child`] - the handle-bound
    /// production helper, never a computed pid) and, short of THE RESUME FALLBACK below, records
    /// the EXISTING [`SpawnResult::liveness_fault`] shape, same as `liveness::sweep`'s own fault
    /// for the stepwise driver, just recorded HERE rather than by an external sweep. An expiry
    /// AFTER a result already landed is not a stop at all - it degrades to the SAME soft-break the
    /// post-result read-error path already takes (round-2 precedent), letting the existing reap
    /// decide the child's fate.
    ///
    /// HOLD (spec 105): a `system/api_retry` line of a category whose row holds is a sighting
    /// ([`Driver::sight_retry`]). The first `result` closes the session's input whatever it says.
    /// Past a resume's refusal (THE RESUME FALLBACK below), a non-error one is recorded and closes
    /// the launch `completed`, and one with `is_error: true` records nothing: no later `result`
    /// acts, and the launch takes its class after the child exits, as a no-result launch does
    /// ([`Driver::classify_no_result`]), or takes THE STOP when a bounded session never does,
    /// sighting the class its session reported while its close stays `stopped` with no class
    /// ([`Driver::close_stopped`]). On an unbounded launch a live child that ignores its input
    /// close keeps the turn until it exits, as a no-result child does, and a read error behind that
    /// `result` fails the launch unclassed, as a read error before any result does.
    ///
    /// THE RESUME FALLBACK (spec 105 criterion 1): a `--resume` launch awaits its session until
    /// the first `system/init`. Ending while it still awaits one - at the end of the stream,
    /// silent to the stop, on a read error, with or without the refusal's own `result` line -
    /// means the session it asked for never showed, so the launch closes `interrupted`
    /// ([`Driver::session_gone`]) and returns [`StreamEnd::NoSession`]: no result, no liveness
    /// fault and no `stopped` close stand for it, one close and one next action, the fresh
    /// launch [`AgentDriver::spawn`] lands. A launch that saw `system/init` takes no fallback,
    /// whatever its ending, and a fresh launch has nothing to await.
    ///
    /// HELD MEANS NO LAUNCH (spec 105) outranks that fallback, since a fresh launch is a launch and
    /// runs past the decorator's gate: the fallback takes a run proved unheld
    /// ([`Driver::run_held`]), read once the launch has ended, the stop's grace, the reap and the
    /// drain joins behind it, and once the lines already on its stream are drained and sighted
    /// ([`Driver::drain_lines`]), so a hold that lands while it ends counts, one its own last line
    /// sights among them. An ending that closes `interrupted` for its class or a held run
    /// ([`Driver::fault`]) returns [`StreamEnd::Interrupted`], and on a run not proved unheld the
    /// stop and the read error keep their own endings, each returning its error. Off that path a
    /// resume with no `system/init` falls back whatever its class, and a host with no hold sink
    /// reads no hold and always falls back.
    fn read_stream(
        &self,
        launch: Launch,
        opts: &SpawnOpts,
        max_wall_clock: u64,
    ) -> Result<StreamEnd, Error> {
        // The session the first `system/init` names when it is not the one the launch named: the
        // launch runs as it from that line on (the `system/init` arm below).
        let ran_as = std::cell::OnceCell::<String>::new();
        let mut id = LaunchId {
            ordinal: launch.ordinal,
            session_id: &launch.session_id,
            resumed_from: &launch.resumed_from,
            model: &launch.model,
        };
        let mut reaper = crate::reaped_child::ReapedChild::new(launch.child);

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
        let (line_rx, stdout_reader) = crate::reaped_child::line_channel(stdout);
        // The first message goes on its own thread as well (see the doc above), which holds the
        // session's input until the host closes it.
        let (input_open, close) = std::sync::mpsc::channel();
        let message = launch.message;
        let input =
            reaper.child_mut().stdin.take().map(|stdin| {
                std::thread::spawn(move || write_first_message(stdin, &message, close))
            });
        let mut pipes = Pipes {
            input_open: Some(input_open),
            input,
            stderr_drain,
            stdout_reader,
        };

        let marker = liveness::marker_path(&self.scratch_root, &opts.run_id, &opts.id);
        let mut stream_file = open_stream_file(
            stream_path(&self.scratch_root, &opts.run_id, &opts.id, launch.ordinal).as_deref(),
        );

        let mut resolved_model = String::new();
        let mut result: Option<AgentResult> = None;
        // FAILURE CLASS's second-priority source (spec 104 criterion 5, Design: "the last
        // `api_retry.error`"): the category of the MOST RECENT `system/api_retry` line THIS
        // launch's own reader has seen so far - `None` until the first one arrives, then
        // overwritten by every later one, so a session that ends without a result classifies
        // from whichever category was live right before it stopped.
        let mut last_api_retry_category: Option<String> = None;
        // The `error` of the LATEST assistant line, `None` once a clean turn follows it: the
        // class a session reports on the line it prints ahead of its `result`, which reaches this
        // reader where the fire-and-forget `StopFailure` hook may not.
        let mut assistant_error: Option<String> = None;
        // Set by a first `result` with `is_error: true`: the session answered nothing, and no
        // later `result` line may answer for it.
        let mut errored = false;
        // THE RESUME FALLBACK's one predicate (see the doc above): a resume with no
        // `system/init` yet.
        let mut awaiting_session = launch.resumed;
        // adj-u104-stream round-5 REQUIRED FIX (op-104-stream-clean-eof-waits-before-the-reap):
        // distinguishes the loop's two exit shapes below. `false` when the loop instead runs
        // to real exhaustion (a clean EOF: the child closed its stdout pipe on its own,
        // with no read error) - see the `wait()` this flag gates, just past the loop.
        let mut ended_via_break = false;
        // THE RESUME FALLBACK's read error: it left the loop with the session unseen, and the tail
        // answers it, the hold read once the launch has ended.
        let mut read_error = None;
        let bound = std::time::Duration::from_secs(max_wall_clock);
        // What the silent time counts from, stamped before `armed_at` so the wake a bound
        // away from a line never reads a hair under it.
        let mut last_line = SystemTime::now();
        // The wait in hand: when it was armed and for how long, a span rather than a deadline
        // since a bound is the operator's number and an `Instant` sum of it could overflow.
        let mut armed_at = std::time::Instant::now();
        let mut armed_for = bound;

        loop {
            let recv_timeout = if max_wall_clock == 0 {
                // Unbounded: still wake periodically rather than an infinite wait, so the
                // loop can re-check the child (the wake-up arm below) - purely a
                // "wake up occasionally" budget, never itself a wall-clock bound.
                UNBOUNDED_POLL
            } else {
                // Landing exactly on the wait and running past it both mean "expire
                // without blocking".
                armed_for.saturating_sub(armed_at.elapsed())
            };

            let received = if recv_timeout.is_zero() {
                Err(std::sync::mpsc::RecvTimeoutError::Timeout)
            } else {
                line_rx.recv_timeout(recv_timeout)
            };

            let line = match received {
                Ok(line) => {
                    last_line = SystemTime::now();
                    armed_at = std::time::Instant::now();
                    armed_for = bound;
                    line
                }
                // The stream's end: the clean EOF, so ended_via_break stays false.
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) if result.is_none() => {
                    if max_wall_clock != 0 {
                        let silent = self.silence(opts, last_line);
                        if silent.is_some_and(|silent| liveness::past_bound(silent, bound)) {
                            // THE STOP (spec 104 criterion 6): genuine SILENCE with no result
                            // yet, on a BOUNDED launch only. Runs the whole sequence inline so
                            // the caller sees exactly one outcome for "this launch never
                            // produced a result". A resume silent to here never showed its
                            // session: the same sequence ends the child first, then the hold read
                            // chooses, the fallback answering a run proved unheld with no fault of
                            // its own.
                            if awaiting_session {
                                self.stop_session(reaper, pipes, opts, self.stop_grace);
                                self.drain_lines(&line_rx, &mut stream_file, opts, id);
                                if self.run_held(opts) == HoldRead::Unheld {
                                    return Ok(self.session_gone(opts, id));
                                }
                                let message = self.announce_stop(opts, max_wall_clock);
                                return self.close_stopped(
                                    opts,
                                    id,
                                    message,
                                    assistant_error.as_deref(),
                                );
                            }
                            return self.stop_for_wall_clock_silence(
                                reaper,
                                pipes,
                                opts,
                                id,
                                max_wall_clock,
                                assistant_error.as_deref(),
                            );
                        }
                        armed_at = std::time::Instant::now();
                        armed_for = next_wait(bound, silent);
                    }
                    // Past that a wake-up is never a stop, and on an unbounded launch it never
                    // reads as one. 2026-09-28: the `max_wall_clock` 0 wake-up took THE STOP,
                    // so every unbounded session that thought in silence for
                    // [`UNBOUNDED_POLL`] was stopped as a liveness fault reading "silent for
                    // 0s" - a poll interval read as a wall-clock bound. The tell: a stop
                    // message naming a 0 s bound. The wake-up only re-checks the child, on a
                    // bounded launch too, whose silence a hold can keep under the bound: one
                    // that has exited while a descendant still holds the stdout pipe will
                    // never send EOF, so its exit ends the read here as a session with no
                    // result (classified below); a live child keeps its turn.
                    if matches!(reaper.child_mut().try_wait(), Ok(Some(_))) {
                        break;
                    }
                    continue;
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
                // adj-u104-stream round-2 REQUIRED FIX: `record_result_unless_ended` may
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
                // failure it always was. A launch that still awaits its session stops the read
                // the same way and the tail below answers it, the hold read past the reap.
                Err(e) if result.is_some() => {
                    self.record_progress(
                        opts,
                        &format!("stream (read error after result, ignored): {e}"),
                    );
                    ended_via_break = true;
                    break;
                }
                Err(e) if awaiting_session => {
                    self.record_progress(
                        opts,
                        &format!("stream (read error before system/init): {e}"),
                    );
                    read_error = Some(e);
                    ended_via_break = true;
                    break;
                }
                Err(e) => return Err(read_agent_stream_error(opts, &e)),
            };

            // "every line touches the spawn's liveness marker - the host proves life,
            // the agent is never asked to" (architecture addendum §4.2).
            touch_liveness_marker(marker.as_deref());
            write_stream_line(&mut stream_file, &line);

            let Ok(v) = serde_json::from_str::<Value>(&line) else {
                // "a line that is not JSON is recorded as a progress line and skipped."
                self.record_progress(opts, &format!("stream (unparseable): {line}"));
                continue;
            };

            match line_kind(&v) {
                (Some("system"), Some("init")) => {
                    awaiting_session = false;
                    // The session the binary runs is the one its `system/init` names: one other
                    // than the launch named is noted, naming both, and every close and the answer
                    // of the launch carry it from here on, so the next attempt resumes and a
                    // relaunch forks the session that ran. The launch's open record keeps the
                    // session it launched as. Only the first such line takes.
                    if let Some(named) = v.get("session_id").and_then(Value::as_str) {
                        if !named.is_empty()
                            && named != id.session_id
                            && ran_as.set(named.to_string()).is_ok()
                        {
                            self.record_progress(
                                opts,
                                &format!(
                                    "stream: launched as session {}, system/init named session \
                                     {named}, which the launch runs as",
                                    launch.session_id
                                ),
                            );
                            id = LaunchId {
                                session_id: ran_as.get().map_or(id.session_id, String::as_str),
                                ..id
                            };
                        }
                    }
                    resolved_model = v
                        .get("model")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string();
                    if let Some(servers) = v.get("mcp_servers").and_then(Value::as_array) {
                        self.record_progress(opts, &format!("mcp servers: {servers:?}"));
                    }
                    // A LAUNCH FAULT, not a note (spec 104 THE STREAM: "a `rigger` server
                    // that did not connect fails the launch as a fault"). 2026-09-28 probe
                    // against the real claude 2.1.283: `rigger mcp --spawn` exited before it
                    // connected, init reported it `failed`, this arm wrote one progress line
                    // and the session ran its whole task with no rigger tools while the host
                    // recorded a success. A spawn without its server cannot ground, emit or
                    // record through the graph, so it is not a working spawn: the session is
                    // stopped here, before its task runs, and the port returns the fault's
                    // class. The tell of this defect class: a progress note where a failure
                    // class belongs.
                    if let Some(status) = rigger_server_fault(&v) {
                        return self.stop_for_launch_fault(reaper, pipes, opts, id, &status);
                    }
                }
                (Some("system"), Some("api_retry")) => {
                    self.record_progress(opts, &api_retry_line(&v));
                    self.record_api_retry(opts, &v);
                    // FAILURE CLASS's second-priority source (criterion 5): the LATEST
                    // category wins, a later line overwriting an earlier one exactly like
                    // `resolved_model` does as the stream plays.
                    if let Some(category) = self.sight_retry(opts, id, &v) {
                        last_api_retry_category = Some(category.to_string());
                    }
                }
                (Some("assistant"), _) => {
                    assistant_error = message_error(&v).map(str::to_string);
                }
                // A live progress note ONLY - never the denial count, which the `result`
                // message's own `permission_denials` array carries ([`spawn_result_from`]):
                // a PreToolUse hook block emits no event here, yet the result lists it.
                (Some("system"), Some("permission_denied")) => {
                    self.record_progress(opts, "permission denied");
                }
                // A resume's `result` with no `system/init` before it is the host's refusal,
                // never the session's answer: Claude Code 2.1.314 prints one for a session it
                // cannot find. It goes unrecorded, since the run log keeps the first real result
                // of a spawn id and this one would stand in the fresh launch's place. Input closes
                // as after any result, so a session held on it ends and the fallback answers.
                (Some("result"), _) if awaiting_session => {
                    pipes.close_input();
                }
                // adj-u104-stream round-3 REQUIRED FIX 1: only the FIRST result-type line
                // acts - a genuine SECOND one (the comment above this match's `Err(e) if
                // result.is_some()` arm calls it anticipated, not an error) must never
                // overwrite the already-captured `result`, nor re-derive it from a
                // `resolved_model` that may have drifted since (another `system/init`
                // line between the two results). The `result.is_none()` GUARD on this
                // arm - rather than an `if` inside a catch-all `(Some("result"), _)` arm -
                // means a later duplicate falls straight through to the `_` arm below: no
                // `spawn_result_from`/`record_result_unless_ended` round trip for it either,
                // matching "the log holds one result" (CONSTRAINTS WALK) instead of
                // merely relying on that call's own idempotence to paper over it. The
                // `errored` half of the guard does the same behind an `is_error` first
                // `result`, which sets no `result` of its own.
                (Some("result"), _) if result.is_none() && !errored => {
                    errored = result_errored(&v);
                    if !errored {
                        // The real result supersedes a liveness fault a courier recorded for
                        // a launch this one replaced, and lands nothing beside a real result
                        // already standing.
                        let res = spawn_result_from(&v, opts, &resolved_model);
                        spawn_store::record_result_unless_ended(self.run_store, &res)?;
                        self.close(opts, id, progress::ENDED_COMPLETED, "");
                        // The session this launch ran as rides back for the conductor to record:
                        // a resume reaches this arm only once its `system/init` showed the
                        // session, which is the one the launch ran as, the session its spawn
                        // named unless that line named another, and the fallback's fresh launch
                        // records an empty `resumed_from`.
                        result = Some(AgentResult {
                            output: res.output,
                            resolved_model: resolved_model.clone(),
                            session_id: id.session_id.to_string(),
                            resumed_from: launch.resumed_from.clone(),
                        });
                    }
                    // "the host closes the input after the first `result`"; a session that would
                    // otherwise wait on more input can now exit.
                    pipes.close_input();
                }
                _ => {
                    // user turns, hook events, every other subtype, AND a duplicate
                    // result-type line once the first one acted (falls through here past
                    // the guarded arm above): raw persistence + liveness already happened
                    // above; no structured record for this criterion (out of THE STREAM's
                    // Done-when scope).
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
        // The `break` path (a read error, or post-result silence) is
        // UNCHANGED: neither is evidence of in-progress teardown, so it keeps going
        // straight to the signal-capable drop exactly as before this fix.
        if !ended_via_break {
            // Its input closes first, so a session still reading it sees the end and exits.
            pipes.close_input();
            let _ = reaper.child_mut().wait();
        }

        // Reap through the ONE canonical mechanism, on every exit path alike (a clean
        // EOF, or the `break` above on a read error): `ReapedChild::drop`
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
        let stderr_tail = self.join_pipes(opts, pipes, ORDINARY_DRAIN_JOIN_BOUND);

        match (result, read_error) {
            (Some(result), _) => Ok(StreamEnd::Result(result)),
            (None, Some(e)) => {
                self.drain_lines(&line_rx, &mut stream_file, opts, id);
                if self.run_held(opts) == HoldRead::Unheld {
                    Ok(self.session_gone(opts, id))
                } else {
                    Err(read_agent_stream_error(opts, &e))
                }
            }
            (None, None) => {
                let (class, message) = self.classify_no_result(
                    opts,
                    assistant_error.as_deref(),
                    &last_api_retry_category,
                    &stderr_tail,
                );
                let read = self.ending_read(opts, class);
                if awaiting_session && read == Some(HoldRead::Unheld) {
                    return Ok(self.session_gone(opts, id));
                }
                self.fault(opts, id, class, message, read)
            }
        }
    }

    /// THE RESUME FALLBACK's ending (see [`Driver::read_stream`]'s doc): close the launch
    /// `interrupted`, log the fallback on the spawn's progress and leave the fresh launch to
    /// [`AgentDriver::spawn`], so no attempt is charged and no failure stands for it.
    fn session_gone(&self, opts: &SpawnOpts, id: LaunchId) -> StreamEnd {
        self.close(opts, id, progress::ENDED_INTERRUPTED, "");
        self.record_progress(
            opts,
            &format!(
                "stream: session {} to resume yielded no system/init, falling back to a fresh \
                 launch",
                id.resumed_from
            ),
        );
        StreamEnd::NoSession
    }

    /// A FAILURE HAS A CLASS (spec 104 criterion 5): class a session that ended with no usable
    /// `result` - the natural loop-exhaustion/clean-EOF exit
    /// [`read_stream`](Self::read_stream)'s bottom falls through to, exactly the
    /// CONSTRAINTS WALK shape ("the child exits before `system/init` ... a fault of class
    /// `unknown` carrying the stderr tail, never a hang"), whether or not an `is_error`
    /// `result` came first - into the class its sources give ([`Driver::no_result_class`]) and
    /// the message its ending carries ([`Driver::fault`]), which carries the class and
    /// `stderr_tail` (lossily decoded, trimmed).
    fn classify_no_result(
        &self,
        opts: &SpawnOpts,
        assistant_error: Option<&str>,
        last_api_retry_category: &Option<String>,
        stderr_tail: &[u8],
    ) -> (AgentFailure, String) {
        let class = self.no_result_class(opts, assistant_error, last_api_retry_category.as_deref());
        let tail = String::from_utf8_lossy(stderr_tail);
        let message = format!(
            "claude_code driver: {:?}: the agent stream ended with no result (class {class}; \
             stderr tail: {})",
            opts.id,
            tail.trim()
        );
        (class, message)
    }

    /// The class a launch with no usable `result` takes from its sources (spec 104 criterion 5).
    /// `assistant_error` is the FIRST-priority source, the class the session printed on its last
    /// assistant line. Next comes the `StopFailure` hook's record (keyed on `opts.id` AND scoped
    /// to `opts.run_id` - adj-u104c5 REQUIRED FIX 1, adv-u104c5-stopfailure-crosses-run-boundary:
    /// a stale record from an old or unrelated run must never outrank the live session's own
    /// `api_retry` category, matching `rigger status`'s own established progress-event
    /// run-scoping convention at `src/main.rs`), read from THIS driver's own progress store and
    /// scoped to this launch by stream order ([`progress::latest_stop_failure_class`]). The two
    /// fold against `last_api_retry_category` via [`classify_failure`]. A progress-store read
    /// failure degrades to "no `StopFailure` record found" (`None`) rather than failing the whole
    /// classification - the second source (`last_api_retry_category`) and the `unknown` floor
    /// both still apply, so a store hiccup here never turns a classifiable failure into an opaque
    /// one (`classify_no_result_degrades_to_the_api_retry_floor_when_the_progress_store_read_fails`
    /// below proves this branch is reachable).
    fn no_result_class(
        &self,
        opts: &SpawnOpts,
        assistant_error: Option<&str>,
        last_api_retry_category: Option<&str>,
    ) -> AgentFailure {
        // THIS spawn's run's progress alone - its own stream, the one read `rigger status`
        // folds too (spec 101).
        let stop_failure_class = progress::read_run(self.progress_store, &opts.run_id)
            .ok()
            .and_then(|events| progress::latest_stop_failure_class(&events, &opts.id));
        classify_failure(
            assistant_error.or(stop_failure_class.as_deref()),
            last_api_retry_category,
        )
    }

    /// End a launch as a FAULT (spec 104 THE LAUNCH: "at exit it closes the record with
    /// `ended: ... fault ...` and the class"; FAILURE CLASS: "the port returns the class as
    /// data"): close the launch record `fault` under `class`, and return the classed `Error`
    /// through the port. The ONE ending both fault shapes share - a session that ended with no
    /// result ([`Driver::classify_no_result`]) and a session whose `rigger` MCP server did not
    /// connect ([`Driver::stop_for_launch_fault`]).
    ///
    /// HELD MEANS NO LAUNCH AND NO CHARGE (spec 105) decides ahead of that close, from `read`, the
    /// hold read the ending took ([`Driver::ending_read`]): the launch closes `interrupted` under
    /// `class` instead when no read was taken, since the class interrupts on its own, or when the
    /// read proved the run held, whatever the class, and the error rides
    /// [`StreamEnd::Interrupted`], marked interrupted: no result, no attempt, the scratch kept. A
    /// close the hold read decided carries that read's head as its stamp ([`Driver::close_at`]). A
    /// run proved unheld and a read that fails, proving neither, close `fault`.
    ///
    /// The holds follow the close and come before the `Error` returns: an ending of a class whose
    /// row holds or waits sights that class ([`Driver::sight_ending`]), then a `fault` close counts
    /// the failed closes ([`Driver::sight_unstable`]), which an `interrupted` close never enters. A
    /// class whose row faults holds nothing on its own, so it takes its spawn site's existing path.
    fn fault(
        &self,
        opts: &SpawnOpts,
        id: LaunchId,
        class: AgentFailure,
        message: String,
        read: Option<HoldRead>,
    ) -> Result<StreamEnd, Error> {
        let interrupted = matches!(read, None | Some(HoldRead::Held(_)));
        let ended = if interrupted {
            progress::ENDED_INTERRUPTED
        } else {
            progress::ENDED_FAULT
        };
        match read {
            Some(HoldRead::Held(head)) => self.close_at(opts, id, ended, class.as_str(), head),
            _ => self.close(opts, id, ended, class.as_str()),
        }
        self.sight_ending(opts, id, class);
        if interrupted {
            return Ok(StreamEnd::Interrupted(launch_interrupted(class, message)));
        }
        self.sight_unstable(opts, id);
        Err(no_result_error(class, message))
    }

    /// Sight the `class` a launch ended under when its row holds or waits, with no `detail` since
    /// an ending is no sighting line ([`Driver::sight`]): the one sighting [`Driver::fault`] and
    /// [`Driver::close_stopped`] take for an ending.
    fn sight_ending(&self, opts: &SpawnOpts, id: LaunchId, class: AgentFailure) {
        if class.row().waits() {
            self.sight(opts, id, class.as_str(), "");
        }
    }

    /// The hold read a classed ending takes, what [`Driver::fault`] closes on and the no-result
    /// ending's fallback weighs: none when `class`'s row holds or waits on a host with a hold sink,
    /// since that class interrupts the launch on its own, and otherwise [`Driver::run_held`], so a
    /// read holds only what it proved.
    fn ending_read(&self, opts: &SpawnOpts, class: AgentFailure) -> Option<HoldRead> {
        let class_interrupts = class.row().waits() && self.hold_sink.is_some();
        (!class_interrupts).then(|| self.run_held(opts))
    }

    /// What this run's hold read proves once a launch has ended, what [`Driver::fault`] and the
    /// fallback at the stop and the read error weigh (spec 105): the fallback runs inside the
    /// spawn, past the decorator's gate, so the read follows the ending. A host with no hold sink
    /// reads no hold and answers [`HoldRead::Unheld`]. A read that fails is a progress line and
    /// answers [`HoldRead::Unproved`].
    fn run_held(&self, opts: &SpawnOpts) -> HoldRead {
        let Some(sink) = self.hold_sink else {
            return HoldRead::Unheld;
        };
        match read_current_hold_at_head(sink, STREAM) {
            Ok((Some(_), head)) => HoldRead::Held(head),
            Ok((None, _)) => HoldRead::Unheld,
            Err(e) => {
                self.record_progress(opts, &format!("hold not read: {}", e.0));
                HoldRead::Unproved
            }
        }
    }

    /// Close this launch's record `ended` under `class`, stamped with the run stream's revision
    /// read just now: the one close an ending that closes writes, [`Driver::close_at`] that
    /// revision. A failed read leaves the close unstamped.
    fn close(&self, opts: &SpawnOpts, id: LaunchId, ended: &str, class: &str) {
        let revision = self
            .hold_sink
            .and_then(|sink| run_store::run_stream_head(sink).ok().flatten());
        self.close_at(opts, id, ended, class, revision);
    }

    /// Close this launch's record `ended` under `class`, stamped with `revision`: the head of the
    /// run read that decided the ending, so a `RunReleased` landing after that read lies past the
    /// close ([`released_past`]). Best-effort, a courtesy for reconciliation and for the
    /// failed-close count, never the source of truth: the classed `Error` or the recorded result
    /// is, and a lost close only leaves a record the supervisor's start-up reconciliation closes
    /// later.
    fn close_at(
        &self,
        opts: &SpawnOpts,
        id: LaunchId,
        ended: &str,
        class: &str,
        revision: Option<crate::eventstore::Revision>,
    ) {
        let _ = progress_store::record_launch(
            self.progress_store,
            &opts.run_id,
            &SpawnLaunched {
                revision,
                ..SpawnLaunched::closed(opts.id.clone(), id.ordinal, id.session_id, ended, class)
            },
        );
    }

    /// One sighting of `cause` by this launch, a line's when `detail` carries its error text:
    /// the hold's own writer decides whether it appends ([`run_store::record_hold_sighting`]).
    /// A host with no hold sink appends nothing. A sighting that fails to land is a progress line,
    /// since a lost hold never fails the spawn that saw it: the next sighting holds the run.
    fn sight(&self, opts: &SpawnOpts, id: LaunchId, cause: &str, detail: &str) {
        let Some(sink) = self.hold_sink else {
            return;
        };
        let sighting = run_store::HoldSighting {
            cause,
            detail,
            model: id.model,
        };
        if let Err(e) = run_store::record_hold_sighting(sink, &sighting) {
            self.record_progress(opts, &format!("hold sighting {cause} not recorded: {e}"));
        }
    }

    /// Sight the category an `api_retry` line `v` carries, that line's one sighting: a category
    /// whose row holds holds the run ([`Driver::sight`], the category its detail). Returns the
    /// category, `None` for a line that carries none. The read loop and the drain at a launch's
    /// end ([`Driver::drain_lines`]) both sight a line through it.
    fn sight_retry<'v>(&self, opts: &SpawnOpts, id: LaunchId, v: &'v Value) -> Option<&'v str> {
        let category = message_error(v)?;
        if AgentFailure::from_category(category).row().disposition == Disposition::Hold {
            self.sight(opts, id, category, category);
        }
        Some(category)
    }

    /// Drain the lines already on a launch's stream, ahead of the hold read at its end (spec 105
    /// HELD): until `line_rx` is empty or closed, each line is written to `stream_file` and an
    /// `api_retry` among them sighted as the read loop sights one ([`Driver::sight_retry`]), so a
    /// hold the launch's last lines sighted holds the run before that read. It acts on nothing
    /// else: a drained `result` stays unread, the ending takes no class from a drained line and no
    /// close changes. A line that does not read is skipped.
    fn drain_lines(
        &self,
        line_rx: &std::sync::mpsc::Receiver<std::io::Result<String>>,
        stream_file: &mut Option<std::fs::File>,
        opts: &SpawnOpts,
        id: LaunchId,
    ) {
        while let Ok(read) = line_rx.try_recv() {
            let Ok(line) = read else {
                continue;
            };
            write_stream_line(stream_file, &line);
            let Ok(v) = serde_json::from_str::<Value>(&line) else {
                continue;
            };
            if line_kind(&v) == (Some("system"), Some("api_retry")) {
                self.sight_retry(opts, id, &v);
            }
        }
    }

    /// Re-read this run's failed closes, this launch's own already written, and sight
    /// `api-unstable` at the third in a row (spec 105): concurrent closes each re-read, and the
    /// hold's writer leaves one `RunHeld` between them. A read that fails is a progress line and
    /// counts nothing, the next close re-reads.
    fn sight_unstable(&self, opts: &SpawnOpts, id: LaunchId) {
        let Some(sink) = self.hold_sink else {
            return;
        };
        let count = sink
            .last_position(run::STREAM, run::TYPE_RUN_RELEASED)
            .and_then(|released| {
                let events = progress::read_run(self.progress_store, &opts.run_id)?;
                Ok(progress::failed_closes_since(&events, released))
            });
        match count {
            Ok(count) if count >= progress::API_UNSTABLE_CLOSES => {
                self.sight(opts, id, API_UNSTABLE, "");
            }
            Ok(_) => {}
            Err(e) => self.record_progress(opts, &format!("failed closes not counted: {e}")),
        }
    }

    /// A LAUNCH FAULT (spec 104 THE STREAM: "a `rigger` server that did not connect fails the
    /// launch as a fault"): stop the session through THE STOP's own sequence
    /// ([`Driver::stop_session`]) and end the launch under class `unknown` ([`Driver::fault`]) -
    /// the class the CONSTRAINTS WALK gives a launch that never became a working session, and
    /// the only one that fits, since every named category is an API-side error the session
    /// reported and this one is the host's own finding. The message carries the server's
    /// reported status.
    ///
    /// No grace: THE STOP's grace buys a session that is winding down time to exit on its own,
    /// but a session whose init just arrived is STARTING its task, and closing its input does
    /// not stop the turn already in flight - every second of grace is a second of that task
    /// running with its tools and without rigger's (2026-09-28 probe: the whole task ran).
    fn stop_for_launch_fault(
        &self,
        reaper: crate::reaped_child::ReapedChild,
        pipes: Pipes,
        opts: &SpawnOpts,
        id: LaunchId,
        status: &str,
    ) -> Result<StreamEnd, Error> {
        let class = AgentFailure::Unknown;
        let message = format!(
            "claude_code driver: {:?}: its {} MCP server reported status {status:?} at init, \
             not connected - the session was stopped so its task never runs without the rigger \
             tools (class {class})",
            opts.id,
            hooks::MCP_SERVER_NAME
        );
        self.record_progress(opts, &format!("stream: {message}"));
        self.stop_session(reaper, pipes, opts, std::time::Duration::ZERO);
        let read = self.ending_read(opts, class);
        self.fault(opts, id, class, message, read)
    }

    /// How long this session has really been silent since `last_line`, the reading THE STOP
    /// weighs against `max_wall_clock` (spec 105 LIVENESS): [`liveness::silent_time`] over the
    /// run's holds and, from [`Driver::beat_source`], this spawn's announced retry delays and
    /// the supervisor's beats. With no beat source it reads no record and counts from the last
    /// line. A record read that fails reads no silence at all (`None`): that wake proves nothing
    /// and the next wake, a poll away ([`next_wait`]), reads again, so a store hiccup never stops
    /// a healthy session.
    fn silence(&self, opts: &SpawnOpts, last_line: SystemTime) -> Option<std::time::Duration> {
        let now = SystemTime::now();
        let Some(beat_source) = self.beat_source else {
            return Some(liveness::silent_time(last_line, now, &[], &[], None));
        };
        match (
            read_current_run(self.run_store, STREAM),
            progress::read_run(beat_source, &opts.run_id),
        ) {
            (Ok((run_events, _)), Ok(progress_events)) => Some(liveness::silent_time(
                last_line,
                now,
                &liveness::hold_intervals(&run_events, now),
                &liveness::retry_delays(&progress_events, &opts.id),
                Some(&liveness::beat_times(&progress_events)),
            )),
            _ => None,
        }
    }

    /// THE STOP when a session's silent time passes `max_wall_clock` (spec 104 criterion 6),
    /// factored out of
    /// [`Driver::read_stream`]'s loop so its one call site there reads as a single named step:
    /// runs THE STOP sequence ([`Driver::stop_session`], with the full [`Driver::stop_grace`]),
    /// then ends the launch under the class `assistant_error` reported
    /// ([`Driver::close_stopped`]).
    /// THE STOP
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
    /// own fault. A resume that never showed its session never comes here: its arm runs the
    /// sequence with no line yet and reads the hold behind it, since no fault stands for a session
    /// that was never there to go silent, so on a run proved unheld it takes the fallback
    /// ([`Driver::session_gone`]), and on any other the same line and ending as here.
    fn stop_for_wall_clock_silence(
        &self,
        reaper: crate::reaped_child::ReapedChild,
        pipes: Pipes,
        opts: &SpawnOpts,
        id: LaunchId,
        max_wall_clock: u64,
        assistant_error: Option<&str>,
    ) -> Result<StreamEnd, Error> {
        let message = self.announce_stop(opts, max_wall_clock);
        self.stop_session(reaper, pipes, opts, self.stop_grace);
        self.close_stopped(opts, id, message, assistant_error)
    }

    /// Say on the spawn's progress why it was stopped: the stop's `message`.
    fn announce_stop(&self, opts: &SpawnOpts, max_wall_clock: u64) -> String {
        let message = stop_message(&opts.id, max_wall_clock);
        self.record_progress(opts, &format!("stream: {message}"));
        message
    }

    /// End a launch THE STOP's sequence has ended, `message` naming why: records the EXISTING
    /// liveness-fault shape and closes the launch record `stopped` with no class. Its child gone,
    /// it takes the class its session reported, `assistant_error` or its `StopFailure` record
    /// ([`Driver::no_result_class`]), and sights it as any ending does ([`Driver::sight_ending`]);
    /// its `api_retry` lines take no part, each sighted when it was read, one sighting a line.
    /// Then it counts the failed closes ([`Driver::sight_unstable`]) before the `Err` returns.
    fn close_stopped(
        &self,
        opts: &SpawnOpts,
        id: LaunchId,
        message: String,
        assistant_error: Option<&str>,
    ) -> Result<StreamEnd, Error> {
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

        // "the launch ends `stopped`", the liveness-fault result above being the truth of it.
        self.close(opts, id, progress::ENDED_STOPPED, "");
        self.sight_ending(opts, id, self.no_result_class(opts, assistant_error, None));
        self.sight_unstable(opts, id);

        Err(Error(format!(
            "claude_code driver: {:?}: {message}",
            opts.id
        )))
    }

    /// THE STOP sequence itself (spec 104 STOP: "closes the session's input, waits 30 s, then
    /// ends the child through the sanctioned lifecycle helper on the child's own handle"),
    /// the ONE stop mechanism every ending that must stop a live session goes through - a
    /// silence expiry ([`Driver::stop_for_wall_clock_silence`], the full
    /// [`Driver::stop_grace`]), a resume silent with no session ([`Driver::read_stream`]'s stop
    /// arm, the full grace too) and a launch fault ([`Driver::stop_for_launch_fault`], no
    /// grace) - so none ever signals a session any other way. `grace` bounds only the
    /// passive wait for the child to exit on its own; the pipe joins stay bounded by
    /// [`Driver::stop_grace`] whatever `grace` is. Records nothing: what a stop MEANS is each
    /// caller's own record.
    fn stop_session(
        &self,
        mut reaper: crate::reaped_child::ReapedChild,
        mut pipes: Pipes,
        opts: &SpawnOpts,
        grace: std::time::Duration,
    ) {
        // "closes the session's input stream" - the SAME close THE STREAM's own success path
        // uses to end a session's turn ([`Pipes::close_input`]); a write the session never read
        // holds it until the end below ends that write.
        pipes.close_input();

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

        // "waits a grace period" (`grace`: self.stop_grace at a silence expiry - 30s in
        // production, an injected shorter one in a test proving the full sequence): poll
        // for the child exiting on its own, without blocking the full grace when it
        // already has. Bounded by
        // `saturating_duration_since` rather than a raw `now() < deadline` comparison: the
        // exact tie between `now()` and a fixed `Instant` a prior addition computed is
        // unobservable to any test, so a boundary operator there is one no test observes;
        // this shape leaves no such operator at all.
        let deadline = std::time::Instant::now() + grace;
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

        drop(reaper);
        // Bounded (spec 104 criterion 6 round-4 fix, [`Driver::join_within`]): the child's
        // own pipe copy closes the moment `end_child` above finishes it and its known
        // descendants, but a descendant that had ALREADY escaped the tree before that
        // snapshot ran still holds its own copy open, and no signal this function sent
        // could ever reach it - this is the backstop that keeps THE STOP returning anyway.
        // Bounded by `self.stop_grace` here (THIS is THE STOP's own wait-for-a-graceful-
        // exit concern, unlike `read_stream`'s ORDINARY-exit joins just below, which use
        // the dedicated `ORDINARY_DRAIN_JOIN_BOUND` instead).
        self.join_pipes(opts, pipes, self.stop_grace);
    }

    /// Join a reaped launch's pipe threads, each within `bound` ([`Driver::join_within`]), and
    /// return the stderr tail: empty when its drain outran the bound, panicked or had no pipe.
    /// The session's input closes first ([`Pipes::close_input`]), so the writer returns, and a
    /// write that failed leaves a progress line naming its error.
    fn join_pipes(
        &self,
        opts: &SpawnOpts,
        mut pipes: Pipes,
        bound: std::time::Duration,
    ) -> Vec<u8> {
        pipes.close_input();
        if let Some(Err(e)) = pipes
            .input
            .and_then(|writer| self.join_within(opts, "stdin", writer, bound))
        {
            self.record_progress(opts, &format!("stream (task write failed): {e}"));
        }
        let tail = pipes
            .stderr_drain
            .and_then(|handle| self.join_within(opts, "stderr", handle, bound))
            .unwrap_or_default();
        self.join_within(opts, "stdout", pipes.stdout_reader, bound);
        tail
    }

    /// Best-effort progress line (spec 14's mechanism, spec 104's own writer): a lost
    /// progress line is a smaller loss than a lost launch record or a lost result, so
    /// this never fails the spawn the way [`Driver::launch`]'s `SpawnLaunched` write does.
    fn record_progress(&self, opts: &SpawnOpts, activity: &str) {
        let _ = progress_store::record(self.progress_store, &opts.run_id, &opts.id, activity);
    }

    /// Best-effort [`ApiRetry`] of one `system/api_retry` message `v` (spec 105 LIVENESS), for
    /// the same reason as [`Driver::record_progress`]: a lost record loses one announced
    /// delay, a smaller loss than failing a live spawn that is already retrying.
    fn record_api_retry(&self, opts: &SpawnOpts, v: &Value) {
        let (delay_ms, category) = api_retry_fields(v);
        let retry = ApiRetry {
            spawn: opts.id.clone(),
            delay_ms,
            category: category.to_string(),
        };
        let _ = progress_store::append_stamped(
            self.progress_store,
            &opts.run_id,
            &retry,
            "the api-retry record",
        );
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

/// Write `message` to a session's input and hold the input open until the host closes it by
/// dropping the sender `close` waits on ([`Pipes::close_input`]): the first message's writer, run
/// on a thread of its own ([`Driver::read_stream`]) so a session that never reads its input never
/// holds the host in a write. A write such a session never reads returns when the child ends.
/// Returns the write's own outcome; the input closes as this returns.
fn write_first_message(
    mut stdin: ChildStdin,
    message: &str,
    close: std::sync::mpsc::Receiver<()>,
) -> std::io::Result<()> {
    let written = writeln!(stdin, "{message}");
    let _ = close.recv();
    written
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
    // operator that no test could observe.
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
    ///
    /// RESUME BY SESSION (spec 105): before any launch the current run's log is read
    /// ([`read_or_park`]), then the launch records and the hold sink ([`Driver::first_launch`]),
    /// and a failed read among them returns its error marked a store fault ([`store_fault`]),
    /// once, before the reclaim, so the spawn's scratch stands for the launch a restart makes. A spawn the run ended is answered from its result with no launch, an
    /// error result as that error, carrying the session and resolved model the result recorded
    /// and no `resumed_from`, so a restarted run does not run a finished agent again. A spawn
    /// whose only result is a liveness fault, or one only a prior run ended, is launched. A
    /// spawn whose request does not stand in the run is parked first, stamped with the run and
    /// this host's scratch root, so the budget's admission finds every spawn this host ran. The
    /// first launch resumes the session this run's latest launch of the id left `interrupted`,
    /// after a release or a restart alike, telling it which ([`Driver::first_launch`]).
    ///
    /// THE RESUME FALLBACK (spec 105 criterion 1) lands here: a launch that ends
    /// [`StreamEnd::NoSession`] is followed, inside this call and charged no attempt, by a fresh
    /// launch with the full `prompt` at the next ordinal, and the spawn's result is that
    /// launch's. The loop runs at most twice, since a fresh launch never ends `NoSession`.
    ///
    /// HELD MEANS NO LAUNCH AND NO CHARGE (spec 105): a launch that ends
    /// [`StreamEnd::Interrupted`] returns its error with its scratch kept, since the spawn has
    /// not ended, only that launch: the same id launches again once the hold releases, resuming
    /// that launch's session, and the run's own end reclaims whatever no next launch does. The
    /// stop and the read error of a run not proved unheld return their error through the reclaim
    /// like any other `Err`, and the decorator weighs what comes next.
    fn spawn(
        &self,
        agent: &AgentDef,
        prompt: &str,
        opts: &SpawnOpts,
        _emit: &dyn Fn(&str, Value) -> Result<(), Error>,
    ) -> Result<AgentResult, Error> {
        // THE STOP (spec 104 criterion 6): the agent's own resolved bound (spec 10 unit
        // 3, `AgentDef::max_wall_clock`, already folded from `defaults.max_wall_clock` at
        // config-load time) - 0 stays unbounded, the established convention this field's
        // own doc already sets.
        let first = match self
            .before_launch(agent, prompt, opts)
            .map_err(store_fault)?
        {
            Prelaunch::Answered(res) => {
                let session_id = res.meta_str(crate::spawn::META_SESSION_ID);
                return answered(res).map(|answer| AgentResult {
                    session_id,
                    ..answer
                });
            }
            Prelaunch::Launch(first) => first,
        };
        let bound = agent.max_wall_clock.unwrap_or(0);
        let mut launched = self.launch_as(
            first.session,
            agent,
            &first.task,
            opts,
            self.progress_store,
            Some(first.ordinal),
        );
        let result = loop {
            match launched.and_then(|launch| self.read_stream(launch, opts, bound)) {
                Ok(StreamEnd::Result(result)) => break Ok(result),
                Ok(StreamEnd::NoSession) => {
                    let session = LaunchSession::fresh();
                    let store = self.progress_store;
                    launched = self.launch_as(session, agent, prompt, opts, store, None);
                }
                Ok(StreamEnd::Interrupted(e)) => return Err(e),
                Err(e) => break Err(e),
            }
        };
        // This call's terminus: `read_stream` reaped the session (or the launch never started
        // one), whatever its outcome, and this host records its result directly, never through
        // the `rigger result` courier that reclaims a stepwise spawn's scratch - so reclaim the
        // scratch its spawn MCP server names (`rigger_scratch`, under this host's own scratch
        // root) and its system-prompt file sits in here.
        reclaim_spawn_registered_scratch(&self.scratch_root, &opts.run_id, &opts.id);
        result
    }
}

/// The task a hold probe's canary is given: the least a session can answer.
pub const CANARY_TASK: &str = "Reply with the single word: ok";

/// The canary's `--settings`: every hook off, the operator's own included.
const CANARY_SETTINGS: &str = r#"{"disableAllHooks":true}"#;

impl HoldProbe for Driver<'_> {
    /// THE HOLD PROBE (spec 105 RELEASE): whether a launch on the hold `owed` probes would answer
    /// now, within `owed.delay`. A hold whose cause a login fixes reads the login first
    /// ([`crate::claude_auth::logged_in`]), and a session logged out runs no canary. The canary is
    /// a host entry apart from [`AgentDriver::spawn`]: a session on the hold's model in `root`,
    /// on this process's environment with no tools, no MCP server, no hooks and no session file
    /// ([`canary_args`]), its task [`CANARY_TASK`], read with the readers [`Driver::read_stream`]
    /// uses. Its first result answers, the input closing on it so the session ends on its own;
    /// one that never comes by the delay, or a canary that does not start, fails. The login read
    /// and the canary share the one deadline. It writes no store: the caller records what the
    /// outcome decides.
    fn probe(&self, root: &str, owed: &OwedProbe) -> ProbeOutcome {
        let deadline = std::time::Instant::now() + owed.delay;
        let bin = bin_or_path_default(&self.bin, crate::claude_auth::DEFAULT_BIN);
        if owed.checks_login() && !crate::claude_auth::logged_in(bin, deadline) {
            return ProbeOutcome::LoggedOut;
        }
        let mut category = None;
        let mut passed = false;
        let spawned = crate::subprocess::command_in(bin, root)
            .args(canary_args(&owed.held.model))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn();
        if let Ok(child) = spawned {
            let mut canary = crate::reaped_child::ReapedChild::new(child);
            let mut input = canary.child_mut().stdin.take();
            if let Some(stdin) = input.as_mut() {
                let _ = writeln!(stdin, "{}", first_user_message(CANARY_TASK));
            }
            canary.read_lines_until(deadline, |line| {
                let Ok(v) = serde_json::from_str::<Value>(line) else {
                    return std::ops::ControlFlow::Continue(());
                };
                match line_kind(&v) {
                    (Some("system"), Some("api_retry")) if input.is_some() => {
                        if let Some(latest) = message_error(&v) {
                            category = Some(latest.to_string());
                        }
                    }
                    (Some("result"), _) if input.is_some() => {
                        passed = !result_errored(&v);
                        input = None;
                    }
                    _ => {}
                }
                std::ops::ControlFlow::Continue(())
            });
        }
        if passed {
            ProbeOutcome::Passed
        } else {
            ProbeOutcome::Failed(category.unwrap_or_else(|| AgentFailure::Unknown.as_str().into()))
        }
    }
}

/// The hold probe's canary argv on `model`: [`stream_json_args`], then no tools, no MCP server
/// (`--strict-mcp-config` with no config), no hooks ([`CANARY_SETTINGS`]), no session file and
/// [`model_args`]. Never `--bare`, which reads no OAuth login and so would probe nothing the
/// operator's sessions use.
fn canary_args(model: &str) -> Vec<String> {
    let mut args = stream_json_args();
    args.extend(
        [
            "--tools",
            "",
            "--strict-mcp-config",
            "--settings",
            CANARY_SETTINGS,
            "--no-session-persistence",
        ]
        .map(str::to_string),
    );
    args.extend(model_args(model));
    args
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
/// ([`crate::driver::replay::spawn_scratch_path`]). Public for the binary's two readers of the
/// layout: a done run's scratch teardown, which removes this area's per-run entries beside its
/// siblings', and a command that runs this host outside a loop run (`rigger critique`, spec 112),
/// which finds the run directory the transcripts [`remove_transcripts`] removes sit in, and
/// removes it once it is left empty.
pub const AGENT_STREAM_SUBDIR: &str = "agent-stream";

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

/// The launch the file named `file` would be a transcript of, beside the stream leaf named
/// `leaf`: the first run of digits after the leaf name. [`remove_transcripts`] holds the file to
/// the path [`stream_path`] builds for that launch, so a file of another spawn whose leaf extends
/// this one's (attempt 10's beside attempt 1's) never matches.
fn transcript_launch(leaf: &str, file: &str) -> Option<u32> {
    let digits: String = file
        .strip_prefix(leaf)?
        .chars()
        .skip_while(|c| !c.is_ascii_digit())
        .take_while(char::is_ascii_digit)
        .collect();
    digits.parse().ok()
}

/// Remove every transcript spawn `spawn_id` of run `run_id` left under `scratch_root`, one per
/// launch: a file is removed only when it is the path [`stream_path`] builds for its launch, so
/// nothing else beside them is touched - for a command that runs this host outside a loop run
/// (`rigger critique`, spec 112), whose transcripts no run teardown removes. An empty scratch root
/// or spawn id removes nothing. Best-effort, as the transcript itself is: a file that cannot be
/// removed stays.
pub fn remove_transcripts(scratch_root: &str, run_id: &str, spawn_id: &str) {
    let Some(leaf) = liveness::scratch_subpath(scratch_root, AGENT_STREAM_SUBDIR, run_id, spawn_id)
    else {
        return;
    };
    let (Some(dir), Some(name)) = (leaf.parent(), leaf.file_name().and_then(|n| n.to_str())) else {
        return;
    };
    for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let path = entry.path();
        let launch = entry
            .file_name()
            .to_str()
            .and_then(|file| transcript_launch(name, file));
        let named = launch.and_then(|launch| stream_path(scratch_root, run_id, spawn_id, launch));
        if named.as_deref() == Some(path.as_path()) {
            let _ = std::fs::remove_file(path);
        }
    }
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

/// Append one stream `line` to the launch's raw stream file, when [`open_stream_file`] opened
/// one. Best-effort: the transcript is a diagnostic/audit artifact, never the record of truth
/// (the progress store and the run store are), so a full disk here must not fail an
/// otherwise-healthy spawn.
fn write_stream_line(stream_file: &mut Option<std::fs::File>, line: &str) {
    if let Some(f) = stream_file.as_mut() {
        let _ = writeln!(f, "{line}");
    }
}

/// The announced delay and the category of a `system/api_retry` message: the one reading of
/// the two fields the waiting line and the [`ApiRetry`] record both carry, a field the line
/// lacks reading as `0` and `unknown`. A retry the API sent no response headers for announces
/// its wait as the header wait `no_response.retry_wait_ms` beside a zero `retry_delay_ms`, so
/// that wait is the delay when the line carries it.
fn api_retry_fields(v: &Value) -> (u64, &str) {
    (
        v.pointer("/no_response/retry_wait_ms")
            .or_else(|| v.get("retry_delay_ms"))
            .and_then(Value::as_u64)
            .unwrap_or(0),
        message_error(v).unwrap_or("unknown"),
    )
}

/// A stream message's `type` and `subtype`, the pair every reader of this host's stream
/// dispatches on.
fn line_kind(v: &Value) -> (Option<&str>, Option<&str>) {
    (
        v.get("type").and_then(Value::as_str),
        v.get("subtype").and_then(Value::as_str),
    )
}

/// The error category a stream message names (its `error`), when it names one: a
/// `system/api_retry` line's, or an assistant line's ahead of the result it fails.
fn message_error(v: &Value) -> Option<&str> {
    v.get("error").and_then(Value::as_str)
}

/// Whether a `result` message is the session reporting that it answered nothing: its `is_error`
/// is `true`.
fn result_errored(v: &Value) -> bool {
    v.get("is_error").and_then(Value::as_bool) == Some(true)
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
    let (delay_ms, category) = api_retry_fields(v);
    let attempt = v.get("attempt").and_then(Value::as_u64).unwrap_or(0);
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

/// The failure of a launch whose stream failed to read with no result on hand: the spawn's own,
/// which [`Driver::read_stream`] returns from the loop or, for a launch still awaiting its
/// session, from its tail.
fn read_agent_stream_error(opts: &SpawnOpts, e: &std::io::Error) -> Error {
    Error(format!(
        "claude_code driver: {:?}: read agent stream: {e}",
        opts.id
    ))
}

/// Map a `result` stream message to the [`SpawnResult`] THE STREAM records (Design:
/// "the `result` message becomes the `SpawnResult`: `output`, and `meta` with
/// `resolved_model`, `session_id`, `usage` (`input`, `output`, `cache_creation`,
/// `cache_read`), `turns`, `cost_usd`, `permission_denials`"). `permission_denials` has ONE
/// source of truth: the length of the result message's own `permission_denials` array -
/// never a count of `system/permission_denied` stream events, which [`Driver::read_stream`]
/// keeps as live progress notes only. 2026-09-28 probe against the real claude 2.1.283: a
/// PreToolUse hook block (the graph-first lookup guard denying a text search) emits no
/// `permission_denied` event yet is listed in the result's array, so the event count
/// recorded 0 where the session itself reported 1; a real permission denial appears in both,
/// so adding the two would double count. The tell: a recorded denial count that disagrees
/// with the result line in the raw stream transcript. Every numeric field defaults to its
/// zero value when the real message omits it, so a result line missing a field it usually
/// carries still yields a well-formed record rather than failing the whole spawn over one
/// absent number.
fn spawn_result_from(v: &Value, opts: &SpawnOpts, resolved_model: &str) -> SpawnResult {
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
        crate::spawn::META_RESOLVED_MODEL: resolved_model,
        crate::spawn::META_SESSION_ID: session_id,
        "usage": {
            "input": get_u64("input_tokens"),
            "output": get_u64("output_tokens"),
            "cache_creation": get_u64("cache_creation_input_tokens"),
            "cache_read": get_u64("cache_read_input_tokens"),
        },
        "turns": v.get("num_turns").and_then(Value::as_u64).unwrap_or(0),
        "cost_usd": v.get("total_cost_usd").and_then(Value::as_f64).unwrap_or(0.0),
        "permission_denials": v
            .get("permission_denials")
            .and_then(Value::as_array)
            .map_or(0, Vec::len),
    });
    SpawnResult::ok(opts.id.clone(), output).with_meta(meta)
}

/// The status the spawn's own MCP server ([`hooks::MCP_SERVER_NAME`], the one server
/// [`spawn_config_args`] configures) reported in a `system/init` message, when that status is
/// anything but `connected` - a LAUNCH FAULT (spec 104 THE STREAM: "a `rigger` server that did
/// not connect fails the launch as a fault"). `None` when it connected, or when init lists no
/// entry for it. Every other server's status is not this host's concern:
/// `--strict-mcp-config` configures no other, and a stray one never decides whether the
/// spawn's own surface works.
fn rigger_server_fault(init: &Value) -> Option<String> {
    let server = init
        .get("mcp_servers")?
        .as_array()?
        .iter()
        .find(|s| s.get("name").and_then(Value::as_str) == Some(hooks::MCP_SERVER_NAME))?;
    let status = server
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or_default();
    (status != "connected").then(|| status.to_string())
}

/// `task` framed with the live-progress instruction ([`progress_framed`]), reported through
/// the tool every launch of this host pre-approves ([`SPAWN_BOUND_TOOLS`]).
pub fn framed(task: &str) -> String {
    let report =
        format!("by calling the {SPAWN_PROGRESS_TOOL} tool with that line as its activity");
    progress_framed(task, &report)
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

/// The roots a launch's write guard admits: the session's working directory,
/// [`crate::subprocess::session_dir`] of `dir` as [`crate::subprocess::command_in`] runs it, then
/// the spawn's scratch `container` when it has one. Each is made absolute here, since the guard
/// resolves a root lexically and never against the session's cwd, so it refuses an empty or
/// relative root and with it every write.
fn write_roots(dir: &str, container: Option<&Path>) -> Result<Vec<String>, Error> {
    std::iter::once(crate::subprocess::session_dir(dir))
        .chain(container)
        .map(|root| {
            std::path::absolute(root)
                .map(|root| root.to_string_lossy().into_owned())
                .map_err(|e| {
                    Error(format!(
                        "claude_code driver: resolve the write root {}: {e}",
                        root.display()
                    ))
                })
        })
        .collect()
}

/// The headless stream-json base every launch of this host starts with: print mode, typed
/// stream-json lines out and in, and every message on the stream.
fn stream_json_args() -> Vec<String> {
    [
        "-p",
        "--output-format",
        "stream-json",
        "--input-format",
        "stream-json",
        "--verbose",
    ]
    .map(str::to_string)
    .to_vec()
}

/// `--model <model>`, or nothing for an empty `model`, which leaves the session on its default.
fn model_args(model: &str) -> Vec<String> {
    if model.is_empty() {
        Vec::new()
    } else {
        vec!["--model".to_string(), model.to_string()]
    }
}

/// Build the typed `claude` headless invocation (architecture addendum §4.1 table): the
/// ONE argv authority for this driver, exactly as `cli::build_args` is for the cli driver
/// - every field below is a fact, never inferred at read time.
///
/// Neither prompt is an argv string, since Linux caps one argument at `MAX_ARG_STRLEN` (131072
/// bytes): the task is the first stream-json message on stdin and the persona reaches the
/// session through the file `persona` holds (`--system-prompt-file`), omitted for an empty
/// persona. The launch runs as `session`: `--session-id` for a fresh one, `--resume` to
/// continue the one the spawn names ([`LaunchSession`], shared with the cli host).
///
/// `--settings` carries the hooks bound to this spawn ([`hooks::install_spawn_hooks`]) ahead of
/// the session's own: the write guard rooted at `write_roots` and the `StopFailure` entries, both
/// running `rigger_bin`.
///
/// `--json-schema` (verdict personas, §4.5) is deliberately out of this criterion's scope
/// (the Done-when text names session id, stream-json, persona, model, tools, permission
/// flags, MCP config and settings only) and is not built here; a later criterion adds it
/// alongside whichever caller knows a persona is a verdict role, through this same
/// function rather than a second argv authority. Fails only when the spawn's own
/// `settings_json` is not a JSON object the spawn's hooks and the session settings can merge into.
pub fn build_args(
    agent: &AgentDef,
    persona: &SystemPromptFile,
    opts: &SpawnOpts,
    session: &LaunchSession,
    rigger_bin: &str,
    write_roots: &[String],
) -> Result<Vec<String>, Error> {
    let mut args = stream_json_args();
    args.extend(session.args());
    args.extend(persona.args());
    args.extend(model_args(&agent.model_for_attempt(opts.attempt)));
    if !agent.fallback_model.is_empty() {
        args.push("--fallback-model".to_string());
        args.push(agent.fallback_model.clone());
    }
    args.extend(allowed_tools_args(agent, &SPAWN_BOUND_TOOLS)?);
    let settings = json_arg(
        hooks::install_spawn_hooks(
            opts.settings_json.as_bytes(),
            rigger_bin,
            &opts.id,
            write_roots,
        ),
        "settings",
    )?;
    args.extend(spawn_config_args(
        rigger_bin,
        &["mcp", "--spawn", &opts.id],
        &settings,
    )?);
    Ok(args)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::{store_fault, AgentFailure};
    use crate::eventstore::sqlite::Store;
    use crate::eventstore::{Direction, EventStore, SilentStore};
    use crate::test_support::implementer_opts as opts;
    use crate::test_support::{
        captured_system_prompt, fixture_script, no_emit, spawn_result_rows, PROMPT_CAPTURE_DIR,
    };
    use std::io::Read;
    use std::time::Duration;

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
        o.settings_json = "{\"model\":\"opus\"}".to_string();
        let scratch = tempfile::tempdir().unwrap();
        let persona =
            system_prompt_file(&o.system_prompt, Some(scratch.path().to_path_buf())).unwrap();
        let args = build_args(&a, &persona, &o, &fresh_session("sess-123"), "rigger", &[]).unwrap();

        assert_eq!(args[0], "-p");
        // No positional prompt: the task is the first stream-json message on stdin.
        assert_eq!(args[1], "--output-format");
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
        assert_eq!(
            std::fs::read_to_string(get_val("--system-prompt-file")).unwrap(),
            "You implement findings.",
            "the persona reaches the session through its file, never as an argv string"
        );
        assert!(!args.iter().any(|x| x == "--system-prompt"));
        assert_eq!(get_val("--model"), "sonnet");
        assert_eq!(get_val("--fallback-model"), "haiku");
        assert_eq!(
            get_val("--allowed-tools"),
            format!(
                "Read,Bash,{},{}",
                hooks::helper_mcp_tools().unwrap().join(","),
                SPAWN_BOUND_TOOLS.join(",")
            )
        );
        assert_eq!(get_val("--permission-mode"), "default");
        assert_eq!(get_val("--permission-prompts"), "none");
        assert!(args.iter().any(|x| x == "--strict-mcp-config"));
        let settings: serde_json::Value = serde_json::from_str(&get_val("--settings")).unwrap();
        assert_eq!(
            settings["model"], "opus",
            "the spawn's own settings survive the merge"
        );
        assert_eq!(
            settings["hooks"]["SessionStart"][0]["hooks"][0]["command"],
            hooks::PRIME_COMMAND
        );
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
    /// (`read_stream` only); `append`/`read_all`/`read_all_page`/`log_head`/`subscribe_*` are
    /// never reached by `classify_no_result`'s read-only path, so they degrade the same inert
    /// way `SilentStore`'s own unused arms do.
    struct FailingReadStore;
    impl FailingReadStore {
        /// The one failure every read of this double answers.
        fn read_failure() -> crate::eventstore::Error {
            crate::eventstore::Error::Backend("simulated progress-store read failure".to_string())
        }
    }
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
            Err(FailingReadStore::read_failure())
        }
        fn read_all(
            &self,
            _from: crate::eventstore::Position,
            _dir: Direction,
            _filter: &crate::eventstore::Filter,
        ) -> Result<Vec<crate::eventstore::Event>, crate::eventstore::Error> {
            Ok(Vec::new())
        }
        fn read_all_page(
            &self,
            _from: crate::eventstore::Position,
            _filter: &crate::eventstore::Filter,
            _max: usize,
        ) -> Result<Vec<crate::eventstore::Event>, crate::eventstore::Error> {
            Ok(Vec::new())
        }
        fn log_head(
            &self,
        ) -> Result<Option<crate::eventstore::Position>, crate::eventstore::Error> {
            Ok(None)
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
        fn last_position(
            &self,
            _stream: &str,
            _event_type: &str,
        ) -> Result<Option<crate::eventstore::Revision>, crate::eventstore::Error> {
            Ok(None)
        }
        fn read_stream_typed(
            &self,
            _stream: &str,
            _from: crate::eventstore::Revision,
            _selection: crate::eventstore::TypeSelection,
        ) -> Result<Vec<crate::eventstore::Event>, crate::eventstore::Error> {
            Err(FailingReadStore::read_failure())
        }
        fn read_stream_positions(
            &self,
            _stream: &str,
            _batch: usize,
            _sink: &mut dyn FnMut(
                &[crate::eventstore::Position],
            ) -> Result<(), crate::eventstore::Error>,
        ) -> Result<(), crate::eventstore::Error> {
            Err(FailingReadStore::read_failure())
        }
        fn read_stream_batched(
            &self,
            _stream: &str,
            _from: crate::eventstore::Revision,
            _batch: usize,
            _sink: &mut crate::eventstore::EventBatchSink,
        ) -> Result<(), crate::eventstore::Error> {
            Err(FailingReadStore::read_failure())
        }
        fn latest_in_group(
            &self,
            _stream: &str,
            _group: &str,
        ) -> Result<Option<crate::eventstore::GroupHead>, crate::eventstore::Error> {
            Err(FailingReadStore::read_failure())
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
        let (class, message) =
            driver.classify_no_result(&o, None, &Some("overloaded".to_string()), b"stderr tail");
        assert_eq!(
            class,
            AgentFailure::Overloaded,
            "a failed progress-store read must still fall through to the api_retry \
             category, never fail the whole classification: {message}"
        );
    }

    /// The silence THE STOP weighs at a wake: with no beat source the wall time since the last
    /// line, read from no store; with one, a record that does not read, from either store, leaves
    /// the wake uncounted rather than reading as a stop, and the next wake a poll away rather than
    /// a whole bound. The readable store carries a beat the last line follows, so a working read
    /// of either pair counts the silence since that line and re-arms for the rest of the bound.
    #[test]
    fn the_stop_counts_from_the_line_with_no_source_and_not_at_all_on_a_failed_read() {
        let readable = Store::open(":memory:").unwrap();
        let o = opts("u/implementer#0");
        progress_store::append_stamped(
            &readable,
            &o.run_id,
            &progress::SupervisorBeat::default(),
            "a supervisor beat",
        )
        .unwrap();
        let after_the_beat = SystemTime::now();
        let bound = Duration::from_secs(60);
        let working = Driver {
            beat_source: Some(&readable),
            run_store: &readable,
            ..Driver::default()
        };
        let counted = working
            .silence(&o, after_the_beat)
            .expect("a working read counts the silence since the line");
        assert!(
            next_wait(bound, Some(counted)) > UNBOUNDED_POLL,
            "a working read re-arms for the rest of the bound: {counted:?}"
        );
        let quiet = Duration::from_secs(40);
        let last_line = SystemTime::now() - quiet;

        let no_source = Driver {
            run_store: &FailingReadStore,
            progress_store: &FailingReadStore,
            ..Driver::default()
        };
        let counted = no_source
            .silence(&o, last_line)
            .expect("a host with no beat source reads no record");
        assert!(
            counted >= quiet && counted < quiet + Duration::from_secs(60),
            "wall time since the line: {counted:?}"
        );

        let failed_reads: [(&str, &dyn EventStore, &dyn EventStore); 2] = [
            ("the beat source", &FailingReadStore, &readable),
            ("the run store", &readable, &FailingReadStore),
        ];
        for (case, beat_source, run_store) in failed_reads {
            let driver = Driver {
                beat_source: Some(beat_source),
                run_store,
                ..Driver::default()
            };
            assert_eq!(
                next_wait(bound, driver.silence(&o, after_the_beat)),
                UNBOUNDED_POLL,
                "{case}: the next wake"
            );
        }
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
        let (class, message) =
            driver.classify_no_result(&o, None, &Some("overloaded".to_string()), b"tail");
        assert_eq!(
            class, expected,
            "a StopFailure recorded under run {record_run:?} must classify run-1's no-result \
             exit as {expected}: {message}"
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

    /// The roots of a launch's write guard: the session's working directory, an empty `dir`
    /// being the host's own, then the spawn's scratch container when it has one, each absolute.
    #[test]
    fn write_roots_are_the_sessions_cwd_then_the_container_each_absolute() {
        let cwd = std::env::current_dir().unwrap();
        let cwd = cwd.to_string_lossy().into_owned();
        let under = |rel: &str| format!("{cwd}/{rel}");
        let rows = [
            ("/wt/u", Some("/scratch/s"), vec!["/wt/u", "/scratch/s"]),
            ("/wt/u", None, vec!["/wt/u"]),
            ("", Some("/scratch/s"), vec![cwd.as_str(), "/scratch/s"]),
            ("", None, vec![cwd.as_str()]),
            ("rel/wt", None, vec![under("rel/wt").leak()]),
            ("/wt/u", Some("rel/s"), vec!["/wt/u", under("rel/s").leak()]),
        ];
        for (dir, container, expected) in rows {
            assert_eq!(
                write_roots(dir, container.map(std::path::Path::new)).unwrap(),
                expected,
                "dir {dir:?}, container {container:?}"
            );
        }
    }

    /// A launch whose write root cannot be made absolute, or whose spawn settings are no JSON
    /// object, is refused with the error naming what could not be composed.
    #[test]
    fn a_launch_refuses_a_write_root_or_settings_it_cannot_compose() {
        assert_eq!(
            write_roots("/wt", Some(std::path::Path::new("")))
                .unwrap_err()
                .0,
            "claude_code driver: resolve the write root : cannot make an empty path absolute"
        );
        let mut o = opts("u/implementer#0");
        o.settings_json = "[]".to_string();
        let refused = build_args(
            &AgentDef::default(),
            &SystemPromptFile::default(),
            &o,
            &fresh_session("sess"),
            "/custom/rigger",
            &["/wt".to_string()],
        )
        .unwrap_err();
        assert_eq!(
            refused.0,
            "driver: compose the spawn's settings: hooks: settings.json is not a JSON object"
        );
    }

    /// A launch's settings carry its own hooks ahead of the session's, on its own settings.
    #[test]
    fn build_args_settings_carry_the_spawn_hooks_ahead_of_the_session_ones() {
        let write_guard = "'/custom/rigger' guard-write --root '/wt' --root '/scratch/s'";
        for (grep_guard, pre_tool_use) in [
            (false, vec![write_guard]),
            (true, vec![write_guard, hooks::GREP_GUARD_COMMAND]),
        ] {
            let mut o = opts("u/implementer#0");
            o.settings_json = "{\"model\":\"opus\"}".to_string();
            o.host = crate::agent::SpawnHost { grep_guard };
            let args = build_args(
                &AgentDef::default(),
                &SystemPromptFile::default(),
                &o,
                &fresh_session("sess"),
                "/custom/rigger",
                &["/wt".to_string(), "/scratch/s".to_string()],
            )
            .unwrap();
            let i = args.iter().position(|x| x == "--settings").unwrap();
            let settings: serde_json::Value = serde_json::from_str(&args[i + 1]).unwrap();
            assert_eq!(settings["model"], "opus", "grep_guard {grep_guard}");
            let commands = |event: &str| -> Vec<&str> {
                settings["hooks"][event]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|entry| entry["hooks"][0]["command"].as_str().unwrap())
                    .collect()
            };
            assert_eq!(
                commands("PreToolUse"),
                pre_tool_use,
                "grep_guard {grep_guard}"
            );
            assert_eq!(
                commands("SessionStart"),
                [hooks::PRIME_COMMAND],
                "grep_guard {grep_guard}"
            );
            let stop_failures = commands("StopFailure");
            assert_eq!(
                stop_failures.len(),
                AgentFailure::CATEGORIES.len(),
                "grep_guard {grep_guard}"
            );
            assert_eq!(
                stop_failures[0],
                "'/custom/rigger' hook stop-failure --spawn 'u/implementer#0' --class rate_limit",
                "grep_guard {grep_guard}"
            );
        }
    }

    #[test]
    fn build_args_mcp_config_names_the_spawn_bound_server() {
        let a = AgentDef::default();
        let o = opts("u7-launch/implementer#2");
        let args = build_args(
            &a,
            &SystemPromptFile::default(),
            &o,
            &fresh_session("sess"),
            "rigger",
            &[],
        )
        .unwrap();
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
        let args = build_args(
            &a,
            &SystemPromptFile::default(),
            &o,
            &fresh_session("sess"),
            "/custom/path/rigger",
            &[],
        )
        .unwrap();
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
        let persona = system_prompt_file(&bare.system_prompt, None).unwrap();
        let args = build_args(&a, &persona, &bare, &fresh_session("sess"), "rigger", &[]).unwrap();
        assert!(!args.iter().any(|x| x == "--system-prompt-file"));
        assert!(!args.iter().any(|x| x == "--model"));
        assert!(!args.iter().any(|x| x == "--fallback-model"));
        // The helpers' MCP tools and this host's bound tools are pre-approved even for an agent
        // declaring no tools.
        let ti = args.iter().position(|x| x == "--allowed-tools").unwrap();
        assert_eq!(
            args[ti + 1],
            format!(
                "{},{}",
                hooks::helper_mcp_tools().unwrap().join(","),
                SPAWN_BOUND_TOOLS.join(",")
            )
        );
        // The always-on flags are still present, the session configuration included.
        assert!(args.iter().any(|x| x == "--settings"));
        assert!(args.iter().any(|x| x == "--agents"));
        assert!(args.iter().any(|x| x == "--strict-mcp-config"));
        assert!(args.iter().any(|x| x == "--session-id"));
    }

    /// A fresh launch session under the fixed `id`, so an argv test compares exact values.
    fn fresh_session(id: &str) -> LaunchSession {
        LaunchSession::new(id.to_string(), None, false)
    }

    // ---- probe(): the hold probe's login read and canary, via the checked-in arms fixture ----

    /// The probe owed to the open hold of `cause` on `model`, bounded by `delay`.
    fn owed(cause: &str, model: &str, delay: Duration) -> crate::hold_probe::OwedProbe {
        crate::hold_probe::OwedProbe {
            held: run::RunHeld {
                cause: cause.to_string(),
                model: model.to_string(),
                ..run::RunHeld::default()
            },
            revision: 7,
            delay,
            due: SystemTime::now(),
        }
    }

    /// One probe through a stand-in `claude`: the open hold's cause and model, the shell of the
    /// login read and of the canary, the probe's delay, and what the probe answers, whether it
    /// read the login, whether it ran the canary, how long it took (from, below) and how many
    /// sleeps the stand-in started.
    struct ProbeRow {
        name: &'static str,
        cause: &'static str,
        model: &'static str,
        auth: String,
        canary: String,
        delay: Duration,
        outcome: ProbeOutcome,
        reads_login: bool,
        runs_canary: bool,
        elapsed: (Duration, Duration),
        sleepers: usize,
    }

    /// A probe through a stand-in `claude`: a credential cause reads the login first, and a read
    /// that finds the session logged out, or answers nothing within the delay, runs no canary;
    /// otherwise the canary runs on the hold's model in the project root, with no tools, no
    /// server, no hooks and no session file, and its first result answers: a result with no
    /// error passes, and an error result, a stream that ends with none or no result within the
    /// delay fails on the last `api_retry` category ahead of the first result, `unknown` with none.
    /// The login read and the canary share the one bound, the probe's delay, a probe writes
    /// no store, and no process of the stand-in outlives the probe.
    #[cfg(unix)]
    #[test]
    fn a_probe_reads_a_credential_holds_login_then_runs_its_canary_within_the_delay() {
        use crate::test_support::{
            api_retry_lines, arms_claude, assert_no_sleeper_left, canary_argv, failing_canary,
            printing, probe_arms, recorded_argv, recorded_task, sleeping, ERROR_RESULT,
            LOGGED_IN_STATUS, LOGGED_OUT_STATUS, PASSING_CANARY,
        };
        const MODEL: &str = "claude-opus-4-1";
        let long = Duration::from_secs(5);
        let short = Duration::from_millis(600);
        let quick = (Duration::ZERO, Duration::from_secs(3));
        let at = |bound: Duration| (bound, bound + Duration::from_millis(1500));
        let logged_in = printing(LOGGED_IN_STATUS);
        let logged_out = printing(LOGGED_OUT_STATUS);
        let failed = |category: &str| ProbeOutcome::Failed(category.to_string());
        let rows = [
            ProbeRow {
                name: "a logged-out credential hold",
                cause: "authentication_failed",
                model: MODEL,
                auth: logged_out.clone(),
                canary: PASSING_CANARY.to_string(),
                delay: long,
                outcome: ProbeOutcome::LoggedOut,
                reads_login: true,
                runs_canary: false,
                elapsed: quick,
                sleepers: 0,
            },
            ProbeRow {
                name: "a login read that answers nothing within the delay",
                cause: "authentication_failed",
                model: MODEL,
                auth: sleeping(5),
                canary: PASSING_CANARY.to_string(),
                delay: short,
                outcome: ProbeOutcome::LoggedOut,
                reads_login: true,
                runs_canary: false,
                elapsed: at(short),
                sleepers: 1,
            },
            ProbeRow {
                name: "a logged-in credential hold whose canary passes",
                cause: "oauth_org_not_allowed",
                model: MODEL,
                auth: logged_in.clone(),
                canary: PASSING_CANARY.to_string(),
                delay: long,
                outcome: ProbeOutcome::Passed,
                reads_login: true,
                runs_canary: true,
                elapsed: quick,
                sleepers: 0,
            },
            ProbeRow {
                name: "a hold no login fixes, on the default model",
                cause: "rate_limit",
                model: "",
                auth: logged_out.clone(),
                canary: PASSING_CANARY.to_string(),
                delay: long,
                outcome: ProbeOutcome::Passed,
                reads_login: false,
                runs_canary: true,
                elapsed: quick,
                sleepers: 0,
            },
            ProbeRow {
                name: "a cloud credential hold, whose canary runs alone",
                cause: "cloud_credential_error",
                model: MODEL,
                auth: logged_out.clone(),
                canary: PASSING_CANARY.to_string(),
                delay: long,
                outcome: ProbeOutcome::Passed,
                reads_login: false,
                runs_canary: true,
                elapsed: quick,
                sleepers: 0,
            },
            ProbeRow {
                name: "an error result, on its stream's last category",
                cause: "rate_limit",
                model: MODEL,
                auth: logged_in.clone(),
                canary: failing_canary(&["billing_error", "server_error"]),
                delay: long,
                outcome: failed("server_error"),
                reads_login: false,
                runs_canary: true,
                elapsed: quick,
                sleepers: 0,
            },
            ProbeRow {
                name: "an error result on a stream with no api_retry line",
                cause: "rate_limit",
                model: MODEL,
                auth: logged_in.clone(),
                canary: failing_canary(&[]),
                delay: long,
                outcome: failed("unknown"),
                reads_login: false,
                runs_canary: true,
                elapsed: quick,
                sleepers: 0,
            },
            ProbeRow {
                name: "a passing result after the first, an error result, answers nothing",
                cause: "rate_limit",
                model: MODEL,
                auth: logged_in.clone(),
                canary: format!(
                    "{}{ERROR_RESULT}{PASSING_CANARY}",
                    api_retry_lines(&["overloaded"], 1)
                ),
                delay: long,
                outcome: failed("overloaded"),
                reads_login: false,
                runs_canary: true,
                elapsed: quick,
                sleepers: 0,
            },
            ProbeRow {
                name: "an api_retry line after the first result names no category",
                cause: "rate_limit",
                model: MODEL,
                auth: logged_in.clone(),
                canary: format!(
                    "{}{ERROR_RESULT}{}{PASSING_CANARY}    exit 1\n",
                    api_retry_lines(&["overloaded"], 1),
                    api_retry_lines(&["rate_limit"], 1)
                ),
                delay: long,
                outcome: failed("overloaded"),
                reads_login: false,
                runs_canary: true,
                elapsed: quick,
                sleepers: 0,
            },
            ProbeRow {
                name: "a stream that ends with no result",
                cause: "rate_limit",
                model: MODEL,
                auth: logged_in.clone(),
                canary: format!("{}    exit 1\n", api_retry_lines(&["overloaded"], 1)),
                delay: long,
                outcome: failed("overloaded"),
                reads_login: false,
                runs_canary: true,
                elapsed: quick,
                sleepers: 0,
            },
            ProbeRow {
                name: "a canary that answers nothing within the delay",
                cause: "rate_limit",
                model: MODEL,
                auth: logged_in.clone(),
                canary: format!("{}{}", api_retry_lines(&["overloaded"], 1), sleeping(5)),
                delay: short,
                outcome: failed("overloaded"),
                reads_login: false,
                runs_canary: true,
                elapsed: at(short),
                sleepers: 1,
            },
            ProbeRow {
                name: "a slow login read, which the canary's bound counts",
                cause: "authentication_failed",
                model: MODEL,
                auth: format!("{logged_in}{}", sleeping(2)),
                canary: sleeping(5),
                delay: Duration::from_millis(2500),
                outcome: failed("unknown"),
                reads_login: true,
                runs_canary: true,
                elapsed: at(Duration::from_millis(2500)),
                sleepers: 2,
            },
        ];
        for row in rows {
            let name = row.name;
            let fake = tempfile::tempdir().unwrap();
            let root = tempfile::tempdir().unwrap();
            let (progress, run_store, hold_sink) = (
                Store::open(":memory:").unwrap(),
                Store::open(":memory:").unwrap(),
                Store::open(":memory:").unwrap(),
            );
            let driver = Driver {
                bin: arms_claude(fake.path(), &probe_arms(&row.auth, &row.canary)),
                progress_store: &progress,
                run_store: &run_store,
                beat_source: Some(&progress),
                hold_sink: Some(&hold_sink),
                ..Driver::default()
            };
            let started = std::time::Instant::now();
            let outcome = driver.probe(
                root.path().to_str().unwrap(),
                &owed(row.cause, row.model, row.delay),
            );
            let elapsed = started.elapsed();
            assert_eq!(outcome, row.outcome, "{name}");
            let (from, below) = row.elapsed;
            assert!(
                from <= elapsed && elapsed < below,
                "{name}: the probe answered after {elapsed:?}, outside {from:?}..{below:?}"
            );
            assert_no_sleeper_left(fake.path(), row.sleepers, name);
            let auth_argv = fake.path().join("auth-argv");
            if row.reads_login {
                assert_eq!(
                    recorded_argv(&auth_argv),
                    ["auth", "status", "--json"],
                    "{name}: the login read"
                );
            } else {
                assert!(!auth_argv.exists(), "{name}: the probe read no login");
            }
            let canary = fake.path().join("canary-argv");
            if row.runs_canary {
                assert_eq!(recorded_argv(&canary), canary_argv(row.model), "{name}");
                assert_eq!(
                    std::fs::read_to_string(fake.path().join("canary-cwd")).unwrap(),
                    format!(
                        "{}\n",
                        std::fs::canonicalize(root.path()).unwrap().display()
                    ),
                    "{name}: the canary runs in the project root"
                );
                assert_eq!(
                    recorded_task(&fake.path().join("canary-first")),
                    CANARY_TASK,
                    "{name}: the canary's task"
                );
            } else {
                assert!(!canary.exists(), "{name}: the probe ran no canary");
            }
            for (store, what) in [
                (&progress, "progress"),
                (&run_store, "run"),
                (&hold_sink, "hold"),
            ] {
                assert_eq!(store.log_head().unwrap(), None, "{name}: the {what} store");
            }
        }
        let driver = Driver {
            bin: "/definitely/does/not/exist/claude-xyz".to_string(),
            ..Driver::default()
        };
        let started = std::time::Instant::now();
        assert_eq!(
            driver.probe(".", &owed("rate_limit", MODEL, long)),
            failed("unknown"),
            "a canary that does not start fails on no category"
        );
        assert!(started.elapsed() < quick.1, "and at once");
    }

    // ---- launch(): real subprocess via the checked-in fixture ----

    fn fixture_bin() -> String {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/claude-code-echo-agent.sh")
            .to_string_lossy()
            .into_owned()
    }

    /// Write `launch`'s first message as [`Driver::read_stream`] does, its input closing with the
    /// write, then reap the fixture and read what it printed.
    fn read_fixture_lines(launch: &mut Launch) -> Vec<String> {
        let (input_open, close) = std::sync::mpsc::channel();
        drop(input_open);
        let stdin = launch.child.stdin.take().expect("piped stdin");
        write_first_message(stdin, &launch.message, close).expect("the fixture reads its input");
        let child = &mut launch.child;
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
        // attempt, not after a successful one. The record names the directory the launch was
        // to run in (spec 105): the next supervisor start reaps that directory as it closes the
        // record this failed spawn left open.
        let driver = Driver {
            bin: "/definitely/does/not/exist/claude-xyz".to_string(),
            rigger_bin: "rigger".to_string(),
            ..Driver::default()
        };
        let store = Store::open(":memory:").unwrap();
        let mut o = opts("u104-launch/implementer#0");
        o.run_id = "run-1".to_string();
        o.dir = "/definitely/does/not/exist/scratch/u104-launch".to_string();

        let err = match driver.launch(&AgentDef::default(), "do the thing", &o, &store) {
            Ok(_) => panic!("spawning a nonexistent binary must fail"),
            Err(e) => e,
        };
        assert!(err.0.contains("u104-launch/implementer#0"), "{}", err.0);

        let recorded = crate::progress::read_run(&store, "run-1").unwrap();
        assert_eq!(
            recorded.len(),
            1,
            "the launch record lands even though the process never started"
        );
        assert_eq!(recorded[0].type_, crate::progress::TYPE_SPAWN_LAUNCHED);
        let sl: SpawnLaunched = serde_json::from_slice(&recorded[0].data).unwrap();
        assert_eq!(sl.spawn, "u104-launch/implementer#0");
        assert!(!sl.session_id.is_empty(), "a session id was still minted");
        assert_eq!(
            (sl.dir.as_str(), sl.ended),
            ("/definitely/does/not/exist/scratch/u104-launch", None),
            "the open record names the directory the launch was to run in"
        );
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

        let lines = read_fixture_lines(&mut launch);
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
        let lines = read_fixture_lines(&mut launch);
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
    fn launch_applies_the_harness_env_and_lets_opts_env_override_it() {
        // Every headless worker gets the harness environment (`harness_env`): the print-mode
        // idle ceiling off, so a session that ended its turn waiting on a background helper
        // stays open for the helper's result. `opts.env` is applied after it, so an
        // operator's own value under the same key wins.
        let driver = Driver {
            bin: fixture_bin(),
            rigger_bin: "rigger".to_string(),
            ..Driver::default()
        };
        let store = Store::open(":memory:").unwrap();
        let ceiling = |env: Vec<(String, String)>| {
            let mut o = opts("u/implementer#0");
            o.env = env;
            let mut launch = driver
                .launch(&AgentDef::default(), "task", &o, &store)
                .unwrap();
            read_fixture_lines(&mut launch)
                .into_iter()
                .find(|l| l.starts_with("CLAUDE_CODE_PRINT_BG_WAIT_CEILING_MS="))
                .expect("the fixture echoes the ceiling")
        };
        assert_eq!(
            ceiling(Vec::new()),
            "CLAUDE_CODE_PRINT_BG_WAIT_CEILING_MS=0"
        );
        assert_eq!(
            ceiling(vec![(
                "CLAUDE_CODE_PRINT_BG_WAIT_CEILING_MS".to_string(),
                "900000".to_string()
            )]),
            "CLAUDE_CODE_PRINT_BG_WAIT_CEILING_MS=900000",
            "an opts.env value under the same key wins"
        );
    }

    /// The worker is told the spawn it runs as (`RIGGER_SPAWN`), so its own `rigger emit` is
    /// stamped with its author exactly as a `rigger_emit` call through its spawn-bound MCP
    /// server is.
    #[test]
    fn launch_exports_the_spawn_id_to_the_worker() {
        let driver = Driver {
            bin: fixture_bin(),
            rigger_bin: "rigger".to_string(),
            ..Driver::default()
        };
        let store = Store::open(":memory:").unwrap();
        let mut launch = driver
            .launch(
                &AgentDef::default(),
                "task",
                &opts("u/implementer#2"),
                &store,
            )
            .unwrap();
        let lines = read_fixture_lines(&mut launch);
        assert!(
            lines.iter().any(|l| l == "RIGGER_SPAWN=u/implementer#2"),
            "the worker sees its spawn id: {lines:?}"
        );
    }

    #[test]
    fn launch_frames_the_task_as_the_first_stream_json_user_message() {
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
        let lines = read_fixture_lines(&mut launch);
        let stdin_line = lines
            .iter()
            .find(|l| l.starts_with("STDIN="))
            .expect("fixture echoed the first stdin line");
        let json_text = stdin_line.trim_start_matches("STDIN=");
        let v: serde_json::Value = serde_json::from_str(json_text).unwrap();
        assert_eq!(v["type"], "user");
        assert_eq!(v["message"]["role"], "user");
        assert_eq!(v["message"]["content"], framed("implement the thing"));
    }

    #[test]
    fn launch_leaves_stdin_open_for_the_reader_to_close() {
        // THE STREAM (criterion 2) writes the first message and closes input after the first
        // `result`, not this criterion - so `launch` must hand back a child whose stdin handle
        // is still present (not dropped/closed).
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
            "stdin must stay open for the reader"
        );
        // Reap: `wait()` closes the input first, so the fixture reads its end and exits on its
        // own - this never blocks, matching every sibling test in this file
        // (`read_fixture_lines`'s own `.wait()`), never leaving a zombie behind.
        launch.child.wait().unwrap();
    }

    #[test]
    fn a_persona_past_the_per_argument_limit_reaches_the_session_through_its_file() {
        // Linux fails `execve` with `E2BIG` for any one argv string past `MAX_ARG_STRLEN`
        // (32 pages of 4096 bytes), so a full persona travels as the file
        // `--system-prompt-file` names, under the spawn's scratch.
        let scratch = tempfile::tempdir().unwrap();
        let capture = tempfile::tempdir().unwrap();
        let driver = Driver {
            bin: std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../tests/fixtures/prompt-capture-agent.sh")
                .to_string_lossy()
                .into_owned(),
            scratch_root: scratch.path().to_string_lossy().into_owned(),
            ..Driver::default()
        };
        let store = Store::open(":memory:").unwrap();
        let mut o = opts("u/adversary#0");
        o.system_prompt = "p".repeat(131_072 + 1);
        o.env = vec![(
            PROMPT_CAPTURE_DIR.to_string(),
            capture.path().to_string_lossy().into_owned(),
        )];

        let mut launch = driver
            .launch(&AgentDef::default(), "task", &o, &store)
            .unwrap();
        drop(launch.child.stdin.take());
        launch.child.wait().unwrap();

        assert_eq!(
            captured_system_prompt(capture.path()),
            o.system_prompt,
            "the whole persona arrived through the system-prompt file"
        );
        let path = std::fs::read_to_string(capture.path().join("system-prompt-path")).unwrap();
        assert!(
            std::path::Path::new(&path).starts_with(scratch.path()),
            "the file lives under the spawn's scratch, which the spawn's terminus reclaims: {path}"
        );
    }

    #[test]
    fn a_launch_asked_to_resume_continues_the_recorded_session_with_the_resume_task() {
        // Both session hosts share the one session authority (`LaunchSession`): a launch whose
        // spawn names a session to continue resumes it - `--resume`, never a second minted
        // `--session-id` - records that it did, and sends only the resume task, since the
        // session already holds the rest.
        let driver = Driver {
            bin: fixture_bin(),
            rigger_bin: "rigger".to_string(),
            ..Driver::default()
        };
        let store = Store::open(":memory:").unwrap();
        let mut o = opts("u/implementer#1");
        o.resumed_from = "sess-prior".to_string();
        o.resume_task = "fix exactly these".to_string();

        let mut launch = driver
            .launch(&AgentDef::default(), "the full task", &o, &store)
            .unwrap();
        let lines = read_fixture_lines(&mut launch);

        assert_eq!(launch.session_id, "sess-prior");
        let flag = |name: &str| {
            launch
                .args
                .iter()
                .position(|x| x == name)
                .map(|i| launch.args[i + 1].clone())
        };
        assert_eq!(flag("--resume").as_deref(), Some("sess-prior"));
        assert_eq!(flag("--session-id"), None, "{:?}", launch.args);
        let recorded = crate::progress::read_run(&store, &o.run_id).unwrap();
        let sl: SpawnLaunched = serde_json::from_slice(&recorded[0].data).unwrap();
        assert_eq!(sl.session_id, "sess-prior");
        assert_eq!(sl.resumed_from.as_deref(), Some("sess-prior"));
        let stdin_line = lines
            .iter()
            .find(|l| l.starts_with("STDIN="))
            .expect("fixture echoed the first stdin line");
        let v: serde_json::Value =
            serde_json::from_str(stdin_line.trim_start_matches("STDIN=")).unwrap();
        assert_eq!(v["message"]["content"], framed("fix exactly these"));
    }

    /// THE RELAUNCH FORKS (spec 105, RESUME BY SESSION): the relaunch of an interrupted launch
    /// resumes its session as a fork under a session id the host mints, so an orphan of that
    /// launch keeps its transcript to itself, and the record carries the minted id with
    /// `resumed_from` naming the interrupted session.
    #[test]
    fn a_relaunch_forks_the_interrupted_session_under_a_minted_id() {
        let store = Store::open(":memory:").unwrap();
        let driver = Driver {
            bin: fixture_bin(),
            rigger_bin: "rigger".to_string(),
            progress_store: &store,
            ..Driver::default()
        };
        let o = opts("u/implementer#1");
        let interrupted = "sess-interrupted";
        for record in [
            SpawnLaunched {
                spawn: o.id.clone(),
                session_id: interrupted.to_string(),
                ..Default::default()
            },
            SpawnLaunched::closed(&o.id, 0, interrupted, progress::ENDED_INTERRUPTED, ""),
        ] {
            progress_store::record_launch(&store, &o.run_id, &record).unwrap();
        }

        let first = driver.first_launch("the full task", &o).unwrap();
        let mut launch = driver
            .launch_as(
                first.session,
                &AgentDef::default(),
                &first.task,
                &o,
                &store,
                Some(first.ordinal),
            )
            .unwrap();
        read_fixture_lines(&mut launch);

        let minted = launch.session_id.clone();
        assert_ne!(
            minted, interrupted,
            "the fork runs under a session of its own"
        );
        let at = launch.args.iter().position(|x| x == "--resume");
        assert_eq!(
            at.and_then(|at| launch.args.get(at..at + 5)),
            Some(
                &[
                    "--resume".to_string(),
                    interrupted.to_string(),
                    "--fork-session".to_string(),
                    "--session-id".to_string(),
                    minted.clone(),
                ][..]
            ),
            "{:?}",
            launch.args
        );
        let recorded = crate::progress::read_run(&store, &o.run_id).unwrap();
        let sl: SpawnLaunched = serde_json::from_slice(&recorded[2].data).unwrap();
        assert_eq!(
            (sl.launch, sl.session_id, sl.resumed_from),
            (1, minted, Some(interrupted.to_string()))
        );
        assert_eq!(launch.resumed_from, interrupted);
    }

    /// The ordinal comes from the record, so a launch that cannot read the record cannot name
    /// its own: it fails with the store's error and starts nothing, never a launch that might
    /// truncate a prior transcript.
    #[test]
    fn a_launch_that_cannot_read_the_record_starts_nothing() {
        let driver = Driver {
            bin: fixture_bin(),
            rigger_bin: "rigger".to_string(),
            ..Driver::default()
        };

        let err = match driver.launch(
            &AgentDef::default(),
            "task",
            &opts("u/implementer#0"),
            &FailingReadStore,
        ) {
            Ok(mut launch) => {
                launch.child.wait().unwrap();
                panic!("an unread record must not reach a launch");
            }
            Err(e) => e,
        };

        assert_eq!(err.0, FailingReadStore::read_failure().to_string());
    }

    /// A STORE FAULT (spec 105): a store read failing before the spawn's first launch, of the
    /// run's log or of the spawn's launch records, launches nothing and returns the read's error
    /// marked a store fault, keeping the spawn's scratch, so a restart relaunches it.
    #[test]
    fn a_store_read_failing_before_the_launch_is_a_store_fault_keeping_the_scratch() {
        let readable = Store::open(":memory:").unwrap();
        let rows: [(&str, &dyn EventStore, &dyn EventStore); 2] = [
            ("the run's log", &FailingReadStore, &readable),
            ("the launch records", &readable, &FailingReadStore),
        ];
        for (name, run_store, progress_store) in rows {
            let scratch_root = tempfile::tempdir().unwrap();
            let root = scratch_root.path().to_string_lossy().into_owned();
            let o = opts("u/implementer#0");
            let leaf = spawn_scratch_path(&root, &o.run_id, &o.id).unwrap();
            std::fs::create_dir_all(&leaf).unwrap();
            std::fs::write(leaf.join("build.log"), "built").unwrap();
            let driver = Driver {
                run_store,
                progress_store,
                scratch_root: root,
                ..Driver::default()
            };

            let err = driver
                .spawn(&AgentDef::default(), "task", &o, &no_emit)
                .map(|result| result.output)
                .unwrap_err();

            assert_eq!(
                (err.0, leaf.join("build.log").exists()),
                (
                    store_fault(Error(FailingReadStore::read_failure().to_string())).0,
                    true
                ),
                "{name}"
            );
        }
    }

    #[test]
    fn a_launch_relaunches_at_a_new_ordinal_with_no_resumed_from_this_spec() {
        // CONSTRAINTS WALK: "Relaunch - a new session id and the next launch ordinal."
        // A launch whose spawn names no session to continue is fresh, so two successive
        // launches for the same spawn get two DIFFERENT session ids, and the record, not the
        // caller, moves the ordinal.
        let driver = Driver {
            bin: fixture_bin(),
            rigger_bin: "rigger".to_string(),
            ..Driver::default()
        };
        let store = Store::open(":memory:").unwrap();
        let o = opts("u/implementer#0");
        let mut first = driver
            .launch(&AgentDef::default(), "t", &o, &store)
            .unwrap();
        first.child.wait().unwrap();

        let mut second = driver
            .launch(&AgentDef::default(), "t", &o, &store)
            .unwrap();
        second.child.wait().unwrap();

        assert_ne!(first.session_id, second.session_id);
        let recorded = crate::progress::read_run(&store, &o.run_id).unwrap();
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

    /// A spawn's transcripts are the files [`stream_path`] names for it, one per launch, and they
    /// go together: another spawn's (attempt 10's, whose sanitized id extends attempt 1's) and any
    /// other file beside them stay.
    #[test]
    fn remove_transcripts_removes_each_launch_of_its_spawn_and_nothing_beside_it() {
        let scratch = tempfile::tempdir().unwrap();
        let root = scratch.path().to_str().unwrap();
        let transcript =
            |spawn_id: &str, launch: u32| stream_path(root, "run-1", spawn_id, launch).unwrap();
        let removed = [
            transcript("u1/adversary#1", 0),
            transcript("u1/adversary#1", 1),
        ];
        let dir = removed[0].parent().unwrap().to_path_buf();
        let kept = [
            transcript("u1/adversary#10", 0),
            dir.join("u1_2fadversary_231.01.jsonl"),
            dir.join("u1_2fadversary_231.notes"),
        ];
        std::fs::create_dir_all(&dir).unwrap();
        for file in removed.iter().chain(&kept) {
            std::fs::write(file, "{}").unwrap();
        }
        let left = || {
            let mut files: Vec<std::path::PathBuf> = std::fs::read_dir(&dir)
                .unwrap()
                .map(|entry| entry.unwrap().path())
                .collect();
            files.sort();
            files
        };
        let mut kept = kept.to_vec();
        kept.sort();
        remove_transcripts(root, "run-1", "u1/adversary#1");
        assert_eq!(left(), kept, "only attempt 1's launches go");
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

    /// `line`'s announced delay and category are `expected`.
    fn assert_api_retry_fields(line: &str, expected: (u64, &str)) {
        let v: Value = serde_json::from_str(line).unwrap();
        assert_eq!(api_retry_fields(&v), expected);
    }

    crate::test_cases! {
        // The exact shape a probe of the real CLI showed (spec 104 Problem section).
        api_retry_fields_reads_the_delay_and_the_category_of_the_real_line:
            assert_api_retry_fields(
                r#"{"type":"system","subtype":"api_retry","attempt":1,"max_retries":10,
                    "retry_delay_ms":618,"error_status":401,"error":"authentication_failed"}"#,
                (618, "authentication_failed"),
            );
        api_retry_fields_floors_a_missing_delay_alone:
            assert_api_retry_fields(r#"{"error":"overloaded"}"#, (0, "overloaded"));
        api_retry_fields_floors_a_missing_category_alone:
            assert_api_retry_fields(r#"{"retry_delay_ms":618}"#, (618, "unknown"));
        api_retry_fields_floors_a_field_of_the_wrong_type:
            assert_api_retry_fields(r#"{"retry_delay_ms":"soon","error":429}"#, (0, "unknown"));
        // The no-response shape: no headers came inside the first-byte window, so the line
        // announces the retry's header wait under `no_response` beside a zero delay.
        api_retry_fields_reads_the_header_wait_of_a_no_response_retry:
            assert_api_retry_fields(
                r#"{"type":"system","subtype":"api_retry","attempt":1,"max_retries":1,
                    "retry_delay_ms":0,"error_status":null,"error":"unknown",
                    "no_response":{"waited_ms":90000,"retry_wait_ms":180000}}"#,
                (180000, "unknown"),
            );
    }

    /// A clean spawn of the checked-in `script` leaves exactly one [`progress::ApiRetry`] per
    /// `(delay_ms, category)` of `announced`, in stream order, on the run's progress stream.
    fn assert_spawn_records_api_retries(script: &str, announced: &[(u64, &str)]) {
        let progress_store = Store::open(":memory:").unwrap();
        let run_store = Store::open(":memory:").unwrap();
        let driver = Driver {
            bin: fixture_script(script),
            progress_store: &progress_store,
            run_store: &run_store,
            ..Driver::default()
        };
        let mut o = opts("u105/implementer#0");
        o.run_id = "run-1".to_string();

        driver
            .spawn(&AgentDef::default(), "do the thing", &o, &no_emit)
            .unwrap();

        let recorded = crate::test_support::payloads_of_type(
            &progress::read_run(&progress_store, &o.run_id).unwrap(),
            progress::TYPE_API_RETRY,
        );
        let expected: Vec<Value> = announced
            .iter()
            .map(|&(delay_ms, category)| {
                serde_json::json!({"spawn": o.id, "delay_ms": delay_ms, "category": category})
            })
            .collect();
        assert_eq!(recorded, expected);
    }

    crate::test_cases! {
        spawn_appends_one_api_retry_per_line_in_stream_order: assert_spawn_records_api_retries(
            "claude-code-two-api-retries-agent.sh",
            &[(618, "rate_limit"), (30_000, "overloaded")],
        );
        spawn_appends_no_api_retry_for_a_stream_without_one: assert_spawn_records_api_retries(
            "claude-code-mcp-server-connected-agent.sh",
            &[],
        );
    }

    /// RESUME BY SESSION, the host's half (spec 105): a spawn the current run has ended is
    /// answered from its result with no launch, carrying the recorded session and resolved model
    /// and no `resumed_from` whatever resume its opts asked for, and an ended error answers as
    /// that error. A spawn whose only result is a liveness fault, or one a prior run alone ended,
    /// is launched, and its real result supersedes the fault. Each spawn is parked in the current
    /// run before its first launch, stamped with its run and the host's scratch root, unless its
    /// request already stands there, so a launch that cannot start leaves its request.
    #[test]
    fn the_host_answers_an_ended_spawn_from_the_log_and_parks_every_other_before_its_launch() {
        const ID: &str = "u105-park/implementer#0";
        /// The answer's session when it is the one the spawn's launch ran as, which the host
        /// mints.
        const LAUNCHED: &str = "<the launch's session>";
        /// The marker root of a request the host parked: its own scratch root.
        const SCRATCH: &str = "<the host's scratch root>";
        type Answer = Result<(String, String, String, String), String>;
        let answer = |output: &str, model: &str, session: &str| -> Answer {
            Ok((output.into(), model.into(), session.into(), String::new()))
        };
        let launched = || {
            answer(
                "done: the answer is 42",
                "claude-sonnet-4-5-20250929",
                LAUNCHED,
            )
        };
        let row_of = |output: &str, error: &str, fault: bool| {
            (ID.to_string(), output.to_string(), error.to_string(), fault)
        };
        let real = || row_of("done: the answer is 42", "", false);
        let fault = || SpawnResult::liveness_fault(ID, "silent past its bound", "stalled");
        let stream = || fixture_script("claude-code-stream-agent.sh");
        struct Row {
            name: &'static str,
            bin: String,
            resumed_from: &'static str,
            /// Recorded under a run the spawn's run superseded, each beside its request.
            prior_run: Vec<SpawnResult>,
            /// Whether the spawn's request stands in its run before the call.
            requested: bool,
            /// Recorded in the spawn's run before the call.
            standing: Vec<SpawnResult>,
            answer: Answer,
            /// Each launch record of the spawn's run: its ordinal and how it ended.
            launches: Vec<(u32, Option<&'static str>)>,
            /// Every request on the stream: its run and marker root.
            requests: Vec<(&'static str, &'static str)>,
            /// Every result on the stream ([`spawn_result_rows`]).
            results: Vec<(String, String, String, bool)>,
        }
        let rows = [
            Row {
                name: "a real result of the run answers with no launch",
                bin: stream(),
                resumed_from: "sess-asked",
                prior_run: vec![],
                requested: true,
                standing: vec![spawn_result_from(
                    &serde_json::json!({"type": "result", "result": "from the log",
                        "session_id": "sess-logged"}),
                    &opts(ID),
                    "claude-logged",
                )],
                answer: answer("from the log", "claude-logged", "sess-logged"),
                launches: vec![],
                requests: vec![("run-1", "")],
                results: vec![row_of("from the log", "", false)],
            },
            Row {
                name: "an error result of the run answers as that error with no launch",
                bin: stream(),
                resumed_from: "",
                prior_run: vec![],
                requested: true,
                standing: vec![SpawnResult::failed(ID, "the session stopped")],
                answer: Err("the session stopped".into()),
                launches: vec![],
                requests: vec![("run-1", "")],
                results: vec![row_of("", "the session stopped", false)],
            },
            Row {
                name: "a liveness fault alone launches and the real result supersedes it",
                bin: stream(),
                resumed_from: "",
                prior_run: vec![],
                requested: true,
                standing: vec![fault()],
                answer: launched(),
                launches: vec![(0, None), (0, Some(progress::ENDED_COMPLETED))],
                requests: vec![("run-1", "")],
                results: vec![row_of("", "silent past its bound", true), real()],
            },
            Row {
                name: "a result of a prior run alone parks again and launches",
                bin: stream(),
                resumed_from: "",
                prior_run: vec![SpawnResult::ok(ID, "from a prior run")],
                requested: false,
                standing: vec![],
                answer: launched(),
                launches: vec![(0, None), (0, Some(progress::ENDED_COMPLETED))],
                requests: vec![("run-0", ""), ("run-1", SCRATCH)],
                results: vec![row_of("from a prior run", "", false), real()],
            },
            Row {
                name: "an unrequested spawn parks, then launches",
                bin: stream(),
                resumed_from: "",
                prior_run: vec![],
                requested: false,
                standing: vec![],
                answer: launched(),
                launches: vec![(0, None), (0, Some(progress::ENDED_COMPLETED))],
                requests: vec![("run-1", SCRATCH)],
                results: vec![real()],
            },
            Row {
                name: "a launch that cannot start leaves the request it parked",
                bin: "/definitely/does/not/exist/claude-xyz".into(),
                resumed_from: "",
                prior_run: vec![],
                requested: false,
                standing: vec![],
                answer: Err(format!(
                    "claude_code driver: launch {ID:?}: No such file or directory (os error 2)"
                )),
                launches: vec![(0, None)],
                requests: vec![("run-1", SCRATCH)],
                results: vec![],
            },
        ];
        for row in rows {
            let name = row.name;
            let (progress_store, run_store) = (
                Store::open(":memory:").unwrap(),
                Store::open(":memory:").unwrap(),
            );
            let scratch = tempfile::tempdir().unwrap();
            let scratch_root = scratch.path().to_string_lossy().into_owned();
            let mut o = opts(ID);
            o.run_id = "run-1".to_string();
            o.resumed_from = row.resumed_from.to_string();
            let request = crate::agent::spawn_request(&AgentDef::default(), "do the thing", &o);
            run_store::ensure_started(&run_store, &["prior".to_string()]).unwrap();
            for res in &row.prior_run {
                spawn_store::park_in_run(&run_store, &request, "run-0", "").unwrap();
                spawn_store::record_result(&run_store, res).unwrap();
            }
            run_store::ensure_started(&run_store, &["current".to_string()]).unwrap();
            if row.requested {
                spawn_store::park_in_run(&run_store, &request, "run-1", "").unwrap();
            }
            for res in &row.standing {
                spawn_store::record_result(&run_store, res).unwrap();
            }
            let driver = Driver {
                bin: row.bin,
                rigger_bin: "rigger".to_string(),
                progress_store: &progress_store,
                run_store: &run_store,
                scratch_root: scratch_root.clone(),
                ..Driver::default()
            };

            let spawned = driver.spawn(&AgentDef::default(), "do the thing", &o, &no_emit);

            let launches: Vec<SpawnLaunched> = progress::read_run(&progress_store, "run-1")
                .unwrap()
                .iter()
                .filter(|e| e.type_ == progress::TYPE_SPAWN_LAUNCHED)
                .map(|e| serde_json::from_slice(&e.data).unwrap())
                .collect();
            let launch_session = launches.first().map(|l| l.session_id.clone());
            let got: Answer = spawned
                .map(|r| {
                    let session = if Some(&r.session_id) == launch_session.as_ref() {
                        LAUNCHED.to_string()
                    } else {
                        r.session_id
                    };
                    (r.output, r.resolved_model, session, r.resumed_from)
                })
                .map_err(|e| e.0);
            assert_eq!(got, row.answer, "{name}: the answer");
            assert_eq!(
                launches
                    .iter()
                    .map(|l| (l.launch, l.ended.as_deref()))
                    .collect::<Vec<_>>(),
                row.launches,
                "{name}: the launches"
            );
            let events = run_store
                .read_stream(STREAM, 0, Direction::Forward)
                .unwrap();
            let requests: Vec<(String, String)> = events
                .iter()
                .filter(|e| e.type_ == crate::spawn::TYPE_SPAWN_REQUESTED)
                .map(|e| {
                    let meta = |key: &str| e.meta.get(key).cloned().unwrap_or_default();
                    let root = meta(crate::spawn::META_MARKER_ROOT);
                    let root = if root == scratch_root {
                        SCRATCH.into()
                    } else {
                        root
                    };
                    (meta(crate::run::META_RUN_ID), root)
                })
                .collect();
            assert_eq!(
                requests,
                row.requests
                    .iter()
                    .map(|&(run, root)| (run.to_string(), root.to_string()))
                    .collect::<Vec<_>>(),
                "{name}: the requests"
            );
            assert_eq!(
                spawn_result_rows(&events),
                row.results,
                "{name}: the results"
            );
        }
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
                "permission_denials":[
                    {"tool_name":"Bash","tool_use_id":"toolu_1","tool_input":{"command":"ls"}},
                    {"tool_name":"Write","tool_use_id":"toolu_2","tool_input":{"file_path":"/x"}}]}"#,
        )
        .unwrap();
        let o = opts("u1/implementer#0");
        let res = spawn_result_from(&v, &o, "claude-sonnet-4-5-20250929");

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
        // The result message's own `permission_denials` array is the count's one source.
        assert_eq!(res.meta["permission_denials"], 2);
    }

    #[test]
    fn spawn_result_from_degrades_gracefully_on_missing_fields() {
        let v: Value = serde_json::from_str(r#"{"type":"result","result":"ok"}"#).unwrap();
        let o = opts("u/implementer#0");
        let res = spawn_result_from(&v, &o, "");
        assert_eq!(res.output, "ok");
        assert_eq!(res.meta["permission_denials"], 0);
        assert_eq!(res.meta["usage"]["input"], 0);
        assert_eq!(res.meta["turns"], 0);
        assert_eq!(res.meta["cost_usd"], 0.0);
    }

    #[test]
    fn rigger_server_fault_reads_only_the_rigger_servers_init_status() {
        let fault = |init: &str| rigger_server_fault(&serde_json::from_str(init).unwrap());
        assert_eq!(
            fault(r#"{"mcp_servers":[{"name":"rigger","status":"failed"}]}"#).as_deref(),
            Some("failed")
        );
        assert_eq!(
            fault(r#"{"mcp_servers":[{"name":"rigger"}]}"#).as_deref(),
            Some(""),
            "a rigger entry with no status did not connect either"
        );
        assert_eq!(
            fault(
                r#"{"mcp_servers":[{"name":"other","status":"failed"},{"name":"rigger","status":"connected"}]}"#
            ),
            None
        );
        assert_eq!(
            fault(r#"{"mcp_servers":[{"name":"other","status":"failed"}]}"#),
            None
        );
        assert_eq!(fault(r#"{"model":"m"}"#), None);
    }
}
