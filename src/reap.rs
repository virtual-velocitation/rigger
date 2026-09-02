//! Reap processes rooted in a dir rigger is about to remove (spec 23), with a base-guard,
//! TOCTOU recheck and handle-bound-only signal API (spec 78, THE REAPER).
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
//! Three entry points share one scan authority and one termination sequence:
//! - [`processes_rooted_under`] - the pure detection primitive, `(pid, command)` for every
//!   process whose cwd resolves strictly inside a base dir. Both the teardown reap AND
//!   `rigger validate`'s leaked-process advisory (spec 23, unit 2) consume this exact
//!   function; there is no second scan. UNGUARDED by [`is_reapable_base`] - `rigger
//!   validate` deliberately scans a whole scratch tree for visibility, which the reap's own
//!   boundary (below) would otherwise refuse as "the base itself".
//! - [`reap_processes_rooted_under`] - the teardown reap that kills what the scan finds,
//!   gated by [`is_reapable_base`] so it can ONLY ever act on a base the caller's own
//!   `authorized_root` covers.
//! - [`reap_authorized`] - the termination sequence itself (SIGTERM, grace, rescan, SIGKILL),
//!   factored out so [`Worktree::remove`](crate::worktree::Worktree::remove)'s independent,
//!   git-identity-based authorization reuses the ONE implementation rather than a second,
//!   parallel one. [`reap_processes_rooted_under`] is exactly `is_reapable_base` then this.
//!
//! Best-effort and platform-tolerant. Detection is Linux-first via `/proc/<pid>/cwd`
//! (read with `std::fs::read_link`, std-only - no `libc`); on a platform without `/proc`
//! it is a graceful no-op returning empty, NEVER a hard error, so teardown and validate
//! keep working on any platform.
//!
//! SAFETY BOUNDARY (load-bearing, spec 23 + spec 78): the scan matches ONLY a process whose
//! canonicalized cwd equals the base dir or lies strictly under it (`<base>/...`), by path
//! COMPONENTS, never a raw string prefix - so a sibling dir whose path merely shares a
//! string prefix (`<base>-x`) is never matched. On TOP of that, [`reap_processes_rooted_under`]
//! additionally requires the base itself to canonicalize to somewhere STRICTLY under an
//! `authorized_root` the CALLER supplies ([`is_reapable_base`]) - so a caller that ever
//! computed a wrong or widened base relative to the root it meant to reap under gets a
//! logged no-op instead of a kill. `authorized_root` is never re-derived here (no hardcoded
//! `<repo>/.rigger/tmp` literal, no git resolution of the caller's repo): the caller passes
//! the SAME resolved root it already used to build `base_dir` itself
//! ([`crate::worktree::scratch_root_path_from_env`] for the run's own scratch tree, or a
//! registered mutation-scratch root under `$XDG_CACHE_HOME`/`$HOME/.cache` for the
//! `cargo-mutants` tree, spec 77 criteria 2-3) - so the boundary can never silently diverge
//! from what the rest of the codebase already treats as authoritative, however that root is
//! placed (a relocated `RIGGER_TMPDIR`/`defaults.workdir`, or a cache home entirely outside
//! any git tree). [`Worktree::remove`](crate::worktree::Worktree::remove) is the one
//! exception: a worktree's own dir can legitimately live anywhere relative to its repo (the
//! same relocation surface), so there is no `authorized_root` any caller could compute that
//! would reliably contain it; it authorizes its reap by GIT IDENTITY instead (is `self.dir`
//! CURRENTLY a registered worktree checked out on `self.branch`?) and calls
//! [`reap_authorized`] directly, bypassing this containment gate entirely - see that
//! function's own doc comment.
//!
//! SIGNAL API (spec 78): every signal rigger issues to a process it does not hold a
//! [`std::process::Child`] handle to goes through `rustix::process::kill_process` - never a
//! shell-out to `kill(1)`, never `libc::kill`, never a process-group (negative pid) target.
//! `rustix::process::Pid::from_raw` accepts negative raw values (it rejects only zero), so
//! [`send_signal`] and [`is_signal_eligible`] each carry their OWN explicit `pid > 1` guard
//! rather than leaning on the type to refuse one. `default-features = false` + `std` +
//! `process` selects rustix's linux_raw backend, so the `--no-default-features` build pulls
//! no `libc` crate edge and reaps identically.
//!
//! TOCTOU (spec 78): the scan and the signal are not atomic - a pid can exit and be
//! recycled by the kernel onto an unrelated process in the gap between them. Every
//! candidate's start time (`/proc/<pid>/stat` field 22, kernel-immutable for a pid's
//! lifetime) is captured at scan time and re-read, alongside its cwd, IMMEDIATELY before it
//! is actually signalled; either differing skips the signal (spec 78, [`signal_if_unchanged`]).
//! [`reap_processes_rooted_under`] also never signals pid 0 or 1, its own pid, or any
//! ancestor of its own process (the reaper's launching shell/session, walked via
//! `/proc/<pid>/status`'s `PPid:` chain, [`ancestor_pids`]) - so a coincidence or a
//! recycled pid can never reach upward into the process tree running rigger itself.

use rustix::process::{Pid, Signal};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// How long a well-behaved process is given to exit on SIGTERM before it is SIGKILLed.
/// Short: teardown is on the hot path (every unit worktree removal), and a process that
/// ignores SIGTERM should not stall the run - a fraction of a second is ample for a process
/// that handles the signal, and the SIGKILL backstop reaps the rest.
const GRACE: std::time::Duration = std::time::Duration::from_millis(300);

/// Every process whose resolved cwd is `base_dir` itself or strictly inside it, as
/// `(pid, command)`. The SINGLE scan authority both the teardown reap
/// ([`reap_processes_rooted_under`]) and `rigger validate`'s leaked-process advisory
/// (spec 23, unit 2) consume - there is no second implementation. UNGUARDED: unlike the
/// reap, this pure detection primitive is not scoped by [`is_reapable_base`], so a caller
/// (like the validate advisory) may point it at `.rigger/tmp` itself for full visibility.
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

/// One candidate captured at scan time: the pid and its start time
/// (`/proc/<pid>/stat` field 22, in clock ticks since boot). The kernel guarantees this is
/// IMMUTABLE for the life of a pid and can only repeat if the pid itself is reused, so
/// comparing it again right before the signal is the TOCTOU witness that the pid still
/// names the SAME process the scan found, not one the kernel recycled onto that number in
/// the meantime (spec 78).
struct ScanEntry {
    pid: u32,
    starttime: u64,
}

/// [`processes_rooted_under`] (the one scan authority) paired with each match's start time
/// captured at this same instant, for the reaper's TOCTOU recheck. A pid whose start time
/// cannot be read (it already exited between the cwd scan and this read) is dropped -
/// nothing to compare against later, nothing worth signalling now.
fn scan_with_starttime(base: &Path) -> Vec<ScanEntry> {
    processes_rooted_under(base)
        .into_iter()
        .filter_map(|(pid, _)| pid_starttime(pid).map(|starttime| ScanEntry { pid, starttime }))
        .collect()
}

/// The process start time from `/proc/<pid>/stat` field 22 (`starttime`, clock ticks since
/// boot) - the TOCTOU witness [`scan_with_starttime`] records and [`signal_if_unchanged`]
/// re-reads immediately before signalling. Field 22 is located from the LAST `)` in the
/// line rather than by naive whitespace-splitting, because field 2 (`comm`, the process
/// name in parens) may itself contain spaces or parens. `None` when the process has already
/// exited or `/proc` is unavailable.
fn pid_starttime(pid: u32) -> Option<u64> {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    let after_comm = stat.rsplit_once(')')?.1;
    // Fields after `comm`, 1-indexed: state, ppid, pgrp, session, tty_nr, tpgid, flags,
    // minflt, cminflt, majflt, cmajflt, utime, stime, cutime, cstime, priority, nice,
    // num_threads, itrealvalue, starttime - the 20th, so index 19 (0-based).
    after_comm.split_whitespace().nth(19)?.parse().ok()
}

/// `pid`'s parent pid from `/proc/<pid>/status`'s `PPid:` field. `None` when the process is
/// gone or the field cannot be read/parsed.
fn read_ppid(pid: u32) -> Option<u32> {
    let status = std::fs::read_to_string(format!("/proc/{pid}/status")).ok()?;
    status
        .lines()
        .find_map(|line| line.strip_prefix("PPid:"))
        .and_then(|value| value.trim().parse().ok())
}

/// Every ancestor pid of `pid`, walking `/proc/<pid>/status`'s `PPid:` chain upward (spec
/// 78): the reaper's own launching shell, terminal, session leader, and so on up to (but
/// excluding, since it is refused unconditionally elsewhere) init. Used to refuse ever
/// signalling anything on the REAPER's own lineage, however a computed or recycled pid
/// might coincide with one. Best-effort: the walk stops at a pid <= 1, an unreadable
/// `PPid:` field, or a repeat (a cycle, which `/proc` should never produce, but the walk
/// must terminate regardless).
fn ancestor_pids(pid: u32) -> HashSet<u32> {
    let mut out = HashSet::new();
    let mut current = pid;
    while let Some(parent) = read_ppid(current) {
        if parent <= 1 || !out.insert(parent) {
            break;
        }
        current = parent;
    }
    out
}

/// Whether `pid` is EVER eligible to be signalled by the reaper, independent of cwd/base:
/// never 0 or 1 (init - an EXPLICIT guard, not left to the type: the pinned rustix's
/// `Pid::from_raw` accepts negative raw values, only zero is rejected, so a pid this small
/// must be refused before it ever reaches the syscall), never the reaper's own pid, and
/// never one of the reaper's own ancestors (spec 78).
fn is_signal_eligible(pid: u32, self_pid: u32, self_ancestors: &HashSet<u32>) -> bool {
    pid > 1 && pid != self_pid && !self_ancestors.contains(&pid)
}

/// Re-read `target`'s cwd and start time IMMEDIATELY before signalling it, and signal only
/// if it is [`is_signal_eligible`] AND both still match the base and what the scan
/// recorded. This is the TOCTOU guard (spec 78): a pid that exited and was recycled onto
/// an unrelated process (even one that happens to also be rooted under `base`) between the
/// scan and this call is silently skipped, never signalled.
fn signal_if_unchanged(
    target: &ScanEntry,
    base: &Path,
    self_pid: u32,
    self_ancestors: &HashSet<u32>,
    signal: Signal,
) {
    if !is_signal_eligible(target.pid, self_pid, self_ancestors) {
        return;
    }
    let Ok(cwd) = std::fs::read_link(format!("/proc/{}/cwd", target.pid)) else {
        return;
    };
    if !is_inside(&cwd, base) {
        return;
    }
    if pid_starttime(target.pid) != Some(target.starttime) {
        return;
    }
    send_signal(signal, target.pid);
}

/// Send `signal` to `pid` via the internal rustix syscall - the reaper's ONE sanctioned
/// signalling call (the `no-os-kill` gate, spec 78, excludes only this function and
/// `tests/common/mod.rs::terminate_pid` from its ban on shelling out to `kill`/`pkill`/
/// `killall` or calling a signal API directly). `pid > 1` is an EXPLICIT guard here too
/// (belt-and-braces alongside [`is_signal_eligible`]'s own check, since the pinned rustix's
/// `Pid::from_raw` accepts negative raw values and rejects only zero). Best-effort: ESRCH
/// (the process already exited) or any other failure (permission, ...) is silently ignored,
/// since the reap is teardown cleanup, never a hard error that could fail a worktree
/// removal or a step.
fn send_signal(signal: Signal, pid: u32) {
    if pid <= 1 {
        return;
    }
    let Ok(raw) = i32::try_from(pid) else {
        return;
    };
    let Some(rpid) = Pid::from_raw(raw) else {
        return;
    };
    let _ = rustix::process::kill_process(rpid, signal);
}

/// Validate that `base_dir` is a directory the reaper is authorized to touch (spec 78, THE
/// REAPER; spec 78 round-2 amendment, decision `u78c2r2-authorized-root-caller-supplied`):
/// it must canonicalize, exist, and lie STRICTLY under `authorized_root` (also
/// canonicalized) - a root the CALLER resolves and supplies, via the SAME authority it
/// already used to build `base_dir` itself, never re-derived here from `base_dir`'s own
/// git/filesystem position (a prior round did exactly that - a hardcoded `<repo>/.rigger/tmp`
/// literal resolved from `base_dir`'s own git context - and it silently turned every
/// production reap of a relocated scratch root, or the registered mutation-scratch root
/// under a cache home, into an unconditional no-op: neither can ever canonicalize under any
/// project's own `.rigger/tmp` by construction). Refused - logged, `None` - for: an
/// unresolvable `authorized_root`, `base_dir` equal to it, a nonexistent `base_dir` (fails
/// to canonicalize), or a symlink that canonicalizes outside it. Never widens, never falls
/// back - the caller must no-op on `None`.
fn is_reapable_base(base_dir: &Path, authorized_root: &Path) -> Option<PathBuf> {
    let refuse = |root_display: &str| {
        eprintln!(
            "rigger: reap refused: {} is not strictly under {root_display}",
            base_dir.display()
        );
        None
    };

    let Ok(root) = authorized_root.canonicalize() else {
        return refuse(&authorized_root.display().to_string());
    };
    let root_display = root.display().to_string();
    let Ok(base) = base_dir.canonicalize() else {
        return refuse(&root_display);
    };
    if base != root && base.starts_with(&root) {
        Some(base)
    } else {
        refuse(&root_display)
    }
}

/// Reap every process rooted STRICTLY inside `base_dir` before rigger removes that dir
/// (spec 23, unit 1; spec 78, THE REAPER): SIGTERM every match, wait a short grace for the
/// well-behaved to exit, then SIGKILL whatever is STILL rooted inside - so no process
/// outlives the worktree/scratch dir it ran in.
///
/// Gated by [`is_reapable_base`]: `base_dir` must canonicalize to somewhere STRICTLY under
/// `authorized_root` or this is a logged no-op that signals nothing - the boundary is
/// checked ONCE, up front, never re-derived per-pid. Once authorized, the actual
/// SIGTERM/grace/rescan/SIGKILL sequence is [`reap_authorized`] - the ONE termination
/// implementation this and [`crate::worktree::Worktree::remove`]'s own, independently
/// (git-identity) authorized reap both run.
///
/// Best-effort and platform-tolerant: where `/proc` is absent the scan finds nothing and
/// this is a graceful no-op.
pub fn reap_processes_rooted_under(base_dir: &Path, authorized_root: &Path) {
    let Some(base) = is_reapable_base(base_dir, authorized_root) else {
        return;
    };
    reap_authorized(base);
}

/// The reap's termination sequence for an ALREADY-AUTHORIZED `base` (SIGTERM every match,
/// wait a short grace, then SIGKILL whatever is STILL rooted inside) - `pub(crate)` so a
/// caller with its OWN independent authorization can reuse the ONE implementation rather
/// than a second, parallel one (the charter's "never a second parallel implementation
/// reconciled after the fact"). [`reap_processes_rooted_under`] is exactly
/// [`is_reapable_base`] then this; [`crate::worktree::Worktree::remove`] is the other
/// caller - a worktree's own dir can legitimately live anywhere relative to its repo
/// (`defaults.workdir`/`RIGGER_TMPDIR` relocation, tested in
/// `tests/scratch_workdir_config.rs`, with no necessary containment relationship to the
/// repo at all), so no `authorized_root` any caller could compute would reliably contain
/// it; it instead confirms `self.dir` IS a real, currently-checked-out git worktree of
/// `self.branch` (the same `worktree_on_branch` predicate `Worktree::create`'s own
/// fast-path adoption already trusts) before calling straight in here with the
/// already-canonicalized dir, bypassing [`is_reapable_base`]'s containment gate entirely.
///
/// Every candidate is [`is_signal_eligible`] (never pid <= 1, the reaper's own pid, or one
/// of its own ancestors) and TOCTOU-rechecked ([`signal_if_unchanged`]) immediately before
/// it is actually signalled, via [`send_signal`] - rigger's one sanctioned signal call.
///
/// The SIGKILL pass RE-SCANS rather than reusing the SIGTERM candidate list: a process that
/// already exited on SIGTERM is gone from the re-scan (so it is not signalled, closing a
/// pid-recycle window where its number was reused by an unrelated process outside the
/// base), and only what is genuinely still rooted inside gets its OWN fresh start-time
/// baseline and is force-killed - the TOCTOU guard holds for the SIGKILL pass too.
///
/// `base` is assumed already canonical (both callers canonicalize before calling in);
/// best-effort and platform-tolerant throughout - where `/proc` is absent the scan finds
/// nothing and this is a graceful no-op.
pub(crate) fn reap_authorized(base: PathBuf) {
    let self_pid = std::process::id();
    let self_ancestors = ancestor_pids(self_pid);

    let term_targets = scan_with_starttime(&base);
    if term_targets.is_empty() {
        return;
    }
    for target in &term_targets {
        signal_if_unchanged(target, &base, self_pid, &self_ancestors, Signal::TERM);
    }
    std::thread::sleep(GRACE);
    // Re-scan so only processes STILL rooted inside are force-killed - each gets its own
    // fresh start-time baseline here, then is rechecked again immediately below.
    for target in scan_with_starttime(&base) {
        signal_if_unchanged(&target, &base, self_pid, &self_ancestors, Signal::KILL);
    }
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
    use std::process::{Child, Command as StdCommand};

    /// A throwaway git repo with an empty `.rigger/tmp` created inside it, so
    /// `is_reapable_base` (and therefore `reap_processes_rooted_under`) accepts a dir under
    /// it - mirroring the real shape rigger's own `.rigger/tmp` lives in (spec 78).
    struct FakeRepo {
        _root: tempfile::TempDir,
        root_path: PathBuf,
        tmp: PathBuf,
    }

    impl FakeRepo {
        fn new() -> Self {
            let root = tempfile::tempdir().unwrap();
            let root_path = root.path().canonicalize().unwrap();
            let status = StdCommand::new("git")
                .arg("-C")
                .arg(&root_path)
                .args(["init", "-q"])
                .status()
                .expect("spawn git init");
            assert!(status.success(), "git init must succeed for the fixture");
            let tmp = root_path.join(".rigger").join("tmp");
            std::fs::create_dir_all(&tmp).unwrap();
            Self {
                _root: root,
                root_path,
                tmp,
            }
        }

        /// A fresh, existing subdir under `.rigger/tmp` - a VALID reapable base.
        fn base(&self, name: &str) -> PathBuf {
            let dir = self.tmp.join(name);
            std::fs::create_dir_all(&dir).unwrap();
            dir
        }
    }

    /// Spawn a long-lived `sleep` whose cwd is `dir`, so it appears in `/proc` rooted there.
    fn sleeper_in(dir: &Path) -> Child {
        StdCommand::new("sleep")
            .arg("300")
            .current_dir(dir)
            .spawn()
            .expect("spawn sleep")
    }

    /// Spawn a long-lived process rooted at `dir` that IGNORES SIGTERM, so only the SIGKILL
    /// escalation can reap it - exercising the full SIGTERM-then-SIGKILL mechanism.
    fn sigterm_ignorer_in(dir: &Path) -> Child {
        StdCommand::new("sh")
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

    /// Kill-and-wait a fixture child unconditionally, ignoring errors - test cleanup only,
    /// via the `Child` handle it was spawned with (never a computed pid).
    fn cleanup(child: &mut Child) {
        let _ = child.kill();
        let _ = child.wait();
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
        cleanup(&mut inside);
        cleanup(&mut outside);
        cleanup(&mut sib);

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
    fn is_inside_matches_on_components_not_string_prefix() {
        assert!(is_inside(Path::new("/a/b"), Path::new("/a/b")));
        assert!(is_inside(Path::new("/a/b/c"), Path::new("/a/b")));
        assert!(!is_inside(Path::new("/a/bc"), Path::new("/a/b")));
        assert!(!is_inside(Path::new("/a"), Path::new("/a/b")));
    }

    #[test]
    fn is_reapable_base_accepts_a_dir_strictly_under_the_given_authorized_root() {
        let repo = FakeRepo::new();
        let base = repo.base("rigger-wt-uexample");
        assert_eq!(
            is_reapable_base(&base, &repo.tmp),
            Some(base.canonicalize().unwrap())
        );
    }

    #[test]
    fn is_reapable_base_accepts_a_root_that_is_not_named_dot_rigger_tmp_at_all() {
        // The fix (spec 78 round 2, `u78c2r2-authorized-root-caller-supplied`): the boundary
        // is whatever `authorized_root` the caller supplies, never a hardcoded
        // `<repo>/.rigger/tmp` literal re-derived from `base_dir`'s own git context - so a
        // registered mutation-scratch root under a cache home (never nested under any
        // project's `.rigger/tmp`, and not even inside a git repo at all) is authorized just
        // as readily.
        let cache_home = tempfile::tempdir().unwrap();
        let mutation_root = cache_home.path().join("rigger-mutants");
        let base = mutation_root.join("some-spawn-id");
        std::fs::create_dir_all(&base).unwrap();
        assert_eq!(
            is_reapable_base(&base, &mutation_root),
            Some(base.canonicalize().unwrap())
        );
    }

    #[test]
    fn is_reapable_base_refuses_a_dir_that_is_not_under_the_given_authorized_root() {
        // The repo root is a real, existing dir - just not under `authorized_root` (here,
        // its own `.rigger/tmp` subdir).
        let repo = FakeRepo::new();
        assert_eq!(is_reapable_base(&repo.root_path, &repo.tmp), None);
    }

    #[test]
    fn is_reapable_base_refuses_the_authorized_root_itself() {
        let repo = FakeRepo::new();
        assert_eq!(is_reapable_base(&repo.tmp, &repo.tmp), None);
    }

    #[test]
    fn is_reapable_base_refuses_a_nonexistent_dir() {
        let repo = FakeRepo::new();
        let absent = repo.tmp.join("never-created");
        assert_eq!(is_reapable_base(&absent, &repo.tmp), None);
    }

    #[test]
    fn is_reapable_base_refuses_an_unresolvable_authorized_root() {
        // The authorized root itself is now caller-supplied, so an authorized_root that
        // cannot canonicalize (never created) must refuse too, not just an absent base_dir.
        let repo = FakeRepo::new();
        let base = repo.base("scratch");
        let never_created_root = repo.tmp.join("never-created-root");
        assert_eq!(is_reapable_base(&base, &never_created_root), None);
    }

    #[test]
    fn is_reapable_base_refuses_a_symlink_under_the_authorized_root_that_escapes_it() {
        let repo = FakeRepo::new();
        let outside = tempfile::tempdir().unwrap();
        let real_outside_target = outside.path().join("real-target");
        std::fs::create_dir_all(&real_outside_target).unwrap();
        let link = repo.tmp.join("escape-link");
        std::os::unix::fs::symlink(&real_outside_target, &link).unwrap();
        assert_eq!(is_reapable_base(&link, &repo.tmp), None);
    }

    #[test]
    fn reap_kills_a_sigterm_ignoring_child_inside_and_spares_one_outside() {
        // The teardown reap (spec 23, spec 78): a process rooted inside a VALID base is
        // reaped even when it IGNORES SIGTERM (the SIGKILL escalation after the grace does
        // it), while a process rooted OUTSIDE the base is left ALIVE.
        let repo = FakeRepo::new();
        let base = repo.base("scratch");

        let mut inside = sigterm_ignorer_in(&base);
        let mut outside = sleeper_in(&repo.root_path);

        assert!(
            wait_until(|| processes_rooted_under(&base)
                .iter()
                .any(|(pid, _)| *pid == inside.id())),
            "precondition: the inside child is detected before the reap"
        );

        reap_processes_rooted_under(&base, &repo.tmp);

        let inside_died = wait_for_exit(&mut inside);
        // The outside sleeper must still be running; capture before cleanup.
        let outside_alive = matches!(outside.try_wait(), Ok(None));

        cleanup(&mut outside);
        // Belt and braces: if the inside child somehow survived, do not leak it.
        if !inside_died {
            cleanup(&mut inside);
        }

        assert!(
            inside_died,
            "a SIGTERM-ignoring process rooted inside a valid base must be SIGKILLed"
        );
        assert!(
            outside_alive,
            "a process rooted OUTSIDE the base must survive the reap (safety boundary)"
        );
    }

    #[test]
    fn reap_kills_a_process_under_an_authorized_root_that_is_not_a_dot_rigger_tmp_tree() {
        // End-to-end proof of the fix at the public entry point: an authorized_root with NO
        // relationship whatsoever to any git repo or `.rigger/tmp` naming (mirroring a
        // registered mutation-scratch root under a cache home, or a `defaults.workdir`/
        // `RIGGER_TMPDIR`-relocated scratch root) still reaps a live, SIGTERM-ignoring
        // process rooted inside it.
        let root_dir = tempfile::tempdir().unwrap();
        let authorized_root = root_dir.path().join("relocated-scratch");
        std::fs::create_dir_all(&authorized_root).unwrap();
        let base = authorized_root.join("some-registered-leaf");
        std::fs::create_dir_all(&base).unwrap();

        let mut inside = sigterm_ignorer_in(&base);
        assert!(
            wait_until(|| processes_rooted_under(&base)
                .iter()
                .any(|(pid, _)| *pid == inside.id())),
            "precondition: the inside child is detected before the reap"
        );

        reap_processes_rooted_under(&base, &authorized_root);

        let inside_died = wait_for_exit(&mut inside);
        if !inside_died {
            cleanup(&mut inside);
        }
        assert!(
            inside_died,
            "a relocated/cache-home-style authorized_root with no .rigger/tmp relationship \
             must still authorize the reap"
        );
    }

    #[test]
    fn reap_is_a_graceful_no_op_when_nothing_is_rooted_inside() {
        // No process rooted inside a valid, empty base: the reap does nothing and never
        // errors, so teardown proceeds on any platform.
        let repo = FakeRepo::new();
        let base = repo.base("scratch");
        // A sleeper OUTSIDE the base must be untouched by a reap scoped to the empty base.
        let mut outside = sleeper_in(&repo.root_path);
        reap_processes_rooted_under(&base, &repo.tmp);
        let outside_alive = matches!(outside.try_wait(), Ok(None));
        cleanup(&mut outside);
        assert!(
            outside_alive,
            "an empty-base reap touches nothing outside it"
        );
    }

    #[test]
    fn reap_is_a_logged_no_op_for_a_base_refused_by_is_reapable_base() {
        // A base that is not strictly under the given authorized_root (here, the repo root
        // itself, checked against its own `.rigger/tmp`) must NEVER be reaped, even when a
        // process is genuinely rooted inside it - the base-guard is checked BEFORE any
        // scan/signal, never bypassed.
        let repo = FakeRepo::new();
        let mut rooted_at_repo_root = sleeper_in(&repo.root_path);
        assert!(wait_until(|| processes_rooted_under(&repo.root_path)
            .iter()
            .any(|(pid, _)| *pid == rooted_at_repo_root.id())));

        reap_processes_rooted_under(&repo.root_path, &repo.tmp);

        let still_alive = matches!(rooted_at_repo_root.try_wait(), Ok(None));
        cleanup(&mut rooted_at_repo_root);
        assert!(
            still_alive,
            "a refused base (the repo root itself) must never be reaped"
        );
    }

    #[test]
    fn reap_authorized_kills_a_sigterm_ignoring_process_given_an_already_authorized_base() {
        // [`reap_authorized`] is the termination sequence [`crate::worktree::Worktree::remove`]
        // calls directly after its OWN git-identity authorization (never through
        // `is_reapable_base`'s containment gate) - proves it independently performs the same
        // SIGTERM-then-grace-then-SIGKILL sequence given a bare, pre-canonicalized base.
        let root = tempfile::tempdir().unwrap();
        let base = root.path().canonicalize().unwrap();

        let mut inside = sigterm_ignorer_in(&base);
        assert!(wait_until(|| processes_rooted_under(&base)
            .iter()
            .any(|(pid, _)| *pid == inside.id())));

        reap_authorized(base);

        let inside_died = wait_for_exit(&mut inside);
        if !inside_died {
            cleanup(&mut inside);
        }
        assert!(
            inside_died,
            "reap_authorized must SIGKILL a SIGTERM-ignoring process given an authorized base, \
             with no containment gate in front of it"
        );
    }

    #[test]
    fn pid_starttime_is_stable_across_reads_for_a_live_process_and_none_for_a_bogus_pid() {
        let mut child = sleeper_in(Path::new("/"));
        let first = pid_starttime(child.id());
        let second = pid_starttime(child.id());
        cleanup(&mut child);
        assert!(first.is_some(), "a live process has a readable starttime");
        assert_eq!(first, second, "starttime is immutable for a live pid");
        assert_eq!(
            pid_starttime(u32::MAX),
            None,
            "no process exists at this pid"
        );
    }

    #[test]
    fn read_ppid_finds_the_spawning_process() {
        let mut child = sleeper_in(Path::new("/"));
        let ppid = read_ppid(child.id());
        cleanup(&mut child);
        assert_eq!(
            ppid,
            Some(std::process::id()),
            "the child's parent is this process"
        );
    }

    #[test]
    fn ancestor_pids_walks_through_an_intermediate_process() {
        // The test binary itself runs as pid 1 of its own namespace (`.cargo/pidns-runner.sh`),
        // so `ancestor_pids` on a DIRECT child would trivially stop at that pid (already
        // refused unconditionally elsewhere, so never separately recorded - see
        // `is_signal_eligible`). Spawn a GRANDCHILD instead (a shell that backgrounds a
        // `sleep` and prints its pid) so the walk must pass THROUGH a real intermediate
        // pid (the shell) to prove it is a multi-hop walk, not a single `read_ppid` call.
        let mut shell = StdCommand::new("sh")
            .arg("-c")
            .arg("sleep 300 & echo $!; wait")
            .current_dir(Path::new("/"))
            .stdout(std::process::Stdio::piped())
            .spawn()
            .expect("spawn shell with a backgrounded grandchild");
        let stdout = shell.stdout.take().expect("piped stdout");
        let mut line = String::new();
        std::io::BufRead::read_line(&mut std::io::BufReader::new(stdout), &mut line)
            .expect("read the grandchild's pid line");
        let grandchild_pid: u32 = line.trim().parse().expect("parse the grandchild pid");

        let ancestors = ancestor_pids(grandchild_pid);
        let shell_pid = shell.id();
        cleanup(&mut shell);

        assert!(
            ancestors.contains(&shell_pid),
            "ancestor_pids must walk THROUGH the intermediate shell to find it: \
             {ancestors:?} (shell pid {shell_pid})"
        );
    }

    #[test]
    fn is_signal_eligible_refuses_pid_zero_and_one_self_and_ancestors() {
        let mut ancestors = HashSet::new();
        ancestors.insert(500);
        assert!(!is_signal_eligible(0, 999, &ancestors), "pid 0");
        assert!(!is_signal_eligible(1, 999, &ancestors), "pid 1 (init)");
        assert!(!is_signal_eligible(999, 999, &ancestors), "self");
        assert!(!is_signal_eligible(500, 999, &ancestors), "an ancestor");
        assert!(is_signal_eligible(501, 999, &ancestors), "an eligible pid");
    }

    #[test]
    fn signal_if_unchanged_signals_when_eligible_and_everything_matches() {
        // Positive control: proves the happy path actually reaches `send_signal`.
        let repo = FakeRepo::new();
        let base = repo.base("scratch");
        let mut child = sleeper_in(&base);
        let starttime = wait_until(|| pid_starttime(child.id()).is_some());
        assert!(starttime, "precondition: starttime is readable");
        let target = ScanEntry {
            pid: child.id(),
            starttime: pid_starttime(child.id()).unwrap(),
        };
        signal_if_unchanged(
            &target,
            &base,
            std::process::id(),
            &HashSet::new(),
            Signal::KILL,
        );
        let died = wait_for_exit(&mut child);
        if !died {
            cleanup(&mut child);
        }
        assert!(died, "an eligible, matching target must be signalled");
    }

    #[test]
    fn signal_if_unchanged_skips_a_starttime_mismatch() {
        // The TOCTOU guard (spec 78): even though the pid and cwd both genuinely match, a
        // starttime that no longer matches the scan's recorded value means the scan's
        // identity is stale - skip rather than signal.
        let repo = FakeRepo::new();
        let base = repo.base("scratch");
        let mut child = sleeper_in(&base);
        let real_starttime = wait_until(|| pid_starttime(child.id()).is_some());
        assert!(real_starttime);
        let wrong = ScanEntry {
            pid: child.id(),
            starttime: pid_starttime(child.id()).unwrap().wrapping_add(1),
        };
        signal_if_unchanged(
            &wrong,
            &base,
            std::process::id(),
            &HashSet::new(),
            Signal::KILL,
        );
        let still_alive = matches!(child.try_wait(), Ok(None));
        cleanup(&mut child);
        assert!(
            still_alive,
            "a starttime mismatch must be skipped, never signalled"
        );
    }

    #[test]
    fn signal_if_unchanged_skips_when_cwd_is_outside_the_given_base() {
        // The pid and starttime both genuinely match, but the base passed in does not
        // contain the process's cwd - must be skipped (mirrors "cwd changed" between scan
        // and signal: from this call's point of view, it no longer matches).
        let repo = FakeRepo::new();
        let base = repo.base("scratch");
        let other_base = repo.base("unrelated");
        let mut child = sleeper_in(&base);
        let ready = wait_until(|| pid_starttime(child.id()).is_some());
        assert!(ready);
        let target = ScanEntry {
            pid: child.id(),
            starttime: pid_starttime(child.id()).unwrap(),
        };
        signal_if_unchanged(
            &target,
            &other_base,
            std::process::id(),
            &HashSet::new(),
            Signal::KILL,
        );
        let still_alive = matches!(child.try_wait(), Ok(None));
        cleanup(&mut child);
        assert!(
            still_alive,
            "a cwd outside the given base must be skipped, never signalled"
        );
    }

    #[test]
    fn signal_if_unchanged_skips_when_pid_is_marked_as_self_or_an_ancestor() {
        let repo = FakeRepo::new();
        let base = repo.base("scratch");
        let mut child = sleeper_in(&base);
        let ready = wait_until(|| pid_starttime(child.id()).is_some());
        assert!(ready);
        let target = ScanEntry {
            pid: child.id(),
            starttime: pid_starttime(child.id()).unwrap(),
        };
        // Pretend the child IS "self" - it must never be signalled.
        signal_if_unchanged(&target, &base, child.id(), &HashSet::new(), Signal::KILL);
        let alive_as_self = matches!(child.try_wait(), Ok(None));
        assert!(alive_as_self, "a pid equal to self_pid must be skipped");

        // Pretend the child is a recorded ancestor - it must never be signalled.
        let mut ancestors = HashSet::new();
        ancestors.insert(child.id());
        signal_if_unchanged(&target, &base, std::process::id(), &ancestors, Signal::KILL);
        let alive_as_ancestor = matches!(child.try_wait(), Ok(None));
        cleanup(&mut child);
        assert!(
            alive_as_ancestor,
            "a pid recorded as an ancestor of self must be skipped"
        );
    }
}
