//! Integration test for `rigger graph --show <entity>` (spec 58, criterion 1: the show
//! surface's RESOLUTION and OUTPUT). It drives the COMPILED `rigger` binary against a throwaway
//! project whose `graph.db` is seeded with code-entity nodes - folded from the ALWAYS-compiled
//! `CodeEntityExtracted` events, so the test runs identically in BOTH feature lanes (default and
//! `--no-default-features`) - and whose working tree holds the real source the body is read from.
//!
//! It proves the three faces of criterion 1 through the real CLI path (`main::cmd_graph` ->
//! `cmd_graph_show` -> `Projector::locate` -> body read from the working tree):
//!   - SHOW BY UNIQUE NAME: a bare name with one definition prints its site + line-numbered body.
//!   - SHOW BY ID: the full `<file>::<name>` id resolves to the SAME entity and body.
//!   - AMBIGUOUS NAME: a bare name with several definitions LISTS them (sorted, with files) and
//!     prints NO body - the call-views honesty rule (never guess among candidates).

use std::path::Path;
use std::process::Command;

use rigger::contextgraph::sqlite::Projector;
use rigger::contextgraph::{Projection, TYPE_CODE_ENTITY_EXTRACTED};
use rigger::eventstore::Event;

/// The compiled `rigger` binary under test (Cargo sets this for integration tests).
fn rigger_bin() -> &'static str {
    env!("CARGO_BIN_EXE_rigger")
}

/// A throwaway project dir that is its own git repo, so `project_identity()` (which scopes the
/// namespaced streams and the graph project) is stable across the seed and the binary's reads.
fn temp_project() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let _ = Command::new("git")
        .args(["init", "-q"])
        .current_dir(dir.path())
        .status();
    dir
}

/// The project identity the binary resolves for `root`, mirrored here so the seeded `graph.db`
/// lands under the exact project scope the compiled binary reads back: the git top-level basename
/// (no tracked `.rigger/project.id` is seeded here), else `root`'s own basename.
fn run_stream_identity(root: &Path) -> String {
    let toplevel = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["rev-parse", "--show-toplevel"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .filter(|s| !s.is_empty());
    let base = toplevel.as_deref().map(Path::new).unwrap_or(root);
    base.file_name()
        .and_then(|n| n.to_str())
        .filter(|s| !s.is_empty())
        .map(String::from)
        .unwrap_or_else(|| "rigger".to_string())
}

/// Create the `.rigger/` dir under `root` so `Projector::open` can lay `graph.db` beside it.
fn seed_rigger_dir(root: &Path) {
    std::fs::create_dir_all(root.join(".rigger")).unwrap();
}

/// Run `rigger <args...>` in `cwd` and return (stdout, stderr, success). Opts out of the
/// auto-started dashboard and points the instance registry at a throwaway state dir, exactly as
/// the other CLI integration tests do, so a short-lived inspector invocation spawns nothing that
/// outlives the test.
fn run_rigger(cwd: &Path, args: &[&str]) -> (String, String, bool) {
    let state = tempfile::tempdir().expect("temp XDG_STATE_HOME");
    let out = Command::new(rigger_bin())
        .args(args)
        .current_dir(cwd)
        .env("RIGGER_NO_DASH", "1")
        .env("XDG_STATE_HOME", state.path())
        .output()
        .expect("failed to spawn the rigger binary");
    (
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
        out.status.success(),
    )
}

/// Seed one code-entity DEFINITION node into the persisted `graph.db` by folding a
/// `CodeEntityExtracted` event (the ALWAYS-compiled fold), exactly as a real extraction pass
/// would - so this seeding is feature-lane independent (no `symbols` extractor required).
fn seed_def(p: &Projector, pos: u64, file: &str, name: &str, kind: &str, line: u32) {
    let payload = format!(
        r#"{{"file":"{file}","name":"{name}","kind":"{kind}","line":{line},"lang":"rust"}}"#
    );
    let mut e = Event::new(TYPE_CODE_ENTITY_EXTRACTED, payload.into_bytes());
    e.position = pos;
    p.apply(&e).unwrap();
}

#[test]
fn graph_show_resolves_by_id_and_name_and_lists_ambiguous_candidates() {
    let dir = temp_project();
    let root = dir.path();
    seed_rigger_dir(root);

    // Working tree the body is read from. `a.rs` defines `alpha` (line 1) then `shared` (line 3,
    // with a distinctive body token); `b.rs` defines a SECOND `shared` (line 1, its own token).
    std::fs::write(
        root.join("a.rs"),
        "fn alpha() {}\n\
         \n\
         fn shared() {\n\
         \x20\x20\x20\x20let marker_in_a = 1;\n\
         }\n",
    )
    .unwrap();
    std::fs::write(
        root.join("b.rs"),
        "fn shared() {\n\
         \x20\x20\x20\x20let marker_in_b = 2;\n\
         }\n",
    )
    .unwrap();

    // Seed the graph: alpha@a.rs:1, shared@a.rs:3, shared@b.rs:1. Distinct positions so each
    // fold is a distinct (non-deduped) apply.
    let id = run_stream_identity(root);
    {
        let p =
            Projector::open(root.join(".rigger").join("graph.db").to_str().unwrap(), &id).unwrap();
        seed_def(&p, 1, "a.rs", "alpha", "function", 1);
        seed_def(&p, 2, "a.rs", "shared", "function", 3);
        seed_def(&p, 3, "b.rs", "shared", "function", 1);
    }

    // (1) SHOW BY UNIQUE NAME: `alpha` has one definition -> its site + kind + degree + a
    //     line-numbered body.
    let (out, err, ok) = run_rigger(root, &["graph", "--show", "alpha"]);
    assert!(ok, "graph --show alpha must succeed; stderr: {err}");
    assert!(
        out.contains("a.rs:1"),
        "prints the definition SITE (file:line); got:\n{out}"
    );
    assert!(
        out.contains("function"),
        "prints the entity KIND; got:\n{out}"
    );
    assert!(
        out.contains("degree"),
        "prints the entity's one-hop DEGREE; got:\n{out}"
    );
    assert!(
        out.lines()
            .any(|l| l.contains("fn alpha") && l.contains('1')),
        "the definition BODY is shown, line-numbered with its 1-based line; got:\n{out}"
    );

    // (2) SHOW BY ID: the full `<file>::<name>` id resolves to the SAME entity, and its body is
    //     read from the working tree at the recorded line - a.rs's `shared`, never b.rs's.
    let (out_id, err, ok) = run_rigger(root, &["graph", "--show", "a.rs::shared"]);
    assert!(ok, "graph --show a.rs::shared must succeed; stderr: {err}");
    assert!(
        out_id.contains("a.rs:3"),
        "the full id resolves to shared@a.rs:3; got:\n{out_id}"
    );
    assert!(
        out_id.contains("marker_in_a"),
        "the body read from the working tree at a.rs:3 is shown; got:\n{out_id}"
    );
    assert!(
        !out_id.contains("marker_in_b"),
        "only a.rs's `shared` body is shown, never b.rs's; got:\n{out_id}"
    );
    assert!(
        out_id
            .lines()
            .any(|l| l.contains("fn shared") && l.contains('3')),
        "the body is line-numbered from its recorded line (3); got:\n{out_id}"
    );

    // (3) AMBIGUOUS NAME: `shared` has two definitions -> LIST both (sorted by id, each with its
    //     file) and print NO body (never guess among candidates).
    let (out_amb, err, ok) = run_rigger(root, &["graph", "--show", "shared"]);
    assert!(ok, "graph --show shared must succeed; stderr: {err}");
    assert!(
        out_amb.contains("a.rs::shared") && out_amb.contains("b.rs::shared"),
        "both candidates are listed by id; got:\n{out_amb}"
    );
    let ia = out_amb
        .find("a.rs::shared")
        .expect("a.rs::shared candidate present");
    let ib = out_amb
        .find("b.rs::shared")
        .expect("b.rs::shared candidate present");
    assert!(
        ia < ib,
        "candidates are listed SORTED by id (a.rs before b.rs); got:\n{out_amb}"
    );
    assert!(
        !out_amb.contains("marker_in_a") && !out_amb.contains("marker_in_b"),
        "an ambiguous name prints NO body (the honesty rule); got:\n{out_amb}"
    );
}
