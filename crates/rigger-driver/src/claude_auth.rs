//! THE LOGIN READ (spec 105 RELEASE): the one reader of `claude auth status --json`, which the
//! hold probe runs ahead of its canary on a hold whose cause a login fixes, and which
//! `rigger validate` reports from. The JSON alone answers: the session is logged in only when the
//! report's `loggedIn` is `true`, and a report that does not come by the deadline, does not parse
//! or names no such key answers not logged in.

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{
        printing, probe_arms, recorded_argv, HANGS, LOGGED_IN_STATUS, LOGGED_OUT_STATUS,
        PASSING_CANARY,
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
    /// at the deadline, as does a binary that does not start.
    #[cfg(unix)]
    #[test]
    fn the_login_read_answers_the_report_by_its_deadline() {
        let quick = (Duration::ZERO, Duration::from_secs(3));
        let at_the_bound = |bound| (bound, bound + Duration::from_secs(2));
        let short = Duration::from_millis(300);
        let long = Duration::from_secs(5);
        // (name, the report's shell, the bound, logged in, elapsed from and below)
        let rows = [
            ("logged in", printing(LOGGED_IN_STATUS), long, true, quick),
            (
                "logged out",
                printing(LOGGED_OUT_STATUS),
                long,
                false,
                quick,
            ),
            (
                "no report by the deadline",
                HANGS.to_string(),
                short,
                false,
                at_the_bound(short),
            ),
        ];
        for (name, report, bound, expected, (from, below)) in rows {
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
}
