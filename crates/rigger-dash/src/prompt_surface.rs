//! The dash's prompt texts (spec 121, criterion 16): every tool description and `inputSchema`
//! property description the MCP server's `tool_list` advertises on its two surfaces, each one
//! `pub const` whose doc comment opens with its class, CONTRACT, since each is the advertised
//! meaning of its tool or argument. THE NAMING RULE names them: a text two or more surfaces
//! advertise word for word is one shared const, `<TOOL>` for a tool's description and
//! `<TOOL>_<ARG>` for a property's; any other text is `<SURFACE>_<TOOL>` or
//! `<SURFACE>_<TOOL>_<ARG>`, its surface prefix `SPAWN_BOUND` or `LOOKUP`. No text
//! carries a slot. The registry types are `rigger_domain::prompt_surface`'s. The module is pure
//! consts and compiles in every lane.

/// CONTRACT: the spawn-bound surface's `rigger_emit` description: the record is stamped
/// with the bound spawn.
pub const SPAWN_BOUND_RIGGER_EMIT: &str =
    "Record a decision on the shared event log, live, so other agents see it immediately. Stamped with the spawn this server is bound to; optionally set valid_from (the bi-temporal time the fact became true).";

/// CONTRACT: the spawn-bound surface's description of `rigger_emit`'s `meta` argument.
pub const SPAWN_BOUND_RIGGER_EMIT_META: &str =
    "Metadata entries (string->string). meta.spawn, if set, must name THIS server's own bound spawn.";

/// CONTRACT: the spawn-bound surface's description of `rigger_emit`'s `valid_from` argument.
pub const SPAWN_BOUND_RIGGER_EMIT_VALID_FROM: &str =
    "When the fact became true: unix nanoseconds (integer) or an RFC3339 timestamp string.";

/// CONTRACT: the spawn-bound surface's `rigger_peers` description: the bound spawn's own
/// unit's records rank first.
pub const SPAWN_BOUND_RIGGER_PEERS: &str =
    "List the decisions, lessons, AND review findings recorded so far, this run's and earlier runs', so you do not work blind to them. Each record is one line, `<kind> <id> | LIVE|HISTORICAL | [by <reviewer> | ]<summary>`, best first: your own unit's records, then this run's, then those naming more of `files`, then the newest, with reviewer verdicts last. The reply stays under one size budget; when records are left out, `left_out` counts them and says how to read one by id (`rigger_graph` `show` <id> returns a whole record). Pass `files` to scope the result to decisions, lessons, and findings that touch those files; omit it for every file.";

/// CONTRACT: the lookup surface's `rigger_peers` description: with no spawn bound, this run's
/// records rank first.
pub const LOOKUP_RIGGER_PEERS: &str =
    "List the decisions, lessons, AND review findings recorded so far, this run's and earlier runs', so you do not work blind to them. Each record is one line, `<kind> <id> | LIVE|HISTORICAL | [by <reviewer> | ]<summary>`, best first: this run's records, then those naming more of `files`, then the newest, with reviewer verdicts last. The reply stays under one size budget; when records are left out, `left_out` counts them and says how to read one by id (`rigger_graph` `show` <id> returns a whole record). Pass `files` to scope the result to decisions, lessons, and findings that touch those files; omit it for every file.";

/// CONTRACT: the `rigger_ground` description the spawn-bound and lookup surfaces share.
pub const RIGGER_GROUND: &str =
    "The MEMORY-adjacent intent lookup (spec 92): rank code entities by relevance to a natural-language query. Same as `rigger ground \"<query>\" [<k>]`.";

/// CONTRACT: the description of `rigger_ground`'s `k` argument, which the spawn-bound and
/// lookup surfaces share.
pub const RIGGER_GROUND_K: &str = "how many results (default 8)";

/// CONTRACT: the description of `rigger_ground`'s `query` argument, which the spawn-bound
/// and lookup surfaces share.
pub const RIGGER_GROUND_QUERY: &str = "the natural-language query";

/// CONTRACT: the description of `rigger_ground`'s `tests` argument, which the spawn-bound and
/// lookup surfaces share: the text of `rigger_graph`'s, since both tools take the one `tests`
/// setting.
pub const RIGGER_GROUND_TESTS: &str = RIGGER_GRAPH_TESTS;

/// CONTRACT: the `rigger_graph` description the spawn-bound and lookup surfaces share. It names
/// the tool's three forms and their arguments, the CLI form each is the same as, the sections a
/// `lookup` reply carries and the tree it answers from.
pub const RIGGER_GRAPH: &str =
    "Three lookups in one tool; pass exactly one of `lookup`, `show` or \
`around`. `lookup` <term>... answers each term from three sources in one reply, each section \
labelled by its source: `graph` (each entity the term names, its definition site `file:line`, its \
callers and callees; a term resolves exactly, by case or separator variant, by prefix or fuzzily, \
and one that resolves to no entity is answered with the nearest names), `text` (every line of your \
working tree holding the term, scanned live) and `prose:<kind>` (specs, their criteria, personas, \
instructions, skills, docs, decisions, lessons and findings, ranked by relevance). Every section \
answers from your own working tree: the graph takes each file that differs there from the \
commit it was built at from your tree, and the label heading the section says so; a tree that \
cannot be compared withholds every graph hit and counts them in the label. Same as `rigger graph --lookup <term>...`. `show` \
<entity> gives its definition site (same as `rigger graph --show <entity>`); `around` \
<file|entity> (optionally `depth`) its structural neighborhood (same as `rigger graph --around \
<file|entity> --depth <n>`). Worked examples - who calls <name>: pass `around` <name>; where is \
<name> defined: pass `show` <name>.";

/// CONTRACT: the description of `rigger_graph`'s `depth` argument, which the spawn-bound
/// and lookup surfaces share.
pub const RIGGER_GRAPH_DEPTH: &str = "neighborhood depth for `around` (default 2)";

/// CONTRACT: the description of `rigger_graph`'s `lookup` argument, which the spawn-bound and
/// lookup surfaces share.
pub const RIGGER_GRAPH_LOOKUP: &str =
    "one or more terms: names, paths or words; each is answered in its own group of every section";

/// CONTRACT: the description of `rigger_graph`'s `tests` argument, which the spawn-bound and
/// lookup surfaces share.
pub const RIGGER_GRAPH_TESTS: &str =
    "whether test code is shown: `exclude` (the default), `include` or `only`";

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

/// The dash's groups by surface, the spawn-bound and lookup tool lists in turn, each in
/// the document order of the descriptions `tool_list` advertises on it; a shared const is a group
/// on each surface that advertises it. Its readers are tests, so it is compiled for tests alone.
#[cfg(test)]
pub fn prompt_groups() -> &'static [rigger_domain::prompt_surface::Group] {
    rigger_domain::prompt_groups![
        "mcpserver::tool_list::SpawnBound" => [
            SPAWN_BOUND_RIGGER_EMIT,
            SPAWN_BOUND_RIGGER_EMIT_META,
            SPAWN_BOUND_RIGGER_EMIT_VALID_FROM,
            SPAWN_BOUND_RIGGER_PEERS,
            RIGGER_GROUND,
            RIGGER_GROUND_K,
            RIGGER_GROUND_QUERY,
            RIGGER_GROUND_TESTS,
            RIGGER_GRAPH,
            RIGGER_GRAPH_DEPTH,
            RIGGER_GRAPH_LOOKUP,
            RIGGER_GRAPH_TESTS,
            SPAWN_BOUND_RIGGER_PROGRESS,
            SPAWN_BOUND_RIGGER_PROGRESS_ACTIVITY,
            SPAWN_BOUND_RIGGER_SCRATCH,
        ],
        "mcpserver::tool_list::Lookup" => [
            LOOKUP_RIGGER_PEERS,
            RIGGER_GROUND,
            RIGGER_GROUND_K,
            RIGGER_GROUND_QUERY,
            RIGGER_GROUND_TESTS,
            RIGGER_GRAPH,
            RIGGER_GRAPH_DEPTH,
            RIGGER_GRAPH_LOOKUP,
            RIGGER_GRAPH_TESTS,
        ],
    ]
}
