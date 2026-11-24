//! THE LOGIN READ (spec 105): the one reader of `claude auth status --json`, which the hold probe
//! runs ahead of its canary on a hold whose cause a login fixes (RELEASE) and `rigger validate`
//! runs to report the auth method (WHAT A PERSON SEES). The JSON alone answers, each read on its
//! own key: the session is logged in only when the report's `loggedIn` is `true`, and its auth
//! method is the report's `authMethod` string; a report that does not come by the deadline, does
//! not parse or names no such key answers not logged in, and no auth method. Every read and
//! every session starts the one [`SessionHost`]: which binary the host is and how it is started.

use std::ops::ControlFlow;
use std::process::Stdio;
use std::time::Instant;

use serde::Deserialize;

use crate::child_tree::ReapedChild;

use rigger_domain::spawn::AGENT_TOKEN_VAR;
pub use rigger_domain::spawn::RIGGER_AGENT_TOKEN_VAR;

/// The ONE credential rule every launch of the agent binary applies (unit spawns of both hosts,
/// the hold probe's canary and the login reads `claude auth status` that the probe and `rigger
/// validate` make): a non-empty [`RIGGER_AGENT_TOKEN_VAR`] becomes [`AGENT_TOKEN_VAR`] on `cmd`
/// alone, and is not passed on itself. Unset or empty, the inherited environment rides through
/// untouched, an ambient [`AGENT_TOKEN_VAR`] included. The value is set on the command and held
/// nowhere else: no field, no record, no message.
fn apply_agent_credential(cmd: &mut std::process::Command) {
    if let Some(token) = agent_token() {
        cmd.env(AGENT_TOKEN_VAR, token);
        cmd.env_remove(RIGGER_AGENT_TOKEN_VAR);
    }
}

/// The token [`apply_agent_credential`] gives spawned agents: [`RIGGER_AGENT_TOKEN_VAR`]'s value,
/// when it is set and not empty.
fn agent_token() -> Option<std::ffi::OsString> {
    std::env::var_os(RIGGER_AGENT_TOKEN_VAR).filter(|v| !v.is_empty())
}

/// Whether this process gives spawned agents a login token ([`apply_agent_credential`]), the one
/// fact about the credential a caller may keep: never its value.
pub fn agent_token_set() -> bool {
    agent_token().is_some()
}

/// The agent binary a [`SessionHost`] given none starts, from `PATH`.
pub const DEFAULT_BIN: &str = "claude";

/// THE SESSION HOST: which binary the host is and how it is started, the one home every start
/// of it reads. A driver of either host kind holds one and starts each session from it; the
/// hold probe reads its login and starts its canary from it; `rigger run` builds the one its
/// driver holds before any start effect and reads that host's version first, so the version it
/// checks is the binary it launches; and `rigger validate` reads the login and the version of
/// the same default host. A host given no binary is [`DEFAULT_BIN`] on `PATH`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SessionHost {
    /// The binary as given; empty is [`DEFAULT_BIN`] on `PATH`.
    bin: String,
}

impl SessionHost {
    /// The host the binary `bin` is; empty is [`DEFAULT_BIN`] on `PATH`.
    pub fn at(bin: impl Into<String>) -> Self {
        SessionHost { bin: bin.into() }
    }

    /// The binary this host starts.
    pub fn bin(&self) -> &str {
        crate::driver::bin_or_path_default(&self.bin, DEFAULT_BIN)
    }

    /// The one start of this host: a command for its binary that runs in
    /// [`crate::subprocess::session_dir`] of `dir`, on the environment a spawn gets
    /// ([`apply_agent_credential`]).
    pub(crate) fn command_in(&self, dir: &str) -> std::process::Command {
        let mut cmd = crate::subprocess::command(self.bin());
        started_in(&mut cmd, dir);
        cmd
    }

    /// The start of a session of this host: [`Self::command_in`]'s start, as a command whose
    /// process starts a tree contained as `how`. An error where this OS holds no tree as `how`.
    pub(crate) fn tree_in(
        &self,
        dir: &str,
        how: &crate::child_tree::Containment,
    ) -> std::io::Result<crate::child_tree::TreeCommand> {
        let mut tree = crate::child_tree::TreeCommand::new(self.bin(), how)?;
        started_in(tree.command_mut(), dir);
        Ok(tree)
    }
}

/// What every start of a host sets on its command: the directory, [`crate::subprocess::session_dir`]
/// of `dir`, and the environment a spawn gets ([`apply_agent_credential`]).
fn started_in(cmd: &mut std::process::Command, dir: &str) {
    cmd.current_dir(crate::subprocess::session_dir(dir));
    apply_agent_credential(cmd);
}

impl From<String> for SessionHost {
    fn from(bin: String) -> Self {
        SessionHost::at(bin)
    }
}

impl From<&str> for SessionHost {
    fn from(bin: &str) -> Self {
        SessionHost::at(bin)
    }
}

/// The one key of the report the login read decides on.
#[derive(Deserialize)]
struct AuthStatus {
    #[serde(rename = "loggedIn")]
    logged_in: bool,
}

/// The one key of the report the auth-method read decides on.
#[derive(Deserialize)]
struct AuthMethod {
    #[serde(rename = "authMethod")]
    auth_method: String,
}

/// Whether `host`'s session is logged in, by its `auth status --json` report as it stands by
/// `deadline` ([`report`]).
pub fn logged_in(host: &SessionHost, deadline: Instant) -> bool {
    parse_logged_in(&report(host, deadline))
}

/// The auth method `host`'s `auth status --json` report names by `deadline` ([`report`]): none
/// when the report is not one JSON object naming an `authMethod` string.
pub fn auth_method(host: &SessionHost, deadline: Instant) -> Option<String> {
    serde_json::from_str::<AuthMethod>(&report(host, deadline))
        .ok()
        .map(|status| status.auth_method)
}

/// The lines `host`'s `auth status --json` printed by `deadline`, joined ([`printed`]): a binary
/// that does not start reports nothing at once.
fn report(host: &SessionHost, deadline: Instant) -> String {
    printed(host, &["auth", "status", "--json"], deadline).unwrap_or_default()
}

/// The one bounded read of what the session host prints for a read that carries no task: the
/// lines `host` given `args` printed by `deadline`, joined, started as every session of it is
/// ([`SessionHost::command_in`]). The child reads no input and its stderr and exit status go
/// unread; a child still running at `deadline` is reaped ([`ReapedChild`]). The error is the
/// one the binary did not start on.
pub(crate) fn printed(
    host: &SessionHost,
    args: &[&str],
    deadline: Instant,
) -> std::io::Result<String> {
    let mut cmd = host.command_in("");
    let child = cmd
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()?;
    let mut lines = Vec::new();
    ReapedChild::new(child).read_lines_until(deadline, |line| {
        lines.push(line.to_string());
        ControlFlow::Continue(())
    });
    Ok(lines.join("\n"))
}

/// Whether `report` is one JSON object whose `loggedIn` is `true`.
fn parse_logged_in(report: &str) -> bool {
    serde_json::from_str::<AuthStatus>(report).is_ok_and(|status| status.logged_in)
}

/// The two token variables' ambient values for the length of one test, restored on drop. Tests
/// that hold one run under the `claude_code_ambient_env` serial key, since the process
/// environment is global.
#[cfg(test)]
pub(crate) struct TokenEnv(Vec<(&'static str, Option<std::ffi::OsString>)>);

#[cfg(test)]
impl TokenEnv {
    /// Set (`Some`) or remove (`None`) the long-lived token variable and the rigger-named one.
    pub(crate) fn set(agent: Option<&str>, rigger: Option<&str>) -> Self {
        let vars = [
            ("CLAUDE_CODE_OAUTH_TOKEN", agent),
            ("RIGGER_CLAUDE_CODE_OAUTH_TOKEN", rigger),
        ];
        let saved = vars
            .iter()
            .map(|(k, _)| (*k, std::env::var_os(k)))
            .collect();
        for (k, v) in vars {
            match v {
                Some(v) => std::env::set_var(k, v),
                None => std::env::remove_var(k),
            }
        }
        Self(saved)
    }
}

#[cfg(test)]
impl Drop for TokenEnv {
    fn drop(&mut self) {
        for (k, v) in self.0.drain(..) {
            match v {
                Some(v) => std::env::set_var(k, v),
                None => std::env::remove_var(k),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{
        assert_no_sleeper_left, printing, probe_arms, recorded_argv, sleeping, LOGGED_IN_STATUS,
        LOGGED_OUT_STATUS, PASSING_CANARY,
    };
    use std::time::{Duration, Instant};

    #[test]
    fn a_report_is_logged_in_only_on_a_true_logged_in_key() {
        let rows = [
            (
                "the installed claude's logged-in report",
                LOGGED_IN_STATUS,
                true,
            ),
            ("a logged-out report", LOGGED_OUT_STATUS, false),
            (
                "a report with no loggedIn key",
                r#"{"authMethod":"claude.ai"}"#,
                false,
            ),
            ("loggedIn as a string", r#"{"loggedIn":"true"}"#, false),
            ("loggedIn null", r#"{"loggedIn":null}"#, false),
            (
                "text that is no JSON",
                "Logged in as operator@example.com",
                false,
            ),
            ("no output", "", false),
            (
                "two reports",
                "{\"loggedIn\":true}\n{\"loggedIn\":true}",
                false,
            ),
            ("an array of reports", r#"[{"loggedIn":true}]"#, false),
        ];
        for (name, report, expected) in rows {
            assert_eq!(parse_logged_in(report), expected, "{name}");
        }
    }

    /// The login read through a stand-in `claude`: it runs `claude auth status --json` and
    /// answers the report, and a report that has not come by the deadline answers not logged in
    /// at the deadline, leaving no process of the stand-in behind, as does a binary that does
    /// not start.
    #[cfg(unix)]
    #[test]
    fn the_login_read_answers_the_report_by_its_deadline() {
        let quick = (Duration::ZERO, Duration::from_secs(3));
        let at_the_bound = |bound| (bound, bound + Duration::from_secs(2));
        let short = Duration::from_millis(300);
        let long = Duration::from_secs(5);
        // (name, the report's shell, the bound, logged in, elapsed from and below, the sleeps the
        // stand-in starts)
        let rows = [
            (
                "logged in",
                printing(LOGGED_IN_STATUS),
                long,
                true,
                quick,
                0,
            ),
            (
                "logged out",
                printing(LOGGED_OUT_STATUS),
                long,
                false,
                quick,
                0,
            ),
            (
                "no report by the deadline",
                sleeping(5),
                short,
                false,
                at_the_bound(short),
                1,
            ),
        ];
        for (name, report, bound, expected, (from, below), sleepers) in rows {
            let fake = tempfile::tempdir().unwrap();
            let bin =
                crate::test_support::arms_claude(fake.path(), &probe_arms(&report, PASSING_CANARY));
            let started = Instant::now();
            let answer = logged_in(&SessionHost::at(bin), started + bound);
            let elapsed = started.elapsed();
            assert_eq!(answer, expected, "{name}");
            assert!(
                from <= elapsed && elapsed < below,
                "{name}: answered after {elapsed:?}, outside {from:?}..{below:?}"
            );
            assert_eq!(
                recorded_argv(&fake.path().join("auth-argv")),
                ["auth", "status", "--json"],
                "{name}: the read's argv"
            );
            assert!(
                !fake.path().join("canary-argv").exists(),
                "{name}: the read runs no session"
            );
            assert_no_sleeper_left(fake.path(), sleepers, name);
        }
        let absent = tempfile::tempdir().unwrap();
        let bin = absent.path().join("claude");
        let started = Instant::now();
        assert!(
            !logged_in(&SessionHost::at(bin.to_str().unwrap()), started + long),
            "a binary that does not start answers not logged in"
        );
        assert!(started.elapsed() < quick.1, "and at once");
    }

    /// The login read runs `claude` on the environment a spawn gets: with the rigger-named token
    /// set it sees the long-lived token variable, so the method it reports is the spawns' own.
    #[cfg(unix)]
    #[test]
    #[serial_test::serial(claude_code_ambient_env)]
    fn the_login_read_runs_on_the_spawns_token_environment() {
        let _env = TokenEnv::set(Some("ambient"), Some("rigger-named"));
        let fake = tempfile::tempdir().unwrap();
        let auth = "    printf '{\"authMethod\": \"%s\"}\n' \"$CLAUDE_CODE_OAUTH_TOKEN\" >&3\n";
        let bin = crate::test_support::arms_claude(fake.path(), &probe_arms(auth, PASSING_CANARY));
        assert_eq!(
            auth_method(
                &SessionHost::at(bin),
                Instant::now() + Duration::from_secs(5)
            )
            .as_deref(),
            Some("rigger-named")
        );
    }

    /// THE SESSION HOST IS ONE HOME: a host given no binary is `claude` on `PATH`, a host given
    /// one names it, and its version read, its login read, a session's start and the too-old line
    /// each run or name that one binary, so a run that checks its host's version checks the
    /// binary its driver starts.
    #[cfg(unix)]
    #[test]
    fn a_session_host_is_the_one_binary_its_reads_and_starts_run() {
        for host in [SessionHost::default(), SessionHost::at("")] {
            assert_eq!(host.bin(), DEFAULT_BIN, "a host given no binary");
        }
        let fake = tempfile::tempdir().unwrap();
        let bin = crate::test_support::arms_claude(
            fake.path(),
            &probe_arms(&printing(LOGGED_IN_STATUS), PASSING_CANARY),
        );
        let host = SessionHost::at(bin.clone());
        assert_eq!(host.bin(), bin, "a host given a binary");
        assert_eq!(
            crate::claude_version::read(&host, crate::claude_version::READ_BOUND)
                .map(|version| version.to_string()),
            Ok("2.1.139".to_string()),
            "the version read runs the host's binary"
        );
        assert!(
            logged_in(&host, Instant::now() + Duration::from_secs(5)),
            "the login read runs the host's binary"
        );
        assert_eq!(
            recorded_argv(&fake.path().join("auth-argv")),
            ["auth", "status", "--json"]
        );
        assert_eq!(
            host.command_in("").get_program(),
            std::ffi::OsStr::new(&bin),
            "a session starts the host's binary"
        );
        let below = crate::claude_version::parse("2.1.138").unwrap();
        let line = crate::claude_version::too_old(&host, below).unwrap();
        assert!(
            line.contains(&format!("`{bin}`")),
            "the too-old line names the host's binary: {line}"
        );
    }
}
