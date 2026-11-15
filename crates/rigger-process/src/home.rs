//! The one read of the user's home directory (spec 125, THE HOME). Every default rigger roots
//! under the home starts from [`home_dir`]: the state home the instance registry lives under and
//! the cache home the scratch roots live under, each after its own XDG variable.

use std::ffi::OsString;

/// The user's home directory, or `None` when no home directory is known. On unix it is the
/// `HOME` variable, an empty value counting as unset, with no password-database fallback;
/// elsewhere it is [`std::env::home_dir`].
pub fn home_dir() -> Option<OsString> {
    None
}

/// `value`, an environment variable's, as rigger reads it: `None` when it is unset or empty, so an
/// empty value counts as unset.
pub fn non_empty(value: Option<OsString>) -> Option<OsString> {
    let _ = value;
    None
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
