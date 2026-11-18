//! The one reader of the session host's version, `claude --version`: the version it prints, and
//! whether that version is below the one the installed exec-form hooks need
//! ([`crate::hooks::MIN_HOST_VERSION`]). `rigger validate` reports it and `rigger run` refuses to
//! launch on a host below it, both with the one line [`too_old`] words.

use std::fmt;
use std::time::{Duration, Instant};

use crate::claude_auth::{printed, SessionHost};
use crate::hooks::MIN_HOST_VERSION;

/// A session host version, `major.minor.patch`, ordered by each part as a number.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct HostVersion {
    /// The first part.
    pub major: u32,
    /// The second part.
    pub minor: u32,
    /// The third part.
    pub patch: u32,
}

impl fmt::Display for HostVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

/// How long a version read waits on `<host> --version`.
pub const READ_BOUND: Duration = Duration::from_secs(5);

/// The version `host`'s binary prints for `--version` within `bound` ([`printed`], [`parse`]),
/// or the error naming why none was read: the binary did not start, printed nothing by the
/// bound, or printed a first line that opens with no version.
pub fn read(host: &SessionHost, bound: Duration) -> Result<HostVersion, String> {
    let read = format!("`{} --version`", host.bin());
    let text = printed(host, &["--version"], Instant::now() + bound)
        .map_err(|e| format!("{read} did not start: {e}"))?;
    let first = text.lines().next().unwrap_or_default().trim();
    if first.is_empty() {
        return Err(format!(
            "{read} printed nothing within {} s",
            bound.as_secs()
        ));
    }
    parse(&text).ok_or_else(|| format!("{read} printed no version: {first}"))
}

/// The version `printed` opens with: its first line's leading word as three parts, each a
/// number, as the session host prints it (`2.1.139 (Claude Code)`). Anything else is none, so
/// text from some other binary is never read as a version.
pub fn parse(printed: &str) -> Option<HostVersion> {
    let word = printed.lines().next()?.split_whitespace().next()?;
    let mut parts = word.split('.').map(|part| part.parse::<u32>().ok());
    let version = HostVersion {
        major: parts.next()??,
        minor: parts.next()??,
        patch: parts.next()??,
    };
    parts.next().is_none().then_some(version)
}

/// The one line `rigger validate` warns with and `rigger run` refuses with when `host`'s
/// `version` is below [`MIN_HOST_VERSION`], naming the host's binary; none when it is not.
pub fn too_old(host: &SessionHost, version: HostVersion) -> Option<String> {
    (version < MIN_HOST_VERSION).then(|| {
        format!(
            "the session host `{}` is {version}, below {MIN_HOST_VERSION}, the version that \
             added the hook `args` field rigger's exec-form hooks are written with; update it \
             to {MIN_HOST_VERSION} or later",
            host.bin()
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::probe_arms;
    use std::time::Instant;

    fn v(major: u32, minor: u32, patch: u32) -> HostVersion {
        HostVersion {
            major,
            minor,
            patch,
        }
    }

    /// A version is the first line's leading word as three numbers, compared part by part as
    /// numbers, and is too old below 2.1.139; anything else reads no version, so it is never too
    /// old.
    #[test]
    fn a_version_is_the_leading_word_and_too_old_below_the_hooks_minimum() {
        // (what `--version` printed, the version read, whether it is too old)
        let rows = [
            ("2.1.99", Some(v(2, 1, 99)), true),
            ("2.1.138", Some(v(2, 1, 138)), true),
            ("2.1.139", Some(v(2, 1, 139)), false),
            ("2.1.322", Some(v(2, 1, 322)), false),
            ("garbage", None, false),
            ("2.1.139 (Claude Code)", Some(v(2, 1, 139)), false),
            ("2.1.138 (Claude Code)\n", Some(v(2, 1, 138)), true),
            // What the `false` every test PATH holds as `claude` prints for `--version`.
            ("false (GNU coreutils) 9.4", None, false),
            ("", None, false),
            ("2.1", None, false),
            ("2.1.139.5", None, false),
            ("v2.1.139", None, false),
            ("Claude Code 2.1.139", None, false),
        ];
        for (printed, version, below) in rows {
            assert_eq!(parse(printed), version, "{printed:?}");
            assert_eq!(
                version
                    .and_then(|version| too_old(&SessionHost::default(), version))
                    .is_some(),
                below,
                "{printed:?}: too old"
            );
        }
    }

    /// The too-old line names the host's binary, its version and the minimum, and says of the
    /// minimum only what the host's changelog establishes: it is the version that added the hook
    /// `args` field rigger's exec-form hooks are written with.
    #[test]
    fn the_too_old_line_names_the_host_its_version_and_the_minimum_that_added_the_args_field() {
        assert_eq!(
            too_old(&SessionHost::default(), v(2, 1, 138)).as_deref(),
            Some(
                "the session host `claude` is 2.1.138, below 2.1.139, the version that added the \
                 hook `args` field rigger's exec-form hooks are written with; update it to 2.1.139 \
                 or later"
            )
        );
    }

    /// The read runs `bin --version` on no input: the checked-in stand-in answers it at once, on
    /// none of the call logs its other reads write, and a binary that does not start is an error
    /// naming it.
    #[cfg(unix)]
    #[test]
    fn the_read_runs_the_version_flag_and_names_a_binary_that_does_not_start() {
        let fake = tempfile::tempdir().unwrap();
        let bin = crate::test_support::arms_claude(fake.path(), &probe_arms("", ""));
        let started = Instant::now();
        assert_eq!(read(&SessionHost::at(bin), READ_BOUND), Ok(v(2, 1, 139)));
        assert!(started.elapsed() < Duration::from_secs(3), "at once");
        for log in ["auth-argv", "canary-argv", "canary-cwd", "canary-first"] {
            assert!(!fake.path().join(log).exists(), "the read writes no {log}");
        }
        let absent = tempfile::tempdir().unwrap();
        let bin = absent.path().join("claude");
        let bin = bin.to_str().unwrap();
        let err = read(&SessionHost::at(bin), READ_BOUND).unwrap_err();
        assert!(
            err.starts_with(&format!("`{bin} --version` did not start: ")),
            "{err}"
        );
    }
}
