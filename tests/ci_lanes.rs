//! Guard the CI invariant that EVERY feature lane stays fully gated, and that CI and the loop's
//! check-in gate run the same lanes.
//!
//! rigger ships with `turbovec` as a *default* feature, a deliberate `--no-default-features`
//! "grep-only" opt-out, and the pure `core` lane (`--no-default-features --features core`, see
//! `Cargo.toml`). Those are distinct `cfg` universes: code behind a feature vanishes in a lane
//! that drops it, and code only *reachable* there (fallback paths, the "built without turbovec"
//! branch) is dead in the others. Any lane can grow a lint - an unused import, dead code, a
//! `needless_return` - that `cargo build` still accepts but `cargo clippy -- -D warnings`
//! rejects, or a test that fails in that universe alone.
//!
//! So the gate battery only holds if it runs on EVERY lane. The default lane's battery - `fmt`,
//! `clippy --all-targets -D warnings`, `build` and `test` - is listed in the committed workflow
//! itself. The other two lanes run through ONE script, `.rigger/gates/lanes.sh <no-default|core>`:
//! CI calls it, and so does the check-in stage's `lanes` gate, so the loop and CI never run
//! different commands. The script derives the core lane's members from `cargo metadata` (every
//! workspace member that declares a `core` feature), so a new crate joins the lane without an
//! edit anywhere, and it fails a test step whose every test binary runs zero tests: a lane that
//! tests nothing is an instrument that cannot fail, never a green lane.
//!
//! Discriminating by construction. The default lane's commands are prefixes of a light-lane
//! command (which adds `--no-default-features`), so a naive substring match for the default
//! battery would be satisfied by a light-lane line. The default-lane assertions are therefore
//! ANCHORED: the matched physical line must contain the gate tokens AND must NOT contain
//! `--no-default-features` (see `assert_lane_command`'s `forbidden` arg).
//!
//! It is intentionally NOT feature-gated: it reads YAML files and runs a shell script, touching
//! no turbovec/grep symbols, so it runs identically in every lane that runs it.

mod common;
use common::repo::{repo_root, repo_text, stub_path, table_declares_key, table_lines};
use common::shell_outcome;
use std::process::Command;

/// The script CI and the check-in stage's `lanes` gate run each non-default feature lane
/// through, relative to the repository root.
const LANES_SCRIPT: &str = ".rigger/gates/lanes.sh";

/// The feature lanes [`LANES_SCRIPT`] runs, by the argument that selects each.
const LANES: [&str; 2] = ["no-default", "core"];

/// The committed CI workflow, resolved from the crate manifest dir so the test is
/// CWD-independent (integration tests may run from anywhere).
fn workflow_yaml() -> serde_yaml::Value {
    let text = repo_text(".github/workflows/rust.yml");
    serde_yaml::from_str(&text)
        .unwrap_or_else(|e| panic!("CI workflow .github/workflows/rust.yml is not valid YAML: {e}"))
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

/// The `with.targets` value of the named job's `dtolnay/rust-toolchain@*` step, or an empty
/// string if the job has no such step or that step declares no extra targets. Toolchain
/// targets are a step INPUT, not a `run:` script, so this reads `uses`/`with` rather than
/// `job_run_scripts`'s `run:` text.
fn job_toolchain_targets(workflow: &serde_yaml::Value, job: &str) -> String {
    let steps = workflow
        .get("jobs")
        .and_then(|j| j.get(job))
        .and_then(|j| j.get("steps"))
        .and_then(|s| s.as_sequence())
        .unwrap_or_else(|| panic!("workflow has no `jobs.{job}.steps` sequence"));

    steps
        .iter()
        .find(|step| {
            step.get("uses")
                .and_then(|u| u.as_str())
                .is_some_and(|u| u.starts_with("dtolnay/rust-toolchain"))
        })
        .and_then(|step| step.get("with"))
        .and_then(|w| w.get("targets"))
        .and_then(|t| t.as_str())
        .unwrap_or("")
        .to_string()
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
        "the lane must run {what}: no single command line contained \
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

/// CI runs both non-default lanes through the lanes script and lists no light-lane or core-lane
/// cargo command of its own: a hand-listed lane drifts from the one the check-in gate runs, and a
/// hand-listed core step reaches only the package it names - the root package's `--lib` alone runs
/// zero tests, while every member crate's core-cfg tests go unrun.
#[test]
fn ci_runs_the_no_default_and_core_lanes_through_the_lanes_script() {
    let script = job_run_scripts(&workflow_yaml(), "build-test");
    for lane in LANES {
        let call = format!("sh {LANES_SCRIPT} {lane}");
        assert!(
            script.lines().any(|line| line.trim() == call),
            "the build-test job must run the {lane} lane as `{call}`.\nScript was:\n{script}"
        );
    }
    let hand_listed: Vec<&str> = script
        .lines()
        .map(str::trim)
        .filter(|line| line.starts_with("cargo ") && line.contains("--no-default-features"))
        .collect();
    assert!(
        hand_listed.is_empty(),
        "every no-default and core lane command runs through {LANES_SCRIPT}, never as a CI line \
         of its own: {hand_listed:#?}"
    );
}

/// The check-in stage gates both non-default lanes through the very commands CI runs: its
/// `lanes` gate calls the lanes script once per lane, so the loop's check-in and CI never run
/// different lane commands, and the post-merge re-gate covers the merged tree with them. Like
/// the `test` gate, it first sources the container runtime snippet, so a container-backed test
/// runs under the gate instead of skipping.
#[test]
fn ci_and_the_lanes_gate_run_one_script_that_derives_its_members() {
    let workflow: serde_yaml::Value = serde_yaml::from_str(&repo_text(".rigger/workflow.yml"))
        .expect(".rigger/workflow.yml must be valid YAML");
    let listed = workflow["stages"]["checkin"]["gates"]
        .as_sequence()
        .is_some_and(|gates| gates.iter().any(|gate| gate.as_str() == Some("lanes")));
    assert!(listed, "the checkin stage must list the `lanes` gate");
    let gate = workflow["gates"]["lanes"]["run"]
        .as_str()
        .expect(".rigger/workflow.yml must declare a `lanes` gate with a `run` command");
    assert!(
        gate.starts_with(". .rigger/gates/container-env.sh"),
        "the `lanes` gate must source the container runtime snippet first: {gate}"
    );
    let ci = job_run_scripts(&workflow_yaml(), "build-test");
    for lane in LANES {
        let call = format!("sh {LANES_SCRIPT} {lane}");
        assert!(
            gate.contains(&call) && ci.lines().any(|line| line.trim() == call),
            "the `lanes` gate and CI must both run `{call}`: gate {gate:?}"
        );
    }
}

/// The light lane lints every workspace target with warnings denied and runs every workspace
/// test, both without the default features - the battery the default lane runs, on the
/// grep-only `cfg` universe.
#[test]
fn the_no_default_lane_lints_and_tests_the_whole_workspace() {
    let commands = lanes_dry_run("no-default").join("\n");
    assert_lane_command(
        &commands,
        &[
            "cargo clippy",
            "--workspace",
            "--all-targets",
            "--no-default-features",
            "-D warnings",
        ],
        Some("--features"),
        "cargo clippy --workspace --all-targets --no-default-features -- -D warnings",
    );
    assert_lane_command(
        &commands,
        &["cargo test", "--workspace", "--no-default-features"],
        Some("--features"),
        "cargo test --workspace --no-default-features",
    );
}

/// The core lane reaches every workspace member that declares a `core` feature, derived from
/// the workspace rather than listed: each is linted under `--no-default-features --features
/// core`, over all its targets, and its library and integration tests run there too. A member
/// that ships a binary compiles only its library under `core` (THE FEATURE SPLIT gates the
/// binary, and the tests that drive it, behind `store`), so it is linted on its library alone
/// and runs no core test step.
#[test]
fn the_lanes_script_names_every_member_with_a_core_feature() {
    let members = core_members();
    assert!(
        members.len() > 1,
        "the workspace must have several members with a `core` feature, found {members:?}"
    );
    let commands = lanes_dry_run("core");
    let names = |line: &str, member: &str| {
        let tokens: Vec<&str> = line.split_whitespace().collect();
        tokens.windows(2).any(|pair| pair == ["-p", member])
    };
    let with = |prefix: &str, member: &str| -> Vec<String> {
        commands
            .iter()
            .filter(|line| line.starts_with(prefix) && names(line, member))
            .cloned()
            .collect()
    };
    for line in &commands {
        assert!(
            line_has_all(line, &["--no-default-features", "--features core"]),
            "every core-lane command runs under the core cfg: {line}"
        );
    }
    for (member, ships_binary) in &members {
        let clippy = with("cargo clippy", member);
        let tests = with("cargo test", member);
        let targets = if *ships_binary {
            "--lib"
        } else {
            "--all-targets"
        };
        assert!(
            clippy.len() == 1 && clippy[0].split_whitespace().any(|t| t == targets),
            "member `{member}` must be linted once, with `{targets}`: {commands:#?}"
        );
        assert_eq!(
            tests.len(),
            usize::from(!ships_binary),
            "member `{member}` must run its core tests exactly when it ships no binary: \
             {commands:#?}"
        );
        assert!(
            tests
                .iter()
                .all(|line| line.split_whitespace().any(|t| t == "--tests")),
            "member `{member}`'s core tests take its library and integration tests: {tests:#?}"
        );
    }
}

/// A lane's test step whose every test binary reports `running 0 tests` fails, in both lanes:
/// a lane that ran no test proves nothing, so it must never pass as if it had. One binary that
/// ran a test is enough to pass, and a test step that fails fails the lane whatever it ran.
#[test]
fn a_lane_test_step_whose_every_binary_runs_zero_tests_fails() {
    for lane in LANES {
        let (passed, out) = run_lane_with_stub_cargo(lane, "0", "0");
        assert!(
            !passed && out.contains("ran no test"),
            "the {lane} lane must fail a test step whose every binary ran zero tests:\n{out}"
        );
        let (passed, out) = run_lane_with_stub_cargo(lane, "2", "0");
        assert!(
            passed,
            "the {lane} lane must pass a test step in which one binary ran tests:\n{out}"
        );
        let (passed, out) = run_lane_with_stub_cargo(lane, "2", "101");
        assert!(
            !passed,
            "the {lane} lane must fail when its test step fails:\n{out}"
        );
    }
}

/// Every lane names the whole workspace: each `cargo fmt` line of the build-test job carries
/// `--all`, and each `cargo clippy`, `cargo build` and `cargo test` line of the default lane and
/// of the lanes script's no-default lane carries `--workspace`. At the repository root a bare
/// `cargo clippy`, `cargo build` or `cargo test` reaches the root package alone, so a lane that
/// drops the flag still passes while no member crate's own tests run and none of its clippy
/// targets are linted. Flags are matched as whole tokens, so `--all-targets` never passes for
/// `--all`. The core lane names its members one `-p` each, pinned by
/// `the_lanes_script_names_every_member_with_a_core_feature`.
#[test]
fn every_lane_names_the_whole_workspace() {
    let ci = job_run_scripts(&workflow_yaml(), "build-test");
    let light = lanes_dry_run("no-default").join("\n");
    let unscoped: Vec<&str> = lines_missing_the_workspace_scope(&ci)
        .into_iter()
        .chain(lines_missing_the_workspace_scope(&light))
        .collect();
    assert!(
        unscoped.is_empty(),
        "every `cargo fmt` line of the build-test job must carry `--all`, and every `cargo \
         clippy`/`cargo build`/`cargo test` line of the default and no-default lanes must carry \
         `--workspace` (a bare command at the repository root reaches the root package alone). \
         Unscoped: {unscoped:#?}"
    );
}

/// The lines of `script` that run a lane command without naming the whole workspace: a `cargo
/// fmt` without the `--all` token, or a `cargo clippy`/`cargo build`/`cargo test` without the
/// `--workspace` token.
fn lines_missing_the_workspace_scope(script: &str) -> Vec<&str> {
    script
        .lines()
        .map(str::trim)
        .filter(|line| {
            let has = |flag: &str| line.split_whitespace().any(|token| token == flag);
            if line.starts_with("cargo fmt") {
                return !has("--all");
            }
            let lane = ["cargo clippy", "cargo build", "cargo test"]
                .iter()
                .any(|command| line.starts_with(command));
            lane && !has("--workspace")
        })
        .collect()
}

/// The commands `sh .rigger/gates/lanes.sh <lane>` derives for `lane`, run at the repository root
/// under `LANES_DRY=1`, which prints each command as a `+ <command>` line and runs none of them.
fn lanes_dry_run(lane: &str) -> Vec<String> {
    let out = Command::new("sh")
        .arg(LANES_SCRIPT)
        .arg(lane)
        .env("LANES_DRY", "1")
        .current_dir(repo_root())
        .output()
        .expect("sh must spawn");
    let (ok, text) = shell_outcome(&out);
    assert!(
        ok,
        "`LANES_DRY=1 sh {LANES_SCRIPT} {lane}` must succeed:\n{text}"
    );
    text.lines()
        .filter_map(|line| line.strip_prefix("+ "))
        .map(str::to_string)
        .collect()
}

/// `sh .rigger/gates/lanes.sh <lane>` run for real at the repository root against the stand-in
/// `cargo` (`tests/fixtures/lanes-cargo.sh`), whose first test binary runs `tests` tests and
/// whose `cargo test` exits `exit`. Returns (passed, output).
fn run_lane_with_stub_cargo(lane: &str, tests: &str, exit: &str) -> (bool, String) {
    let work = tempfile::tempdir().unwrap();
    let out = Command::new("sh")
        .arg(LANES_SCRIPT)
        .arg(lane)
        .current_dir(repo_root())
        .env(
            "PATH",
            stub_path(work.path(), "cargo", Some("lanes-cargo.sh")),
        )
        .env_remove("LANES_DRY")
        .env("LANES_CARGO_TESTS", tests)
        .env("LANES_CARGO_EXIT", exit)
        .output()
        .expect("sh must spawn");
    shell_outcome(&out)
}

/// Every workspace member that declares a `core` feature, by package name, with whether it ships
/// a binary (a `[[bin]]` table or a `src/main.rs`) - read from the manifests, independently of the
/// lanes script's own `cargo metadata` derivation.
fn core_members() -> Vec<(String, bool)> {
    let root = repo_text("Cargo.toml");
    let members_line = table_lines(&root, "workspace")
        .into_iter()
        .find(|line| line.trim_start().starts_with("members"))
        .expect("the root manifest lists its workspace members");
    members_line
        .split('"')
        .skip(1)
        .step_by(2)
        .filter_map(|dir| {
            let manifest = repo_text(&format!("{dir}/Cargo.toml"));
            if !table_declares_key(&manifest, "features", "core") {
                return None;
            }
            let name = table_lines(&manifest, "package")
                .into_iter()
                .find_map(|line| {
                    let value = line.trim().strip_prefix("name")?.trim().strip_prefix('=')?;
                    Some(value.trim().trim_matches('"').to_string())
                })
                .expect("every member manifest names its package");
            let ships_binary =
                manifest.contains("[[bin]]") || repo_root().join(dir).join("src/main.rs").is_file();
            Some((name, ships_binary))
        })
        .collect()
}

/// The CI workflow raises no test address-space cap: every test step runs under the test
/// runner's default bound (`.cargo/pidns-runner.sh`), the bound the loop's own test gate runs
/// under, so a test that needs more address space than that fails CI exactly as it fails the
/// gate - never passing CI on a raised cap the gate does not have.
#[test]
fn the_ci_workflow_raises_no_test_address_space_cap() {
    let text = repo_text(".github/workflows/rust.yml");
    let raised: Vec<(usize, &str)> = text
        .lines()
        .enumerate()
        .filter(|(_, line)| line.contains("RIGGER_TEST_AS_BYTES"))
        .map(|(index, line)| (index + 1, line.trim()))
        .collect();
    assert!(
        raised.is_empty(),
        ".github/workflows/rust.yml must not name the test runner's address-space override: \
         {raised:#?}"
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

/// The dedicated `kurrentdb` CI job must stay CONSISTENT with the retired feature (spec
/// 47). The adapter is compiled into every build now, so the job carries no
/// `--features kurrentdb` / `-F kurrentdb`: passing the retired feature would make cargo
/// reject the command as an unknown feature and break CI. The job must still run the
/// adapter's contract test (`eventstore::kurrentdb`) against a real KurrentDB - that
/// container-backed proxy-fidelity check is the reason the job exists, so a change that
/// drops the feature flag must not also gut the test it guards - and it must run it in the
/// crate that holds the adapter, `rigger-store-sqlite`: in the root package the same filter
/// matches no test and the job passes having run nothing. Like the rest of this file it
/// parses the committed workflow, so an inconsistency fails at `cargo test` time.
#[test]
fn kurrentdb_job_carries_no_retired_feature_flag_and_still_runs_the_contract_test() {
    let wf = workflow_yaml();
    let script = job_run_scripts(&wf, "kurrentdb");

    assert!(
        !script.contains("--features kurrentdb") && !script.contains("-F kurrentdb"),
        "the `kurrentdb` CI job must NOT pass the retired `kurrentdb` cargo feature (spec 47): the \
         feature no longer exists, so cargo would reject the command as an unknown feature.\n\
         Script was:\n{script}"
    );
    assert!(
        script.contains("eventstore::kurrentdb"),
        "the `kurrentdb` CI job must still run the adapter's contract test (`eventstore::kurrentdb`) \
         against a real KurrentDB - the container-backed proxy-fidelity check the job exists for.\n\
         Script was:\n{script}"
    );
    assert_lane_command(
        &script,
        &[
            "cargo test",
            "-p rigger-store-sqlite",
            "eventstore::kurrentdb",
        ],
        None,
        "the adapter's tests in the crate that holds them (in the root package the filter \
         matches no test)",
    );
}

/// Spec 93 criterion 3, THE BUILD EMBEDS IT: "a workflow-level install in
/// `.github/workflows/rust.yml`; no unit installs it". `build.rs`'s nested cross-compile of
/// `crates/console-core` for `wasm32-unknown-unknown` runs whenever the outer build compiles
/// `crates/rigger-dash/src/dash.rs` - which is every cargo invocation in THIS workflow (none of them build the
/// pure `--features core` lane exclusively): `build-test`'s default AND
/// `--no-default-features` lanes and `install-nolock`'s default-feature install. Each of
/// those two jobs' own `dtolnay/rust-toolchain@*` step must therefore declare the target - a
/// job whose toolchain step lacks it would fail on a runner with no target preinstalled. The
/// `kurrentdb` job builds `rigger-store-sqlite` alone, which never compiles dash.rs.
#[test]
fn every_job_that_builds_dash_installs_the_console_core_wasm_target() {
    let wf = workflow_yaml();
    for job in ["build-test", "install-nolock"] {
        let targets = job_toolchain_targets(&wf, job);
        assert!(
            targets.contains("wasm32-unknown-unknown"),
            "job `{job}`'s rust-toolchain step must declare `targets: wasm32-unknown-unknown` \
             (spec 93 criterion 3: build.rs's nested cross-compile of crates/console-core \
             needs it whenever this job's cargo invocations compile crates/rigger-dash/src/dash.rs, which none \
             of `{job}`'s commands avoid). Found targets: {targets:?}"
        );
    }
}
