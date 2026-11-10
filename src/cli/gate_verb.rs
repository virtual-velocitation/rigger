//! `rigger gate <id>`: an agent's own run of a declared gate on its committed tree, through the
//! driver's gate path, recorded on the run's store where the driver reuses a green verdict
//! instead of running the gate again on the same commit ([`conductor::run_gate_verb`]).

use super::*;

/// `rigger gate <id>`: run gate `<id>` on the committed tree of the git checkout the process
/// stands in, under the workflow the driver gates with (the owning repository's), and record its
/// GateVerdict. Prints the result, the commit and the digest the driver reuses, then the gate's
/// output tail; exits non-zero on a red gate. Refuses, recording nothing, a malformed argv, an
/// undeclared gate (naming the declared ones) and a tree with uncommitted changes.
pub(crate) fn cmd_gate(args: &[String]) -> Res {
    let [gate_id] = args else {
        return Err("gate: usage: rigger gate <id>".into());
    };
    let dir = git_repo();
    if dir.is_empty() {
        return Err("gate: not inside a git checkout".into());
    }
    let spawn = std::env::var(rigger::driver::SPAWN_ENV).unwrap_or_default();
    on_the_run(|loc, store| {
        let repo = loc.repo_root();
        let cfg = load_run_config(&repo)?;
        let run = conductor::run_gate_verb(&cfg, store, &ExecRunner, &repo, &dir, gate_id, &spawn)?;
        let result = if run.pass { "PASS" } else { "FAIL" };
        let digest = if run.digest.is_empty() {
            "none: the gate changed the tree, so the driver will run it again"
        } else {
            &run.digest
        };
        println!("{result} gate {} on {}", run.gate, run.commit);
        println!("digest: {digest}");
        println!("{}", run.evidence);
        if run.pass {
            Ok(())
        } else {
            Err(format!("gate: {} failed on {}", run.gate, run.commit).into())
        }
    })
}
