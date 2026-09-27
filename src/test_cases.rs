//! THE PARAMETERISED-TEST HOME: one `#[test]` per named case, each running its own call into a
//! shared case helper. A family of tests that differ only by the values they feed one check is
//! a single definition (the helper) plus a table of cases - never one hand-copied test body per
//! value. Exported (hidden) so the integration suites use the same definition as the unit
//! tests: `rigger::test_cases!` there, `crate::test_cases!` here.

/// Expands `name: expr;` pairs into one `#[test] fn name() { expr; }` each, carrying any
/// attributes (doc comments included) written above a case onto its generated test.
#[doc(hidden)]
#[macro_export]
macro_rules! test_cases {
    ($($(#[$meta:meta])* $name:ident: $case:expr;)+) => {
        $(
            $(#[$meta])*
            #[test]
            fn $name() {
                $case;
            }
        )+
    };
}
