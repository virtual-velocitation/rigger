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
//! THE ANCHOR IS THIS SPEC'S OWN; ITS CATCHES ARE THE PROJECT'S. The incremental anchor a sweep
//! leaves under the scratch root records the `$RIGGER_RUN_BASE` that sweep was given, and it
//! narrows the next sweep to the diff since its tip, re-running its misses by name, only when that
//! recorded base is this run's and HEAD holds the tip. Any other anchor narrows nothing - a
//! previous spec's, whether its tip is behind, at or past the run base (an escalated check-in the
//! run branch merged after this run started), one that records no base, one HEAD no longer holds
//! or whose tip names no object here: the gate sweeps the whole spec diff and re-runs none of its
//! misses. Its catch map is read whichever run wrote it: every catch whose catching test changed
//! since the owned tip, else since the run base, is re-run by name. Every entry for a mutant this
//! sweep did not examine is carried forward, whichever run recorded it; a re-run catch is
//! re-recorded, and dropped when it now survives. A run base git cannot resolve fails the gate
//! before anything runs.
//!
//! THE GATE OWNS ITS INSTRUMENT. A unit diff that adds an exclusion or examine key to
//! `.cargo/mutants.toml`, or a cargo-mutants skip attribute, fails before any sweep.

mod common;

use common::fixtures::write_file;
use common::git::{git_answer, git_commit_all, git_ok, git_out, init_repo};
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

/// The gate refused before anything ran in `repo`: it failed, took no diff and launched nothing.
fn assert_refused_before_anything_ran(repo: &Path, run: &GateRun, case: &str) {
    assert!(
        !run.passed,
        "{case}: the gate must refuse loud: {}",
        run.output
    );
    assert!(
        !repo.join("unit.diff").exists(),
        "{case}: a refused gate never takes the diff"
    );
    assert_eq!(
        (run.cargo.as_str(), run.scope.as_str()),
        ("", ""),
        "{case}: a refused gate launches nothing"
    );
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
    assert_refused_before_anything_ran(dir, &run, "no RIGGER_RUN_BASE, so no spec diff to sweep");
}

/// Write each `(path, content)` of `files` into `repo`, commit everything as `message`, and return
/// the new HEAD.
fn commit_files(repo: &Path, files: &[(&str, &str)], message: &str) -> String {
    for (rel, content) in files {
        write(repo, rel, content);
    }
    git_commit_all(repo, message);
    git_out(repo, &["rev-parse", "HEAD"])
}

/// Three commits on one line - `origin`, a change to `a.rs`, then a new `b.rs` - returned in
/// that order: the history every anchor case below picks its run base and its anchor from.
fn three_commit_history(repo: &Path) -> [String; 3] {
    init_repo(repo);
    [
        commit_files(repo, &[("a.rs", "fn a() {}\n")], "origin"),
        commit_files(repo, &[("a.rs", "fn a() { 1; }\n")], "a.rs changes"),
        commit_files(repo, &[("b.rs", "fn b() {}\n")], "b.rs lands"),
    ]
}

/// The catch map an earlier sweep recorded, sorted as the gate writes it: per caught mutant, the
/// test binary that failed on it first - `foo` for the one in `x.rs`, `bar` for the one in `y.rs`.
const CAUGHT_BY_FOO_AND_BAR: &str =
    "x.rs:1:4: replace x with ()\tfoo\ny.rs:1:4: replace y with ()\tbar\n";

/// [`three_commit_history`], then `x.rs` and `y.rs` landing with `tests/foo.rs` and
/// `tests/bar.rs`, the binaries that catch their mutants, then a rewrite of `tests/foo.rs` beside
/// `c.rs`, then a rewrite of `tests/bar.rs` beside `d.rs`. Returns the history's first commit,
/// the landing, the foo rewrite and the bar rewrite (HEAD); each arm says which one its run
/// starts from.
fn catch_history(repo: &Path) -> [String; 4] {
    let [origin, _, _] = three_commit_history(repo);
    [
        origin,
        commit_files(
            repo,
            &[
                ("x.rs", "fn x() {}\n"),
                ("y.rs", "fn y() {}\n"),
                ("tests/foo.rs", "#[test]\nfn foo() {}\n"),
                ("tests/bar.rs", "#[test]\nfn bar() {}\n"),
            ],
            "x.rs and y.rs land with the tests that catch their mutants",
        ),
        commit_files(
            repo,
            &[
                ("tests/foo.rs", "#[test]\nfn foo() {\n    x();\n}\n"),
                ("c.rs", "fn c() {}\n"),
            ],
            "tests/foo.rs is rewritten",
        ),
        commit_files(
            repo,
            &[
                ("tests/bar.rs", "#[test]\nfn bar() {\n    y();\n}\n"),
                ("d.rs", "fn d() {}\n"),
            ],
            "tests/bar.rs is rewritten",
        ),
    ]
}

/// The mutant an earlier sweep left unclosed in its anchor.
const ANCHOR_MISS: &str = "a.rs:1:11: replace a with ()";

/// A sha that names no object in any fixture repository here.
const UNKNOWN_SHA: &str = "0123456789abcdef0123456789abcdef01234567";

/// Run the shipped gate in `repo` against `base`, with the incremental anchor an earlier sweep
/// left under the scratch root: `tip` is the tree it examined, [`ANCHOR_MISS`] its one miss,
/// `recorded_base` the run base it was given (`None`: an anchor that records no base) and
/// `caught` its catch map (`caught.map` lines, empty for none).
/// Returns the run and the scratch root, so a test reads back the anchor this sweep left.
fn run_gate_over_anchor(
    repo: &Path,
    base: &str,
    tip: &str,
    recorded_base: Option<&str>,
    caught: &str,
) -> (GateRun, tempfile::TempDir) {
    let scratch = tempfile::tempdir().unwrap();
    write(scratch.path(), "mutation-anchor/tip", &format!("{tip}\n"));
    write(
        scratch.path(),
        "mutation-anchor/missed.txt",
        &format!("{ANCHOR_MISS}\n"),
    );
    write(scratch.path(), "mutation-anchor/caught.map", caught);
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

/// The incremental anchor left under the scratch root `scratch`: its four facts as written - its
/// tip, its misses, the run base it records and its catch map.
fn anchor_left(scratch: &Path) -> (String, String, String, String) {
    let anchor = scratch.join("mutation-anchor");
    let read = |fact: &str| {
        std::fs::read_to_string(anchor.join(fact))
            .unwrap_or_else(|e| panic!("the anchor must hold `{fact}`: {e}"))
    };
    (
        read("tip"),
        read("missed.txt"),
        read("base"),
        read("caught.map"),
    )
}

/// The gate passed, and the only `cargo` it ran was its sweep of `unit.diff`, followed - when it
/// re-runs mutants by name - by the one `--list` that resolves them (`listing`, its argv line).
fn assert_swept(run: &GateRun, listing: Option<&str>, case: &str) {
    assert!(run.passed, "{case}: {}", run.output);
    assert_eq!(
        run.cargo.lines().collect::<Vec<_>>(),
        [Some(run.sweep_line()), listing]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>(),
        "{case}: the one sweep of unit.diff runs, and a mutant is re-run by name only through \
         this listing"
    );
}

/// The gate narrowed nothing: it swept the whole spec diff in `repo` against the run base `base`
/// (passing, with `listing` after the sweep, as [`assert_swept`] reads it), and left under
/// `scratch` the anchor of the tree it swept - HEAD - with this sweep's misses, none, recording
/// `base`, its map still carrying `caught`.
fn assert_narrowed_nothing(
    repo: &Path,
    run: &GateRun,
    scratch: &Path,
    base: &str,
    caught: &str,
    listing: Option<&str>,
    case: &str,
) {
    assert_swept(run, listing, case);
    assert_eq!(
        unit_diff(repo).trim_end(),
        git_out(repo, &["diff", base, "--", "*.rs"]).trim_end(),
        "{case}: the sweep is the whole spec diff against the run base"
    );
    assert_eq!(
        anchor_left(scratch),
        (
            format!("{}\n", git_out(repo, &["rev-parse", "HEAD"])),
            String::new(),
            format!("{base}\n"),
            caught.to_string()
        ),
        "{case}: the anchor this sweep leaves is this spec's own tree with this sweep's misses, \
         recording this run's base, its map carrying every catch this sweep did not examine"
    );
}

#[test]
fn an_anchor_at_or_behind_the_run_base_is_a_previous_specs_so_the_whole_spec_diff_is_swept() {
    let repo = tempfile::tempdir().unwrap();
    let [origin, base, _head] = three_commit_history(repo.path());
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
    // An anchor at or behind RIGGER_RUN_BASE examined no commit of this spec, and its misses are
    // not this spec's.
    for (case, tip) in [("older", &origin), ("equal", &base)] {
        let (run, scratch) = run_gate_over_anchor(repo.path(), &base, tip, Some(&origin), "");
        assert_narrowed_nothing(repo.path(), &run, scratch.path(), &base, "", None, case);
    }
}

#[test]
fn an_anchor_of_this_run_that_head_holds_is_this_specs_own_so_the_re_sweep_starts_from_it() {
    let repo = tempfile::tempdir().unwrap();
    let dir = repo.path();
    // This run started from `base`; its last sweep examined the foo rewrite, and the bar rewrite
    // landed since.
    let [_origin, base, anchor, _head] = catch_history(dir);
    let since_anchor = git_out(dir, &["diff", &anchor, "--", "*.rs"]);
    assert_ne!(
        since_anchor,
        git_out(dir, &["diff", &base, "--", "*.rs"]),
        "fixture precondition: the diff since the anchor is narrower than the spec diff"
    );
    let (run, _scratch) =
        run_gate_over_anchor(dir, &base, &anchor, Some(&base), CAUGHT_BY_FOO_AND_BAR);
    assert_swept(
        &run,
        Some(
            "mutants --list --workspace -F a\\.rs(:[0-9]+:[0-9]+)?: replace a with \\(\\) \
             -F y\\.rs(:[0-9]+:[0-9]+)?: replace y with \\(\\) ",
        ),
        "this spec's own earlier miss is re-run by name, and so is the catch whose test changed \
         since the anchor's tip (bar's) - never the one whose test changed only before it (foo's)",
    );
    assert_eq!(
        unit_diff(dir).trim_end(),
        since_anchor.trim_end(),
        "an anchor recording RIGGER_RUN_BASE whose tip HEAD holds is an earlier sweep of this \
         run, so the re-sweep covers the diff since it"
    );
}

#[test]
fn a_catch_an_earlier_spec_recorded_is_re_run_by_name_when_this_spec_changes_its_test() {
    let repo = tempfile::tempdir().unwrap();
    let dir = repo.path();
    // A previous spec's run started from `origin`, and its last sweep examined the tree this run
    // starts from (`base`), recording a catch by foo and one by bar. This spec rewrites
    // tests/foo.rs beside its c.rs, and nothing else under tests/.
    let [origin, base, foo_rewrite, _] = catch_history(dir);
    git_ok(dir, &["checkout", "-q", &foo_rewrite]);
    assert_eq!(
        git_out(dir, &["diff", "--name-only", &base]),
        "c.rs\ntests/foo.rs",
        "fixture precondition: the spec diff rewrites tests/foo.rs and touches neither x.rs nor y.rs"
    );
    // The record is not this run's, so it narrows nothing and its miss is not re-run; its catches
    // are the project's whichever run recorded them, so the one whose catching test this spec
    // changed is re-run by name on this spec's first sweep, bar's is not, and the map is carried
    // whole.
    for (case, recorded_base) in [
        ("the previous run's base", Some(origin.as_str())),
        ("no recorded base", None),
    ] {
        let (run, scratch) =
            run_gate_over_anchor(dir, &base, &base, recorded_base, CAUGHT_BY_FOO_AND_BAR);
        assert_narrowed_nothing(
            dir,
            &run,
            scratch.path(),
            &base,
            CAUGHT_BY_FOO_AND_BAR,
            Some("mutants --list --workspace -F x\\.rs(:[0-9]+:[0-9]+)?: replace x with \\(\\) "),
            case,
        );
    }
}

#[test]
fn an_earlier_specs_catch_counts_test_changes_from_the_run_base_wherever_its_tip_sits() {
    let repo = tempfile::tempdir().unwrap();
    let dir = repo.path();
    // A previous spec's run started from `origin`, and its last sweep examined the tree where x.rs
    // and y.rs landed with their tests, recording a catch by foo and one by bar. tests/foo.rs was
    // rewritten after that sweep, and this run started from the foo rewrite; this spec rewrites
    // tests/bar.rs beside its d.rs.
    let [origin, landed, base, _head] = catch_history(dir);
    assert_eq!(
        (
            git_out(dir, &["diff", "--name-only", &landed, &base, "--", "tests"]),
            git_out(dir, &["diff", "--name-only", &base, "--", "tests"]),
            git_answer(dir, &["cat-file", "-t", UNKNOWN_SHA]),
        ),
        ("tests/foo.rs".to_string(), "tests/bar.rs".to_string(), None),
        "fixture precondition: tests/foo.rs changed between the record's tip and the run base, \
         this spec changes tests/bar.rs alone, and a pruned tip names no object here"
    );
    // The record is not this run's, so a catch's test change counts from the run base, never from
    // the record's tip. Wherever that tip sits, behind the run base or pruned from the repository,
    // the catch whose test this spec changed (bar's) is re-run by name, and the one whose test
    // changed only before this run started (foo's) is not.
    for (case, tip) in [
        ("a tip behind the run base", landed.as_str()),
        ("a pruned tip", UNKNOWN_SHA),
    ] {
        let (run, scratch) =
            run_gate_over_anchor(dir, &base, tip, Some(&origin), CAUGHT_BY_FOO_AND_BAR);
        assert_narrowed_nothing(
            dir,
            &run,
            scratch.path(),
            &base,
            CAUGHT_BY_FOO_AND_BAR,
            Some("mutants --list --workspace -F y\\.rs(:[0-9]+:[0-9]+)?: replace y with \\(\\) "),
            case,
        );
    }
}

#[test]
fn an_anchor_of_this_run_that_head_does_not_hold_narrows_nothing() {
    let repo = tempfile::tempdir().unwrap();
    let [origin, middle, _head] = three_commit_history(repo.path());
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
    assert_eq!(
        git_answer(repo.path(), &["cat-file", "-t", UNKNOWN_SHA]),
        None,
        "fixture precondition: a pruned anchor's tip names no object in this repository"
    );
    // An anchor HEAD does not hold narrows nothing, even one recording RIGGER_RUN_BASE - a
    // rewritten attempt's, or one whose tip was pruned - and its misses are not re-run by name.
    for (case, tip) in [
        ("a rewritten attempt", rewritten.as_str()),
        ("a pruned tip", UNKNOWN_SHA),
    ] {
        let (run, scratch) = run_gate_over_anchor(repo.path(), &origin, tip, Some(&origin), "");
        assert_narrowed_nothing(repo.path(), &run, scratch.path(), &origin, "", None, case);
    }
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
    let previous_tip = commit_files(
        dir,
        &[("c.rs", "fn c() {}\n")],
        "the previous spec's check-in",
    );
    git_ok(dir, &["checkout", "-q", "-"]);
    git_ok(dir, &["merge", "-q", "--no-edit", &previous_tip]);
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
    // recorded a base records none. Either narrows nothing wherever its tip sits, and the previous
    // spec's miss is not this spec's.
    for (case, recorded_base) in [
        ("the previous run's base", Some(origin.as_str())),
        ("no recorded base", None),
    ] {
        let (run, scratch) = run_gate_over_anchor(dir, &base, &previous_tip, recorded_base, "");
        assert_narrowed_nothing(dir, &run, scratch.path(), &base, "", None, case);
    }
}

#[test]
fn a_run_base_naming_no_commit_here_fails_the_gate_and_never_narrows_to_the_anchor() {
    let repo = tempfile::tempdir().unwrap();
    let dir = repo.path();
    let [_origin, middle, _head] = three_commit_history(dir);
    let object = |rev: &str| git_out(dir, &["rev-parse", rev]);
    for (case, base, kind) in [
        ("an unknown sha", UNKNOWN_SHA.to_string(), None),
        ("a tree", object("HEAD^{tree}"), Some("tree")),
        ("a blob", object("HEAD:a.rs"), Some("blob")),
    ] {
        assert_eq!(
            git_answer(dir, &["cat-file", "-t", &base]).as_deref(),
            kind,
            "{case}: fixture precondition: the run base names no commit in this repository"
        );
        // An anchor HEAD holds that records the same base never stands in for the spec diff.
        let (run, scratch) = run_gate_over_anchor(dir, &base, &middle, Some(&base), "");
        assert_refused_before_anything_ran(dir, &run, case);
        assert!(
            run.output.contains(&format!(
                "mutation gate: RIGGER_RUN_BASE {base} names no commit in this repository"
            )),
            "{case}: the refusal names the base it cannot take as a commit: {}",
            run.output
        );
        assert_eq!(
            anchor_left(scratch.path()),
            (
                format!("{middle}\n"),
                format!("{ANCHOR_MISS}\n"),
                format!("{base}\n"),
                String::new()
            ),
            "{case}: a refused gate leaves the earlier anchor exactly as it was"
        );
    }
}

#[test]
fn the_anchor_a_sweep_leaves_narrows_the_next_sweep_of_its_run_and_never_a_later_runs() {
    let repo = tempfile::tempdir().unwrap();
    let dir = repo.path();
    let [origin, middle, head] = three_commit_history(dir);
    let scratch = tempfile::tempdir().unwrap();
    let mutants = scratch.path().join("cargo-mutants-checkin");
    // One sweep against `base` at the current HEAD, sharing one scratch root with every other
    // sweep here, and the unit's own root reclaimed after it the way a reclaimed unit loses it,
    // so the anchor under the scratch root is the only state one sweep hands the next.
    let sweep = |base: &str| {
        let run = run_gate_with(
            dir,
            Some(base),
            FORTY_GIB_KB,
            true,
            &[("MUTANTS", mutants.to_str().unwrap())],
        );
        assert_swept(
            &run,
            None,
            "an all-caught sweep leaves no miss, so nothing is re-run by name",
        );
        std::fs::remove_dir_all(&mutants).unwrap();
        (unit_diff(dir), anchor_left(scratch.path()))
    };
    let diff_since = |from: &str| git_out(dir, &["diff", from, "--", "*.rs"]);

    // A run that started from `origin` sweeps its spec at `middle`, then again at `head`.
    git_ok(dir, &["checkout", "-q", &middle]);
    let (first, anchor) = sweep(&origin);
    assert_eq!(
        (first.trim_end(), anchor),
        (
            diff_since(&origin).trim_end(),
            (
                format!("{middle}\n"),
                String::new(),
                format!("{origin}\n"),
                String::new()
            )
        ),
        "the run's first sweep is its whole spec diff, and it leaves its tree recording its base"
    );
    git_ok(dir, &["checkout", "-q", "-"]);
    assert_ne!(
        diff_since(&middle),
        diff_since(&origin),
        "fixture precondition: the diff since the first sweep is narrower than the spec diff"
    );
    let (second, anchor) = sweep(&origin);
    assert_eq!(
        (second.trim_end(), anchor),
        (
            diff_since(&middle).trim_end(),
            (
                format!("{head}\n"),
                String::new(),
                format!("{origin}\n"),
                String::new()
            )
        ),
        "the same run's next sweep reads back the base its first sweep wrote, so it covers only \
         the changes since that sweep's tree"
    );

    // A later run recorded `middle` as its base before the earlier run's check-in tip `head`
    // reached the run branch by hand; its own unit lands on top, so HEAD holds that tip.
    // Committed alone: the earlier sweeps' outputs (unit.diff, mutants.out) stay untracked.
    write(dir, "c.rs", "fn c() {}\n");
    git_ok(dir, &["add", "c.rs"]);
    git_ok(dir, &["commit", "-q", "-m", "the later run's unit lands"]);
    let later_head = git_out(dir, &["rev-parse", "HEAD"]);
    assert_ne!(
        diff_since(&head),
        diff_since(&middle),
        "fixture precondition: the diff since the earlier run's tip is narrower than the later \
         run's spec diff"
    );
    let (later, anchor) = sweep(&middle);
    assert_eq!(
        (later.trim_end(), anchor),
        (
            diff_since(&middle).trim_end(),
            (
                format!("{later_head}\n"),
                String::new(),
                format!("{middle}\n"),
                String::new()
            )
        ),
        "the earlier run's anchor records another base, so the later run sweeps its whole spec \
         diff and leaves its own tree recording its own base"
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
