//! Spawn fixtures.

use rigger::eventstore::Event;
use rigger::spawn::{spawn_id, SpawnEvent, SpawnRequest, SpawnResult, TYPE_SPAWN_RESULT};

/// A minimal request for the crate's unit tests: the deterministic id derived from `unit` +
/// `role` + `attempt` (so it cannot drift from the labels), every optional field empty.
#[cfg(test)]
pub fn test_request(
    unit: &str,
    stage: &str,
    role: &str,
    attempt: u32,
    prompt: &str,
) -> SpawnRequest {
    SpawnRequest {
        id: spawn_id(unit, role, attempt),
        unit: unit.to_string(),
        stage: stage.to_string(),
        prompt: prompt.to_string(),
        ..SpawnRequest::default()
    }
}

/// The spawn host a test's spawns carry: the one test-side spelling of the struct the production
/// resolver `hooks::resolve_spawn_host` returns, with the grep-guard off, over a checkout holding
/// exactly the [`OPERATOR_HELPER_COPIES`], so a spawn that reads its helpers anywhere but the
/// host carries the embedded text instead of these copies, and a test pinning the copies goes
/// red. A test that needs the flag set spreads this one.
#[cfg(any(feature = "store", not(feature = "core")))]
pub fn test_spawn_host() -> rigger_domain::agent::SpawnHost {
    rigger_domain::agent::SpawnHost {
        grep_guard: false,
        helpers: OPERATOR_HELPER_COPIES
            .iter()
            .map(|(file, text)| rigger_domain::agent::HelperCopy {
                name: file.to_string(),
                text: text.to_string(),
                present: true,
            })
            .collect(),
    }
}

/// An operator's helper copy as its `(file, text)` under `.claude/agents/`.
pub type HelperFile<'a> = (&'a str, &'a str);

/// An operator's own copies of the two fan-out helpers: each differs from the embedded helper in
/// every field, and the `lookup` copy declares two of the three rigger tools the embedded one
/// does, in another order.
pub const OPERATOR_HELPER_COPIES: [HelperFile; 2] = [
    (
        "lookup.md",
        "---\nname: lookup\ndescription: The operator's own lookup helper.\nmodel: sonnet\n\
         tools: mcp__rigger__rigger_peers, mcp__rigger__rigger_graph, Read\n---\n\
         Answer one question about one node, the operator's way.\n",
    ),
    (
        "verify.md",
        "---\nname: verify\ndescription: The operator's own verify helper.\nmodel: haiku\n\
         tools: Read, Bash\n---\nRun one check, the operator's way.\n",
    ),
];

/// An operator's helper copy that does not parse: its frontmatter names no helper.
pub const NAMELESS_HELPER_COPY: &str = "---\nmodel: haiku\n---\nRun one check.\n";

/// The rigger tools [`OPERATOR_HELPER_COPIES`] declare, once each in helper then declaration
/// order, comma-joined as a launch's pre-approval lists them.
pub const OPERATOR_HELPER_TOOLS: &str = "mcp__rigger__rigger_peers,mcp__rigger__rigger_graph";

/// Write `text` as the operator's helper copy `file` under `root`'s `.claude/agents/`.
pub fn write_helper_copy(root: &std::path::Path, file: &str, text: impl AsRef<[u8]>) {
    let agents = root.join(".claude/agents");
    std::fs::create_dir_all(&agents).unwrap();
    std::fs::write(agents.join(file), text).unwrap();
}

/// Write each of `copies` as an operator's helper copy under `root` ([`write_helper_copy`]).
pub fn write_helper_copies(root: &std::path::Path, copies: &[HelperFile]) {
    for (file, text) in copies {
        write_helper_copy(root, file, text);
    }
}

/// The `--agents` value [`OPERATOR_HELPER_COPIES`] define: each helper keyed by its name, its
/// other frontmatter fields with `tools` as a list, and its body as the prompt.
pub fn operator_helpers_agents_json() -> serde_json::Value {
    serde_json::json!({
        "lookup": {
            "description": "The operator's own lookup helper.",
            "model": "sonnet",
            "tools": ["mcp__rigger__rigger_peers", "mcp__rigger__rigger_graph", "Read"],
            "prompt": "Answer one question about one node, the operator's way.\n"
        },
        "verify": {
            "description": "The operator's own verify helper.",
            "model": "haiku",
            "tools": ["Read", "Bash"],
            "prompt": "Run one check, the operator's way.\n"
        }
    })
}

/// One row per `SpawnResult` among `events`, in their order: its id, output and error, and
/// whether it is a liveness fault.
pub fn spawn_result_rows(events: &[Event]) -> Vec<(String, String, String, bool)> {
    events
        .iter()
        .filter(|e| e.type_ == TYPE_SPAWN_RESULT)
        .map(|e| {
            let result = SpawnResult::from_event(e).unwrap();
            let fault = result.is_liveness_fault();
            (result.id, result.output, result.error, fault)
        })
        .collect()
}

/// The absolute path of the checked-in fixture agent `script` under the workspace's
/// `tests/fixtures`, walking up from the manifest of the test crate compiling this file: the
/// root package, or a member crate below it.
pub fn fixture_script(script: &str) -> String {
    let relative = std::path::Path::new("tests/fixtures").join(script);
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .map(|dir| dir.join(&relative))
        .find(|path| path.exists())
        .unwrap_or_else(|| panic!("no checked-in fixture {script}"))
        .to_string_lossy()
        .into_owned()
}

/// The environment variable naming the directory the checked-in
/// `tests/fixtures/prompt-capture-agent.sh` copies what it was handed into.
pub const PROMPT_CAPTURE_DIR: &str = "PROMPT_CAPTURE_DIR";

/// The text of the file `file` the prompt-capture agent wrote into `capture`: `system-prompt`,
/// the text of the file its `--system-prompt-file` argument named; `stdin`, the headless host's
/// first stream-json line, or everything the cli host wrote.
pub fn captured(capture: &std::path::Path, file: &str) -> String {
    std::fs::read_to_string(capture.join(file)).unwrap()
}

/// The task a stand-in `claude` saved at `path`: the `content` of the host's first stream-json
/// user message.
pub fn recorded_task(path: &std::path::Path) -> String {
    first_message_task(&std::fs::read_to_string(path).unwrap())
}

/// The task the headless host's first stream-json user message `line` carries: its `content`.
pub fn first_message_task(line: &str) -> String {
    let message: serde_json::Value = serde_json::from_str(line.trim_end()).unwrap();
    message["message"]["content"]
        .as_str()
        .expect("the first user message carries its task as text")
        .to_string()
}

/// The frame the cli host closes spawn `id`'s task with: the driver's separator, then the line
/// naming the `rigger progress` command and its arguments only.
pub fn cli_progress_frame(id: &str) -> String {
    format!(
        "\n\n--- rigger driver ---\nLIVE PROGRESS: record a progress line by running (Bash): \
         rigger progress '{id}' '<one line: what you just did>'."
    )
}

/// The `rigger_progress` entry a spawn's own MCP server lists: what the tool records and its one
/// argument, with no word on when to report.
pub fn progress_tool_entry() -> serde_json::Value {
    serde_json::json!({
        "name": "rigger_progress",
        "description": "Record one live progress line for the spawn this server is bound to, so \
                        an observer sees you working between milestones.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "activity": {
                    "type": "string",
                    "description": "a short one-line description of what you just did"
                }
            },
            "required": ["activity"]
        }
    })
}

/// The `rigger_scratch` entry a spawn's own MCP server lists: the reaped scratch container every
/// probe repo, worktree, test build and build cache lives under, naming no toolchain's cache
/// variable, with no argument.
pub fn scratch_tool_entry() -> serde_json::Value {
    serde_json::json!({
        "name": "rigger_scratch",
        "description": "Your own rigger-assigned scratch container - the one \
                        directory every probe repo, verification worktree, test \
                        build, or build cache you create must live under, never \
                        /tmp or a session scratchpad. Reaped once your result \
                        records.",
        "inputSchema": {"type": "object", "properties": {}}
    })
}

/// Each text the MCP tool list `tools` advertises beside its stem under spec 121's THE NAMING
/// RULE, in document order: a tool's description under `<TOOL>`, then each `inputSchema` property
/// description under `<TOOL>_<ARG>`.
pub fn stemmed_texts(tools: &serde_json::Value) -> Vec<(String, &str)> {
    let mut texts = Vec::new();
    for tool in tools.as_array().unwrap() {
        let stem = tool["name"].as_str().unwrap().to_uppercase();
        texts.push((stem.clone(), tool["description"].as_str().unwrap()));
        for (arg, property) in tool["inputSchema"]["properties"].as_object().unwrap() {
            if let Some(text) = property["description"].as_str() {
                texts.push((format!("{stem}_{}", arg.to_uppercase()), text));
            }
        }
    }
    texts
}

/// The argv a stand-in `claude` saved at `path`, one NUL-terminated field per argument.
pub fn recorded_argv(path: &std::path::Path) -> Vec<String> {
    std::fs::read_to_string(path)
        .unwrap()
        .split_terminator('\0')
        .map(str::to_string)
        .collect()
}

/// One launch a stand-in `claude` put on record through `tests/fixtures/launch-args.sh`'s
/// `record_launch`.
pub struct StandInLaunch {
    /// The spawn the launch ran as.
    pub spawn: String,
    /// Its working directory, every link resolved.
    pub cwd: String,
    /// The arguments it was started with.
    pub argv: Vec<String>,
    /// The system prompt it was served, when its arm recorded it ([`RECORD_EACH_LAUNCH`]).
    pub system_prompt: Option<String>,
    /// Its first user message, the stream-json line carrying the task it was served
    /// ([`first_message_task`]), when its arm recorded it ([`RECORD_EACH_LAUNCH`]).
    pub first_message: Option<String>,
}

/// The opening of an arm of the `claude-code-arms-agent.sh` stand-in whose launches all stay on
/// record, through the stand-in's `record_launch`: the launch leaves its argv, working directory
/// and spawn in a directory of its own under `launches`, with the system prompt and first user
/// message it was served beside them, which [`stand_in_launches`] reads back.
pub const RECORD_EACH_LAUNCH: &str = r#"    record_launch "$here/launches" "$@"
    printf '%s' "$sp" > "$here/launches/$$/system-prompt"
    printf '%s' "$first" > "$here/launches/$$/first-message"
"#;

/// Each launch put on record under `dir`, in the order of its spawn.
pub fn stand_in_launches(dir: &std::path::Path) -> Vec<StandInLaunch> {
    let mut launches: Vec<StandInLaunch> = std::fs::read_dir(dir)
        .unwrap()
        .map(|launch| {
            let launch = launch.unwrap().path();
            let read = |name: &str| std::fs::read_to_string(launch.join(name)).unwrap();
            let served = |name: &str| std::fs::read_to_string(launch.join(name)).ok();
            StandInLaunch {
                spawn: read("spawn"),
                cwd: read("cwd").trim_end().to_string(),
                argv: recorded_argv(&launch.join("argv")),
                system_prompt: served("system-prompt"),
                first_message: served("first-message"),
            }
        })
        .collect();
    launches.sort_by(|a, b| a.spawn.cmp(&b.spawn));
    launches
}

/// The task the one launch of `spawn` recorded under `dir` was served, its first user message's
/// content ([`first_message_task`]): panics naming the spawns on record unless exactly one launch
/// ran as `spawn` and its arm recorded the message.
pub fn served_task(dir: &std::path::Path, spawn: &str) -> String {
    let launches = stand_in_launches(dir);
    let named: Vec<&str> = launches.iter().map(|l| l.spawn.as_str()).collect();
    match launches
        .iter()
        .filter(|l| l.spawn == spawn)
        .collect::<Vec<_>>()[..]
    {
        [StandInLaunch {
            first_message: Some(first),
            ..
        }] => first_message_task(first),
        _ => panic!("one launch of {spawn} recording its task; on record: {named:?}"),
    }
}

/// Write `dir/claude-arms`, the shell `case` over the system prompt `$sp` the checked-in
/// `claude-code-arms-agent.sh` stand-in sources beside its symlink in `dir`: `arms`, each
/// `*MARKER*) ... ;;`, then an arm failing loudly on any prompt they do not name.
pub fn write_claude_arms(dir: &std::path::Path, arms: &str) {
    std::fs::write(
        dir.join("claude-arms"),
        format!(
            r#"case "$sp" in
{arms}  *)
    echo "fake-claude: unrecognized system prompt: $sp" 1>&2
    exit 1
    ;;
esac
"#
        ),
    )
    .unwrap();
}

/// The arm of [`write_claude_arms`] for the launches naming no system prompt, the hold probe's:
/// the `claude auth status --json` read records its argv as `auth-argv` and runs `auth`, whose
/// output, on its stdout or straight to the host's stream, is the status it reports; the canary
/// records its argv as `canary-argv`, its working directory as `canary-cwd` and its first user
/// message as `canary-first`, then runs `canary`.
pub fn probe_arms(auth: &str, canary: &str) -> String {
    format!(
        r#"  "")
    if [ "$1" = auth ]; then
        printf '%s\0' "$@" > "$here/auth-argv"
{auth}    else
        printf '%s\0' "$@" > "$here/canary-argv"
        pwd -P > "$here/canary-cwd"
        printf '%s\n' "$first" > "$here/canary-first"
{canary}    fi
    ;;
"#
    )
}

/// A `claude auth status --json` report of a logged-in session, every key the installed `claude`
/// reports, each value a placeholder.
pub const LOGGED_IN_STATUS: &str = r#"{
  "loggedIn": true,
  "authMethod": "claude.ai",
  "apiProvider": "firstParty",
  "analyticsDisabled": false,
  "projectsDirectory": "/home/operator/.claude/projects",
  "configDirectory": "/home/operator/.claude",
  "email": "operator@example.com",
  "orgId": "00000000-0000-0000-0000-000000000000",
  "orgName": "Example Org",
  "subscriptionType": "max"
}"#;

/// A `claude auth status --json` report of a logged-out session.
pub const LOGGED_OUT_STATUS: &str = r#"{
  "loggedIn": false,
  "authMethod": "none",
  "apiProvider": "firstParty"
}"#;

/// The shell of a [`probe_arms`] body printing `text`, as it stands, straight to the host's
/// stream, so a body that then sleeps ([`sleeping`]) has answered already.
pub fn printing(text: &str) -> String {
    format!("    cat >&3 <<'TEXT'\n{text}\nTEXT\n")
}

/// The shell of a [`probe_arms`] body that sleeps `seconds` in the stand-in's own place,
/// answering nothing more and holding the host's stream open: it records its pid as a line of
/// `sleepers` beside the stand-in, then becomes the sleep, so the sleep is the very process the
/// host's deadline ends ([`assert_no_sleeper_left`]).
pub fn sleeping(seconds: u32) -> String {
    format!("    exec sh -c 'echo $$ >> \"$1\"; exec sleep {seconds}' sh \"$here/sleepers\"\n")
}

/// The stand-in `claude` in `dir` started exactly `count` sleeps ([`sleeping`]) and none is left
/// once the read that launched it has returned: each pid they recorded is gone from `/proc` within
/// 500 ms, read every 20 ms and never signalled. A host with no `/proc` fails the check rather
/// than reading every pid as gone.
pub fn assert_no_sleeper_left(dir: &std::path::Path, count: usize, name: &str) {
    assert!(
        std::path::Path::new("/proc/self").exists(),
        "{name}: the sleeper check reads /proc, which this host does not mount"
    );
    let recorded = std::fs::read_to_string(dir.join("sleepers")).unwrap_or_default();
    let pids: Vec<&str> = recorded.lines().collect();
    assert_eq!(
        pids.len(),
        count,
        "{name}: the sleeps the fake started: {pids:?}"
    );
    let alive = |pid: &&str| std::path::Path::new("/proc").join(pid).exists();
    let given_up = std::time::Instant::now() + std::time::Duration::from_millis(500);
    while pids.iter().any(alive) && std::time::Instant::now() < given_up {
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    let left: Vec<&str> = pids.into_iter().filter(alive).collect();
    assert_eq!(
        left,
        Vec::<&str>::new(),
        "{name}: the sleeps still running once the read returned"
    );
}

/// The shell of a canary that passes: it marks `canary-ran` and answers, the stand-in printing a
/// result with `is_error: false`.
pub const PASSING_CANARY: &str = "    : > \"$here/canary-ran\"\n";

/// The shell printing one `api_retry` line of each of `categories` to the host's stream, each
/// announcing a wait of `delay_ms`.
pub fn api_retry_lines(categories: &[&str], delay_ms: u32) -> String {
    categories
        .iter()
        .map(|category| {
            format!(
                "    printf '%s\\n' '{{\"type\":\"system\",\"subtype\":\"api_retry\",\"attempt\":1,\
                 \"max_retries\":10,\"retry_delay_ms\":{delay_ms},\"error_status\":500,\
                 \"error\":\"{category}\"}}' >&3\n"
            )
        })
        .collect()
}

/// The shell printing, to the host's stream, the `is_error` result a session ending on an API
/// error prints.
pub const ERROR_RESULT: &str = r#"    printf '%s\n' '{"type":"result","subtype":"success","is_error":true,"num_turns":1,"session_id":"arms","result":"the API failed"}' >&3
"#;

/// The shell of a canary that fails: it sights each of `categories` on its own `api_retry` line,
/// prints an `is_error` result, marks `canary-ran` and exits 1.
pub fn failing_canary(categories: &[&str]) -> String {
    format!(
        "{}{ERROR_RESULT}    : > \"$here/canary-ran\"\n    exit 1\n",
        api_retry_lines(categories, 1)
    )
}

/// The argv the hold probe's canary is launched with on `model`: the host's stream-json base, no
/// tools, no MCP server, no hooks, no session file, and `--model` only for a model it names.
pub fn canary_argv(model: &str) -> Vec<String> {
    let mut argv = vec![
        "-p",
        "--output-format",
        "stream-json",
        "--input-format",
        "stream-json",
        "--verbose",
        "--tools",
        "",
        "--strict-mcp-config",
        "--settings",
        r#"{"disableAllHooks":true}"#,
        "--no-session-persistence",
    ];
    if !model.is_empty() {
        argv.extend(["--model", model]);
    }
    argv.into_iter().map(str::to_string).collect()
}

/// A stand-in `claude` in `dir`: the checked-in `claude-code-arms-agent.sh` linked as
/// `dir/claude`, answering from `arms` ([`write_claude_arms`]). Its path.
#[cfg(unix)]
pub fn arms_claude(dir: &std::path::Path, arms: &str) -> String {
    let claude = dir.join("claude");
    std::os::unix::fs::symlink(fixture_script("claude-code-arms-agent.sh"), &claude).unwrap();
    write_claude_arms(dir, arms);
    claude.to_string_lossy().into_owned()
}
