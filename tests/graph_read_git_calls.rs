//! A graph read spawns git only to compare the caller's worktree with the head it answers from,
//! in every build that serves the read: the CLI's reads and the MCP graph tool alike.

use crate::common;
use common::cli::{rigger_command, run_rigger_envs, run_rigger_ok, temp_project};
use common::fixtures::{git_commit_all, git_out, write_text};
use common::mcp::McpSession;
use serde_json::json;

/// GIVEN a committed, built project with one file edited since, WHEN `rigger graph` answers
/// `--around` the file, `--show` its function and `--lookup` the function's name, and the MCP
/// graph tool answers `around` and `show` of them, each read traced by git itself (`GIT_TRACE`),
/// THEN each read's one open runs exactly two git commands: the worktree diff against the head's
/// tree and the untracked listing. A build with no code store has no snapshot to take the head
/// from, so it also asks git for the head's branch, the object it names, and that object's commit
/// and tree in one query.
#[test]
fn one_open_runs_exactly_the_diff_and_the_untracked_listing() {
    let dir = temp_project();
    let root = dir.path();
    write_text(root, "src/lib.rs", "pub fn alpha() {}\n");
    write_text(root, "src/other.rs", "pub fn beta() {}\n");
    git_commit_all(root, "source");
    run_rigger_ok(root, &["graph", "build"]);
    write_text(root, "src/other.rs", "pub fn gamma() {}\n");
    let traces = tempfile::tempdir().unwrap();
    let head = git_out(root, &["rev-parse", "HEAD"]);
    let mut expected = vec!["diff --raw".to_string(), "ls-files --others".into()];
    if !cfg!(feature = "symbols") {
        expected.extend([
            format!("rev-parse '{head}^{{commit}}'"),
            "rev-parse --verify".into(),
            "symbolic-ref --short".into(),
        ]);
    }
    // (read, the arguments)
    let cases: [(&str, &[&str]); 3] = [
        (
            "around",
            &["graph", "--around", "src/lib.rs", "--depth", "1"],
        ),
        ("show", &["graph", "--show", "src/lib.rs::alpha"]),
        ("lookup", &["graph", "--lookup", "alpha"]),
    ];
    for (case, args) in cases {
        let trace = traces.path().join(case);
        let (_, err, ok) = run_rigger_envs(root, args, &[("GIT_TRACE", trace.to_str().unwrap())]);
        assert!(ok, "{case}: the read answers; stderr:\n{err}");
        let traced = std::fs::read_to_string(&trace).unwrap_or_default();
        assert_eq!(
            git_commands(&traced),
            expected,
            "{case}: one open runs the diff and the untracked listing alone; trace:\n{traced}"
        );
    }

    // The MCP graph tool: each call is a read of its own over the one session.
    let trace = traces.path().join("mcp");
    let mut mcp = McpSession::from_command(rigger_command(
        root,
        &["mcp"],
        &[("GIT_TRACE", trace.to_str().unwrap())],
        root,
    ));
    mcp.initialize();
    // (read, the tool's arguments)
    let calls = [
        ("mcp around", json!({"around": "src/lib.rs", "depth": 1})),
        ("mcp show", json!({"show": "src/lib.rs::alpha"})),
    ];
    for (case, arguments) in calls {
        let before = std::fs::read_to_string(&trace).unwrap_or_default().len();
        let answer = mcp.graph_tool(arguments);
        let traced = std::fs::read_to_string(&trace).unwrap_or_default()[before..].to_string();
        assert!(!answer.is_null(), "{case}: the tool answers");
        assert_eq!(
            git_commands(&traced),
            expected,
            "{case}: one open runs the diff and the untracked listing alone; trace:\n{traced}"
        );
    }
    mcp.finish_cleanly();
}

/// The git commands a `GIT_TRACE` log names, sorted: each git process names its command once, as
/// a built-in or as the program it runs, by its first two words.
fn git_commands(traced: &str) -> Vec<String> {
    let mut commands: Vec<String> = (traced.lines())
        .filter_map(|l| {
            let (_, rest) = l
                .split_once("trace: built-in: git ")
                .or_else(|| l.split_once("trace: exec: "))?;
            Some(rest.split(' ').take(2).collect::<Vec<_>>().join(" "))
        })
        .collect();
    commands.sort();
    commands
}
