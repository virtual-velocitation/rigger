//! THE PRINCIPLE GATES ARE DEFINITION: the gates that hold every unit to the engineering
//! principles mechanically are declared in this repository's own `.rigger/workflow.yml` AND in
//! the workflow `rigger init` scaffolds for a consumer project, each listed on the stages it
//! guards. Both are loaded through the production parser (`rigger::config_store::load`), so a
//! gate that is merely mentioned in a comment, or declared but never listed on a stage, fails.

mod common;
use common::cli::{run_rigger, temp_project};
use common::repo::repo_root;
use std::path::Path;

/// One principle gate: its id, the stages that must list it, and the text this repository's
/// own command for it must carry.
struct PrincipleGate {
    id: &'static str,
    stages: &'static [&'static str],
    repo_command: &'static str,
}

const PRINCIPLE_GATES: &[PrincipleGate] = &[PrincipleGate {
    id: "boundary",
    stages: &["implement", "checkin"],
    repo_command: "cargo test --test boundary_audit",
}];

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
            let listed = wf
                .stages
                .get(*stage)
                .is_some_and(|s| s.gates.iter().any(|g| g == gate.id));
            if !listed {
                missing.push(format!("stage `{stage}` does not list gate `{}`", gate.id));
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
