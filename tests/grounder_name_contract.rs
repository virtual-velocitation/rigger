//! PERIPHERY (contract / API) tests for the grounder NAME-RESOLUTION contract
//! (spec 57, criterion 1), driven as an EXTERNAL consumer of the `rigger` crate.
//!
//! The in-module `#[cfg(test)]` tests in `src/grounder/mod.rs` exercise the same three
//! items, but from INSIDE the crate - they would pass even if the items were
//! `pub(crate)`. This file lives in the integration-test crate, so it reaches
//! `grounder_for`, `is_retired_grounder`, and `retired_grounder_error` ONLY through the
//! crate's PUBLIC surface: compiling it at all is the proof those three items are
//! genuinely reachable by an external caller, which is the API guarantee c1 adds.
//!
//! It also pins the ONE consistency the unit tests never state as a single fact: the
//! predicate, the error constructor, and the resolver agree. `is_retired_grounder(n)`
//! is true for EXACTLY the names `grounder_for` rejects with `retired_grounder_error(n)`
//! - so the three public items can never drift apart under a later edit.
//!
//! `grounder_for` is feature-INDEPENDENT (its `symbols` arm is not cfg-gated;
//! `main::select_grounder` is the feature-gated interceptor, not this function), so every
//! assertion below holds identically in both feature lanes and the file is ungated.

use rigger::grounder::{grounder_for, is_retired_grounder, retired_grounder_error};

/// What the name contract says each resolver input must do.
#[derive(Clone, Copy)]
enum Contract {
    /// A retired engine (spec 57): rejected with the migration error.
    Retired,
    /// An explicit opt-in that constructs a grounder here (`grep` / `nop`).
    AcceptedOk,
    /// The structural default - the unset/empty name AND the explicit `symbols` name.
    SymbolsDefault,
    /// A typo / removed-but-not-retired name: the generic unknown-grounder error.
    Unknown,
}

/// The public name contract, driven as an external caller: for EVERY resolver input, the
/// predicate `is_retired_grounder`, the constructor `retired_grounder_error`, and the
/// resolver `grounder_for` agree, and each category behaves as spec 57 dictates. One table
/// so a new name is accounted for in exactly one place.
#[test]
fn public_name_contract_predicate_error_and_resolver_agree() {
    use Contract::*;
    let table: &[(&str, Contract)] = &[
        // Retired engines: the vector engine, its alias, and the symbols-composite -
        // case-insensitive and whitespace-trimmed like every other resolver name.
        ("turbovec", Retired),
        ("vector", Retired),
        ("hybrid", Retired),
        ("TurboVec", Retired),
        ("  Hybrid ", Retired),
        ("VECTOR", Retired),
        // Explicit opt-ins - the only names that construct a grounder in this resolver.
        ("grep", AcceptedOk),
        ("nop", AcceptedOk),
        ("GREP", AcceptedOk),
        (" Nop ", AcceptedOk),
        // The structural default: the unset/empty name AND the explicit `symbols` name.
        ("", SymbolsDefault),
        ("   ", SymbolsDefault),
        ("symbols", SymbolsDefault),
        ("SYMBOLS", SymbolsDefault),
        (" Symbols ", SymbolsDefault),
        // Typos / removed-but-not-retired names.
        ("bogus", Unknown),
        ("sem", Unknown),
        ("embed", Unknown),
        ("bogus-grounder", Unknown),
    ];

    for &(name, contract) in table {
        let expected_retired = retired_grounder_error(name);
        let got = grounder_for(name, ".");
        // `Box<dyn Grounder>` is not `Debug`, so never `{:?}` the Ok arm.
        let got_desc = match &got {
            Ok(_) => "Ok(<grounder>)".to_string(),
            Err(e) => format!("Err({e:?})"),
        };
        let got_retired_err =
            got.as_ref().err().map(String::as_str) == Some(expected_retired.as_str());
        let is_retired = is_retired_grounder(name);

        // THE binding: the predicate is true for EXACTLY the names the resolver rejects with
        // the migration error - so the three public items can never drift apart.
        assert_eq!(
            is_retired, got_retired_err,
            "is_retired_grounder({name:?})={is_retired} must match whether grounder_for returns \
             the migration error retired_grounder_error({name:?}); grounder_for gave {got_desc}"
        );

        match contract {
            Retired => {
                assert!(is_retired, "{name:?} is a retired engine name");
                let err = got
                    .err()
                    .expect("a retired name is rejected, never a grounder");
                let low = err.to_lowercase();
                assert!(
                    low.contains("retire") && low.contains("symbols"),
                    "the migration error must name the retirement and the symbols default; got: {err}"
                );
                assert!(
                    !low.contains("feature"),
                    "a retired name is gone for good - not a missing feature to rebuild; got: {err}"
                );
                assert!(
                    !err.contains("unknown grounder"),
                    "a retired name must not read as a typo; got: {err}"
                );
            }
            AcceptedOk => {
                assert!(
                    !is_retired,
                    "{name:?} is an explicit opt-in, not a retired name"
                );
                assert!(
                    got.is_ok(),
                    "{name:?} must construct a grounder; got error: {:?}",
                    got.as_ref().err()
                );
            }
            SymbolsDefault => {
                assert!(
                    !is_retired,
                    "the symbols default is not a retired name: {name:?}"
                );
                // Feature-independent here: `grounder_for`'s `symbols` arm is not cfg-gated, so
                // the unset default and the explicit `symbols` name land on the SAME loud symbols
                // branch in BOTH lanes (`select_grounder` is the feature-gated interceptor).
                let err = got.err().unwrap_or_else(|| {
                    panic!(
                        "{name:?} is the symbols default - a loud error in the \
                         feature-independent resolver, never a silent grep"
                    )
                });
                let low = err.to_lowercase();
                assert!(
                    low.contains("symbols"),
                    "the unset / symbols default must name the symbols grounder; got: {err}"
                );
                assert!(
                    !low.contains("turbovec"),
                    "the default is symbols, not the retired turbovec engine; got: {err}"
                );
                assert!(
                    !err.contains("unknown grounder"),
                    "the default is a known name, not a typo; got: {err}"
                );
            }
            Unknown => {
                assert!(
                    !is_retired,
                    "{name:?} is an unknown name, not a retired one"
                );
                let err = got
                    .err()
                    .expect("an unknown name is a hard error, never a silent grep");
                assert!(
                    err.contains("unknown grounder"),
                    "an unknown name must hit the generic unknown-grounder arm; got: {err}"
                );
                assert!(
                    err.contains("symbols") && err.contains("grep") && err.contains("nop"),
                    "the unknown-name message must advertise the exact accepted set; got: {err}"
                );
                assert!(
                    !err.to_lowercase().contains("turbovec"),
                    "the accepted set must not advertise a retired engine as a choice; got: {err}"
                );
            }
        }
    }
}

/// The public constructor `retired_grounder_error`, exercised directly as an external
/// caller: it echoes the given (retired) grounder name, names the retirement and the
/// surviving `symbols` default, points at the explicit `grep` / `nop` opt-ins, and never
/// carries the old "rebuild the feature" wording or the generic unknown-grounder message.
#[test]
fn retired_grounder_error_is_a_migration_message_that_echoes_the_name() {
    let err = retired_grounder_error("turbovec");
    assert!(
        err.contains("turbovec"),
        "it echoes the configured name; got: {err}"
    );
    let low = err.to_lowercase();
    assert!(
        low.contains("retire"),
        "it names the retirement; got: {err}"
    );
    assert!(
        low.contains("symbols"),
        "it points at the symbols default; got: {err}"
    );
    assert!(
        low.contains("grep") && low.contains("nop"),
        "it names the explicit opt-ins; got: {err}"
    );
    assert!(
        !low.contains("feature"),
        "a retired name is not a missing feature to rebuild; got: {err}"
    );
    assert!(
        !err.contains("unknown grounder"),
        "it is a migration message, not a typo message; got: {err}"
    );
}
