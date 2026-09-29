//! Periphery (integration) test for spec 84 criterion 1's own Done-when: "THE MAP LANDS
//! LABELLED". Design's CONSTRAINTS WALK is explicit that this claim is proven against a REAL
//! graph, never a fixture: "Real store - every rendering criterion is proven against this
//! repository's actual graph, not a fixture: a fixture cannot reproduce the file-co-location
//! degeneration this spec exists to fix."
//!
//! "This repository's real store" here means this project's OWN real source tree
//! (`env!("CARGO_MANIFEST_DIR")/src`), ingested through the SAME production entry points a cold
//! `rigger graph build` + `rigger graph communities` use
//! (`grounder::symbols::events::project_batches`, `community::Coupling::from_graph`/`detect`/
//! `events`) - never a hand-typed fixture graph, and never dependent on a specific operator's own
//! `.rigger/graph.db` (which a fresh checkout, a CI runner, or this very unit's own isolated
//! worktree does not carry - there is no prior run history to read here). This reproduces spec
//! 84's own Goal exactly: communities that form by REAL file co-location, at real repository
//! scale - the thing a two-node hand fixture cannot exhibit.
//!
//! Gated on `symbols` (the tree-sitter extraction pass `project_batches` needs), mirroring
//! `live_project_ingestion.rs`'s own per-test gate - the light (`--no-default-features`) lane
//! compiles this file to nothing rather than fail to find the extraction module.

// Every use of these lives behind `#[cfg(feature = "symbols")]` below (the extraction pass this
// whole file exists to drive against), so the imports themselves are gated too - otherwise the
// light (`--no-default-features`) lane would compile this file down to nothing BUT these
// top-level imports and flag every one of them unused.
#[cfg(feature = "symbols")]
use rigger::community;
#[cfg(feature = "symbols")]
use rigger::console::map;
#[cfg(feature = "symbols")]
use rigger::contextgraph::sqlite::Projector;
#[cfg(feature = "symbols")]
use rigger::contextgraph::Graph;

/// Ingest this repository's own `src/` tree into a fresh in-memory projection, run the real
/// community-detection pass over its real coupling layer, and return the resulting live graph -
/// the "real store" this criterion's rendering claim is proven against.
#[cfg(feature = "symbols")]
fn real_graph() -> Graph {
    use rigger::grounder::symbols::events::project_batches;

    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/src");
    let batches = project_batches(root);
    assert!(
        !batches.is_empty(),
        "this repository's own src/ tree must yield extraction batches - a barren tree would \
         silently make this test's 'real store' claim vacuous"
    );

    let projector = Projector::open(":memory:", "code-lens-map-real-store-test")
        .expect("open an in-memory projection");
    // `project_batches` mints raw events via `Event::new` (position 0 - the STORE's own job to
    // stamp on append, per that constructor's own doc); the projector's idempotency guard keys on
    // `position` being globally unique (`INSERT OR IGNORE INTO applied (position)`), so folding
    // straight off `Event::new`'s default would apply only the very first position-0 event and
    // silently `IGNORE` every other one. Stamp a real, strictly-increasing position onto every
    // event before folding - exactly what a real store append does - so this in-memory projection
    // folds its whole real batch, not a near-empty one.
    let mut next_position: u64 = 1;
    for (_, events) in &batches {
        let mut stamped = events.clone();
        for e in &mut stamped {
            e.position = next_position;
            next_position += 1;
        }
        assert_eq!(
            rigger::contextgraph::Fold::of_batch(Some(&projector), &stamped),
            rigger::contextgraph::Fold::Folded,
            "fold this repository's own real extraction batch"
        );
    }

    let whole = projector.whole().expect("read the whole live projection");
    let coupling = community::Coupling::from_graph(&whole);
    assert!(
        !coupling.is_empty(),
        "this repository's own coupling layer must be non-empty - the community-detection pass \
         this criterion's districts read needs real coupling edges to group"
    );
    let assignment = community::detect(&coupling, community::DEFAULT_RESOLUTION);
    let mut community_events = community::events(&assignment);
    for e in &mut community_events {
        e.position = next_position;
        next_position += 1;
    }
    assert_eq!(
        rigger::contextgraph::Fold::of_batch(Some(&projector), &community_events),
        rigger::contextgraph::Fold::Folded,
        "fold the real community-detection pass's own events"
    );

    projector
        .whole()
        .expect("read the community-derived live projection")
}

/// THE MAP LANDS LABELLED (spec 84 criterion 1's own Done-when, verbatim): "opening the code
/// lens against this repository's real store renders the map at full extent in which every
/// visible node is a code entity with a placed label and kind dot, every district carries its
/// purpose label, and no node or district is labelled with a file name - and zooming in strictly
/// increases the labelled-entity count without ever drawing an unlabelled node."
#[cfg(feature = "symbols")]
#[test]
fn the_map_lands_labelled_against_this_repositorys_real_store() {
    let graph = real_graph();
    let model = map::build(&graph);

    assert!(
        !model.districts.is_empty(),
        "this repository's real store must yield at least one district"
    );
    assert!(
        !model.entities.is_empty(),
        "this repository's real store must yield at least one map entity"
    );

    let full_extent = map::frame(&model, 1400.0, 900.0, &map::Camera::default(), None);

    // "every district carries its purpose label" - present at every zoom, never blank, never a
    // file name.
    assert_eq!(
        full_extent.districts.len(),
        model.districts.len(),
        "every district's pill must be present at the full-extent frame"
    );
    for d in &full_extent.districts {
        assert!(
            !d.purpose.is_empty(),
            "a district's purpose label must never be blank"
        );
        assert!(
            !d.purpose.ends_with(".rs") && !d.purpose.contains('/'),
            "district {:?} is labelled with what looks like a file name",
            d.purpose
        );
    }

    // "every visible node is a code entity with a placed label and kind dot ... and no node ...
    // is labelled with a file name".
    assert!(
        !full_extent.entities.is_empty(),
        "the full-extent frame over the real store must draw at least one entity"
    );
    for e in &full_extent.entities {
        assert!(!e.name.is_empty(), "entity {:?} has no placed label", e.id);
        assert!(!e.kind.is_empty(), "entity {:?} has no kind dot", e.id);
        assert!(
            !e.name.ends_with(".rs") && !e.name.contains('/'),
            "entity {:?} is labelled with what looks like a file name: {:?}",
            e.id,
            e.name
        );
    }

    // "zooming in strictly increases the labelled-entity count without ever drawing an
    // unlabelled node" - reasserted directly against the real graph, not just the synthetic
    // fixtures `console::map`'s own unit tests use.
    let zoomed_in = map::frame(
        &model,
        1400.0,
        900.0,
        &map::Camera {
            zoom: 6.0,
            ..Default::default()
        },
        None,
    );
    assert!(
        zoomed_in.entities.len() > full_extent.entities.len(),
        "zooming in against the real store must strictly increase the labelled-entity count: \
         full extent {} vs zoomed in {}",
        full_extent.entities.len(),
        zoomed_in.entities.len()
    );
    for e in &zoomed_in.entities {
        assert!(
            !e.name.is_empty(),
            "a drawn entity must always carry a placed label"
        );
        assert!(
            !e.name.ends_with(".rs") && !e.name.contains('/'),
            "entity {:?} is labelled with what looks like a file name: {:?}",
            e.id,
            e.name
        );
    }
}

/// THE BUDGET ITSELF (BUDGETS, `docs/architecture-addendum-mission-control.md:250`): "A frame
/// of the map at full extent stays under 8 ms of core time." Timed against this repository's
/// own real store (the same graph `the_map_lands_labelled_against_this_repositorys_real_store`
/// proves correctness against), full extent (`zoom: 0.0`), the same scale spec 84 c1's own
/// Done-when claim renders at. Release mode only (`cfg(not(debug_assertions))` is rustc's own
/// release-mode signal, matching `console-core`'s own `fold_reset`/`fold_push` budget tests at
/// `crates/console-core/src/lib.rs` - a debug build's unoptimized layout pass is legitimately
/// much slower and asserting the bound there would be a flaky, meaningless test). Only
/// `map::frame` itself is timed - `real_graph()`'s ingest and `map::build`'s district/layout
/// pass are a one-time-per-session cost, not part of the per-frame budget the Design text
/// scopes this bound to.
#[cfg(feature = "symbols")]
#[test]
#[cfg(not(debug_assertions))]
fn frame_at_full_extent_completes_under_8ms_of_core_time_against_the_real_store() {
    let graph = real_graph();
    let model = map::build(&graph);

    let start = std::time::Instant::now();
    let full_extent = map::frame(&model, 1400.0, 900.0, &map::Camera::default(), None);
    let elapsed = start.elapsed();

    assert!(
        !full_extent.entities.is_empty(),
        "a vacuous (empty) frame would make the timing assertion below meaningless"
    );
    assert!(
        elapsed.as_millis() < 8,
        "map::frame at full extent took {elapsed:?}, over the 8ms budget"
    );
}

/// Design's LEGEND paragraph names exactly four entity-dot kind colours plus one neutral default
/// for everything else (`KIND_COLOUR_DEFAULT`, [`map::kind_colour`]'s own doc). The existing
/// `kind_colour_paints_every_real_draw_entity_kind_a_built_graph_produces`
/// (`tests/console_map_kind_colour_periphery.rs`) already drives this through a real
/// `build`/`frame` pipeline, but over a small HAND-BUILT six-kind fixture - its two out-of-palette
/// kinds (`method`, `module`) are the test author's own educated guess at what a real extractor
/// emits, never verified against the extractor itself. This repository's own real store is the
/// one place that guess is actually checked: every kind THIS repository's own extraction pass
/// (`crates/rigger-grounder/src/grounder/symbols/extract.rs::kind_of` / `events.rs::kind_str`) really assigns to a drawn
/// entity must resolve through `kind_colour` without panicking, named or default - proving the
/// documented four-plus-default palette is exhaustive against the real extractor's real output,
/// not just the fixture author's guess of what that output would be. (`legend`'s own wire colour
/// values against `kind_colour`/`BLAST_RADIUS_COLOUR` are already independently proven, and
/// exhaustively so across every row including the null-colour ones, by
/// `legend_json_colour_values_match_their_independent_public_authorities` in that same file -
/// `legend` itself takes no graph, so a real-store variant of that check would add nothing.)
#[cfg(feature = "symbols")]
#[test]
fn kind_colour_covers_every_kind_this_repositorys_real_store_actually_draws() {
    let graph = real_graph();
    let model = map::build(&graph);
    let full_extent = map::frame(&model, 1400.0, 900.0, &map::Camera::default(), None);

    let named_kinds: std::collections::BTreeSet<&str> =
        map::KIND_COLOURS.iter().map(|(k, _, _)| *k).collect();
    let mut saw_named_kind = false;
    let mut saw_default_kind = false;
    for e in &full_extent.entities {
        let colour = map::kind_colour(&e.kind);
        if named_kinds.contains(e.kind.as_str()) {
            saw_named_kind = true;
        } else {
            saw_default_kind = true;
            assert_eq!(
                colour,
                map::KIND_COLOUR_DEFAULT,
                "real entity {:?} has kind {:?}, outside KIND_COLOURS - it must paint with the \
                 documented neutral default, never a silently-absent lookup",
                e.id,
                e.kind
            );
        }
    }
    assert!(
        saw_named_kind,
        "this repository's real store must draw at least one entity of a named KIND_COLOURS \
         kind (e.g. a function) - otherwise this test cannot prove the named-colour branch at all"
    );
    assert!(
        saw_default_kind,
        "this repository's real store must draw at least one entity OUTSIDE the four named \
         kinds (e.g. a method or a module-level item) - otherwise this test cannot prove the \
         default-colour branch at all, the exact gap a hand-picked fixture would hide"
    );
}
