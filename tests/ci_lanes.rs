//! Guard the CI invariant that BOTH feature lanes stay fully gated.
//!
//! rigger ships with `turbovec` as a *default* feature and a deliberate
//! `--no-default-features` "grep-only" opt-out (see `Cargo.toml`). Those are two
//! distinct `cfg` universes: code behind `#[cfg(feature = "turbovec")]` vanishes in
//! the grep-only lane, and code that is only *reachable* there (fallback paths, the
//! "built without turbovec" branch) is dead in the default lane. Either can grow a
//! lint - an unused import, dead code, a `needless_return` - that `cargo build` still
//! accepts but `cargo clippy -- -D warnings` rejects.
//!
//! So the gate battery only holds if it runs on BOTH lanes. Historically the workflow
//! ran `cargo clippy --all-targets -- -D warnings` on the default (turbovec) lane but
//! only `cargo build` + `cargo test` on the grep-only lane - no clippy - so a
//! warning that surfaced *only* under `--no-default-features` could reach `main`
//! unnoticed. This test parses the committed workflow and asserts the full battery -
//! `fmt`, `clippy --all-targets -D warnings`, and `test` - is present for each lane,
//! so dropping a lane's gate fails here instead of silently eroding CI.
//!
//! It is intentionally NOT feature-gated: it reads a YAML file and touches no
//! turbovec/grep symbols, so it runs identically in both lanes and is a real member
//! of each lane's `cargo test` battery.

use std::path::PathBuf;

/// The committed CI workflow, resolved from the crate manifest dir so the test is
/// CWD-independent (integration tests may run from anywhere).
fn workflow_yaml() -> serde_yaml::Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join(".github")
        .join("workflows")
        .join("rust.yml");
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("cannot read CI workflow at {}: {e}", path.display()));
    serde_yaml::from_str(&text)
        .unwrap_or_else(|e| panic!("CI workflow at {} is not valid YAML: {e}", path.display()))
}

/// Every `run:` script across the steps of the named job, concatenated. A step's
/// `run` may be a single command or a multi-line block; both flatten to text we can
/// substring-match the gate commands against.
fn job_run_scripts(workflow: &serde_yaml::Value, job: &str) -> String {
    let steps = workflow
        .get("jobs")
        .and_then(|j| j.get(job))
        .and_then(|j| j.get("steps"))
        .and_then(|s| s.as_sequence())
        .unwrap_or_else(|| panic!("workflow has no `jobs.{job}.steps` sequence"));

    steps
        .iter()
        .filter_map(|step| step.get("run").and_then(|r| r.as_str()))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Assert `haystack` contains every fragment of `needles` in order, i.e. that a single
/// command with all those tokens is present (order-tolerant across the whole script is
/// too weak - `clippy` and `--no-default-features` could come from two different
/// steps). We match on one physical line so the tokens belong to the SAME command.
fn assert_command_present(script: &str, needles: &[&str], what: &str) {
    let found = script.lines().any(|line| {
        let mut rest = line;
        needles.iter().all(|needle| match rest.find(needle) {
            Some(idx) => {
                rest = &rest[idx + needle.len()..];
                true
            }
            None => false,
        })
    });
    assert!(
        found,
        "CI workflow (job build-test) must run {what}: no single `run:` line contained \
         all of {needles:?}.\nScript was:\n{script}"
    );
}

/// The turbovec (default-feature) lane must run the full gate battery: fmt check,
/// clippy over all targets with warnings denied, and the test suite.
#[test]
fn turbovec_lane_runs_the_full_gate_battery() {
    let wf = workflow_yaml();
    let script = job_run_scripts(&wf, "build-test");

    assert_command_present(&script, &["cargo fmt", "--check"], "cargo fmt --check");
    // The default lane omits `--no-default-features`, so match the clippy tokens
    // WITHOUT it; the grep-only test below asserts the flagged variant separately.
    assert_command_present(
        &script,
        &["cargo clippy", "--all-targets", "-D warnings"],
        "cargo clippy --all-targets -- -D warnings on the turbovec (default) build",
    );
    assert_command_present(&script, &["cargo test"], "cargo test on the turbovec build");
}

/// The grep-only (`--no-default-features`) lane must run the SAME battery, each command
/// carrying `--no-default-features`. This is the coverage the workflow historically
/// lacked (it ran build+test but no clippy on this lane): without clippy here, a lint
/// that only surfaces under the grep-only `cfg` reaches `main` unchecked.
#[test]
fn grep_only_lane_runs_the_full_gate_battery() {
    let wf = workflow_yaml();
    let script = job_run_scripts(&wf, "build-test");

    assert_command_present(
        &script,
        &[
            "cargo clippy",
            "--no-default-features",
            "--all-targets",
            "-D warnings",
        ],
        "cargo clippy --no-default-features --all-targets -- -D warnings on the grep-only build",
    );
    assert_command_present(
        &script,
        &["cargo build", "--no-default-features"],
        "cargo build --no-default-features on the grep-only build",
    );
    assert_command_present(
        &script,
        &["cargo test", "--no-default-features"],
        "cargo test --no-default-features on the grep-only build",
    );
}
