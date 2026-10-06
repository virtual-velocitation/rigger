//! The default AgentDriver: spawn agents by shelling out to the `claude` CLI, so
//! Rigger depends on no particular editor or runtime. A subprocess agent cannot
//! call the in-process emit callback WHILE it runs (it has no live MCP channel),
//! so this driver bridges emission AFTER the run: it parses the agent's stdout for
//! the emit-protocol lines (DecisionMade / ReviewFinding / UnitProposed JSON, the
//! same shape the EMIT_PROTOCOL / review_protocol instruct the agent to print) and
//! replays each through `emit`. So the cli path also records decisions, extends the
//! living DAG (UnitProposed), and feeds the side-car - the only difference from the
//! workflow driver is that emission is post-hoc, not live mid-run.

use serde_json::Value;

use crate::agent::{AgentDriver, AgentResult, Error, SpawnOpts, TYPE_UNIT_PROPOSED};
use crate::config::AgentDef;
use crate::contextgraph::{TYPE_DECISION_MADE, TYPE_REVIEW_FINDING};
use crate::driver::{allowed_tools_args, bin_or_path_default, harness_env, spawn_config_args};

/// Driver spawns agents via the `claude` CLI.
pub struct Driver {
    /// The `claude` binary to run. Empty resolves to `"claude"` on `$PATH`.
    pub bin: String,
    /// The `rigger` binary each spawn runs as its MCP server (`<rigger_bin> mcp`). Empty
    /// resolves to `"rigger"` on `$PATH`, the same rule the headless host's own
    /// `rigger_bin` follows.
    pub rigger_bin: String,
}

impl Default for Driver {
    fn default() -> Self {
        Driver {
            bin: "claude".to_string(),
            rigger_bin: "rigger".to_string(),
        }
    }
}

impl AgentDriver for Driver {
    fn spawn(
        &self,
        agent: &AgentDef,
        prompt: &str,
        opts: &SpawnOpts,
        emit: &dyn Fn(&str, Value) -> Result<(), Error>,
    ) -> Result<AgentResult, Error> {
        let bin = bin_or_path_default(&self.bin, "claude");
        let mut cmd = crate::subprocess::command_in(bin, &opts.dir);
        // Live progress (spec 14): frame the same per-step progress instruction the workflow
        // drivers give, so a worker on this path also reports what it is doing between
        // milestones. (This synchronous, non-parking path has no parked frontier entry, so the
        // current consolidator does not surface it - the emit is recorded and future-proof.)
        let framed = format!(
            "{prompt}\n\n--- rigger driver ---\nLIVE PROGRESS: after each significant step (a search, a build, a commit, a decision) report ONE short line of what you just did by running (Bash): rigger progress '{}' '<one line: what you just did>'. Keep it flowing while you work.",
            opts.id
        );
        cmd.args(build_args(
            agent,
            &framed,
            opts,
            bin_or_path_default(&self.rigger_bin, "rigger"),
        )?);
        // The harness environment every headless worker gets ([`harness_env`]), applied
        // first so the build environment below overrides it under the same key.
        for (k, v) in harness_env() {
            cmd.env(k, v);
        }
        // The ONE build-environment authority's second injection site (spec 65): every
        // var the resolver derived (empty when no wrapper is configured, applying
        // nothing) so this agent's OWN `cargo test`/`cargo build` invocations hit the
        // same compilation cache under the same settings a gate build gets.
        for (k, v) in &opts.env {
            cmd.env(k, v);
        }
        let out = cmd
            .output()
            .map_err(|e| Error(format!("cli driver: spawn agent {:?}: {e}", agent.id)))?;
        let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
        // Bridge emission: a subprocess agent has no live MCP channel, so its
        // decisions/findings are printed to stdout per the EMIT_PROTOCOL /
        // review_protocol. Replay each through `emit` so the cli path also records
        // decisions, extends the living DAG (UnitProposed), and feeds the side-car -
        // exactly what the workflow driver does live. Done BEFORE the exit-status
        // check so a partially-failed run still records what the agent decided.
        bridge_emits(&stdout, emit)?;
        // A blocking subprocess driver does not learn the resolved model id (spec 05 line
        // 52 sources it from the worker's `rigger result --meta` on the stepwise path), so
        // it leaves it empty and the metadata is then omitted.
        let result = AgentResult {
            output: stdout,
            resolved_model: String::new(),
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

/// The emit-protocol event types the cli driver bridges from agent stdout. Only
/// these self-describing types are replayed; any other JSON line (the agent's
/// final result JSON, a verdict line, ad-hoc logging) is ignored.
fn is_emit_type(type_: &str) -> bool {
    matches!(
        type_,
        TYPE_DECISION_MADE | TYPE_REVIEW_FINDING | TYPE_UNIT_PROPOSED
    )
}

/// Parse an agent's stdout for emit-protocol lines and replay each through `emit`.
///
/// An emit line is a single-line JSON object carrying its own `type` (one of
/// `DecisionMade` / `ReviewFinding` / `UnitProposed`) and a `data` object - the
/// same `{type, data}` shape the rigger_emit MCP tool takes, so the cli and
/// workflow paths record byte-identical events. Lines that are not JSON, not
/// objects, or not one of the emit types are skipped (an agent prints plenty of
/// other text). A bare object with the protocol fields but no `data` wrapper (the
/// EMIT_PROTOCOL prints the decision fields at the top level) is tolerated: the
/// object minus `type` becomes the data.
///
/// The first `emit` error is propagated (the event store is down, etc.); it is not
/// swallowed, since a lost decision silently corrupts the cross-agent memory.
fn bridge_emits(
    stdout: &str,
    emit: &dyn Fn(&str, Value) -> Result<(), Error>,
) -> Result<(), Error> {
    for line in stdout.lines() {
        let line = line.trim();
        if !line.starts_with('{') {
            continue;
        }
        let Ok(Value::Object(obj)) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        let Some(type_) = obj.get("type").and_then(Value::as_str) else {
            continue;
        };
        if !is_emit_type(type_) {
            continue;
        }
        // Prefer an explicit `data` object (the rigger_emit shape). Absent it, treat
        // the rest of the object (everything but `type`) as the data, so an agent
        // that printed the EMIT_PROTOCOL's flat decision fields still records.
        let data = match obj.get("data") {
            Some(d) => d.clone(),
            None => {
                let mut rest = obj.clone();
                rest.remove("type");
                Value::Object(rest)
            }
        };
        emit(type_, data)?;
    }
    Ok(())
}

/// Build the `claude` headless invocation: the grounded task is the `-p` prompt and
/// the agent's PERSONA (its role) is the SYSTEM prompt (`--system-prompt`), with the
/// model and allowed tools the agent declares. The persona is taken from
/// `opts.system_prompt` - the conductor's single persona source (`SpawnOpts::system_prompt`,
/// set from `AgentDef::prompt`) - NOT read from `agent.prompt` here, so the cli and
/// workflow paths thread the SAME persona and cannot diverge. An empty persona omits
/// the flag (the agent runs with the default system prompt). `opts.attempt` selects the
/// cascade rung ([`AgentDef::model_for_attempt`], spec 10 unit 4): a `model_ladder`
/// agent runs on the rung it escalated to for this remediation attempt.
///
/// The spawn configuration follows (see [`spawn_config_args`]: permissions, then the
/// operator's read-only `<rigger_bin> mcp` server, the session settings and the helpers),
/// so this argv and the headless host's agree on everything but the protocol flags. This
/// host keeps the operator's server rather than a spawn-bound one because its emits travel
/// on stdout ([`bridge_emits`]), not through the server. Fails only when
/// `opts.settings_json` is not a JSON object the session settings can merge into.
pub fn build_args(
    agent: &AgentDef,
    prompt: &str,
    opts: &SpawnOpts,
    rigger_bin: &str,
) -> Result<Vec<String>, Error> {
    let mut args = vec!["-p".to_string(), prompt.to_string()];
    if !opts.system_prompt.is_empty() {
        args.push("--system-prompt".to_string());
        args.push(opts.system_prompt.clone());
    }
    let model = agent.model_for_attempt(opts.attempt);
    if !model.is_empty() {
        args.push("--model".to_string());
        args.push(model);
    }
    args.extend(allowed_tools_args(agent)?);
    args.extend(spawn_config_args(
        rigger_bin,
        &["mcp"],
        &opts.settings_json,
    )?);
    Ok(args)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::rc::Rc;
    use std::sync::Mutex;

    /// The (type, data) pairs `emit` was called with, captured by the test recorder.
    type Recorded = Rc<RefCell<Vec<(String, Value)>>>;

    /// A test emit sink: records every (type, data) pair `emit` was called with.
    fn recorder() -> (impl Fn(&str, Value) -> Result<(), Error>, Recorded) {
        let calls: Recorded = Rc::new(RefCell::new(Vec::new()));
        let sink = calls.clone();
        let emit = move |t: &str, v: Value| {
            sink.borrow_mut().push((t.to_string(), v));
            Ok(())
        };
        (emit, calls)
    }

    #[test]
    fn bridge_emits_replays_each_emit_protocol_line() {
        // A fake agent stdout with a DecisionMade and a UnitProposed line (plus
        // chatter and the agent's final result JSON, which must NOT be emitted).
        let stdout = "\
thinking out loud, not json\n\
{\"type\":\"DecisionMade\",\"data\":{\"id\":\"d1\",\"summary\":\"split the pipeline\",\"governs\":[\"a.rs\"]}}\n\
{\"some\":\"unrelated json object\"}\n\
{\"type\":\"UnitProposed\",\"data\":{\"id\":\"u1\",\"agent\":\"implementer\",\"coverage\":\"c1\"}}\n\
{\"id\":\"final\",\"pass\":true,\"evidence\":\"done\"}\n";

        let (emit, calls) = recorder();
        bridge_emits(stdout, &emit).unwrap();

        let calls = calls.borrow();
        assert_eq!(
            calls.len(),
            2,
            "exactly the two emit-protocol lines are bridged: {calls:?}"
        );

        let (t0, d0) = &calls[0];
        assert_eq!(t0, TYPE_DECISION_MADE);
        assert_eq!(d0["id"], "d1");
        assert_eq!(d0["summary"], "split the pipeline");
        assert_eq!(d0["governs"][0], "a.rs");

        let (t1, d1) = &calls[1];
        assert_eq!(t1, TYPE_UNIT_PROPOSED);
        assert_eq!(d1["id"], "u1");
        assert_eq!(d1["agent"], "implementer");
        assert_eq!(d1["coverage"], "c1");
    }

    #[test]
    fn bridge_emits_bridges_a_review_finding_and_tolerates_a_flat_object() {
        // A ReviewFinding with an explicit data wrapper, and a flat DecisionMade
        // (the EMIT_PROTOCOL prints the decision fields at the top level) - the
        // flat form's non-type fields become the data.
        let stdout = "\
{\"type\":\"ReviewFinding\",\"data\":{\"id\":\"f1\",\"summary\":\"skips the buffer\",\"about\":[\"combat.rs\"]}}\n\
{\"type\":\"DecisionMade\",\"id\":\"d2\",\"summary\":\"flat form\",\"governs\":[\"b.rs\"]}\n";

        let (emit, calls) = recorder();
        bridge_emits(stdout, &emit).unwrap();

        let calls = calls.borrow();
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0].0, TYPE_REVIEW_FINDING);
        assert_eq!(calls[0].1["id"], "f1");
        assert_eq!(calls[0].1["about"][0], "combat.rs");
        // The flat object's data is everything but `type`.
        assert_eq!(calls[1].0, TYPE_DECISION_MADE);
        assert_eq!(calls[1].1["id"], "d2");
        assert_eq!(calls[1].1["summary"], "flat form");
        assert!(
            calls[1].1.get("type").is_none(),
            "the type field is not duplicated into the data"
        );
    }

    #[test]
    fn bridge_emits_propagates_the_first_emit_error() {
        // A failing emit (e.g. the event store is down) must not be swallowed: a lost
        // decision silently corrupts the cross-agent memory.
        let stdout = "{\"type\":\"DecisionMade\",\"data\":{\"id\":\"d1\"}}\n";
        let emit = |_: &str, _: Value| Err(Error("store down".into()));
        let err = bridge_emits(stdout, &emit).unwrap_err();
        assert_eq!(err.0, "store down");
    }

    #[cfg(unix)]
    #[test]
    fn spawn_shells_out_and_bridges_the_agents_emits() {
        // End-to-end through the real spawn path: a tiny executable shell script
        // acts as the "agent" binary. It ignores its args (build_args passes `-p
        // <prompt>`) and prints two emit-protocol lines plus chatter. The driver
        // must shell out, capture stdout, and bridge BOTH emits through the
        // callback - proving the cli path records decisions and extends the DAG.
        //
        // The agent binary is a checked-in executable fixture (never written at
        // test time), so no concurrent test's fork+exec can inherit a write fd to
        // it - which is what used to cause an intermittent ETXTBSY here under
        // parallel test execution.
        let bin = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/fake-agent.sh")
            .to_string_lossy()
            .into_owned();

        let driver = Driver {
            bin,
            ..Driver::default()
        };
        let calls = Mutex::new(Vec::new());
        let emit = |t: &str, v: Value| {
            calls.lock().unwrap().push((t.to_string(), v));
            Ok(())
        };
        let result = driver
            .spawn(
                &AgentDef {
                    id: "echoer".into(),
                    ..Default::default()
                },
                "the task",
                &SpawnOpts {
                    system_prompt: String::new(),
                    dir: String::new(),
                    isolation: false,
                    parallel: false,
                    blast_radius: Vec::new(),
                    ..Default::default()
                },
                &emit,
            )
            .unwrap();

        // The full stdout is returned as the agent result.
        assert!(result.output.contains("from a real subprocess"));
        // Both emit-protocol lines were bridged; the chatter and the final result
        // JSON were not.
        let calls = calls.lock().unwrap();
        assert_eq!(
            calls.len(),
            2,
            "the subprocess agent's two emits were bridged: {calls:?}"
        );
        assert_eq!(calls[0].0, TYPE_DECISION_MADE);
        assert_eq!(calls[0].1["id"], "sd1");
        assert_eq!(calls[1].0, TYPE_UNIT_PROPOSED);
        assert_eq!(calls[1].1["id"], "su1");
    }

    #[cfg(unix)]
    #[test]
    fn spawn_applies_opts_env_to_the_subprocess() {
        // The ONE build-environment authority's second injection site (spec 65): the
        // conductor resolves the shared wrapper/cache/incremental vars into
        // `SpawnOpts.env`, and this driver must apply EVERY one of them to the spawned
        // agent process - the same as `gate::ExecRunner::run` does for a gate build - so
        // an agent's own `cargo test` invocation hits the same compilation cache. A
        // fixture "agent" echoes what it actually sees; empty `env` (the default,
        // wrapper off) must not inject anything either.
        let bin = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/env-echo-agent.sh")
            .to_string_lossy()
            .into_owned();
        let driver = Driver {
            bin,
            ..Driver::default()
        };
        let emit = |_: &str, _: Value| Ok(());

        let with = driver
            .spawn(
                &AgentDef {
                    id: "e".into(),
                    ..Default::default()
                },
                "task",
                &SpawnOpts {
                    env: vec![
                        ("RUSTC_WRAPPER".to_string(), "sccache".to_string()),
                        ("SCCACHE_DIR".to_string(), "/shared/build-cache".to_string()),
                        ("CARGO_INCREMENTAL".to_string(), "0".to_string()),
                    ],
                    ..Default::default()
                },
                &emit,
            )
            .unwrap();
        assert!(
            with.output.contains("RUSTC_WRAPPER=sccache"),
            "output: {:?}",
            with.output
        );
        assert!(with.output.contains("SCCACHE_DIR=/shared/build-cache"));
        assert!(with.output.contains("CARGO_INCREMENTAL=0"));

        let without = driver
            .spawn(
                &AgentDef {
                    id: "e".into(),
                    ..Default::default()
                },
                "task",
                &SpawnOpts::default(),
                &emit,
            )
            .unwrap();
        assert!(
            without.output.contains("RUSTC_WRAPPER=\n"),
            "an empty SpawnOpts.env must inject nothing: {:?}",
            without.output
        );
    }

    #[cfg(unix)]
    #[test]
    fn spawn_applies_the_harness_env_and_lets_opts_env_override_it() {
        // Every headless worker gets the harness environment (`harness_env`): the print-mode
        // idle ceiling off, so a worker that ended its turn waiting on a background helper
        // stays open for the helper's result. The build environment is applied after it, so
        // an operator's own value under the same key wins.
        let bin = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/env-echo-agent.sh")
            .to_string_lossy()
            .into_owned();
        let driver = Driver {
            bin,
            ..Driver::default()
        };
        let emit = |_: &str, _: Value| Ok(());
        let agent = AgentDef {
            id: "e".into(),
            ..Default::default()
        };

        let default = driver
            .spawn(&agent, "task", &SpawnOpts::default(), &emit)
            .unwrap();
        assert!(
            default
                .output
                .contains("CLAUDE_CODE_PRINT_BG_WAIT_CEILING_MS=0\n"),
            "output: {:?}",
            default.output
        );

        let overridden = driver
            .spawn(
                &agent,
                "task",
                &SpawnOpts {
                    env: vec![(
                        "CLAUDE_CODE_PRINT_BG_WAIT_CEILING_MS".to_string(),
                        "900000".to_string(),
                    )],
                    ..Default::default()
                },
                &emit,
            )
            .unwrap();
        assert!(
            overridden
                .output
                .contains("CLAUDE_CODE_PRINT_BG_WAIT_CEILING_MS=900000\n"),
            "an opts.env value under the same key wins: {:?}",
            overridden.output
        );
    }

    /// The index of `flag` in the built `args`, panicking when it is absent.
    fn arg_index(args: &[String], flag: &str) -> usize {
        args.iter()
            .position(|x| x == flag)
            .unwrap_or_else(|| panic!("{flag} must be in the args: {args:?}"))
    }

    /// `persona_tools` followed by the helpers' MCP tools, the `--allowed-tools` value a
    /// persona declaring `persona_tools` runs with.
    fn with_helper_tools(persona_tools: &str) -> String {
        format!(
            "{persona_tools},{}",
            crate::hooks::helper_mcp_tools().unwrap().join(",")
        )
    }

    /// The spawn options carrying `system_prompt` as the persona and `attempt` as the
    /// remediation attempt, everything else at its default.
    fn persona_opts(system_prompt: &str, attempt: u32) -> SpawnOpts {
        SpawnOpts {
            system_prompt: system_prompt.to_string(),
            attempt,
            ..Default::default()
        }
    }

    /// `build_args` for `agent` with the task `prompt`, persona `system_prompt` and remediation
    /// `attempt`, naming the `rigger` binary on `$PATH` for the MCP server.
    fn args_for(agent: &AgentDef, prompt: &str, system_prompt: &str, attempt: u32) -> Vec<String> {
        build_args(
            agent,
            prompt,
            &persona_opts(system_prompt, attempt),
            "rigger",
        )
        .unwrap()
    }

    #[test]
    fn persona_is_the_system_prompt_task_is_the_prompt() {
        // The persona (the agent's role) is threaded in as the `system_prompt` arg -
        // the conductor's single persona source - and goes to `--system-prompt`; the
        // grounded task is the `-p` prompt. The persona is NOT read from agent.prompt
        // here, so the cli path uses the same persona source the workflow path does.
        let a = AgentDef {
            id: "impl".into(),
            model: "sonnet".into(),
            tools: vec!["Read".into(), "Bash".into()],
            // agent.prompt is deliberately set but must NOT leak into the args: the
            // persona arrives via the system_prompt parameter, not from this field.
            prompt: "stale body that must not be used".into(),
            ..Default::default()
        };
        let args = args_for(&a, "do the thing", "You implement findings.", 0);
        // The grounded task is the -p prompt, and the persona is NOT spliced into it.
        let pi = arg_index(&args, "-p");
        assert_eq!(args[pi + 1], "do the thing");
        assert!(!args[pi + 1].contains("You implement findings."));
        assert!(!args[pi + 1].contains("stale body"));
        // The persona is the system prompt.
        let si = arg_index(&args, "--system-prompt");
        assert_eq!(args[si + 1], "You implement findings.");
        let mi = arg_index(&args, "--model");
        assert_eq!(args[mi + 1], "sonnet");
        let ti = arg_index(&args, "--allowed-tools");
        assert_eq!(args[ti + 1], with_helper_tools("Read,Bash"));
    }

    #[test]
    fn recurse_false_drops_the_agent_tool_from_allowed_tools() {
        let a = AgentDef {
            id: "impl".into(),
            tools: vec!["Read".into(), "Agent".into()],
            recurse: false,
            ..Default::default()
        };
        let args = args_for(&a, "task", "", 0);
        let ti = arg_index(&args, "--allowed-tools");
        assert_eq!(args[ti + 1], with_helper_tools("Read"));
        assert!(!args[ti + 1].contains("Agent"));
    }

    #[test]
    fn recurse_true_keeps_the_agent_tool() {
        let a = AgentDef {
            id: "lead".into(),
            tools: vec!["Read".into(), "Agent".into()],
            recurse: true,
            ..Default::default()
        };
        let args = args_for(&a, "task", "", 0);
        let ti = arg_index(&args, "--allowed-tools");
        assert_eq!(args[ti + 1], with_helper_tools("Read,Agent"));
    }

    #[test]
    fn a_model_ladder_agent_runs_the_attempts_rung() {
        // The cli invocation's `--model` is the cascade rung this attempt resolves (spec 10
        // unit 4): attempt 0 runs the cheap first rung, a later remediation attempt the next.
        let a = AgentDef {
            id: "impl".into(),
            model_ladder: vec!["haiku".into(), "sonnet".into(), "opus".into()],
            ..Default::default()
        };
        let model_at = |attempt: u32| {
            let args = args_for(&a, "task", "", attempt);
            let mi = arg_index(&args, "--model");
            args[mi + 1].clone()
        };
        assert_eq!(model_at(0), "haiku", "attempt 0 runs the first rung");
        assert_eq!(
            model_at(1),
            "sonnet",
            "the first remediation advances one rung"
        );
        assert_eq!(
            model_at(5),
            "opus",
            "past the top clamps at the strongest rung"
        );
    }

    #[test]
    fn minimal_agent_with_no_persona_yields_the_prompt_the_helper_tools_and_the_configuration() {
        // No persona (empty system_prompt) omits --system-prompt entirely, so a bare
        // agent's args are exactly the task prompt, the helpers' pre-approved MCP tools
        // and the spawn configuration every spawn carries.
        let args = args_for(
            &AgentDef {
                id: "bare".into(),
                ..Default::default()
            },
            "task",
            "",
            0,
        );
        let mut expected = vec![
            "-p".to_string(),
            "task".to_string(),
            "--allowed-tools".to_string(),
            crate::hooks::helper_mcp_tools().unwrap().join(","),
        ];
        expected.extend(spawn_config_args("rigger", &["mcp"], "").unwrap());
        assert_eq!(args, expected);
    }

    /// The JSON value that follows `flag` in `args`, panicking when it is absent or not JSON.
    fn flag_json(args: &[String], flag: &str) -> Value {
        serde_json::from_str(&args[arg_index(args, flag) + 1])
            .unwrap_or_else(|e| panic!("{flag} is not JSON ({e}): {args:?}"))
    }

    #[test]
    fn build_args_hands_the_spawn_the_operators_mcp_server_settings_and_helpers() {
        // A unit worktree carries no `.mcp.json` or `.claude/settings.json`, so the worker
        // gets them on its command line: the operator's read-only `rigger mcp` server alone,
        // the three session settings and the two fan-out helpers.
        let a = AgentDef {
            id: "impl".into(),
            tools: vec!["Read".into(), "Agent".into()],
            recurse: true,
            ..Default::default()
        };
        let args = build_args(
            &a,
            "task",
            &persona_opts("persona", 0),
            "/custom/bin/rigger",
        )
        .unwrap();
        let mcp = flag_json(&args, "--mcp-config");
        let servers = mcp["mcpServers"].as_object().unwrap();
        assert_eq!(servers.len(), 1, "exactly one server: {mcp}");
        assert_eq!(servers["rigger"]["command"], "/custom/bin/rigger");
        assert_eq!(servers["rigger"]["args"], serde_json::json!(["mcp"]));
        assert!(args.iter().any(|x| x == "--strict-mcp-config"));
        let settings = &args[arg_index(&args, "--settings") + 1];
        assert!(
            crate::hooks::carries_session_settings(settings.as_bytes()),
            "the spawn's settings carry the prime hook, the grep-guard and the status line: {settings}"
        );
        let agents = flag_json(&args, "--agents");
        assert!(agents.get("lookup").is_some(), "{agents}");
        assert!(agents.get("verify").is_some(), "{agents}");
        assert_eq!(args[arg_index(&args, "--permission-mode") + 1], "default");
        assert_eq!(args[arg_index(&args, "--permission-prompts") + 1], "none");
    }

    #[test]
    fn build_args_pre_approves_the_helpers_mcp_tools_once() {
        // Under `--permission-prompts none` a tool nobody pre-approved is denied, so the
        // `lookup` helper's graph calls must be on the allow list; a persona that already
        // declares one does not get it twice.
        let a = AgentDef {
            id: "impl".into(),
            tools: vec!["Read".into(), "mcp__rigger__rigger_graph".into()],
            ..Default::default()
        };
        let args = args_for(&a, "task", "", 0);
        assert_eq!(
            args[arg_index(&args, "--allowed-tools") + 1],
            "Read,mcp__rigger__rigger_graph,mcp__rigger__rigger_ground,mcp__rigger__rigger_peers"
        );
    }

    #[test]
    fn build_args_orders_task_persona_model_tools_permissions_then_configuration() {
        let a = AgentDef {
            id: "impl".into(),
            model: "opus".into(),
            tools: vec!["Read".into()],
            ..Default::default()
        };
        let args = args_for(&a, "task", "persona", 0);
        let order: Vec<usize> = [
            "-p",
            "--system-prompt",
            "--model",
            "--allowed-tools",
            "--permission-mode",
            "--permission-prompts",
            "--mcp-config",
            "--strict-mcp-config",
            "--settings",
            "--agents",
        ]
        .iter()
        .map(|flag| arg_index(&args, flag))
        .collect();
        assert!(
            order.windows(2).all(|w| w[0] < w[1]),
            "the flags follow the documented order: {args:?}"
        );
    }

    #[test]
    fn build_args_carries_the_spawns_own_settings_under_the_session_settings() {
        let opts = SpawnOpts {
            settings_json: "{\"model\":\"opus\"}".to_string(),
            ..Default::default()
        };
        let args = build_args(&AgentDef::default(), "task", &opts, "rigger").unwrap();
        let settings = flag_json(&args, "--settings");
        assert_eq!(
            settings["model"], "opus",
            "the spawn's own settings survive"
        );
        assert!(crate::hooks::carries_session_settings(
            settings.to_string().as_bytes()
        ));
    }

    #[test]
    fn build_args_fails_when_the_spawns_settings_are_not_a_json_object() {
        let opts = SpawnOpts {
            settings_json: "[1]".to_string(),
            ..Default::default()
        };
        let err = build_args(&AgentDef::default(), "task", &opts, "rigger").unwrap_err();
        assert!(err.0.contains("settings"), "{}", err.0);
    }
}
