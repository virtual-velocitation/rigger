//! A RUN SURVIVES ITS DRIVER: a live run's driver keeps a record of itself in the step lock it
//! holds (`.rigger/step.lock`), the binary it runs as among it, and four verbs invoked from the
//! project's main checkout check it: the session-start hook `rigger prime`, `rigger status` read by
//! a person (neither `--line` nor `--json`), `rigger release` and `rigger resume-unit`
//! ([`check_point`]). No other verb checks, so none pays for it; the login entry a run opted into
//! checks it too (below). A record whose lock no driver
//! holds is a driver that died, by a crash, a forced end, a hangup or a reboot, and the check
//! relaunches it by adoption, in the background, through the binary it recorded and with the
//! variables its roots and its tools' search path resolve from set as it had them
//! ([`ROOTS_ENV`]), as a hand `rigger run <spec> --base <ref>` would. A driver that ran on a credential ([`CREDENTIALS_ENV`])
//! is relaunched only by a check whose environment sets it too. The relaunched driver's stdout and stderr go to one
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
//! A RUN COMES BACK AT LOGIN WHERE ITS USER OPTED IN: a driver started with
//! `--relaunch-at-login` registers, once it holds the step lock, one entry under its project's key
//! with the OS's login-start facility ([`rigger::login_start`]), which runs its binary as
//! `rigger run --at-login <root>` at the user's next login ([`relaunch_at_login`]). A driver the
//! machine's end killed (a power loss, a reboot, a forced end) leaves its record and the entry,
//! and that login relaunches the run through the one relaunch entry; a driver parked on an
//! operator hold leaves both for `rigger release`. Without the flag nothing is registered.
//!
//! The record's one reaper is the driver that wrote it, as it ends; a record whose driver ended
//! without reaping it is reaped by the check that relaunches it, by the driver that relaunch
//! starts, or, when no relaunch is owed, by the check itself. The login entry's reaper is the
//! driver that registered it, as it empties its record; an entry it left is reaped by the login
//! that finds no relaunch owed: a run that ended, a run last started without the flag, a record a
//! relaunch cleared, and a project or store since removed.

use super::*;
use rigger::interrupt::{empty_on_interrupt, InterruptTrigger};
use rigger::login_start::{run_key, LoginStart};
use rigger::sleep_hold::Unavailable;
use std::collections::{BTreeMap, BTreeSet};
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
/// (`RIGGER_TMPDIR`, then the cache home's `XDG_CACHE_HOME` and the home directory), the state
/// home that roots the instance registry (`XDG_STATE_HOME`, then the home directory), and the walk
/// that finds the project's repository and store (`GIT_CEILING_DIRECTORIES`,
/// `GIT_DISCOVERY_ACROSS_FILESYSTEM`), and the home directory ([`rigger_process::home_dir`]) by
/// the variable it is read from on this OS ([`rigger_process::home::HOME_ENV`]), with the search
/// path the driver finds its tools on ([`rigger::subprocess::PATH_ENV`]): the agent host, git and
/// the project's gate tools, which the short search path a login manager starts a login entry on
/// does not reach. A driver records each, set or not, and its relaunch sets or removes each on the
/// child as the driver had it, so the relaunched driver resolves the roots and finds the tools the
/// one that died did.
const ROOTS_ENV: [&str; 7] = [
    rigger::subprocess::PATH_ENV,
    rigger::worktree::SCRATCH_ROOT_ENV,
    rigger::driver::replay::CACHE_HOME_ENV,
    rigger_process::home::HOME_ENV,
    rigger::registry::STATE_HOME_ENV,
    rigger_worktree_git::discover::CEILINGS,
    rigger_worktree_git::discover::ACROSS_DEVICES,
];

/// A credential a driver can run on, by the variable it is named by.
struct Credential {
    /// The variable it is read from.
    var: &'static str,
    /// Whether this process's environment sets it.
    set: fn() -> bool,
    /// Whether a driver's own arguments give it, beside its environment.
    given: fn(&RunArgs) -> bool,
    /// How else than by its variable an operator passes it to a relaunch by hand, said after it.
    or_else: &'static str,
}

/// The credentials a driver can run on: the login token its agents get, and the server event
/// store's connection string, which `--conn` gives too. A driver records which it had, never a
/// value, and only a check whose environment sets each of them relaunches it, since a relaunch
/// without one runs its agents on another login or drives another store.
const CREDENTIALS_ENV: [Credential; 2] = [
    Credential {
        var: rigger::claude_auth::RIGGER_AGENT_TOKEN_VAR,
        set: rigger::claude_auth::agent_token_set,
        given: |_| false,
        or_else: "",
    },
    Credential {
        var: CONN_ENV,
        set: || env_conn().is_some(),
        given: |parsed| parsed.conn.as_deref().is_some_and(|conn| !conn.is_empty()),
        or_else: ", or with `--conn` and the connection string added",
    },
];

/// The variables of the [`CREDENTIALS_ENV`] this process has: those its environment sets, and,
/// for a driver started with `parsed`, those its arguments give.
fn credentials_had(parsed: Option<&RunArgs>) -> BTreeSet<String> {
    CREDENTIALS_ENV
        .iter()
        .filter(|credential| {
            (credential.set)() || parsed.is_some_and(|parsed| (credential.given)(parsed))
        })
        .map(|credential| credential.var.to_string())
        .collect()
}

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
    /// Which of [`CREDENTIALS_ENV`] the driver had set: never a value.
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    credentials: BTreeSet<String>,
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
/// return or panic (this guard's drop) or by an interactive interrupt (its trigger), the login
/// entry it registered removed with it.
pub(super) struct KeptRecord {
    /// Dropped before the lock, so the trigger never outlives the file it empties.
    _interrupt: InterruptTrigger,
    lock: HeldLock,
    /// The record a driver before this one left in the lock, when it parses.
    prior: Option<DriverRecord>,
    /// Whether the record is left for `rigger release` ([`KeptRecord::leave_for_release`]).
    left: std::cell::Cell<bool>,
    /// The login entry this driver registered ([`KeptRecord::register_at_login`]).
    at_login: std::cell::OnceCell<AtLogin>,
}

/// A login entry a driver registered: the facility it is registered with, and its key.
struct AtLogin {
    start: Box<dyn LoginStart>,
    key: String,
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
            at_login: std::cell::OnceCell::new(),
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

    /// Register this driver's relaunch with the login-start facility `start`, the user's opt-in
    /// (`--relaunch-at-login`): one entry under the project's key ([`run_key`]) that runs this
    /// binary as `run --at-login <root>`, `<root>` the project whose store is at `dir`, so a login
    /// after the machine went down mid-run relaunches the run ([`relaunch_at_login`]). The entry
    /// is removed as the record is emptied, and left beside a record left for `rigger release`.
    /// A registration the facility refuses is the launch's refusal, naming why.
    pub(super) fn register_at_login(
        &self,
        start: Box<dyn LoginStart>,
        dir: &Path,
    ) -> Result<(), String> {
        let refused = |why: String| {
            format!("rigger run: {RELAUNCH_AT_LOGIN_FLAG}: no login entry was made: {why}")
        };
        let dir = dir
            .canonicalize()
            .map_err(|e| refused(format!("{}: {e}", dir.display())))?;
        let program = standing_executable();
        let (Some(root), Some(bin)) = (dir.parent().and_then(Path::to_str), program.to_str())
        else {
            return Err(refused(format!(
                "the project {} or the binary {} is not a UTF-8 path",
                dir.display(),
                program.display()
            )));
        };
        let key = run_key(root);
        let args = ["run", AT_LOGIN_FLAG, root].map(str::to_string);
        start
            .register(&key, bin, &args)
            .map_err(|Unavailable(why)| refused(why))?;
        let _ = self.at_login.set(AtLogin { start, key });
        Ok(())
    }

    /// Record this driver: started in `cwd` against the store at `dir` with `parsed`, driving
    /// `run`. A driver a relaunch of the same run started carries when that relaunch was made; a
    /// driver started by hand removes what an earlier relaunched driver said, so `rigger status`
    /// stops saying a relaunched driver ended once that driver is gone. A record that cannot be
    /// written is said on stderr; the run drives on, unrelaunchable.
    pub(super) fn keep(&self, parsed: &RunArgs, cwd: &Path, dir: &Path, run: &str) {
        let started_by_hand = self
            .prior
            .as_ref()
            .and_then(|prior| prior.relaunched_at)
            .is_none();
        let revived_at = self
            .prior
            .as_ref()
            .filter(|prior| parsed.adopt.is_some() && prior.run == run)
            .and_then(|prior| prior.relaunched_at);
        let record = DriverRecord {
            dir: dir.canonicalize().unwrap_or_else(|_| dir.to_path_buf()),
            cwd: cwd.to_path_buf(),
            args: relaunch_args(parsed, run),
            bin: Some(standing_executable()),
            run: run.to_string(),
            env: roots_env(),
            credentials: credentials_had(Some(parsed)),
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
        if let Err(e) = started_by_hand
            .then(|| forget_driver_output(dir))
            .transpose()
        {
            stderr_line(&format!(
                "rigger run: an earlier relaunched driver's output was not removed ({e})"
            ));
        }
    }
}

impl Drop for KeptRecord {
    /// Empty the record and remove the login entry this driver registered, unless the record is
    /// left for `rigger release`. An entry that cannot be removed is said on stderr; the next login
    /// finds no relaunch owed and removes it ([`relaunch_at_login`]).
    fn drop(&mut self) {
        if self.left.get() {
            return;
        }
        let _ = self.lock.replace("");
        if let Some(AtLogin { start, key }) = self.at_login.take() {
            if let Err(Unavailable(why)) = start.unregister(&key) {
                stderr_line(&format!(
                    "rigger run: the login entry {key} was not removed ({why}); the next login \
                     removes it"
                ));
            }
        }
    }
}

/// The arguments that relaunch the run `parsed` started as `run`: `run`, the spec, an explicit
/// base (the flag's or `RIGGER_BASE`'s), the event store a flag chose, the opt-in to a login
/// entry, then `--adopt <run>`. `--fresh` and `--rebase-definition` are a start's own, and
/// `--conn` may carry a credential, so none is recorded.
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
    if parsed.relaunch_at_login {
        args.push(RELAUNCH_AT_LOGIN_FLAG.to_string());
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
        credentials: credentials_had(None),
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
/// an operator hold stands on a run, and which credentials its own environment sets.
struct Check<'a> {
    now: u64,
    released: bool,
    in_main_checkout: &'a dyn Fn() -> bool,
    operator_holds: &'a dyn Fn(&str) -> bool,
    credentials: BTreeSet<String>,
}

/// Relaunch the driver whose record the step lock under `dir` holds when no driver holds that
/// lock, as `check` reads it, through `launcher`: the line to say, `None` when there is nothing to
/// say. Every failure to read the lock is read as nothing to do. A check invoked from a linked
/// worktree, and a run an operator hold stands on, leave the record as it is: the hold is the
/// run's stop. A check that follows the operator's release relaunches a driver that died soon
/// after its own relaunch too, since the operator asked for it. A driver that ran on a
/// credential is relaunched only from an environment that sets it too, and is otherwise left as
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
        return check
            .released
            .then(|| unrelaunched_release(&path))
            .flatten();
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
    if let Some(var) = record.credentials.difference(&check.credentials).next() {
        let or_else = CREDENTIALS_ENV
            .iter()
            .find(|credential| credential.var == var)
            .map_or("", |credential| credential.or_else);
        return Some(format!(
            "rigger: the driver of run {} ran on a credential this invocation's environment does \
             not have ({var} unset), so rigger relaunched nothing; relaunch it by hand from an \
             environment that sets {var}{or_else}: {}",
            record.run,
            record.by_hand()
        ));
    }
    let bin = record.bin.clone().unwrap_or_else(standing_executable);
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

/// What a `rigger release` invoked from a linked worktree says of the dead driver whose record
/// the step lock at `path` holds, read without the lock since nothing here changes it: that its
/// run is released and what relaunches it, since no check from a linked worktree relaunches.
fn unrelaunched_release(path: &Path) -> Option<String> {
    let record = serde_json::from_str::<DriverRecord>(&std::fs::read_to_string(path).ok()?).ok()?;
    Some(format!(
        "rigger: run {} is released, and no driver is alive for it; this is a linked worktree, \
         so rigger relaunched nothing: `rigger status` or `rigger release` from the main \
         checkout relaunches it",
        record.run
    ))
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

/// THE LOGIN ENTRY'S RUN, `rigger run --at-login <root>`: relaunch the run of the project at
/// `root` whose driver died through the one relaunch entry ([`relaunch_a_dead_driver`]) while a
/// login owes it one ([`owed_at_login`]); once none is owed, before that relaunch or after it,
/// remove the project's entry from `start`. So the entry outlives only a driver the machine's end
/// killed, and every other end is reaped at the next login.
pub(super) fn relaunch_at_login(root: &str, start: &dyn LoginStart) -> Res {
    let dir = Path::new(root).join(RIGGER_DIR);
    if owed_at_login(&dir) {
        relaunch_a_dead_driver(&Site::of(Path::new(root)), false);
        if owed_at_login(&dir) {
            return Ok(());
        }
    }
    start
        .unregister(&run_key(root))
        .map_err(|Unavailable(why)| {
            format!("rigger run {AT_LOGIN_FLAG}: the login entry of {root} was not removed: {why}")
        })?;
    Ok(())
}

/// Whether a login owes the project whose store is at `dir` its relaunch: a driver holds the step
/// lock there, or none does and the lock holds the record of a driver started with
/// `--relaunch-at-login`. A store or lock that is gone, and a free lock over an emptied,
/// unreadable or unflagged record, owe nothing. A lock that cannot be taken is read as held, so a
/// failure to read it never removes the entry of a run still owed.
fn owed_at_login(dir: &Path) -> bool {
    let path = dir.join("step.lock");
    if !path.is_file() {
        return false;
    }
    match HeldLock::try_take(&path) {
        Ok(Some(lock)) => lock
            .contents()
            .ok()
            .and_then(|text| serde_json::from_str::<DriverRecord>(&text).ok())
            .is_some_and(|record| record.args.iter().any(|arg| arg == RELAUNCH_AT_LOGIN_FLAG)),
        Ok(None) | Err(_) => true,
    }
}

/// This binary's path as it stands, the path a relaunch of it runs: an upgrade that replaced it
/// while it runs leaves the new build there ([`rigger::detached::standing_path`]).
fn standing_executable() -> PathBuf {
    rigger::detached::standing_path(Path::new(&running_executable()))
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
                bin: Some(standing_executable()),
                run: "a-run".into(),
                env: env.clone(),
                credentials: BTreeSet::new(),
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
                credentials: BTreeSet::new(),
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
            spec: Some("specs/a.md".into()),
            base: Some("main".into()),
            ..RunArgs::default()
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

    /// A relaunch starts the driver on the PATH the driver that died found its tools on, not the
    /// checker's: a login manager starts what a login entry runs on its own short PATH, which
    /// finds none of the agent host, git or the project's gate tools the run was started with.
    #[test]
    fn a_relaunch_starts_the_driver_on_the_path_it_found_its_tools_on() {
        let dir = tempfile::tempdir().unwrap();
        let parsed = RunArgs {
            adopt: Some("a-run".into()),
            ..RunArgs::default()
        };
        let kept = KeptRecord::hold(acquire_step_lock(dir.path()).unwrap());
        kept.keep(&parsed, dir.path(), dir.path(), "a-run");
        let text = kept.lock.contents().unwrap();
        drop(kept);
        std::fs::write(dir.path().join("step.lock"), &text).unwrap();
        let check = Check {
            now: now_secs(),
            released: false,
            in_main_checkout: &|| true,
            operator_holds: &|_| false,
            credentials: BTreeSet::new(),
        };
        let keeper = EnvKeeper::default();
        revive(dir.path(), &check, &keeper);
        let handed: Vec<Option<OsString>> = keeper
            .0
            .into_inner()
            .into_iter()
            .map(|env| env.get("PATH").cloned().flatten())
            .collect();
        assert_eq!(handed, vec![std::env::var_os("PATH")], "record: {text}");
    }

    /// A driver given the store connection by `--conn` has had the store connection as surely as
    /// one given it by its variable: its record names the variable, never the value, and a check
    /// whose environment does not set the variable relaunches nothing, saying how the operator
    /// passes the connection to a relaunch by hand. A flag left empty gives no connection.
    #[test]
    fn a_driver_given_the_connection_by_its_flag_is_relaunched_only_where_the_connection_is_set() {
        let url = "kurrentdb://127.0.0.1:2113?tls=false";
        let rows = [
            ("a driver given no connection flag", None, false),
            ("a driver given the connection by its flag", Some(url), true),
            ("a driver given an empty connection flag", Some(""), false),
        ];
        for (name, conn, flagged) in rows {
            let dir = tempfile::tempdir().unwrap();
            let parsed = RunArgs {
                conn: conn.map(str::to_string),
                adopt: Some("a-run".into()),
                ..RunArgs::default()
            };
            let kept = KeptRecord::hold(acquire_step_lock(dir.path()).unwrap());
            kept.keep(&parsed, dir.path(), dir.path(), "a-run");
            let text = kept.lock.contents().unwrap();
            drop(kept);
            std::fs::write(dir.path().join("step.lock"), &text).unwrap();
            let record: DriverRecord = serde_json::from_str(&text).unwrap();
            let mut expected = credentials_had(None);
            if flagged {
                expected.insert(CONN_ENV.to_string());
            }
            let check = Check {
                now: now_secs(),
                released: false,
                in_main_checkout: &|| true,
                operator_holds: &|_| false,
                credentials: BTreeSet::new(),
            };
            let said = revive(dir.path(), &check, &EnvKeeper::default()).unwrap_or_default();
            assert_eq!(
                (
                    record.credentials,
                    said.contains(&format!("({CONN_ENV} unset)")) && said.contains("`--conn`"),
                    text.contains(url) || said.contains(url),
                ),
                (expected, flagged, false),
                "{name}; record: {text}; said: {said}"
            );
        }
    }

    /// A panic that unwinds the driver is a stop on purpose: the keeper's drop empties the record
    /// as the unwind passes it, so no check relaunches the run.
    #[test]
    fn a_panic_that_unwinds_the_driver_empties_its_record() {
        let dir = tempfile::tempdir().unwrap();
        let parsed = RunArgs {
            spec: Some("specs/a.md".into()),
            base: Some("main".into()),
            ..RunArgs::default()
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

    /// What a login-start facility was asked, in order, kept in memory; given a reason, it refuses
    /// every registration with it, recording nothing for it.
    #[derive(Clone, Default)]
    struct Asked {
        calls: std::rc::Rc<std::cell::RefCell<Vec<String>>>,
        refuses: Option<&'static str>,
    }

    impl LoginStart for Asked {
        fn register(&self, key: &str, program: &str, args: &[String]) -> Result<(), Unavailable> {
            if let Some(why) = self.refuses {
                return Err(Unavailable(why.to_string()));
            }
            self.calls
                .borrow_mut()
                .push(format!("register {key} {program} {}", args.join(" ")));
            Ok(())
        }

        fn unregister(&self, key: &str) -> Result<(), Unavailable> {
            self.calls.borrow_mut().push(format!("unregister {key}"));
            Ok(())
        }

        fn entries(&self) -> Result<Vec<rigger::login_start::Entry>, Unavailable> {
            Ok(Vec::new())
        }
    }

    /// How a driver that took the step lock ends, for
    /// [`a_driver_removes_its_login_entry_as_its_record_is_emptied`].
    #[derive(Clone, Copy, Debug, PartialEq)]
    enum Ends {
        Returns,
        Panics,
        ParksForRelease,
    }

    /// A driver started with `--relaunch-at-login` registers, once it holds the step lock, one
    /// entry under its project's key ([`run_key`]) that runs this binary as `run --at-login
    /// <root>`, and removes it as its record is emptied, by a return or a panic that unwinds it; a
    /// driver parked for `rigger release` leaves the entry beside the record it leaves. A
    /// registration the facility refuses refuses the launch, naming why, and leaves nothing for
    /// the driver's end to remove.
    #[test]
    fn a_driver_removes_its_login_entry_as_its_record_is_emptied() {
        let rows = [
            ("a driver that returns", None, Ends::Returns, true),
            ("a driver a panic unwinds", None, Ends::Panics, true),
            (
                "a driver parked for release",
                None,
                Ends::ParksForRelease,
                false,
            ),
            (
                "a driver whose facility refuses",
                Some("no login session here"),
                Ends::Returns,
                false,
            ),
        ];
        for (case, refuses, ends, removed) in rows {
            let project = tempfile::tempdir().unwrap();
            let dir = project.path().join(RIGGER_DIR);
            std::fs::create_dir_all(&dir).unwrap();
            let root = project.path().canonicalize().unwrap();
            let root = root.to_str().unwrap();
            let key = run_key(root);
            let asked = Asked {
                refuses,
                ..Asked::default()
            };
            let parsed = RunArgs {
                adopt: Some("a-run".into()),
                relaunch_at_login: true,
                ..RunArgs::default()
            };
            let refused = std::cell::RefCell::new(None);
            let unwound = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let kept = KeptRecord::hold(acquire_step_lock(&dir).unwrap());
                if let Err(why) = kept.register_at_login(Box::new(asked.clone()), &dir) {
                    *refused.borrow_mut() = Some(why);
                    return;
                }
                kept.keep(&parsed, &dir, &dir, "a-run");
                match ends {
                    Ends::Returns => {}
                    Ends::Panics => panic!("the driver panicked mid-run"),
                    Ends::ParksForRelease => kept.leave_for_release(),
                }
            }));
            let mut expected = Vec::new();
            if refuses.is_none() {
                expected.push(format!(
                    "register {key} {} run --at-login {root}",
                    standing_executable().display()
                ));
            }
            if removed {
                expected.push(format!("unregister {key}"));
            }
            let refused = refused.into_inner();
            assert_eq!(
                (
                    unwound.is_err(),
                    asked.calls.borrow().clone(),
                    refused
                        .as_deref()
                        .map(|why| refuses.is_some_and(|reason| why.contains(reason))),
                ),
                (ends == Ends::Panics, expected, refuses.map(|_| true)),
                "{case}; refused: {refused:?}"
            );
        }
    }

    /// A login owes a project its relaunch while the step lock holds the record of a driver
    /// started with `--relaunch-at-login`, and while a driver holds the lock at all; a store or lock
    /// that is gone, and a free lock over an emptied, unreadable or unflagged record, owe nothing.
    #[test]
    fn a_login_owes_a_relaunch_only_over_a_flagged_record_or_a_held_lock() {
        let record = |args: &[&str]| {
            serde_json::to_string(&DriverRecord {
                dir: PathBuf::from("/home/dev/project/.rigger"),
                cwd: PathBuf::from("/home/dev/project"),
                args: args.iter().map(|arg| arg.to_string()).collect(),
                bin: None,
                run: "a-run".into(),
                env: BTreeMap::new(),
                credentials: BTreeSet::new(),
                relaunched_at: None,
                revived_at: None,
            })
            .unwrap()
        };
        let flagged = record(&["run", "--relaunch-at-login", "--adopt", "a-run"]);
        let unflagged = record(&["run", "--adopt", "a-run"]);
        let rows: [(&str, Option<&str>, bool, bool); 7] = [
            ("no store", None, false, false),
            ("an emptied record", Some(""), false, false),
            (
                "a record that does not parse",
                Some("not a record"),
                false,
                false,
            ),
            ("an unflagged record", Some(&unflagged), false, false),
            ("a flagged record", Some(&flagged), false, true),
            ("a held lock over an emptied record", Some(""), true, true),
            (
                "a held lock over an unflagged record",
                Some(&unflagged),
                true,
                true,
            ),
        ];
        for (case, text, held, owed) in rows {
            let project = tempfile::tempdir().unwrap();
            let dir = project.path().join(RIGGER_DIR);
            if let Some(text) = text {
                std::fs::create_dir_all(&dir).unwrap();
                std::fs::write(dir.join("step.lock"), text).unwrap();
            }
            let _holder =
                held.then(|| HeldLock::try_take(&dir.join("step.lock")).unwrap().unwrap());
            assert_eq!(owed_at_login(&dir), owed, "{case}");
        }
    }
}
