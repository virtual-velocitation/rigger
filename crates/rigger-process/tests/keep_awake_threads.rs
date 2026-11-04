//! Spec 105's Linux keep-awake adapter starts no thread that outlives an acquire: the process
//! holds the same threads once the host's acquire has answered as it held before the call.
//!
//! It is the only test in its binary because a thread count is exact only while no other test
//! runs in the same process.

#![cfg(all(target_os = "linux", any(feature = "store", not(feature = "core"))))]

extern crate rigger_process as rigger;

#[allow(dead_code, unused_imports)]
#[path = "../../../tests/common/fixtures/host.rs"]
mod host_fixtures;

use host_fixtures::thread_ids;
use rigger_domain::sleep_hold::SleepHold;
use rigger_process::keep_awake::OsSleepHold;
use rigger_process::reap::stat_field_after_comm;
use std::collections::BTreeSet;

/// The kernel's flag for a thread that has begun to exit (`PF_EXITING`, the `flags` field of
/// `proc(5)`'s `stat`). It is set before the exiting thread wakes its joiner, and the thread is
/// still listed for a moment after that join returns.
const PF_EXITING: u64 = 0x4;

/// The ids of the threads this process holds that have not begun to exit. A thread whose
/// `stat` no longer reads has left; one carrying [`PF_EXITING`] is leaving, so a thread joined
/// a moment ago is never counted, with no wait for the kernel to unlist it.
fn living_threads() -> BTreeSet<u32> {
    thread_ids()
        .into_iter()
        .filter(|&id| {
            stat_field_after_comm(id, 6)
                .and_then(|flags| flags.parse::<u64>().ok())
                .is_some_and(|flags| flags & PF_EXITING == 0)
        })
        .collect()
}

/// The host's acquire, granted or not, leaves the process holding exactly the threads it held
/// before the call, its guard still standing: the adapter runs its connection on a runtime the
/// calling thread drives and joins every thread that runtime started before it returns.
#[test]
fn an_acquire_leaves_the_process_holding_the_threads_it_held_before() {
    let before = living_threads();
    let held = OsSleepHold.acquire("the rigger-process keep-awake thread test");
    let after = living_threads();
    assert_eq!(
        after,
        before,
        "the threads this process holds once the host's acquire answered {:?}",
        held.as_ref().map(|_| "granted")
    );
    drop(held);
}
