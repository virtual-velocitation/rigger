//! Periphery tests for spec 78's TEST HELPER (`tests/common/mod.rs::terminate_pid`/`is_alive`),
//! the ONE sanctioned test-side signal call, and the guard behavior every other suite's
//! converted call sites (formerly `tests/cli.rs::reap_pid`, formerly a shelled-out probe/
//! termination pair in `tests/reset_build_cache_periphery.rs`) now depend on.
//!
//! `common::mod.rs` is compiled into every suite that declares `mod common;` but has no
//! `#[cfg(test)]` module of its own, so no suite exercises its CONTRACT directly today - this
//! file is that contract's home: the pid-0/pid-1/own-pid refusals, and that the happy path
//! actually reaches and ends a real process this test holds no `Child` handle to (exactly the
//! shape every converted call site uses it in).

mod common;

use std::process::{Child, Command};
use std::time::Duration;

/// Spawn a real, long-lived `sleep` - the test signals it ONLY by its bare pid through
/// [`common::terminate_pid`], never through this returned `Child`'s own `kill`, mirroring how
/// every converted call site (a detached dash read back from a marker file, an orphaned build
/// read back from a pidfile) uses the helper: it has no handle, only a number.
fn spawn_sleeper() -> Child {
    Command::new("sleep")
        .arg("300")
        .spawn()
        .expect("spawn a sleeper fixture")
}

/// Poll `pred` until it holds or a generous timeout elapses; returns whether it held.
fn wait_until(mut pred: impl FnMut() -> bool) -> bool {
    for _ in 0..200 {
        if pred() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    false
}

#[test]
fn terminate_pid_ends_a_real_process_it_holds_no_child_handle_to() {
    let mut child = spawn_sleeper();
    let pid = child.id();
    assert!(
        common::is_alive(pid),
        "the freshly spawned fixture must be alive before termination"
    );

    common::terminate_pid(pid);

    let died = wait_until(|| matches!(child.try_wait(), Ok(Some(_))));
    // Reap regardless of the assertion outcome, so a failure never leaks the fixture.
    let _ = child.wait();
    assert!(
        died,
        "terminate_pid must actually end pid {pid} within the timeout"
    );
    assert!(
        !common::is_alive(pid),
        "the pid must read as dead once terminate_pid has ended it"
    );
}

#[test]
fn terminate_pid_on_an_already_exited_pid_is_a_silent_no_op() {
    // "ignores ESRCH" (spec 78, THE TEST HELPER): calling it a SECOND time, after the target
    // has already exited, must never panic - the target being gone is exactly the state a
    // "make sure this is dead" caller wants.
    let mut child = spawn_sleeper();
    let pid = child.id();
    common::terminate_pid(pid);
    assert!(wait_until(|| matches!(child.try_wait(), Ok(Some(_)))));
    let _ = child.wait();

    common::terminate_pid(pid); // must not panic
}

#[test]
fn is_alive_is_false_for_a_pid_that_was_never_a_real_process() {
    // Mirrors `send_signal`'s own liveness twin in `src/reap.rs`: a bogus pid answers
    // `false`, never errors or panics.
    assert!(!common::is_alive(u32::MAX));
}

#[test]
#[should_panic(expected = "not a real process")]
fn terminate_pid_refuses_pid_zero() {
    // Pid 0 can never coincide with this (or any) process's own pid, so this message is
    // deterministic regardless of whether the namespace runner is in effect.
    common::terminate_pid(0);
}

#[test]
#[should_panic]
fn terminate_pid_refuses_pid_one() {
    // Always panics, but WHICH message fires is environment-dependent: under
    // `.cargo/pidns-runner.sh` (spec 78, THE NAMESPACE RUNNER) this test binary itself runs
    // as pid 1 of its own namespace, so pid 1 IS this process's own pid there (the "own pid"
    // message); outside the namespace (`RIGGER_PIDNS=off`) it is refused as "not a real
    // process" instead. `terminate_pid_refuses_its_callers_own_pid` below pins the "own pid"
    // message deterministically via `std::process::id()`, and
    // `terminate_pid_refuses_pid_zero` above pins "not a real process" deterministically via
    // a pid that can never be anyone's own - between the two, both messages are proven.
    common::terminate_pid(1);
}

#[test]
#[should_panic(expected = "own pid")]
fn terminate_pid_refuses_its_callers_own_pid() {
    common::terminate_pid(std::process::id());
}
