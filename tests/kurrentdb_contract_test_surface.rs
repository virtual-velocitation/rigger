//! Spec 47, criterion 3 - the KurrentDB contract test surface.
//!
//! Criterion 3 says: "the existing KurrentDB contract test compiles and runs (or
//! gracefully skips without a container runtime) in BOTH lanes." Retiring the
//! build-time `kurrentdb` cargo feature un-gated the adapter module - and with it the
//! backend-agnostic contract test `passes_the_contract` in
//! `crates/rigger-store-sqlite/src/eventstore/kurrentdb.rs`, which drives `eventstore::contract::assert_contract`
//! against a real KurrentDB container. That test now compiles into EVERY lane's
//! `cargo test`, and it must keep two properties, neither of which any other test pins:
//!
//!   1. UN-GATED IN EVERY LANE. The adapter file carries no `#[cfg(feature = ...)]`
//!      gate, so both the adapter AND its contract test compile and run in the default
//!      lane and the `--no-default-features` lane alike. The sibling
//!      `kurrentdb_always_available.rs` scan only catches the ONE retired feature name
//!      (`kurrentdb`); a regression that re-gated the contract test behind ANY other
//!      cargo feature (`#[cfg(feature = "integration")]`, say) would silently compile it
//!      out of a lane and slip past that scan. This guard forbids ANY cargo-feature gate
//!      in the adapter file, so the contract test cannot be conditionally compiled out of
//!      either lane again.
//!
//!   2. GRACEFULLY SKIPS WITH NO CONTAINER RUNTIME. The contract test needs a container
//!      runtime; a CI box without one (the common case) must stay GREEN. The test starts
//!      its container through the shared fixture `start_kurrentdb`
//!      (tests/common/fixtures/kurrentdb.rs), which on a failed start PRINTS a skip notice
//!      and returns `None` - it never force-unwraps the start or panics - and the test
//!      RETURNS on that `None`. This is the load-bearing promise of the
//!      criterion's parenthetical, and its regression is SILENT: a change from `return`
//!      to `panic!` on the start-failure arm stays green on every box that HAS a runtime
//!      and reds only on the boxes that lack one - exactly where no author would notice.
//!      This guard fails at `cargo test` time instead.
//!
//! Deliberately NOT feature-gated and it touches no backend symbol: it reads the adapter
//! source as text (resolved from `CARGO_MANIFEST_DIR`, so it is CWD-independent) and runs
//! identically in both feature lanes, a real member of each lane's `cargo test` battery.

mod common;

use common::repo::repo_text;

/// The body of the first `fn <name>` in `src`, from its opening `{` to the matching `}`
/// (brace-balanced, so nested blocks are included). Panics if the function or a balanced
/// body is absent - the guard is worthless if it silently matches nothing.
fn fn_body(src: &str, name: &str) -> String {
    let sig = format!("fn {name}");
    let at = src
        .find(&sig)
        .unwrap_or_else(|| panic!("`{sig}` not found in the adapter source"));
    let open = src[at..]
        .find('{')
        .map(|i| at + i)
        .unwrap_or_else(|| panic!("`{sig}` has no opening brace"));
    let mut depth = 0usize;
    for (idx, ch) in src[open..].char_indices() {
        match ch {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return src[open..=open + idx].to_string();
                }
            }
            _ => {}
        }
    }
    panic!("`{sig}` body has unbalanced braces");
}

/// CRITERION 3, part one - the backend-agnostic contract test is PRESENT and un-gated,
/// so it compiles and runs in BOTH lanes. `passes_the_contract` is the test that runs
/// `eventstore::contract::assert_contract` against a real KurrentDB; deleting it would
/// silently drop the store's only real-backend contract coverage. And the adapter file
/// must carry NO `#[cfg(feature = ...)]` gate: a cargo-feature predicate would compile
/// the adapter (and the contract test with it) OUT of whichever lane lacks that feature,
/// breaking "runs in BOTH lanes". Space-insensitive so `feature="x"` is caught too.
#[test]
fn the_contract_test_is_present_and_never_gated_on_a_cargo_feature() {
    let src = repo_text("crates/rigger-store-sqlite/src/eventstore/kurrentdb.rs");

    assert!(
        src.contains("fn passes_the_contract"),
        "crates/rigger-store-sqlite/src/eventstore/kurrentdb.rs must still define the backend-agnostic contract test \
         `passes_the_contract` (spec 47 criterion 3: the existing contract test compiles and runs \
         in both lanes) - it is gone"
    );

    let squeezed: String = src.chars().filter(|c| !c.is_whitespace()).collect();
    assert!(
        !squeezed.contains("cfg(feature="),
        "the KurrentDB adapter (crates/rigger-store-sqlite/src/eventstore/kurrentdb.rs) must carry NO `#[cfg(feature = ...)]` \
         gate: the build-time feature is retired (spec 47), so a cargo-feature predicate here would \
         compile the adapter and its `passes_the_contract` contract test OUT of the lane that lacks \
         the feature, breaking 'compiles and runs in BOTH lanes'"
    );
}

/// CRITERION 3, part two - the contract test GRACEFULLY SKIPS with no container runtime.
/// Every KurrentDB-backed test boots its server through the one shared fixture
/// `start_kurrentdb` (tests/common/fixtures/kurrentdb.rs): its failed start prints a skip
/// notice and returns `None`, never force-unwrapping the start or panicking, and
/// `passes_the_contract` returns on that `None`. A regression in either is SILENT (green
/// wherever a runtime exists, red only where it does not - the CI boxes without one), so it is
/// pinned here at `cargo test` time rather than left to surface as a lane failure on a
/// runtime-less box.
#[test]
fn the_contract_test_gracefully_skips_without_a_container_runtime() {
    let fixture = fn_body(
        &repo_text("tests/common/fixtures/kurrentdb.rs"),
        "start_kurrentdb",
    );
    let start_at = fixture.find("image.start()").unwrap_or_else(|| {
        panic!(
            "`start_kurrentdb` must attempt to start a container (`image.start()`) - it is the \
             step that can be absent, and the skip is its failure path; found neither"
        )
    });

    // The start result must be handled, never force-unwrapped: `.expect(` / `.unwrap(`
    // applied to the start would PANIC a runtime-less box instead of skipping it.
    let squeezed_fixture: String = fixture.chars().filter(|c| !c.is_whitespace()).collect();
    for forced in ["image.start()).unwrap", "image.start()).expect"] {
        assert!(
            !squeezed_fixture.contains(forced),
            "`start_kurrentdb` must not force-unwrap the container start ({forced}...): a box \
             with no container runtime must SKIP (spec 47 criterion 3), not panic"
        );
    }

    // The start-failure arm skips: within the start's `match`, from the call to the `};` that
    // closes it, an `Err` arm prints a notice and returns `None`, and nothing panics.
    let rest = &fixture[start_at..];
    let window = &rest[..rest.find("};").unwrap_or(rest.len())];
    assert!(
        window.contains("Err") && window.contains("return None"),
        "the container-start failure must be a graceful skip - an `Err` arm that returns `None` \
         (spec 47 criterion 3: 'gracefully skips without a container runtime'); the start \
         handling is:\n{window}"
    );
    for panicky in ["panic!", ".unwrap(", ".expect("] {
        assert!(
            !window.contains(panicky),
            "the container-start failure arm must SKIP, not `{panicky}` - a runtime-less CI box must \
             stay green (spec 47 criterion 3); the start handling is:\n{window}"
        );
    }

    // The contract test boots through that fixture and returns on its `None`.
    let contract: String = fn_body(
        &repo_text("crates/rigger-store-sqlite/src/eventstore/kurrentdb.rs"),
        "passes_the_contract",
    )
    .chars()
    .filter(|c| !c.is_whitespace())
    .collect();
    assert!(
        contract.contains("start_kurrentdb(&rt)else{return;"),
        "`passes_the_contract` must boot its server through the shared `start_kurrentdb` fixture \
         and return when it yields no container (spec 47 criterion 3); its body is:\n{contract}"
    );
}
