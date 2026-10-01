//! THE PRINCIPLE GATES ARE DEFINITION: the gates that hold every unit to the engineering
//! principles mechanically are declared in this repository's own `.rigger/workflow.yml` AND in
//! the workflow `rigger init` scaffolds for a consumer project, each listed on the stages it
//! guards. Both are loaded through the production parser (`rigger::config_store::load`), so a
//! gate that is merely mentioned in a comment, or declared but never listed on a stage, fails.

mod common;
use common::cli::{run_rigger, temp_project};
use common::git::{git_commit_all, git_ok, git_out, init_repo};
use common::repo::repo_root;
use std::collections::BTreeMap;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::Command;

/// One principle gate: its id, the stages that must list it, the text this repository's own
/// command for it must carry, and the gates it must run before wherever both are listed.
struct PrincipleGate {
    id: &'static str,
    stages: &'static [&'static str],
    repo_command: &'static str,
    precedes: &'static [&'static str],
}

const PRINCIPLE_GATES: &[PrincipleGate] = &[
    PrincipleGate {
        id: "boundary",
        stages: &["implement", "checkin"],
        repo_command: "cargo test --test boundary_audit",
        precedes: &[],
    },
    // The audit gate regenerates `docs/audit/*` before it asserts, so it must run before the
    // `test` gate, whose plain `cargo test` includes the audit's drift guards: a unit whose
    // only audit difference is a stale generated catalog is then never red.
    PrincipleGate {
        id: "audit",
        stages: &["implement", "checkin"],
        repo_command: "RIGGER_AUDIT_WRITE=1 cargo test --test simplification_audit",
        precedes: &["test"],
    },
    PrincipleGate {
        id: "red-before-green",
        stages: &["implement"],
        repo_command: "sh .rigger/gates/red-before-green.sh",
        precedes: &[],
    },
];

/// Every principle gate missing from the workflow at `root`: undeclared, not listed on a stage
/// it guards, or (when `repo` is set) declared with a command other than this repository's.
fn missing_principle_gates(root: &Path, repo: bool) -> Vec<String> {
    let cfg = rigger::config_store::load(root.to_str().unwrap())
        .unwrap_or_else(|e| panic!("the workflow at {} must load: {e}", root.display()));
    let wf = &cfg.workflow;
    let mut missing = Vec::new();
    for gate in PRINCIPLE_GATES {
        match wf.gates.get(gate.id) {
            None => missing.push(format!("gate `{}` is not declared", gate.id)),
            Some(g) if repo && !g.run.contains(gate.repo_command) => missing.push(format!(
                "gate `{}` must run `{}`, got {:?}",
                gate.id, gate.repo_command, g.run
            )),
            Some(_) => {}
        }
        for stage in gate.stages {
            let gates = wf
                .stages
                .get(*stage)
                .map(|s| s.gates.clone())
                .unwrap_or_default();
            let Some(at) = gates.iter().position(|g| g == gate.id) else {
                missing.push(format!("stage `{stage}` does not list gate `{}`", gate.id));
                continue;
            };
            for later in gate.precedes {
                if gates
                    .iter()
                    .position(|g| g == later)
                    .is_some_and(|l| l < at)
                {
                    missing.push(format!(
                        "stage `{stage}` runs gate `{later}` before gate `{}`",
                        gate.id
                    ));
                }
            }
        }
    }
    missing
}

#[test]
fn this_repository_wires_every_principle_gate_on_the_stages_it_guards() {
    let missing = missing_principle_gates(&repo_root(), true);
    assert!(missing.is_empty(), ".rigger/workflow.yml: {missing:#?}");
}

/// A consumer project scaffolded by `rigger init` carries every principle gate on the stages it
/// guards and every persona checklist line ([`PERSONA_CHECKLIST`]).
#[test]
fn a_scaffolded_consumer_project_carries_every_principle_gate_and_checklist_line() {
    let dir = temp_project();
    let (_out, err, ok) = run_rigger(dir.path(), &["init"]);
    assert!(ok, "rigger init must succeed; stderr:\n{err}");
    let mut missing = missing_principle_gates(dir.path(), false);
    missing.extend(missing_checklist_lines(dir.path()));
    assert!(missing.is_empty(), "the scaffolded .rigger/: {missing:#?}");
}

/// This repository's own command for gate `id`, as the production parser loads it.
fn repo_gate_command(id: &str) -> String {
    let cfg = rigger::config_store::load(repo_root().to_str().unwrap()).unwrap();
    cfg.workflow.gates[id].run.clone()
}

/// The shipped `audit` gate run in a fixture repository whose committed catalog is `committed`,
/// against a stand-in `cargo`: in write mode it regenerates the catalog as `fresh`; otherwise it
/// asserts, failing with `red` output when `red` is set. Returns (passed, output).
fn run_audit_gate(committed: &str, fresh: &str, red: Option<&str>) -> (bool, String) {
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path().join("repo");
    std::fs::create_dir_all(repo.join("docs/audit")).unwrap();
    init_repo(&repo);
    std::fs::write(repo.join("docs/audit/catalog.json"), committed).unwrap();
    git_commit_all(&repo, "catalog");
    let bin = dir.path().join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    let assert_step = match red {
        Some(output) => format!("echo '{output}'; exit 101"),
        None => "echo 'test result: ok'".to_string(),
    };
    let cargo = bin.join("cargo");
    std::fs::write(
        &cargo,
        format!(
            "#!/bin/sh\nif [ \"$RIGGER_AUDIT_WRITE\" = 1 ]; then printf '{fresh}' > \
             docs/audit/catalog.json; exit 0; fi\n{assert_step}\n"
        ),
    )
    .unwrap();
    std::fs::set_permissions(&cargo, std::fs::Permissions::from_mode(0o755)).unwrap();
    let path = format!(
        "{}:{}",
        bin.display(),
        std::env::var("PATH").unwrap_or_default()
    );
    let out = Command::new("sh")
        .arg("-c")
        .arg(repo_gate_command("audit"))
        .current_dir(&repo)
        .env("PATH", path)
        .output()
        .unwrap();
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    (out.status.success(), text)
}

#[test]
fn the_audit_gate_passes_a_fresh_committed_catalog_silently() {
    let (passed, out) = run_audit_gate("same", "same", None);
    assert!(passed, "{out}");
    assert!(!out.contains("not committed"), "{out}");
}

#[test]
fn the_audit_gate_names_a_regenerated_uncommitted_catalog_without_failing_on_drift_alone() {
    let (passed, out) = run_audit_gate("stale", "fresh", None);
    assert!(passed, "drift alone must never fail the gate: {out}");
    assert!(
        out.contains("audit: docs/audit was regenerated for this tree and is not committed"),
        "the gate must report the uncommitted regeneration distinctly: {out}"
    );
}

#[test]
fn the_audit_gate_fails_red_assertions_with_its_own_diagnostic() {
    let (passed, out) = run_audit_gate("same", "same", Some("clusters with no disposition"));
    assert!(!passed, "{out}");
    assert!(out.contains("clusters with no disposition"), "{out}");
    assert!(out.contains("error[audit]:"), "{out}");
}

/// A fixture repository whose `rigger-run` branch holds one committed source file with a
/// trailing test module, checked out on a fresh unit branch.
fn unit_branch_repo() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path();
    init_repo(repo);
    std::fs::create_dir_all(repo.join("src")).unwrap();
    std::fs::write(
        repo.join("src/lib.rs"),
        "pub fn f() -> u8 {\n    1\n}\n\n#[cfg(test)]\nmod tests {\n    #[test]\n    fn f_is_one() {\n        assert_eq!(super::f(), 1);\n    }\n}\n",
    )
    .unwrap();
    git_commit_all(repo, "base");
    git_ok(repo, &["branch", "rigger-run"]);
    git_ok(repo, &["checkout", "-q", "-b", "unit"]);
    dir
}

/// Write `content` to `rel` in `repo` and commit it as `msg`.
fn commit_file(repo: &Path, rel: &str, content: &str, msg: &str) {
    let path = repo.join(rel);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, content).unwrap();
    git_commit_all(repo, msg);
}

/// The shipped red-before-green gate script run on `repo`'s unit branch against `rigger-run`.
/// Returns (passed, output).
fn run_red_before_green(repo: &Path) -> (bool, String) {
    let script = repo_root().join(".rigger/gates/red-before-green.sh");
    let out = Command::new("sh")
        .arg(script)
        .current_dir(repo)
        .output()
        .unwrap();
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    (out.status.success(), text)
}

const LIB_WITH_G: &str = "pub fn f() -> u8 {\n    1\n}\n\npub fn g() -> u8 {\n    2\n}\n\n#[cfg(test)]\nmod tests {\n    #[test]\n    fn f_is_one() {\n        assert_eq!(super::f(), 1);\n    }\n}\n";

/// A fixture test file, committed on its own as the red half.
const TEST_FILE: &str = "#[test]\nfn g_is_two() {}\n";

/// The gate passes a unit branch that `commits` (each `(path, content, message)`, in order)
/// build on the fixture base.
fn red_before_green_passes(commits: &[(&str, &str, &str)]) {
    let dir = unit_branch_repo();
    for (rel, content, msg) in commits {
        commit_file(dir.path(), rel, content, msg);
    }
    let (passed, out) = run_red_before_green(dir.path());
    assert!(passed, "{out}");
}

rigger::test_cases! {
    /// A test commit, then the source commit it drives.
    red_before_green_passes_a_test_commit_before_the_source_commit: red_before_green_passes(&[
        ("tests/g.rs", TEST_FILE, "red"),
        ("src/lib.rs", LIB_WITH_G, "green"),
    ]);
    /// One commit whose source change adds its own `#[test]`.
    red_before_green_passes_one_commit_that_carries_its_own_test: red_before_green_passes(&[(
        "src/lib.rs",
        &LIB_WITH_G.replace(
            "        assert_eq!(super::f(), 1);\n    }\n",
            "        assert_eq!(super::f(), 1);\n    }\n\n    #[test]\n    fn g_is_two() {\n        \
             assert_eq!(super::g(), 2);\n    }\n",
        ),
        "test and code together",
    )]);
    /// One commit whose source change also edits a test inside the trailing test module.
    red_before_green_counts_an_edit_inside_the_trailing_test_module_as_a_test:
        red_before_green_passes(&[(
            "src/lib.rs",
            &LIB_WITH_G.replace("assert_eq!(super::f(), 1);", "assert_eq!(super::f(), 1, \"f\");"),
            "code with a changed test",
        )]);
    /// A branch that never touches source.
    red_before_green_passes_a_branch_with_no_source_commit: red_before_green_passes(&[(
        "docs/note.md",
        "a note\n",
        "docs only",
    )]);
}

#[test]
fn red_before_green_fails_a_source_commit_no_test_commit_precedes_naming_it() {
    let dir = unit_branch_repo();
    commit_file(dir.path(), "src/lib.rs", LIB_WITH_G, "green first");
    let sha = git_out(dir.path(), &["rev-parse", "--short", "HEAD"]);
    commit_file(dir.path(), "tests/g.rs", TEST_FILE, "red after");
    let (passed, out) = run_red_before_green(dir.path());
    assert!(!passed, "{out}");
    assert!(
        out.contains("error[red-before-green]")
            && out.contains(&sha)
            && out.contains("green first"),
        "the failure must name the offending commit: {out}"
    );
}

/// The review checklist line each persona carries for the principle gates, as `(agent file,
/// line)`: the lens names the principle, the adjudicator never trades a boundary red away, the
/// test lens checks red before green.
const PERSONA_CHECKLIST: &[(&str, &str)] = &[
    (
        "architecture-reviewer.md",
        "Name the SOLID principle for each finding",
    ),
    (
        "adjudicator.md",
        "A red `boundary` gate is non-negotiable: reject, never balance it against other evidence.",
    ),
    (
        "sdet.md",
        "Confirm the unit's first source commit follows a test commit (red before green).",
    ),
];

/// Every checklist line missing from the persona files under `root/.rigger/agents`.
fn missing_checklist_lines(root: &Path) -> Vec<String> {
    PERSONA_CHECKLIST
        .iter()
        .filter(|(file, line)| {
            let text =
                std::fs::read_to_string(root.join(".rigger/agents").join(file)).unwrap_or_default();
            !text.contains(line)
        })
        .map(|(file, line)| format!("{file}: {line:?}"))
        .collect()
}

#[test]
fn every_persona_carries_its_principle_gate_checklist_line() {
    let missing = missing_checklist_lines(&repo_root());
    assert!(missing.is_empty(), ".rigger/agents: {missing:#?}");
}

/// The check-in mutation gate's logic lives in ONE shipped script: this repository's `mutation`
/// gate runs it, and `rigger init` writes the identical file into a consumer project, so the
/// sweep's bounds and scope reach every consumer rather than living in this repository alone.
/// The script sources the container runtime snippet from beside itself, so `rigger init`
/// writes that file too, identical as well.
#[test]
fn the_mutation_gate_runs_the_shipped_script_and_init_writes_the_same_script() {
    assert_eq!(
        repo_gate_command("mutation"),
        "sh .rigger/gates/mutation.sh",
        "the gate is a one-line invocation of the shipped script"
    );
    let dir = temp_project();
    let (_out, err, ok) = run_rigger(dir.path(), &["init"]);
    assert!(ok, "rigger init must succeed; stderr:\n{err}");
    for file in ["mutation.sh", "container-env.sh"] {
        let path = format!(".rigger/gates/{file}");
        let shipped = std::fs::read_to_string(repo_root().join(&path))
            .unwrap_or_else(|e| panic!("the shipped {path}: {e}"));
        let scaffolded = std::fs::read_to_string(dir.path().join(&path))
            .unwrap_or_else(|e| panic!("rigger init must write {path}: {e}"));
        assert_eq!(
            scaffolded, shipped,
            "the consumer gets the same {path}, byte for byte"
        );
    }
}

/// The `fmt`, `clippy` and `build` gates cover every workspace crate: a bare `cargo clippy` or
/// `cargo build` in this repository reaches only the root package, so a crate's own code would
/// never be linted or built under any unit. Clippy stays on its one lane, the default features.
#[test]
fn the_fmt_clippy_and_build_gates_cover_the_workspace() {
    for (gate, run) in [
        ("fmt", "cargo fmt --all --check"),
        (
            "clippy",
            "cargo clippy --workspace --all-targets -- -D warnings",
        ),
        ("build", "cargo build --workspace"),
    ] {
        assert_eq!(repo_gate_command(gate), run, "the `{gate}` gate");
    }
}

/// The container runtime snippet a gate that runs tests sources, relative to the worktree.
const CONTAINER_SNIPPET: &str = ".rigger/gates/container-env.sh";

/// The per-unit `test` gate runs every workspace crate's tests, never the root package's
/// alone (a bare `cargo test` here tests only the root package, so no crate's own unit tests
/// would run under any unit), and it first sources the container runtime snippet the
/// repository carries, so the container-backed tests run instead of skipping.
#[test]
fn the_test_gate_covers_the_workspace_with_the_container_runtime() {
    assert_eq!(
        repo_gate_command("test"),
        "if test -f .rigger/gates/container-env.sh; then . .rigger/gates/container-env.sh || \
         exit 1; fi; cargo test --workspace"
    );
    assert!(repo_root().join(CONTAINER_SNIPPET).is_file());
}

/// A PATH that finds `tool` in a directory under `work`, as the fixture `stub` when one is
/// given, ahead of the ambient PATH; with no stub the directory is empty and the whole PATH.
fn stub_path(work: &Path, tool: &str, stub: Option<&str>) -> String {
    let bin = work.join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    let Some(stub) = stub else {
        return bin.display().to_string();
    };
    std::os::unix::fs::symlink(
        repo_root().join("tests/fixtures").join(stub),
        bin.join(tool),
    )
    .unwrap();
    format!(
        "{}:{}",
        bin.display(),
        std::env::var("PATH").unwrap_or_default()
    )
}

/// A finished shell's success and its output, stdout then stderr.
fn shell_outcome(out: &std::process::Output) -> (bool, String) {
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    (out.status.success(), text)
}

/// This repository's `test` gate command run under `sh -c` - as the conductor runs every gate -
/// in the worktree `tree`, with a stand-in `cargo` that records its argv. Returns (passed,
/// output, the argv line cargo recorded - empty when cargo never ran).
fn run_test_gate(work: &Path, tree: &Path) -> (bool, String, String) {
    let dump = work.join("cargo.dump");
    let out = Command::new("sh")
        .arg("-c")
        .arg(repo_gate_command("test"))
        .current_dir(tree)
        .env("PATH", stub_path(work, "cargo", Some("recording-cargo.sh")))
        .env("RECORDING_CARGO_DUMP_FILE", &dump)
        .output()
        .unwrap();
    let args = std::fs::read_to_string(&dump)
        .unwrap_or_default()
        .lines()
        .next()
        .unwrap_or_default()
        .to_string();
    let (passed, text) = shell_outcome(&out);
    (passed, text, args)
}

/// The gate string comes from the operator's workflow but runs inside the unit's worktree, and
/// a unit branch can predate the snippet: the tests still run, without it.
#[test]
fn the_test_gate_runs_the_tests_in_a_worktree_that_predates_the_snippet() {
    let work = tempfile::tempdir().unwrap();
    let tree = work.path().join("tree");
    std::fs::create_dir_all(&tree).unwrap();
    let (passed, out, args) = run_test_gate(work.path(), &tree);
    assert!(passed, "{out}");
    assert_eq!(args, "ARGS:test --workspace", "{out}");
}

/// A snippet that is present but fails to source fails the gate before any test runs, never a
/// silent run without the container runtime.
#[test]
fn the_test_gate_fails_when_its_snippet_fails_to_source() {
    let work = tempfile::tempdir().unwrap();
    let tree = work.path().join("tree");
    std::fs::create_dir_all(tree.join(".rigger/gates")).unwrap();
    std::fs::write(tree.join(CONTAINER_SNIPPET), "false\n").unwrap();
    let (passed, out, args) = run_test_gate(work.path(), &tree);
    assert!(
        !passed,
        "a snippet that fails to source must fail the gate: {out}"
    );
    assert!(args.is_empty(), "cargo must not run: {args}");
}

/// The one listing the container runtime snippet asks for: every container carrying the test
/// label, running or not, so a container without the label is never listed.
const LABELLED_LISTING: &str = "ps -a --filter label=rigger.test --format {{.ID}} {{.CreatedAt}}";

/// Sources the shipped container runtime snippet as a gate does, with a podman socket where
/// the snippet looks for one when `runtime`, and, when `cli`, a stand-in `podman` on PATH whose
/// labelled test containers are `fresh` (5 s old) and `old` (an hour old); without `cli` the
/// PATH holds no container CLI at all. `max_age` sets RIGGER_TEST_CONTAINER_MAX_AGE_S. Neither
/// DOCKER_HOST nor the test runner's cap RIGGER_TEST_AS_BYTES reaches the snippet. The snippet
/// must source cleanly; returns its output, one line per `podman` call, and the environment it
/// leaves exported to the tests the gate runs next.
fn source_container_snippet(
    runtime: bool,
    cli: bool,
    max_age: Option<&str>,
) -> (String, Vec<String>, BTreeMap<String, String>) {
    let work = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(work.path().join("podman")).unwrap();
    let _socket = runtime.then(|| {
        std::os::unix::net::UnixListener::bind(work.path().join("podman/podman.sock")).unwrap()
    });
    let log = work.path().join("podman.log");
    let exported = work.path().join("exported.env");
    let mut cmd = Command::new("/bin/sh");
    cmd.arg("-c")
        .arg(". \"$0\" && /usr/bin/env -0 > \"$1\"")
        .arg(repo_root().join(CONTAINER_SNIPPET))
        .arg(&exported)
        .env(
            "PATH",
            stub_path(work.path(), "podman", cli.then_some("recording-podman.sh")),
        )
        .env("XDG_RUNTIME_DIR", work.path())
        .env("RECORDING_PODMAN_LOG", &log)
        .env("RECORDING_PODMAN_CONTAINERS", "fresh:5 old:3600")
        .env_remove("DOCKER_HOST")
        .env_remove("RIGGER_TEST_AS_BYTES");
    match max_age {
        Some(age) => cmd.env("RIGGER_TEST_CONTAINER_MAX_AGE_S", age),
        None => cmd.env_remove("RIGGER_TEST_CONTAINER_MAX_AGE_S"),
    };
    let (sourced, out) = shell_outcome(&cmd.output().unwrap());
    assert!(sourced, "the snippet must source cleanly: {out}");
    let calls = std::fs::read_to_string(&log)
        .unwrap_or_default()
        .lines()
        .map(str::to_string)
        .collect();
    let exported = std::fs::read_to_string(&exported)
        .unwrap()
        .split('\0')
        .filter_map(|var| var.split_once('='))
        .map(|(name, value)| (name.to_string(), value.to_string()))
        .collect();
    (out, calls, exported)
}

/// Having found the podman socket, the snippet exports DOCKER_HOST at it and leaves the test
/// runner's per-process address-space cap as the caller left it (unset here, so the runner's own
/// default): the gate judges the container-backed tests under the same cap as a plain
/// `cargo test` run with DOCKER_HOST already set, never under a raised one.
#[test]
fn the_container_snippet_exports_the_found_socket_and_leaves_the_test_runner_cap_alone() {
    let (out, _, exported) = source_container_snippet(true, true, None);
    let socket = format!("unix://{}/podman/podman.sock", exported["XDG_RUNTIME_DIR"]);
    assert_eq!(exported.get("DOCKER_HOST"), Some(&socket), "{out}");
    assert_eq!(exported.get("RIGGER_TEST_AS_BYTES"), None, "{out}");
}

/// Once it has found the runtime, the snippet removes the labelled test containers an earlier
/// run left behind past the default max age of 600 s, and only those: a fresh one stays.
#[test]
fn the_container_snippet_removes_only_the_labelled_test_containers_past_their_age() {
    let (out, calls, _) = source_container_snippet(true, true, None);
    assert_eq!(calls, [LABELLED_LISTING, "rm -f old"], "{out}");
}

/// With RIGGER_TEST_CONTAINER_MAX_AGE_S=0 every labelled test container goes, the fresh one too.
#[test]
fn the_container_snippet_removes_every_labelled_test_container_at_a_zero_max_age() {
    let (out, calls, _) = source_container_snippet(true, true, Some("0"));
    assert_eq!(
        calls,
        [LABELLED_LISTING, "rm -f fresh", "rm -f old"],
        "{out}"
    );
}

/// With no runtime found the snippet lists and removes nothing.
#[test]
fn the_container_snippet_removes_nothing_when_it_finds_no_runtime() {
    let (out, calls, _) = source_container_snippet(false, true, Some("0"));
    assert!(calls.is_empty(), "{out}{calls:?}");
}

/// A runtime with neither a podman nor a docker CLI on PATH costs one line of output, never
/// a failed gate.
#[test]
fn the_container_snippet_goes_on_without_a_container_cli() {
    let (out, _, _) = source_container_snippet(true, false, Some("0"));
    assert_eq!(out.lines().count(), 1, "{out}");
}
