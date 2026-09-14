//! Periphery (CLI, real-binary) test for spec 89, criterion 2's own Done-when text: "the
//! reaper and `validate` account for the new root". `rigger validate`'s FOOTPRINT ACCOUNTING
//! (spec 77 criterion 6) is periphery-tested in `tests/cli.rs`
//! (`validate_reports_footprint_by_category_and_flags_a_dead_share_breach` and siblings) - but
//! every one of those tests points `RIGGER_TMPDIR` (and `XDG_CACHE_HOME`, for the mutation-
//! scratch category) at a fixture-controlled directory, for spec 77-era hermeticity. None of
//! them ever leaves the scratch root at its DEFAULT, unconfigured resolution - so none can see
//! whether `cmd_validate`'s "shared build cache" / "worktrees" / "per-unit caches" categories
//! were wired to the relocated cache-home default this criterion introduces, or quietly kept
//! reading the pre-relocation `<repo>/.rigger/tmp` path (which this criterion's own Design
//! text says a real spawn's worktree no longer lives under at all). `tests/reset_build_cache_
//! periphery.rs` proves the SAME default-root resolution for `rigger reset --build-cache`, but
//! its own header explicitly disclaims `rigger validate`'s footprint category as "spec 77
//! criterion 5's own surface" (a DIFFERENT criterion, not this one) - leaving this criterion's
//! own "validate accounts for the new root" clause unproven by any existing test. This file
//! closes that gap: real bytes seeded at the path `worktree::cache_scratch_root_from` computes
//! for a wholly unconfigured project, then `rigger validate` driven with no `RIGGER_TMPDIR`
//! override at all (only `common::rigger_courier`'s own `XDG_CACHE_HOME` isolation), proving
//! the wiring - not just the pure resolver - reaches the relocated default.

use std::path::Path;
use std::process::Command;

mod common;

fn temp_project() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("create temp project");
    let _ = Command::new("git")
        .args(["init", "-q"])
        .current_dir(dir.path())
        .status();
    dir
}

/// Seed an initialized, otherwise-empty `.rigger/events.db`, mirroring
/// `tests/reset_build_cache_periphery.rs::seed_store` - `rigger validate` needs a resolvable
/// store only to anchor the scratch root at the same repo root every other scratch-touching
/// command uses.
fn seed_store(root: &Path) {
    std::fs::create_dir_all(root.join(".rigger")).unwrap();
    std::fs::File::create(root.join(".rigger").join("events.db")).unwrap();
}

/// Run `rigger <args...>` in `cwd` through the isolated courier (its own throwaway
/// `XDG_CACHE_HOME` is the ONLY scratch-relevant environment this file ever sets - no
/// `RIGGER_TMPDIR`, no configured `defaults.workdir` - so every call genuinely resolves the
/// DEFAULT rung, never an override).
fn run_rigger(cwd: &Path, args: &[&str]) -> (String, String, bool) {
    let state = tempfile::tempdir().expect("create a temp XDG_STATE_HOME");
    let out = common::rigger_courier()
        .args(args)
        .current_dir(cwd)
        .env("RIGGER_NO_DASH", "1")
        .env("XDG_STATE_HOME", state.path())
        .output()
        .expect("failed to spawn the rigger binary");
    (
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
        out.status.success(),
    )
}

/// Spec 89, criterion 2: with NEITHER `RIGGER_TMPDIR` NOR a configured `defaults.workdir` set -
/// the ordinary shape of every real project - `rigger validate`'s "shared build cache"
/// footprint category measures real bytes seeded at the relocated cache-home default
/// (`common::default_scratch_root(root).join("cargo-target")`), never 0B (the silent-miss a
/// still-pre-relocation-wired `cmd_validate` would report) and never the pre-relocation
/// `<repo>/.rigger/tmp/cargo-target` path (left deliberately empty here, so a wrong-but-
/// nonzero report from THAT path cannot masquerade as success).
#[test]
fn validate_measures_the_shared_build_cache_at_the_relocated_default_root_with_no_override() {
    let dir = temp_project();
    let root = dir.path();

    // `rigger init` scaffolds the `.rigger/agents/` tree `validate` requires to resolve
    // config, mirroring `tests/cli.rs::validate_reports_footprint_by_category_and_flags_a_
    // dead_share_breach`'s own setup.
    let (_out, err, ok) = run_rigger(root, &["init"]);
    assert!(ok, "rigger init must succeed; stderr:\n{err}");
    for args in [
        &["config", "user.email", "t@example.com"][..],
        &["config", "user.name", "t"],
        &["add", "-A"],
        &["commit", "-q", "-m", "scaffold"],
    ] {
        assert!(
            Command::new("git")
                .args(args)
                .current_dir(root)
                .status()
                .expect("spawn git")
                .success(),
            "git {args:?} must succeed while seeding the repo"
        );
    }
    seed_store(root);

    // The OLD, pre-relocation default location - left untouched (not even created), so a
    // regression back to it would report a real category miss (0B), never a false positive.
    let old_default = root.join(".rigger").join("tmp").join("cargo-target");
    assert!(
        !old_default.exists(),
        "fixture bug: the pre-relocation default must not exist, or a regression back to \
         reading it could coincidentally still measure something"
    );

    // 4096 bytes at the RELOCATED default - matching the exact byte count (and so the exact
    // "4.0K" formatting) `tests/cli.rs::validate_reports_footprint_by_category_and_flags_a_
    // dead_share_breach` already pins for this same category's human-readable output, so this
    // test's expectation is not a hand-guessed format.
    let relocated_cache = common::default_scratch_root(root).join("cargo-target");
    std::fs::create_dir_all(&relocated_cache).expect("create the relocated shared cache dir");
    std::fs::write(relocated_cache.join("x.rlib"), [0u8; 4096])
        .expect("seed 4096 bytes at the relocated default shared build cache");

    let (out, err, ok) = run_rigger(root, &["validate"]);
    assert!(
        ok,
        "validate must exit 0 even while flagging a footprint advisory; stderr:\n{err}"
    );
    assert!(
        out.contains("footprint: shared build cache 4.0K"),
        "the bytes seeded at the RELOCATED default root must be measured - a wiring mistake \
         that left cmd_validate reading the pre-relocation path would report 0B here instead; \
         stdout:\n{out}"
    );
}
