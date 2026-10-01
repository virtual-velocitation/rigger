//! PERIPHERY (API) test for the KurrentDB adapter's footprint in the process that opens it: a
//! `Store` holds one runtime thread, never one per core.
//!
//! Every thread a process holds costs a stack and an allocator arena of address space, and the
//! test runner caps each test process's address space, so a store whose runtime ran tokio's
//! default of one worker per core would outgrow that cap on a many-core host before a single
//! event was appended. The adapter's own test pins the runtime it builds; this one drives the
//! public `Store::open` against a real server and counts the threads the process gains.
//!
//! It is the only test in its binary because a thread count is exact only while no other test
//! runs in the same process. Without a reachable container runtime it skips with its fixture.

mod common;

use common::fixtures::{wait_until_for, with_kurrentdb};
use rigger::eventstore::kurrentdb::Store;
use std::collections::BTreeSet;

/// The ids of every thread this process holds right now.
fn thread_ids() -> BTreeSet<u64> {
    std::fs::read_dir("/proc/self/task")
        .expect("this process's thread list")
        .map(|task| {
            task.expect("a thread entry")
                .file_name()
                .to_str()
                .and_then(|id| id.parse().ok())
                .expect("a numeric thread id")
        })
        .collect()
}

/// Opening a store adds exactly one thread to the process - its runtime's one worker, on a host
/// of any core count - and dropping it takes that thread back. The connection string names the
/// server by address, so the client resolves no name and starts no resolver thread of its own.
///
/// The drop side polls instead of reading once. Dropping the store joins its worker, and that
/// join returns when the exiting thread wakes its joiner, a moment before the kernel removes the
/// thread from `/proc/self/task`, so a correct store's worker can still be listed right after the
/// drop. The poll gives it a second to leave; a worker that never leaves is still listed then.
#[test]
fn an_open_kurrentdb_store_adds_one_thread_and_its_drop_takes_it_back() {
    with_kurrentdb(|conn| {
        let by_address = conn.replacen("//localhost:", "//127.0.0.1:", 1);
        assert_ne!(
            by_address, conn,
            "the fixture names its server as localhost"
        );
        let before = thread_ids();
        let store = Store::open(&by_address).expect("open the store");
        let opened = thread_ids();
        assert_eq!(
            opened.difference(&before).count(),
            1,
            "threads added by one open store on a host of {:?} cores",
            std::thread::available_parallelism()
        );
        drop(store);
        let mut left = BTreeSet::new();
        // 40 reads 25 ms apart: a one-second deadline.
        wait_until_for(40, || {
            left = thread_ids().difference(&before).copied().collect();
            left.is_empty()
        });
        assert_eq!(
            left,
            BTreeSet::new(),
            "threads the dropped store added, still listed a second after the drop"
        );
    });
}
