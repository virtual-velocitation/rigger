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
//! Best-effort and platform-tolerant. Every fact the reaper reads about a process - the process
//! list, a process's cwd, command, parent and start time - comes through the process interface
//! ([`crate::os_process`]); on an OS that interface cannot read, detection is a graceful no-op
//! returning empty, NEVER a hard error, so teardown and validate keep working on any platform.
//! Windows reports no process's working directory, so there the cwd scan finds nothing.
//!
//! SAFETY BOUNDARY (load-bearing, spec 23 + spec 78): the scan matches ONLY a process whose
//! canonicalized cwd equals the base dir or lies strictly under it (`<base>/...`), by path
//! COMPONENTS, never a raw string prefix - so a sibling dir whose path merely shares a
//! string prefix (`<base>-x`) is never matched. On TOP of that, [`reap_processes_rooted_under`]
//! additionally requires the base itself to RESOLVE (lexically, [`resolve_lexically`] -
//! existence not required, see below) to somewhere STRICTLY under an `authorized_root` the
//! CALLER supplies ([`is_reapable_base`]) - so a caller that ever computed a wrong or
//! widened base relative to the root it meant to reap under gets a logged no-op instead of a
//! kill. `authorized_root` is never re-derived here (no hardcoded
//! `<repo>/.rigger/tmp` literal, no git resolution of the caller's repo): the caller passes
//! the SAME resolved root it already used to build `base_dir` itself
//! ([`crate::worktree::scratch_root_path_from_env`] for the run's own scratch tree, or any
//! other registered scratch root, wherever it is placed) - so the boundary can never silently
//! diverge
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
//! THE GUARD COMPARES PATHS, NOT EXISTENCE (spec 89 criterion 3): [`is_reapable_base`] and
//! [`processes_rooted_under`] both resolve `base_dir` via [`resolve_lexically`] rather than
//! requiring it to exist. A `base_dir` that resolves strictly under `authorized_root` but no
//! longer exists is ALREADY RECLAIMED - authorized exactly like a live one, never a logged
//! refusal (a spawn's own scratch dir that was never created would otherwise fail
//! `base_dir.canonicalize()` and log a false "not strictly under" refusal on every single
//! `rigger result`). And a process can hold a now-DELETED dir as its cwd, which the process
//! interface names by that dir's path ([`crate::os_process::cwd`]), so it still lies in the base
//! ([`crate::os_process::cwd_lies_in`]): a test binary that loops after the tool that built its
//! scratch tree removed that tree out from under it holds exactly such a cwd, and a cwd-rooted
//! reaper that never matched the deleted path would never even try to signal it. A genuinely
//! OUTSIDE `base_dir` is still refused, whether or not it exists.
//!
//! SIGNAL API (spec 78): every signal rigger issues to a process it does not hold a
//! [`std::process::Child`] handle to goes through [`send_signal`], whose unix arm is
//! `rustix::process::kill_process` - never a shell-out to `kill(1)`, never `libc::kill`, never
//! a process-group (negative pid) target - whose Windows arm opens the one pid for termination
//! and ends it, and whose arm for any other OS sends nothing.
//! `rustix::process::Pid::from_raw` accepts negative raw values (it rejects only zero), so
//! [`send_signal`] and [`is_signal_eligible`] each carry their OWN explicit `pid > 1` guard
//! rather than leaning on the type to refuse one. `default-features = false` + `std` +
//! `process` selects rustix's linux_raw backend, so the `--no-default-features` build pulls
//! no `libc` crate edge and reaps identically.
//!
//! TOCTOU (spec 78): the scan and the signal are not atomic - a pid can exit and be
//! recycled by the kernel onto an unrelated process in the gap between them. Every
//! candidate's start time ([`crate::os_process::start_time`], the same for a pid's whole life)
//! is captured at scan time and re-read, alongside its cwd, IMMEDIATELY before it
//! is actually signalled; either differing skips the signal (spec 78, [`signal_if_unchanged`]).
//! [`reap_processes_rooted_under`] also never signals pid 0 or 1, its own pid, or any
//! ancestor of its own process (the reaper's launching shell/session, walked up the parent
//! chain [`crate::os_process::parent`] reads, [`ancestor_pids`]) - so a coincidence or a
//! recycled pid can never reach upward into the process tree running rigger itself.

use crate::os_process;
use std::collections::{HashMap, HashSet, VecDeque};
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
/// Best-effort, over the process interface ([`crate::os_process`]). `base_dir` is resolved
/// LEXICALLY ([`resolve_lexically`], spec 89 criterion 3) rather than required to exist - a
/// live process can hold a now-deleted dir as its cwd, and this scan must still find it.
/// Returns EMPTY - a graceful no-op, never an error - when no ancestor of `base_dir` at all can
/// be resolved (practically unreachable) or the interface lists no process (an OS it cannot
/// read), so teardown and validate work anywhere.
///
/// Containment is RESOLVED-PATH STRICT-INSIDE, matched on path components: a process is
/// returned iff its cwd as the interface names it ([`crate::os_process::cwd`]: absolute,
/// resolved, and named by its path even once removed) lies in the resolved `base_dir`
/// ([`crate::os_process::cwd_lies_in`]): equals it or starts with it as a path prefix.
/// Component matching (not string prefix) is the load-bearing safety boundary -
/// `<base>-sibling` shares a string prefix with `<base>` but is a different component and is
/// never matched, so a process outside the exact dir is never reaped. The scanning process
/// itself is excluded (rigger never reaps its own pid). Each
/// match's command ([`crate::os_process::command`]) is purely descriptive: it names the leak in
/// the `rigger validate` advisory and never affects which processes are reaped.
pub fn processes_rooted_under(base_dir: &Path) -> Vec<(u32, String)> {
    // Resolve the base LEXICALLY (spec 89 criterion 3), not by requiring it to exist: a
    // process can hold a now-DELETED dir as its cwd (a test binary still looping after the
    // tool that built its scratch tree removed that tree out from under it), and such a
    // process is exactly what this scan must still find. `resolve_lexically` follows
    // symlinks in whatever portion of `base_dir` still exists (so the resolved cwd of a LIVE
    // process still matches a symlinked component), then lexically resolves any missing
    // suffix - it fails only in the practically-unreachable case where no ancestor at all can
    // be resolved.
    let Some(base) = resolve_lexically(base_dir) else {
        return Vec::new();
    };
    let self_pid = std::process::id();
    // Never reap the scanning process itself. A cwd that cannot be read (the process exited
    // since the listing, or it belongs to another user) is simply skipped - best-effort.
    os_process::pids()
        .into_iter()
        .filter(|&pid| pid != self_pid)
        .filter(|&pid| os_process::cwd_lies_in(pid, &base))
        .map(|pid| (pid, os_process::command(pid)))
        .collect()
}

/// One process as a scan or a walk found it: the pid and its start time
/// ([`crate::os_process::start_time`]), the same for the life of a pid and repeated only if the
/// pid itself is reused, so comparing it again right before the signal is the TOCTOU witness
/// that the pid still names the SAME process the scan found, not one the kernel recycled onto
/// that number in the meantime (spec 78).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Identity {
    /// The process's pid.
    pub pid: u32,
    /// Its start time when the pid was read, the witness [`still_started`] checks.
    pub start: u64,
}

/// [`processes_rooted_under`] (the one scan authority) paired with each match's start time
/// captured at this same instant, for the reaper's TOCTOU recheck. A pid whose start time
/// cannot be read (it already exited between the cwd scan and this read) is dropped -
/// nothing to compare against later, nothing worth signalling now.
fn scan_with_starttime(base: &Path) -> Vec<Identity> {
    processes_rooted_under(base)
        .into_iter()
        .filter_map(|(pid, _)| pid_starttime(pid).map(|start| Identity { pid, start }))
        .collect()
}

/// The Linux process-stat field parser, re-exported for the Linux keep-awake thread test, which
/// reads a thread's `flags` field through it.
#[cfg(target_os = "linux")]
pub use crate::os_process::linux::stat_field_after_comm;

/// The process start time ([`crate::os_process::start_time`]) - the TOCTOU witness
/// [`scan_with_starttime`] records and [`signal_if_unchanged`] re-reads immediately before
/// signalling. `None` when the process has already exited or its start time cannot be read.
/// Public so a test that owns a process it did not spawn reads the witness it later checks
/// through [`still_started`].
pub fn pid_starttime(pid: u32) -> Option<u64> {
    os_process::start_time(pid)
}

/// Whether `pid` still carries the start time `started` it was read with ([`pid_starttime`]):
/// the one witness check that refuses a pid that has exited, or was recycled onto another process,
/// since it was read. Public so a test owning a process it did not spawn signals it only while
/// this holds.
pub fn still_started(pid: u32, started: u64) -> bool {
    pid_starttime(pid) == Some(started)
}

/// Every ancestor pid of `pid`, walking the parent chain ([`crate::os_process::parent`]) upward
/// (spec 78): the reaper's own launching shell, terminal, session leader, and so on up to (but
/// excluding, since it is refused unconditionally elsewhere) init. Used to refuse ever
/// signalling anything on the REAPER's own lineage, however a computed or recycled pid
/// might coincide with one. Best-effort: the walk stops at a pid <= 1, an unreadable
/// parent, or a repeat (a cycle, which the OS should never report, but the walk must
/// terminate regardless).
fn ancestor_pids(pid: u32) -> HashSet<u32> {
    let mut out = HashSet::new();
    let mut current = pid;
    while let Some(parent) = os_process::parent(current) {
        if parent <= 1 || !out.insert(parent) {
            break;
        }
        current = parent;
    }
    out
}

/// Every descendant of `root_pid` - its children, their children, and so on - snapshotted as
/// `(pid, starttime)` pairs at the moment of the call (spec 104 criterion 6 round-4 fix,
/// decision `op-104-stop-end-the-tree-and-bound-the-joins`): [`end_child`]'s counterpart to
/// [`ancestor_pids`]'s upward walk, but downward, so a process the held child FORKED but
/// never `exec`'d (and so never became the held child itself) - one that inherited a stdout
/// or stderr pipe write end before going silent - is still found and can still be ended,
/// even though [`end_child`] only ever held a [`std::process::Child`] handle to its direct
/// parent.
///
/// The process interface gives every process its OWN parent, never a parent's list of children,
/// so the walk reads ONE process table ([`crate::os_process::table`]: each process's parent and
/// start time from a single listing) and walks it in [`descendants_in`]. Matched by PID-TREE
/// membership alone, deliberately NEVER by working directory: a live sibling spawn's own
/// process can share this same worktree as its cwd without being anywhere in this child's
/// process tree (`op-104-stop-no-sweep-at-wall-clock-stop`'s sibling-safety guarantee, which
/// this walk must never reopen), so it can never appear here regardless of what it shares on
/// disk.
///
/// Called ONCE, before the FIRST signal [`end_child`] sends: a process reparents to its
/// nearest surviving ancestor the instant its own parent exits, so a walk taken any later
/// risks missing a descendant the kernel has already handed off elsewhere - the walk must
/// see the tree as it stood before any of this ending began. Each entry carries the start time
/// the table read for the SAME TOCTOU recheck [`scan_with_starttime`] already gives the
/// cwd-scanned reap - a descendant that has since exited, or whose pid the kernel has since
/// recycled onto an unrelated process, is refused by that recheck immediately before either
/// signal.
///
/// Best-effort and platform-tolerant like every other scan in this module: an OS the process
/// interface cannot read, or a process the listing could not read, contributes nothing, never a
/// hard error.
fn descendants_of(root_pid: u32) -> Vec<Identity> {
    descendants_in(&os_process::table(), root_pid)
}

/// The descendants of `root_pid` in `table`, children before grandchildren and each generation in
/// the table's order: empty when the table does not list `root_pid`. A process is kept only if it
/// started no earlier than the parent it names, because a Windows entry keeps a dead parent's pid
/// and Windows may give that pid to a later process; a process failing that test is no descendant,
/// and nor is anything below it. `seen` (the root included) ends the walk on a cycle, which the
/// OS's own parent/child relationship should never produce but the walk must survive.
fn descendants_in(table: &[os_process::Entry], root_pid: u32) -> Vec<Identity> {
    let Some(root) = table.iter().find(|entry| entry.pid == root_pid) else {
        return Vec::new();
    };
    let mut children_of: HashMap<u32, Vec<&os_process::Entry>> = HashMap::new();
    for entry in table {
        children_of.entry(entry.parent).or_default().push(entry);
    }
    let mut out = Vec::new();
    let mut seen: HashSet<u32> = HashSet::from([root_pid]);
    let mut frontier = VecDeque::from([root]);
    while let Some(parent) = frontier.pop_front() {
        for &kid in children_of.get(&parent.pid).into_iter().flatten() {
            if kid.start < parent.start || !seen.insert(kid.pid) {
                continue;
            }
            out.push(Identity {
                pid: kid.pid,
                start: kid.start,
            });
            frontier.push_back(kid);
        }
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

/// Whether `target` is still safe to signal RIGHT NOW, independent of any cwd/base match:
/// [`is_signal_eligible`] AND its start time, re-read this instant, still matches what the
/// scan recorded. This is the pid-identity half of the TOCTOU guard (spec 78) - a pid that
/// exited and was recycled onto an unrelated process in the gap since the scan reads a
/// DIFFERENT start time and is refused - shared by both the cwd-scanned signal path
/// ([`signal_if_unchanged`]) and the pid-tree-scanned one
/// ([`signal_descendant_if_unchanged`]), so there is exactly ONE TOCTOU recheck regardless
/// of which kind of snapshot found the candidate.
fn toctou_still_same_process(
    target: &Identity,
    self_pid: u32,
    self_ancestors: &HashSet<u32>,
) -> bool {
    is_signal_eligible(target.pid, self_pid, self_ancestors)
        && still_started(target.pid, target.start)
}

/// Re-read `target`'s cwd ([`crate::os_process::cwd`]) IMMEDIATELY before signalling it, and
/// signal only if it still lies in `base` ([`crate::os_process::cwd_lies_in`]) AND
/// [`toctou_still_same_process`]. This is the cwd half of the TOCTOU guard (spec 78): a pid
/// that exited and was recycled onto an unrelated process (even one that happens to also be
/// rooted under `base`) between the scan and this call is silently skipped, never signalled,
/// and so is a pid whose cwd cannot be read.
fn signal_if_unchanged(
    target: &Identity,
    base: &Path,
    self_pid: u32,
    self_ancestors: &HashSet<u32>,
    signal: Signal,
) {
    if !os_process::cwd_lies_in(target.pid, base) {
        return;
    }
    if toctou_still_same_process(target, self_pid, self_ancestors) {
        send_signal(signal, target.pid);
    }
}

/// Signal a descendant [`descendants_of`]'s PID-TREE walk found, after the SAME TOCTOU
/// recheck [`signal_if_unchanged`]'s own cwd-scanned callers already get
/// ([`toctou_still_same_process`]) - but deliberately NO cwd/base check at all (spec 104
/// criterion 6 round-4 fix): this candidate was matched by process-tree membership, not by
/// working directory, so a descendant that has since `chdir`'d somewhere else is still
/// correctly signalled - only a RECYCLED pid (the kernel reused the number for an unrelated
/// process) is skipped, exactly as for any other reap candidate.
fn signal_descendant_if_unchanged(
    target: &Identity,
    self_pid: u32,
    self_ancestors: &HashSet<u32>,
    signal: Signal,
) {
    if toctou_still_same_process(target, self_pid, self_ancestors) {
        send_signal(signal, target.pid);
    }
}

/// The two ways the reaper ends a process: [`Signal::Term`] asks it to exit, and
/// [`Signal::Hard`] ends it without asking. Windows has no request to exit that an arbitrary
/// process can catch, so there both end the process at once.
#[derive(Clone, Copy)]
enum Signal {
    /// Ask the process to exit (`SIGTERM` on unix).
    Term,
    /// End the process without asking (`SIGKILL` on unix).
    Hard,
}

/// Send `signal` to `pid` - the reaper's ONE sanctioned signalling call (the `no-os-kill`
/// gate, spec 78, exempts only this function and `tests/common/mod.rs::terminate_pid` from
/// its ban on calling a signal API directly, never from its ban on shelling out to a process
/// termination utility, and this function alone from its ban on the Windows
/// process-termination call): on unix the internal rustix syscall, on Windows the process
/// opened for termination and ended with exit code 1, on any other OS nothing. `pid > 1` is an
/// EXPLICIT guard here too (belt-and-braces alongside [`is_signal_eligible`]'s own check, since
/// the pinned rustix's `Pid::from_raw` accepts negative raw values and rejects only zero).
/// Best-effort: ESRCH (the process already exited) or any other failure (permission, ...) is
/// silently ignored, since the reap is teardown cleanup, never a hard error that could fail a
/// worktree removal or a step.
fn send_signal(signal: Signal, pid: u32) {
    if pid <= 1 {
        return;
    }
    std::cfg_select! {
        unix => {
            use rustix::process::Pid;
            let Ok(raw) = i32::try_from(pid) else {
                return;
            };
            let Some(rpid) = Pid::from_raw(raw) else {
                return;
            };
            let signal = match signal {
                Signal::Term => rustix::process::Signal::TERM,
                Signal::Hard => rustix::process::Signal::KILL,
            };
            let _ = rustix::process::kill_process(rpid, signal);
        }
        target_os = "windows" => {
            use windows_sys::Win32::Foundation::{CloseHandle, FALSE};
            use windows_sys::Win32::System::Threading::{
                OpenProcess, TerminateProcess, PROCESS_TERMINATE,
            };
            // Both signals end the process: see [`Signal`].
            let _ = signal;
            // SAFETY: the call takes no pointer and answers a null handle on failure.
            let handle = unsafe { OpenProcess(PROCESS_TERMINATE, FALSE, pid) };
            if handle.is_null() {
                return;
            }
            // SAFETY: the handle was opened above with termination access, and is closed once.
            unsafe {
                TerminateProcess(handle, 1);
                CloseHandle(handle);
            }
        }
        _ => {
            let _ = signal;
        }
    }
}

/// An OPAQUE snapshot of a process's descendants, taken by [`snapshot_descendants`] and
/// spent by [`end_child`] (spec 104 criterion 6 round-4 fix, decision
/// `op-104-stop-end-the-tree-and-bound-the-joins`). A SEPARATE snapshot step, rather than
/// [`end_child`] always taking its own fresh one internally, exists so a caller that must
/// give the child its OWN passive period to exit UNSIGNALLED BEFORE ever calling
/// [`end_child`] at all (THE STOP, `driver::claude_code::Driver::stop_for_wall_clock_silence`:
/// it closes the session's input, then waits `stop_grace` for the agent to notice and exit
/// gracefully, with no signal sent at all, before ever ending it) can still capture the
/// descendant tree at the CORRECT moment - before that passive wait, never after: a
/// descendant reparents to its nearest surviving ancestor the INSTANT its own parent exits,
/// gracefully or not, so a snapshot taken only once that wait has already ended (or taken
/// only inside [`end_child`] itself, which is not even called until after it) would already
/// be too late for a session that wound down entirely on its own.
pub struct DescendantSnapshot(Vec<Identity>);

/// Snapshot `pid`'s current descendants (see [`descendants_of`]) into an opaque
/// [`DescendantSnapshot`] a caller can hold across its OWN later wait before spending it via
/// [`end_child`]. A caller with no such earlier wait of its own can snapshot immediately
/// before calling [`end_child`] - there is no self-snapshotting shortcut, so every caller's
/// snapshot timing is visible at its own call site rather than hidden inside this module.
pub fn snapshot_descendants(pid: u32) -> DescendantSnapshot {
    DescendantSnapshot(descendants_of(pid))
}

/// End a process rigger still holds a live [`std::process::Child`] handle to, AND every
/// descendant it forked but never `exec`'d (spec 104 criterion 6, STOP;
/// `docs/architecture-addendum-claude-code-integration.md` §4.6; round-4 fix, decision
/// `op-104-stop-end-the-tree-and-bound-the-joins`): SIGTERM the child, wait [`GRACE`] for it
/// to exit on its own, then finish it via [`send_signal`] (SIGKILL) if it has not - the
/// crate's ONE implementation of "end a held child," never a second, parallel one. This
/// promotes the tests-only `cleanup()` fixture helper's pattern (same file, above) into
/// production code, running the identical TERM-then-grace-then-KILL escalation
/// [`reap_authorized`] already runs for a cwd-SCANNED base (via [`signal_if_unchanged`]), but
/// keyed on a HELD HANDLE instead: the `pid` [`send_signal`] receives for the child is read
/// directly off the live `child` the caller still owns (`child.id()`), never a marker, a
/// pidfile, or a process scan (never a COMPUTED pid) - so this stays the handle-bound
/// counterpart to [`send_signal`]'s cwd-scanned production callers, and the escalation
/// deliberately does NOT call the standard library's own `Child::kill`, which would be a
/// second, `no-os-kill`-gate-invisible signalling path (see decision
/// `sdet-u104stop-endchild-bypasses-send-signal`).
///
/// `descendants` (an opaque [`DescendantSnapshot`] from [`snapshot_descendants`], taken by
/// the CALLER at whatever moment is correct for it - see that function's own doc) then get
/// the identical TERM-then-grace-then-KILL escalation, each signal going through
/// [`signal_descendant_if_unchanged`]'s own TOCTOU recheck. `child` may have ALREADY exited
/// by the time this is called (a caller's own earlier passive wait may already have
/// collected it) - the child itself then needs nothing further, but `descendants` (captured
/// by the caller while it was still alive) still might, so `descendants` is always processed
/// regardless of the child's own state; only a `child` ALREADY exited on entry with an EMPTY
/// snapshot is a true no-op, nothing left to end at all. Each is matched by PID-TREE
/// membership alone, deliberately never by cwd: a live SIBLING spawn's process can share
/// this held child's worktree as its cwd without being anywhere in ITS process tree, so this
/// can end the held child's own leftovers without ever touching a sibling's legitimate work
/// (`op-104-stop-no-sweep-at-wall-clock-stop`) - the exact hazard a cwd sweep here would
/// reopen. `descendants` is never rescanned once the direct child is gone: any real
/// descendant has already reparented away from it by then, so a rescan rooted at its pid
/// would find nothing at all - each entry is simply rechecked immediately before its own
/// TERM, then again before its own KILL.
pub fn end_child(child: &mut std::process::Child, descendants: DescendantSnapshot) {
    let descendants = descendants.0;
    if !matches!(child.try_wait(), Ok(Some(_))) {
        send_signal(Signal::Term, child.id());
        let deadline = std::time::Instant::now() + GRACE;
        loop {
            if matches!(child.try_wait(), Ok(Some(_))) {
                break;
            }
            if std::time::Instant::now() >= deadline {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        // Still alive past the grace: finish it through send_signal (SIGKILL) - the
        // crate's ONE signalling call, the same one reap_authorized's own KILL escalation
        // already uses two functions above via signal_if_unchanged - then collect it so no
        // zombie survives this function.
        if !matches!(child.try_wait(), Ok(Some(_))) {
            send_signal(Signal::Hard, child.id());
        }
        let _ = child.wait();
    }

    // The descendants, from whichever snapshot the caller took - TERM every one, wait the
    // SAME grace period (a flat wait, mirroring reap_authorized's own TERM-then-grace-then-
    // KILL shape for a cwd-scanned base - no early-exit poll needed here, since this is
    // best-effort teardown, not the caller-visible path the child's own grace loop above
    // already keeps fast), then KILL whatever is still alive.
    if !descendants.is_empty() {
        let self_pid = std::process::id();
        let self_ancestors = ancestor_pids(self_pid);
        for d in &descendants {
            signal_descendant_if_unchanged(d, self_pid, &self_ancestors, Signal::Term);
        }
        std::thread::sleep(GRACE);
        for d in &descendants {
            signal_descendant_if_unchanged(d, self_pid, &self_ancestors, Signal::Hard);
        }
    }
}

/// Validate that `base_dir` is a directory the reaper is authorized to touch (spec 78, THE
/// REAPER; spec 78 round-2 amendment, decision `u78c2r2-authorized-root-caller-supplied`;
/// spec 89 criterion 3, THE RECLAIM GUARD COMPARES PATHS): `authorized_root` must
/// canonicalize (it is the CALLER's own already-resolved, persistent root - a root the
/// caller resolves and supplies, via the SAME authority it already used to build `base_dir`
/// itself, never re-derived here from `base_dir`'s own git/filesystem position); `base_dir`
/// is resolved LEXICALLY ([`resolve_lexically`]) rather than required to exist, and must lie
/// STRICTLY under the canonicalized root. Refused - logged, `None` - for: an unresolvable
/// `authorized_root`, `base_dir` equal to it, or a `base_dir` that resolves outside it
/// (including via a symlink in whatever portion of it exists). A `base_dir` that resolves
/// STRICTLY UNDER the root but no longer exists is ALREADY RECLAIMED - authorized (`Some`)
/// exactly like a live one, never a logged refusal: a spawn's own scratch dir that was never
/// created would otherwise fail `base_dir.canonicalize()` and log a false "not strictly
/// under" refusal on EVERY `rigger result`, and a base already removed out from under a
/// still-running process (a looping test binary whose scratch tree is gone) must stay
/// authorized so [`processes_rooted_under`]'s deleted-cwd match can
/// still find and reap it. Never widens the boundary itself, never falls back on a genuine
/// escape - the caller must no-op on `None`.
fn is_reapable_base(base_dir: &Path, authorized_root: &Path) -> Option<PathBuf> {
    let refuse = |root_display: &str| {
        crate::whole_line::say(format_args!(
            "rigger: reap refused: {} is not strictly under {root_display}",
            base_dir.display()
        ));
        None
    };

    let Ok(root) = authorized_root.canonicalize() else {
        return refuse(&authorized_root.display().to_string());
    };
    let root_display = root.display().to_string();
    let Some(base) = resolve_lexically(base_dir) else {
        return refuse(&root_display);
    };
    if base != root && base.starts_with(&root) {
        Some(base)
    } else {
        refuse(&root_display)
    }
}

/// Resolve `path` as far as the filesystem allows WITHOUT requiring it to exist (spec 89
/// criterion 3, THE RECLAIM GUARD COMPARES PATHS - "normalizes the joined path lexically"):
/// canonicalize the longest existing ancestor - so a symlink anywhere in the portion that
/// DOES exist is still followed, preserving the escape-detection guarantee
/// [`is_reapable_base`]'s callers rely on for whatever part of the path is actually there
/// today - then lexically re-append whatever suffix does not exist, resolving any `..` in it
/// by plain path-component arithmetic (never touching the filesystem for a component that
/// is not there to canonicalize; `Path`'s own component parser already normalizes away
/// interior `.` segments). Returns `None` only if NO ancestor at all can be canonicalized,
/// which does not happen in practice - the filesystem root always resolves.
fn resolve_lexically(path: &Path) -> Option<PathBuf> {
    let mut missing = Vec::new();
    let mut current = path;
    loop {
        if let Ok(mut resolved) = current.canonicalize() {
            for component in missing.into_iter().rev() {
                match component {
                    std::path::Component::ParentDir => {
                        resolved.pop();
                    }
                    std::path::Component::Normal(name) => resolved.push(name),
                    // `CurDir`, `RootDir` and `Prefix` never occur in the MISSING suffix we
                    // collect here - each pushed component came from stripping ONE trailing
                    // component off `current` (never the root itself, which always
                    // canonicalizes and ends the loop above before falling through here).
                    _ => {}
                }
            }
            return Some(resolved);
        }
        let component = current.components().next_back()?;
        missing.push(component);
        current = current.parent()?;
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
/// Best-effort and platform-tolerant: on an OS the process interface cannot read the scan
/// finds nothing and this is a graceful no-op.
pub fn reap_processes_rooted_under(base_dir: &Path, authorized_root: &Path) {
    let Some(base) = is_reapable_base(base_dir, authorized_root) else {
        return;
    };
    reap_authorized(base);
}

/// Reap any process rooted in `dir` (spec 23), then remove the dir. The reap runs BEFORE the
/// removal so no process outlives the dir holding a now-deleted cwd; both halves are
/// best-effort and never fail the caller. `authorized_root` (spec 78 round 2, decision
/// `u78c2r2-authorized-root-caller-supplied`) is the SAME resolved root the caller already
/// used to build `dir` - never re-derived here - so this reap is safe on any relocated
/// scratch root (`RIGGER_TMPDIR`/`defaults.workdir`) or registered scratch root under a cache
/// home, and still never touches a process outside `authorized_root`. On an OS the process
/// interface cannot read the reap is a graceful no-op and only the removal runs. The shared
/// teardown for every scratch area rigger removes - the binary's sweeps and the per-spawn
/// reclaim the driver crate owns (`driver::replay::reclaim_spawn_registered_scratch`) - so
/// both crates reach the ONE reap-then-remove; the worktree-removal reap point is
/// [`crate::worktree::Worktree::remove`], which authorizes its own reap differently (git
/// identity, not containment - see its own doc comment).
pub fn reap_then_remove_dir(dir: &Path, authorized_root: &Path) {
    reap_processes_rooted_under(dir, authorized_root);
    let _ = std::fs::remove_dir_all(dir);
}

/// The reap's termination sequence for an ALREADY-AUTHORIZED `base` (SIGTERM every match,
/// wait a short grace, then SIGKILL whatever is STILL rooted inside) - `pub` so a
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
/// `base` is assumed already resolved (both callers resolve before calling in - one via
/// [`is_reapable_base`]'s [`resolve_lexically`], the other via `Worktree::remove`'s own
/// `canonicalize`); it need not currently exist (spec 89 criterion 3 - a base already
/// removed out from under a still-running process is exactly the case this reaps).
/// Best-effort and platform-tolerant throughout - on an OS the process interface cannot read
/// the scan finds nothing and this is a graceful no-op.
pub fn reap_authorized(base: PathBuf) {
    let self_pid = std::process::id();
    let self_ancestors = ancestor_pids(self_pid);

    let term_targets = scan_with_starttime(&base);
    if term_targets.is_empty() {
        return;
    }
    for target in &term_targets {
        signal_if_unchanged(target, &base, self_pid, &self_ancestors, Signal::Term);
    }
    std::thread::sleep(GRACE);
    // Re-scan so only processes STILL rooted inside are force-killed - each gets its own
    // fresh start-time baseline here, then is rechecked again immediately below.
    for target in scan_with_starttime(&base) {
        signal_if_unchanged(&target, &base, self_pid, &self_ancestors, Signal::Hard);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::assert_teardown_reaps_what_is_rooted_inside;
    use crate::test_support::cleanup;
    use crate::test_support::sigterm_ignorer_in;
    use crate::test_support::sleeper_in;
    use crate::test_support::wait_until;
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

    /// Poll `child.try_wait()` until the process has exited or a generous timeout elapses;
    /// returns whether it exited.
    fn wait_for_exit(child: &mut Child) -> bool {
        wait_until(|| matches!(child.try_wait(), Ok(Some(_))))
    }

    // ---- end_child (spec 104 criterion 6, STOP): the handle-bound production reap ----

    #[test]
    fn end_child_term_stops_a_well_behaved_process_within_grace() {
        let dir = tempfile::tempdir().unwrap();
        let mut child = sleeper_in(dir.path());
        let snapshot = snapshot_descendants(child.id());
        let started = std::time::Instant::now();
        end_child(&mut child, snapshot);
        assert!(
            started.elapsed() < GRACE,
            "a TERM-responsive child must exit on the signal, not wait out the full grace"
        );
        assert!(
            matches!(child.try_wait(), Ok(Some(_))),
            "the child must be reaped (no zombie left behind)"
        );
    }

    #[test]
    fn end_child_escalates_to_kill_when_the_process_ignores_term() {
        let dir = tempfile::tempdir().unwrap();
        let mut child = sigterm_ignorer_in(dir.path());
        let snapshot = snapshot_descendants(child.id());
        end_child(&mut child, snapshot);
        assert!(
            matches!(child.try_wait(), Ok(Some(_))),
            "a TERM-ignoring child must still be ended, via the SIGKILL escalation"
        );
    }

    #[test]
    fn end_child_is_a_noop_on_an_already_exited_child() {
        let mut child = StdCommand::new("true").spawn().expect("spawn true");
        assert!(
            wait_for_exit(&mut child),
            "the fixture must exit on its own first"
        );
        // Must not panic, hang, or send a signal to a pid that may have been recycled -
        // `try_wait` already reaped it, so this is a pure no-op.
        let snapshot = snapshot_descendants(child.id());
        end_child(&mut child, snapshot);
        assert!(matches!(child.try_wait(), Ok(Some(_))));
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
        // stand-in for an OS the process interface cannot read) yields EMPTY, never an error -
        // so teardown and validate keep working where the dir or the process list is not there.
        let root = tempfile::tempdir().unwrap();
        let absent = root.path().join("never-created");
        assert!(processes_rooted_under(&absent).is_empty());
    }

    #[test]
    fn processes_rooted_under_matches_a_process_whose_cwd_was_deleted_out_from_under_it() {
        // spec 89 criterion 3, second half: the kernel marks a live process's cwd " (deleted)"
        // once the directory it still holds has been removed. Reproduces the spec-80 incident
        // verbatim (a mutant test binary looped for eight days at ~17 cores because the
        // reaper's cwd match never saw through that marker once the mutation tool removed its
        // tree). Both halves of this fix are exercised here: `processes_rooted_under` must
        // still be ABLE to scan for a base dir that no longer exists (not short-circuit to
        // empty), and the interface's cwd must name the removed dir by its path so it still lies
        // in the base.
        let root = tempfile::tempdir().unwrap();
        let base = root.path().join("scratch");
        std::fs::create_dir_all(&base).unwrap();
        let base = base.canonicalize().unwrap();

        let mut inside = sleeper_in(&base);
        assert!(
            wait_until(|| processes_rooted_under(&base)
                .iter()
                .any(|(pid, _)| *pid == inside.id())),
            "precondition: detected before the dir is removed"
        );

        std::fs::remove_dir_all(&base).expect("remove the dir out from under the live cwd");

        let found_after_delete = wait_until(|| {
            processes_rooted_under(&base)
                .iter()
                .any(|(pid, _)| *pid == inside.id())
        });
        cleanup(&mut inside);

        assert!(
            found_after_delete,
            "a process whose cwd was deleted out from under it must still be matched, via \
             the kernel's \" (deleted)\" suffix stripped"
        );
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
        // registered scratch root under a cache home (never nested under any project's
        // `.rigger/tmp`, and not even inside a git repo at all) is authorized just as readily.
        let cache_home = tempfile::tempdir().unwrap();
        let scratch_root = cache_home.path().join("rigger-scratch");
        let base = scratch_root.join("some-spawn-id");
        std::fs::create_dir_all(&base).unwrap();
        assert_eq!(
            is_reapable_base(&base, &scratch_root),
            Some(base.canonicalize().unwrap())
        );
    }

    /// `is_reapable_base` refuses `base_of(repo)` as a base under the repo's own `.rigger/tmp`
    /// authorized root.
    fn assert_is_reapable_base_refuses(base_of: fn(&FakeRepo) -> PathBuf) {
        let repo = FakeRepo::new();
        assert_eq!(is_reapable_base(&base_of(&repo), &repo.tmp), None);
    }

    crate::test_cases! {
        /// The repo root is a real, existing dir - just not under `authorized_root` (here, its
        /// own `.rigger/tmp` subdir).
        is_reapable_base_refuses_a_dir_that_is_not_under_the_given_authorized_root: assert_is_reapable_base_refuses(
            |repo| repo.root_path.clone(),
        );
        is_reapable_base_refuses_the_authorized_root_itself: assert_is_reapable_base_refuses(|repo| repo.tmp.clone());
    }

    #[test]
    fn is_reapable_base_authorizes_a_gone_target_strictly_under_the_root_instead_of_refusing() {
        // spec 89 criterion 3 (THE RECLAIM GUARD COMPARES PATHS): a target that no longer
        // exists but lexically resolves strictly under the authorized root is ALREADY
        // RECLAIMED, not refused - the guard compares PATHS, never requires the leaf to
        // exist. Before this fix, `base_dir.canonicalize()` failed for a gone leaf and this
        // returned `None` via the LOGGED refusal branch (a spawn-scratch reclaim
        // hit exactly this on every `rigger result`, per `adj-u91c4-reclaim-refusal-
        // corroborates-orphan-finding`: a spawn's own mutation-scratch dir, never created,
        // logged a scary "not strictly under" line every single time).
        let repo = FakeRepo::new();
        let absent = repo.tmp.join("never-created");
        assert_eq!(
            is_reapable_base(&absent, &repo.tmp),
            Some(repo.tmp.canonicalize().unwrap().join("never-created")),
            "a gone-but-under-root target must be AUTHORIZED (Some), never refused (None)"
        );
    }

    #[test]
    fn is_reapable_base_still_refuses_a_gone_target_that_would_resolve_outside_the_root() {
        // The other half of the same fix: leniency for a MISSING leaf must never widen the
        // boundary itself - a gone target that resolves OUTSIDE the root is still refused,
        // by name, exactly as a live one would be.
        let repo = FakeRepo::new();
        let outside_parent = tempfile::tempdir().unwrap();
        let gone_and_outside = outside_parent.path().join("never-created-and-outside");
        assert_eq!(is_reapable_base(&gone_and_outside, &repo.tmp), None);
    }

    /// Build a symlink under `repo.tmp` that escapes it (a real target dir under an
    /// unrelated tempdir it does not contain) - the shared fixture for every symlink-escape
    /// test, so the two near-identical setups collapse onto ONE implementation. Returns the escaping symlink's own
    /// path and the `TempDir` guard the caller must keep alive for its target to still
    /// exist.
    #[cfg(unix)]
    fn escaping_symlink(repo: &FakeRepo) -> (tempfile::TempDir, PathBuf) {
        let outside = tempfile::tempdir().unwrap();
        let real_outside_target = outside.path().join("real-target");
        std::fs::create_dir_all(&real_outside_target).unwrap();
        let link = repo.tmp.join("escape-link");
        std::os::unix::fs::symlink(&real_outside_target, &link).unwrap();
        (outside, link)
    }

    #[test]
    #[cfg(unix)]
    fn is_reapable_base_refuses_a_gone_leaf_beneath_a_symlink_that_escapes_the_root() {
        // Leniency for a missing LEAF must never defeat the EXISTING symlink-escape guard:
        // when an ancestor component that DOES exist is a symlink escaping the root, the
        // best-effort resolution follows it (as `canonicalize` always has) before the
        // missing suffix is lexically reattached, so the escape is still caught.
        let repo = FakeRepo::new();
        let (_outside, link) = escaping_symlink(&repo);
        let gone_leaf_beneath_link = link.join("never-created-child");
        assert_eq!(is_reapable_base(&gone_leaf_beneath_link, &repo.tmp), None);
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
    #[cfg(unix)]
    fn is_reapable_base_refuses_a_symlink_under_the_authorized_root_that_escapes_it() {
        let repo = FakeRepo::new();
        let (_outside, link) = escaping_symlink(&repo);
        assert_eq!(is_reapable_base(&link, &repo.tmp), None);
    }

    #[test]
    fn reap_kills_a_sigterm_ignoring_child_inside_and_spares_one_outside() {
        // The teardown reap (spec 23, spec 78): a process rooted inside a VALID base is
        // reaped even when it IGNORES SIGTERM (the SIGKILL escalation after the grace does
        // it), while a process rooted OUTSIDE the base is left ALIVE.
        let repo = FakeRepo::new();
        let base = repo.base("scratch");

        assert_teardown_reaps_what_is_rooted_inside(
            &base,
            Some(&repo.root_path),
            || reap_processes_rooted_under(&base, &repo.tmp),
            "the reap of a valid base",
        );
    }

    #[test]
    fn reap_kills_a_process_under_an_authorized_root_that_is_not_a_dot_rigger_tmp_tree() {
        // End-to-end proof of the fix at the public entry point: an authorized_root with NO
        // relationship whatsoever to any git repo or `.rigger/tmp` naming (mirroring a
        // registered scratch root under a cache home, or a `defaults.workdir`/
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
    fn reap_kills_a_process_whose_base_dir_was_already_removed_before_the_reap_call() {
        // End-to-end proof that spec 89 criterion 3's two fixes compose: a base dir removed
        // out from under a still-running process is (1) still AUTHORIZED by
        // `is_reapable_base` (it resolves strictly under the root even though it is gone -
        // never refused), and (2) the process rooted inside it is still FOUND and killed by
        // `processes_rooted_under`'s deleted-cwd match - reproducing the spec-80 8-day-hang
        // incident and proving this fix actually closes it, not just the false-refusal log
        // line.
        let repo = FakeRepo::new();
        let base = repo.base("scratch");

        let mut inside = sigterm_ignorer_in(&base);
        assert!(wait_until(|| processes_rooted_under(&base)
            .iter()
            .any(|(pid, _)| *pid == inside.id())));

        std::fs::remove_dir_all(&base).expect("remove the dir out from under the live cwd");

        reap_processes_rooted_under(&base, &repo.tmp);

        let inside_died = wait_for_exit(&mut inside);
        if !inside_died {
            cleanup(&mut inside);
        }
        assert!(
            inside_died,
            "a process rooted in an already-removed-but-authorized base must still be \
             SIGKILLed, never silently left as a runaway"
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

    /// [`reap_authorized`] is the termination sequence [`crate::worktree::Worktree::remove`]
    /// calls directly after its OWN git-identity authorization (never through
    /// `is_reapable_base`'s containment gate): given a bare, pre-canonicalized base, it asks the
    /// child `spawn` roots there to exit, waits the grace, then ends what is still rooted there.
    /// `spawn` gets the base and a marker path outside it. The child must end with `ended.0` as
    /// its exit code (`None` when a signal ended it) and leave the marker exactly when `ended.1`.
    fn assert_reap_authorized_ends(spawn: fn(&Path, &Path) -> Child, ended: (Option<i32>, bool)) {
        let root = tempfile::tempdir().unwrap();
        let root_path = root.path().canonicalize().unwrap();
        let base = root_path.join("base");
        std::fs::create_dir(&base).unwrap();
        let marker = root_path.join("marker");

        let mut inside = spawn(&base, &marker);
        assert!(wait_until(|| processes_rooted_under(&base)
            .iter()
            .any(|(pid, _)| *pid == inside.id())));

        reap_authorized(base);

        let mut status = None;
        wait_until(|| {
            status = inside.try_wait().ok().flatten();
            status.is_some()
        });
        if status.is_none() {
            cleanup(&mut inside);
        }
        assert_eq!(
            (status.map(|status| status.code()), marker.exists()),
            (Some(ended.0), ended.1),
            "reap_authorized ends the child rooted at an authorized base, with no containment \
             gate in front of it: (its exit code once ended, whether it left the marker)"
        );
    }

    /// Spawn a long-lived process rooted at `dir` that HANDLES the request to exit: on it, the
    /// process creates `marker` and exits 0. Answers once the handler is set, which the process
    /// says by creating `marker`'s sibling `trapped`, so a request sent after this returns is
    /// never one that arrived before the handler.
    fn term_handler_in(dir: &Path, marker: &Path) -> Child {
        let trapped = marker.with_file_name("trapped");
        let child = StdCommand::new("sh")
            .arg("-c")
            .arg("trap 'touch \"$0\"; exit 0' TERM; : > \"$1\"; sleep 300 & wait")
            .arg(marker)
            .arg(&trapped)
            .current_dir(dir)
            .spawn()
            .expect("spawn a TERM-handling fixture process");
        assert!(wait_until(|| trapped.exists()), "the handler is set");
        child
    }

    crate::test_cases! {
        /// A process that ignores the request to exit is ended without asking: a signal ends it,
        /// and no handler runs.
        reap_authorized_kills_a_sigterm_ignoring_process_given_an_already_authorized_base:
            assert_reap_authorized_ends(|dir, _| sigterm_ignorer_in(dir), (None, false));
        /// The request to exit comes first: a process that handles it runs its handler and exits
        /// 0 within the grace, before anything ends it without asking.
        reap_authorized_asks_a_term_handling_process_to_exit_first_given_an_already_authorized_base:
            assert_reap_authorized_ends(term_handler_in, (Some(0), true));
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
    fn ancestor_pids_walks_through_an_intermediate_process() {
        // The test binary itself runs as pid 1 of its own namespace (`.cargo/pidns-runner.sh`),
        // so `ancestor_pids` on a DIRECT child would trivially stop at that pid (already
        // refused unconditionally elsewhere, so never separately recorded - see
        // `is_signal_eligible`). Spawn a GRANDCHILD instead (a shell that backgrounds a
        // `sleep` and prints its pid) so the walk must pass THROUGH a real intermediate
        // pid (the shell) to prove it is a multi-hop walk, not a single parent read.
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

    /// One process-table row: `pid`, the parent its record names, and its start time.
    fn row(pid: u32, parent: u32, start: u64) -> os_process::Entry {
        os_process::Entry { pid, parent, start }
    }

    /// A walk row: what it shows, the table, and the `(pid, start)` descendants of pid 10.
    type WalkRow = (&'static str, Vec<os_process::Entry>, Vec<(u32, u64)>);

    /// THE DESCENDANT WALK (one table, start order): from a root the table lists, every process
    /// whose named parent is the root or one of its descendants, parents before their children in
    /// the table's order, each with the start time the table read. A child that started before the
    /// parent it names is no descendant (on Windows that parent's pid was taken by a later process),
    /// and neither are its own children; a child started with its parent is one. A root the table
    /// does not list has none, the root is never its own descendant, and a cycle of parents ends.
    #[test]
    fn the_descendant_walk_follows_named_parents_in_start_order_from_a_listed_root() {
        let rows: [WalkRow; 7] = [
            ("a root with no child", vec![row(10, 1, 100)], vec![]),
            (
                "children before grandchildren, in the table's order",
                vec![
                    row(13, 11, 130),
                    row(10, 1, 100),
                    row(12, 10, 120),
                    row(11, 10, 110),
                ],
                vec![(12, 120), (11, 110), (13, 130)],
            ),
            (
                "a child started with its parent",
                vec![row(10, 1, 100), row(11, 10, 100)],
                vec![(11, 100)],
            ),
            (
                "a child started before the parent it names, and its own child",
                vec![row(10, 1, 100), row(11, 10, 99), row(12, 11, 150)],
                vec![],
            ),
            (
                "a stranger whose parent is another process",
                vec![row(10, 1, 100), row(20, 1, 105), row(21, 20, 106)],
                vec![],
            ),
            (
                "a root the table does not list",
                vec![row(11, 10, 110)],
                vec![],
            ),
            (
                "a cycle of parents through the root",
                vec![row(10, 11, 100), row(11, 10, 100)],
                vec![(11, 100)],
            ),
        ];
        let walked: Vec<(&str, Vec<(u32, u64)>)> = rows
            .iter()
            .map(|(what, table, _)| {
                let found = descendants_in(table, 10)
                    .iter()
                    .map(|entry| (entry.pid, entry.start))
                    .collect();
                (*what, found)
            })
            .collect();
        let expected: Vec<(&str, Vec<(u32, u64)>)> = rows
            .iter()
            .map(|(what, _, found)| (*what, found.clone()))
            .collect();
        assert_eq!(walked, expected);
    }

    /// The walk over the live table finds a grandchild through the shell that started it, with
    /// the start time the grandchild reads.
    #[cfg(unix)]
    #[test]
    fn the_descendant_walk_finds_a_live_grandchild_through_its_parent() {
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
        let grandchild: u32 = line.trim().parse().expect("parse the grandchild pid");
        let found: Vec<(u32, Option<u64>)> = descendants_of(shell.id())
            .iter()
            .map(|entry| (entry.pid, Some(entry.start)))
            .collect();
        let expected = vec![(grandchild, pid_starttime(grandchild))];
        cleanup(&mut shell);
        assert_eq!(found, expected);
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
        let target = Identity {
            pid: child.id(),
            start: pid_starttime(child.id()).unwrap(),
        };
        signal_if_unchanged(
            &target,
            &base,
            std::process::id(),
            &HashSet::new(),
            Signal::Hard,
        );
        let died = wait_for_exit(&mut child);
        if !died {
            cleanup(&mut child);
        }
        assert!(died, "an eligible, matching target must be signalled");
    }

    /// A live `sleep` rooted in a fresh `scratch` base whose recorded starttime is shifted by
    /// `starttime_shift`, signalled against the base named `signal_base`, must be SKIPPED -
    /// still alive afterwards.
    fn assert_signal_skipped(starttime_shift: u64, signal_base: &str, why: &str) {
        let repo = FakeRepo::new();
        let base = repo.base("scratch");
        let signal_base = repo.base(signal_base);
        let mut child = sleeper_in(&base);
        let ready = wait_until(|| pid_starttime(child.id()).is_some());
        assert!(ready, "precondition: starttime is readable");
        let target = Identity {
            pid: child.id(),
            start: pid_starttime(child.id())
                .unwrap()
                .wrapping_add(starttime_shift),
        };
        signal_if_unchanged(
            &target,
            &signal_base,
            std::process::id(),
            &HashSet::new(),
            Signal::Hard,
        );
        let still_alive = matches!(child.try_wait(), Ok(None));
        cleanup(&mut child);
        assert!(still_alive, "{why}");
    }

    crate::test_cases! {
        // The TOCTOU guard (spec 78): even though the pid and cwd both genuinely match, a
        // starttime that no longer matches the scan's recorded value means the scan's
        // identity is stale - skip rather than signal.
        signal_if_unchanged_skips_a_starttime_mismatch: assert_signal_skipped(
            1,
            "scratch",
            "a starttime mismatch must be skipped, never signalled",
        );
        // The pid and starttime both genuinely match, but the base passed in does not
        // contain the process's cwd - must be skipped (mirrors "cwd changed" between scan
        // and signal: from this call's point of view, it no longer matches).
        signal_if_unchanged_skips_when_cwd_is_outside_the_given_base: assert_signal_skipped(
            0,
            "unrelated",
            "a cwd outside the given base must be skipped, never signalled",
        );
    }

    #[test]
    fn signal_if_unchanged_skips_when_pid_is_marked_as_self_or_an_ancestor() {
        let repo = FakeRepo::new();
        let base = repo.base("scratch");
        let mut child = sleeper_in(&base);
        let ready = wait_until(|| pid_starttime(child.id()).is_some());
        assert!(ready);
        let target = Identity {
            pid: child.id(),
            start: pid_starttime(child.id()).unwrap(),
        };
        // Pretend the child IS "self" - it must never be signalled.
        signal_if_unchanged(&target, &base, child.id(), &HashSet::new(), Signal::Hard);
        let alive_as_self = matches!(child.try_wait(), Ok(None));
        assert!(alive_as_self, "a pid equal to self_pid must be skipped");

        // Pretend the child is a recorded ancestor - it must never be signalled.
        let mut ancestors = HashSet::new();
        ancestors.insert(child.id());
        signal_if_unchanged(&target, &base, std::process::id(), &ancestors, Signal::Hard);
        let alive_as_ancestor = matches!(child.try_wait(), Ok(None));
        cleanup(&mut child);
        assert!(
            alive_as_ancestor,
            "a pid recorded as an ancestor of self must be skipped"
        );
    }
}
