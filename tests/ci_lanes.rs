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
//! unnoticed. This test parses the committed workflow and asserts the SAME battery -
//! `fmt`, `clippy --all-targets -D warnings`, `build`, and `test` - is present for
//! EACH lane, so dropping any lane's gate fails here instead of silently eroding CI.
//!
//! Discriminating by construction. The two lanes' commands differ only by the
//! `--no-default-features` flag, so a naive substring match for the turbovec battery
//! is satisfied by the grep-only lines and stays green even if the ENTIRE turbovec
//! lane is deleted. To close that hole, the turbovec assertions are ANCHORED: the
//! matched physical line must contain the gate tokens AND must NOT contain
//! `--no-default-features` (see `assert_lane_command`'s `forbidden` arg). The
//! grep-only assertions require `--no-default-features` positively. `cargo fmt`, which
//! takes no feature flags and runs once for both `cfg` universes, is asserted (anchored
//! against `--no-default-features`, which it never carries) in both tests: formatting
//! is part of each lane's battery even though a single shared step covers it.
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

/// Does `line` contain every fragment of `needles` in order? (An ordered-subsequence
/// substring match on ONE physical line, so the tokens belong to the SAME command -
/// order-tolerant matching across the whole script is too weak: `clippy` and a flag
/// could come from two different steps.)
fn line_has_all(line: &str, needles: &[&str]) -> bool {
    let mut rest = line;
    needles.iter().all(|needle| match rest.find(needle) {
        Some(idx) => {
            rest = &rest[idx + needle.len()..];
            true
        }
        None => false,
    })
}

/// Assert some single `run:` line contains every token of `needles` (in order) AND
/// does NOT contain `forbidden`. The `forbidden` clause is what makes a lane assertion
/// DISCRIMINATING: the turbovec (default) lane's commands are prefixes of the grep-only
/// lane's (which just add `--no-default-features`), so without excluding that flag the
/// turbovec assertions would be satisfied by grep-only lines and pass even if the whole
/// turbovec lane were deleted. Pass `forbidden = None` when no anchor is needed (e.g. a
/// grep-only assertion that already requires `--no-default-features` positively).
fn assert_lane_command(script: &str, needles: &[&str], forbidden: Option<&str>, what: &str) {
    let found = script
        .lines()
        .any(|line| line_has_all(line, needles) && forbidden.is_none_or(|bad| !line.contains(bad)));
    assert!(
        found,
        "CI workflow must run {what}: no single `run:` line contained \
         all of {needles:?}{}.\nScript was:\n{script}",
        match forbidden {
            Some(bad) => format!(" while NOT containing {bad:?}"),
            None => String::new(),
        }
    );
}

/// The turbovec (default-feature) lane must run the full gate battery: fmt check,
/// clippy over all targets with warnings denied, the default `cargo build`, and the
/// test suite. Every feature-sensitive assertion is ANCHORED to exclude
/// `--no-default-features` so a matching grep-only line cannot vacuously satisfy it -
/// deleting the turbovec lane makes this test fail (verified by the reviewer's lane-
/// deletion simulation), which is the whole point of the guard.
#[test]
fn turbovec_lane_runs_the_full_gate_battery() {
    let wf = workflow_yaml();
    let script = job_run_scripts(&wf, "build-test");
    const NO_DEFAULTS: &str = "--no-default-features";

    // `cargo fmt` takes no feature flags and runs once for both cfg universes; it never
    // carries --no-default-features, so anchoring against that flag is a no-op here but
    // keeps the assertion shape uniform across lanes.
    assert_lane_command(
        &script,
        &["cargo fmt", "--check"],
        Some(NO_DEFAULTS),
        "cargo fmt --check",
    );
    assert_lane_command(
        &script,
        &["cargo clippy", "--all-targets", "-D warnings"],
        Some(NO_DEFAULTS),
        "cargo clippy --all-targets -- -D warnings on the turbovec (default) build",
    );
    assert_lane_command(
        &script,
        &["cargo build"],
        Some(NO_DEFAULTS),
        "cargo build on the turbovec (default) build",
    );
    assert_lane_command(
        &script,
        &["cargo test"],
        Some(NO_DEFAULTS),
        "cargo test on the turbovec (default) build",
    );
}

/// The grep-only (`--no-default-features`) lane must run the SAME battery, each
/// feature-sensitive command carrying `--no-default-features`. This is the coverage the
/// workflow historically lacked (it ran build+test but no clippy on this lane): without
/// clippy here, a lint that only surfaces under the grep-only `cfg` reaches `main`
/// unchecked. `cargo fmt` is the shared, feature-independent step that covers both
/// lanes, so it is asserted here too (anchored against `--no-default-features`, which
/// fmt never carries) - fmt is part of this lane's battery just as it is the turbovec
/// lane's.
#[test]
fn grep_only_lane_runs_the_full_gate_battery() {
    let wf = workflow_yaml();
    let script = job_run_scripts(&wf, "build-test");
    const NO_DEFAULTS: &str = "--no-default-features";

    assert_lane_command(
        &script,
        &["cargo fmt", "--check"],
        Some(NO_DEFAULTS),
        "cargo fmt --check (shared, covers the grep-only lane)",
    );
    assert_lane_command(
        &script,
        &["cargo clippy", NO_DEFAULTS, "--all-targets", "-D warnings"],
        None,
        "cargo clippy --no-default-features --all-targets -- -D warnings on the grep-only build",
    );
    assert_lane_command(
        &script,
        &["cargo build", NO_DEFAULTS],
        None,
        "cargo build --no-default-features on the grep-only build",
    );
    assert_lane_command(
        &script,
        &["cargo test", NO_DEFAULTS],
        None,
        "cargo test --no-default-features on the grep-only build",
    );
}

/// The `install-nolock` job must run `cargo install --path .` WITHOUT `--locked` and then
/// execute the resulting binary. That job is the regression guard for dependency skew on a
/// FRESH resolve (`cargo install` without `--locked` ignores Cargo.lock and re-resolves to
/// the newest versions the manifest constraints allow - exactly how an end user installs,
/// and where a transitive crate like `ort-sys` can skew forward past the `ort` it must
/// match). `Cargo.toml` pins `ort-sys = "=2.0.0-rc.9"` to keep that resolve coherent; this
/// test ensures the CI job that PROVES the pin holds cannot be quietly deleted or have its
/// teeth pulled by someone adding `--locked` (which would make the install pass by reusing
/// the committed lockfile, defeating the entire point of the guard). Like the rest of this
/// file it parses the committed workflow rather than the running config, so it fails at
/// `cargo test` time - in the very build-test lane above - if the guard erodes.
#[test]
fn install_nolock_job_runs_a_fresh_unlocked_install_and_executes_the_binary() {
    let wf = workflow_yaml();
    let script = job_run_scripts(&wf, "install-nolock");

    // The load-bearing command: a path install that re-resolves from scratch. Anchored to
    // FORBID `--locked`, because a `--locked` install reuses Cargo.lock and would never
    // exercise a fresh resolution - the exact thing this job exists to test.
    assert_lane_command(
        &script,
        &["cargo install", "--path", "."],
        Some("--locked"),
        "cargo install --path . WITHOUT --locked (a --locked install would reuse Cargo.lock \
         and never exercise a fresh resolution, defeating the dep-skew guard)",
    );

    // A clean resolve that yields a broken binary is still a regression, so the job must
    // actually run the installed executable. It lives under the temp --root the install
    // step wrote to; asserting the `bin/rigger` invocation keeps the "prove it runs" step
    // from being dropped while the install step stays.
    assert_lane_command(
        &script,
        &["/bin/rigger"],
        None,
        "execution of the freshly-installed rigger binary (so a clean resolve that produces \
         a non-working binary still fails CI)",
    );
}
