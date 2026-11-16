//! The one read of the user's home directory (spec 125, THE HOME). Every default rigger roots
//! under the home starts from [`home_dir`]: the state home the instance registry lives under and
//! the cache home the scratch roots live under, each after its own XDG variable ([`xdg_or_home`]).

use std::ffi::OsString;
use std::path::PathBuf;

/// The variable the user's home is read from: `HOME`, which [`home_dir`] reads on unix, and on
/// Windows `USERPROFILE`, which [`std::env::home_dir`] reads before it asks the OS.
pub const HOME_ENV: &str = if cfg!(windows) { "USERPROFILE" } else { "HOME" };

/// The user's home directory, or `None` when no home directory is known. On unix it is the
/// `HOME` variable, an empty value counting as unset, with no password-database fallback;
/// elsewhere it is [`std::env::home_dir`].
pub fn home_dir() -> Option<OsString> {
    std::cfg_select! {
        unix => { non_empty(std::env::var_os(HOME_ENV)) }
        _ => { std::env::home_dir().map(std::path::PathBuf::into_os_string) }
    }
}

/// `value`, an environment variable's, as the home and XDG reads take it: `None` when it is unset
/// or empty, so an empty value counts as unset.
pub fn non_empty(value: Option<OsString>) -> Option<OsString> {
    value.filter(|v| !v.is_empty())
}

/// The directory an XDG base-directory variable roots: `xdg`, the variable's value, when it is
/// non-empty, else `home` joined with each of `default`'s components when `home` is non-empty,
/// else `None`. The one rule the cache home and the state home each follow with its own default.
pub fn xdg_or_home(
    xdg: Option<OsString>,
    home: Option<OsString>,
    default: &[&str],
) -> Option<PathBuf> {
    if let Some(dir) = non_empty(xdg) {
        return Some(PathBuf::from(dir));
    }
    Some(
        default
            .iter()
            .fold(PathBuf::from(non_empty(home)?), |dir, part| dir.join(part)),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// [`non_empty`] of `value` answers `expected`.
    fn reads(value: Option<&str>, expected: Option<&str>) {
        assert_eq!(
            non_empty(value.map(OsString::from)),
            expected.map(OsString::from)
        );
    }

    // GIVEN an environment variable's value, WHEN rigger reads it, THEN an unset or empty value is
    // unset and any other value, whitespace included, is itself.
    rigger_domain::test_cases! {
        an_unset_value_is_unset: reads(None, None);
        an_empty_value_is_unset: reads(Some(""), None);
        a_blank_value_is_itself: reads(Some(" "), Some(" "));
        a_path_is_itself: reads(Some("/home/operator"), Some("/home/operator"));
    }

    /// [`xdg_or_home`] of `xdg` and `home` under the default `.local/state` answers `expected`.
    fn roots(xdg: Option<&str>, home: Option<&str>, expected: Option<&str>) {
        assert_eq!(
            xdg_or_home(
                xdg.map(OsString::from),
                home.map(OsString::from),
                &[".local", "state"]
            ),
            expected.map(PathBuf::from)
        );
    }

    // GIVEN an XDG variable's value and a home, WHEN the directory it roots is read, THEN a
    // non-empty value wins, else the default under a non-empty home, else none.
    rigger_domain::test_cases! {
        a_set_xdg_value_wins_over_the_home: roots(Some("/xdg"), Some("/home/dev"), Some("/xdg"));
        an_empty_xdg_value_falls_to_the_home: roots(Some(""), Some("/home/dev"), Some("/home/dev/.local/state"));
        an_unset_xdg_value_falls_to_the_home: roots(None, Some("/home/dev"), Some("/home/dev/.local/state"));
        an_empty_home_is_no_home: roots(None, Some(""), None);
        no_xdg_value_and_no_home_is_none: roots(None, None, None);
    }

    /// On unix the home is the test process's own `HOME` when it is non-empty, and unknown when it
    /// is unset or empty.
    #[cfg(unix)]
    #[test]
    fn on_unix_the_home_is_the_processes_non_empty_home() {
        match std::env::var_os("HOME") {
            Some(home) if !home.is_empty() => assert_eq!(home_dir(), Some(home)),
            _ => assert_eq!(home_dir(), None),
        }
    }
}
