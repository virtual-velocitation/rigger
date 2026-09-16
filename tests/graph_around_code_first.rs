//! Integration test for `rigger graph --around <file>` printing the file's neighborhood CODE
//! FIRST (spec 92, u92c6 - "A FILE'S NEIGHBORHOOD IS CODE FIRST"): the file's code entities and
//! their typed relations render in one unbroken block, then the file's governing decisions and
//! findings render in a SEPARATE, capped section (newest ten, with a count of the rest) - never
//! interleaved.
//!
//! Evidence this fixes (u88c1, 2026-09-11): "`graph --around` returned only generic decision-node
//! spam (not code structure) for conductor.rs" - a loop agent fell back to grep for
//! `driver.spawn` call sites the graph already held as `CALLS` edges, because the decisions
//! crowded every code entity off the page.
//!
//! The fixture seeds two code-entity definitions directly into `graph.db` (the always-compiled
//! `CodeEntityExtracted` fold, exactly as `graph_show_surface.rs` seeds) at a position range far
//! above anything the real store below issues, so the two seeding paths can never collide on the
//! graph's own per-position `applied` idempotency ledger. It then drives THIRTEEN decisions and
//! findings about the SAME file through the REAL `rigger emit` CLI path (the same one a loop
//! agent uses) in a known order, so the store's own monotonic event position is the "newest"
//! signal under test - never a wall clock, which no node carries.

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
/// `graph_show_surface.rs` seeds - feature-lane independent, no `symbols` extractor required.
fn seed_def(p: &Projector, pos: u64, file: &str, name: &str, kind: &str, line: u32) {
    let payload = format!(
        r#"{{"file":"{file}","name":"{name}","kind":"{kind}","line":{line},"lang":"rust"}}"#
    );
    let mut e = Event::new(TYPE_CODE_ENTITY_EXTRACTED, payload.into_bytes());
    e.position = pos;
    p.apply(&e).unwrap();
}

/// The `node <id> <kind>` lines of a `rigger graph --around` transcript, in the ORDER they
/// printed (their 0-based line index) - parsed exactly, never substring-matched, so "code first,
/// never interleaved" is a claim about POSITION in the output, not merely membership.
fn node_lines(out: &str) -> Vec<(usize, String)> {
    out.lines()
        .enumerate()
        .filter_map(|(i, l)| {
            l.trim_start()
                .strip_prefix("node ")
                .and_then(|rest| rest.split_whitespace().next())
                .map(|id| (i, id.to_string()))
        })
        .collect()
}

#[test]
fn around_lists_code_first_then_caps_decisions_and_findings_to_the_newest_ten() {
    let dir = temp_project();
    let root = dir.path();
    seed_store(root);

    let file = "big.rs";

    // Code structure: two definitions in the file under test, seeded directly at a position
    // range (100_000+) the real store below never reaches, so the two seeding paths never
    // collide on graph.db's own per-position applied ledger.
    {
        let id = run_stream_identity(root);
        let p =
            Projector::open(root.join(".rigger").join("graph.db").to_str().unwrap(), &id).unwrap();
        seed_def(&p, 100_001, file, "alpha", "function", 1);
        seed_def(&p, 100_002, file, "beta", "function", 5);
    }

    // THIRTEEN governing decisions/findings about the same file, through the REAL `rigger emit`
    // CLI path, in emission order n0..n12 (even indices DecisionMade, odd ReviewFinding) - so the
    // newest-ten cap under test spans BOTH kinds in one shared section, never two kind-scoped
    // caps of ten each.
    for i in 0..13 {
        let nid = format!("n{i}");
        if i % 2 == 0 {
            let payload =
                format!(r#"{{"id":"{nid}","summary":"decision {i}","governs":["{file}"]}}"#);
            let (_o, err, ok) = run_rigger(root, &["emit", "DecisionMade", &payload]);
            assert!(ok, "emit DecisionMade {nid} must succeed; stderr: {err}");
        } else {
            let payload = format!(
                r#"{{"id":"{nid}","by":"lens:tech","summary":"finding {i}","about":["{file}"]}}"#
            );
            let (_o, err, ok) = run_rigger(root, &["emit", "ReviewFinding", &payload]);
            assert!(ok, "emit ReviewFinding {nid} must succeed; stderr: {err}");
        }
    }

    let (out, err, ok) = run_rigger(root, &["graph", "--around", file, "--depth", "2"]);
    assert!(ok, "graph --around must succeed; stderr: {err}");

    let nodes = node_lines(&out);
    let line_of =
        |id: &str| -> Option<usize> { nodes.iter().find(|(_, n)| n == id).map(|(i, _)| *i) };

    // CODE FIRST: the file itself and both its definitions are present.
    let file_line = line_of(file).unwrap_or_else(|| panic!("file node missing; got:\n{out}"));
    let alpha_line =
        line_of("big.rs::alpha").unwrap_or_else(|| panic!("alpha definition missing; got:\n{out}"));
    let beta_line =
        line_of("big.rs::beta").unwrap_or_else(|| panic!("beta definition missing; got:\n{out}"));

    // NEWEST TEN: n3..n12 (the ten most-recently emitted) survive; n0..n2 (the three oldest) do
    // not - proving the cap keys on RECENCY across both kinds together, not a per-kind cap.
    for i in 3..13 {
        let nid = format!("n{i}");
        assert!(
            line_of(&nid).is_some(),
            "n{i} is among the newest ten and must be shown; got:\n{out}"
        );
    }
    for i in 0..3 {
        let nid = format!("n{i}");
        assert!(
            line_of(&nid).is_none(),
            "n{i} is among the three oldest and must be capped out of the shown section; got:\n{out}"
        );
    }

    // NEVER INTERLEAVED: every code node line printed strictly before every shown narrative node
    // line - the file's structure is never pushed off the page by the decision/finding pile.
    let last_code_line = file_line.max(alpha_line).max(beta_line);
    let first_narrative_line = (3..13)
        .filter_map(|i| line_of(&format!("n{i}")))
        .min()
        .expect("at least one narrative node line must be present");
    assert!(
        last_code_line < first_narrative_line,
        "every code node must print BEFORE every governing decision/finding node, never \
         interleaved; last code line {last_code_line}, first narrative line \
         {first_narrative_line}; got:\n{out}"
    );

    // A COUNT OF THE REST: the three capped-out entries are still named, as a total.
    assert!(
        out.contains("+3") && out.contains("more"),
        "the transcript must report a count of the entries the cap dropped (3); got:\n{out}"
    );
}
