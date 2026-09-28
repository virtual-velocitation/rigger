//! Agent drivers: adapters implementing the conductor's AgentDriver port. `cli`
//! is the default (shell out to the `claude` CLI, so Rigger depends on no
//! particular runtime); `workflow` is the in-Claude-Code alternative.
//! `claude_code` is the native headless-session host (spec 104) landing alongside them.

pub mod claude_code;
pub mod cli;
pub mod replay;
pub mod workflow;
