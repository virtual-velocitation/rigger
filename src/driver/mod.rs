//! Agent drivers: adapters implementing the conductor's AgentDriver port. `cli`
//! is the default (shell out to the `claude` CLI, so Rigger depends on no
//! particular runtime); `workflow` is the in-Claude-Code alternative.

pub mod cli;
