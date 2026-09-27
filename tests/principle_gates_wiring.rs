//! THE PRINCIPLE GATES ARE DEFINITION: the gates that hold every unit to the engineering
//! principles mechanically are declared in this repository's own `.rigger/workflow.yml` AND in
//! the workflow `rigger init` scaffolds for a consumer project, each listed on the stages it
//! guards. Both are loaded through the production parser (`rigger::config_store::load`), so a
//! gate that is merely mentioned in a comment, or declared but never listed on a stage, fails.

mod common;
use common::cli::{run_rigger, temp_project};
use common::git::{git_commit_all, init_repo};
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

#[test]
fn a_scaffolded_consumer_project_carries_every_principle_gate() {
    let dir = temp_project();
    let (_out, err, ok) = run_rigger(dir.path(), &["init"]);
    assert!(ok, "rigger init must succeed; stderr:\n{err}");
    let missing = missing_principle_gates(dir.path(), false);
    assert!(
        missing.is_empty(),
        "the scaffolded workflow.yml: {missing:#?}"
    );
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
