//! `rigger gate <id>`: an agent's request that the driver gate its committed tree. The driver
//! serving the project's run answers it over the run's gate channel ([`rigger::gate_socket`]):
//! it runs the declared gate, signs and records the verdict, and reuses a green one instead of
//! running the gate again on the same commit. With no driver serving, the verb runs the gate for
//! information only and records nothing ([`conductor::run_gate_for_information`]).

use super::*;

use rigger::gate_channel::{GateReply, GateRequest};

/// The file name of the run's gate request socket inside the project's store directory.
pub(crate) const GATE_SOCKET: &str = "gate.sock";

/// `rigger gate <id>`: ask the driver serving the project to run gate `<id>` on the committed
/// tree of the unit worktree the process stands in. Prints the result, the commit and the digest
/// the driver reuses, then the gate's output tail; exits non-zero on a red gate. A refusal (a
/// malformed argv, a directory that is not a unit worktree the run is driving, an undeclared gate,
/// a tree with uncommitted changes) is the error, and records nothing, and so is an OS the
/// channel has no adapter for. With no driver serving, the gate runs once here for information
/// only and nothing is recorded, which the verb says.
pub(crate) fn cmd_gate(args: &[String]) -> Res {
    let [gate_id] = args else {
        return Err("gate: usage: rigger gate <id>".into());
    };
    let dir = git_repo();
    if dir.is_empty() {
        return Err("gate: not inside a git checkout".into());
    }
    on_the_run(|loc, store| {
        let request = GateRequest {
            dir: dir.clone(),
            gate: gate_id.clone(),
        };
        let socket = loc.dir.join(GATE_SOCKET);
        let asked = rigger::gate_socket::ask(&socket, &request);
        let unserved = match asked {
            Ok(Some(GateReply::Ran {
                gate,
                pass,
                commit,
                digest,
                evidence,
                ..
            })) => return print_gate_run(&gate, pass, &commit, &digest, &evidence),
            Ok(Some(GateReply::Refused { reason })) => return Err(reason.into()),
            Ok(None) => "no driver is serving this project".to_string(),
            Err(e) if e.kind() == std::io::ErrorKind::Unsupported => {
                return Err(format!(
                    "gate: unsupported here: {e}; the driver runs the gate itself before it acts \
                     on it"
                )
                .into())
            }
            // A socket path longer than the OS allows: the driver could not bind it either.
            Err(e) if e.kind() == std::io::ErrorKind::InvalidInput => format!(
                "the gate socket path {} is too long for this OS ({e})",
                socket.display()
            ),
            Err(e) => return Err(format!("gate: asking the driver failed: {e}").into()),
        };
        let repo = loc.repo_root();
        let cfg = load_run_config(&repo)?;
        let run =
            conductor::run_gate_for_information(&cfg, store, &ExecRunner, &repo, &dir, gate_id)?;
        eprintln!(
            "rigger gate: {unserved}, so this run is for information only and recorded nothing; \
             the driver runs the gate itself before it acts on it"
        );
        print_gate_run(&run.gate, run.pass, &run.commit, &run.digest, &run.evidence)
    })
}

/// Print one gate run's result, commit, digest and output tail; a red gate is the error.
fn print_gate_run(gate: &str, pass: bool, commit: &str, digest: &str, evidence: &str) -> Res {
    let result = if pass { "PASS" } else { "FAIL" };
    let digest = if digest.is_empty() {
        "none: the worktree changed while the gate ran"
    } else {
        digest
    };
    println!("{result} gate {gate} on {commit}");
    println!("digest: {digest}");
    println!("{evidence}");
    if pass {
        Ok(())
    } else {
        Err(format!("gate: {gate} failed on {commit}").into())
    }
}
