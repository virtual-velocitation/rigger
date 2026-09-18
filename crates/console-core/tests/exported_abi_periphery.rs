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
/// against the length of the exact answer `rigger::contextgraph::query::graph_query` itself
/// returns for the identical graph and kind - an expected value computed from the same
/// authority the ABI wires to, never a hardcoded number.
#[test]
fn console_call_wires_graph_load_and_graph_query_through_the_public_abi() {
    use rigger::contextgraph::query::graph_query;
    use rigger::contextgraph::{Graph, Node, KIND_FILE};

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
    use rigger::console::map;
    use rigger::contextgraph::{
        Edge, Graph, Node, KIND_CODE_ENTITY, REL_IN_COMMUNITY, TIER_EXTRACTED,
    };
    use rigger::eventstore::Position;

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
    let direct = map::frame(&model, 800.0, 600.0, 0.0);
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
    use rigger::contextgraph::{
        Edge, Graph, Node, KIND_CODE_ENTITY, REL_IN_COMMUNITY, TIER_EXTRACTED,
    };
    use rigger::eventstore::Position;

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
    use rigger::console::map;
    use rigger::contextgraph::{
        Edge, Graph, Node, KIND_CODE_ENTITY, REL_IN_COMMUNITY, TIER_EXTRACTED,
    };
    use rigger::eventstore::Position;

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
    let direct = map::frame(&model, 800.0, 600.0, 0.0);
    let expected_len = serde_json::to_vec(&direct).unwrap().len();
    assert_eq!(
        reply_len(fresh_frame),
        expected_len,
        "map_frame after the post-reload map_build must answer real data from the NEW graph \
         through the exported ABI, matching console::map's own frame() output for it"
    );
}
