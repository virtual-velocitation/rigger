//! PERIPHERY (spec 93, criterion 2: "THE MODULE AND ITS ABI"). This file drives the crate's
//! declared public surface - `console_alloc`, `console_free`, `console_call` - exactly as an
//! external Rust consumer would, with none of the private-item access the crate's own
//! `#[cfg(test)]` modules (`dispatch_tests`, `abi_tests`, `budget_tests`, all inside
//! `src/lib.rs`) rely on. Those modules compile as part of the library itself via `use
//! super::*`, so they can call `dispatch`, construct a bare `ConsoleSession`, and read a
//! reply through `call_inner`'s own untruncated pointer - a backdoor around this ABI's own
//! documented pointer-packing lossiness (see `pack`'s doc in `src/lib.rs`) that a genuine
//! external caller does not have. This is exactly the layer those tests are structurally
//! blind to: a regression that narrowed one of the three exports from `pub` to `pub(crate)`,
//! dropped `rlib` from the crate's `crate-type`, or otherwise broke the crate's own linkage
//! boundary would compile clean inside `lib.rs` (same compilation unit, full private access)
//! but fail to compile HERE - the one place that links against `console-core` the way any
//! other Rust caller (or, for the ABI's real audience, a wasm host) actually would.
//!
//! On this native host a packed `console_call` reply's pointer half is only the low 32 bits
//! of a real heap address that routinely exceeds `u32::MAX` (the same asymmetry `pack`'s own
//! doc explains), so an external caller here has no way to dereference it. These tests never
//! try; every assertion below reads back only what the documented ABI actually promises on a
//! native host - the packed reply's length half - and proves state and content-shaped claims
//! through comparable lengths (derived from the same authorities the crate itself wires to,
//! never a hardcoded magic number) rather than by reconstructing an unreadable pointer.

use console_core::{console_alloc, console_call, console_free};
use rigger_domain::spawn::SpawnEvent;

/// Write `bytes` into a fresh `console_alloc`'d buffer and hand back its pointer - the same
/// alloc-then-write sequence a real caller performs before every `console_call`.
unsafe fn alloc_and_write(bytes: &[u8]) -> *mut u8 {
    let ptr = console_alloc(bytes.len());
    assert!(
        !ptr.is_null(),
        "console_alloc must never answer a null pointer"
    );
    if !bytes.is_empty() {
        unsafe {
            std::ptr::copy_nonoverlapping(bytes.as_ptr(), ptr, bytes.len());
        }
    }
    ptr
}

/// Drive one `op`/`input` pair through the exported `console_call` ONLY - allocate both
/// argument buffers via the public `console_alloc`, write their bytes, call `console_call`,
/// and free the two argument buffers afterward (a caller's own responsibility for what it
/// passed in; the reply buffer is `console_call`'s allocation, deliberately left unfreed here
/// since these tests never have a real pointer to free it with - seen only through the
/// packed, native-lossy return `pack` documents).
unsafe fn call(op: &str, input: &str) -> u64 {
    unsafe {
        let op_ptr = alloc_and_write(op.as_bytes());
        let in_ptr = alloc_and_write(input.as_bytes());
        let packed = console_call(
            op_ptr as *const u8,
            op.len(),
            in_ptr as *const u8,
            input.len(),
        );
        console_free(op_ptr, op.len());
        console_free(in_ptr, input.len());
        packed
    }
}

/// The length half of a packed `console_call` return - the one half meaningful on every host,
/// native included (`pack`'s own doc).
fn reply_len(packed: u64) -> usize {
    (packed & 0xFFFF_FFFF) as usize
}

/// `console_alloc`/`console_free` round-trip real bytes through the real, unpacked pointer
/// they hand back - the exported allocator half of the ABI, proven on its own exactly as a
/// caller uses it before ever calling `console_call`.
#[test]
fn console_alloc_and_console_free_round_trip_bytes_through_the_public_abi() {
    let bytes = b"periphery round trip";
    unsafe {
        let ptr = alloc_and_write(bytes);
        let read = std::slice::from_raw_parts(ptr, bytes.len());
        assert_eq!(
            read, bytes,
            "console_alloc did not hand back a writable, readable buffer of the requested length"
        );
        console_free(ptr, bytes.len());
    }
}

/// A real op answers a non-empty reply through the literal exported `console_call` - THE
/// MODULE AND ITS ABI's "the twelve ops answer with JSON replies", proven at the actual entry
/// point an external caller uses, not through `dispatch` directly.
#[test]
fn console_call_answers_a_nonempty_reply_for_a_known_op() {
    let packed = unsafe { call("statusline", "{}") };
    assert!(
        reply_len(packed) > 0,
        "a real op must answer a non-empty reply through the exported ABI"
    );
}

/// An unknown op answers a non-empty (error) reply through the exported ABI - THE MODULE AND
/// ITS ABI's "an unknown op ... returns an error reply", proven externally: never silence,
/// never a panic that would abort the whole test process instead of answering.
#[test]
fn console_call_answers_an_error_reply_for_an_unknown_op() {
    let packed = unsafe { call("not-a-real-op", "{}") };
    assert!(
        reply_len(packed) > 0,
        "an unknown op must still answer a non-empty error reply through the exported ABI"
    );
}

/// Malformed input to a real parsing op answers a non-empty (error) reply through the
/// exported ABI - proven externally, not only via `dispatch`.
#[test]
fn console_call_answers_an_error_reply_for_malformed_input() {
    let packed = unsafe { call("fold_reset", "not json") };
    assert!(
        reply_len(packed) > 0,
        "malformed input must still answer a non-empty error reply through the exported ABI, \
         never a panic"
    );
}

/// The module's thread-local session persists ACROSS separate `console_call` invocations -
/// exactly what a page relies on when it calls `fold_reset` once and `statusline` on every
/// later render - proven through two real calls to the exported ABI, not one `dispatch` call
/// against a hand-built session. Reply content is unreadable natively (see module doc), so
/// this proves persistence the one way the public ABI allows on this host: `statusline`'s
/// reply length changes once a unit has actually been folded in, which can only happen if the
/// second call saw the first call's effect on the SAME session.
#[test]
fn console_call_session_state_persists_across_separate_calls() {
    let before = reply_len(unsafe { call("statusline", "{}") });

    let reset = unsafe {
        call(
            "fold_reset",
            r#"{"events":[{"type":"UnitStarted","data":{"id":"u1"},"position":1}]}"#,
        )
    };
    assert!(
        reply_len(reset) > 0,
        "fold_reset must answer a non-empty reply through the exported ABI"
    );

    let after = reply_len(unsafe { call("statusline", "{}") });
    assert_ne!(
        before, after,
        "statusline's reply length must change once fold_reset has recorded a unit - the \
         session must persist across separate console_call invocations, not reset on every call"
    );
}

/// `graph_load` then `graph_query` wire through the exported ABI to the SAME library query
/// engine `console-core` depends on (spec 93's "THE GRAPH QUERIES ARE EXPORTED": "graph_query
/// answers ... with the same results the library's query functions return for the same
/// graph") - proven through two real `console_call` invocations, not through `dispatch`
/// directly. Reply content is unreadable natively, so this compares the reply's LENGTH
/// against the length of the exact answer `rigger_domain::contextgraph::query::graph_query` itself
/// returns for the identical graph and kind - an expected value computed from the same
/// authority the ABI wires to, never a hardcoded number.
#[test]
fn console_call_wires_graph_load_and_graph_query_through_the_public_abi() {
    use rigger_domain::contextgraph::query::graph_query;
    use rigger_domain::contextgraph::{Graph, Node, KIND_FILE};

    let graph = Graph {
        nodes: vec![Node {
            id: "src/a.rs".to_string(),
            kind: KIND_FILE.to_string(),
            attrs: Default::default(),
        }],
        edges: Vec::new(),
    };
    let payload = serde_json::to_vec(&graph).expect("serializing the fixture graph");

    let loaded = unsafe { call("graph_load", std::str::from_utf8(&payload).unwrap()) };
    assert!(
        reply_len(loaded) > 0,
        "graph_load must answer a non-empty reply through the exported ABI"
    );

    let params = br#"{"query":"src/a.rs"}"#;
    let queried = unsafe {
        call(
            "graph_query",
            r#"{"kind":"search","params":{"query":"src/a.rs"}}"#,
        )
    };

    let direct = graph_query(&graph, "search", params)
        .expect("the library's own graph_query must answer this fixture");
    let expected_len = serde_json::to_vec(&direct).unwrap().len();

    assert_eq!(
        reply_len(queried),
        expected_len,
        "graph_query's reply through the exported ABI must be the same length as the \
         library's own graph_query answer for the identical graph and kind"
    );
}

/// `map_build` then `map_frame` wire through the exported ABI to the SAME `console::map` engine
/// spec 84 criterion 1 built (this crate's own dispatch doc: "`map_build`/`map_frame` are spec
/// 84 criterion 1's own engine ... real data now, not the stub spec 93 left them as") - proven
/// through two real `console_call` invocations, not through `dispatch` directly. This is exactly
/// the layer the implementer's own same-crate `dispatch_tests` (e.g.
/// `map_build_answers_real_district_and_entity_counts_from_the_loaded_graph`) are structurally
/// blind to: a regression that broke ONLY the FFI marshaling of this new cross-crate call (a
/// truncated pointer, a dropped byte, a mis-packed length) would still pass every internal
/// `dispatch()` call, which never crosses the exported boundary at all. Reply content is
/// unreadable natively (see module doc), so this compares the reply's LENGTH against the length
/// of the exact answer `console::map::build`/`console::map::frame` themselves return for the
/// identical graph and viewport - an expected value computed from the same authority the ABI
/// wires to, never a hardcoded number.
#[test]
fn console_call_wires_map_build_and_map_frame_through_the_public_abi() {
    use rigger_console::console::map;
    use rigger_domain::contextgraph::{
        Edge, Graph, Node, KIND_CODE_ENTITY, REL_IN_COMMUNITY, TIER_EXTRACTED,
    };
    use rigger_domain::eventstore::Position;

    let graph = Graph {
        nodes: vec![Node {
            id: "src/a.rs::f".to_string(),
            kind: KIND_CODE_ENTITY.to_string(),
            attrs: [
                ("name".to_string(), "f".to_string()),
                ("kind".to_string(), "function".to_string()),
            ]
            .into_iter()
            .collect(),
        }],
        edges: vec![Edge {
            from: "src/a.rs::f".to_string(),
            to: "community/1/0".to_string(),
            rel: REL_IN_COMMUNITY.to_string(),
            valid_from: 0,
            valid_to: None,
            source: Position::default(),
            tier: TIER_EXTRACTED.to_string(),
        }],
    };
    let payload = serde_json::to_vec(&graph).expect("serializing the fixture graph");

    let loaded = unsafe { call("graph_load", std::str::from_utf8(&payload).unwrap()) };
    assert!(
        reply_len(loaded) > 0,
        "graph_load must answer a non-empty reply through the exported ABI"
    );

    let build = unsafe { call("map_build", r#"{"w":800,"h":600}"#) };
    assert!(
        reply_len(build) > 0,
        "map_build must answer a non-empty reply through the exported ABI"
    );

    let frame = unsafe { call("map_frame", r#"{"zoom":0}"#) };

    let model = map::build(&graph);
    let direct = map::frame(&model, 800.0, 600.0, &map::Camera::default(), None);
    let expected_len = serde_json::to_vec(&direct).unwrap().len();

    assert_eq!(
        reply_len(frame),
        expected_len,
        "map_frame's reply through the exported ABI must be the same length as console::map's \
         own frame() answer for the identical graph and viewport"
    );
}

/// `map_frame` at a higher zoom answers a strictly LONGER reply than the full-extent (zoom 0)
/// frame of the same built model, proven through the real exported ABI - the semantic-zoom
/// monotonic-eligibility property the implementer's own `dispatch_tests::
/// map_frame_zooming_in_answers_more_entities_through_the_wire` already pins, but only against
/// the private `dispatch()` backdoor; reproven here across the actual FFI boundary these
/// periphery tests exist to guard.
#[test]
fn console_call_map_frame_zooming_in_answers_a_longer_reply_through_the_public_abi() {
    use rigger_domain::contextgraph::{
        Edge, Graph, Node, KIND_CODE_ENTITY, REL_IN_COMMUNITY, TIER_EXTRACTED,
    };
    use rigger_domain::eventstore::Position;

    let mut nodes = Vec::new();
    let mut edges = Vec::new();
    for d in 0..6 {
        for m in 0..14 {
            let id = format!("src/worktree.rs::g_{d}_{m}");
            nodes.push(Node {
                id: id.clone(),
                kind: KIND_CODE_ENTITY.to_string(),
                attrs: [
                    ("name".to_string(), format!("g_{d}_{m}")),
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
    let graph = Graph { nodes, edges };
    let payload = serde_json::to_vec(&graph).expect("serializing the fixture graph");

    unsafe {
        call("graph_load", std::str::from_utf8(&payload).unwrap());
        call("map_build", r#"{"w":1200,"h":800}"#);
    }

    let full_extent = unsafe { call("map_frame", r#"{"zoom":0}"#) };
    let zoomed_in = unsafe { call("map_frame", r#"{"zoom":6}"#) };

    assert!(
        reply_len(zoomed_in) > reply_len(full_extent),
        "zooming in through the real exported ABI must answer a strictly longer reply: \
         full extent {} vs zoomed in {}",
        reply_len(full_extent),
        reply_len(zoomed_in)
    );
}

/// A same-session `graph_load` reload invalidates whatever `map_build` computed from the PRIOR
/// graph (adv-u84c1-map-stale-after-graph-reload): `map_frame` must go back to the documented
/// "no map built" error until the caller calls `map_build` again, proven through the real
/// exported ABI - the implementer's own same-crate `dispatch_tests::
/// map_frame_errors_again_after_a_graph_reload_until_map_build_runs_again` drives this identical
/// fix but only through the private `dispatch()` backdoor (see this crate's own `call_inner`
/// doc), never across the FFI boundary this file exists to guard: a regression that broke only
/// the ABI-level wiring of the reload-clears-the-map fix (the marshaling `console_call` adds on
/// top of `dispatch`) would still pass that internal test. Reply content is unreadable natively
/// (see module doc), so the post-reload reply's length is compared against the exact
/// `{"error": ...}` reply length `op_map_frame`'s own documented "no map built - call map_build
/// first" contract text serializes to - the same length-of-a-known-value technique this file's
/// other assertions use, never a hardcoded byte count.
#[test]
fn console_call_map_frame_errors_again_after_a_graph_reload_through_the_public_abi() {
    use rigger_console::console::map;
    use rigger_domain::contextgraph::{
        Edge, Graph, Node, KIND_CODE_ENTITY, REL_IN_COMMUNITY, TIER_EXTRACTED,
    };
    use rigger_domain::eventstore::Position;

    fn one_entity_graph(fn_name: &str) -> Graph {
        Graph {
            nodes: vec![Node {
                id: format!("src/a.rs::{fn_name}"),
                kind: KIND_CODE_ENTITY.to_string(),
                attrs: [
                    ("name".to_string(), fn_name.to_string()),
                    ("kind".to_string(), "function".to_string()),
                ]
                .into_iter()
                .collect(),
            }],
            edges: vec![Edge {
                from: format!("src/a.rs::{fn_name}"),
                to: "community/1/0".to_string(),
                rel: REL_IN_COMMUNITY.to_string(),
                valid_from: 0,
                valid_to: None,
                source: Position::default(),
                tier: TIER_EXTRACTED.to_string(),
            }],
        }
    }

    let first = one_entity_graph("f");
    let first_bytes = serde_json::to_vec(&first).expect("serializing the first fixture graph");
    unsafe {
        call("graph_load", std::str::from_utf8(&first_bytes).unwrap());
        call("map_build", r#"{"w":800,"h":600}"#);
    }
    let built_frame = unsafe { call("map_frame", r#"{"zoom":0}"#) };
    assert!(
        reply_len(built_frame) > 0,
        "map_frame after a real map_build must answer a non-empty reply through the exported ABI"
    );

    // Reload, same session - no map_build in between.
    let second = one_entity_graph("g");
    let second_bytes = serde_json::to_vec(&second).expect("serializing the second fixture graph");
    let reload = unsafe { call("graph_load", std::str::from_utf8(&second_bytes).unwrap()) };
    assert!(
        reply_len(reload) > 0,
        "graph_load must answer a non-empty reply through the exported ABI"
    );

    let stale_frame = unsafe { call("map_frame", r#"{"zoom":0}"#) };
    let expected_error_len = serde_json::to_vec(&serde_json::json!({
        "error": "map_frame: no map built - call map_build first"
    }))
    .unwrap()
    .len();
    assert_eq!(
        reply_len(stale_frame),
        expected_error_len,
        "map_frame through the real exported ABI must go back to the documented \"no map \
         built\" error after a same-session graph_load reload, never silently keep answering \
         the pre-reload model"
    );

    // A fresh map_build after the reload answers real data again from the NEW graph.
    unsafe {
        call("map_build", r#"{"w":800,"h":600}"#);
    }
    let fresh_frame = unsafe { call("map_frame", r#"{"zoom":0}"#) };

    let model = map::build(&second);
    let direct = map::frame(&model, 800.0, 600.0, &map::Camera::default(), None);
    let expected_len = serde_json::to_vec(&direct).unwrap().len();
    assert_eq!(
        reply_len(fresh_frame),
        expected_len,
        "map_frame after the post-reload map_build must answer real data from the NEW graph \
         through the exported ABI, matching console::map's own frame() output for it"
    );
}

/// `map_hit` wires through the exported ABI to the SAME `console::map::hit` engine (spec 84
/// criterion 2's own) - proven the same way this file's own `map_build`/`map_frame` test above
/// does: a real `console_call` at the exact screen point `console::map::frame` itself placed an
/// entity's dot, its reply length compared against `console::map::hit`'s own direct answer for
/// the identical camera/point.
#[test]
fn console_call_wires_map_hit_through_the_public_abi() {
    use rigger_console::console::map;
    use rigger_domain::contextgraph::{
        Edge, Graph, Node, KIND_CODE_ENTITY, REL_IN_COMMUNITY, TIER_EXTRACTED,
    };
    use rigger_domain::eventstore::Position;

    let graph = Graph {
        nodes: vec![Node {
            id: "src/a.rs::f".to_string(),
            kind: KIND_CODE_ENTITY.to_string(),
            attrs: [
                ("name".to_string(), "f".to_string()),
                ("kind".to_string(), "function".to_string()),
            ]
            .into_iter()
            .collect(),
        }],
        edges: vec![Edge {
            from: "src/a.rs::f".to_string(),
            to: "community/1/0".to_string(),
            rel: REL_IN_COMMUNITY.to_string(),
            valid_from: 0,
            valid_to: None,
            source: Position::default(),
            tier: TIER_EXTRACTED.to_string(),
        }],
    };
    let payload = serde_json::to_vec(&graph).expect("serializing the fixture graph");

    unsafe {
        call("graph_load", std::str::from_utf8(&payload).unwrap());
        call("map_build", r#"{"w":800,"h":600}"#);
    }

    let model = map::build(&graph);
    let target = map::frame(&model, 800.0, 600.0, &map::Camera::default(), None)
        .entities
        .into_iter()
        .next()
        .expect("the one fixture entity must be drawn at full extent");

    let hit = unsafe {
        call(
            "map_hit",
            &format!(r#"{{"zoom":0,"x":{},"y":{}}}"#, target.x, target.y),
        )
    };

    let direct = map::hit(
        &model,
        800.0,
        600.0,
        &map::Camera::default(),
        None,
        target.x,
        target.y,
    );
    let id = match direct {
        Some(map::Hit::Entity(id)) => id,
        other => panic!("the exact projected dot of a real entity must hit that entity: {other:?}"),
    };
    let expected_len = serde_json::to_vec(&serde_json::json!({ "hit": "entity", "id": id }))
        .unwrap()
        .len();

    assert_eq!(
        reply_len(hit),
        expected_len,
        "map_hit's reply through the exported ABI must be the same length as the reply shape \
         built from console::map::hit's own direct answer for the identical point"
    );
}

/// `graph_query`'s `map_landmarks` kind wires through the exported ABI - spec 84 criterion 2's
/// own Explore rail candidate lists, proven crossing the SAME real FFI boundary the map_build/
/// map_frame/map_hit tests above already do, not merely through the crate's own private
/// `dispatch_tests` (already covered in `src/lib.rs`).
#[test]
fn console_call_wires_graph_query_map_landmarks_through_the_public_abi() {
    use rigger_console::console::map;
    use rigger_domain::contextgraph::{
        Edge, Graph, Node, KIND_CODE_ENTITY, REL_IN_COMMUNITY, TIER_EXTRACTED,
    };
    use rigger_domain::eventstore::Position;

    let graph = Graph {
        nodes: vec![Node {
            id: "src/a.rs::f".to_string(),
            kind: KIND_CODE_ENTITY.to_string(),
            attrs: [
                ("name".to_string(), "f".to_string()),
                ("kind".to_string(), "function".to_string()),
            ]
            .into_iter()
            .collect(),
        }],
        edges: vec![Edge {
            from: "src/a.rs::f".to_string(),
            to: "community/1/0".to_string(),
            rel: REL_IN_COMMUNITY.to_string(),
            valid_from: 0,
            valid_to: None,
            source: Position::default(),
            tier: TIER_EXTRACTED.to_string(),
        }],
    };
    let payload = serde_json::to_vec(&graph).expect("serializing the fixture graph");

    unsafe {
        call("graph_load", std::str::from_utf8(&payload).unwrap());
        call("map_build", r#"{"w":800,"h":600}"#);
    }

    let queried = unsafe {
        call(
            "graph_query",
            r#"{"kind":"map_landmarks","params":{"limit":5}}"#,
        )
    };

    let model = map::build(&graph);
    let direct = map::landmarks(&model, 5);
    let expected_len = serde_json::to_vec(&serde_json::json!({ "candidates": direct }))
        .unwrap()
        .len();

    assert_eq!(
        reply_len(queried),
        expected_len,
        "graph_query's map_landmarks reply through the exported ABI must be the same length as \
         console::map::landmarks's own direct answer for the identical graph"
    );
}

/// The SAME two-district fixture `console::map`'s own `cross_district_graph` unit-test helper
/// builds (hub/leaf in one district, bridge/quiet in another, bridge -> hub the one cross-
/// district edge) - shared by every `map_bridges`/`map_argued_about`/`map_changing`/`map_search`/
/// `map_fit_*` periphery test below, so each test's own body stays about the OP's ABI wiring,
/// never about re-deriving a fixture graph (the same discipline `built_map_session` in
/// `src/lib.rs`'s own `dispatch_tests` already keeps).
fn cross_district_fixture() -> rigger_domain::contextgraph::Graph {
    use rigger_domain::contextgraph::{
        Edge, Graph, Node, KIND_CODE_ENTITY, REL_CALLS, REL_IN_COMMUNITY, TIER_EXTRACTED,
    };
    use rigger_domain::eventstore::Position;

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
    let edge = |from: &str, to: &str, rel: &str| Edge {
        from: from.to_string(),
        to: to.to_string(),
        rel: rel.to_string(),
        valid_from: 0,
        valid_to: None,
        source: Position::default(),
        tier: TIER_EXTRACTED.to_string(),
    };

    Graph {
        nodes: vec![
            code("src/worktree.rs::hub", "hub"),
            code("src/worktree.rs::leaf", "leaf"),
            code("src/dash.rs::bridge", "bridge"),
            code("src/dash.rs::quiet", "quiet"),
        ],
        edges: vec![
            edge("src/worktree.rs::hub", "community/1/0", REL_IN_COMMUNITY),
            edge("src/worktree.rs::leaf", "community/1/0", REL_IN_COMMUNITY),
            edge("src/dash.rs::bridge", "community/1/1", REL_IN_COMMUNITY),
            edge("src/dash.rs::quiet", "community/1/1", REL_IN_COMMUNITY),
            edge("src/worktree.rs::hub", "src/worktree.rs::leaf", REL_CALLS),
            // The only cross-district edge: dash::bridge calls worktree::hub.
            edge("src/dash.rs::bridge", "src/worktree.rs::hub", REL_CALLS),
        ],
    }
}

/// A single district with a real hull (one community, six members) - the SAME shape
/// `console::map`'s own `hit_falls_back_to_a_district_when_no_entity_is_near_but_the_click_is_\
/// inside_its_hull` unit test builds via `populous_graph(1, 1, 6)`, reproduced here since that
/// helper is `#[cfg(test)]`-private to `crates/rigger-console/src/console/map.rs` and unreachable from this external
/// periphery crate. `cross_district_fixture`'s own two-member districts are too small to carry a
/// hull radius any probe point can land inside without also landing within `map_hit`'s own hit
/// radius of a dot - this fixture exists ONLY for the district-hit and no-hit periphery tests
/// below, which need a real gap between "inside the hull" and "near a dot".
fn six_member_district_graph() -> rigger_domain::contextgraph::Graph {
    use rigger_domain::contextgraph::{
        Edge, Graph, Node, KIND_CODE_ENTITY, REL_CALLS, REL_IN_COMMUNITY, TIER_EXTRACTED,
    };
    use rigger_domain::eventstore::Position;

    let mut nodes = Vec::new();
    let mut edges = Vec::new();
    for i in 0..6 {
        let id = format!("src/worktree.rs::f_{i}");
        nodes.push(Node {
            id: id.clone(),
            kind: KIND_CODE_ENTITY.to_string(),
            attrs: [
                ("name".to_string(), format!("f_{i}")),
                ("kind".to_string(), "function".to_string()),
            ]
            .into_iter()
            .collect(),
        });
        edges.push(Edge {
            from: id,
            to: "community/1/0".to_string(),
            rel: REL_IN_COMMUNITY.to_string(),
            valid_from: 0,
            valid_to: None,
            source: Position::default(),
            tier: TIER_EXTRACTED.to_string(),
        });
    }
    edges.push(Edge {
        from: "src/worktree.rs::f_0".to_string(),
        to: "src/worktree.rs::f_1".to_string(),
        rel: REL_CALLS.to_string(),
        valid_from: 0,
        valid_to: None,
        source: Position::default(),
        tier: TIER_EXTRACTED.to_string(),
    });
    Graph { nodes, edges }
}

/// `map_hit`'s DISTRICT branch (a click inside a district's hull, near no entity dot) wires
/// through the exported ABI to the same tagged `{"hit":"district","purpose":...}` shape
/// `console::map::hit`'s own `Hit::District` answers directly - the map_hit ABI test above this
/// one only ever proves the ENTITY branch; this is a genuinely different wire shape (a `purpose`
/// field, not an `id`), never exercised by that test.
#[test]
fn console_call_wires_map_hit_through_the_public_abi_to_a_district() {
    use rigger_console::console::map;

    let graph = six_member_district_graph();
    let payload = serde_json::to_vec(&graph).expect("serializing the fixture graph");

    unsafe {
        call("graph_load", std::str::from_utf8(&payload).unwrap());
        call("map_build", r#"{"w":900,"h":700}"#);
    }

    let model = map::build(&graph);
    let dl = map::frame(&model, 900.0, 700.0, &map::Camera::default(), None);
    let d = dl.districts.first().expect("one district in this fixture");
    let x = d.x + d.radius * 0.99;
    let y = d.y;
    // The probe must not accidentally land on an entity dot - the same guard
    // console::map's own district-fallback unit test asserts before trusting its result.
    let too_far_from_any_dot = dl
        .entities
        .iter()
        .all(|e| ((e.x - x).powi(2) + (e.y - y).powi(2)).sqrt() > 10.0);
    assert!(
        too_far_from_any_dot,
        "the probe point must not land on an entity dot for this test to prove the district \
         fallback: {dl:?}"
    );

    let hit = unsafe { call("map_hit", &format!(r#"{{"zoom":0,"x":{x},"y":{y}}}"#)) };

    let direct = map::hit(&model, 900.0, 700.0, &map::Camera::default(), None, x, y);
    let purpose = match direct {
        Some(map::Hit::District(purpose)) => purpose,
        other => panic!(
            "the probe point inside the hull, away from every dot, must hit the district: \
             {other:?}"
        ),
    };
    let expected_len =
        serde_json::to_vec(&serde_json::json!({ "hit": "district", "purpose": purpose }))
            .unwrap()
            .len();

    assert_eq!(
        reply_len(hit),
        expected_len,
        "map_hit's district-hit reply through the exported ABI must be the same length as the \
         reply shape built from console::map::hit's own direct answer for the identical point"
    );
}

/// `map_hit`'s NONE branch (a click far from every entity and every district hull) wires through
/// the exported ABI to the same tagged `{"hit":"none"}` shape `console::map::hit`'s own `None`
/// answers directly - the third and last of `map_hit`'s three tagged reply shapes, so a
/// regression that left this branch answering the wrong tag (or panicking) would compile clean
/// but never be caught by either of the other two map_hit periphery tests.
#[test]
fn console_call_wires_map_hit_through_the_public_abi_to_none() {
    use rigger_console::console::map;

    let graph = six_member_district_graph();
    let payload = serde_json::to_vec(&graph).expect("serializing the fixture graph");

    unsafe {
        call("graph_load", std::str::from_utf8(&payload).unwrap());
        call("map_build", r#"{"w":900,"h":700}"#);
    }

    let hit = unsafe { call("map_hit", r#"{"zoom":0,"x":-1.0e9,"y":-1.0e9}"#) };

    let model = map::build(&graph);
    let direct = map::hit(
        &model,
        900.0,
        700.0,
        &map::Camera::default(),
        None,
        -1.0e9,
        -1.0e9,
    );
    assert_eq!(
        direct, None,
        "a point this far from everything must hit nothing directly"
    );

    let expected_len = serde_json::to_vec(&serde_json::json!({ "hit": "none" }))
        .unwrap()
        .len();

    assert_eq!(
        reply_len(hit),
        expected_len,
        "map_hit's no-hit reply through the exported ABI must be the same length as the \
         documented {{\"hit\":\"none\"}} shape"
    );
}

/// `graph_query`'s `map_bridges` kind wires through the exported ABI - the SAME real FFI
/// boundary the `map_landmarks` test above proves, but a genuinely different match arm in
/// `op_map_query` (a copy-paste swap between the two would compile clean and pass every
/// `dispatch_tests` case that only ever calls each kind by its own name).
#[test]
fn console_call_wires_graph_query_map_bridges_through_the_public_abi() {
    use rigger_console::console::map;

    let graph = cross_district_fixture();
    let payload = serde_json::to_vec(&graph).expect("serializing the fixture graph");

    unsafe {
        call("graph_load", std::str::from_utf8(&payload).unwrap());
        call("map_build", r#"{"w":900,"h":700}"#);
    }

    let queried = unsafe {
        call(
            "graph_query",
            r#"{"kind":"map_bridges","params":{"limit":5}}"#,
        )
    };

    let model = map::build(&graph);
    let direct = map::bridges_between_districts(&model, 5);
    assert!(
        !direct.is_empty(),
        "the fixture's own cross-district edge must produce at least one bridge candidate"
    );
    let expected_len = serde_json::to_vec(&serde_json::json!({ "candidates": direct }))
        .unwrap()
        .len();

    assert_eq!(
        reply_len(queried),
        expected_len,
        "graph_query's map_bridges reply through the exported ABI must be the same length as \
         console::map::bridges_between_districts's own direct answer for the identical graph"
    );
}

/// `graph_query`'s `map_argued_about` kind wires through the exported ABI - the one `map_*` kind
/// whose `op_map_query` arm reads `session.graph` directly (not just `session.map`), so this is
/// the one test proving that second piece of session state actually crosses the real ABI too.
#[test]
fn console_call_wires_graph_query_map_argued_about_through_the_public_abi() {
    use rigger_console::console::map;
    use rigger_domain::contextgraph::{Edge, Node, KIND_FINDING, REL_ABOUT, TIER_EXTRACTED};
    use rigger_domain::eventstore::Position;

    let mut graph = cross_district_fixture();
    graph.nodes.push(Node {
        id: "finding/1".to_string(),
        kind: KIND_FINDING.to_string(),
        attrs: Default::default(),
    });
    graph.edges.push(Edge {
        from: "finding/1".to_string(),
        to: "src/worktree.rs::hub".to_string(),
        rel: REL_ABOUT.to_string(),
        valid_from: 0,
        valid_to: None,
        source: Position::default(),
        tier: TIER_EXTRACTED.to_string(),
    });
    let payload = serde_json::to_vec(&graph).expect("serializing the fixture graph");

    unsafe {
        call("graph_load", std::str::from_utf8(&payload).unwrap());
        call("map_build", r#"{"w":900,"h":700}"#);
    }

    let queried = unsafe {
        call(
            "graph_query",
            r#"{"kind":"map_argued_about","params":{"limit":5}}"#,
        )
    };

    let model = map::build(&graph);
    let direct = map::argued_about_in_review(&model, &graph, 5);
    assert!(
        !direct.is_empty(),
        "the fixture's own pinned finding must produce at least one argued-about candidate"
    );
    let expected_len = serde_json::to_vec(&serde_json::json!({ "candidates": direct }))
        .unwrap()
        .len();

    assert_eq!(
        reply_len(queried),
        expected_len,
        "graph_query's map_argued_about reply through the exported ABI must be the same length \
         as console::map::argued_about_in_review's own direct answer for the identical graph"
    );
}

/// `map_argued_about`'s live-edge-only filter (`console::map::argued_about_in_review`'s own
/// `valid_to.is_some()` skip) wires through the exported ABI - the test above this one only ever
/// pins findings with LIVE edges, so it could not catch a regression that counted a SUPERSEDED
/// finding too. This fixture pins one live finding on `hub` and one SUPERSEDED finding (`valid_to:
/// Some(...)`) on `bridge` - `bridge` must never appear in the reply, only `hub`.
#[test]
fn console_call_wires_graph_query_map_argued_about_excludes_a_superseded_finding_edge_through_the_public_abi(
) {
    use rigger_console::console::map;
    use rigger_domain::contextgraph::{Edge, Node, KIND_FINDING, REL_ABOUT, TIER_EXTRACTED};
    use rigger_domain::eventstore::Position;

    let mut graph = cross_district_fixture();
    graph.nodes.push(Node {
        id: "finding/live".to_string(),
        kind: KIND_FINDING.to_string(),
        attrs: Default::default(),
    });
    graph.edges.push(Edge {
        from: "finding/live".to_string(),
        to: "src/worktree.rs::hub".to_string(),
        rel: REL_ABOUT.to_string(),
        valid_from: 0,
        valid_to: None,
        source: Position::default(),
        tier: TIER_EXTRACTED.to_string(),
    });
    graph.nodes.push(Node {
        id: "finding/superseded".to_string(),
        kind: KIND_FINDING.to_string(),
        attrs: Default::default(),
    });
    graph.edges.push(Edge {
        from: "finding/superseded".to_string(),
        to: "src/dash.rs::bridge".to_string(),
        rel: REL_ABOUT.to_string(),
        valid_from: 0,
        valid_to: Some(100),
        source: Position::default(),
        tier: TIER_EXTRACTED.to_string(),
    });
    let payload = serde_json::to_vec(&graph).expect("serializing the fixture graph");

    unsafe {
        call("graph_load", std::str::from_utf8(&payload).unwrap());
        call("map_build", r#"{"w":900,"h":700}"#);
    }

    let queried = unsafe {
        call(
            "graph_query",
            r#"{"kind":"map_argued_about","params":{"limit":5}}"#,
        )
    };

    let model = map::build(&graph);
    let direct = map::argued_about_in_review(&model, &graph, 5);
    assert!(
        direct.iter().any(|c| c.id == "src/worktree.rs::hub"),
        "the live-pinned entity must still be a candidate: {direct:?}"
    );
    assert!(
        direct.iter().all(|c| c.id != "src/dash.rs::bridge"),
        "the entity whose ONLY pinned finding is superseded must never be a candidate: {direct:?}"
    );
    let expected_len = serde_json::to_vec(&serde_json::json!({ "candidates": direct }))
        .unwrap()
        .len();

    assert_eq!(
        reply_len(queried),
        expected_len,
        "graph_query's map_argued_about reply through the exported ABI must exclude the \
         superseded finding edge the same way console::map::argued_about_in_review's own direct \
         answer does"
    );
}

/// `graph_query`'s `map_changing` kind wires through the exported ABI - the one `map_*` kind
/// whose own params carry a field (`touched`) none of the others do, so this is the one test
/// proving THAT field actually deserializes across the real ABI boundary, not merely within
/// `ChangingParams`'s own crate-internal `dispatch_tests`.
#[test]
fn console_call_wires_graph_query_map_changing_through_the_public_abi() {
    use rigger_console::console::map;

    let graph = cross_district_fixture();
    let payload = serde_json::to_vec(&graph).expect("serializing the fixture graph");

    unsafe {
        call("graph_load", std::str::from_utf8(&payload).unwrap());
        call("map_build", r#"{"w":900,"h":700}"#);
    }

    let queried = unsafe {
        call(
            "graph_query",
            r#"{"kind":"map_changing","params":{"limit":5,"touched":["src/worktree.rs::leaf"]}}"#,
        )
    };

    let model = map::build(&graph);
    let touched: std::collections::BTreeSet<String> =
        ["src/worktree.rs::leaf".to_string()].into_iter().collect();
    let direct = map::changing_right_now(&model, &touched, 5);
    assert_eq!(
        direct.len(),
        1,
        "the fixture's own single touched id must produce exactly one candidate: {direct:?}"
    );
    let expected_len = serde_json::to_vec(&serde_json::json!({ "candidates": direct }))
        .unwrap()
        .len();

    assert_eq!(
        reply_len(queried),
        expected_len,
        "graph_query's map_changing reply through the exported ABI must be the same length as \
         console::map::changing_right_now's own direct answer for the identical touched set"
    );
}

/// `graph_query`'s `map_search` kind wires through the exported ABI - the one `map_*` kind whose
/// reply wraps its rows in `"hits"`, never `"candidates"` (the shape every other kind above and
/// below this test answers), so this is the one test proving that DIFFERENT top-level wire key
/// actually crosses the real ABI, not merely `op_map_query`'s own in-crate call.
#[test]
fn console_call_wires_graph_query_map_search_through_the_public_abi() {
    use rigger_console::console::map;

    let graph = cross_district_fixture();
    let payload = serde_json::to_vec(&graph).expect("serializing the fixture graph");

    unsafe {
        call("graph_load", std::str::from_utf8(&payload).unwrap());
        call("map_build", r#"{"w":900,"h":700}"#);
    }

    let queried = unsafe {
        call(
            "graph_query",
            r#"{"kind":"map_search","params":{"query":"HUB","limit":5}}"#,
        )
    };

    let model = map::build(&graph);
    let direct = map::search(&model, "HUB", 5);
    assert_eq!(
        direct.len(),
        1,
        "the fixture's own case-insensitive 'HUB' query must find exactly hub: {direct:?}"
    );
    let expected_len = serde_json::to_vec(&serde_json::json!({ "hits": direct }))
        .unwrap()
        .len();

    assert_eq!(
        reply_len(queried),
        expected_len,
        "graph_query's map_search reply through the exported ABI must be the same length as \
         console::map::search's own direct answer for the identical query"
    );
}

/// `graph_query`'s `map_fit_whole` kind wires through the exported ABI - the one `map_*` kind
/// with NO params at all and a bare `{"cx":...,"cy":...,"zoom":...}` reply (never the
/// `"candidates"`/`"hits"` wrapper every ranked-list kind above answers).
#[test]
fn console_call_wires_graph_query_map_fit_whole_through_the_public_abi() {
    use rigger_console::console::map;

    let graph = cross_district_fixture();
    let payload = serde_json::to_vec(&graph).expect("serializing the fixture graph");

    unsafe {
        call("graph_load", std::str::from_utf8(&payload).unwrap());
        call("map_build", r#"{"w":900,"h":700}"#);
    }

    let queried = unsafe { call("graph_query", r#"{"kind":"map_fit_whole","params":{}}"#) };

    let model = map::build(&graph);
    let direct = map::fit_whole_map(&model);
    let expected_len = serde_json::to_vec(&serde_json::json!({
        "cx": direct.cx,
        "cy": direct.cy,
        "zoom": direct.zoom
    }))
    .unwrap()
    .len();

    assert_eq!(
        reply_len(queried),
        expected_len,
        "graph_query's map_fit_whole reply through the exported ABI must be the same length as \
         console::map::fit_whole_map's own direct answer for the identical graph"
    );
}

/// `graph_query`'s `map_fit_district` kind wires through the exported ABI on its SUCCESS path -
/// the same bare `{"cx":...,"cy":...,"zoom":...}` shape `map_fit_whole` answers, but reached
/// through a distinct params struct (`purpose`, a real district name from the built model) and a
/// distinct source of the viewport (`session.map_viewport`, set by the prior `map_build` call -
/// never resent on this op's own params, unlike `map_build`/`map_hit` above).
#[test]
fn console_call_wires_graph_query_map_fit_district_through_the_public_abi() {
    use rigger_console::console::map;

    let graph = cross_district_fixture();
    let payload = serde_json::to_vec(&graph).expect("serializing the fixture graph");

    unsafe {
        call("graph_load", std::str::from_utf8(&payload).unwrap());
        call("map_build", r#"{"w":900,"h":700}"#);
    }

    let model = map::build(&graph);
    let purpose = model
        .districts
        .first()
        .expect("the fixture's own two districts")
        .purpose
        .clone();
    let params =
        serde_json::json!({ "kind": "map_fit_district", "params": { "purpose": purpose } });

    let queried = unsafe { call("graph_query", &params.to_string()) };

    let direct = map::fit_district(&model, 900.0, 700.0, &purpose)
        .expect("the fixture's own district, read back from the built model, must fit");
    let expected_len = serde_json::to_vec(&serde_json::json!({
        "cx": direct.cx,
        "cy": direct.cy,
        "zoom": direct.zoom
    }))
    .unwrap()
    .len();

    assert_eq!(
        reply_len(queried),
        expected_len,
        "graph_query's map_fit_district reply through the exported ABI must be the same length \
         as console::map::fit_district's own direct answer for the identical district"
    );
}

/// `graph_query`'s `map_fit_district` kind's ERROR path (an unknown district purpose) wires
/// through the exported ABI to the same `{"error": ...}` reply every other malformed/unresolved
/// op call answers - `map_fit_district` is the only `map_*` kind with a real error branch of its
/// own (every ranked-list kind above always answers a - possibly empty - list), so this is the
/// one test proving that branch reaches the real exported ABI, not merely `op_map_query`'s own
/// in-crate call.
#[test]
fn console_call_graph_query_map_fit_district_of_unknown_purpose_answers_an_error_through_the_public_abi(
) {
    use rigger_console::console::map;

    let graph = cross_district_fixture();
    let payload = serde_json::to_vec(&graph).expect("serializing the fixture graph");

    unsafe {
        call("graph_load", std::str::from_utf8(&payload).unwrap());
        call("map_build", r#"{"w":900,"h":700}"#);
    }

    let queried = unsafe {
        call(
            "graph_query",
            r#"{"kind":"map_fit_district","params":{"purpose":"does-not-exist"}}"#,
        )
    };

    let model = map::build(&graph);
    assert_eq!(
        map::fit_district(&model, 900.0, 700.0, "does-not-exist"),
        None,
        "the fixture's own two districts must never carry this made-up purpose"
    );
    let expected_len = serde_json::to_vec(&serde_json::json!({
        "error": format!(
            "graph_query: map_fit_district: unknown district {:?}",
            "does-not-exist"
        )
    }))
    .unwrap()
    .len();

    assert_eq!(
        reply_len(queried),
        expected_len,
        "graph_query's map_fit_district error reply through the exported ABI must be the same \
         length as op_map_query's own documented error-message shape for an unknown purpose"
    );
}

/// `graph_query`'s `map_fit_entity` kind (round 2, closing the rail/search fly-to-entity gap)
/// wires through the exported ABI on its SUCCESS path - the same bare
/// `{"cx":...,"cy":...,"zoom":...}` shape `map_fit_district` answers, reached through its own
/// `id` params struct.
#[test]
fn console_call_wires_graph_query_map_fit_entity_through_the_public_abi() {
    use rigger_console::console::map;

    let graph = cross_district_fixture();
    let payload = serde_json::to_vec(&graph).expect("serializing the fixture graph");

    unsafe {
        call("graph_load", std::str::from_utf8(&payload).unwrap());
        call("map_build", r#"{"w":900,"h":700}"#);
    }

    let model = map::build(&graph);
    let id = model
        .entities
        .first()
        .expect("the fixture's own entities")
        .id
        .clone();
    let params = serde_json::json!({ "kind": "map_fit_entity", "params": { "id": id } });

    let queried = unsafe { call("graph_query", &params.to_string()) };

    let direct = map::fit_entity(&model, 900.0, 700.0, &id)
        .expect("the fixture's own entity, read back from the built model, must fit");
    let expected_len = serde_json::to_vec(&serde_json::json!({
        "cx": direct.cx,
        "cy": direct.cy,
        "zoom": direct.zoom
    }))
    .unwrap()
    .len();

    assert_eq!(
        reply_len(queried),
        expected_len,
        "graph_query's map_fit_entity reply through the exported ABI must be the same length as \
         console::map::fit_entity's own direct answer for the identical entity"
    );
}

/// `graph_query`'s `map_fit_entity` kind's ERROR path (an unknown entity id) wires through the
/// exported ABI to the same `{"error": ...}` reply every other malformed/unresolved op call
/// answers - the same proof `map_fit_district`'s own unknown-purpose test gives its sibling kind.
#[test]
fn console_call_graph_query_map_fit_entity_of_unknown_id_answers_an_error_through_the_public_abi() {
    use rigger_console::console::map;

    let graph = cross_district_fixture();
    let payload = serde_json::to_vec(&graph).expect("serializing the fixture graph");

    unsafe {
        call("graph_load", std::str::from_utf8(&payload).unwrap());
        call("map_build", r#"{"w":900,"h":700}"#);
    }

    let queried = unsafe {
        call(
            "graph_query",
            r#"{"kind":"map_fit_entity","params":{"id":"does-not-exist"}}"#,
        )
    };

    let model = map::build(&graph);
    assert_eq!(
        map::fit_entity(&model, 900.0, 700.0, "does-not-exist"),
        None,
        "the fixture's own entities must never carry this made-up id"
    );
    let expected_len = serde_json::to_vec(&serde_json::json!({
        "error": format!(
            "graph_query: map_fit_entity: unknown entity {:?}",
            "does-not-exist"
        )
    }))
    .unwrap()
    .len();

    assert_eq!(
        reply_len(queried),
        expected_len,
        "graph_query's map_fit_entity error reply through the exported ABI must be the same \
         length as op_map_query's own documented error-message shape for an unknown id"
    );
}

/// A single-community fixture built to put two entities' dots within [`map::HIT_RADIUS_PX`]-
/// equivalent screen range of one another WITHOUT drifting them onto the low-rank ids `hit`'s own
/// budget already admits: every one of `N` members gets 3 self-loop `REL_CALLS` edges (degree
/// `4` counting the shared `REL_IN_COMMUNITY` edge) EXCEPT `a`/`b`, which get 2 self-loops plus
/// ONE mutual `REL_CALLS` edge between themselves - tying every member at the SAME degree, so
/// rank is pure id order (`a`/`b`'s own ids place them at ranks 494/549, far past any zoom's
/// `budget`). That same mutual edge makes `b` one of `a`'s own [`map::capped_neighbors`] (never
/// exported, but exercised the same way `frame`'s own CONSTRAINTS WALK test already forces an
/// out-of-budget neighbour into view), so selecting `a` draws BOTH regardless of budget - letting
/// their real, degree-tied-shortest world-distance (~52 units, the golden-angle spiral's own
/// floor, confirmed empirically, never a hardcoded magic number) be shrunk under any target pixel
/// gap by an ordinary positive zoom, far from every district pill's own reserved label footprint
/// (`a`'s own world radius is 700+ units out, nowhere near the district centre a pill reserves
/// around).
fn tiebreak_fixture() -> (rigger_domain::contextgraph::Graph, String, String) {
    use rigger_domain::contextgraph::{
        Edge, Graph, Node, KIND_CODE_ENTITY, REL_CALLS, REL_IN_COMMUNITY, TIER_EXTRACTED,
    };
    use rigger_domain::eventstore::Position;

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
    let edge = |from: &str, to: &str, rel: &str| Edge {
        from: from.to_string(),
        to: to.to_string(),
        rel: rel.to_string(),
        valid_from: 0,
        valid_to: None,
        source: Position::default(),
        tier: TIER_EXTRACTED.to_string(),
    };

    const N: usize = 600;
    const A_IDX: usize = 494;
    const B_IDX: usize = 549;
    let id_of = |i: usize| format!("src/z.rs::e{i:03}");

    let mut nodes = Vec::new();
    let mut edges = Vec::new();
    for i in 0..N {
        let id = id_of(i);
        nodes.push(code(&id, &format!("e{i:03}")));
        edges.push(edge(&id, "community/1/0", REL_IN_COMMUNITY));
        let self_loops = if i == A_IDX || i == B_IDX { 2 } else { 3 };
        for _ in 0..self_loops {
            edges.push(edge(&id, &id, REL_CALLS));
        }
    }
    edges.push(edge(&id_of(A_IDX), &id_of(B_IDX), REL_CALLS));

    (Graph { nodes, edges }, id_of(A_IDX), id_of(B_IDX))
}

/// `map_hit`'s nearest-wins tie-break (the `d < bd` replacement inside `console::map::hit`'s own
/// entity loop - HIT_RADIUS_PX's doc cites a regression test for exactly this, but never proves
/// it crossing the real ABI): two entities both within hit range of one click must answer the
/// STRICTLY NEARER one, not merely the first one the loop happens to examine. `a` (the lower-rank
/// selection, examined FIRST since `MapModel::entities` walks rank-ascending) sits at the click
/// itself's own equal-but-nonzero distance while `b` (examined SECOND, as a forced neighbour) sits
/// exactly ON the click - if `hit` merely kept its first in-range match instead of comparing
/// distances, this would answer `a` and fail.
#[test]
fn console_call_wires_map_hit_through_the_public_abi_preferring_the_nearer_of_two_in_range_dots() {
    use rigger_console::console::map;

    let (graph, a_id, b_id) = tiebreak_fixture();
    let model = map::build(&graph);
    let (vw, vh) = (900.0, 700.0);

    // Measure the pair's FULL-EXTENT screen gap (both forced into view by selecting `a`) to
    // derive a zoom that shrinks it under HIT_RADIUS_PX, without hardcoding a magic scale.
    let full = map::frame(&model, vw, vh, &map::Camera::default(), Some(a_id.as_str()));
    let a0 = full
        .entities
        .iter()
        .find(|e| e.id == a_id)
        .expect("a drawn at full extent as the selection");
    let b0 = full
        .entities
        .iter()
        .find(|e| e.id == b_id)
        .expect("b drawn at full extent as a's own forced neighbour");
    let d0 = ((a0.x - b0.x).powi(2) + (a0.y - b0.y).powi(2)).sqrt();
    assert!(
        d0 > 0.0,
        "the fixture's own pair must not already coincide at full extent"
    );

    const TARGET_GAP_PX: f64 = 6.0;
    let zoom = TARGET_GAP_PX / d0;
    let camera = map::Camera {
        cx: 0.0,
        cy: 0.0,
        zoom,
    };

    let dl = map::frame(&model, vw, vh, &camera, Some(a_id.as_str()));
    let a = dl
        .entities
        .iter()
        .find(|e| e.id == a_id)
        .expect("a must still be drawn at the shrunk zoom");
    let b = dl
        .entities
        .iter()
        .find(|e| e.id == b_id)
        .expect("b must still be drawn at the shrunk zoom, close beside a");
    let gap = ((a.x - b.x).powi(2) + (a.y - b.y).powi(2)).sqrt();
    assert!(
        gap < 10.0,
        "the fixture's own shrunk pair must land inside map_hit's real hit radius: {gap}px"
    );

    // The click lands EXACTLY on b's own dot - a is close (the shrunk gap above) but strictly
    // farther, so a correct hit() must answer b, never a.
    let (px, py) = (b.x, b.y);
    let a_dist_to_click = ((a.x - px).powi(2) + (a.y - py).powi(2)).sqrt();
    assert!(
        a_dist_to_click > 0.0 && a_dist_to_click < 10.0,
        "a must be a GENUINE competing candidate (in range, but farther than b) for this test to \
         exercise the tie-break at all: {a_dist_to_click}px"
    );

    let direct = map::hit(&model, vw, vh, &camera, Some(a_id.as_str()), px, py);
    assert_eq!(
        direct,
        Some(map::Hit::Entity(b_id.clone())),
        "console::map::hit must prefer the strictly nearer b over the farther-but-still-in-range \
         a, examined first"
    );

    unsafe {
        call(
            "graph_load",
            std::str::from_utf8(&serde_json::to_vec(&graph).unwrap()).unwrap(),
        );
        call("map_build", &format!(r#"{{"w":{vw},"h":{vh}}}"#));
    }
    let hit = unsafe {
        call(
            "map_hit",
            &format!(r#"{{"zoom":{zoom},"cx":0,"cy":0,"sel":"{a_id}","x":{px},"y":{py}}}"#),
        )
    };
    let expected_len = serde_json::to_vec(&serde_json::json!({ "hit": "entity", "id": b_id }))
        .unwrap()
        .len();
    assert_eq!(
        reply_len(hit),
        expected_len,
        "map_hit's tie-break reply through the exported ABI must be the same length as the \
         reply shape built from console::map::hit's own direct (correct) answer"
    );
}

/// `graph_query`'s `map_legend` kind (spec 84 criterion 3) wires through the exported ABI,
/// answering the SAME rows `console::map::legend` answers directly - and, unlike every other
/// `map_*` kind, needs NEITHER a loaded graph NOR a built map first: the legend is static
/// content (this call makes no `graph_load`/`map_build` call at all, proving the "no map built"
/// precondition every other map kind carries does not apply here).
#[test]
fn console_call_wires_graph_query_map_legend_through_the_public_abi_before_any_map_is_built() {
    use rigger_console::console::map;

    let queried = unsafe { call("graph_query", r#"{"kind":"map_legend","params":{}}"#) };

    let expected_len = serde_json::to_vec(&serde_json::json!({ "entries": map::legend() }))
        .unwrap()
        .len();

    assert_eq!(
        reply_len(queried),
        expected_len,
        "graph_query's map_legend reply through the exported ABI, called with no prior \
         graph_load/map_build, must be the same length as console::map::legend's own direct \
         answer"
    );
}

/// `scrub_track` (spec 94 criterion 3, THE POSITION MODEL) wires through the exported ABI to
/// `console::scrub_track`'s own marks/ticks - proven crossing the real FFI boundary, not merely
/// through this crate's own private `dispatch_tests` (already covered in `src/lib.rs`). Like
/// `map_legend` above, needs no prior `graph_load`/`map_build`: only a `fold_reset`'d session.
#[test]
fn console_call_wires_scrub_track_through_the_public_abi() {
    use rigger_console::console;
    use rigger_domain::eventstore::Event;

    let events_json = r#"{"events":[
        {"type":"UnitIntegrated","data":{"id":"u1","commit":"abc"},"position":1}
    ]}"#;
    let reset = unsafe { call("fold_reset", events_json) };
    assert!(
        reply_len(reset) > 0,
        "fold_reset must answer a non-empty reply through the exported ABI"
    );

    let queried = unsafe { call("scrub_track", "{}") };

    let mut event = Event::new("UnitIntegrated", br#"{"id":"u1","commit":"abc"}"#.to_vec());
    event.position = 1;
    let direct = console::scrub_track(&[event]);
    let expected_len = serde_json::to_vec(&serde_json::json!({
        "marks": direct.marks,
        "ticks": direct.ticks
    }))
    .unwrap()
    .len();

    assert_eq!(
        reply_len(queried),
        expected_len,
        "scrub_track's reply through the exported ABI must be the same length as \
         console::scrub_track's own direct answer for the identical event log"
    );
}

/// `palette_commands` (spec 94 criterion 4, THE PALETTE) wires through the exported ABI to
/// `console::palette_commands`'s own entries - proven crossing the real FFI boundary, not
/// merely through this crate's own private `dispatch_tests` (already covered in `src/lib.rs`).
/// Like `scrub_track` above, needs no prior `graph_load`/`map_build`: only a `fold_reset`'d
/// session, and a stream that carries both a unit and a recorded spawn so the reply's length
/// differs from the empty-session case, not merely a vacuous match.
#[test]
fn console_call_wires_palette_commands_through_the_public_abi() {
    use rigger_console::console;
    use rigger_domain::eventstore::Event;

    let events_json = r#"{"events":[
        {"type":"UnitStarted","data":{"id":"u1"},"position":1},
        {"type":"SpawnRequested","data":{"id":"u1/implementer#0","unit":"u1","stage":"implement","prompt":"do it"},"position":2}
    ]}"#;
    let reset = unsafe { call("fold_reset", events_json) };
    assert!(
        reply_len(reset) > 0,
        "fold_reset must answer a non-empty reply through the exported ABI"
    );

    let queried = unsafe { call("palette_commands", "{}") };

    let mut e1 = Event::new("UnitStarted", br#"{"id":"u1"}"#.to_vec());
    e1.position = 1;
    let mut e2 = Event::new(
        "SpawnRequested",
        br#"{"id":"u1/implementer#0","unit":"u1","stage":"implement","prompt":"do it"}"#.to_vec(),
    );
    e2.position = 2;
    let events = [e1, e2];
    let state = console::fold(&events, 0).unwrap();
    let direct = console::palette_commands(&events, &state.units);
    let expected_len = serde_json::to_vec(&serde_json::json!({ "commands": direct }))
        .unwrap()
        .len();

    assert_eq!(
        reply_len(queried),
        expected_len,
        "palette_commands's reply through the exported ABI must be the same length as \
         console::palette_commands's own direct answer for the identical event log"
    );
}

/// `palette_commands` degrades on a malformed recorded `SpawnRequested` event THROUGH THE
/// EXPORTED ABI - never fails (CONSTRAINTS WALK: "An older run lacking a field - the fold
/// renders the blank, never fails") - not merely through this crate's own private
/// `dispatch_tests`
/// (`palette_commands_omits_only_a_malformed_recorded_spawns_agent_row`, `src/lib.rs`, which
/// pushes the malformed event directly onto `session.events` in-process, never crossing
/// `console_call`'s pointer-packing marshaling). This native host's packed reply pointer is
/// unreadable (see this file's own header), so - exactly like
/// `console_call_wires_palette_commands_through_the_public_abi` above - the proof is a length
/// match against `console::palette_commands`'s own direct (degraded) answer for the identical
/// malformed event log, the strongest content proof available at this boundary; a stale `?`
/// that still propagated the parse error would instead answer the much shorter `{"error":
/// ...}` reply, so the length match is a real behavioral assertion, not a vacuous one. The
/// malformed spawn event still folds cleanly through the real `fold_reset` op first - neither
/// `ledger::project` nor `blocker::from_events` reads a `SpawnRequested` payload (confirmed by
/// reading both) - so this is the one way to land a syntactically-valid-wire,
/// semantically-malformed spawn event in a session through the real ABI at all, then observe
/// `palette_commands` degrade on it for real.
#[test]
fn console_call_palette_commands_degrades_on_a_malformed_recorded_spawn_through_the_public_abi() {
    use rigger_console::console;
    use rigger_domain::eventstore::Event;

    // A JSON string, not a spawn-request object - well-formed wire JSON (so fold_reset's own
    // parsing and console::fold succeed) but not a shape SpawnRequest::from_event can parse.
    let events_json = r#"{"events":[
        {"type":"SpawnRequested","data":"not a spawn request","position":1}
    ]}"#;
    let reset = unsafe { call("fold_reset", events_json) };
    assert!(
        reply_len(reset) > 0,
        "fold_reset must fold the malformed spawn event cleanly through the real ABI \
         (its own wire JSON is well-formed; neither ledger::project nor blocker::from_events \
         reads a SpawnRequested payload)"
    );

    let queried = unsafe { call("palette_commands", "{}") };

    let mut bad = Event::new("SpawnRequested", br#""not a spawn request""#.to_vec());
    bad.position = 1;
    let events = [bad];
    let state = console::fold(&events, 0).unwrap();
    let direct = console::palette_commands(&events, &state.units);
    assert!(
        direct.iter().all(|c| c.kind != "agent"),
        "a malformed spawn must never produce an agent row: {direct:?}"
    );
    let expected_len = serde_json::to_vec(&serde_json::json!({ "commands": direct }))
        .unwrap()
        .len();

    assert_eq!(
        reply_len(queried),
        expected_len,
        "palette_commands must still answer a real, degraded (never error) reply through the \
         exported ABI when a recorded SpawnRequested is malformed - matching \
         console::palette_commands's own direct answer for the identical event log"
    );
}

/// The round-3 fix's own required proof (`adj-u94c4-r2-verdict-reject-palette-commands-fail-total`:
/// "add a case proving a valid spawn survives alongside a malformed one in the SAME event
/// log"), THROUGH THE EXPORTED ABI. That mixed-log case landed only as an in-process unit test
/// (`palette_commands_keeps_a_valid_spawn_alongside_a_malformed_one`, `crates/rigger-console/src/console/mod.rs`) and
/// this crate's own private `dispatch_tests` backdoor still covers only the single-malformed-
/// only scenario - never crossing `console_call`'s pointer-packing marshaling with BOTH an agent
/// worth keeping and one worth dropping in the SAME session. The single-malformed ABI test
/// above only proves the degrade is total when NOTHING survives; it cannot tell "skips
/// per-event" apart from "an all-or-nothing guard that happens to also degrade instead of
/// error" - only a mixed log does that, exactly the distinction `d-u94c4-r3-palette-commands-
/// degrades-not-fails` exists to fix.
#[test]
fn console_call_palette_commands_keeps_a_valid_spawn_alongside_a_malformed_one_through_the_public_abi(
) {
    use rigger_console::console;
    use rigger_domain::eventstore::Event;

    let events_json = r#"{"events":[
        {"type":"SpawnRequested","data":{"id":"u1/implementer#0","unit":"u1","stage":"implement","prompt":"do it"},"position":1},
        {"type":"SpawnRequested","data":"not a spawn request","position":2}
    ]}"#;
    let reset = unsafe { call("fold_reset", events_json) };
    assert!(
        reply_len(reset) > 0,
        "fold_reset must fold a mixed valid/malformed spawn log cleanly through the real ABI"
    );

    let queried = unsafe { call("palette_commands", "{}") };

    let mut valid = Event::new(
        "SpawnRequested",
        br#"{"id":"u1/implementer#0","unit":"u1","stage":"implement","prompt":"do it"}"#.to_vec(),
    );
    valid.position = 1;
    let mut bad = Event::new("SpawnRequested", br#""not a spawn request""#.to_vec());
    bad.position = 2;
    let events = [valid, bad];
    let state = console::fold(&events, 0).unwrap();
    let direct = console::palette_commands(&events, &state.units);
    let agents: Vec<&str> = direct
        .iter()
        .filter(|c| c.kind == "agent")
        .map(|c| c.id.as_str())
        .collect();
    assert_eq!(
        agents,
        vec!["u1/implementer#0"],
        "test setup must actually keep the one valid agent while dropping the malformed entry: \
         {direct:?}"
    );
    let expected_len = serde_json::to_vec(&serde_json::json!({ "commands": direct }))
        .unwrap()
        .len();

    assert_eq!(
        reply_len(queried),
        expected_len,
        "palette_commands must keep the valid agent alongside a dropped malformed one through \
         the exported ABI - the same length as console::palette_commands's own direct answer \
         for the identical mixed event log"
    );
}

/// `palette_commands` scopes its agent section to the SAME explicit `position` argument as
/// its courtroom section, THROUGH THE EXPORTED ABI (spec 94 criterion 4, THE PALETTE;
/// adv-u94c4-r3-palette-agent-list-ignores-scrub-cursor-courtroom-list-does-not). Round-3
/// fixed this via an ambient `session.cursor` `fold_at` set as a side effect; round-4's own
/// REQUIRED FIX (adj-u94c4-r4-verdict-reject-push-while-scrubbed-desync) replaced that with
/// an explicit `position` argument `palette_commands` itself takes, since the ambient
/// cursor could be silently reset to live by an intervening `fold_push`/`fold_reset` - see
/// `console_call_palette_commands_answers_the_scrubbed_window_even_after_a_live_push_races_it_through_the_public_abi`
/// below for that race, proven through this SAME exported ABI. This test proves the static
/// case still holds under the new explicit-argument design - no `fold_at` call at all, just
/// `palette_commands` called directly with `{"position":2}`. This native host's packed
/// reply pointer is unreadable (see this file's own header), so the proof is a length match
/// against `console::palette_commands`'s own direct answer over the SAME truncated event
/// window - and, so the match is not vacuous, the direct answer is built from an events
/// slice that explicitly EXCLUDES u2 and its spawn (asserted below before the length is
/// even computed), so a regressed scoping (the agent section falling back to the full
/// unscoped log) would answer a LONGER reply through the real ABI than this scrubbed-window
/// length, not merely the same one by coincidence.
#[test]
fn console_call_palette_commands_scopes_to_an_explicit_position_through_the_public_abi() {
    use rigger_console::console;
    use rigger_domain::eventstore::Event;

    let events_json = r#"{"events":[
        {"type":"UnitStarted","data":{"id":"u1"},"position":1},
        {"type":"SpawnRequested","data":{"id":"u1/implementer#0","unit":"u1","stage":"implement","prompt":"do it"},"position":2},
        {"type":"UnitStarted","data":{"id":"u2"},"position":3},
        {"type":"SpawnRequested","data":{"id":"u2/implementer#0","unit":"u2","stage":"implement","prompt":"do it"},"position":4}
    ]}"#;
    let reset = unsafe { call("fold_reset", events_json) };
    assert!(
        reply_len(reset) > 0,
        "fold_reset must fold the four-event stream cleanly through the real ABI"
    );

    // No `fold_at` call at all - ask `palette_commands` directly for position 2, through
    // the real exported ABI, before u2 ever appears in the argument.
    let queried = unsafe { call("palette_commands", r#"{"position":2}"#) };

    // The requested prefix only: u1's own two events. u2 and its spawn are past that
    // position and must be absent from BOTH the courtroom and the agent sections.
    let mut e1 = Event::new("UnitStarted", br#"{"id":"u1"}"#.to_vec());
    e1.position = 1;
    let mut e2 = Event::new(
        "SpawnRequested",
        br#"{"id":"u1/implementer#0","unit":"u1","stage":"implement","prompt":"do it"}"#.to_vec(),
    );
    e2.position = 2;
    let events = [e1, e2];
    let state = console::fold(&events, 0).unwrap();
    let direct = console::palette_commands(&events, &state.units);
    let courtrooms: Vec<&str> = direct
        .iter()
        .filter(|c| c.kind == "courtroom")
        .map(|c| c.id.as_str())
        .collect();
    let agents: Vec<&str> = direct
        .iter()
        .filter(|c| c.kind == "agent")
        .map(|c| c.id.as_str())
        .collect();
    assert_eq!(
        courtrooms,
        vec!["u1"],
        "test setup must scope the courtroom section to u1 only, excluding u2: {direct:?}"
    );
    assert_eq!(
        agents,
        vec!["u1/implementer#0"],
        "test setup must scope the agent section to u1's own spawn only, excluding u2's: \
         {direct:?}"
    );
    let expected_len = serde_json::to_vec(&serde_json::json!({ "commands": direct }))
        .unwrap()
        .len();

    assert_eq!(
        reply_len(queried),
        expected_len,
        "palette_commands must scope its agent section to the SAME explicit position as its \
         courtroom section through the exported ABI - a regression (the agent section \
         falling back to the full unscoped log) would answer a longer reply than this \
         scoped-window length, not the same one"
    );
}

/// THE REQUIRED FIX's own proof (adj-u94c4-r4-verdict-reject-push-while-scrubbed-desync),
/// THROUGH THE EXPORTED ABI - mirroring
/// `palette_commands_answers_the_scrubbed_window_even_after_a_live_push_races_it` in
/// `src/lib.rs`'s own private `dispatch_tests`, which never crosses `console_call`'s
/// pointer-packing marshaling: (a) fold to a scrub position via `fold_at`, (b) push a new
/// event via `fold_push` with NO intervening `fold_at` call (exactly what `connectStream`'s
/// push handler does in `crates/rigger-dash/src/console.html` while the page is scrubbed - it calls
/// `fold_push` unconditionally, then skips `render()`/`fold_at` because `STATE.live` is
/// false), (c) assert `palette_commands`, called with that SAME position, still answers the
/// SCRUBBED window through the real ABI, not the live one a regressed ambient-cursor design
/// would silently answer instead. As above, the packed reply pointer is unreadable on this
/// native host, so the proof is a length match against `console::palette_commands`'s own
/// direct answer over the scrubbed window - made non-vacuous by first asserting that window
/// explicitly excludes u2 (the pushed unit), so a regression (falling back to the
/// now-extended live log) would answer a LONGER reply, not coincidentally the same one.
#[test]
fn console_call_palette_commands_answers_the_scrubbed_window_even_after_a_live_push_races_it_through_the_public_abi(
) {
    use rigger_console::console;
    use rigger_domain::eventstore::Event;

    let events_json = r#"{"events":[
        {"type":"UnitStarted","data":{"id":"u1"},"position":1},
        {"type":"SpawnRequested","data":{"id":"u1/implementer#0","unit":"u1","stage":"implement","prompt":"do it"},"position":2}
    ]}"#;
    let reset = unsafe { call("fold_reset", events_json) };
    assert!(
        reply_len(reset) > 0,
        "fold_reset must fold the two-event stream cleanly through the real ABI"
    );

    // (a) Scrub back to right after u1's own spawn, through the real ABI.
    let scrubbed = unsafe { call("fold_at", r#"{"position":2}"#) };
    assert!(
        reply_len(scrubbed) > 0,
        "fold_at must answer a non-empty reply through the exported ABI"
    );

    // (b) A live event streams in - u2 and its own spawn - with NO intervening `fold_at`
    // call, through the real ABI.
    let pushed1 = unsafe {
        call(
            "fold_push",
            r#"{"type":"UnitStarted","data":{"id":"u2"},"position":3}"#,
        )
    };
    assert!(
        reply_len(pushed1) > 0,
        "fold_push must answer a non-empty reply through the exported ABI"
    );
    let pushed2 = unsafe {
        call(
            "fold_push",
            r#"{"type":"SpawnRequested","data":{"id":"u2/implementer#0","unit":"u2","stage":"implement","prompt":"do it"},"position":4}"#,
        )
    };
    assert!(
        reply_len(pushed2) > 0,
        "fold_push must answer a non-empty reply through the exported ABI"
    );

    // (c) Ask `palette_commands` for the SAME position 2 through the real ABI - it must
    // still exclude u2 and its spawn from BOTH sections, not silently answer live.
    let queried = unsafe { call("palette_commands", r#"{"position":2}"#) };

    let mut e1 = Event::new("UnitStarted", br#"{"id":"u1"}"#.to_vec());
    e1.position = 1;
    let mut e2 = Event::new(
        "SpawnRequested",
        br#"{"id":"u1/implementer#0","unit":"u1","stage":"implement","prompt":"do it"}"#.to_vec(),
    );
    e2.position = 2;
    let events = [e1, e2];
    let state = console::fold(&events, 0).unwrap();
    let direct = console::palette_commands(&events, &state.units);
    assert!(
        !direct.iter().any(|c| c.kind == "courtroom" && c.id == "u2"),
        "test setup's own scrubbed-window answer must exclude u2: {direct:?}"
    );
    let expected_len = serde_json::to_vec(&serde_json::json!({ "commands": direct }))
        .unwrap()
        .len();

    assert_eq!(
        reply_len(queried),
        expected_len,
        "palette_commands must answer the SCRUBBED window through the exported ABI even \
         after a live push races it with no intervening fold_at - a regression (falling \
         back to the now-extended live log) would answer a longer reply than this \
         scrubbed-window length, not the same one"
    );
}

/// `scrub_track`'s VERDICT marks read [`rigger_domain::spawn::Adjudication::verdict`] - spec 94
/// criterion 3's additive FIELD on the already-`pub` `Adjudication` struct (a plain
/// `pub fn`/`struct`/`enum`/`trait`/`const`/`type` grep misses a new field on an existing
/// item; this test exists precisely because that field never crosses a real boundary
/// anywhere else). Proven crossing the real exported ABI, not merely `console::mod`'s own
/// in-process tests (`scrub_track_marks_unit_verdicts_red_and_green`) or this crate's
/// private `dispatch_tests`, both of which call `dispatch`/`scrub_track` directly and never
/// go through `console_call`'s pointer-packing marshaling. A REJECT answers a different
/// reply length than an APPROVE would (their tooltips differ: "u1: reject" vs "u1:
/// approve"), each matching `console::scrub_track`'s own direct answer for the identical
/// adjudicator result - proving the verdict literal itself, not just a mark's bare
/// presence, survives the ABI.
#[test]
fn console_call_wires_scrub_track_reject_verdict_marks_through_the_public_abi() {
    use rigger_console::console;
    use rigger_domain::spawn::SpawnResult;

    let event = SpawnResult::ok(
        "u1/adjudicator#0",
        r#"{"verdict":"reject","cause":"genuine-defect"}"#,
    )
    .to_event()
    .unwrap();
    let data: serde_json::Value = serde_json::from_slice(&event.data).unwrap();
    let wire = serde_json::json!({
        "events": [{ "type": event.type_, "data": data, "position": 1 }]
    })
    .to_string();

    let reset = unsafe { call("fold_reset", &wire) };
    assert!(
        reply_len(reset) > 0,
        "fold_reset must answer a non-empty reply through the exported ABI"
    );
    let queried = unsafe { call("scrub_track", "{}") };

    let mut direct_event = event;
    direct_event.position = 1;
    let direct = console::scrub_track(&[direct_event]);
    assert_eq!(direct.marks.len(), 1, "test setup: {direct:?}");
    assert_eq!(
        direct.marks[0].color, "red",
        "test setup must actually produce a reject mark: {direct:?}"
    );
    let expected_len = serde_json::to_vec(&serde_json::json!({
        "marks": direct.marks,
        "ticks": direct.ticks
    }))
    .unwrap()
    .len();

    assert_eq!(
        reply_len(queried),
        expected_len,
        "scrub_track's reject verdict mark through the exported ABI must be the same length \
         as console::scrub_track's own direct answer for the identical adjudicator result"
    );
}

/// The APPROVE half of `console_call_wires_scrub_track_reject_verdict_marks_through_the_public_abi`'s
/// own doc - same real-ABI proof, the opposite verdict literal, so a regression that only
/// wired one branch of `spawn::Adjudication::verdict`'s two known values through the ABI
/// (or wired neither and always answered a fixed color) cannot pass both tests at once.
#[test]
fn console_call_wires_scrub_track_approve_verdict_marks_through_the_public_abi() {
    use rigger_console::console;
    use rigger_domain::spawn::SpawnResult;

    let event = SpawnResult::ok("u1/adjudicator#0", r#"{"verdict":"approve"}"#)
        .to_event()
        .unwrap();
    let data: serde_json::Value = serde_json::from_slice(&event.data).unwrap();
    let wire = serde_json::json!({
        "events": [{ "type": event.type_, "data": data, "position": 1 }]
    })
    .to_string();

    let reset = unsafe { call("fold_reset", &wire) };
    assert!(
        reply_len(reset) > 0,
        "fold_reset must answer a non-empty reply through the exported ABI"
    );
    let queried = unsafe { call("scrub_track", "{}") };

    let mut direct_event = event;
    direct_event.position = 1;
    let direct = console::scrub_track(&[direct_event]);
    assert_eq!(direct.marks.len(), 1, "test setup: {direct:?}");
    assert_eq!(
        direct.marks[0].color, "green",
        "test setup must actually produce an approve mark: {direct:?}"
    );
    let expected_len = serde_json::to_vec(&serde_json::json!({
        "marks": direct.marks,
        "ticks": direct.ticks
    }))
    .unwrap()
    .len();

    assert_eq!(
        reply_len(queried),
        expected_len,
        "scrub_track's approve verdict mark through the exported ABI must be the same length \
         as console::scrub_track's own direct answer for the identical adjudicator result"
    );
}

/// `WireEvent`'s additive `recorded_at` (spec 94 criterion 3, `d-u94c3-wire-event-recorded-at`)
/// reaches `console::scrub_track`'s hour ticks through the REAL exported ABI - not merely
/// through this crate's private `dispatch_tests`
/// (`fold_reset_recorded_at_feeds_scrub_tracks_hour_ticks`), which calls `dispatch` directly
/// and never crosses `console_call`'s own pointer-packing marshaling. Two wire events an hour
/// apart, each carrying `recorded_at`, answer a `scrub_track` reply the same length as
/// `console::scrub_track`'s own direct answer for the SAME two events built with
/// `Event::recorded_at` set natively - proving the wire value actually reaches the `Event`,
/// not merely surviving JSON parsing and then being dropped before the fold sees it.
#[test]
fn console_call_scrub_track_hour_ticks_read_recorded_at_across_the_public_abi() {
    use rigger_console::console;
    use rigger_domain::eventstore::Event;
    use std::time::{Duration, SystemTime};

    let events_json = r#"{"events":[
        {"type":"UnitStarted","data":{"id":"u1"},"position":1,"recorded_at":100},
        {"type":"UnitIntegrated","data":{"id":"u1","commit":"abc"},"position":2,"recorded_at":4000}
    ]}"#;
    let reset = unsafe { call("fold_reset", events_json) };
    assert!(
        reply_len(reset) > 0,
        "fold_reset must answer a non-empty reply through the exported ABI"
    );

    let queried = unsafe { call("scrub_track", "{}") };

    let mut e0 = Event::new("UnitStarted", br#"{"id":"u1"}"#.to_vec());
    e0.position = 1;
    e0.recorded_at = SystemTime::UNIX_EPOCH + Duration::from_secs(100);
    let mut e1 = Event::new("UnitIntegrated", br#"{"id":"u1","commit":"abc"}"#.to_vec());
    e1.position = 2;
    e1.recorded_at = SystemTime::UNIX_EPOCH + Duration::from_secs(4000);
    let direct = console::scrub_track(&[e0, e1]);
    assert_eq!(
        direct.ticks.len(),
        1,
        "test setup must actually cross an hour boundary: {direct:?}"
    );
    let expected_len = serde_json::to_vec(&serde_json::json!({
        "marks": direct.marks,
        "ticks": direct.ticks
    }))
    .unwrap()
    .len();

    assert_eq!(
        reply_len(queried),
        expected_len,
        "scrub_track's hour ticks through the exported ABI must be the same length as \
         console::scrub_track's own direct answer when the wire events carry recorded_at"
    );
}

/// Back-compat half of `d-u94c3-wire-event-recorded-at`: a wire event that OMITS
/// `recorded_at` entirely - every snapshot recorded before this criterion, and every other
/// `fold_reset` call in this file - still answers `scrub_track`'s documented degrade (no
/// ticks, never an error) through the REAL exported ABI, not just `hour_ticks`'s own
/// in-process unit test.
#[test]
fn console_call_scrub_track_omits_ticks_through_the_public_abi_when_the_wire_carries_no_recorded_at(
) {
    let reset = unsafe {
        call(
            "fold_reset",
            r#"{"events":[{"type":"UnitStarted","data":{"id":"u1"},"position":1}]}"#,
        )
    };
    assert!(
        reply_len(reset) > 0,
        "fold_reset must answer a non-empty reply through the exported ABI"
    );

    let queried = unsafe { call("scrub_track", "{}") };
    let expected_len = serde_json::to_vec(&serde_json::json!({ "marks": [], "ticks": [] }))
        .unwrap()
        .len();
    assert_eq!(
        reply_len(queried),
        expected_len,
        "a wire event with no recorded_at must degrade to an empty ticks list through the \
         exported ABI, never an error or a spurious tick"
    );
}

/// `fold_push`'s idempotency-by-position fix
/// (`adj-u94c3-verdict-reject-stream-reconnect-duplicate-events` /
/// `u94c3-fix-stream-reconnect-duplicate-events`) wires through the REAL exported ABI - not
/// merely this crate's own private
/// `dispatch_tests::fold_push_is_idempotent_for_a_position_already_applied`, which calls
/// `dispatch` directly and never crosses `console_call`'s pointer-packing marshaling. A real
/// `EventSource` reconnect replaying an already-delivered frame (exactly what
/// `tests/dash_console_stream_periphery.rs`'s own real-socket reconnect test proves the
/// SERVER half of) must never grow the fold past ONE mark for the ONE real event: pushing the
/// identical wire frame twice through `console_call` answers the SAME reply length the first
/// push did, and the following `scrub_track` answers exactly the single-mark length a lone
/// push would - never a longer reply carrying a second, duplicate mark for the same event.
#[test]
fn console_call_fold_push_is_idempotent_for_a_position_already_applied_through_the_public_abi() {
    use rigger_console::console;
    use rigger_domain::eventstore::Event;

    let reset = unsafe {
        call(
            "fold_reset",
            r#"{"events":[{"type":"UnitStarted","data":{"id":"u1"},"position":1}]}"#,
        )
    };
    assert!(
        reply_len(reset) > 0,
        "fold_reset must answer a non-empty reply through the exported ABI"
    );

    let push = r#"{"type":"UnitIntegrated","data":{"id":"u1","commit":"abc"},"position":2}"#;
    let first = unsafe { call("fold_push", push) };
    assert!(
        reply_len(first) > 0,
        "the first fold_push must answer a non-empty reply through the exported ABI"
    );
    // The SAME frame again - exactly what a reconnect replaying an already-delivered range
    // hands the core, never a value this test invents.
    let second = unsafe { call("fold_push", push) };
    assert_eq!(
        reply_len(second),
        reply_len(first),
        "a duplicate fold_push for a position already applied must answer the SAME fold \
         state through the exported ABI, never a changed one"
    );

    let queried = unsafe { call("scrub_track", "{}") };

    let mut e0 = Event::new("UnitStarted", br#"{"id":"u1"}"#.to_vec());
    e0.position = 1;
    let mut e1 = Event::new("UnitIntegrated", br#"{"id":"u1","commit":"abc"}"#.to_vec());
    e1.position = 2;
    let direct = console::scrub_track(&[e0, e1]);
    assert_eq!(
        direct.marks.len(),
        1,
        "test setup: the one real UnitIntegrated event must produce exactly one mark: {direct:?}"
    );
    let expected_len = serde_json::to_vec(&serde_json::json!({
        "marks": direct.marks,
        "ticks": direct.ticks
    }))
    .unwrap()
    .len();

    assert_eq!(
        reply_len(queried),
        expected_len,
        "scrub_track through the exported ABI must render exactly ONE mark for the ONE real \
         event even after fold_push saw its position twice - a duplicate delivery must never \
         render a second mark for the same real event"
    );
}
