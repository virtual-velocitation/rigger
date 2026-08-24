//! Spec 66, criterion 5 (REMINDER DEDUP) - the ONE cross-process seam none of the six
//! REMINDER DEDUP tests the implementer's own commit added to `tests/cli.rs` reach:
//! what `cmd_workflow` (src/main.rs) actually HANDS DOWN to the child it spawns.
//!
//! WHY THIS FILE, DISTINCT FROM `tests/cli.rs`'s six new tests. Those six
//! (`step_reminder_*`, `run_reminder_*`, `workflow_reminder_*`) each drive one of the three
//! reminder-printing call sites and inspect THAT SAME process's own stdout/stderr - the
//! nesting-surface half of the pid-scoped contract (does THIS invocation print or stay
//! silent). None of them read back what `cmd_workflow` writes into the environment of the
//! Node-shim child it spawns: `cmd.env(SPEC_LINT_REMINDER_PID_ENV,
//! std::process::id().to_string())` (src/main.rs, right after the shim command is built) is
//! set unconditionally - after, and independent of, the print/suppress decision - and their
//! own un-provisioned-shim setup makes `cmd_workflow` fail at `locate_shim` before that line
//! ever runs, so it is never reached, let alone read back, by that suite.
//!
//! This file provisions a real `RIGGER_SHIM` / `RIGGER_NODE` stub (`locate_shim`'s
//! `RIGGER_SHIM` escape hatch, and `cmd_workflow`'s own `RIGGER_NODE` override) that reports
//! exactly what it received - its own `RIGGER_SPEC_LINT_REMINDER_PID` env value and its own
//! real direct OS parent pid (the POSIX special shell parameter `$PPID`) - so `cmd_workflow`'s
//! spawn actually runs and its child's env is inspectable, closing the gap with two directions:
//!
//!   1. no inbound sentinel at all - the child must still receive a genuine, own-pid stamp;
//!   2. an inbound sentinel that already suppresses `cmd_workflow`'s OWN reminder print (the
//!      test process names its own pid, matching the real-direct-parent check) - the child
//!      must STILL receive a fresh stamp naming `cmd_workflow`'s pid, never the suppressed,
//!      now-stale inbound value forwarded verbatim and never an omitted stamp. This is
//!      exactly the scenario `cmd.env(SPEC_LINT_REMINDER_PID_ENV, ...)`'s own doc comment
//!      calls out: "Set unconditionally (not only when the print above actually fired) so an
//!      already-suppressed `rigger workflow` still hands a genuine, verifiable link to its
//!      own child rather than a stale or absent one."

use std::path::Path;

mod common;
use common::rigger_courier;

/// Write an executable POSIX-sh stub standing in for the Node driver `cmd_workflow` would
/// normally exec (`RIGGER_NODE`), doubling as the `RIGGER_SHIM` target (`locate_shim` only
/// checks the path exists - its content is never read since we never run real Node). It
/// reports what it actually received on `RIGGER_SPEC_LINT_REMINDER_PID` and its own real
/// direct OS parent pid to `out_path`, as one line `sentinel=<value-or-empty> ppid=<n>`, then
/// exits non-zero so the test never waits on work that was never going to happen.
///
/// Reads its parent pid via the POSIX special shell parameter `$PPID`, NOT by forking `awk`
/// over `/proc/self/status`: a forked reader's OWN `/proc/self` names ITSELF, whose real
/// parent is this script (one hop too close), not this script's parent - `$PPID` is the
/// shell's own built-in, read with no subprocess and thus no extra hop.
#[cfg(unix)]
fn write_pid_reporting_stub(path: &Path, out_path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    let script = format!(
        "#!/bin/sh\n\
         printf 'sentinel=%s ppid=%s\\n' \"${{RIGGER_SPEC_LINT_REMINDER_PID:-}}\" \"$PPID\" > '{}'\n\
         exit 3\n",
        out_path.display()
    );
    std::fs::write(path, script).unwrap_or_else(|e| panic!("write stub {}: {e}", path.display()));
    let mut perms = std::fs::metadata(path).unwrap().permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(path, perms).unwrap();
}

/// Parse the stub's `sentinel=<...> ppid=<...>` report line into (sentinel, ppid).
#[cfg(unix)]
fn parse_report(report: &str) -> (String, String) {
    let mut sentinel = String::new();
    let mut ppid = String::new();
    for part in report.split_whitespace() {
        if let Some(v) = part.strip_prefix("sentinel=") {
            sentinel = v.to_string();
        } else if let Some(v) = part.strip_prefix("ppid=") {
            ppid = v.to_string();
        }
    }
    (sentinel, ppid)
}

/// No inbound sentinel: `cmd_workflow` must still stamp a genuine own-pid value on the
/// spawned child - the stamped value must equal the child's own real direct parent pid
/// (i.e. `rigger workflow`'s own pid), not be absent or some unrelated value.
#[cfg(unix)]
#[test]
fn workflow_stamps_its_own_pid_on_the_spawned_child_with_no_inbound_sentinel() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let stub = root.join("node-stub.sh");
    let out = root.join("report.txt");
    write_pid_reporting_stub(&stub, &out);

    let status = rigger_courier()
        .args(["workflow", "specs/42-widgets.md"])
        .current_dir(root)
        .env("RIGGER_SHIM", &stub)
        .env("RIGGER_NODE", &stub)
        .status()
        .expect("failed to spawn rigger workflow");
    assert!(
        !status.success(),
        "the stub always exits non-zero, so `rigger workflow` must surface that as a failure"
    );

    let report = std::fs::read_to_string(&out)
        .unwrap_or_else(|e| panic!("the stub must run and write {}: {e}", out.display()));
    let (sentinel, ppid) = parse_report(&report);
    assert!(
        !sentinel.is_empty(),
        "cmd_workflow must stamp SOME pid on the spawned child's env; got: {report:?}"
    );
    assert_eq!(
        sentinel, ppid,
        "the stamped sentinel must equal the child's own real direct parent pid (i.e. \
         `rigger workflow`'s own pid) - not an inherited, stale, or unrelated value; got: \
         {report:?}"
    );
}

/// An inbound sentinel naming this test process's own pid suppresses `cmd_workflow`'s OWN
/// reminder print (the same real-direct-parent match `tests/cli.rs`'s
/// `workflow_reminder_is_suppressed_when_env_names_the_real_direct_parent_pid` already pins),
/// but the spawned child must STILL receive a fresh, genuine stamp naming `cmd_workflow`'s
/// own pid, never the now-stale inbound value forwarded unchanged and never an omitted one.
#[cfg(unix)]
#[test]
fn workflow_still_stamps_a_fresh_own_pid_on_the_child_even_when_its_own_reminder_was_suppressed() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let stub = root.join("node-stub.sh");
    let out = root.join("report.txt");
    write_pid_reporting_stub(&stub, &out);

    // This test process is `rigger workflow`'s real direct OS parent, so naming its own pid
    // here suppresses `rigger workflow`'s own reminder print.
    let own_pid = std::process::id().to_string();
    let status = rigger_courier()
        .args(["workflow", "specs/42-widgets.md"])
        .current_dir(root)
        .env("RIGGER_SHIM", &stub)
        .env("RIGGER_NODE", &stub)
        .env("RIGGER_SPEC_LINT_REMINDER_PID", &own_pid)
        .status()
        .expect("failed to spawn rigger workflow");
    assert!(!status.success(), "the stub always exits non-zero");

    let report = std::fs::read_to_string(&out)
        .unwrap_or_else(|e| panic!("the stub must run and write {}: {e}", out.display()));
    let (sentinel, ppid) = parse_report(&report);
    assert_eq!(
        sentinel, ppid,
        "even when its OWN reminder is suppressed, `rigger workflow` must still stamp a \
         fresh, valid pid (its own) on the spawned child - never omit the stamp; got: \
         {report:?}"
    );
    assert_ne!(
        sentinel, own_pid,
        "the child must see `rigger workflow`'s OWN pid, not the test process's pid it was \
         handed on the inbound RIGGER_SPEC_LINT_REMINDER_PID (a stale value one hop too far \
         up the chain for this child) forwarded verbatim; got: {report:?}"
    );
}
