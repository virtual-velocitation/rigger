//! Shared support for the courier registry-refresh periphery suites
//! (`tests/courier_registry_refresh_periphery.rs`, `..._boundary_periphery.rs`,
//! `..._fence_periphery.rs`, `tests/registry_refresh_driver_courier_convergence_periphery.rs`).
//! Included by each suite through `#[path]`, next to its own `mod common;`.

use std::process::Output;

/// Assert the `rigger <args>` invocation that produced `out` succeeded, naming its argv and both
/// output streams when it did not.
pub fn assert_ok(out: &Output, args: &[&str]) {
    assert!(
        out.status.success(),
        "rigger {args:?} failed: {}\n{}",
        String::from_utf8_lossy(&out.stderr),
        String::from_utf8_lossy(&out.stdout)
    );
}
