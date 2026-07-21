//! Reap processes rooted in a dir rigger is about to remove (spec 23).
//!
//! rigger owns the lifecycle of the per-unit worktrees and agent-scratch dirs it creates
//! under `<repo>/.rigger/tmp/`, but historically tore them down by removing the DIR only -
//! it never reaped a process whose working directory was INSIDE that dir. Such a process (a
//! build an agent left running, a tool the harness spawned inside the worktree, a stray
//! server) then outlived its dir: it held a now-deleted cwd and leaked memory. This module
//! closes that: before rigger removes a dir it owns, it finds every process whose resolved
//! cwd is inside that dir and reaps it (SIGTERM, a short grace, then SIGKILL), so nothing
//! outlives the dir. It extends spec 19b's no-orphaned-processes guarantee (rigger's OWN
//! children) to ANY process rooted in a dir rigger owns, regardless of who spawned it.
//!
//! Two entry points share one scan authority:
//! - [`processes_rooted_under`] - the pure detection primitive, `(pid, command)` for every
//!   process whose cwd resolves strictly inside a base dir. Both the teardown reap AND
//!   `rigger validate`'s leaked-process advisory (spec 23, unit 2) consume this exact
//!   function; there is no second scan.
//! - [`reap_processes_rooted_under`] - the teardown reap that kills what the scan finds.
//!
//! Best-effort and platform-tolerant. Detection is Linux-first via `/proc/<pid>/cwd`
//! (read with `std::fs::read_link`, std-only - no `libc`); on a platform without `/proc`
//! it is a graceful no-op returning empty, NEVER a hard error, so teardown and validate
//! keep working on any platform. Killing uses the `kill` command via [`std::process::Command`]
//! (SIGTERM then SIGKILL), NOT a `libc` call, so the `--no-default-features` build - which
//! carries no `libc` - reaps identically.
//!
//! SAFETY BOUNDARY (load-bearing): the scan matches ONLY a process whose canonicalized cwd
//! equals the base dir or lies strictly under it (`<base>/...`), by path COMPONENTS, never a
//! raw string prefix - so a sibling dir whose path merely shares a string prefix (`<base>-x`)
//! is never matched. The base is always a dir rigger owns under `<repo>/.rigger/tmp/`, so a
//! process rooted at the repo root, in another project, or anywhere outside rigger's own
//! scratch is NEVER reaped (an editor's language server or a user's shell at the repo root is
//! off-limits).

use std::path::Path;

/// How long a well-behaved process is given to exit on SIGTERM before it is SIGKILLed.
/// Short: teardown is on the hot path (every unit worktree removal), and a process that
/// ignores SIGTERM should not stall the run - a fraction of a second is ample for a process
/// that handles the signal, and the SIGKILL backstop reaps the rest.
const GRACE: std::time::Duration = std::time::Duration::from_millis(300);

/// Every process whose resolved cwd is `base_dir` itself or strictly inside it, as
/// `(pid, command)`. The SINGLE scan authority both the teardown reap
/// ([`reap_processes_rooted_under`]) and `rigger validate`'s leaked-process advisory
/// (spec 23, unit 2) consume - there is no second implementation.
///
/// Best-effort and Linux-first via `/proc/<pid>/cwd`. Returns EMPTY - a graceful no-op,
/// never an error - when `base_dir` cannot be canonicalized (it does not exist) or `/proc`
/// is absent or unreadable (a non-Linux platform), so teardown and validate work anywhere.
///
/// Containment is CANONICAL-PATH STRICT-INSIDE, matched on path components: a process is
/// returned iff its canonicalized cwd equals the canonicalized `base_dir` or starts with it
/// as a path prefix. Component matching (not string prefix) is the load-bearing safety
/// boundary - `<base>-sibling` shares a string prefix with `<base>` but is a different
/// component and is never matched, so a process outside the exact dir is never reaped.
/// The scanning process itself is excluded (rigger never reaps its own pid).
pub fn processes_rooted_under(base_dir: &Path) -> Vec<(u32, String)> {
    // Canonicalize the base so a symlinked component matches the kernel-resolved cwd, and so
    // an absent dir short-circuits to empty (nothing to scan). This never creates the dir.
    let Ok(base) = base_dir.canonicalize() else {
        return Vec::new();
    };
    let proc = Path::new("/proc");
    // No `/proc` (a non-Linux platform, or one where it cannot be read): a graceful no-op.
    let Ok(entries) = std::fs::read_dir(proc) else {
        return Vec::new();
    };
    let self_pid = std::process::id();
    let mut out = Vec::new();
    for entry in entries.flatten() {
        let name = entry.file_name();
        // `/proc/<pid>` entries are the numeric dirs; skip `/proc/self`, `/proc/meminfo`, etc.
        let Some(pid) = name.to_str().and_then(|n| n.parse::<u32>().ok()) else {
            continue;
        };
        // Never reap the scanning process itself.
        if pid == self_pid {
            continue;
        }
        // `/proc/<pid>/cwd` is a symlink the kernel resolves to the process's absolute,
        // canonical working directory. `read_link` is std-only (no `libc`). A read that fails
        // (the process exited between the readdir and here, or it belongs to another user and
        // its cwd is unreadable) is simply skipped - best-effort.
        let Ok(cwd) = std::fs::read_link(proc.join(&name).join("cwd")) else {
            continue;
        };
        if is_inside(&cwd, &base) {
            out.push((pid, read_command(proc, &name)));
        }
    }
    out
}

/// The process's command for the advisory, from `/proc/<pid>/cmdline` (NUL-separated argv)
/// with a fallback to `/proc/<pid>/comm` (the short name) and finally an empty string. Purely
/// descriptive - it names the leak in the `rigger validate` advisory and never affects which
/// processes are reaped.
fn read_command(proc: &Path, pid_name: &std::ffi::OsStr) -> String {
    let dir = proc.join(pid_name);
    if let Ok(bytes) = std::fs::read(dir.join("cmdline")) {
        let joined = bytes
            .split(|b| *b == 0)
            .filter(|part| !part.is_empty())
            .map(|part| String::from_utf8_lossy(part).into_owned())
            .collect::<Vec<_>>()
            .join(" ");
        if !joined.is_empty() {
            return joined;
        }
    }
    std::fs::read_to_string(dir.join("comm"))
        .map(|s| s.trim().to_string())
        .unwrap_or_default()
}

/// Reap every process rooted STRICTLY inside `base_dir` before rigger removes that dir
/// (spec 23, unit 1): SIGTERM every match, wait a short grace for the well-behaved to exit,
/// then SIGKILL whatever is STILL rooted inside - so no process outlives the worktree/scratch
/// dir it ran in. Scoped to the exact `base_dir` via [`processes_rooted_under`], so a process
/// rooted at the repo root, in another project, or anywhere outside rigger's own scratch is
/// NEVER touched (the load-bearing safety boundary).
///
/// The SIGKILL pass RE-SCANS rather than reusing the SIGTERM pid list: a process that already
/// exited on SIGTERM is gone from the re-scan (so it is not signalled, closing a pid-recycle
/// window where its number was reused by an unrelated process outside the base), and only what
/// is genuinely still rooted inside is force-killed - the boundary holds for the SIGKILL too.
///
/// Best-effort and platform-tolerant: where `/proc` is absent the scan finds nothing and this
/// is a graceful no-op. Killing uses the `kill` command via [`std::process::Command`], NOT a
/// `libc` call, so the `--no-default-features` build (no `libc`) reaps identically.
pub fn reap_processes_rooted_under(base_dir: &Path) {
    let victims = processes_rooted_under(base_dir);
    if victims.is_empty() {
        return;
    }
    for (pid, _) in &victims {
        send_signal("TERM", *pid);
    }
    std::thread::sleep(GRACE);
    // Re-scan so only processes STILL rooted inside are force-killed (a SIGTERM-exited pid is
    // gone; a recycled pid outside the base is not matched) - the safety boundary again.
    for (pid, _) in processes_rooted_under(base_dir) {
        send_signal("KILL", pid);
    }
}

/// Send `signal` (e.g. `"TERM"`, `"KILL"`) to `pid` via the `kill` command. Best-effort: a
/// failure (the process already exited, or insufficient permission) is ignored - the reap is
/// teardown cleanup, never a hard error that could fail a worktree removal or a step. Uses
/// `kill(1)` rather than a `libc` call so the `--no-default-features` build (no `libc`) reaps
/// identically.
fn send_signal(signal: &str, pid: u32) {
    let _ = std::process::Command::new("kill")
        .arg(format!("-{signal}"))
        .arg(pid.to_string())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();
}

/// Whether `cwd` is `base` itself or strictly under it, matched on path COMPONENTS. Both are
/// absolute (the `/proc` cwd link resolves to an absolute path; `base` is canonicalized by
/// the caller). `Path::starts_with` is component-wise, so `/a/bc` never matches `/a/b` - the
/// safety boundary against a raw string-prefix false match.
fn is_inside(cwd: &Path, base: &Path) -> bool {
    cwd.starts_with(base)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::{Child, Command};

    /// Spawn a long-lived `sleep` whose cwd is `dir`, so it appears in `/proc` rooted there.
    fn sleeper_in(dir: &Path) -> Child {
        Command::new("sleep")
            .arg("300")
            .current_dir(dir)
            .spawn()
            .expect("spawn sleep")
    }

    /// Spawn a long-lived process rooted at `dir` that IGNORES SIGTERM, so only the SIGKILL
    /// escalation can reap it - exercising the full SIGTERM-then-SIGKILL mechanism.
    fn sigterm_ignorer_in(dir: &Path) -> Child {
        Command::new("sh")
            .arg("-c")
            .arg("trap '' TERM; while :; do sleep 1; done")
            .current_dir(dir)
            .spawn()
            .expect("spawn sigterm-ignoring child")
    }

    /// Poll `child.try_wait()` until the process has exited or a generous timeout elapses;
    /// returns whether it exited.
    fn wait_for_exit(child: &mut Child) -> bool {
        wait_until(|| matches!(child.try_wait(), Ok(Some(_))))
    }

    /// Poll until `pred` holds or a generous timeout elapses; returns whether it held.
    fn wait_until(mut pred: impl FnMut() -> bool) -> bool {
        for _ in 0..200 {
            if pred() {
                return true;
            }
            std::thread::sleep(std::time::Duration::from_millis(25));
        }
        false
    }

    #[test]
    fn processes_rooted_under_matches_only_processes_strictly_inside_the_base() {
        // The load-bearing safety boundary (spec 23): the scan must return a process whose
        // cwd is INSIDE the base dir, and must NEVER return one rooted at the base's parent
        // (outside) or in a SIBLING dir whose path merely shares a string prefix (`<base>-x`).
        let root = tempfile::tempdir().unwrap();
        let base = root.path().join("scratch");
        let inner = base.join("inner");
        // A sibling whose path is a STRING prefix match of `base` but a different component -
        // the trap a naive `cwd_str.starts_with(base_str)` would fall into.
        let sibling = root.path().join("scratch-evil");
        std::fs::create_dir_all(&inner).unwrap();
        std::fs::create_dir_all(&sibling).unwrap();

        let mut inside = sleeper_in(&inner);
        let mut outside = sleeper_in(root.path());
        let mut sib = sleeper_in(&sibling);

        let found = wait_until(|| {
            processes_rooted_under(&base)
                .iter()
                .any(|(pid, _)| *pid == inside.id())
        });

        // Capture the scan once for the exclusion assertions.
        let scanned = processes_rooted_under(&base);
        let pids: Vec<u32> = scanned.iter().map(|(pid, _)| *pid).collect();

        // Reap the fixtures before asserting, so a failed assert never leaks sleepers.
        let _ = inside.kill();
        let _ = outside.kill();
        let _ = sib.kill();
        let _ = inside.wait();
        let _ = outside.wait();
        let _ = sib.wait();

        assert!(
            found,
            "a process rooted inside the base dir must be detected"
        );
        assert!(
            pids.contains(&inside.id()),
            "the inside process (pid {}) is in the scan: {pids:?}",
            inside.id()
        );
        assert!(
            !pids.contains(&outside.id()),
            "a process rooted at the base's PARENT (outside) must never be matched (pid {})",
            outside.id()
        );
        assert!(
            !pids.contains(&sib.id()),
            "a SIBLING sharing a string prefix (`<base>-evil`) must never be matched (pid {})",
            sib.id()
        );
    }

    #[test]
    fn processes_rooted_under_is_a_graceful_no_op_when_the_base_is_absent() {
        // Platform tolerance / read-only safety: an absent base (nothing to scan, and the
        // stand-in for an absent `/proc`) yields EMPTY, never an error - so teardown and
        // validate keep working where the dir or `/proc` is not there.
        let root = tempfile::tempdir().unwrap();
        let absent = root.path().join("never-created");
        assert!(processes_rooted_under(&absent).is_empty());
    }

    #[test]
    fn reap_kills_a_sigterm_ignoring_child_inside_and_spares_one_outside() {
        // The teardown reap (spec 23): a process rooted inside the base is reaped even when it
        // IGNORES SIGTERM (the SIGKILL escalation after the grace does it), while a process
        // rooted OUTSIDE the base is left ALIVE - the safety boundary holds for the kill too.
        let root = tempfile::tempdir().unwrap();
        let base = root.path().join("scratch");
        std::fs::create_dir_all(&base).unwrap();

        let mut inside = sigterm_ignorer_in(&base);
        let mut outside = sleeper_in(root.path());

        assert!(
            wait_until(|| processes_rooted_under(&base)
                .iter()
                .any(|(pid, _)| *pid == inside.id())),
            "precondition: the inside child is detected before the reap"
        );

        reap_processes_rooted_under(&base);

        let inside_died = wait_for_exit(&mut inside);
        // The outside sleeper must still be running; capture before cleanup.
        let outside_alive = matches!(outside.try_wait(), Ok(None));

        let _ = outside.kill();
        let _ = outside.wait();
        // Belt and braces: if the inside child somehow survived, do not leak it.
        if !inside_died {
            let _ = inside.kill();
            let _ = inside.wait();
        }

        assert!(
            inside_died,
            "a SIGTERM-ignoring process rooted inside the base must be SIGKILLed by the reap"
        );
        assert!(
            outside_alive,
            "a process rooted OUTSIDE the base must survive the reap (safety boundary)"
        );
    }

    #[test]
    fn reap_is_a_graceful_no_op_when_nothing_is_rooted_inside() {
        // No process rooted inside (and the absent-`/proc` stand-in): the reap does nothing and
        // never errors, so teardown proceeds on any platform.
        let root = tempfile::tempdir().unwrap();
        let base = root.path().join("scratch");
        std::fs::create_dir_all(&base).unwrap();
        // A sleeper OUTSIDE the base must be untouched by a reap scoped to the empty base.
        let mut outside = sleeper_in(root.path());
        reap_processes_rooted_under(&base);
        let outside_alive = matches!(outside.try_wait(), Ok(None));
        let _ = outside.kill();
        let _ = outside.wait();
        assert!(
            outside_alive,
            "an empty-base reap touches nothing outside it"
        );
    }

    #[test]
    fn is_inside_matches_on_components_not_string_prefix() {
        assert!(is_inside(Path::new("/a/b"), Path::new("/a/b")));
        assert!(is_inside(Path::new("/a/b/c"), Path::new("/a/b")));
        assert!(!is_inside(Path::new("/a/bc"), Path::new("/a/b")));
        assert!(!is_inside(Path::new("/a"), Path::new("/a/b")));
    }
}
