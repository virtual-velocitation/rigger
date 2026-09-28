//! Periphery (real-git) proof for the check-in `mutation` gate: the SHIPPED script
//! `.rigger/gates/mutation.sh` - the file `.rigger/workflow.yml` runs and `rigger init` writes
//! into a consumer project - driven against fixture repositories with a stand-in `cargo` and a
//! stand-in `systemd-run` on a fixture PATH (`tests/fixtures/mutation-gate-*.sh`, argv
//! capture), so every launch decision is read back exactly and no real sweep ever runs.
//!
//! THE DIFF BASE (spec 91, adv-u91c2-mutation-gate-diff-base-collapses-to-empty). The gate
//! diffs the whole spec against `$RIGGER_RUN_BASE` - the run branch's tip AT THE MOMENT the run
//! started (`RunStarted.base_tip`) - never a merge base with the run branch: the checkin
//! stage's own worktree branches off the run branch AFTER every implement unit integrated, so
//! a merge base there is already HEAD and the sweep would certify nothing, every run. A run
//! with no recorded base refuses loud.
//!
//! THE SWEEP'S BOUNDS AND SCOPE (gap 92 and the check-in budget). The sweep mutates the whole
//! workspace (`--workspace`: the root manifest is a package, so a bare sweep would see only
//! the root package's files) and tests, per mutant, only the packages its diff touches plus
//! the root package - never `--test-workspace`; it runs under nextest; the baseline run stays
//! on (nothing proves the scoped packages green on this tree - see the script's header); and
//! it runs inside its own systemd scope bounded to half of `MemAvailable`, with `-j` sized
//! from that bound, or unbounded with an advisory where no systemd user manager exists.
//!
//! THE GATE OWNS ITS INSTRUMENT. A unit diff that adds an exclusion or examine key to
//! `.cargo/mutants.toml`, or a cargo-mutants skip attribute, fails before any sweep.

mod common;

use common::git::{git_commit_all, git_ok, git_out, init_repo};
use common::repo::repo_root;
use std::path::{Path, PathBuf};
use std::process::Command;

/// The shipped gate script.
fn gate_script() -> PathBuf {
    repo_root()
        .join(".rigger")
        .join("gates")
        .join("mutation.sh")
}

/// The tools the gate script runs besides `cargo` and `systemd-run`.
const GATE_TOOLS: &[&str] = &[
    "git", "awk", "sed", "sort", "cat", "rm", "mkdir", "mv", "cp", "head", "tr", "wc", "dirname",
    "env", "xargs", "grep", "true",
];

/// The first executable named `tool` on the ambient PATH.
fn ambient_tool(tool: &str) -> PathBuf {
    std::env::var_os("PATH")
        .into_iter()
        .flat_map(|p| std::env::split_paths(&p).collect::<Vec<_>>())
        .map(|d| d.join(tool))
        .find(|p| p.is_file())
        .unwrap_or_else(|| panic!("`{tool}` must be on PATH for the mutation gate fixture"))
}

/// A fixture PATH directory holding the gate's ordinary tools, the stand-in `cargo`, and - when
/// `with_systemd_run` - the stand-in `systemd-run`. Every entry is a symlink, so no test ever
/// writes a file it then executes.
fn fixture_bin(dir: &Path, with_systemd_run: bool) -> PathBuf {
    let bin = dir.join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    for tool in GATE_TOOLS {
        std::os::unix::fs::symlink(ambient_tool(tool), bin.join(tool)).unwrap();
    }
    let fixtures = repo_root().join("tests").join("fixtures");
    std::os::unix::fs::symlink(fixtures.join("mutation-gate-cargo.sh"), bin.join("cargo")).unwrap();
    if with_systemd_run {
        std::os::unix::fs::symlink(
            fixtures.join("mutation-gate-systemd-run.sh"),
            bin.join("systemd-run"),
        )
        .unwrap();
    }
    bin
}

/// What one run of the shipped gate did.
struct GateRun {
    passed: bool,
    output: String,
    /// One line per `cargo` invocation, its argv space-joined (empty when cargo never ran).
    cargo: String,
    /// One line per `systemd-run` invocation (empty when it never ran or is absent).
    scope: String,
}

impl GateRun {
    /// The main sweep's `cargo` argv line (the one that sweeps `unit.diff`).
    fn sweep_line(&self) -> &str {
        self.cargo
            .lines()
            .find(|l| l.contains("mutants") && l.contains("--in-diff unit.diff"))
            .unwrap_or_else(|| {
                panic!(
                    "no sweep of unit.diff was launched; cargo saw:\n{}",
                    self.cargo
                )
            })
    }
}

/// Run the shipped gate in `repo` with `base` as `$RIGGER_RUN_BASE` (unset when `None`), a
/// meminfo reporting `mem_available_kb`, and the fixture PATH.
fn run_gate(
    repo: &Path,
    base: Option<&str>,
    mem_available_kb: u64,
    with_systemd_run: bool,
) -> GateRun {
    let work = tempfile::tempdir().unwrap();
    let bin = fixture_bin(work.path(), with_systemd_run);
    let meminfo = work.path().join("meminfo");
    std::fs::write(
        &meminfo,
        format!("MemTotal:       65000000 kB\nMemAvailable:   {mem_available_kb} kB\n"),
    )
    .unwrap();
    let cargo_capture = work.path().join("cargo.argv");
    let scope_capture = work.path().join("scope.argv");
    let mut cmd = Command::new("/bin/sh");
    cmd.arg(gate_script())
        .current_dir(repo)
        .env("PATH", &bin)
        .env(
            "MUTANTS",
            work.path().join("scratch").join("cargo-mutants-checkin"),
        )
        .env("RIGGER_MEMINFO", &meminfo)
        .env("RIGGER_ARGV_CAPTURE", &cargo_capture)
        .env("RIGGER_SCOPE_CAPTURE", &scope_capture)
        .env_remove("CARGO_TARGET_DIR");
    match base {
        Some(b) => cmd.env("RIGGER_RUN_BASE", b),
        None => cmd.env_remove("RIGGER_RUN_BASE"),
    };
    let out = cmd.output().expect("run the shipped mutation gate");
    GateRun {
        passed: out.status.success(),
        output: format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        ),
        cargo: std::fs::read_to_string(&cargo_capture).unwrap_or_default(),
        scope: std::fs::read_to_string(&scope_capture).unwrap_or_default(),
    }
}

/// Write `content` to `rel` under `repo`, creating parent directories.
fn write(repo: &Path, rel: &str, content: &str) {
    let path = repo.join(rel);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, content).unwrap();
}

/// A three-package workspace (`fixture-root` at the root, `alpha` and `beta` under crates/)
/// with one commit, returning its base sha; a later change to `alpha` alone is the unit diff.
fn workspace_repo(repo: &Path) -> String {
    init_repo(repo);
    write(
        repo,
        "Cargo.toml",
        "[package]\nname = \"fixture-root\"\nversion = \"0.1.0\"\n\n[workspace]\nmembers = [\".\", \"crates/alpha\", \"crates/beta\"]\n",
    );
    write(repo, "src/lib.rs", "pub fn root() -> u8 {\n    1\n}\n");
    for name in ["alpha", "beta"] {
        write(
            repo,
            &format!("crates/{name}/Cargo.toml"),
            &format!("[package]\nname = \"{name}\"\nversion = \"0.1.0\"\n\n[dependencies]\n"),
        );
        write(
            repo,
            &format!("crates/{name}/src/lib.rs"),
            "pub fn f(a: u8, b: u8) -> u8 {\n    a + b\n}\n",
        );
    }
    git_commit_all(repo, "base");
    let base = git_out(repo, &["rev-parse", "HEAD"]);
    write(
        repo,
        "crates/alpha/src/lib.rs",
        "pub fn f(a: u8, b: u8) -> u8 {\n    a * b\n}\n",
    );
    git_commit_all(repo, "unit changes alpha");
    base
}

/// 40 GiB available: a 20 GiB bound, three 6 GiB jobs.
const FORTY_GIB_KB: u64 = 40 * 1024 * 1024;

#[test]
fn the_shipped_mutation_gate_guards_on_rigger_run_base_never_a_merge_base() {
    let script = std::fs::read_to_string(gate_script()).expect("the shipped gate script");
    assert!(
        script.contains("test -n \"$RIGGER_RUN_BASE\""),
        "the shipped mutation gate must guard on RIGGER_RUN_BASE before diffing"
    );
    assert!(
        script.contains("git diff \"$RIGGER_RUN_BASE\""),
        "the shipped mutation gate must diff against RIGGER_RUN_BASE"
    );
    assert!(
        !script.contains("merge-base"),
        "the shipped mutation gate must never recompute a merge-base with the run branch - it \
         collapses to HEAD once the checkin worktree branches off the run branch \
         (adv-u91c2-mutation-gate-diff-base-collapses-to-empty)"
    );
}

#[test]
fn mutation_gate_diffs_against_rigger_run_base_capturing_the_whole_spec_diff() {
    let repo = tempfile::tempdir().unwrap();
    let dir = repo.path();
    git_ok(dir, &["init", "-q"]);
    git_ok(dir, &["config", "user.email", "t@example.com"]);
    git_ok(dir, &["config", "user.name", "t"]);
    std::fs::write(dir.join("a.rs"), "fn a() {}\n").unwrap();
    git_commit_all(dir, "origin");

    // The run branch anchors HERE - RIGGER_RUN_BASE is stamped from this exact tip.
    git_ok(dir, &["checkout", "-q", "-b", "rigger-run"]);
    let base_tip = git_out(dir, &["rev-parse", "HEAD"]);

    // Two implement units land on rigger-run BEFORE the checkin stage's worktree exists.
    std::fs::write(dir.join("a.rs"), "fn a() { 1; }\n").unwrap();
    git_commit_all(dir, "unit one lands");
    std::fs::write(dir.join("b.rs"), "fn b() {}\n").unwrap();
    git_commit_all(dir, "unit two lands");

    // The checkin stage's OWN worktree branches off rigger-run's tip after both landed.
    git_ok(dir, &["checkout", "-q", "-b", "checkin-worktree"]);
    let head = git_out(dir, &["rev-parse", "HEAD"]);
    let merge_base = git_out(dir, &["merge-base", "rigger-run", "HEAD"]);
    assert_eq!(
        merge_base, head,
        "fixture precondition: the checkin worktree's merge-base with rigger-run must already \
         equal HEAD - the topology a merge-base diff silently sweeps nothing against"
    );

    let run = run_gate(dir, Some(&base_tip), FORTY_GIB_KB, true);
    assert!(
        run.passed,
        "the gate must pass on an all-caught sweep: {}",
        run.output
    );

    let produced =
        std::fs::read_to_string(dir.join("unit.diff")).expect("unit.diff must be written");
    let expected = git_out(dir, &["diff", &base_tip, "--", "*.rs"]);
    assert!(
        !expected.is_empty(),
        "fixture precondition: the whole spec diff must itself be non-empty"
    );
    assert_eq!(
        produced.trim_end(),
        expected.trim_end(),
        "RIGGER_RUN_BASE must diff against the run's ACTUAL starting tip, capturing the whole \
         spec diff across every implement unit"
    );
}

#[test]
fn mutation_gate_refuses_loud_when_rigger_run_base_is_unset_rather_than_sweeping_an_empty_diff() {
    let repo = tempfile::tempdir().unwrap();
    let dir = repo.path();
    init_repo(dir);
    std::fs::write(dir.join("a.rs"), "fn a() {}\n").unwrap();
    git_commit_all(dir, "origin");

    let run = run_gate(dir, None, FORTY_GIB_KB, true);
    assert!(
        !run.passed,
        "with no RIGGER_RUN_BASE the gate must fail loud, never sweep an empty diff: {}",
        run.output
    );
    assert!(
        !dir.join("unit.diff").exists(),
        "a refused gate must never even attempt the git diff"
    );
    assert!(
        run.cargo.is_empty(),
        "a refused gate launches no sweep: {}",
        run.cargo
    );
}

#[test]
fn the_sweep_mutates_the_workspace_and_tests_only_the_touched_packages_plus_the_root() {
    let repo = tempfile::tempdir().unwrap();
    let base = workspace_repo(repo.path());
    let run = run_gate(repo.path(), Some(&base), FORTY_GIB_KB, true);
    assert!(run.passed, "{}", run.output);
    let sweep = run.sweep_line();
    assert!(
        sweep.contains("--workspace "),
        "the root manifest is a package, so only --workspace lets --in-diff see crates/: {sweep}"
    );
    assert!(
        sweep.contains("--test-package alpha ") && sweep.contains("--test-package fixture-root "),
        "each mutant runs the touched package's tests plus the root package's: {sweep}"
    );
    assert!(
        !sweep.contains("beta"),
        "an untouched package's tests are never run for this diff's mutants: {sweep}"
    );
    assert!(
        !sweep.contains("--test-workspace"),
        "the sweep never widens to every package's tests: {sweep}"
    );
    assert!(sweep.contains("--test-tool nextest "), "{sweep}");
    assert!(
        sweep.contains("--timeout 300 ") && !sweep.contains("--timeout-multiplier"),
        "the baseline tests only the mutated packages, so each mutant's bound is absolute: {sweep}"
    );
    assert!(
        !sweep.contains("--baseline skip"),
        "nothing proves the scoped packages green on this tree, so the baseline stays on: {sweep}"
    );
}

#[test]
fn the_sweep_runs_in_its_own_scope_bounded_by_half_of_mem_available() {
    let repo = tempfile::tempdir().unwrap();
    let base = workspace_repo(repo.path());

    let run = run_gate(repo.path(), Some(&base), FORTY_GIB_KB, true);
    assert!(run.passed, "{}", run.output);
    let bounded = run
        .scope
        .lines()
        .find(|l| l.contains("cargo mutants"))
        .unwrap_or_else(|| {
            panic!(
                "the sweep must launch through systemd-run; saw:\n{}",
                run.scope
            )
        });
    assert!(
        bounded.starts_with("--user --scope ")
            && bounded.contains("-p MemoryMax=20971520K -- env -u CARGO_TARGET_DIR cargo mutants "),
        "a transient user scope bounded to half of the 40 GiB available: {bounded}"
    );
    assert!(
        run.sweep_line().contains("-j 3 "),
        "20 GiB / 6 GiB per job = 3: {}",
        run.sweep_line()
    );

    // 8 GiB available: a 4 GiB bound holds less than one 6 GiB job, and the floor is one.
    let repo = tempfile::tempdir().unwrap();
    let base = workspace_repo(repo.path());
    let run = run_gate(repo.path(), Some(&base), 8 * 1024 * 1024, true);
    assert!(run.passed, "{}", run.output);
    assert!(
        run.scope.contains("-p MemoryMax=4194304K "),
        "{}",
        run.scope
    );
    assert!(run.sweep_line().contains("-j 1 "), "{}", run.sweep_line());
}

#[test]
fn without_systemd_run_the_sweep_runs_unbounded_and_the_gate_says_so() {
    let repo = tempfile::tempdir().unwrap();
    let base = workspace_repo(repo.path());
    let run = run_gate(repo.path(), Some(&base), FORTY_GIB_KB, false);
    assert!(run.passed, "{}", run.output);
    assert!(
        run.output.contains("advisory") && run.output.contains("WITHOUT its own memory bound"),
        "the fallback names what the operator loses: {}",
        run.output
    );
    assert!(
        run.scope.is_empty(),
        "no scope wrapper exists here: {}",
        run.scope
    );
    assert!(run.sweep_line().contains("-j 3 "), "{}", run.sweep_line());
}

#[test]
fn the_gate_refuses_every_narrowing_token_before_any_sweep() {
    // Built from parts so this file's own diff never carries a token the gate refuses.
    let keys = [
        concat!("exclude", "_re"),
        concat!("examine", "_re"),
        concat!("exclude", "_globs"),
        concat!("examine", "_globs"),
    ];
    let skip = concat!("mutants::", "skip");
    for token in keys.iter().copied().chain([skip]) {
        let repo = tempfile::tempdir().unwrap();
        let base = workspace_repo(repo.path());
        if token == skip {
            write(
                repo.path(),
                "crates/alpha/src/lib.rs",
                &format!("#[{token}]\npub fn f(a: u8, b: u8) -> u8 {{\n    a * b\n}}\n"),
            );
        } else {
            write(
                repo.path(),
                ".cargo/mutants.toml",
                &format!("{token} = [\"f\"]\n"),
            );
        }
        git_commit_all(repo.path(), "unit narrows the sweep");
        let run = run_gate(repo.path(), Some(&base), FORTY_GIB_KB, true);
        assert!(!run.passed, "`{token}` must fail the gate: {}", run.output);
        assert!(
            run.output.contains("the gate owns its instrument"),
            "`{token}`: the refusal names why: {}",
            run.output
        );
        assert!(
            !run.cargo.contains("mutants"),
            "`{token}`: no sweep may run once the unit narrowed it: {}",
            run.cargo
        );
    }
}
