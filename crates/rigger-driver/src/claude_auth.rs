//! THE LOGIN READ (spec 105): the one reader of `claude auth status --json`, which the hold probe
//! runs ahead of its canary on a hold whose cause a login fixes (RELEASE) and `rigger validate`
//! runs to report the auth method (WHAT A PERSON SEES). The JSON alone answers, each read on its
//! own key: the session is logged in only when the report's `loggedIn` is `true`, and its auth
//! method is the report's `authMethod` string; a report that does not come by the deadline, does
//! not parse or names no such key answers not logged in, and no auth method.

use std::ops::ControlFlow;
use std::process::Stdio;
use std::time::Instant;

use serde::Deserialize;

use crate::reaped_child::ReapedChild;

/// The long-lived agent token's variable, the one the agent CLI reads.
const AGENT_TOKEN_VAR: &str = "CLAUDE_CODE_OAUTH_TOKEN";

/// The variable an operator sets in rigger's environment to give the long-lived token to spawned
/// agents only: the agent CLI ignores it, so the operator's own sessions keep their login.
const RIGGER_AGENT_TOKEN_VAR: &str = "RIGGER_CLAUDE_CODE_OAUTH_TOKEN";

/// The ONE credential rule every launch of the agent binary applies (unit spawns of both hosts,
/// the hold probe's canary and the login reads `claude auth status` that the probe and `rigger
/// validate` make): a non-empty [`RIGGER_AGENT_TOKEN_VAR`] becomes [`AGENT_TOKEN_VAR`] on `cmd`
/// alone, and is not passed on itself. Unset or empty, the inherited environment rides through
/// untouched, an ambient [`AGENT_TOKEN_VAR`] included. The value is set on the command and held
/// nowhere else: no field, no record, no message.
pub(crate) fn apply_agent_credential(cmd: &mut std::process::Command) {
    if let Some(token) = std::env::var_os(RIGGER_AGENT_TOKEN_VAR).filter(|v| !v.is_empty()) {
        cmd.env(AGENT_TOKEN_VAR, token);
        cmd.env_remove(RIGGER_AGENT_TOKEN_VAR);
    }
}

/// The agent binary a driver launches from `PATH` when it is given none, and so the one whose
/// login `rigger validate` reads.
pub const DEFAULT_BIN: &str = "claude";

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

/// Whether `bin`'s session is logged in, by its `auth status --json` report as it stands by
/// `deadline` ([`report`]).
pub fn logged_in(bin: &str, deadline: Instant) -> bool {
    parse_logged_in(&report(bin, deadline))
}

/// The auth method `bin`'s `auth status --json` report names by `deadline` ([`report`]): none
/// when the report is not one JSON object naming an `authMethod` string.
pub fn auth_method(bin: &str, deadline: Instant) -> Option<String> {
    serde_json::from_str::<AuthMethod>(&report(bin, deadline))
        .ok()
        .map(|status| status.auth_method)
}

/// The lines `bin auth status --json` printed by `deadline`, joined: the child reads no input
/// and its stderr and exit status go unread, and a binary that does not start reports nothing at
/// once.
fn report(bin: &str, deadline: Instant) -> String {
    let mut cmd = crate::subprocess::command(bin);
    apply_agent_credential(&mut cmd);
    let Ok(child) = cmd
        .args(["auth", "status", "--json"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    else {
        return String::new();
    };
    let mut report = Vec::new();
    ReapedChild::new(child).read_lines_until(deadline, |line| {
        report.push(line.to_string());
        ControlFlow::Continue(())
    });
    report.join("\n")
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
            let answer = logged_in(&bin, started + bound);
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
            !logged_in(bin.to_str().unwrap(), started + long),
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
            auth_method(&bin, Instant::now() + Duration::from_secs(5)).as_deref(),
            Some("rigger-named")
        );
    }
}
