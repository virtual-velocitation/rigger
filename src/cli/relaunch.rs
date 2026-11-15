//! A RUN SURVIVES ITS DRIVER: a live run's driver keeps a record of itself in the step lock it
//! holds (`.rigger/step.lock`), the binary it runs as among it, and four verbs invoked from the
//! project's main checkout check it: the session-start hook `rigger prime`, `rigger status` read by
//! a person (neither `--line` nor `--json`), `rigger release` and `rigger resume-unit`
//! ([`check_point`]). No other verb checks, so none pays for it. A record whose lock no driver
//! holds is a driver that died, by a crash, a forced end, a hangup or a reboot, and the check
//! relaunches it by adoption, in the background, through the binary it recorded and with the
//! variables its roots resolve from set as it had them ([`ROOTS_ENV`]), as a hand
//! `rigger run <spec> --base <ref>` would. A driver that gave its agents the login token is
//! relaunched only by a check whose environment gives it too. The relaunched driver's stdout and stderr go to one
//! file under the store (`.rigger/driver.out`, [`DRIVER_OUTPUT_FILE`]), what it would have said on
//! the operator's terminal; each relaunch overwrites it and `rigger reset --runs` removes it once no driver holds the step
//! lock.
//!
//! STOPPED ON PURPOSE is every end the driver itself sees: any return of `rigger run` after it
//! took the step lock (done, escalated with nothing left, a halt, a refusal, an error), a panic
//! that unwinds it, and the operator's interactive interrupt (Ctrl-C), which the driver answers by
//! emptying its record ([`rigger::interrupt`]); a driver started ignoring interrupts is not ended
//! by one. Each empties the record, so nothing relaunches the run.
//!
//! AN OPERATOR HOLD IS THE STOP: no check relaunches a run an operator hold (`rigger hold`) stands
//! on, and a driver started to adopt one parks the run's next launch and ends, leaving its record,
//! so a held run stops for good however its driver then ends, until `rigger release` relaunches
//! it. A hold of any other writer, an outage's, is waited out by the driver and released by itself.
//!
//! The record's one reaper is the driver that wrote it, as it ends; a record whose driver ended
//! without reaping it is reaped by the check that relaunches it, by the driver that relaunch
//! starts, or, when no relaunch is owed, by the check itself.

use super::*;
use rigger::interrupt::{empty_on_interrupt, InterruptTrigger};
use std::collections::BTreeMap;
use std::ffi::OsString;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// The file under [`RIGGER_DIR`] a relaunched driver's stdout and stderr go to.
pub(crate) const DRIVER_OUTPUT_FILE: &str = "driver.out";

/// How long a relaunch is given to take the step lock before a check reads it as never started.
const GRACE: Duration = Duration::from_secs(30);
/// How long a relaunched driver must live for its death to be relaunched again: a driver that
/// dies sooner is left stopped, so a run whose driver cannot live is never relaunched in a loop.
const BACKOFF: Duration = Duration::from_secs(600);

/// The variables the run's roots resolve from, each named where it is read: the scratch root
/// (`RIGGER_TMPDIR`, then the cache home's `XDG_CACHE_HOME` and `HOME`), the state home that
/// roots the instance registry and the build cache (`XDG_STATE_HOME`, then `HOME`), and the walk
/// that finds the project's repository and store (`GIT_CEILING_DIRECTORIES`,
/// `GIT_DISCOVERY_ACROSS_FILESYSTEM`). A driver records each, set or not, and its relaunch sets or
/// removes each on the child as the driver had it, so the relaunched driver resolves the roots the
/// one that died did.
const ROOTS_ENV: [&str; 6] = [
    rigger::worktree::SCRATCH_ROOT_ENV,
    rigger::driver::replay::CACHE_HOME_ENV,
    rigger::registry::HOME_ENV,
    rigger::registry::STATE_HOME_ENV,
    rigger_worktree_git::discover::CEILINGS,
    rigger_worktree_git::discover::ACROSS_DEVICES,
];

/// Each of [`ROOTS_ENV`] with its value in this process, `None` where it is unset.
fn roots_env() -> BTreeMap<String, Option<OsString>> {
    ROOTS_ENV
        .iter()
        .map(|var| ((*var).to_string(), std::env::var_os(var)))
        .collect()
}

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
    /// The path of the binary the driver runs as, the one a relaunch starts. A record an older
    /// driver wrote names none, and is relaunched through the binary that checks it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    bin: Option<PathBuf>,
    /// The run the driver drives.
    run: String,
    /// Each of [`ROOTS_ENV`] as the driver had it: its raw value, `None` where it was unset. A
    /// record an older driver wrote has none, and is relaunched with the checker's.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    env: BTreeMap<String, Option<OsString>>,
    /// Whether the driver gave its agents the login token
    /// ([`rigger::claude_auth::agent_token_set`]): never the token itself.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    agent_token: bool,
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

    /// Whether a relaunch of this record, made inside the [`GRACE`] it is given to take the step
    /// lock, is still starting at `now`: its driver is live, though it holds no lock yet.
    fn starting(&self, now: u64) -> bool {
        self.relaunched_at
            .is_some_and(|at| Duration::from_secs(now.saturating_sub(at)) < GRACE)
    }
}

/// Whether the step lock under `rigger_dir` holds a record whose relaunch is still starting
/// ([`DriverRecord::starting`]), read without the lock, which its caller holds or found free.
fn a_relaunch_is_starting(rigger_dir: &Path) -> bool {
    std::fs::read_to_string(rigger_dir.join("step.lock"))
        .ok()
        .and_then(|text| serde_json::from_str::<DriverRecord>(&text).ok())
        .is_some_and(|record| record.starting(now_secs()))
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
    /// Whether the record is left for `rigger release` ([`KeptRecord::leave_for_release`]).
    left: std::cell::Cell<bool>,
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
            left: std::cell::Cell::new(false),
        }
    }

    /// Leave this driver's record in the lock as it ends, for `rigger release` to relaunch: the
    /// driver parked on an operator hold, a stop on purpose, so the record carries no relaunch it
    /// was started by.
    pub(super) fn leave_for_release(&self) {
        let parked = self
            .lock
            .contents()
            .ok()
            .and_then(|text| serde_json::from_str::<DriverRecord>(&text).ok());
        let Some(record) = parked else {
            return;
        };
        let record = DriverRecord {
            revived_at: None,
            ..record
        };
        let written = serde_json::to_string(&record)
            .map_err(std::io::Error::other)
            .and_then(|text| self.lock.replace(&text));
        if written.is_ok() {
            self.left.set(true);
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
            bin: Some(PathBuf::from(running_executable())),
            run: run.to_string(),
            env: roots_env(),
            agent_token: rigger::claude_auth::agent_token_set(),
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
        if !self.left.get() {
            let _ = self.lock.replace("");
        }
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
    /// Start the binary `bin` with `args` in `cwd`, in the background, with each of `env` set to
    /// its value or, given none, removed, its stdout and stderr in the file at `output`: the
    /// started driver's pid.
    fn launch(
        &self,
        bin: &Path,
        args: &[String],
        env: &BTreeMap<String, Option<OsString>>,
        cwd: &Path,
        output: &Path,
    ) -> std::io::Result<u32>;
}

/// The host's launcher: the binary, started detached ([`rigger::detached::start`]).
struct DetachedLauncher;

impl Launcher for DetachedLauncher {
    fn launch(
        &self,
        bin: &Path,
        args: &[String],
        env: &BTreeMap<String, Option<OsString>>,
        cwd: &Path,
        output: &Path,
    ) -> std::io::Result<u32> {
        rigger::detached::start(
            bin,
            args,
            env.iter().map(|(var, value)| (var, value.as_ref())),
            cwd,
            output,
        )
    }
}

/// The recording launcher [`rigger::detached::RELAUNCH_ENV`]'s `record:<path>` selects: a launch
/// appends `relaunch <bin> <args>` to `<path>` and starts nothing, its pid `0`.
struct RecordingLauncher(PathBuf);

impl Launcher for RecordingLauncher {
    fn launch(
        &self,
        bin: &Path,
        args: &[String],
        _env: &BTreeMap<String, Option<OsString>>,
        _cwd: &Path,
        _output: &Path,
    ) -> std::io::Result<u32> {
        append_line(
            &self.0,
            &format!("relaunch {} {}", bin.display(), args.join(" ")),
        )?;
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

/// When an invocation checks for a dead driver ([`check_point`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CheckPoint {
    /// Not at all.
    Never,
    /// Before its verb, since the verb may end the process itself.
    Before,
    /// After its verb, whose effect the relaunch must see.
    After,
}

/// When the invocation `args` (the verb first) checks for a dead driver: the session start
/// (`prime`) and `status` read by a person, neither `--line` nor `--json`, before their verb;
/// `release` and `resume-unit`, which change what a relaunch does, after it; every other verb,
/// never, so it pays nothing for the check.
pub(crate) fn check_point(args: &[String]) -> CheckPoint {
    let rest = args.get(1..).unwrap_or_default();
    match args.first().map(String::as_str) {
        Some("prime") => CheckPoint::Before,
        Some("status") if !rest.iter().any(|a| a == "--line" || a == "--json") => {
            CheckPoint::Before
        }
        Some("release" | "resume-unit") => CheckPoint::After,
        _ => CheckPoint::Never,
    }
}

/// THE RELAUNCH ENTRY: relaunch the run whose driver died in the project at `site`, the one call
/// the checking verbs make ([`check_point`]); `released` is whether it follows the operator's
/// `rigger release`. Silent unless it relaunched a driver or left a run stopped, which it says on
/// stderr in one line. An invocation inside a run (a spawn, a gate) or from a linked worktree
/// checks nothing.
pub(crate) fn relaunch_a_dead_driver(site: &Site, released: bool) {
    let inside_a_run = [
        rigger::driver::SPAWN_ENV,
        rigger::gate::STORE_FENCE_ENV,
        rigger::gate_channel::GATE_SOCKET_ENV,
    ]
    .iter()
    .any(|var| std::env::var_os(var).is_some());
    if inside_a_run {
        return;
    }
    let Ok(root) = site.project_root() else {
        return;
    };
    let dir = root.join(RIGGER_DIR);
    let check = Check {
        now: now_secs(),
        released,
        in_main_checkout: &|| main_worktree_or_refuse(site, "").is_ok(),
        operator_holds: &|run| operator_holds(site, run),
        agent_token: rigger::claude_auth::agent_token_set(),
    };
    let said = launcher().map_or_else(Some, |launcher| revive(&dir, &check, launcher.as_ref()));
    if let Some(line) = said {
        stderr_line(&line);
    }
}

/// Whether an operator hold stands on `run`, the current run of the project at `site`. A store
/// that cannot be read answers no hold: the relaunched driver reads the hold again as it adopts
/// the run and parks on an operator hold it finds.
fn operator_holds(site: &Site, run: &str) -> bool {
    require_store_dir_in(site)
        .and_then(|(loc, selection)| {
            over_the_run_store_at(&loc, &selection, |store| {
                let current = runscope::read::read_current_run_id(store, conductor::STREAM)?;
                let hold = runscope::read::read_current_hold(store, conductor::STREAM)?;
                Ok(current.as_deref() == Some(run) && hold.is_some_and(|hold| hold.operator))
            })
        })
        .unwrap_or(false)
}

/// What one check reads beside the step lock: the time it runs at, whether it follows the
/// operator's `rigger release`, whether it was invoked from the project's main checkout, whether
/// an operator hold stands on a run, and whether its own environment gives agents the login
/// token.
struct Check<'a> {
    now: u64,
    released: bool,
    in_main_checkout: &'a dyn Fn() -> bool,
    operator_holds: &'a dyn Fn(&str) -> bool,
    agent_token: bool,
}

/// Relaunch the driver whose record the step lock under `dir` holds when no driver holds that
/// lock, as `check` reads it, through `launcher`: the line to say, `None` when there is nothing to
/// say. Every failure to read the lock is read as nothing to do. A check invoked from a linked
/// worktree, and a run an operator hold stands on, leave the record as it is: the hold is the
/// run's stop. A check that follows the operator's release relaunches a driver that died soon
/// after its own relaunch too, since the operator asked for it. A driver that gave its agents the
/// login token is relaunched only from an environment that has it too, and is otherwise left as
/// it is, naming the command that relaunches it by hand. A record whose binary is gone is
/// cleared, naming that command.
fn revive(dir: &Path, check: &Check, launcher: &dyn Launcher) -> Option<String> {
    let now = check.now;
    let path = dir.join("step.lock");
    // An absent or empty record is the common case, answered with no lock taken and no file made.
    if std::fs::metadata(&path).map_or(true, |m| m.len() == 0) {
        return None;
    }
    if !(check.in_main_checkout)() {
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
    if (check.operator_holds)(&record.run) {
        return Some(format!(
            "rigger: run {} is held by the operator and no driver is alive for it; `rigger \
             release` relaunches it",
            record.run
        ));
    }
    let since = |at: u64| Duration::from_secs(now.saturating_sub(at));
    if let Some(at) = record.relaunched_at {
        if record.starting(now) {
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
    let backoff = record
        .revived_at
        .filter(|at| !check.released && since(*at) < BACKOFF);
    if let Some(at) = backoff {
        let _ = lock.replace("");
        return Some(format!(
            "rigger: run {} lost its driver again {}s after rigger relaunched it, so rigger left \
             it stopped; find why with `rigger status`, then relaunch it by hand: {}",
            record.run,
            since(at).as_secs(),
            record.by_hand()
        ));
    }
    if record.agent_token && !check.agent_token {
        return Some(format!(
            "rigger: the driver of run {} ran its agents on the login token in {}, which this \
             invocation's environment does not set, so rigger relaunched nothing; relaunch it by \
             hand from an environment that sets it: {}",
            record.run,
            rigger::claude_auth::RIGGER_AGENT_TOKEN_VAR,
            record.by_hand()
        ));
    }
    let bin = record
        .bin
        .clone()
        .unwrap_or_else(|| PathBuf::from(running_executable()));
    if !bin.is_file() {
        let _ = lock.replace("");
        return Some(format!(
            "rigger: the binary {} that drove run {} no longer exists, so rigger left the run \
             stopped; relaunch it by hand: {}",
            bin.display(),
            record.run,
            record.by_hand()
        ));
    }
    let output = dir.join(DRIVER_OUTPUT_FILE);
    let stamped = DriverRecord {
        relaunched_at: Some(now),
        ..record.clone()
    };
    let stamp = serde_json::to_string(&stamped).ok()?;
    lock.replace(&stamp).ok()?;
    // The lock is released before the launch, so the relaunched driver can take it.
    drop(lock);
    Some(
        match launcher.launch(&bin, &record.args, &record.env, &record.cwd, &output) {
            Ok(pid) => format!(
            "rigger: no driver was alive for this project's run {}, so rigger relaunched it in \
             the background (pid {pid}), its output in {}: {}. Follow it with `rigger status`.",
            record.run,
            output.display(),
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
        },
    )
}

/// What `rigger status` says of a relaunched driver that ended before its run did: the file its
/// output went to, while that file holds something, no driver holds the step lock under
/// `rigger_dir`, no relaunch is still starting ([`a_relaunch_is_starting`]) and the run is not
/// `done`; `None` otherwise.
pub(crate) fn stopped_relaunch_line(rigger_dir: &Path, done: bool) -> Option<String> {
    let output = rigger_dir.join(DRIVER_OUTPUT_FILE);
    if done || std::fs::metadata(&output).map_or(true, |m| m.len() == 0) {
        return None;
    }
    let lock = HeldLock::try_take(&rigger_dir.join("step.lock")).ok()??;
    if a_relaunch_is_starting(rigger_dir) {
        return None;
    }
    drop(lock);
    Some(format!(
        "relaunched driver: ended before its run did; what it said is in {}",
        output.display()
    ))
}

/// Remove the relaunched driver's output under `rigger_dir`, `rigger reset --runs`'s reap of
/// it once no driver holds the step lock and no relaunch is still starting
/// ([`a_relaunch_is_starting`]), whose driver is about to write it: whether one was removed.
pub(crate) fn forget_driver_output(rigger_dir: &Path) -> std::io::Result<bool> {
    if a_relaunch_is_starting(rigger_dir) {
        return Ok(false);
    }
    match std::fs::remove_file(rigger_dir.join(DRIVER_OUTPUT_FILE)) {
        Ok(()) => Ok(true),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A launcher that keeps the environment each launch is handed and starts nothing.
    #[derive(Default)]
    struct EnvKeeper(std::cell::RefCell<Vec<BTreeMap<String, Option<OsString>>>>);

    impl Launcher for EnvKeeper {
        fn launch(
            &self,
            _bin: &Path,
            _args: &[String],
            env: &BTreeMap<String, Option<OsString>>,
            _cwd: &Path,
            _output: &Path,
        ) -> std::io::Result<u32> {
            self.0.borrow_mut().push(env.clone());
            Ok(0)
        }
    }

    /// A relaunch hands its launcher the roots variables the dead driver recorded, each set or
    /// removed as the driver had it, and a record an older driver wrote, which recorded none,
    /// hands it none, so the relaunch keeps the checker's.
    #[test]
    fn a_relaunch_hands_the_launcher_the_roots_variables_the_driver_recorded() {
        let recorded: BTreeMap<String, Option<OsString>> = [
            (
                rigger::worktree::SCRATCH_ROOT_ENV,
                Some("/the/driver's/scratch"),
            ),
            (rigger::registry::STATE_HOME_ENV, None),
        ]
        .into_iter()
        .map(|(var, value)| (var.to_string(), value.map(OsString::from)))
        .collect();
        let rows: [(&str, BTreeMap<String, Option<OsString>>); 2] = [
            ("a driver that recorded its roots variables", recorded),
            ("a driver that recorded none", BTreeMap::new()),
        ];
        for (name, env) in rows {
            let dir = tempfile::tempdir().unwrap();
            let record = DriverRecord {
                dir: dir.path().canonicalize().unwrap(),
                cwd: dir.path().to_path_buf(),
                args: vec!["run".into(), "--adopt".into(), "a-run".into()],
                bin: Some(PathBuf::from(running_executable())),
                run: "a-run".into(),
                env: env.clone(),
                agent_token: false,
                relaunched_at: None,
                revived_at: None,
            };
            std::fs::write(
                dir.path().join("step.lock"),
                serde_json::to_string(&record).unwrap(),
            )
            .unwrap();
            let check = Check {
                now: now_secs(),
                released: false,
                in_main_checkout: &|| true,
                operator_holds: &|_| false,
                agent_token: false,
            };
            let keeper = EnvKeeper::default();
            revive(dir.path(), &check, &keeper);
            assert_eq!(keeper.0.into_inner(), vec![env], "{name}");
        }
    }

    /// A kept record holds each of the roots variables as this process has it, set or not.
    #[test]
    fn a_kept_record_holds_each_roots_variable_as_the_driver_has_it() {
        let dir = tempfile::tempdir().unwrap();
        let parsed = RunArgs {
            store: None,
            conn: None,
            spec: Some("specs/a.md".into()),
            fresh: false,
            rebase_definition: false,
            base: Some("main".into()),
            adopt: None,
        };
        let kept = KeptRecord::hold(acquire_step_lock(dir.path()).unwrap());
        kept.keep(&parsed, dir.path(), dir.path(), "a-run");
        let record: DriverRecord = serde_json::from_str(&kept.lock.contents().unwrap()).unwrap();
        let expected: BTreeMap<String, Option<OsString>> = ROOTS_ENV
            .iter()
            .map(|var| ((*var).to_string(), std::env::var_os(var)))
            .collect();
        assert_eq!(record.env, expected);
    }

    /// A panic that unwinds the driver is a stop on purpose: the keeper's drop empties the record
    /// as the unwind passes it, so no check relaunches the run.
    #[test]
    fn a_panic_that_unwinds_the_driver_empties_its_record() {
        let dir = tempfile::tempdir().unwrap();
        let parsed = RunArgs {
            store: None,
            conn: None,
            spec: Some("specs/a.md".into()),
            fresh: false,
            rebase_definition: false,
            base: Some("main".into()),
            adopt: None,
        };
        let unwound = std::panic::catch_unwind(|| {
            let kept = KeptRecord::hold(acquire_step_lock(dir.path()).unwrap());
            kept.keep(&parsed, dir.path(), dir.path(), "a-run");
            let kept_record = kept.lock.contents().unwrap();
            assert!(
                kept_record.contains("a-run"),
                "a live driver keeps its record: {kept_record:?}"
            );
            panic!("the driver panicked mid-run");
        });
        assert!(unwound.is_err(), "the driver's panic unwound it");
        assert_eq!(
            std::fs::read_to_string(dir.path().join("step.lock")).unwrap(),
            "",
            "a panic that unwinds the driver leaves no record to relaunch"
        );
    }
}
