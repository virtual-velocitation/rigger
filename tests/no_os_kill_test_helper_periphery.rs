//! Periphery tests for spec 78's TEST HELPER (`tests/common/mod.rs::terminate_pid`/`is_alive`),
//! the ONE sanctioned test-side signal call, and the guard behavior every other suite's
//! converted call sites (formerly `tests/cli.rs::reap_pid`, formerly a shelled-out probe/
//! termination pair in `tests/reset_build_cache_periphery.rs`) now depend on.
//!
//! `common::mod.rs` is compiled into every suite that declares `mod common;` but has no
//! `#[cfg(test)]` module of its own, so no suite exercises its CONTRACT directly today - this
//! file is that contract's home: the pid-0/pid-1/own-pid refusals, that the happy path actually
//! reaches and ends a real process this test holds no `Child` handle to (exactly the shape every
//! converted call site uses it in), and - closing a gap a `sleep`-only fixture cannot see - that
//! termination is genuinely SIGKILL rather than a SIGTERM that merely happens to also end a
//! non-trapping target, mirroring the identical proof `src/reap.rs`'s own periphery suites
//! (`tests/mutation_scratch_reap_base_guard_periphery.rs` and its two siblings) already run for
//! the production reaper's SIGKILL escalation stage.

mod common;

use std::path::Path;
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

/// Spawn a long-lived process that IGNORES SIGTERM, so only SIGKILL (uncatchable, unblockable)
/// can end it - a target `sleep` cannot discriminate a correct SIGKILL from a wrong-but-still-
/// fatal SIGTERM. Mirrors the fixture already established in
/// `tests/mutation_scratch_reap_base_guard_periphery.rs` and its two siblings, for the same
/// reason: proving the SIGKILL-specific promise, not merely that "some signal" ended the target -
/// but touches `ready_marker` only AFTER the trap is installed, so the caller can wait for that
/// mark rather than race the shell's own startup: a signal sent before the trap statement has
/// run kills the process via SIGTERM's DEFAULT disposition, which would look identical to a
/// true SIGTERM-vs-SIGKILL failure without actually proving one (caught empirically: a
/// same-instant signal after spawn killed this exact fixture in ~25ms via signal 15, before the
/// unmarked version of this fixture ever reached its own `trap` statement).
fn spawn_sigterm_ignorer(ready_marker: &Path) -> Child {
    Command::new("sh")
        .arg("-c")
        .arg(format!(
            "trap '' TERM; touch {}; while :; do sleep 1; done",
            shell_quote(ready_marker)
        ))
        .spawn()
        .expect("spawn a SIGTERM-ignoring fixture process")
}

/// Single-quote `path` for safe interpolation into the `sh -c` argument above (tempdir paths
/// contain no single quotes, but escaping properly rather than assuming that keeps this
/// fixture correct even if the temp-path format ever changes).
fn shell_quote(path: &Path) -> String {
    format!("'{}'", path.display().to_string().replace('\'', r"'\''"))
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

/// Kill-and-wait a fixture child unconditionally, ignoring errors - through the `Child` handle
/// this file spawned it with, never a computed pid. Deliberately NOT a bare `child.wait()`: a
/// test that already confirmed `terminate_pid` ended the target can still reach this after a
/// genuine regression where it did NOT, and a bare `wait()` on a still-living child blocks
/// FOREVER (empirically hit while proving `terminate_pid_uses_sigkill_not_sigterm` fails for
/// the right reason: a still-alive fixture hung the whole test binary past any bounded
/// timeout). Calling `kill()` first guarantees `wait()` afterward returns promptly regardless
/// of whether the code under test worked, so a real regression fails FAST with a clear
/// assertion message instead of hanging the suite. Mirrors the identical helper in
/// `tests/mutation_scratch_reap_base_guard_periphery.rs` and its siblings.
fn cleanup(child: &mut Child) {
    let _ = child.kill();
    let _ = child.wait();
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
    // cleanup() never leaks the fixture regardless of the assertion outcome below, and never
    // blocks indefinitely the way a bare child.wait() would if terminate_pid regressed.
    cleanup(&mut child);
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
fn terminate_pid_uses_sigkill_not_sigterm() {
    // terminate_pid's own doc comment promises it "SIGKILLs via the internal rustix syscall" -
    // a specific signal, not merely "some signal that ends the target". A plain `sleep` dies
    // to SIGTERM too, so `terminate_pid_ends_a_real_process_it_holds_no_child_handle_to` above
    // cannot tell a correct SIGKILL apart from a regression to SIGTERM; a SIGTERM-ignoring
    // fixture can, the same way every sibling reap-periphery test in this tree already proves
    // the production reaper's own SIGKILL escalation stage.
    let ready_dir = tempfile::tempdir().unwrap();
    let ready_marker = ready_dir.path().join("trap-installed");
    let mut child = spawn_sigterm_ignorer(&ready_marker);
    let pid = child.id();
    assert!(
        common::is_alive(pid),
        "the freshly spawned fixture must be alive before termination"
    );
    assert!(
        wait_until(|| ready_marker.exists()),
        "precondition: the fixture must have installed its SIGTERM trap (signalled by its own \
         marker touch) before this test signals it - signalling any earlier would race the \
         shell's startup and could kill it via SIGTERM's default disposition instead of \
         actually exercising the trap, proving nothing about terminate_pid's own signal choice"
    );

    common::terminate_pid(pid);

    let died = wait_until(|| matches!(child.try_wait(), Ok(Some(_))));
    // cleanup() never leaks the fixture regardless of the assertion outcome below, and never
    // blocks indefinitely the way a bare child.wait() would if terminate_pid regressed to
    // SIGTERM (which this fixture ignores) - see cleanup()'s own doc comment.
    cleanup(&mut child);
    assert!(
        died,
        "terminate_pid must end pid {pid} even though it ignores SIGTERM - a regression to \
         SIGTERM here would leave this fixture running forever and this assertion would time \
         out instead of failing fast"
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
