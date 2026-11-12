//! Periphery (integration) tests for spec 49 criterion 1: the parallel map behind every ordered
//! fan-out. These run OUTSIDE the crate, over the library's PUBLIC surface (`rigger::...`), so they
//! guard a boundary the inside-out unit tests are structurally blind to: the parallel primitive is
//! actually EXPORTED and behaves when driven as an external consumer would drive it (an item that
//! were accidentally `pub(crate)` would still pass every in-crate test yet be unreachable here).
//! The `parallel` module is compiled unconditionally, so this runs in both feature lanes.

use std::sync::Mutex;

/// The parallel primitive is a PUBLIC authority (`rigger::parallel::map_ordered`), so an external
/// caller must be able to fan a pure map across a real worker pool and get the results back in INPUT
/// order - never completion order - with an honest engagement count. The inside-out unit tests prove
/// this through `super::map_ordered`; this pins the same contract at the crate EDGE, and adds a
/// property they do not state outright: over a concurrent run every item is visited exactly once (no
/// drop, no duplicate under interleaving). Ungated: it runs in both feature lanes.
#[test]
fn map_ordered_public_boundary_preserves_order_visits_once_and_fans_out() {
    // A width passed EXPLICITLY spawns a real pool of `min(workers, len)` threads regardless of the
    // host's core count, so `engaged > 1` here is a by-construction guarantee, not a timing gamble.
    let items: Vec<usize> = (0..256).collect();
    let seen: Mutex<Vec<usize>> = Mutex::new(Vec::new());

    let (out, engaged) = rigger::parallel::map_ordered(&items, 8, |&x| {
        // A `Fn` closure sharing a Mutex records that this input was visited, proving the concurrent
        // map neither drops nor duplicates work.
        seen.lock().unwrap().push(x);
        x * x
    });

    let expected: Vec<usize> = items.iter().map(|&x| x * x).collect();
    assert_eq!(
        out, expected,
        "results come back in INPUT order (index-preserving), never completion order"
    );
    assert!(
        engaged > 1,
        "256 items over an explicit width of 8 fans across more than one worker; got {engaged}"
    );

    let mut visited = seen.into_inner().unwrap();
    visited.sort_unstable();
    assert_eq!(
        visited, items,
        "every input is visited exactly once under the concurrent map - no drop, no duplicate"
    );

    // The default width authority a width-less walk falls back to is always a usable pool.
    assert!(
        rigger::parallel::default_workers() >= 1,
        "the default parse width is never zero, even when parallelism cannot be queried"
    );
}
