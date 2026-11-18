//! Installs Rigger's Claude Code integration: merges a SessionStart hook into
//! .claude/settings.json - preserving every other setting - so a session opened
//! in a Rigger repository starts primed with the project's recent decisions.
//!
//! This is also the one home of WHAT a rigger session carries: the hook and status-line
//! commands, the MCP server name and the fan-out helper agents. `rigger setup` writes them into
//! the operator's checkout; the headless host hands the same values to every spawn on its
//! command line (a fresh unit worktree carries no `.claude/` of its own), so the two cannot
//! drift. The fan-out helpers are the operator's to tune: setup writes each only where the
//! checkout holds no copy, and [`resolve_spawn_host`] reads the checkout's copies back
//! ([`resolved_helpers`]), so the copy is the definition every spawn carries. The hooks bound to
//! one spawn are composed here too, by [`install_spawn_hooks`]: only the headless host hands
//! those over, and `rigger setup` never writes them. The PreToolUse grep-guard is the workflow's
//! to declare (`defaults.grep_guard`): [`resolve_spawn_host`] reads the key once per command for
//! every spawn, and `rigger setup` installs the guard ([`install_grep_guard`]) or removes
//! rigger's own entry ([`remove_grep_guard`]) to match.

use std::path::Path;

use rigger_domain::quoting::single_quoted;
use rigger_domain::review::EDITING_TOOLS;
use serde_json::{json, Map, Value};

use crate::agent::{AgentFailure, HelperCopy, SpawnHost, TreeDuty};

#[derive(Debug, thiserror::Error)]
#[error("hooks: {0}")]
pub struct Error(pub String);

/// The command the SessionStart hook runs: prime the session with the instructions in force
/// and the project's recent decisions.
pub const PRIME_COMMAND: &str = "rigger prime";

/// The matcher and command the graph-first lookup hook installs under `PreToolUse` (spec 92,
/// criterion 4). Fires on the built-in `Grep` tool and on `Bash` (a `grep` may be buried inside
/// an arbitrary shell command); the real narrowing happens in `rigger grep-guard` itself, so a
/// Bash call that is not a grep at all is a silent allow, never a false bounce.
pub const GREP_GUARD_MATCHER: &str = "Grep|Bash";
/// See [`GREP_GUARD_MATCHER`].
pub const GREP_GUARD_COMMAND: &str = "rigger grep-guard";

/// The tools the write guard judges, every one of [`EDITING_TOOLS`], as the matcher it installs
/// under `PreToolUse`.
fn write_guard_matcher() -> String {
    EDITING_TOOLS.map(|(tool, _)| tool).join("|")
}

/// The editor's status line command (spec 94, criterion 5): the same one-line summary
/// `rigger status` prints first.
pub const STATUS_LINE_COMMAND: &str = "rigger status --line";

/// The name rigger's MCP server is registered under, so its tools are `mcp__rigger__*`.
pub const MCP_SERVER_NAME: &str = "rigger";

/// The fan-out helpers every persona dispatches (the built-in working discipline names them):
/// `lookup`, one Haiku instance per graph node, and `verify`, one Sonnet instance for builds
/// and test runs. The text is this repository's own committed `.claude/agents/` helpers: what
/// `rigger init` and `rigger setup` scaffold where a checkout holds no copy, and what a spawn
/// carries for a helper whose copy the checkout lacks ([`resolved_helpers`]).
pub const HELPER_AGENTS: &[(&str, &str)] = &[
    (
        "lookup.md",
        include_str!("../../../.claude/agents/lookup.md"),
    ),
    (
        "verify.md",
        include_str!("../../../.claude/agents/verify.md"),
    ),
];

/// The directory, relative to a checkout's root, holding the operator's copies of the
/// [`HELPER_AGENTS`]: where `rigger setup` scaffolds them and [`resolved_helpers`] reads them.
pub const HELPER_AGENTS_DIR: &str = ".claude/agents";

/// The host values every spawn of one command carries, resolved once from the workflow's
/// `defaults` and the checkout at `repo_root`: the PreToolUse grep-guard exactly when
/// `defaults.grep_guard` declares it, and the fan-out helpers [`resolved_helpers`] reads there.
/// Fails as [`resolved_helpers`] does.
pub fn resolve_spawn_host(
    defaults: &rigger_domain::config::Defaults,
    repo_root: &Path,
) -> Result<SpawnHost, Error> {
    Ok(SpawnHost {
        grep_guard: defaults.grep_guard,
        helpers: resolved_helpers(repo_root)?,
    })
}

/// The fan-out helpers a spawn of the checkout at `root` carries, one per [`HELPER_AGENTS`] entry
/// in its order: the operator's copy under [`HELPER_AGENTS_DIR`] where the checkout holds one,
/// whatever its text, else the embedded text. A missing file, or an agents path that is no
/// directory, holds no copy; a copy that cannot be read, does not parse as a helper definition,
/// or names the helper another file defines, fails naming its path and never falls back to the
/// embedded text, so `--agents` and the tool pre-approval read one helper per name.
pub fn resolved_helpers(root: &Path) -> Result<Vec<HelperCopy>, Error> {
    let mut named: Vec<(String, std::path::PathBuf, HelperCopy)> = Vec::new();
    for (file, embedded) in HELPER_AGENTS {
        let path = root.join(HELPER_AGENTS_DIR).join(file);
        let (text, present) = match std::fs::read_to_string(&path) {
            Ok(text) => (text, true),
            Err(e) if is_absent(&e) => (embedded.to_string(), false),
            Err(e) => return Err(helper_error(path.display(), e)),
        };
        let (name, _) = parse_helper(path.display(), &text)?;
        if let Some((_, first_path, first)) = named.iter().find(|(seen, ..)| *seen == name) {
            // The embedded helpers name themselves apart, so of the two one is an operator's copy:
            // the error names that copy, and the other file.
            let (copy, other) = if present {
                (&path, first.name.as_str())
            } else {
                (first_path, *file)
            };
            return Err(helper_error(
                copy.display(),
                format!("the name {name:?} is also {other}'s"),
            ));
        }
        named.push((
            name,
            path,
            HelperCopy {
                name: file.to_string(),
                text,
                present,
            },
        ));
    }
    Ok(named.into_iter().map(|(.., copy)| copy).collect())
}

/// Whether the failed read `e` found no file at the path: nothing there, or a path component
/// that is no directory. The one test for "an operator's copy that is not there is no copy",
/// read wherever a project's own file shadows a shipped default.
pub fn is_absent(e: &std::io::Error) -> bool {
    matches!(
        e.kind(),
        std::io::ErrorKind::NotFound | std::io::ErrorKind::NotADirectory
    )
}

/// The error a helper definition fails with: the helper `label` names, then its `reason`.
fn helper_error(label: impl std::fmt::Display, reason: impl std::fmt::Display) -> Error {
    Error(format!("helper agent {label}: {reason}"))
}

/// The helper definition `text` holds, as `--agents` keys it: its name and its fields, its
/// frontmatter read as the YAML Claude Code reads (`tools` as a list, whether written as one or
/// as a comma-separated string) and its body as the prompt. A text that does not parse, or whose
/// frontmatter names no helper, fails naming `label`.
fn parse_helper(
    label: impl std::fmt::Display,
    text: &str,
) -> Result<(String, Map<String, Value>), Error> {
    let (front, body) =
        rigger_domain::config::split_frontmatter(text).map_err(|e| helper_error(&label, e))?;
    let mut def: Map<String, Value> = rigger_domain::config::parse_yaml_naming_unknown_keys(front)
        .map_err(|e| helper_error(&label, e))?;
    let Some(Value::String(name)) = def.remove("name") else {
        return Err(helper_error(&label, "no name"));
    };
    if let Some(Value::String(tools)) = def.get("tools") {
        let tools: Vec<Value> = tools.split(',').map(|tool| tool.trim().into()).collect();
        def.insert("tools".to_string(), tools.into());
    }
    def.insert("prompt".to_string(), body.into());
    Ok((name, def))
}

/// Merge the session settings rigger installs - the SessionStart prime hook and the status line,
/// and the PreToolUse grep-guard when `grep_guard` is set - into `existing` settings JSON, through
/// the same installers `rigger setup` runs one by one, preserving every other setting. With
/// `grep_guard` unset it removes nothing: settings already carrying the guard keep it; set, the
/// guard's installer replaces an entry under a drifted matcher ([`install_grep_guard`]).
/// `existing` may be empty.
pub fn install_session_settings(existing: &[u8], grep_guard: bool) -> Result<Vec<u8>, Error> {
    let mut merged = install_session_start(existing, PRIME_COMMAND)?;
    if grep_guard {
        merged = install_grep_guard(&merged)?;
    }
    install_status_line(&merged, STATUS_LINE_COMMAND)
}

/// Merge the PreToolUse grep-guard into `existing` settings JSON, preserving every other
/// setting: an inner hook running [`GREP_GUARD_COMMAND`] under any matcher but
/// [`GREP_GUARD_MATCHER`] is stripped first, its block dropped once it empties, so exactly one
/// entry under the current matcher remains. `existing` may be empty.
pub fn install_grep_guard(existing: &[u8]) -> Result<Vec<u8>, Error> {
    let mut settings = parse_settings(existing)?;
    strip_grep_guard(&mut settings, |matcher| matcher != Some(GREP_GUARD_MATCHER));
    push_hook_block(
        &mut settings,
        "PreToolUse",
        GREP_GUARD_MATCHER,
        GREP_GUARD_COMMAND,
        None,
        None,
    )?;
    encode_settings(&settings)
}

/// Remove rigger's own PreToolUse grep-guard from `existing` settings JSON: every inner hook
/// running [`GREP_GUARD_COMMAND`], under any matcher, is stripped, each block it empties dropped,
/// then the `PreToolUse` array, then the `hooks` object, each once it empties; every other hook
/// and setting is kept. Settings carrying no guard come back as the very bytes they were, so a
/// file that never carried it is never rewritten. `existing` may be empty.
pub fn remove_grep_guard(existing: &[u8]) -> Result<Vec<u8>, Error> {
    let mut settings = parse_settings(existing)?;
    if !strip_grep_guard(&mut settings, |_| true) {
        return Ok(existing.to_vec());
    }
    if let Some(hooks) = settings.get_mut("hooks").and_then(Value::as_object_mut) {
        if hooks
            .get("PreToolUse")
            .and_then(Value::as_array)
            .is_some_and(Vec::is_empty)
        {
            hooks.remove("PreToolUse");
        }
        if hooks.is_empty() {
            settings.remove("hooks");
        }
    }
    encode_settings(&settings)
}

/// Whether the settings JSON `existing` carries the PreToolUse grep-guard: an inner `PreToolUse`
/// hook running [`GREP_GUARD_COMMAND`], under any matcher. Unparseable settings carry none.
pub fn carries_grep_guard(existing: &[u8]) -> bool {
    serde_json::from_slice::<Value>(existing).is_ok_and(|settings| {
        settings
            .pointer("/hooks/PreToolUse")
            .and_then(Value::as_array)
            .is_some_and(|blocks| has_command(blocks, GREP_GUARD_COMMAND))
    })
}

/// Strip every inner `PreToolUse` hook running [`GREP_GUARD_COMMAND`] from the blocks whose
/// matcher `strips` accepts, dropping each block it empties; whether it stripped any. Settings
/// with no `PreToolUse` array carry nothing to strip.
fn strip_grep_guard(
    settings: &mut Map<String, Value>,
    strips: impl Fn(Option<&str>) -> bool,
) -> bool {
    let Some(blocks) = settings
        .get_mut("hooks")
        .and_then(Value::as_object_mut)
        .and_then(|hooks| hooks.get_mut("PreToolUse"))
        .and_then(Value::as_array_mut)
    else {
        return false;
    };
    let mut stripped = false;
    blocks.retain_mut(|block| {
        if !strips(block.get("matcher").and_then(Value::as_str)) {
            return true;
        }
        let Some(inner) = block.get_mut("hooks").and_then(Value::as_array_mut) else {
            return true;
        };
        let before = inner.len();
        inner.retain(|hook| !runs(hook, GREP_GUARD_COMMAND));
        let removed = inner.len() < before;
        stripped |= removed;
        !(removed && inner.is_empty())
    });
    stripped
}

/// The seconds Claude Code gives a spawn's `Stop` hook (`rigger hook stop`) before it cancels
/// the hook and lets the session stop. The hook only reads git, so a minute is far past any
/// answer it gives; without it a hung read would hold the session for Claude Code's ten-minute
/// default.
pub const STOP_HOOK_TIMEOUT_SECS: u32 = 60;

/// Merge the hooks bound to one spawn into `existing` settings JSON, preserving every other
/// setting: under `PreToolUse` the write guard, `guard-write` rooted at `roots` in order; under
/// `StopFailure` one entry per [`AgentFailure::CATEGORIES`], `hook stop-failure` recording that
/// category for `spawn`; and, for a spawn the conductor takes something from the tree at `dir`
/// of once its session ends (its `duty`), under `Stop` the `hook stop` asking whether the tree is
/// one that step takes, with [`STOP_HOOK_TIMEOUT_SECS`]: `--branch <branch>` for a tree the
/// conductor commits onto that branch, `--own-commits` for one it lands only the commits of. Each
/// runs `rigger_bin` under `shell`, which reads `rigger_bin`, `spawn`, each root, `dir` and the
/// branch as one word whatever they hold ([`HookShell::word`]), and each entry names `shell` where
/// Claude Code would not pick it alone ([`HookShell::field`]). `existing` may be empty.
pub fn install_spawn_hooks(
    existing: &[u8],
    rigger_bin: &str,
    spawn: &str,
    roots: &[String],
    dir: &str,
    duty: &TreeDuty,
    shell: HookShell,
) -> Result<Vec<u8>, Error> {
    let bin = shell.program(rigger_bin);
    let spawn = shell.word(spawn);
    let field = shell.field();
    let mut guard = format!("{bin} guard-write");
    for root in roots {
        guard.push_str(&format!(" --root {}", shell.word(root)));
    }
    let matcher = write_guard_matcher();
    let mut merged = merge_hook_block(existing, "PreToolUse", &matcher, &guard, None, field)?;
    for category in AgentFailure::CATEGORIES {
        let class = category.as_str();
        let command = format!("{bin} hook stop-failure --spawn {spawn} --class {class}");
        merged = merge_hook_block(&merged, "StopFailure", class, &command, None, field)?;
    }
    let asks = match duty {
        TreeDuty::None => None,
        TreeDuty::Checkpoint { branch } => Some(format!("--branch {}", shell.word(branch))),
        TreeDuty::OwnCommits => Some("--own-commits".to_string()),
    };
    if let Some(asks) = asks {
        let dir = shell.word(dir);
        let command = format!("{bin} hook stop --spawn {spawn} --dir {dir} {asks}");
        let timeout = Some(STOP_HOOK_TIMEOUT_SECS);
        merged = merge_hook_block(&merged, "Stop", "", &command, timeout, field)?;
    }
    Ok(merged)
}

/// The shell Claude Code runs a hook's `command` under, which decides how the command spells
/// each word. Claude Code hands a command to `sh -c` on macOS and Linux, and on Windows to Git
/// Bash, or to PowerShell where Git Bash is not installed, unless the hook's `shell` field names
/// one (<https://code.claude.com/docs/en/hooks>).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HookShell {
    /// POSIX `sh`, Claude Code's own pick on macOS and Linux.
    Posix,
    /// PowerShell, which the hook names, so whether Git Bash is installed changes nothing.
    PowerShell,
}

/// The characters PowerShell reads as a single quote: `'`, U+2018, U+2019, U+201A and U+201B
/// (PowerShell's `CharExtensions.IsSingleQuote`).
const POWERSHELL_SINGLE_QUOTES: [char; 5] = ['\'', '\u{2018}', '\u{2019}', '\u{201A}', '\u{201B}'];

impl HookShell {
    /// The shell the hooks rigger installs on this OS run under: PowerShell on Windows, POSIX
    /// `sh` everywhere else.
    pub const HOST: HookShell = if cfg!(windows) {
        HookShell::PowerShell
    } else {
        HookShell::Posix
    };

    /// `arg` as one word this shell reads back as exactly `arg`: between single quotes, inside
    /// which neither shell expands anything, each single quote `arg` holds spelled as the shell
    /// reads one there. POSIX `sh` reads none, so each `'` closes the quote, is escaped and
    /// reopens it; PowerShell reads a single-quote character doubled as one, as its own
    /// `CodeGeneration.EscapeSingleQuotedStringContent` writes it, for each of
    /// [`POWERSHELL_SINGLE_QUOTES`].
    fn word(self, arg: &str) -> String {
        match self {
            HookShell::Posix => single_quoted(arg, r"'\''"),
            HookShell::PowerShell => {
                let mut quoted = String::from("'");
                for c in arg.chars() {
                    if POWERSHELL_SINGLE_QUOTES.contains(&c) {
                        quoted.push(c);
                    }
                    quoted.push(c);
                }
                quoted.push('\'');
                quoted
            }
        }
    }

    /// The start of a command running the program at `path`: its [`word`](Self::word), behind
    /// PowerShell's call operator `&`, without which PowerShell reads a quoted path as a string.
    fn program(self, path: &str) -> String {
        match self {
            HookShell::Posix => self.word(path),
            HookShell::PowerShell => format!("& {}", self.word(path)),
        }
    }

    /// The `shell` field a hook names this shell by: none for POSIX `sh`, which Claude Code picks
    /// wherever rigger runs it, `powershell` for PowerShell.
    fn field(self) -> Option<&'static str> {
        match self {
            HookShell::Posix => None,
            HookShell::PowerShell => Some("powershell"),
        }
    }
}

/// Whether the settings JSON `existing` already carries every session setting
/// [`install_session_settings`] installs with `grep_guard`: installing them again with the same
/// flag would change nothing, so a guard the flag does not declare is no shortfall. Empty or
/// unparseable settings carry none.
pub fn carries_session_settings(existing: &[u8], grep_guard: bool) -> bool {
    let parse = |bytes: &[u8]| serde_json::from_slice::<Value>(bytes).ok();
    match (
        parse(existing),
        install_session_settings(existing, grep_guard),
    ) {
        (Some(current), Ok(merged)) => parse(&merged) == Some(current),
        _ => false,
    }
}

/// The `helpers` as the JSON object Claude Code's `--agents` flag takes:
/// `{name: {description, model, tools: [..], prompt}}`, each field the value the helper's own
/// YAML frontmatter gives it and the prompt its body, verbatim ([`parse_helper`]). A helper that
/// does not parse fails naming its file.
pub fn helper_agents_json(helpers: &[HelperCopy]) -> Result<Value, Error> {
    let mut agents = Map::new();
    for helper in helpers {
        let (name, def) = parse_helper(&helper.name, &helper.text)?;
        agents.insert(name, Value::Object(def));
    }
    Ok(Value::Object(agents))
}

/// The rigger MCP tools ([`MCP_SERVER_NAME`]'s `mcp__rigger__*`) the `helpers` declare in their
/// frontmatter, in helper then declaration order and each once: what a headless spawn must
/// pre-approve for its `lookup` helper to reach the graph, since under
/// `--permission-prompts none` a tool nobody pre-approved is denied. A helper that does not
/// parse fails naming its file.
pub fn helper_mcp_tools(helpers: &[HelperCopy]) -> Result<Vec<String>, Error> {
    let prefix = format!("mcp__{MCP_SERVER_NAME}__");
    let mut tools: Vec<String> = Vec::new();
    for helper in helpers {
        let (_, def) = parse_helper(&helper.name, &helper.text)?;
        for tool in def
            .get("tools")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            if let Some(tool) = tool.as_str().filter(|t| t.starts_with(&prefix)) {
                if !tools.iter().any(|t| t == tool) {
                    tools.push(tool.to_string());
                }
            }
        }
    }
    Ok(tools)
}

/// Merge a SessionStart hook that runs `command` into the settings JSON. Idempotent
/// (installing twice does not duplicate the hook) and preserves all other settings.
/// `existing` may be empty. A thin wrapper over [`merge_hook_block`] fixed to the
/// `SessionStart` event with an empty matcher (rigger installs the only entry that will
/// ever exist there).
pub fn install_session_start(existing: &[u8], command: &str) -> Result<Vec<u8>, Error> {
    merge_hook_block(existing, "SessionStart", "", command, None, None)
}

/// Merge one hook block that runs `command` into `existing` settings JSON, under the
/// named top-level hook `event_key` (`"SessionStart"`, `"PreToolUse"`, ...), filtered by
/// `matcher` (tool names under `PreToolUse`, a failure class under `StopFailure`, empty for
/// an event `rigger` never matcher-filters, e.g. `SessionStart`), given `timeout` seconds when
/// one is named (Claude Code's own default otherwise) and run under the `shell` it names
/// ([`HookShell::field`]; Claude Code's own pick otherwise). Idempotent (installing the
/// same `command` under the same `event_key` twice does not duplicate the block) and preserves
/// every other setting, every other event key, and every other block already present under
/// THIS event key - a general-purpose event (`PreToolUse`) may already carry entries for a
/// different tool, matcher, or command a person or another tool installed, so this always
/// APPENDS a new block rather than ever replacing the array wholesale, and touches nothing
/// under any matcher/command this call did not itself install. `existing` may be empty. The
/// one mutation authority [`install_session_start`] and [`install_spawn_hooks`] build on, so
/// no event kind can diverge on what "merge a hook block"
/// means; [`install_grep_guard`] pushes its block through the same [`push_hook_block`].
fn merge_hook_block(
    existing: &[u8],
    event_key: &str,
    matcher: &str,
    command: &str,
    timeout: Option<u32>,
    shell: Option<&str>,
) -> Result<Vec<u8>, Error> {
    let mut settings = parse_settings(existing)?;
    push_hook_block(&mut settings, event_key, matcher, command, timeout, shell)?;
    encode_settings(&settings)
}

/// The settings JSON `existing` as its top-level object, empty settings as an empty one.
fn parse_settings(existing: &[u8]) -> Result<Map<String, Value>, Error> {
    let root: Value = if existing.iter().all(u8::is_ascii_whitespace) {
        json!({})
    } else {
        serde_json::from_slice(existing).map_err(|e| Error(format!("parse settings.json: {e}")))?
    };
    match root {
        Value::Object(settings) => Ok(settings),
        _ => Err(Error("settings.json is not a JSON object".into())),
    }
}

/// Append a block running `command` under `matcher` to the `event_key` array of `settings`'
/// `hooks`, creating either when absent, unless a block of that array already runs `command`;
/// the inner hook carries `timeout` seconds and the `shell` when either is named.
fn push_hook_block(
    settings: &mut Map<String, Value>,
    event_key: &str,
    matcher: &str,
    command: &str,
    timeout: Option<u32>,
    shell: Option<&str>,
) -> Result<(), Error> {
    let hooks = settings.entry("hooks").or_insert_with(|| json!({}));
    let hooks_obj = hooks
        .as_object_mut()
        .ok_or_else(|| Error("\"hooks\" is not an object".into()))?;
    let event = hooks_obj.entry(event_key).or_insert_with(|| json!([]));
    let event_arr = event
        .as_array_mut()
        .ok_or_else(|| Error(format!("\"{event_key}\" is not an array")))?;
    if !has_command(event_arr, command) {
        let mut hook = json!({"type": "command", "command": command});
        if let Some(timeout) = timeout {
            hook["timeout"] = json!(timeout);
        }
        if let Some(shell) = shell {
            hook["shell"] = json!(shell);
        }
        event_arr.push(json!({"matcher": matcher, "hooks": [hook]}));
    }
    Ok(())
}

/// `settings` as the bytes of a settings file: pretty-printed, ending in a newline.
fn encode_settings(settings: &Map<String, Value>) -> Result<Vec<u8>, Error> {
    let mut out = serde_json::to_vec_pretty(settings)
        .map_err(|e| Error(format!("encode settings.json: {e}")))?;
    out.push(b'\n');
    Ok(out)
}

/// Whether any block of `blocks` holds an inner hook running `command`.
fn has_command(blocks: &[Value], command: &str) -> bool {
    blocks.iter().any(|block| {
        block
            .get("hooks")
            .and_then(Value::as_array)
            .is_some_and(|inner| inner.iter().any(|hook| runs(hook, command)))
    })
}

/// Whether the inner hook `hook` runs `command`.
fn runs(hook: &Value, command: &str) -> bool {
    hook.get("command").and_then(Value::as_str) == Some(command)
}

/// Merge one stdio MCP server entry into `.mcp.json`'s `mcpServers` object (spec 92,
/// criterion 4: the operator's own Claude Code session gets `rigger_peers` /
/// `rigger_ground` / `rigger_graph` the same way the loop's agents do). Unlike the
/// settings-hook merges above, rigger OWNS this one key exclusively - `name` is always
/// `"rigger"` in production - so the entry is written UNCONDITIONALLY (replacing whatever
/// was there under that name, the same self-heal a drifted binary path needs) while every
/// OTHER top-level key and every OTHER server entry in the file is preserved untouched.
/// Because the write is unconditional on the SAME inputs, calling this twice with the same
/// `name`/`command`/`args` is idempotent by construction: the second call reproduces the
/// same bytes. The entry is marked [`MCP_ALWAYS_LOAD`], so the session that reads the file has
/// the server's tools from its first turn. `existing` may be empty.
pub fn install_mcp_server(
    existing: &[u8],
    name: &str,
    command: &str,
    args: &[&str],
) -> Result<Vec<u8>, Error> {
    merge_mcp_server(
        existing,
        name,
        json!({"command": command, "args": args, MCP_ALWAYS_LOAD: true}),
    )
}

/// The host's per-server MCP key that puts every tool the server lists into the session's prompt
/// up front instead of deferring it behind the host's tool search (which a session must call
/// before a deferred tool's schema is in hand).
pub const MCP_ALWAYS_LOAD: &str = "alwaysLoad";

/// The MCP configuration a headless spawn launches with: rigger's one stdio server
/// (`command args..` under [`MCP_SERVER_NAME`]), the same entry [`install_mcp_server`] writes, so
/// the graph tools are callable from the spawn's first turn. Only rigger's own entry carries the
/// mark; it changes no other server and no host-wide setting.
pub fn spawn_mcp_config(command: &str, args: &[&str]) -> Result<Vec<u8>, Error> {
    install_mcp_server(b"", MCP_SERVER_NAME, command, args)
}

/// Merge `entry` under `name` into `existing`'s `mcpServers` object, replacing whatever was there
/// under that name and preserving every other key; see [`install_mcp_server`].
fn merge_mcp_server(existing: &[u8], name: &str, entry: Value) -> Result<Vec<u8>, Error> {
    let mut root: Value = if existing.iter().all(u8::is_ascii_whitespace) {
        json!({})
    } else {
        serde_json::from_slice(existing).map_err(|e| Error(format!("parse .mcp.json: {e}")))?
    };
    let obj = root
        .as_object_mut()
        .ok_or_else(|| Error(".mcp.json is not a JSON object".into()))?;
    let servers = obj.entry("mcpServers").or_insert_with(|| json!({}));
    let servers_obj = servers
        .as_object_mut()
        .ok_or_else(|| Error("\"mcpServers\" is not an object".into()))?;
    servers_obj.insert(name.to_string(), entry);
    let mut out =
        serde_json::to_vec_pretty(&root).map_err(|e| Error(format!("encode .mcp.json: {e}")))?;
    out.push(b'\n');
    Ok(out)
}

/// The settings key naming the model the host's advisor tool runs on. A session with no value
/// configured has no advisor, and the empty string is the value that removes one a lower settings
/// layer configures: probed on the agent CLI, `--settings '{"advisorModel": ""}'` removes the tool
/// from a session whose user settings name an advisor, while a disallowed-tools entry does not
/// (the tool is server side and is not in the session's tool list) and `null` is read as unset.
const ADVISOR_MODEL_KEY: &str = "advisorModel";

/// Merge `advisorModel` set to the empty string into the settings JSON, so the session these
/// settings launch has no advisor whatever the host's own settings configure; every other setting
/// is preserved, and a value already there is replaced. `existing` may be empty.
pub fn install_no_advisor(existing: &[u8]) -> Result<Vec<u8>, Error> {
    let mut settings = parse_settings(existing)?;
    settings.insert(ADVISOR_MODEL_KEY.to_string(), json!(""));
    encode_settings(&settings)
}

/// Merge the editor's `statusLine` setting into the settings JSON (spec 94, criterion 5: THE
/// STATUSLINE COMMAND) - the same one-line summary `rigger status --line` prints, running
/// under the person's conversation. Like [`install_mcp_server`]'s `mcpServers` entry, rigger
/// OWNS this top-level key exclusively (there is only ever one status line), so the value is
/// written UNCONDITIONALLY - replacing whatever a drifted older build (or a hand edit) left
/// there - while every OTHER top-level key is preserved untouched. Because the write is
/// unconditional on the same `command`, calling this twice reproduces the same bytes
/// (idempotent by construction, the same guarantee [`install_mcp_server`] documents).
/// `existing` may be empty.
pub fn install_status_line(existing: &[u8], command: &str) -> Result<Vec<u8>, Error> {
    let mut root: Value = if existing.iter().all(u8::is_ascii_whitespace) {
        json!({})
    } else {
        serde_json::from_slice(existing).map_err(|e| Error(format!("parse settings.json: {e}")))?
    };
    let obj = root
        .as_object_mut()
        .ok_or_else(|| Error("settings.json is not a JSON object".into()))?;
    obj.insert(
        "statusLine".to_string(),
        json!({"type": "command", "command": command}),
    );
    let mut out = serde_json::to_vec_pretty(&root)
        .map_err(|e| Error(format!("encode settings.json: {e}")))?;
    out.push(b'\n');
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{
        hook, hook_block, write_helper_copies, write_helper_copy, OPERATOR_HELPER_COPIES,
    };

    /// `install` onto an empty settings file writes every one of `needles`, and installing again
    /// onto its own output leaves each command of `counted` exactly as many times as it names.
    fn assert_installs_idempotently(
        install: impl Fn(&[u8]) -> Result<Vec<u8>, Error>,
        needles: &[&str],
        counted: &[(&str, usize)],
    ) {
        let first = install(b"").unwrap();
        let s = String::from_utf8(first.clone()).unwrap();
        for needle in needles {
            assert!(s.contains(needle), "{needle} is installed: {s}");
        }
        let s2 = String::from_utf8(install(&first).unwrap()).unwrap();
        let installed: Vec<(&str, usize)> = counted
            .iter()
            .map(|&(command, _)| (command, s2.matches(command).count()))
            .collect();
        assert_eq!(installed, counted, "installing twice must not duplicate");
    }

    /// Merge a `PreToolUse` block with no timeout and no shell field into `existing`.
    fn pretooluse(existing: &[u8], matcher: &str, command: &str) -> Result<Vec<u8>, Error> {
        merge_hook_block(existing, "PreToolUse", matcher, command, None, None)
    }

    crate::test_cases! {
        installs_and_is_idempotent: assert_installs_idempotently(
            |existing| install_session_start(existing, "rigger prime"),
            &["SessionStart", "rigger prime"],
            &[("rigger prime", 1)],
        );
        pretooluse_hook_installs_and_is_idempotent: assert_installs_idempotently(
            |existing| pretooluse(existing, "Grep|Bash", "rigger grep-guard"),
            &["PreToolUse", "rigger grep-guard", "\"matcher\": \"Grep|Bash\""],
            &[("rigger grep-guard", 1)],
        );
        spawn_hooks_install_and_are_idempotent: assert_installs_idempotently(
            |existing| {
                let duty = TreeDuty::Checkpoint { branch: "rigger/u/u".to_string() };
                let roots = ["/wt".to_string()];
                install_spawn_hooks(existing, "rigger", "u/i#0", &roots, "/wt", &duty, HookShell::Posix)
            },
            &["PreToolUse", "StopFailure", "'rigger' guard-write --root '/wt'", "\"Stop\""],
            &[
                ("guard-write", 1),
                ("hook stop-failure", AgentFailure::CATEGORIES.len()),
                ("hook stop ", 1),
            ],
        );
    }

    /// `install_no_advisor` sets `advisorModel` to the empty string over empty settings and over
    /// settings that already name an advisor, keeps every other setting, and reproduces its own
    /// bytes when applied again.
    #[test]
    fn no_advisor_sets_the_empty_model_keeping_the_other_settings() {
        let rows = [
            ("", json!({"advisorModel": ""})),
            (
                r#"{"advisorModel":"strong","model":"opus"}"#,
                json!({"advisorModel": "", "model": "opus"}),
            ),
            (
                r#"{"advisorModel":null,"hooks":{}}"#,
                json!({"advisorModel": "", "hooks": {}}),
            ),
        ];
        for (existing, expected) in rows {
            let once = install_no_advisor(existing.as_bytes()).unwrap();
            let settings: Value = serde_json::from_slice(&once).unwrap();
            assert_eq!(settings, expected, "settings {existing:?}");
            assert_eq!(install_no_advisor(&once).unwrap(), once, "idempotent");
        }
        assert!(install_no_advisor(b"[]").is_err(), "not an object");
    }

    /// One argument of a hook command as each shell's word: between single quotes, inside which
    /// neither shell expands `"`, `\`, `$`, a backtick, `;`, `&`, `|` or non-ASCII, with each single
    /// quote it holds spelled as that shell reads one there. POSIX `sh` closes the quote, escapes
    /// the `'` and reopens it; PowerShell doubles every character it reads as a single quote, which
    /// are `'` and U+2018, U+2019, U+201A and U+201B (its own
    /// `CodeGeneration.EscapeSingleQuotedStringContent` over `CharExtensions.IsSingleQuote`).
    #[test]
    fn a_hook_word_is_single_quoted_by_its_shells_rule() {
        let rows = [
            ("rigger", "'rigger'", "'rigger'"),
            ("/a b/rigger", "'/a b/rigger'", "'/a b/rigger'"),
            ("it's", r"'it'\''s'", "'it''s'"),
            ("a'b'c", r"'a'\''b'\''c'", "'a''b''c'"),
            (r#"say "hi""#, r#"'say "hi"'"#, r#"'say "hi"'"#),
            (
                r"C:\Program Files\r.exe",
                r"'C:\Program Files\r.exe'",
                r"'C:\Program Files\r.exe'",
            ),
            (
                "/wt/\u{fc}ber/\u{65e5}",
                "'/wt/\u{fc}ber/\u{65e5}'",
                "'/wt/\u{fc}ber/\u{65e5}'",
            ),
            (
                "\u{2018}a\u{2019} \u{201A}b\u{201B}",
                "'\u{2018}a\u{2019} \u{201A}b\u{201B}'",
                "'\u{2018}\u{2018}a\u{2019}\u{2019} \u{201A}\u{201A}b\u{201B}\u{201B}'",
            ),
            (
                "rigger/u/a;b&c$d`e|f",
                "'rigger/u/a;b&c$d`e|f'",
                "'rigger/u/a;b&c$d`e|f'",
            ),
            ("u/implementer#0", "'u/implementer#0'", "'u/implementer#0'"),
            ("", "''", "''"),
        ];
        assert_eq!(
            rows.map(|(arg, _, _)| (HookShell::Posix.word(arg), HookShell::PowerShell.word(arg))),
            rows.map(|(_, posix, powershell)| (posix.to_string(), powershell.to_string()))
        );
    }

    /// Every POSIX hook command a spawn carries, run by a real `sh -c`, runs its program with
    /// exactly the argv it was built from, whatever the program's path, the spawn, the roots, the
    /// tree and the branch hold: spaces, single and double quotes, backslashes, non-ASCII, and
    /// `;`, `&`, `$`, a backtick and `|`. The program is a script printing its argv.
    #[cfg(unix)]
    #[test]
    fn a_posix_hook_command_runs_its_program_with_the_argv_it_was_built_from() {
        use std::os::unix::fs::PermissionsExt;
        let scratch = tempfile::tempdir().unwrap();
        let bin_dir = scratch.path().join("bin it's \"q\" \\ \u{fc};&$`|");
        std::fs::create_dir_all(&bin_dir).unwrap();
        let bin = bin_dir.join("rigger");
        std::fs::write(&bin, "#!/bin/sh\nprintf '%s\\0' \"$@\"\n").unwrap();
        std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755)).unwrap();
        let bin = bin.to_str().unwrap();
        let branch = "rigger/u/a;b&c$d`e|f";
        let rows = [
            (
                "u/implementer#0",
                vec!["/wt it's"],
                "/wt it's",
                TreeDuty::Checkpoint {
                    branch: branch.to_string(),
                },
                vec!["--branch", branch],
            ),
            (
                "plan/\u{fc} \"x\"#0",
                vec![r"C:\wt\a b", "/scratch/\u{65e5} \u{2019}$HOME"],
                r"C:\wt\a b",
                TreeDuty::OwnCommits,
                vec!["--own-commits"],
            ),
        ];
        for (spawn, roots, dir, duty, asks) in rows {
            let settings: Value = serde_json::from_slice(
                &install_spawn_hooks(
                    b"",
                    bin,
                    spawn,
                    &owned(&roots),
                    dir,
                    &duty,
                    HookShell::Posix,
                )
                .unwrap(),
            )
            .unwrap();
            let guard = ["guard-write"]
                .into_iter()
                .chain(roots.iter().flat_map(|root| ["--root", *root]));
            let mut ran = vec![(
                &settings["hooks"]["PreToolUse"][0],
                owned(&guard.collect::<Vec<_>>()),
            )];
            for (i, category) in AgentFailure::CATEGORIES.iter().enumerate() {
                let argv = [
                    "hook",
                    "stop-failure",
                    "--spawn",
                    spawn,
                    "--class",
                    category.as_str(),
                ];
                ran.push((&settings["hooks"]["StopFailure"][i], owned(&argv)));
            }
            let stop = ["hook", "stop", "--spawn", spawn, "--dir", dir]
                .into_iter()
                .chain(asks);
            ran.push((
                &settings["hooks"]["Stop"][0],
                owned(&stop.collect::<Vec<_>>()),
            ));
            for (block, argv) in ran {
                let command = block["hooks"][0]["command"].as_str().unwrap();
                let out = rigger_process::subprocess::shell_command(command, scratch.path())
                    .output()
                    .unwrap();
                assert!(out.status.success(), "{command}: {out:?}");
                let printed = String::from_utf8(out.stdout).unwrap();
                assert_eq!(
                    printed.split_terminator('\0').collect::<Vec<_>>(),
                    argv,
                    "{command}"
                );
            }
        }
    }

    /// `words` as owned strings.
    fn owned(words: &[&str]) -> Vec<String> {
        words.iter().map(|word| word.to_string()).collect()
    }

    /// A spawn's hooks in each shell over a bare and a hostile `rigger_bin`, one and two roots,
    /// and each tree duty: the whole `PreToolUse`, `StopFailure` and `Stop` arrays, every category
    /// in order, on the settings kept. Only a spawn with a duty gets the `Stop` hook, with its
    /// timeout, asking of a checkpointed tree on its branch and of a producer's tree for its own
    /// commits. A POSIX entry names no shell, so Claude Code runs it under `sh -c` as before; a
    /// PowerShell entry names `powershell`, so Claude Code never hands it to Git Bash, runs the
    /// quoted program behind the call operator `&`, and exits with the program's own status, which
    /// PowerShell's `-Command` would otherwise turn from the write guard's blocking 2 into 1.
    #[test]
    fn spawn_hooks_are_the_write_guard_one_stop_failure_entry_per_category_and_the_stop_check() {
        // The block `hook_block` builds, its inner hook given `timeout` and the shell `field`
        // names, each where one is named.
        let entry = |field: Option<&str>, matcher: &str, command: &str, timeout: Option<u32>| {
            let mut block = hook_block(matcher, command);
            if let Some(timeout) = timeout {
                block["hooks"][0]["timeout"] = json!(timeout);
            }
            if let Some(field) = field {
                block["hooks"][0]["shell"] = json!(field);
            }
            block
        };
        let checkpoint = |branch: &str| TreeDuty::Checkpoint {
            branch: branch.to_string(),
        };
        // A PowerShell spawn on Windows: a program path with a space and a quote, a tree with
        // non-ASCII, and a branch with every character the POSIX rows guard against and U+2019.
        let ps = r"& 'C:\Program Files\it''s\rigger.exe'";
        let ps_guard = format!(
            "{ps} guard-write --root 'C:\\wt\\u \u{fc}' --root 'C:\\scratch\\s'; \
             exit $LASTEXITCODE"
        );
        let ps_stop_failure = format!(
            "{ps} hook stop-failure --spawn 'u/implementer#0' --class {{class}}; exit $LASTEXITCODE"
        );
        let ps_stop = format!(
            "{ps} hook stop --spawn 'u/implementer#0' --dir 'C:\\wt\\u \u{fc}' \
             --branch 'rigger/u/a;b&c$d`e|f\u{2019}\u{2019}'; exit $LASTEXITCODE"
        );
        let rows = [
            (
                (HookShell::Posix, None),
                ("rigger", "u/implementer#0", vec!["/wt"], "/wt", checkpoint("rigger/u/u")),
                "'rigger' guard-write --root '/wt'",
                "'rigger' hook stop-failure --spawn 'u/implementer#0' --class {class}",
                Some("'rigger' hook stop --spawn 'u/implementer#0' --dir '/wt' --branch 'rigger/u/u'"),
            ),
            (
                (HookShell::Posix, None),
                ("rigger", "plan/implementer#0", vec!["/wt plan"], "/wt plan", TreeDuty::OwnCommits),
                "'rigger' guard-write --root '/wt plan'",
                "'rigger' hook stop-failure --spawn 'plan/implementer#0' --class {class}",
                Some("'rigger' hook stop --spawn 'plan/implementer#0' --dir '/wt plan' --own-commits"),
            ),
            (
                (HookShell::Posix, None),
                (
                    "/opt/it's here/rigger",
                    "u/reviewer#1",
                    vec!["/wt tree", "/scratch/s"],
                    "/wt tree",
                    TreeDuty::None,
                ),
                r"'/opt/it'\''s here/rigger' guard-write --root '/wt tree' --root '/scratch/s'",
                r"'/opt/it'\''s here/rigger' hook stop-failure --spawn 'u/reviewer#1' --class {class}",
                None,
            ),
            (
                (HookShell::PowerShell, Some("powershell")),
                (
                    r"C:\Program Files\it's\rigger.exe",
                    "u/implementer#0",
                    vec!["C:\\wt\\u \u{fc}", r"C:\scratch\s"],
                    "C:\\wt\\u \u{fc}",
                    checkpoint("rigger/u/a;b&c$d`e|f\u{2019}"),
                ),
                ps_guard.as_str(),
                ps_stop_failure.as_str(),
                Some(ps_stop.as_str()),
            ),
            (
                (HookShell::PowerShell, Some("powershell")),
                ("rigger", "plan/\"x\"#0", vec!["/wt plan"], "/wt plan", TreeDuty::OwnCommits),
                "& 'rigger' guard-write --root '/wt plan'; exit $LASTEXITCODE",
                "& 'rigger' hook stop-failure --spawn 'plan/\"x\"#0' --class {class}; \
                 exit $LASTEXITCODE",
                Some(
                    "& 'rigger' hook stop --spawn 'plan/\"x\"#0' --dir '/wt plan' --own-commits; \
                     exit $LASTEXITCODE",
                ),
            ),
        ];
        for ((shell, field), (bin, spawn, roots, dir, duty), guard, stop_failure, stop) in rows {
            let stop_failures: Vec<Value> = AgentFailure::CATEGORIES
                .iter()
                .map(|category| {
                    let class = category.as_str();
                    entry(field, class, &stop_failure.replace("{class}", class), None)
                })
                .collect();
            let stop = stop.map_or(Value::Null, |command| {
                json!([entry(field, "", command, Some(STOP_HOOK_TIMEOUT_SECS))])
            });
            let settings = install_spawn_hooks(
                br#"{"model":"opus"}"#,
                bin,
                spawn,
                &owned(&roots),
                dir,
                &duty,
                shell,
            )
            .unwrap();
            assert_eq!(
                serde_json::from_slice::<Value>(&settings).unwrap()["hooks"]["Stop"],
                stop,
                "{spawn}"
            );
            assert_settings(
                settings,
                &[
                    ("/model", Value::from("opus")),
                    (
                        "/hooks/PreToolUse",
                        json!([entry(
                            field,
                            "Edit|MultiEdit|Write|NotebookEdit",
                            guard,
                            None
                        )]),
                    ),
                    ("/hooks/StopFailure", Value::Array(stop_failures)),
                ],
            );
        }
    }

    /// `settings` as the bytes of a compact settings file.
    fn bytes(settings: &Value) -> Vec<u8> {
        serde_json::to_vec(settings).unwrap()
    }

    /// The session settings a spawn's `--settings` carries: the prime hook and the status line
    /// always, the grep-guard only with the flag set, and never a removal, so settings already
    /// carrying the guard keep it with the flag off.
    #[test]
    fn session_settings_merge_the_grep_guard_only_when_declared() {
        let prime = json!([hook_block("", PRIME_COMMAND)]);
        let status = hook(STATUS_LINE_COMMAND);
        let guard = json!([hook_block(GREP_GUARD_MATCHER, GREP_GUARD_COMMAND)]);
        let guarded = bytes(&json!({"hooks": {"PreToolUse": guard}}));
        let primed = json!({"hooks": {"SessionStart": prime}, "statusLine": status});
        let primed_and_guarded = json!({
            "hooks": {"SessionStart": prime, "PreToolUse": guard},
            "statusLine": status,
        });
        let rows: [(&[u8], bool, &Value); 4] = [
            (b"", false, &primed),
            (b"", true, &primed_and_guarded),
            (&guarded, false, &primed_and_guarded),
            (&guarded, true, &primed_and_guarded),
        ];
        for (existing, grep_guard, expected) in rows {
            let merged = install_session_settings(existing, grep_guard).unwrap();
            assert_eq!(
                (
                    &serde_json::from_slice::<Value>(&merged).unwrap(),
                    install_session_settings(&merged, grep_guard).unwrap()
                ),
                (expected, merged.clone()),
                "{} with grep_guard {grep_guard}",
                String::from_utf8_lossy(existing)
            );
        }
    }

    /// The helper `file` as the resolver reads it: the operator's copy of
    /// [`OPERATOR_HELPER_COPIES`] when `present`, else the embedded text.
    fn resolved(file: &str, present: bool) -> HelperCopy {
        let texts: &[(&str, &str)] = if present {
            &OPERATOR_HELPER_COPIES
        } else {
            HELPER_AGENTS
        };
        let (_, text) = texts.iter().find(|(f, _)| *f == file).unwrap();
        HelperCopy {
            name: file.to_string(),
            text: text.to_string(),
            present,
        }
    }

    /// The spawn host carries the grep-guard exactly when the workflow's defaults declare it, and
    /// each helper as the operator's copy under `<root>/.claude/agents/` when that file exists,
    /// else as the embedded text: a file of another name, or an agents path that is no directory,
    /// holds no copy, and a copy equal to the embedded text is still the operator's.
    #[test]
    fn the_spawn_host_carries_the_declared_grep_guard_and_each_helper_resolved_at_the_root() {
        struct Row {
            name: &'static str,
            grep_guard: bool,
            setup: fn(&std::path::Path),
            helpers: Vec<HelperCopy>,
        }
        let rows = [
            Row {
                name: "a root with no .claude reads both embedded helpers",
                grep_guard: false,
                setup: |_| {},
                helpers: vec![resolved("lookup.md", false), resolved("verify.md", false)],
            },
            Row {
                name: "the declared grep-guard rides beside the helpers",
                grep_guard: true,
                setup: |_| {},
                helpers: vec![resolved("lookup.md", false), resolved("verify.md", false)],
            },
            Row {
                name: "an empty agents directory reads both embedded helpers",
                grep_guard: false,
                setup: |root| std::fs::create_dir_all(root.join(".claude/agents")).unwrap(),
                helpers: vec![resolved("lookup.md", false), resolved("verify.md", false)],
            },
            Row {
                name: "an operator lookup copy replaces lookup alone",
                grep_guard: false,
                setup: |root| write_helper_copy(root, "lookup.md", OPERATOR_HELPER_COPIES[0].1),
                helpers: vec![resolved("lookup.md", true), resolved("verify.md", false)],
            },
            Row {
                name: "an operator verify copy replaces verify alone",
                grep_guard: false,
                setup: |root| write_helper_copy(root, "verify.md", OPERATOR_HELPER_COPIES[1].1),
                helpers: vec![resolved("lookup.md", false), resolved("verify.md", true)],
            },
            Row {
                name: "both operator copies replace both helpers",
                grep_guard: true,
                setup: |root| write_helper_copies(root, &OPERATOR_HELPER_COPIES),
                helpers: vec![resolved("lookup.md", true), resolved("verify.md", true)],
            },
            Row {
                name: "a copy equal to the embedded text is still the operator's",
                grep_guard: false,
                setup: |root| write_helper_copy(root, "lookup.md", HELPER_AGENTS[0].1),
                helpers: vec![
                    HelperCopy {
                        present: true,
                        ..resolved("lookup.md", false)
                    },
                    resolved("verify.md", false),
                ],
            },
            Row {
                name: "a helper-shaped file under another name is no copy",
                grep_guard: false,
                setup: |root| write_helper_copy(root, "reviewer.md", OPERATOR_HELPER_COPIES[0].1),
                helpers: vec![resolved("lookup.md", false), resolved("verify.md", false)],
            },
            Row {
                name: "an agents path that is a file holds no copy",
                grep_guard: false,
                setup: |root| {
                    std::fs::create_dir_all(root.join(".claude")).unwrap();
                    std::fs::write(root.join(".claude/agents"), "not a directory").unwrap();
                },
                helpers: vec![resolved("lookup.md", false), resolved("verify.md", false)],
            },
        ];
        for row in rows {
            let root = tempfile::tempdir().unwrap();
            (row.setup)(root.path());
            let defaults = rigger_domain::config::Defaults {
                grep_guard: row.grep_guard,
                ..Default::default()
            };
            assert_eq!(
                resolve_spawn_host(&defaults, root.path()).map_err(|e| e.to_string()),
                Ok(SpawnHost {
                    grep_guard: row.grep_guard,
                    helpers: row.helpers,
                }),
                "{}",
                row.name
            );
        }
    }

    /// A copy that exists but cannot be read, does not parse as a helper definition, or names the
    /// helper another file defines, fails the resolution with an error naming that copy, and for a
    /// repeated name the other file, never falling back to the embedded text.
    #[test]
    fn an_unreadable_or_malformed_helper_copy_fails_the_resolution_naming_the_file() {
        const NO_FRONTMATTER: &str = "Answer one question.\n";
        const UNTERMINATED: &str = "---\nname: verify\n";
        struct Row {
            name: &'static str,
            setup: fn(&std::path::Path),
            file: &'static str,
            reason: fn(&std::path::Path) -> String,
        }
        let rows = [
            Row {
                name: "a copy with no frontmatter",
                setup: |root| write_helper_copy(root, "lookup.md", NO_FRONTMATTER),
                file: "lookup.md",
                reason: |_| {
                    rigger_domain::config::split_frontmatter(NO_FRONTMATTER)
                        .unwrap_err()
                        .to_string()
                },
            },
            Row {
                name: "a frontmatter that is no YAML mapping",
                setup: |root| write_helper_copy(root, "lookup.md", "---\nname lookup\n---\nbody\n"),
                file: "lookup.md",
                reason: |_| {
                    rigger_domain::config::parse_yaml_naming_unknown_keys::<Map<String, Value>>(
                        "name lookup",
                    )
                    .unwrap_err()
                },
            },
            Row {
                name: "a lookup copy naming the helper verify defines",
                setup: |root| {
                    write_helper_copy(root, "lookup.md", "---\nname: verify\n---\nbody\n")
                },
                file: "lookup.md",
                reason: |_| "the name \"verify\" is also verify.md's".to_string(),
            },
            Row {
                name: "a verify copy naming the helper the lookup copy defines",
                setup: |root| {
                    write_helper_copy(root, "lookup.md", OPERATOR_HELPER_COPIES[0].1);
                    write_helper_copy(root, "verify.md", "---\nname: lookup\n---\nbody\n");
                },
                file: "verify.md",
                reason: |_| "the name \"lookup\" is also lookup.md's".to_string(),
            },
            Row {
                name: "a copy with no name",
                setup: |root| {
                    write_helper_copy(root, "lookup.md", "---\nmodel: haiku\n---\nbody\n")
                },
                file: "lookup.md",
                reason: |_| "no name".to_string(),
            },
            Row {
                name: "a copy that is not text",
                setup: |root| write_helper_copy(root, "lookup.md", b"---\n\xff\n---\n"),
                file: "lookup.md",
                reason: |path| std::fs::read_to_string(path).unwrap_err().to_string(),
            },
            Row {
                name: "a copy that is a directory",
                setup: |root| {
                    std::fs::create_dir_all(root.join(".claude/agents/lookup.md")).unwrap()
                },
                file: "lookup.md",
                reason: |path| std::fs::read_to_string(path).unwrap_err().to_string(),
            },
            Row {
                name: "a malformed verify copy beside a valid lookup copy",
                setup: |root| {
                    write_helper_copy(root, "lookup.md", OPERATOR_HELPER_COPIES[0].1);
                    write_helper_copy(root, "verify.md", UNTERMINATED);
                },
                file: "verify.md",
                reason: |_| {
                    rigger_domain::config::split_frontmatter(UNTERMINATED)
                        .unwrap_err()
                        .to_string()
                },
            },
        ];
        for row in rows {
            let root = tempfile::tempdir().unwrap();
            (row.setup)(root.path());
            let path = root.path().join(".claude/agents").join(row.file);
            assert_eq!(
                resolve_spawn_host(&rigger_domain::config::Defaults::default(), root.path())
                    .map_err(|e| e.to_string()),
                Err(format!(
                    "hooks: helper agent {}: {}",
                    path.display(),
                    (row.reason)(&path)
                )),
                "{}",
                row.name
            );
        }
    }

    /// Settings carry the session settings when merging them again with the same flag changes
    /// nothing: a guard the flag does not declare is no shortfall, a missing one is.
    #[test]
    fn carried_session_settings_read_the_grep_guard_flag() {
        let prime = json!([hook_block("", PRIME_COMMAND)]);
        let status = hook(STATUS_LINE_COMMAND);
        let guard = json!([hook_block(GREP_GUARD_MATCHER, GREP_GUARD_COMMAND)]);
        let primed = bytes(&json!({"hooks": {"SessionStart": prime}, "statusLine": status}));
        let primed_and_guarded = bytes(&json!({
            "hooks": {"SessionStart": prime, "PreToolUse": guard},
            "statusLine": status,
        }));
        let guarded_beside_an_empty_block = bytes(&json!({
            "hooks": {
                "SessionStart": prime,
                "PreToolUse": [guard[0], {"matcher": "Write", "hooks": []}],
            },
            "statusLine": status,
        }));
        let rows: [(&[u8], bool, bool); 7] = [
            (b"", false, false),
            (b"not json", true, false),
            (&primed, false, true),
            (&primed, true, false),
            (&primed_and_guarded, true, true),
            (&primed_and_guarded, false, true),
            (&guarded_beside_an_empty_block, true, true),
        ];
        let carried: Vec<bool> = rows
            .iter()
            .map(|(existing, grep_guard, _)| carries_session_settings(existing, *grep_guard))
            .collect();
        assert_eq!(carried, rows.map(|(_, _, carries)| carries));
    }

    /// The guard's install: one entry under the current matcher, appended beside every foreign
    /// hook and every foreign block, one already empty included, an entry under a drifted matcher
    /// replaced so exactly one remains, and a second install writing the same bytes.
    #[test]
    fn the_grep_guard_installs_once_under_the_current_matcher() {
        let guard = hook_block(GREP_GUARD_MATCHER, GREP_GUARD_COMMAND);
        let prettier = hook_block("Write", "prettier --write");
        let rows = [
            (json!({}), json!({"hooks": {"PreToolUse": [guard]}})),
            (
                json!({"model": "opus", "hooks": {"PreToolUse": [prettier]}}),
                json!({"model": "opus", "hooks": {"PreToolUse": [prettier, guard]}}),
            ),
            (
                json!({"hooks": {"PreToolUse": [hook_block("Grep", GREP_GUARD_COMMAND)]}}),
                json!({"hooks": {"PreToolUse": [guard]}}),
            ),
            (
                json!({"hooks": {"PreToolUse": [
                    {"matcher": "Bash", "hooks": [hook("prettier --write"), hook(GREP_GUARD_COMMAND)]},
                ]}}),
                json!({"hooks": {"PreToolUse": [
                    {"matcher": "Bash", "hooks": [hook("prettier --write")]},
                    guard,
                ]}}),
            ),
            (
                json!({"hooks": {"PreToolUse": [guard]}}),
                json!({"hooks": {"PreToolUse": [guard]}}),
            ),
            (
                json!({"hooks": {"PreToolUse": [{"matcher": "Write", "hooks": []}]}}),
                json!({"hooks": {"PreToolUse": [{"matcher": "Write", "hooks": []}, guard]}}),
            ),
        ];
        for (existing, expected) in rows {
            let installed = install_grep_guard(&bytes(&existing)).unwrap();
            assert_eq!(
                (
                    serde_json::from_slice::<Value>(&installed).unwrap(),
                    install_grep_guard(&installed).unwrap()
                ),
                (expected, installed.clone()),
                "installed into {existing}"
            );
        }
    }

    /// The guard's removal strips every inner hook running the guard's command, whatever its
    /// matcher, drops each block, then the `PreToolUse` array, then the `hooks` object it
    /// empties, and keeps every other hook, block and setting, a foreign block already empty
    /// included.
    #[test]
    fn the_grep_guard_removal_strips_only_its_own_entry() {
        let guard = hook_block(GREP_GUARD_MATCHER, GREP_GUARD_COMMAND);
        let prime = hook_block("", PRIME_COMMAND);
        let prettier = hook_block("Write", "prettier --write");
        let rows = [
            (
                json!({"model": "opus", "hooks": {"PreToolUse": [guard]}}),
                json!({"model": "opus"}),
            ),
            (
                json!({"hooks": {"SessionStart": [prime], "PreToolUse": [guard]}}),
                json!({"hooks": {"SessionStart": [prime]}}),
            ),
            (
                json!({"hooks": {"PreToolUse": [
                    {"matcher": GREP_GUARD_MATCHER, "hooks": [hook(GREP_GUARD_COMMAND), hook("prettier --write")]},
                ]}}),
                json!({"hooks": {"PreToolUse": [
                    {"matcher": GREP_GUARD_MATCHER, "hooks": [hook("prettier --write")]},
                ]}}),
            ),
            (
                json!({"hooks": {"PreToolUse": [prettier, guard]}}),
                json!({"hooks": {"PreToolUse": [prettier]}}),
            ),
            (
                json!({"model": "opus", "hooks": {"PreToolUse": [hook_block("Grep", GREP_GUARD_COMMAND)]}}),
                json!({"model": "opus"}),
            ),
            (
                json!({"hooks": {"PreToolUse": [{"matcher": "Write", "hooks": []}, guard]}}),
                json!({"hooks": {"PreToolUse": [{"matcher": "Write", "hooks": []}]}}),
            ),
        ];
        for (existing, expected) in rows {
            let removed = remove_grep_guard(&bytes(&existing)).unwrap();
            assert_eq!(
                serde_json::from_slice::<Value>(&removed).unwrap(),
                expected,
                "removed from {existing}"
            );
        }
    }

    /// Settings that carry no guard - none at all, no `PreToolUse` array, or the guard's command
    /// under another event - come back from the removal as the very bytes they were, so a
    /// settings file never carrying the guard is never rewritten.
    #[test]
    fn the_grep_guard_removal_returns_settings_without_the_guard_untouched() {
        let rows: [&[u8]; 6] = [
            b"",
            br#"{"model":"opus"}"#,
            br#"{"hooks":"none"}"#,
            br#"{"hooks":{"PreToolUse":{}}}"#,
            br#"{"hooks":{"PreToolUse":[{"matcher":"Write","hooks":[{"type":"command","command":"prettier --write"}]}]}}"#,
            br#"{"hooks":{"SessionStart":[{"matcher":"","hooks":[{"type":"command","command":"rigger grep-guard"}]}]}}"#,
        ];
        let removed: Vec<Vec<u8>> = rows
            .iter()
            .map(|existing| remove_grep_guard(existing).unwrap())
            .collect();
        assert_eq!(removed, rows.map(<[u8]>::to_vec));
    }

    /// A settings file rigger alone gave the guard returns to its shape before the install: the
    /// guard installed into it and then removed leaves the file the merge would reserialize.
    #[test]
    fn the_grep_guard_removal_undoes_its_install_byte_for_byte() {
        let rows = [
            json!({"model": "opus"}),
            json!({"model": "opus", "hooks": {"PreToolUse": [hook_block("Write", "prettier --write")]}}),
            json!({"hooks": {"SessionStart": [hook_block("", PRIME_COMMAND)]}, "statusLine": hook(STATUS_LINE_COMMAND)}),
        ];
        for before in rows {
            let mut reserialized = serde_json::to_vec_pretty(&before).unwrap();
            reserialized.push(b'\n');
            let installed = install_grep_guard(&bytes(&before)).unwrap();
            assert_eq!(
                String::from_utf8(remove_grep_guard(&installed).unwrap()).unwrap(),
                String::from_utf8(reserialized).unwrap(),
                "installed into and removed from {before}"
            );
        }
    }

    /// The guard is present when any inner `PreToolUse` hook runs its command, under any
    /// matcher; under another event, or in settings that do not parse, it is not.
    #[test]
    fn the_grep_guard_is_carried_by_any_pretooluse_hook_running_its_command() {
        let rows: [(Vec<u8>, bool); 7] = [
            (Vec::new(), false),
            (b"not json".to_vec(), false),
            (
                bytes(&json!({"hooks": {"PreToolUse": [hook_block("Write", "prettier --write")]}})),
                false,
            ),
            (
                bytes(&json!({"hooks": {"SessionStart": [hook_block("", GREP_GUARD_COMMAND)]}})),
                false,
            ),
            (
                bytes(
                    &json!({"hooks": {"PreToolUse": [hook_block(GREP_GUARD_MATCHER, GREP_GUARD_COMMAND)]}}),
                ),
                true,
            ),
            (
                bytes(&json!({"hooks": {"PreToolUse": [hook_block("Grep", GREP_GUARD_COMMAND)]}})),
                true,
            ),
            (
                bytes(&json!({"hooks": {"PreToolUse": [
                    {"matcher": "Bash", "hooks": [hook("prettier --write"), hook(GREP_GUARD_COMMAND)]},
                ]}})),
                true,
            ),
        ];
        let carried: Vec<bool> = rows
            .iter()
            .map(|(existing, _)| carries_grep_guard(existing))
            .collect();
        assert_eq!(carried, rows.map(|(_, carries)| carries));
    }

    /// Settings that are not a JSON object refuse the guard's install and its removal alike.
    #[test]
    fn the_grep_guard_edits_refuse_settings_that_are_not_an_object() {
        let refusals: Vec<String> = [install_grep_guard, remove_grep_guard]
            .map(|edit| edit(b"[]").unwrap_err().to_string())
            .to_vec();
        assert_eq!(refusals, ["hooks: settings.json is not a JSON object"; 2]);
    }

    /// The write guard's matcher is every editing tool, each named once, in the list's order.
    #[test]
    fn the_write_guard_matcher_is_the_editing_tools_joined() {
        let matcher = write_guard_matcher();
        assert_eq!(
            matcher.split('|').collect::<Vec<_>>(),
            EDITING_TOOLS.map(|(tool, _)| tool),
            "the write guard matches every editing tool and no other: {matcher}"
        );
    }

    /// Spawn settings that are not a JSON object refuse the merge.
    #[test]
    fn spawn_hooks_refuse_settings_that_are_not_an_object() {
        let none = TreeDuty::None;
        let refused =
            install_spawn_hooks(b"[]", "rigger", "u/i#0", &[], "", &none, HookShell::HOST)
                .unwrap_err();
        assert_eq!(
            refused.to_string(),
            "hooks: settings.json is not a JSON object"
        );
    }

    /// The installed settings JSON `out` holds each `(pointer, value)` of `expected`.
    fn assert_settings(out: Vec<u8>, expected: &[(&str, Value)]) {
        let v: Value = serde_json::from_slice(&out).unwrap();
        for (pointer, value) in expected {
            assert_eq!(v.pointer(pointer), Some(value), "{pointer} in {v}");
        }
    }

    crate::test_cases! {
        preserves_other_settings: assert_settings(
            install_session_start(br#"{"model":"opus"}"#, "rigger prime").unwrap(),
            &[
                ("/model", Value::from("opus")),
                ("/hooks/SessionStart/0/hooks/0/command", Value::from("rigger prime")),
            ],
        );
        /// Both installers merge into the SAME settings.json's "hooks" object (SessionStart vs
        /// PreToolUse) - neither must clobber the other's event key.
        pretooluse_hook_composes_with_the_session_start_hook: assert_settings(
            pretooluse(
                &install_session_start(b"", "rigger prime").unwrap(),
                "Grep|Bash",
                "rigger grep-guard",
            )
            .unwrap(),
            &[
                ("/hooks/SessionStart/0/hooks/0/command", Value::from("rigger prime")),
                ("/hooks/PreToolUse/0/hooks/0/command", Value::from("rigger grep-guard")),
            ],
        );
        mcp_server_preserves_other_servers_and_other_top_level_keys: assert_settings(
            install_mcp_server(
                br#"{
            "someOtherSetting": true,
            "mcpServers": {
                "unrelated": {"command": "some-other-tool", "args": []}
            }
        }"#,
                "rigger",
                "rigger",
                &["mcp"],
            )
            .unwrap(),
            &[
                ("/someOtherSetting", Value::from(true)),
                ("/mcpServers/unrelated/command", Value::from("some-other-tool")),
                ("/mcpServers/rigger/command", Value::from("rigger")),
            ],
        );
        /// An older rigger build (or a hand edit) wrote a different shape under "rigger" - a
        /// fresh install self-heals it to the current shape, the same drift-repair every other
        /// `rigger setup` step performs.
        mcp_server_self_heals_a_drifted_entry: assert_settings(
            install_mcp_server(
                br#"{"mcpServers": {"rigger": {"command": "/old/stale/path", "args": []}}}"#,
                "rigger",
                "rigger",
                &["mcp"],
            )
            .unwrap(),
            &[
                ("/mcpServers/rigger/command", Value::from("rigger")),
                ("/mcpServers/rigger/args/0", Value::from("mcp")),
            ],
        );
        status_line_preserves_other_top_level_settings: assert_settings(
            install_status_line(
                br#"{
            "model": "opus",
            "hooks": {
                "SessionStart": [
                    {"matcher": "", "hooks": [{"type": "command", "command": "rigger prime"}]}
                ]
            }
        }"#,
                "rigger status --line",
            )
            .unwrap(),
            &[
                ("/model", Value::from("opus")),
                ("/hooks/SessionStart/0/hooks/0/command", Value::from("rigger prime")),
                ("/statusLine/command", Value::from("rigger status --line")),
            ],
        );
        /// An older rigger build (or a hand edit, or someone else's status line) left a
        /// different shape under "statusLine" - a fresh install self-heals it, the same
        /// drift-repair every other `rigger setup` step performs and `install_mcp_server`'s own
        /// "rigger" entry documents.
        status_line_self_heals_a_drifted_entry: assert_settings(
            install_status_line(
                br#"{"statusLine": {"type": "command", "command": "/old/stale/statusline.sh"}}"#,
                "rigger status --line",
            )
            .unwrap(),
            &[("/statusLine/command", Value::from("rigger status --line"))],
        );
    }

    #[test]
    fn pretooluse_hook_preserves_other_settings_and_other_pretooluse_entries() {
        // A machine may already carry a PreToolUse hook of its own (a different matcher,
        // a different command) - the merge must not clobber it, and every other top-level
        // setting must survive too.
        let existing = br#"{
            "model": "opus",
            "hooks": {
                "PreToolUse": [
                    {"matcher": "Write", "hooks": [{"type": "command", "command": "prettier --write"}]}
                ]
            }
        }"#;
        let out = pretooluse(existing, "Grep|Bash", "rigger grep-guard").unwrap();
        let v: Value = serde_json::from_slice(&out).unwrap();
        assert_eq!(v["model"], "opus");
        let arr = v["hooks"]["PreToolUse"].as_array().unwrap();
        assert_eq!(
            arr.len(),
            2,
            "the pre-existing block must survive alongside the new one"
        );
        assert!(arr
            .iter()
            .any(|b| b["hooks"][0]["command"] == "prettier --write"));
        assert!(arr.iter().any(
            |b| b["hooks"][0]["command"] == "rigger grep-guard" && b["matcher"] == "Grep|Bash"
        ));
    }

    /// The rigger tools the given helpers declare, in declaration order and each once, read from
    /// the copies handed in; a copy that does not parse names its helper.
    #[test]
    fn helper_mcp_tools_are_the_rigger_tools_the_given_helpers_declare() {
        let rows = [
            (
                "the embedded helpers",
                vec![resolved("lookup.md", false), resolved("verify.md", false)],
                Ok(vec![
                    "mcp__rigger__rigger_graph",
                    "mcp__rigger__rigger_ground",
                    "mcp__rigger__rigger_peers",
                ]),
            ),
            (
                "the operator's copies",
                vec![resolved("lookup.md", true), resolved("verify.md", true)],
                Ok(vec![
                    "mcp__rigger__rigger_peers",
                    "mcp__rigger__rigger_graph",
                ]),
            ),
            (
                "a tool two helpers declare lands once, where it first came",
                vec![
                    resolved("lookup.md", true),
                    HelperCopy {
                        text: "---\nname: verify\n\
                               tools: mcp__rigger__rigger_graph, mcp__rigger__rigger_ground\n\
                               ---\nbody\n"
                            .to_string(),
                        ..resolved("verify.md", true)
                    },
                ],
                Ok(vec![
                    "mcp__rigger__rigger_peers",
                    "mcp__rigger__rigger_graph",
                    "mcp__rigger__rigger_ground",
                ]),
            ),
            ("no helpers", vec![], Ok(vec![])),
            (
                "a YAML list of tools declares each of its entries",
                vec![HelperCopy {
                    text:
                        "---\nname: lookup\ntools: [mcp__rigger__rigger_graph, Read]\n---\nbody\n"
                            .to_string(),
                    ..resolved("lookup.md", true)
                }],
                Ok(vec!["mcp__rigger__rigger_graph"]),
            ),
            (
                "a copy with no name",
                vec![HelperCopy {
                    text: "---\nmodel: haiku\n---\nbody\n".to_string(),
                    ..resolved("lookup.md", true)
                }],
                Err("hooks: helper agent lookup.md: no name"),
            ),
        ];
        for (name, helpers, expected) in rows {
            assert_eq!(
                helper_mcp_tools(&helpers).map_err(|e| e.to_string()),
                expected
                    .map(|tools| tools.iter().map(|t| t.to_string()).collect::<Vec<_>>())
                    .map_err(str::to_string),
                "{name}"
            );
        }
    }

    /// A helper copy's frontmatter reads as the YAML Claude Code reads it in the operator's own
    /// session: a blank line and a comment are no field, a quoted value is its text, and `tools`
    /// is a list whether written as one or as a comma-separated string.
    #[test]
    fn a_helper_copys_frontmatter_reads_as_yaml() {
        let rows = [
            (
                "a blank line and a comment",
                "---\nname: lookup\n\n# the operator's note\nmodel: sonnet\n---\nbody\n",
                json!({"model": "sonnet", "prompt": "body\n"}),
            ),
            (
                "a quoted name and a quoted value holding a colon",
                "---\nname: \"lookup\"\ndescription: \"Answer: one node.\"\n---\nbody\n",
                json!({"description": "Answer: one node.", "prompt": "body\n"}),
            ),
            (
                "a flow list of tools",
                "---\nname: lookup\ntools: [mcp__rigger__rigger_graph, Read]\n---\nbody\n",
                json!({"tools": ["mcp__rigger__rigger_graph", "Read"], "prompt": "body\n"}),
            ),
            (
                "a block list of tools",
                "---\nname: lookup\ntools:\n  - Read\n  - Bash\n---\nbody\n",
                json!({"tools": ["Read", "Bash"], "prompt": "body\n"}),
            ),
            (
                "a comma-separated string of tools",
                "---\nname: lookup\ntools: Read, Bash\n---\nbody\n",
                json!({"tools": ["Read", "Bash"], "prompt": "body\n"}),
            ),
        ];
        for (name, text, expected) in rows {
            let copy = HelperCopy {
                text: text.to_string(),
                ..resolved("lookup.md", true)
            };
            assert_eq!(
                helper_agents_json(&[copy]).map_err(|e| e.to_string()),
                Ok(json!({ "lookup": expected })),
                "{name}"
            );
        }
    }

    #[test]
    fn mcp_server_installs_and_is_idempotent() {
        let first = install_mcp_server(b"", "rigger", "rigger", &["mcp"]).unwrap();
        let v: Value = serde_json::from_slice(&first).unwrap();
        assert_eq!(v["mcpServers"]["rigger"]["command"], "rigger");
        assert_eq!(v["mcpServers"]["rigger"]["args"][0], "mcp");

        let second = install_mcp_server(&first, "rigger", "rigger", &["mcp"]).unwrap();
        assert_eq!(
            first, second,
            "installing twice must reproduce the same bytes"
        );
    }

    #[test]
    fn status_line_installs_and_is_idempotent() {
        let first = install_status_line(b"", "rigger status --line").unwrap();
        let v: Value = serde_json::from_slice(&first).unwrap();
        assert_eq!(v["statusLine"]["type"], "command");
        assert_eq!(v["statusLine"]["command"], "rigger status --line");

        let second = install_status_line(&first, "rigger status --line").unwrap();
        assert_eq!(
            first, second,
            "installing twice must reproduce the same bytes"
        );
    }
}
