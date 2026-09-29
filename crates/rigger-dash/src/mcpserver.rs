//! Exposes the conductor's workflow bridge over MCP (JSON-RPC 2.0 on stdio) so a
//! Claude Code workflow shim can drive it: rigger_next picks up the next queued
//! agent spawn, rigger_result reports its outcome, rigger_emit records a decision
//! live to the event store, and rigger_peers lists peers' decisions. A plain
//! newline-delimited stdio loop - no async runtime needed.

use std::io::{BufRead, Write};
use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde_json::{json, Value};

use crate::contextgraph::{wired, Fold, Located, Projection};
use crate::driver::workflow::Driver;
use crate::eventstore::{Event, EventStore, ExpectedRevision};
use crate::grounder::Grounder;
use crate::sidecar::Sidecar;

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

/// The MCP bridge over the workflow driver, event store, side-car, and (optionally)
/// the context-graph projector.
pub struct Server<'a> {
    driver: &'a Driver,
    store: &'a dyn EventStore,
    stream: String,
    /// The live context-graph projector. When set, an emitted event is folded into
    /// the graph the moment it is appended - so a ReviewFinding (or DecisionMade) an
    /// agent emits via rigger_emit becomes retrievable through `graph_context` by the
    /// agents that ground afterwards (the adversary / adjudicator). Without this, the
    /// workflow-driver path would write findings only to the log and the side-car, and
    /// the graph - the system's cross-agent memory - would never see them.
    graph: Option<&'a dyn Projection>,
    /// The operator's own read-only grounder port (spec 92, criterion 4's fix round). Wiring
    /// one via [`with_grounder`] (or recording a resolution failure through its `Err`
    /// arm) is what MARKS this `Server` as the operator's lookup
    /// surface (`rigger mcp`, registered into `.mcp.json`): [`tool_list`]/[`call_tool`] then
    /// advertise and serve exactly `rigger_peers`/`rigger_ground`/`rigger_graph` instead of the
    /// workflow-driver bridge's `rigger_next`/`rigger_result`/`rigger_emit`/`rigger_peers`/
    /// `rigger_activity` - ONE dispatch and read loop answering two tool surfaces over the SAME
    /// ports (`graph` doubles as the `around`/`show` source too), rather than a second parallel
    /// MCP loop reaching for the concrete grounder/`Projector` across the crate boundary (the
    /// DI-extension fix for the reject this closes). Both `None` on a workflow-bridge server.
    grounder: Option<&'a dyn Grounder>,
    /// Set instead of [`grounder`](Server::grounder) when the caller's own grounder resolution
    /// FAILED (e.g. `--no-default-features` with no `defaults.grounder` pinned, spec 57's
    /// never-silently-degrade grounder-selection contract) - via
    /// [`with_grounder`]'s `Err` arm. Still marks the LOOKUP surface (peers/graph must keep
    /// answering; a grounder misconfiguration is not a reason to refuse the whole server), but
    /// `rigger_ground` reports this reason as an honest tool-call error instead of silently
    /// returning empty results, exactly as the pre-fix lazy resolution did (a
    /// process-wide startup failure here would be a regression: it would take `rigger_peers`
    /// and `rigger_graph` down over a `rigger_ground`-only misconfiguration).
    grounder_unavailable: Option<String>,
    /// The SEPARATE progress store (spec 14, unit 2). When set, `rigger_activity` reads this
    /// run's `AgentProgress` from it to present the live per-agent view. `None` on a server
    /// started without one - `rigger_activity` then reports the frontier with no activity
    /// detail (the frontier alone is still useful).
    progress: Option<&'a dyn EventStore>,
    /// The scratch root whose `agent-live/` holds the liveness markers, so `rigger_activity`
    /// stats each in-flight spawn's marker IN RUST and PRESENTS the age (retiring the JS
    /// driver's `stat` probe). Empty when unknown - the view then omits liveness ages.
    scratch_root: String,
    /// The deterministic id of the spawn currently being SERVED - set when `rigger_next`
    /// hands a spawn to the shim, cleared when `rigger_result` reports it. The shim serves
    /// agents SERIALLY (one `rigger_next` -> run the agent -> `rigger_result` at a time, its
    /// single `runWorkflow` loop), so between those two calls EVERY `rigger_emit` is that
    /// spawn's. `rigger_emit` therefore stamps the emit with this id
    /// ([`META_SPAWN`](crate::conductor::META_SPAWN)), giving the workflow driver the same
    /// per-spawn correlation the verdict-channel-mismatch backstop (spec 18, unit 3) keys on.
    /// Without it a concurrent sibling adjudicator's approve, sharing the reviewer role token
    /// on the ONE stream, would be misattributed by a bare position window.
    current_spawn: Mutex<Option<String>>,
    /// The id this `Server` is BOUND to (spec 104, `rigger mcp --spawn <id>`), set once at
    /// construction via [`with_spawn`](Server::with_spawn) - the launched agent's own MCP
    /// server, `--strict-mcp-config` naming this exact invocation, so every record it writes
    /// is attributed BY CONSTRUCTION, never a runtime guess. `Some` marks this `Server` as
    /// the THIRD tool surface (see [`Surface`]): [`tool_list`](Server::tool_list) and
    /// [`call_tool`](Server::call_tool) both read it through [`surface`](Server::surface), so
    /// the two can never disagree on which tools this instance serves. Unlike
    /// [`current_spawn`](Server::current_spawn) (set/cleared per `rigger_next`/`rigger_result`
    /// on the SHARED workflow bridge), this is fixed for the server's whole lifetime - one
    /// spawn, one child process, one MCP server (Design's CONSTRAINTS WALK: "Concurrent
    /// spawns - one child, one reader, one MCP server process each; no shared file").
    spawn: Option<String>,
}

/// The tool surface a [`Server`] instance answers - computed once by
/// [`surface`](Server::surface) so [`tool_list`](Server::tool_list) and
/// [`call_tool`](Server::call_tool) can never drift on which tools an instance serves.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Surface {
    /// The default: the workflow-driver bridge the loop's shim polls
    /// (`rigger_next`/`rigger_result`/`rigger_emit`/`rigger_peers`/`rigger_activity`).
    Workflow,
    /// A grounder wired (spec 92, criterion 4): the operator's read-only lookup surface
    /// (`rigger_peers`/`rigger_ground`/`rigger_graph`), reached by `rigger mcp` with no
    /// `--spawn`.
    Lookup,
    /// A spawn bound (spec 104, criterion 3): the launched agent's own MCP server
    /// (`rigger_emit`/`rigger_peers`/`rigger_ground`/`rigger_graph`/`rigger_progress`/
    /// `rigger_scratch`), reached by `rigger mcp --spawn <id>`. Takes priority over `Lookup`
    /// when both a grounder and a spawn are wired - a spawn-bound server always serves the
    /// write-scoped six, never the narrower three.
    SpawnBound,
}

impl<'a> Server<'a> {
    pub fn new(driver: &'a Driver, store: &'a dyn EventStore, stream: &str) -> Self {
        Server {
            driver,
            store,
            stream: stream.to_string(),
            graph: None,
            grounder: None,
            grounder_unavailable: None,
            progress: None,
            scratch_root: String::new(),
            current_spawn: Mutex::new(None),
            spawn: None,
        }
    }

    /// Wire the progress store + scratch root so `rigger_activity` can present the live
    /// per-agent view (spec 14): the current run's progress joined with the frontier and the
    /// liveness-marker ages rigger reads here. Absent, the tool still answers - just without
    /// activity or liveness detail.
    pub fn with_progress(mut self, progress: &'a dyn EventStore, scratch_root: &str) -> Self {
        self.progress = Some(progress);
        self.scratch_root = scratch_root.to_string();
        self
    }

    /// Wire the live context-graph projector so emitted events fold into the graph as
    /// they are appended (the workflow-driver path's bridge from rigger_emit to the
    /// graph, mirroring the conductor's own `emit_with_actor`).
    pub fn with_graph(mut self, graph: &'a dyn Projection) -> Self {
        self.graph = Some(graph);
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
    /// reason as its own tool-call error. Either way, wiring this is what turns this `Server`
    /// into the operator's LOOKUP surface: see [`grounder`](Server::grounder)'s doc comment for
    /// what that switches in [`tool_list`](Server::tool_list) and
    /// [`call_tool`](Server::call_tool).
    pub fn with_grounder(mut self, grounder: Result<&'a dyn Grounder, String>) -> Self {
        match grounder {
            Ok(g) => self.grounder = Some(g),
            Err(reason) => self.grounder_unavailable = Some(reason),
        }
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

    /// Serve MCP over the given streams until the input closes (the shim's stdin).
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

    /// The two tool surfaces one `Server` answers (spec 92, criterion 4's fix round): the
    /// workflow-driver bridge the loop's shim polls, or the operator's read-only lookup surface
    /// `rigger mcp` serves - see [`grounder`](Server::grounder)'s doc comment for what marks a
    /// `Server` instance as the latter. [`with_grounder`](Server::with_grounder) marks it with
    /// either an `Ok` grounder or an `Err` resolution failure - a grounder
    /// resolution failure still gets the lookup surface (peers/graph keep answering), it just
    /// makes `rigger_ground` itself report that failure. Kept as one small helper so
    /// [`tool_list`](Server::tool_list) and [`call_tool`](Server::call_tool) can never disagree
    /// on which surface an instance is.
    fn is_lookup_surface(&self) -> bool {
        self.grounder.is_some() || self.grounder_unavailable.is_some()
    }

    /// The ONE authority for which [`Surface`] this instance answers as - see
    /// [`spawn`](Server::spawn)'s and [`Surface`]'s doc comments for the ordering. Both
    /// [`tool_list`] and [`call_tool`] read it, so the advertised list can never drift from
    /// what is actually dispatchable.
    fn surface(&self) -> Surface {
        if self.spawn.is_some() {
            Surface::SpawnBound
        } else if self.is_lookup_surface() {
            Surface::Lookup
        } else {
            Surface::Workflow
        }
    }

    fn call_tool(&self, id: Value, name: &str, args: &Value) -> String {
        let surface = self.surface();
        let result = match (surface, name) {
            (_, "rigger_peers") => self.tool_peers(args),
            (Surface::Lookup | Surface::SpawnBound, "rigger_ground") => self.tool_ground(args),
            (Surface::Lookup | Surface::SpawnBound, "rigger_graph") => self.tool_graph(args),
            (Surface::Workflow, "rigger_next") => self.tool_next(),
            (Surface::Workflow, "rigger_result") => self.tool_result(args),
            (Surface::Workflow, "rigger_emit") => self.tool_emit(args),
            (Surface::Workflow, "rigger_activity") => self.tool_activity(),
            (Surface::SpawnBound, "rigger_emit") => self.tool_emit_bound(args),
            (Surface::SpawnBound, "rigger_progress") => self.tool_progress(args),
            (Surface::SpawnBound, "rigger_scratch") => self.tool_scratch(),
            _ => return err(id, -32602, &format!("unknown tool {name}")),
        };
        match result {
            Ok(structured) => ok(
                id,
                json!({
                    "content": [{"type": "text", "text": structured.to_string()}],
                    "structuredContent": structured,
                }),
            ),
            // A tool may request a specific JSON-RPC code (e.g. -32602 for an
            // unknown spawn id); otherwise an internal error is -32603.
            Err(e) => err(id, e.code, &e.message),
        }
    }

    fn tool_next(&self) -> Result<Value, ToolError> {
        match self.driver.next() {
            Some(req) => {
                // Track the spawn now being served so its live `rigger_emit` calls are
                // stamped with its id (serial serving: this spawn owns every emit until its
                // `rigger_result`). Set BEFORE returning the request, since the shim runs the
                // agent - and the agent emits - only after receiving it.
                *self.current_spawn.lock().unwrap() = Some(req.id.clone());
                serde_json::to_value(req).map_err(|e| ToolError::new(INTERNAL_ERROR, e.to_string()))
            }
            // An empty id means "no spawn right now". `done` disambiguates the two
            // cases the shim cannot otherwise tell apart: `done:true` means the
            // conductor has finished and the shim should exit; `done:false` means the
            // conductor is still running (grounding, or between waves) and the shim
            // must poll again rather than exit. Without `done`, an early empty `next`
            // looks identical to a finished run and the shim exits before the first
            // spawn is even enqueued.
            None => Ok(json!({"id": "", "done": self.driver.is_finished()})),
        }
    }

    fn tool_result(&self, args: &Value) -> Result<Value, ToolError> {
        let id = args.get("id").and_then(Value::as_str).unwrap_or_default();
        let output = args
            .get("output")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let error = args
            .get("error")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        // AUTHORITATIVE MODEL IDENTITY (spec 61 c10 round 2): the shim's runWorkflow
        // reports the resolved model id it observed from the Agent SDK's own
        // structured terminal metadata as `meta.resolved_model` - the SAME wire key
        // the stepwise `rigger result --meta '{"resolved_model": ..}'` convention uses
        // ([`spawn::META_RESOLVED_MODEL`]). Read it here and pass it straight through
        // to the driver so it lands on `AgentResult::resolved_model` instead of being
        // silently dropped; empty when the shim reported none (never guessed).
        let resolved_model = args
            .get("meta")
            .and_then(|m| m.get(crate::spawn::META_RESOLVED_MODEL))
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        // An unknown / stale id means no spawn is waiting on this result. Report
        // it as invalid-params rather than swallowing it: silent success here
        // would leave the real spawn blocked forever (the shim thinks it
        // delivered a result it never did).
        if self.driver.result(id, output, error, resolved_model) {
            // The served spawn is done; no later emit belongs to it (the next `rigger_next`
            // names the next spawn). Clear only when THIS spawn is the one being served, so a
            // stale/unknown-id result never unstamps the live spawn's emits.
            let mut current = self.current_spawn.lock().unwrap();
            if current.as_deref() == Some(id) {
                *current = None;
            }
            Ok(json!({}))
        } else {
            Err(ToolError::new(
                INVALID_PARAMS,
                format!("unknown spawn id {id:?}"),
            ))
        }
    }

    fn tool_emit(&self, args: &Value) -> Result<Value, ToolError> {
        // Stamp the emit with the id of the spawn currently being served, so a GATING
        // adjudicator's approve-shaped verdict recorded via `rigger_emit` is attributable to
        // THAT spawn EXACTLY (the per-spawn correlation the verdict-channel-mismatch backstop
        // keys on). Authoritative: the server sets META_SPAWN, never the agent's args.
        let args = self.stamp_current_spawn(args);
        self.emit(&args)
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

    /// Return `args` with its `meta.spawn` set to the id of the spawn currently being served
    /// ([`current_spawn`](Server::current_spawn)), so an emit landing between a spawn's
    /// `rigger_next` and its `rigger_result` carries that spawn's
    /// [`META_SPAWN`](crate::conductor::META_SPAWN) stamp. When no spawn is being served the
    /// args are returned untouched (a stray emit stays unstamped). The server writes the
    /// stamp authoritatively, overriding any `meta.spawn` the agent supplied.
    fn stamp_current_spawn(&self, args: &Value) -> Value {
        let spawn = match self.current_spawn.lock().unwrap().clone() {
            Some(s) => s,
            None => return args.clone(),
        };
        let mut args = args.clone();
        let obj = match args.as_object_mut() {
            Some(o) => o,
            None => return args,
        };
        Self::stamp_spawn_meta(obj, &spawn);
        args
    }

    /// `rigger_emit` on the SPAWN-BOUND surface (spec 104, criterion 3): stamps `meta.spawn`
    /// with the id this server is BOUND to ([`with_spawn`](Server::with_spawn)) - ATTRIBUTION
    /// BY CONSTRUCTION, unlike [`stamp_current_spawn`]'s workflow-bridge surface, which is a
    /// runtime guess (set/cleared per `rigger_next`/`rigger_result`, unstamped between
    /// spawns). A write NAMING a DIFFERENT spawn is REFUSED outright, never silently
    /// corrected: the generic `meta` pass-through this tool's schema already carries (shared
    /// with the workflow bridge's `rigger_emit`) is the one vector through which a
    /// spawn-bound session could otherwise misattribute a decision to a sibling spawn it
    /// shares no memory with. Naming the server's OWN bound spawn is not "another spawn" -
    /// it is accepted exactly like an unstamped emit. The stamp itself goes through
    /// [`stamp_spawn_meta`](Server::stamp_spawn_meta), shared with `stamp_current_spawn`, so a
    /// present-but-non-object `meta` (e.g. a stray `meta: null`) can never silently bypass it.
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

    /// Insert `meta.spawn = spawn` into an args object's `meta` field - the ONE stamping site
    /// both [`tool_emit_bound`](Server::tool_emit_bound) and
    /// [`stamp_current_spawn`](Server::stamp_current_spawn) call, so the two attribution paths
    /// can never drift apart (spec 104 review round 1 upheld their duplicate, byte-identical
    /// inline copies as a DRY defect carrying the same latent bug). A `meta` that is present
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

    /// The current run's id, resolved fresh from the run stream - the SAME read
    /// [`tool_activity`](Server::tool_activity) takes for its own `run_id`, shared here for
    /// the two spawn-bound tools ([`tool_progress`], [`tool_scratch`]) that need it to
    /// resolve a per-run record/path, never a second parallel resolution.
    fn current_run_id(&self) -> Result<String, ToolError> {
        Ok(self.read_current_run()?.1)
    }

    /// `rigger_progress` (spec 104's spawn MCP server, addendum §4.3): record one live
    /// activity line for the BOUND spawn to the SAME progress store + record shape
    /// `rigger progress <id> "<activity>"` and [`tool_activity`]'s `with_progress` wiring
    /// already use ([`crate::progress_store::record`]) - never a second parallel writer. No
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
    /// at its terminus, over the SAME `scratch_root` [`with_progress`](Server::with_progress)
    /// already wires for [`tool_activity`]'s liveness ages. No argument at all: the
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
    /// whose `about` intersect it come back; absent or empty, every one does. The findings
    /// are how concurrent review lenses see each other's findings LIVE, before any of
    /// them grounds again - the same side-car channel that surfaces peer decisions and the
    /// lessons a capped prompt section elided.
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
        Ok(peers_json(&peers, &files))
    }

    /// `rigger_ground` (spec 92, criterion 4's fix round): the operator's own MEMORY-adjacent
    /// intent lookup, over the SAME [`Grounder`] port [`with_grounder`](Server::with_grounder)
    /// wires - the exact trait `rigger ground` resolves through `select_grounder`, so ground's
    /// ranking (spec 92 criterion 3's territory) is inherited automatically, never
    /// re-implemented here. Only reachable on the lookup surface (a grounder wired OR its
    /// resolution failure recorded); [`call_tool`] never dispatches here otherwise. Argument
    /// validation runs BEFORE the grounder-unavailable check, so a genuinely missing `query`
    /// is always reported as that - never masked by an unrelated resolution failure.
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
        let grounder = self.grounder.expect(
            "tool_ground is dispatched only on the lookup surface, which always carries \
             either a grounder or a recorded resolution failure",
        );
        let results: Vec<Value> = grounder
            .ground(query, k)
            .into_iter()
            .map(|r| json!({"file": r.file, "line": r.line, "text": r.text}))
            .collect();
        Ok(json!({"results": results}))
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

    /// `rigger_graph` (spec 92, criterion 4's fix round): the STRUCTURE (`around`) and
    /// resolution (`show`) lookups, both over the SAME `graph` port `with_graph` already wires
    /// for the workflow bridge's event fold - `around` calls the trait's existing
    /// [`Projection::subgraph`], `show` the trait's [`Projection::locate`] added alongside this
    /// fix, so no second graph-reading implementation is needed for either selector. Only
    /// reachable on the lookup surface; [`call_tool`] never dispatches here otherwise.
    fn tool_graph(&self, args: &Value) -> Result<Value, ToolError> {
        let show = args.get("show").and_then(Value::as_str).unwrap_or("");
        let around = args.get("around").and_then(Value::as_str).unwrap_or("");
        let depth = args.get("depth").and_then(Value::as_i64).unwrap_or(2);
        if show.is_empty() && around.is_empty() {
            return Err("rigger_graph: pass `show` <entity> or `around` <file|entity>".into());
        }
        let graph = self
            .graph
            .ok_or("rigger_graph: no context graph is wired on this server")?;
        self.refuse_if_rebuild_owed("rigger_graph")?;
        if !show.is_empty() {
            let located = graph.locate(show).map_err(|e| e.to_string())?;
            return Ok(match located {
                Located::None => json!({"status": "none"}),
                Located::Many(cands) => json!({
                    "status": "many",
                    "candidates": cands.iter().map(|c| json!({"id": c.id, "file": c.file})).collect::<Vec<_>>(),
                }),
                Located::One(site) => json!({
                    "status": "one",
                    "site": {
                        "id": site.id,
                        "file": site.file,
                        "line": site.line,
                        "kind": if site.kind.is_empty() { "?" } else { site.kind.as_str() },
                        "degree": site.degree,
                    },
                }),
            });
        }
        let g = graph
            .subgraph(&[around.to_string()], depth)
            .map_err(|e| e.to_string())?;
        Ok(json!({
            "nodes": g.nodes.iter().map(|n| json!({"id": n.id, "kind": n.kind})).collect::<Vec<_>>(),
            "edges": g.edges.iter().map(|e| json!({"from": e.from, "rel": e.rel, "to": e.to})).collect::<Vec<_>>(),
        }))
    }

    /// `rigger_activity` (spec 14, unit 2): present the live per-agent view of the current
    /// run - for every in-flight agent, what it is doing, its heartbeat age, and its last
    /// store milestone - so the shim gets up-to-the-second agent state over MCP with NO
    /// filesystem access of its own. Rigger CONSOLIDATES the run stream, this run's progress,
    /// and the marker ages it stats HERE in Rust, and returns the same shape `rigger status
    /// --json` prints. Read-only; the progress store and markers are optional (a server
    /// started without them still returns the frontier).
    fn tool_activity(&self) -> Result<Value, ToolError> {
        let (run_events, run_id) = self.read_current_run()?;
        let run_events = run_events.as_slice();

        let prog_events: Vec<Event> = match self.progress {
            Some(store) => crate::progress::read_run(store, &run_id).unwrap_or_default(),
            None => Vec::new(),
        };

        let now = SystemTime::now();
        let mut liveness_ages: std::collections::HashMap<String, u64> =
            std::collections::HashMap::new();
        if !self.scratch_root.is_empty() {
            let frontier = crate::spawn::step_result(run_events)
                .map_err(|e| ToolError::new(INTERNAL_ERROR, e.to_string()))?
                .wave;
            liveness_ages =
                crate::liveness::marker_ages(&self.scratch_root, &run_id, &frontier, now)
                    .into_iter()
                    .collect();
        }

        let view = crate::progress::consolidate(run_events, &prog_events, &liveness_ages, now)
            .map_err(|e| ToolError::new(INTERNAL_ERROR, e.to_string()))?;
        serde_json::to_value(view).map_err(|e| ToolError::new(INTERNAL_ERROR, e.to_string()))
    }

    /// The tools THIS instance advertises: the spawn-bound surface (spec 104, `--spawn <id>`)
    /// gets exactly the six named tools; the operator's lookup surface (spec 92, a grounder
    /// wired, no spawn) gets `rigger_peers`/`rigger_ground`/`rigger_graph`; the
    /// workflow-driver bridge (the default) gets its usual five. One method, driven by
    /// [`surface`](Server::surface), so the advertised list can never drift from what
    /// [`call_tool`](Server::call_tool) actually dispatches.
    fn tool_list(&self) -> Value {
        match self.surface() {
            Surface::SpawnBound => json!([
                {"name": "rigger_emit", "description": "Record a decision on the shared event log, live, so other agents see it immediately. Stamped with the spawn this server is bound to; optionally set valid_from (the bi-temporal time the fact became true).", "inputSchema": {"type": "object", "properties": {"type": {"type": "string"}, "data": {"type": "object"}, "meta": {"type": "object", "description": "Metadata entries (string->string). meta.spawn, if set, must name THIS server's own bound spawn.", "additionalProperties": {"type": "string"}}, "valid_from": {"description": "When the fact became true: unix nanoseconds (integer) or an RFC3339 timestamp string.", "type": ["integer", "string"]}}, "required": ["type", "data"]}},
                {"name": "rigger_peers", "description": "List the decisions, lessons, AND review findings recorded so far this run, so you do not work blind to them. Pass `files` to scope the result to decisions, lessons, and findings that touch those files; omit it to see every one.", "inputSchema": {"type": "object", "properties": {"files": {"type": "array", "items": {"type": "string"}}}}},
                {"name": "rigger_ground", "description": "The MEMORY-adjacent intent lookup (spec 92): rank code entities by relevance to a natural-language query. Same as `rigger ground \"<query>\" [<k>]`.", "inputSchema": {"type": "object", "properties": {"query": {"type": "string", "description": "the natural-language query"}, "k": {"type": "integer", "description": "how many results (default 8)"}}, "required": ["query"]}},
                {"name": "rigger_graph", "description": "The STRUCTURE and resolution lookups: pass `show` <entity> for its definition site (same as `rigger graph --show <entity>`), or `around` <file|entity> (optionally `depth`) for its structural neighborhood (same as `rigger graph --around <file|entity> --depth <n>`). Pass exactly one of `show`/`around`.", "inputSchema": {"type": "object", "properties": {"show": {"type": "string"}, "around": {"type": "string"}, "depth": {"type": "integer", "description": "neighborhood depth for `around` (default 2)"}}}},
                {"name": "rigger_progress", "description": "Record one live progress line for the spawn this server is bound to, so an observer sees you working between milestones. Report one short line after each significant step; never batch them up.", "inputSchema": {"type": "object", "properties": {"activity": {"type": "string", "description": "a short one-line description of what you just did"}}, "required": ["activity"]}},
                {"name": "rigger_scratch", "description": "Your own rigger-assigned scratch container - the one directory every probe repo, verification worktree, test build, or CARGO_TARGET_DIR you create must live under, never /tmp or a session scratchpad. Reaped the moment your result records.", "inputSchema": {"type": "object", "properties": {}}},
            ]),
            Surface::Lookup => json!([
                {"name": "rigger_peers", "description": "List the decisions, lessons, AND review findings recorded so far this run, so you do not work blind to them. Pass `files` to scope the result to decisions, lessons, and findings that touch those files; omit it to see every one.", "inputSchema": {"type": "object", "properties": {"files": {"type": "array", "items": {"type": "string"}}}}},
                {"name": "rigger_ground", "description": "The MEMORY-adjacent intent lookup (spec 92): rank code entities by relevance to a natural-language query. Same as `rigger ground \"<query>\" [<k>]`.", "inputSchema": {"type": "object", "properties": {"query": {"type": "string", "description": "the natural-language query"}, "k": {"type": "integer", "description": "how many results (default 8)"}}, "required": ["query"]}},
                {"name": "rigger_graph", "description": "The STRUCTURE and resolution lookups: pass `show` <entity> for its definition site (same as `rigger graph --show <entity>`), or `around` <file|entity> (optionally `depth`) for its structural neighborhood (same as `rigger graph --around <file|entity> --depth <n>`). Pass exactly one of `show`/`around`.", "inputSchema": {"type": "object", "properties": {"show": {"type": "string"}, "around": {"type": "string"}, "depth": {"type": "integer", "description": "neighborhood depth for `around` (default 2)"}}}},
            ]),
            Surface::Workflow => json!([
                {"name": "rigger_next", "description": "Pick up the next queued agent spawn. The id is empty when nothing is waiting.", "inputSchema": {"type": "object", "properties": {}}},
                {"name": "rigger_result", "description": "Report an agent's final result by spawn id.", "inputSchema": {"type": "object", "properties": {"id": {"type": "string"}, "output": {"type": "string"}, "error": {"type": "string"}}, "required": ["id"]}},
                {"name": "rigger_emit", "description": "Record a decision on the shared event log, live, so other agents see it immediately. Optionally set meta (e.g. the acting agent, which stamps the graph's DECIDED edge) and valid_from (the bi-temporal time the fact became true).", "inputSchema": {"type": "object", "properties": {"type": {"type": "string"}, "data": {"type": "object"}, "meta": {"type": "object", "description": "Metadata entries (string->string), e.g. {\"actor\": \"<agent-id>\"}.", "additionalProperties": {"type": "string"}}, "valid_from": {"description": "When the fact became true: unix nanoseconds (integer) or an RFC3339 timestamp string.", "type": ["integer", "string"]}}, "required": ["type", "data"]}},
                {"name": "rigger_peers", "description": "List the decisions, lessons, AND review findings other agents have raised so far this run, so you do not work blind to them (concurrent reviewers see each other's findings live; lessons recover what a capped prompt section elided). Pass `files` (your blast-radius) to scope the result to decisions, lessons, and findings that touch those files; omit it to see every one.", "inputSchema": {"type": "object", "properties": {"files": {"type": "array", "items": {"type": "string"}, "description": "The agent's blast-radius: only decisions whose `governs`, and lessons and findings whose `about`, intersect these files are returned. Omit for all."}}}},
                {"name": "rigger_activity", "description": "The live per-agent view of the current run: one entry per in-flight agent with its stage, latest activity, how long since that activity and its last heartbeat, and its last event-store milestone (and how long ago - the blackout this fills). Rigger consolidates the run stream, the progress store, and the liveness markers and presents them here, so you get up-to-the-second agent state with no filesystem access of your own.", "inputSchema": {"type": "object", "properties": {}}},
            ]),
        }
    }
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
pub fn emit_event<'g, G: std::ops::Deref<Target = dyn Projection + 'g>>(
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
/// return it carrying the position the store issued.
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
    // Refuse a payload the context-graph fold cannot apply BEFORE the append: the fold would
    // skip it silently, leaving an event in the log that never becomes a node.
    rigger_graph_sqlite::contextgraph::check_fold_payload(typ, &bytes)
        .map_err(|e| format!("rigger_emit: refusing to emit: {e}"))?;

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

    event.position = store
        .append(stream, ExpectedRevision::Any, std::slice::from_ref(&event))
        .and_then(|appended| appended.one(&format!("the {typ} on {stream:?}")))
        .map_err(|e| e.to_string())?;
    Ok(event)
}

/// The shared core of `rigger_peers` (the MCP tool) and `rigger peers` (the CLI):
/// the peers' decisions, lessons, AND review findings, scoped to `files` (empty = all),
/// EXACTLY as the MCP tool returns them - `{"decisions": [...], "lessons": [...],
/// "findings": [...]}`. Both paths call this so the CLI and the MCP surface stay
/// identical. The three sections mirror the three capped prompt sections
/// (`graph_context`), so the elision note each capped section renders - "recover the
/// full set with `rigger peers <file>`" - is honest for every section: what the prompt
/// trims, this surface returns in full (adj-u1gap17).
pub fn peers_json(peers: &Sidecar, files: &[String]) -> Value {
    let decisions: Vec<Value> = peers
        .peers_for::<crate::sidecar::PeerDecision>(files)
        .iter()
        .map(|d| json!({"id": d.id, "summary": d.summary, "governs": d.governs, "live": d.live}))
        .collect();
    let lessons: Vec<Value> = peers
        .peers_for::<crate::sidecar::PeerLesson>(files)
        .iter()
        .map(|l| json!({"id": l.id, "summary": l.summary, "about": l.about}))
        .collect();
    let findings: Vec<Value> = peers
        .peers_for::<crate::sidecar::PeerFinding>(files)
        .iter()
        .map(|f| json!({"id": f.id, "by": f.by, "summary": f.summary, "about": f.about}))
        .collect();
    json!({"decisions": decisions, "lessons": lessons, "findings": findings})
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

/// The JSON-RPC 2.0 success envelope. Private: both tool surfaces `Server` answers (the
/// workflow-driver bridge and the operator's lookup surface, spec 92 criterion 4's fix round)
/// are served through this ONE `Server`/`handle`/`call_tool` now, so no code outside this file
/// needs the envelope any more - retiring the operator MCP surface's second parallel read loop
/// retired its last external caller too.
fn ok(id: Value, result: Value) -> String {
    json!({"jsonrpc": "2.0", "id": id, "result": result}).to_string()
}

/// The JSON-RPC 2.0 error envelope; see [`ok`]'s doc comment for why this is private.
fn err(id: Value, code: i64, message: &str) -> String {
    json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": message}}).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eventstore::sqlite::Store;
    use crate::eventstore::{Direction, Filter};
    use crate::spawn::SpawnEvent;
    use std::io::Cursor;

    /// Send one JSON-RPC request line to `server` and parse its one reply.
    fn call(server: &Server, input: &str) -> Value {
        let mut output = Vec::new();
        server.run(Cursor::new(input), &mut output).unwrap();
        serde_json::from_str(String::from_utf8(output).unwrap().trim()).unwrap()
    }

    /// A `tools/call` request (id 1) for tool `name` with empty arguments.
    fn tools_call(name: &str) -> String {
        json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "tools/call",
            "params": {"name": name, "arguments": {}},
        })
        .to_string()
    }

    /// THE MCP TOOLS READ FROM THE BOUNDARY (spec 101): over a log holding 200,000 derived
    /// events and two superseded runs before the boundary, every tool that reads the run -
    /// `rigger_peers`, `rigger_activity` and the spawn-bound `rigger_scratch` - costs exactly one
    /// read of the run's own events plus the typed carry-over per call, asserted through the
    /// counting store double, and still answers from the whole carry-over.
    #[test]
    fn the_mcp_tools_read_the_run_from_its_boundary_and_the_carry_over_by_type() {
        use crate::test_support::{seed_one_shot_fixture, ReadCountingStore};

        let inner = Store::open(":memory:").unwrap();
        let fixture = seed_one_shot_fixture(&inner, "run", &[]);
        let store = ReadCountingStore::new(&inner);
        let driver = Driver::new();
        let server = Server::new(&driver, &store, "run");

        let peers = call(&server, &tools_call("rigger_peers"));
        let ids = |section: &str| -> Vec<String> {
            peers["result"]["structuredContent"][section]
                .as_array()
                .unwrap_or_else(|| panic!("a {section} array: {peers}"))
                .iter()
                .map(|v| v["id"].as_str().unwrap().to_string())
                .collect()
        };
        assert_eq!(ids("decisions"), ["d-a", "d-c"]);
        assert_eq!(ids("lessons"), ["l-a"]);
        assert_eq!(ids("findings"), ["f-b", "f-c"]);
        assert_eq!(store.reads(), fixture.reads("run", 1));

        let activity = call(&server, &tools_call("rigger_activity"));
        assert_eq!(activity["result"]["structuredContent"], json!([]));
        assert_eq!(store.reads(), fixture.reads("run", 2));

        let scratch = tempfile::tempdir().unwrap();
        let progress = Store::open(":memory:").unwrap();
        let bound = Server::new(&driver, &store, "run")
            .with_progress(&progress, scratch.path().to_str().unwrap())
            .with_spawn("u/implementer#0");
        let resp = call(&bound, &tools_call("rigger_scratch"));
        assert!(
            resp.get("result").is_some(),
            "rigger_scratch answers: {resp}"
        );
        assert_eq!(store.reads(), fixture.reads("run", 3));
        assert_eq!(store.materialized(), 3 * fixture.cost());
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

    /// A fresh workflow-bridge server (bound to `spawn` when given) answers `input` with a
    /// JSON-RPC error of `code`, echoing `id` when one is given; the reply is returned.
    fn assert_error_reply(
        spawn: Option<&str>,
        input: &str,
        id: Option<Value>,
        code: i64,
        why: &str,
    ) -> Value {
        let store = Store::open(":memory:").unwrap();
        let driver = Driver::new();
        let server = Server::new(&driver, &store, "run");
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

    /// The `DecisionMade` event a fresh workflow-bridge server stores for the `rigger_emit`
    /// request `input`.
    fn emitted_decision(input: &str) -> Event {
        let store = Store::open(":memory:").unwrap();
        let driver = Driver::new();
        let server = Server::new(&driver, &store, "run");
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
        let driver = Driver::new();
        let server = Server::new(&driver, &store, "run").with_spawn("u104-spawn-mcp/implementer#0");
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
        let driver = Driver::new();
        let server = Server::new(&driver, &store, "run");

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

    #[test]
    fn emit_is_stamped_with_the_serially_served_spawn_id() {
        // Reject-fix (spec 18, unit 3): the workflow live path correlates a gating adjudicator's
        // approve to THAT spawn by stamping its live `rigger_emit` with the served spawn's id
        // ([`META_SPAWN`]), the per-spawn signal the verdict-channel-mismatch backstop keys on.
        // The shim serves agents SERIALLY - `rigger_next` hands out one spawn, its agent runs
        // and emits, then `rigger_result` reports it - so every emit between those two calls is
        // that spawn's. This drives the server's own tools in that exact order and proves:
        //  - an emit while a spawn is being served carries META_SPAWN = the served spawn id;
        //  - after `rigger_result`, no spawn is being served, so a stray emit is UNSTAMPED
        //    (never misattributed to the just-finished spawn);
        //  - the NEXT served spawn stamps its own emits with its own id.
        use crate::conductor::{AgentDriver, SpawnOpts, META_SPAWN};
        use crate::config::AgentDef;
        use std::time::{Duration, Instant};

        let store = Store::open(":memory:").unwrap();
        let driver = Driver::new();
        let server = Server::new(&driver, &store, "run");

        let adj_id = "u/adjudicator#0";
        let sibling_id = "v/adjudicator#0";
        // Read the LAST DecisionMade whose data `id` matches, and return its META_SPAWN stamp.
        let stamp_of = |data_id: &str| -> Option<String> {
            let events = store.read_stream("run", 0, Direction::Forward).unwrap();
            events
                .iter()
                .rev()
                .find(|e| {
                    e.type_ == "DecisionMade"
                        && serde_json::from_slice::<Value>(&e.data)
                            .ok()
                            .and_then(|v| v["id"].as_str().map(str::to_string))
                            .as_deref()
                            == Some(data_id)
                })
                .and_then(|e| e.meta.get(META_SPAWN).cloned())
        };

        // The conductor side parks a gating spawn under its DETERMINISTIC opts.id and blocks
        // on its result - exactly as `run_reviewer` calls `driver.spawn`.
        fn serve(driver: &Driver, id: &str) {
            let emit = |_: &str, _: Value| Ok(());
            let _ = driver.spawn(
                &AgentDef {
                    id: "judge".into(),
                    ..Default::default()
                },
                "adjudicate",
                &SpawnOpts {
                    id: id.to_string(),
                    ..Default::default()
                },
                &emit,
            );
        }

        std::thread::scope(|s| {
            s.spawn(|| serve(&driver, adj_id));

            // The shim pulls the spawn via rigger_next (retry until the background thread has
            // queued it), which marks it the spawn now being served.
            let pull = |want: &str| {
                let deadline = Instant::now() + Duration::from_secs(2);
                loop {
                    let next = server.tool_next().unwrap();
                    if next["id"] == want {
                        return;
                    }
                    assert!(Instant::now() < deadline, "spawn {want:?} was never queued");
                    std::thread::sleep(Duration::from_millis(1));
                }
            };
            pull(adj_id);

            // An approve emitted WHILE `adj_id` is being served carries its stamp.
            server
                .tool_emit(&json!({"type":"DecisionMade","data":{"id":"a1","verdict":"approve"}}))
                .unwrap();
            assert_eq!(
                stamp_of("a1").as_deref(),
                Some(adj_id),
                "an emit while a spawn is served is stamped with THAT spawn's id"
            );

            // Report the result: the served spawn is done, so no emit belongs to it now.
            server
                .tool_result(&json!({"id": adj_id, "output": "done"}))
                .unwrap();
            server
                .tool_emit(&json!({"type":"DecisionMade","data":{"id":"stray"}}))
                .unwrap();
            assert_eq!(
                stamp_of("stray"),
                None,
                "an emit with no spawn being served is left UNSTAMPED, never misattributed"
            );

            // The NEXT served spawn stamps its own emits with its own id.
            s.spawn(|| serve(&driver, sibling_id));
            pull(sibling_id);
            server
                .tool_emit(&json!({"type":"DecisionMade","data":{"id":"a2","verdict":"approve"}}))
                .unwrap();
            assert_eq!(
                stamp_of("a2").as_deref(),
                Some(sibling_id),
                "the next served spawn stamps its emits with ITS id, not the previous spawn's"
            );
            server
                .tool_result(&json!({"id": sibling_id, "output": "done"}))
                .unwrap();
        });
    }

    /// Reject-fix (spec 104 review round 1): `stamp_current_spawn` shares
    /// `tool_emit_bound`'s latent gap - `obj.entry("meta").or_insert_with(...)` only runs its
    /// closure on a VACANT entry, so a caller-supplied non-object `meta` (e.g. a stray
    /// `meta: null`) occupies the key and the stamping `if let` used to skip it silently,
    /// landing the event with NO `META_SPAWN` at all while `rigger_emit` still reported
    /// success. Drives `tool_emit` (the workflow-bridge surface `rigger_emit` uses while a
    /// spawn is being served) with each non-object shape and proves the stamp still lands.
    #[test]
    fn stray_emit_with_non_object_meta_while_served_is_still_stamped() {
        use crate::conductor::META_SPAWN;

        for meta in [Value::Null, json!([]), json!("x")] {
            let store = Store::open(":memory:").unwrap();
            let driver = Driver::new();
            let server = Server::new(&driver, &store, "run");

            // A spawn currently being served: the exact state `rigger_next` leaves the
            // server in, set directly here since the next/result machinery that produces
            // it is proven separately by `emit_is_stamped_with_the_serially_served_spawn_id`.
            *server.current_spawn.lock().unwrap() = Some("adj/implementer#0".to_string());

            server
                .tool_emit(&json!({"type":"DecisionMade","data":{"id":"d1"},"meta":meta.clone()}))
                .unwrap();

            let events = store.read_stream("run", 0, Direction::Forward).unwrap();
            let recorded = events
                .iter()
                .find(|e| e.type_ == "DecisionMade")
                .expect("the emit must still land in the store");
            assert_eq!(
                recorded.meta.get(META_SPAWN).map(String::as_str),
                Some("adj/implementer#0"),
                "a non-object meta ({meta:?}) must be force-replaced and stamped, never \
                 silently dropped and left unattributed"
            );
        }
    }

    /// Reject-fix (spec 61 c10 round 2): the prior round's `meta.resolved_model` never
    /// reached a real driver because `tool_result` dropped `args.meta` entirely and
    /// `workflow::Driver::result` hardcoded `resolved_model: String::new()`. This drives
    /// the REAL `mcpserver.rs::tool_result` (not a mock server, unlike shim.test.mjs's
    /// coverage) exactly as a `rigger serve` / `rigger run --driver workflow` spawn does:
    /// the conductor side calls `AgentDriver::spawn` and blocks on its `AgentResult`; the
    /// shim side pulls the request via `rigger_next` and reports it via `rigger_result`
    /// with `meta.resolved_model` set, precisely the shape `runWorkflow` in shim.mjs
    /// sends. Proves the id lands on the `AgentResult` the conductor actually receives -
    /// the production sink this criterion owns end to end, not a test double's mock.
    #[test]
    fn tool_result_meta_resolved_model_reaches_the_real_agent_result() {
        use crate::conductor::{AgentDriver, AgentResult, SpawnOpts};
        use crate::config::AgentDef;
        use std::time::{Duration, Instant};

        let store = Store::open(":memory:").unwrap();
        let driver = Driver::new();
        let server = Server::new(&driver, &store, "run");

        let spawn_id = "u/implementer#0";
        let outcome: Mutex<Option<Result<AgentResult, crate::conductor::Error>>> = Mutex::new(None);

        std::thread::scope(|s| {
            // The conductor side: exactly what `run_single_stage` does - call
            // `AgentDriver::spawn` and block until the shim reports a result.
            s.spawn(|| {
                let emit = |_: &str, _: Value| Ok(());
                let r = driver.spawn(
                    &AgentDef {
                        id: "impl".into(),
                        ..Default::default()
                    },
                    "implement it",
                    &SpawnOpts {
                        id: spawn_id.to_string(),
                        ..Default::default()
                    },
                    &emit,
                );
                *outcome.lock().unwrap() = Some(r);
            });

            // The shim side: rigger_next (retry until queued) then rigger_result with
            // meta.resolved_model, exactly the shape shim.mjs's runWorkflow builds when
            // `resolvedModelFromUsage` observed exactly one authoritative model id.
            let deadline = Instant::now() + Duration::from_secs(2);
            loop {
                let next = server.tool_next().unwrap();
                if next["id"] == spawn_id {
                    break;
                }
                assert!(Instant::now() < deadline, "spawn was never queued");
                std::thread::sleep(Duration::from_millis(1));
            }
            server
                .tool_result(&json!({
                    "id": spawn_id,
                    "output": "done",
                    "meta": {"resolved_model": "claude-opus-4-8-20260101"}
                }))
                .unwrap();
        });

        let result = outcome
            .into_inner()
            .unwrap()
            .expect("the conductor's spawn() must have returned")
            .expect("a successful rigger_result must not error the spawn");
        assert_eq!(
            result.resolved_model, "claude-opus-4-8-20260101",
            "meta.resolved_model from the real tool_result call must reach the \
             AgentResult the conductor receives from AgentDriver::spawn, not be dropped"
        );
    }

    #[test]
    fn activity_tool_presents_the_live_per_agent_view() {
        // spec 14, criterion 3 (MCP half): `rigger_activity` returns the consolidated view
        // over MCP, so the shim gets each in-flight agent's activity + milestone with no
        // filesystem access of its own.
        let store = Store::open(":memory:").unwrap();
        let progress = Store::open(":memory:").unwrap();
        let driver = Driver::new();

        // A run: a unit started, its implementer parked (in-flight, no result yet).
        let run_id = crate::run_store::ensure_started(&store, &["crit".to_string()]).unwrap();
        store
            .append(
                "run",
                ExpectedRevision::Any,
                &[Event::new("UnitStarted", b"{\"id\":\"u\"}".to_vec())],
            )
            .unwrap();
        let req = crate::spawn::test_request("u", "u", "implementer", 0, "do it");
        store
            .append("run", ExpectedRevision::Any, &[req.to_event().unwrap()])
            .unwrap();

        // Its latest activity, in the SEPARATE progress store, scoped to the run.
        crate::progress_store::record(&progress, &run_id, &req.id, "grep #12: conductor.rs")
            .unwrap();

        // No scratch root in a unit test, so liveness ages are simply omitted from the view.
        let server = Server::new(&driver, &store, "run").with_progress(&progress, "");
        let input = r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"rigger_activity","arguments":{}}}"#;
        let mut output = Vec::new();
        server.run(Cursor::new(input), &mut output).unwrap();

        let resp: Value = serde_json::from_str(String::from_utf8(output).unwrap().trim()).unwrap();
        let view = &resp["result"]["structuredContent"];
        assert_eq!(
            view.as_array().map(|a| a.len()),
            Some(1),
            "one in-flight agent"
        );
        assert_eq!(view[0]["id"], req.id);
        assert_eq!(view[0]["stage"], "u");
        assert_eq!(view[0]["latest_activity"], "grep #12: conductor.rs");
        assert_eq!(view[0]["last_milestone"], "UnitStarted");
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
            Fold::NotFolded("graph: no context graph is wired".to_string()),
            "a surface wired with no graph folds nothing and says so"
        );

        // The MCP path: drive the same args through the server's rigger_emit tool.
        let mcp_store = Store::open(":memory:").unwrap();
        let driver = Driver::new();
        let server = Server::new(&driver, &mcp_store, "run");
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
        let driver = Driver::new();
        let server = Server::new(&driver, &store, "run");
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

    /// A `LessonLearned` whose `about` is an array of paths passes the shape check, is
    /// appended, and folds into the wired graph as a node reachable from the file it is about.
    #[test]
    fn emit_event_appends_and_folds_a_well_shaped_lesson() {
        use crate::contextgraph::{sqlite::Projector, Projection};

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
        let driver = Driver::new();
        // The CLI path: render through the shared core, scoped to a.rs.
        let core = peers_json(
            &Sidecar::read(&store, "run").unwrap(),
            &["a.rs".to_string()],
        );

        // The MCP path: the same scope through the rigger_peers tool.
        let server = Server::new(&driver, &store, "run");
        let resp = call(&server, PEERS_OF_A_RS);
        let tool = &resp["result"]["structuredContent"];

        assert_eq!(
            &core, tool,
            "the shared peers_json core and the MCP tool must return the same value"
        );
        assert_eq!(core["decisions"].as_array().unwrap().len(), 1);
        assert_eq!(core["decisions"][0]["id"], "da");
        // The lessons section is present and blast-radius scoped exactly like decisions.
        assert_eq!(
            core["lessons"].as_array().unwrap().len(),
            1,
            "`rigger peers a.rs` must return the a.rs lesson (the recovery the elision note names)"
        );
        assert_eq!(core["lessons"][0]["id"], "la");
    }

    #[test]
    fn emit_tool_folds_a_review_finding_into_the_wired_graph() {
        // The workflow-driver path's bridge from rigger_emit to the graph: when a
        // graph is wired, a ReviewFinding an agent emits folds into a KIND_FINDING node
        // the moment it lands, so an agent that grounds afterwards retrieves it via
        // graph_context (not via the conductor hand-threading prompts). De-noise (spec 43):
        // only the finding's CONTENT is projected (the node and its ABOUT edge to the code) -
        // the reviewer's provenance is NOT projected as a KIND_AGENT node or a REL_RAISED edge.
        use crate::contextgraph::{self, sqlite::Projector, Projection};

        let store = Store::open(":memory:").unwrap();
        let driver = Driver::new();
        let graph = Projector::open(":memory:", "test").unwrap();
        let server = Server::new(&driver, &store, "run").with_graph(&graph);

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
    /// a.rs: its `section` must hold exactly one entry, which is returned.
    fn sole_peer_of_a_rs<P: crate::sidecar::Peer>(
        type_: &str,
        seeded: [Value; 2],
        section: &str,
    ) -> Value {
        let store = Store::open(":memory:").unwrap();
        for data in seeded {
            append_run(&store, type_, data);
        }
        let driver = Driver::new();
        assert_eq!(Sidecar::read(&store, "run").unwrap().peers::<P>().len(), 2);
        let server = Server::new(&driver, &store, "run");

        let resp = call(&server, PEERS_OF_A_RS);
        let arr = resp["result"]["structuredContent"][section]
            .as_array()
            .unwrap_or_else(|| panic!("a {section} array: {resp}"));
        assert_eq!(
            arr.len(),
            1,
            "files=[a.rs] returns only the a.rs {section} entry: {resp}"
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
                "decisions",
            )["id"],
            "da"
        );
        // Item 4: rigger_peers surfaces peer review FINDINGS as well as decisions, so a
        // concurrent reviewer scoped to its files sees a finding about one of them.
        peers_tool_surfaces_findings_scoped_to_the_files_arg: {
            let finding = sole_peer_of_a_rs::<crate::sidecar::PeerFinding>(
                crate::contextgraph::TYPE_REVIEW_FINDING,
                [
                    json!({"id": "fa", "by": "lensA", "summary": "x", "about": ["a.rs"]}),
                    json!({"id": "fb", "by": "lensA", "summary": "x", "about": ["b.rs"]}),
                ],
                "findings",
            );
            assert_eq!(finding["id"], "fa");
            assert_eq!(finding["by"], "lensA");
        };
    }

    #[test]
    fn next_reports_done_only_after_the_conductor_finishes() {
        // rigger_next must distinguish "nothing queued yet" (done:false, keep
        // polling) from "the run is over" (done:true, exit). Before finish() an
        // empty next is done:false; after finish() it is done:true. This is what
        // stops the shim exiting before the first spawn is even enqueued.
        let store = Store::open(":memory:").unwrap();
        let driver = Driver::new();
        let server = Server::new(&driver, &store, "run");

        let call = r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"rigger_next","arguments":{}}}"#;

        // Before finish: empty id, done:false.
        let mut out = Vec::new();
        server.run(Cursor::new(call), &mut out).unwrap();
        let resp: Value = serde_json::from_str(String::from_utf8(out).unwrap().trim()).unwrap();
        let sc = &resp["result"]["structuredContent"];
        assert_eq!(sc["id"], "", "no spawn queued yet");
        assert_eq!(sc["done"], false, "a running conductor is not done: {resp}");

        // After finish: empty id, done:true.
        driver.finish();
        let mut out = Vec::new();
        server.run(Cursor::new(call), &mut out).unwrap();
        let resp: Value = serde_json::from_str(String::from_utf8(out).unwrap().trim()).unwrap();
        let sc = &resp["result"]["structuredContent"];
        assert_eq!(sc["id"], "", "still no spawn");
        assert_eq!(
            sc["done"], true,
            "a finished conductor reports done so the shim exits: {resp}"
        );
    }

    #[test]
    fn initialize_advertises_tools() {
        let store = Store::open(":memory:").unwrap();
        let driver = Driver::new();
        let server = Server::new(&driver, &store, "run");

        let input = "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"initialize\"}\n\
                     {\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"tools/list\"}";
        let mut output = Vec::new();
        server.run(Cursor::new(input), &mut output).unwrap();
        let text = String::from_utf8(output).unwrap();
        assert!(text.contains("rigger_next") && text.contains("rigger_emit"));
    }

    crate::test_cases! {
        // No spawn is pending, so id "999" is unknown. The shim must get an
        // error, not a silent success that would block the conductor forever.
        rigger_result_for_an_unknown_id_is_an_error: {
            let resp = assert_error_reply(
                None,
                r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"rigger_result","arguments":{"id":"999","output":"done"}}}"#,
                Some(json!(1)),
                -32602,
                "an unknown spawn id must be an invalid-params error",
            );
            assert!(resp.get("result").is_none(), "no success result: {resp}");
        };
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
    // `with_grounder` marks a `Server` as the operator's lookup surface, ONE dispatch and read
    // loop answering both tool surfaces - proven here at the unit level; `tests/cli.rs`'s
    // `mcp_*` tests prove the same thing end to end through the real `rigger mcp` binary.
    // =======================================================================================

    /// Without a grounder wired, `tool_list` is UNCHANGED from before this fix round: the
    /// workflow-driver bridge's usual five tools, in the same order - the regression backstop
    /// proving the DI extension never altered `rigger serve`'s own surface.
    #[test]
    fn tool_list_without_a_grounder_is_the_unchanged_workflow_surface() {
        let store = Store::open(":memory:").unwrap();
        let driver = Driver::new();
        let server = Server::new(&driver, &store, "run");

        let names = tool_names(&server);
        assert_eq!(
            names,
            vec![
                "rigger_next",
                "rigger_result",
                "rigger_emit",
                "rigger_peers",
                "rigger_activity"
            ]
        );
    }

    /// Wiring a grounder switches `tool_list` to exactly the operator's three lookups, in the
    /// order `rigger mcp` has always advertised them - never the workflow-lifecycle tools,
    /// which do not apply outside a run.
    #[test]
    fn tool_list_with_a_grounder_is_exactly_the_operator_lookup_surface() {
        use crate::grounder::Nop;

        let store = Store::open(":memory:").unwrap();
        let driver = Driver::new();
        let grounder = Nop;
        let server = Server::new(&driver, &store, "run").with_grounder(Ok(&grounder));

        let names = tool_names(&server);
        assert_eq!(names, vec!["rigger_peers", "rigger_ground", "rigger_graph"]);
    }

    /// The lookup surface answers `rigger_ground` through the wired [`Grounder`] port, and
    /// REJECTS a workflow-lifecycle tool name as unknown - it is not merely unadvertised, it
    /// is genuinely undispatchable on this surface, so an operator session can never emit onto
    /// the run's event stream through the MCP tool that exists to emit for LOOP agents.
    #[test]
    fn lookup_surface_serves_ground_and_rejects_workflow_tools() {
        use crate::grounder::Nop;

        let store = Store::open(":memory:").unwrap();
        let driver = Driver::new();
        let grounder = Nop;
        let server = Server::new(&driver, &store, "run").with_grounder(Ok(&grounder));

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

    crate::test_cases! {
        /// The SYMMETRIC direction of `lookup_surface_serves_ground_and_rejects_workflow_tools`
        /// (closes sdet-u92c4r2-workflow-surface-reject-of-ground-graph-untested): a `Server` built
        /// the workflow-driver way - no grounder, no graph wired, exactly what `rigger serve`/the
        /// loop's shim gets - must reject `rigger_ground`/`rigger_graph` as UNKNOWN TOOLS through
        /// `call_tool`'s own `(lookup, name)` gate, never reach `tool_ground`/`tool_graph`
        /// themselves. This matters beyond an unadvertised name: `tool_ground` `.expect()`s a
        /// grounder that is genuinely absent on this surface, so a future match-arm refactor that
        /// let either tool through would panic the whole server mid-run instead of answering
        /// `-32602` - this test is the one that would go red for that regression.
        workflow_surface_rejects_ground_and_graph_as_unknown_tools: for name in ["rigger_ground", "rigger_graph"] {
            assert_error_reply(
                None,
                &tools_call(name),
                None,
                -32602,
                &format!(
                    "{name} must be UNDISPATCHABLE (not merely unadvertised) on the workflow \
                     surface"
                ),
            );
        };
    }

    /// Reject-fix regression: `with_grounder(Err(..))` (the graceful-degrade path a caller
    /// takes when its OWN grounder resolution failed) still marks the lookup surface - the tool
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
        let driver = Driver::new();
        let server = Server::new(&driver, &store, "run")
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
            resp["result"]["structuredContent"]["decisions"][0]["id"], "d1",
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

    /// `rigger_graph`'s `show` selector on the lookup surface: seeds one real code-entity
    /// definition into a `Projector` (the same `CodeEntityExtracted` fold a real extraction
    /// pass produces) and drives it through `Server::call_tool`, proving the trait's new
    /// [`Projection::locate`] reaches a real caller holding only `&dyn Projection` - the DI
    /// extension this fix round adds, reusing the SAME `graph` port `with_graph` already wires
    /// for the event fold, rather than a second implementation reaching for the concrete
    /// `sqlite::Projector` from outside the crate.
    #[test]
    fn lookup_surface_rigger_graph_show_resolves_via_the_locate_trait_method() {
        use crate::contextgraph::sqlite::Projector;
        use crate::contextgraph::TYPE_CODE_ENTITY_EXTRACTED;
        use crate::grounder::Nop;

        let store = Store::open(":memory:").unwrap();
        let driver = Driver::new();
        let grounder = Nop;
        let graph = Projector::open(":memory:", "test").unwrap();
        let payload =
            r#"{"file":"src/widget.rs","name":"frobnicate","kind":"fn","line":7,"lang":"rust"}"#;
        let mut e = Event::new(TYPE_CODE_ENTITY_EXTRACTED, payload.as_bytes().to_vec());
        e.position = 1;
        crate::test_support::folds(&graph, std::slice::from_ref(&e));

        let server = Server::new(&driver, &store, "run")
            .with_graph(&graph)
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
        assert_eq!(structured["site"]["kind"], "fn");
    }

    /// Spec 101: a wired graph that owes its rebuild is refused by every tool whose answer depends
    /// on the fold - `rigger_graph` (both selectors) and `rigger_ground` - at once, naming
    /// `rigger setup` and never reading the projection, while `rigger_emit` on the same server
    /// still appends (its fold refused, the rebuild re-deriving it from the log).
    #[test]
    fn tools_that_depend_on_the_fold_refuse_a_graph_owing_its_rebuild_while_emit_appends() {
        use crate::contextgraph::{Error as GraphError, Graph, REBUILD_OWED};
        use crate::grounder::Nop;

        struct Owing;
        impl Projection for Owing {
            fn apply(
                &self,
                _: &Event,
                _access: crate::contextgraph::FoldAccess,
            ) -> Result<(), GraphError> {
                Err(GraphError(REBUILD_OWED.to_string()))
            }
            fn subgraph(&self, _: &[String], _: i64) -> Result<Graph, GraphError> {
                panic!("a graph owing its rebuild is never read")
            }
            fn resolve(&self, _: &str) -> Result<Option<String>, GraphError> {
                panic!("a graph owing its rebuild is never read")
            }
            fn rebuild_owed(&self) -> Result<bool, GraphError> {
                Ok(true)
            }
        }

        let store = Store::open(":memory:").unwrap();
        let driver = Driver::new();
        let grounder = Nop;
        let graph = Owing;
        let server = Server::new(&driver, &store, "run")
            .with_graph(&graph)
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
            json!({
                "position": log[0].position,
                "folded": false,
                "reason": format!("graph: {REBUILD_OWED}"),
            }),
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
        use crate::contextgraph::TYPE_CODE_ENTITY_EXTRACTED;
        use crate::grounder::Nop;

        let store = Store::open(":memory:").unwrap();
        let driver = Driver::new();
        let grounder = Nop;
        let graph = Projector::open(":memory:", "test").unwrap();
        for (pos, file) in [(1, "src/a.rs"), (2, "src/b.rs")] {
            let payload = format!(
                r#"{{"file":"{file}","name":"shared","kind":"fn","line":1,"lang":"rust"}}"#
            );
            let mut e = Event::new(TYPE_CODE_ENTITY_EXTRACTED, payload.into_bytes());
            e.position = pos;
            crate::test_support::folds(&graph, std::slice::from_ref(&e));
        }

        let server = Server::new(&driver, &store, "run")
            .with_graph(&graph)
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
    // spawn (`rigger mcp --spawn <id>`), adding a THIRD surface distinct from both the
    // workflow-driver bridge and the operator's lookup surface - proven here at the unit
    // level; `tests/cli.rs`'s `mcp_spawn_*` tests prove the same thing end to end through the
    // real `rigger mcp --spawn` binary.
    // =======================================================================================

    /// The spawn-bound surface advertises EXACTLY the six tools the design names, in the
    /// order it names them - never the workflow bridge's `rigger_next`/`rigger_result`/
    /// `rigger_activity` (there is no result tool: the session's own final message is the
    /// result) and never the lookup surface's narrower three.
    #[test]
    fn spawn_bound_tool_list_is_exactly_the_six_named_tools() {
        use crate::grounder::Nop;

        let store = Store::open(":memory:").unwrap();
        let driver = Driver::new();
        let progress = Store::open(":memory:").unwrap();
        let grounder = Nop;
        let server = Server::new(&driver, &store, "run")
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
    }

    crate::test_cases! {
        /// No result tool: `rigger_next` and `rigger_result` are UNDISPATCHABLE on the
        /// spawn-bound surface (not merely unadvertised) - a launched agent's session ends with
        /// its own final message, never a self-reported result over this MCP surface.
        spawn_bound_surface_has_no_result_tool: for name in ["rigger_next", "rigger_result", "rigger_activity"] {
            assert_error_reply(
                Some("u/implementer#0"),
                &tools_call(name),
                None,
                -32602,
                &format!("{name} must be UNDISPATCHABLE on the spawn-bound surface"),
            );
        };
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
        let driver = Driver::new();
        let server = Server::new(&driver, &store, "run").with_spawn("u/implementer#0");

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
        let driver = Driver::new();
        let progress = Store::open(":memory:").unwrap();
        let server = Server::new(&driver, &store, "run")
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
        let driver = Driver::new();
        let progress = Store::open(":memory:").unwrap();
        let server = Server::new(&driver, &store, "run")
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
        let driver = Driver::new();
        let progress = Store::open(":memory:").unwrap();
        let server = Server::new(&driver, &store, "run")
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
        let driver = Driver::new();
        let grounder = Nop;
        let graph = Projector::open(":memory:", "test").unwrap();
        let server = Server::new(&driver, &store, "run")
            .with_grounder(Ok(&grounder))
            .with_graph(&graph)
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
}
