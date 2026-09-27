//! The table-driven test declaration every suite that declares `mod common;` shares.

/// Declares one `#[test]` per case of a table-driven test family: each case names its test
/// (plus any attributes, e.g. its doc or `#[should_panic]`) and gives the expression that runs
/// it - normally one call into the family's shared case body - so a family of same-shaped
/// tests has ONE definition of its body instead of a copy per test. Exported at the suite's
/// crate root, so a suite invokes it as `test_cases!` (or `crate::test_cases!` from a nested
/// module).
#[macro_export]
macro_rules! test_cases {
    ($($(#[$attr:meta])* $name:ident => $body:expr;)+) => {
        $(
            $(#[$attr])*
            #[test]
            fn $name() {
                $body;
            }
        )+
    };
}
