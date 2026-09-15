//! Installs Rigger's Claude Code integration: merges a SessionStart hook into
//! .claude/settings.json - preserving every other setting - so a session opened
//! in a Rigger repository starts primed with the project's recent decisions.

use serde_json::{json, Value};

#[derive(Debug, thiserror::Error)]
#[error("hooks: {0}")]
pub struct Error(pub String);

/// Merge a SessionStart hook that runs `command` into the settings JSON. Idempotent
/// (installing twice does not duplicate the hook) and preserves all other settings.
/// `existing` may be empty.
pub fn install_session_start(existing: &[u8], command: &str) -> Result<Vec<u8>, Error> {
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
    let session = hooks_obj.entry("SessionStart").or_insert_with(|| json!([]));
    let session_arr = session
        .as_array_mut()
        .ok_or_else(|| Error("\"SessionStart\" is not an array".into()))?;
    if !has_command(session_arr, command) {
        session_arr.push(json!({
            "matcher": "",
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
/// hook). Sibling to [`install_session_start`], with the same idempotence and
/// other-settings-preserving guarantees, but for a DIFFERENT event: unlike `SessionStart`
/// (where rigger installs the only entry that will ever exist there), `PreToolUse` is a
/// general-purpose event a machine may already carry OTHER entries under - for a different
/// tool, a different matcher, a formatter, a linter, anything a person or another tool
/// installed. So this merge APPENDS a new block rather than ever replacing the array
/// wholesale, and touches nothing under any matcher/command this call did not itself
/// install. `existing` may be empty.
pub fn install_pretooluse_hook(
    existing: &[u8],
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
    let pretool = hooks_obj.entry("PreToolUse").or_insert_with(|| json!([]));
    let pretool_arr = pretool
        .as_array_mut()
        .ok_or_else(|| Error("\"PreToolUse\" is not an array".into()))?;
    if !has_command(pretool_arr, command) {
        pretool_arr.push(json!({
            "matcher": matcher,
            "hooks": [{"type": "command", "command": command}],
        }));
    }
    let mut out = serde_json::to_vec_pretty(&root)
        .map_err(|e| Error(format!("encode settings.json: {e}")))?;
    out.push(b'\n');
    Ok(out)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn installs_and_is_idempotent() {
        let first = install_session_start(b"", "rigger prime").unwrap();
        let s = String::from_utf8(first.clone()).unwrap();
        assert!(s.contains("SessionStart") && s.contains("rigger prime"));
        let second = install_session_start(&first, "rigger prime").unwrap();
        let s2 = String::from_utf8(second).unwrap();
        assert_eq!(
            s2.matches("rigger prime").count(),
            1,
            "installing twice must not duplicate"
        );
    }

    #[test]
    fn preserves_other_settings() {
        let out = install_session_start(br#"{"model":"opus"}"#, "rigger prime").unwrap();
        let v: Value = serde_json::from_slice(&out).unwrap();
        assert_eq!(v["model"], "opus");
        assert_eq!(
            v["hooks"]["SessionStart"][0]["hooks"][0]["command"],
            "rigger prime"
        );
    }

    #[test]
    fn pretooluse_hook_installs_and_is_idempotent() {
        let first = install_pretooluse_hook(b"", "Grep|Bash", "rigger grep-guard").unwrap();
        let s = String::from_utf8(first.clone()).unwrap();
        assert!(s.contains("PreToolUse") && s.contains("rigger grep-guard"));
        assert!(s.contains("\"matcher\": \"Grep|Bash\""));
        let second = install_pretooluse_hook(&first, "Grep|Bash", "rigger grep-guard").unwrap();
        let s2 = String::from_utf8(second).unwrap();
        assert_eq!(
            s2.matches("rigger grep-guard").count(),
            1,
            "installing twice must not duplicate"
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
    fn pretooluse_hook_composes_with_the_session_start_hook() {
        // Both installers merge into the SAME settings.json's "hooks" object (SessionStart
        // vs PreToolUse) - neither must clobber the other's event key.
        let after_session_start = install_session_start(b"", "rigger prime").unwrap();
        let out = install_pretooluse_hook(&after_session_start, "Grep|Bash", "rigger grep-guard")
            .unwrap();
        let v: Value = serde_json::from_slice(&out).unwrap();
        assert_eq!(
            v["hooks"]["SessionStart"][0]["hooks"][0]["command"],
            "rigger prime"
        );
        assert_eq!(
            v["hooks"]["PreToolUse"][0]["hooks"][0]["command"],
            "rigger grep-guard"
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
    fn mcp_server_preserves_other_servers_and_other_top_level_keys() {
        let existing = br#"{
            "someOtherSetting": true,
            "mcpServers": {
                "unrelated": {"command": "some-other-tool", "args": []}
            }
        }"#;
        let out = install_mcp_server(existing, "rigger", "rigger", &["mcp"]).unwrap();
        let v: Value = serde_json::from_slice(&out).unwrap();
        assert_eq!(v["someOtherSetting"], true);
        assert_eq!(v["mcpServers"]["unrelated"]["command"], "some-other-tool");
        assert_eq!(v["mcpServers"]["rigger"]["command"], "rigger");
    }

    #[test]
    fn mcp_server_self_heals_a_drifted_entry() {
        // An older rigger build (or a hand edit) wrote a different shape under "rigger" -
        // a fresh install self-heals it to the current shape, the same drift-repair every
        // other `rigger setup` step performs.
        let existing = br#"{"mcpServers": {"rigger": {"command": "/old/stale/path", "args": []}}}"#;
        let out = install_mcp_server(existing, "rigger", "rigger", &["mcp"]).unwrap();
        let v: Value = serde_json::from_slice(&out).unwrap();
        assert_eq!(v["mcpServers"]["rigger"]["command"], "rigger");
        assert_eq!(v["mcpServers"]["rigger"]["args"][0], "mcp");
    }
}
