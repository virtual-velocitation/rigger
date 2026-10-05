//! Agent drivers: adapters implementing the conductor's AgentDriver port. `cli`
//! is the default (shell out to the `claude` CLI, so Rigger depends on no
//! particular runtime); `workflow` is the in-Claude-Code alternative.
//! `claude_code` is the native headless-session host (spec 104) landing alongside them.
//!
//! Both process hosts launch a headless `claude` session, so what such a session needs
//! beyond its protocol (permissions, MCP server, settings, helpers) is composed here once,
//! by [`spawn_config_args`], and each host's own `build_args` appends it.

use crate::agent::Error;
use crate::hooks;

pub mod claude_code;
pub mod cli;
pub mod replay;
pub mod workflow;

/// The permission mode every headless spawn runs under (architecture addendum §4.1): paired
/// with `--permission-prompts none`, whatever would otherwise prompt is DENIED (and, on the
/// stream-json host, reported on the stream) rather than silently blocking on a prompt nobody
/// can answer. Not configurable per agent - every persona runs unattended the same way.
const PERMISSION_MODE: &str = "default";

/// A configured binary, or `default` (resolved on `$PATH`) when none is configured - the
/// ONE empty-means-default rule every host's `bin` and `rigger_bin` follow.
pub(crate) fn bin_or_path_default<'s>(configured: &'s str, default: &'s str) -> &'s str {
    if configured.is_empty() {
        default
    } else {
        configured
    }
}

/// One composed configuration as the inline JSON argument its flag takes, or the error naming
/// which configuration could not be composed.
fn json_arg(composed: Result<Vec<u8>, hooks::Error>, what: &str) -> Result<String, Error> {
    composed
        .and_then(|bytes| String::from_utf8(bytes).map_err(|e| hooks::Error(e.to_string())))
        .map_err(|e| Error(format!("driver: compose the spawn's {what}: {e}")))
}

/// The unattended configuration every headless spawn carries, whichever host launches it. It
/// lives here, beside both hosts rather than in [`hooks`], because it is argv: `hooks` keeps
/// the values a rigger session carries (the one home `rigger setup` also installs from), and
/// this composes them into the flags a host passes. A headless session reads its hooks, MCP
/// servers and subagents from its working directory and a fresh unit worktree carries none of
/// them (`.claude/` and `.mcp.json` are machine-local, and a consumer's `.gitignore` ignores
/// `.claude`), so they travel on the command line as inline JSON - nothing is written into the
/// worktree, which the attempt checkpoint would commit, and no file outlives the launch:
///
/// - `--permission-mode default` with `--permission-prompts none`: see [`PERMISSION_MODE`];
/// - `--mcp-config` names exactly one server, `<rigger_bin> <mcp_args..>` under
///   [`hooks::MCP_SERVER_NAME`], and `--strict-mcp-config` keeps every other MCP configuration
///   out. The host picks the server: the stream-json host binds each spawn its own
///   (`mcp --spawn <id>`, §4.3), the cli host the operator's read-only one (`mcp`, the entry
///   `rigger setup` writes into `.mcp.json`);
/// - `--settings` is the spawn's own `settings_json` (may be empty) with the SessionStart prime
///   hook, the PreToolUse grep-guard and the status line merged in;
/// - `--agents` defines the `lookup` and `verify` fan-out helpers.
///
/// Fails only when `settings_json` is not a JSON object the session settings can merge into.
pub(crate) fn spawn_config_args(
    rigger_bin: &str,
    mcp_args: &[&str],
    settings_json: &str,
) -> Result<Vec<String>, Error> {
    let helpers = hooks::helper_agents_json()
        .map_err(|e| Error(format!("driver: compose the spawn's agents: {e}")))?;
    Ok(vec![
        "--permission-mode".to_string(),
        PERMISSION_MODE.to_string(),
        "--permission-prompts".to_string(),
        "none".to_string(),
        "--mcp-config".to_string(),
        json_arg(
            hooks::install_mcp_server(b"", hooks::MCP_SERVER_NAME, rigger_bin, mcp_args),
            "MCP config",
        )?,
        "--strict-mcp-config".to_string(),
        "--settings".to_string(),
        json_arg(
            hooks::install_session_settings(settings_json.as_bytes()),
            "settings",
        )?,
        "--agents".to_string(),
        helpers.to_string(),
    ])
}
