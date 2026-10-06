//! Agent drivers: adapters implementing the conductor's AgentDriver port. `cli`
//! is the default (shell out to the `claude` CLI, so Rigger depends on no
//! particular runtime); `workflow` is the in-Claude-Code alternative.
//! `claude_code` is the native headless-session host (spec 104) landing alongside them.
//!
//! Both process hosts launch a headless `claude` session, so what such a session needs
//! beyond its protocol (permissions, MCP server, settings, helpers) is composed here once,
//! by [`spawn_config_args`], and each host's own `build_args` appends it.

use std::path::PathBuf;

use crate::agent::Error;
use crate::config::AgentDef;
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

/// The `--allowed-tools` pair every headless spawn passes: the tools `agent` is granted
/// ([`AgentDef::allowed_tools`]: `recurse: false` strips any fan-out tool, runaway-proof by
/// construction, §3.1, §6), then the helpers' rigger MCP tools ([`hooks::helper_mcp_tools`]),
/// each name once. The helpers' tools are pre-approved because under `--permission-prompts
/// none` nothing answers a prompt, so a `lookup` would otherwise be denied every graph call it
/// makes; the critic's host (`rigger critique`, on the headless host) is the first production
/// beneficiary, `rigger run`'s cli-host workers the second. Always present, since the helpers
/// declare their tools.
pub(crate) fn allowed_tools_args(agent: &AgentDef) -> Result<Vec<String>, Error> {
    let mut tools = agent.allowed_tools();
    for tool in hooks::helper_mcp_tools()
        .map_err(|e| Error(format!("driver: compose the spawn's allowed tools: {e}")))?
    {
        if !tools.contains(&tool) {
            tools.push(tool);
        }
    }
    Ok(vec!["--allowed-tools".to_string(), tools.join(",")])
}

/// One composed configuration as the inline JSON argument its flag takes, or the error naming
/// which configuration could not be composed.
fn json_arg(composed: Result<Vec<u8>, hooks::Error>, what: &str) -> Result<String, Error> {
    composed
        .and_then(|bytes| String::from_utf8(bytes).map_err(|e| hooks::Error(e.to_string())))
        .map_err(|e| Error(format!("driver: compose the spawn's {what}: {e}")))
}

/// The harness environment every headless worker gets, whichever host launches it: the
/// `(key, value)` pairs each host applies to the spawned `claude -p` process at its env
/// injection site, BEFORE the build environment (`SpawnOpts::env`). `Command::env` is
/// last-write-wins per key, so a value the operator puts in the build environment under the
/// same key overrides the one here.
///
/// Today exactly one pair, `CLAUDE_CODE_PRINT_BG_WAIT_CEILING_MS=0`. In print mode a session
/// that started a background helper (a subagent dispatched without waiting on it, or a
/// background command) stays open after its final turn until that work completes, but only
/// for a default ceiling of ten minutes of continuous idle waiting; past it the harness stops
/// whatever is still running and DROPS its partial result. A gating persona that handed its
/// battery to a background helper and ended its turn to wait then exits with no verdict line,
/// and the host records that verdict-less stdout as the spawn's result. With the ceiling at
/// `0` the session stays open, the helper's result arrives, the persona takes its turn on it
/// and records the verdict. The wait stays bounded by the harness's own limits: a subagent
/// that streams no progress for its stall timeout (`CLAUDE_CODE_ASYNC_AGENT_STALL_TIMEOUT_MS`,
/// ten minutes by default) is aborted and reported to the parent, which then takes a turn,
/// and a background command runs under its own time limit; a healthy helper that runs for an
/// hour holds the session open, which is the behaviour wanted - the work is in flight.
///
/// The rejected lever: `CLAUDE_CODE_DISABLE_BACKGROUND_TASKS=1` disables background work
/// entirely, the `run_in_background` parameter on Bash and subagent tools and
/// auto-backgrounding included, and a command that reaches the foreground timeout then stops
/// instead of moving to the background - so every persona's long build battery, which runs as
/// a background or auto-backgrounded command, would stop at the foreground cap.
///
/// Sources: <https://code.claude.com/docs/en/headless.md> (the print-mode wait on background
/// work and its ceiling) and <https://code.claude.com/docs/en/env-vars.md> (the variable,
/// its ten-minute default and `0` waiting without one).
pub(crate) fn harness_env() -> [(&'static str, &'static str); 1] {
    [("CLAUDE_CODE_PRINT_BG_WAIT_CEILING_MS", "0")]
}

/// The file a spawn's system prompt (its persona) is written to, inside the spawn's scratch.
const SYSTEM_PROMPT_FILE: &str = "system-prompt.md";

/// A spawn's system prompt, written to a file the headless session reads through
/// `--system-prompt-file`, and removed when this value drops. A host holds it until its spawn
/// ends, so the session can read the file whenever it starts.
///
/// The persona never travels as an argv string because Linux caps any ONE argument at
/// `MAX_ARG_STRLEN` (32 pages, 131072 bytes on a 4 KiB-page system): a longer one fails
/// `execve` with `E2BIG` before the session starts, whatever the total argv size. A full
/// persona can cross that limit, so every spawn takes the same path whatever its size.
///
/// Its default holds no file: a spawn with no system prompt.
#[derive(Default)]
pub struct SystemPromptFile {
    /// The absolute path of the written file; `None` when the spawn has no system prompt.
    path: Option<PathBuf>,
}

impl SystemPromptFile {
    /// The `--system-prompt-file <path>` pair this spawn passes, or none for an empty system
    /// prompt (the session then runs with its default one).
    pub fn args(&self) -> Vec<String> {
        self.path.as_ref().map_or_else(Vec::new, |path| {
            vec![
                "--system-prompt-file".to_string(),
                path.to_string_lossy().into_owned(),
            ]
        })
    }
}

impl Drop for SystemPromptFile {
    fn drop(&mut self) {
        if let Some(path) = &self.path {
            let _ = std::fs::remove_file(path);
        }
    }
}

/// The ONE prompt-delivery authority for the system prompt, whichever host launches the
/// spawn: write `system_prompt` to `<spawn_scratch>/system-prompt.md` and return the guard
/// that owns it ([`SystemPromptFile`]). An empty `system_prompt` writes nothing.
///
/// `spawn_scratch` is the spawn's own scratch directory
/// ([`replay::spawn_scratch_path`]), created here when absent; the spawn's terminus reclaims
/// that directory whole, so the file shares its lifecycle (and the orphan sweep covers a host
/// that crashed mid-spawn). A spawn with no scratch directory - a spawn with no worktree on
/// the cli host, or any spawn of a run with no repository - writes a uniquely named file in
/// the system temp directory instead. Either way the guard removes the file when it drops.
/// The path is absolute: the session runs in the spawn's worktree, not the host's cwd.
pub(crate) fn system_prompt_file(
    system_prompt: &str,
    spawn_scratch: Option<PathBuf>,
) -> Result<SystemPromptFile, Error> {
    if system_prompt.is_empty() {
        return Ok(SystemPromptFile { path: None });
    }
    let (dir, name) = match spawn_scratch {
        Some(dir) => (dir, SYSTEM_PROMPT_FILE.to_string()),
        None => (
            std::env::temp_dir(),
            format!("rigger-system-prompt-{}.md", uuid::Uuid::new_v4()),
        ),
    };
    let write = || -> std::io::Result<PathBuf> {
        std::fs::create_dir_all(&dir)?;
        let path = std::path::absolute(dir.join(name))?;
        std::fs::write(&path, system_prompt)?;
        Ok(path)
    };
    let path = write().map_err(|e| {
        Error(format!(
            "driver: write the spawn's system prompt under {}: {e}",
            dir.display()
        ))
    })?;
    Ok(SystemPromptFile { path: Some(path) })
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
