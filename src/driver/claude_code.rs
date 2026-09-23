//! The native Claude Code agent host (spec 104: rigger hosts its agents as headless
//! Claude Code sessions; `docs/architecture-addendum-claude-code-integration.md` §4).
//! Replaces the blocking `cli` driver's `Command::output()` (one shot, learn nothing
//! until exit, infer the rest) with a typed, streaming launch: every fact about an agent,
//! its session, its model, its tools, its permissions, travels as an explicit argv or
//! stream field, never inferred from a prompt sentence or parsed stdout.
//!
//! This file lands incrementally, criterion by criterion (spec 104's own "one host, ship
//! it whole" intent still holds - the criteria are a delivery split, not a design split):
//! criterion 1 (THE LAUNCH IS TYPED, this module's current content) owns argv, cwd,
//! environment and the open half of the launch record. THE STREAM (criterion 2) reads
//! what this criterion starts and completes `impl AgentDriver for Driver`; until it
//! lands, `Driver` here is not yet a conforming `AgentDriver` and `rigger run`
//! (`src/main.rs`) keeps using `cli::Driver`.

use std::io::Write;
use std::process::{Child, Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::conductor::{Error, SpawnOpts};
use crate::config::AgentDef;
use crate::eventstore::EventStore;
use crate::progress::SpawnLaunched;
use crate::progress_store;

/// The permission mode this host always passes (architecture addendum §4.1): paired with
/// `--permission-prompts none`, whatever would otherwise prompt is DENIED and reported on
/// the stream, rather than silently blocking on a prompt nobody can answer. Not
/// configurable per agent - every persona runs unattended through the same path (§2,
/// invariant 6: one host).
const PERMISSION_MODE: &str = "default";

/// Spawns agents as headless Claude Code sessions.
pub struct Driver {
    /// The `claude` binary to run. Empty resolves to `"claude"` on `$PATH`, same
    /// fallback convention as [`crate::driver::cli::Driver`].
    pub bin: String,
    /// How this host invokes ITSELF as the spawn's bound MCP server
    /// (`<rigger_bin> mcp --spawn <id>`, §4.3). Empty resolves to `"rigger"` on `$PATH` -
    /// a test points this at a fixture binary instead of a real `rigger` install.
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

/// A started launch: the live child (stdin still open - THE STREAM, criterion 2, writes
/// any later operator note and closes it after the first `result`), the session id this
/// host minted for it, and the exact argv it ran with (kept for the caller's own
/// diagnostics/tests; not re-derivable from `Child`).
pub struct Launch {
    pub child: Child,
    pub session_id: String,
    pub args: Vec<String>,
}

impl Driver {
    /// THE LAUNCH (spec 104 criterion 1): start one child process for this spawn launch.
    ///
    /// Ordering is the criterion: `SpawnLaunched` is appended to the progress store
    /// BEFORE the child is started (so a launch that never even started a process because
    /// `bin` does not exist still cannot be re-attempted silently - the record proves the
    /// attempt), and it is the ONLY event this call writes - closing it belongs to a
    /// later criterion. The environment the child sees is the operator's ambient
    /// environment (inherited unchanged, credential included) plus `opts.env`; this
    /// function neither reads nor sets a credential variable of its own.
    pub fn launch(
        &self,
        agent: &AgentDef,
        task: &str,
        opts: &SpawnOpts,
        store: &dyn EventStore,
    ) -> Result<Launch, Error> {
        let session_id = uuid::Uuid::new_v4().to_string();
        let args = build_args(agent, opts, &session_id, self.rigger_bin());

        let started = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        progress_store::record_launch(
            store,
            &opts.run_id,
            &SpawnLaunched {
                spawn: opts.id.clone(),
                launch: opts.launch,
                session_id: session_id.clone(),
                resumed_from: if opts.resumed_from.is_empty() {
                    None
                } else {
                    Some(opts.resumed_from.clone())
                },
                started,
            },
        )?;

        let bin = self.bin();
        let mut cmd = Command::new(bin);
        cmd.args(&args);
        if !opts.dir.is_empty() {
            cmd.current_dir(&opts.dir);
        }
        // The ONE build-environment authority's injection site for this driver (spec 65),
        // exactly like the cli driver applies it: every var the resolver derived, on top
        // of the inherited ambient environment (`Command` never clears it) - so the
        // operator's own credential rides through untouched and unread.
        for (k, v) in &opts.env {
            cmd.env(k, v);
        }
        cmd.stdin(Stdio::piped());
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());
        let mut child = cmd
            .spawn()
            .map_err(|e| Error(format!("claude_code driver: launch {:?}: {e}", opts.id)))?;

        // The task is the first user message on the input stream (§4.1); stdin stays
        // open afterward (§4.2's "the host closes the input after the first `result`" is
        // the reader's call, not this one's).
        if let Some(stdin) = child.stdin.as_mut() {
            let msg = first_user_message(task);
            writeln!(stdin, "{msg}").map_err(|e| {
                Error(format!(
                    "claude_code driver: write task to {:?}: {e}",
                    opts.id
                ))
            })?;
        }

        Ok(Launch {
            child,
            session_id,
            args,
        })
    }

    fn bin(&self) -> &str {
        if self.bin.is_empty() {
            "claude"
        } else {
            &self.bin
        }
    }

    fn rigger_bin(&self) -> &str {
        if self.rigger_bin.is_empty() {
            "rigger"
        } else {
            &self.rigger_bin
        }
    }
}

/// The typed stream-json first input message: `{"type":"user","message":{"role":"user",
/// "content":task}}`, one compact line (stream-json is line-delimited).
fn first_user_message(task: &str) -> String {
    serde_json::json!({
        "type": "user",
        "message": { "role": "user", "content": task },
    })
    .to_string()
}

/// The `--mcp-config` value naming exactly one server, this spawn's own bound MCP server
/// (§4.3): `<rigger_bin> mcp --spawn <spawn_id>`. Passed as a JSON string, never a file
/// (§4.1).
fn mcp_config_json(spawn_id: &str, rigger_bin: &str) -> String {
    serde_json::json!({
        "mcpServers": {
            "rigger": {
                "command": rigger_bin,
                "args": ["mcp", "--spawn", spawn_id],
            },
        },
    })
    .to_string()
}

/// Build the typed `claude` headless invocation (architecture addendum §4.1 table): the
/// ONE argv authority for this driver, exactly as `cli::build_args` is for the cli driver
/// - every field below is a fact, never inferred at read time.
///
/// `--json-schema` (verdict personas, §4.5) is deliberately out of this criterion's scope
/// (the Done-when text names session id, stream-json, persona, model, tools, permission
/// flags, MCP config and settings only) and is not built here; a later criterion adds it
/// alongside whichever caller knows a persona is a verdict role, through this same
/// function rather than a second argv authority.
pub fn build_args(
    agent: &AgentDef,
    opts: &SpawnOpts,
    session_id: &str,
    rigger_bin: &str,
) -> Vec<String> {
    let mut args = vec![
        "-p".to_string(),
        "--output-format".to_string(),
        "stream-json".to_string(),
        "--input-format".to_string(),
        "stream-json".to_string(),
        "--verbose".to_string(),
        "--session-id".to_string(),
        session_id.to_string(),
    ];
    if !opts.system_prompt.is_empty() {
        args.push("--system-prompt".to_string());
        args.push(opts.system_prompt.clone());
    }
    let model = agent.model_for_attempt(opts.attempt);
    if !model.is_empty() {
        args.push("--model".to_string());
        args.push(model);
    }
    let tools = agent.allowed_tools();
    if !tools.is_empty() {
        args.push("--allowed-tools".to_string());
        args.push(tools.join(","));
    }
    args.push("--permission-mode".to_string());
    args.push(PERMISSION_MODE.to_string());
    args.push("--permission-prompts".to_string());
    args.push("none".to_string());
    args.push("--mcp-config".to_string());
    args.push(mcp_config_json(&opts.id, rigger_bin));
    args.push("--strict-mcp-config".to_string());
    if !opts.settings_json.is_empty() {
        args.push("--settings".to_string());
        args.push(opts.settings_json.clone());
    }
    args
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eventstore::sqlite::Store;
    use crate::eventstore::{Direction, SilentStore};
    use std::io::Read;

    fn opts(id: &str) -> SpawnOpts {
        SpawnOpts {
            id: id.to_string(),
            attempt: 0,
            system_prompt: "You implement findings.".to_string(),
            ..Default::default()
        }
    }

    // ---- build_args: pure, no process ----

    #[test]
    fn build_args_types_the_full_launch() {
        let a = AgentDef {
            id: "impl".into(),
            model: "sonnet".into(),
            tools: vec!["Read".into(), "Bash".into()],
            ..Default::default()
        };
        let mut o = opts("u1/implementer#0");
        o.settings_json = "{\"hooks\":{}}".to_string();
        let args = build_args(&a, &o, "sess-123", "rigger");

        assert_eq!(args[0], "-p");
        let get_val = |flag: &str| -> String {
            let i = args
                .iter()
                .position(|x| x == flag)
                .unwrap_or_else(|| panic!("missing flag {flag}: {args:?}"));
            args[i + 1].clone()
        };
        assert_eq!(get_val("--output-format"), "stream-json");
        assert_eq!(get_val("--input-format"), "stream-json");
        assert!(args.iter().any(|x| x == "--verbose"));
        assert_eq!(get_val("--session-id"), "sess-123");
        assert_eq!(get_val("--system-prompt"), "You implement findings.");
        assert_eq!(get_val("--model"), "sonnet");
        assert_eq!(get_val("--allowed-tools"), "Read,Bash");
        assert_eq!(get_val("--permission-mode"), "default");
        assert_eq!(get_val("--permission-prompts"), "none");
        assert!(args.iter().any(|x| x == "--strict-mcp-config"));
        assert_eq!(get_val("--settings"), "{\"hooks\":{}}");
    }

    #[test]
    fn build_args_mcp_config_names_the_spawn_bound_server() {
        let a = AgentDef::default();
        let o = opts("u7-launch/implementer#2");
        let args = build_args(&a, &o, "sess", "rigger");
        let i = args.iter().position(|x| x == "--mcp-config").unwrap();
        let cfg: serde_json::Value = serde_json::from_str(&args[i + 1]).unwrap();
        assert_eq!(cfg["mcpServers"]["rigger"]["command"], "rigger");
        assert_eq!(
            cfg["mcpServers"]["rigger"]["args"],
            serde_json::json!(["mcp", "--spawn", "u7-launch/implementer#2"])
        );
        // Exactly one server named.
        assert_eq!(cfg["mcpServers"].as_object().unwrap().len(), 1);
    }

    #[test]
    fn build_args_mcp_config_uses_the_configured_rigger_bin() {
        let a = AgentDef::default();
        let o = opts("u/implementer#0");
        let args = build_args(&a, &o, "sess", "/custom/path/rigger");
        let i = args.iter().position(|x| x == "--mcp-config").unwrap();
        let cfg: serde_json::Value = serde_json::from_str(&args[i + 1]).unwrap();
        assert_eq!(
            cfg["mcpServers"]["rigger"]["command"],
            "/custom/path/rigger"
        );
    }

    #[test]
    fn build_args_omits_empty_optional_flags() {
        let a = AgentDef::default();
        let o = opts("u/implementer#0"); // settings_json left empty by opts()
        let mut bare = o;
        bare.system_prompt = String::new();
        let args = build_args(&a, &bare, "sess", "rigger");
        assert!(!args.iter().any(|x| x == "--system-prompt"));
        assert!(!args.iter().any(|x| x == "--model"));
        assert!(!args.iter().any(|x| x == "--allowed-tools"));
        assert!(!args.iter().any(|x| x == "--settings"));
        // The always-on flags are still present.
        assert!(args.iter().any(|x| x == "--strict-mcp-config"));
        assert!(args.iter().any(|x| x == "--session-id"));
    }

    // ---- launch(): real subprocess via the checked-in fixture ----

    fn fixture_bin() -> String {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/claude-code-echo-agent.sh")
            .to_string_lossy()
            .into_owned()
    }

    fn read_fixture_lines(child: &mut Child) -> Vec<String> {
        child.wait().expect("fixture agent exits");
        let mut out = String::new();
        child
            .stdout
            .take()
            .expect("piped stdout")
            .read_to_string(&mut out)
            .unwrap();
        out.lines().map(str::to_string).collect()
    }

    #[test]
    fn launch_records_before_the_child_starts_even_when_spawn_fails() {
        // spec 104 criterion 1: the ordering IS the criterion. Point `bin` at a path
        // that cannot possibly exist, so `Command::spawn` fails - and prove the
        // SpawnLaunched record was still written, because it happens before the spawn
        // attempt, not after a successful one.
        let driver = Driver {
            bin: "/definitely/does/not/exist/claude-xyz".to_string(),
            rigger_bin: "rigger".to_string(),
        };
        let store = Store::open(":memory:").unwrap();
        let mut o = opts("u104-launch/implementer#0");
        o.run_id = "run-1".to_string();

        let err = match driver.launch(&AgentDef::default(), "do the thing", &o, &store) {
            Ok(_) => panic!("spawning a nonexistent binary must fail"),
            Err(e) => e,
        };
        assert!(err.0.contains("u104-launch/implementer#0"), "{}", err.0);

        let recorded = store
            .read_stream(crate::progress::STREAM, 0, Direction::Forward)
            .unwrap();
        assert_eq!(
            recorded.len(),
            1,
            "the launch record lands even though the process never started"
        );
        assert_eq!(recorded[0].type_, crate::progress::TYPE_SPAWN_LAUNCHED);
        let sl: SpawnLaunched = serde_json::from_slice(&recorded[0].data).unwrap();
        assert_eq!(sl.spawn, "u104-launch/implementer#0");
        assert!(!sl.session_id.is_empty(), "a session id was still minted");
    }

    #[test]
    fn launch_records_a_lost_write_loudly_rather_than_spawning_blind() {
        // The store write itself failing (e.g. down) must also stop the launch before
        // any process exists - never silently spawn against an unrecorded launch.
        let driver = Driver::default();
        let o = opts("u/implementer#0");
        let err = match driver.launch(&AgentDef::default(), "task", &o, &SilentStore) {
            Ok(_) => panic!("a launch nobody can record must not proceed"),
            Err(e) => e,
        };
        assert!(
            err.0.to_lowercase().contains("nothing") || err.0.contains("u/implementer#0"),
            "{}",
            err.0
        );
    }

    #[test]
    fn launch_starts_the_child_in_the_spawns_dir_with_a_minted_session_id() {
        let dir = std::env::temp_dir();
        let driver = Driver {
            bin: fixture_bin(),
            rigger_bin: "rigger".to_string(),
        };
        let store = Store::open(":memory:").unwrap();
        let mut o = opts("u/implementer#0");
        o.dir = dir.to_string_lossy().into_owned();

        let mut launch = driver
            .launch(&AgentDef::default(), "the task", &o, &store)
            .unwrap();

        // A real UUID v4 was minted and threaded onto argv as --session-id.
        assert_eq!(
            launch
                .args
                .iter()
                .position(|x| x == "--session-id")
                .map(|i| &launch.args[i + 1]),
            Some(&launch.session_id)
        );
        assert_eq!(
            launch.session_id.len(),
            36,
            "session id: {}",
            launch.session_id
        );

        let lines = read_fixture_lines(&mut launch.child);
        let cwd = std::fs::canonicalize(&lines[0]).unwrap();
        let want = std::fs::canonicalize(&dir).unwrap();
        assert_eq!(cwd, want, "child ran in the spawn's dir");
    }

    #[test]
    #[serial_test::serial(claude_code_ambient_env)]
    fn launch_applies_opts_env_and_inherits_the_ambient_credential_untouched() {
        // spec 104 criterion 1: "the environment is the operator's plus opts.env; the
        // host sets and reads no credential variable" - proven two ways in one test: a
        // pre-existing ambient var (standing in for the operator's credential) reaches
        // the child UNCHANGED with no entry in opts.env for it, and an opts.env pair
        // reaches the child too. `#[serial]` (docs/architecture-addendum, Cargo.toml's
        // own comment on `serial_test`): this test mutates the process-global
        // environment, a resource `cargo test`'s default parallelism must not interleave.
        std::env::set_var("ANTHROPIC_API_KEY", "operators-own-credential");
        let driver = Driver {
            bin: fixture_bin(),
            rigger_bin: "rigger".to_string(),
        };
        let store = Store::open(":memory:").unwrap();
        let mut o = opts("u/implementer#0");
        o.env = vec![("RIGGER_TEST_VAR".to_string(), "from-opts-env".to_string())];
        assert!(
            !o.env.iter().any(|(k, _)| k == "ANTHROPIC_API_KEY"),
            "the credential is never IN opts.env - it must ride the inherited environment"
        );

        let mut launch = driver
            .launch(&AgentDef::default(), "task", &o, &store)
            .unwrap();
        let lines = read_fixture_lines(&mut launch.child);
        assert!(
            lines.contains(&"ANTHROPIC_API_KEY=operators-own-credential".to_string()),
            "lines: {lines:?}"
        );
        assert!(
            lines.contains(&"RIGGER_TEST_VAR=from-opts-env".to_string()),
            "lines: {lines:?}"
        );
        std::env::remove_var("ANTHROPIC_API_KEY");
    }

    #[test]
    fn launch_writes_the_task_as_the_first_stream_json_user_message() {
        let driver = Driver {
            bin: fixture_bin(),
            rigger_bin: "rigger".to_string(),
        };
        let store = Store::open(":memory:").unwrap();
        let o = opts("u/implementer#0");

        let mut launch = driver
            .launch(&AgentDef::default(), "implement the thing", &o, &store)
            .unwrap();
        let lines = read_fixture_lines(&mut launch.child);
        let stdin_line = lines
            .iter()
            .find(|l| l.starts_with("STDIN="))
            .expect("fixture echoed the first stdin line");
        let json_text = stdin_line.trim_start_matches("STDIN=");
        let v: serde_json::Value = serde_json::from_str(json_text).unwrap();
        assert_eq!(v["type"], "user");
        assert_eq!(v["message"]["role"], "user");
        assert_eq!(v["message"]["content"], "implement the thing");
    }

    #[test]
    fn launch_leaves_stdin_open_for_the_reader_to_close() {
        // THE STREAM (criterion 2) closes input after the first `result`, not this
        // criterion - so `launch` must hand back a child whose stdin handle is still
        // present (not dropped/closed) after writing the first message.
        let driver = Driver {
            bin: fixture_bin(),
            rigger_bin: "rigger".to_string(),
        };
        let store = Store::open(":memory:").unwrap();
        let o = opts("u/implementer#0");
        let launch = driver
            .launch(&AgentDef::default(), "task", &o, &store)
            .unwrap();
        assert!(
            launch.child.stdin.is_some(),
            "stdin must stay open after the first message"
        );
    }

    #[test]
    fn a_launch_relaunches_at_a_new_ordinal_with_no_resumed_from_this_spec() {
        // CONSTRAINTS WALK: "Relaunch - a new session id and the next launch ordinal."
        // Every launch spec 104 itself performs is fresh (resume is spec 105's), so two
        // successive launches for the same spawn get two DIFFERENT session ids even when
        // the caller advances only `launch`.
        let driver = Driver {
            bin: fixture_bin(),
            rigger_bin: "rigger".to_string(),
        };
        let store = Store::open(":memory:").unwrap();
        let mut o = opts("u/implementer#0");
        o.launch = 0;
        let mut first = driver
            .launch(&AgentDef::default(), "t", &o, &store)
            .unwrap();
        first.child.wait().unwrap();

        o.launch = 1;
        let mut second = driver
            .launch(&AgentDef::default(), "t", &o, &store)
            .unwrap();
        second.child.wait().unwrap();

        assert_ne!(first.session_id, second.session_id);
        let recorded = store
            .read_stream(crate::progress::STREAM, 0, Direction::Forward)
            .unwrap();
        assert_eq!(recorded.len(), 2);
        let launches: Vec<u32> = recorded
            .iter()
            .map(|e| {
                serde_json::from_slice::<SpawnLaunched>(&e.data)
                    .unwrap()
                    .launch
            })
            .collect();
        assert_eq!(launches, vec![0, 1]);
    }
}
