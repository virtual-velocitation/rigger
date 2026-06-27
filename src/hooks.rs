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
}
