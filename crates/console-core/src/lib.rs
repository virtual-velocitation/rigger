//! The Mission Control console's small, dependency-free WebAssembly calling surface (spec
//! 93, criterion 2: "THE MODULE AND ITS ABI"). This crate is the ONE member of the workspace
//! (besides the root `rigger` package itself) - see the root `Cargo.toml`'s own "THE MEMBER
//! CRATE" doc comment for why a bare `cargo build`/`test` at the repository root never
//! recurses into it. It depends on `rigger` with `default-features = false, features =
//! ["core"]` (THE PURE SUBSET, spec 93 criterion 1), so everything this crate calls is
//! already proven `wasm32-unknown-unknown`-clean by that criterion's own audit.
//!
//! [`dispatch`] is the WHOLE op-dispatch contract: a JSON op name plus a JSON input, in;
//! a JSON reply, out; never a panic (an unknown op, malformed input, or an unsatisfiable
//! query all answer with the documented `{"error": ...}` reply instead). It is plain,
//! pointer-free Rust, so it is the primary thing this crate's own tests exercise directly -
//! no `unsafe`, no allocator, no WebAssembly runtime needed to prove the CONTRACT.
//!
//! [`console_alloc`], [`console_free`], and [`console_call`] are the three (and only three -
//! THE MODULE AND ITS ABI's own words) `#[no_mangle] extern "C"` exports THE BUILD (spec 93
//! criterion 3) embeds into the running dash. They are a thin pointer-marshaling shell around
//! [`dispatch`]: real heap pointers in and out (so a page's `console_alloc` call hands back a
//! genuine `wasm32` linear-memory address a page can slice with nothing more than
//! `instance.exports.memory`), and `console_call`'s own `u64` return packs that reply
//! pointer's low 32 bits alongside the reply length - see [`pack`]'s own doc for why only the
//! LENGTH half of that packing is treated as meaningful on a native host.

use std::cell::RefCell;

use rigger::console;
use rigger::contextgraph::query::{graph_load, graph_query};
use rigger::contextgraph::Graph;
use rigger::eventstore::Event;

/// The remediation bound `console::fold` needs when a caller never supplies its own via
/// `fold_reset`'s `max_retries` field. Mirrors the same "unset -> 3" fallback
/// `conductor.rs`'s own `defaults.max_retries` design-intent note already documents for the
/// rest of the binary, so a page that never sends a bound reads blockers exactly as the CLI's
/// own default configuration would.
const DEFAULT_MAX_RETRIES: u32 = 3;

/// One page's whole console session: the accumulated event log (owned so `fold_push` can
/// append without a caller resending history every call), the configured remediation bound,
/// the last computed fold (what `view`/`statusline` answer from), and an optional loaded
/// graph (`graph_query`'s subject). `thread_local` per THE MEMBER CRATE's own design note - "a
/// page's module is single-threaded" - so this needs no lock and no `Send`/`Sync` bound.
struct ConsoleSession {
    events: Vec<Event>,
    max_retries: u32,
    current: console::ConsoleState,
    graph: Option<Graph>,
}

impl ConsoleSession {
    fn new() -> Self {
        ConsoleSession {
            events: Vec::new(),
            max_retries: DEFAULT_MAX_RETRIES,
            current: console::fold(&[], DEFAULT_MAX_RETRIES).unwrap_or_default(),
            graph: None,
        }
    }
}

thread_local! {
    static CONSOLE: RefCell<ConsoleSession> = RefCell::new(ConsoleSession::new());
}

/// The wire shape for one event a page sends into `fold_reset`/`fold_push`: only the three
/// fields any `core` fold actually reads (`console::fold` -> `ledger::project` /
/// `blocker::from_events`, both proven to read only `type_`, `data`, and `position` - see
/// this crate's own DecisionMade for the citation) - never the store-stamped fields
/// (`id`/`stream`/`meta`/`valid_from`/`recorded_at`/`revision`) a `core`-lane caller has no
/// business minting anyway.
#[derive(serde::Deserialize)]
struct WireEvent {
    #[serde(rename = "type")]
    type_: String,
    // `RawValue` captures `data`'s raw JSON text span WITHOUT building a `serde_json::Value`
    // tree (BUDGETS, spec 93 criterion 2: 10,000 events under 16ms) - `into_event` below then
    // hands its bytes straight to `Event::new` with zero re-serialization. A `Value` here
    // would cost a full parse AND, to get back to the `Vec<u8>` `Event::data` needs, a full
    // re-serialize on top - doubled work this op's own budget cannot afford.
    #[serde(default)]
    data: Option<Box<serde_json::value::RawValue>>,
    #[serde(default)]
    position: u64,
}

impl WireEvent {
    fn into_event(self) -> Event {
        let bytes = self
            .data
            .map(|d| d.get().as_bytes().to_vec())
            .unwrap_or_else(|| b"null".to_vec());
        let mut e = Event::new(self.type_, bytes);
        e.position = self.position;
        e
    }
}

fn error_reply(msg: impl Into<String>) -> Vec<u8> {
    let v = serde_json::json!({ "error": msg.into() });
    serde_json::to_vec(&v).unwrap_or_else(|_| b"{\"error\":\"console-core: internal\"}".to_vec())
}

fn ok_json(v: serde_json::Value) -> Vec<u8> {
    serde_json::to_vec(&v)
        .unwrap_or_else(|_| error_reply("console-core: reply serialization failed"))
}

/// The four facts `console::fold` computes, as the wire shape every `fold_*` op answers with.
/// ONE render authority (this function), so `fold_reset`/`fold_push`/`fold_at` never drift
/// from one another's reply shape.
fn fold_state_reply(state: &console::ConsoleState) -> serde_json::Value {
    serde_json::json!({
        "units": state.units,
        "blockers": state.blockers,
        "dock": state.dock.needs_you,
        "statusline": state.statusline,
    })
}

#[derive(serde::Deserialize)]
struct FoldResetInput {
    events: Vec<WireEvent>,
    #[serde(default)]
    max_retries: Option<u32>,
}

fn op_fold_reset(session: &mut ConsoleSession, input: &[u8]) -> Vec<u8> {
    let parsed: FoldResetInput = match serde_json::from_slice(input) {
        Ok(v) => v,
        Err(e) => return error_reply(format!("fold_reset: malformed input: {e}")),
    };
    let events: Vec<Event> = parsed
        .events
        .into_iter()
        .map(WireEvent::into_event)
        .collect();
    let max_retries = parsed.max_retries.unwrap_or(session.max_retries);
    match console::fold(&events, max_retries) {
        Ok(state) => {
            session.events = events;
            session.max_retries = max_retries;
            session.current = state;
            ok_json(fold_state_reply(&session.current))
        }
        Err(e) => error_reply(format!("fold_reset: {e}")),
    }
}

fn op_fold_push(session: &mut ConsoleSession, input: &[u8]) -> Vec<u8> {
    let ev: WireEvent = match serde_json::from_slice(input) {
        Ok(v) => v,
        Err(e) => return error_reply(format!("fold_push: malformed input: {e}")),
    };
    let mut events = session.events.clone();
    events.push(ev.into_event());
    match console::fold(&events, session.max_retries) {
        Ok(state) => {
            session.events = events;
            session.current = state;
            ok_json(fold_state_reply(&session.current))
        }
        Err(e) => error_reply(format!("fold_push: {e}")),
    }
}

#[derive(serde::Deserialize)]
struct FoldAtInput {
    position: u64,
}

fn op_fold_at(session: &mut ConsoleSession, input: &[u8]) -> Vec<u8> {
    let p: FoldAtInput = match serde_json::from_slice(input) {
        Ok(v) => v,
        Err(e) => return error_reply(format!("fold_at: malformed input: {e}")),
    };
    let slice: Vec<Event> = session
        .events
        .iter()
        .filter(|e| e.position <= p.position)
        .cloned()
        .collect();
    match console::fold(&slice, session.max_retries) {
        Ok(state) => {
            session.current = state.clone();
            ok_json(fold_state_reply(&state))
        }
        Err(e) => error_reply(format!("fold_at: {e}")),
    }
}

#[derive(serde::Deserialize)]
struct ViewInput {
    name: String,
}

/// `view` answers a NAMED facet of the last computed fold (`fold_reset`/`fold_push`/
/// `fold_at`'s own `current`), a generic, extensible accessor so a later spec (94-98) adds a
/// view NAME without ever changing this op's own shape. Only the facets `console::fold`
/// (spec 93 criterion 4) actually computes today (`units`, `blockers`, `dock`, `statusline`)
/// answer with real data; every other name (`theater`/`courtroom`/`plan`/`brief`/`health`/
/// `next_steps`, specs 94-98's own view models, per this spec's Notes: "consumed by specs
/// 94-98") is a documented `{"error": ...}` reply, the same "not yet available" disposition
/// this crate uses for the map ops (see [`dispatch`]'s own doc), never a stall or a stub
/// return shaped like real data.
fn op_view(session: &ConsoleSession, input: &[u8]) -> Vec<u8> {
    let v: ViewInput = match serde_json::from_slice(input) {
        Ok(v) => v,
        Err(e) => return error_reply(format!("view: malformed input: {e}")),
    };
    match v.name.as_str() {
        "units" => ok_json(serde_json::json!({ "units": session.current.units })),
        "blockers" => ok_json(serde_json::json!({ "blockers": session.current.blockers })),
        "dock" => ok_json(serde_json::json!({ "dock": session.current.dock.needs_you })),
        "statusline" => ok_json(serde_json::json!({ "statusline": session.current.statusline })),
        other => error_reply(format!("view: {other:?} is not yet available")),
    }
}

#[derive(serde::Deserialize)]
struct GraphQueryInput {
    kind: String,
    #[serde(default)]
    params: serde_json::Value,
}

fn op_graph_load(session: &mut ConsoleSession, input: &[u8]) -> Vec<u8> {
    match graph_load(input) {
        Ok(g) => {
            let nodes = g.nodes.len();
            let edges = g.edges.len();
            session.graph = Some(g);
            ok_json(serde_json::json!({ "ok": true, "nodes": nodes, "edges": edges }))
        }
        Err(e) => error_reply(format!("graph_load: {e}")),
    }
}

fn op_graph_query(session: &ConsoleSession, input: &[u8]) -> Vec<u8> {
    let q: GraphQueryInput = match serde_json::from_slice(input) {
        Ok(v) => v,
        Err(e) => return error_reply(format!("graph_query: malformed input: {e}")),
    };
    let Some(graph) = session.graph.as_ref() else {
        return error_reply("graph_query: no graph loaded - call graph_load first");
    };
    let params_bytes = serde_json::to_vec(&q.params).unwrap_or_default();
    match graph_query(graph, &q.kind, &params_bytes) {
        Ok(v) => ok_json(v),
        Err(e) => error_reply(format!("graph_query: {e}")),
    }
}

/// Answer one op call: `op` names one of the twelve ops THE MEMBER CRATE's design lists;
/// `input` is that op's raw JSON argument bytes. Never panics - every failure path (an
/// unknown op, malformed JSON, a graph query with no graph loaded yet) is a JSON `{"error":
/// ...}` reply instead, the ABI's own documented contract.
///
/// `map_build`/`map_frame`/`map_hit` (the map engine's ops) and `scrub_track`/
/// `palette_commands` (view models no unit in this spec's DAG builds - see `view`'s own doc)
/// answer with that SAME error-reply disposition rather than a stub shaped like real data:
/// spec 93's own plan-critique record (finding `rf-pc93r2-c2-map-ops-not-self-disclaimed`)
/// names exactly this stubbing as the correct, non-blocking disposition for the map ops
/// pending spec 84's own engine, and the identical reasoning ("no unit in this DAG claims
/// it") applies to the still-unbuilt view models specs 94-98 own.
fn dispatch(session: &mut ConsoleSession, op: &str, input: &[u8]) -> Vec<u8> {
    match op {
        "fold_reset" => op_fold_reset(session, input),
        "fold_push" => op_fold_push(session, input),
        "fold_at" => op_fold_at(session, input),
        "view" => op_view(session, input),
        "graph_load" => op_graph_load(session, input),
        "graph_query" => op_graph_query(session, input),
        "map_build" | "map_frame" | "map_hit" => error_reply(format!(
            "{op}: the map engine is spec 84's, not yet available"
        )),
        "scrub_track" => error_reply("scrub_track: not yet available"),
        "statusline" => ok_json(serde_json::json!({ "statusline": session.current.statusline })),
        "palette_commands" => error_reply("palette_commands: not yet available"),
        other => error_reply(format!("unknown op: {other}")),
    }
}

/// Allocate `len` bytes on the heap and hand back a real, stable pointer (never moved or
/// reclaimed until a matching [`console_free`]). On `wasm32-unknown-unknown` this pointer IS
/// a genuine linear-memory address (Rust's own allocator manages the module's one memory
/// region there, capped by the wasm spec itself at 4 GiB, so it always fits the `u32` a page
/// reads back) - the standard "no binding generator" wasm ABI shape: a page calls this to
/// reserve space, writes its request bytes at the returned address via
/// `instance.exports.memory.buffer`, then calls [`console_call`] with that same address.
#[no_mangle]
pub extern "C" fn console_alloc(len: usize) -> *mut u8 {
    let mut buf: Vec<u8> = vec![0u8; len];
    let ptr = buf.as_mut_ptr();
    std::mem::forget(buf);
    ptr
}

/// Release a region [`console_alloc`] returned. `ptr`/`len` must be EXACTLY a pair this
/// module's own `console_alloc` produced and has not already freed - the same contract every
/// alloc/free-pair-based ABI without a binding generator carries; a null `ptr` is a no-op
/// (never a defect: `console_call` never hands back a null reply pointer, but a page freeing
/// defensively should never crash on one).
///
/// # Safety
///
/// `ptr`/`len` must be exactly a pair [`console_alloc`] returned that has not already been
/// freed - the caller contract above, restated as the function's own safety requirement
/// (`unsafe` here is a Rust-side caller-discipline marker only; it changes nothing about the
/// exported C/wasm symbol a page or the wasm runtime calls).
#[no_mangle]
pub unsafe extern "C" fn console_free(ptr: *mut u8, len: usize) {
    if ptr.is_null() {
        return;
    }
    // SAFETY: per the documented contract above, `ptr`/`len` name exactly one live
    // allocation this module's own `console_alloc` produced with this same `len` - the
    // identical shape (`len`-capacity, `len`-length `Vec<u8>`) `console_alloc` built it as,
    // so reconstructing and dropping it here is exactly undoing that allocation.
    unsafe {
        drop(Vec::from_raw_parts(ptr, len, len));
    }
}

/// The pointer-marshaling core BOTH [`console_call`] (the exported, packed-`u64`-returning
/// ABI function) and this crate's own native tests share: read `op`/`input` as byte slices
/// (the caller's contract - see [`console_call`]'s own doc), run [`dispatch`], write the JSON
/// reply into a fresh [`console_alloc`]'d buffer, and hand back the REAL (untruncated)
/// pointer and length - never packed, so a same-crate native test can dereference it directly
/// without the native pointer-width loss [`pack`]'s own doc explains. One authority so
/// `console_call` is a thin, easily-eyeballed wrapper, never a second copy of this logic.
fn call_inner(op: &[u8], input: &[u8]) -> (*mut u8, usize) {
    let op_str = std::str::from_utf8(op).unwrap_or("");
    let reply = CONSOLE.with(|c| dispatch(&mut c.borrow_mut(), op_str, input));
    let ptr = console_alloc(reply.len());
    if !reply.is_empty() {
        // SAFETY: `ptr` was just allocated by `console_alloc(reply.len())` above, so it is a
        // live, exactly-`reply.len()`-byte, non-overlapping destination for `reply`'s own
        // bytes.
        unsafe {
            std::ptr::copy_nonoverlapping(reply.as_ptr(), ptr, reply.len());
        }
    }
    (ptr, reply.len())
}

/// Pack a reply pointer and length into `console_call`'s single `u64` return (a WebAssembly
/// function can return only one scalar, so packing two is the standard workaround a binding
/// generator would otherwise automate). The length half (the low 32 bits) is ALWAYS
/// meaningful - a JSON reply is never anywhere near 4 GiB. The pointer half (the high 32
/// bits) is meaningful ONLY on `wasm32-unknown-unknown`, where a real pointer is inherently
/// `<= u32::MAX` (the target's whole address space is capped there); on a native host a real
/// heap pointer routinely exceeds `u32::MAX` (empirically verified on this repository's own
/// host: freshly allocated pointers land past `0x5xxx_xxxx_xxxx`, roughly the 47th bit, well
/// outside a `u32`), so truncating it here is EXPECTED lossy plumbing, not a bug - this
/// crate's own native tests never reconstruct a pointer from this packed value; they use
/// [`call_inner`]'s untruncated return instead (see its own doc).
fn pack(ptr: *mut u8, len: usize) -> u64 {
    ((ptr as usize as u32 as u64) << 32) | (len as u32 as u64)
}

/// The exported op-call ABI (spec 93 criterion 2's `console_call`): `op_ptr`/`op_len` name
/// the op's name bytes, `in_ptr`/`in_len` its JSON argument bytes - both byte ranges the
/// caller owns and keeps alive for the duration of this call, the standard borrow-for-the-
/// call FFI shape. Returns the JSON reply's pointer and length packed via [`pack`] (see its
/// own doc for the packing's native-vs-wasm32 asymmetry). The reply itself is a fresh
/// [`console_alloc`]'d buffer the caller is responsible for releasing with [`console_free`]
/// once it has read it.
///
/// # Safety
///
/// `op_ptr`/`op_len` and `in_ptr`/`in_len` must each be either `(dangling_or_null, 0)` or a
/// live, readable byte range the caller owns for the duration of this call - the caller
/// contract above, restated as the function's own safety requirement.
#[no_mangle]
pub unsafe extern "C" fn console_call(
    op_ptr: *const u8,
    op_len: usize,
    in_ptr: *const u8,
    in_len: usize,
) -> u64 {
    // SAFETY: per the doc above, `op_ptr`/`op_len` and `in_ptr`/`in_len` are caller-owned,
    // live byte ranges for the duration of this call; an empty range never dereferences its
    // (possibly dangling/null) pointer at all.
    let op = if op_len == 0 {
        &[][..]
    } else {
        unsafe { std::slice::from_raw_parts(op_ptr, op_len) }
    };
    let input = if in_len == 0 {
        &[][..]
    } else {
        unsafe { std::slice::from_raw_parts(in_ptr, in_len) }
    };
    let (ptr, len) = call_inner(op, input);
    pack(ptr, len)
}

#[cfg(test)]
mod dispatch_tests {
    use super::*;

    fn call(session: &mut ConsoleSession, op: &str, input: &str) -> serde_json::Value {
        let bytes = dispatch(session, op, input.as_bytes());
        serde_json::from_slice(&bytes).expect("every dispatch reply must be valid JSON")
    }

    /// An unknown op name is the documented `{"error": ...}` reply, never a panic.
    #[test]
    fn unknown_op_answers_with_an_error_reply() {
        let mut s = ConsoleSession::new();
        let v = call(&mut s, "not-a-real-op", "{}");
        assert!(v.get("error").is_some(), "{v:?}");
    }

    /// Malformed JSON input to a real op is an error reply, never a panic - proven across
    /// every op that actually parses its input (the map/scrub/palette stubs ignore their
    /// input entirely, so they are not part of this claim).
    #[test]
    fn malformed_input_answers_with_an_error_reply_for_every_parsing_op() {
        let mut s = ConsoleSession::new();
        for op in [
            "fold_reset",
            "fold_push",
            "fold_at",
            "view",
            "graph_load",
            "graph_query",
        ] {
            let v = call(&mut s, op, "not json");
            assert!(
                v.get("error").is_some(),
                "op {op:?} did not error on malformed input: {v:?}"
            );
        }
    }

    /// `fold_reset` over a recorded stream answers `console::fold`'s own unit statuses,
    /// blockers, dock and statusline - not a second, independently derived copy.
    #[test]
    fn fold_reset_answers_console_folds_own_facts() {
        let mut s = ConsoleSession::new();
        let input = r#"{"events":[
            {"type":"UnitStarted","data":{"id":"u1"},"position":1},
            {"type":"UnitIntegrated","data":{"id":"u1","commit":"abc"},"position":2}
        ]}"#;
        let v = call(&mut s, "fold_reset", input);
        assert_eq!(v["units"]["u1"], "integrated", "{v:?}");
        assert_eq!(v["statusline"], "- . 1/1 units . done", "{v:?}");
        assert_eq!(v["blockers"], serde_json::json!([]), "{v:?}");
    }

    /// `fold_push` appends ONE event onto whatever `fold_reset` already loaded, and answers
    /// the RECOMPUTED fold over the whole accumulated stream - not just the pushed event.
    #[test]
    fn fold_push_appends_onto_the_existing_stream() {
        let mut s = ConsoleSession::new();
        call(
            &mut s,
            "fold_reset",
            r#"{"events":[{"type":"UnitStarted","data":{"id":"u1"},"position":1}]}"#,
        );
        let v = call(
            &mut s,
            "fold_push",
            r#"{"type":"UnitIntegrated","data":{"id":"u1","commit":"abc"},"position":2}"#,
        );
        assert_eq!(v["units"]["u1"], "integrated", "{v:?}");
    }

    /// `fold_at` answers the fold as of an EARLIER position without discarding the session's
    /// full history: a later `view` call still sees the position it scrubbed to (this op
    /// updates `current`), but the underlying event log is untouched.
    #[test]
    fn fold_at_answers_a_historical_position() {
        let mut s = ConsoleSession::new();
        call(
            &mut s,
            "fold_reset",
            r#"{"events":[
                {"type":"UnitStarted","data":{"id":"u1"},"position":1},
                {"type":"UnitIntegrated","data":{"id":"u1","commit":"abc"},"position":2}
            ]}"#,
        );
        let at_1 = call(&mut s, "fold_at", r#"{"position":1}"#);
        assert_eq!(at_1["units"]["u1"], "grounding", "{at_1:?}");
        assert_eq!(
            s.events.len(),
            2,
            "fold_at must not truncate the stored log"
        );
    }

    /// `view` answers a single named facet of the last computed fold.
    #[test]
    fn view_answers_a_single_named_facet() {
        let mut s = ConsoleSession::new();
        call(
            &mut s,
            "fold_reset",
            r#"{"events":[{"type":"UnitStarted","data":{"id":"u1"},"position":1}]}"#,
        );
        let v = call(&mut s, "view", r#"{"name":"statusline"}"#);
        assert_eq!(v["statusline"], "u1 . 0/1 units . working", "{v:?}");
    }

    /// A `view` name naming a not-yet-built view model (specs 94-98's own) is an error
    /// reply, never a stub shaped like real data.
    #[test]
    fn view_of_an_unbuilt_view_model_answers_with_an_error_reply() {
        let mut s = ConsoleSession::new();
        let v = call(&mut s, "view", r#"{"name":"theater"}"#);
        assert!(v.get("error").is_some(), "{v:?}");
    }

    /// `statusline` is a direct shortcut to the SAME field `view("statusline")` answers -
    /// proven identical, never a second derivation.
    #[test]
    fn statusline_op_matches_view_statusline() {
        let mut s = ConsoleSession::new();
        call(
            &mut s,
            "fold_reset",
            r#"{"events":[{"type":"UnitStarted","data":{"id":"u1"},"position":1}]}"#,
        );
        let direct = call(&mut s, "statusline", "{}");
        let via_view = call(&mut s, "view", r#"{"name":"statusline"}"#);
        assert_eq!(
            direct["statusline"], via_view["statusline"],
            "{direct:?} {via_view:?}"
        );
    }

    /// Every one of the three map ops answers with the documented error-reply stub (spec
    /// 84's own engine, not yet available), never a panic and never data shaped like a real
    /// map frame.
    #[test]
    fn map_ops_answer_with_a_not_yet_available_error_reply() {
        let mut s = ConsoleSession::new();
        for op in ["map_build", "map_frame", "map_hit"] {
            let v = call(&mut s, op, "{}");
            assert!(v.get("error").is_some(), "op {op:?}: {v:?}");
        }
    }

    /// `scrub_track` and `palette_commands` - view models no unit in this spec's DAG builds
    /// - likewise answer with an error reply rather than fabricated data.
    #[test]
    fn scrub_track_and_palette_commands_answer_with_a_not_yet_available_error_reply() {
        let mut s = ConsoleSession::new();
        for op in ["scrub_track", "palette_commands"] {
            let v = call(&mut s, op, "{}");
            assert!(v.get("error").is_some(), "op {op:?}: {v:?}");
        }
    }

    /// `graph_query` before any `graph_load` is an error reply, never a panic on a `None`
    /// graph.
    #[test]
    fn graph_query_before_graph_load_answers_with_an_error_reply() {
        let mut s = ConsoleSession::new();
        let v = call(
            &mut s,
            "graph_query",
            r#"{"kind":"search","params":{"query":"x"}}"#,
        );
        assert!(v.get("error").is_some(), "{v:?}");
    }

    /// `graph_load` then `graph_query` answers the SAME result the library's own
    /// `contextgraph::query::graph_query` returns for the same graph and kind - proven via
    /// the `search` kind (no fixture-graph seed needed beyond a couple of nodes).
    #[test]
    fn graph_load_then_graph_query_answers_the_librarys_own_result() {
        use rigger::contextgraph::{Graph, Node, KIND_FILE};
        let g = Graph {
            nodes: vec![Node {
                id: "src/a.rs".to_string(),
                kind: KIND_FILE.to_string(),
                attrs: Default::default(),
            }],
            edges: Vec::new(),
        };
        let payload = serde_json::to_vec(&g).unwrap();
        let mut s = ConsoleSession::new();
        let loaded = call(&mut s, "graph_load", std::str::from_utf8(&payload).unwrap());
        assert_eq!(loaded["nodes"], 1, "{loaded:?}");
        let v = call(
            &mut s,
            "graph_query",
            r#"{"kind":"search","params":{"query":"src/a.rs"}}"#,
        );
        let direct = graph_query(&g, "search", br#"{"query":"src/a.rs"}"#).unwrap();
        assert_eq!(v, direct, "{v:?} != {direct:?}");
    }

    /// `graph_query` for an unknown kind is an error reply, never a panic.
    #[test]
    fn graph_query_unknown_kind_answers_with_an_error_reply() {
        use rigger::contextgraph::Graph;
        let g = Graph {
            nodes: Vec::new(),
            edges: Vec::new(),
        };
        let payload = serde_json::to_vec(&g).unwrap();
        let mut s = ConsoleSession::new();
        call(&mut s, "graph_load", std::str::from_utf8(&payload).unwrap());
        let v = call(
            &mut s,
            "graph_query",
            r#"{"kind":"no-such-kind","params":{}}"#,
        );
        assert!(v.get("error").is_some(), "{v:?}");
    }
}

/// THE MODULE AND ITS ABI: the exported `console_alloc`/`console_free`/`console_call` three,
/// exercised through their real, `#[no_mangle] extern "C"` signatures - real pointers in and
/// out, no shortcuts through `dispatch` directly - proving the pointer-marshaling shell
/// itself, not just the pure logic it wraps.
#[cfg(test)]
mod abi_tests {
    use super::*;

    unsafe fn write_bytes(ptr: *mut u8, bytes: &[u8]) {
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), ptr, bytes.len());
    }

    /// `console_alloc` hands back a writable, readable buffer of exactly the requested
    /// length; `console_free` releases it without crashing.
    #[test]
    fn console_alloc_round_trips_written_bytes_and_console_free_releases_it() {
        let bytes = b"hello console";
        let ptr = console_alloc(bytes.len());
        assert!(!ptr.is_null());
        unsafe {
            write_bytes(ptr, bytes);
            let read = std::slice::from_raw_parts(ptr, bytes.len());
            assert_eq!(read, bytes);
            console_free(ptr, bytes.len());
        }
    }

    /// `console_alloc(0)` is a valid, non-null, zero-length allocation - a page's own
    /// dispatch of an op with no JSON body (`"{}"` still has bytes, but this proves the
    /// degenerate case never panics).
    #[test]
    fn console_alloc_of_zero_length_does_not_panic() {
        let ptr = console_alloc(0);
        unsafe {
            console_free(ptr, 0);
        }
    }

    /// `console_free` on a null pointer is a documented no-op, never a crash.
    #[test]
    fn console_free_of_a_null_pointer_is_a_no_op() {
        unsafe {
            console_free(std::ptr::null_mut(), 0);
        }
    }

    /// `call_inner` (the pointer-marshaling core `console_call` wraps) answers the SAME JSON
    /// `dispatch` itself would, read back through its own untruncated pointer - the ABI
    /// shell adds no behavior of its own.
    #[test]
    fn call_inner_matches_dispatch_through_real_pointers() {
        let op = b"statusline";
        let input = b"{}";
        let (ptr, len) = call_inner(op, input);
        let reply: serde_json::Value = unsafe {
            let v = serde_json::from_slice(std::slice::from_raw_parts(ptr, len)).unwrap();
            console_free(ptr, len);
            v
        };
        let mut s = ConsoleSession::new();
        let direct: serde_json::Value =
            serde_json::from_slice(&dispatch(&mut s, "statusline", input)).unwrap();
        assert_eq!(reply, direct, "{reply:?} != {direct:?}");
    }

    /// The EXPORTED `console_call` (real pointers in, `u64`-packed reply out) is exercised
    /// through its literal signature end to end: allocate op/input buffers via
    /// `console_alloc`, write the bytes a page would, call `console_call`, and check the
    /// packed reply's length half - the one half [`pack`]'s own doc says is meaningful on
    /// every host, native included. Full CONTENT round-tripping through the packed pointer
    /// is proven instead by `call_inner_matches_dispatch_through_real_pointers` above (see
    /// [`pack`]'s doc for why a native pointer cannot itself survive this packing).
    #[test]
    fn exported_console_call_answers_a_correctly_lengthed_packed_reply() {
        let op = b"unknown-op-for-this-test";
        let input = b"{}";
        let op_ptr = console_alloc(op.len());
        let in_ptr = console_alloc(input.len());
        unsafe {
            write_bytes(op_ptr, op);
            write_bytes(in_ptr, input);
        }

        let packed = unsafe {
            console_call(
                op_ptr as *const u8,
                op.len(),
                in_ptr as *const u8,
                input.len(),
            )
        };
        let len = (packed & 0xFFFF_FFFF) as usize;

        let mut s = ConsoleSession::new();
        let want = dispatch(&mut s, std::str::from_utf8(op).unwrap(), input);
        assert_eq!(
            len,
            want.len(),
            "packed length half must match dispatch's own reply length"
        );
        assert!(len > 0, "an error reply is never empty");

        unsafe {
            console_free(op_ptr, op.len());
            console_free(in_ptr, input.len());
        }
    }

    /// `console_call` with empty op/input ranges never dereferences a dangling pointer - the
    /// empty-slice short circuit in both `console_call` and `call_inner`'s callers.
    #[test]
    fn console_call_with_empty_ranges_does_not_crash() {
        let packed = unsafe { console_call(std::ptr::null(), 0, std::ptr::null(), 0) };
        let len = (packed & 0xFFFF_FFFF) as usize;
        assert!(
            len > 0,
            "an empty op name must still answer an unknown-op error reply"
        );
    }
}

/// BUDGETS (spec 93 criterion 2's own, per `d-spec93-design-owns-query-relocation-and-fold-
/// budget`): a fold of 10,000 console events completes in under 16ms natively in release
/// mode, asserted here by driving `fold_reset` (the bulk load path) exactly as a page would
/// on first opening a run - never a loop of 10,000 individual `fold_push` calls, which is a
/// different (and not budgeted) access pattern.
#[cfg(test)]
mod budget_tests {
    use super::*;

    /// A REPRESENTATIVE 10,000-event console stream (BUDGETS' own "10,000 console events"),
    /// not a synthetic worst case: a real rigger run's log is dominated by non-lifecycle
    /// events (`DecisionMade`, findings, progress - the bulk of what agents actually emit,
    /// this very unit's own session included) over a MODEST number of units, never one
    /// distinct unit per event. `console::fold`'s two consumers (`ledger::project`,
    /// `blocker::budget_halt`) both skip a non-lifecycle type with a single string-compare
    /// miss (see `ledger.rs`'s own `apply` match and `blocker.rs`'s own `budget_halt` match) -
    /// so this fixture's `DecisionMade` majority exercises exactly that cheap path, while its
    /// 40-unit lifecycle minority exercises the real projection work, matching this repo's
    /// own observed run-log shape rather than an artificial all-lifecycle stream no real run
    /// ever produces.
    const UNITS: u64 = 40;
    const EVENTS_PER_UNIT: u64 = 10_000 / UNITS;

    fn ten_thousand_events_reset_input() -> String {
        let mut events = String::from(r#"{"events":["#);
        for i in 0..(UNITS * EVENTS_PER_UNIT) {
            if i > 0 {
                events.push(',');
            }
            let unit = i / EVENTS_PER_UNIT;
            let offset = i % EVENTS_PER_UNIT;
            let (type_, data) = if offset == 0 {
                ("UnitStarted", format!(r#"{{"id":"u{unit}"}}"#))
            } else if offset == EVENTS_PER_UNIT / 2 && unit.is_multiple_of(5) {
                (
                    "UnitFailed",
                    format!(r#"{{"id":"u{unit}","attempts":1,"cause":"reject"}}"#),
                )
            } else if offset == EVENTS_PER_UNIT - 1 && !unit.is_multiple_of(9) {
                (
                    "UnitIntegrated",
                    format!(r#"{{"id":"u{unit}","commit":"c{i:08x}"}}"#),
                )
            } else {
                (
                    "DecisionMade",
                    format!(
                        r#"{{"id":"d-u{unit}-{offset}","summary":"a representative decision body of roughly the length a real implementer or reviewer records, naming what changed and why, for unit u{unit} at step {offset}","governs":["src/example.rs"],"supersedes":""}}"#
                    ),
                )
            };
            events.push_str(&format!(
                r#"{{"type":"{type_}","data":{data},"position":{i}}}"#
            ));
        }
        events.push_str("]}");
        events
    }

    /// Correctness (every profile): `fold_reset` over the 10,000-event fixture answers
    /// without error and folds every one of its `UNITS` distinct units, with every unit whose
    /// number is not a multiple of 9 integrated (matching the fixture's own generation rule).
    #[test]
    fn fold_reset_of_ten_thousand_events_answers_correctly() {
        let input = ten_thousand_events_reset_input();
        let mut s = ConsoleSession::new();
        let bytes = dispatch(&mut s, "fold_reset", input.as_bytes());
        let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert!(v.get("error").is_none(), "{v:?}");
        let units = v["units"].as_object().unwrap();
        assert_eq!(units.len(), UNITS as usize, "{units:?}");
        assert_eq!(units["u1"], "integrated", "{units:?}");
        assert_eq!(
            units["u9"], "grounding",
            "unit 9 must NOT be integrated: {units:?}"
        );
    }

    /// THE BUDGET ITSELF: under 16ms, release mode only (`cfg(not(debug_assertions))` is
    /// rustc's own release-mode signal - a debug build's unoptimized fold is legitimately
    /// much slower and asserting the bound there would be a flaky, meaningless test, exactly
    /// why the spec's own wording scopes the bound to "natively in release mode").
    #[test]
    #[cfg(not(debug_assertions))]
    fn fold_reset_of_ten_thousand_events_completes_under_16ms_in_release_mode() {
        let input = ten_thousand_events_reset_input();
        let mut s = ConsoleSession::new();
        let start = std::time::Instant::now();
        let bytes = dispatch(&mut s, "fold_reset", input.as_bytes());
        let elapsed = start.elapsed();
        let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert!(v.get("error").is_none(), "{v:?}");
        assert!(
            elapsed.as_millis() < 16,
            "fold_reset of 10,000 events took {elapsed:?}, over the 16ms budget"
        );
    }
}
