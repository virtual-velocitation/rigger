//! Installs Rigger's Claude Code integration: merges a SessionStart hook into
//! .claude/settings.json - preserving every other setting - so a session opened
//! in a Rigger repository starts primed with the project's recent decisions.
//!
//! This is also the one home of WHAT a rigger session carries: the hook and status-line
//! commands, the MCP server name and the fan-out helper agents. `rigger setup` writes them into
//! the operator's checkout; the headless host hands the same values to every spawn on its
//! command line (a fresh unit worktree carries no `.claude/` of its own), so the two cannot
//! drift.

use serde_json::{json, Map, Value};

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

/// Merge a SessionStart hook that runs `command` into the settings JSON. Idempotent
/// (installing twice does not duplicate the hook) and preserves all other settings.
/// `existing` may be empty. A thin wrapper over [`merge_hook_block`] fixed to the
/// `SessionStart` event with an empty matcher (rigger installs the only entry that will
/// ever exist there).
pub fn install_session_start(existing: &[u8], command: &str) -> Result<Vec<u8>, Error> {
    merge_hook_block(existing, "SessionStart", "", command)
}

/// Merge one hook block that runs `command` into `existing` settings JSON, under the
/// named top-level hook `event_key` (`"SessionStart"`, `"PreToolUse"`, ...), matched
/// against tool calls by `matcher` (empty for an event `rigger` never matcher-filters,
/// e.g. `SessionStart`). Idempotent (installing the same `command` under the same
/// `event_key` twice does not duplicate the block) and preserves every other setting,
/// every other event key, and every other block already present under THIS event key -
/// a general-purpose event (`PreToolUse`) may already carry entries for a different
/// tool, matcher, or command a person or another tool installed, so this always APPENDS
/// a new block rather than ever replacing the array wholesale, and touches nothing under
/// any matcher/command this call did not itself install. `existing` may be empty. The
/// one mutation authority both [`install_session_start`] and [`install_pretooluse_hook`]
/// build on, so the two event kinds can never diverge on what "merge a hook block" means.
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
    /// onto its own output does not duplicate `command`.
    fn assert_installs_idempotently(
        install: impl Fn(&[u8]) -> Result<Vec<u8>, Error>,
        needles: &[&str],
        command: &str,
    ) {
        let first = install(b"").unwrap();
        let s = String::from_utf8(first.clone()).unwrap();
        for needle in needles {
            assert!(s.contains(needle), "{needle} is installed: {s}");
        }
        let s2 = String::from_utf8(install(&first).unwrap()).unwrap();
        assert_eq!(
            s2.matches(command).count(),
            1,
            "installing twice must not duplicate"
        );
    }

    crate::test_cases! {
        installs_and_is_idempotent: assert_installs_idempotently(
            |existing| install_session_start(existing, "rigger prime"),
            &["SessionStart", "rigger prime"],
            "rigger prime",
        );
        pretooluse_hook_installs_and_is_idempotent: assert_installs_idempotently(
            |existing| install_pretooluse_hook(existing, "Grep|Bash", "rigger grep-guard"),
            &["PreToolUse", "rigger grep-guard", "\"matcher\": \"Grep|Bash\""],
            "rigger grep-guard",
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
