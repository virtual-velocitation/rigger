//! THE PRINCIPLE GATES ARE DEFINITION: the gates that hold every unit to the engineering
//! principles mechanically are declared in this repository's own `.rigger/workflow.yml` AND in
//! the workflow `rigger init` scaffolds for a consumer project, each listed on the stages it
//! guards. Both are loaded through the production parser (`rigger::config_store::load`), so a
//! gate that is merely mentioned in a comment, or declared but never listed on a stage, fails.

mod common;
use common::cli::{run_rigger, temp_project};
use common::git::{git_commit_all, git_ok, git_out, init_repo};
use common::repo::repo_root;
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
    let shipped = std::fs::read_to_string(repo_root().join(".rigger/gates/mutation.sh"))
        .expect("the shipped mutation gate script");
    let scaffolded = std::fs::read_to_string(dir.path().join(".rigger/gates/mutation.sh"))
        .expect("rigger init must write .rigger/gates/mutation.sh");
    assert_eq!(
        scaffolded, shipped,
        "the consumer gets the same script, byte for byte"
    );
}
