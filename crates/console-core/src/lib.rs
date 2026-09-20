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

/// One page's whole console session: the accumulated event log (owned so `fold_push` can
/// append without a caller resending history every call), the configured remediation bound
/// AS THE CALLER GAVE IT (`0` means "never set" - the exact sentinel `console::fold` ->
/// `blocker::from_events` -> `blocker::effective_max_retries` already resolves to
/// `safety::MAX_RETRIES` internally), the last computed fold (what `view`/`statusline` answer
/// from), and an optional loaded graph (`graph_query`'s subject). `thread_local` per THE
/// MEMBER CRATE's own design note - "a page's module is single-threaded" - so this needs no
/// lock and no `Send`/`Sync` bound.
///
/// This crate resolves nothing itself: `blocker::effective_max_retries` is the ONE place
/// `0` maps to `safety::MAX_RETRIES`, so `max_retries` here is threaded straight through
/// verbatim, never shadowed by a second, independently-maintained copy of that constant.
struct ConsoleSession {
    events: Vec<Event>,
    max_retries: u32,
    current: console::ConsoleState,
    graph: Option<Graph>,
    /// The last `map_build`-computed map engine model (spec 84 criterion 1: districts, rank,
    /// world-space layout) - `map_frame` renders a screen-space [`console::map::DrawList`] from
    /// this at whatever zoom it is called with, exactly like `graph`/`graph_query` above.
    map: Option<console::map::MapModel>,
    /// The viewport `(w, h)` the last `map_build` call fixed - the ABI doc's own grouping,
    /// `map_build(w,h)` then `map_frame(cam,sel)`: the viewport is set once at build time, not
    /// resent on every frame (a resize calls `map_build` again). `(0.0, 0.0)` before any
    /// `map_build` call, which `map_frame` never reaches anyway (it errors first - no map).
    map_viewport: (f64, f64),
}

impl ConsoleSession {
    fn new() -> Self {
        ConsoleSession {
            events: Vec::new(),
            max_retries: 0,
            current: console::fold(&[], 0).unwrap_or_default(),
            graph: None,
            map: None,
            map_viewport: (0.0, 0.0),
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
    // ADDITIVE to `d-u94c2-wire-event-shape` (d-u94c3-wire-event-recorded-at): the wall
    // clock second this event was originally recorded, needed by `console::scrub_track`'s
    // hour ticks (`Event::mint_time`'s own core-lane doc names exactly this: "a caller that
    // needs a real timestamp on a core-built Event... reads it off the wire it decoded the
    // event from"). Optional and defaulted so a caller that omits it - every snapshot
    // recorded before this criterion, and every existing test above - still folds
    // correctly; `scrub_track` degrades to no ticks rather than erroring on its absence.
    #[serde(default)]
    recorded_at: Option<u64>,
}

impl WireEvent {
    fn into_event(self) -> Event {
        let bytes = self
            .data
            .map(|d| d.get().as_bytes().to_vec())
            .unwrap_or_else(|| b"null".to_vec());
        let mut e = Event::new(self.type_, bytes);
        e.position = self.position;
        if let Some(secs) = self.recorded_at {
            e.recorded_at =
                std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(secs);
        }
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
/// `scrub_track()` (spec 94 c3, THE POSITION MODEL): the session's accumulated event log's
/// marks and hour ticks, straight from `console::scrub_track` - not a second, independently
/// derived copy. Takes no input (like `map_legend`) and never errors: an empty/fresh session
/// answers the empty track, since no event log is a legitimate state, not a caller mistake.
fn op_scrub_track(session: &ConsoleSession) -> Vec<u8> {
    let track = console::scrub_track(&session.events);
    ok_json(serde_json::json!({ "marks": track.marks, "ticks": track.ticks }))
}

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
            // A reload invalidates any map `map_build` computed from the PRIOR graph (adv-u84c1-
            // map-stale-after-graph-reload): the page re-fetches the graph payload per index stamp
            // while the session persists (docs/architecture-addendum-mission-control.md's own "THE
            // GRAPH PAYLOAD" note - this is the documented normal flow, not an edge case), so
            // `map_frame` must never silently keep answering the pre-reload model. Clearing both
            // here forces the same "call map_build first" error `map_frame` already gives a session
            // that never built one - never a second staleness check duplicated at the read site.
            session.map = None;
            session.map_viewport = (0.0, 0.0);
            ok_json(serde_json::json!({ "ok": true, "nodes": nodes, "edges": edges }))
        }
        Err(e) => error_reply(format!("graph_load: {e}")),
    }
}

/// The eight `map_*` kinds spec 84 criterion 2 adds to `graph_query` (the Explore rail's own
/// four candidate lists, search, and the three camera RESETS - `fit_whole_map`/`fit_district`/
/// `fit_entity`, the last added round 2 to close the rail/search fly-to-entity gap): routed here
/// rather than to the library's own `contextgraph::query::graph_query` (which knows nothing of a
/// built map, only a `Graph`), because `graph_query` is this crate's ONE extensible multi-kind
/// query op - THE MODULE AND ITS ABI's own twelve-op budget has no room for eight more named
/// ops, and `graph_query` already exists precisely to carry a `kind` + arbitrary `params`
/// payload. Every kind here needs `session.map` (`map_build` fills it); `map_argued_about`
/// additionally reads `session.graph` directly, always `Some` whenever `session.map` is
/// (`map_build` itself requires a loaded graph, and a reload clears both together - see
/// `op_graph_load`'s own doc).
const MAP_QUERY_KINDS: &[&str] = &[
    "map_landmarks",
    "map_bridges",
    "map_argued_about",
    "map_changing",
    "map_search",
    "map_fit_whole",
    "map_fit_district",
    "map_fit_entity",
];

/// A rail chip's own reasonable working set when a caller omits `limit` entirely - unlike
/// [`MapBuildInput`]/[`MapFrameInput`]'s numeric fields (where a `0.0` degenerate viewport/zoom
/// is a harmless no-op a real caller never actually sends), an OMITTED `limit` is a plausible
/// real mistake and a silent `0` would make every rail chip look permanently empty - a caller
/// that truly wants zero candidates can still ask for `"limit":0` explicitly.
fn default_rail_limit() -> usize {
    8
}

/// Shared `limit` shape for `map_landmarks`/`map_bridges`/`map_argued_about` (Design, EXPLORE
/// RAIL).
#[derive(serde::Deserialize)]
struct RailLimitParams {
    #[serde(default = "default_rail_limit")]
    limit: usize,
}

impl Default for RailLimitParams {
    fn default() -> Self {
        RailLimitParams {
            limit: default_rail_limit(),
        }
    }
}

/// `map_changing`'s own extra field: the entity ids the CALLER already knows the live run is
/// touching right now (Design, EXPLORE RAIL: "Changing right now (the live run's blast radius)"),
/// since this crate has no run-lifecycle state of its own to derive it from (see
/// [`console::map::changing_right_now`]'s own doc for why that fn takes the same set as a plain
/// parameter rather than reaching into a run's event log itself). An omitted `touched` is the
/// documented EMPTY-STATE (no run live), not a parsing error.
#[derive(serde::Deserialize)]
struct ChangingParams {
    #[serde(default = "default_rail_limit")]
    limit: usize,
    #[serde(default)]
    touched: Vec<String>,
}

impl Default for ChangingParams {
    fn default() -> Self {
        ChangingParams {
            limit: default_rail_limit(),
            touched: Vec::new(),
        }
    }
}

/// `map_search`'s own params (Design, EXPLORE RAIL: "a search box (prefix and substring, kind
/// and degree beside each hit)"). An omitted `query` is the empty string, which
/// [`console::map::search`] itself already answers with no hits - never a parsing error.
#[derive(serde::Deserialize)]
struct SearchParams {
    #[serde(default)]
    query: String,
    #[serde(default = "default_rail_limit")]
    limit: usize,
}

impl Default for SearchParams {
    fn default() -> Self {
        SearchParams {
            query: String::new(),
            limit: default_rail_limit(),
        }
    }
}

/// `map_fit_district`'s own param: the district `purpose` (from a prior `map_hit`'s own
/// `Hit::District`, or one of `map_frame`'s own district pills) to fit the camera to. An empty/
/// omitted `purpose` simply matches no real district - [`console::map::fit_district`]'s own
/// documented `None` contract, answered as an error reply here rather than a crash.
#[derive(serde::Deserialize, Default)]
struct FitDistrictParams {
    #[serde(default)]
    purpose: String,
}

/// `map_fit_entity`'s own param (round 2): the entity `id` - a rail chip's or search hit's own
/// `id` field, echoed straight back - to fly the camera to. An empty/omitted `id` simply matches
/// no real entity - [`console::map::fit_entity`]'s own documented `None` contract, answered as
/// an error reply here rather than a crash, the SAME discipline [`FitDistrictParams`] already
/// keeps for its own `purpose` field.
#[derive(serde::Deserialize, Default)]
struct FitEntityParams {
    #[serde(default)]
    id: String,
}

fn op_map_query(session: &ConsoleSession, kind: &str, params: &serde_json::Value) -> Vec<u8> {
    let Some(model) = session.map.as_ref() else {
        return error_reply(format!(
            "graph_query: {kind}: no map built - call map_build first"
        ));
    };
    match kind {
        "map_landmarks" => {
            let p: RailLimitParams = serde_json::from_value(params.clone()).unwrap_or_default();
            ok_json(serde_json::json!({ "candidates": console::map::landmarks(model, p.limit) }))
        }
        "map_bridges" => {
            let p: RailLimitParams = serde_json::from_value(params.clone()).unwrap_or_default();
            ok_json(serde_json::json!({
                "candidates": console::map::bridges_between_districts(model, p.limit)
            }))
        }
        "map_argued_about" => {
            let p: RailLimitParams = serde_json::from_value(params.clone()).unwrap_or_default();
            let Some(graph) = session.graph.as_ref() else {
                return error_reply(
                    "graph_query: map_argued_about: no graph loaded - call graph_load first",
                );
            };
            ok_json(serde_json::json!({
                "candidates": console::map::argued_about_in_review(model, graph, p.limit)
            }))
        }
        "map_changing" => {
            let p: ChangingParams = serde_json::from_value(params.clone()).unwrap_or_default();
            let touched: std::collections::BTreeSet<String> = p.touched.into_iter().collect();
            ok_json(serde_json::json!({
                "candidates": console::map::changing_right_now(model, &touched, p.limit)
            }))
        }
        "map_search" => {
            let p: SearchParams = serde_json::from_value(params.clone()).unwrap_or_default();
            ok_json(serde_json::json!({ "hits": console::map::search(model, &p.query, p.limit) }))
        }
        "map_fit_whole" => {
            let cam = console::map::fit_whole_map(model);
            ok_json(serde_json::json!({ "cx": cam.cx, "cy": cam.cy, "zoom": cam.zoom }))
        }
        "map_fit_district" => {
            let p: FitDistrictParams = serde_json::from_value(params.clone()).unwrap_or_default();
            let (w, h) = session.map_viewport;
            match console::map::fit_district(model, w, h, &p.purpose) {
                Some(cam) => {
                    ok_json(serde_json::json!({ "cx": cam.cx, "cy": cam.cy, "zoom": cam.zoom }))
                }
                None => error_reply(format!(
                    "graph_query: map_fit_district: unknown district {:?}",
                    p.purpose
                )),
            }
        }
        "map_fit_entity" => {
            let p: FitEntityParams = serde_json::from_value(params.clone()).unwrap_or_default();
            let (w, h) = session.map_viewport;
            match console::map::fit_entity(model, w, h, &p.id) {
                Some(cam) => {
                    ok_json(serde_json::json!({ "cx": cam.cx, "cy": cam.cy, "zoom": cam.zoom }))
                }
                None => error_reply(format!(
                    "graph_query: map_fit_entity: unknown entity {:?}",
                    p.id
                )),
            }
        }
        other => error_reply(format!("graph_query: unreachable map kind {other:?}")),
    }
}

/// `graph_query`'s `map_legend` kind (spec 84 criterion 3): answered here, BEFORE either the
/// [`MAP_QUERY_KINDS`] "no map built" check or the "no graph loaded" check below, because
/// [`console::map::legend`] is static content - the SAME rows for every session, needing neither
/// a loaded graph nor a built map. Every other `map_*` kind genuinely reads `session.map`, so it
/// stays inside [`MAP_QUERY_KINDS`]/[`op_map_query`]; `map_legend` does not, so it is not one of
/// them - a caller can fetch the legend the instant a page loads, before it ever sees real data.
const LEGEND_QUERY_KIND: &str = "map_legend";

fn op_graph_query(session: &ConsoleSession, input: &[u8]) -> Vec<u8> {
    let q: GraphQueryInput = match serde_json::from_slice(input) {
        Ok(v) => v,
        Err(e) => return error_reply(format!("graph_query: malformed input: {e}")),
    };
    if q.kind == LEGEND_QUERY_KIND {
        return ok_json(serde_json::json!({ "entries": console::map::legend() }));
    }
    if MAP_QUERY_KINDS.contains(&q.kind.as_str()) {
        return op_map_query(session, &q.kind, &q.params);
    }
    let Some(graph) = session.graph.as_ref() else {
        return error_reply("graph_query: no graph loaded - call graph_load first");
    };
    let params_bytes = serde_json::to_vec(&q.params).unwrap_or_default();
    match graph_query(graph, &q.kind, &params_bytes) {
        Ok(v) => ok_json(v),
        Err(e) => error_reply(format!("graph_query: {e}")),
    }
}

/// The `map_build(w,h)` wire input (spec 84 criterion 1): the viewport `map_frame` renders into
/// until the next `map_build` call. Both default to `0.0` on a malformed/omitted field rather
/// than erroring the op - a degenerate viewport yields a degenerate (not a crashing) frame.
#[derive(serde::Deserialize, Default)]
struct MapBuildInput {
    #[serde(default)]
    w: f64,
    #[serde(default)]
    h: f64,
}

/// Build the map engine's model (spec 84 criterion 1) from whatever `graph_load` last loaded, and
/// fix the viewport every subsequent `map_frame` call renders into. Errors when no graph is
/// loaded yet - the same "call the loader first" contract `graph_query` already keeps.
fn op_map_build(session: &mut ConsoleSession, input: &[u8]) -> Vec<u8> {
    let p: MapBuildInput = match serde_json::from_slice(input) {
        Ok(v) => v,
        Err(e) => return error_reply(format!("map_build: malformed input: {e}")),
    };
    let Some(graph) = session.graph.as_ref() else {
        return error_reply("map_build: no graph loaded - call graph_load first");
    };
    let model = console::map::build(graph);
    let districts = model.districts.len();
    let entities = model.entities.len();
    session.map_viewport = (p.w, p.h);
    session.map = Some(model);
    ok_json(serde_json::json!({ "ok": true, "districts": districts, "entities": entities }))
}

/// The `map_frame(cam,sel)` wire input, now WHOLE (spec 84 criterion 2 extends criterion 1's
/// zoom-only `cam` with the pan `cx`/`cy`, plus the `sel` half): `zoom`/`cx`/`cy` together are
/// [`console::map::Camera`] (`zoom <= 0.0` is the full-extent camera and ignores `cx`/`cy` - see
/// that type's own doc), and `sel` is the selected entity id (absent/`null` = no selection). All
/// four default on a malformed/omitted field rather than erroring the op - a degenerate camera
/// yields a degenerate (not a crashing) frame, the same discipline [`MapBuildInput`] already
/// keeps for its own numeric fields.
#[derive(serde::Deserialize, Default)]
struct MapFrameInput {
    #[serde(default)]
    zoom: f64,
    #[serde(default)]
    cx: f64,
    #[serde(default)]
    cy: f64,
    #[serde(default)]
    sel: Option<String>,
}

/// Render one frame of the last `map_build`-computed model at the given camera/selection, using
/// the viewport that build call fixed. Errors when no map has been built yet.
fn op_map_frame(session: &ConsoleSession, input: &[u8]) -> Vec<u8> {
    let p: MapFrameInput = match serde_json::from_slice(input) {
        Ok(v) => v,
        Err(e) => return error_reply(format!("map_frame: malformed input: {e}")),
    };
    let Some(model) = session.map.as_ref() else {
        return error_reply("map_frame: no map built - call map_build first");
    };
    let (w, h) = session.map_viewport;
    let camera = console::map::Camera {
        cx: p.cx,
        cy: p.cy,
        zoom: p.zoom,
    };
    let draw = console::map::frame(model, w, h, &camera, p.sel.as_deref());
    match serde_json::to_value(&draw) {
        Ok(v) => ok_json(v),
        Err(e) => error_reply(format!("map_frame: reply serialization failed: {e}")),
    }
}

/// The `map_hit(cam,sel,x,y)` wire input: the SAME camera/selection shape [`MapFrameInput`]
/// takes, plus the SCREEN-space click point being tested.
#[derive(serde::Deserialize, Default)]
struct MapHitInput {
    #[serde(default)]
    zoom: f64,
    #[serde(default)]
    cx: f64,
    #[serde(default)]
    cy: f64,
    #[serde(default)]
    sel: Option<String>,
    #[serde(default)]
    x: f64,
    #[serde(default)]
    y: f64,
}

/// Hit-test a click against the last `map_build`-computed model, at the given camera/selection -
/// spec 84 criterion 2's own engine ([`console::map::hit`], which reuses `frame`'s own projection
/// so this can never drift from what a page actually draws). Errors when no map has been built
/// yet, the SAME contract `map_frame` already keeps. The reply is tagged (`"hit": "entity"` with
/// an `id`, `"hit": "district"` with a `purpose`, or `"hit": "none"`) so a page never has to infer
/// which shape it got.
fn op_map_hit(session: &ConsoleSession, input: &[u8]) -> Vec<u8> {
    let p: MapHitInput = match serde_json::from_slice(input) {
        Ok(v) => v,
        Err(e) => return error_reply(format!("map_hit: malformed input: {e}")),
    };
    let Some(model) = session.map.as_ref() else {
        return error_reply("map_hit: no map built - call map_build first");
    };
    let (w, h) = session.map_viewport;
    let camera = console::map::Camera {
        cx: p.cx,
        cy: p.cy,
        zoom: p.zoom,
    };
    match console::map::hit(model, w, h, &camera, p.sel.as_deref(), p.x, p.y) {
        Some(console::map::Hit::Entity(id)) => {
            ok_json(serde_json::json!({ "hit": "entity", "id": id }))
        }
        Some(console::map::Hit::District(purpose)) => {
            ok_json(serde_json::json!({ "hit": "district", "purpose": purpose }))
        }
        None => ok_json(serde_json::json!({ "hit": "none" })),
    }
}

/// Answer one op call: `op` names one of the twelve ops THE MEMBER CRATE's design lists;
/// `input` is that op's raw JSON argument bytes. Never panics - every failure path (an
/// unknown op, malformed JSON, a graph query with no graph loaded yet) is a JSON `{"error":
/// ...}` reply instead, the ABI's own documented contract.
///
/// `map_build`/`map_frame` are spec 84 criterion 1's own engine (districts, rank, label
/// placement - see [`console::map`]'s own doc); `map_frame`'s pan/selection half and `map_hit`
/// (hit-testing) are criterion 2's, per the plan-critique record `u84-plan-dag-c1-c2-c3-c4`: "c2
/// ... adds map_hit on top of c1's entity model" - real data now, not the stubs spec 93/c1 left
/// them as. Criterion 2 also adds five `map_*` `kind`s to `graph_query` (the Explore rail's four
/// candidate lists plus search - see [`MAP_QUERY_KINDS`]'s own doc for why they ride `graph_query`
/// rather than a new op). `scrub_track` (spec 94 criterion 3, THE POSITION MODEL) answers real
/// marks/ticks now, straight from [`console::scrub_track`] - not the spec-93 stub any more.
/// `palette_commands` (criterion 4's own territory) still answers the documented error-reply
/// stub rather than fabricated data.
fn dispatch(session: &mut ConsoleSession, op: &str, input: &[u8]) -> Vec<u8> {
    match op {
        "fold_reset" => op_fold_reset(session, input),
        "fold_push" => op_fold_push(session, input),
        "fold_at" => op_fold_at(session, input),
        "view" => op_view(session, input),
        "graph_load" => op_graph_load(session, input),
        "graph_query" => op_graph_query(session, input),
        "map_build" => op_map_build(session, input),
        "map_frame" => op_map_frame(session, input),
        "map_hit" => op_map_hit(session, input),
        "scrub_track" => op_scrub_track(session),
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

    /// A `fold_reset` that never sends `max_retries` (the field omitted entirely, `None`
    /// after parsing) resolves through the ONE remaining authority - `console::fold` ->
    /// `blocker::from_events` -> `blocker::effective_max_retries` mapping the unset `0` this
    /// crate threads through to `safety::MAX_RETRIES` - never a second, locally-minted
    /// fallback constant. Proven by a `Failed` unit's rendered blocker line naming the SAME
    /// bound `safety::MAX_RETRIES` carries today (3), so a drift in that one authority is
    /// exactly what would break this test, never a copy console-core keeps in step by hand.
    #[test]
    fn fold_reset_without_max_retries_uses_blockers_own_default_authority() {
        let mut s = ConsoleSession::new();
        let input = r#"{"events":[
            {"type":"UnitStarted","data":{"id":"u1"},"position":1},
            {"type":"UnitFailed","data":{"id":"u1","attempts":1,"cause":"reject"},"position":2}
        ]}"#;
        let v = call(&mut s, "fold_reset", input);
        assert_eq!(
            v["blockers"],
            serde_json::json!(["u1: reject-recurrence #1/3 (reject)"]),
            "{v:?}"
        );
    }

    /// `FoldResetInput.max_retries`'s `Some(n)` override branch, previously untested
    /// (sdet-u93c2-max-retries-override-untested): a caller-supplied bound is honored end to
    /// end through `console::fold`/`blocker::classify`, not silently replaced by whatever
    /// `blocker::effective_max_retries` would otherwise pick - proven with a value (7) that
    /// differs from `safety::MAX_RETRIES` (3), so the two paths cannot be confused.
    #[test]
    fn fold_reset_honors_an_explicit_max_retries_override() {
        let mut s = ConsoleSession::new();
        let input = r#"{"events":[
            {"type":"UnitStarted","data":{"id":"u1"},"position":1},
            {"type":"UnitFailed","data":{"id":"u1","attempts":1,"cause":"reject"},"position":2}
        ],"max_retries":7}"#;
        let v = call(&mut s, "fold_reset", input);
        assert_eq!(
            v["blockers"],
            serde_json::json!(["u1: reject-recurrence #1/7 (reject)"]),
            "{v:?}"
        );
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

    /// `map_hit` before any `map_build` is the SAME "call map_build first" error reply
    /// `map_frame` already answers - never a panic on a `None` map.
    #[test]
    fn map_hit_before_map_build_answers_an_error_reply() {
        let mut s = ConsoleSession::new();
        let v = call(&mut s, "map_hit", "{}");
        assert!(v.get("error").is_some(), "{v:?}");
    }

    /// A `graph_load`ed + `map_build`-built session with one district, two entities, one CALLS
    /// edge between them (worktree.rs::spawn_worktree -> worktree.rs::remove_worktree) - the
    /// shared fixture every `map_hit`/`graph_query` `map_*`-kind test below builds on, so each
    /// test's own assertions stay about the OP's wiring, not about re-deriving a fixture graph.
    fn built_map_session() -> ConsoleSession {
        use rigger::contextgraph::{
            Edge, Graph, Node, KIND_CODE_ENTITY, REL_CALLS, REL_IN_COMMUNITY, TIER_EXTRACTED,
        };
        use rigger::eventstore::Position;
        let code = |id: &str, name: &str| Node {
            id: id.to_string(),
            kind: KIND_CODE_ENTITY.to_string(),
            attrs: [
                ("name".to_string(), name.to_string()),
                ("kind".to_string(), "function".to_string()),
            ]
            .into_iter()
            .collect(),
        };
        let membership = |id: &str| Edge {
            from: id.to_string(),
            to: "community/1/0".to_string(),
            rel: REL_IN_COMMUNITY.to_string(),
            valid_from: 0,
            valid_to: None,
            source: Position::default(),
            tier: TIER_EXTRACTED.to_string(),
        };
        let g = Graph {
            nodes: vec![
                code("src/worktree.rs::spawn_worktree", "spawn_worktree"),
                code("src/worktree.rs::remove_worktree", "remove_worktree"),
            ],
            edges: vec![
                membership("src/worktree.rs::spawn_worktree"),
                membership("src/worktree.rs::remove_worktree"),
                Edge {
                    from: "src/worktree.rs::spawn_worktree".to_string(),
                    to: "src/worktree.rs::remove_worktree".to_string(),
                    rel: REL_CALLS.to_string(),
                    valid_from: 0,
                    valid_to: None,
                    source: Position::default(),
                    tier: TIER_EXTRACTED.to_string(),
                },
            ],
        };
        let payload = serde_json::to_vec(&g).unwrap();
        let mut s = ConsoleSession::new();
        call(&mut s, "graph_load", std::str::from_utf8(&payload).unwrap());
        call(&mut s, "map_build", r#"{"w":800,"h":600}"#);
        s
    }

    /// `map_hit` at the exact screen position `map_frame` itself just placed an entity's dot
    /// answers that SAME entity - proven by comparing against `console::map::hit`'s own direct
    /// answer for the identical camera/point, never a hardcoded id.
    #[test]
    fn map_hit_answers_the_entity_the_click_landed_on() {
        let mut s = built_map_session();
        let frame = call(&mut s, "map_frame", r#"{"zoom":0}"#);
        let entity = &frame["entities"][0];
        let x = entity["x"].as_f64().unwrap();
        let y = entity["y"].as_f64().unwrap();
        let v = call(
            &mut s,
            "map_hit",
            &format!(r#"{{"zoom":0,"x":{x},"y":{y}}}"#),
        );
        assert_eq!(v["hit"], "entity", "{v:?}");
        assert_eq!(v["id"], entity["id"].clone(), "{v:?}");
    }

    /// A click nowhere near anything answers `{"hit": "none"}`, never an error and never a
    /// fabricated id.
    #[test]
    fn map_hit_of_empty_space_answers_hit_none() {
        let mut s = built_map_session();
        let v = call(&mut s, "map_hit", r#"{"zoom":0,"x":-1.0e9,"y":-1.0e9}"#);
        assert_eq!(v["hit"], "none", "{v:?}");
    }

    /// A malformed `map_hit` input is an error reply, never a panic - the SAME per-parsing-op
    /// discipline the other real ops already keep.
    #[test]
    fn map_hit_answers_an_error_reply_on_malformed_input() {
        let mut s = ConsoleSession::new();
        let v = call(&mut s, "map_hit", "not json");
        assert!(v.get("error").is_some(), "{v:?}");
    }

    /// Every `map_*` `graph_query` kind before `map_build` is the same "call map_build first"
    /// error reply - proven once across all eight, mirroring
    /// `malformed_input_answers_with_an_error_reply_for_every_parsing_op`'s own uniform-property
    /// pattern.
    #[test]
    fn graph_query_map_kinds_before_map_build_answer_with_an_error_reply() {
        for kind in [
            "map_landmarks",
            "map_bridges",
            "map_argued_about",
            "map_changing",
            "map_search",
            "map_fit_whole",
            "map_fit_district",
            "map_fit_entity",
        ] {
            let mut s = ConsoleSession::new();
            let v = call(
                &mut s,
                "graph_query",
                &format!(r#"{{"kind":"{kind}","params":{{}}}}"#),
            );
            assert!(v.get("error").is_some(), "kind {kind:?}: {v:?}");
        }
    }

    /// `graph_query`'s `map_landmarks` kind answers the SAME candidates `console::map::landmarks`
    /// itself returns for the identical model - proven via the real session/dispatch path, not
    /// just the library call.
    #[test]
    fn graph_query_map_landmarks_matches_the_librarys_own_result() {
        let mut s = built_map_session();
        let v = call(
            &mut s,
            "graph_query",
            r#"{"kind":"map_landmarks","params":{"limit":5}}"#,
        );
        let candidates = v["candidates"].as_array().unwrap();
        assert_eq!(candidates.len(), 2, "{v:?}");
        let direct = console::map::landmarks(s.map.as_ref().unwrap(), 5);
        assert_eq!(candidates[0]["id"], direct[0].id, "{v:?}");
    }

    /// `graph_query`'s `map_bridges` kind: the fixture's two entities are in the SAME (only)
    /// district, so neither has a cross-district edge - the candidate list must be empty, not
    /// merely non-erroring.
    #[test]
    fn graph_query_map_bridges_is_empty_when_nothing_crosses_a_district() {
        let mut s = built_map_session();
        let v = call(
            &mut s,
            "graph_query",
            r#"{"kind":"map_bridges","params":{}}"#,
        );
        assert_eq!(v["candidates"], serde_json::json!([]), "{v:?}");
    }

    /// `graph_query`'s `map_changing` kind: empty `touched` answers an empty list (Design's own
    /// EMPTY-STATE); a `touched` naming one of the fixture's own entities answers exactly that
    /// one.
    #[test]
    fn graph_query_map_changing_filters_to_the_callers_own_touched_list() {
        let mut s = built_map_session();
        let empty = call(
            &mut s,
            "graph_query",
            r#"{"kind":"map_changing","params":{}}"#,
        );
        assert_eq!(empty["candidates"], serde_json::json!([]), "{empty:?}");

        let filled = call(
            &mut s,
            "graph_query",
            r#"{"kind":"map_changing","params":{"touched":["src/worktree.rs::spawn_worktree"]}}"#,
        );
        let candidates = filled["candidates"].as_array().unwrap();
        assert_eq!(candidates.len(), 1, "{filled:?}");
        assert_eq!(
            candidates[0]["id"], "src/worktree.rs::spawn_worktree",
            "{filled:?}"
        );
    }

    /// `graph_query`'s `map_argued_about` kind reads live `KIND_FINDING`/`REL_ABOUT` edges
    /// straight off `session.graph` (never a second copy) - proven by loading a graph that
    /// carries a finding pinned to one of the fixture's own entities.
    #[test]
    fn graph_query_map_argued_about_reads_pinned_findings_from_the_loaded_graph() {
        use rigger::contextgraph::{
            Edge, Graph, Node, KIND_CODE_ENTITY, KIND_FINDING, REL_ABOUT, REL_IN_COMMUNITY,
            TIER_EXTRACTED,
        };
        use rigger::eventstore::Position;
        let code = |id: &str, name: &str| Node {
            id: id.to_string(),
            kind: KIND_CODE_ENTITY.to_string(),
            attrs: [
                ("name".to_string(), name.to_string()),
                ("kind".to_string(), "function".to_string()),
            ]
            .into_iter()
            .collect(),
        };
        let g = Graph {
            nodes: vec![
                code("src/worktree.rs::spawn_worktree", "spawn_worktree"),
                Node {
                    id: "f1".to_string(),
                    kind: KIND_FINDING.to_string(),
                    attrs: Default::default(),
                },
            ],
            edges: vec![
                Edge {
                    from: "src/worktree.rs::spawn_worktree".to_string(),
                    to: "community/1/0".to_string(),
                    rel: REL_IN_COMMUNITY.to_string(),
                    valid_from: 0,
                    valid_to: None,
                    source: Position::default(),
                    tier: TIER_EXTRACTED.to_string(),
                },
                Edge {
                    from: "f1".to_string(),
                    to: "src/worktree.rs::spawn_worktree".to_string(),
                    rel: REL_ABOUT.to_string(),
                    valid_from: 0,
                    valid_to: None,
                    source: Position::default(),
                    tier: TIER_EXTRACTED.to_string(),
                },
            ],
        };
        let payload = serde_json::to_vec(&g).unwrap();
        let mut s = ConsoleSession::new();
        call(&mut s, "graph_load", std::str::from_utf8(&payload).unwrap());
        call(&mut s, "map_build", r#"{"w":800,"h":600}"#);

        let v = call(
            &mut s,
            "graph_query",
            r#"{"kind":"map_argued_about","params":{}}"#,
        );
        let candidates = v["candidates"].as_array().unwrap();
        assert_eq!(candidates.len(), 1, "{v:?}");
        assert_eq!(
            candidates[0]["id"], "src/worktree.rs::spawn_worktree",
            "{v:?}"
        );
    }

    /// `graph_query`'s `map_search` kind answers hits matching the fixture's own entity name,
    /// carrying kind and degree - and an empty query answers no hits.
    #[test]
    fn graph_query_map_search_matches_by_name() {
        let mut s = built_map_session();
        let v = call(
            &mut s,
            "graph_query",
            r#"{"kind":"map_search","params":{"query":"spawn"}}"#,
        );
        let hits = v["hits"].as_array().unwrap();
        assert_eq!(hits.len(), 1, "{v:?}");
        assert_eq!(hits[0]["id"], "src/worktree.rs::spawn_worktree", "{v:?}");

        let empty = call(
            &mut s,
            "graph_query",
            r#"{"kind":"map_search","params":{"query":""}}"#,
        );
        assert_eq!(empty["hits"], serde_json::json!([]), "{empty:?}");
    }

    /// `graph_query`'s `map_fit_whole` kind answers the SAME camera
    /// `console::map::fit_whole_map` itself returns for the built model - the full-extent
    /// sentinel (`zoom: 0`) centred on the model's own bounds.
    #[test]
    fn graph_query_map_fit_whole_matches_the_librarys_own_result() {
        let mut s = built_map_session();
        let v = call(
            &mut s,
            "graph_query",
            r#"{"kind":"map_fit_whole","params":{}}"#,
        );
        let direct = console::map::fit_whole_map(s.map.as_ref().unwrap());
        assert_eq!(v["cx"].as_f64(), Some(direct.cx), "{v:?}");
        assert_eq!(v["cy"].as_f64(), Some(direct.cy), "{v:?}");
        assert_eq!(v["zoom"].as_f64(), Some(direct.zoom), "{v:?}");
    }

    /// `graph_query`'s `map_fit_district` kind answers the SAME camera
    /// `console::map::fit_district` itself returns for the fixture's own (only) district, using
    /// the session's own `map_build`-fixed viewport - and an unknown purpose is an error reply,
    /// never a panic.
    #[test]
    fn graph_query_map_fit_district_matches_the_librarys_own_result() {
        let mut s = built_map_session();
        let purpose = s.map.as_ref().unwrap().districts[0].purpose.clone();
        let v = call(
            &mut s,
            "graph_query",
            &format!(r#"{{"kind":"map_fit_district","params":{{"purpose":"{purpose}"}}}}"#),
        );
        let direct =
            console::map::fit_district(s.map.as_ref().unwrap(), 800.0, 600.0, &purpose).unwrap();
        assert_eq!(v["cx"].as_f64(), Some(direct.cx), "{v:?}");
        assert_eq!(v["cy"].as_f64(), Some(direct.cy), "{v:?}");
        assert_eq!(v["zoom"].as_f64(), Some(direct.zoom), "{v:?}");

        let unknown = call(
            &mut s,
            "graph_query",
            r#"{"kind":"map_fit_district","params":{"purpose":"no-such-district"}}"#,
        );
        assert!(unknown.get("error").is_some(), "{unknown:?}");
    }

    /// `graph_query`'s `map_fit_entity` kind (round 2) answers the SAME camera
    /// `console::map::fit_entity` itself returns for the fixture's own entity, using the
    /// session's own `map_build`-fixed viewport - and an unknown id is an error reply, never a
    /// panic, mirroring `map_fit_district`'s own success/error shape exactly.
    #[test]
    fn graph_query_map_fit_entity_matches_the_librarys_own_result() {
        let mut s = built_map_session();
        let id = s.map.as_ref().unwrap().entities[0].id.clone();
        let v = call(
            &mut s,
            "graph_query",
            &format!(r#"{{"kind":"map_fit_entity","params":{{"id":"{id}"}}}}"#),
        );
        let direct = console::map::fit_entity(s.map.as_ref().unwrap(), 800.0, 600.0, &id).unwrap();
        assert_eq!(v["cx"].as_f64(), Some(direct.cx), "{v:?}");
        assert_eq!(v["cy"].as_f64(), Some(direct.cy), "{v:?}");
        assert_eq!(v["zoom"].as_f64(), Some(direct.zoom), "{v:?}");

        let unknown = call(
            &mut s,
            "graph_query",
            r#"{"kind":"map_fit_entity","params":{"id":"no-such-entity"}}"#,
        );
        assert!(unknown.get("error").is_some(), "{unknown:?}");
    }

    /// `graph_query`'s `map_legend` kind (spec 84 criterion 3) answers the SAME rows
    /// `console::map::legend` itself returns, on a completely FRESH session - no `graph_load`,
    /// no `map_build` - proving the legend needs neither, unlike every other `map_*` kind above.
    #[test]
    fn graph_query_map_legend_matches_the_librarys_own_result_on_a_fresh_session() {
        let mut s = ConsoleSession::new();
        let v = call(
            &mut s,
            "graph_query",
            r#"{"kind":"map_legend","params":{}}"#,
        );
        let entries = v["entries"].as_array().expect("{v:?}");
        let direct = console::map::legend();
        assert_eq!(entries.len(), direct.len(), "{v:?}");
        for (wired, row) in entries.iter().zip(direct.iter()) {
            assert_eq!(wired["id"], row.id, "{v:?}");
            assert_eq!(wired["treatment"], row.treatment, "{v:?}");
        }
    }

    /// `map_build`/`map_frame` before any `graph_load` are error replies, never a panic on a
    /// `None` graph/map - the same "call the loader first" contract `graph_query` already keeps.
    #[test]
    fn map_build_and_map_frame_before_graph_load_answer_with_an_error_reply() {
        let mut s = ConsoleSession::new();
        let build = call(&mut s, "map_build", r#"{"w":800,"h":600}"#);
        assert!(build.get("error").is_some(), "{build:?}");
        let frame = call(&mut s, "map_frame", r#"{"zoom":0}"#);
        assert!(frame.get("error").is_some(), "{frame:?}");
    }

    /// A `graph_load`ed graph with a live community membership, then `map_build`, answers real
    /// district/entity counts - not the spec-93 stub. `map_frame` with no prior `map_build` still
    /// errors even once a graph is loaded (the two calls are independently gated).
    #[test]
    fn map_build_answers_real_district_and_entity_counts_from_the_loaded_graph() {
        use rigger::contextgraph::{
            Edge, Graph, Node, KIND_CODE_ENTITY, REL_IN_COMMUNITY, TIER_EXTRACTED,
        };
        use rigger::eventstore::Position;
        let g = Graph {
            nodes: vec![Node {
                id: "src/worktree.rs::spawn_worktree".to_string(),
                kind: KIND_CODE_ENTITY.to_string(),
                attrs: [
                    ("name".to_string(), "spawn_worktree".to_string()),
                    ("kind".to_string(), "function".to_string()),
                ]
                .into_iter()
                .collect(),
            }],
            edges: vec![Edge {
                from: "src/worktree.rs::spawn_worktree".to_string(),
                to: "community/1/0".to_string(),
                rel: REL_IN_COMMUNITY.to_string(),
                valid_from: 0,
                valid_to: None,
                source: Position::default(),
                tier: TIER_EXTRACTED.to_string(),
            }],
        };
        let payload = serde_json::to_vec(&g).unwrap();
        let mut s = ConsoleSession::new();
        call(&mut s, "graph_load", std::str::from_utf8(&payload).unwrap());

        let frame_before_build = call(&mut s, "map_frame", r#"{"zoom":0}"#);
        assert!(
            frame_before_build.get("error").is_some(),
            "map_frame must still error before its OWN map_build call: {frame_before_build:?}"
        );

        let build = call(&mut s, "map_build", r#"{"w":800,"h":600}"#);
        assert_eq!(build["ok"], true, "{build:?}");
        assert_eq!(build["districts"], 1, "{build:?}");
        assert_eq!(build["entities"], 1, "{build:?}");

        let frame = call(&mut s, "map_frame", r#"{"zoom":0}"#);
        assert!(frame.get("error").is_none(), "{frame:?}");
        assert_eq!(frame["districts"].as_array().unwrap().len(), 1, "{frame:?}");
        let entities = frame["entities"].as_array().unwrap();
        assert_eq!(entities.len(), 1, "{frame:?}");
        assert_eq!(entities[0]["name"], "spawn_worktree", "{frame:?}");
    }

    /// A same-session `graph_load` reload invalidates the PRIOR `map_build`: `map_frame` must go
    /// back to the "no map built" error until the caller calls `map_build` again against the new
    /// graph (adv-u84c1-map-stale-after-graph-reload) - the page re-fetches the graph payload per
    /// index stamp while the session persists (docs/architecture-addendum-mission-control.md's
    /// own "THE GRAPH PAYLOAD" note), so silently answering the pre-reload model would be wrong,
    /// not merely stale-looking. A fresh `map_build` after the reload still answers real counts
    /// from the NEW graph, proving the session's viewport also reset rather than merely erroring.
    #[test]
    fn map_frame_errors_again_after_a_graph_reload_until_map_build_runs_again() {
        use rigger::contextgraph::{
            Edge, Graph, Node, KIND_CODE_ENTITY, REL_IN_COMMUNITY, TIER_EXTRACTED,
        };
        use rigger::eventstore::Position;
        let one_entity_graph = |fn_name: &str| {
            let g = Graph {
                nodes: vec![Node {
                    id: format!("src/worktree.rs::{fn_name}"),
                    kind: KIND_CODE_ENTITY.to_string(),
                    attrs: [
                        ("name".to_string(), fn_name.to_string()),
                        ("kind".to_string(), "function".to_string()),
                    ]
                    .into_iter()
                    .collect(),
                }],
                edges: vec![Edge {
                    from: format!("src/worktree.rs::{fn_name}"),
                    to: "community/1/0".to_string(),
                    rel: REL_IN_COMMUNITY.to_string(),
                    valid_from: 0,
                    valid_to: None,
                    source: Position::default(),
                    tier: TIER_EXTRACTED.to_string(),
                }],
            };
            serde_json::to_vec(&g).unwrap()
        };
        let mut s = ConsoleSession::new();

        let first = one_entity_graph("spawn_worktree");
        call(&mut s, "graph_load", std::str::from_utf8(&first).unwrap());
        let build = call(&mut s, "map_build", r#"{"w":800,"h":600}"#);
        assert_eq!(build["ok"], true, "{build:?}");
        let frame = call(&mut s, "map_frame", r#"{"zoom":0}"#);
        assert!(frame.get("error").is_none(), "{frame:?}");

        // Reload, same session - no map_build in between.
        let second = one_entity_graph("reap_terminate");
        let reload = call(&mut s, "graph_load", std::str::from_utf8(&second).unwrap());
        assert_eq!(reload["ok"], true, "{reload:?}");

        let stale_frame = call(&mut s, "map_frame", r#"{"zoom":0}"#);
        assert!(
            stale_frame.get("error").is_some(),
            "map_frame must error after a reload with no map_build yet, never silently answer \
             the pre-reload model: {stale_frame:?}"
        );

        let rebuild = call(&mut s, "map_build", r#"{"w":800,"h":600}"#);
        assert_eq!(rebuild["ok"], true, "{rebuild:?}");
        let fresh_frame = call(&mut s, "map_frame", r#"{"zoom":0}"#);
        assert!(fresh_frame.get("error").is_none(), "{fresh_frame:?}");
        let entities = fresh_frame["entities"].as_array().unwrap();
        assert_eq!(entities.len(), 1, "{fresh_frame:?}");
        assert_eq!(entities[0]["name"], "reap_terminate", "{fresh_frame:?}");
    }

    /// `map_frame` at a higher zoom answers a strictly larger entities array than the full-extent
    /// (zoom 0) frame of the SAME built model - proving the ABI wiring carries the map engine's
    /// own monotonic-zoom property (pinned directly against the model in `console::map`'s own
    /// tests) through the wire, not just in the pure function.
    #[test]
    fn map_frame_zooming_in_answers_more_entities_through_the_wire() {
        use rigger::contextgraph::{
            Edge, Graph, Node, KIND_CODE_ENTITY, REL_IN_COMMUNITY, TIER_EXTRACTED,
        };
        use rigger::eventstore::Position;
        let mut nodes = Vec::new();
        let mut edges = Vec::new();
        for d in 0..6 {
            for m in 0..14 {
                let id = format!("src/worktree.rs::f_{d}_{m}");
                nodes.push(Node {
                    id: id.clone(),
                    kind: KIND_CODE_ENTITY.to_string(),
                    attrs: [
                        ("name".to_string(), format!("f_{d}_{m}")),
                        ("kind".to_string(), "function".to_string()),
                    ]
                    .into_iter()
                    .collect(),
                });
                edges.push(Edge {
                    from: id,
                    to: format!("community/1/{d}"),
                    rel: REL_IN_COMMUNITY.to_string(),
                    valid_from: 0,
                    valid_to: None,
                    source: Position::default(),
                    tier: TIER_EXTRACTED.to_string(),
                });
            }
        }
        let g = Graph { nodes, edges };
        let payload = serde_json::to_vec(&g).unwrap();
        let mut s = ConsoleSession::new();
        call(&mut s, "graph_load", std::str::from_utf8(&payload).unwrap());
        call(&mut s, "map_build", r#"{"w":1200,"h":800}"#);

        let out = call(&mut s, "map_frame", r#"{"zoom":0}"#);
        let in_ = call(&mut s, "map_frame", r#"{"zoom":6}"#);
        let out_n = out["entities"].as_array().unwrap().len();
        let in_n = in_["entities"].as_array().unwrap().len();
        assert!(
            in_n > out_n,
            "zooming in through the wire must strictly increase the entities array: {out_n} vs {in_n}"
        );
    }

    /// A malformed `map_build`/`map_frame` input is an error reply, never a panic - the SAME
    /// per-parsing-op discipline `malformed_input_answers_with_an_error_reply_for_every_parsing_op`
    /// already proves for the other six parsing ops.
    #[test]
    fn map_build_and_map_frame_answer_with_an_error_reply_on_malformed_input() {
        let mut s = ConsoleSession::new();
        let build = call(&mut s, "map_build", "not json");
        assert!(build.get("error").is_some(), "{build:?}");
        let frame = call(&mut s, "map_frame", "not json");
        assert!(frame.get("error").is_some(), "{frame:?}");
    }

    /// `palette_commands` - a view model no unit in this spec's DAG builds yet (spec 94
    /// criterion 4's own territory) - answers with an error reply rather than fabricated
    /// data. `scrub_track` graduated out of this stub set (spec 94 criterion 3, THE
    /// POSITION MODEL) - see its own tests below.
    #[test]
    fn palette_commands_answers_with_a_not_yet_available_error_reply() {
        let mut s = ConsoleSession::new();
        let v = call(&mut s, "palette_commands", "{}");
        assert!(v.get("error").is_some(), "{v:?}");
    }

    /// `scrub_track` (spec 94 c3) answers the SAME marks/ticks `console::scrub_track`
    /// itself returns for the session's accumulated event log - not a second,
    /// independently-derived copy - proven with a real `UnitIntegrated` mark. Malformed
    /// input is accepted (`scrub_track` takes none): the op ignores its `input` entirely,
    /// like the map/legend ops that need no argument.
    #[test]
    fn scrub_track_answers_console_scrub_tracks_own_marks() {
        let mut s = ConsoleSession::new();
        call(
            &mut s,
            "fold_reset",
            r#"{"events":[
                {"type":"UnitIntegrated","data":{"id":"u1","commit":"abc"},"position":1}
            ]}"#,
        );
        let v = call(&mut s, "scrub_track", "{}");
        let marks = v["marks"].as_array().expect("{v:?}");
        assert_eq!(marks.len(), 1, "{v:?}");
        assert_eq!(marks[0]["kind"], "integration", "{v:?}");
        assert_eq!(marks[0]["color"], "accent", "{v:?}");
        assert_eq!(marks[0]["tall"], true, "{v:?}");
        assert_eq!(marks[0]["position"], 1, "{v:?}");
        assert_eq!(v["ticks"], serde_json::json!([]), "{v:?}");
    }

    /// `fold_reset`'s wire events carry an OPTIONAL `recorded_at` (unix seconds) - additive
    /// to `d-u94c2-wire-event-shape`'s `{"type":..,"data":..,"position":..}` (never
    /// required: a caller that omits it, exactly like every snapshot recorded before this
    /// criterion, still folds and answers `scrub_track` with no ticks, never an error) - and
    /// `scrub_track`'s hour ticks read it straight through, proving the wire value actually
    /// reaches `Event::recorded_at`, not just a field this crate parses and discards.
    #[test]
    fn fold_reset_recorded_at_feeds_scrub_tracks_hour_ticks() {
        let mut s = ConsoleSession::new();
        call(
            &mut s,
            "fold_reset",
            r#"{"events":[
                {"type":"UnitStarted","data":{"id":"u1"},"position":1,"recorded_at":100},
                {"type":"UnitIntegrated","data":{"id":"u1","commit":"abc"},"position":2,"recorded_at":4000}
            ]}"#,
        );
        let v = call(&mut s, "scrub_track", "{}");
        let ticks = v["ticks"].as_array().expect("{v:?}");
        assert_eq!(ticks.len(), 1, "{v:?}");
        assert_eq!(ticks[0]["position"], 2, "{v:?}");
    }

    /// `scrub_track` on a completely fresh session (no `fold_reset` yet) answers an empty
    /// track, never an error - mirroring `map_legend`'s "needs nothing loaded" shape rather
    /// than the map ops' "call the loader first" error contract, since an empty event log
    /// is a legitimate (if boring) state, not a caller mistake.
    #[test]
    fn scrub_track_on_a_fresh_session_answers_an_empty_track() {
        let mut s = ConsoleSession::new();
        let v = call(&mut s, "scrub_track", "{}");
        assert_eq!(v, serde_json::json!({"marks": [], "ticks": []}), "{v:?}");
    }

    /// `fold_at` answers the fold as of position N for EVERY N the session's log spans
    /// (spec 94 c3's own "for every N" wording) - not just one hand-picked position -
    /// proven by scrubbing every position from 0 through the head and comparing each
    /// answer against an independent `console::fold` over the same prefix.
    #[test]
    fn fold_at_answers_every_position_from_zero_through_the_head() {
        let mut s = ConsoleSession::new();
        let events_json = r#"{"events":[
            {"type":"UnitStarted","data":{"id":"u1"},"position":2},
            {"type":"UnitStarted","data":{"id":"u2"},"position":5},
            {"type":"UnitIntegrated","data":{"id":"u1","commit":"abc"},"position":7},
            {"type":"UnitEscalated","data":{"id":"u2"},"position":9}
        ]}"#;
        call(&mut s, "fold_reset", events_json);
        let full_events: Vec<rigger::eventstore::Event> = vec![
            rigger::eventstore::Event::new("UnitStarted", br#"{"id":"u1"}"#.to_vec()),
            rigger::eventstore::Event::new("UnitStarted", br#"{"id":"u2"}"#.to_vec()),
            rigger::eventstore::Event::new(
                "UnitIntegrated",
                br#"{"id":"u1","commit":"abc"}"#.to_vec(),
            ),
            rigger::eventstore::Event::new("UnitEscalated", br#"{"id":"u2"}"#.to_vec()),
        ];
        let positions = [2u64, 5, 7, 9];
        let mut with_positions: Vec<rigger::eventstore::Event> = full_events;
        for (e, p) in with_positions.iter_mut().zip(positions.iter()) {
            e.position = *p;
        }
        for n in 0..=10u64 {
            let want_events: Vec<&rigger::eventstore::Event> =
                with_positions.iter().filter(|e| e.position <= n).collect();
            let want_events: Vec<rigger::eventstore::Event> =
                want_events.into_iter().cloned().collect();
            let want = console::fold(&want_events, 0).unwrap();
            let got = call(&mut s, "fold_at", &format!(r#"{{"position":{n}}}"#));
            assert_eq!(
                got["units"],
                serde_json::to_value(&want.units).unwrap(),
                "fold_at({n}) units mismatch: {got:?}"
            );
            assert_eq!(
                got["statusline"], want.statusline,
                "fold_at({n}) statusline mismatch: {got:?}"
            );
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
/// budget`): "a fold of 10,000 console events completes in under 16ms natively in release
/// mode... asserted by its natively compiled ABI tests driving `fold_reset`/`fold_push`" - a
/// PER-FOLD bound (singular: "a fold"), proven here two ways at the SAME 10,000-event scale:
/// `fold_reset` driven once as the bulk load path a page uses on first opening a run, and
/// `fold_push` driven across a RUN of consecutive calls once the session already holds
/// (and keeps holding, past 10,000) that many events - the hot-incremental-path use pattern
/// its own doc names, proving every individual call, not just one lucky one, stays under
/// budget. Neither test sums many independent calls' wall-clock against this single-fold
/// budget: summing N per-fold costs to bound a whole SESSION's lifetime total is a different,
/// uncosted question this text never asks (a page that has been open for hours accumulates
/// wall-clock across thousands of pushes no matter how fast any one of them is - that is a
/// session-lifetime property, not this fold's own per-call latency).
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

    /// The ONE authority for what event `i` in the representative fixture looks like - shared
    /// by the bulk `fold_reset` builder below and the `fold_push` budget test, so pushing
    /// event `i` one at a time and bulk-loading events `0..=i` always mean the identical
    /// stream, never two independently-maintained copies of the same generation rule.
    fn nth_event_fixture(i: u64) -> (&'static str, String) {
        let unit = i / EVENTS_PER_UNIT;
        let offset = i % EVENTS_PER_UNIT;
        if offset == 0 {
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
        }
    }

    /// A `fold_reset` body over the first `n` events of the representative fixture.
    fn events_reset_input(n: u64) -> String {
        let mut events = String::from(r#"{"events":["#);
        for i in 0..n {
            if i > 0 {
                events.push(',');
            }
            let (type_, data) = nth_event_fixture(i);
            events.push_str(&format!(
                r#"{{"type":"{type_}","data":{data},"position":{i}}}"#
            ));
        }
        events.push_str("]}");
        events
    }

    fn ten_thousand_events_reset_input() -> String {
        events_reset_input(UNITS * EVENTS_PER_UNIT)
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

    /// THE BUDGET ITSELF, `fold_push`'s own half: bulk-loads a session to just under the
    /// 10,000-event mark via `fold_reset` (untimed - that path is proven above), then times a
    /// RUN of consecutive `fold_push` calls that carries the session past 10,000, one at a
    /// time - `fold_push`'s own documented hot-incremental-path use pattern, at the scale
    /// BUDGETS names. Every one of the `TAIL` pushes, not just the first, must individually
    /// stay under 16ms - `op_fold_push`'s accumulate-and-refold design (clone the whole
    /// history, re-run `console::fold`) costs essentially what a `fold_reset` of the same
    /// size costs, so this is the SAME per-fold budget
    /// `fold_reset_of_ten_thousand_events_completes_under_16ms_in_release_mode` proves,
    /// driven through `fold_push`'s own call path (the real clone-and-refold cost) instead of
    /// `fold_reset`'s. This is a per-call bound, not a session-lifetime sum - see this
    /// module's own doc for why summing `TAIL` independent calls against a single-fold budget
    /// would be a different, uncosted question.
    #[test]
    #[cfg(not(debug_assertions))]
    fn fold_push_at_ten_thousand_events_completes_under_16ms_per_call_in_release_mode() {
        const TAIL: u64 = 100;
        let total = UNITS * EVENTS_PER_UNIT;
        let head = total - TAIL;

        let mut s = ConsoleSession::new();
        let reset_bytes = dispatch(&mut s, "fold_reset", events_reset_input(head).as_bytes());
        let reset_v: serde_json::Value = serde_json::from_slice(&reset_bytes).unwrap();
        assert!(reset_v.get("error").is_none(), "{reset_v:?}");

        for i in head..total {
            let (type_, data) = nth_event_fixture(i);
            let push_input = format!(r#"{{"type":"{type_}","data":{data},"position":{i}}}"#);
            let start = std::time::Instant::now();
            let bytes = dispatch(&mut s, "fold_push", push_input.as_bytes());
            let elapsed = start.elapsed();
            let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
            assert!(v.get("error").is_none(), "{v:?}");
            assert!(
                elapsed.as_millis() < 16,
                "fold_push at {} events took {elapsed:?}, over the 16ms budget",
                i + 1
            );
        }
    }
}
