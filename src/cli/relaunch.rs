//! A RUN SURVIVES ITS DRIVER: a live run's driver keeps a record of itself in the step lock it
//! holds (`.rigger/step.lock`), and every other rigger invocation in the project, the session-start
//! hook `rigger prime` among them, checks it. A record whose lock no driver holds is a driver that
//! died, by a crash, a forced end, a hangup or a reboot, and the check relaunches it by adoption, in
//! the background, as a hand `rigger run <spec> --base <ref>` would.
//!
//! STOPPED ON PURPOSE is every end the driver itself sees: any return of `rigger run` after it
//! took the step lock (done, escalated with nothing left, a halt, a refusal, an error), a panic,
//! and the operator's interactive interrupt (Ctrl-C), which the driver answers by emptying its
//! record ([`rigger::interrupt`]). Each empties the record, so nothing relaunches the run. A held
//! run is relaunched and waits in its hold, so `rigger hold` is how an operator keeps a run from
//! spending whatever ends its driver.
//!
//! The record's one reaper is the driver that wrote it, as it ends; a record whose driver ended
//! without reaping it is reaped by the check that relaunches it, by the driver that relaunch
//! starts, or, when no relaunch is owed, by the check itself.

use super::*;
use rigger::interrupt::{empty_on_interrupt, InterruptTrigger};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// How long a relaunch is given to take the step lock before a check reads it as never started.
const GRACE: Duration = Duration::from_secs(30);
/// How long a relaunched driver must live for its death to be relaunched again: a driver that
/// dies sooner is left stopped, so a run whose driver cannot live is never relaunched in a loop.
const BACKOFF: Duration = Duration::from_secs(600);

/// The record a live driver keeps of itself in its step lock: how to relaunch it, and the
/// relaunches it carries.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
struct DriverRecord {
    /// The physical `.rigger` directory the step lock lives in, so a record carried with a copy
    /// of the project to another place is never relaunched there.
    dir: PathBuf,
    /// The working directory the driver was started in.
    cwd: PathBuf,
    /// The arguments that relaunch the driver, `run` first, ending `--adopt <run>`.
    args: Vec<String>,
    /// The run the driver drives.
    run: String,
    /// When a check started a relaunch of this record, in seconds since the Unix epoch.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    relaunched_at: Option<u64>,
    /// When the relaunch that started this driver was made, in seconds since the Unix epoch.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    revived_at: Option<u64>,
}

impl DriverRecord {
    /// The command line that relaunches this driver by hand.
    fn by_hand(&self) -> String {
        format!("rigger {}", self.args.join(" "))
    }
}

/// Seconds since the Unix epoch, now.
fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// The step lock a live driver holds, keeping its record: emptied as the driver ends, by any
/// return or panic (this guard's drop) or by an interactive interrupt (its trigger).
pub(super) struct KeptRecord {
    /// Dropped before the lock, so the trigger never outlives the file it empties.
    _interrupt: InterruptTrigger,
    lock: HeldLock,
    /// The record a driver before this one left in the lock, when it parses.
    prior: Option<DriverRecord>,
}

impl KeptRecord {
    /// Hold `lock`, the step lock this driver took, as the keeper of its record.
    pub(super) fn hold(lock: HeldLock) -> KeptRecord {
        let prior = lock
            .contents()
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok());
        KeptRecord {
            _interrupt: empty_on_interrupt(lock.file()),
            lock,
            prior,
        }
    }

    /// Record this driver: started in `cwd` against the store at `dir` with `parsed`, driving
    /// `run`. A driver a relaunch of the same run started carries when that relaunch was made.
    /// A record that cannot be written is said on stderr; the run drives on, unrelaunchable.
    pub(super) fn keep(&self, parsed: &RunArgs, cwd: &Path, dir: &Path, run: &str) {
        let revived_at = self
            .prior
            .as_ref()
            .filter(|prior| parsed.adopt.is_some() && prior.run == run)
            .and_then(|prior| prior.relaunched_at);
        let record = DriverRecord {
            dir: dir.canonicalize().unwrap_or_else(|_| dir.to_path_buf()),
            cwd: cwd.to_path_buf(),
            args: relaunch_args(parsed, run),
            run: run.to_string(),
            relaunched_at: None,
            revived_at,
        };
        let written = serde_json::to_string(&record)
            .map_err(std::io::Error::other)
            .and_then(|text| self.lock.replace(&text));
        if let Err(e) = written {
            stderr_line(&format!(
                "rigger run: no record of this driver was kept ({e}), so a death of it is not \
                 relaunched"
            ));
        }
    }
}

impl Drop for KeptRecord {
    fn drop(&mut self) {
        let _ = self.lock.replace("");
    }
}

/// The arguments that relaunch the run `parsed` started as `run`: `run`, the spec, an explicit
/// base (the flag's or `RIGGER_BASE`'s), the event store a flag chose, then `--adopt <run>`.
/// `--fresh` and `--rebase-definition` are a start's own, and `--conn` may carry a credential,
/// so none is recorded.
fn relaunch_args(parsed: &RunArgs, run: &str) -> Vec<String> {
    let mut args = vec!["run".to_string()];
    args.extend(parsed.spec.clone());
    let (base, explicit) = resolve_run_base(
        parsed.base.as_deref(),
        std::env::var("RIGGER_BASE").ok().as_deref(),
    );
    if explicit {
        args.extend(["--base".to_string(), base]);
    }
    if let Some(store) = parsed.store {
        let kind = match store {
            StoreKind::Sqlite => "sqlite",
            StoreKind::KurrentDb => "kurrentdb",
        };
        args.extend(["--eventstore".to_string(), kind.to_string()]);
    }
    args.extend(["--adopt".to_string(), run.to_string()]);
    args
}

/// How a check starts a relaunched driver: an interface, so a test records the relaunch it
/// would start.
trait Launcher {
    /// Start `rigger <args>` in `cwd`, in the background: the started driver's pid.
    fn launch(&self, args: &[String], cwd: &Path) -> std::io::Result<u32>;
}

/// The host's launcher: the running binary, started detached ([`rigger::detached::start`]).
struct DetachedLauncher;

impl Launcher for DetachedLauncher {
    fn launch(&self, args: &[String], cwd: &Path) -> std::io::Result<u32> {
        rigger::detached::start(Path::new(&running_executable()), args, cwd)
    }
}

/// The recording launcher [`rigger::detached::RELAUNCH_ENV`]'s `record:<path>` selects: a launch
/// appends `relaunch <args>` to `<path>` and starts nothing, its pid `0`.
struct RecordingLauncher(PathBuf);

impl Launcher for RecordingLauncher {
    fn launch(&self, args: &[String], _cwd: &Path) -> std::io::Result<u32> {
        append_line(&self.0, &format!("relaunch {}", args.join(" ")))?;
        Ok(0)
    }
}

/// The launcher [`rigger::detached::RELAUNCH_ENV`] selects: unset or empty, the host's; a
/// `record:<path>`, the recording one; any other value, an error naming it, so a mistyped knob
/// never starts a driver unasked.
fn launcher() -> Result<Box<dyn Launcher>, String> {
    let env = rigger::detached::RELAUNCH_ENV;
    let knob = std::env::var_os(env).unwrap_or_default();
    let knob = knob.to_string_lossy();
    if knob.is_empty() {
        return Ok(Box::new(DetachedLauncher));
    }
    match knob.strip_prefix("record:") {
        Some(path) => Ok(Box::new(RecordingLauncher(PathBuf::from(path)))),
        None => Err(format!("{env}={knob} names no launcher")),
    }
}

/// Whether `verb`'s check runs after the verb rather than before it: the verbs that change what a
/// relaunch would do (`hold`, `release`, `resume-unit`) or need the step lock a relaunched driver
/// would take (`reset`, `setup`, `init`). `run` takes no check: it is the driver itself.
pub(crate) fn checks_after(verb: &str) -> bool {
    matches!(
        verb,
        "hold" | "release" | "resume-unit" | "reset" | "setup" | "init"
    )
}

/// THE RELAUNCH ENTRY: relaunch the run whose driver died in the project at `site`, the one
/// call every invocation and a login start make. Silent unless it relaunched a driver or left a
/// run stopped, which it says on stderr in one line. An invocation inside a run (a spawn, a
/// gate), and `rigger run` itself (`verb`), check nothing.
pub(crate) fn relaunch_a_dead_driver(site: &Site, verb: &str) {
    let inside_a_run = [
        rigger::driver::SPAWN_ENV,
        rigger::gate::STORE_FENCE_ENV,
        rigger::gate_channel::GATE_SOCKET_ENV,
    ]
    .iter()
    .any(|var| std::env::var_os(var).is_some());
    if verb == "run" || inside_a_run {
        return;
    }
    let root = site.project_root();
    site.lend_discovery();
    let Ok(root) = root else {
        return;
    };
    let dir = root.join(RIGGER_DIR);
    let said = launcher().map_or_else(Some, |launcher| revive(&dir, now_secs(), launcher.as_ref()));
    if let Some(line) = said {
        stderr_line(&line);
    }
}

/// Relaunch the driver whose record the step lock under `dir` holds when no driver holds that
/// lock, at `now`, through `launcher`: the line to say, `None` when there is nothing to say. Every
/// failure to read the lock is read as nothing to do.
fn revive(dir: &Path, now: u64, launcher: &dyn Launcher) -> Option<String> {
    let path = dir.join("step.lock");
    // An absent or empty record is the common case, answered with no lock taken and no file made.
    if std::fs::metadata(&path).map_or(true, |m| m.len() == 0) {
        return None;
    }
    let lock = HeldLock::try_take(&path).ok()??;
    let text = lock.contents().ok()?;
    if text.is_empty() {
        return None;
    }
    let here = dir.canonicalize().ok();
    let record = match serde_json::from_str::<DriverRecord>(&text) {
        Ok(record) if here.as_deref() == Some(record.dir.as_path()) => record,
        _ => {
            let _ = lock.replace("");
            return None;
        }
    };
    let since = |at: u64| Duration::from_secs(now.saturating_sub(at));
    if let Some(at) = record.relaunched_at {
        if since(at) < GRACE {
            return None;
        }
        let _ = lock.replace("");
        return Some(format!(
            "rigger: the relaunch of run {} made {}s ago never started a driver, so rigger left \
             the run stopped; relaunch it by hand: {}",
            record.run,
            since(at).as_secs(),
            record.by_hand()
        ));
    }
    if let Some(at) = record.revived_at.filter(|at| since(*at) < BACKOFF) {
        let _ = lock.replace("");
        return Some(format!(
            "rigger: run {} lost its driver again {}s after rigger relaunched it, so rigger left \
             it stopped; find why with `rigger status`, then relaunch it by hand: {}",
            record.run,
            since(at).as_secs(),
            record.by_hand()
        ));
    }
    let stamped = DriverRecord {
        relaunched_at: Some(now),
        ..record.clone()
    };
    let stamp = serde_json::to_string(&stamped).ok()?;
    lock.replace(&stamp).ok()?;
    // The lock is released before the launch, so the relaunched driver can take it.
    drop(lock);
    Some(match launcher.launch(&record.args, &record.cwd) {
        Ok(pid) => format!(
            "rigger: no driver was alive for this project's run {}, so rigger relaunched it in \
             the background (pid {pid}): {}. Follow it with `rigger status`.",
            record.run,
            record.by_hand()
        ),
        Err(e) => {
            if let Ok(Some(lock)) = HeldLock::try_take(&path) {
                let _ = lock.replace("");
            }
            format!(
                "rigger: no driver was alive for this project's run {}, and its relaunch failed \
                 ({e}); relaunch it by hand: {}",
                record.run,
                record.by_hand()
            )
        }
    })
}
