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
//! THE REAPER ENDS A MUTANT, NEVER THE SWEEP (gap 100). The scope carries
//! `OOMPolicy=continue`, so a process the kernel's OOM reaper ends inside it is that one
//! mutant's outcome; each copy runs its share of the cores as nextest test threads, so the
//! sweep's total in-flight test processes stay at the core count whatever `-j` is. A mutant
//! whose TEST phase was ended by a signal is a detection, like a timeout; one whose BUILD
//! phase was (cargo-mutants files a signal-ended compiler as unviable and exits 0 either way)
//! fails the gate by name as an environment failure.
//!
//! THE ANCHOR IS THIS SPEC'S OWN. The incremental anchor a sweep leaves under the scratch root
//! records the `$RIGGER_RUN_BASE` that sweep was given, and it narrows the next sweep to the diff
//! since its tip only when that recorded base is this run's and HEAD holds the tip. Any other
//! anchor narrows nothing - a previous spec's, whether its tip is behind, at or past the run base
//! (an escalated check-in the run branch merged after this run started), one that records no
//! base, one HEAD no longer holds: the gate sweeps the whole spec diff and re-runs none of its
//! misses. A run base git cannot resolve fails the gate before anything runs.
//!
//! THE GATE OWNS ITS INSTRUMENT. A unit diff that adds an exclusion or examine key to
//! `.cargo/mutants.toml`, or a cargo-mutants skip attribute, fails before any sweep.

mod common;

use common::fixtures::write_file;
use common::git::{git_commit_all, git_ok, git_out, init_repo, run_git};
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
    "env", "xargs", "grep", "true", "nproc",
];

/// This machine's core count, as the gate reads it.
fn cores() -> u64 {
    let out = Command::new("nproc").output().expect("run nproc");
    String::from_utf8_lossy(&out.stdout)
        .trim()
        .parse()
        .expect("nproc prints a number")
}

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
    run_gate_with(repo, base, mem_available_kb, with_systemd_run, &[])
}

/// [`run_gate`] with extra environment for the fixture tools.
fn run_gate_with(
    repo: &Path,
    base: Option<&str>,
    mem_available_kb: u64,
    with_systemd_run: bool,
    env: &[(&str, &str)],
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
        .env_remove("CARGO_TARGET_DIR")
        .envs(env.iter().copied());
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
    write_file(&repo.join(rel), content.as_bytes());
}

/// The `unit.diff` the gate wrote in `repo`: the diff its main sweep covered.
fn unit_diff(repo: &Path) -> String {
    std::fs::read_to_string(repo.join("unit.diff")).expect("unit.diff must be written")
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

/// 40 GiB available: a 20 GiB bound, four 5 GiB jobs, capped at three.
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
    // The run branch anchors at `origin` - RIGGER_RUN_BASE is stamped from that exact tip - and
    // two implement units (the a.rs change, then b.rs) land on it BEFORE the checkin stage's
    // worktree exists.
    let [base_tip, _unit_one, _unit_two] = three_commit_history(dir);
    git_ok(dir, &["branch", "rigger-run"]);

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

    let produced = unit_diff(dir);
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

/// Three commits on one line - `origin`, a change to `a.rs`, then a new `b.rs` - returned in
/// that order: the history every anchor case below picks its run base and its anchor from.
fn three_commit_history(repo: &Path) -> [String; 3] {
    init_repo(repo);
    write(repo, "a.rs", "fn a() {}\n");
    git_commit_all(repo, "origin");
    let origin = git_out(repo, &["rev-parse", "HEAD"]);
    write(repo, "a.rs", "fn a() { 1; }\n");
    git_commit_all(repo, "a.rs changes");
    let middle = git_out(repo, &["rev-parse", "HEAD"]);
    write(repo, "b.rs", "fn b() {}\n");
    git_commit_all(repo, "b.rs lands");
    let head = git_out(repo, &["rev-parse", "HEAD"]);
    [origin, middle, head]
}

/// The mutant an earlier sweep left unclosed in its anchor.
const ANCHOR_MISS: &str = "a.rs:1:11: replace a with ()";

/// Run the shipped gate in `repo` against `base`, with the incremental anchor an earlier sweep
/// left under the scratch root: `tip` is the tree it examined, [`ANCHOR_MISS`] its one miss and
/// `recorded_base` the run base it was given (`None`: an anchor that records no base).
/// Returns the run and the scratch root, so a test reads back the anchor this sweep left.
fn run_gate_over_anchor(
    repo: &Path,
    base: &str,
    tip: &str,
    recorded_base: Option<&str>,
) -> (GateRun, tempfile::TempDir) {
    let scratch = tempfile::tempdir().unwrap();
    write(scratch.path(), "mutation-anchor/tip", &format!("{tip}\n"));
    write(
        scratch.path(),
        "mutation-anchor/missed.txt",
        &format!("{ANCHOR_MISS}\n"),
    );
    if let Some(recorded) = recorded_base {
        write(
            scratch.path(),
            "mutation-anchor/base",
            &format!("{recorded}\n"),
        );
    }
    let mutants = scratch.path().join("cargo-mutants-checkin");
    let run = run_gate_with(
        repo,
        Some(base),
        FORTY_GIB_KB,
        true,
        &[("MUTANTS", mutants.to_str().unwrap())],
    );
    (run, scratch)
}

/// The incremental anchor left under the scratch root `scratch`: its tip, its misses and the run
/// base it records, as written.
fn anchor_left(scratch: &Path) -> (String, String, String) {
    let anchor = scratch.join("mutation-anchor");
    let read = |fact: &str| {
        std::fs::read_to_string(anchor.join(fact))
            .unwrap_or_else(|e| panic!("the anchor must hold `{fact}`: {e}"))
    };
    (read("tip"), read("missed.txt"), read("base"))
}

#[test]
fn an_anchor_at_or_behind_the_run_base_is_a_previous_specs_so_the_whole_spec_diff_is_swept() {
    let repo = tempfile::tempdir().unwrap();
    let [origin, base, head] = three_commit_history(repo.path());
    let spec_diff = git_out(repo.path(), &["diff", &base, "--", "*.rs"]);
    assert_eq!(
        spec_diff
            .lines()
            .filter(|l| l.starts_with("+++ "))
            .collect::<Vec<_>>(),
        vec!["+++ b/b.rs"],
        "fixture precondition: the spec diff is b.rs alone, and a.rs's change predates the run"
    );
    // A previous spec's sweeps, of a run that started from `origin` and so record it: `older`
    // examined that run's own starting tree, `equal` exactly the tree this run started from.
    for (case, tip) in [("older", &origin), ("equal", &base)] {
        let (run, scratch) = run_gate_over_anchor(repo.path(), &base, tip, Some(&origin));
        assert!(run.passed, "{case}: {}", run.output);
        let produced = unit_diff(repo.path());
        assert_eq!(
            produced.trim_end(),
            spec_diff.trim_end(),
            "{case}: an anchor at or behind RIGGER_RUN_BASE examined no commit of this spec, so \
             the sweep is the whole spec diff - never every change since a previous spec's sweep"
        );
        assert_eq!(
            run.cargo.lines().collect::<Vec<_>>(),
            vec![run.sweep_line()],
            "{case}: a previous spec's misses are not this spec's - the one sweep of unit.diff \
             runs and no by-name rerun is even listed"
        );
        assert_eq!(
            anchor_left(scratch.path()),
            (format!("{head}\n"), String::new(), format!("{base}\n")),
            "{case}: the anchor this sweep leaves is this spec's own tree with this sweep's misses, \
             recording this run's base"
        );
    }
}

#[test]
fn an_anchor_of_this_run_that_head_holds_is_this_specs_own_so_the_re_sweep_starts_from_it() {
    let repo = tempfile::tempdir().unwrap();
    let [origin, anchor, _head] = three_commit_history(repo.path());
    let since_anchor = git_out(repo.path(), &["diff", &anchor, "--", "*.rs"]);
    assert_ne!(
        since_anchor,
        git_out(repo.path(), &["diff", &origin, "--", "*.rs"]),
        "fixture precondition: the diff since the anchor is narrower than the spec diff"
    );
    let (run, _scratch) = run_gate_over_anchor(repo.path(), &origin, &anchor, Some(&origin));
    assert!(run.passed, "{}", run.output);
    let produced = unit_diff(repo.path());
    assert_eq!(
        produced.trim_end(),
        since_anchor.trim_end(),
        "an anchor recording RIGGER_RUN_BASE whose tip HEAD holds is an earlier sweep of this \
         run, so the re-sweep covers the diff since it"
    );
    let cargo = run.cargo.lines().collect::<Vec<_>>();
    assert_eq!(
        cargo.len(),
        2,
        "the sweep, then the listing that resolves the earlier miss by name: {}",
        run.cargo
    );
    assert_eq!(
        cargo[1], "mutants --list --workspace -F a\\.rs(:[0-9]+:[0-9]+)?: replace a with \\(\\) ",
        "this spec's own earlier miss is re-run by name"
    );
}

#[test]
fn an_anchor_of_this_run_that_head_does_not_hold_narrows_nothing() {
    let repo = tempfile::tempdir().unwrap();
    let [origin, middle, head] = three_commit_history(repo.path());
    // A checkin attempt rewritten away: a commit of this spec (past the run base) that the
    // current HEAD no longer holds, its tree the one `middle` carries.
    let tree = format!("{middle}^{{tree}}");
    let rewritten = git_out(
        repo.path(),
        &[
            "commit-tree",
            &tree,
            "-p",
            &origin,
            "-m",
            "rewritten attempt",
        ],
    );
    let count = |range: String| git_out(repo.path(), &["rev-list", "--count", &range]);
    assert_eq!(
        (
            count(format!("{origin}..{rewritten}")),
            count(format!("HEAD..{rewritten}")),
        ),
        ("1".to_string(), "1".to_string()),
        "fixture precondition: the anchor is past the run base and is not an ancestor of HEAD"
    );
    let spec_diff = git_out(repo.path(), &["diff", &origin, "--", "*.rs"]);
    assert_ne!(
        spec_diff,
        git_out(repo.path(), &["diff", &rewritten, "--", "*.rs"]),
        "fixture precondition: the diff since the anchor is narrower than the spec diff"
    );
    let (run, scratch) = run_gate_over_anchor(repo.path(), &origin, &rewritten, Some(&origin));
    assert!(run.passed, "{}", run.output);
    let produced = unit_diff(repo.path());
    assert_eq!(
        produced.trim_end(),
        spec_diff.trim_end(),
        "an anchor HEAD does not hold narrows nothing, even one recording RIGGER_RUN_BASE: the \
         sweep is the whole spec diff"
    );
    assert_eq!(
        run.cargo.lines().collect::<Vec<_>>(),
        vec![run.sweep_line()],
        "a discarded anchor's misses are not re-run by name"
    );
    assert_eq!(
        anchor_left(scratch.path()),
        (format!("{head}\n"), String::new(), format!("{origin}\n")),
        "the anchor this sweep leaves is the tree it swept with this sweep's misses, recording \
         this run's base"
    );
}

#[test]
fn a_previous_specs_tip_the_run_branch_merged_after_the_run_base_narrows_nothing() {
    let repo = tempfile::tempdir().unwrap();
    let dir = repo.path();
    // A previous spec's run started from `origin` and landed the a.rs change; this run started
    // from `base`, and its unit landed b.rs.
    let [origin, base, _unit] = three_commit_history(dir);
    // That spec's check-in tip, one commit off this run's base: its sweep left the anchor, it
    // escalated, and it was landed by hand onto the run branch during this run.
    git_ok(dir, &["checkout", "-q", &base]);
    write(dir, "c.rs", "fn c() {}\n");
    git_commit_all(dir, "the previous spec's check-in");
    let previous_tip = git_out(dir, &["rev-parse", "HEAD"]);
    git_ok(dir, &["checkout", "-q", "-"]);
    git_ok(dir, &["merge", "-q", "--no-edit", &previous_tip]);
    let head = git_out(dir, &["rev-parse", "HEAD"]);
    let count = |range: String| git_out(dir, &["rev-list", "--count", &range]);
    assert_eq!(
        (
            count(format!("HEAD..{previous_tip}")),
            count(format!("{base}..{previous_tip}")),
        ),
        ("0".to_string(), "1".to_string()),
        "fixture precondition: HEAD holds the previous spec's tip and it is past the run base, so \
         ancestry alone reads it as this spec's own"
    );
    let spec_diff = git_out(dir, &["diff", &base, "--", "*.rs"]);
    assert_eq!(
        spec_diff
            .lines()
            .filter(|l| l.starts_with("+++ "))
            .collect::<Vec<_>>(),
        vec!["+++ b/b.rs", "+++ b/c.rs"],
        "fixture precondition: the spec diff is b.rs and the merged c.rs; the anchor's miss in \
         a.rs is outside it"
    );
    // The previous spec's anchor records its own run's base; one written before the gate
    // recorded a base records none.
    for (case, recorded_base) in [
        ("the previous run's base", Some(origin.as_str())),
        ("no recorded base", None),
    ] {
        let (run, scratch) = run_gate_over_anchor(dir, &base, &previous_tip, recorded_base);
        assert!(run.passed, "{case}: {}", run.output);
        assert_eq!(
            unit_diff(dir).trim_end(),
            spec_diff.trim_end(),
            "{case}: an anchor recording no base of this run narrows nothing, wherever its tip \
             sits: the sweep is the whole spec diff"
        );
        assert_eq!(
            run.cargo.lines().collect::<Vec<_>>(),
            vec![run.sweep_line()],
            "{case}: the previous spec's miss is not this spec's - the one sweep of unit.diff runs \
             and no by-name rerun is even listed"
        );
        assert_eq!(
            anchor_left(scratch.path()),
            (format!("{head}\n"), String::new(), format!("{base}\n")),
            "{case}: the anchor this sweep leaves is this spec's own tree with this sweep's \
             misses, recording this run's base"
        );
    }
}

#[test]
fn a_run_base_naming_no_commit_here_fails_the_gate_and_never_narrows_to_the_anchor() {
    let repo = tempfile::tempdir().unwrap();
    let [_origin, middle, _head] = three_commit_history(repo.path());
    let unknown_base = "0123456789abcdef0123456789abcdef01234567";
    assert!(
        !run_git(
            repo.path(),
            &["cat-file", "-e", &format!("{unknown_base}^{{commit}}")]
        )
        .status
        .success(),
        "fixture precondition: the run base names no commit in this repository"
    );
    let (run, scratch) =
        run_gate_over_anchor(repo.path(), unknown_base, &middle, Some(unknown_base));
    assert!(
        !run.passed,
        "a run base git cannot resolve leaves no spec diff to sweep, and an anchor HEAD holds \
         that records the same base never stands in for it: {}",
        run.output
    );
    assert!(
        run.output.contains(&format!(
            "mutation gate: RIGGER_RUN_BASE {unknown_base} names no commit in this repository"
        )),
        "the refusal names the base it cannot resolve: {}",
        run.output
    );
    assert_eq!(
        (run.cargo.as_str(), run.scope.as_str()),
        ("", ""),
        "the gate launches nothing once the spec diff cannot be taken"
    );
    assert_eq!(
        anchor_left(scratch.path()),
        (
            format!("{middle}\n"),
            format!("{ANCHOR_MISS}\n"),
            format!("{unknown_base}\n")
        ),
        "a failed gate leaves the earlier anchor exactly as it was"
    );
}

#[test]
fn a_run_base_naming_an_object_that_is_no_commit_fails_the_gate_before_anything_runs() {
    let repo = tempfile::tempdir().unwrap();
    let dir = repo.path();
    three_commit_history(dir);
    for (kind, object) in [("tree", "HEAD^{tree}"), ("blob", "HEAD:a.rs")] {
        let base = git_out(dir, &["rev-parse", object]);
        assert_eq!(
            git_out(dir, &["cat-file", "-t", &base]),
            kind,
            "fixture precondition: the run base names a {kind} this repository holds"
        );
        let run = run_gate(dir, Some(&base), FORTY_GIB_KB, true);
        assert!(
            !run.passed,
            "{kind}: a run base that is no commit has no spec diff to sweep: {}",
            run.output
        );
        assert!(
            run.output.contains(&format!(
                "mutation gate: RIGGER_RUN_BASE {base} names no commit in this repository"
            )),
            "{kind}: the refusal names the base it cannot take as a commit: {}",
            run.output
        );
        assert!(
            !dir.join("unit.diff").exists(),
            "{kind}: the refusal comes before the gate takes any diff"
        );
        assert_eq!(
            (run.cargo.as_str(), run.scope.as_str()),
            ("", ""),
            "{kind}: the gate launches nothing once the run base is refused"
        );
    }
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
        "20 GiB / 5 GiB per job = 4, capped at 3: {}",
        run.sweep_line()
    );

    // 20 GiB available: a 10 GiB bound holds two 5 GiB jobs (the measured per-copy figure).
    let repo = tempfile::tempdir().unwrap();
    let base = workspace_repo(repo.path());
    let run = run_gate(repo.path(), Some(&base), 20 * 1024 * 1024, true);
    assert!(run.passed, "{}", run.output);
    assert!(run.sweep_line().contains("-j 2 "), "{}", run.sweep_line());

    // 8 GiB available: a 4 GiB bound holds less than one 5 GiB job, and the floor is one.
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

#[test]
fn the_sweep_scope_lets_the_reaper_end_one_mutant_never_the_whole_sweep() {
    let repo = tempfile::tempdir().unwrap();
    let base = workspace_repo(repo.path());
    let run = run_gate(repo.path(), Some(&base), FORTY_GIB_KB, true);
    assert!(run.passed, "{}", run.output);
    let bounded = run
        .scope
        .lines()
        .find(|l| l.contains("cargo mutants"))
        .unwrap_or_else(|| panic!("the sweep must launch through systemd-run: {}", run.scope));
    assert!(
        bounded.contains(" -p OOMPolicy=continue "),
        "systemd's default OOMPolicy=stop ends the whole scope when the reaper ends one process \
         in it; the sweep's scope must continue: {bounded}"
    );
}

#[test]
fn each_copy_runs_its_share_of_the_cores_so_total_test_fanout_is_constant() {
    let cores = cores();
    for (avail_kb, jobs) in [
        (FORTY_GIB_KB, 3),
        (20 * 1024 * 1024, 2),
        (8 * 1024 * 1024, 1),
    ] {
        let repo = tempfile::tempdir().unwrap();
        let base = workspace_repo(repo.path());
        let run = run_gate(repo.path(), Some(&base), avail_kb, true);
        assert!(run.passed, "{}", run.output);
        let threads = (cores / jobs).max(1);
        let sweep = run.sweep_line();
        assert!(
            sweep.contains(&format!("-j {jobs} "))
                && sweep
                    .trim_end()
                    .ends_with(&format!(" -- --test-threads {threads}")),
            "{jobs} copies x {threads} nextest threads each stay at the {cores} cores whatever -j \
             is; the bound goes after `--`, which cargo-mutants hands to the test phase only: \
             {sweep}"
        );
    }
}

#[test]
fn a_test_phase_ended_by_a_signal_is_a_detection_like_a_timeout() {
    let repo = tempfile::tempdir().unwrap();
    let base = workspace_repo(repo.path());
    let run = run_gate_with(
        repo.path(),
        Some(&base),
        FORTY_GIB_KB,
        true,
        &[("RIGGER_FIXTURE_ENDED", "test")],
    );
    assert!(
        run.passed,
        "the mutant made its tests grow until the reaper ended them - a detection: {}",
        run.output
    );
    assert!(
        run.output.contains(
            "crates/alpha/src/lib.rs:2:5: replace f -> u8 with 0 - its test phase ended on signal 9"
        ) && run.output.contains("counted as a detection"),
        "the gate names the mutant it counted: {}",
        run.output
    );
}

#[test]
fn a_build_ended_by_a_signal_fails_the_gate_by_name_as_an_environment_failure() {
    for mode in ["build", "rustc"] {
        let repo = tempfile::tempdir().unwrap();
        let base = workspace_repo(repo.path());
        let run = run_gate_with(
            repo.path(),
            Some(&base),
            FORTY_GIB_KB,
            true,
            &[("RIGGER_FIXTURE_ENDED", mode)],
        );
        assert!(
            !run.passed,
            "`{mode}`: a mutant whose build was ended was never tested - never a silent pass: {}",
            run.output
        );
        assert!(
            run.output.contains(
                "error[mutation]: ENDED crates/alpha/src/lib.rs:2:5: replace f -> u8 with 0 - its build phase ended on signal 9"
            ) && run.output.contains("environment failure"),
            "`{mode}`: the verdict names the mutant, the phase and the cause: {}",
            run.output
        );
    }
}
