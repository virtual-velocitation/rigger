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

use common::is_running;

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
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

/// Run the shipped runner around a command printing the wrapped process's `/proc/self/limits` and
/// its `MALLOC_ARENA_MAX`, with the runner's environment `envs` (on a clean slate for the
/// variables this suite drives, so every bound printed is the runner's own), returning what it
/// did and printed.
fn run_the_runner_reading_its_memory_bounds(envs: &[(&str, &str)]) -> Output {
    let tmp = tempfile::tempdir().expect("tempdir");
    let mut cmd = Command::new(pidns_runner_path());
    cmd.arg("/bin/sh")
        .arg("-c")
        .arg("cat /proc/self/limits; echo \"MALLOC_ARENA_MAX=${MALLOC_ARENA_MAX-unset}\"")
        .env("RIGGER_TEST_TMPDIR", tmp.path())
        .env_remove("RIGGER_TEST_AS_BYTES")
        .env_remove("RIGGER_PIDNS")
        .env_remove("MALLOC_ARENA_MAX");
    for (k, v) in envs {
        cmd.env(k, v);
    }
    cmd.output().expect("run the shipped runner")
}

/// What the wrapped process printed, for a runner invocation that ran it; one that did not run it
/// fails the test with the runner's stderr.
fn wrapped_output(out: &Output) -> String {
    assert!(
        out.status.success(),
        "the runner must run its wrapped command; stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// Gap 92: the per-test cap is sized for a mutation sweep's CONCURRENCY, not for one runaway.
/// At the old 24 GiB default, eight concurrent children of one loop mutant grew to ~5.5 GiB each
/// and the global OOM killer took the check-in step instead; at 4 GiB each such child fails its
/// own allocation first, and the whole suite still passes under it.
///
/// Every assertion runs on every host, with or without unprivileged user namespaces:
/// - the plain path (`RIGGER_PIDNS=off`, a throwaway CI host) caps the wrapped process at 4 GiB,
///   and an explicit `RIGGER_TEST_AS_BYTES` overrides that cap. The runner computes the cap once
///   (`as_bytes`, one home for both paths), so the override proven here is the value the
///   namespace path applies too.
/// - the namespace path (the default) either runs the wrapped process under the same 4 GiB cap,
///   on a host that can create a user+pid namespace (a workstation), or fails CLOSED on a host
///   that cannot (a hosted CI runner): it exits non-zero with its refusal and runs nothing
///   unsandboxed. Both outcomes are runner guarantees. The runner's own outcome is the host's
///   capability probe, so this test never probes the host a second way.
#[test]
fn the_runner_caps_each_test_process_address_space_at_4_gib_on_every_path_it_runs() {
    const FOUR_GIB: &str = "4294967296";
    // The cap (in bytes) the wrapped process read from its own `/proc/self/limits`.
    let applied_cap = |out: &Output| -> String {
        let limits = wrapped_output(out);
        limits
            .lines()
            .find(|l| l.starts_with("Max address space"))
            .and_then(|l| l.split_whitespace().nth(3))
            .unwrap_or_else(|| panic!("no address-space line in {limits}"))
            .to_string()
    };
    assert_eq!(
        applied_cap(&run_the_runner_reading_its_memory_bounds(&[(
            "RIGGER_PIDNS",
            "off"
        )])),
        FOUR_GIB,
        "the plain path (a CI host) caps each test process at 4 GiB"
    );
    assert_eq!(
        applied_cap(&run_the_runner_reading_its_memory_bounds(&[
            ("RIGGER_PIDNS", "off"),
            ("RIGGER_TEST_AS_BYTES", "2147483648"),
        ])),
        "2147483648",
        "an explicit cap still overrides the default (lower here: this test process already \
         runs under the 4 GiB hard limit, which no child can raise)"
    );
    let namespaced = run_the_runner_reading_its_memory_bounds(&[]);
    if namespaced.status.success() {
        assert_eq!(
            applied_cap(&namespaced),
            FOUR_GIB,
            "the namespace path caps each test process the same way"
        );
    } else {
        let stderr = String::from_utf8_lossy(&namespaced.stderr);
        assert!(
            stderr.contains(
                "cannot create a user+pid namespace on this host; \
                 REFUSING to run the test binary unsandboxed"
            ),
            "a runner that cannot create its namespace must fail closed with its refusal, and \
             fail no other way; stderr: {stderr}"
        );
        assert!(
            namespaced.stdout.is_empty(),
            "a refusing runner must run nothing unsandboxed; stdout: {}",
            String::from_utf8_lossy(&namespaced.stdout)
        );
    }
}

/// The address-space cap above measures a test's real demand only while the allocator's own
/// reservations stay bounded: glibc malloc opens another arena, reserving 64 MiB of address space,
/// whenever a thread finds the arenas it tried locked, up to eight per core, so a suite running
/// its tests on many threads under load reserves gigabytes it never touches and trips the cap.
/// The runner bounds the arena count for the wrapped process on every path it runs, beside the
/// cap; a host that cannot create the namespace refuses to run anything, as the cap test pins.
#[test]
fn the_runner_bounds_each_test_process_malloc_arenas_on_every_path_it_runs() {
    // The arena bound the wrapped process saw in its own environment.
    let arena_bound = |out: &Output| -> String {
        let printed = wrapped_output(out);
        printed
            .lines()
            .find_map(|l| l.strip_prefix("MALLOC_ARENA_MAX="))
            .unwrap_or_else(|| panic!("no MALLOC_ARENA_MAX line in {printed}"))
            .to_string()
    };
    assert_eq!(
        arena_bound(&run_the_runner_reading_its_memory_bounds(&[(
            "RIGGER_PIDNS",
            "off"
        )])),
        "2",
        "the plain path (a CI host) bounds each test process at two malloc arenas"
    );
    let namespaced = run_the_runner_reading_its_memory_bounds(&[]);
    if namespaced.status.success() {
        assert_eq!(
            arena_bound(&namespaced),
            "2",
            "the namespace path bounds each test process the same way"
        );
    }
}
