//! The dash's prompt texts (spec 121, criterion 16): every tool description and `inputSchema`
//! property description the MCP server's `tool_list` advertises on its three surfaces, each one
//! `pub const` whose doc comment opens with its class, CONTRACT, since each is the advertised
//! meaning of its tool or argument. THE NAMING RULE names them: a text two or more surfaces
//! advertise word for word is one shared const, `<TOOL>` for a tool's description and
//! `<TOOL>_<ARG>` for a property's; any other text is `<SURFACE>_<TOOL>` or
//! `<SURFACE>_<TOOL>_<ARG>`, its surface prefix `SPAWN_BOUND`, `LOOKUP` or `WORKFLOW`. No text
//! carries a slot. The registry types are `rigger_domain::prompt_surface`'s. The module is pure
//! consts and compiles in every lane.

/// CONTRACT: the spawn-bound surface's `rigger_emit` description: the record is stamped
/// with the bound spawn.
pub const SPAWN_BOUND_RIGGER_EMIT: &str =
    "Record a decision on the shared event log, live, so other agents see it immediately. Stamped with the spawn this server is bound to; optionally set valid_from (the bi-temporal time the fact became true).";

/// CONTRACT: the spawn-bound surface's description of `rigger_emit`'s `meta` argument.
pub const SPAWN_BOUND_RIGGER_EMIT_META: &str =
    "Metadata entries (string->string). meta.spawn, if set, must name THIS server's own bound spawn.";

/// CONTRACT: the description of `rigger_emit`'s `valid_from` argument, which the
/// spawn-bound and workflow surfaces share.
pub const RIGGER_EMIT_VALID_FROM: &str =
    "When the fact became true: unix nanoseconds (integer) or an RFC3339 timestamp string.";

/// CONTRACT: the `rigger_peers` description the spawn-bound and lookup surfaces share.
pub const RIGGER_PEERS: &str =
    "List the decisions, lessons, AND review findings recorded so far, this run's and earlier runs' (each decision labeled live or historical), so you do not work blind to them. Each section returns its newest records under a byte budget; when any are left out, `left_out` counts them and says how to read one by id. Pass `files` to scope the result to decisions, lessons, and findings that touch those files; omit it for every file.";

/// CONTRACT: the `rigger_ground` description the spawn-bound and lookup surfaces share.
pub const RIGGER_GROUND: &str =
    "The MEMORY-adjacent intent lookup (spec 92): rank code entities by relevance to a natural-language query. Same as `rigger ground \"<query>\" [<k>]`.";

/// CONTRACT: the description of `rigger_ground`'s `k` argument, which the spawn-bound and
/// lookup surfaces share.
pub const RIGGER_GROUND_K: &str = "how many results (default 8)";

/// CONTRACT: the description of `rigger_ground`'s `query` argument, which the spawn-bound
/// and lookup surfaces share.
pub const RIGGER_GROUND_QUERY: &str = "the natural-language query";

/// CONTRACT: the `rigger_graph` description the spawn-bound and lookup surfaces share.
pub const RIGGER_GRAPH: &str =
    "The STRUCTURE and resolution lookups: pass `show` <entity> for its definition site (same as `rigger graph --show <entity>`), or `around` <file|entity> (optionally `depth`) for its structural neighborhood (same as `rigger graph --around <file|entity> --depth <n>`). Pass exactly one of `show`/`around`.";

/// CONTRACT: the description of `rigger_graph`'s `depth` argument, which the spawn-bound
/// and lookup surfaces share.
pub const RIGGER_GRAPH_DEPTH: &str = "neighborhood depth for `around` (default 2)";

/// CONTRACT: the spawn-bound surface's `rigger_progress` description.
pub const SPAWN_BOUND_RIGGER_PROGRESS: &str =
    "Record one live progress line for the spawn this server is bound to, so an observer sees you working between milestones.";

/// CONTRACT: the spawn-bound surface's description of `rigger_progress`'s
/// `activity` argument.
pub const SPAWN_BOUND_RIGGER_PROGRESS_ACTIVITY: &str =
    "a short one-line description of what you just did";

/// CONTRACT: the spawn-bound surface's `rigger_scratch` description.
pub const SPAWN_BOUND_RIGGER_SCRATCH: &str =
    "Your own rigger-assigned scratch container - the one directory every probe repo, verification worktree, test build, or build cache you create must live under, never /tmp or a session scratchpad. Reaped once your result records.";

/// CONTRACT: the workflow surface's `rigger_next` description.
pub const WORKFLOW_RIGGER_NEXT: &str =
    "Pick up the next queued agent spawn. The id is empty when nothing is waiting.";

/// CONTRACT: the workflow surface's `rigger_result` description.
pub const WORKFLOW_RIGGER_RESULT: &str = "Report an agent's final result by spawn id.";

/// CONTRACT: the workflow surface's `rigger_emit` description.
pub const WORKFLOW_RIGGER_EMIT: &str =
    "Record a decision on the shared event log, live, so other agents see it immediately. Optionally set meta (e.g. the acting agent, which stamps the graph's DECIDED edge) and valid_from (the bi-temporal time the fact became true).";

/// CONTRACT: the workflow surface's description of `rigger_emit`'s `meta` argument.
pub const WORKFLOW_RIGGER_EMIT_META: &str =
    "Metadata entries (string->string), e.g. {\"actor\": \"<agent-id>\"}.";

/// CONTRACT: the workflow surface's `rigger_peers` description.
pub const WORKFLOW_RIGGER_PEERS: &str =
    "List the decisions, lessons, AND review findings other agents have recorded so far, this run's and earlier runs' (each decision labeled live or historical), so you do not work blind to them (concurrent reviewers see each other's findings live). Each section returns its newest records under a byte budget; when any are left out, `left_out` counts them and says how to read one by id. Pass `files` (your blast-radius) to scope the result to decisions, lessons, and findings that touch those files; omit it for every file.";

/// CONTRACT: the workflow surface's description of `rigger_peers`'s `files` argument.
pub const WORKFLOW_RIGGER_PEERS_FILES: &str =
    "The agent's blast-radius: only decisions whose `governs`, and lessons and findings whose `about`, name one of these files (a `path:line` citation names its file) are returned. Omit for all.";

/// CONTRACT: the workflow surface's `rigger_activity` description.
pub const WORKFLOW_RIGGER_ACTIVITY: &str =
    "The live per-agent view of the current run: one entry per in-flight agent with its stage, latest activity, how long since that activity and its last heartbeat, and its last event-store milestone (and how long ago - the blackout this fills). Rigger consolidates the run stream, the progress store, and the liveness markers and presents them here, so you get up-to-the-second agent state with no filesystem access of your own.";

/// The dash's groups by surface, the spawn-bound, lookup and workflow tool lists in turn, each in
/// the document order of the descriptions `tool_list` advertises on it; a shared const is a group
/// on each surface that advertises it. Its readers are tests, so it is compiled for tests alone.
#[cfg(test)]
pub fn prompt_groups() -> &'static [rigger_domain::prompt_surface::Group] {
    rigger_domain::prompt_groups![
        "mcpserver::tool_list::SpawnBound" => [
            SPAWN_BOUND_RIGGER_EMIT,
            SPAWN_BOUND_RIGGER_EMIT_META,
            RIGGER_EMIT_VALID_FROM,
            RIGGER_PEERS,
            RIGGER_GROUND,
            RIGGER_GROUND_K,
            RIGGER_GROUND_QUERY,
            RIGGER_GRAPH,
            RIGGER_GRAPH_DEPTH,
            SPAWN_BOUND_RIGGER_PROGRESS,
            SPAWN_BOUND_RIGGER_PROGRESS_ACTIVITY,
            SPAWN_BOUND_RIGGER_SCRATCH,
        ],
        "mcpserver::tool_list::Lookup" => [
            RIGGER_PEERS,
            RIGGER_GROUND,
            RIGGER_GROUND_K,
            RIGGER_GROUND_QUERY,
            RIGGER_GRAPH,
            RIGGER_GRAPH_DEPTH,
        ],
        "mcpserver::tool_list::Workflow" => [
            WORKFLOW_RIGGER_NEXT,
            WORKFLOW_RIGGER_RESULT,
            WORKFLOW_RIGGER_EMIT,
            WORKFLOW_RIGGER_EMIT_META,
            RIGGER_EMIT_VALID_FROM,
            WORKFLOW_RIGGER_PEERS,
            WORKFLOW_RIGGER_PEERS_FILES,
            WORKFLOW_RIGGER_ACTIVITY,
        ],
    ]
}
