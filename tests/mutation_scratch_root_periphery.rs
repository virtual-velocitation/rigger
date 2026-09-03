//! Periphery (API contract) test for spec 77 criterion 6's new public surface
//! `driver::replay::mutation_scratch_root`.
//!
//! WHAT THE INSIDE-OUT TESTS ARE STRUCTURALLY BLIND TO.
//!
//! `src/driver/replay.rs`'s own unit test pins `mutation_scratch_root`'s output for one
//! literal cache-home input, in isolation. `src/main.rs`'s FOOTPRINT ACCOUNTING (spec 77
//! criterion 5) silently DEPENDS on a relationship neither side's unit tests can see across
//! the module boundary: `footprint_report_for` measures the "registered scratch roots"
//! category by walking whatever directory `mutation_scratch_root` computes, trusting that
//! it is the exact directory every spawn's real mutation-scratch tree (the one
//! `mutation_scratch_path` actually creates) lives one level under. If the two functions'
//! path-joining ever drifted apart (a future edit to one that forgets the other, e.g. a
//! renamed subdir constant applied on only one side), each function's own unit test would
//! keep passing, each pinning only its OWN literal output, while `rigger validate` would
//! silently under-report, or stop reporting entirely, real mutation-scratch disk usage.
//! This file closes that gap over the crate's PUBLIC surface: it drives both functions
//! together and proves the invariant directly, so a future divergence fails HERE, at the
//! exact seam, rather than only downstream in the footprint-accounting integration (or
//! never, if no test ever seeds real bytes there).

use std::path::Path;

use rigger::driver::replay::{mutation_scratch_path, mutation_scratch_root};

#[test]
fn mutation_scratch_root_is_always_the_immediate_parent_of_every_mutation_scratch_path() {
    let cache_home = Path::new("/home/u/.cache");
    let root = mutation_scratch_root(cache_home);

    // Representative spawn ids covering every role/attempt/unit shape the crate's own
    // `mutation_scratch_path` tests already exercise (plain, dashed unit id, multi-digit
    // attempt), so this invariant is proven across the same input space, not just one case.
    for spawn_id in [
        "u77c2/implementer#0",
        "u1/implementer#4",
        "unit-with-dashes/adversary#12",
        "u/weird#id",
    ] {
        let leaf = mutation_scratch_path(cache_home, spawn_id).unwrap_or_else(|| {
            panic!("[{spawn_id}] a well-formed spawn id must never encode to None")
        });
        assert_eq!(
            leaf.parent(),
            Some(root.as_path()),
            "[{spawn_id}] footprint accounting's \"registered scratch roots\" category walks \
             mutation_scratch_root expecting every mutation_scratch_path leaf to live \
             directly under it - a mismatch here means rigger validate would silently stop \
             measuring this spawn's real mutation-scratch tree"
        );
    }
}

#[test]
fn mutation_scratch_root_is_the_bare_root_not_a_reencoded_spawn_leaf() {
    // mutation_scratch_root takes no spawn id at all, so it has none of
    // mutation_scratch_path's own degenerate cases (an empty spawn id encodes to None,
    // per mutation_scratch_path's doc comment) - proving the two are genuinely distinct
    // functions with distinct contracts, not the same join under two names.
    let cache_home = Path::new("/home/u/.cache");
    assert_eq!(
        mutation_scratch_root(cache_home),
        cache_home.join("rigger-mutants"),
        "the bare registered root, with no spawn leaf joined"
    );
    assert_eq!(
        mutation_scratch_path(cache_home, ""),
        None,
        "unlike mutation_scratch_root, mutation_scratch_path(_, \"\") stays None - the one \
         degenerate shape only the leaf-joining function has"
    );
}
