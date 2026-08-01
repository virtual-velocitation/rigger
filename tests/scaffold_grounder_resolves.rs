//! PERIPHERY (integration / API) test: every SHIPPED config artifact names a LIVE
//! grounder, never a RETIRED one (spec 57 capstone).
//!
//! The name-resolution contract (`grounder_for` / `is_retired_grounder` /
//! `retired_grounder_error`) is proven for FIXED input strings by
//! `tests/grounder_name_contract.rs`. This file proves the OTHER half - the one the
//! in-module scaffold test (`scaffold_parses_into_a_valid_config`, which asserts only that
//! the parsed default string equals `"symbols"`) is structurally blind to: that the value
//! the shipped artifacts ACTUALLY carry is a live grounder name when fed through that same
//! public resolver. It closes the exact seam a fresh `rigger init` (or a copy of the demo)
//! opens - `.rigger/workflow.yml` -> grounder selection: a config that names a retired
//! engine PARSES fine yet hard-errors at first `rigger ground`, the gate-invisible
//! regression spec 57 exists to end (the scaffold once shipped a retired default and the
//! only test pinned the string, so nothing caught it).
//!
//! Two shipped surfaces share one contract. The first is the SCAFFOLD `rigger init`
//! writes - driven END-TO-END through the compiled binary, then loaded and resolved
//! through the crate's PUBLIC surface (`SCAFFOLD_WORKFLOW` is private to the binary, so
//! only driving `rigger init` reaches the real artifact). The second is the committed DEMO
//! config `examples/demo/.rigger/workflow.yml`. For each, the emitted `defaults.grounder`
//! must NOT be retired and, fed through the resolver, must land on the `symbols` default
//! branch (never the migration error). A future edit reverting either artifact to
//! `turbovec` / `vector` / `hybrid` turns this RED - the guard the campaign lacked when
//! the scaffold shipped a retired name.
//!
//! Feature-independent: `grounder_for`'s `symbols` arm is not cfg-gated
//! (`main::select_grounder` is the gated interceptor, not this function), so every
//! assertion holds identically in both feature lanes and the file is ungated.

use rigger::grounder::{grounder_for, is_retired_grounder, retired_grounder_error};
use std::path::PathBuf;
use std::process::Command;

/// The compiled `rigger` binary under test (Cargo sets this for integration tests).
fn rigger_bin() -> &'static str {
    env!("CARGO_BIN_EXE_rigger")
}

/// Assert that `grounder` (a value a shipped config ACTUALLY carries) is a LIVE name: not a
/// retired engine, and, fed through the public resolver, it does NOT trip the retirement
/// migration error - it resolves as either an explicit `grep` / `nop` opt-in (an `Ok`
/// grounder) or the `symbols` default (whose loud, feature-independent error names
/// `symbols`, never a retired engine). `whence` names the artifact for a failure message.
fn assert_shipped_grounder_is_live(grounder: &str, whence: &str) {
    // 1. It is not a retired engine name - the single fact the campaign's regression violated.
    assert!(
        !is_retired_grounder(grounder),
        "{whence} names grounder {grounder:?}, a RETIRED engine (spec 57): a fresh consumer \
         parses it fine then hard-errors at first `rigger ground`. It must name a live grounder \
         (symbols / grep / nop)."
    );

    // 2. Fed through the public resolver, it must NOT be rejected with the retirement
    //    migration error - the exact failure a retired default produces.
    let got = grounder_for(grounder, ".");
    let retired = retired_grounder_error(grounder);
    assert_ne!(
        got.as_ref().err().map(String::as_str),
        Some(retired.as_str()),
        "{whence} grounder {grounder:?} must resolve, not trip the retirement migration error"
    );

    // 3. Characterise the accepted resolution so a silent grep degrade can never masquerade
    //    as success. `grep` / `nop` construct a grounder (`Ok`); the `symbols` default is a
    //    loud, feature-independent error that NAMES symbols and never a retired engine
    //    (`select_grounder` wires the real grounder when the feature is built).
    if let Err(e) = got {
        let low = e.to_lowercase();
        assert!(
            low.contains("symbols"),
            "{whence} default {grounder:?} must resolve on the symbols branch; got: {e}"
        );
        assert!(
            !low.contains("retire") && !low.contains("turbovec"),
            "{whence} default {grounder:?} must not read as a retired engine; got: {e}"
        );
    }
}

/// The SCAFFOLD `rigger init` writes, proven END-TO-END. Drive the compiled binary in a
/// throwaway git repo, then load the emitted `.rigger/workflow.yml` through the PUBLIC
/// `rigger::config` API and feed its `defaults.grounder` through the PUBLIC grounder
/// resolver. Unit tests cannot reach this seam: `SCAFFOLD_WORKFLOW` is private to the
/// binary and `scaffold_parses_into_a_valid_config` stops at the parsed string - so an
/// init that scaffolds a retired grounder would parse-pass yet break at first grounding,
/// invisibly. This binds the real emitted artifact to the resolver contract.
#[test]
fn rigger_init_scaffolds_a_live_grounder_default() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    // A real git repo mirrors how `rigger init` is actually used (stable project identity).
    let _ = Command::new("git")
        .args(["init", "-q"])
        .current_dir(root)
        .status();

    let out = Command::new(rigger_bin())
        .arg("init")
        .current_dir(root)
        .output()
        .expect("run the compiled `rigger init`");
    assert!(
        out.status.success(),
        "`rigger init` must scaffold cleanly; stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    // A bonus the string-pin cannot state: the scaffold `rigger init` writes is a config the
    // PUBLIC loader accepts whole, not merely a YAML file that happens to parse.
    let cfg = rigger::config::load(root.to_str().unwrap())
        .expect("the scaffolded project must load through the public config API");
    assert_shipped_grounder_is_live(
        &cfg.workflow.defaults.grounder,
        "the `rigger init` scaffold",
    );
}

/// The committed DEMO config a user copies as a starting point: its `defaults.grounder`
/// must be a live grounder too, so `rigger ground` in a copy of the demo never hits the
/// retirement error. Resolved from `CARGO_MANIFEST_DIR` so the test is CWD-independent
/// (integration tests may run from anywhere), and loaded through the same public API.
#[test]
fn shipped_demo_config_names_a_live_grounder_default() {
    let demo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/demo");
    let cfg = rigger::config::load(demo.to_str().unwrap())
        .unwrap_or_else(|e| panic!("the shipped demo config must load: {e}"));
    assert_shipped_grounder_is_live(
        &cfg.workflow.defaults.grounder,
        "the shipped examples/demo config",
    );
}
