//! Periphery tests for the combined lookup through the shipped binary: `rigger graph --lookup`
//! and `rigger_graph`'s `lookup` over `rigger mcp`, each answering from a graph `rigger graph
//! build` folded and from the caller's own working tree as it stands.

use std::path::Path;

use crate::common;
use common::cli::run_rigger;
use common::git::{commit_files, linked_worktree};
use common::mcp::McpSession;

/// The host's limit on one tool result's text, in characters.
const HOST_LIMIT: usize = 50_000;

/// A git repository at `root` holding `files`, committed, and its graph built from them.
fn built(root: &Path, files: &[(&str, &str)]) {
    let _ = common::git::run_git(root, &["init", "-q"]);
    for args in [
        ["config", "user.email", "t@t"],
        ["config", "user.name", "t"],
    ] {
        let _ = common::git::run_git(root, &args);
    }
    std::fs::create_dir_all(root.join(".rigger")).unwrap();
    commit_files(root, files, "fixture");
    let (out, err, ok) = run_rigger(root, &["graph", "build"]);
    assert!(ok, "graph build: {err}\n{out}");
}

/// `rigger graph --lookup <terms>...` in `cwd`: its stdout, the command required to succeed.
fn cli_lookup(cwd: &Path, terms: &[&str]) -> String {
    let args: Vec<&str> = ["graph", "--lookup"].iter().chain(terms).copied().collect();
    let (out, err, ok) = run_rigger(cwd, &args);
    assert!(ok, "graph --lookup {terms:?}: {err}\n{out}");
    out
}

/// The text of `rigger_graph`'s `lookup` of `terms` over `mcp`.
fn mcp_lookup(mcp: &mut McpSession, terms: &[&str]) -> String {
    let reply = mcp.tool_call("rigger_graph", serde_json::json!({ "lookup": terms }));
    reply["result"]["content"][0]["text"]
        .as_str()
        .unwrap_or_else(|| panic!("a text reply: {reply}"))
        .to_string()
}

/// The body of the `## <name>` section of `reply`, up to the next section.
fn section<'a>(reply: &'a str, name: &str) -> &'a str {
    let head = format!("## {name}\n");
    let start = reply
        .find(&head)
        .unwrap_or_else(|| panic!("a {name} section:\n{reply}"))
        + head.len();
    let rest = &reply[start..];
    &rest[..rest.find("\n## ").unwrap_or(rest.len())]
}

const KEPT: &str = "pub fn widget_kept() {}\npub fn kept_caller() { widget_kept(); }\n";
const CHANGED: &str = "pub fn widget_changed() {}\n";
const GONE: &str = "pub fn widget_gone() {}\n";
const BODY: &str = "pub fn widget_body() {}\n";

/// A repository whose graph was built, then whose tree moved: one file changed so the graph would
/// record it differently, one changed in a way the graph records nothing of, one deleted and one
/// added since.
fn diverged() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    built(
        root,
        &[
            ("src/kept.rs", KEPT),
            ("src/changed.rs", CHANGED),
            ("src/gone.rs", GONE),
            ("src/body.rs", BODY),
            ("docs/widgets.md", "# Widgets\n\nThe widget_kept rule.\n"),
        ],
    );
    std::fs::write(
        root.join("src/changed.rs"),
        "// moved\npub fn widget_changed() {}\n",
    )
    .unwrap();
    std::fs::write(
        root.join("src/body.rs"),
        "pub fn widget_body() { let now = 1; }\n",
    )
    .unwrap();
    std::fs::remove_file(root.join("src/gone.rs")).unwrap();
    std::fs::write(root.join("src/added.rs"), "pub fn widget_added() {}\n").unwrap();
    dir
}

/// A file changed, deleted or added since the graph was built is answered by the text half from
/// the tree as it stands and withheld by the graph half, which names the file; a file the tree
/// holds as the graph ingested it, or changed in nothing the graph records of it (the generation
/// its bytes extract to is the graph's), is answered by both.
#[test]
fn a_file_changed_deleted_or_added_is_answered_by_text_and_withheld_by_graph() {
    let dir = diverged();
    let withheld = "changed in your tree; see the text results";
    let cases: [(&str, &[String], &[&str]); 5] = [
        (
            "widget_kept",
            &["src/kept.rs:1 src/kept.rs::widget_kept".to_string()],
            &["src/kept.rs:1: pub fn widget_kept() {}"],
        ),
        (
            "widget_changed",
            &[format!("{withheld}: src/changed.rs")],
            &["src/changed.rs:2: pub fn widget_changed() {}"],
        ),
        (
            "widget_body",
            &["src/body.rs:1 src/body.rs::widget_body".to_string()],
            &["src/body.rs:1: pub fn widget_body() { let now = 1; }"],
        ),
        (
            "widget_gone",
            &[format!("{withheld}: src/gone.rs")],
            &["no line of your tree holds widget_gone"],
        ),
        (
            "widget_added",
            &["no entity named widget_added".to_string()],
            &["src/added.rs:1: pub fn widget_added() {}"],
        ),
    ];
    for (term, graph_says, text_says) in cases {
        let reply = cli_lookup(dir.path(), &[term]);
        for line in graph_says {
            assert!(
                section(&reply, "graph").contains(line.as_str()),
                "{term}: the graph section says {line:?}:\n{reply}"
            );
        }
        for line in text_says {
            assert!(
                section(&reply, "text").contains(line),
                "{term}: the text section says {line:?}:\n{reply}"
            );
        }
    }
}

/// `rigger_graph`'s `lookup` and `rigger graph --lookup` answer the same body for the same terms
/// over the same tree.
#[test]
fn the_mcp_lookup_and_the_cli_lookup_answer_the_same_body() {
    let dir = diverged();
    let terms = ["widget_kept", "widget_added", "widgets"];
    let mut mcp = McpSession::start(dir.path());
    let served = mcp_lookup(&mut mcp, &terms);
    let _ = mcp.finish();
    let printed = cli_lookup(dir.path(), &terms);
    assert_eq!(served, printed);
    assert!(
        section(&printed, "prose").contains("- prose:doc docs/widgets.md:1 \"Widgets\""),
        "{printed}"
    );
}

/// A spawn's server, started in the spawn's worktree as its host starts it, answers the text half
/// from that worktree, and the operator's lookup in the owning checkout from the checkout.
#[test]
fn a_spawn_server_answers_the_text_half_from_its_own_worktree() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    built(root, &[("src/kept.rs", KEPT)]);
    let elsewhere = tempfile::tempdir().unwrap();
    let worktree = linked_worktree(root, elsewhere.path().join("wt"));
    std::fs::write(worktree.join("src/only_here.rs"), "fn worktree_only() {}\n").unwrap();

    let mut mcp = McpSession::start_with(&worktree, &["mcp", "--spawn", "u/implementer#0"]);
    let served = mcp_lookup(&mut mcp, &["worktree_only"]);
    let _ = mcp.finish();
    assert!(
        section(&served, "text").contains("src/only_here.rs:1: fn worktree_only() {}"),
        "{served}"
    );
    let operator = cli_lookup(root, &["worktree_only"]);
    assert!(
        section(&operator, "text").contains("no line of your tree holds worktree_only"),
        "{operator}"
    );
}

/// A graph the CLI cannot open is stated in the graph section, and the text and prose halves
/// still answer.
#[test]
fn a_graph_that_cannot_be_opened_still_answers_text_and_prose() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let _ = common::git::run_git(root, &["init", "-q"]);
    std::fs::create_dir_all(root.join(".rigger/graph.db")).unwrap();
    std::fs::write(root.join("a.rs"), "fn gizmo() {}\n").unwrap();
    std::fs::create_dir_all(root.join("docs")).unwrap();
    std::fs::write(root.join("docs/a.md"), "# Gizmo\nthe gizmo\n").unwrap();
    let reply = cli_lookup(root, &["gizmo"]);
    assert!(
        section(&reply, "graph").starts_with("not answered: the context graph could not be opened"),
        "{reply}"
    );
    assert!(
        section(&reply, "text").contains("a.rs:1: fn gizmo() {}"),
        "{reply}"
    );
    assert!(
        section(&reply, "prose").contains("prose:doc docs/a.md:1"),
        "{reply}"
    );
}

/// The terms of the worst case, and a tree where every one of them reaches more entities, lines
/// and prose sections than its reply can hold.
const TERMS: usize = 12;
const FILES: usize = 20;

fn worst_case_tree(root: &Path) -> Vec<String> {
    let terms: Vec<String> = (0..TERMS).map(|t| format!("lookup_term_{t:02}")).collect();
    let all = terms.join(" ");
    let padding = "x".repeat(300);
    let mut files = Vec::new();
    for f in 0..FILES {
        let mut code = String::new();
        for term in &terms {
            code.push_str(&format!(
                "pub fn {term}_in_{f:02}() {{ {term}_in_{next:02}(); }} // {all} {padding}\n",
                next = (f + 1) % FILES
            ));
        }
        let mut prose = String::new();
        for term in &terms {
            prose.push_str(&format!("## {term}\n\n{all} {padding}\n\n"));
        }
        files.push((format!("src/f{f:02}.rs"), code));
        files.push((format!("docs/d{f:02}.md"), prose));
    }
    let refs: Vec<(&str, &str)> = files
        .iter()
        .map(|(p, c)| (p.as_str(), c.as_str()))
        .collect();
    built(root, &refs);
    terms
}

/// The worst-case multi-term lookup through the real tool path stays under the host's limit on
/// one tool result, every section present and each stating what it cut.
#[test]
fn the_worst_case_multi_term_reply_stays_under_the_hosts_limit() {
    let dir = tempfile::tempdir().unwrap();
    let terms = worst_case_tree(dir.path());
    let refs: Vec<&str> = terms.iter().map(String::as_str).collect();
    let mut mcp = McpSession::start(dir.path());
    let reply = mcp_lookup(&mut mcp, &refs);
    let _ = mcp.finish();
    assert!(
        reply.chars().count() < HOST_LIMIT,
        "{} characters",
        reply.chars().count()
    );
    for name in ["graph", "text", "prose"] {
        assert!(
            section(&reply, name).contains("truncated:"),
            "the {name} section states its cut:\n{reply}"
        );
    }
}
