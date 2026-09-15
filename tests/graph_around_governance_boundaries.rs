//! Periphery boundary tests for `rigger graph --around <file>` (spec 92, u92c6 follow-up).
//!
//! `graph_around_code_first.rs` proves the CORE contract: code prints before narrative, and a
//! mix of thirteen decisions/findings caps to the newest ten. This file independently
//! re-enumerates the SAME CLI output contract and locks in three boundary conditions that test
//! does not exercise:
//!
//! - a file with NO governing decisions/findings never prints the governing-section header or a
//!   "+N more" trailer at all - the `narrative_nodes.is_empty()` guard, never a zero-item section;
//! - exactly `AROUND_GOVERNANCE_CAP` (10) governing items print in full with NO trailing "+N
//!   more" line - the cap boundary itself, distinct from the already-covered over-the-cap case;
//! - a decision's GOVERNS edge / a finding's ABOUT edge - which `Projector::subgraph` DOES
//!   return, since both endpoints are reachable from the seed file - never appears in the printed
//!   edge block, only code-to-code edges do. This is the edge-level form of the u88c1 "decision
//!   spam" bug: pre-fix, `cmd_graph` printed every edge `subgraph` returned undifferentiated, so
//!   a GOVERNS/ABOUT line interleaved with the file's real structural edges. The test proves the
//!   edge is actually present upstream (so a passing assertion means the CLI filters it, not that
//!   it was never there to filter).

use std::path::Path;
use std::process::Command;

use rigger::contextgraph::sqlite::Projector;
use rigger::contextgraph::{Projection, TYPE_CODE_ENTITY_EXTRACTED};
use rigger::eventstore::Event;

// The compiled `rigger` binary under test is located at RUNTIME by the shared authority in
// `tests/common`: a path baked in at compile time goes stale the moment the target dir moves.
mod common;
use common::rigger_bin;

/// A throwaway project dir that is its own git repo, so `project_identity()` is stable across
/// the seed and the binary's reads.
fn temp_project() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let _ = Command::new("git")
        .args(["init", "-q"])
        .current_dir(dir.path())
        .status();
    dir
}

/// Seed an initialized `.rigger/events.db`, standing in for the store a prior `rigger run`/`step`
/// would have created - the store-opening couriers refuse to fabricate one from the wrong cwd.
fn seed_store(root: &Path) {
    let rigger = root.join(".rigger");
    std::fs::create_dir_all(&rigger).unwrap();
    std::fs::File::create(rigger.join("events.db")).unwrap();
}

/// The project identity the binary resolves for `root`, mirrored here so the seeded `graph.db`
/// lands under the exact project scope the compiled binary reads back.
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

/// Run `rigger <args...>` in `cwd`, opting out of the auto-started dashboard and pointing the
/// instance registry at a throwaway state dir, exactly as the other CLI integration tests do.
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
/// `CodeEntityExtracted` event directly (the ALWAYS-compiled fold), exactly as
/// `graph_show_surface.rs` / `graph_around_code_first.rs` seed - feature-lane independent.
fn seed_def(p: &Projector, pos: u64, file: &str, name: &str, kind: &str, line: u32) {
    let payload = format!(
        r#"{{"file":"{file}","name":"{name}","kind":"{kind}","line":{line},"lang":"rust"}}"#
    );
    let mut e = Event::new(TYPE_CODE_ENTITY_EXTRACTED, payload.into_bytes());
    e.position = pos;
    p.apply(&e).unwrap();
}

/// The bare ids from every `node <id> <kind>` line of a `rigger graph --around` transcript,
/// parsed exactly (never substring-matched).
fn node_ids(out: &str) -> Vec<String> {
    out.lines()
        .filter_map(|l| {
            l.trim_start()
                .strip_prefix("node ")
                .and_then(|rest| rest.split_whitespace().next())
                .map(str::to_string)
        })
        .collect()
}

/// Every printed `edge <from> -<rel>-> <to>` line, parsed into its (from, rel, to) triple -
/// never substring-matched, so an assertion about which edges printed is a claim about the
/// parsed fields, not a text search a coincidental substring could satisfy.
fn edge_lines(out: &str) -> Vec<(String, String, String)> {
    out.lines()
        .filter_map(|l| {
            let rest = l.trim_start().strip_prefix("edge ")?;
            let (from, rest) = rest.split_once(" -")?;
            let (rel, to) = rest.split_once("-> ")?;
            Some((
                from.trim().to_string(),
                rel.trim().to_string(),
                to.trim().to_string(),
            ))
        })
        .collect()
}

#[test]
fn around_omits_the_governing_section_entirely_when_the_file_has_no_decisions_or_findings() {
    let dir = temp_project();
    let root = dir.path();
    seed_store(root);
    let file = "lonely.rs";

    {
        let id = run_stream_identity(root);
        let p =
            Projector::open(root.join(".rigger").join("graph.db").to_str().unwrap(), &id).unwrap();
        seed_def(&p, 100_001, file, "solo", "function", 1);
    }

    let (out, err, ok) = run_rigger(root, &["graph", "--around", file, "--depth", "2"]);
    assert!(ok, "graph --around must succeed; stderr: {err}");

    assert!(
        node_ids(&out).contains(&"lonely.rs::solo".to_string()),
        "the code definition must still print with no governing items present; got:\n{out}"
    );
    assert!(
        !out.contains("governing decision"),
        "a file with zero decisions/findings must never print the governing-section header; \
         got:\n{out}"
    );
    assert!(
        !out.contains("more decision") && !out.contains("more finding"),
        "a file with zero decisions/findings must never print a '+N more' trailer; got:\n{out}"
    );
}

#[test]
fn around_shows_exactly_ten_with_no_trailing_count_at_the_cap_boundary() {
    let dir = temp_project();
    let root = dir.path();
    seed_store(root);
    let file = "exactly_ten.rs";

    {
        let id = run_stream_identity(root);
        let p =
            Projector::open(root.join(".rigger").join("graph.db").to_str().unwrap(), &id).unwrap();
        seed_def(&p, 100_001, file, "solo", "function", 1);
    }

    // Exactly AROUND_GOVERNANCE_CAP (10) governing items - the boundary itself, distinct from
    // the 13-item over-the-cap case `graph_around_code_first.rs` already covers.
    for i in 0..10 {
        let nid = format!("n{i}");
        let payload = format!(r#"{{"id":"{nid}","summary":"decision {i}","governs":["{file}"]}}"#);
        let (_o, err, ok) = run_rigger(root, &["emit", "DecisionMade", &payload]);
        assert!(ok, "emit DecisionMade {nid} must succeed; stderr: {err}");
    }

    let (out, err, ok) = run_rigger(root, &["graph", "--around", file, "--depth", "2"]);
    assert!(ok, "graph --around must succeed; stderr: {err}");

    let ids = node_ids(&out);
    for i in 0..10 {
        let nid = format!("n{i}");
        assert!(
            ids.contains(&nid),
            "n{i} is within the cap and must be shown; got:\n{out}"
        );
    }
    assert!(
        out.contains("(newest 10 shown)"),
        "the header must report all ten governing items as shown; got:\n{out}"
    );
    assert!(
        !out.contains("more decision") && !out.contains("more finding"),
        "exactly ten governing items must never print a '+N more' trailer (only 11+ does); \
         got:\n{out}"
    );
}

#[test]
fn around_never_prints_a_governs_or_about_edge_even_though_subgraph_returns_it() {
    let dir = temp_project();
    let root = dir.path();
    seed_store(root);
    let file = "governed.rs";

    {
        let id = run_stream_identity(root);
        let p =
            Projector::open(root.join(".rigger").join("graph.db").to_str().unwrap(), &id).unwrap();
        seed_def(&p, 100_001, file, "guarded", "function", 1);
    }

    let payload =
        format!(r#"{{"id":"d1","summary":"a decision about governed.rs","governs":["{file}"]}}"#);
    let (_o, err, ok) = run_rigger(root, &["emit", "DecisionMade", &payload]);
    assert!(ok, "emit DecisionMade d1 must succeed; stderr: {err}");
    let payload = format!(
        r#"{{"id":"f1","by":"lens:tech","summary":"a finding about governed.rs","about":["{file}"]}}"#
    );
    let (_o, err, ok) = run_rigger(root, &["emit", "ReviewFinding", &payload]);
    assert!(ok, "emit ReviewFinding f1 must succeed; stderr: {err}");

    // Fixture sanity: the raw subgraph must actually contain the GOVERNS/ABOUT edges, or the
    // assertion below proves nothing about the CLI's own filter.
    {
        let id = run_stream_identity(root);
        let p =
            Projector::open(root.join(".rigger").join("graph.db").to_str().unwrap(), &id).unwrap();
        let g = p.subgraph(&[file.to_string()], 2).unwrap();
        assert!(
            g.edges.iter().any(|e| e.rel == "GOVERNS" && e.from == "d1"),
            "fixture sanity: raw subgraph must contain d1's GOVERNS edge, or this test proves \
             nothing; edges: {:?}",
            g.edges
        );
        assert!(
            g.edges.iter().any(|e| e.rel == "ABOUT" && e.from == "f1"),
            "fixture sanity: raw subgraph must contain f1's ABOUT edge, or this test proves \
             nothing; edges: {:?}",
            g.edges
        );
    }

    let (out, err, ok) = run_rigger(root, &["graph", "--around", file, "--depth", "2"]);
    assert!(ok, "graph --around must succeed; stderr: {err}");

    for (from, rel, to) in edge_lines(&out) {
        assert_ne!(
            rel, "GOVERNS",
            "a GOVERNS edge must never print in the edge block; edge {from} -{rel}-> {to}; \
             got:\n{out}"
        );
        assert_ne!(
            rel, "ABOUT",
            "an ABOUT edge must never print in the edge block; edge {from} -{rel}-> {to}; \
             got:\n{out}"
        );
        assert!(
            from != "d1" && to != "d1" && from != "f1" && to != "f1",
            "the decision/finding id must never appear as a printed edge endpoint; edge \
             {from} -{rel}-> {to}; got:\n{out}"
        );
    }
    assert!(
        node_ids(&out).contains(&"d1".to_string()) && node_ids(&out).contains(&"f1".to_string()),
        "the decision and finding must still print as NODES in the governing section - only \
         their edges are filtered; got:\n{out}"
    );
}
