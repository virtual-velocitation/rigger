//! Installs Rigger's Claude Code integration: merges a SessionStart hook into
//! .claude/settings.json - preserving every other setting - so a session opened
//! in a Rigger repository starts primed with the project's recent decisions.
//!
//! This is also the one home of WHAT a rigger session carries: the hook and status-line
//! commands, the MCP server name and the fan-out helper agents. `rigger setup` writes them into
//! the operator's checkout; the headless host hands the same values to every spawn on its
//! command line (a fresh unit worktree carries no `.claude/` of its own), so the two cannot
//! drift. The hooks bound to one spawn are composed here too, by [`install_spawn_hooks`]: only
//! the headless host hands those over, and `rigger setup` never writes them.

use rigger_domain::quoting::single_quoted;
use rigger_domain::review::EDITING_TOOLS;
use serde_json::{json, Map, Value};

use crate::agent::AgentFailure;

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
/// and test runs. The text is this repository's own committed `.claude/agents/` helpers, so
/// what `rigger init` scaffolds and what a headless spawn receives cannot drift from them.
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

/// Merge every session setting rigger installs - the SessionStart prime hook, the PreToolUse
/// grep-guard and the status line - into `existing` settings JSON, through the same installers
/// `rigger setup` runs one by one, preserving every other setting. `existing` may be empty.
pub fn install_session_settings(existing: &[u8]) -> Result<Vec<u8>, Error> {
    let primed = install_session_start(existing, PRIME_COMMAND)?;
    let guarded = install_pretooluse_hook(&primed, GREP_GUARD_MATCHER, GREP_GUARD_COMMAND)?;
    install_status_line(&guarded, STATUS_LINE_COMMAND)
}

/// Merge the hooks bound to one spawn into `existing` settings JSON, preserving every other
/// setting: under `PreToolUse` the write guard, `guard-write` rooted at `roots` in order, and
/// under `StopFailure` one entry per [`AgentFailure::CATEGORIES`], `hook stop-failure` recording
/// that category for `spawn`. Both run `rigger_bin`, and the shell a hook command runs under reads
/// `rigger_bin`, `spawn` and each root as one word whatever they hold ([`shell_quote`]). `existing`
/// may be empty.
pub fn install_spawn_hooks(
    existing: &[u8],
    rigger_bin: &str,
    spawn: &str,
    roots: &[String],
) -> Result<Vec<u8>, Error> {
    let bin = shell_quote(rigger_bin);
    let spawn = shell_quote(spawn);
    let mut guard = format!("{bin} guard-write");
    for root in roots {
        guard.push_str(&format!(" --root {}", shell_quote(root)));
    }
    let mut merged = install_pretooluse_hook(existing, &write_guard_matcher(), &guard)?;
    for category in AgentFailure::CATEGORIES {
        let class = category.as_str();
        let command = format!("{bin} hook stop-failure --spawn {spawn} --class {class}");
        merged = merge_hook_block(&merged, "StopFailure", class, &command)?;
    }
    Ok(merged)
}

/// `arg` as one POSIX shell word: each `'` it holds closes the quote, is escaped and reopens it.
fn shell_quote(arg: &str) -> String {
    single_quoted(arg, r"'\''")
}

/// Whether the settings JSON `existing` already carries every session setting
/// [`install_session_settings`] installs: installing them again would change nothing. Empty or
/// unparseable settings carry none.
pub fn carries_session_settings(existing: &[u8]) -> bool {
    let parse = |bytes: &[u8]| serde_json::from_slice::<Value>(bytes).ok();
    match (parse(existing), install_session_settings(existing)) {
        (Some(current), Ok(merged)) => parse(&merged) == Some(current),
        _ => false,
    }
}

/// The [`HELPER_AGENTS`] as the JSON object Claude Code's `--agents` flag takes:
/// `{name: {description, model, tools: [..], prompt}}`, each field read from the helper's own
/// frontmatter and the prompt its body, verbatim.
pub fn helper_agents_json() -> Result<Value, Error> {
    let mut agents = Map::new();
    for (file, text) in HELPER_AGENTS {
        let (front, body) = rigger_domain::config::split_frontmatter(text)
            .map_err(|e| Error(format!("helper agent {file}: {e}")))?;
        let mut def = Map::new();
        let mut name = None;
        for line in front.lines() {
            let (key, value) = line
                .split_once(':')
                .ok_or_else(|| Error(format!("helper agent {file}: frontmatter line {line:?}")))?;
            let value = value.trim();
            match key {
                "name" => name = Some(value),
                "tools" => {
                    def.insert(
                        key.to_string(),
                        value.split(',').map(str::trim).collect::<Vec<_>>().into(),
                    );
                }
                _ => {
                    def.insert(key.to_string(), value.into());
                }
            }
        }
        def.insert("prompt".to_string(), body.into());
        let name = name.ok_or_else(|| Error(format!("helper agent {file}: no name")))?;
        agents.insert(name.to_string(), Value::Object(def));
    }
    Ok(Value::Object(agents))
}

/// The rigger MCP tools ([`MCP_SERVER_NAME`]'s `mcp__rigger__*`) the [`HELPER_AGENTS`] declare
/// in their frontmatter, in declaration order and each once: what a headless spawn must
/// pre-approve for its `lookup` helper to reach the graph, since under
/// `--permission-prompts none` a tool nobody pre-approved is denied.
pub fn helper_mcp_tools() -> Result<Vec<String>, Error> {
    let prefix = format!("mcp__{MCP_SERVER_NAME}__");
    let mut tools: Vec<String> = Vec::new();
    for def in helper_agents_json()?
        .as_object()
        .into_iter()
        .flat_map(|a| a.values())
    {
        for tool in def["tools"].as_array().into_iter().flatten() {
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
    merge_hook_block(existing, "SessionStart", "", command)
}

/// Merge one hook block that runs `command` into `existing` settings JSON, under the
/// named top-level hook `event_key` (`"SessionStart"`, `"PreToolUse"`, ...), filtered by
/// `matcher` (tool names under `PreToolUse`, a failure class under `StopFailure`, empty for
/// an event `rigger` never matcher-filters, e.g. `SessionStart`). Idempotent (installing the
/// same `command` under the same `event_key` twice does not duplicate the block) and preserves
/// every other setting, every other event key, and every other block already present under
/// THIS event key - a general-purpose event (`PreToolUse`) may already carry entries for a
/// different tool, matcher, or command a person or another tool installed, so this always
/// APPENDS a new block rather than ever replacing the array wholesale, and touches nothing
/// under any matcher/command this call did not itself install. `existing` may be empty. The
/// one mutation authority [`install_session_start`], [`install_pretooluse_hook`] and
/// [`install_spawn_hooks`] build on, so no event kind can diverge on what "merge a hook block"
/// means.
fn merge_hook_block(
    existing: &[u8],
    event_key: &str,
    matcher: &str,
    command: &str,
) -> Result<Vec<u8>, Error> {
    let mut root: Value = if existing.iter().all(u8::is_ascii_whitespace) {
        json!({})
    } else {
        serde_json::from_slice(existing).map_err(|e| Error(format!("parse settings.json: {e}")))?
    };
    let obj = root
        .as_object_mut()
        .ok_or_else(|| Error("settings.json is not a JSON object".into()))?;
    let hooks = obj.entry("hooks").or_insert_with(|| json!({}));
    let hooks_obj = hooks
        .as_object_mut()
        .ok_or_else(|| Error("\"hooks\" is not an object".into()))?;
    let event = hooks_obj.entry(event_key).or_insert_with(|| json!([]));
    let event_arr = event
        .as_array_mut()
        .ok_or_else(|| Error(format!("\"{event_key}\" is not an array")))?;
    if !has_command(event_arr, command) {
        event_arr.push(json!({
            "matcher": matcher,
            "hooks": [{"type": "command", "command": command}],
        }));
    }
    let mut out = serde_json::to_vec_pretty(&root)
        .map_err(|e| Error(format!("encode settings.json: {e}")))?;
    out.push(b'\n');
    Ok(out)
}

fn has_command(session_start: &[Value], command: &str) -> bool {
    session_start.iter().any(|block| {
        block
            .get("hooks")
            .and_then(Value::as_array)
            .is_some_and(|inner| {
                inner
                    .iter()
                    .any(|h| h.get("command").and_then(Value::as_str) == Some(command))
            })
    })
}

/// Merge a PreToolUse hook that runs `command` (matched against tool calls by `matcher`,
/// e.g. `"Grep|Bash"`) into the settings JSON (spec 92, criterion 4: the graph-first lookup
/// hook). A thin wrapper over [`merge_hook_block`] fixed to the `PreToolUse` event - see
/// there for the shared idempotence and other-settings-preserving guarantees `PreToolUse`'s
/// general-purpose nature (a machine may already carry entries for a different tool,
/// matcher, or command under this same event) relies on. `existing` may be empty.
pub fn install_pretooluse_hook(
    existing: &[u8],
    matcher: &str,
    command: &str,
) -> Result<Vec<u8>, Error> {
    merge_hook_block(existing, "PreToolUse", matcher, command)
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
/// same bytes. `existing` may be empty.
pub fn install_mcp_server(
    existing: &[u8],
    name: &str,
    command: &str,
    args: &[&str],
) -> Result<Vec<u8>, Error> {
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
    servers_obj.insert(name.to_string(), json!({"command": command, "args": args}));
    let mut out =
        serde_json::to_vec_pretty(&root).map_err(|e| Error(format!("encode .mcp.json: {e}")))?;
    out.push(b'\n');
    Ok(out)
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

    crate::test_cases! {
        installs_and_is_idempotent: assert_installs_idempotently(
            |existing| install_session_start(existing, "rigger prime"),
            &["SessionStart", "rigger prime"],
            &[("rigger prime", 1)],
        );
        pretooluse_hook_installs_and_is_idempotent: assert_installs_idempotently(
            |existing| install_pretooluse_hook(existing, "Grep|Bash", "rigger grep-guard"),
            &["PreToolUse", "rigger grep-guard", "\"matcher\": \"Grep|Bash\""],
            &[("rigger grep-guard", 1)],
        );
        spawn_hooks_install_and_are_idempotent: assert_installs_idempotently(
            |existing| install_spawn_hooks(existing, "rigger", "u/i#0", &["/wt".to_string()]),
            &["PreToolUse", "StopFailure", "'rigger' guard-write --root '/wt'"],
            &[
                ("guard-write", 1),
                ("hook stop-failure", AgentFailure::CATEGORIES.len()),
            ],
        );
    }

    /// An argument of a hook command stays one word whatever it holds.
    #[test]
    fn shell_quote_keeps_an_argument_one_word() {
        let rows = [
            ("rigger", "'rigger'"),
            ("/a b/rigger", "'/a b/rigger'"),
            ("it's", r"'it'\''s'"),
            ("a'b'c", r"'a'\''b'\''c'"),
            ("u/implementer#0", "'u/implementer#0'"),
            ("$HOME;`ls`", "'$HOME;`ls`'"),
            ("", "''"),
        ];
        assert_eq!(
            rows.map(|(arg, _)| shell_quote(arg)),
            rows.map(|(_, quoted)| quoted)
        );
    }

    /// A spawn's hooks over a bare and a hostile `rigger_bin`, one and two roots: the whole
    /// `PreToolUse` and `StopFailure` arrays, every category in order, on the settings kept.
    #[test]
    fn spawn_hooks_are_the_write_guard_and_one_stop_failure_entry_per_category() {
        let rows = [
            (
                "rigger",
                "u/implementer#0",
                vec!["/wt"],
                "'rigger' guard-write --root '/wt'",
                "'rigger' hook stop-failure --spawn 'u/implementer#0' --class",
            ),
            (
                "/opt/it's here/rigger",
                "u/reviewer#1",
                vec!["/wt tree", "/scratch/s"],
                r"'/opt/it'\''s here/rigger' guard-write --root '/wt tree' --root '/scratch/s'",
                r"'/opt/it'\''s here/rigger' hook stop-failure --spawn 'u/reviewer#1' --class",
            ),
        ];
        for (bin, spawn, roots, guard, stop_failure) in rows {
            let roots: Vec<String> = roots.into_iter().map(str::to_string).collect();
            let stop_failures: Vec<Value> = crate::agent::AgentFailure::CATEGORIES
                .iter()
                .map(|category| {
                    let class = category.as_str();
                    hook_block(class, &format!("{stop_failure} {class}"))
                })
                .collect();
            assert_settings(
                install_spawn_hooks(br#"{"model":"opus"}"#, bin, spawn, &roots).unwrap(),
                &[
                    ("/model", Value::from("opus")),
                    (
                        "/hooks/PreToolUse",
                        json!([hook_block("Edit|MultiEdit|Write|NotebookEdit", guard)]),
                    ),
                    ("/hooks/StopFailure", Value::Array(stop_failures)),
                ],
            );
        }
    }

    /// One hook block running `command` under `matcher`, as the installers write it.
    fn hook_block(matcher: &str, command: &str) -> Value {
        json!({"matcher": matcher, "hooks": [hook(command)]})
    }

    /// One inner hook running `command`.
    fn hook(command: &str) -> Value {
        json!({"type": "command", "command": command})
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

    /// The spawn host carries the grep-guard exactly when the workflow's defaults declare it.
    #[test]
    fn the_spawn_host_carries_the_declared_grep_guard() {
        let resolved = [false, true].map(|grep_guard| {
            resolve_spawn_host(&rigger_domain::config::Defaults {
                grep_guard,
                ..Default::default()
            })
        });
        assert_eq!(
            resolved,
            [
                SpawnHost { grep_guard: false },
                SpawnHost { grep_guard: true },
            ]
        );
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
        let rows: [(&[u8], bool, bool); 6] = [
            (b"", false, false),
            (b"not json", true, false),
            (&primed, false, true),
            (&primed, true, false),
            (&primed_and_guarded, true, true),
            (&primed_and_guarded, false, true),
        ];
        let carried: Vec<bool> = rows
            .iter()
            .map(|(existing, grep_guard, _)| carries_session_settings(existing, *grep_guard))
            .collect();
        assert_eq!(carried, rows.map(|(_, _, carries)| carries));
    }

    /// The guard's install: one entry under the current matcher, appended beside every foreign
    /// hook, an entry under a drifted matcher replaced so exactly one remains, and a second
    /// install writing the same bytes.
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
    /// empties, and keeps every other hook and setting.
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
        let refused = install_spawn_hooks(b"[]", "rigger", "u/implementer#0", &[]).unwrap_err();
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
            install_pretooluse_hook(
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
        let out = install_pretooluse_hook(existing, "Grep|Bash", "rigger grep-guard").unwrap();
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

    #[test]
    fn helper_mcp_tools_are_the_rigger_tools_the_helpers_declare() {
        assert_eq!(
            helper_mcp_tools().unwrap(),
            [
                "mcp__rigger__rigger_graph",
                "mcp__rigger__rigger_ground",
                "mcp__rigger__rigger_peers",
            ]
        );
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
