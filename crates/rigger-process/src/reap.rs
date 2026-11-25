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
//! - [`processes_rooted_under`] - the pure detection primitive, `(pid, executable name)` for
//!   every process whose cwd resolves strictly inside a base dir. Both the teardown reap AND
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
//! [`std::process::Child`] handle to goes through [`send_signal`]. On Linux and Windows it goes
//! through a process descriptor opened on the target before the signal's checks ([`address`]): a
//! pidfd on Linux, a process handle on Windows, through which the identity check also reads the
//! process's start time. So it reaches the process the checks passed or none. Elsewhere, and on a
//! Linux host where descriptors are unavailable ([`no_process_descriptors`]), its unix arm is
//! `rustix::process::kill_process` - never a shell-out to `kill(1)`, never `libc::kill`, never
//! a process-group (negative pid) target - its Windows arm, for a held child addressed by its pid,
//! opens the one pid for termination and ends it, and its arm for any other OS sends nothing. On
//! Windows a job object this process created is ended through `end_job` alone, the one site of
//! the job end.
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
//!
//! On Linux and Windows the re-read comes after a process descriptor is opened on the pid
//! ([`address`]), and the signal goes through that descriptor, so a pid recycled between the
//! re-read and the signal is not signalled either: the descriptor names the process that held the
//! pid when it was opened, and a re-read that matches proves that is the process the scan found.
//! On Windows the re-read itself goes through the handle, which keeps the pid its process's own.
//! Where descriptors are unavailable that last gap stays open, and a driver says so once
//! ([`no_process_descriptors`]).

use crate::os_process;
use crate::os_process::{Parentage, Ties};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// How long a well-behaved process is given to exit on SIGTERM before it is SIGKILLed.
/// Short: teardown is on the hot path (every unit worktree removal), and a process that
/// ignores SIGTERM should not stall the run - a fraction of a second is ample for a process
/// that handles the signal, and the SIGKILL backstop reaps the rest.
const GRACE: std::time::Duration = std::time::Duration::from_millis(300);

/// Every process whose resolved cwd is `base_dir` itself or strictly inside it, as
/// `(pid, executable name)`. The SINGLE scan authority both the teardown reap
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
/// itself is excluded (rigger never reaps its own pid). Each match's executable name
/// ([`crate::os_process::executable_name`]), never its arguments, which can hold a secret, is
/// purely descriptive: it names the leak in the `rigger validate` advisory and never affects
/// which processes are reaped.
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
        .map(|pid| (pid, os_process::executable_name(pid)))
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

/// Every descendant of `root_pid` - its children, their children, and so on - read as
/// `(pid, starttime)` pairs at the moment of the call: [`ancestor_pids`]'s upward walk, but
/// downward, so a process a held child forked is found though only the child's handle is held.
///
/// The process interface gives every process its OWN parent, never a parent's list of children,
/// so the walk reads ONE process table ([`crate::os_process::table`]: each process's parent and
/// start time from a single listing) and walks it in [`descendants_in`]. Matched by PID-TREE
/// membership alone, deliberately NEVER by working directory: a live sibling spawn's own
/// process can share this same worktree as its cwd without being anywhere in this child's
/// process tree (`op-104-stop-no-sweep-at-wall-clock-stop`'s sibling-safety guarantee, which
/// this walk must never reopen), so it can never appear here regardless of what it shares on
/// disk. Each entry carries the start time the table read, for the TOCTOU recheck
/// ([`toctou_still_same_process`]) every signal to it passes.
///
/// Best-effort and platform-tolerant like every other scan in this module: an OS the process
/// interface cannot read, or a process the listing could not read, contributes nothing, never a
/// hard error.
pub(crate) fn descendants_of(root_pid: u32) -> Vec<Identity> {
    descendants_in(&os_process::table(), root_pid)
}

/// The descendants of `root_pid` in `table`, children before grandchildren and each generation in
/// the table's order, by this OS's tie rule ([`walk_in`], [`os_process::TIES`]).
pub(crate) fn descendants_in(table: &[os_process::Entry], root_pid: u32) -> Vec<Identity> {
    walk_in(table, root_pid, os_process::TIES)
        .generations
        .concat()
}

/// A walk down a process table from a root ([`walk_in`]).
#[derive(Debug, Default, PartialEq, Eq)]
struct Walk {
    /// The root's descendants, one list per generation, its children first, each generation in
    /// the table's order.
    generations: Vec<Vec<Identity>>,
    /// The processes the walk could not tell from strangers, in the order it reached them: each
    /// one started in the same tick as the parent its record names where a tie is not trusted
    /// ([`Parentage::Tie`]), and every process below one.
    untrusted: Vec<Identity>,
}

/// The descendants of `root_pid` in `table`, empty when the table does not list `root_pid`, each
/// process read against the parent its record names by `ties` ([`Ties::parentage`]). A process
/// that started after its parent is a descendant; one that started before it is none, and nor is
/// anything below it, since the parent's pid has been given to a later process: a Windows record
/// keeps a dead parent's pid, and a table read one record at a time can name a parent whose pid a
/// later tree member took while the table was read. One that started in its parent's own tick is
/// a descendant where `ties` trusts a tie, and where it does not, it and everything below it are
/// [`Walk::untrusted`]: a real child the walk cannot vouch for, or a stranger. `seen` (the root
/// included) ends the walk on a cycle, which the OS's own parent/child relationship should never
/// produce but the walk must survive.
fn walk_in(table: &[os_process::Entry], root_pid: u32, ties: Ties) -> Walk {
    let mut walk = Walk::default();
    let Some(root) = table.iter().find(|entry| entry.pid == root_pid) else {
        return walk;
    };
    let mut children_of: HashMap<u32, Vec<&os_process::Entry>> = HashMap::new();
    for entry in table {
        children_of.entry(entry.parent).or_default().push(entry);
    }
    let mut seen: HashSet<u32> = HashSet::from([root_pid]);
    let mut parents = vec![(root, true)];
    while !parents.is_empty() {
        let mut generation = Vec::new();
        let mut next = Vec::new();
        for (parent, trusted) in parents {
            for &kid in children_of.get(&parent.pid).into_iter().flatten() {
                let kept = match ties.parentage(parent.start, kid.start) {
                    Parentage::Parent => trusted,
                    Parentage::Tie => false,
                    Parentage::Later => continue,
                };
                if !seen.insert(kid.pid) {
                    continue;
                }
                let identity = Identity {
                    pid: kid.pid,
                    start: kid.start,
                };
                if kept {
                    generation.push(identity);
                } else {
                    walk.untrusted.push(identity);
                }
                next.push((kid, kept));
            }
        }
        if !generation.is_empty() {
            walk.generations.push(generation);
        }
        parents = next;
    }
    walk
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
/// [`is_signal_eligible`] AND `start`, the start time of the process its signal addresses read
/// this instant ([`started`]), still matches what the scan recorded. This is the pid-identity
/// half of the TOCTOU guard (spec 78) - a pid that exited and was recycled onto an unrelated
/// process in the gap since the scan reads a DIFFERENT start time and is refused - shared by the
/// cwd-scanned signal path ([`signal_if_unchanged`]), the pid-tree-walked one ([`Tree::send`])
/// and the undo of a freeze that reached another process, so there is exactly ONE TOCTOU recheck
/// regardless of which kind of snapshot found the candidate.
fn toctou_still_same_process(
    target: &Identity,
    start: Option<u64>,
    self_pid: u32,
    self_ancestors: &HashSet<u32>,
) -> bool {
    is_signal_eligible(target.pid, self_pid, self_ancestors) && start == Some(target.start)
}

/// Address `target` through `os` ([`EndOs::address`]), then re-read its cwd
/// ([`crate::os_process::cwd`]) IMMEDIATELY before signalling it, and signal it through that
/// address only if it still lies in `base` ([`crate::os_process::cwd_lies_in`]) AND
/// [`toctou_still_same_process`]. This is the cwd half of the TOCTOU guard (spec 78): a pid
/// that exited and was recycled onto an unrelated process (even one that happens to also be
/// rooted under `base`) between the scan and this call is silently skipped, never signalled,
/// and so is a pid whose cwd cannot be read or that cannot be addressed. One a service manager
/// holds apart ([`EndOs::held_apart`]) is never signalled either: answers its unit.
fn signal_if_unchanged(
    os: &mut impl EndOs,
    target: &Identity,
    base: &Path,
    self_pid: u32,
    self_ancestors: &HashSet<u32>,
    signal: Signal,
) -> Option<String> {
    let to = os.address(target.pid)?;
    if !os_process::cwd_lies_in(target.pid, base)
        || !toctou_still_same_process(target, started(&to), self_pid, self_ancestors)
    {
        return None;
    }
    let held = os.held_apart(target.pid);
    if held.is_none() {
        os.send(signal, &to);
    }
    held
}

/// The fault an end reports when a freeze it sent to `pid` by the pid alone reached another
/// process than the one its checks passed ([`Tree::send`]): that process was `resumed` once a
/// second read confirmed it still held the pid, and was sent nothing more where none did.
fn freeze_reached_another(pid: u32, resumed: bool) -> String {
    let undo = if resumed {
        "it was resumed at once"
    } else {
        "its pid did not read as that process again, so it was sent nothing more and may stay \
         frozen"
    };
    format!(
        "a freeze sent to pid {pid} by the pid alone reached another process, one that took the \
         pid after the freeze's checks; {undo}"
    )
}

/// What the reaper sends a process: [`Signal::Term`] asks it to exit, [`Signal::Hard`] ends it
/// without asking, [`Signal::Stop`] freezes it where it stands, so it can neither exit nor fork
/// until it is ended or [`Signal::Resume`]d. Windows has no request to exit that an arbitrary
/// process can catch, so there both ending kinds end the process at once, and it has no freeze, so
/// there the other two send nothing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Signal {
    /// Ask the process to exit (`SIGTERM` on unix).
    Term,
    /// End the process without asking (`SIGKILL` on unix).
    Hard,
    /// Freeze the process (`SIGSTOP` on unix).
    Stop,
    /// Let a frozen process run again (`SIGCONT` on unix).
    Resume,
}

std::cfg_select! {
    unix => {
        /// The one OS call a signal becomes: the target, already checked, and the unix signal.
        type OsCall = (rustix::process::Pid, rustix::process::Signal);
    }
    target_os = "windows" => {
        /// The one OS call a signal becomes: the pid to open for termination and end.
        type OsCall = u32;
    }
    _ => {
        /// No OS call: this OS sends nothing.
        type OsCall = std::convert::Infallible;
    }
}

/// The OS call `signal` to `pid` becomes, or `None` when it must send nothing. `pid > 1` is an
/// EXPLICIT guard (belt-and-braces alongside [`is_signal_eligible`]'s own check, since the pinned
/// rustix's `Pid::from_raw` accepts negative raw values and rejects only zero), and on unix a pid
/// that does not fit a positive `i32` is refused before it can become a negative one. Pure, so
/// each kind's refusals are pinned without a signal sent.
fn os_call(signal: Signal, pid: u32) -> Option<OsCall> {
    if pid <= 1 {
        return None;
    }
    std::cfg_select! {
        unix => {
            let rpid = unix_pid(pid)?;
            let signal = match signal {
                Signal::Term => rustix::process::Signal::TERM,
                Signal::Hard => rustix::process::Signal::KILL,
                Signal::Stop => rustix::process::Signal::STOP,
                Signal::Resume => rustix::process::Signal::CONT,
            };
            Some((rpid, signal))
        }
        target_os = "windows" => {
            // Both ending kinds end the process, and there is no freeze: see [`Signal`].
            match signal {
                Signal::Term | Signal::Hard => Some(pid),
                Signal::Stop | Signal::Resume => None,
            }
        }
        _ => {
            let _ = signal;
            None
        }
    }
}

/// `pid` as a unix pid, or `None` for a pid that would be 0 or negative as one: the pinned
/// rustix's `Pid::from_raw` accepts a negative raw value, so a pid that does not fit a positive
/// `i32` is refused here, before it can become one.
#[cfg(unix)]
fn unix_pid(pid: u32) -> Option<rustix::process::Pid> {
    rustix::process::Pid::from_raw(i32::try_from(pid).ok()?)
}

std::cfg_select! {
    target_os = "linux" => {
        /// A process descriptor: it names the one process it was opened on until it is closed,
        /// so a signal sent through it reaches that process or none, never one that took its pid.
        type Descriptor = std::os::fd::OwnedFd;

        /// Send `call`'s signal through `descriptor`.
        fn send_through(descriptor: &Descriptor, call: OsCall) {
            let (_, signal) = call;
            let _ = rustix::process::pidfd_send_signal(descriptor, signal);
        }

        /// Why opening a process descriptor failed with `error`, as the driver's line says it.
        fn unavailable_because(error: rustix::io::Errno) -> String {
            if error == rustix::io::Errno::NOSYS {
                "this kernel has no process descriptors".to_string()
            } else {
                format!("opening one fails here: {error}")
            }
        }
    }
    target_os = "windows" => {
        /// A process handle, opened for termination and for reading the process's times: while it
        /// is open the OS gives the pid of the process it was opened on to no other process, so
        /// what is read and sent through it reaches that process or none.
        type Descriptor = std::os::windows::io::OwnedHandle;

        /// End the process `descriptor` was opened on, with exit code 1: a signal here is only
        /// ever an end, and `call`, the pid already checked, maps one ([`os_call`]).
        fn send_through(descriptor: &Descriptor, _call: OsCall) {
            use std::os::windows::io::AsRawHandle;
            use windows_sys::Win32::System::Threading::TerminateProcess;
            // SAFETY: the handle is open with termination access and stays open across the call.
            let _ = unsafe { TerminateProcess(descriptor.as_raw_handle(), 1) };
        }
    }
    _ => {
        /// No process descriptor: this OS signals a process by its pid alone.
        type Descriptor = std::convert::Infallible;

        /// No descriptor exists here, so nothing is sent through one.
        fn send_through(descriptor: &Descriptor, _call: OsCall) {
            match *descriptor {}
        }
    }
}

/// Where a signal goes: the pid it names and, on Linux and Windows, the process descriptor opened
/// on that pid before the signal's checks, which the signal is sent through.
struct Addressee {
    pid: u32,
    descriptor: Option<Descriptor>,
}

impl Addressee {
    /// `pid` addressed by its number alone: a held child, whose unreaped handle keeps its pid its
    /// own, and every process where descriptors are unavailable.
    fn by_pid(pid: u32) -> Addressee {
        Addressee {
            pid,
            descriptor: None,
        }
    }
}

/// `pid` addressed for a signal, ahead of every check the signal passes: where this host opens
/// process descriptors (`by_descriptor`, [`no_process_descriptors`]), through one opened on the
/// process that holds `pid` now (a pidfd on Linux, a process handle on Windows), so the signal
/// sent after the checks reaches that process or none; by the pid alone otherwise. `None` when the
/// descriptor does not open, since the process is gone: it is sent nothing. `None` for pid 0 and
/// init (pid 1) on every OS, whatever the checks after would answer, so neither is ever
/// addressed.
fn address(pid: u32, by_descriptor: bool) -> Option<Addressee> {
    if pid <= 1 {
        return None;
    }
    std::cfg_select! {
        target_os = "linux" => {
            if !by_descriptor {
                return Some(Addressee::by_pid(pid));
            }
            let opened =
                rustix::process::pidfd_open(unix_pid(pid)?, rustix::process::PidfdFlags::empty());
            Some(Addressee {
                pid,
                descriptor: Some(opened.ok()?),
            })
        }
        target_os = "windows" => {
            use std::os::windows::io::{FromRawHandle, OwnedHandle};
            use windows_sys::Win32::Foundation::FALSE;
            use windows_sys::Win32::System::Threading::{
                OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_TERMINATE,
            };
            if !by_descriptor {
                return Some(Addressee::by_pid(pid));
            }
            // SAFETY: the call takes no pointer and answers a null handle on failure.
            let opened = unsafe {
                OpenProcess(PROCESS_TERMINATE | PROCESS_QUERY_LIMITED_INFORMATION, FALSE, pid)
            };
            if opened.is_null() {
                return None;
            }
            Some(Addressee {
                pid,
                // SAFETY: the handle is the one the call just answered, owned by nothing else.
                descriptor: Some(unsafe { OwnedHandle::from_raw_handle(opened) }),
            })
        }
        _ => {
            let _ = by_descriptor;
            Some(Addressee::by_pid(pid))
        }
    }
}

/// The start time of the process `to` addresses ([`pid_starttime`]): read through its descriptor
/// where one is held and the OS reads a process's times through it (a Windows process handle),
/// and by its pid otherwise.
fn started(to: &Addressee) -> Option<u64> {
    std::cfg_select! {
        target_os = "windows" => {
            use std::os::windows::io::AsRawHandle;
            match &to.descriptor {
                Some(handle) => os_process::windows::created(handle.as_raw_handle()),
                None => pid_starttime(to.pid),
            }
        }
        _ => {
            pid_starttime(to.pid)
        }
    }
}

/// Why this process cannot address a process through a process descriptor here, where its OS has
/// them: on Linux, the error that opening one on this process answers ([`unavailable_because`]),
/// probed once for this process while the answer lasts ([`probed`], [`lasting`]) and again at each
/// call while it may pass. Any error falls back to the pid, with every check kept. `None` where
/// one opens; on Windows, where a process handle opens on any process this process may end; and
/// on every other OS, which addresses a process by its pid alone.
pub fn no_process_descriptors() -> Option<String> {
    std::cfg_select! {
        target_os = "linux" => {
            static PROBED: std::sync::OnceLock<Option<rustix::io::Errno>> =
                std::sync::OnceLock::new();
            let opened = || {
                let me = rustix::process::getpid();
                rustix::process::pidfd_open(me, rustix::process::PidfdFlags::empty()).err()
            };
            probed(&PROBED, opened, |error| error.is_none_or(lasting))
                .map(unavailable_because)
        }
        _ => {
            None
        }
    }
}

/// The answer `probe` gives, kept in `cache` once `lasts` says it will not change: a kept answer
/// is answered again without probing, and one that may change is probed again at the next read.
#[cfg(any(target_os = "linux", test))]
fn probed<T: Clone>(
    cache: &std::sync::OnceLock<T>,
    probe: impl FnOnce() -> T,
    lasts: impl FnOnce(&T) -> bool,
) -> T {
    if let Some(kept) = cache.get() {
        return kept.clone();
    }
    let answer = probe();
    if lasts(&answer) {
        let _ = cache.set(answer.clone());
    }
    answer
}

/// Whether opening a process descriptor that failed with `error` fails for the life of this
/// process: a kernel with none (`ENOSYS`), or a process refused them (`EPERM`, as a seccomp
/// filter answers). Any other error, such as running out of descriptors or memory, may pass.
#[cfg(target_os = "linux")]
fn lasting(error: rustix::io::Errno) -> bool {
    matches!(error, rustix::io::Errno::NOSYS | rustix::io::Errno::PERM)
}

/// The line this process says on its error output when it signals a session's processes by pid,
/// because process descriptors are unavailable for `reason` ([`no_process_descriptors`]).
fn signalled_by_pid(reason: &str) -> String {
    format!(
        "rigger: process descriptors unavailable: {reason}; a session's processes are signalled \
         by pid after a check, and a process that takes a checked pid before the signal lands \
         can receive it"
    )
}

/// Say why this process signals a session's processes by pid ([`signalled_by_pid`]), once for
/// each `reason`, however many spawns, ends and sweeps fall back for it, the keeper's ends
/// ([`EndReport::by_pid_because`]) among them.
pub fn say_signalled_by_pid(reason: &str) {
    static SAID: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());
    let mut said = SAID
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if first_said(&mut said, reason) {
        crate::whole_line::say(format_args!("{}", signalled_by_pid(reason)));
    }
}

/// Whether `reason` is not yet in `said`, adding it.
fn first_said(said: &mut Vec<String>, reason: &str) -> bool {
    if said.iter().any(|known| known == reason) {
        return false;
    }
    said.push(reason.to_string());
    true
}

/// Send `signal` to `to` - the reaper's ONE sanctioned signalling call (the `no-os-kill`
/// gate, spec 78, exempts only this function and `tests/common/mod.rs::terminate_pid` from
/// its ban on calling a signal API directly, never from its ban on shelling out to a process
/// termination utility, and this file alone from its ban on the Windows process-termination
/// call and on a signal sent through a descriptor): the call [`os_call`] maps it to, through
/// `to`'s process descriptor where it holds one, and otherwise on unix the internal rustix
/// syscall, on Windows the process opened for termination and ended with exit code 1, on any
/// other OS nothing. Best-effort: ESRCH (the process already exited) or any other failure
/// (permission, ...) is silently ignored, since the reap is teardown cleanup, never a hard error
/// that could fail a worktree removal or a step.
fn send_signal(signal: Signal, to: &Addressee) {
    let Some(call) = os_call(signal, to.pid) else {
        return;
    };
    if let Some(descriptor) = &to.descriptor {
        send_through(descriptor, call);
        return;
    }
    std::cfg_select! {
        unix => {
            let (rpid, signal) = call;
            let _ = rustix::process::kill_process(rpid, signal);
        }
        target_os = "windows" => {
            use windows_sys::Win32::Foundation::{CloseHandle, FALSE};
            use windows_sys::Win32::System::Threading::{
                OpenProcess, TerminateProcess, PROCESS_TERMINATE,
            };
            // SAFETY: the call takes no pointer and answers a null handle on failure.
            let handle = unsafe { OpenProcess(PROCESS_TERMINATE, FALSE, call) };
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
            match call {}
        }
    }
}

/// How long an end waits for what it froze to stop and for what it ended to be gone, all its
/// generations together.
pub const END_BOUND: Duration = Duration::from_secs(5);

/// How often an end reads again what it froze or ended.
const POLL: Duration = Duration::from_millis(10);

/// The least time an end gives each generation, and the main, to be gone once [`END_BOUND`] is
/// spent, so a generation that comes after a slow one is still confirmed.
const FLOOR: Duration = Duration::from_millis(100);

/// How many parent links [`chain_reaches`] reads before it answers that the chain does not reach.
const CHAIN_STEPS: usize = 4096;

/// Whether this OS freezes a process ([`Signal::Stop`]); where it does not, an end ends a tree
/// without freezing it first, and confirms no freeze.
const FREEZES: bool = cfg!(unix);

/// A process an end ended, left running, or never signalled.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Leftover {
    /// The process's pid when the end saw it.
    pub pid: u32,
    /// Its executable's file name ([`crate::os_process::executable_name`]), naming it in a
    /// progress line: never its arguments, which can hold a secret.
    pub program: String,
    /// Whether it is still frozen: the end froze it, could not end it, and could not resume it.
    pub stopped: bool,
}

impl Leftover {
    /// `pid` as a leftover, named by its executable as it reads now.
    pub(crate) fn of(pid: u32, stopped: bool) -> Leftover {
        Leftover {
            pid,
            program: os_process::executable_name(pid),
            stopped,
        }
    }
}

/// A process an end left running, never signalled, because a service manager holds it in a unit
/// other than the one the process ending it runs in ([`held_apart`]).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HeldApart {
    /// The process, as it read when the end left it.
    pub process: Leftover,
    /// The name of the unit that holds it, as the service manager names it: `run-r9.scope`.
    pub unit: String,
}

/// The unit a service manager holds a process in apart from the process ending it: the name of
/// the process's unit when `reference`, the ending process's cgroup file
/// ([`crate::os_process::cgroup`]), and `process`, the process's, each read the unified hierarchy
/// alone and each name a unit ([`unit_of`]), and the two units differ. Every other reading, a file
/// that is missing or unreadable, a host with a v1 or hybrid hierarchy, or a group under no unit,
/// answers `None`: the process is ended, as every process was before an end read cgroups.
fn held_apart(reference: Option<&str>, process: Option<&str>) -> Option<String> {
    let reference = unit_of(unified_path(reference?)?)?;
    let unit = unit_of(unified_path(process?)?)?;
    (unit != reference).then(|| unit.rsplit('/').next().unwrap_or(unit).to_string())
}

/// The path of the one group a cgroup file names on a host with the unified hierarchy alone: the
/// file is the single line `0::<path>` (`cgroups(7)`). `None` for any other file.
fn unified_path(cgroup: &str) -> Option<&str> {
    let mut lines = cgroup.lines();
    let path = lines.next()?.strip_prefix("0::")?;
    lines.next().is_none().then_some(path)
}

/// The unit a group's `path` lies in: the path through its deepest component a service manager
/// names a unit it runs a process in, a `.scope` or a `.service`. `None` for a path under no unit.
fn unit_of(path: &str) -> Option<&str> {
    let end = path
        .match_indices('/')
        .map(|(at, _)| at)
        .chain([path.len()])
        .rfind(|&at| path[..at].ends_with(".scope") || path[..at].ends_with(".service"))?;
    Some(&path[..end])
}

/// What an end signalled and what it left: `sent`, the processes besides the main it sent its end
/// to, each running just before its signal; `residue`, the processes it signalled that are still
/// running past [`END_BOUND`]; `skipped`, those still running that it found but never signalled,
/// because at signal time their parent chain no longer reached the tree's root (an escape, or a
/// stranger that took a pid), or because the walk could not tell them from strangers
/// ([`Walk::untrusted`]); `faults`, what the OS refused it; and `by_pid_because`, why it signalled
/// by pid.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EndReport {
    /// Each process besides the main the end sent its end to (a hard end, or a TERM asking it to
    /// exit), once, named by its executable as it read just before the signal, while it still
    /// ran: a process that had exited by then is never named. One still running past the bound
    /// is in `residue` too.
    pub sent: Vec<Leftover>,
    /// Signalled, and still running past the bound.
    pub residue: Vec<Leftover>,
    /// Found, never signalled, and still running.
    pub skipped: Vec<Leftover>,
    /// Found, never signalled, and left running, since a service manager holds each in a unit of
    /// its own ([`HeldApart`]).
    pub held_apart: Vec<HeldApart>,
    /// Each call the OS refused the end, said whole with what followed from it.
    pub faults: Vec<String>,
    /// Why the end addressed each process by its pid, with every check kept, where this host has
    /// process descriptors and the process that ended the tree could not open one
    /// ([`no_process_descriptors`]); `None` where it addressed each through one, or the OS has
    /// none. The holder says it once ([`say_signalled_by_pid`]).
    pub by_pid_because: Option<String>,
}

impl EndReport {
    /// Report `target` as found and never signalled, once, while it is still running: a process
    /// that is gone by then left nothing to report, one the end sent its end to
    /// ([`EndReport::sent`]) was signalled, and one a service manager holds apart is reported so
    /// ([`EndReport::held_apart`]).
    fn skip(&mut self, target: &Identity) {
        let reported = self
            .skipped
            .iter()
            .any(|leftover| leftover.pid == target.pid);
        let accounted = reported || self.was_sent(target.pid) || self.is_held_apart(target.pid);
        if !accounted && running(target) {
            self.skipped.push(Leftover::of(target.pid, false));
        }
    }

    /// Report `target` as held apart in `unit` ([`EndReport::held_apart`]), once, while it is
    /// still running.
    fn hold_apart(&mut self, target: &Identity, unit: String) {
        if !self.is_held_apart(target.pid) && running(target) {
            self.held_apart.push(HeldApart {
                process: Leftover::of(target.pid, false),
                unit,
            });
        }
    }

    /// Whether the end left `pid` to the unit that holds it ([`EndReport::held_apart`]).
    fn is_held_apart(&self, pid: u32) -> bool {
        self.held_apart.iter().any(|held| held.process.pid == pid)
    }

    /// Whether every process of `walk` still running is one a service manager holds apart, and
    /// one is: what is left of the tree is no end's to wait for.
    fn only_held_apart_runs(&self, walk: &[Identity]) -> bool {
        let mut runs = walk.iter().filter(|target| running(target)).peekable();
        runs.peek().is_some() && runs.all(|target| self.is_held_apart(target.pid))
    }

    /// Whether the end sent its end to `pid` ([`EndReport::sent`]).
    fn was_sent(&self, pid: u32) -> bool {
        self.sent.iter().any(|leftover| leftover.pid == pid)
    }

    /// Report `left` as sent the end ([`EndReport::sent`]), unless it is already.
    pub(crate) fn add_sent(&mut self, left: Leftover) {
        if !self.was_sent(left.pid) {
            self.sent.push(left);
        }
    }

    /// Stop reporting `target` as never signalled: a later check passed and it was signalled.
    fn unskip(&mut self, target: &Identity) {
        self.skipped.retain(|leftover| leftover.pid != target.pid);
    }

    /// The lines this end of a session's tree is said in, each led by `host`, the host that says
    /// it: first how many processes besides the main the end was sent to; then how many it left to
    /// a service manager that holds each in a unit of its own ([`left_to_their_units`]); then, in a
    /// line of its own, each call the OS refused the end, as the report says it, and how many
    /// processes the end signalled and could not end, and how many it left running and never
    /// signalled. Each process is named by pid and program, a still-frozen one marked so. Empty
    /// when the end was sent to nothing besides the main, left nothing and nothing was refused.
    pub fn lines(&self, host: &str) -> Vec<String> {
        let named = |left: &[Leftover]| {
            left.iter()
                .map(|left| {
                    let frozen = if left.stopped { ", stopped" } else { "" };
                    format!("{} ({}{frozen})", left.pid, left.program)
                })
                .collect::<Vec<_>>()
                .join(", ")
        };
        let mut parts = Vec::new();
        if !self.residue.is_empty() {
            parts.push(format!(
                "{} it signalled and could not end: {}",
                self.residue.len(),
                named(&self.residue)
            ));
        }
        if !self.skipped.is_empty() {
            parts.push(format!(
                "{} left running, never signalled: {}",
                self.skipped.len(),
                named(&self.skipped)
            ));
        }
        let left = (!parts.is_empty()).then(|| {
            format!(
                "the end of the session left processes running; {}",
                parts.join("; ")
            )
        });
        let said: Vec<String> = self.faults.iter().cloned().chain(left).collect();
        let mut lines = Vec::new();
        if !self.sent.is_empty() {
            let noun = if self.sent.len() == 1 {
                "process"
            } else {
                "processes"
            };
            lines.push(format!(
                "{host}: the end of the session was sent to {} {noun} besides its main: {}",
                self.sent.len(),
                named(&self.sent)
            ));
        }
        if !self.held_apart.is_empty() {
            lines.push(format!(
                "{host}: {}",
                left_to_their_units("the end of the session", &self.held_apart)
            ));
        }
        if !said.is_empty() {
            lines.push(format!("{host}: {}", said.join("; ")));
        }
        lines
    }
}

/// What `end` left running to the service manager that holds each of `held` in a unit of its own,
/// each named by pid, program and unit: one line an end report and a reap say alike.
fn left_to_their_units(end: &str, held: &[HeldApart]) -> String {
    let noun = if held.len() == 1 {
        "process"
    } else {
        "processes"
    };
    let named = held
        .iter()
        .map(|held| {
            format!(
                "{} ({}) in {}",
                held.process.pid, held.process.program, held.unit
            )
        })
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "{end} left {} {noun} running that a service manager holds apart, each in a unit of its \
         own: {named}",
        held.len()
    )
}

/// The OS facilities an end and a sweep use, so a test can add a stranger to the table an end
/// walks, fail a parent read the chain check makes, see what is addressed, or keep one signal from
/// landing. A test can break a chain this way, never make one. The start-time check every signal
/// passes reads the live OS and is never part of this; the reads that confirm where a freeze sent
/// by pid landed, and that a process it reached instead still holds the pid, are, since an answer
/// there can only undo that freeze, or keep it from being undone.
trait EndOs {
    /// The process table ([`crate::os_process::table`]).
    fn table(&mut self) -> Vec<os_process::Entry>;
    /// The parent of `pid` ([`crate::os_process::parent`]), as a signal's chain check reads it.
    fn parent(&mut self, pid: u32) -> Option<u32>;
    /// `pid` addressed for a signal ([`address`]), ahead of every check the signal passes;
    /// `None` when it is gone.
    fn address(&mut self, pid: u32) -> Option<Addressee>;
    /// Send `signal` to `to` ([`send_signal`]).
    fn send(&mut self, signal: Signal, to: &Addressee);
    /// The start time of the process `to` addresses ([`started`]), read again right after a
    /// freeze sent to it by its pid alone, to confirm the freeze reached the process its checks
    /// passed, and once more before a freeze that reached another process is undone, to confirm
    /// that process still holds the pid.
    fn start_after_freeze(&mut self, to: &Addressee) -> Option<u64>;
    /// How a walk of [`EndOs::table`] reads a child started in its parent's own tick
    /// ([`os_process::TIES`]).
    fn ties(&self) -> Ties;
    /// Whether this OS freezes a process ([`FREEZES`]).
    fn freezes(&self) -> bool;
    /// The unit a service manager holds `pid` in apart from the process ending it
    /// ([`held_apart`]), read now.
    fn held_apart(&mut self, pid: u32) -> Option<String>;
}

/// The live OS, why it addresses a process by its pid rather than through a process descriptor
/// ([`no_process_descriptors`]), and the cgroup file of this process, the one ending
/// ([`crate::os_process::cgroup`]), each read once when it is made.
struct LiveOs {
    by_pid_because: Option<String>,
    cgroup: Option<String>,
}

impl LiveOs {
    /// The live OS as this host addresses a process now, seen from this process's cgroup.
    fn new() -> LiveOs {
        LiveOs {
            by_pid_because: no_process_descriptors(),
            cgroup: os_process::cgroup(std::process::id()),
        }
    }

    /// `report`, an end's over this OS, with why the end addressed each process by its pid.
    fn reporting(self, report: EndReport) -> EndReport {
        EndReport {
            by_pid_because: self.by_pid_because,
            ..report
        }
    }
}

impl EndOs for LiveOs {
    fn table(&mut self) -> Vec<os_process::Entry> {
        os_process::table()
    }

    fn parent(&mut self, pid: u32) -> Option<u32> {
        os_process::parent(pid)
    }

    fn address(&mut self, pid: u32) -> Option<Addressee> {
        address(pid, self.by_pid_because.is_none())
    }

    fn send(&mut self, signal: Signal, to: &Addressee) {
        send_signal(signal, to);
    }

    fn start_after_freeze(&mut self, to: &Addressee) -> Option<u64> {
        started(to)
    }

    fn ties(&self) -> Ties {
        os_process::TIES
    }

    fn freezes(&self) -> bool {
        FREEZES
    }

    fn held_apart(&mut self, pid: u32) -> Option<String> {
        held_apart(self.cgroup.as_deref(), os_process::cgroup(pid).as_deref())
    }
}

/// Whether `pid`'s parent chain, read through `parent`, reaches `root`: whether a parent of `pid`,
/// or a parent of one of those, is `root`, read at most [`CHAIN_STEPS`] links up. The walk stops
/// at pid 1 or 0 and at a parent it cannot read, answering that the chain does not reach; a `root`
/// of 1 or 0 is never reached, and nor is `pid` itself, since no process is its own descendant.
fn chain_reaches(pid: u32, root: u32, mut parent: impl FnMut(u32) -> Option<u32>) -> bool {
    if root <= 1 {
        return false;
    }
    let mut current = pid;
    for _ in 0..CHAIN_STEPS {
        match parent(current) {
            Some(next) if next == root => return true,
            Some(next) if next > 1 => current = next,
            _ => return false,
        }
    }
    false
}

/// The tree an end signals into: its root, and this process and its ancestors, which no end ever
/// signals.
struct Tree {
    root: u32,
    self_pid: u32,
    self_ancestors: HashSet<u32>,
}

impl Tree {
    /// The tree under `root`, as this process signals into it.
    fn rooted_at(root: u32) -> Tree {
        let self_pid = std::process::id();
        Tree {
            root,
            self_pid,
            self_ancestors: ancestor_pids(self_pid),
        }
    }

    /// Address `target` through `os` ([`EndOs::address`]), then send `signal` to it through that
    /// address, only if, read right now, `target`'s parent chain in `os` still reaches the root
    /// ([`chain_reaches`]) and its pid still names the process the walk found
    /// ([`toctou_still_same_process`]). A freeze sent by the pid alone, with no descriptor to keep
    /// the pid its process's, is confirmed by reading the start time again
    /// ([`EndOs::start_after_freeze`]): one that reached another process, which took the pid
    /// between the checks and the freeze, is reported in `report`'s faults and counts as not
    /// sent. That process is resumed through the identity check every signal passes, against the
    /// start the read answered, read once more: only while the pid still names it, and otherwise
    /// it is sent nothing more and the fault says so. A hard end or a TERM sent to a target still
    /// running just before it is reported as sent the end ([`EndReport::sent`]), named as it read
    /// then. A target a service manager holds apart ([`EndOs::held_apart`]) is sent nothing but a
    /// resume and is reported held apart. Answers whether it was sent: a process that cannot be
    /// addressed, fails either check, or is held apart, is never signalled.
    fn send(
        &self,
        os: &mut impl EndOs,
        report: &mut EndReport,
        signal: Signal,
        target: &Identity,
    ) -> bool {
        let Some(to) = os.address(target.pid) else {
            return false;
        };
        let holds = chain_reaches(target.pid, self.root, |pid| os.parent(pid))
            && toctou_still_same_process(target, started(&to), self.self_pid, &self.self_ancestors);
        if !holds {
            return false;
        }
        if signal != Signal::Resume {
            if let Some(unit) = os.held_apart(target.pid) {
                report.hold_apart(target, unit);
                return false;
            }
        }
        let ends = matches!(signal, Signal::Term | Signal::Hard);
        let named = (ends && running(target)).then(|| Leftover::of(target.pid, false));
        os.send(signal, &to);
        if signal == Signal::Stop && to.descriptor.is_none() {
            let reached = os.start_after_freeze(&to);
            if let Some(start) = reached.filter(|start| *start != target.start) {
                let another = Identity {
                    pid: target.pid,
                    start,
                };
                let confirmed = toctou_still_same_process(
                    &another,
                    os.start_after_freeze(&to),
                    self.self_pid,
                    &self.self_ancestors,
                );
                if confirmed {
                    os.send(Signal::Resume, &to);
                }
                report
                    .faults
                    .push(freeze_reached_another(target.pid, confirmed));
                return false;
            }
        }
        if let Some(left) = named {
            report.add_sent(left);
        }
        true
    }

    /// The root's descendants in `table`, one list per generation, read by `os`'s tie rule
    /// ([`walk_in`]): each process the walk could not tell from a stranger is reported skipped and
    /// never signalled.
    fn walk(
        &self,
        os: &impl EndOs,
        table: &[os_process::Entry],
        report: &mut EndReport,
    ) -> Vec<Vec<Identity>> {
        let walk = walk_in(table, self.root, os.ties());
        for target in &walk.untrusted {
            report.skip(target);
        }
        walk.generations
    }

    /// Walk the root's descendants in `os`'s table ([`Tree::walk`]) and send `signal` to each
    /// through [`Tree::send`], checking each one afresh at every walk: one the checks refuse is
    /// reported skipped, once, and one a later walk's checks pass is signalled and no longer
    /// reported, since a check can fail for a moment, as when a parent exits and is reaped while
    /// it is read. Answers the walk.
    fn signal_walk(
        &self,
        os: &mut impl EndOs,
        report: &mut EndReport,
        signal: Signal,
    ) -> Vec<Identity> {
        let table = os.table();
        let walk = self.walk(os, &table, report).concat();
        for target in &walk {
            if self.send(os, report, signal, target) {
                report.unskip(target);
            } else {
                report.skip(target);
            }
        }
        walk
    }
}

/// Whether `target` is still running: its pid still names it and it has not exited. A process
/// that has exited and is not yet reaped has closed every descriptor it held.
pub(crate) fn running(target: &Identity) -> bool {
    still_started(target.pid, target.start) && os_process::state(target.pid) != Some('Z')
}

/// Whether a process in `state` ([`crate::os_process::state`]) is frozen: stopped, by a signal or
/// by a tracer.
fn frozen_state(state: char) -> bool {
    matches!(state, 'T' | 't')
}

/// Whether `pid` is frozen.
fn frozen(pid: u32) -> bool {
    os_process::state(pid).is_some_and(frozen_state)
}

/// Whether `pid` is frozen, has exited, or is gone.
fn frozen_or_gone(pid: u32) -> bool {
    os_process::state(pid).is_none_or(|state| frozen_state(state) || state == 'Z')
}

/// Reads `done` every [`POLL`] until it holds or `deadline` passes; whether it held.
pub(crate) fn poll_until(deadline: Instant, mut done: impl FnMut() -> bool) -> bool {
    loop {
        if done() {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(POLL);
    }
}

/// The processes of the tree under `main` still running, read without signalling any: `main`,
/// while its pid still names it, and every running descendant a walk finds under it. A tree whose
/// keeper is gone is reported this way, since what the keeper held is no longer this process's to
/// signal.
pub fn left_running(main: &Identity) -> Vec<Leftover> {
    if !running(main) {
        return Vec::new();
    }
    let mut left = vec![Leftover::of(main.pid, false)];
    left.extend(
        descendants_of(main.pid)
            .iter()
            .filter(|target| running(target))
            .map(|target| Leftover::of(target.pid, false)),
    );
    left
}

/// End the process tree under the held child `main`, which this process spawned and has not yet
/// reaped. `remembered` are the processes a watch of the tree saw while it ran.
///
/// The main is frozen first, then the tree under it one generation at a time, parents before
/// children ([`Tree::walk`]): each generation is read from a fresh table once the one above it
/// is confirmed frozen, and a process is frozen only while its parent reads frozen. A frozen
/// parent can neither exit nor reap, so the parent chain of every process the end froze stays
/// whole while the end runs. A process whose parent does not read frozen is ended without a
/// freeze. The walk is repeated until one finds nothing new or the bound passes; past the bound a
/// walk freezes nothing, so a member that never freezes (an OS with no freeze, or a process that
/// cannot stop) cannot keep the end walking. Every process the end found is then ended one
/// generation at a time, by its depth when first found, the deepest first, each confirmed gone
/// before its parents are ended ([`end_found`]). The tree is then walked again, and what the walks
/// find that no end pass took, as a process a member that never froze started meanwhile, is frozen
/// and ended the same way, round after round, until a round ends nothing new or the bound passes.
/// The main is ended last, reaped through its handle. A process started meanwhile by a member an
/// end pass already ended has left the tree with its parent's end, where no chain reaches it, and
/// is neither signalled nor reported. No process is asked to exit first: one that exited on asking
/// would leave its children to an ancestor outside the tree, where no chain reaches them.
///
/// Every signal to a process other than the main passes [`Tree::send`]'s checks; the main is
/// signalled through the pid its unreaped handle keeps its own. Each walk checks every process it
/// lists afresh, and one that refuses a process no walk refused before is followed by another: a
/// process the checks refuse at every walk, and a remembered one still running outside the tree, is
/// reported skipped and never signalled, and one a service manager holds apart is never frozen or
/// ended and is reported so ([`EndReport::held_apart`]). A process still running past [`END_BOUND`]
/// is resumed if the end froze it, and reported as residue. One the end froze and then could no
/// longer reach, as when a third party ends its frozen parent, is reported still frozen: no signal
/// leaves the tree, a resume among them. A main that has already exited is reaped and nothing is
/// signalled: what it left has a parent outside the tree, and its pid may name another process.
pub fn end_watched(main: &mut std::process::Child, remembered: &[Identity]) -> EndReport {
    let mut os = LiveOs::new();
    let report = end_watched_with(&mut os, main, remembered, END_BOUND);
    os.reporting(report)
}

/// What a watched end found of the tree under its main: each process at its depth when first
/// found, those it froze, those whose checks refused a freeze, and those an end pass took
/// ([`end_found`]).
#[derive(Default)]
struct Found {
    by_depth: Vec<Vec<Identity>>,
    listed: HashSet<Identity>,
    stopped: HashSet<Identity>,
    refused: HashSet<Identity>,
    taken: HashSet<Identity>,
}

impl Found {
    /// Record `target` at `depth` unless it was found before; whether it is new.
    fn record(&mut self, target: Identity, depth: usize) -> bool {
        if !self.listed.insert(target) {
            return false;
        }
        if self.by_depth.len() <= depth {
            self.by_depth.resize_with(depth + 1, Vec::new);
        }
        self.by_depth[depth].push(target);
        true
    }
}

/// One walk of the tree under `tree`'s root for [`end_watched`], a generation at a time: each
/// generation is read from a fresh table and every member recorded in `found`; each member not yet
/// frozen is frozen while its parent reads frozen, and the generation is confirmed frozen, until
/// `deadline`, before the next is read. Once `deadline` has passed, every generation left is
/// recorded and none frozen. A member an end pass already took ([`Found::taken`]) is not frozen
/// again. Answers whether the walk found a process, froze one, or had one refused, that no walk had
/// before.
fn freeze_walk(
    os: &mut impl EndOs,
    tree: &Tree,
    found: &mut Found,
    report: &mut EndReport,
    deadline: Instant,
) -> bool {
    let mut progress = false;
    for depth in 0.. {
        let table = os.table();
        let generations = tree.walk(os, &table, report);
        if generations.len() <= depth {
            break;
        }
        if Instant::now() >= deadline {
            for (deeper, generation) in generations.iter().enumerate().skip(depth) {
                for target in generation {
                    progress |= found.record(*target, deeper);
                }
            }
            break;
        }
        let mut new = Vec::new();
        for target in &generations[depth] {
            progress |= found.record(*target, depth);
            let parent_frozen = || {
                table
                    .iter()
                    .find(|entry| entry.pid == target.pid)
                    .is_some_and(|entry| frozen(entry.parent))
            };
            let settled = found.stopped.contains(target) || found.taken.contains(target);
            if !os.freezes() || settled || !parent_frozen() {
                continue;
            }
            if tree.send(os, report, Signal::Stop, target) {
                report.unskip(target);
                found.stopped.insert(*target);
                new.push(target.pid);
                progress = true;
            } else {
                report.skip(target);
                progress |= found.refused.insert(*target);
            }
        }
        poll_until(deadline, || new.iter().all(|&pid| frozen_or_gone(pid)));
    }
    progress
}

/// [`end_watched`] over `os`, its confirmations bounded by `bound`.
fn end_watched_with(
    os: &mut impl EndOs,
    main: &mut std::process::Child,
    remembered: &[Identity],
    bound: Duration,
) -> EndReport {
    let mut report = EndReport::default();
    let deadline = Instant::now() + bound;
    if !matches!(main.try_wait(), Ok(None)) {
        for target in remembered {
            report.skip(target);
        }
        return report;
    }
    let main_pid = main.id();
    let main_at = Addressee::by_pid(main_pid);
    let tree = Tree::rooted_at(main_pid);
    os.send(Signal::Stop, &main_at);
    if os.freezes() {
        poll_until(deadline, || frozen_or_gone(main_pid));
    }
    let mut found = Found::default();
    loop {
        while freeze_walk(os, &tree, &mut found, &mut report, deadline) && Instant::now() < deadline
        {
        }
        if !end_found(os, &tree, &mut found, &mut report, deadline) || Instant::now() >= deadline {
            break;
        }
    }
    if matches!(main.try_wait(), Ok(None)) {
        os.send(Signal::Hard, &main_at);
    }
    let main_deadline = deadline.max(Instant::now() + FLOOR);
    if !poll_until(main_deadline, || !matches!(main.try_wait(), Ok(None))) {
        os.send(Signal::Resume, &main_at);
        report.residue.push(Leftover::of(main_pid, false));
    }
    for target in remembered
        .iter()
        .filter(|target| !found.listed.contains(target))
    {
        report.skip(target);
    }
    report
}

/// One end pass of [`end_watched`]: each process in `found` that no pass took yet
/// ([`Found::taken`]) is taken, one generation at a time, by its depth when first found, the
/// deepest first. Each is sent a hard end through [`Tree::send`], and the generation is confirmed
/// gone, until `deadline` or for [`FLOOR`] at least, before its parents are ended. One still
/// running then is resumed if the end froze it, and reported as residue; one the checks refuse is
/// reported skipped, or, where the end froze it and it still runs, as residue. Answers whether it
/// took any.
fn end_found(
    os: &mut impl EndOs,
    tree: &Tree,
    found: &mut Found,
    report: &mut EndReport,
    deadline: Instant,
) -> bool {
    let mut took = false;
    for generation in found.by_depth.iter().rev() {
        let mut ended = Vec::new();
        for target in generation {
            if !found.taken.insert(*target) {
                continue;
            }
            took = true;
            let stopped = found.stopped.contains(target);
            if tree.send(os, report, Signal::Hard, target) {
                report.unskip(target);
                ended.push(*target);
            } else if !stopped {
                report.skip(target);
            } else if running(target) {
                report.residue.push(Leftover::of(target.pid, true));
            }
        }
        let generation_deadline = deadline.max(Instant::now() + FLOOR);
        poll_until(generation_deadline, || !ended.iter().any(running));
        for target in ended.iter().filter(|target| running(target)) {
            let stopped = found.stopped.contains(target);
            let resumed = stopped && tree.send(os, report, Signal::Resume, target);
            report
                .residue
                .push(Leftover::of(target.pid, stopped && !resumed));
        }
    }
    took
}

/// End every descendant of this process: the keeper's end, where every orphan of the tree it
/// keeps reparents to it, so every descendant's parent chain reaches it. `graceful` first asks
/// each to exit and waits up to [`GRACE`] for them; then each one a walk finds is ended, walking
/// again until `children_left` answers that no child is left, a walk finds nothing still running
/// but processes a service manager holds apart, which are left running and reported so
/// ([`EndReport::held_apart`]), or [`END_BOUND`] passes, when what the last walk ended is given up
/// to [`FLOOR`] to be gone and what is still running is reported as residue. Every signal passes
/// [`Tree::send`]'s checks, made afresh at every walk; a process they refuse at every walk is
/// reported skipped and never signalled.
pub fn end_own_descendants(graceful: bool, children_left: impl FnMut() -> bool) -> EndReport {
    let mut os = LiveOs::new();
    let report = end_descendants_with(
        &mut os,
        std::process::id(),
        graceful,
        children_left,
        END_BOUND,
    );
    os.reporting(report)
}

/// [`end_own_descendants`] over `os`, from `root`, bounded by `bound`.
fn end_descendants_with(
    os: &mut impl EndOs,
    root: u32,
    graceful: bool,
    mut children_left: impl FnMut() -> bool,
    bound: Duration,
) -> EndReport {
    let mut report = EndReport::default();
    let deadline = Instant::now() + bound;
    let tree = Tree::rooted_at(root);
    if graceful {
        tree.signal_walk(os, &mut report, Signal::Term);
        let grace_end = Instant::now() + GRACE;
        poll_until(grace_end, || !children_left());
    }
    while children_left() {
        let walk = tree.signal_walk(os, &mut report, Signal::Hard);
        if report.only_held_apart_runs(&walk) {
            break;
        }
        if Instant::now() >= deadline {
            let ended: Vec<Identity> = walk
                .into_iter()
                .filter(|target| {
                    let left = |pid| report.skipped.iter().any(|left| left.pid == pid);
                    !left(target.pid) && !report.is_held_apart(target.pid)
                })
                .collect();
            poll_until(Instant::now() + FLOOR, || !ended.iter().any(running));
            for target in ended.iter().filter(|target| running(target)) {
                report.residue.push(Leftover::of(target.pid, false));
            }
            break;
        }
        std::thread::sleep(POLL);
    }
    report
}

std::cfg_select! {
    target_os = "windows" => {
        /// End the job object `job`, which this process created and alone holds, and every process
        /// in it, the held child `main` among them: the job end's ONE sanctioned site
        /// (`TerminateJobObject`, the `no-os-kill` gate). The job's members are read first
        /// ([`job_members`]); the end then waits, for at most [`END_BOUND`], until the job counts
        /// no active process and no member read still runs ([`Member::running`]). The count alone
        /// answers as soon as the OS marks each process ended, before the process has exited and
        /// closed its handles: on a windows-latest runner it read 0 at its first read, 0.14 ms
        /// after the end, while a member still ran and held the output (CI run 44258832257, from
        /// diagnostic commit 137580dc7, since dropped). Past the bound, the job is closed, which
        /// ends what it still holds (`JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`), and the processes that
        /// still ran are waited on for [`FLOOR`]: each the close ended is named in a fault, and
        /// each still running is reported as residue. Then the main is reaped. A job the OS will
        /// not end is reported as a fault, with no wait for it, since nothing was sent to it. The
        /// kernel names a job's members, so no member is found by a walk, and none is signalled
        /// one at a time; each member but the main still running just before the end is reported
        /// as sent it ([`EndReport::sent`]).
        pub fn end_job(
            job: std::os::windows::io::OwnedHandle,
            main: &mut std::process::Child,
        ) -> EndReport {
            end_job_with(job, main, END_BOUND)
        }

        /// [`end_job`], waiting at most `bound` for the job's processes to exit before the job is
        /// closed.
        pub(crate) fn end_job_with(
            job: std::os::windows::io::OwnedHandle,
            main: &mut std::process::Child,
            bound: Duration,
        ) -> EndReport {
            use std::os::windows::io::AsRawHandle;
            use windows_sys::Win32::Foundation::FALSE;
            use windows_sys::Win32::System::JobObjects::TerminateJobObject;
            let handle = job.as_raw_handle();
            let mut report = EndReport::default();
            let (held, whole) = job_members(handle);
            if !whole {
                report.faults.push(format!(
                    "the job of session {} could not be listed whole, so the end waited only for \
                     the processes it listed",
                    main.id()
                ));
            }
            let main_pid = main.id();
            let sent: Vec<Leftover> = held
                .iter()
                .filter(|member| member.identity.pid != main_pid && member.running())
                .map(|member| Leftover::of(member.identity.pid, false))
                .collect();
            // SAFETY: the handle is a job this process created and still holds; the call takes
            // no pointer.
            let terminated = unsafe { TerminateJobObject(handle, 1) } != FALSE;
            report.sent = if terminated { sent } else { Vec::new() };
            if !terminated {
                let error = std::io::Error::last_os_error();
                report.faults.push(format!(
                    "the job of session {} could not be ended ({error}), so the end did not wait \
                     for it",
                    main.id()
                ));
                let _ = main.try_wait();
            } else if poll_until(Instant::now() + bound, || {
                job_active(handle) == Some(0) && !held.iter().any(Member::running)
            }) {
                let _ = main.wait();
            } else {
                let mut late: Vec<Member> =
                    held.into_iter().filter(|member| member.running()).collect();
                let (listed, _) = job_members(handle);
                for member in listed {
                    if !late.iter().any(|known| known.identity == member.identity) {
                        late.push(member);
                    }
                }
                drop(job);
                poll_until(Instant::now() + FLOOR, || !late.iter().any(Member::running));
                let (left, ended): (Vec<&Member>, Vec<&Member>) =
                    late.iter().partition(|member| member.running());
                if !ended.is_empty() {
                    let pids: Vec<String> = ended
                        .iter()
                        .map(|member| member.identity.pid.to_string())
                        .collect();
                    report.faults.push(format!(
                        "the job of session {} still held {} past the end's bound; closing the \
                         job ended them",
                        main.id(),
                        pids.join(", ")
                    ));
                }
                report.residue = left
                    .iter()
                    .map(|member| Leftover::of(member.identity.pid, false))
                    .collect();
                let _ = main.try_wait();
                return report;
            }
            drop(job);
            report
        }

        /// How many processes the job `job` counts as active, or `None` when it cannot be read. A
        /// process stops counting once the OS marks it ended, which can be before it has exited:
        /// on a windows-latest runner the count read 0 at its first read, 0.14 ms after the end,
        /// while a member still ran and held the output (CI run 44258832257, from diagnostic
        /// commit 137580dc7, since dropped).
        fn job_active(job: windows_sys::Win32::Foundation::HANDLE) -> Option<u32> {
            use windows_sys::Win32::Foundation::FALSE;
            use windows_sys::Win32::System::JobObjects::{
                JobObjectBasicAccountingInformation, QueryInformationJobObject,
                JOBOBJECT_BASIC_ACCOUNTING_INFORMATION,
            };
            let mut counts = JOBOBJECT_BASIC_ACCOUNTING_INFORMATION::default();
            // SAFETY: `counts` is the writable structure the information class names, of the size
            // given, and no returned length is asked for.
            let read = unsafe {
                QueryInformationJobObject(
                    job,
                    JobObjectBasicAccountingInformation,
                    (&raw mut counts).cast(),
                    size_of::<JOBOBJECT_BASIC_ACCOUNTING_INFORMATION>() as u32,
                    std::ptr::null_mut(),
                )
            } != FALSE;
            read.then_some(counts.ActiveProcesses)
        }

        /// A process read as a job's member ([`job_member`]): its identity, and the handle it was
        /// read through, which keeps its pid its own while the member is held.
        pub(crate) struct Member {
            pub(crate) identity: Identity,
            handle: std::os::windows::io::OwnedHandle,
        }

        impl Member {
            /// Whether the process still runs: a wait on its handle for no time times out while it
            /// runs, and answers once it has exited.
            pub(crate) fn running(&self) -> bool {
                use std::os::windows::io::AsRawHandle;
                use windows_sys::Win32::Foundation::WAIT_TIMEOUT;
                use windows_sys::Win32::System::Threading::WaitForSingleObject;
                // SAFETY: the handle is open with synchronize access and stays open across the
                // call; a wait for no time takes no pointer.
                let waited = unsafe { WaitForSingleObject(self.handle.as_raw_handle(), 0) };
                waited == WAIT_TIMEOUT
            }
        }

        /// The process `pid` read as a member of the job `job`: opened for waiting on and for
        /// reading its times, proved in the job through that handle (`IsProcessInJob`), and its
        /// start time read through the same handle. `None` when it cannot be opened, is not in the
        /// job (its pid now names a process outside it), or its start time cannot be read.
        pub(crate) fn job_member(
            job: windows_sys::Win32::Foundation::HANDLE,
            pid: u32,
        ) -> Option<Member> {
            use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
            use windows_sys::Win32::Foundation::FALSE;
            use windows_sys::Win32::System::JobObjects::IsProcessInJob;
            use windows_sys::Win32::System::Threading::{
                OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SYNCHRONIZE,
            };
            // SAFETY: the call takes no pointer and answers a null handle on failure.
            let opened = unsafe {
                OpenProcess(PROCESS_SYNCHRONIZE | PROCESS_QUERY_LIMITED_INFORMATION, FALSE, pid)
            };
            if opened.is_null() {
                return None;
            }
            // SAFETY: the handle is the one the call just answered, owned by nothing else.
            let handle = unsafe { OwnedHandle::from_raw_handle(opened) };
            let mut inside = FALSE;
            // SAFETY: both handles are open across the call and `inside` is a writable BOOL.
            let read = unsafe { IsProcessInJob(handle.as_raw_handle(), job, &mut inside) } != FALSE;
            if !read || inside == FALSE {
                return None;
            }
            let start = os_process::windows::created(handle.as_raw_handle())?;
            Some(Member {
                identity: Identity { pid, start },
                handle,
            })
        }

        /// Each process the job `job` holds ([`job_pids`]), read as a member ([`job_member`]); one
        /// gone, or outside the job, by the time it is opened is not named. Whether the job's list
        /// was read whole.
        pub(crate) fn job_members(
            job: windows_sys::Win32::Foundation::HANDLE,
        ) -> (Vec<Member>, bool) {
            let (pids, whole) = job_pids(job);
            let members = pids.into_iter().filter_map(|pid| job_member(job, pid)).collect();
            (members, whole)
        }

        /// The pid of each process the job `job` holds, its nested jobs' among them, read until
        /// the list is whole ([`whole_list`]), and whether it is.
        fn job_pids(job: windows_sys::Win32::Foundation::HANDLE) -> (Vec<u32>, bool) {
            use windows_sys::Win32::Foundation::FALSE;
            use windows_sys::Win32::System::JobObjects::{
                JobObjectBasicProcessIdList, QueryInformationJobObject,
                JOBOBJECT_BASIC_PROCESS_ID_LIST,
            };
            // The list is two counts and then one word per pid: `header` words come before the
            // first pid.
            let header = std::mem::offset_of!(JOBOBJECT_BASIC_PROCESS_ID_LIST, ProcessIdList)
                / size_of::<usize>();
            whole_list(|room| {
                let mut list = vec![0usize; header + room];
                // SAFETY: `list` is the given number of writable bytes, aligned as the structure
                // is, and no returned length is asked for.
                let read = unsafe {
                    QueryInformationJobObject(
                        job,
                        JobObjectBasicProcessIdList,
                        list.as_mut_ptr().cast(),
                        (list.len() * size_of::<usize>()) as u32,
                        std::ptr::null_mut(),
                    )
                } != FALSE;
                // SAFETY: `list` holds at least the structure's counts and its first pid, and is
                // aligned for it.
                let counts =
                    unsafe { list.as_ptr().cast::<JOBOBJECT_BASIC_PROCESS_ID_LIST>().read() };
                let listed = if read {
                    (counts.NumberOfProcessIdsInList as usize).min(room)
                } else {
                    0
                };
                ListRead {
                    read,
                    pids: list[header..header + listed].iter().map(|&pid| pid as u32).collect(),
                    assigned: counts.NumberOfAssignedProcesses as usize,
                }
            })
        }
    }
    _ => {}
}

/// One read of a job's process list ([`whole_list`]): whether the call succeeded, the pids it
/// listed, and how many processes the job counted.
#[cfg(any(windows, test))]
struct ListRead {
    read: bool,
    pids: Vec<u32>,
    assigned: usize,
}

/// How many times [`whole_list`] reads a job's process list at most.
#[cfg(any(windows, test))]
const LIST_READS: usize = 4;

/// The list `read` answers, and whether it is whole: every process the job counted at that read.
/// `read` is given room for 64 pids first. A list shorter than the count its read reports, from a
/// call that failed for want of room or one that succeeded with only what fit, is read again into
/// room for that count, at most [`LIST_READS`] times in all, since processes can join the job
/// between reads; the longest list read is answered when none is whole. A call that fails with
/// room to spare is not read again.
#[cfg(any(windows, test))]
fn whole_list(mut read: impl FnMut(usize) -> ListRead) -> (Vec<u32>, bool) {
    let mut room = 64;
    let mut longest = Vec::new();
    for _ in 0..LIST_READS {
        let answer = read(room);
        if answer.read && answer.pids.len() >= answer.assigned {
            return (answer.pids, true);
        }
        if !answer.read && answer.assigned <= room {
            break;
        }
        room = room.max(answer.assigned);
        if answer.pids.len() > longest.len() {
            longest = answer.pids;
        }
    }
    (longest, false)
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
    let base = strictly_under(base_dir, authorized_root);
    if base.is_none() {
        let root_display = authorized_root.canonicalize().map_or_else(
            |_| authorized_root.display().to_string(),
            |root| root.display().to_string(),
        );
        crate::whole_line::say(format_args!(
            "rigger: reap refused: {} is not strictly under {root_display}",
            base_dir.display()
        ));
    }
    base
}

/// Where `path` resolves, when that is strictly under `root`: the one containment test for
/// anything rigger reaps, removes or takes over under a root it was handed. Both sides are
/// compared as resolved paths, component by component, never as string prefixes: `root` must
/// canonicalize, and `path` is resolved lexically ([`resolve_lexically`]), so it need not exist
/// while a link in whatever part of it does exist is still followed. `None` for a root that
/// cannot be resolved (an empty one included), a path equal to the root, or a path that resolves
/// outside it.
pub fn strictly_under(path: &Path, root: &Path) -> Option<PathBuf> {
    let root = root.canonicalize().ok()?;
    let resolved = resolve_lexically(path)?;
    (resolved != root && resolved.starts_with(&root)).then_some(resolved)
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
///
/// A process a service manager holds in a unit other than this process's is left running, never
/// signalled, and named with its unit in one line ([`left_to_their_units`]).
pub fn reap_authorized(base: PathBuf) {
    let term_targets = scan_with_starttime(&base);
    if term_targets.is_empty() {
        return;
    }
    let os = &mut LiveOs::new();
    if let Some(reason) = &os.by_pid_because {
        say_signalled_by_pid(reason);
    }
    let held = reap_authorized_with(os, &base, term_targets);
    if !held.is_empty() {
        let reap = format!("the reap of {}", base.display());
        crate::whole_line::say(format_args!(
            "rigger: {}",
            left_to_their_units(&reap, &held)
        ));
    }
}

/// [`reap_authorized`]'s signals over `os`, from `term_targets`, the scan of `base` it began
/// with: answers each process it left to the unit a service manager holds it in, once.
fn reap_authorized_with(
    os: &mut impl EndOs,
    base: &Path,
    term_targets: Vec<Identity>,
) -> Vec<HeldApart> {
    let self_pid = std::process::id();
    let self_ancestors = ancestor_pids(self_pid);
    let mut report = EndReport::default();
    for target in &term_targets {
        let held = signal_if_unchanged(os, target, base, self_pid, &self_ancestors, Signal::Term);
        if let Some(unit) = held {
            report.hold_apart(target, unit);
        }
    }
    std::thread::sleep(GRACE);
    // Re-scan so only processes STILL rooted inside are force-killed - each gets its own
    // fresh start-time baseline here, then is rechecked again immediately below.
    for target in scan_with_starttime(base) {
        let held = signal_if_unchanged(os, &target, base, self_pid, &self_ancestors, Signal::Hard);
        if let Some(unit) = held {
            report.hold_apart(&target, unit);
        }
    }
    report.held_apart
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(unix)]
    use crate::test_support::assert_teardown_reaps_what_is_rooted_inside;
    use crate::test_support::cleanup;
    #[cfg(unix)]
    use crate::test_support::release;
    #[cfg(unix)]
    use crate::test_support::sigterm_ignorer_in;
    use crate::test_support::sleeper_in;
    use crate::test_support::wait_until;
    #[cfg(unix)]
    use std::process::Child;
    use std::process::Command as StdCommand;

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
    #[cfg(unix)]
    fn wait_for_exit(child: &mut Child) -> bool {
        wait_until(|| matches!(child.try_wait(), Ok(Some(_))))
    }

    // Unix only: Windows reports no process's working directory.
    #[cfg(unix)]
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

    // Unix only: Windows reports no process's working directory.
    #[cfg(unix)]
    #[test]
    fn processes_rooted_under_names_a_process_by_its_executable_never_its_arguments() {
        let root = tempfile::tempdir().unwrap();
        let mut inside = sleeper_in(root.path());
        // Until its exec is done the sleeper is still this test's own image, already in its cwd.
        wait_until(|| {
            processes_rooted_under(root.path())
                .iter()
                .any(|(pid, _)| *pid == inside.id())
                && !os_process::is_unexeced_fork_of(inside.id(), std::process::id())
        });
        let program = os_process::executable_name(inside.id());

        let named: Vec<String> = processes_rooted_under(root.path())
            .into_iter()
            .filter(|(pid, _)| *pid == inside.id())
            .map(|(_, name)| name)
            .collect();

        cleanup(&mut inside);
        assert_eq!(
            (named, program.contains("300")),
            (vec![program], false),
            "the scan names the sleeper by its executable, never its argument `300`"
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

    // Unix only: Windows reports no process's working directory.
    #[cfg(unix)]
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

    // Unix only: Windows reports no process's working directory.
    #[cfg(unix)]
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

    // Unix only: Windows reports no process's working directory.
    #[cfg(unix)]
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

    // Unix only: Windows reports no process's working directory.
    #[cfg(unix)]
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

    // Unix only: Windows reports no process's working directory.
    #[cfg(unix)]
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
    #[cfg(unix)]
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
    #[cfg(unix)]
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
        // Unix only: Windows reports no process's working directory.
        #[cfg(unix)]
        reap_authorized_kills_a_sigterm_ignoring_process_given_an_already_authorized_base:
            assert_reap_authorized_ends(|dir, _| sigterm_ignorer_in(dir), (None, false));
        /// The request to exit comes first: a process that handles it runs its handler and exits
        /// 0 within the grace, before anything ends it without asking.
        // Unix only: Windows reports no process's working directory.
        #[cfg(unix)]
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

    /// [`address`] answers `None` for `pid`, with descriptors and without: nothing is ever
    /// addressed there, so no signal can be sent to it.
    fn never_addressed(pid: u32) {
        for by_descriptor in [true, false] {
            assert!(
                address(pid, by_descriptor).is_none(),
                "pid {pid} is addressed (by_descriptor {by_descriptor})"
            );
        }
    }

    crate::test_cases! {
        pid_0_is_never_addressed: never_addressed(0);
        init_is_never_addressed: never_addressed(1);
    }

    /// A job's process list, read by [`whole_list`] from a fake job that counts `counts[i]`
    /// processes at its `i`-th read: a read into room for fewer lists only those that fit, and
    /// then succeeds where `short_reads_succeed` and fails otherwise, since the OS is documented
    /// to answer either way. Answers how many pids the list holds, whether it is whole, and the
    /// room each read was given.
    fn listed_from(counts: &[usize], short_reads_succeed: bool) -> (usize, bool, Vec<usize>) {
        let mut rooms = Vec::new();
        let mut reads = counts.iter();
        let (pids, whole) = whole_list(|room| {
            rooms.push(room);
            let assigned = *reads.next().expect("no more reads than the job's counts");
            let fits = assigned.min(room);
            let read = fits == assigned || short_reads_succeed;
            ListRead {
                read,
                pids: if read {
                    (0..fits as u32).collect()
                } else {
                    Vec::new()
                },
                assigned,
            }
        });
        (pids.len(), whole, rooms)
    }

    // Pure rows: the fake job stands in for the OS's list call, which only Windows has.
    crate::test_cases! {
        a_job_the_first_read_has_room_for_is_read_once:
            assert_eq!(listed_from(&[3], false), (3, true, vec![64]));
        a_job_longer_than_the_first_read_is_read_again_into_room_for_all_when_the_short_read_fails:
            assert_eq!(listed_from(&[100, 100], false), (100, true, vec![64, 100]));
        a_job_longer_than_the_first_read_is_read_again_into_room_for_all_when_the_short_read_lists_part:
            assert_eq!(listed_from(&[100, 100], true), (100, true, vec![64, 100]));
        a_job_that_grows_at_every_read_answers_its_longest_list_as_not_whole:
            assert_eq!(
                listed_from(&[100, 200, 300, 400], true),
                (300, false, vec![64, 100, 200, 300])
            );
        a_read_that_fails_with_room_to_spare_is_not_read_again_and_lists_nothing: {
            let mut reads = 0;
            let listed = whole_list(|_| {
                reads += 1;
                ListRead {
                    read: false,
                    pids: Vec::new(),
                    assigned: 0,
                }
            });
            assert_eq!((listed, reads), ((Vec::new(), false), 1));
        };
    }

    /// What two reads of one cache through [`probed`] answer when the probe would answer `first`
    /// and then `second`, and how many times it ran: an answer of 9 stands for an error that may
    /// pass, every other answer for one that lasts.
    fn read_twice(first: Option<u8>, second: Option<u8>) -> ([Option<u8>; 2], usize) {
        let cache = std::sync::OnceLock::new();
        let mut answers = [first, second].into_iter();
        let mut probes = 0;
        let mut read = || {
            probed(
                &cache,
                || {
                    probes += 1;
                    answers.next().flatten()
                },
                |answer: &Option<u8>| *answer != Some(9),
            )
        };
        let read = [read(), read()];
        (read, probes)
    }

    crate::test_cases! {
        a_probe_that_passes_is_kept_and_not_run_again:
            assert_eq!(read_twice(None, Some(1)), ([None, None], 1));
        a_probe_that_fails_for_good_is_kept_and_not_run_again:
            assert_eq!(read_twice(Some(1), None), ([Some(1), Some(1)], 1));
        a_probe_that_fails_for_now_is_run_again:
            assert_eq!(read_twice(Some(9), None), ([Some(9), None], 2));
    }

    /// Whether each of `reasons`, in turn, is said ([`first_said`]): each is said the first
    /// time only.
    fn said(reasons: &[&str]) -> Vec<bool> {
        let mut seen = Vec::new();
        reasons
            .iter()
            .map(|reason| first_said(&mut seen, reason))
            .collect()
    }

    crate::test_cases! {
        a_reason_is_said_once: assert_eq!(said(&["a", "a"]), [true, false]);
        each_reason_is_said_once: assert_eq!(said(&["a", "b", "a", "b"]), [true, true, false, false]);
    }

    #[test]
    fn a_host_without_process_descriptors_says_its_sessions_are_signalled_by_pid() {
        assert_eq!(
            signalled_by_pid("this kernel has no process descriptors"),
            "rigger: process descriptors unavailable: this kernel has no process descriptors; a \
             session's processes are signalled by pid after a check, and a process that takes a \
             checked pid before the signal lands can receive it"
        );
    }

    /// An end over the live OS reports why it signalled by pid when it did.
    #[test]
    fn an_end_that_signalled_by_pid_says_why_in_its_report() {
        let os = LiveOs {
            by_pid_because: Some("this kernel has no process descriptors".to_string()),
            cgroup: None,
        };

        let report = os.reporting(EndReport::default());

        assert_eq!(
            report.by_pid_because.as_deref(),
            Some("this kernel has no process descriptors")
        );
    }

    /// Process handles, this OS's process descriptors: a handle opened on a process keeps that
    /// process's pid its own until the handle closes, so what is read and sent through it reaches
    /// that process or none.
    #[cfg(windows)]
    mod handles {
        use super::*;

        /// Two running processes and an address naming the second's pid that holds a handle
        /// opened on the first: whether the identity check reads each one's start time through
        /// it, and which of them a hard end sent through it ends.
        #[test]
        fn a_handle_opened_on_the_first_reaches_the_first_whatever_pid_is_named() {
            let mut held = [sleeper_in(Path::new("/")), sleeper_in(Path::new("/"))];
            let starts = held
                .each_ref()
                .map(|child| pid_starttime(child.id()).expect("a running process's start time"));
            let to = Addressee {
                pid: held[1].id(),
                descriptor: address(held[0].id(), true).and_then(|to| to.descriptor),
            };
            assert!(
                to.descriptor.is_some(),
                "a handle opens on a running process"
            );

            let read = starts.map(|start| started(&to) == Some(start));
            send_signal(Signal::Hard, &to);
            wait_until(|| {
                held.iter_mut()
                    .any(|child| matches!(child.try_wait(), Ok(Some(_))))
            });
            let ended = held
                .each_mut()
                .map(|child| matches!(child.try_wait(), Ok(Some(_))));

            for child in &mut held {
                cleanup(child);
            }
            assert_eq!((read, ended), ([true, false], [true, false]));
        }

        /// A process is addressed through a handle opened on it while any handle holds it, running
        /// or exited, and its start time is read through that handle; a pid no process holds opens
        /// none and is not addressed. An exited process addressed this way reads as not running
        /// ([`running`]), so an end neither reports it sent nor waits on it.
        #[test]
        fn a_held_process_is_addressed_through_a_handle_and_a_pid_no_process_holds_is_not() {
            let mut live = sleeper_in(Path::new("/"));
            // Its `Child` holds a handle on it past its exit, so its pid stays its own.
            let mut exited = sleeper_in(Path::new("/"));
            let pids = [live.id(), exited.id()];
            let starts =
                pids.map(|pid| pid_starttime(pid).expect("a running process's start time"));
            cleanup(&mut exited);

            let addressed = [pids[0], pids[1], u32::MAX]
                .map(|pid| address(pid, true).map(|to| (to.descriptor.is_some(), started(&to))));
            let read_running: [bool; 2] = std::array::from_fn(|i| {
                running(&Identity {
                    pid: pids[i],
                    start: starts[i],
                })
            });

            cleanup(&mut live);
            assert_eq!(
                (addressed, read_running),
                (
                    [
                        Some((true, Some(starts[0]))),
                        Some((true, Some(starts[1]))),
                        None
                    ],
                    [true, false]
                ),
                "the running process and the exited one still held are each addressed through a \
                 handle that reads its start time, and only the running one reads as running; the \
                 pid no process holds is not addressed"
            );
        }
    }

    // Unix only: on Windows the fixture shell's `$!` is a pid of the shell's own, not the OS's.
    #[cfg(unix)]
    #[test]
    fn ancestor_pids_walks_through_an_intermediate_process() {
        // The test binary itself runs as pid 1 of its own namespace (`.cargo/pidns-runner.sh`),
        // so `ancestor_pids` on a DIRECT child would trivially stop at that pid (already
        // refused unconditionally elsewhere, so never separately recorded - see
        // `is_signal_eligible`). Spawn a GRANDCHILD instead (a shell that backgrounds a
        // child and prints its pid) so the walk must pass THROUGH a real intermediate pid (the
        // shell) to prove it is a multi-hop walk, not a single parent read.
        let (shell, grandchild_pid) = shell_with_a_child();

        let ancestors = ancestor_pids(grandchild_pid);
        let shell_pid = shell.id();
        release(shell);

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

    /// `(pid, start)` of each process in `found`.
    fn pids_and_starts(found: &[Identity]) -> Vec<(u32, u64)> {
        found.iter().map(|entry| (entry.pid, entry.start)).collect()
    }

    /// A walk row: what it shows, the tie rule, the table, and the `(pid, start)` descendants of
    /// pid 10 and the processes the walk could not tell from strangers.
    type WalkRow = (
        &'static str,
        Ties,
        Vec<os_process::Entry>,
        Vec<(u32, u64)>,
        Vec<(u32, u64)>,
    );

    /// What a walk row shows, and the `(pid, start)` descendants and untrusted processes a walk
    /// found.
    type Walked = (&'static str, Vec<(u32, u64)>, Vec<(u32, u64)>);

    /// THE DESCENDANT WALK (one table, start order): from a root the table lists, every process
    /// whose named parent is the root or one of its descendants, parents before their children in
    /// the table's order, each with the start time the table read. A child that started before the
    /// parent it names is no descendant (that parent's pid was taken by a later process), and
    /// neither are its own children, under either tie rule. A child started with its parent is one
    /// where ties are trusted; where they are not, it and everything below it are the walk's
    /// untrusted processes. A root the table does not list has none, the root is never its own
    /// descendant, and a cycle of parents ends.
    #[test]
    fn the_descendant_walk_follows_named_parents_in_start_order_from_a_listed_root() {
        let rows: [WalkRow; 10] = [
            (
                "a root with no child",
                Ties::Trusted,
                vec![row(10, 1, 100)],
                vec![],
                vec![],
            ),
            (
                "children before grandchildren, in the table's order",
                Ties::Trusted,
                vec![
                    row(13, 11, 130),
                    row(10, 1, 100),
                    row(12, 10, 120),
                    row(11, 10, 110),
                ],
                vec![(12, 120), (11, 110), (13, 130)],
                vec![],
            ),
            (
                "a child started with its parent, ties trusted",
                Ties::Trusted,
                vec![row(10, 1, 100), row(11, 10, 100), row(12, 11, 150)],
                vec![(11, 100), (12, 150)],
                vec![],
            ),
            (
                "a child started with its parent, ties untrusted",
                Ties::Untrusted,
                vec![row(10, 1, 100), row(11, 10, 100), row(12, 11, 150)],
                vec![],
                vec![(11, 100), (12, 150)],
            ),
            (
                "a grandchild started with its parent, ties untrusted",
                Ties::Untrusted,
                vec![row(10, 1, 100), row(11, 10, 110), row(12, 11, 110)],
                vec![(11, 110)],
                vec![(12, 110)],
            ),
            (
                "a child started before the parent it names, and its own child, ties trusted",
                Ties::Trusted,
                vec![row(10, 1, 100), row(11, 10, 99), row(12, 11, 150)],
                vec![],
                vec![],
            ),
            (
                "a child started before the parent it names, and its own child, ties untrusted",
                Ties::Untrusted,
                vec![row(10, 1, 100), row(11, 10, 99), row(12, 11, 150)],
                vec![],
                vec![],
            ),
            (
                "a stranger whose parent is another process",
                Ties::Trusted,
                vec![row(10, 1, 100), row(20, 1, 105), row(21, 20, 106)],
                vec![],
                vec![],
            ),
            (
                "a root the table does not list",
                Ties::Trusted,
                vec![row(11, 10, 110)],
                vec![],
                vec![],
            ),
            (
                "a cycle of parents through the root",
                Ties::Trusted,
                vec![row(10, 11, 100), row(11, 10, 100)],
                vec![(11, 100)],
                vec![],
            ),
        ];
        let walked: Vec<Walked> = rows
            .iter()
            .map(|(what, ties, table, _, _)| {
                let walk = walk_in(table, 10, *ties);
                (
                    *what,
                    pids_and_starts(&walk.generations.concat()),
                    pids_and_starts(&walk.untrusted),
                )
            })
            .collect();
        let expected: Vec<Walked> = rows
            .iter()
            .map(|(what, _, _, members, untrusted)| (*what, members.clone(), untrusted.clone()))
            .collect();
        assert_eq!(walked, expected);
    }

    /// The walk every end and watch reads takes this OS's tie rule ([`os_process::TIES`]): a child
    /// started with its parent is a descendant on Linux and macOS, and none on Windows.
    #[test]
    fn the_descendant_walk_reads_a_tie_by_this_oss_rule() {
        let table = [row(10, 1, 100), row(11, 10, 100)];
        let expected: Vec<(u32, u64)> = std::cfg_select! {
            target_os = "windows" => { vec![] }
            _ => { vec![(11, 100)] }
        };
        assert_eq!(pids_and_starts(&descendants_in(&table, 10)), expected);
    }

    /// A shell in `/` with one background child that reads the shell's stdin, which this test
    /// holds, and that child's pid, read once the shell has said it. [`release`] closes the
    /// stdin, so the child reads its end and exits and the shell's `wait` returns: nothing the
    /// fixture started outlives the test on an OS whose test run ends no process left behind.
    #[cfg(unix)]
    fn shell_with_a_child() -> (Child, u32) {
        let mut shell = StdCommand::new("sh")
            .arg("-c")
            .arg("exec 3<&0; cat <&3 >/dev/null & echo $!; wait")
            .current_dir(Path::new("/"))
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .spawn()
            .expect("spawn shell with a backgrounded child");
        let stdout = shell.stdout.take().expect("piped stdout");
        let mut line = String::new();
        std::io::BufRead::read_line(&mut std::io::BufReader::new(stdout), &mut line)
            .expect("read the child's pid line");
        let child = line.trim().parse().expect("parse the child's pid");
        (shell, child)
    }

    /// The walk over the live table finds a grandchild through the shell that started it, with
    /// the start time the grandchild reads.
    #[cfg(unix)]
    #[test]
    fn the_descendant_walk_finds_a_live_grandchild_through_its_parent() {
        let (shell, grandchild) = shell_with_a_child();
        let found: Vec<(u32, Option<u64>)> = descendants_of(shell.id())
            .iter()
            .map(|entry| (entry.pid, Some(entry.start)))
            .collect();
        let expected = vec![(grandchild, pid_starttime(grandchild))];
        release(shell);
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

    // Unix only: Windows reports no process's working directory.
    #[cfg(unix)]
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
            &mut LiveOs::new(),
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
            &mut LiveOs::new(),
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
        signal_if_unchanged(
            &mut LiveOs::new(),
            &target,
            &base,
            child.id(),
            &HashSet::new(),
            Signal::Hard,
        );
        let alive_as_self = matches!(child.try_wait(), Ok(None));
        assert!(alive_as_self, "a pid equal to self_pid must be skipped");

        // Pretend the child is a recorded ancestor - it must never be signalled.
        let mut ancestors = HashSet::new();
        ancestors.insert(child.id());
        signal_if_unchanged(
            &mut LiveOs::new(),
            &target,
            &base,
            std::process::id(),
            &ancestors,
            Signal::Hard,
        );
        let alive_as_ancestor = matches!(child.try_wait(), Ok(None));
        cleanup(&mut child);
        assert!(
            alive_as_ancestor,
            "a pid recorded as an ancestor of self must be skipped"
        );
    }

    // ---- THE ENDS (spec 104 STOP): a tree ended through the handle that holds its root, read
    // through the FIFO witness, so no signal of the test's own decides what was left running ----

    #[cfg(unix)]
    mod ends {
        use super::*;
        use crate::test_support::{Reading, Witness};
        use std::os::unix::process::ExitStatusExt;
        use std::time::Duration;

        /// The OS call `signal` becomes: nothing for pid 0, pid 1 or a pid that would be negative
        /// as a unix pid, and `expected` to pid 2.
        fn maps_and_refuses(signal: Signal, expected: rustix::process::Signal) {
            for pid in [0, 1, 0x8000_0000, u32::MAX] {
                assert_eq!(
                    os_call(signal, pid),
                    None,
                    "{signal:?} to pid {pid} sends nothing"
                );
            }
            let two = rustix::process::Pid::from_raw(2).expect("2 is a pid");
            assert_eq!(
                os_call(signal, 2),
                Some((two, expected)),
                "{signal:?} to pid 2"
            );
        }

        crate::test_cases! {
            a_term_is_sigterm_and_never_reaches_pid_0_1_or_a_negative_pid:
                maps_and_refuses(Signal::Term, rustix::process::Signal::TERM);
            a_hard_end_is_sigkill_and_never_reaches_pid_0_1_or_a_negative_pid:
                maps_and_refuses(Signal::Hard, rustix::process::Signal::KILL);
            a_stop_is_sigstop_and_never_reaches_pid_0_1_or_a_negative_pid:
                maps_and_refuses(Signal::Stop, rustix::process::Signal::STOP);
            a_resume_is_sigcont_and_never_reaches_pid_0_1_or_a_negative_pid:
                maps_and_refuses(Signal::Resume, rustix::process::Signal::CONT);
        }

        /// Whether `pid`'s chain reaches each root of `answers` over a parent reader that knows
        /// only `links` (child, parent): every other pid's parent is unreadable.
        fn chain(links: &[(u32, u32)], pid: u32, answers: &[(u32, bool)]) {
            let parent = |of: u32| {
                links
                    .iter()
                    .find(|(child, _)| *child == of)
                    .map(|(_, parent)| *parent)
            };
            for &(root, expected) in answers {
                assert_eq!(
                    chain_reaches(pid, root, parent),
                    expected,
                    "does {pid}'s chain over {links:?} reach {root}?"
                );
            }
        }

        /// A chain of `length` links, each pid the child of the pid below it, from pid
        /// `length + 2` down to pid 2.
        fn long_chain(length: u32) -> Vec<(u32, u32)> {
            (2..length + 2).map(|pid| (pid + 1, pid)).collect()
        }

        crate::test_cases! {
            a_chain_reaches_its_parent_and_no_other_pid:
                chain(&[(10, 5)], 10, &[(5, true), (6, false)]);
            a_chain_reaches_a_root_through_an_intermediate:
                chain(&[(10, 7), (7, 5)], 10, &[(5, true), (7, true), (3, false)]);
            a_chain_stops_at_init:
                chain(&[(10, 7), (7, 1), (1, 0)], 10, &[(7, true), (5, false)]);
            a_chain_never_reaches_init_or_pid_0_as_a_root:
                chain(&[(10, 2), (2, 1), (1, 0)], 10, &[(2, true), (1, false), (0, false)]);
            a_chain_ends_on_a_cycle:
                chain(&[(10, 11), (11, 12), (12, 11)], 10, &[(12, true), (99, false)]);
            a_chain_ends_at_an_unreadable_parent:
                chain(&[(10, 7)], 10, &[(7, true), (5, false)]);
            a_process_is_not_its_own_descendant:
                chain(&[(10, 5)], 10, &[(10, false), (5, true)]);
            a_chain_walks_at_most_4096_steps:
                chain(&long_chain(5000), 5002, &[(906, true), (905, false)]);
        }

        /// The fixture shell `body` (`Witness::script`), started under `witness`.
        fn tree(witness: &Witness, body: &str) -> Child {
            StdCommand::new("sh")
                .arg("-c")
                .arg(Witness::script(body))
                .envs(witness.env())
                .stdin(std::process::Stdio::piped())
                .spawn()
                .expect("spawn the fixture shell")
        }

        /// The one child `pid` has in the process table.
        fn only_child(pid: u32) -> u32 {
            let kids: Vec<u32> = os_process::table()
                .into_iter()
                .filter(|entry| entry.parent == pid)
                .map(|entry| entry.pid)
                .collect();
            assert_eq!(kids.len(), 1, "{pid} has one child: {kids:?}");
            kids[0]
        }

        /// Each process `report` left as residue, and whether it is still frozen.
        fn residue(report: &EndReport) -> Vec<(u32, bool)> {
            report
                .residue
                .iter()
                .map(|leftover| (leftover.pid, leftover.stopped))
                .collect()
        }

        /// The pids `report` names as sent its end, in order, and the rest of the report.
        fn split(report: EndReport) -> (Vec<u32>, EndReport) {
            let mut sent: Vec<u32> = report.sent.iter().map(|left| left.pid).collect();
            sent.sort_unstable();
            let rest = EndReport {
                sent: Vec::new(),
                ..report
            };
            (sent, rest)
        }

        /// Each process besides `main` that `os` was asked to send an end to (a hard end or a
        /// TERM), in order.
        fn ended_by(os: &Rigged, main: u32) -> Vec<u32> {
            let mut pids: Vec<u32> = os
                .sent
                .iter()
                .filter(|(signal, pid)| {
                    matches!(signal, Signal::Term | Signal::Hard) && *pid != main
                })
                .map(|(_, pid)| *pid)
                .collect();
            pids.sort_unstable();
            pids.dedup();
            pids
        }

        /// Whether `pid` is running and not frozen.
        fn runs(pid: u32) -> bool {
            os_process::state(pid).is_some_and(|state| !frozen_state(state) && state != 'Z')
        }

        /// The live OS as an end sees it, with `stranger` (a pid and the tree member it poses
        /// under) and `ghost` (a process gone before the end, and the member it poses under) added
        /// to the table, `vanishing` left out of every read of the table once it was sent a freeze,
        /// the parent of `unreadable_once` unreadable at its first read, the parent of `stranded`'s
        /// pid unreadable once that pid was sent its signal, every freeze of `unstoppable` and
        /// every hard end of `swallowed` dropped, and `forker` started on one more process ahead of
        /// each read of the table or each hard end, as its [`ForkAt`] says. `tied`'s parent's
        /// record reads `tied`'s start, and `ties`, where some, is the tie rule a walk reads the
        /// table by. Every process is addressed by its pid alone where `by_pid` holds, as on an OS
        /// without process descriptors, and the next reads, as many as `misfires` names, of where a
        /// freeze sent to its pid by the pid alone landed read another process's start. Where
        /// `no_freeze` holds, the OS has no freeze, as on Windows: every freeze and resume is
        /// dropped. `held`, where some, is a process a service manager holds apart in the unit
        /// named with it. Every signal the end sends is recorded, dropped or not, and so is each
        /// process sent a freeze while its parent did not read frozen, and each address and parent
        /// read, in order.
        #[derive(Default)]
        struct Rigged {
            tied: Option<u32>,
            ties: Option<Ties>,
            stranger: Option<(u32, u32)>,
            ghost: Option<(Identity, u32)>,
            vanishing: Option<u32>,
            unreadable_once: Option<u32>,
            stranded: Option<(u32, Signal)>,
            unstoppable: Option<u32>,
            swallowed: Option<u32>,
            forker: Option<Forker>,
            by_pid: bool,
            misfires: Option<(u32, usize)>,
            no_freeze: bool,
            held: Option<(u32, &'static str)>,
            sent: Vec<(Signal, u32)>,
            frozen_under_a_running_parent: Vec<u32>,
            steps: Vec<Step>,
        }

        /// One step a signal takes through [`Rigged`] before it is sent: its target addressed, or
        /// a parent read for its chain check.
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        enum Step {
            Addressed(u32),
            ParentRead(u32),
        }

        /// A tree member that starts a process each time a line reaches its `input`, `left` times
        /// more, `at` each read of the table or each hard end.
        struct Forker {
            pid: u32,
            input: std::process::ChildStdin,
            left: usize,
            at: ForkAt,
        }

        /// When a [`Forker`] is asked for its next process.
        #[derive(Clone, Copy, PartialEq, Eq)]
        enum ForkAt {
            /// Ahead of each read of the table.
            Read,
            /// Ahead of each hard end sent to a process other than the forker.
            HardEnd,
        }

        impl Forker {
            /// Start one more process under the member, if any is left, and wait for it in the
            /// table.
            fn fork(&mut self) {
                use std::io::Write;
                if self.left == 0 {
                    return;
                }
                self.left -= 1;
                let pid = self.pid;
                let children = || -> HashSet<u32> {
                    os_process::table()
                        .iter()
                        .filter(|entry| entry.parent == pid)
                        .map(|entry| entry.pid)
                        .collect()
                };
                let before = children();
                self.input
                    .write_all(b"\n")
                    .expect("ask the member for a process");
                assert!(
                    crate::test_support::wait_until(|| !children().is_subset(&before)),
                    "the member starts a process"
                );
            }
        }

        impl EndOs for Rigged {
            fn table(&mut self) -> Vec<os_process::Entry> {
                if let Some(forker) = self.forker.as_mut().filter(|f| f.at == ForkAt::Read) {
                    forker.fork();
                }
                let mut table = os_process::table();
                if let Some((pid, parent)) = self.stranger {
                    let start = os_process::start_time(pid).expect("the stranger is running");
                    table.retain(|entry| entry.pid != pid);
                    table.push(os_process::Entry { pid, parent, start });
                }
                if let Some((ghost, parent)) = self.ghost {
                    table.push(os_process::Entry {
                        pid: ghost.pid,
                        parent,
                        start: ghost.start,
                    });
                }
                if let Some(vanishing) = self.vanishing {
                    if self.sent.contains(&(Signal::Stop, vanishing)) {
                        table.retain(|entry| entry.pid != vanishing);
                    }
                }
                if let Some(tied) = self.tied {
                    let read = table.iter().find(|entry| entry.pid == tied);
                    let (parent, start) =
                        read.map(|e| (e.parent, e.start)).expect("tied is listed");
                    for entry in table.iter_mut().filter(|entry| entry.pid == parent) {
                        entry.start = start;
                    }
                }
                table
            }

            fn parent(&mut self, pid: u32) -> Option<u32> {
                self.steps.push(Step::ParentRead(pid));
                if self.unreadable_once == Some(pid) {
                    self.unreadable_once = None;
                    return None;
                }
                if let Some((stranded, signal)) = self.stranded {
                    if stranded == pid && self.sent.contains(&(signal, pid)) {
                        return None;
                    }
                }
                os_process::parent(pid)
            }

            fn address(&mut self, pid: u32) -> Option<Addressee> {
                self.steps.push(Step::Addressed(pid));
                address(pid, !self.by_pid && no_process_descriptors().is_none())
            }

            fn send(&mut self, signal: Signal, to: &Addressee) {
                let pid = to.pid;
                if signal == Signal::Hard {
                    let ending = |f: &&mut Forker| f.at == ForkAt::HardEnd && f.pid != pid;
                    if let Some(forker) = self.forker.as_mut().filter(ending) {
                        forker.fork();
                    }
                }
                self.sent.push((signal, pid));
                if signal == Signal::Stop && !os_process::parent(pid).is_some_and(frozen) {
                    self.frozen_under_a_running_parent.push(pid);
                }
                let dropped = match signal {
                    Signal::Hard => self.swallowed == Some(pid),
                    Signal::Stop => self.no_freeze || self.unstoppable == Some(pid),
                    Signal::Resume => self.no_freeze,
                    Signal::Term => false,
                };
                if !dropped {
                    send_signal(signal, to);
                }
            }

            fn start_after_freeze(&mut self, to: &Addressee) -> Option<u64> {
                let start = started(to);
                match &mut self.misfires {
                    Some((pid, left)) if *pid == to.pid && *left > 0 => {
                        *left -= 1;
                        start.map(|start| start + 1)
                    }
                    _ => start,
                }
            }

            fn ties(&self) -> Ties {
                self.ties.unwrap_or(os_process::TIES)
            }

            fn freezes(&self) -> bool {
                FREEZES && !self.no_freeze
            }

            fn held_apart(&mut self, pid: u32) -> Option<String> {
                let (held, unit) = self.held?;
                (held == pid).then(|| unit.to_string())
            }
        }

        /// A signal's target is addressed before its chain is checked, so the signal goes to the
        /// process the check passed, and the target is sent its signal once.
        #[test]
        fn a_signal_addresses_its_target_before_it_checks_the_chain() {
            let mut witness = Witness::new();
            let mut root = tree(&witness, "holder; up; held");
            assert!(witness.wait_up(2), "the fixture's two processes are up");
            let member = only_child(root.id());
            let target = Identity {
                pid: member,
                start: os_process::start_time(member).expect("the member is running"),
            };
            let mut os = Rigged::default();

            let sent = Tree::rooted_at(root.id()).send(
                &mut os,
                &mut EndReport::default(),
                Signal::Hard,
                &target,
            );

            assert_eq!(
                (sent, os.steps, os.sent),
                (
                    true,
                    vec![Step::Addressed(member), Step::ParentRead(member)],
                    vec![(Signal::Hard, member)]
                )
            );
            assert_eq!(witness.release(), Reading::Ended);
            assert!(crate::test_support::wait_until(|| matches!(
                root.try_wait(),
                Ok(Some(_))
            )));
        }

        /// The watched end of a tree whose one member's first freeze is read, right after, as having
        /// reached another process, as when one took the member's pid between the freeze's checks
        /// and the freeze, with the next of `misfires` reads reading that process again,
        /// addressed `by_pid` alone or through a process descriptor. By the pid, the process the
        /// freeze reached is resumed at once only where a second read confirms it still holds the
        /// pid, and is sent nothing more where none does (`resumed`), and the end says which;
        /// through a descriptor, which reaches the member or none, nothing is read again
        /// (`resumed` is `None`). The tree is ended either way.
        fn a_freeze_read_as_reaching_another_process(
            by_pid: bool,
            misfires: usize,
            resumed: Option<bool>,
        ) {
            let mut witness = Witness::new();
            let mut main = tree(&witness, "holder; up; held");
            assert!(witness.wait_up(2), "the fixture's two processes are up");
            let holder = only_child(main.id());
            let mut os = Rigged {
                by_pid,
                misfires: Some((holder, misfires)),
                ..Rigged::default()
            };

            let report = end_watched_with(&mut os, &mut main, &[], END_BOUND);

            let freeze = position(&os.sent, (Signal::Stop, holder));
            let next = os.sent.get(freeze + 1).copied();
            let faults: Vec<String> = resumed
                .map(|resumed| freeze_reached_another(holder, resumed))
                .into_iter()
                .collect();
            assert_eq!(
                (
                    next == Some((Signal::Resume, holder)),
                    split(report),
                    witness.read()
                ),
                (
                    resumed == Some(true),
                    (
                        vec![holder],
                        EndReport {
                            faults,
                            ..EndReport::default()
                        }
                    ),
                    Reading::Ended
                ),
                "by pid {by_pid}: {:?}",
                os.sent
            );
        }

        crate::test_cases! {
            a_freeze_by_pid_that_reached_another_process_resumes_it_once_confirmed:
                a_freeze_read_as_reaching_another_process(true, 2, Some(true));
            a_freeze_by_pid_that_reached_another_process_unconfirmed_sends_nothing_more:
                a_freeze_read_as_reaching_another_process(true, 1, Some(false));
            #[cfg(target_os = "linux")]
            a_freeze_through_a_descriptor_is_never_read_again:
                a_freeze_read_as_reaching_another_process(false, 2, None);
        }

        /// Process descriptors, on the one OS that has them.
        #[cfg(target_os = "linux")]
        mod descriptors {
            use super::*;

            /// Two held processes, a hard end sent naming the second's pid, through a descriptor
            /// opened on the process `opened_on` names where it is some and by the pid alone where
            /// it is none: the signal each process ended on, in order.
            fn a_hard_end_naming_the_second_ends(
                opened_on: Option<usize>,
                ended: [Option<i32>; 2],
            ) {
                let mut held = [sleeper_in(Path::new("/")), sleeper_in(Path::new("/"))];
                let to = Addressee {
                    pid: held[1].id(),
                    descriptor: opened_on.map(|which| {
                        address(held[which].id(), true)
                            .and_then(|to| to.descriptor)
                            .expect("a descriptor opens on a running process")
                    }),
                };

                send_signal(Signal::Hard, &to);

                wait_until(|| {
                    held.iter_mut()
                        .any(|child| matches!(child.try_wait(), Ok(Some(_))))
                });
                let signals = held.each_mut().map(|child| {
                    child
                        .try_wait()
                        .expect("read the process's status")
                        .and_then(|status| status.signal())
                });
                for child in &mut held {
                    cleanup(child);
                }
                assert_eq!(signals, ended);
            }

            crate::test_cases! {
                a_descriptor_opened_on_the_first_ends_the_first_whatever_pid_is_named:
                    a_hard_end_naming_the_second_ends(Some(0), [Some(9), None]);
                a_signal_by_pid_ends_the_process_its_pid_names:
                    a_hard_end_naming_the_second_ends(None, [None, Some(9)]);
            }

            /// What [`address`] answers, with descriptors or without (`by_descriptor`), for a
            /// running process and for one already gone: `expected` for each, as whether its
            /// address holds a descriptor, or `None` where it is not addressed.
            fn addressed(by_descriptor: bool, expected: [Option<bool>; 2]) {
                let mut running = sleeper_in(Path::new("/"));
                let mut gone = sleeper_in(Path::new("/"));
                cleanup(&mut gone);
                let pids = [running.id(), gone.id()];

                let answers = pids.map(|pid| {
                    address(pid, by_descriptor).map(|to| (to.pid, to.descriptor.is_some()))
                });

                cleanup(&mut running);
                assert_eq!(
                    answers,
                    [
                        expected[0].map(|held| (pids[0], held)),
                        expected[1].map(|held| (pids[1], held)),
                    ]
                );
            }

            crate::test_cases! {
                with_descriptors_a_running_process_is_held_by_one_and_a_gone_one_is_not_addressed:
                    addressed(true, [Some(true), None]);
                without_descriptors_both_are_addressed_by_pid:
                    addressed(false, [Some(false), Some(false)]);
            }

            #[test]
            fn this_kernel_opens_process_descriptors() {
                assert_eq!(no_process_descriptors(), None);
            }

            crate::test_cases! {
                a_kernel_without_process_descriptors_lacks_them_for_good:
                    assert!(lasting(rustix::io::Errno::NOSYS));
                a_process_refused_them_is_refused_for_good:
                    assert!(lasting(rustix::io::Errno::PERM));
                a_process_out_of_descriptors_may_open_one_later:
                    assert!(!lasting(rustix::io::Errno::MFILE));
                a_host_out_of_descriptors_may_open_one_later:
                    assert!(!lasting(rustix::io::Errno::NFILE));
                a_host_out_of_memory_may_open_one_later:
                    assert!(!lasting(rustix::io::Errno::NOMEM));
            }

            crate::test_cases! {
                a_kernel_without_process_descriptors_says_so:
                    assert_eq!(
                        unavailable_because(rustix::io::Errno::NOSYS),
                        "this kernel has no process descriptors"
                    );
                a_refused_process_descriptor_names_the_error:
                    assert_eq!(
                        unavailable_because(rustix::io::Errno::PERM),
                        "opening one fails here: Operation not permitted (os error 1)"
                    );
            }
        }

        /// Where of the sent signals `wanted` is, panicking when it was never sent.
        fn position(sent: &[(Signal, u32)], wanted: (Signal, u32)) -> usize {
            sent.iter()
                .position(|&signal| signal == wanted)
                .unwrap_or_else(|| panic!("{wanted:?} was sent: {sent:?}"))
        }

        #[test]
        fn the_watched_end_freezes_the_tree_then_ends_it_deepest_first_and_never_asks() {
            let mut witness = Witness::new();
            let mut main = tree(&witness, "holder; ( holder; up; held ) & up; held");
            assert!(witness.wait_up(4), "the fixture's four processes are up");
            let table = os_process::table();
            let middle = table
                .iter()
                .find(|entry| {
                    entry.parent == main.id() && table.iter().any(|kid| kid.parent == entry.pid)
                })
                .expect("the main's child that has a child of its own")
                .pid;
            let deepest = only_child(middle);
            let mut os = Rigged::default();

            let report = end_watched_with(&mut os, &mut main, &[], END_BOUND);

            assert_eq!(
                (witness.read(), split(report)),
                (
                    Reading::Ended,
                    (ended_by(&os, main.id()), EndReport::default())
                )
            );
            let status = main.try_wait().expect("read the main's status");
            assert_eq!(
                status.and_then(|status| status.signal()),
                Some(9),
                "the main is ended hard"
            );
            let sent = os.sent;
            assert!(
                !sent.iter().any(|(signal, _)| *signal == Signal::Term),
                "no TERM: {sent:?}"
            );
            assert_eq!(
                sent.first(),
                Some(&(Signal::Stop, main.id())),
                "the main freezes first"
            );
            assert_eq!(
                sent.last(),
                Some(&(Signal::Hard, main.id())),
                "the main ends last"
            );
            let last_stop = sent.iter().rposition(|(signal, _)| *signal == Signal::Stop);
            let first_hard = sent.iter().position(|(signal, _)| *signal == Signal::Hard);
            assert!(
                last_stop < first_hard,
                "every freeze comes before any end: {sent:?}"
            );
            assert!(
                position(&sent, (Signal::Hard, deepest)) < position(&sent, (Signal::Hard, middle)),
                "a child ends before its parent: {sent:?}"
            );
            assert_eq!(
                os.frozen_under_a_running_parent,
                vec![main.id()],
                "every process but the main is frozen under a frozen parent"
            );
        }

        /// A member whose chain check fails at its first freeze, its parent unreadable at that
        /// read: its child is not frozen while the member runs, and both are frozen, the member
        /// first, once a later walk's check passes, then ended.
        #[test]
        fn the_watched_end_freezes_a_child_only_while_its_parent_is_frozen() {
            let mut witness = Witness::new();
            let mut main = tree(&witness, "( holder; up; held ) & up; held");
            assert!(witness.wait_up(3), "the fixture's three processes are up");
            let middle = only_child(main.id());
            let deepest = only_child(middle);
            let mut os = Rigged {
                unreadable_once: Some(middle),
                ..Rigged::default()
            };

            let report = end_watched_with(&mut os, &mut main, &[], END_BOUND);

            assert_eq!(
                (witness.read(), split(report)),
                (
                    Reading::Ended,
                    (ended_by(&os, main.id()), EndReport::default())
                )
            );
            let sent = os.sent;
            assert_eq!(
                os.frozen_under_a_running_parent,
                vec![main.id()],
                "no child is frozen while its parent runs: {sent:?}"
            );
            assert!(
                position(&sent, (Signal::Stop, middle)) < position(&sent, (Signal::Stop, deepest)),
                "the member is frozen before its child: {sent:?}"
            );
        }

        /// A process the end froze whose parent chain is unreadable once `stranded_after` was sent
        /// to it, as when a third party ends its frozen parent: the end can no longer reach it,
        /// so it is reported as residue still frozen, and no resume is sent to it. Its freeze is
        /// dropped, so the test's release ends it after.
        fn the_watched_end_reports_still_frozen_a_process_it_froze_and_lost(
            stranded_after: Signal,
        ) {
            let mut witness = Witness::new();
            let mut main = tree(&witness, "( holder; up; held ) & up; held");
            assert!(witness.wait_up(3), "the fixture's three processes are up");
            let middle = only_child(main.id());
            let deepest = only_child(middle);
            let mut os = Rigged {
                unstoppable: Some(deepest),
                swallowed: Some(deepest),
                stranded: Some((deepest, stranded_after)),
                ..Rigged::default()
            };

            let report = end_watched_with(&mut os, &mut main, &[], Duration::from_secs(1));

            assert_eq!(
                (residue(&report), report.skipped),
                (vec![(deepest, true)], vec![]),
                "{stranded_after:?}: reported still frozen"
            );
            assert_eq!(
                (
                    os.sent.contains(&(Signal::Hard, deepest)),
                    os.sent.contains(&(Signal::Resume, deepest)),
                ),
                (stranded_after == Signal::Hard, false),
                "{stranded_after:?}: ended only while reachable, never resumed: {:?}",
                os.sent
            );
            assert!(
                matches!(main.try_wait(), Ok(Some(_))),
                "the main is ended and reaped"
            );
            assert_eq!(
                witness.read(),
                Reading::Held,
                "the process left behind holds the witness"
            );
            assert_eq!(witness.release(), Reading::Ended);
        }

        crate::test_cases! {
            the_watched_end_reports_still_frozen_a_process_it_froze_and_could_not_end:
                the_watched_end_reports_still_frozen_a_process_it_froze_and_lost(Signal::Stop);
            the_watched_end_reports_still_frozen_a_process_it_ended_and_could_not_resume:
                the_watched_end_reports_still_frozen_a_process_it_froze_and_lost(Signal::Hard);
        }

        #[test]
        fn the_watched_end_of_a_main_with_no_descendant_freezes_and_ends_it_alone() {
            let mut witness = Witness::new();
            let mut main = tree(&witness, "up; held");
            assert!(witness.wait_up(1), "the fixture is up");
            let mut os = Rigged::default();

            let report = end_watched_with(&mut os, &mut main, &[], END_BOUND);

            assert_eq!(
                (witness.read(), report),
                (Reading::Ended, EndReport::default())
            );
            let status = main.try_wait().expect("read the main's status");
            assert_eq!(status.and_then(|status| status.signal()), Some(9));
            assert_eq!(
                os.sent,
                vec![(Signal::Stop, main.id()), (Signal::Hard, main.id())]
            );
        }

        #[test]
        fn the_watched_end_resumes_and_reports_a_process_it_could_not_end() {
            let mut witness = Witness::new();
            let mut main = tree(&witness, "( holder; up; held ) & up; held");
            assert!(witness.wait_up(3), "the fixture's three processes are up");
            let middle = only_child(main.id());
            let middle_start = os_process::start_time(middle).expect("the middle's start time");
            let deepest = only_child(middle);
            let mut os = Rigged {
                swallowed: Some(deepest),
                ..Rigged::default()
            };

            let report = end_watched_with(&mut os, &mut main, &[], Duration::from_secs(1));

            assert_eq!(
                (residue(&report), report.skipped),
                (vec![(deepest, false)], vec![])
            );
            assert!(
                crate::test_support::wait_until(|| runs(deepest)),
                "the process left running is resumed, not left frozen: {:?}",
                os_process::state(deepest)
            );
            assert!(
                crate::test_support::wait_until(|| !still_started(middle, middle_start)
                    || os_process::state(middle) == Some('Z')),
                "its parent is ended: gone, or exited and left unreaped by a pid 1 that reaps \
                 no orphan"
            );
            assert!(
                matches!(main.try_wait(), Ok(Some(_))),
                "the main is ended and reaped"
            );
            assert_eq!(
                witness.read(),
                Reading::Held,
                "the process left running holds the witness"
            );
            assert_eq!(witness.release(), Reading::Ended);
        }

        /// How a stranger, a process outside the tree, reaches an end.
        #[derive(Clone, Copy, Debug)]
        enum Posing {
            /// It appears in the table as a child of a tree member.
            InTheTable,
            /// The watcher remembered it.
            Remembered,
        }

        /// A watched end whose stranger reaches it as `posing` never signals the stranger and
        /// reports it skipped, and still ends the tree.
        fn the_watched_end_never_signals_a_stranger(posing: Posing) {
            let mut witness = Witness::new();
            let mut main = tree(&witness, "holder; up; held");
            assert!(witness.wait_up(2), "the fixture's two processes are up");
            let dir = tempfile::tempdir().expect("a directory for the stranger");
            let mut stranger = sleeper_in(dir.path());
            assert!(crate::test_support::wait_until(|| runs(stranger.id())));
            let identity = Identity {
                pid: stranger.id(),
                start: os_process::start_time(stranger.id()).expect("the stranger's start"),
            };
            let (mut os, remembered) = match posing {
                Posing::InTheTable => (
                    Rigged {
                        stranger: Some((stranger.id(), main.id())),
                        ..Rigged::default()
                    },
                    vec![],
                ),
                Posing::Remembered => (Rigged::default(), vec![identity]),
            };

            let report = end_watched_with(&mut os, &mut main, &remembered, END_BOUND);

            let alive = matches!(stranger.try_wait(), Ok(None)) && runs(stranger.id());
            cleanup(&mut stranger);
            let skipped: Vec<u32> = report.skipped.iter().map(|leftover| leftover.pid).collect();
            assert_eq!(
                (alive, skipped, report.residue, witness.read()),
                (true, vec![identity.pid], vec![], Reading::Ended),
                "{posing:?}: the stranger runs on, reported skipped, and the tree is ended"
            );
            assert!(
                !os.sent.iter().any(|(_, pid)| *pid == identity.pid),
                "{posing:?}: nothing is sent to the stranger: {:?}",
                os.sent
            );
        }

        crate::test_cases! {
            the_watched_end_never_signals_a_stranger_in_the_table:
                the_watched_end_never_signals_a_stranger(Posing::InTheTable);
            the_watched_end_never_signals_a_stranger_it_remembered:
                the_watched_end_never_signals_a_stranger(Posing::Remembered);
        }

        /// A watched end whose main's record reads its holder's start, so the two tie, read by
        /// `ties`: where a tie is trusted the holder is ended with the tree, and where it is not
        /// the holder is never signalled and is reported skipped, and the rest of the tree is
        /// still ended.
        fn the_watched_end_reads_a_tie_by_its_rule(ties: Ties, left: Reading) {
            let mut witness = Witness::new();
            let mut main = tree(&witness, "holder; up; held");
            assert!(witness.wait_up(2), "the fixture's two processes are up");
            let holder = only_child(main.id());
            let mut os = Rigged {
                tied: Some(holder),
                ties: Some(ties),
                ..Rigged::default()
            };

            let report = end_watched_with(&mut os, &mut main, &[], END_BOUND);

            let skipped: Vec<u32> = report.skipped.iter().map(|leftover| leftover.pid).collect();
            let signalled = os.sent.iter().any(|(_, pid)| *pid == holder);
            let expected_skipped = match left {
                Reading::Held => vec![holder],
                Reading::Ended => vec![],
            };
            assert_eq!(
                (witness.read(), skipped, signalled, report.residue),
                (left, expected_skipped, left == Reading::Ended, vec![]),
                "{ties:?}: the holder tied with its main: {:?}",
                os.sent
            );
            assert!(
                matches!(main.try_wait(), Ok(Some(_))),
                "{ties:?}: the main is ended and reaped"
            );
            assert_eq!(witness.release(), Reading::Ended);
        }

        crate::test_cases! {
            the_watched_end_ends_a_child_tied_with_its_main_where_ties_are_trusted:
                the_watched_end_reads_a_tie_by_its_rule(Ties::Trusted, Reading::Ended);
            the_watched_end_reports_a_child_tied_with_its_main_where_ties_are_untrusted:
                the_watched_end_reads_a_tie_by_its_rule(Ties::Untrusted, Reading::Held);
        }

        #[test]
        fn the_watched_end_of_a_main_already_gone_signals_nothing_and_reports_what_it_remembered() {
            use std::io::Write;
            let mut witness = Witness::new();
            let mut main = tree(&witness, "holder; up; read -r unused");
            assert!(witness.wait_up(2), "the fixture's two processes are up");
            let holder = only_child(main.id());
            let remembered = [Identity {
                pid: holder,
                start: os_process::start_time(holder).expect("the holder's start"),
            }];
            let mut stdin = main.stdin.take().expect("the main's input");
            let _ = stdin.write_all(b"\n");
            drop(stdin);
            assert!(
                crate::test_support::wait_until(|| os_process::state(main.id()) == Some('Z')),
                "the main exits"
            );
            let mut os = Rigged::default();

            let report = end_watched_with(&mut os, &mut main, &remembered, END_BOUND);

            let skipped: Vec<u32> = report.skipped.iter().map(|leftover| leftover.pid).collect();
            assert_eq!(
                (os.sent, skipped, report.residue),
                (vec![], vec![holder], vec![])
            );
            assert_eq!(
                witness.read(),
                Reading::Held,
                "the holder the main left runs on"
            );
            assert_eq!(witness.release(), Reading::Ended);
        }

        /// A tree member that is never frozen, as on an OS with no freeze, and starts a process at
        /// every read of the table: the watched end stops walking once its bound passes, long
        /// before the member stops forking, and still ends every process it found, and the main.
        #[test]
        fn the_watched_end_stops_walking_at_its_bound_when_a_member_keeps_forking() {
            const FORKS: usize = 200;
            let mut witness = Witness::new();
            let mut main = tree(&witness, "up; while read -r unused; do holder; done; held");
            assert!(witness.wait_up(1), "the fixture is up");
            let input = main.stdin.take().expect("the main's input");
            let mut os = Rigged {
                forker: Some(Forker {
                    pid: main.id(),
                    input,
                    left: FORKS,
                    at: ForkAt::Read,
                }),
                unstoppable: Some(main.id()),
                ..Rigged::default()
            };
            let bound = Duration::from_secs(1);

            let started = std::time::Instant::now();
            let report = end_watched_with(&mut os, &mut main, &[], bound);
            let elapsed = started.elapsed();

            assert_eq!(
                os.frozen_under_a_running_parent,
                vec![main.id()],
                "no process under the member that never froze is frozen"
            );
            let forks_left = os.forker.as_ref().map_or(0, |forker| forker.left);
            assert_eq!(
                (split(report), witness.read(), forks_left > 0),
                (
                    (ended_by(&os, main.id()), EndReport::default()),
                    Reading::Ended,
                    true
                ),
                "the end stopped walking with {forks_left} of {FORKS} forks left, and ended every \
                 process it found"
            );
            assert!(
                elapsed < bound + Duration::from_secs(2),
                "the end returns near its bound: {elapsed:?}"
            );
            let status = main.try_wait().expect("read the main's status");
            assert_eq!(
                status.and_then(|status| status.signal()),
                Some(9),
                "the main is ended hard"
            );
        }

        /// A tree on an OS with no freeze whose main starts a process each time the end ends one
        /// of the tree's other processes, twice: the end walks the tree again after it ends what it
        /// found, and ends what the main started meanwhile, until a walk finds nothing new, so
        /// nothing the main started is left running once the main is ended.
        #[test]
        fn the_watched_end_walks_again_until_it_finds_nothing_the_tree_started_while_it_ended() {
            const FORKS: usize = 2;
            let mut witness = Witness::new();
            let mut main = tree(
                &witness,
                "holder; up; while read -r unused; do holder; done; held",
            );
            assert!(witness.wait_up(2), "the fixture's two processes are up");
            let input = main.stdin.take().expect("the main's input");
            let mut os = Rigged {
                forker: Some(Forker {
                    pid: main.id(),
                    input,
                    left: FORKS,
                    at: ForkAt::HardEnd,
                }),
                no_freeze: true,
                ..Rigged::default()
            };

            let report = end_watched_with(&mut os, &mut main, &[], END_BOUND);

            let forks_left = os.forker.as_ref().map_or(0, |forker| forker.left);
            assert_eq!(
                (split(report), forks_left, witness.read()),
                (
                    (ended_by(&os, main.id()), EndReport::default()),
                    0,
                    Reading::Ended
                ),
                "every process the main started while the end ran is ended: {:?}",
                os.sent
            );
        }

        /// A process the end froze that a later walk no longer lists is still ended, before its
        /// parent: the end ends every process it froze, whichever walk listed it last.
        #[test]
        fn the_watched_end_ends_a_process_it_froze_that_a_later_walk_missed() {
            let mut witness = Witness::new();
            let mut main = tree(&witness, "( holder; up; held ) & up; held");
            assert!(witness.wait_up(3), "the fixture's three processes are up");
            let middle = only_child(main.id());
            let deepest = only_child(middle);
            let mut os = Rigged {
                vanishing: Some(deepest),
                ..Rigged::default()
            };

            let report = end_watched_with(&mut os, &mut main, &[], END_BOUND);

            assert_eq!(
                (witness.read(), split(report)),
                (
                    Reading::Ended,
                    (ended_by(&os, main.id()), EndReport::default())
                )
            );
            let sent = os.sent;
            assert!(
                position(&sent, (Signal::Hard, deepest)) < position(&sent, (Signal::Hard, middle)),
                "the process the later walk missed ends before its parent: {sent:?}"
            );
        }

        /// The end of a root's own descendants, `graceful` or not, with a stranger posing under
        /// the root: the stranger is never signalled and is reported skipped, and the root's
        /// descendants are ended, so the root, which waits for them, exits.
        fn the_descendant_end_never_signals_a_stranger(graceful: bool) {
            let mut witness = Witness::new();
            let mut root = tree(&witness, "holder; holder; up; wait");
            assert!(witness.wait_up(3), "the fixture's three processes are up");
            let dir = tempfile::tempdir().expect("a directory for the stranger");
            let mut stranger = sleeper_in(dir.path());
            assert!(crate::test_support::wait_until(|| runs(stranger.id())));
            let stranger_pid = stranger.id();
            let mut os = Rigged {
                stranger: Some((stranger_pid, root.id())),
                ..Rigged::default()
            };
            let root_pid = root.id();
            let children_left = || !descendants_in(&os_process::table(), root_pid).is_empty();

            let report =
                end_descendants_with(&mut os, root_pid, graceful, children_left, END_BOUND);

            let alive = matches!(stranger.try_wait(), Ok(None)) && runs(stranger_pid);
            cleanup(&mut stranger);
            let root_exited =
                crate::test_support::wait_until(|| matches!(root.try_wait(), Ok(Some(_))));
            let skipped: Vec<u32> = report.skipped.iter().map(|leftover| leftover.pid).collect();
            assert_eq!(
                (alive, skipped, report.residue, root_exited, witness.read()),
                (true, vec![stranger_pid], vec![], true, Reading::Ended),
                "graceful {graceful}: the stranger runs on, reported skipped, and the tree is ended"
            );
            assert!(
                !os.sent.iter().any(|(_, pid)| *pid == stranger_pid),
                "nothing is sent to the stranger: {:?}",
                os.sent
            );
            assert_eq!(
                os.sent.iter().any(|(signal, _)| *signal == Signal::Term),
                graceful,
                "a TERM round only when graceful: {:?}",
                os.sent
            );
        }

        crate::test_cases! {
            the_graceful_descendant_end_never_signals_a_stranger:
                the_descendant_end_never_signals_a_stranger(true);
            the_hard_descendant_end_never_signals_a_stranger:
                the_descendant_end_never_signals_a_stranger(false);
        }

        /// Which end a row runs.
        #[derive(Clone, Copy, Debug)]
        enum Ending {
            /// [`end_watched_with`], of the held root.
            Watched,
            /// [`end_descendants_with`], of the root's descendants.
            Descendants,
        }

        /// `ending` of the tree under `root` through `os`, bounded by `bound`.
        fn end(ending: Ending, os: &mut Rigged, root: &mut Child, bound: Duration) -> EndReport {
            let root_pid = root.id();
            match ending {
                Ending::Watched => end_watched_with(os, root, &[], bound),
                Ending::Descendants => end_descendants_with(
                    os,
                    root_pid,
                    false,
                    || !descendants_in(&os_process::table(), root_pid).is_empty(),
                    bound,
                ),
            }
        }

        /// `ending` over a tree whose table also lists, under its root, a process that exited and
        /// was reaped before the end: the end neither signals it nor reports it, since nothing of
        /// it is left running, and still ends the tree.
        fn an_end_never_reports_a_process_gone_before_its_signal(ending: Ending) {
            let mut witness = Witness::new();
            let mut root = tree(&witness, "holder; up; wait");
            assert!(witness.wait_up(2), "the fixture's two processes are up");
            let mut gone = StdCommand::new("true")
                .spawn()
                .expect("spawn a process that exits");
            let ghost = Identity {
                pid: gone.id(),
                start: os_process::start_time(gone.id()).expect("its start, read before its reap"),
            };
            gone.wait().expect("reap the process that exits");
            let root_pid = root.id();
            let mut os = Rigged {
                ghost: Some((ghost, root_pid)),
                ..Rigged::default()
            };

            let report = end(ending, &mut os, &mut root, END_BOUND);

            let root_exited =
                crate::test_support::wait_until(|| matches!(root.try_wait(), Ok(Some(_))));
            assert_eq!(
                (split(report), root_exited, witness.read()),
                (
                    (ended_by(&os, root_pid), EndReport::default()),
                    true,
                    Reading::Ended
                ),
                "{ending:?}: only what the end signalled is reported and the tree is ended"
            );
            assert!(
                !os.sent.iter().any(|(_, pid)| *pid == ghost.pid),
                "{ending:?}: nothing is sent to the gone process: {:?}",
                os.sent
            );
        }

        crate::test_cases! {
            the_watched_end_never_reports_a_process_gone_before_its_signal:
                an_end_never_reports_a_process_gone_before_its_signal(Ending::Watched);
            the_descendant_end_never_reports_a_process_gone_before_its_signal:
                an_end_never_reports_a_process_gone_before_its_signal(Ending::Descendants);
        }

        /// `ending` over a tree whose middle process a service manager holds apart: the middle is
        /// sent nothing and is named with its unit, its own child, still in the ending process's
        /// unit, is ended and named as sent, and the end returns well within its bound, without
        /// waiting on the process it left. The test's release, never a signal, ends what is left.
        fn an_end_leaves_a_process_a_service_manager_holds_apart(ending: Ending) {
            let mut witness = Witness::new();
            let mut root = tree(&witness, "( holder; up; held ) & up; held");
            assert!(witness.wait_up(3), "the fixture's three processes are up");
            let root_pid = root.id();
            let middle = only_child(root_pid);
            let deepest = only_child(middle);
            let mut os = Rigged {
                held: Some((middle, "run-r9.scope")),
                ..Rigged::default()
            };

            let began = Instant::now();
            let report = end(ending, &mut os, &mut root, END_BOUND);
            let took = began.elapsed();

            let held: Vec<(u32, String)> = report
                .held_apart
                .iter()
                .map(|held| (held.process.pid, held.unit.clone()))
                .collect();
            let (sent, rest) = split(report);
            assert_eq!(
                (sent, held, rest.skipped, rest.residue, witness.read()),
                (
                    vec![deepest],
                    vec![(middle, "run-r9.scope".to_string())],
                    vec![],
                    vec![],
                    Reading::Held,
                ),
                "{ending:?}: the held-apart middle runs on, named with its unit; its child is ended"
            );
            let released = witness.release();
            let still: Vec<_> = [root_pid, middle, deepest]
                .into_iter()
                .map(|pid| (pid, os_process::state(pid), os_process::parent(pid)))
                .collect();
            let ps = StdCommand::new("ps")
                .args(["-A", "-o", "pid,ppid,stat,wchan,command"])
                .output()
                .map(|out| String::from_utf8_lossy(&out.stdout).into_owned())
                .unwrap_or_default();
            let near: Vec<&str> = ps
                .lines()
                .filter(|line| {
                    [root_pid, middle, deepest].iter().any(|pid| {
                        line.split_whitespace()
                            .take(2)
                            .any(|f| f == pid.to_string())
                    })
                })
                .collect();
            assert_eq!(
                released,
                Reading::Ended,
                "{ending:?}: the release ends what the end left: {still:?} {near:#?} {:?}",
                os.sent
            );
            assert!(
                !os.sent
                    .iter()
                    .any(|(signal, pid)| *pid == middle && *signal != Signal::Resume),
                "{ending:?}: nothing is sent to the held-apart middle: {:?}",
                os.sent
            );
            assert!(
                took < END_BOUND / 2,
                "{ending:?}: the end returns without waiting on what it left: {took:?}"
            );
        }

        /// A reap of a directory two fixture processes run in, one of them held apart by a
        /// service manager: the held-apart one is sent nothing, left running and answered with its
        /// unit, and the other, its child, is ended. The test's release, never a signal, ends what
        /// is left.
        #[test]
        fn a_reap_leaves_a_process_a_service_manager_holds_apart() {
            let repo = FakeRepo::new();
            let base = repo.base("scratch");
            let mut witness = Witness::new();
            let mut root = StdCommand::new("sh")
                .arg("-c")
                .arg(Witness::script("holder; up; held"))
                .envs(witness.env())
                .current_dir(&base)
                .spawn()
                .expect("spawn the fixture shell");
            assert!(witness.wait_up(2), "the fixture's two processes are up");
            let child = only_child(root.id());
            let mut os = Rigged {
                held: Some((root.id(), "run-r9.scope")),
                ..Rigged::default()
            };

            let held = reap_authorized_with(&mut os, &base, scan_with_starttime(&base));

            let held: Vec<(u32, String)> = held
                .into_iter()
                .map(|held| (held.process.pid, held.unit))
                .collect();
            let left_running = witness.read();
            let ended = ended_by(&os, 0);
            let released = witness.release();
            root.wait().expect("reap the fixture shell");
            assert_eq!(
                (held, ended, left_running, released),
                (
                    vec![(root.id(), "run-r9.scope".to_string())],
                    vec![child],
                    Reading::Held,
                    Reading::Ended
                ),
                "the held-apart process runs on, named with its unit; its child is ended: {:?}",
                os.sent
            );
        }

        crate::test_cases! {
            the_watched_end_leaves_a_process_a_service_manager_holds_apart:
                an_end_leaves_a_process_a_service_manager_holds_apart(Ending::Watched);
            the_descendant_end_leaves_a_process_a_service_manager_holds_apart:
                an_end_leaves_a_process_a_service_manager_holds_apart(Ending::Descendants);
        }

        /// `ending` over a tree whose member has a child that exited and is left unreaped: the
        /// member starts the child and becomes `cat`, which never waits for a child, and the child
        /// exits only once `cat` opens the FIFO the child writes to, so it exits after that exec
        /// whatever order the two run in, and no shell is left to reap it. The end names the
        /// member as one it was sent to, by the executable read before its signal, and never the
        /// child, which the table still lists but which had exited before the end reached it.
        fn an_end_names_what_it_signalled_and_no_process_that_had_exited(ending: Ending) {
            let mut witness = Witness::new();
            let dir = tempfile::tempdir().expect("a directory for the child's FIFO");
            let opened = dir.path().join("opened");
            crate::test_support::make_fifo(&opened);
            let mut root = tree(
                &witness,
                &format!(
                    "sh -c ': >\"$1\" & exec cat \"$1\" - <&8' sh '{}' & up; wait",
                    opened.display()
                ),
            );
            assert!(witness.wait_up(1), "the fixture's root is up");
            let member = only_child(root.id());
            let exited = || {
                os_process::table()
                    .into_iter()
                    .find(|entry| {
                        entry.parent == member && os_process::state(entry.pid) == Some('Z')
                    })
                    .map(|entry| entry.pid)
            };
            assert!(
                crate::test_support::wait_until(|| exited().is_some()),
                "the member's child has exited and is left unreaped"
            );
            let program = os_process::executable_name(member);
            let mut os = Rigged::default();

            let report = end(ending, &mut os, &mut root, END_BOUND);

            let root_exited =
                crate::test_support::wait_until(|| matches!(root.try_wait(), Ok(Some(_))));
            let named: Vec<(u32, String)> = report
                .sent
                .iter()
                .map(|leftover| (leftover.pid, leftover.program.clone()))
                .collect();
            assert_eq!(
                (
                    named,
                    report.residue,
                    report.skipped,
                    root_exited,
                    witness.read()
                ),
                (
                    vec![(member, program)],
                    vec![],
                    vec![],
                    true,
                    Reading::Ended
                ),
                "{ending:?}: the member alone is named as one the end was sent to: {:?}",
                os.sent
            );
        }

        crate::test_cases! {
            the_watched_end_names_what_it_signalled_and_no_process_that_had_exited:
                an_end_names_what_it_signalled_and_no_process_that_had_exited(Ending::Watched);
            the_descendant_end_names_what_it_signalled_and_no_process_that_had_exited:
                an_end_names_what_it_signalled_and_no_process_that_had_exited(Ending::Descendants);
        }

        /// `ending` over a tree whose one descendant fails its chain check once, its parent
        /// unreadable at that read and readable at the next, as when its parent exits and is
        /// reaped mid-check: a later check passes, so the end signals it, ends it, and reports
        /// only that it was sent the end.
        fn an_end_signals_a_process_whose_chain_check_failed_once(ending: Ending) {
            let mut witness = Witness::new();
            let mut root = tree(&witness, "holder; up; wait");
            assert!(witness.wait_up(2), "the fixture's two processes are up");
            let holder = only_child(root.id());
            let mut os = Rigged {
                unreadable_once: Some(holder),
                ..Rigged::default()
            };

            let report = end(ending, &mut os, &mut root, END_BOUND);

            let root_exited =
                crate::test_support::wait_until(|| matches!(root.try_wait(), Ok(Some(_))));
            assert_eq!(
                (split(report), root_exited, witness.read()),
                ((vec![holder], EndReport::default()), true, Reading::Ended),
                "{ending:?}: the process is ended and only its end is reported"
            );
            assert!(
                os.sent.contains(&(Signal::Hard, holder)),
                "{ending:?}: the process is ended hard once its check passes: {:?}",
                os.sent
            );
        }

        crate::test_cases! {
            the_watched_end_signals_a_process_whose_chain_check_failed_once:
                an_end_signals_a_process_whose_chain_check_failed_once(Ending::Watched);
            the_descendant_end_signals_a_process_whose_chain_check_failed_once:
                an_end_signals_a_process_whose_chain_check_failed_once(Ending::Descendants);
        }

        /// The end of the root's descendants, bounded by `bound`, of a tree whose one member
        /// survives every hard end: the end returns at its bound and reports the member as
        /// residue, never frozen; the test's release ends it after, and the root, which waits for
        /// it, exits.
        #[test]
        fn the_descendant_end_reports_at_its_bound_a_process_that_survives_it() {
            let mut witness = Witness::new();
            let mut root = tree(&witness, "holder; up; wait");
            assert!(witness.wait_up(2), "the fixture's two processes are up");
            let holder = only_child(root.id());
            let mut os = Rigged {
                swallowed: Some(holder),
                ..Rigged::default()
            };
            let bound = Duration::from_secs(1);

            let started = Instant::now();
            let report = end(Ending::Descendants, &mut os, &mut root, bound);
            let elapsed = started.elapsed();

            assert_eq!(
                (residue(&report), report.skipped),
                (vec![(holder, false)], vec![])
            );
            assert!(
                elapsed >= bound && elapsed < bound + Duration::from_secs(2),
                "the end returns at its bound: {elapsed:?}"
            );
            assert_eq!(
                witness.read(),
                Reading::Held,
                "the process that survived holds the witness"
            );
            assert_eq!(witness.release(), Reading::Ended);
            assert!(crate::test_support::wait_until(|| matches!(
                root.try_wait(),
                Ok(Some(_))
            )));
        }

        /// The end of the root's descendants, bounded by `bound`, of a tree whose one member
        /// survives every hard end and starts a process at every read of the table: the end stops
        /// walking at its bound, long before the member stops forking, ends every process the
        /// member started, the last among them, and reports the member alone as residue.
        #[test]
        fn the_descendant_end_stops_at_its_bound_when_a_member_keeps_forking() {
            const FORKS: usize = 200;
            let mut witness = Witness::new();
            let mut root = tree(
                &witness,
                "exec 3<&0; ( up; while read -r unused <&3; do holder; done; held ) & up; wait",
            );
            assert!(witness.wait_up(2), "the fixture's two processes are up");
            let member = only_child(root.id());
            let input = root.stdin.take().expect("the root's input");
            let mut os = Rigged {
                forker: Some(Forker {
                    pid: member,
                    input,
                    left: FORKS,
                    at: ForkAt::Read,
                }),
                swallowed: Some(member),
                ..Rigged::default()
            };
            let bound = Duration::from_secs(1);

            let started = Instant::now();
            let report = end(Ending::Descendants, &mut os, &mut root, bound);
            let elapsed = started.elapsed();

            let forks_left = os.forker.as_ref().map_or(0, |forker| forker.left);
            assert_eq!(
                (residue(&report), report.skipped, forks_left > 0),
                (vec![(member, false)], vec![], true),
                "the end stopped walking with {forks_left} of {FORKS} forks left"
            );
            assert!(
                elapsed >= bound && elapsed < bound + Duration::from_secs(2),
                "the end returns at its bound: {elapsed:?}"
            );
            drop(os);
            assert_eq!(
                witness.read(),
                Reading::Held,
                "the member alone holds the witness"
            );
            assert_eq!(witness.release(), Reading::Ended);
            assert!(crate::test_support::wait_until(|| matches!(
                root.try_wait(),
                Ok(Some(_))
            )));
        }
    }

    /// The lines the stream host says an end that answered `sent`, `residue`, `skipped` and
    /// `faults` in.
    fn assert_end_lines(
        sent: &[(u32, &str, bool)],
        residue: &[(u32, &str, bool)],
        skipped: &[(u32, &str, bool)],
        faults: &[&str],
        expected: &[&str],
    ) {
        let left = |rows: &[(u32, &str, bool)]| {
            rows.iter()
                .map(|&(pid, program, stopped)| Leftover {
                    pid,
                    program: program.to_string(),
                    stopped,
                })
                .collect()
        };
        let report = EndReport {
            sent: left(sent),
            residue: left(residue),
            skipped: left(skipped),
            faults: faults.iter().map(|fault| fault.to_string()).collect(),
            ..EndReport::default()
        };
        assert_eq!(report.lines("stream"), expected);
    }

    crate::test_cases! {
        an_end_sent_to_nothing_that_left_nothing_is_no_line: assert_end_lines(&[], &[], &[], &[], &[]);
        an_end_that_could_not_end_a_process_names_it_and_whether_it_is_stopped: assert_end_lines(
            &[],
            &[(41, "sleep", false), (42, "node", true)],
            &[],
            &[],
            &["stream: the end of the session left processes running; 2 it signalled and could \
               not end: 41 (sleep), 42 (node, stopped)"],
        );
        an_end_names_each_process_it_was_sent_to_by_pid_and_program: assert_end_lines(
            &[(41, "sleep", false), (42, "node", false)],
            &[],
            &[],
            &[],
            &["stream: the end of the session was sent to 2 processes besides its main: 41 \
               (sleep), 42 (node)"],
        );
        an_end_sent_to_one_process_that_left_one_says_each_in_a_line_of_its_own:
            assert_end_lines(
                &[(41, "sleep", false)],
                &[],
                &[(77, "daemon", false)],
                &[],
                &[
                    "stream: the end of the session was sent to 1 process besides its main: 41 \
                     (sleep)",
                    "stream: the end of the session left processes running; 1 left running, \
                     never signalled: 77 (daemon)",
                ],
            );
        an_end_that_never_signalled_a_process_names_it_as_left_running:
            assert_end_lines(
                &[],
                &[(41, "sleep", false)],
                &[(77, "daemon", false)],
                &[],
                &["stream: the end of the session left processes running; 1 it signalled and \
                   could not end: 41 (sleep); 1 left running, never signalled: 77 (daemon)"],
            );
        an_end_the_os_refused_says_why: assert_end_lines(
            &[],
            &[],
            &[],
            &["the job of session 7 could not be ended (denied), so the end did not wait for it"],
            &["stream: the job of session 7 could not be ended (denied), so the end did not wait \
               for it"],
        );
        an_end_that_hit_a_fault_and_left_a_process_says_both: assert_end_lines(
            &[],
            &[(41, "sleep", false)],
            &[],
            &["the job of session 7 could not be ended (denied), so the end did not wait for it"],
            &["stream: the job of session 7 could not be ended (denied), so the end did not wait \
               for it; the end of the session left processes running; 1 it signalled and could \
               not end: 41 (sleep)"],
        );
    }

    /// An end that left a process to the unit a service manager holds it in names it, its program
    /// and its unit in a line of its own, between what the end was sent to and what it left.
    #[test]
    fn an_end_that_left_a_process_to_its_unit_names_the_unit_in_a_line_of_its_own() {
        let left = |pid, program: &str| Leftover {
            pid,
            program: program.to_string(),
            stopped: false,
        };
        let report = EndReport {
            sent: vec![left(41, "sleep")],
            held_apart: vec![
                HeldApart {
                    process: left(50, "conmon"),
                    unit: "run-r9.scope".to_string(),
                },
                HeldApart {
                    process: left(51, "dbus-daemon"),
                    unit: "dbus.service".to_string(),
                },
            ],
            skipped: vec![left(77, "daemon")],
            ..EndReport::default()
        };

        assert_eq!(
            report.lines("stream"),
            [
                "stream: the end of the session was sent to 1 process besides its main: 41 (sleep)",
                "stream: the end of the session left 2 processes running that a service manager \
                 holds apart, each in a unit of its own: 50 (conmon) in run-r9.scope, 51 \
                 (dbus-daemon) in dbus.service",
                "stream: the end of the session left processes running; 1 left running, never \
                 signalled: 77 (daemon)",
            ]
        );
    }

    /// The cgroup file of a process in the terminal scope an end of a session runs in.
    const SESSION: &str =
        "0::/user.slice/user-1000.slice/user@1000.service/app.slice/app-term-7.scope\n";

    /// Which unit, if any, holds a process whose cgroup file reads `process` apart from the end
    /// whose own reads `reference`.
    fn assert_held_apart(reference: Option<&str>, process: Option<&str>, expected: Option<&str>) {
        assert_eq!(
            held_apart(reference, process),
            expected.map(String::from),
            "{reference:?} ending {process:?}"
        );
    }

    crate::test_cases! {
        a_process_in_the_ends_own_scope_is_ended:
            assert_held_apart(Some(SESSION), Some(SESSION), None);
        a_process_in_a_group_under_the_ends_own_scope_is_ended: assert_held_apart(
            Some(SESSION),
            Some("0::/user.slice/user-1000.slice/user@1000.service/app.slice/app-term-7.scope/sub\n"),
            None,
        );
        a_process_a_service_manager_holds_in_a_scope_of_its_own_is_held_apart: assert_held_apart(
            Some(SESSION),
            Some("0::/user.slice/user-1000.slice/user@1000.service/app.slice/run-r9.scope\n"),
            Some("run-r9.scope"),
        );
        a_process_a_service_holds_is_held_apart_by_that_service: assert_held_apart(
            Some(SESSION),
            Some("0::/user.slice/user-1000.slice/user@1000.service/app.slice/dbus.service/x\n"),
            Some("dbus.service"),
        );
        a_process_with_no_cgroup_file_is_ended: assert_held_apart(Some(SESSION), None, None);
        an_end_that_reads_no_cgroup_file_of_its_own_ends_every_process: assert_held_apart(
            None,
            Some("0::/user.slice/user-1000.slice/user@1000.service/app.slice/run-r9.scope\n"),
            None,
        );
        a_process_in_no_unit_is_ended: assert_held_apart(Some(SESSION), Some("0::/\n"), None);
        an_end_in_no_unit_ends_every_process: assert_held_apart(
            Some("0::/\n"),
            Some("0::/user.slice/user-1000.slice/user@1000.service/app.slice/run-r9.scope\n"),
            None,
        );
        a_process_on_a_v1_host_is_ended: assert_held_apart(
            Some("4:pids:/user.slice/app-term-7.scope\n1:name=systemd:/user.slice/app-term-7.scope\n"),
            Some("4:pids:/user.slice/run-r9.scope\n1:name=systemd:/user.slice/run-r9.scope\n"),
            None,
        );
        a_process_on_a_hybrid_host_is_ended: assert_held_apart(
            Some("1:name=systemd:/user.slice/app-term-7.scope\n0::/user.slice/app-term-7.scope\n"),
            Some("1:name=systemd:/user.slice/run-r9.scope\n0::/user.slice/run-r9.scope\n"),
            None,
        );
        a_process_whose_cgroup_file_reads_empty_is_ended:
            assert_held_apart(Some(SESSION), Some(""), None);
    }
}
