//! Fixtures that read this repository's own checked-in files: sources, docs, manifests (as text,
//! or with the lock through cargo's resolver) and the committed audit ledgers.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

pub use super::fixtures::table_lines;
use super::fixtures::{recorded_argv, recorded_task};

/// The repository root (the package's manifest directory).
pub fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// `cargo tree --prefix none` with `args`, run from [`repo_root`] so the resolver reads this
/// repository's committed manifests and lock whatever the process CWD. The cargo binary is the
/// one that built this suite - `env!("CARGO")` is set by cargo at build time and always points
/// at a real cargo - so the suite needs nothing on `PATH`. `--prefix none` prints bare
/// `name vX.Y.Z` lines instead of tree glyphs, the one rendering that is stable across cargo
/// versions: PR #27's CI ran a newer stable whose glyph rendering broke a glyph-stripping parse.
pub fn cargo_tree(args: &[&str]) -> Output {
    let cargo_bin = env!("CARGO");
    Command::new(cargo_bin)
        .current_dir(repo_root())
        .args(["tree", "--prefix", "none"])
        .args(args)
        .output()
        .unwrap_or_else(|e| panic!("failed to spawn `{cargo_bin} tree {args:?}`: {e}"))
}

/// The committed simplification audit report, relative to [`repo_root`].
pub const AUDIT_REPORT_PATH: &str = "docs/audit/2026-09-simplification-audit.md";

/// The committed duplication catalog, relative to [`repo_root`].
pub const DUPLICATION_CATALOG_PATH: &str = "docs/audit/duplication-catalog.json";

/// The text of the checked-in file at `rel` (relative to [`repo_root`]).
pub fn repo_text(rel: &str) -> String {
    super::fixtures::read_text(&repo_root().join(rel))
}

/// A PATH that finds `tool` in a directory under `work`, as the checked-in `tests/fixtures`
/// script `stub` when one is given, ahead of the ambient PATH; with no stub the directory is
/// empty and the whole PATH.
pub fn stub_path(work: &Path, tool: &str, stub: Option<&str>) -> String {
    let bin = work.join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    let Some(stub) = stub else {
        return bin.display().to_string();
    };
    std::os::unix::fs::symlink(
        repo_root().join("tests/fixtures").join(stub),
        bin.join(tool),
    )
    .unwrap();
    format!(
        "{}:{}",
        bin.display(),
        std::env::var("PATH").unwrap_or_default()
    )
}

/// The checked-in session silent 7 s, past the host's unbounded wake-up interval, then ending in a
/// result.
pub const SILENT_PAST_THE_POLL: &str = "claude-code-silent-past-the-poll-then-result-agent.sh";

/// The checked-in stand-in for the critic's `claude`, which [`write_critique_stub`] puts first on
/// PATH.
const CRITIQUE_STUB: &str = "claude-code-critique-agent.sh";

/// The files the critique stub reads and writes beside its PATH symlink (`<work>/bin`): the
/// transcript it replays on its first spawn, the one it replays on every later spawn, its spawn
/// count, and its first spawn's stdin line, argv, the system prompt its `--system-prompt-file`
/// named and the `KURRENTDB_CONN` it was started with.
const CRITIQUE_TRANSCRIPT: &str = "critique-transcript.jsonl";
const CRITIQUE_LATER_TRANSCRIPT: &str = "critique-later-transcript.jsonl";
const CRITIQUE_SPAWNS: &str = "critique-spawns";
const CRITIQUE_TASK: &str = "critique-task.jsonl";
const CRITIQUE_ARGV: &str = "critique-argv";
/// The critique stub's record of the system prompt it was started with: the content of the file
/// its `--system-prompt-file` named, copied at spawn time (the host removes that file once the
/// spawn ends). Read with [`critique_stub_record`].
pub const CRITIQUE_SYSTEM_PROMPT: &str = "critique-system-prompt";
/// The critique stub's record of the `KURRENTDB_CONN` it was started with, `unset` when its
/// environment carried none. Read with [`critique_stub_record`].
pub const CRITIQUE_CONN: &str = "critique-conn";

/// A PATH that runs the checked-in critique stub as `claude`, ahead of the ambient PATH, set on
/// the one `rigger critique` command that should reach it. On its first spawn the stub replays a
/// stream-json session whose `result` is `critique`: an init that reports the spawn's `rigger`
/// server connected, then the result line, `critique` JSON-encoded so its finding lines and
/// verdict line survive. On every later spawn it replays the same init and a result answering
/// [`CLEAN_CRITIQUE`](super::fixtures::CLEAN_CRITIQUE), a zero-line approve, so a call whose first pass
/// does not exhaust the critique stops at its next attempt (spec 123, THE SHARED STUB). Only data
/// is written here, beside the stub's symlink in `<work>/bin`; the executable is the checked-in
/// fixture. Each call takes a fresh `work` directory.
pub fn write_critique_stub(work: &Path, critique: &str) -> String {
    write_critique_stub_then(work, critique, super::fixtures::CLEAN_CRITIQUE)
}

/// [`write_critique_stub`] whose later spawns answer `later` in place of the zero-line approve,
/// so a test can stand in for a critic whose later passes keep adding finding lines.
pub fn write_critique_stub_then(work: &Path, critique: &str, later: &str) -> String {
    write_critique_session(work, "connected", critique, later)
}

/// [`write_critique_stub`] whose replayed init reports the spawn's `rigger` server with
/// `rigger_server` as its status, so a test can stand in for a launch whose server did not
/// connect (any status but `connected`).
pub fn write_critique_stub_reporting(work: &Path, critique: &str, rigger_server: &str) -> String {
    write_critique_session(
        work,
        rigger_server,
        critique,
        super::fixtures::CLEAN_CRITIQUE,
    )
}

/// The critique stub's PATH under `work`, its first spawn answering `first` and every later one
/// answering `later`, each replayed init reporting the spawn's `rigger` server as `rigger_server`.
fn write_critique_session(work: &Path, rigger_server: &str, first: &str, later: &str) -> String {
    let path = stub_path(work, "claude", Some(CRITIQUE_STUB));
    let session = "77777777-7777-4777-8777-777777777777";
    let init = serde_json::json!({
        "type": "system",
        "subtype": "init",
        "session_id": session,
        "tools": ["Read", "Glob"],
        "mcp_servers": [{"name": "rigger", "status": rigger_server}],
        "model": "claude-opus-stub",
        "permissionMode": "default",
        "apiKeySource": "none",
    });
    let result = |answer: &str| {
        serde_json::json!({
            "type": "result",
            "subtype": "success",
            "is_error": false,
            "num_turns": 1,
            "result": answer,
            "session_id": session,
            "total_cost_usd": 0.0,
            "usage": {"input_tokens": 1, "output_tokens": 1},
            "permission_denials": [],
        })
    };
    for (transcript, answer) in [
        (CRITIQUE_TRANSCRIPT, first),
        (CRITIQUE_LATER_TRANSCRIPT, later),
    ] {
        std::fs::write(
            work.join("bin").join(transcript),
            format!("{init}\n{}\n", result(answer)),
        )
        .unwrap();
    }
    path
}

/// How many times the critique stub under `work` was spawned.
pub fn critique_stub_spawns(work: &Path) -> usize {
    std::fs::read_to_string(work.join("bin").join(CRITIQUE_SPAWNS))
        .map_or(0, |count| count.lines().count())
}

/// The critique stub's hold, which stops each spawn before it replays while it stands, and the
/// record a spawn writes once it is held.
const CRITIQUE_HOLD: &str = "critique-hold";
const CRITIQUE_HELD: &str = "critique-held";

/// Hold the critique stub under `work`, then on a thread run `act` once a spawn is held and
/// release it: `act` runs while that pass is in flight. Join the handle after the command ends.
pub fn hold_critique_stub(
    work: &Path,
    act: impl FnOnce() + Send + 'static,
) -> std::thread::JoinHandle<()> {
    let bin = work.join("bin");
    std::fs::write(bin.join(CRITIQUE_HOLD), "").unwrap();
    std::thread::spawn(move || {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
        while !bin.join(CRITIQUE_HELD).exists() {
            assert!(
                std::time::Instant::now() < deadline,
                "no critique spawn was held"
            );
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        act();
        std::fs::remove_file(bin.join(CRITIQUE_HOLD)).unwrap();
    })
}

/// The words the shell a hook runs under reads out of the hook `command`: the argv the command
/// starts, whatever quoting spells it.
pub fn shell_words(command: &str) -> Vec<String> {
    let dir = tempfile::tempdir().unwrap();
    let recorded = dir.path().join("words");
    let out = std::process::Command::new("sh")
        .arg("-c")
        .arg(format!("printf '%s\\0' {command} > \"$0\""))
        .arg(&recorded)
        .output()
        .unwrap();
    assert!(out.status.success(), "the shell reads {command:?}: {out:?}");
    recorded_argv(&recorded)
}

/// Each hook `settings` carries under `event`, in order: its matcher and the [`shell_words`] of
/// its one command.
pub fn hook_words(settings: &serde_json::Value, event: &str) -> Vec<(String, Vec<String>)> {
    settings["hooks"][event]
        .as_array()
        .unwrap_or_else(|| panic!("no {event} hooks in {settings}"))
        .iter()
        .map(|entry| {
            (
                entry["matcher"].as_str().unwrap().to_string(),
                shell_words(entry["hooks"][0]["command"].as_str().unwrap()),
            )
        })
        .collect()
}

/// The session's grep guard as [`hook_words`] reads it: the tools it matches and the words of the
/// command it runs.
pub fn grep_guard_hook() -> (String, Vec<String>) {
    let words = ["rigger", "grep-guard"].map(str::to_string);
    ("Grep|Bash".to_string(), words.to_vec())
}

/// A launch's `settings` carry its per-spawn hooks: under `PreToolUse` the write guard running
/// `bin` with `roots` in order, then the session's grep guard exactly when `grep_guard` (the
/// workflow's `defaults.grep_guard`) declares it, and under `StopFailure` one entry per category
/// running `bin` for `spawn`, in category order.
pub fn assert_carries_spawn_hooks(
    settings: &serde_json::Value,
    bin: &str,
    spawn: &str,
    roots: &[String],
    grep_guard: bool,
) {
    let words = |text: &[&str]| -> Vec<String> { text.iter().map(|w| w.to_string()).collect() };
    let mut guard = words(&[bin, "guard-write"]);
    for root in roots {
        guard.extend(words(&["--root", root]));
    }
    let mut pre_tool_use = vec![("Edit|MultiEdit|Write|NotebookEdit".to_string(), guard)];
    if grep_guard {
        pre_tool_use.push(grep_guard_hook());
    }
    assert_eq!(
        hook_words(settings, "PreToolUse"),
        pre_tool_use,
        "the write guard, then the session's grep guard when declared ({grep_guard}): {settings}"
    );
    let stop_failures: Vec<(String, Vec<String>)> = rigger::conductor::AgentFailure::CATEGORIES
        .iter()
        .map(|category| {
            let class = category.as_str();
            let command = [
                bin,
                "hook",
                "stop-failure",
                "--spawn",
                spawn,
                "--class",
                class,
            ];
            (class.to_string(), words(&command))
        })
        .collect();
    assert_eq!(
        hook_words(settings, "StopFailure"),
        stop_failures,
        "one entry per category: {settings}"
    );
}

/// The `--agents` value a launch carries over a checkout whose `.claude/agents/` holds exactly
/// `copies`, each one of the [`super::fixtures::OPERATOR_HELPER_COPIES`]: a copied helper is
/// defined by its [`super::fixtures::operator_helpers_agents_json`] entry, every other one by its
/// embedded text, this repository's own committed `.claude/agents/` helper.
pub fn handed_helpers_agents_json(copies: &[(&str, &str)]) -> serde_json::Value {
    let operator = super::fixtures::operator_helpers_agents_json();
    let mut agents = serde_json::Map::new();
    for (file, embedded) in rigger::hooks::HELPER_AGENTS {
        let name = file.trim_end_matches(".md");
        let def = if copies.iter().any(|(copied, _)| copied == file) {
            operator[name].clone()
        } else {
            let copy = rigger_domain::agent::HelperCopy {
                name: file.to_string(),
                text: embedded.to_string(),
                present: false,
            };
            rigger::hooks::helper_agents_json(&[copy]).unwrap()[name].clone()
        };
        agents.insert(name.to_string(), def);
    }
    serde_json::Value::Object(agents)
}

/// The value `argv` carries right after `flag`.
pub fn flag_value<'a>(argv: &'a [String], flag: &str) -> &'a str {
    let at = argv
        .iter()
        .position(|a| a == flag)
        .unwrap_or_else(|| panic!("the argv carries {flag}: {argv:?}"));
    &argv[at + 1]
}

/// The task the critique stub under `work` was handed on its first spawn: the `content` of the
/// host's first stream-json user message. A later spawn's task is its `SpawnRequested` `prompt`
/// on the critique stream.
pub fn critique_stub_task(work: &Path) -> String {
    recorded_task(&work.join("bin").join(CRITIQUE_TASK))
}

/// What the critique stub under `work` recorded of its first start under `record`
/// ([`CRITIQUE_CONN`] or [`CRITIQUE_SYSTEM_PROMPT`]).
pub fn critique_stub_record(work: &Path, record: &str) -> String {
    std::fs::read_to_string(work.join("bin").join(record)).unwrap()
}

/// The argv the critique stub under `work` was started with on its first spawn.
pub fn critique_stub_argv(work: &Path) -> Vec<String> {
    recorded_argv(&work.join("bin").join(CRITIQUE_ARGV))
}

/// The committed JSON file at `rel`, decoded as the documented `contract` a downstream
/// consumer relies on - failing loudly, naming both, when it does not decode.
pub fn committed_json<T: serde::de::DeserializeOwned>(rel: &str, contract: &str) -> T {
    serde_json::from_str(&repo_text(rel))
        .unwrap_or_else(|e| panic!("{rel} does not deserialize as the documented {contract}: {e}"))
}

/// The committed JSON ledger `rel`, decoded as `contract`, holds at least one entry - a
/// downstream consumer pinning counts against it never silently sees nothing.
pub fn assert_committed_ledger_is_nonempty<T: serde::de::DeserializeOwned>(
    rel: &str,
    contract: &str,
) {
    let entries: Vec<T> = committed_json(rel, contract);
    assert!(
        !entries.is_empty(),
        "{rel} deserialized to zero entries - a downstream consumer pinning counts against \
         this file would silently see nothing"
    );
}

/// Decoding the committed JSON file `rel` as `contract` and re-encoding it reproduces the
/// committed bytes exactly - a downstream consumer decoding and re-encoding it never silently
/// diverges from the committed artifact.
pub fn assert_committed_json_round_trips<T>(rel: &str, contract: &str)
where
    T: serde::de::DeserializeOwned + serde::Serialize,
{
    let decoded: T = committed_json(rel, contract);
    let mut reencoded = serde_json::to_string_pretty(&decoded).expect("the contract re-serializes");
    reencoded.push('\n');
    assert_eq!(
        repo_text(rel),
        reencoded,
        "{rel} does not round-trip byte-for-byte through the documented {contract}"
    );
}

/// The committed text file `rel`, compared lowercase, carries none of `phrasings`; the failure
/// names what `rel` must describe instead (`why`) and every phrasing still present.
pub fn assert_doc_carries_none_of(rel: &str, phrasings: &[&str], why: &str) {
    let text = repo_text(rel).to_lowercase();
    let present: Vec<&str> = phrasings
        .iter()
        .copied()
        .filter(|phrasing| text.contains(phrasing))
        .collect();
    assert!(
        present.is_empty(),
        "{rel} {why}. Phrasings still present in the document: {present:#?}"
    );
}

/// Whether the TOML table `[header]` in `manifest` declares `key` (as `key = ..`, `key=..` or a
/// dotted `key.sub = ..`).
pub fn table_declares_key(manifest: &str, header: &str, key: &str) -> bool {
    table_lines(manifest, header).iter().any(|line| {
        let t = line.trim();
        t == key
            || t.starts_with(&format!("{key} "))
            || t.starts_with(&format!("{key}="))
            || t.starts_with(&format!("{key}."))
    })
}

/// Every production `.rs` file under `root`'s `src/` and each `crates/*/src/`, as
/// `(repo-relative path, text)`.
pub fn production_sources(root: &Path) -> Vec<(String, String)> {
    let mut files = Vec::new();
    collect_rs_files(&root.join("src"), &mut files);
    collect_rs_files(&root.join("crates"), &mut files);
    files
        .into_iter()
        .map(|p| {
            let rel = p
                .strip_prefix(root)
                .unwrap_or(&p)
                .to_string_lossy()
                .replace('\\', "/");
            (rel, p)
        })
        .filter(|(rel, _)| rel.starts_with("src/") || rel.split('/').nth(2) == Some("src"))
        .map(|(rel, p)| {
            let text = super::fixtures::read_text(&p);
            (rel, text)
        })
        .collect()
}

/// Every `.rs` file strictly under `dir`, recursively, appended to `out` in sorted (deterministic)
/// order regardless of readdir order; an unreadable `dir` contributes nothing.
pub fn collect_rs_files(dir: &Path, out: &mut Vec<PathBuf>) {
    collect_files_with_extension(dir, "rs", out);
}

/// Every file with extension `ext` strictly under `dir`, recursively, appended to `out` in
/// sorted (deterministic) order regardless of readdir order; an unreadable `dir` contributes
/// nothing.
pub fn collect_files_with_extension(dir: &Path, ext: &str, out: &mut Vec<PathBuf>) {
    let mut files = Vec::new();
    collect_files(dir, &mut files);
    out.extend(
        files
            .into_iter()
            .filter(|path| path.extension().and_then(|e| e.to_str()) == Some(ext)),
    );
}

/// Every file strictly under `dir`, of any extension, recursively, appended to `out` in sorted
/// (deterministic) order regardless of readdir order; an unreadable `dir` contributes nothing.
pub fn collect_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<_> = entries.flatten().map(|e| e.path()).collect();
    entries.sort();
    for path in entries {
        if path.is_dir() {
            collect_files(&path, out);
        } else {
            out.push(path);
        }
    }
}

/// Call `visit` with the path and text of every `.rs` file under `dir`, recursively.
pub fn for_each_rs_file(dir: &Path, visit: &mut dyn FnMut(&Path, &str)) {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(e) => panic!("cannot read dir {}: {e}", dir.display()),
    };
    for entry in entries {
        let path = entry.expect("dir entry must be readable").path();
        if path.is_dir() {
            for_each_rs_file(&path, visit);
        } else if path.extension().is_some_and(|x| x == "rs") {
            let text = super::fixtures::read_text(&path);
            visit(&path, &text);
        }
    }
}

/// The production source of the CLI composition root (`src/main.rs` and its `src/cli/` command
/// modules), each file with its trailing `#[cfg(test)] mod tests { ... }` unit-test module
/// stripped: a rule that governs SHIPPING code
/// must not count the test code that legitimately opens throwaway stores and projections
/// directly. Falls back to the whole source when the marker is absent, so a future reshaping
/// never makes a scan pass by silently scanning nothing.
pub fn production_main_rs() -> String {
    [
        "src/main.rs",
        "src/cli/mod.rs",
        "src/cli/dashboard.rs",
        "src/cli/eval.rs",
        "src/cli/graph.rs",
        "src/cli/guard.rs",
        "src/cli/hygiene.rs",
        "src/cli/observe.rs",
        "src/cli/run.rs",
        "src/cli/setup.rs",
        "src/cli/validate.rs",
    ]
    .iter()
    .map(|rel| production_part(&repo_text(rel)).to_string())
    .collect::<Vec<_>>()
    .join("\n")
}

/// `src` up to its trailing `#[cfg(test)]\nmod tests {` unit-test module (this tree keeps the
/// module last in every file), or the whole of `src` when it has none.
pub fn production_part(src: &str) -> &str {
    src.find("#[cfg(test)]\nmod tests {")
        .map_or(src, |cut| &src[..cut])
}

/// The `(name, needle)` rows of `table` whose needle `text` does not contain, each rendered as
/// `{name}  ({label}: {needle:?})`.
pub fn missing_rows(text: &str, table: &[(&str, &str)], label: &str) -> Vec<String> {
    table
        .iter()
        .filter(|(_, needle)| !text.contains(needle))
        .map(|(name, needle)| format!("{name}  ({label}: {needle:?})"))
        .collect()
}

/// Run the REAL `workerLabel(req)` - extracted verbatim from the shipped `workflows/rigger.js`
/// along with the `PERSONA_VERB`/`ROSTER_VERB` tables and the `bareRoleOf`/`personaOf`/
/// `firstSentence`/`roleAttempt` helpers it calls - under a real `node` subprocess, for a wave
/// item `id` titled `title`, stamped with `req.unit` and `req.reviews` when given (omitted
/// entirely otherwise, as an ordinary build unit's wave item). Returns the rendered label, or
/// `None` when `node` is not on PATH (missing node is an environment fact, never a test failure).
pub fn worker_label(
    id: &str,
    title: &str,
    unit: Option<&str>,
    reviews: Option<&[&str]>,
) -> Option<String> {
    let src = repo_text("workflows/rigger.js");
    let verb_table = super::fixtures::js_declaration(&src, "const PERSONA_VERB = {");
    let roster_table = super::fixtures::js_declaration(&src, "const ROSTER_VERB = {");
    let bare_role_of = super::fixtures::js_declaration(&src, "function bareRoleOf(role) {");
    let persona_of = super::fixtures::js_declaration(&src, "function personaOf(role) {");
    let first_sentence = super::fixtures::js_declaration(&src, "function firstSentence(s) {");
    let role_attempt = super::fixtures::js_declaration(&src, "function roleAttempt(id) {");
    let worker_label = super::fixtures::js_declaration(&src, "function workerLabel(req) {");

    let mut req = serde_json::json!({ "id": id, "title": title });
    if let Some(unit) = unit {
        req["unit"] = serde_json::Value::String(unit.to_string());
    }
    if let Some(reviews) = reviews {
        req["reviews"] = serde_json::Value::Array(
            reviews
                .iter()
                .map(|s| serde_json::Value::String(s.to_string()))
                .collect(),
        );
    }
    let req = req.to_string();
    let script = format!(
        "{verb_table}\n{roster_table}\n{bare_role_of}\n{persona_of}\n{first_sentence}\n\
         {role_attempt}\n{worker_label}\n\
         process.stdout.write(workerLabel(JSON.parse(process.argv[2])) + '\\n')\n"
    );

    let node = std::env::var("RIGGER_NODE").unwrap_or_else(|_| "node".to_string());
    let mut f = tempfile::NamedTempFile::new().unwrap();
    std::io::Write::write_all(&mut f, script.as_bytes()).unwrap();

    match std::process::Command::new(&node)
        .arg(f.path())
        .arg(&req)
        .output()
    {
        Ok(out) => {
            assert!(
                out.status.success(),
                "the real workerLabel must run without throwing on {req}: {}",
                String::from_utf8_lossy(&out.stderr)
            );
            let text = String::from_utf8_lossy(&out.stdout).into_owned();
            Some(text.trim_end_matches('\n').to_string())
        }
        Err(e) => {
            assert!(
                e.kind() == std::io::ErrorKind::NotFound,
                "node failed for a reason other than being absent: {e}"
            );
            None
        }
    }
}
