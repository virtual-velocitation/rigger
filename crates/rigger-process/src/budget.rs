//! Machine-wide build concurrency budget (spec 65): a file-lock slot directory caps how
//! many actual compiler invocations run concurrently ACROSS every rigger process on the
//! machine, so N unit worktrees building in parallel never stack N compiler fleets into
//! memory at once. THE AUTHORITATIVE STATE IS THE SLOT FILES on disk, never any in-process
//! counter: a fixed set of `slot-<i>.lock` files, each held via std's exclusive file lock
//! (`flock(2)` on unix, `LockFileEx` on Windows) for
//! exactly the wrapped build's duration and auto-released by the kernel the instant the
//! holding process exits - cleanly or crashed. That is deliberate: an in-process counter,
//! an environment variable, or any state a crash could strand is NOT an implementation of
//! this budget. A hung build simply holds its slot; the existing agent liveness watchdogs -
//! never this module - are what eventually kill hung work and so free it. This module adds
//! no second reaper.
//!
//! Slots bound BUILDS, not agents: only an actual compiler invocation
//! ([`crate::gate::ExecRunner::run`]) acquires a slot. A reviewer reading code or querying
//! the graph never calls into this module at all, so it is never budget-gated.

use std::fs::{self, File, OpenOptions, TryLockError};
use std::path::PathBuf;
use std::time::Duration;

/// The machine-wide slot directory every rigger process on the machine shares absent
/// explicit configuration: the OS temp dir, NOT the repo - the budget spans every project
/// checked out on this machine, so it cannot live under any one project's `.rigger/`. A
/// fixed `rigger-build-budget` name means every process on the machine resolves the SAME
/// directory, and thus contends over the SAME slot files, with no coordination needed.
pub fn default_slot_dir() -> PathBuf {
    std::env::temp_dir().join("rigger-build-budget")
}

/// How long [`BuildBudget::acquire`] sleeps between polls of the slot directory while it
/// waits for one to free. Short enough that a waiting build starts promptly once a slot
/// frees; long enough not to hammer the filesystem while every slot stays held.
const POLL_INTERVAL: Duration = Duration::from_millis(50);

/// BuildBudget is the resolved, PURE config a caller re-derives per call (mirroring
/// [`crate::gate::BuildEnv`]) from the committed `build.max_concurrent` value: how many
/// slots exist and where their lock files live. It holds no lock itself - [`acquire`] mints
/// the actual guard.
///
/// `Default` (`max_concurrent: 0`, an empty `slot_dir`) is the unlimited, no-gating shape -
/// the same "unset means nothing happens" convention a [`crate::gate::BuildEnv`] declaring
/// nothing follows, exporting nothing - so a caller that never resolves one from config gates
/// nothing.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BuildBudget {
    max_concurrent: u32,
    slot_dir: PathBuf,
}

impl BuildBudget {
    /// Resolve the budget from the workflow's `build.max_concurrent` config (spec 65): `0`
    /// means unlimited - every [`acquire`]
    /// call is admitted immediately and no slot directory is ever created or touched. A
    /// positive N caps concurrent builds at N, machine-wide, under `slot_dir`.
    ///
    /// [`acquire`]: BuildBudget::acquire
    pub fn new(max_concurrent: u32, slot_dir: PathBuf) -> BuildBudget {
        BuildBudget {
            max_concurrent,
            slot_dir,
        }
    }

    /// Acquire one of the `max_concurrent` machine-wide slots, BLOCKING until one is free.
    /// `max_concurrent == 0` (unlimited) returns immediately holding no lock at all - the
    /// unlimited path never touches the filesystem.
    ///
    /// Polls each of the `max_concurrent` numbered slot files in turn with a non-blocking
    /// [`File::try_lock`]; the first free slot wins and its held [`File`] becomes the returned
    /// [`SlotGuard`], which releases the instant it drops - on a clean return, an early `?`, or
    /// the process being killed (the kernel releases the lock on any exit of the holding
    /// process). A hung build holding its slot is therefore exactly as bounded as the liveness
    /// watchdog that eventually kills it; this module adds no second reaper and strands nothing
    /// on its own crash either.
    ///
    /// Best-effort against filesystem trouble unrelated to the build itself: a slot
    /// directory that cannot be created (a read-only or absent temp mount) degrades to
    /// unlimited for this call rather than wedging the loop, and the guard's
    /// [`SlotGuard::unenforced`] names the directory and the OS error. A pass in which no slot is held by
    /// another process, yet none could be opened and locked, is the last slot's error: no
    /// holder will ever free a slot, so waiting would never end.
    pub fn acquire(&self) -> std::io::Result<SlotGuard> {
        if self.max_concurrent == 0 {
            return Ok(SlotGuard::default());
        }
        if let Err(e) = fs::create_dir_all(&self.slot_dir) {
            return Ok(SlotGuard {
                held: None,
                unenforced: format!(
                    "slot directory {} could not be created: {e}",
                    self.slot_dir.display()
                ),
            });
        }
        loop {
            let (mut held, mut failure) = (false, None);
            for i in 0..self.max_concurrent {
                let path = self.slot_dir.join(format!("slot-{i}.lock"));
                let slot = OpenOptions::new()
                    .create(true)
                    .write(true)
                    .truncate(false)
                    .open(&path)
                    .map_err(TryLockError::Error)
                    .and_then(|f| f.try_lock().map(|()| f));
                match slot {
                    Ok(f) => {
                        return Ok(SlotGuard {
                            held: Some(f),
                            unenforced: String::new(),
                        });
                    }
                    Err(TryLockError::WouldBlock) => held = true,
                    Err(TryLockError::Error(e)) => {
                        failure = Some(std::io::Error::new(
                            e.kind(),
                            format!("build budget slot {}: {e}", path.display()),
                        ))
                    }
                }
            }
            if let (false, Some(e)) = (held, failure) {
                return Err(e);
            }
            std::thread::sleep(POLL_INTERVAL);
        }
    }
}

/// RAII guard for one held budget slot ([`BuildBudget::acquire`]). Dropping it releases the
/// lock explicitly (the OS would also release it on process exit, including an abnormal
/// one that never runs this `Drop`) so the next waiter's poll finds it free without delay.
/// Holds no lock at all - `Drop` is then a no-op - on the unlimited / degraded-to-unlimited
/// path, where `held` is `None`.
#[derive(Default)]
pub struct SlotGuard {
    held: Option<File>,
    unenforced: String,
}

impl SlotGuard {
    /// Why the build this guard admits runs outside the budget: the slot directory that could
    /// not be created and the OS error, or empty when a slot is held or the budget is unlimited.
    pub fn unenforced(&self) -> &str {
        &self.unenforced
    }
}

impl Drop for SlotGuard {
    fn drop(&mut self) {
        if let Some(f) = &self.held {
            let _ = f.unlock();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::wait_until_for;
    use std::process::Command;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;

    #[test]
    fn unlimited_budget_never_touches_the_filesystem_and_never_blocks() {
        // spec 65: `max_concurrent: 0` means unlimited - every acquire is admitted
        // immediately, and (since nothing is gated) no slot directory is ever created.
        let dir = tempfile::tempdir().unwrap();
        let slot_dir = dir.path().join("slots");
        let budget = BuildBudget::new(0, slot_dir.clone());

        let _a = budget.acquire().unwrap();
        let _b = budget.acquire().unwrap();
        let _c = budget.acquire().unwrap();

        assert!(
            !slot_dir.exists(),
            "the unlimited path must never create the slot directory"
        );
    }

    #[test]
    fn acquire_blocks_the_n_plus_1th_build_until_a_slot_frees() {
        // The done-when's central claim: with `max_concurrent: N`, the N+1th concurrent
        // build blocks until a slot frees, then proceeds.
        let dir = tempfile::tempdir().unwrap();
        let slot_dir = dir.path().join("slots");
        let budget = BuildBudget::new(1, slot_dir);

        let first = budget.acquire().unwrap();

        let budget2 = budget.clone();
        let acquired = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&acquired);
        let handle = std::thread::spawn(move || {
            let _second = budget2.acquire().unwrap();
            flag.store(true, Ordering::SeqCst);
        });

        std::thread::sleep(Duration::from_millis(200));
        assert!(
            !acquired.load(Ordering::SeqCst),
            "the only slot is still held by `first`; the N+1th build must block"
        );

        drop(first);
        assert!(
            wait_until_for(400, || acquired.load(Ordering::SeqCst)),
            "the waiting build must acquire once the held slot is released"
        );
        handle.join().unwrap();
    }

    #[test]
    fn a_slot_directory_where_no_slot_can_be_locked_is_an_error_not_an_endless_wait() {
        // No slot file opens (each slot path is a directory), so no holder will ever free one:
        // the acquire must answer that error rather than poll forever.
        let dir = tempfile::tempdir().unwrap();
        let slot_dir = dir.path().join("slots");
        for i in 0..2 {
            fs::create_dir_all(slot_dir.join(format!("slot-{i}.lock"))).unwrap();
        }
        let budget = BuildBudget::new(2, slot_dir);
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || tx.send(budget.acquire().is_err()));
        assert_eq!(
            rx.recv_timeout(Duration::from_secs(5)),
            Ok(true),
            "an acquire no slot can satisfy must return its error"
        );
    }

    /// A slot directory that cannot be created (it lies under a regular file) admits the build
    /// outside the budget, and its guard says why the budget was not enforced; an unlimited
    /// budget and a held slot say nothing.
    #[test]
    fn a_slot_directory_that_cannot_be_created_admits_the_build_and_says_why() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("file");
        fs::write(&file, b"").unwrap();
        let slot_dir = file.join("slots");
        let create_error = fs::create_dir_all(&slot_dir).unwrap_err();
        let unenforced = |budget: BuildBudget| budget.acquire().unwrap().unenforced().to_string();
        assert_eq!(
            [
                unenforced(BuildBudget::new(1, slot_dir.clone())),
                unenforced(BuildBudget::new(0, slot_dir.clone())),
                unenforced(BuildBudget::new(1, dir.path().join("slots"))),
            ],
            [
                format!(
                    "slot directory {} could not be created: {create_error}",
                    slot_dir.display()
                ),
                String::new(),
                String::new(),
            ],
            "only a budget that admitted the build outside its slots says why"
        );
    }

    #[test]
    fn a_freed_slot_is_reused_so_a_third_waiter_also_proceeds() {
        // Beyond a single hand-off: the SAME slot file must be reusable indefinitely, not
        // consumed by its first release.
        let dir = tempfile::tempdir().unwrap();
        let slot_dir = dir.path().join("slots");
        let budget = BuildBudget::new(1, slot_dir);

        let first = budget.acquire().unwrap();
        drop(first);
        let second = budget.acquire().unwrap();
        drop(second);
        let _third = budget.acquire().unwrap();
        // Reaching here without hanging proves the slot is reusable across releases.
    }

    /// Whether this host's `flock(1)` supports `-F`/`--no-fork` (util-linux >= 2.32): probed
    /// from `--help` text rather than a version-string parse, since that is the actual
    /// capability [`slot_releases_when_its_holder_process_exits_abnormally`] depends on.
    fn flock_supports_no_fork() -> bool {
        Command::new("flock")
            .arg("--help")
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).contains("--no-fork"))
            .unwrap_or(false)
    }

    #[test]
    fn slot_releases_when_its_holder_process_exits_abnormally() {
        // spec 65: "auto-released by the kernel on any exit, clean or crashed" - a holder
        // that is SIGKILLed (never runs its own cleanup) must still free the slot the
        // instant it dies. Simulated with a real external process (`flock(1)`) holding the
        // exact slot file `BuildBudget` itself would open, since the guarantee is about the
        // kernel's flock semantics on that file, not about which process took the lock.
        //
        // spec 78, THE BUDGET FIXTURE: the holder must be ONE process that holds the lock
        // itself, handle-bound to termination - never a forked wrapper-plus-child pair that
        // can only be reached as a unit via a process-group signal. `--no-fork` (util-linux's
        // `-F`) EXECs `sleep` in place of forking, so the single spawned process IS the
        // locked fd's sole holder for its whole life; `Child::kill()` + `Child::wait()` on
        // the handle that spawned it is then sufficient - no `process_group(0)`, no negative
        // pid, no `--` argv separator anywhere in this fixture. Skips (rather than falling
        // back to the old process-group kill) when the host's `flock(1)` predates the flag.
        if !flock_supports_no_fork() {
            eprintln!(
                "skipping slot_releases_when_its_holder_process_exits_abnormally: this host's \
                 flock(1) has no -F/--no-fork (needs util-linux >= 2.32), and spec 78 forbids \
                 falling back to the process-group kill this fixture used before that flag \
                 existed"
            );
            return;
        }

        let dir = tempfile::tempdir().unwrap();
        let slot_dir = dir.path().join("slots");
        fs::create_dir_all(&slot_dir).unwrap();
        let slot_path = slot_dir.join("slot-0.lock");

        let mut holder = Command::new("flock")
            .arg("--no-fork")
            .arg("-x")
            .arg(&slot_path)
            .arg("sleep")
            .arg("300")
            .spawn()
            .expect("spawn an external flock holder for the fixture");

        // Wait until the external holder has actually taken the lock before probing it.
        assert!(
            wait_until_for(400, || Command::new("flock")
                .arg("-n")
                .arg("-x")
                .arg(&slot_path)
                .arg("true")
                .status()
                .map(|s| !s.success())
                .unwrap_or(false)),
            "the external fixture holder must take slot-0 before the test proceeds"
        );

        let budget = BuildBudget::new(1, slot_dir);
        let done = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&done);
        let budget2 = budget.clone();
        let handle = std::thread::spawn(move || {
            let _g = budget2.acquire().unwrap();
            flag.store(true, Ordering::SeqCst);
        });

        std::thread::sleep(Duration::from_millis(200));
        assert!(
            !done.load(Ordering::SeqCst),
            "the slot is externally held; our acquire must block"
        );

        // Abnormal death through the Child HANDLE that spawned this one process - the holder
        // never runs its own cleanup, so only the kernel's own release-on-exit can free the
        // slot.
        holder.kill().expect("SIGKILL the fixture holder");
        holder.wait().expect("reap the killed fixture holder");

        assert!(
            wait_until_for(400, || done.load(Ordering::SeqCst)),
            "the kernel must release the flock the instant the abnormal holder exits"
        );
        handle.join().unwrap();
    }
}
