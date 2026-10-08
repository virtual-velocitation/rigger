//! The agent-host port (workspace split): the `AgentDriver` trait the conductor spawns every
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

#[derive(Debug, thiserror::Error)]
#[error("conductor: {0}")]
pub struct Error(pub String);

/// Each listed error converts into a conductor [`Error`] carrying its display text.
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
error_from_display!(
    crate::eventstore::Error,
    crate::worktree::Error,
    serde_json::Error
);

/// What an agent returns when it finishes.
#[derive(Clone, Debug, Default)]
pub struct AgentResult {
    pub output: String,
    /// The RESOLVED model id that actually ran this spawn (spec 05 line 52), or empty when
    /// unknown. The replay driver surfaces it from the worker's `rigger result --meta`
    /// report ([`SpawnResult::meta_str`](crate::spawn::SpawnResult::meta_str));
    /// the conductor copies it onto the spawn's unit events via [`META_MODEL_RESOLVED`].
    /// The blocking drivers (cli/workflow) do not learn it and leave it empty.
    pub resolved_model: String,
    /// The Claude Code session this spawn ran as - the id a later attempt or review round of
    /// the same role on the same unit continues with `--resume` ([`SpawnOpts::resumed_from`]).
    /// The cli host mints it for a fresh launch and keeps the continued one on a resume; the
    /// conductor records it in the run log beside the spawn. Empty from a driver that runs no
    /// resumable session (the workflow and replay drivers, a test double), which then leaves
    /// every later spawn of that role a fresh one.
    pub session_id: String,
    /// The session this spawn CONTINUED: [`SpawnOpts::resumed_from`] when the host resumed it,
    /// empty for a fresh launch - including the fallback a host takes when the session it was
    /// asked to resume no longer exists, so a requested resume that came back empty here is the
    /// record that the host fell back to a fresh spawn.
    pub resumed_from: String,
}

/// Per-spawn options.
#[derive(Default)]
pub struct SpawnOpts {
    /// The spawn's DETERMINISTIC id (`{unit}/{role}#{attempt}`, see
    /// [`spawn_id`](crate::spawn::spawn_id)). A stepwise/replay driver keys on it to
    /// answer an already-recorded spawn from the log or to park an unrecorded one; the
    /// blocking drivers (cli/workflow) ignore it. Empty for a caller that does not use
    /// stepwise replay.
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
    /// is the SINGLE place that sets it (from `agent_def.prompt`), so BOTH drivers
    /// consume the same persona source and cannot diverge: the cli driver writes it to the
    /// file `--system-prompt-file` names, the workflow driver carries it to the shim which
    /// passes it to the Agent SDK `query()` as `options.systemPrompt`. Empty when the
    /// agent declared no body.
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
    /// (§5.3). The workflow driver carries it to the shim, which fetches
    /// blast-radius-filtered peer decisions and injects them at the tool boundary;
    /// the cli driver (a subprocess) cannot do mid-run injection and ignores it.
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
    /// The resolved build environment - `(name, value)` env vars this spawn's agent
    /// process must carry, assembled by the SINGLE `RunCtx::spawn_env` fn from two
    /// independent facets: spec 65's ONE build-environment authority
    /// ([`gate::BuildEnv::vars`], empty when no wrapper is configured - today's
    /// ambient-environment behavior, unchanged), so its OWN `cargo test`/`cargo build`
    /// invocations hit the same wrapper cache under the same settings a gate build
    /// gets; PLUS, unconditionally, spec 77 criterion 1's per-unit `CARGO_TARGET_DIR`
    /// (empty/absent when this spawn's `dir` owns no per-unit cache), so those SAME
    /// invocations land in the one per-unit cache a gate build for that `dir` gets,
    /// never an embedded `target/` dir inside the worktree. The blocking cli driver
    /// applies every pair to its spawned `Command`; a driver with no subprocess of its
    /// own (a test double) may ignore it.
    pub env: Vec<(String, String)>,
    /// The routed review roster (spec 67, criterion 4): for the adversary tier, the
    /// unit's routed lens role tokens (`lens:<agent_id>`, [`review_roster`]); for the
    /// adjudicator tier, that same roster plus [`ROLE_ADVERSARY`] when an adversary
    /// ran ([`adjudicator_roster`]). Empty for every other tier (a lens, or a panel with
    /// no lenses/adversary). The CONDUCTOR is the only honest source - it reads the
    /// panel `review_unit`/`run_fan_out_review_loop` actually routed to (light or full),
    /// never a static declaration a driver guess could get stale against a replayed
    /// lens - so a parking driver copies it verbatim onto
    /// [`SpawnRequest::reviews`](crate::spawn::SpawnRequest::reviews) for the thin
    /// driver to render inside the action phrase.
    pub reviews: Vec<String>,
    /// The per-spawn settings JSON (spec 104 criterion 1): the headless host merges rigger's
    /// session hooks and status line onto it and passes the result as `--settings`. Empty
    /// means no settings of the spawn's own; the cli/workflow drivers ignore this field.
    pub settings_json: String,
    /// The 0-based ordinal of this launch within the spawn's current attempt (spec 104
    /// criterion 1): 0 for the first launch, N for the Nth relaunch after an API-side
    /// fault (spec 104's own FAULT class) or a hold-release resume (spec 105). Distinct
    /// from `attempt`, the remediation counter the deterministic id encodes. 0 for the
    /// cli/workflow drivers, which never relaunch a live session.
    pub launch: u32,
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
        // the parked request so the persisted `SpawnRequested` - and the wave `rigger step`
        // prints from it - carry the WORK the thin driver narrates.
        title: opts.title.clone(),
        // The routed review roster (spec 67, criterion 4): copy the conductor-threaded
        // adversary/adjudicator roster onto the parked request, the same additive seam
        // `title` establishes, so the wave carries it for `workflows/rigger.js` to render.
        reviews: opts.reviews.clone(),
        // The build location the conductor chose (`spawn_env`), recorded so a driver that
        // cannot set the worker's environment names the same directory.
        cargo_target_dir: opts
            .env
            .iter()
            .find(|(name, _)| name == "CARGO_TARGET_DIR")
            .map(|(_, dir)| dir.clone())
            .unwrap_or_default(),
    }
}

/// AgentDriver spawns an agent to completion. The agent records events it emits
/// during its run by calling `emit`, so its decisions reach the log live (the
/// workflow driver wires emit to an in-process tool the agent calls).
pub trait AgentDriver: Send + Sync {
    fn spawn(
        &self,
        agent: &AgentDef,
        prompt: &str,
        opts: &SpawnOpts,
        emit: &dyn Fn(&str, Value) -> Result<(), Error>,
    ) -> Result<AgentResult, Error>;
}

/// The sentinel a stepwise/replay [`AgentDriver`] embeds in its spawn error to signal
/// that a spawn was PARKED - persisted to the log and awaiting an out-of-process
/// result - rather than run to completion or genuinely failed. It uses control
/// characters no real error text carries, so [`is_parked`] recognizes it even after
/// the conductor wraps the driver error with stage/agent context (`format!("... {}",
/// e.0)` keeps the marker as a substring).
///
/// Parking is part of the `AgentDriver` PORT contract, so it lives here beside the
/// trait: the replay adapter constructs the signal via [`parked_spawn`] and the
/// conductor recognizes it via [`is_parked`], and the use case never has to name the
/// adapter to tell a park from a failure.
pub const PARKED_MARKER: &str = "\u{1}rigger:spawn-parked\u{1}";

/// Construct the PARK signal a stepwise driver returns for spawn `id`: it persisted the
/// unrecorded spawn request and cannot answer it in-process. On this signal the
/// conductor unwinds the unit CLEANLY - no `UnitFailed`, no remediation - and the step
/// ends once every in-flight spawn is parked at the frontier; a later step, after the
/// courier records the result, replays it. This is a normal failure, so a non-stepwise
/// driver (which never parks) is entirely unaffected.
pub fn parked_spawn(id: &str) -> Error {
    Error(format!(
        "{PARKED_MARKER} spawn {id:?} parked at the unrecorded frontier"
    ))
}

// ---- FAILURE CLASS (spec 104 criterion 5: A FAILURE HAS A CLASS) ----
//
// adj-u104c5 REQUIRED FIX 3 (arch-u104c5-failure-class-belongs-in-conductor-not-driver):
// lives here, beside `Error`/`AgentDriver`/`PARKED_MARKER`/`is_parked`, rather than in the
// `driver::claude_code` ADAPTER - a port-crossing sentinel-plus-typed-class pair is exactly
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
    ModelNotFound,
    InvalidRequest,
    MaxOutputTokens,
    #[default]
    Unknown,
}

impl AgentFailure {
    /// Every error category, in a fixed order: the exhaustive set [`from_category`] and the
    /// CLI's own `--class` validation both check membership against.
    pub const CATEGORIES: [AgentFailure; 12] = [
        AgentFailure::RateLimit,
        AgentFailure::Overloaded,
        AgentFailure::ServerError,
        AgentFailure::AuthenticationFailed,
        AgentFailure::OauthOrgNotAllowed,
        AgentFailure::CloudCredentialError,
        AgentFailure::BillingError,
        AgentFailure::AccountOnHold,
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
}

impl std::fmt::Display for AgentFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A FAILURE HAS A CLASS (spec 104 criterion 5, Design): "a session that ends without a
/// `result` takes its class from, in order: the record written by the `StopFailure` hook,
/// ... the last `api_retry.error`, else `unknown`." PURE: both sources are already-resolved
/// strings - the `StopFailure` record's class (`None` when no hook fired for this spawn,
/// [`crate::progress::latest_stop_failure_class`]) and the last `system/api_retry` line's
/// `error` THIS launch's own reader saw (`None` when none arrived) - so the priority order
/// itself is testable with no store, no stream, no process.
pub fn classify_failure(
    stop_failure_class: Option<&str>,
    last_api_retry_category: Option<&str>,
) -> AgentFailure {
    stop_failure_class
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
/// of the port this marker lives on.
pub fn no_result_error(class: AgentFailure, message: String) -> Error {
    Error(format!("{FAILURE_MARKER}{class}{FAILURE_MARKER}{message}"))
}

/// Drop the [`FAILURE_MARKER`]-bracketed class [`no_result_error`] embeds ahead of its
/// message, if present, leaving only the human-readable text (adj-u104c5 REQUIRED FIX 2,
/// sdet-u104c5-failure-marker-leaks-unstripped-into-operator-visible-text). Unlike
/// `PARKED_MARKER`/`PLAN_LANDING_MARKER`/`LAND_REFUSED_MARKER` - bare prefixes a plain
/// `.replace(MARKER, "")` cleans - [`FAILURE_MARKER`] brackets DATA (the class name) that
/// the message ALSO repeats in prose (`no_result_error`'s own "class {class}"), so a bare
/// `.replace` would leave that class name's raw text glued onto the message with nothing
/// separating them. Every site that turns a driver spawn `Err` into operator-facing text (a
/// lesson, an escalation reason, a wrapped stage error) calls this FIRST, exactly once,
/// rather than re-deriving the strip; `e` carrying no marker (every other driver's error, or
/// text already stripped) passes through byte-for-byte unchanged.
pub fn strip_failure_marker(e: &Error) -> String {
    let Some(rest) = e.0.strip_prefix(FAILURE_MARKER) else {
        return e.0.clone();
    };
    match rest.find(FAILURE_MARKER) {
        Some(end) => rest[end + FAILURE_MARKER.len_utf8()..].to_string(),
        None => e.0.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The request a spawn parks is derived from its options and its agent alone: every field the
    /// options carry, the agent's model rung for the attempt, its fan-out-stripped tools and its
    /// wall-clock bound - and nothing else of the options (isolation, env, settings, launch).
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
                ("CARGO_TARGET_DIR".into(), "/work/review-target-u".into()),
            ],
            reviews: vec!["lens:sdet".into()],
            settings_json: "{}".into(),
            launch: 2,
            ..SpawnOpts::default()
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
