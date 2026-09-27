//! [`test_cases!`](crate::test_cases): the table-driven shape for a family of tests that
//! differ only by their data. The check is written ONCE, as an ordinary function; each
//! case is one row naming its own test and the arguments it passes, so every case keeps its
//! own test name, pass/fail line, doc comment and attributes (`#[should_panic]`,
//! `#[cfg(unix)]`, ...) while no two cases repeat the check's body.

/// Expands `check; name: (args...); ...` into one `#[test] fn name() { check(args...); }`
/// per row. Attributes written before a row's name - its doc comment included - are carried
/// onto that row's test. Usable from the crate's own test modules (`crate::test_cases!`),
/// the binary's (`rigger::test_cases!`) and every integration-test crate.
#[macro_export]
#[doc(hidden)]
macro_rules! test_cases {
    ($check:expr; $($(#[$meta:meta])* $name:ident: ($($arg:expr),* $(,)?);)+) => {
        $(
            $(#[$meta])*
            #[test]
            fn $name() {
                $check($($arg),*);
            }
        )+
    };
}
