//! The default AgentDriver: spawn agents by shelling out to the `claude` CLI, so
//! Rigger depends on no particular editor or runtime. A subprocess agent cannot
//! call the in-process emit callback, so this driver ignores it; the workflow
//! driver is the one that delivers live emission.

use std::process::Command;

use serde_json::Value;

use crate::conductor::{AgentDriver, AgentResult, Error, SpawnOpts};
use crate::config::AgentDef;

/// Driver spawns agents via the `claude` CLI.
pub struct Driver {
    pub bin: String,
}

impl Default for Driver {
    fn default() -> Self {
        Driver {
            bin: "claude".to_string(),
        }
    }
}

impl AgentDriver for Driver {
    fn spawn(
        &self,
        agent: &AgentDef,
        prompt: &str,
        opts: &SpawnOpts,
        _emit: &dyn Fn(&str, Value) -> Result<(), Error>,
    ) -> Result<AgentResult, Error> {
        let bin = if self.bin.is_empty() {
            "claude"
        } else {
            &self.bin
        };
        let mut cmd = Command::new(bin);
        cmd.args(build_args(agent, prompt));
        if !opts.dir.is_empty() {
            cmd.current_dir(&opts.dir);
        }
        let out = cmd
            .output()
            .map_err(|e| Error(format!("cli driver: spawn agent {:?}: {e}", agent.id)))?;
        let result = AgentResult {
            output: String::from_utf8_lossy(&out.stdout).into_owned(),
        };
        if !out.status.success() {
            return Err(Error(format!(
                "cli driver: agent {:?} exited unsuccessfully ({})",
                agent.id, out.status
            )));
        }
        Ok(result)
    }
}

/// Build the `claude` headless invocation: the agent's instructions plus the task
/// become the prompt, with the model and allowed tools the agent declares.
pub fn build_args(agent: &AgentDef, prompt: &str) -> Vec<String> {
    let full = if agent.prompt.is_empty() {
        prompt.to_string()
    } else {
        format!("{}\n\n{}", agent.prompt, prompt)
    };
    let mut args = vec!["-p".to_string(), full];
    if !agent.model.is_empty() {
        args.push("--model".to_string());
        args.push(agent.model.clone());
    }
    if !agent.tools.is_empty() {
        args.push("--allowed-tools".to_string());
        args.push(agent.tools.join(","));
    }
    args
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn combines_persona_and_task() {
        let a = AgentDef {
            id: "impl".into(),
            model: "sonnet".into(),
            tools: vec!["Read".into(), "Bash".into()],
            prompt: "You implement findings.".into(),
            ..Default::default()
        };
        let args = build_args(&a, "do the thing");
        let pi = args.iter().position(|x| x == "-p").unwrap();
        assert!(args[pi + 1].contains("You implement findings."));
        assert!(args[pi + 1].contains("do the thing"));
        let mi = args.iter().position(|x| x == "--model").unwrap();
        assert_eq!(args[mi + 1], "sonnet");
        let ti = args.iter().position(|x| x == "--allowed-tools").unwrap();
        assert_eq!(args[ti + 1], "Read,Bash");
    }

    #[test]
    fn minimal_agent_yields_just_the_prompt() {
        let args = build_args(
            &AgentDef {
                id: "bare".into(),
                ..Default::default()
            },
            "task",
        );
        assert_eq!(args, ["-p", "task"]);
    }
}
