//! The gate request interface (signed verdicts): how `rigger gate <id>`, run by an agent in a
//! unit's worktree, asks the run's driver to gate that worktree's commit. The driver alone runs a
//! gate whose verdict counts, so the verb is a request: the driver checks the asking worktree is
//! one of its live units and is committed, resolves the gate from its own workflow, runs it,
//! signs and records the verdict, and replies with it. One adapter per OS family sits in the
//! process ring and, of the production code outside it, only the composition root names it.

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

/// One request: gate `gate` on the committed checkout at `dir`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GateRequest {
    /// The asking worktree's top-level directory.
    pub dir: String,
    /// The id of a gate the workflow declares.
    pub gate: String,
}

/// The driver's answer to one [`GateRequest`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "reply", rename_all = "snake_case")]
pub enum GateReply {
    /// The driver ran the gate on `commit` and recorded the signed verdict at `position`;
    /// `digest` is the input digest a later gate of the same command over the same commit is
    /// answered from, empty when the gate changed the tree it gated, which is never a pass.
    Ran {
        gate: String,
        pass: bool,
        commit: String,
        digest: String,
        evidence: String,
        position: u64,
    },
    /// The driver ran nothing and recorded nothing, for `reason`.
    Refused { reason: String },
}

/// The channel a driver serves gate requests on, scoped to its run.
pub trait GateChannel: Sync {
    /// Answer each request with `answer` until [`GateChannel::close`] is called.
    fn serve(&self, answer: &(dyn Fn(GateRequest) -> GateReply + Sync)) -> std::io::Result<()>;
    /// End [`GateChannel::serve`]; safe to call from another thread and more than once.
    fn close(&self);
}

/// `message` as the one JSON line an adapter writes for it, newline included.
pub fn to_line(message: &impl Serialize) -> String {
    let mut line = serde_json::to_string(message).unwrap_or_default();
    line.push('\n');
    line
}

/// The message one JSON line an adapter read carries, or why it carries none.
pub fn from_line<T: DeserializeOwned>(line: &str) -> Result<T, String> {
    serde_json::from_str(line.trim_end())
        .map_err(|e| format!("a malformed gate channel message: {e}"))
}
