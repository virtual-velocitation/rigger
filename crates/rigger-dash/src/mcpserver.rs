//! Serves rigger's tools over MCP (JSON-RPC 2.0 on stdio) on two surfaces: the
//! operator's read-only lookup surface (`rigger mcp`: peers, ground, graph) and a
//! launched agent's own spawn-bound surface (`rigger mcp --spawn <id>`: emit, peers,
//! ground, graph, progress, scratch). A plain newline-delimited stdio loop - no async
//! runtime needed.

use std::io::{BufRead, Write};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde_json::{json, Value};

use crate::contextgraph::query::whole_graph_degree;
use crate::contextgraph::{
    entity_site, open_unowed, site_fields, wired, EntitySite, Fold, Located, OpenGraph, RecordFold,
};
use crate::eventstore::{Event, EventStore, ExpectedRevision};
use crate::grounder::Grounder;
use crate::sidecar::{Peer, PeerDecision, PeerFinding, PeerLesson, RankedPeer, Sidecar};
use rigger_domain::code_view::{Tests, ViewFilter};
use rigger_domain::lookup::Lookup;

/// A tool's failure, carrying the JSON-RPC error code to report it with. Most
/// failures are internal (`-32603`); a bad argument (e.g. an unknown spawn id)
/// is invalid-params (`-32602`).
#[derive(Debug)]
struct ToolError {
    code: i64,
    message: String,
}

/// JSON-RPC's internal-error code: something went wrong server-side.
const INTERNAL_ERROR: i64 = -32603;
/// JSON-RPC's invalid-params code: the caller's arguments were wrong (e.g. a stale/unknown
/// spawn id, a missing required field).
const INVALID_PARAMS: i64 = -32602;

impl ToolError {
    /// A tool failure reported with JSON-RPC `code` ([`INTERNAL_ERROR`] or [`INVALID_PARAMS`]).
    fn new(code: i64, message: impl Into<String>) -> Self {
        ToolError {
            code,
            message: message.into(),
        }
    }
}

impl From<String> for ToolError {
    fn from(message: String) -> Self {
        Self::new(INTERNAL_ERROR, message)
    }
}

impl From<&str> for ToolError {
    fn from(message: &str) -> Self {
        Self::new(INTERNAL_ERROR, message)
    }
}

/// The MCP server over the event store, side-car, and (optionally) the context-graph
/// projector, the grounder, and the progress store.
pub struct Server<'a> {
    store: &'a dyn EventStore,
    stream: String,
    /// The live context-graph projector. When set, an emitted event is folded into
    /// the graph the moment it is appended - so a ReviewFinding (or DecisionMade) an
    /// agent emits via rigger_emit becomes retrievable through `graph_context` by the
    /// agents that ground afterwards (the adversary / adjudicator). Without this, an
    /// emit would write findings only to the log and the side-car, and the graph - the
    /// system's cross-agent memory - would never see them.
    graph: Option<&'a dyn RecordFold>,
    /// Opens the context graph `rigger_graph` reads, once per call, wired via
    /// [`with_context`](Server::with_context). `None` when none is wired: `rigger_graph` then
    /// answers [`NO_GRAPH_WIRED`].
    context: Option<&'a dyn OpenGraph>,
    /// The read-only grounder port `rigger_ground` answers through (spec 92, criterion 4's fix
    /// round), wired via [`with_grounder`] (or a resolution failure recorded through its `Err`
    /// arm) - the SAME port `rigger ground` resolves, so one dispatch and read loop answers
    /// both tool surfaces rather than a second parallel MCP loop reaching for the concrete
    /// grounder across the crate boundary. Both `None` when no grounder is wired:
    /// `rigger_ground` then reports that as its own tool-call error.
    grounder: Option<&'a dyn Grounder>,
    /// Set instead of [`grounder`](Server::grounder) when the caller's own grounder resolution
    /// FAILED (e.g. `--no-default-features` with no `defaults.grounder` pinned, spec 57's
    /// never-silently-degrade grounder-selection contract) - via
    /// [`with_grounder`]'s `Err` arm. Peers and graph keep answering (a grounder
    /// misconfiguration is not a reason to refuse the whole server), but
    /// `rigger_ground` reports this reason as an honest tool-call error instead of silently
    /// returning empty results, exactly as the pre-fix lazy resolution did (a
    /// process-wide startup failure here would be a regression: it would take `rigger_peers`
    /// and `rigger_graph` down over a `rigger_ground`-only misconfiguration).
    grounder_unavailable: Option<String>,
    /// The combined lookup `rigger_graph`'s `lookup` answers through, over the caller's own
    /// working tree, wired via [`with_lookup`](Server::with_lookup). `None` when none is wired:
    /// `lookup` then reports that as its own tool-call error.
    lookup: Option<&'a dyn Lookup>,
    /// The SEPARATE progress store (spec 14) `rigger_progress` records the bound spawn's
    /// activity lines to. `None` on a server started without one - `rigger_progress` then
    /// reports that as its own tool-call error.
    progress: Option<&'a dyn EventStore>,
    /// The scratch root `rigger_scratch` resolves the bound spawn's container under. Empty
    /// when unknown - `rigger_scratch` then reports that as its own tool-call error.
    scratch_root: String,
    /// The id this `Server` is BOUND to (spec 104, `rigger mcp --spawn <id>`), set once at
    /// construction via [`with_spawn`](Server::with_spawn) - the launched agent's own MCP
    /// server, `--strict-mcp-config` naming this exact invocation, so every record it writes
    /// is attributed BY CONSTRUCTION, never a runtime guess. `Some` marks this `Server` as
    /// the spawn-bound surface (see [`Surface`]): [`tool_list`](Server::tool_list) and
    /// [`call_tool`](Server::call_tool) both read it through [`surface`](Server::surface), so
    /// the two can never disagree on which tools this instance serves. It is fixed for the
    /// server's whole lifetime - one spawn, one child process, one MCP server, no shared file.
    spawn: Option<String>,
}

/// The tool surface a [`Server`] instance answers - computed once by
/// [`surface`](Server::surface) so [`tool_list`](Server::tool_list) and
/// [`call_tool`](Server::call_tool) can never drift on which tools an instance serves.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Surface {
    /// The default, no spawn bound (spec 92, criterion 4): the operator's read-only lookup
    /// surface (`rigger_peers`/`rigger_ground`/`rigger_graph`), reached by `rigger mcp` with
    /// no `--spawn`.
    Lookup,
    /// A spawn bound (spec 104, criterion 3): the launched agent's own MCP server
    /// (`rigger_emit`/`rigger_peers`/`rigger_ground`/`rigger_graph`/`rigger_progress`/
    /// `rigger_scratch`), reached by `rigger mcp --spawn <id>` - always the write-scoped six,
    /// never the narrower three.
    SpawnBound,
}

impl<'a> Server<'a> {
    pub fn new(store: &'a dyn EventStore, stream: &str) -> Self {
        Server {
            store,
            stream: stream.to_string(),
            graph: None,
            context: None,
            grounder: None,
            grounder_unavailable: None,
            lookup: None,
            progress: None,
            scratch_root: String::new(),
            spawn: None,
        }
    }

    /// Wire the progress store + scratch root the spawn-bound `rigger_progress` writes to and
    /// `rigger_scratch` resolves under (spec 14, spec 104).
    pub fn with_progress(mut self, progress: &'a dyn EventStore, scratch_root: &str) -> Self {
        self.progress = Some(progress);
        self.scratch_root = scratch_root.to_string();
        self
    }

    /// Wire the record graph so emitted events fold into it as they are appended (rigger_emit's
    /// bridge to the graph, mirroring the conductor's own `emit_with_actor`), and so the tools
    /// that depend on the fold can tell whether it owes its rebuild.
    pub fn with_graph(mut self, graph: &'a dyn RecordFold) -> Self {
        self.graph = Some(graph);
        self
    }

    /// Wire the opener of the context graph `rigger_graph` reads.
    pub fn with_context(mut self, context: &'a dyn OpenGraph) -> Self {
        self.context = Some(context);
        self
    }

    /// Wire the operator's own read-only grounder port (spec 92, criterion 4's fix round) - or
    /// the reason resolving it FAILED. `Ok`: `rigger_ground` answers through it, over the SAME
    /// `Grounder` trait `rigger ground` resolves via `select_grounder` - so ground's ranking
    /// (spec 92 criterion 3's territory) is inherited automatically, never re-implemented here.
    /// `Err` (a caller unable to produce a working `&dyn Grounder`, e.g. `select_grounder`
    /// erred) passes the resolution failure's message instead (see
    /// [`grounder_unavailable`](Server::grounder_unavailable)'s doc comment), so
    /// `rigger_peers`/`rigger_graph` still answer normally and only `rigger_ground` reports the
    /// reason as its own tool-call error.
    pub fn with_grounder(mut self, grounder: Result<&'a dyn Grounder, String>) -> Self {
        match grounder {
            Ok(g) => self.grounder = Some(g),
            Err(reason) => self.grounder_unavailable = Some(reason),
        }
        self
    }

    /// Wire the combined lookup `rigger_graph`'s `lookup` answers through: the one engine the
    /// CLI's `rigger graph --lookup` answers through too, rooted at the caller's working tree.
    pub fn with_lookup(mut self, lookup: &'a dyn Lookup) -> Self {
        self.lookup = Some(lookup);
        self
    }

    /// Bind this `Server` to one spawn (spec 104, `rigger mcp --spawn <id>`): the launched
    /// agent's own MCP server, so [`tool_list`](Server::tool_list)/
    /// [`call_tool`](Server::call_tool) switch to the spawn-bound surface's six tools (see
    /// [`Surface::SpawnBound`]) and every write it serves is attributed to `id` by
    /// construction. Pair with [`with_progress`](Server::with_progress) so
    /// `rigger_progress`/`rigger_scratch` have a store/root to write to and read from.
    pub fn with_spawn(mut self, id: impl Into<String>) -> Self {
        self.spawn = Some(id.into());
        self
    }

    /// Serve MCP over the given streams until the input closes (the client's stdin).
    pub fn run(&self, input: impl BufRead, mut output: impl Write) -> std::io::Result<()> {
        for line in input.lines() {
            let line = line?;
            if line.trim().is_empty() {
                continue;
            }
            // Unparseable input is a JSON-RPC parse error (-32700): reply rather
            // than `continue`, which would silently drop the message and hang a
            // client that is waiting for a response. The id is null because we
            // could not parse the message to recover it (spec §5.1).
            let response = match serde_json::from_str::<Value>(&line) {
                Ok(msg) => self.handle(&msg),
                Err(_) => Some(err(Value::Null, -32700, "parse error")),
            };
            if let Some(response) = response {
                writeln!(output, "{response}")?;
                output.flush()?;
            }
        }
        Ok(())
    }

    fn handle(&self, msg: &Value) -> Option<String> {
        // A JSON-RPC request MUST carry a string `method`. A notification is a
        // request with no `id` and needs no response; a request with an `id` but
        // no usable `method` is malformed and gets an Invalid Request error
        // (-32600), echoing the id when present (spec §4 / §5.1).
        let id = msg.get("id").cloned();
        let method = match msg.get("method").and_then(Value::as_str) {
            Some(m) => m,
            None => {
                return Some(err(
                    id.unwrap_or(Value::Null),
                    -32600,
                    "invalid request: missing method",
                ));
            }
        };
        match method {
            "initialize" => id.map(|id| {
                ok(
                    id,
                    json!({
                        "protocolVersion": "2024-11-05",
                        "capabilities": {"tools": {}},
                        "serverInfo": {"name": "rigger", "version": "0.1.0"},
                    }),
                )
            }),
            "tools/list" => id.map(|id| ok(id, json!({"tools": self.tool_list()}))),
            "tools/call" => {
                // tools/call is a request, so it must carry an id; without one it
                // is treated as a malformed notification and dropped.
                let id = id?;
                // A tools/call missing params or the tool name is an Invalid
                // Params error (-32602): reply rather than drop, so the client is
                // not left hanging on a request it can never get an answer to.
                let name = match msg
                    .get("params")
                    .and_then(|p| p.get("name"))
                    .and_then(Value::as_str)
                {
                    Some(n) => n,
                    None => {
                        return Some(err(
                            id,
                            -32602,
                            "invalid params: tools/call requires params.name",
                        ));
                    }
                };
                let args = msg
                    .get("params")
                    .and_then(|p| p.get("arguments"))
                    .cloned()
                    .unwrap_or_else(|| json!({}));
                Some(self.call_tool(id, name, &args))
            }
            // Other methods with an id are unknown requests (-32601); methods
            // without an id are notifications (initialized, etc.) and stay silent.
            _ => id.map(|id| err(id, -32601, &format!("method not found: {method}"))),
        }
    }

    /// The ONE authority for which [`Surface`] this instance answers as - see
    /// [`spawn`](Server::spawn)'s and [`Surface`]'s doc comments for the ordering. Both
    /// [`tool_list`] and [`call_tool`] read it, so the advertised list can never drift from
    /// what is actually dispatchable.
    fn surface(&self) -> Surface {
        if self.spawn.is_some() {
            Surface::SpawnBound
        } else {
            Surface::Lookup
        }
    }

    fn call_tool(&self, id: Value, name: &str, args: &Value) -> String {
        let surface = self.surface();
        let result = match (surface, name) {
            (_, "rigger_peers") => self.tool_peers(args),
            (_, "rigger_ground") => self.tool_ground(args),
            (_, "rigger_graph") => self.tool_graph(args),
            (Surface::SpawnBound, "rigger_emit") => self.tool_emit_bound(args),
            (Surface::SpawnBound, "rigger_progress") => self.tool_progress(args),
            (Surface::SpawnBound, "rigger_scratch") => self.tool_scratch(),
            _ => Err(ToolError::new(
                INVALID_PARAMS,
                format!("unknown tool {name}"),
            )),
        };
        // Every reply leaves through [`within_frame`], the server's one bound on a tool reply's
        // frame: an oversized result drops its structured copy and cuts its text, an oversized
        // error cuts its message, each with a marked tail.
        match result {
            // A reply that is text, the combined lookup's, is the content's text alone: it has no
            // structured form to carry.
            Ok(Value::String(text)) => within_frame(&text, |text| {
                ok(
                    id.clone(),
                    json!({"content": [{"type": "text", "text": text}]}),
                )
            }),
            Ok(structured) => {
                let text = structured.to_string();
                let whole = ok(
                    id.clone(),
                    json!({
                        "content": [{"type": "text", "text": text}],
                        "structuredContent": structured,
                    }),
                );
                if whole.len() <= TOOL_REPLY_FRAME_BUDGET_BYTES {
                    return whole;
                }
                within_frame(&text, |text| {
                    ok(
                        id.clone(),
                        json!({"content": [{"type": "text", "text": text}]}),
                    )
                })
            }
            // A tool may request a specific JSON-RPC code (e.g. -32602 for an
            // unknown spawn id); otherwise an internal error is -32603.
            Err(e) => within_frame(&e.message, |message| err(id.clone(), e.code, message)),
        }
    }

    /// Emit `args` through the one emit core into this server's store and graph, answering the
    /// position the store issued and whether the event folded, with the reason it did not - an
    /// event whose fold failed on a current graph is re-derived by no rebuild, so the agent must
    /// be told.
    fn emit(&self, args: &Value) -> Result<Value, ToolError> {
        let emitted = emit_event(self.store, &self.stream, || wired(self.graph), args)?;
        Ok(match emitted.fold {
            Fold::Folded => json!({"position": emitted.position, "folded": true}),
            Fold::NotFolded(reason) => {
                json!({"position": emitted.position, "folded": false, "reason": reason})
            }
        })
    }

    /// `rigger_emit` on the SPAWN-BOUND surface (spec 104, criterion 3): stamps `meta.spawn`
    /// with the id this server is BOUND to ([`with_spawn`](Server::with_spawn)) - ATTRIBUTION
    /// BY CONSTRUCTION. A write NAMING a DIFFERENT spawn is REFUSED outright, never silently
    /// corrected: the generic `meta` pass-through this tool's schema carries is the one
    /// vector through which a spawn-bound session could otherwise misattribute a decision to
    /// a sibling spawn it shares no memory with. Naming the server's OWN bound spawn is not
    /// "another spawn" - it is accepted exactly like an unstamped emit. The stamp itself goes
    /// through [`stamp_spawn_meta`](Server::stamp_spawn_meta), so a present-but-non-object
    /// `meta` (e.g. a stray `meta: null`) can never silently bypass it.
    fn tool_emit_bound(&self, args: &Value) -> Result<Value, ToolError> {
        let bound = self.bound_spawn();
        if let Some(named) = args
            .get("meta")
            .and_then(|m| m.get(crate::conductor::META_SPAWN))
            .and_then(Value::as_str)
        {
            if named != bound {
                return Err(ToolError::new(
                    INVALID_PARAMS,
                    format!(
                        "rigger_emit: refusing to emit on behalf of spawn {named:?}: this \
                     server is bound to spawn {bound:?}"
                    ),
                ));
            }
        }
        let mut stamped = args.clone();
        let obj = stamped
            .as_object_mut()
            .ok_or("rigger_emit: arguments must be a JSON object")?;
        Self::stamp_spawn_meta(obj, bound);
        self.emit(&stamped)
    }

    /// Insert `meta.spawn = spawn` into an args object's `meta` field. A `meta` that is present
    /// but NOT an object (e.g. a stray `meta: null`, `[]`, or `"x"` a caller sends) is forced
    /// to a fresh empty object first: `obj.entry("meta").or_insert_with(...)` alone is not
    /// enough, because it only runs its closure on a VACANT entry, so an occupied non-object
    /// `meta` would leave the stamp silently skipped - the exact attribution bypass this
    /// helper exists to close.
    fn stamp_spawn_meta(obj: &mut serde_json::Map<String, Value>, spawn: &str) {
        let meta = obj.entry("meta").or_insert_with(|| json!({}));
        if !meta.is_object() {
            *meta = json!({});
        }
        meta.as_object_mut()
            .expect("meta was just forced to a JSON object")
            .insert(
                crate::conductor::META_SPAWN.to_string(),
                Value::String(spawn.to_string()),
            );
    }

    /// The id this server is bound to. Panics if called off the spawn-bound surface - every
    /// caller ([`tool_emit_bound`], [`tool_progress`], [`tool_scratch`]) is reachable ONLY
    /// through [`call_tool`]'s `Surface::SpawnBound` arms, which exist only when
    /// [`with_spawn`](Server::with_spawn) set this field, so the invariant always holds by
    /// construction.
    fn bound_spawn(&self) -> &str {
        self.spawn.as_deref().expect(
            "tool_emit_bound/tool_progress/tool_scratch are dispatched only on the \
             spawn-bound surface, which always carries a bound id",
        )
    }

    /// The current run a tool folds and its id, read fresh from its boundary with the
    /// carried-over knowledge by type ([`crate::run::read::read_current_run`], spec 101) - never
    /// the whole log.
    fn read_current_run(&self) -> Result<(Vec<Event>, String), ToolError> {
        crate::run::read::read_current_run(self.store, &self.stream)
            .map_err(|e| ToolError::new(INTERNAL_ERROR, e.to_string()))
    }

    /// The current run's id, resolved fresh from the run stream, for the two spawn-bound tools
    /// ([`tool_progress`], [`tool_scratch`]) that need it to resolve a per-run record/path.
    fn current_run_id(&self) -> Result<String, ToolError> {
        Ok(self.read_current_run()?.1)
    }

    /// `rigger_progress` (spec 104's spawn MCP server, addendum §4.3): record one live
    /// activity line for the BOUND spawn to the SAME progress store + record shape
    /// `rigger progress <id> "<activity>"` already uses ([`crate::progress_store::record`]) - never a second parallel writer. No
    /// spawn-naming argument exists on this tool's schema at all: attribution is by
    /// construction, with nothing to refuse.
    fn tool_progress(&self, args: &Value) -> Result<Value, ToolError> {
        let activity = args
            .get("activity")
            .and_then(Value::as_str)
            .ok_or("rigger_progress: missing activity")?;
        if activity.trim().is_empty() {
            return Err("rigger_progress: activity must be non-empty".into());
        }
        let progress = self
            .progress
            .ok_or("rigger_progress: no progress store is wired on this server")?;
        let bound = self.bound_spawn().to_string();
        let run_id = self.current_run_id()?;
        crate::progress_store::record(progress, &run_id, &bound, activity)
            .map(|_| json!({}))
            .map_err(|e| ToolError::new(INTERNAL_ERROR, e.to_string()))
    }

    /// `rigger_scratch` (spec 104's spawn MCP server, addendum §4.3): the BOUND spawn's own
    /// rigger-assigned scratch container - the exact path
    /// [`crate::driver::replay::spawn_scratch_path`] resolves and the per-spawn reclaim reaps
    /// at its terminus, over the `scratch_root` [`with_progress`](Server::with_progress)
    /// wires. No argument at all: the
    /// spawn-bound server can name no OTHER spawn's scratch, by construction.
    fn tool_scratch(&self) -> Result<Value, ToolError> {
        if self.scratch_root.is_empty() {
            return Err("rigger_scratch: no scratch root is wired on this server".into());
        }
        let bound = self.bound_spawn().to_string();
        let run_id = self.current_run_id()?;
        let path = crate::driver::replay::spawn_scratch_path(&self.scratch_root, &run_id, &bound)
            .ok_or_else(|| {
            ToolError::new(
                INTERNAL_ERROR,
                format!("rigger_scratch: {bound:?} does not name a usable scratch path"),
            )
        })?;
        Ok(json!({"path": path.display().to_string()}))
    }

    /// List peers' decisions, lessons, AND review findings, optionally scoped to a
    /// blast-radius (§5.3). When the caller passes a `files` array (the agent's
    /// blast-radius), only decisions whose `governs`, lessons whose `about`, and findings
    /// whose `about` intersect it come back; absent or empty, every one does - ranked for the
    /// spawn this server is bound to (none on the other surfaces) and kept under one budget
    /// (see [`peers_json`]). The findings are how concurrent review
    /// lenses see each other's findings LIVE, before any of them grounds again - the same
    /// side-car channel that surfaces peer decisions and lessons.
    fn tool_peers(&self, args: &Value) -> Result<Value, ToolError> {
        let files: Vec<String> = args
            .get("files")
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default();
        let peers = Sidecar::read(self.store, &self.stream)
            .map_err(|e| ToolError::new(INTERNAL_ERROR, e.to_string()))?;
        Ok(peers_json(&peers, &files, self.spawn.as_deref()))
    }

    /// `rigger_ground` (spec 92, criterion 4's fix round): the operator's own MEMORY-adjacent
    /// intent lookup, over the SAME [`Grounder`] port [`with_grounder`](Server::with_grounder)
    /// wires - the exact trait `rigger ground` resolves through `select_grounder`, so ground's
    /// ranking (spec 92 criterion 3's territory) is inherited automatically, never
    /// re-implemented here. Argument validation runs BEFORE the grounder-unavailable check, so
    /// a genuinely missing `query` is always reported as that - never masked by an unrelated
    /// resolution failure.
    fn tool_ground(&self, args: &Value) -> Result<Value, ToolError> {
        let query = args
            .get("query")
            .and_then(Value::as_str)
            .ok_or("rigger_ground: missing query")?;
        let k = args.get("k").and_then(Value::as_u64).unwrap_or(8) as usize;
        self.refuse_if_rebuild_owed("rigger_ground")?;
        if let Some(reason) = &self.grounder_unavailable {
            // Honest, lazy failure (spec 57's never-silently-degrade contract) - exactly what
            // the pre-fix operator surface reported when `select_grounder` erred, just now
            // reached through the shared `Server` instead of a second read loop. Never a
            // silent empty results array: `Grounder::ground` cannot itself signal failure, so
            // this is the ONLY channel that reports it.
            return Err(format!("rigger_ground: {reason}").into());
        }
        let grounder = self
            .grounder
            .ok_or("rigger_ground: no grounder is wired on this server")?;
        // The rows and their honesty label (6.1) come from one read of the code store.
        let (rows, label) = grounder.ground_labelled(query, k);
        let results: Vec<Value> = rows
            .into_iter()
            .map(|r| json!({"file": r.file, "line": r.line, "text": r.text}))
            .collect();
        Ok(json!({"results": results, "label": label.map(|l| l.to_string())}))
    }

    /// Refuse `tool`, whose answer depends on the fold, while the wired graph owes its rebuild
    /// (spec 101): at once, naming `rigger setup`, never answering from a projection folded under
    /// an older rule. Asked on every call, so a rebuild paid while this server runs is seen.
    fn refuse_if_rebuild_owed(&self, tool: &str) -> Result<(), ToolError> {
        let owed = match self.graph {
            Some(graph) => graph.rebuild_owed().map_err(|e| e.to_string())?,
            None => false,
        };
        if owed {
            return Err(crate::contextgraph::rebuild_owed_refusal(tool).into());
        }
        Ok(())
    }

    /// `rigger_graph` (spec 92, criterion 4's fix round): the combined `lookup`, and the
    /// STRUCTURE (`around`) and resolution (`show`) lookups. `lookup` answers through the wired
    /// [`Lookup`], which states in its own graph section why the graph did not answer (none wired,
    /// a rebuild owed), so it is never refused while its text and prose halves can answer, and it
    /// answers as text. `around` and `show` read the graph [`with_context`](Server::with_context)
    /// opens, under the call's `tests` filter - `around` serves [`crate::contextgraph::around`]
    /// (the graph's subgraph plus the seed's callers), `show` its `locate` - and each answers its
    /// honesty `label` (6.1) as a field. Both render a node through [`site_object`] (spec 119, SHOW
    /// IS ONE RENDERER): `show` the located site whole, `around` every node of the loaded `Graph`
    /// by [`entity_site`] at its edge count there, less the [`AROUND_OMITTED`] `summary` entry.
    fn tool_graph(&self, args: &Value) -> Result<Value, ToolError> {
        let terms = lookup_terms(args.get("lookup"));
        let show = args.get("show").and_then(Value::as_str).unwrap_or("");
        let around = args.get("around").and_then(Value::as_str).unwrap_or("");
        let depth = args.get("depth").and_then(Value::as_i64).unwrap_or(2);
        if terms.is_empty() && show.is_empty() && around.is_empty() {
            return Err(GRAPH_ARGUMENTS_MISSING.into());
        }
        let filter = tests_filter(args.get("tests"))?;
        if !terms.is_empty() {
            let lookup = self
                .lookup
                .ok_or("rigger_graph: no lookup is wired on this server")?;
            let graph = match (self.graph, self.context) {
                (Some(fold), Some(context)) => open_unowed(fold, context, filter).map_err(|e| e.0),
                _ => Err(NO_GRAPH_WIRED.to_string()),
            };
            return lookup
                .lookup(&terms, graph.as_deref().map_err(Clone::clone))
                .map(Value::String)
                .map_err(|e| format!("rigger_graph: {e}").into());
        }
        let context = self
            .context
            .ok_or(format!("rigger_graph: {NO_GRAPH_WIRED}"))?;
        self.refuse_if_rebuild_owed("rigger_graph")?;
        let graph = context.open(filter).map_err(|e| e.to_string())?;
        if !show.is_empty() {
            let located = graph.locate(show).map_err(|e| e.to_string())?;
            let label = graph.stamp_label().to_string();
            return Ok(match located {
                // A record the graph no longer holds (pruned by `rigger reset --runs`, which
                // deletes no event) is read from the run store's log, as `rigger peers` reads it.
                Located::None => match Sidecar::read(self.store, &self.stream)
                    .map_err(|e| e.to_string())?
                    .site_of(show)
                {
                    Some(site) => json!({
                        "label": label,
                        "status": "one",
                        "site": site_object(&site, &[]),
                        "note": crate::sidecar::LOGGED_SITE_NOTE,
                    }),
                    None => json!({"label": label, "status": "none"}),
                },
                Located::Many(cands) => json!({
                    "label": label,
                    "status": "many",
                    "candidates": cands.iter().map(|c| json!({"id": c.id, "file": c.file})).collect::<Vec<_>>(),
                }),
                Located::One(site) => {
                    json!({"label": label, "status": "one", "site": site_object(&site, &[])})
                }
            });
        }
        let g = crate::contextgraph::around(graph.as_ref(), &[around.to_string()], depth)
            .map_err(|e| e.to_string())?;
        // Each loaded node through the one renderer, its degree its edges in this merged graph
        // (`whole_graph_degree` over it): no node is read a second time from the store.
        Ok(json!({
            "label": graph.stamp_label().to_string(),
            "nodes": g
                .nodes
                .iter()
                .map(|n| {
                    site_object(
                        &entity_site(n, whole_graph_degree(&g, &n.id)),
                        &AROUND_OMITTED,
                    )
                })
                .collect::<Vec<_>>(),
            "edges": g.edges.iter().map(|e| json!({"from": e.from, "rel": e.rel, "to": e.to})).collect::<Vec<_>>(),
        }))
    }

    /// The tools THIS instance advertises: the spawn-bound surface (spec 104, `--spawn <id>`)
    /// gets exactly the six named tools; the operator's lookup surface (spec 92, no spawn)
    /// gets `rigger_peers`/`rigger_ground`/`rigger_graph`. One method, driven by
    /// [`surface`](Server::surface), so the advertised list can never drift from what
    /// [`call_tool`](Server::call_tool) actually dispatches.
    fn tool_list(&self) -> Value {
        match self.surface() {
            Surface::SpawnBound => json!([
                {"name": "rigger_emit", "description": "Record a decision on the shared event log, live, so other agents see it immediately. Stamped with the spawn this server is bound to; optionally set valid_from (the bi-temporal time the fact became true).", "inputSchema": {"type": "object", "properties": {"type": {"type": "string"}, "data": {"type": "object"}, "meta": {"type": "object", "description": "Metadata entries (string->string). meta.spawn, if set, must name THIS server's own bound spawn.", "additionalProperties": {"type": "string"}}, "valid_from": {"description": "When the fact became true: unix nanoseconds (integer) or an RFC3339 timestamp string.", "type": ["integer", "string"]}}, "required": ["type", "data"]}},
                {"name": "rigger_peers", "description": "List the decisions, lessons, AND review findings recorded so far, this run's and earlier runs', so you do not work blind to them. Each record is one line, `<kind> <id> | LIVE|HISTORICAL | [by <reviewer> | ]<summary>`, best first: your own unit's records, then this run's, then those naming more of `files`, then the newest, with reviewer verdicts last. The reply stays under one size budget; when records are left out, `left_out` counts them and says how to read one by id (`rigger_graph` `show` <id> returns a whole record). Pass `files` to scope the result to decisions, lessons, and findings that touch those files; omit it for every file.", "inputSchema": {"type": "object", "properties": {"files": {"type": "array", "items": {"type": "string"}}}}},
                {"name": "rigger_ground", "description": "The MEMORY-adjacent intent lookup (spec 92): rank code entities by relevance to a natural-language query. Same as `rigger ground \"<query>\" [<k>]`.", "inputSchema": {"type": "object", "properties": {"query": {"type": "string", "description": "the natural-language query"}, "k": {"type": "integer", "description": "how many results (default 8)"}}, "required": ["query"]}},
                graph_tool(),
                {"name": "rigger_progress", "description": "Record one live progress line for the spawn this server is bound to, so an observer sees you working between milestones.", "inputSchema": {"type": "object", "properties": {"activity": {"type": "string", "description": "a short one-line description of what you just did"}}, "required": ["activity"]}},
                {"name": "rigger_scratch", "description": "Your own rigger-assigned scratch container - the one directory every probe repo, verification worktree, test build, or build cache you create must live under, never /tmp or a session scratchpad. Reaped once your result records.", "inputSchema": {"type": "object", "properties": {}}},
            ]),
            Surface::Lookup => json!([
                {"name": "rigger_peers", "description": "List the decisions, lessons, AND review findings recorded so far, this run's and earlier runs', so you do not work blind to them. Each record is one line, `<kind> <id> | LIVE|HISTORICAL | [by <reviewer> | ]<summary>`, best first: this run's records, then those naming more of `files`, then the newest, with reviewer verdicts last. The reply stays under one size budget; when records are left out, `left_out` counts them and says how to read one by id (`rigger_graph` `show` <id> returns a whole record). Pass `files` to scope the result to decisions, lessons, and findings that touch those files; omit it for every file.", "inputSchema": {"type": "object", "properties": {"files": {"type": "array", "items": {"type": "string"}}}}},
                {"name": "rigger_ground", "description": "The MEMORY-adjacent intent lookup (spec 92): rank code entities by relevance to a natural-language query. Same as `rigger ground \"<query>\" [<k>]`.", "inputSchema": {"type": "object", "properties": {"query": {"type": "string", "description": "the natural-language query"}, "k": {"type": "integer", "description": "how many results (default 8)"}}, "required": ["query"]}},
                graph_tool(),
            ]),
        }
    }
}

/// CONTRACT and FRAMING: the `rigger_graph` tool's description, the one text every surface lists.
/// It names the tool's three forms and their arguments, the CLI form each is the same as, the
/// sections a `lookup` reply carries and the tree it answers from.
pub const RIGGER_GRAPH: &str =
    "Three lookups in one tool; pass exactly one of `lookup`, `show` or \
`around`. `lookup` <term>... answers each term from three sources in one reply, each section \
labelled by its source: `graph` (each entity the term names, its definition site `file:line`, its \
callers and callees; a term resolves exactly, by case or separator variant, by prefix or fuzzily, \
and one that resolves to no entity is answered with the nearest names), `text` (every line of your \
working tree holding the term, scanned live) and `prose:<kind>` (specs, their criteria, personas, \
instructions, skills, docs, decisions, lessons and findings, ranked by relevance). Every section \
answers from your own working tree: a graph hit in a file that differs there from the commit \
the graph describes is withheld and counted in the label heading the section. Same as `rigger graph --lookup <term>...`. `show` \
<entity> gives its definition site (same as `rigger graph --show <entity>`); `around` \
<file|entity> (optionally `depth`) its structural neighborhood (same as `rigger graph --around \
<file|entity> --depth <n>`). Worked examples - who calls <name>: pass `around` <name>; where is \
<name> defined: pass `show` <name>.";

/// CONTRACT: the `rigger_graph` `lookup` argument's description.
pub const RIGGER_GRAPH_LOOKUP: &str =
    "one or more terms: names, paths or words; each is answered in its own group of every section";

/// What `rigger_graph` answers when it is passed none of its three forms.
const GRAPH_ARGUMENTS_MISSING: &str =
    "rigger_graph: pass `lookup` <term>..., `show` <entity> or `around` <file|entity>";

/// Why a server with no context graph wired answers no graph lookup.
const NO_GRAPH_WIRED: &str = "no context graph is wired on this server";

/// The `rigger_graph` entry of a tool list, the same on every surface.
fn graph_tool() -> Value {
    json!({"name": "rigger_graph", "description": RIGGER_GRAPH, "inputSchema": {"type": "object", "properties": {
        "lookup": {"type": "array", "items": {"type": "string"}, "description": RIGGER_GRAPH_LOOKUP},
        "show": {"type": "string"},
        "around": {"type": "string"},
        "depth": {"type": "integer", "description": "neighborhood depth for `around` (default 2)"},
        "tests": {"type": "string", "enum": ["exclude", "include", "only"], "description": RIGGER_GRAPH_TESTS},
    }}})
}

/// CONTRACT: the `rigger_graph` `tests` argument's description.
pub const RIGGER_GRAPH_TESTS: &str =
    "whether test code is shown: `exclude` (the default), `include` or `only`";

/// The view filter a `tests` argument names: `exclude` when it is absent.
pub fn tests_filter(arg: Option<&Value>) -> Result<ViewFilter, String> {
    let tests = match arg {
        None => Tests::Exclude,
        Some(v) => match v.as_str() {
            Some("exclude") => Tests::Exclude,
            Some("include") => Tests::Include,
            Some("only") => Tests::Only,
            _ => {
                return Err(format!(
                    "rigger_graph: `tests` is one of exclude, include or only, not {v}"
                ))
            }
        },
    };
    Ok(ViewFilter { tests })
}

/// The terms of a `lookup` argument: an array of strings, or one string, each term trimmed and
/// the empty ones left out.
fn lookup_terms(arg: Option<&Value>) -> Vec<String> {
    let terms: Vec<&str> = match arg {
        Some(Value::String(term)) => vec![term.as_str()],
        Some(Value::Array(items)) => items.iter().filter_map(Value::as_str).collect(),
        _ => Vec::new(),
    };
    terms
        .into_iter()
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .map(str::to_string)
        .collect()
}

/// The shared core of `rigger_emit` (the MCP tool) and `rigger emit` (the CLI):
/// append `{type, data}` (plus optional `meta`/`valid_from`) to the event store on
/// `stream` AND fold it into the live context graph, EXACTLY as the MCP tool does.
/// Both paths call this so the CLI and the MCP surface stay byte-for-byte identical.
///
/// `args` is the same JSON-shaped object the MCP tool receives:
/// `{"type": <string>, "data": <object>, "meta": {<string>: <string>}?,
/// "valid_from": <int|rfc3339-string>?}`. The CLI builds the same shape from its
/// positional `<type>` + `<json-object>`.
///
/// The event types an agent may emit through the shared surface (`rigger emit` / the
/// `rigger_emit` MCP tool): the context-graph events an agent records (`DecisionMade`,
/// `ReviewFinding`, `LessonLearned`) PLUS the planner's `UnitProposed` refinement - all
/// four flow through [`emit_event`]. The type strings are referenced from their defining
/// constants (never hand-copied) so the allowlist stays in sync with the producers. Any
/// other type - a run-lifecycle boundary or orchestration event - is minted only by the
/// conductor and is refused here (see [`emit_event`]).
const EMITTABLE_TYPES: [&str; 4] = [
    crate::contextgraph::TYPE_DECISION_MADE,
    crate::contextgraph::TYPE_REVIEW_FINDING,
    crate::contextgraph::TYPE_LESSON_LEARNED,
    crate::conductor::TYPE_UNIT_PROPOSED,
];

/// One emit, appended: the [`Position`](crate::eventstore::Position) the store issued and what
/// became of folding it into the context graph.
#[must_use = "an emit whose fold is not reported is a fold that can be silently lost"]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Emitted {
    pub position: crate::eventstore::Position,
    pub fold: Fold,
}

/// The one emit core both surfaces (`rigger emit`, the `rigger_emit` MCP tool) call: refuse what
/// the emit surface refuses, append the event, then fold it into the graph `graph` yields and
/// report the outcome. The log is written FIRST and `graph` is called only after (spec 101): an
/// emit never waits on, or fails over, the context graph, so a fold that cannot happen is
/// reported as [`Fold::NotFolded`], never as a failed emit - the event is durably on the log. A
/// graph that owes its rebuild refuses the fold; that rebuild re-derives the event from the log.
///
/// The fold is unreachable without the store's position ([`crate::eventstore::Appended::one`]):
/// the graph's applied ledger is keyed BY position, so folding at a position the store never
/// issued would mark that location applied forever and swallow the genuine event recorded there.
pub fn emit_event<'g, G: std::ops::Deref<Target = dyn RecordFold + 'g>>(
    store: &dyn EventStore,
    stream: &str,
    graph: impl FnOnce() -> Result<G, crate::contextgraph::Error>,
    args: &Value,
) -> Result<Emitted, String> {
    let event = append_emit(store, stream, args)?;
    Ok(Emitted {
        position: event.position,
        fold: Fold::of(graph(), &event),
    })
}

/// The append half of [`emit_event`]: refuse what the emit surface refuses, append the event, and
/// return it carrying the position the store issued and the stream it was appended to, so its fold
/// reads it as the log does (spec 124, THE APPENDER).
fn append_emit(store: &dyn EventStore, stream: &str, args: &Value) -> Result<Event, String> {
    let typ = args
        .get("type")
        .and_then(Value::as_str)
        .ok_or("rigger_emit: missing type")?;
    // Allowlist the emit surface (spec 22): an agent may emit ONLY the context events it
    // legitimately produces. Refuse everything else - especially a run-lifecycle /
    // orchestration boundary - so a stray `rigger emit RunStarted` from a scratch cwd can
    // never inject a run boundary into the live stream and hijack the conductor's run. The
    // guard runs BEFORE the append, so a refused emit lands NOTHING. An allowlist (not a
    // denylist) is deliberate: a future conductor-owned type is refused by default, never
    // silently injectable.
    if !EMITTABLE_TYPES.contains(&typ) {
        return Err(format!(
            "rigger_emit: refusing to emit {typ:?}: only agent context events \
             ({}) may be emitted this way. Run-lifecycle and orchestration boundaries \
             (RunStarted and its peers) are minted by the conductor, not `rigger emit` - \
             start a new run with `rigger run --fresh`.",
            EMITTABLE_TYPES.join(", ")
        ));
    }
    let data = args.get("data").cloned().unwrap_or_else(|| json!({}));
    let bytes = serde_json::to_vec(&data).map_err(|e| e.to_string())?;

    // The actor metadata stamps the DECIDED edge; valid_from sets the
    // bi-temporal validity (§6). Both are optional builder overrides.
    let mut event = Event::new(typ, bytes);
    if let Some(meta) = args.get("meta").and_then(Value::as_object) {
        for (k, v) in meta {
            let v = v
                .as_str()
                .ok_or_else(|| format!("rigger_emit: meta value for {k:?} must be a string"))?;
            event = event.with_meta(k, v);
        }
    }
    if let Some(vf) = args.get("valid_from") {
        event = event.with_valid_from(parse_valid_from(vf)?);
    }
    // The one emit guard every emit surface runs, on the event as built with its stamps and
    // BEFORE the append, so what it refuses lands nothing.
    rigger_graph_sqlite::contextgraph::check_emit(store, stream, &event)
        .map_err(|e| format!("rigger_emit: {e}"))?;

    let position = store
        .append(stream, ExpectedRevision::Any, std::slice::from_ref(&event))
        .and_then(|appended| appended.one(&format!("the {typ} on {stream:?}")))
        .map_err(|e| e.to_string())?;
    Ok(event.appended_at(stream, position))
}

/// The one size budget of a `rigger_peers` reply, in bytes of its serialized JSON (the text the
/// host reads, which counts at most this many characters): under the host's tool-result limit of
/// about 50K characters with room for the frame, so the reply is delivered inline.
pub const PEERS_REPLY_BUDGET_BYTES: usize = 40_000;

/// The longest a peer line's summary runs, in bytes, before it is clipped with `...`: the line
/// names the record, and `rigger graph --show <id>` reads the whole of it.
pub const PEER_SUMMARY_BYTES: usize = 480;

/// The shared core of `rigger_peers` (the MCP tool) and `rigger peers` (the CLI): the peers'
/// decisions, lessons AND review findings scoped to `files` (empty = all), ranked for the spawn
/// `caller` ([`Sidecar::ranked`]: ceremony last, the caller's own unit first, then the active
/// run's, then those naming more of `files`, then the newest), EXACTLY as the MCP tool returns
/// them - `{"records": [...]}`, one [`peer_line`] each, best first. Both paths call this so the
/// CLI and the MCP surface stay identical. The records kept are the best-ranked prefix under ONE
/// budget for the whole reply, [`PEERS_REPLY_BUDGET_BYTES`], taken through the conductor's one
/// [`within_budget`](crate::conductor::within_budget) rule. When records are left out, a
/// `left_out` object counts them per kind and carries the one-line `note` naming how to narrow
/// the call or read a left-out record by id ([`RECORD_RECOVERY`](crate::conductor::RECORD_RECOVERY)),
/// and the budget holds with that object in the reply.
pub fn peers_json(peers: &Sidecar, files: &[String], caller: Option<&str>) -> Value {
    let ranked = peers.ranked(files, caller);
    let totals = kind_counts(&ranked);
    // The reply's size with no record kept and every record counted out: the most the envelope
    // and the `left_out` object can take, since a count never has more digits than its total.
    let reserve = peers_reply(Vec::new(), totals).to_string().len();
    let kept = crate::conductor::within_budget(
        ranked.iter(),
        usize::MAX,
        PEERS_REPLY_BUDGET_BYTES.saturating_sub(reserve),
        |p| Value::from(peer_line(p)).to_string().len() + ",".len(),
    );
    let kept_counts = kind_counts(kept.iter().copied());
    let out = totals.map(|(noun, total)| (noun, total - count_of(&kept_counts, noun)));
    peers_reply(kept.into_iter().map(peer_line).collect(), out)
}

/// The kinds a peers reply counts, in the order its `left_out` note names them.
const PEER_KINDS: [&str; 3] = [PeerDecision::NOUN, PeerLesson::NOUN, PeerFinding::NOUN];

/// How many of `peers` are of each of [`PEER_KINDS`].
fn kind_counts<'p>(peers: impl IntoIterator<Item = &'p RankedPeer>) -> [(&'static str, usize); 3] {
    let mut counts = PEER_KINDS.map(|noun| (noun, 0));
    for p in peers {
        if let Some((_, n)) = counts.iter_mut().find(|(noun, _)| *noun == p.noun) {
            *n += 1;
        }
    }
    counts
}

fn count_of(counts: &[(&str, usize); 3], noun: &str) -> usize {
    counts
        .iter()
        .find(|(n, _)| *n == noun)
        .map_or(0, |(_, c)| *c)
}

/// A peers reply holding `records`, with a `left_out` object when any of the `out` counts is
/// above zero.
fn peers_reply(records: Vec<String>, out: [(&str, usize); 3]) -> Value {
    let mut reply = json!({ "records": records });
    let counts: Vec<String> = out
        .iter()
        .filter(|(_, n)| *n > 0)
        .map(|(noun, n)| format!("{n} more {noun}(s)"))
        .collect();
    if !counts.is_empty() {
        reply["left_out"] = json!({
            "decisions": count_of(&out, PeerDecision::NOUN),
            "lessons": count_of(&out, PeerLesson::NOUN),
            "findings": count_of(&out, PeerFinding::NOUN),
            "note": format!(
                "(+{} ranked below these left out to keep this reply under budget - narrow with \
                 fewer files, or {})",
                counts.join(", "),
                crate::conductor::RECORD_RECOVERY
            ),
        });
    }
    reply
}

/// One peer record on ONE line: `<kind> <id> | LIVE|HISTORICAL | [by <reviewer> | ]<summary>`,
/// the provenance column absent for a lesson (no run owns one), the summary's whitespace runs
/// collapsed to single spaces and clipped at [`PEER_SUMMARY_BYTES`] with `...`. The id is the
/// one `rigger graph --show <id>` resolves to the whole record.
pub fn peer_line(p: &RankedPeer) -> String {
    let mut line = format!("{} {} | ", p.noun, p.id);
    if let Some(live) = p.live {
        line.push_str(if live { "LIVE | " } else { "HISTORICAL | " });
    }
    if !p.by.is_empty() {
        line.push_str(&format!("by {} | ", p.by));
    }
    let summary = crate::conductor::normalize_ws(&p.summary);
    if summary.len() <= PEER_SUMMARY_BYTES {
        line.push_str(&summary);
    } else {
        let mut cut = PEER_SUMMARY_BYTES - "...".len();
        while !summary.is_char_boundary(cut) {
            cut -= 1;
        }
        line.push_str(&summary[..cut]);
        line.push_str("...");
    }
    line
}

/// Parse a `valid_from` argument into a [`SystemTime`]: a JSON integer of unix
/// nanoseconds, or an RFC3339 string (the common `YYYY-MM-DDTHH:MM:SS[.fff][Z|±HH:MM]`
/// forms). The integer-nanos form is the canonical one (§6).
fn parse_valid_from(v: &Value) -> Result<SystemTime, String> {
    if let Some(nanos) = v.as_i64() {
        return nanos_to_time(nanos);
    }
    if let Some(s) = v.as_str() {
        // Allow a bare integer-as-string too, then fall through to RFC3339.
        if let Ok(nanos) = s.parse::<i64>() {
            return nanos_to_time(nanos);
        }
        return rfc3339_to_time(s);
    }
    Err("rigger_emit: valid_from must be unix-nanos (integer) or an RFC3339 string".into())
}

/// Convert unix nanoseconds (which may be negative, i.e. before the epoch) to a
/// [`SystemTime`].
fn nanos_to_time(nanos: i64) -> Result<SystemTime, String> {
    if nanos >= 0 {
        Ok(UNIX_EPOCH + Duration::from_nanos(nanos as u64))
    } else {
        Ok(UNIX_EPOCH - Duration::from_nanos((-nanos) as u64))
    }
}

/// A dependency-free RFC3339 parser for the common forms: a `YYYY-MM-DD` date, a
/// `T`/space separator, an `HH:MM:SS` time, optional fractional seconds, and a
/// `Z` or `±HH:MM` offset.
fn rfc3339_to_time(s: &str) -> Result<SystemTime, String> {
    let bad = || format!("rigger_emit: invalid RFC3339 valid_from {s:?}");
    let bytes = s.as_bytes();
    if bytes.len() < 19 {
        return Err(bad());
    }
    let num = |a: usize, b: usize| -> Result<i64, String> {
        s.get(a..b)
            .and_then(|p| p.parse::<i64>().ok())
            .ok_or_else(bad)
    };
    let (year, month, day) = (num(0, 4)?, num(5, 7)?, num(8, 10)?);
    let (hour, min, sec) = (num(11, 13)?, num(14, 16)?, num(17, 19)?);
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return Err(bad());
    }

    let rest = &s[19..];
    // Optional fractional seconds.
    let mut idx = 0;
    let rest_bytes = rest.as_bytes();
    let mut nanos_frac: u64 = 0;
    if rest_bytes.first() == Some(&b'.') {
        idx = 1;
        let frac_start = idx;
        while idx < rest_bytes.len() && rest_bytes[idx].is_ascii_digit() {
            idx += 1;
        }
        let mut frac = rest[frac_start..idx].to_string();
        if frac.is_empty() {
            return Err(bad());
        }
        frac.truncate(9);
        while frac.len() < 9 {
            frac.push('0');
        }
        nanos_frac = frac.parse::<u64>().map_err(|_| bad())?;
    }

    // Timezone offset: Z, +HH:MM, or -HH:MM.
    let tz = &rest[idx..];
    let offset_secs: i64 = match tz {
        "Z" | "z" => 0,
        _ => {
            let sign = match tz.as_bytes().first() {
                Some(b'+') => 1,
                Some(b'-') => -1,
                _ => return Err(bad()),
            };
            if tz.len() < 6 {
                return Err(bad());
            }
            let oh: i64 = tz.get(1..3).and_then(|p| p.parse().ok()).ok_or_else(bad)?;
            let om: i64 = tz.get(4..6).and_then(|p| p.parse().ok()).ok_or_else(bad)?;
            sign * (oh * 3600 + om * 60)
        }
    };

    let days = days_from_civil(year, month as u32, day as u32);
    let secs = days * 86_400 + hour * 3600 + min * 60 + sec - offset_secs;
    let total_nanos = secs * 1_000_000_000 + nanos_frac as i64;
    nanos_to_time(total_nanos)
}

/// Days since the unix epoch (1970-01-01) for a civil (proleptic Gregorian) date,
/// per Howard Hinnant's `days_from_civil` algorithm. Works for dates before the
/// epoch (negative result).
fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400; // [0, 399]
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) as i64 + 2) / 5 + d as i64 - 1; // [0, 365]
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy; // [0, 146096]
    era * 146_097 + doe - 719_468
}

/// The byte bound on one tool reply's JSON-RPC frame: half the 16 MiB a host reads from an MCP
/// server before it needs a message boundary, past which it closes the transport and the session
/// loses every tool. Half leaves the frame's envelope and the truncation tail ample room.
const TOOL_REPLY_FRAME_BUDGET_BYTES: usize = 8 * 1024 * 1024;

/// Frame a tool reply carrying `text` through `frame`, kept under
/// [`TOOL_REPLY_FRAME_BUDGET_BYTES`]: a frame that fits goes out whole; otherwise `text` is cut at
/// a character boundary and a marked tail names how many bytes were left out and asks the caller
/// to narrow the call, the cut shrinking in proportion to the overshoot until the escaped frame
/// fits.
fn within_frame(text: &str, frame: impl Fn(&str) -> String) -> String {
    let whole = frame(text);
    if whole.len() <= TOOL_REPLY_FRAME_BUDGET_BYTES {
        return whole;
    }
    let mut keep = text.len().min(TOOL_REPLY_FRAME_BUDGET_BYTES);
    loop {
        while !text.is_char_boundary(keep) {
            keep -= 1;
        }
        let framed = frame(&format!(
            "{}\n[rigger: reply truncated - {} of {} bytes left out to stay under the {}-byte \
             frame cap; narrow the call]",
            &text[..keep],
            text.len() - keep,
            text.len(),
            TOOL_REPLY_FRAME_BUDGET_BYTES
        ));
        if framed.len() <= TOOL_REPLY_FRAME_BUDGET_BYTES || keep == 0 {
            return framed;
        }
        keep = keep * TOOL_REPLY_FRAME_BUDGET_BYTES / framed.len();
    }
}

/// The JSON-RPC 2.0 success envelope. Private: both tool surfaces `Server` answers are served
/// through this ONE `Server`/`handle`/`call_tool`, so no code outside this file needs it.
fn ok(id: Value, result: Value) -> String {
    json!({"jsonrpc": "2.0", "id": id, "result": result}).to_string()
}

/// The JSON-RPC 2.0 error envelope; see [`ok`]'s doc comment for why this is private.
fn err(id: Value, code: i64, message: &str) -> String {
    json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": message}}).to_string()
}

/// The [`site_fields`] entries an `around` node leaves out (spec 119, the operator ruling
/// `spec-119-around-omits-summary`): `summary`, the one unbounded attr, so a neighbourhood hands
/// back ids and site facts, never every finding's prose; `show` keeps it.
const AROUND_OMITTED: [&str; 1] = ["summary"];

/// A located node as the `rigger_graph` tool renders it on `show` and on every `around` node
/// (spec 119, SHOW IS ONE RENDERER): its `id` beside each [`site_fields`] entry not in `omitted`
/// as a key, the values as `site_fields` gives them (`line` and `degree` JSON numbers, every
/// other entry verbatim). `show` omits nothing, `around` [`AROUND_OMITTED`].
fn site_object(site: &EntitySite, omitted: &[&str]) -> Value {
    let mut object = serde_json::Map::from_iter([("id".to_string(), json!(site.id))]);
    object.extend(
        site_fields(site)
            .into_iter()
            .filter(|(entry, _)| !omitted.contains(entry))
            .map(|(entry, value)| (entry.to_string(), value)),
    );
    Value::Object(object)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eventstore::sqlite::Store;
    use crate::eventstore::{Direction, Filter};
    use std::io::Cursor;

    /// Send one JSON-RPC request line to `server` and parse its one reply.
    fn call(server: &Server, input: &str) -> Value {
        let mut output = Vec::new();
        server.run(Cursor::new(input), &mut output).unwrap();
        serde_json::from_str(String::from_utf8(output).unwrap().trim()).unwrap()
    }

    /// A `tools/call` request (id 1) for tool `name` with `arguments`.
    fn tools_call(name: &str, arguments: Value) -> String {
        json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "tools/call",
            "params": {"name": name, "arguments": arguments},
        })
        .to_string()
    }

    /// A sign-off mark a reviewer sets through `rigger_emit` reaches the log with its finding, so
    /// `rigger_peers` ranks the marked finding below an older unmarked one.
    #[test]
    fn a_sign_off_emitted_through_the_tool_ranks_last_in_peers() {
        let store = Store::open(":memory:").unwrap();
        let server = Server::new(&store, "run").with_spawn("u/reviewer#0");
        for data in [
            json!({"id": "f-code", "by": "lens", "summary": "a defect", "about": ["a.rs"]}),
            json!({"id": "f-done", "by": "lens", "summary": "approve", "about": ["a.rs"],
                   "sign_off": true}),
        ] {
            let emitted = call(
                &server,
                &tools_call(
                    "rigger_emit",
                    json!({"type": "ReviewFinding", "data": data}),
                ),
            );
            assert!(emitted.get("error").is_none(), "{emitted}");
        }
        let peers = call(&server, &tools_call("rigger_peers", json!({})));
        assert_eq!(
            peers["result"]["structuredContent"]["records"],
            json!([
                "finding f-code | LIVE | by lens | a defect",
                "finding f-done | LIVE | by lens | approve",
            ])
        );
    }

    /// THE MCP TOOLS READ FROM THE BOUNDARY (spec 101): over a log holding 200,000 ledger
    /// entries and two superseded runs before the boundary, every tool that reads the run -
    /// `rigger_peers` and the spawn-bound `rigger_scratch` - costs exactly one
    /// read of the run's own events plus the typed carry-over per call, asserted through the
    /// counting store double, and still answers from the whole carry-over.
    #[test]
    fn the_mcp_tools_read_the_run_from_its_boundary_and_the_carry_over_by_type() {
        use crate::test_support::{seed_one_shot_fixture, ReadCountingStore};

        let inner = Store::open(":memory:").unwrap();
        let fixture = seed_one_shot_fixture(&inner, "run", &[]);
        let store = ReadCountingStore::new(&inner);
        let server = Server::new(&store, "run");

        let peers = call(&server, &tools_call("rigger_peers", json!({})));
        // The current run's records and the lesson newest first, then the superseded runs'.
        assert_eq!(
            peers["result"]["structuredContent"]["records"],
            json!([
                "finding f-c | LIVE | by lens | found c",
                "decision d-c | LIVE | chose d-c",
                "lesson l-a | learned a",
                "finding f-b | HISTORICAL | by lens | found b",
                "decision d-a | HISTORICAL | chose d-a",
            ])
        );
        assert_eq!(store.reads(), fixture.reads("run", 1));

        let scratch = tempfile::tempdir().unwrap();
        let progress = Store::open(":memory:").unwrap();
        let bound = Server::new(&store, "run")
            .with_progress(&progress, scratch.path().to_str().unwrap())
            .with_spawn("u/implementer#0");
        let resp = call(&bound, &tools_call("rigger_scratch", json!({})));
        assert!(
            resp.get("result").is_some(),
            "rigger_scratch answers: {resp}"
        );
        assert_eq!(store.reads(), fixture.reads("run", 2));
        assert_eq!(store.materialized(), 2 * fixture.cost());
    }

    /// The `rigger_peers` call scoped to `a.rs`.
    const PEERS_OF_A_RS: &str = r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"rigger_peers","arguments":{"files":["a.rs"]}}}"#;

    /// Append one `type_` event carrying `data` to the "run" stream.
    fn append_run(store: &Store, type_: &str, data: Value) {
        store
            .append(
                "run",
                ExpectedRevision::Any,
                &[Event::new(type_, serde_json::to_vec(&data).unwrap())],
            )
            .unwrap();
    }

    /// The names `server` advertises from `tools/list`, in order.
    fn tool_names(server: &Server) -> Vec<String> {
        server
            .tool_list()
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["name"].as_str().unwrap().to_string())
            .collect()
    }

    /// A fresh server (bound to `spawn` when given) answers `input` with a
    /// JSON-RPC error of `code`, echoing `id` when one is given; the reply is returned.
    fn assert_error_reply(
        spawn: Option<&str>,
        input: &str,
        id: Option<Value>,
        code: i64,
        why: &str,
    ) -> Value {
        let store = Store::open(":memory:").unwrap();
        let server = Server::new(&store, "run");
        let server = match spawn {
            Some(spawn) => server.with_spawn(spawn),
            None => server,
        };
        let resp = call(&server, input);
        if let Some(id) = id {
            assert_eq!(resp["id"], id, "the error echoes the request id: {resp}");
        }
        assert_eq!(resp["error"]["code"], code, "{why}: {resp}");
        resp
    }

    /// The `DecisionMade` event a fresh spawn-bound server stores for the `rigger_emit`
    /// request `input`.
    fn emitted_decision(input: &str) -> Event {
        let store = Store::open(":memory:").unwrap();
        let server = Server::new(&store, "run").with_spawn("u/implementer#0");
        call(&server, input);
        store
            .read_all(0, Direction::Forward, &Filter::default())
            .unwrap()
            .into_iter()
            .find(|e| e.type_ == "DecisionMade")
            .expect("stored the emitted event")
    }

    /// A fresh server bound to spawn `u104-spawn-mcp/implementer#0` answers the `rigger_emit`
    /// request `input` with success and stores the `DecisionMade` stamped with that bound
    /// spawn - `why` names the case.
    fn assert_bound_emit_is_stamped(input: &str, why: &str) {
        use crate::conductor::META_SPAWN;

        let store = Store::open(":memory:").unwrap();
        let server = Server::new(&store, "run").with_spawn("u104-spawn-mcp/implementer#0");
        let resp = call(&server, input);
        assert!(
            resp.get("result").is_some(),
            "{why}: the emit must succeed; got:\n{resp}"
        );
        let events = store.read_stream("run", 0, Direction::Forward).unwrap();
        let recorded = events
            .iter()
            .find(|e| e.type_ == "DecisionMade")
            .expect("the emit must still land in the store");
        assert_eq!(
            recorded.meta.get(META_SPAWN).map(String::as_str),
            Some("u104-spawn-mcp/implementer#0"),
            "{why}: the write must be stamped with the BOUND spawn"
        );
    }

    #[test]
    fn emit_tool_appends_to_the_store() {
        let store = Store::open(":memory:").unwrap();
        let server = Server::new(&store, "run").with_spawn("u/implementer#0");

        let input = r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"rigger_emit","arguments":{"type":"DecisionMade","data":{"id":"d1","summary":"x"}}}}"#;
        let mut output = Vec::new();
        server.run(Cursor::new(input), &mut output).unwrap();

        let events = store
            .read_all(0, Direction::Forward, &Filter::default())
            .unwrap();
        assert!(events.iter().any(|e| e.type_ == "DecisionMade"));
        let resp: Value = serde_json::from_str(String::from_utf8(output).unwrap().trim()).unwrap();
        assert_eq!(resp["id"], 1);
        assert!(resp.get("result").is_some());
    }

    /// The shared `emit_event` core (which the CLI `rigger emit` calls directly)
    /// must produce the SAME stored event the MCP `rigger_emit` tool produces from
    /// the same `{type, data}` args - the guarantee that the two surfaces stay
    /// identical because they share one core.
    #[test]
    fn emit_event_core_matches_the_mcp_tool() {
        let args = json!({"type": "DecisionMade", "data": {"id": "d1", "summary": "x"}});

        // The CLI path: call the shared core directly.
        let cli_store = Store::open(":memory:").unwrap();
        let emitted =
            emit_event(&cli_store, "run", || wired(None), &args).expect("the core must append");
        assert_eq!(
            emitted.fold,
            Fold::NotFolded(crate::test_support::unwired()),
            "a surface wired with no graph folds nothing and says so"
        );

        // The MCP path: drive the same args through the server's rigger_emit tool.
        let mcp_store = Store::open(":memory:").unwrap();
        let server = Server::new(&mcp_store, "run").with_spawn("u/implementer#0");
        let input = r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"rigger_emit","arguments":{"type":"DecisionMade","data":{"id":"d1","summary":"x"}}}}"#;
        server.run(Cursor::new(input), &mut Vec::new()).unwrap();

        // Both stores hold one DecisionMade on `run` with byte-identical data.
        let cli = cli_store
            .read_all(0, Direction::Forward, &Filter::default())
            .unwrap();
        let mcp = mcp_store
            .read_all(0, Direction::Forward, &Filter::default())
            .unwrap();
        assert_eq!(cli.len(), 1, "the core appended exactly one event");
        assert_eq!(mcp.len(), 1, "the tool appended exactly one event");
        assert_eq!(cli[0].type_, mcp[0].type_, "same event type");
        assert_eq!(cli[0].stream, mcp[0].stream, "same stream");
        assert_eq!(cli[0].data, mcp[0].data, "same payload bytes");
    }

    /// spec 22, criterion 1: the shared `emit_event` core REFUSES a run-lifecycle
    /// boundary event (`RunStarted`) and other conductor-owned orchestration types, and
    /// appends NOTHING. A stray `rigger emit RunStarted` from a scratch cwd must not be
    /// able to inject a run boundary into the live stream and hijack the conductor's run;
    /// run boundaries are minted ONLY by the conductor. Because the guard lives in the one
    /// shared core, BOTH `rigger emit` (CLI) and `rigger_emit` (MCP) inherit it.
    ///
    /// Driven DIRECTLY over an in-memory `Store` (never a CLI `rigger emit` from a
    /// walk-up-able cwd), so the test can never touch or walk up to a real store - the
    /// exact store-corruption this spec closes is unreproducible here by construction.
    #[test]
    fn emit_event_refuses_run_lifecycle_types_and_appends_nothing() {
        // A run-lifecycle boundary and a peer orchestration event: both conductor-owned,
        // neither agent-emittable.
        for typ in ["RunStarted", "SpawnResult"] {
            let store = Store::open(":memory:").unwrap();
            let args = json!({ "type": typ, "data": {"id": "x"} });

            let err = emit_event(&store, "run", || wired(None), &args)
                .expect_err("a conductor-owned type must be refused, never appended");

            // The error names the offending type and points at the right tool.
            assert!(
                err.contains(typ),
                "the error must name the refused type {typ:?}; got: {err}"
            );
            assert!(
                err.contains("rigger run --fresh"),
                "the error must direct the caller to `rigger run --fresh`; got: {err}"
            );

            // Nothing landed: the guard refuses BEFORE the append, so the store is empty.
            let events = store
                .read_all(0, Direction::Forward, &Filter::default())
                .unwrap();
            assert!(
                events.is_empty(),
                "a refused emit must append NOTHING; found: {events:?}"
            );
        }
    }

    /// A payload the context-graph fold cannot deserialize is refused by the shared emit core
    /// BEFORE the append: a `LessonLearned` whose `about` is one path string (the fold reads an
    /// array of paths) or an empty object (no `id`) lands NOTHING, and the error names the
    /// offending field and the shape the fold expects. Accepting either used to append an event
    /// the fold skipped silently, so the lesson never became a node.
    #[test]
    fn emit_event_refuses_a_payload_the_graph_fold_cannot_apply() {
        for (data, field, shape) in [
            (
                json!({"id": "l1", "about": "src/main.rs"}),
                "about",
                "array",
            ),
            (json!({}), "id", "missing field"),
        ] {
            let store = Store::open(":memory:").unwrap();
            let args = json!({ "type": "LessonLearned", "data": data });
            let err = emit_event(&store, "run", || wired(None), &args)
                .expect_err("a payload the fold cannot apply must be refused");
            assert!(
                err.contains(&format!("`{field}`")) && err.contains(shape),
                "the error must name `{field}` and the expected shape ({shape}); got: {err}"
            );
            let events = store
                .read_all(0, Direction::Forward, &Filter::default())
                .unwrap();
            assert!(
                events.is_empty(),
                "a refused emit must append NOTHING; found: {events:?}"
            );
        }
    }

    /// The same shape check guards the MCP `rigger_emit` tool, because both surfaces route
    /// through the one emit core: the tool answers an error naming the field and appends
    /// nothing.
    #[test]
    fn the_emit_tool_refuses_a_payload_the_graph_fold_cannot_apply() {
        let store = Store::open(":memory:").unwrap();
        let server = Server::new(&store, "run").with_spawn("u/implementer#0");
        let input = r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"rigger_emit","arguments":{"type":"LessonLearned","data":{"id":"l1","about":"src/main.rs"}}}}"#;
        let resp = call(&server, input);
        assert!(
            resp.to_string().contains("`about`"),
            "the tool's reply must name the offending field: {resp}"
        );
        let events = store
            .read_all(0, Direction::Forward, &Filter::default())
            .unwrap();
        assert!(
            events.is_empty(),
            "a refused emit must append NOTHING; found: {events:?}"
        );
    }

    /// Emit the `typ` record `id`, about one file (the location a finding or lesson must name),
    /// through the one emit core, stamped with `spawn` as its `meta.spawn` (unstamped when
    /// `None`): the position the store issued it.
    fn emit_record(
        store: &Store,
        typ: &str,
        id: &str,
        spawn: Option<&str>,
    ) -> Result<crate::eventstore::Position, String> {
        let mut args = json!({"type": typ, "data": {"id": id, "summary": "s", "about": ["a.rs"]}});
        if let Some(spawn) = spawn {
            args["meta"][crate::conductor::META_SPAWN] = json!(spawn);
        }
        emit_event(store, "run", || wired(None), &args).map(|emitted| emitted.position)
    }

    /// The records of the three graph record types `store` holds on `run`.
    fn records_on_run(store: &Store) -> usize {
        store
            .read_stream("run", 0, Direction::Forward)
            .unwrap()
            .iter()
            .filter(|e| crate::run::read::CARRY_OVER_TYPES.contains(&e.type_.as_str()))
            .count()
    }

    /// Append a `RunStarted` boundary for run `run` to `store`'s `run` stream.
    fn start_run(store: &Store, run: &str) {
        let boundary = Event::new(
            crate::run::TYPE_RUN_STARTED,
            json!({"run": run, "spec": "s.md"}).to_string().into_bytes(),
        );
        store
            .append("run", ExpectedRevision::Any, &[boundary])
            .unwrap();
    }

    /// TWO AUTHORS, ONE ID: the graph keeps one node per record id, so a record whose id a live
    /// record of ANOTHER unit or role already holds would hide that record from every reader but
    /// the log. The one emit core refuses it, whatever the two records' types, lands nothing, and
    /// names the holder's position and unit/role and the two ways on: a fresh id, or a decision
    /// that supersedes the holder.
    #[test]
    fn a_record_whose_id_another_authors_live_record_holds_is_refused() {
        for (typ, spawn) in [
            ("DecisionMade", "u2/implementer#0"),
            ("ReviewFinding", "u1/lens:sdet#0"),
            ("LessonLearned", "u1/adjudicator#3~retry1"),
        ] {
            let store = Store::open(":memory:").unwrap();
            let held = emit_record(&store, "DecisionMade", "d-x", Some("u1/implementer#0"))
                .expect("the first author's record is accepted");
            let err = emit_record(&store, typ, "d-x", Some(spawn))
                .expect_err("another author's record under a held id must be refused");
            for named in [
                format!("position {held}"),
                "u1/implementer".to_string(),
                "new id".to_string(),
                "supersedes".to_string(),
            ] {
                assert!(
                    err.contains(&named),
                    "the refusal of {typ} by {spawn} names {named:?}: {err}"
                );
            }
            assert!(
                !err.contains("u1/implementer#0"),
                "the holder is named by its unit/role, never its attempt: {err}"
            );
            assert_eq!(records_on_run(&store), 1, "a refused emit appends nothing");
        }
    }

    /// ONE AUTHOR RE-EMITS: the same unit and role re-emitting its own id is the update path (the
    /// later record wins), across its remediation attempts and reviewer respawns alike, since the
    /// author key leaves the attempt out; an unstamped emit - the operator's - is accepted over
    /// any holder.
    #[test]
    fn the_holders_own_unit_and_role_and_an_unstamped_emit_re_emit_its_id() {
        let store = Store::open(":memory:").unwrap();
        for spawn in [
            Some("u1/adjudicator#0"),
            Some("u1/adjudicator#0"),
            Some("u1/adjudicator#1"),
            Some("u1/adjudicator#1~retry2"),
            None,
        ] {
            emit_record(&store, "DecisionMade", "d-x", spawn)
                .unwrap_or_else(|e| panic!("a re-emit stamped {spawn:?} is accepted: {e}"));
        }
        assert_eq!(records_on_run(&store), 5, "every re-emit is appended");
    }

    /// An UNSTAMPED holder - an operator's record - has no unit or role, so it is another
    /// author to every stamped emit: an agent taking its id is refused, the refusal saying the
    /// holder is unstamped.
    #[test]
    fn a_stamped_record_is_refused_over_an_unstamped_live_holder() {
        let store = Store::open(":memory:").unwrap();
        emit_record(&store, "DecisionMade", "d-x", None).unwrap();
        let err = emit_record(&store, "DecisionMade", "d-x", Some("u1/implementer#0"))
            .expect_err("an agent taking an operator record's id is refused");
        assert!(err.contains("unstamped"), "{err}");
        assert_eq!(records_on_run(&store), 1, "a refused emit appends nothing");
    }

    /// LIVE is the run attribution's rule: a decision or finding only a superseded run recorded
    /// is history and refuses nothing, while the active run's own records do; a lesson is never
    /// attributed away, so a lesson an earlier run recorded still holds its id.
    #[test]
    fn only_a_live_holder_or_a_lesson_refuses_an_id() {
        let store = Store::open(":memory:").unwrap();
        start_run(&store, "r1");
        emit_record(&store, "DecisionMade", "d-x", Some("u1/implementer#0")).unwrap();
        emit_record(&store, "ReviewFinding", "f-x", Some("u1/adversary#0")).unwrap();
        emit_record(&store, "LessonLearned", "l-x", Some("u1/implementer#0")).unwrap();
        start_run(&store, "r2");
        emit_record(&store, "DecisionMade", "d-x", Some("u2/implementer#0"))
            .expect("a superseded run's decision holds no id in the active run");
        emit_record(&store, "ReviewFinding", "f-x", Some("u2/adversary#0"))
            .expect("a superseded run's finding holds no id in the active run");
        let err = emit_record(&store, "LessonLearned", "l-x", Some("u2/implementer#0"))
            .expect_err("a lesson is never history, so it still holds its id");
        assert!(err.contains("u1/implementer"), "{err}");
        let err = emit_record(&store, "DecisionMade", "d-x", Some("u3/implementer#0"))
            .expect_err("the active run's own decision holds its id");
        assert!(err.contains("u2/implementer"), "{err}");
    }

    /// The spawn-bound `rigger_emit` tool reaches the same refusal through the one emit core:
    /// its session is told who holds the id, and nothing is appended.
    #[test]
    fn the_emit_tool_refuses_an_id_another_authors_live_record_holds() {
        let store = Store::open(":memory:").unwrap();
        emit_record(&store, "DecisionMade", "d-x", Some("u1/implementer#0")).unwrap();
        let server = Server::new(&store, "run").with_spawn("u2/implementer#0");
        let input = r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"rigger_emit","arguments":{"type":"DecisionMade","data":{"id":"d-x","summary":"s"}}}}"#;
        let resp = call(&server, input);
        assert!(
            resp.to_string().contains("u1/implementer"),
            "the tool's reply names the holder: {resp}"
        );
        assert_eq!(records_on_run(&store), 1, "a refused emit appends nothing");
    }

    /// A `LessonLearned` whose `about` is an array of paths passes the shape check, is
    /// appended, and folds into the wired graph as a node reachable from the file it is about.
    #[test]
    fn emit_event_appends_and_folds_a_well_shaped_lesson() {
        use crate::contextgraph::sqlite::Projector;
        use crate::test_support::RecordsRead;

        let store = Store::open(":memory:").unwrap();
        let graph = Projector::open(":memory:", "test").unwrap();
        let args = json!({
            "type": "LessonLearned",
            "data": {"id": "l1", "summary": "s", "about": ["src/main.rs"]},
        });
        let emitted = emit_event(&store, "run", || wired(Some(&graph)), &args)
            .expect("a well-shaped lesson is appended");
        let events = store
            .read_all(0, Direction::Forward, &Filter::default())
            .unwrap();
        assert_eq!(events.len(), 1, "the lesson was appended once");
        assert_eq!(
            emitted,
            Emitted {
                position: events[0].position,
                fold: Fold::Folded
            },
            "the emit reports the store's position and the fold it made"
        );
        let g = graph.subgraph(&["src/main.rs".to_string()], 2).unwrap();
        assert!(
            g.nodes.iter().any(|n| n.id == "l1"),
            "the appended lesson folded into the graph: {:?}",
            g.nodes
        );
    }

    /// The shared `peers_json` core (which the CLI `rigger peers` renders from) must
    /// produce the SAME structured value the MCP `rigger_peers` tool returns - both
    /// scope decisions, lessons, and findings to the files arg through the one core.
    #[test]
    fn peers_json_core_matches_the_mcp_tool() {
        let store = Store::open(":memory:").unwrap();
        for (id, governs) in [("da", "a.rs"), ("db", "b.rs")] {
            append_run(
                &store,
                crate::contextgraph::TYPE_DECISION_MADE,
                json!({"id": id, "summary": "x", "governs": [governs]}),
            );
        }
        // One lesson about a.rs, another about b.rs - the lessons half must ride the
        // same one core and the same blast-radius scoping as decisions and findings, so
        // `rigger peers <file>` actually returns the lessons a capped prompt elided.
        for (id, about) in [("la", "a.rs"), ("lb", "b.rs")] {
            append_run(
                &store,
                crate::contextgraph::TYPE_LESSON_LEARNED,
                json!({"id": id, "summary": "y", "about": [about]}),
            );
        }
        // The CLI path: render through the shared core, scoped to a.rs.
        let core = peers_json(
            &Sidecar::read(&store, "run").unwrap(),
            &["a.rs".to_string()],
            None,
        );

        // The MCP path: the same scope through the rigger_peers tool.
        let server = Server::new(&store, "run");
        let resp = call(&server, PEERS_OF_A_RS);
        let tool = &resp["result"]["structuredContent"];

        assert_eq!(
            &core, tool,
            "the shared peers_json core and the MCP tool must return the same value"
        );
        // The a.rs lesson rides the same core and the same scoping as the a.rs decision, newest
        // first, so `rigger peers a.rs` returns the lessons a capped prompt elided.
        assert_eq!(
            core,
            json!({"records": ["lesson la | y", "decision da | LIVE | x"]})
        );
    }

    #[test]
    fn emit_tool_folds_a_review_finding_into_the_wired_graph() {
        // rigger_emit's bridge to the graph: when a graph is wired, a ReviewFinding an agent emits folds into a KIND_FINDING node
        // the moment it lands, so an agent that grounds afterwards retrieves it via
        // graph_context (not via the conductor hand-threading prompts). De-noise (spec 43):
        // only the finding's CONTENT is projected (the node and its ABOUT edge to the code) -
        // the reviewer's provenance is NOT projected as a KIND_AGENT node or a REL_RAISED edge.
        use crate::contextgraph::{self, sqlite::Projector};
        use crate::test_support::RecordsRead;

        let store = Store::open(":memory:").unwrap();
        let graph = Projector::open(":memory:", "test").unwrap();
        let context = crate::test_support::RecordsOnly(&graph);
        let server = Server::new(&store, "run")
            .with_graph(&graph)
            .with_context(&context)
            .with_spawn("u/reviewer#0");

        let input = r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"rigger_emit","arguments":{"type":"ReviewFinding","data":{"id":"f1","summary":"skips the buffer authority","about":["combat.rs"]},"meta":{"actor":"tech-lens"}}}}"#;
        let mut output = Vec::new();
        server.run(Cursor::new(input), &mut output).unwrap();

        // The finding folded into the graph, reachable from the file it is ABOUT.
        let g = graph.subgraph(&["combat.rs".to_string()], 2).unwrap();
        let n = g
            .nodes
            .iter()
            .find(|n| n.id == "f1")
            .expect("the emitted ReviewFinding must fold into the wired graph");
        assert_eq!(n.kind, contextgraph::KIND_FINDING);
        assert_eq!(
            n.attrs.get("summary").map(String::as_str),
            Some("skips the buffer authority")
        );
        // The finding's ABOUT edge to the code it concerns is folded (the content path).
        assert!(
            g.edges
                .iter()
                .any(|e| e.rel == contextgraph::REL_ABOUT && e.from == "f1" && e.to == "combat.rs"),
            "the emitted finding's ABOUT edge to the code is folded"
        );
        // De-noise (spec 43): the actor's provenance is NOT projected.
        assert!(
            !g.nodes.iter().any(|n| n.kind == contextgraph::KIND_AGENT),
            "no KIND_AGENT node is projected for the emitting reviewer"
        );
        assert!(
            !g.edges.iter().any(|e| e.rel == contextgraph::REL_RAISED),
            "no REL_RAISED agent-attribution edge is projected for the folded finding"
        );
    }

    crate::test_cases! {
        emit_tool_carries_meta_actor: assert_eq!(
            emitted_decision(
                r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"rigger_emit","arguments":{"type":"DecisionMade","data":{"id":"d1"},"meta":{"actor":"a7"}}}}"#
            )
            .meta
            .get("actor")
            .map(String::as_str),
            Some("a7")
        );
        /// 2_000_000_000 ns = 2 seconds after the unix epoch.
        emit_tool_sets_valid_from_from_nanos: assert_eq!(
            emitted_decision(
                r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"rigger_emit","arguments":{"type":"DecisionMade","data":{"id":"d1"},"valid_from":2000000000}}}"#
            )
            .valid_from,
            UNIX_EPOCH + Duration::from_nanos(2_000_000_000)
        );
    }

    #[test]
    fn rfc3339_valid_from_parses_to_epoch_seconds() {
        // 1970-01-01T00:00:02Z is two seconds after the epoch.
        let t = parse_valid_from(&json!("1970-01-01T00:00:02Z")).unwrap();
        assert_eq!(t, UNIX_EPOCH + Duration::from_secs(2));
        // A real-world timestamp with an offset.
        let z = parse_valid_from(&json!("2021-01-01T00:00:00Z")).unwrap();
        let off = parse_valid_from(&json!("2021-01-01T01:00:00+01:00")).unwrap();
        assert_eq!(z, off, "the offset is applied back to UTC");
    }

    /// Seed the two `type_` events `seeded` (one about a.rs, one about b.rs) on the run
    /// stream, check the side-car reads both as `P` peers, and call `rigger_peers` scoped to
    /// a.rs: its `records` must hold exactly one line, which is returned.
    fn sole_peer_of_a_rs<P: crate::sidecar::Peer>(type_: &str, seeded: [Value; 2]) -> Value {
        let store = Store::open(":memory:").unwrap();
        for data in seeded {
            append_run(&store, type_, data);
        }
        assert_eq!(Sidecar::read(&store, "run").unwrap().peers::<P>().len(), 2);
        let server = Server::new(&store, "run");

        let resp = call(&server, PEERS_OF_A_RS);
        let arr = resp["result"]["structuredContent"]["records"]
            .as_array()
            .unwrap_or_else(|| panic!("a records array: {resp}"));
        assert_eq!(
            arr.len(),
            1,
            "files=[a.rs] returns only the a.rs record: {resp}"
        );
        arr[0].clone()
    }

    crate::test_cases! {
        peers_tool_scopes_to_the_files_arg: assert_eq!(
            sole_peer_of_a_rs::<crate::sidecar::PeerDecision>(
                crate::contextgraph::TYPE_DECISION_MADE,
                [
                    json!({"id": "da", "summary": "x", "governs": ["a.rs"]}),
                    json!({"id": "db", "summary": "x", "governs": ["b.rs"]}),
                ],
            ),
            "decision da | LIVE | x"
        );
        // Item 4: rigger_peers surfaces peer review FINDINGS as well as decisions, so a
        // concurrent reviewer scoped to its files sees a finding about one of them.
        peers_tool_surfaces_findings_scoped_to_the_files_arg: assert_eq!(
            sole_peer_of_a_rs::<crate::sidecar::PeerFinding>(
                crate::contextgraph::TYPE_REVIEW_FINDING,
                [
                    json!({"id": "fa", "by": "lensA", "summary": "x", "about": ["a.rs"]}),
                    json!({"id": "fb", "by": "lensA", "summary": "x", "about": ["b.rs"]}),
                ],
            ),
            "finding fa | LIVE | by lensA | x"
        );
    }

    /// A summary of `n` copies of a piece that a JSON rendering escapes (quotes, a backslash),
    /// spreads over more than one byte (two- and three-byte characters) and breaks across lines.
    fn awkward_summary(n: usize) -> String {
        "a \"quoted\" \\ path, caf\u{e9} \u{2713}\n\tnext line. ".repeat(n)
    }

    /// Seed `n` decisions, `n` findings and `n / 2` lessons about a.rs, each summary many times a
    /// line's cap, and return the totals: decisions, lessons, findings.
    fn seed_oversized(store: &Store, n: usize) -> [usize; 3] {
        for i in 0..n {
            let summary = awkward_summary(60);
            append_run(
                store,
                crate::contextgraph::TYPE_DECISION_MADE,
                json!({"id": format!("d-{i}"), "summary": summary, "governs": ["a.rs"]}),
            );
            append_run(
                store,
                crate::contextgraph::TYPE_REVIEW_FINDING,
                json!({"id": format!("f-{i}"), "by": "lens", "summary": summary, "about": ["a.rs"]}),
            );
            if i % 2 == 0 {
                append_run(
                    store,
                    crate::contextgraph::TYPE_LESSON_LEARNED,
                    json!({"id": format!("l-{i}"), "summary": summary, "about": ["a.rs"]}),
                );
            }
        }
        [n, n.div_ceil(2), n]
    }

    /// Over a file holding many times the budget of records whose summaries escape, span bytes
    /// and break lines, the `rigger_peers` reply - its text and its structured copy, each as the
    /// host reads it - stays under the one [`PEERS_REPLY_BUDGET_BYTES`] on every surface, keeps
    /// every record on one line, and counts every record it left out by kind, so kept plus left
    /// out is every record.
    #[test]
    fn the_peers_reply_stays_under_one_budget_over_an_oversized_file() {
        let store = Store::open(":memory:").unwrap();
        let totals = seed_oversized(&store, 300);
        for server in [
            Server::new(&store, "run"),
            Server::new(&store, "run").with_spawn("u1/implementer#0"),
        ] {
            let resp = call(&server, PEERS_OF_A_RS);
            let text = resp["result"]["content"][0]["text"].as_str().unwrap();
            let structured = &resp["result"]["structuredContent"];
            assert!(
                text.len() <= PEERS_REPLY_BUDGET_BYTES,
                "the reply text is {} bytes",
                text.len()
            );
            assert_eq!(&serde_json::from_str::<Value>(text).unwrap(), structured);
            let records = structured["records"].as_array().unwrap();
            assert!(
                records.len() > 20,
                "the budget is spent on records, not on padding: kept {}",
                records.len()
            );
            for (i, (noun, out)) in [
                ("decision", "decisions"),
                ("lesson", "lessons"),
                ("finding", "findings"),
            ]
            .into_iter()
            .enumerate()
            {
                let kept = records
                    .iter()
                    .filter(|r| r.as_str().unwrap().starts_with(&format!("{noun} ")))
                    .count();
                assert_eq!(
                    kept + structured["left_out"][out].as_u64().unwrap() as usize,
                    totals[i],
                    "every {noun} is kept or counted out"
                );
            }
            assert!(
                records.iter().all(|r| !r.as_str().unwrap().contains('\n')),
                "every record is one line"
            );
            assert!(structured["left_out"]["note"]
                .as_str()
                .unwrap()
                .contains(crate::conductor::RECORD_RECOVERY));
        }
    }

    /// The bound spawn ranks the tool's reply: a server bound to a spawn of unit u1 puts u1's
    /// older ruling before u2's newer one, and an unbound server puts the newer first.
    #[test]
    fn the_peers_tool_ranks_for_the_spawn_it_is_bound_to() {
        let store = Store::open(":memory:").unwrap();
        for (id, spawn) in [
            ("d-own", "u1/implementer#0"),
            ("d-other", "u2/implementer#0"),
        ] {
            let event = Event::new(
                crate::contextgraph::TYPE_DECISION_MADE,
                serde_json::to_vec(&json!({"id": id, "summary": "s", "governs": ["a.rs"]}))
                    .unwrap(),
            )
            .with_meta(crate::spawn::META_SPAWN, spawn);
            store
                .append("run", ExpectedRevision::Any, &[event])
                .unwrap();
        }
        let records = |server: Server| {
            call(&server, PEERS_OF_A_RS)["result"]["structuredContent"]["records"].clone()
        };
        assert_eq!(
            [
                records(Server::new(&store, "run").with_spawn("u1/implementer#1")),
                records(Server::new(&store, "run")),
            ],
            [
                json!(["decision d-own | LIVE | s", "decision d-other | LIVE | s"]),
                json!(["decision d-other | LIVE | s", "decision d-own | LIVE | s"]),
            ]
        );
    }

    /// The record a [`peer_line`] test renders: `noun` `id` by `by`, `live`, saying `summary`.
    fn peer(noun: &'static str, by: &str, summary: &str, live: Option<bool>) -> RankedPeer {
        RankedPeer {
            noun,
            id: format!("{}1", &noun[..1]),
            by: by.to_string(),
            summary: summary.to_string(),
            live,
        }
    }

    crate::test_cases! {
        /// A live decision: its provenance, and its summary's whitespace runs collapsed.
        peer_line_collapses_a_summary_onto_one_line: assert_eq!(
            peer_line(&peer("decision", "", "chose\n  X\tnow ", Some(true))),
            "decision d1 | LIVE | chose X now"
        );
        /// A historical finding names its reviewer after its provenance.
        peer_line_names_a_findings_reviewer: assert_eq!(
            peer_line(&peer("finding", "lens:sdet", "found y", Some(false))),
            "finding f1 | HISTORICAL | by lens:sdet | found y"
        );
        /// A lesson belongs to no run, so its line carries no provenance.
        peer_line_gives_a_lesson_no_provenance: assert_eq!(
            peer_line(&peer("lesson", "", "learned z", None)),
            "lesson l1 | learned z"
        );
        /// A summary past the cap is clipped on a character boundary and marked.
        peer_line_clips_a_long_summary_on_a_character_boundary: assert_eq!(
            peer_line(&peer("lesson", "", &"\u{e9}".repeat(PEER_SUMMARY_BYTES), None)),
            format!("lesson l1 | {}...", "\u{e9}".repeat((PEER_SUMMARY_BYTES - 3) / 2))
        );
    }

    #[test]
    fn initialize_advertises_tools() {
        let store = Store::open(":memory:").unwrap();
        let server = Server::new(&store, "run");

        let input = "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"initialize\"}\n\
                     {\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"tools/list\"}";
        let mut output = Vec::new();
        server.run(Cursor::new(input), &mut output).unwrap();
        let text = String::from_utf8(output).unwrap();
        assert!(text.contains("rigger_peers") && text.contains("rigger_ground"));
    }

    crate::test_cases! {
        // Unparseable input must not be silently dropped (which hangs the client):
        // it gets a -32700 parse error with a null id.
        malformed_json_gets_a_parse_error: assert_error_reply(
            None,
            "{not valid json",
            Some(Value::Null),
            -32700,
            "unparseable input must be a parse error",
        );
        // A well-formed JSON object that is not a valid JSON-RPC request (no
        // method) must get an Invalid Request error echoing its id, not silence.
        request_missing_method_gets_an_invalid_request_error: assert_error_reply(
            None,
            r#"{"jsonrpc":"2.0","id":7,"params":{}}"#,
            Some(json!(7)),
            -32600,
            "a request with no method is an invalid request",
        );
        // A tools/call missing params.name must get an invalid-params error, not
        // be dropped (which would hang the client awaiting a response).
        tools_call_missing_name_gets_an_invalid_params_error: assert_error_reply(
            None,
            r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{}}"#,
            Some(json!(3)),
            -32602,
            "tools/call without params.name is invalid params",
        );
    }

    /// AN EMIT THE STORE DID NOT WRITE IS NOT AN EMIT. This is the channel every agent
    /// records its decisions and findings on, and the whole point of recording them is that
    /// a CONCURRENT agent reads them; a decision that never landed reads to its author as
    /// recorded and is invisible to everyone else.
    ///
    /// The absence used to come back as `Ok(None)` - a success whose meaning each caller
    /// decided for itself - so the seam asks the one authority instead. The graph fold is
    /// downstream of that answer and is unreachable without a position, which is the
    /// property that keeps the applied ledger from ever marking a location applied that the
    /// store did not issue.
    #[test]
    fn an_emit_the_store_did_not_write_is_reported_as_lost_and_folds_nothing() {
        let args = json!({
            "type": crate::contextgraph::TYPE_DECISION_MADE,
            "data": {"id": "d1", "summary": "a decision"},
        });
        let message = emit_event(
            &crate::eventstore::SilentStore,
            "run",
            || wired(None),
            &args,
        )
        .expect_err("a decision nobody can find was not emitted");
        assert!(
            message.contains("nothing"),
            "the failure says the store wrote nothing: {message}"
        );
        assert!(
            message.contains(crate::contextgraph::TYPE_DECISION_MADE),
            "and names the event that was lost: {message}"
        );
    }

    // =======================================================================================
    // Spec 92, criterion 4's fix round (adj-u92c4-parallel-mcp-loop-instead-of-di-extension):
    // a `Server` with no spawn bound is the operator's lookup surface, ONE dispatch and read
    // loop answering both tool surfaces - proven here at the unit level; `tests/cli.rs`'s
    // `mcp_*` tests prove the same thing end to end through the real `rigger mcp` binary.
    // =======================================================================================

    /// Wiring a grounder switches `tool_list` to exactly the operator's three lookups, in the
    /// order `rigger mcp` advertises them - never the spawn-bound surface's write tools.
    #[test]
    fn tool_list_with_a_grounder_is_exactly_the_operator_lookup_surface() {
        use crate::grounder::Nop;

        let store = Store::open(":memory:").unwrap();
        let grounder = Nop;
        let server = Server::new(&store, "run").with_grounder(Ok(&grounder));

        let names = tool_names(&server);
        assert_eq!(names, vec!["rigger_peers", "rigger_ground", "rigger_graph"]);
    }

    /// Every surface that lists `rigger_graph` gives one worked example of each question it
    /// answers: who calls a name is `around` that name, and where a name is defined is `show` it.
    #[test]
    fn the_graph_tool_gives_a_worked_example_of_each_question_on_every_surface() {
        use crate::grounder::Nop;

        let store = Store::open(":memory:").unwrap();
        let grounder = Nop;
        let lookup = Server::new(&store, "run").with_grounder(Ok(&grounder));
        let spawn_bound = Server::new(&store, "run")
            .with_grounder(Ok(&grounder))
            .with_spawn("u/implementer#0");
        for (surface, server) in [("lookup", lookup), ("spawn-bound", spawn_bound)] {
            let tools = server.tool_list();
            let graph = tools
                .as_array()
                .unwrap()
                .iter()
                .find(|t| t["name"] == "rigger_graph")
                .unwrap_or_else(|| panic!("{surface} lists rigger_graph"));
            let text = graph["description"].as_str().unwrap();
            for example in [
                "who calls <name>: pass `around` <name>",
                "where is <name> defined: pass `show` <name>",
            ] {
                assert!(text.contains(example), "{surface}: {example:?} in {text:?}");
            }
            assert_eq!(text, RIGGER_GRAPH, "{surface} lists the one description");
            let at = |form: &str| text.find(form).unwrap();
            assert!(
                at("`lookup`") < at("`show`") && at("`lookup`") < at("`around`"),
                "{surface}: the description leads with the combined lookup"
            );
            assert_eq!(
                graph["inputSchema"]["properties"]["lookup"]["description"], RIGGER_GRAPH_LOOKUP,
                "{surface}: `lookup` is an argument"
            );
        }
    }

    /// The combined lookup answers as text, and a graph that owes its rebuild, or none wired,
    /// leaves its text and prose halves answering, its graph section stating why: never the
    /// refusal `show` and `around` give.
    #[test]
    fn the_combined_lookup_answers_text_and_prose_when_the_graph_cannot() {
        use crate::contextgraph::{Error as GraphError, REBUILD_OWED};
        use rigger_grounder::lookup::TreeLookup;

        struct Owing;
        impl RecordFold for Owing {
            fn apply(
                &self,
                _: &Event,
                _access: crate::contextgraph::FoldAccess,
            ) -> Result<(), GraphError> {
                Err(GraphError(REBUILD_OWED.to_string()))
            }
            fn rebuild_owed(&self) -> Result<bool, GraphError> {
                Ok(true)
            }
        }

        let tree = tempfile::tempdir().unwrap();
        std::fs::write(tree.path().join("a.rs"), "fn gizmo() {}\n").unwrap();
        std::fs::create_dir(tree.path().join("docs")).unwrap();
        std::fs::write(tree.path().join("docs/a.md"), "# Gizmo\nthe gizmo\n").unwrap();
        let store = Store::open(":memory:").unwrap();
        let lookup = TreeLookup::new(tree.path());
        let graph = Owing;
        let empty = crate::contextgraph::sqlite::Projector::open(":memory:", "test").unwrap();
        let context = crate::test_support::RecordsOnly(&empty);
        let cases: [(Option<&dyn RecordFold>, &str); 2] =
            [(Some(&graph), REBUILD_OWED), (None, NO_GRAPH_WIRED)];
        for (graph, reason) in cases {
            let mut server = Server::new(&store, "run").with_lookup(&lookup);
            if let Some(graph) = graph {
                server = server.with_graph(graph).with_context(&context);
            }
            let input = r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"rigger_graph","arguments":{"lookup":["gizmo"]}}}"#;
            let mut out = Vec::new();
            server.run(Cursor::new(input), &mut out).unwrap();
            let reply: Value =
                serde_json::from_str(String::from_utf8(out).unwrap().trim()).unwrap();
            let text = reply["result"]["content"][0]["text"]
                .as_str()
                .unwrap_or_else(|| panic!("{reply}"));
            for part in [
                format!("## graph\nnot answered: {reason}\n"),
                "a.rs:1: fn gizmo() {}".to_string(),
                "- prose:doc docs/a.md:1".to_string(),
            ] {
                assert!(
                    text.contains(&part),
                    "{reason}: the reply holds {part:?}:\n{text}"
                );
            }
            assert!(
                reply["result"].get("structuredContent").is_none(),
                "the lookup answers as text alone"
            );
        }
    }

    /// The terms `rigger_graph`'s `lookup` argument holds: an array of strings or one string, each
    /// trimmed, the empty ones and anything not a string left out.
    fn lookup_terms_are(arg: Option<Value>, expected: &[&str]) {
        assert_eq!(lookup_terms(arg.as_ref()), expected, "{arg:?}");
    }

    crate::test_cases! {
        an_array_of_terms_is_each_term: lookup_terms_are(Some(json!(["a", "b"])), &["a", "b"]);
        one_string_is_one_term: lookup_terms_are(Some(json!(" gizmo ")), &["gizmo"]);
        empty_and_non_string_items_are_left_out: lookup_terms_are(Some(json!(["", " ", 3, "a"])), &["a"]);
        a_non_string_argument_is_no_term: lookup_terms_are(Some(json!(3)), &[]);
        no_argument_is_no_term: lookup_terms_are(None, &[]);
    }

    /// The lookup surface answers `rigger_ground` through the wired [`Grounder`] port, and
    /// REJECTS the spawn-bound `rigger_emit` as unknown - it is not merely unadvertised, it
    /// is genuinely undispatchable on this surface, so an operator session can never emit onto
    /// the run's event stream through the MCP tool that exists to emit for LOOP agents.
    #[test]
    fn lookup_surface_serves_ground_and_rejects_the_emit_tool() {
        use crate::grounder::Nop;

        let store = Store::open(":memory:").unwrap();
        let grounder = Nop;
        let server = Server::new(&store, "run").with_grounder(Ok(&grounder));

        let ground_input = r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"rigger_ground","arguments":{"query":"anything"}}}"#;
        let mut out = Vec::new();
        server.run(Cursor::new(ground_input), &mut out).unwrap();
        let resp: Value = serde_json::from_str(String::from_utf8(out).unwrap().trim()).unwrap();
        assert!(
            resp["result"]["structuredContent"]["results"].is_array(),
            "rigger_ground must answer with a results array; got:\n{resp}"
        );

        let emit_input = r#"{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"rigger_emit","arguments":{"type":"DecisionMade","data":{}}}}"#;
        let mut out2 = Vec::new();
        server.run(Cursor::new(emit_input), &mut out2).unwrap();
        let resp2: Value = serde_json::from_str(String::from_utf8(out2).unwrap().trim()).unwrap();
        assert_eq!(
            resp2["error"]["code"], -32602,
            "rigger_emit must be UNDISPATCHABLE (not merely unadvertised) on the lookup \
             surface; got:\n{resp2}"
        );
    }

    /// `rigger_ground` returns the grounder's honesty label (6.1) as a field when it has one and
    /// `null` when it does not (a grounder that reads the worktree itself).
    #[test]
    fn ground_returns_the_grounders_label_as_a_field() {
        use crate::composed::Label;
        use crate::grounder::{Grounder, Nop, Ref};

        struct Labelled;
        impl Grounder for Labelled {
            fn ground(&self, _query: &str, _k: usize) -> Vec<Ref> {
                Vec::new()
            }
            fn ground_labelled(&self, query: &str, k: usize) -> (Vec<Ref>, Option<Label>) {
                (
                    self.ground(query, k),
                    Some(Label::unopened("W".to_string(), "locked")),
                )
            }
        }
        let store = Store::open(":memory:").unwrap();
        let ground_input = r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"rigger_ground","arguments":{"query":"anything"}}}"#;
        let labelled = Labelled;
        let nop = Nop;
        for (name, grounder, want) in [
            (
                "a labelled grounder",
                &labelled as &dyn Grounder,
                json!("describes nothing; W cannot be compared; the code store cannot be opened: locked"),
            ),
            ("a grounder with no label", &nop as &dyn Grounder, Value::Null),
        ] {
            let server = Server::new(&store, "run").with_grounder(Ok(grounder));
            let mut out = Vec::new();
            server.run(Cursor::new(ground_input), &mut out).unwrap();
            let resp: Value =
                serde_json::from_str(String::from_utf8(out).unwrap().trim()).unwrap();
            assert_eq!(resp["result"]["structuredContent"]["label"], want, "{name}");
        }
    }

    /// Reject-fix regression: `with_grounder(Err(..))` (the graceful-degrade path a caller
    /// takes when its OWN grounder resolution failed) still serves the lookup surface - the tool
    /// list is unchanged, `rigger_peers` keeps answering - and `rigger_ground` alone reports the
    /// recorded reason as its own tool-call error, lazily, never a silently-empty results array.
    /// Before this fix, `cmd_mcp` propagated a resolution failure with `?`, which would have
    /// taken this whole server down before it answered anything.
    #[test]
    fn with_grounder_unavailable_still_serves_peers_and_reports_ground_lazily() {
        let store = Store::open(":memory:").unwrap();
        let data = serde_json::to_vec(&json!({
            "id": "d1", "summary": "x", "governs": ["a.rs"],
        }))
        .unwrap();
        store
            .append(
                "run",
                ExpectedRevision::Any,
                &[Event::new(crate::contextgraph::TYPE_DECISION_MADE, data)],
            )
            .unwrap();
        let server = Server::new(&store, "run")
            .with_grounder(Err("grounder \"turbovec\" was retired".to_string()));

        // Still the lookup surface - the exact same three tools, tool-list-wise.
        let names: Vec<String> = server
            .tool_list()
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["name"].as_str().unwrap().to_string())
            .collect();
        assert_eq!(names, vec!["rigger_peers", "rigger_ground", "rigger_graph"]);

        // rigger_peers, unrelated to grounding, still answers from the real store.
        let peers_input = r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"rigger_peers","arguments":{}}}"#;
        let mut out = Vec::new();
        server.run(Cursor::new(peers_input), &mut out).unwrap();
        let resp: Value = serde_json::from_str(String::from_utf8(out).unwrap().trim()).unwrap();
        assert_eq!(
            resp["result"]["structuredContent"]["records"][0], "decision d1 | LIVE | x",
            "rigger_peers must keep answering even though the grounder failed to resolve; \
             got:\n{resp}"
        );

        // rigger_ground alone reports the recorded reason, as an error - never silently empty.
        let ground_input = r#"{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"rigger_ground","arguments":{"query":"anything"}}}"#;
        let mut out2 = Vec::new();
        server.run(Cursor::new(ground_input), &mut out2).unwrap();
        let resp2: Value = serde_json::from_str(String::from_utf8(out2).unwrap().trim()).unwrap();
        assert!(
            resp2.get("error").is_some(),
            "rigger_ground must report the recorded resolution failure as an error; got:\n{resp2}"
        );
        assert!(
            resp2["error"]["message"]
                .as_str()
                .unwrap()
                .contains("turbovec"),
            "the error must carry the recorded reason; got:\n{resp2}"
        );

        // A genuinely missing `query` is STILL reported as that, never masked by the recorded
        // grounder-unavailable reason (argument validation runs first).
        let missing_query_input = r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"rigger_ground","arguments":{}}}"#;
        let mut out3 = Vec::new();
        server
            .run(Cursor::new(missing_query_input), &mut out3)
            .unwrap();
        let resp3: Value = serde_json::from_str(String::from_utf8(out3).unwrap().trim()).unwrap();
        assert!(
            resp3["error"]["message"]
                .as_str()
                .unwrap()
                .contains("query"),
            "a missing query must be reported as that, not the unrelated grounder failure; \
             got:\n{resp3}"
        );
    }

    /// `rigger_graph`'s `show` selector on the lookup surface: plants one code-entity
    /// definition in the code view beside a `Projector` and drives it through
    /// `Server::call_tool`, proving [`ContextGraph::locate`] reaches a real caller holding only the
    /// context the server was given, rather than a second implementation reaching for the concrete
    /// `sqlite::Projector` from outside the crate.
    #[test]
    fn lookup_surface_rigger_graph_show_resolves_via_the_locate_trait_method() {
        use crate::contextgraph::sqlite::Projector;
        use crate::grounder::Nop;

        let store = Store::open(":memory:").unwrap();
        let grounder = Nop;
        let graph = Projector::open(":memory:", "test").unwrap();
        let mut tree = crate::test_support::PlantedTree::default();
        tree.define("src/widget.rs", "frobnicate", "function", 7);

        let context = crate::test_support::ViewedRecords {
            records: &graph,
            view: tree.view(),
        };
        let server = Server::new(&store, "run")
            .with_graph(&graph)
            .with_context(&context)
            .with_grounder(Ok(&grounder));

        let input = r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"rigger_graph","arguments":{"show":"frobnicate"}}}"#;
        let mut out = Vec::new();
        server.run(Cursor::new(input), &mut out).unwrap();
        let resp: Value = serde_json::from_str(String::from_utf8(out).unwrap().trim()).unwrap();
        let structured = &resp["result"]["structuredContent"];
        assert_eq!(structured["status"], "one");
        assert_eq!(structured["site"]["id"], "src/widget.rs::frobnicate");
        assert_eq!(structured["site"]["file"], "src/widget.rs");
        assert_eq!(structured["site"]["line"], 7);
        assert_eq!(structured["site"]["kind"], "function");
    }

    /// The `rigger_graph` answer to `arguments` of a lookup surface wired to `graph`, reading it
    /// through `context`: the structured content the show-renderer tests read (spec 119, SHOW IS
    /// ONE RENDERER).
    fn shown_graph_answer_over(
        graph: &crate::contextgraph::sqlite::Projector,
        context: &dyn crate::contextgraph::OpenGraph,
        arguments: Value,
    ) -> Value {
        let store = Store::open(":memory:").unwrap();
        let grounder = crate::grounder::Nop;
        let server = Server::new(&store, "run")
            .with_graph(graph)
            .with_context(context)
            .with_grounder(Ok(&grounder));
        call(&server, &tools_call("rigger_graph", arguments))["result"]["structuredContent"].clone()
    }

    /// The MCP `show` of an id the graph no longer holds falls back to the run store's log, as the
    /// CLI's does: the record's site as a graph node's, with the note naming where it was read
    /// from; an id the log does not hold either still answers none.
    #[test]
    fn rigger_graph_show_reads_a_record_the_graph_no_longer_holds_from_the_log() {
        let graph = crate::contextgraph::sqlite::Projector::open(":memory:", "test").unwrap();
        let store = Store::open(":memory:").unwrap();
        let decision = json!({"id": "d-old", "summary": "chose X", "governs": ["a.rs", "b.rs"]});
        store
            .append(
                "run",
                ExpectedRevision::Any,
                &[Event::new(
                    crate::contextgraph::TYPE_DECISION_MADE,
                    serde_json::to_vec(&decision).unwrap(),
                )],
            )
            .unwrap();
        let grounder = crate::grounder::Nop;
        let context = crate::test_support::RecordsOnly(&graph);
        let server = Server::new(&store, "run")
            .with_graph(&graph)
            .with_context(&context)
            .with_grounder(Ok(&grounder));
        let show = |id: &str| {
            call(&server, &tools_call("rigger_graph", json!({"show": id})))["result"]
                ["structuredContent"]
                .clone()
        };
        let label = crate::contextgraph::ContextGraph::stamp_label(
            &crate::test_support::records_graph(&graph),
        )
        .to_string();
        assert_eq!(
            [show("d-old"), show("nope")],
            [
                json!({
                    "status": "one",
                    "site": {"id": "d-old", "kind": "decision", "degree": 2, "summary": "chose X"},
                    "note": "read from the event log: the graph no longer holds this record",
                    "label": label,
                }),
                json!({"status": "none", "label": label}),
            ]
        );
    }

    /// Spec 119, SHOW IS ONE RENDERER: the MCP `show` gives each node by exact id as its `id` and
    /// the `site_fields` entries as keys - `line` and `degree` JSON numbers, the `doc` of a
    /// document node, and the finding its `by`, `unit` and `summary` with no `file` and no `line` -
    /// each object equal, without order, to `site_fields` of the located site. The cases no
    /// extractor produces (see the fixture) are not exercised.
    #[test]
    fn rigger_graph_show_gives_each_node_its_id_and_its_site_fields_as_keys() {
        use crate::contextgraph::sqlite::Projector;
        use crate::test_support::{
            assert_shown_tree_function_carries_proven_by, assert_shown_tree_sites_are_site_fields,
            fold_shown_tree_finding, shown_tree_planted_view, SHOWN_TREE_IDS,
        };

        let graph = Projector::open(":memory:", "test").unwrap();
        fold_shown_tree_finding(&graph);
        let context = crate::test_support::ViewedRecords {
            records: &graph,
            view: shown_tree_planted_view(),
        };
        let sites = SHOWN_TREE_IDS.map(|id| {
            shown_graph_answer_over(&graph, &context, json!({"show": id}))["site"].clone()
        });
        assert_eq!(
            sites,
            [
                json!({"id": "src/limits.rs::LIMIT", "file": "src/limits.rs", "line": 2,
                       "kind": "constant", "degree": 1, "name": "LIMIT", "lang": "rust"}),
                json!({"id": "src/limits.rs::Bounds", "file": "src/limits.rs", "line": 4,
                       "kind": "type", "degree": 1, "name": "Bounds", "lang": "rust"}),
                json!({"id": "src/limits.rs::blank", "file": "src/limits.rs", "line": 6,
                       "kind": "function", "degree": 1, "name": "blank", "lang": "rust"}),
                json!({"id": "f-shown", "kind": "finding", "degree": 2, "by": "lens:sdet",
                       "unit": "u7", "summary": "the width overflows"}),
                json!({"id": "docs/decisions/one-renderer.md",
                       "file": "docs/decisions/one-renderer.md", "line": 0,
                       "kind": "arch-decision", "degree": 0, "title": "One renderer",
                       "doc": "docs/decisions/one-renderer.md"}),
                json!({"id": "docs/x.md#sec", "file": "docs/x.md", "line": 0,
                       "kind": "design-doc", "degree": 0, "title": "Sec", "doc": "docs/x.md"}),
            ],
            "each node's site object"
        );
        let composed = crate::test_support::view_graph(&graph, &context.view);
        assert_shown_tree_function_carries_proven_by(&composed);
        assert_shown_tree_sites_are_site_fields(&composed, &sites);
    }

    /// Spec 119, SHOW IS ONE RENDERER: the MCP `around` renders every node of the `Graph` it
    /// loaded - the seed file and the definitions and finding one hop from it - as `entity_site`
    /// over the loaded `Node` less its `summary` entry, its degree the node's edges in that
    /// merged graph, with no per-node `site_of` or `one_hop_degree` read: the finding, at the
    /// depth bound, shows the one `ABOUT` edge the graph holds and no `summary`, where `show`
    /// gives it the two edges the store holds and its `summary`.
    #[test]
    fn rigger_graph_around_renders_each_loaded_node_with_its_degree_in_the_merged_graph() {
        use crate::contextgraph::sqlite::Projector;
        use crate::test_support::{
            assert_shown_tree_around_is_entity_sites, fold_shown_tree_finding,
            shown_tree_around_nodes, shown_tree_planted_view, SHOWN_TREE_FILE,
        };

        let graph = Projector::open(":memory:", "test").unwrap();
        fold_shown_tree_finding(&graph);
        let context = crate::test_support::ViewedRecords {
            records: &graph,
            view: shown_tree_planted_view(),
        };
        let answer = shown_graph_answer_over(
            &graph,
            &context,
            json!({"around": SHOWN_TREE_FILE, "depth": 1}),
        );
        let shown = |id: &str| {
            shown_graph_answer_over(&graph, &context, json!({"show": id}))["site"].clone()
        };
        let nodes = shown_tree_around_nodes(&answer);
        assert_eq!(
            nodes,
            [
                json!({"id": "f-shown", "kind": "finding", "degree": 1, "by": "lens:sdet",
                       "unit": "u7"}),
                json!({"id": "src/limits.rs", "file": "src/limits.rs", "line": 0,
                       "kind": "file", "degree": 4, "lang": "rust"}),
                shown("src/limits.rs::Bounds"),
                shown("src/limits.rs::LIMIT"),
                shown("src/limits.rs::blank"),
            ],
            "every loaded node at its degree in the merged graph, each definition as show gives it"
        );
        assert_shown_tree_around_is_entity_sites(
            &crate::test_support::view_graph(&graph, &context.view),
            &nodes,
        );
    }

    /// Spec 101: a wired graph that owes its rebuild is refused by every tool whose answer depends
    /// on the fold - `rigger_graph` (both selectors) and `rigger_ground` - at once, naming
    /// `rigger setup` and never reading the projection, while `rigger_emit` on the same server
    /// still appends (its fold refused, the rebuild re-deriving it from the log).
    #[test]
    fn tools_that_depend_on_the_fold_refuse_a_graph_owing_its_rebuild_while_emit_appends() {
        use crate::contextgraph::{Error as GraphError, REBUILD_OWED};
        use crate::grounder::Nop;

        struct Owing;
        impl RecordFold for Owing {
            fn apply(
                &self,
                _: &Event,
                _access: crate::contextgraph::FoldAccess,
            ) -> Result<(), GraphError> {
                Err(GraphError(REBUILD_OWED.to_string()))
            }
            fn rebuild_owed(&self) -> Result<bool, GraphError> {
                Ok(true)
            }
        }

        let store = Store::open(":memory:").unwrap();
        let grounder = Nop;
        let graph = Owing;
        let empty = crate::contextgraph::sqlite::Projector::open(":memory:", "test").unwrap();
        let context = crate::test_support::RecordsOnly(&empty);
        let server = Server::new(&store, "run")
            .with_graph(&graph)
            .with_context(&context)
            .with_grounder(Ok(&grounder))
            .with_spawn("u/implementer#0");
        let call = |name: &str, args: &str| {
            let input = format!(
                r#"{{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{{"name":"{name}","arguments":{args}}}}}"#
            );
            let mut out = Vec::new();
            server.run(Cursor::new(input), &mut out).unwrap();
            serde_json::from_str::<Value>(String::from_utf8(out).unwrap().trim()).unwrap()
        };
        for (tool, args) in [
            ("rigger_graph", r#"{"around":"src/f.rs"}"#),
            ("rigger_graph", r#"{"show":"alpha"}"#),
            ("rigger_ground", r#"{"query":"alpha"}"#),
        ] {
            assert_eq!(
                call(tool, args)["error"]["message"],
                format!("{tool}: {REBUILD_OWED}"),
                "{tool} {args} refuses naming the rebuild"
            );
        }
        let emitted = call(
            "rigger_emit",
            r#"{"type":"DecisionMade","data":{"id":"d","summary":"s","governs":[],"supersedes":""}}"#,
        );
        let log = store
            .read_stream("run", 0, crate::eventstore::Direction::Forward)
            .unwrap();
        assert_eq!(
            emitted["result"]["structuredContent"],
            crate::test_support::emit_answer(
                log[0].position,
                Some(&crate::test_support::graph_error(REBUILD_OWED))
            ),
            "the emit answers that the owed graph did not fold it, naming the rebuild"
        );
        assert_eq!(
            log.iter().map(|e| e.type_.as_str()).collect::<Vec<_>>(),
            vec!["DecisionMade"],
            "the emit appended although the graph refused its fold"
        );
    }

    /// The sibling of `lookup_surface_rigger_graph_show_resolves_via_the_locate_trait_method`
    /// for [`Located::Many`]: this diff's own new `"status": "many"` JSON shape
    /// (`tool_graph`, the candidate-list branch) has no coverage anywhere else - the CLI's
    /// `graph --show` ambiguous listing (spec 58) proves `Located::Many` itself is produced
    /// correctly, but never runs through this unit's own MCP rendering of it. Two entities
    /// sharing the bare name `shared` in different files must come back as
    /// `{"status":"many","candidates":[...]}`, SORTED by id exactly like the CLI surface
    /// already asserts, never a guess among them (the call-views honesty rule this whole
    /// resolution order exists to uphold).
    #[test]
    fn lookup_surface_rigger_graph_show_lists_ambiguous_candidates() {
        use crate::contextgraph::sqlite::Projector;
        use crate::grounder::Nop;

        let store = Store::open(":memory:").unwrap();
        let grounder = Nop;
        let graph = Projector::open(":memory:", "test").unwrap();
        let mut tree = crate::test_support::PlantedTree::default();
        for file in ["src/a.rs", "src/b.rs"] {
            tree.define(file, "shared", "function", 1);
        }

        let context = crate::test_support::ViewedRecords {
            records: &graph,
            view: tree.view(),
        };
        let server = Server::new(&store, "run")
            .with_graph(&graph)
            .with_context(&context)
            .with_grounder(Ok(&grounder));

        let input = r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"rigger_graph","arguments":{"show":"shared"}}}"#;
        let mut out = Vec::new();
        server.run(Cursor::new(input), &mut out).unwrap();
        let resp: Value = serde_json::from_str(String::from_utf8(out).unwrap().trim()).unwrap();
        let structured = &resp["result"]["structuredContent"];
        assert_eq!(
            structured["status"], "many",
            "two same-named entities must resolve ambiguous, never a guess; got:\n{resp}"
        );
        let candidates = structured["candidates"].as_array().unwrap();
        assert_eq!(
            candidates
                .iter()
                .map(|c| c["id"].as_str().unwrap())
                .collect::<Vec<_>>(),
            vec!["src/a.rs::shared", "src/b.rs::shared"],
            "candidates must be SORTED by id, never in seed/discovery order; got:\n{resp}"
        );
        assert_eq!(candidates[0]["file"], "src/a.rs");
        assert_eq!(candidates[1]["file"], "src/b.rs");
        assert!(
            structured.get("site").is_none(),
            "an ambiguous result must print NO single site - the honesty rule; got:\n{resp}"
        );
    }

    // =======================================================================================
    // Spec 104, criterion 3 (THE SPAWN MCP SERVER): `with_spawn` binds a `Server` to one
    // spawn (`rigger mcp --spawn <id>`), a surface distinct from the operator's lookup
    // surface - proven here at the unit
    // level; `tests/cli.rs`'s `mcp_spawn_*` tests prove the same thing end to end through the
    // real `rigger mcp --spawn` binary.
    // =======================================================================================

    /// The spawn-bound surface advertises EXACTLY the six tools the design names, in the
    /// order it names them - no result tool (the session's own final message is the result)
    /// and never the lookup surface's narrower three.
    #[test]
    fn spawn_bound_tool_list_is_exactly_the_six_named_tools() {
        use crate::grounder::Nop;

        let store = Store::open(":memory:").unwrap();
        let progress = Store::open(":memory:").unwrap();
        let grounder = Nop;
        let server = Server::new(&store, "run")
            .with_grounder(Ok(&grounder))
            .with_progress(&progress, "/scratch/root")
            .with_spawn("u104-spawn-mcp/implementer#0");

        let names = tool_names(&server);
        assert_eq!(
            names,
            vec![
                "rigger_emit",
                "rigger_peers",
                "rigger_ground",
                "rigger_graph",
                "rigger_progress",
                "rigger_scratch",
            ]
        );
        // Every tool a headless launch pre-approves as the bound server's is one this server
        // registers: the launch spells the names in `SPAWN_BOUND_TOOLS`, the server here.
        let prefix = format!("mcp__{}__", rigger_driver::hooks::MCP_SERVER_NAME);
        let unregistered: Vec<&str> = rigger_domain::agent::SPAWN_BOUND_TOOLS
            .iter()
            .copied()
            .filter(|t| {
                !t.strip_prefix(&prefix)
                    .is_some_and(|n| names.iter().any(|r| r == n))
            })
            .collect();
        assert_eq!(unregistered, Vec::<&str>::new());
    }

    /// The spawn-bound surface lists the tool `name` as exactly `expected`, once found by name.
    fn assert_spawn_bound_entry(name: &str, expected: Value) {
        let store = Store::open(":memory:").unwrap();
        let tools = Server::new(&store, "run")
            .with_spawn("u/implementer#0")
            .tool_list();
        let entry = tools.as_array().unwrap().iter().find(|t| t["name"] == name);
        assert_eq!(entry, Some(&expected));
    }

    crate::test_cases! {
        /// The spawn-bound surface's progress tool names what it records and its one argument
        /// only: when to report is the communication built-in's, never the tool description's.
        the_progress_tool_names_what_it_records_and_its_argument_only:
            assert_spawn_bound_entry("rigger_progress", crate::test_support::progress_tool_entry());
        /// Spec 121's THE SCRATCH TOOL NAMES NO TOOLCHAIN: the spawn-bound surface's scratch tool
        /// says "build cache" where it named one toolchain's cache variable and "Reaped once your
        /// result records.", its container's contract otherwise word for word, and takes no
        /// argument.
        the_scratch_tool_names_no_toolchain:
            assert_spawn_bound_entry("rigger_scratch", crate::test_support::scratch_tool_entry());
    }

    crate::test_cases! {
        /// Every write is stamped with the bound spawn BY CONSTRUCTION: an emit with no
        /// `meta.spawn` at all still lands stamped with the id the server was bound to at
        /// startup, exactly like a launched agent that never thinks about attribution.
        spawn_bound_emit_stamps_the_bound_spawn_with_no_meta_supplied: assert_bound_emit_is_stamped(
            r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"rigger_emit","arguments":{"type":"DecisionMade","data":{"id":"d1","summary":"x"}}}}"#,
            "an emit with no meta.spawn supplied",
        );
        /// Reject-fix (spec 104 review round 1): a present-but-non-object `meta` (e.g. a stray
        /// `meta: null` a launched agent sends) must never silently bypass the "ATTRIBUTION BY
        /// CONSTRUCTION" stamp this tool's own doc comment promises.
        /// `obj.entry("meta").or_insert_with(...)` only runs its closure on a VACANT entry, so an
        /// OCCUPIED non-object `meta` used to leave the stamping `if let` skipped entirely while
        /// `emit_event` still reported success - the event landed with no `META_SPAWN` key at
        /// all, unattributed. Drives each non-object shape through the REAL `tools/call` path and
        /// proves the write still lands, correctly stamped with the bound spawn.
        spawn_bound_emit_forces_a_non_object_meta_to_a_stamped_object: for meta in [Value::Null, json!([]), json!("x")] {
            let meta_json = meta.to_string();
            assert_bound_emit_is_stamped(
                &format!(
                    r#"{{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{{"name":"rigger_emit","arguments":{{"type":"DecisionMade","data":{{"id":"d1"}},"meta":{meta_json}}}}}}}"#
                ),
                &format!(
                    "a non-object meta ({meta:?}) must be force-replaced and stamped, never \
                     silently dropped and left unattributed"
                ),
            );
        };
    }

    /// A write NAMING another spawn is refused outright - never silently corrected - and
    /// nothing lands in the store: the one vector the spawn-bound surface's `rigger_emit`
    /// schema exposes (the generic `meta` pass-through) can never misattribute a decision to
    /// a spawn this session is not.
    #[test]
    fn spawn_bound_emit_refuses_a_write_naming_another_spawn() {
        let store = Store::open(":memory:").unwrap();
        let server = Server::new(&store, "run").with_spawn("u/implementer#0");

        let input = r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"rigger_emit","arguments":{"type":"DecisionMade","data":{"id":"d1"},"meta":{"spawn":"v/implementer#0"}}}}"#;
        let mut out = Vec::new();
        server.run(Cursor::new(input), &mut out).unwrap();
        let resp: Value = serde_json::from_str(String::from_utf8(out).unwrap().trim()).unwrap();
        assert_eq!(
            resp["error"]["code"], -32602,
            "an emit naming a DIFFERENT spawn must be refused; got:\n{resp}"
        );

        let events = store.read_stream("run", 0, Direction::Forward).unwrap();
        assert!(
            !events.iter().any(|e| e.type_ == "DecisionMade"),
            "a refused emit must land NOTHING in the store"
        );

        // Naming the SAME spawn the server is bound to is not "another spawn": it goes
        // through exactly like an unstamped emit.
        let same_input = r#"{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"rigger_emit","arguments":{"type":"DecisionMade","data":{"id":"d2"},"meta":{"spawn":"u/implementer#0"}}}}"#;
        let mut out2 = Vec::new();
        server.run(Cursor::new(same_input), &mut out2).unwrap();
        let resp2: Value = serde_json::from_str(String::from_utf8(out2).unwrap().trim()).unwrap();
        assert!(
            resp2.get("result").is_some(),
            "naming the server's OWN bound spawn must be allowed; got:\n{resp2}"
        );
    }

    /// `rigger_progress` records a live activity line for the bound spawn, with no
    /// spawn-naming argument at all - attribution by construction, nothing to refuse.
    #[test]
    fn spawn_bound_progress_records_for_the_bound_spawn() {
        use crate::progress::{AgentProgress, TYPE_AGENT_PROGRESS};

        let store = Store::open(":memory:").unwrap();
        let progress = Store::open(":memory:").unwrap();
        let server = Server::new(&store, "run")
            .with_progress(&progress, "/scratch/root")
            .with_spawn("u104-spawn-mcp/implementer#0");

        let input = r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"rigger_progress","arguments":{"activity":"ran the gates"}}}"#;
        let mut out = Vec::new();
        server.run(Cursor::new(input), &mut out).unwrap();
        let resp: Value = serde_json::from_str(String::from_utf8(out).unwrap().trim()).unwrap();
        assert!(
            resp.get("result").is_some(),
            "progress must succeed; got:\n{resp}"
        );

        let events = progress
            .read_stream(crate::progress::STREAM, 0, Direction::Forward)
            .unwrap();
        let recorded: Vec<AgentProgress> = events
            .iter()
            .filter(|e| e.type_ == TYPE_AGENT_PROGRESS)
            .map(|e| serde_json::from_slice(&e.data).unwrap())
            .collect();
        assert_eq!(
            recorded,
            vec![AgentProgress {
                id: "u104-spawn-mcp/implementer#0".to_string(),
                activity: "ran the gates".to_string(),
                revision: None,
            }],
            "the progress report must be attributed to the BOUND spawn with the given activity"
        );

        // The run stream itself grows by nothing: progress never lands on the replay log.
        let run_events = store.read_stream("run", 0, Direction::Forward).unwrap();
        assert!(run_events.is_empty());
    }

    /// A missing or empty `activity` is refused, mirroring `rigger progress`'s own CLI
    /// validation - never a silently-recorded blank line.
    #[test]
    fn spawn_bound_progress_refuses_a_missing_or_empty_activity() {
        let store = Store::open(":memory:").unwrap();
        let progress = Store::open(":memory:").unwrap();
        let server = Server::new(&store, "run")
            .with_progress(&progress, "/scratch/root")
            .with_spawn("u/implementer#0");

        for args in [r#"{}"#, r#"{"activity":"  "}"#] {
            let input = format!(
                r#"{{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{{"name":"rigger_progress","arguments":{args}}}}}"#
            );
            let mut out = Vec::new();
            server.run(Cursor::new(input), &mut out).unwrap();
            let resp: Value = serde_json::from_str(String::from_utf8(out).unwrap().trim()).unwrap();
            assert!(
                resp.get("error").is_some(),
                "an empty/missing activity must be refused; got:\n{resp}"
            );
        }
    }

    /// `rigger_scratch` answers with the BOUND spawn's own scratch container - the exact path
    /// [`crate::driver::replay::spawn_scratch_path`] resolves - with no argument at all: the
    /// spawn-bound server can name no OTHER spawn's scratch, by construction.
    #[test]
    fn spawn_bound_scratch_answers_with_the_bound_spawns_own_path() {
        let store = Store::open(":memory:").unwrap();
        let progress = Store::open(":memory:").unwrap();
        let server = Server::new(&store, "run")
            .with_progress(&progress, "/scratch/root")
            .with_spawn("u104-spawn-mcp/implementer#0");

        let input = r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"rigger_scratch","arguments":{}}}"#;
        let mut out = Vec::new();
        server.run(Cursor::new(input), &mut out).unwrap();
        let resp: Value = serde_json::from_str(String::from_utf8(out).unwrap().trim()).unwrap();

        let want = crate::driver::replay::spawn_scratch_path(
            "/scratch/root",
            "",
            "u104-spawn-mcp/implementer#0",
        )
        .unwrap();
        assert_eq!(
            resp["result"]["structuredContent"]["path"],
            want.display().to_string(),
            "rigger_scratch must answer with the SAME path spawn_scratch_path resolves; got:\n{resp}"
        );
    }

    /// The spawn-bound surface still answers `rigger_peers`/`rigger_ground`/`rigger_graph`
    /// exactly as the lookup surface does - read-only tools need no spawn attribution at all,
    /// so binding a spawn only ADDS tools, never narrows the read surface.
    #[test]
    fn spawn_bound_surface_still_serves_peers_ground_and_graph() {
        use crate::contextgraph::sqlite::Projector;
        use crate::grounder::Nop;

        let store = Store::open(":memory:").unwrap();
        let grounder = Nop;
        let graph = Projector::open(":memory:", "test").unwrap();
        let context = crate::test_support::RecordsOnly(&graph);
        let server = Server::new(&store, "run")
            .with_grounder(Ok(&grounder))
            .with_graph(&graph)
            .with_context(&context)
            .with_spawn("u/implementer#0");

        for (name, args) in [
            ("rigger_peers", "{}"),
            ("rigger_ground", r#"{"query":"anything"}"#),
            ("rigger_graph", r#"{"around":"does-not-exist.rs"}"#),
        ] {
            let input = format!(
                r#"{{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{{"name":"{name}","arguments":{args}}}}}"#
            );
            let mut out = Vec::new();
            server.run(Cursor::new(input), &mut out).unwrap();
            let resp: Value = serde_json::from_str(String::from_utf8(out).unwrap().trim()).unwrap();
            assert!(
                resp.get("result").is_some(),
                "{name} must still answer on the spawn-bound surface; got:\n{resp}"
            );
        }
    }

    /// The frame size past which the host closes a stdio MCP server's transport: one message
    /// over it and every rigger tool is gone for the rest of the session.
    const HOST_FRAME_LIMIT_BYTES: usize = 16 * 1024 * 1024;

    /// The one raw frame `server` writes in answer to `request`.
    fn raw_frame(server: &Server, request: &str) -> String {
        let mut out = Vec::new();
        server.run(Cursor::new(request), &mut out).unwrap();
        String::from_utf8(out).unwrap()
    }

    /// No tool result may overflow the host's frame: the `rigger_graph` `show` of a decision
    /// whose summary alone is 9 MiB - carried twice whole, as text and as structured content -
    /// comes back as one frame under the limit, its text cut with a marked tail and no
    /// structured content beside it.
    #[test]
    fn an_oversized_tool_result_is_cut_under_the_frame_limit_with_a_marked_tail() {
        use crate::contextgraph::sqlite::Projector;

        let store = Store::open(":memory:").unwrap();
        let grounder = crate::grounder::Nop;
        let graph = Projector::open(":memory:", "test").unwrap();
        let payload = json!({
            "id": "d-huge",
            "summary": "y".repeat(9 * 1024 * 1024),
            "governs": ["a.rs"],
        });
        let mut e = Event::new(
            crate::contextgraph::TYPE_DECISION_MADE,
            payload.to_string().into_bytes(),
        );
        e.position = 1;
        crate::test_support::folds(&graph, std::slice::from_ref(&e));
        let context = crate::test_support::RecordsOnly(&graph);
        let server = Server::new(&store, "run")
            .with_graph(&graph)
            .with_context(&context)
            .with_grounder(Ok(&grounder));

        let frame = raw_frame(
            &server,
            &tools_call("rigger_graph", json!({"show": "d-huge"})),
        );
        assert!(
            frame.len() < HOST_FRAME_LIMIT_BYTES,
            "the reply frame stays under the host's limit; it was {} bytes",
            frame.len()
        );
        let reply: Value = serde_json::from_str(frame.trim()).unwrap();
        let text = reply["result"]["content"][0]["text"].as_str().unwrap();
        assert!(
            text.starts_with('{') && text.contains("[rigger: reply truncated"),
            "the text keeps the reply's head and marks the cut"
        );
        assert!(
            reply["result"].get("structuredContent").is_none(),
            "a cut reply carries no structured content"
        );
    }

    /// A tool error is bounded by the same cap: a refused `rigger_emit` whose message echoes a
    /// 17 MiB type comes back as one error frame under the limit, its message cut with a marked
    /// tail.
    #[test]
    fn an_oversized_tool_error_is_cut_under_the_frame_limit_with_a_marked_tail() {
        let store = Store::open(":memory:").unwrap();
        let server = Server::new(&store, "run").with_spawn("u/implementer#0");

        let request = tools_call(
            "rigger_emit",
            json!({"type": "X".repeat(17 * 1024 * 1024), "data": {}}),
        );
        let frame = raw_frame(&server, &request);
        assert!(
            frame.len() < HOST_FRAME_LIMIT_BYTES,
            "the error frame stays under the host's limit; it was {} bytes",
            frame.len()
        );
        let reply: Value = serde_json::from_str(frame.trim()).unwrap();
        let message = reply["error"]["message"].as_str().unwrap();
        assert!(
            message.starts_with("rigger_emit: refusing")
                && message.contains("[rigger: reply truncated"),
            "the message keeps its head and marks the cut"
        );
    }
}
