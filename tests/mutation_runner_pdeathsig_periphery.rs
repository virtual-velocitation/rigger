//! Periphery test for spec 91's RUNNER GUARANTEE (criterion 2): "a timed-out mutant's test
//! process dies with the `cargo` that launched it" - proven directly against the shipped
//! `.cargo/pidns-runner.sh` (landed ahead of this unit, commit ad778dd, "a test process dies
//! with the cargo that launched it") rather than against `cargo mutants` itself, which this
//! suite has no way to time out deterministically.
//!
//! cargo-mutants times a mutant out by ending its `cargo test` CHILD; `cargo` is the runner's
//! own PARENT, so nothing signals the runner directly - before the shipped fix, the runner (and
//! the real test binary under it) survived every such timeout as an orphan, one of them spinning
//! at full CPU for 6.6 hours while holding cargo-mutants' own output pipe open (spec 91's Goal).
//! The fix marks the runner's own pid `setpriv --pdeathsig` (TERM on the plain path this test
//! drives, KILL under the pid-namespace path) so it - and the real test binary it wraps - dies
//! the instant its LAUNCHER exits, with no signal from the launcher required at all.
//!
//! This is a LAUNCHER-EXITS fixture, not a launcher-signals one: the launcher below merely
//! returns (`exit 0`) after confirming the wrapped binary has genuinely started - it never
//! sends the runner (or its child) any signal itself. Everything that dies here dies purely
//! because its parent thread group exited, which is the exact mechanism a real `cargo test`
//! parent dying (killed by cargo-mutants, or simply finishing) exercises in production.

mod common;

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

/// The shipped runner script, resolved the same CWD-independent way every other committed-file
/// pin in this suite does (`env!("CARGO_MANIFEST_DIR")`), so this test runs identically
/// regardless of the process's current directory.
fn pidns_runner_path() -> PathBuf {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(".cargo")
        .join("pidns-runner.sh");
    assert!(
        path.exists(),
        "the shipped test runner must exist at {}",
        path.display()
    );
    path
}

/// Poll `pred` until it holds or `bound` elapses; returns whether it held. Mirrors the
/// established `wait_until` idiom this crate's own suites already use for a real subprocess's
/// non-deterministic timing (`gate::tests::wait_until`, `budget::tests::wait_until`) - never a
/// fixed sleep, which either races a fast machine or wastes a slow one.
fn wait_until(bound: Duration, mut pred: impl FnMut() -> bool) -> bool {
    let deadline = Instant::now() + bound;
    loop {
        if pred() {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(Duration::from_millis(25));
    }
}

/// Whether `pid` is a genuinely RUNNING process - unlike `common::is_alive` (a `kill(pid, 0)`-
/// equivalent existence probe), this reports `false` for a REAPED-PENDING ZOMBIE too, which
/// still answers alive to that probe: POSIX keeps a terminated child's process-table entry
/// (holding no memory, no open file descriptors, no CPU time) until its parent calls `wait()`
/// on it. This suite's OWN test binary runs as pid 1 of its own pid namespace
/// (`.cargo/pidns-runner.sh`), so a process this fixture backgrounds and never holds a `Child`
/// handle to gets reparented to US (the namespace's implicit reaper) the instant its own
/// launcher exits - and this test deliberately never runs a reap loop, so a process that HAS
/// genuinely terminated via `pdeathsig` sits as an unreaped zombie for the rest of the test,
/// which is exactly the state this predicate must still call "not surviving" (a zombie holds
/// no pipe open and burns no CPU - precisely what the criterion cares about). Reads
/// `/proc/<pid>/stat`'s state field directly - a plain filesystem read, never a signal - so a
/// genuinely absent pid and a zombie one are treated identically ("not running"). Linux-only,
/// matching this crate's own existing `/proc`-reading conventions (`.cargo/pidns-runner.sh`,
/// `src/reap.rs`).
fn is_running(pid: u32) -> bool {
    let stat = match std::fs::read_to_string(format!("/proc/{pid}/stat")) {
        Ok(s) => s,
        Err(_) => return false,
    };
    // Fields are "<pid> (<comm>) <state> ..."; `comm` may itself contain spaces or parens, so
    // the state char is the first token after the LAST ')', never a naive whitespace split.
    match stat.rfind(')') {
        Some(idx) => !stat[idx + 1..].trim_start().starts_with('Z'),
        None => false,
    }
}

/// Read a pidfile a fixture wrote as its own decimal `u32`, once it is non-empty (a plain
/// `File::create` truncates to empty before the writer's first byte lands, so callers must
/// wait for non-empty content, never merely for the path to exist).
fn read_pid(path: &Path) -> u32 {
    std::fs::read_to_string(path)
        .unwrap_or_default()
        .trim()
        .parse()
        .unwrap_or_else(|_| panic!("pidfile {} must contain a decimal pid", path.display()))
}

#[test]
fn a_launcher_that_merely_exits_ends_the_runner_and_its_wrapped_process_with_it() {
    let dir = tempfile::tempdir().expect("tempdir");
    let runner_pidfile = dir.path().join("runner.pid");
    let child_pidfile = dir.path().join("child.pid");

    // The wrapped "test binary": a tiny shell that reports ITS OWN pid (before `exec`, so the
    // reported pid and the eventual `sleep` pid are identical - `exec` replaces the image, not
    // the pid) and then becomes a long-lived `sleep`, mirroring this crate's own sanctioned
    // `spawn_sleeper` fixture shape (tests/no_os_kill_test_helper_periphery.rs) - a real,
    // signal-terminable process with no descendants of its own to complicate the proof.
    let wrapped = format!(
        "echo $$ > {} ; exec sleep 300",
        shell_quote(child_pidfile.display().to_string())
    );

    // The LAUNCHER: backgrounds the runner (RIGGER_PIDNS=off - the portable, non-namespaced
    // path, so this test needs no unprivileged user namespaces), captures the runner's own pid
    // via `$!`, then busy-waits for the wrapped binary to actually report itself before exiting
    // - so the runner's full exec chain (nice -> setpriv -> timeout -> fork) has DEFINITELY
    // already run setpriv's pdeathsig registration by the time this launcher exits. Without
    // this wait, an exit racing the runner's own startup could beat pdeathsig's registration
    // (a documented Linux behavior: a parent already dead when PR_SET_PDEATHSIG is installed
    // never signals) - a real cargo-mutants timeout always fires long after `cargo test` has
    // fully started, so waiting for genuine startup is the faithful shape of the fixture, not a
    // shortcut around it.
    let launcher = format!(
        "RIGGER_PIDNS=off {} sh -c {} & echo $! > {} ; \
         i=0; while [ ! -s {} ] && [ $i -lt 200 ]; do sleep 0.05; i=$((i+1)); done; exit 0",
        shell_quote(pidns_runner_path().display().to_string()),
        shell_quote(&wrapped),
        shell_quote(runner_pidfile.display().to_string()),
        shell_quote(child_pidfile.display().to_string()),
    );

    let status = Command::new("sh")
        .arg("-c")
        .arg(&launcher)
        .status()
        .expect("spawn the launcher");
    assert!(status.success(), "the launcher itself must exit cleanly");

    assert!(
        child_pidfile
            .metadata()
            .map(|m| m.len() > 0)
            .unwrap_or(false),
        "the wrapped process never reported its own pid - the runner never started; the \
         launcher's busy-wait should have blocked on exactly this"
    );
    let runner_pid = read_pid(&runner_pidfile);
    let child_pid = read_pid(&child_pidfile);

    // THE PROOF: the launcher has ALREADY exited (the `Command::status()` call above only
    // returns once it has) with no signal to anything ever sent by this test - yet both the
    // runner (marked `--pdeathsig`) and the real wrapped process it forked must become dead
    // within a generous bound. Before the shipped fix, neither ever did: they were orphaned
    // and ran for the runner's own hour-long cap at minimum (spec 91's Goal cites one that ran
    // 6.6 hours under `systemd --user`).
    assert!(
        wait_until(Duration::from_secs(10), || !is_running(runner_pid)),
        "the runner (pid {runner_pid}) must die once its launcher exits - no signal was ever \
         sent to it, only pdeathsig firing on the launcher's own exit explains its death"
    );
    assert!(
        wait_until(Duration::from_secs(10), || !is_running(child_pid)),
        "the wrapped process (pid {child_pid}, a plain `sleep 300`) must die WITH the runner - \
         a sweep waiting on this pid's own output pipe must never be left holding an orphan"
    );

    // Best-effort cleanup: covers the failure path above (an assertion panic still leaves a
    // real subprocess on the machine otherwise), never load-bearing for the proof itself.
    if common::is_alive(child_pid) {
        common::terminate_pid(child_pid);
    }
    if common::is_alive(runner_pid) {
        common::terminate_pid(runner_pid);
    }
}

/// Single-quote `s` for safe interpolation into the `sh -c` scripts above - mirrors
/// `tests/no_os_kill_test_helper_periphery.rs::shell_quote`'s own convention, widened to take a
/// plain string (rather than only a `Path`) so it quotes both a real path AND the wrapped
/// command string with one function; a tempdir path never contains a single quote, but
/// escaping properly keeps this correct even if that ever changed.
fn shell_quote(s: impl AsRef<str>) -> String {
    format!("'{}'", s.as_ref().replace('\'', r"'\''"))
}
