//! A small, dependency-free parallel primitive: [`map_ordered`] fans a pure per-item map across a
//! `std::thread::scope` worker pool yet returns the results in the SAME order as the input, so a
//! caller can parse in parallel while emitting exactly the sequence a serial walk would have
//! produced. Parallelism becomes observationally invisible (the rebuild-byte-identical discipline):
//! only timing differs, never the output order.
//!
//! This is the ONE parallel-map authority the ingest walk builds on; there is no second, forked
//! parallel implementation to reconcile.

use std::collections::HashSet;
use std::thread::ThreadId;

/// Apply `f` to every element of `items` across up to `workers` threads, returning the results in
/// the SAME order as `items` together with the number of DISTINCT worker threads that actually ran
/// `f` (the engagement count).
///
/// Order preservation is by construction: the items are split into `min(workers, len)` contiguous,
/// non-empty ranges, each range is mapped on its own thread, and the per-range outputs are
/// concatenated in range order - so the result is identical to `items.iter().map(f).collect()`
/// regardless of which thread finished first. Parallelism is thus observationally invisible: only
/// timing differs from a serial map.
///
/// Engagement is honest and deterministic: because every range is non-empty, every spawned thread
/// runs `f` at least once, so the count of distinct [`ThreadId`]s that executed `f` equals the pool
/// size `min(workers, len)`. `workers <= 1` (or a workload of 0 or 1 items) short-circuits to an
/// inline serial map on the CURRENT thread - no pool thread is added, and the engagement count is 1
/// when there is work and 0 when there is none. That inline path is the serial walk a parallel walk
/// is compared against, so there is no second serial implementation to drift from.
pub fn map_ordered<T, U, F>(items: &[T], workers: usize, f: F) -> (Vec<U>, usize)
where
    T: Sync,
    U: Send,
    F: Fn(&T) -> U + Sync,
{
    let n = items.len();
    if workers <= 1 || n <= 1 {
        // Serial: run f inline on the current thread. This IS the serial oracle a parallel walk is
        // proven byte-identical against - the same code path, not a hand-rolled twin.
        let out: Vec<U> = items.iter().map(&f).collect();
        return (out, usize::from(n > 0));
    }
    let pool = workers.min(n);
    let f = &f;
    let per_chunk: Vec<(ThreadId, Vec<U>)> = std::thread::scope(|scope| {
        let handles: Vec<_> = chunk_bounds(n, pool)
            .into_iter()
            .map(|(start, end)| {
                scope.spawn(move || {
                    let id = std::thread::current().id();
                    let out: Vec<U> = items[start..end].iter().map(f).collect();
                    (id, out)
                })
            })
            .collect();
        handles.into_iter().map(|h| h.join().unwrap()).collect()
    });
    // Concatenate the chunk outputs in chunk order (index-preserving) and count the distinct threads
    // that produced a non-empty chunk - the honest measure of how many workers actually ran f.
    let mut ids: HashSet<ThreadId> = HashSet::new();
    let mut out: Vec<U> = Vec::with_capacity(n);
    for (id, chunk) in per_chunk {
        if !chunk.is_empty() {
            ids.insert(id);
        }
        out.extend(chunk);
    }
    (out, ids.len())
}

/// Partition `n` items into `pool` contiguous, as-even-as-possible, NON-EMPTY `[start, end)` ranges,
/// in order (requires `1 <= pool <= n`). The first `n % pool` ranges take one extra item. Because
/// every range is non-empty, concatenating per-range outputs reproduces the original order and every
/// worker does real work.
fn chunk_bounds(n: usize, pool: usize) -> Vec<(usize, usize)> {
    let base = n / pool;
    let extra = n % pool;
    let mut bounds = Vec::with_capacity(pool);
    let mut start = 0;
    for c in 0..pool {
        let len = base + usize::from(c < extra);
        bounds.push((start, start + len));
        start += len;
    }
    bounds
}

/// The default worker width for a [`map_ordered`] walk: one worker per logical core, falling back to
/// a single worker when the machine's parallelism cannot be queried. This is the ONE place the
/// default width is decided, so every walk that does not name a width agrees on it. The width never
/// changes a walk's OUTPUT (the map is index-preserving) - only how fast it is produced.
pub fn default_workers() -> usize {
    std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1)
}

#[cfg(test)]
mod tests {
    use super::{chunk_bounds, default_workers, map_ordered};
    use std::sync::{Arc, Barrier};

    #[test]
    fn default_workers_is_at_least_one() {
        // A walk that does not name a width must always get a usable pool - at least one worker even
        // when the machine's parallelism cannot be queried, so the walk never degrades to zero work.
        assert!(default_workers() >= 1, "the default pool is never empty");
    }

    #[test]
    fn chunk_bounds_partitions_into_non_empty_ordered_covering_ranges() {
        // The determinism guarantee rests on this: every range is non-empty (so every worker runs f
        // and engagement is exactly the pool size), the ranges are contiguous and cover 0..n in
        // order (so concatenation reproduces the input order), and the sizes differ by at most one.
        for (n, pool) in [(8, 3), (8, 8), (7, 2), (10, 4), (1, 1), (5, 5)] {
            let bounds = chunk_bounds(n, pool);
            assert_eq!(
                bounds.len(),
                pool,
                "one range per worker (n={n}, pool={pool})"
            );
            assert_eq!(bounds[0].0, 0, "the first range starts at 0");
            assert_eq!(bounds[pool - 1].1, n, "the last range ends at n");
            let mut prev_end = 0;
            let mut sizes = Vec::new();
            for (start, end) in &bounds {
                assert_eq!(
                    *start, prev_end,
                    "ranges are contiguous with no gap or overlap"
                );
                assert!(end > start, "every range is non-empty (n={n}, pool={pool})");
                sizes.push(end - start);
                prev_end = *end;
            }
            let (min, max) = (sizes.iter().min().unwrap(), sizes.iter().max().unwrap());
            assert!(
                max - min <= 1,
                "range sizes differ by at most one; got {sizes:?}"
            );
        }
    }

    #[test]
    fn map_ordered_preserves_input_order_regardless_of_completion() {
        // The core contract: results come back in INPUT order, never completion order - the property
        // the byte-identical emit relies on. Square 100 items across 8 workers and the output must be
        // 0,1,4,9,... in index order however the pool interleaved.
        let items: Vec<usize> = (0..100).collect();
        let (out, engaged) = map_ordered(&items, 8, |&x| x * x);
        let expected: Vec<usize> = (0..100).map(|x| x * x).collect();
        assert_eq!(
            out, expected,
            "results are index-preserving, not completion-ordered"
        );
        assert!(
            engaged > 1,
            "a 100-item workload over 8 workers engages more than one thread; got {engaged}"
        );
    }

    #[test]
    fn map_ordered_engages_every_worker_deterministically() {
        // Engagement is deterministic BY CONSTRUCTION: contiguous non-empty chunks give each of the
        // pool = min(workers, n) threads real work. A barrier makes the proof airtight - f cannot
        // return until all eight threads have arrived, so if fewer than eight threads ran the barrier
        // would deadlock. Reaching the assertion at all proves eight distinct threads executed f.
        let items: Vec<usize> = (0..8).collect();
        let barrier = Arc::new(Barrier::new(8));
        let (out, engaged) = map_ordered(&items, 8, |&x| {
            barrier.wait();
            x
        });
        assert_eq!(out, items, "order is preserved through the barrier");
        assert_eq!(
            engaged, 8,
            "all eight workers engaged (the barrier would deadlock otherwise)"
        );
    }

    #[test]
    fn map_ordered_is_the_serial_walk_at_one_worker() {
        // Width 1 runs f inline on the current thread - no pool thread added. This IS the serial
        // oracle the ingest determinism proof compares the parallel walk against, so it must report
        // exactly one engaged worker and, of course, preserve order.
        let items: Vec<usize> = (0..5).collect();
        let (out, engaged) = map_ordered(&items, 1, |&x| x + 1);
        assert_eq!(out, vec![1, 2, 3, 4, 5], "serial map preserves order");
        assert_eq!(
            engaged, 1,
            "a single worker runs inline: one engaged thread"
        );
    }

    #[test]
    fn map_ordered_caps_the_pool_at_the_item_count() {
        // Two items over eight requested workers can engage at most two threads - the pool is capped
        // at the item count so no thread is spawned with nothing to do (which would misreport
        // engagement and waste a thread).
        let items: Vec<usize> = vec![10, 20];
        let (out, engaged) = map_ordered(&items, 8, |&x| x);
        assert_eq!(out, vec![10, 20]);
        assert_eq!(engaged, 2, "the pool is capped at the two available items");
    }

    #[test]
    fn map_ordered_over_nothing_engages_no_worker() {
        // An empty input does no work, so no thread is engaged - the count is zero, not one.
        let items: Vec<usize> = vec![];
        let (out, engaged) = map_ordered(&items, 8, |&x: &usize| x);
        assert!(out.is_empty());
        assert_eq!(engaged, 0, "no work engages no worker");
    }
}
