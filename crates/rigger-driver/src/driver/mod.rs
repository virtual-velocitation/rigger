//! Agent drivers: adapters implementing the conductor's AgentDriver port. `claude_code` is the
//! headless-session host `rigger run` and `rigger critique` launch their agents through;
//! `workflow` is the in-Claude-Code alternative; `cli` is the blocking one-shot host `rigger
//! canary` probes with; `replay` is the stepwise path `rigger step` drives.
//!
//! Both process hosts launch a headless `claude` session, so what such a session needs
//! beyond its protocol (permissions, MCP server, settings, helpers) is composed here once,
//! by [`spawn_config_args`], and each host's own `build_args` appends it.

use std::path::PathBuf;

use crate::agent::{Error, SpawnHost, SpawnOpts};
use crate::config::AgentDef;
use crate::hooks;
use crate::progress::SpawnLaunched;

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
/// construction, §3.1, §6), then the rigger MCP tools of the helpers `host` carries
/// ([`hooks::helper_mcp_tools`]), then the `bound` tools the launching host's own server serves,
/// each name once. The helpers' tools are pre-approved because under `--permission-prompts none`
/// nothing answers a prompt, so a `lookup` would otherwise be denied every graph call it makes,
/// and a host's `bound` tools for the same reason. Always present; fails when a helper `host`
/// carries does not parse.
pub(crate) fn allowed_tools_args(
    agent: &AgentDef,
    host: &SpawnHost,
    bound: &[&str],
) -> Result<Vec<String>, Error> {
    let helpers = hooks::helper_mcp_tools(&host.helpers)
        .map_err(|e| Error(format!("driver: compose the spawn's allowed tools: {e}")))?;
    let mut tools = agent.allowed_tools();
    for tool in helpers
        .into_iter()
        .chain(bound.iter().map(|tool| tool.to_string()))
    {
        if !tools.contains(&tool) {
            tools.push(tool);
        }
    }
    Ok(vec!["--allowed-tools".to_string(), tools.join(",")])
}

/// `task` framed with the progress verb every process host gives (spec 14): the separator, then
/// one line naming how a worker records a progress line on this host. `report` is that verb and
/// its arguments, completing the sentence "record a progress line ...". When to report is the
/// `communication` built-in's.
pub(crate) fn progress_framed(task: &str, report: &str) -> String {
    format!("{task}\n\n--- rigger driver ---\nLIVE PROGRESS: record a progress line {report}.")
}

/// One composed configuration as the inline JSON argument its flag takes, or the error naming
/// which configuration could not be composed.
fn json_arg(composed: Result<Vec<u8>, hooks::Error>, what: &str) -> Result<String, Error> {
    composed
        .and_then(|bytes| String::from_utf8(bytes).map_err(|e| hooks::Error(e.to_string())))
        .map_err(|e| Error(format!("driver: compose the spawn's {what}: {e}")))
}

/// The environment variable a worker reads the spawn it runs as from: the host exports the
/// spawn's id under it ([`harness_env`]), and the worker's own `rigger emit` stamps that id as the
/// record's author when no `--spawn` names one, exactly as the spawn-bound MCP server stamps a
/// `rigger_emit` call. The emit core judges a record id's ownership by that stamp.
pub const SPAWN_ENV: &str = "RIGGER_SPAWN";

/// The harness environment every headless worker gets, whichever host launches it: the
/// `(key, value)` pairs each host applies to the spawned `claude -p` process of spawn `spawn` at
/// its env injection site, BEFORE the build environment (`SpawnOpts::env`). `Command::env` is
/// last-write-wins per key, so a value the operator puts in the build environment under the
/// same key overrides the one here.
///
/// Two pairs. [`SPAWN_ENV`] carries `spawn`, set even when empty so a worker never inherits the
/// id of a spawn that launched its host. `CLAUDE_CODE_PRINT_BG_WAIT_CEILING_MS=0` is the other,
/// and the rest of this comment is about it. In print mode a session
/// that ended its final turn with a background subagent still running (one dispatched without
/// waiting on it) stays open until that subagent completes, but only for a default ceiling of
/// ten minutes of continuous idle waiting; past it the harness stops whatever is still running
/// and DROPS its partial result. A gating persona that handed its battery to a background
/// subagent and ended its turn to wait then exits with no verdict line, and the host records
/// that verdict-less stdout as the spawn's result. With the ceiling at `0` the session stays
/// open, the subagent's result arrives, the persona takes its turn on it and records the
/// verdict. The wait stays bounded by the harness's own limits: a subagent that streams no
/// progress for its stall timeout (`CLAUDE_CODE_ASYNC_AGENT_STALL_TIMEOUT_MS`, ten minutes by
/// default) is aborted and reported to the parent, which then takes a turn.
///
/// A background command is different and depends on the Claude Code version. Versions 2.1.163
/// through 2.1.291 stop a background command still running about five seconds after the final
/// result once stdin has closed, whatever the ceiling (measured at 5.2 to 5.3 seconds on
/// 2.1.290 in seven spawns); 2.1.292 and later wait for it until it exits or reaches its own
/// time limit. The loop relies on neither wait: the working discipline's `Your turn's end is
/// your session's end` section requires every job an agent starts to finish inside its turn,
/// so the exit-time wait is a safety net at most.
///
/// The rejected lever: `CLAUDE_CODE_DISABLE_BACKGROUND_TASKS=1` disables background work
/// entirely, the `run_in_background` parameter on Bash and subagent tools and
/// auto-backgrounding included. With it, a command that reaches the foreground timeout stops
/// instead of moving to the background, so a battery that outlives the timeout could never be
/// polled to its exit line as the working discipline prescribes.
///
/// Sources: <https://code.claude.com/docs/en/headless.md> (the print-mode wait on background
/// work and its ceiling) and <https://code.claude.com/docs/en/env-vars.md> (the variable,
/// its ten-minute default and `0` waiting without one).
pub(crate) fn harness_env(spawn: &str) -> [(&'static str, &str); 2] {
    [
        (SPAWN_ENV, spawn),
        ("CLAUDE_CODE_PRINT_BG_WAIT_CEILING_MS", "0"),
    ]
}

/// What `claude -p --resume <id>` prints on stderr, exiting non-zero before any turn runs, when
/// no transcript of session `<id>` exists. This is Claude Code's own wording, matched as text
/// because the refusal carries no structured code: pinned against Claude Code 2.1.290 by the
/// 2026-10-07 probe (`claude -p --resume <unknown uuid> < /dev/null` printed `No conversation
/// found with session ID: <id>`, empty stdout, exit 1), whose literal output the cli host's
/// `a_missing_session_is_read_from_claude_codes_own_refusal` test replays. A later Claude Code
/// that rewords it turns the cli host's fallback off - the resume then fails as an agent error -
/// so re-probe on an upgrade. The cli host, which asked for a resume and reads this, falls back
/// to a fresh launch with the full task. The stream-json host reads no text: a resume that
/// yields no `system/init` falls back whatever it printed.
pub(crate) const MISSING_SESSION: &str = "No conversation found with session ID";

/// The Claude Code session one launch runs as - the ONE session authority both process hosts
/// share. A launch continues the session [`SpawnOpts::resumed_from`] names (`--resume <id>`), or
/// starts a fresh one under an id the host mints (`--session-id <uuid>`), so the host knows the id
/// without reading it back from the session's output; a resumed session keeps its id, so
/// [`id`](Self::id) is the session either way. The stream-json host's relaunch of an interrupted
/// launch ([`relaunch`](Self::relaunch)) passes both, forking the session that launch ran as into
/// one under a minted id (`--resume <session> --fork-session --session-id <uuid>`; the host CLI
/// refuses `--session-id` beside `--resume` without `--fork-session`), so an orphan of a
/// supervisor that died without unwinding keeps the interrupted transcript to itself.
///
/// A resume carries the rest of the argv unchanged. The session snapshots its system prompt on
/// its first request and reuses that record on a resume, so the persona file passed again is
/// read only once the conversation is compacted; but the tools it may use
/// (`--allowed-tools`), its permission mode and its MCP servers are per-launch, never part of
/// the session - a resume that omitted them ran with none of them - so they are passed again.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LaunchSession {
    /// The session id this launch runs as.
    pub id: String,
    /// The session this launch continues: `id` on a resume, the session the interrupted launch
    /// ran as on a relaunch, which forks it into `id`, and `None` on a fresh launch.
    continues: Option<String>,
    /// Whether the launch sends [`SpawnOpts::resume_task`]: a resume of the session the spawn
    /// names does, and a relaunch does when the launch it continues carried it.
    sends_resume_task: bool,
}

/// Whether the launch `launched` records carried the resume task of a spawn resuming `named`
/// (spec 105, RESUME BY SESSION), read along its chain of sessions over `records`, the spawn's
/// launch records in this run. A record whose session or `resumed_from` is a non-empty `named`
/// carried it. One whose `resumed_from` names another session carried it when the open record of
/// the launch that ran as that session did, since each record names only the session it
/// continued, so a third relaunch reads back through two; a launch runs as the session its open
/// record names, or as the one its close names when its `system/init` named another. A fresh
/// launch's record, the fallback's among them, and one whose `resumed_from` names a session no
/// launch ran as did not. The walk takes at most one step a record, so a cycle among them ends it
/// uncarried.
fn carried_resume_task(launched: &SpawnLaunched, records: &[SpawnLaunched], named: &str) -> bool {
    let continued = |record: &SpawnLaunched| {
        let session = record
            .resumed_from
            .as_deref()
            .filter(|session| !session.is_empty())?;
        let ran = records
            .iter()
            .find(|earlier| earlier.session_id == session)?;
        records
            .iter()
            .find(|earlier| earlier.is_open() && earlier.launch == ran.launch)
    };
    !named.is_empty()
        && std::iter::successors(Some(launched), |record| continued(record))
            .take(records.len() + 1)
            .any(|record| {
                record.session_id == named || record.resumed_from.as_deref() == Some(named)
            })
}

impl LaunchSession {
    /// The session `opts` asks for: a resume of [`SpawnOpts::resumed_from`], which never forks,
    /// or a freshly minted one.
    pub fn of(opts: &SpawnOpts) -> LaunchSession {
        if opts.resumed_from.is_empty() {
            LaunchSession::fresh()
        } else {
            let id = opts.resumed_from.clone();
            LaunchSession::new(id.clone(), Some(id), true)
        }
    }

    /// A fresh session under a newly minted id.
    pub fn fresh() -> LaunchSession {
        LaunchSession::new(uuid::Uuid::new_v4().to_string(), None, false)
    }

    /// The session `id`, continuing `continues` and sending [`SpawnOpts::resume_task`] when
    /// `sends_resume_task`: the one constructor the others name.
    fn new(id: String, continues: Option<String>, sends_resume_task: bool) -> LaunchSession {
        LaunchSession {
            id,
            continues,
            sends_resume_task,
        }
    }

    /// The relaunch of the interrupted launch `launched` records, closed by `interrupted`, for
    /// the spawn `opts` whose launch records in this run are `records`: a fork of the session that
    /// launch ran as, the one `interrupted` names (another than `launched` names when its
    /// `system/init` named another), under a newly minted id, sending [`SpawnOpts::resume_task`]
    /// when that launch carried it ([`carried_resume_task`]), so the task keeps travelling along a
    /// chain of relaunches, each itself interrupted and relaunched.
    pub fn relaunch(
        launched: &SpawnLaunched,
        interrupted: &SpawnLaunched,
        records: &[SpawnLaunched],
        opts: &SpawnOpts,
    ) -> LaunchSession {
        let carried = carried_resume_task(launched, records, &opts.resumed_from);
        let minted = LaunchSession::fresh().id;
        LaunchSession::new(minted, Some(interrupted.session_id.clone()), carried)
    }

    /// Whether the launch continues a session rather than starting one: a resume or a relaunch.
    pub fn resumed(&self) -> bool {
        self.continues.is_some()
    }

    /// The session flags this launch passes: `--session-id <id>` on a fresh launch,
    /// `--resume <id>` on a resume, and `--resume <session> --fork-session --session-id <id>` on
    /// a relaunch of `session`.
    pub fn args(&self) -> Vec<String> {
        let (resume, session) = ("--resume".to_string(), "--session-id".to_string());
        match &self.continues {
            None => vec![session, self.id.clone()],
            Some(continued) if *continued == self.id => vec![resume, self.id.clone()],
            Some(continued) => vec![
                resume,
                continued.clone(),
                "--fork-session".to_string(),
                session,
                self.id.clone(),
            ],
        }
    }

    /// What the launch sends of the spawn's task: the full `prompt` when it starts fresh,
    /// [`SpawnOpts::resume_task`] when it continues a session that carried it (the session
    /// already holds the rest), and nothing otherwise. A relaunch puts its interruption note in
    /// front of it.
    pub fn task<'a>(&self, prompt: &'a str, opts: &'a SpawnOpts) -> &'a str {
        if !self.resumed() {
            prompt
        } else if self.sends_resume_task {
            &opts.resume_task
        } else {
            ""
        }
    }

    /// The session the launch continued, as the spawn's result records it
    /// ([`AgentResult::resumed_from`](crate::agent::AgentResult::resumed_from)): `id` on a
    /// resume, the interrupted launch's session on a relaunch, empty on a fresh launch.
    pub fn resumed_from(&self) -> String {
        self.continues.clone().unwrap_or_default()
    }
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
/// - `--settings` is `settings_json` (may be empty), with `advisorModel` set to the empty string
///   when `agent` runs without the advisor ([`AgentDef::without_advisor`]), with the SessionStart
///   prime hook and the status line merged in, and the PreToolUse grep-guard when `host` carries it
///   ([`hooks::install_session_settings`]). The host hands in what its spawn runs on: the
///   stream-json host the spawn's own settings with the hooks bound to it already merged
///   ([`hooks::install_spawn_hooks`]), the cli host the spawn's own settings alone;
/// - `--agents` defines the `lookup` and `verify` fan-out helpers `host` carries
///   ([`hooks::helper_agents_json`]): the operator's copy of each where the checkout holds one.
///
/// Fails when `settings_json` is not a JSON object the session settings can merge into, or a
/// helper `host` carries does not parse.
pub(crate) fn spawn_config_args(
    agent: &AgentDef,
    rigger_bin: &str,
    mcp_args: &[&str],
    settings_json: &str,
    host: &SpawnHost,
) -> Result<Vec<String>, Error> {
    let helpers = hooks::helper_agents_json(&host.helpers)
        .map_err(|e| Error(format!("driver: compose the spawn's agents: {e}")))?;
    let own_settings = if agent.without_advisor() {
        json_arg(
            hooks::install_no_advisor(settings_json.as_bytes()),
            "settings",
        )?
    } else {
        settings_json.to_string()
    };
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
            hooks::install_session_settings(own_settings.as_bytes(), host.grep_guard),
            "settings",
        )?,
        "--agents".to_string(),
        helpers.to_string(),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::HelperCopy;
    use crate::test_support::{operator_helpers_agents_json, test_spawn_host};

    /// `allowed_tools_args` over every order and repeat a grant can arrive in: the agent's own
    /// tools, then the host's helpers', then the host's bound ones, each name once where it first
    /// came.
    #[test]
    fn allowed_tools_chain_the_agent_the_helpers_and_the_bound_tools_each_name_once() {
        let host = test_spawn_host();
        let helpers = hooks::helper_mcp_tools(&host.helpers).unwrap();
        struct Case<'a> {
            name: &'a str,
            tools: Vec<&'a str>,
            bound: Vec<&'a str>,
            after_the_helpers: Vec<&'a str>,
        }
        let cases = [
            Case {
                name: "no bound tools adds nothing after the helpers",
                tools: vec!["Read"],
                bound: vec![],
                after_the_helpers: vec![],
            },
            Case {
                name: "bound tools follow the helpers in their own order",
                tools: vec!["Read"],
                bound: vec!["bound-a", "bound-b"],
                after_the_helpers: vec!["bound-a", "bound-b"],
            },
            Case {
                name: "an agent that lists a bound tool keeps it at its own place",
                tools: vec!["bound-a", "Read"],
                bound: vec!["bound-a", "bound-b"],
                after_the_helpers: vec!["bound-b"],
            },
            Case {
                name: "a bound tool a helper declares is not repeated",
                tools: vec![],
                bound: vec![helpers[0].as_str(), "bound-a"],
                after_the_helpers: vec!["bound-a"],
            },
            Case {
                name: "a repeated bound tool lands once",
                tools: vec![],
                bound: vec!["bound-a", "bound-a"],
                after_the_helpers: vec!["bound-a"],
            },
        ];
        for case in cases {
            let agent = AgentDef {
                tools: case.tools.iter().map(|t| t.to_string()).collect(),
                ..AgentDef::default()
            };
            let expected: Vec<&str> = case
                .tools
                .iter()
                .copied()
                .chain(helpers.iter().map(String::as_str))
                .chain(case.after_the_helpers.iter().copied())
                .collect();
            assert_eq!(
                allowed_tools_args(&agent, &host, &case.bound).unwrap(),
                ["--allowed-tools".to_string(), expected.join(",")],
                "{}",
                case.name
            );
        }
    }

    /// Both readers of the spawn host's helpers read the copies the host carries and nothing
    /// else: `spawn_config_args` builds `--agents` from them and `allowed_tools_args` pre-approves
    /// the rigger tools they declare, and a copy that does not parse fails both, naming it.
    #[test]
    fn the_agents_and_the_pre_approval_are_the_helpers_the_host_carries() {
        let operator = test_spawn_host().helpers;
        let operator_json = operator_helpers_agents_json();
        struct Row {
            name: &'static str,
            helpers: Vec<HelperCopy>,
            agents: Result<serde_json::Value, &'static str>,
            allowed: Result<&'static str, &'static str>,
        }
        let rows = [
            Row {
                name: "both operator copies",
                helpers: operator.clone(),
                agents: Ok(operator_json.clone()),
                allowed: Ok("Read,mcp__rigger__rigger_peers,mcp__rigger__rigger_graph"),
            },
            Row {
                name: "the verify copy alone declares no rigger tool",
                helpers: vec![operator[1].clone()],
                agents: Ok(serde_json::json!({ "verify": operator_json["verify"].clone() })),
                allowed: Ok("Read"),
            },
            Row {
                name: "a copy with no name",
                helpers: vec![HelperCopy {
                    text: "---\nmodel: haiku\n---\nbody\n".to_string(),
                    ..operator[0].clone()
                }],
                agents: Err("conductor: driver: compose the spawn's agents: \
                             hooks: helper agent lookup.md: no name"),
                allowed: Err("conductor: driver: compose the spawn's allowed tools: \
                              hooks: helper agent lookup.md: no name"),
            },
        ];
        let agent = AgentDef {
            tools: vec!["Read".to_string()],
            ..AgentDef::default()
        };
        for row in rows {
            let host = SpawnHost {
                helpers: row.helpers,
                ..test_spawn_host()
            };
            let agents = spawn_config_args(&AgentDef::default(), "rigger", &["mcp"], "", &host)
                .map(|args| {
                    let at = args.iter().position(|a| a == "--agents").unwrap();
                    serde_json::from_str::<serde_json::Value>(&args[at + 1]).unwrap()
                });
            assert_eq!(
                agents.map_err(|e| e.to_string()),
                row.agents.map_err(str::to_string),
                "{}",
                row.name
            );
            assert_eq!(
                allowed_tools_args(&agent, &host, &[]).map_err(|e| e.to_string()),
                row.allowed
                    .map(|tools| vec!["--allowed-tools".to_string(), tools.to_string()])
                    .map_err(str::to_string),
                "{}",
                row.name
            );
        }
    }

    /// Whichever host launches the spawn, its one MCP server is marked to load every tool it lists
    /// into the session's prompt up front, so the graph tools are callable from the first turn
    /// instead of sitting behind the host's tool search; the mark sits on rigger's own entry, the
    /// only server the spawn configures.
    #[test]
    fn every_hosts_spawn_loads_the_rigger_tools_up_front() {
        let rows: [(&str, &[&str]); 2] = [
            ("the stream-json host's spawn-bound server", &["mcp", "--spawn", "u/implementer#0"]),
            ("the cli host's read-only server", &["mcp"]),
        ];
        for (name, mcp_args) in rows {
            let args = spawn_config_args(
                &AgentDef::default(),
                "rigger",
                mcp_args,
                "",
                &test_spawn_host(),
            )
            .unwrap();
            let at = args.iter().position(|a| a == "--mcp-config").unwrap();
            let cfg: serde_json::Value = serde_json::from_str(&args[at + 1]).unwrap();
            let servers = cfg["mcpServers"].as_object().unwrap();
            assert_eq!(servers.len(), 1, "{name}: one server alone: {cfg}");
            assert_eq!(
                servers[hooks::MCP_SERVER_NAME]["alwaysLoad"],
                serde_json::json!(true),
                "{name}: rigger's tools load up front: {cfg}"
            );
        }
    }
}
