//! The agent-host interface (workspace split): the `AgentDriver` trait the conductor spawns every
//! agent through, its per-spawn options, result and error, and the park and failure-class
//! sentinels that cross it. It names only domain types; the adapters live in `rigger-driver`,
//! and the root `rigger` crate re-exports every item under its historical `rigger::conductor`
//! path.

use serde_json::Value;

use crate::config::AgentDef;
use crate::spawn::SpawnRequest;

/// The event a planning stage's agent emits to add a unit to the run DAG at
/// runtime (the living-DAG / spawnUnit mechanic).
pub const TYPE_UNIT_PROPOSED: &str = "UnitProposed";

/// The name a headless session sees its live-progress tool under: the spawn-bound `rigger`
/// server's `rigger_progress`.
pub const SPAWN_PROGRESS_TOOL: &str = "mcp__rigger__rigger_progress";

/// The recording tools of a spawn's own bound `rigger` server, as a headless session names them:
/// the tools it records its decisions, its progress and its scratch through.
pub const SPAWN_BOUND_TOOLS: [&str; 3] = [
    "mcp__rigger__rigger_emit",
    SPAWN_PROGRESS_TOOL,
    "mcp__rigger__rigger_scratch",
];

#[derive(Debug, thiserror::Error)]
#[error("conductor: {0}")]
pub struct Error(pub String);

/// Each listed error converts into a conductor [`Error`] carrying its display text. The worktree
/// errors are not listed: a worktree result reaches a conductor error only through the
/// conductor's one git step, which marks a conductor fault as a halt and a refusal the unit's own
/// work caused as the stage's failure.
macro_rules! error_from_display {
    ($($source:ty),+) => {
        $(
            impl From<$source> for Error {
                fn from(e: $source) -> Self {
                    Error(e.to_string())
                }
            }
        )+
    };
}
error_from_display!(crate::eventstore::Error, serde_json::Error);

/// What an agent returns when it finishes.
#[derive(Clone, Debug, Default)]
pub struct AgentResult {
    pub output: String,
    /// The RESOLVED model id that actually ran this spawn (spec 05 line 52), or empty when
    /// unknown. The replay driver surfaces it from the recorded result's metadata
    /// ([`SpawnResult::meta_str`](crate::spawn::SpawnResult::meta_str));
    /// the conductor copies it onto the spawn's unit events via [`META_MODEL_RESOLVED`].
    /// The headless host reads it off the session's stream; the cli driver does not learn it
    /// and leaves it empty.
    pub resolved_model: String,
    /// The Claude Code session this spawn ran as - the id a later attempt or review round of
    /// the same role on the same unit continues with `--resume` ([`SpawnOpts::resumed_from`]).
    /// A process host mints it for a fresh launch and for the fork a relaunch continues an
    /// interrupted session as, keeping the continued one on a resume; the headless host answering
    /// a spawn from the run log carries the session its recorded result names, and the conductor
    /// records it in the run log beside the spawn. Empty from a driver that runs no resumable
    /// session (the workflow and replay drivers, a test double), which then leaves every later
    /// spawn of that role a fresh one.
    pub session_id: String,
    /// The session the spawn's last launch CONTINUED, a relaunch of an interrupted launch
    /// included: [`SpawnOpts::resumed_from`] when the host resumed it, and the session the
    /// interrupted launch ran as when the host relaunched one, forking it into
    /// [`AgentResult::session_id`], so behind a chain of relaunches, or a relaunch of the
    /// fallback's fresh launch, it names a session the spawn never asked for. Empty for a fresh
    /// launch no relaunch followed - including the fallback a host takes when the session it was
    /// asked to resume no longer exists - and for a spawn the headless host answers from the run
    /// log, which launches nothing.
    pub resumed_from: String,
}

/// The host values a headless spawn carries, resolved once per command by
/// `rigger_driver::hooks::resolve_spawn_host` from the workflow's `defaults` and the operator's
/// checkout, never set by hand.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SpawnHost {
    /// Whether the spawn's settings carry the PreToolUse grep-guard: `defaults.grep_guard`.
    pub grep_guard: bool,
    /// The fan-out helpers the spawn defines and pre-approves, one per helper rigger ships, in
    /// its order: the operator's copy under `.claude/agents/` where the checkout holds one, else
    /// the embedded text.
    pub helpers: Vec<HelperCopy>,
    /// The one directory, relative to the tree's root and ending in `/`, a producer's commits
    /// may touch: the project's declared specs location, which a producer's `Stop` hook and the
    /// conductor's landing of its commits each take as the stage's output.
    pub producer_output: String,
}

/// One fan-out helper's definition as a spawn host resolved it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HelperCopy {
    /// The helper's file name under `.claude/agents/` (`lookup.md`, `verify.md`).
    pub name: String,
    /// The helper's definition: its frontmatter and its prompt body.
    pub text: String,
    /// Whether `text` is the operator's own copy, read from the checkout, rather than the
    /// embedded one.
    pub present: bool,
}

/// Per-spawn options, built through [`SpawnOpts::new`] from a resolved [`SpawnHost`].
pub struct SpawnOpts {
    /// The spawn's DETERMINISTIC id (`{unit}/{role}#{attempt}`, see
    /// [`spawn_id`](crate::spawn::spawn_id)). The replay driver and the headless host key on it
    /// to answer an already-recorded spawn from the log or to park an unrecorded one; every other
    /// driver names the spawn by it. Empty for a caller that does not replay from the log.
    pub id: String,
    /// The unit this spawn belongs to - the parked request's `unit` (and the display
    /// label's unit half). Empty when the caller does not park.
    pub unit: String,
    /// The stage that produced this spawn - the parked request's `stage` (the thin
    /// driver's per-unit `opts.phase` label half). Empty when the caller does not park.
    pub stage: String,
    /// The 0-based remediation attempt this spawn runs under (the same integer the
    /// deterministic `id` encodes after `#`). Every driver resolves the actual spawn
    /// model through [`AgentDef::model_for_attempt`](crate::config::AgentDef::model_for_attempt)
    /// with it, so a `model_ladder` agent (spec 10 unit 4) escalates one rung per attempt
    /// and the model that runs matches the [`META_MODEL_ALIAS`] the conductor stamps for
    /// the same attempt. 0 for a caller that does not remediate.
    pub attempt: u32,
    /// The agent's PERSONA - its role instructions, the markdown body of its
    /// `.rigger/agents/<id>.md` definition (`AgentDef::prompt`). It belongs as the
    /// agent's SYSTEM prompt, distinct from the grounded task `prompt`. The conductor
    /// is the SINGLE place that sets it (from `agent_def.prompt`), so every driver
    /// consumes the same persona source and cannot diverge: a process host writes it to the
    /// file `--system-prompt-file` names. Empty when the agent declared no body.
    pub system_prompt: String,
    /// The working directory the agent runs in: an isolated worktree, or "" for
    /// the current dir.
    pub dir: String,
    /// Whether this spawn runs in an isolated git worktree (§6). False when the
    /// agent runs in the current dir (no repo, or `isolation: none`).
    pub isolation: bool,
    /// Whether this spawn is one of several running concurrently in a fan-out
    /// stage (§6). False for a single-worker stage.
    pub parallel: bool,
    /// The agent's blast-radius: the grounded seed files this spawn is scoped to
    /// (§5.3), carried onto the parked spawn request. A subprocess driver
    /// cannot do mid-run injection and ignores it.
    pub blast_radius: Vec<String>,
    /// The id of the run this spawn belongs to (spec 06, unit 1), set by the conductor
    /// from the current run. A parking driver stamps it into the `SpawnRequested`
    /// event's [`crate::run::META_RUN_ID`] metadata so the parked spawn is attributable
    /// to its run; the blocking drivers ignore it. Empty for a caller outside a run.
    pub run_id: String,
    /// The spawn's LIVE WORK-LINE (spec 19a, c4): the unit's criterion (its
    /// [`Stage::coverage`](crate::config::Stage), trimmed), which a parking driver copies
    /// onto the [`SpawnRequest::title`](crate::spawn::SpawnRequest::title) so the thin
    /// driver narrates the actual WORK, not just the `{unit}:{stage}` progress-group label.
    /// Empty for a stage with no criterion (a plan/canary spawn), which then serializes
    /// exactly as before - a purely additive field the blocking drivers ignore.
    pub title: String,
    /// The spawn's build environment - the `(name, value)` env vars this spawn's agent
    /// process must carry, assembled by the SINGLE `RunCtx::spawn_env` fn: the workflow's
    /// declared `build.env` resolved over the spawn's site cache
    /// ([`build_cache`](Self::build_cache)), then the run base and the gate socket when the
    /// run has them. With nothing declared it carries no build variable, so the agent's own
    /// builds inherit rigger's environment unchanged. The blocking cli driver applies every
    /// pair to its spawned `Command`; a driver with no subprocess of its own (a test double)
    /// may ignore it.
    pub env: Vec<(String, String)>,
    /// The build cache [`env`](Self::env) resolves `{build_cache}` over: the spawn's unit
    /// sibling cache when an exported variable uses `{build_cache}`, else empty (nothing
    /// declared uses it, or this spawn's `dir` has no unit sibling, so a standalone review
    /// spawn, `re_plan`'s planner or a canary spawn carries none). A parking driver records it
    /// as the request's `cargo_target_dir`.
    pub build_cache: String,
    /// The routed review roster (spec 67, criterion 4): the role tokens of the spawns of
    /// every review step before this spawn's ([`step_roster`](crate::review::step_roster)),
    /// so a later adversary step's carries the lens and adversary tokens before it and the
    /// adjudicator's every token of the panel. Empty for every other spawn (a lens, or a step
    /// with none before it). The CONDUCTOR is the only honest source - it reads the
    /// panel `review_unit`/`run_fan_out_review_loop` actually routed to (light or full),
    /// never a static declaration a driver guess could get stale against a replayed
    /// lens - so a parking driver copies it verbatim onto
    /// [`SpawnRequest::reviews`](crate::spawn::SpawnRequest::reviews) for the thin
    /// driver to render inside the action phrase.
    pub reviews: Vec<String>,
    /// The per-spawn settings JSON (spec 104 criterion 1): the stream-json host merges the
    /// hooks bound to the spawn and rigger's session hooks and status line onto it, the cli
    /// host the session ones alone, and each passes the result as `--settings`. Empty means no
    /// settings of the spawn's own.
    pub settings_json: String,
    /// The host values the spawn's launch follows: whether the session hosts merge the
    /// PreToolUse grep-guard into [`settings_json`](Self::settings_json), and the fan-out
    /// helpers they define and pre-approve.
    pub host: SpawnHost,
    /// The session id this launch CONTINUES (`claude -p --resume <session_id>`), empty
    /// for a fresh launch. The conductor sets it on a later attempt or review round of a
    /// role whose earlier spawn on the same unit recorded its session
    /// ([`AgentResult::session_id`]), so the persona picks up where it left off instead of
    /// re-reading the criterion, the grounding slice and the diff from nothing. The session
    /// hosts honour it; the workflow and replay drivers ignore it.
    pub resumed_from: String,
    /// The task a RESUMED launch sends in place of the full `prompt`: what changed since the
    /// session's last turn (the prior-failure block, or the review round's delta and REQUIRED
    /// list), since the session already holds the criterion and the slice. Meaningful only
    /// with [`resumed_from`](Self::resumed_from); a host whose resume finds no such session
    /// falls back to a fresh launch with the full `prompt`.
    pub resume_task: String,
    /// What a session host's relaunch of this spawn's interrupted launch sends after its
    /// interruption note and ahead of any [`resume_task`](Self::resume_task) it carries: what
    /// the conductor learned since that launch began, which the forked session never saw. Sent
    /// once: a relaunch whose carried task already holds it sends only that task. A launch that
    /// is no relaunch ignores it; empty sends nothing.
    pub relaunch_task: String,
    /// What the conductor takes from this spawn's worktree ([`dir`](Self::dir)) once its session
    /// ends. Set at the conductor's writing spawn sites alone; [`TreeDuty::None`] everywhere
    /// else. A session host that installs per-spawn hooks gives a spawn with a duty a `Stop` hook
    /// (`rigger hook stop`) asking whether the tree is one that step takes, so the session that
    /// left it otherwise fixes it before it ends.
    pub tree_duty: TreeDuty,
}

/// What the conductor takes from a writing spawn's worktree once the spawn's session ends, which
/// is what that session's `Stop` hook asks of the tree before the session may stop.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum TreeDuty {
    /// Nothing is taken from the tree: a reviewer, or a spawn with no worktree. No `Stop` hook.
    #[default]
    None,
    /// The conductor commits the whole tree onto `branch` - the attempt's checkpoint, or the
    /// conflict resolution's merge and the checkpoint after it - so the hook asks whether that
    /// commit, or the merge before it, would be refused.
    Checkpoint { branch: String },
    /// The conductor lands only the commits the session made in the tree - a producer's - and
    /// never commits the tree itself, so the hook asks whether work of the producer's output,
    /// the only work that lands, is left outside a commit.
    OwnCommits {
        /// The producer's output: [`SpawnHost::producer_output`].
        output: String,
    },
}

/// Whether `path`, relative to the tree's root, lies in the producer's `output`
/// ([`SpawnHost::producer_output`]), a directory relative to the same root named with or without
/// its closing `/`: the scope decision the conductor refuses a producer's commits on, and the one
/// a producer's `Stop` hook holds its session on: only uncommitted work in the output, the only
/// work that lands, holds it.
pub fn in_producer_output(path: &str, output: &str) -> bool {
    let dir = output.trim_end_matches('/');
    path.strip_prefix(dir)
        .is_some_and(|rest| rest.starts_with('/'))
}

impl SpawnOpts {
    /// The options of a spawn carrying `host`, every other field empty, false or 0, for a
    /// caller to spread its own fields over.
    pub fn new(host: SpawnHost) -> SpawnOpts {
        SpawnOpts {
            id: String::new(),
            unit: String::new(),
            stage: String::new(),
            attempt: 0,
            system_prompt: String::new(),
            dir: String::new(),
            isolation: false,
            parallel: false,
            blast_radius: Vec::new(),
            run_id: String::new(),
            title: String::new(),
            env: Vec::new(),
            build_cache: String::new(),
            reviews: Vec::new(),
            settings_json: String::new(),
            host,
            resumed_from: String::new(),
            resume_task: String::new(),
            relaunch_task: String::new(),
            tree_duty: TreeDuty::None,
        }
    }
}

/// The [`SpawnRequest`] a spawn parks, derived from the spawn's arguments alone - the ONE mapping
/// from a spawn's options to its recorded request, shared by every caller that parks one (the
/// replay driver, `rigger critique`): its deterministic id, unit, stage, persona, dir,
/// blast-radius, work-line and review roster come from `opts`; its granted tools from the agent
/// (already fan-out-stripped by [`AgentDef::allowed_tools`]); and its task prompt from `prompt`.
/// Its model is the cascade rung this attempt resolves ([`AgentDef::model_for_attempt`], spec 10
/// unit 4), so a `model_ladder` agent parks a request naming the rung it escalated to for
/// `opts.attempt` - the same rung the conductor stamps as the requested alias. Its
/// `max_wall_clock` (resolved from `defaults.max_wall_clock` at config load) rides along too, so
/// the parked spawn also carries its per-role liveness bound (spec 10, unit 3).
pub fn spawn_request(agent: &AgentDef, prompt: &str, opts: &SpawnOpts) -> SpawnRequest {
    SpawnRequest {
        id: opts.id.clone(),
        unit: opts.unit.clone(),
        stage: opts.stage.clone(),
        prompt: prompt.to_string(),
        system_prompt: opts.system_prompt.clone(),
        model: agent.model_for_attempt(opts.attempt),
        tools: agent.allowed_tools(),
        dir: opts.dir.clone(),
        blast_radius: opts.blast_radius.clone(),
        max_wall_clock: agent.max_wall_clock,
        // The live work-line (spec 19a, c4): copy the conductor-threaded unit criterion onto
        // the parked request so the persisted `SpawnRequested` carries the WORK every reader of
        // the request shows.
        title: opts.title.clone(),
        // The routed review roster (spec 67, criterion 4): copy the conductor-threaded
        // adversary/adjudicator roster onto the parked request, the same additive seam
        // `title` establishes, so the persisted request carries it for a reader to render.
        reviews: opts.reviews.clone(),
        // The build cache the conductor chose (`spawn_env`), recorded so every reader of the
        // spawn names the same directory.
        cargo_target_dir: opts.build_cache.clone(),
    }
}

/// AgentDriver spawns an agent to completion. The agent records events it emits
/// during its run: the workflow and headless drivers' agents record through the `rigger`
/// MCP server while the spawn runs, so both leave `emit` unused, and the blocking cli
/// driver replays the emit lines of its agent's stdout through `emit` after the run.
///
/// A driver that launches more than once inside one spawn (a resume fallback) launches fresh
/// only on a run it has proved unheld, reading the hold after the ending. A driver handed no
/// hold reads none and its fallback stands, so a driver with a fallback sits behind the
/// held-run decorator (`HeldDriver`) only when it is handed the run's hold. The blocking cli
/// driver `rigger canary` probes with and the critique host, which is handed no hold sink, are
/// that no-hold case, and neither sits behind the decorator.
pub trait AgentDriver: Send + Sync {
    fn spawn(
        &self,
        agent: &AgentDef,
        prompt: &str,
        opts: &SpawnOpts,
        emit: &dyn Fn(&str, Value) -> Result<(), Error>,
    ) -> Result<AgentResult, Error>;
}

/// The sentinel a parking [`AgentDriver`] embeds in its spawn error to signal
/// that a spawn was PARKED - left for an out-of-process result or a relaunch
/// ([`parked_spawn`]) - rather than run to completion or genuinely failed. It uses control
/// characters no real error text carries, so [`is_parked`] recognizes it even after
/// the conductor wraps the driver error with stage/agent context (`format!("... {}",
/// e.0)` keeps the marker as a substring).
///
/// Parking is part of the `AgentDriver` INTERFACE contract, so it lives here beside the
/// trait: a parking adapter constructs the signal via [`parked_spawn`] and the
/// conductor recognizes it via [`is_parked`], and the use case never has to name the
/// adapter to tell a park from a failure.
pub const PARKED_MARKER: &str = "\u{1}rigger:spawn-parked\u{1}";

/// Construct the PARK signal a driver returns for spawn `id` it cannot answer in-process: the
/// replay driver persisted the unrecorded spawn request. On this signal the conductor unwinds
/// the unit CLEANLY - no `UnitFailed`, no remediation - and its pass ends once every in-flight
/// spawn is parked; a later pass over the same log replays the result recorded for the spawn
/// (`rigger result`). This is a normal failure, so a spawn that never parks is entirely
/// unaffected.
pub fn parked_spawn(id: &str) -> Error {
    Error(format!(
        "{PARKED_MARKER} spawn {id:?} parked: nothing serves it in-process"
    ))
}

/// Sentinel prefixed to the [`Error`] a spawn's in-process emit callback returns when the emit
/// guard refused a record the agent emitted - a payload the fold cannot apply, or an id another
/// seat holds - so the conductor tells the refusal (a defect in what the agent printed, charged
/// to the implementer's attempt or re-run once with the refusal in every other author's prompt)
/// from a spawn fault (an outage, rerun uncharged) without reading the guard's prose. It crosses the `AgentDriver` interface inside the spawn's error, as [`PARKED_MARKER`]
/// does, in the same control-character idiom; [`strip_failure_marker`] drops it before the
/// text reaches an agent's prompt or the operator.
pub const EMIT_REFUSED_MARKER: &str = "\u{1}rigger:emit-refused\u{1}";

/// Sentinel heading the message of the [`Error`] a spawn returns for a launch the host closed
/// `interrupted` ([`launch_interrupted`]): the host decides interrupted once, and the decision
/// crosses the `AgentDriver` interface with its error, in the [`PARKED_MARKER`] idiom, so the held-run
/// decorator relaunches on it without reading the hold again. It sits behind the class bracket,
/// which stays at the head for [`failure_class`]; [`strip_failure_marker`] drops both before the
/// text reaches an agent's prompt or the operator.
pub const INTERRUPTED_MARKER: &str = "\u{1}rigger:launch-interrupted\u{1}";

/// Sentinel heading the [`Error`] of a store read that failed before a spawn launched
/// ([`store_fault`]): an infra fault, never the outcome of a launched spawn. The host marks it
/// once at its spawn boundary and the held-run decorator marks its own hold read, so the
/// decorator records no result for it (a restart relaunches the spawn) and the conflict resolver
/// retries it as an infra crash instead of spending a round. It crosses the `AgentDriver` interface
/// inside the spawn's error, in the [`PARKED_MARKER`] control-character idiom;
/// [`strip_failure_marker`] drops it before the text reaches an agent's prompt or the operator.
pub const STORE_FAULT_MARKER: &str = "\u{1}rigger:store-fault\u{1}";

/// Mark `e`, the error of a store read that failed before a spawn launched, with
/// [`STORE_FAULT_MARKER`].
pub fn store_fault(e: Error) -> Error {
    Error(format!("{STORE_FAULT_MARKER}{}", e.0))
}

// ---- FAILURE CLASS (spec 104 criterion 5: A FAILURE HAS A CLASS) ----
//
// adj-u104c5 REQUIRED FIX 3 (arch-u104c5-failure-class-belongs-in-conductor-not-driver):
// lives here, beside `Error`/`AgentDriver`/`PARKED_MARKER`/`is_parked`, rather than in the
// `driver::claude_code` ADAPTER - an interface-crossing sentinel-plus-typed-class pair is exactly
// `PARKED_MARKER`'s own shape, and spec 105's hold controller (every `AgentDriver`
// implementation, not just this one host) will read this class the same way `is_parked`
// already reads `PARKED_MARKER`.

/// Claude Code's own error category (Design's FAILURE CLASS; architecture addendum §5.1),
/// plus `Unknown` for a session that ends with none of the below ever observed. Every
/// variant is API-side (Design: "Every class is API-side").
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AgentFailure {
    RateLimit,
    Overloaded,
    ServerError,
    AuthenticationFailed,
    OauthOrgNotAllowed,
    CloudCredentialError,
    BillingError,
    AccountOnHold,
    VerificationRequired,
    ModelNotFound,
    InvalidRequest,
    MaxOutputTokens,
    #[default]
    Unknown,
}

/// What a failure category does to the run: spec 105's disposition table, the verbs its rows
/// read. [`AgentFailure::row`] is the one table; the host reads it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Disposition {
    /// The run holds on the first `api_retry` or `StopFailure` carrying the category.
    Hold,
    /// The session retries by itself; the run holds only when a session ends of the category.
    Wait,
    /// The category never holds: the error takes its spawn site's existing path.
    Fault,
    /// The API refused the request as the conductor built it: the conductor's fault, so the run
    /// holds as on a [`CONDUCTOR_FAULT`] and the close counts toward no outage.
    Rejected,
}

/// One row of the disposition table: what a category does to the run, the action a hold of it
/// names, whether that action needs a person and whether it is a credential category.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DispositionRow {
    pub disposition: Disposition,
    /// The `RunHeld.action` of a hold of this cause, empty for a category that never holds.
    pub action: &'static str,
    /// The `RunHeld.needs_person` of a hold of this cause.
    pub needs_person: bool,
    /// Whether a probe of a hold of this cause reads the login (`claude auth status`) before
    /// its canary: a logged-out answer is the cause itself, so no canary runs.
    pub credential: bool,
    /// Whether a probe can release a hold of this cause (spec 105 RELEASE): `false` for a cause
    /// whose hold only `rigger release` ends and for a category that never holds.
    pub probed: bool,
}

impl DispositionRow {
    /// Whether a session that ended of this category waits on the run, a hold or a wait row,
    /// rather than failing its spawn as a fault or a rejected row does.
    pub fn waits(&self) -> bool {
        matches!(self.disposition, Disposition::Hold | Disposition::Wait)
    }

    /// Whether a hold of this row is for the orchestrator first (spec 105): it holds, needs no
    /// person and no probe ends it, so the orchestrator's fix and its `rigger release` do, and
    /// the same fault again after a release goes to a person ([`FAULT_RECURRED`]).
    pub fn for_orchestrator(&self) -> bool {
        self.disposition == Disposition::Hold && !self.needs_person && !self.probed
    }

    /// Whether a hold of this row asks someone to act: it needs a person or it is for the
    /// orchestrator ([`DispositionRow::for_orchestrator`]). Only such a cause moves an open hold.
    pub fn asks(&self) -> bool {
        self.needs_person || self.for_orchestrator()
    }
}

/// The cause of the hold three consecutive failed launches set: the API is failing with no
/// category that names a person's action.
pub const API_UNSTABLE: &str = "api-unstable";

/// The cause of the hold `rigger hold` appends: the operator holds the run, giving a reason.
pub const OPERATOR: &str = "operator";

/// The cause of the hold the conductor appends on a fault of its own that ends the run's drive, a
/// spawn the API refused included: the fault, not a unit's work, stopped the run. Its row holds it
/// for the orchestrator, the session that launched the run and watches it, which fixes the cause
/// at its root and releases the run; [`FAULT_RECURRED`] and [`FAULT_ON_A_PERSONS_HOLD`] hold it
/// for a person instead.
pub const CONDUCTOR_FAULT: &str = "conductor-fault";

/// A hold for the orchestrator ([`DispositionRow::for_orchestrator`]) held for a person instead:
/// a hold its run released held it on the same fault, so the orchestrator's fix did not hold.
pub const FAULT_RECURRED: DispositionRow = DispositionRow {
    action: "the orchestrator's fix did not hold: a person decides, then runs rigger release",
    needs_person: true,
    ..CONDUCTOR_FAULT_ROW
};

/// A hold for the orchestrator held for a person instead: it landed on an open hold that already
/// waits on one, so its release stays theirs.
pub const FAULT_ON_A_PERSONS_HOLD: DispositionRow = DispositionRow {
    action: "a person holds this run: they read the fault, decide, then run rigger release",
    needs_person: true,
    ..CONDUCTOR_FAULT_ROW
};

/// A conductor-fault hold of a unit's own work git refused (a checkpoint commit, the merge of the
/// run branch into its worktree), held for the orchestrator: the unit was charged the attempt the
/// refusal ended, so its release retries the unit with the refusal in its next prompt.
pub const UNIT_WORK_REFUSED: DispositionRow = for_the_orchestrator(
    "git refused a unit's own work, the refusal in the fault rigger status prints under this \
     hold: rigger release retries the unit with it; fix the tree first only if the fault shows \
     no agent session can change it",
);

/// The conductor-fault row: held for the orchestrator, which reads the fault `rigger status`
/// prints under the hold, fixes its cause and releases the run.
const CONDUCTOR_FAULT_ROW: DispositionRow = for_the_orchestrator(
    "read the fault rigger status prints under this hold, fix its cause at the root, then run \
     rigger release",
);

/// The row of a hold for the orchestrator naming `action`: no person first, no probe ends it.
const fn for_the_orchestrator(action: &'static str) -> DispositionRow {
    DispositionRow {
        disposition: Disposition::Hold,
        action,
        needs_person: false,
        credential: false,
        probed: false,
    }
}

/// The action of a hold that clears without anyone: a wait-class category's and `api-unstable`'s.
const RETRIES_BY_ITSELF: &str = "none, the run retries by itself";

const WAITS: DispositionRow = DispositionRow {
    disposition: Disposition::Wait,
    action: RETRIES_BY_ITSELF,
    needs_person: false,
    credential: false,
    probed: true,
};

const FAULTS: DispositionRow = DispositionRow {
    disposition: Disposition::Fault,
    action: "",
    needs_person: false,
    credential: false,
    probed: false,
};

const REJECTED: DispositionRow = DispositionRow {
    disposition: Disposition::Rejected,
    ..FAULTS
};

/// The row a hold of `action` reads that a person resolves first, a probe ending it once their
/// act has cleared its cause.
const fn holds(action: &'static str) -> DispositionRow {
    DispositionRow {
        disposition: Disposition::Hold,
        action,
        needs_person: true,
        credential: false,
        probed: true,
    }
}

/// The disposition table's row for a hold's `cause`: `api-unstable`'s, `operator`'s,
/// `conductor-fault`'s, or the category's. The `operator` row needs a person, names no action and
/// is never probed: no row can hold a per-hold reason, so `rigger hold`'s append puts its reason
/// in the action over the row's. The `conductor-fault` row is held for the orchestrator, the fault
/// its detail.
pub fn cause_row(cause: &str) -> DispositionRow {
    match cause {
        API_UNSTABLE => DispositionRow {
            disposition: Disposition::Hold,
            ..WAITS
        },
        OPERATOR => DispositionRow {
            probed: false,
            ..holds("")
        },
        CONDUCTOR_FAULT => CONDUCTOR_FAULT_ROW,
        category => AgentFailure::from_category(category).row(),
    }
}

impl AgentFailure {
    /// Every error category, in a fixed order: the exhaustive set [`from_category`] and the
    /// CLI's own `--class` validation both check membership against.
    pub const CATEGORIES: [AgentFailure; 13] = [
        AgentFailure::RateLimit,
        AgentFailure::Overloaded,
        AgentFailure::ServerError,
        AgentFailure::AuthenticationFailed,
        AgentFailure::OauthOrgNotAllowed,
        AgentFailure::CloudCredentialError,
        AgentFailure::BillingError,
        AgentFailure::AccountOnHold,
        AgentFailure::VerificationRequired,
        AgentFailure::ModelNotFound,
        AgentFailure::InvalidRequest,
        AgentFailure::MaxOutputTokens,
        AgentFailure::Unknown,
    ];

    /// Claude Code's own category string for this class, exactly as it appears in a
    /// `system/api_retry` line's `error` field (Design's Problem section probe) and as
    /// baked into the installed `--class <category>` hook command.
    pub fn as_str(self) -> &'static str {
        match self {
            AgentFailure::RateLimit => "rate_limit",
            AgentFailure::Overloaded => "overloaded",
            AgentFailure::ServerError => "server_error",
            AgentFailure::AuthenticationFailed => "authentication_failed",
            AgentFailure::OauthOrgNotAllowed => "oauth_org_not_allowed",
            AgentFailure::CloudCredentialError => "cloud_credential_error",
            AgentFailure::BillingError => "billing_error",
            AgentFailure::AccountOnHold => "account_on_hold",
            AgentFailure::VerificationRequired => "verification_required",
            AgentFailure::ModelNotFound => "model_not_found",
            AgentFailure::InvalidRequest => "invalid_request",
            AgentFailure::MaxOutputTokens => "max_output_tokens",
            AgentFailure::Unknown => "unknown",
        }
    }

    /// Parse Claude Code's category string back into a class - LENIENT: a category this
    /// crate does not (yet) recognize degrades to [`AgentFailure::Unknown`] rather than
    /// failing, exactly matching Design's own fallback ("else `unknown`") since the string
    /// classified here can come from either of FAILURE CLASS's two sources (a `StopFailure`
    /// record, or a live `system/api_retry` line) - either of which may carry a category
    /// Claude Code adds after this crate is built, and a session must never fail to classify
    /// merely because the string is unfamiliar.
    pub fn from_category(category: &str) -> AgentFailure {
        AgentFailure::CATEGORIES
            .into_iter()
            .find(|c| c.as_str() == category)
            .unwrap_or(AgentFailure::Unknown)
    }

    /// Whether the API refused the request as the conductor built it: this category's row
    /// rejects ([`Disposition::Rejected`]).
    pub fn is_rejected(self) -> bool {
        self.row().disposition == Disposition::Rejected
    }

    /// This category's row of the disposition table (spec 105): THE one table, beside the enum
    /// so a new category cannot compile without one.
    pub fn row(self) -> DispositionRow {
        match self {
            AgentFailure::AuthenticationFailed | AgentFailure::OauthOrgNotAllowed => {
                DispositionRow {
                    credential: true,
                    ..holds("run claude auth login")
                }
            }
            AgentFailure::CloudCredentialError => holds("refresh the cloud provider's credentials"),
            AgentFailure::BillingError => holds("fix the account's billing"),
            AgentFailure::AccountOnHold => holds("resolve the account's hold"),
            AgentFailure::VerificationRequired => holds("complete the account's verification"),
            AgentFailure::ModelNotFound => for_the_orchestrator(
                "fix the agent's configured model, restart rigger run with --rebase-definition, \
                 then run rigger release",
            ),
            AgentFailure::RateLimit | AgentFailure::Overloaded | AgentFailure::ServerError => WAITS,
            AgentFailure::InvalidRequest => REJECTED,
            AgentFailure::MaxOutputTokens | AgentFailure::Unknown => FAULTS,
        }
    }
}

impl std::fmt::Display for AgentFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A FAILURE HAS A CLASS (spec 104 criterion 5, Design): "a session that ends without a
/// `result` takes its class from, in order: the record written by the `StopFailure` hook,
/// ... the last `api_retry.error`, else `unknown`." PURE: both sources are already-resolved
/// strings - the class the session itself reported (the `error` of the last assistant line,
/// else the launch's `StopFailure` record, [`crate::progress::latest_stop_failure_class`];
/// `None` when neither did) and the last `system/api_retry` line's `error` THIS launch's own
/// reader saw (`None` when none arrived) - so the priority order itself is testable with no
/// store, no stream, no process.
pub fn classify_failure(
    reported_class: Option<&str>,
    last_api_retry_category: Option<&str>,
) -> AgentFailure {
    reported_class
        .or(last_api_retry_category)
        .map(AgentFailure::from_category)
        .unwrap_or(AgentFailure::Unknown)
}

/// Sentinel embedding an [`AgentFailure`] class into a driver [`Error`] message so "the
/// port returns the class as data" (Design, FAILURE CLASS) without widening
/// `AgentDriver::spawn`'s `Result<AgentResult, Error>` signature - the SAME control-
/// character-sentinel idiom [`PARKED_MARKER`] already establishes, reused rather than a
/// second, parallel convention. `\u{2}` (STX) never appears in ordinary error prose.
const FAILURE_MARKER: char = '\u{2}';

/// Build the `Error` a `read_stream` call returns for a session that ended without a
/// result, carrying `class` ahead of the human-readable `message`. `pub`: `driver::claude_code::Driver`'s own
/// `classify_no_result` is this crate's one production caller today, on the adapter side
/// of the interface this marker lives on.
pub fn no_result_error(class: AgentFailure, message: String) -> Error {
    Error(format!("{FAILURE_MARKER}{class}{FAILURE_MARKER}{message}"))
}

/// The `Error` a spawn returns for a launch the host closed `interrupted` on `class`: the
/// [`no_result_error`] of `class` with [`INTERRUPTED_MARKER`] heading its `message`, so the class
/// stays at the head of the error for [`failure_class`].
pub fn launch_interrupted(class: AgentFailure, message: String) -> Error {
    no_result_error(class, format!("{INTERRUPTED_MARKER}{message}"))
}

/// Drop the class marker a driver spawn [`Error`] carries ahead of its message, if present,
/// leaving only the human-readable text: the [`FAILURE_MARKER`]-bracketed class
/// [`no_result_error`] embeds (adj-u104c5 REQUIRED FIX 2,
/// sdet-u104c5-failure-marker-leaks-unstripped-into-operator-visible-text), with the
/// [`INTERRUPTED_MARKER`] a [`launch_interrupted`] error carries behind it, the
/// [`EMIT_REFUSED_MARKER`] the conductor's agent emit puts ahead of an emit guard's refusal, or
/// the [`STORE_FAULT_MARKER`] heading a [`store_fault`] (each dropped here because its readers -
/// the held-run decorator for the first, the implementer's attempt arm and the refused record's
/// re-run for the second, the held-run decorator and the conflict resolver for the third - match
/// it before this strip runs, while `PARKED_MARKER` and `BUDGET_MARKER` pass through untouched, since
/// callers upstream still branch on them).
/// Unlike `PARKED_MARKER`/`PLAN_LANDING_MARKER`/`LAND_REFUSED_MARKER` - bare prefixes a plain
/// `.replace(MARKER, "")` cleans - [`FAILURE_MARKER`] brackets DATA (the class name) that
/// the message ALSO repeats in prose (`no_result_error`'s own "class {class}"), so a bare
/// `.replace` would leave that class name's raw text glued onto the message with nothing
/// separating them. Every site that turns a driver spawn `Err` into text an agent or the
/// operator reads (a retry prompt, a lesson, an escalation reason, a wrapped stage error)
/// calls this FIRST, exactly once, rather than re-deriving the strip; `e` carrying no marker
/// (every other driver's error, or text already stripped) passes through byte-for-byte
/// unchanged.
pub fn strip_failure_marker(e: &Error) -> String {
    if let Some(text) = [EMIT_REFUSED_MARKER, STORE_FAULT_MARKER]
        .iter()
        .find_map(|marker| e.0.strip_prefix(marker))
    {
        return text.to_string();
    }
    match failure_parts(e) {
        Some((_, message)) => message
            .strip_prefix(INTERRUPTED_MARKER)
            .unwrap_or(message)
            .to_string(),
        None => e.0.clone(),
    }
}

/// The class `e` carries in a whole [`FAILURE_MARKER`] bracket at its head, a name no binary
/// knows reading as [`AgentFailure::from_category`] reads it; `None` for an error with no such
/// bracket, wrapped errors included, since only a driver's own return carries one there.
pub fn failure_class(e: &Error) -> Option<AgentFailure> {
    failure_parts(e).map(|(class, _)| AgentFailure::from_category(class))
}

/// The class and the message of an error [`no_result_error`] built, the one reading of its
/// bracket; `None` for any error with no whole bracket at its head.
fn failure_parts(e: &Error) -> Option<(&str, &str)> {
    e.0.strip_prefix(FAILURE_MARKER)?.split_once(FAILURE_MARKER)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::test_spawn_host;

    const RETRIES: &str = "none, the run retries by itself";
    const LOGIN: &str = "run claude auth login";
    const MODEL: &str = "fix the agent's configured model, restart rigger run with \
                         --rebase-definition, then run rigger release";

    /// THE CLASS AN ERROR CARRIES: the class [`no_result_error`] bracketed at the head of the
    /// error, a name no binary knows reading as `unknown` as [`AgentFailure::from_category`] has
    /// it, and none for an error with no whole bracket at its head.
    #[test]
    fn an_error_answers_the_class_it_carries() {
        let bracketed = no_result_error(AgentFailure::RateLimit, "class rate_limit".into());
        let rows: [(&str, Error, Option<AgentFailure>); 7] = [
            (
                "a class at the head",
                Error(bracketed.0.clone()),
                Some(AgentFailure::RateLimit),
            ),
            (
                "the unknown class",
                no_result_error(AgentFailure::Unknown, "no class".into()),
                Some(AgentFailure::Unknown),
            ),
            (
                "a name no binary knows",
                Error(format!("{FAILURE_MARKER}new_category{FAILURE_MARKER}text")),
                Some(AgentFailure::Unknown),
            ),
            ("no marker", Error("the session stopped".into()), None),
            (
                "a bracket never closed",
                Error(format!("{FAILURE_MARKER}rate_limit")),
                None,
            ),
            (
                "a bracket behind wrapping context",
                Error(format!("stage: {}", bracketed.0)),
                None,
            ),
            (
                "an emit refusal",
                Error(format!("{EMIT_REFUSED_MARKER}refused")),
                None,
            ),
        ];
        for (name, error, want) in rows {
            assert_eq!(failure_class(&error), want, "{name}");
        }
    }

    /// THE INTERRUPTED MARK (spec 105): the error a launch the host closed `interrupted` returns
    /// keeps its class at the head, and stripped it reads as its message alone.
    #[test]
    fn an_interrupted_launchs_error_answers_its_class_and_strips_to_its_message() {
        let e = launch_interrupted(
            AgentFailure::Unknown,
            "the stream ended with no result".into(),
        );
        assert_eq!(
            (failure_class(&e), strip_failure_marker(&e)),
            (
                Some(AgentFailure::Unknown),
                "the stream ended with no result".to_string()
            )
        );
    }

    /// THE STORE FAULT MARK (spec 105): the error of a store read that failed before a spawn
    /// launched carries no class, and stripped it reads as the read's own error.
    #[test]
    fn a_store_faults_error_carries_no_class_and_strips_to_the_reads_error() {
        let e = store_fault(Error("event store: the run read failed".into()));
        assert_eq!(
            (failure_class(&e), strip_failure_marker(&e)),
            (None, "event store: the run read failed".to_string())
        );
    }

    /// THE DISPOSITION TABLE (spec 105): every category, by its wire name, maps to the disposition,
    /// the action and the need for a person decided for it, whether it is a credential category
    /// whose probe reads the login first and whether a probe ends its hold; a name no binary knows
    /// faults as `unknown` does. `model_not_found` alone is for the orchestrator first.
    #[test]
    fn every_category_maps_to_its_decided_disposition_action_and_person() {
        use Disposition::{Fault, Hold, Rejected, Wait};
        let rows: [(&str, Disposition, &str, bool, bool, bool); 14] = [
            ("rate_limit", Wait, RETRIES, false, false, true),
            ("overloaded", Wait, RETRIES, false, false, true),
            ("server_error", Wait, RETRIES, false, false, true),
            ("authentication_failed", Hold, LOGIN, true, true, true),
            ("oauth_org_not_allowed", Hold, LOGIN, true, true, true),
            (
                "cloud_credential_error",
                Hold,
                "refresh the cloud provider's credentials",
                true,
                false,
                true,
            ),
            (
                "billing_error",
                Hold,
                "fix the account's billing",
                true,
                false,
                true,
            ),
            (
                "account_on_hold",
                Hold,
                "resolve the account's hold",
                true,
                false,
                true,
            ),
            (
                "verification_required",
                Hold,
                "complete the account's verification",
                true,
                false,
                true,
            ),
            ("model_not_found", Hold, MODEL, false, false, false),
            ("invalid_request", Rejected, "", false, false, false),
            ("max_output_tokens", Fault, "", false, false, false),
            ("unknown", Fault, "", false, false, false),
            ("a_category_no_binary_knows", Fault, "", false, false, false),
        ];
        for (category, disposition, action, needs_person, credential, probed) in rows {
            let row = DispositionRow {
                disposition,
                action,
                needs_person,
                credential,
                probed,
            };
            assert_eq!(
                AgentFailure::from_category(category).row(),
                row,
                "{category}"
            );
        }
        for category in AgentFailure::CATEGORIES {
            let named = rows.iter().filter(|row| row.0 == category.as_str());
            assert_eq!(named.count(), 1, "{category} has one row here");
        }
    }

    /// A hold's `cause` reads its row from the one table, the three causes no category names
    /// included: `api-unstable` needs no person and the run retries by itself; `operator` needs a
    /// person, names no action, `rigger hold`'s reason standing in for it, and no probe ends it;
    /// `conductor-fault` is held for the orchestrator, its action what the orchestrator does, and
    /// its two rows for a person name why a person decides.
    #[test]
    fn a_holds_cause_reads_the_table_api_unstable_and_operator_included() {
        let rows = [
            (
                API_UNSTABLE,
                DispositionRow {
                    disposition: Disposition::Hold,
                    action: RETRIES,
                    needs_person: false,
                    credential: false,
                    probed: true,
                },
            ),
            (
                "operator",
                DispositionRow {
                    disposition: Disposition::Hold,
                    action: "",
                    needs_person: true,
                    credential: false,
                    probed: false,
                },
            ),
            (
                "conductor-fault",
                DispositionRow {
                    disposition: Disposition::Hold,
                    action: "read the fault rigger status prints under this hold, fix its cause \
                             at the root, then run rigger release",
                    needs_person: false,
                    credential: false,
                    probed: false,
                },
            ),
            (
                "authentication_failed",
                AgentFailure::AuthenticationFailed.row(),
            ),
            ("rate_limit", AgentFailure::RateLimit.row()),
        ];
        for (cause, row) in rows {
            assert_eq!(cause_row(cause), row, "{cause}");
        }
        assert_eq!(API_UNSTABLE, "api-unstable");
        let for_a_person = |action| DispositionRow {
            disposition: Disposition::Hold,
            action,
            needs_person: true,
            credential: false,
            probed: false,
        };
        assert_eq!(
            [FAULT_RECURRED, FAULT_ON_A_PERSONS_HOLD],
            [
                for_a_person(
                    "the orchestrator's fix did not hold: a person decides, then runs rigger \
                     release"
                ),
                for_a_person(
                    "a person holds this run: they read the fault, decide, then run rigger release"
                ),
            ],
            "a hold for the orchestrator's rows for a person"
        );
        assert_eq!(
            UNIT_WORK_REFUSED,
            DispositionRow {
                disposition: Disposition::Hold,
                action: "git refused a unit's own work, the refusal in the fault rigger status \
                         prints under this hold: rigger release retries the unit with it; fix the \
                         tree first only if the fault shows no agent session can change it",
                needs_person: false,
                credential: false,
                probed: false,
            },
            "a unit's refused work is held for the orchestrator, whose release retries the unit"
        );
    }

    /// WHO A HOLD ASKS (spec 105): a cause is for the orchestrator first when it holds, needs no
    /// person and no probe ends it, `conductor-fault` and `model_not_found`; it asks someone to act
    /// when it needs a person or is for the orchestrator; an outage, `api-unstable` and a category
    /// that never holds ask no one.
    #[test]
    fn a_hold_is_for_the_orchestrator_when_it_needs_no_person_and_no_probe_ends_it() {
        let rows: [(&str, bool, bool); 9] = [
            ("conductor-fault", true, true),
            ("model_not_found", true, true),
            ("operator", false, true),
            ("authentication_failed", false, true),
            ("billing_error", false, true),
            ("rate_limit", false, false),
            ("api-unstable", false, false),
            ("invalid_request", false, false),
            ("unknown", false, false),
        ];
        for (cause, for_orchestrator, asks) in rows {
            let row = cause_row(cause);
            assert_eq!(
                (row.for_orchestrator(), row.asks()),
                (for_orchestrator, asks),
                "{cause}"
            );
        }
        assert!(
            !FAULT_RECURRED.for_orchestrator() && FAULT_RECURRED.asks(),
            "a recurred fault's row asks a person"
        );
    }

    /// The request a spawn parks is derived from its options and its agent alone: every field the
    /// options carry, the build cache as its `cargo_target_dir`, the agent's model rung for the
    /// attempt, its fan-out-stripped tools and its wall-clock bound - and nothing else of the
    /// options (isolation, env, settings), so a `CARGO_TARGET_DIR` the environment carries is
    /// never read as the build location.
    #[test]
    fn a_parked_request_is_derived_from_its_spawn_options_and_its_agent() {
        let agent = AgentDef {
            id: "critic".into(),
            model_ladder: vec!["sonnet".into(), "opus".into()],
            tools: vec!["Read".into(), "Agent".into(), "Glob".into()],
            max_wall_clock: Some(900),
            prompt: "the persona".into(),
            ..AgentDef::default()
        };
        let opts = SpawnOpts {
            id: "u/adversary#1".into(),
            unit: "u".into(),
            stage: "review".into(),
            attempt: 1,
            system_prompt: "the system prompt".into(),
            dir: "/work/u".into(),
            isolation: true,
            blast_radius: vec!["src/a.rs".into()],
            run_id: "run-1".into(),
            title: "the criterion".into(),
            env: vec![
                ("K".into(), "V".into()),
                ("CARGO_TARGET_DIR".into(), "/work/declared-elsewhere".into()),
            ],
            build_cache: "/work/review-target-u".into(),
            reviews: vec!["lens:sdet".into()],
            settings_json: "{}".into(),
            ..SpawnOpts::new(test_spawn_host())
        };
        assert_eq!(
            spawn_request(&agent, "the task", &opts),
            SpawnRequest {
                id: "u/adversary#1".into(),
                unit: "u".into(),
                stage: "review".into(),
                prompt: "the task".into(),
                system_prompt: "the system prompt".into(),
                model: "opus".into(),
                tools: vec!["Read".into(), "Glob".into()],
                dir: "/work/u".into(),
                blast_radius: vec!["src/a.rs".into()],
                max_wall_clock: Some(900),
                title: "the criterion".into(),
                reviews: vec!["lens:sdet".into()],
                cargo_target_dir: "/work/review-target-u".into(),
            }
        );
    }
}
