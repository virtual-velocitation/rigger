//! SDET periphery test for spec 90 criterion 1 (hermetic test git): the REAL runner-wiring
//! path, which is the one boundary claim `tests/hermetic_test_git_audit.rs` (the implementer's
//! own Done-when audit test) does not reach.
//!
//! That file proves two things: the runner SCRIPT's own source text assigns and exports the
//! fixed git-hermeticity block, and a `git commit` given that block directly ON ITS OWN
//! `Command` (via `.env(...)`, or by inheriting whatever this process's ambient environment
//! already happens to be) beats a hostile global config. Neither is a direct, unconditional
//! proof of the WIRING itself: that `.cargo/config.toml` really routes every test binary
//! through `.cargo/pidns-runner.sh` for the target this crate actually builds and tests on, and
//! that when `cargo test` invokes a real test binary through that chain (`nice` -> `setpriv` ->
//! `unshare` -> `timeout` -> `prlimit` -> the binary), the exported block survives the whole
//! chain and lands in that binary's own ambient process environment - with the test setting
//! nothing itself. A reverted or retargeted `runner =` line, or an exec hop in the script that
//! drops exported vars, would leave every assertion in the other file green (it reads the
//! script's text directly by hardcoded path, and it sets its own env on the child it spawns) while
//! the real guarantee - "every test binary gets this, hermetically, without asking" - silently
//! stops holding. This file is itself one of "every test binary" the runner promises to cover,
//! so it can observe its own environment directly, the same way `src/worktree.rs`'s and
//! `src/conductor.rs`'s ~43 `git commit` sites (which set no identity of their own) actually do.

use std::path::PathBuf;

/// The identity + no-prompt keys criterion 1 completes (spec 90's own Design text: "this spec
/// completes the block"), scoped deliberately to exclude the pre-landed environment-config
/// signing-suppression channel `tests/hermetic_test_git_audit.rs` already owns. Two reasons:
/// (1) that channel already has a stronger, more direct end-to-end proof than a plain
/// `env::var` check could add - its control/treatment pair drives a REAL `git commit` against
/// a hostile global config and shows gpg is never invoked, which an ambient-env check here
/// would only approximate; (2) that file's own `no_test_source_sets_the_signing_hermetic_variables_itself`
/// audit bans its two literal key/value tokens from appearing anywhere else under `src/` and
/// `tests/` (see decision `u90c1-audit-scope-signing-only` for the exact, deliberately narrow
/// scope) - this file does not own that channel and must not re-name either token, even in a
/// comment explaining why. Kept as an independent copy rather than shared with that file's own
/// constant - integration test files are separate binaries with no shared module here, and this
/// file proves a different claim (the wiring) so it must not couple to that file's.
const EXPECTED: [(&str, &str); 5] = [
    ("GIT_AUTHOR_NAME", "rigger-test"),
    ("GIT_AUTHOR_EMAIL", "rigger-test@localhost"),
    ("GIT_COMMITTER_NAME", "rigger-test"),
    ("GIT_COMMITTER_EMAIL", "rigger-test@localhost"),
    ("GIT_TERMINAL_PROMPT", "0"),
];

/// Criterion 1's missing half, part one: THIS test binary is itself one of "every test binary"
/// the runner promises to cover. If `cargo test` did not actually route it through
/// `.cargo/pidns-runner.sh` (a broken or reverted `.cargo/config.toml` entry, a target-triple
/// mismatch, or an `unshare`/`setpriv`/`timeout`/`prlimit` exec hop that dropped the exported
/// vars along the way) every assertion in `tests/hermetic_test_git_audit.rs` would still pass
/// in full - it never reads this process's real environment, only the runner script's source
/// text and a child process it configures by hand. This test reads `std::env::var` directly:
/// no subprocess, no simulation, no fixture - the actual ambient environment this actual test
/// process is running under, right now, as `cargo test` really invoked it.
#[test]
fn this_test_binary_actually_inherits_the_runners_fixed_git_identity_from_its_own_environment() {
    let mut missing = Vec::new();
    let mut wrong = Vec::new();
    for (key, want) in EXPECTED {
        match std::env::var(key) {
            Ok(got) if got == want => {}
            Ok(got) => wrong.push(format!("{key}: expected {want:?}, found {got:?}")),
            Err(_) => missing.push(key),
        }
    }
    assert!(
        missing.is_empty() && wrong.is_empty(),
        "this test binary's own process environment does not carry the runner's fixed \
         identity + no-prompt block - either `.cargo/config.toml`'s target-runner entry is not \
         routing `cargo test` through `.cargo/pidns-runner.sh` for this target, or that \
         script's exec chain (nice/setpriv/unshare/timeout/prlimit) is dropping the exported \
         vars before reaching the test binary. missing: {missing:?}; wrong: {wrong:?}"
    );
}

/// Criterion 1's missing half, part two: the other audit test hardcodes the path
/// `.cargo/pidns-runner.sh` and reads it directly, so it would keep passing even if
/// `.cargo/config.toml` stopped pointing there at all - the script's promises would then be
/// true of a script nothing actually invokes, and part one above would start failing with no
/// test naming the real cause. This test proves the wiring on the other side: the crate's
/// `.cargo/config.toml` really does name THIS script as the runner for the triple this repo
/// builds and tests on.
#[test]
fn cargo_config_actually_wires_the_hermetic_runner_for_the_test_target() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join(".cargo")
        .join("config.toml");
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));

    let mut in_target_section = false;
    let mut runner_line = None;
    for raw in text.lines() {
        let line = raw.trim();
        if line.starts_with('[') {
            in_target_section = line == "[target.x86_64-unknown-linux-gnu]";
            continue;
        }
        if in_target_section && line.starts_with("runner") {
            runner_line = Some(line.to_string());
            break;
        }
    }
    let runner_line = runner_line.unwrap_or_else(|| {
        panic!(
            "{} has no `runner = ...` entry under [target.x86_64-unknown-linux-gnu] - every \
             test binary on this target would run WITHOUT the hermetic-git block (and without \
             the pid-namespace containment spec 78 requires) instead of through the vetted \
             script",
            path.display()
        )
    });
    assert!(
        runner_line.contains(".cargo/pidns-runner.sh"),
        "[target.x86_64-unknown-linux-gnu]'s runner entry ({runner_line:?}) must point at \
         .cargo/pidns-runner.sh - the exact script tests/hermetic_test_git_audit.rs reads and \
         asserts the fixed git-hermeticity block against, and the exact script this file's own \
         env-inheritance test above depends on actually running under; any other target \
         silently defeats both proofs at once"
    );
}
