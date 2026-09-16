//! Acceptance test for `rigger graph --show <entity>` GRACEFUL STALENESS (spec 58, criterion 2:
//! the degrade path). It drives the COMPILED `rigger` binary against a throwaway project whose
//! `graph.db` is seeded from the ALWAYS-compiled `CodeEntityExtracted` fold (so the seeding runs
//! identically in BOTH feature lanes) and whose working tree no longer matches the recorded
//! location - the definition of a stale entity.
//!
//! It proves the criterion as ONE behavioral contract over the TWO drift shapes the spec names -
//! (a) the recorded FILE is missing, and (b) the recorded LINE drifted onto a neighbouring
//! definition - asserting the criterion's invariants together: the command EXITS SUCCESS (never an
//! error), and NO WRONG TEXT is ever presented as current (never a neighbour's body under another
//! entity's header). This is the developer/acceptance layer that owns the done-when line; it is
//! distinct from the SDET periphery, which attributes each internal degrade/heal ARM to its guard.
//!
//! The MISSING-FILE degrade is pre-extent (the working-tree read fails before any grammar is
//! consulted), so its stale note is asserted LANE-INDEPENDENTLY - unchanged by spec 92.
//!
//! The LINE-DRIFT shape's contract CHANGED under spec 92 (FRESH ON EVERY INTEGRATION, criterion 1),
//! which supersedes this criterion's original "always refuse a drifted line" rule
//! (`adj-u58c1-r3-approve`) for the case a name-only search resolves unambiguously: the surface now
//! HEALS to the moved definition's LIVE line and shows its real body, noting the drift, in the
//! `symbols` lane (the light lane still derives no extent, so it still shows no body at all - the
//! "never wrong text" invariant it existed to prove is now the narrower, permanent one: a NEIGHBOUR
//! at the recorded line is never shown under another entity's header, in EITHER lane).

use std::path::Path;
use std::process::Command;

use rigger::contextgraph::sqlite::Projector;
use rigger::contextgraph::{Projection, TYPE_CODE_ENTITY_EXTRACTED};
use rigger::eventstore::Event;

// The compiled `rigger` binary under test is located at RUNTIME by the shared authority in
// `tests/common`: a path baked in at compile time goes stale the moment the target dir moves,
// and every suite that spawns the product then dies with a bare NotFound.
mod common;
use common::rigger_bin;

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

/// Open the seeded `graph.db` under `root`'s `.rigger/`, scoped to the identity the binary reads.
fn open_graph(root: &Path) -> Projector {
    let id = run_stream_identity(root);
    Projector::open(root.join(".rigger").join("graph.db").to_str().unwrap(), &id).unwrap()
}

/// Seed one code-entity DEFINITION node into the persisted `graph.db` by folding a
/// `CodeEntityExtracted` event (the ALWAYS-compiled fold), exactly as a real extraction pass would -
/// so this seeding is feature-lane independent (no `symbols` extractor required). Seeding the entity
/// at a chosen `line` is how a drifted location is expressed: the graph records one site while the
/// working tree holds another.
fn seed_def(p: &Projector, pos: u64, file: &str, name: &str, kind: &str, line: u32) {
    let payload = format!(
        r#"{{"file":"{file}","name":"{name}","kind":"{kind}","line":{line},"lang":"rust"}}"#
    );
    let mut e = Event::new(TYPE_CODE_ENTITY_EXTRACTED, payload.into_bytes());
    e.position = pos;
    p.apply(&e).unwrap();
}

/// Run `rigger <args...>` in `cwd` and return (stdout, stderr, success). Opts out of the
/// auto-started dashboard and points the instance registry at a throwaway state dir, exactly as the
/// other CLI integration tests do, so a short-lived inspector invocation spawns nothing that
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

/// Count the line-numbered body lines in a `--show` output. Each body line is printed as
/// `  <n> | <text>` (a right-padded 1-based line number, then ` | `, then the source), so a line
/// whose text BEFORE the first ` | ` parses as a number is a body line; the site/kind/degree header
/// and the stale-location / extent-unavailable notes never carry that shape.
fn body_line_count(out: &str) -> usize {
    out.lines()
        .filter(|l| {
            l.trim_start()
                .split_once(" | ")
                .map(|(pre, _)| pre.trim().parse::<u32>().is_ok())
                .unwrap_or(false)
        })
        .count()
}

/// DRIFT SHAPE (a): the recorded FILE is missing from the working tree. The show surface still
/// prints the recorded SITE header (the graph facts survive), replaces the body with a STALE note,
/// and EXITS SUCCESS - never an error. This degrade is pre-extent (the working-tree read fails
/// before any grammar is consulted), so the whole contract holds LANE-INDEPENDENTLY.
#[test]
fn graph_show_degrades_gracefully_when_the_recorded_file_is_missing() {
    let dir = temp_project();
    let root = dir.path();
    seed_rigger_dir(root);

    // `ghost` is recorded at gone.rs:7, but gone.rs is never written to the working tree - so its
    // recorded location no longer matches the tree (the file was deleted since the graph was built).
    // A sibling `present.rs` exists and is folded too, so the graph is genuinely populated (the miss
    // is a real drift, not an empty db).
    std::fs::write(root.join("present.rs"), "fn present() {}\n").unwrap();
    {
        let p = open_graph(root);
        seed_def(&p, 1, "present.rs", "present", "function", 1);
        seed_def(&p, 2, "gone.rs", "ghost", "function", 7);
    }

    let (out, err, ok) = run_rigger(root, &["graph", "--show", "ghost"]);

    // NEVER AN ERROR: a missing working-tree file exits SUCCESS.
    assert!(
        ok,
        "a missing recorded file degrades gracefully (exit SUCCESS, never an error); stderr: {err}"
    );
    // THE SITE SURVIVES: the recorded site header and the graph facts (kind/degree) still print.
    assert!(
        out.contains("gone.rs:7"),
        "the recorded site header survives a missing file; got:\n{out}"
    );
    assert!(
        out.contains("degree"),
        "the graph facts (kind/degree) still print when the body is unavailable; got:\n{out}"
    );
    // A STALE NOTE stands in for the body - lane-independent, since the read fails before any extent
    // derivation.
    assert!(
        out.contains("stale"),
        "a stale-location note replaces the body for a missing file; got:\n{out}"
    );
    // NEVER WRONG TEXT: no line-numbered body is presented as the entity's current source.
    assert_eq!(
        body_line_count(&out),
        0,
        "no line-numbered body is printed for a missing file; got:\n{out}"
    );
}

/// DRIFT SHAPE (b): the recorded LINE drifted so that a DIFFERENT (neighbouring) definition now sits
/// at the entity's recorded site. The criterion's sharpest clause - "never wrong text presented as
/// current" - means the surface must NEVER lift that NEIGHBOUR's body under the drifted entity's
/// header. Under spec 92 the drifted entity's OWN body is a different matter: when its name is
/// unambiguous elsewhere in the file, the `symbols` lane now HEALS to it and shows the live body
/// (never refuses); the light lane still cannot derive any extent at all, so it still shows no body
/// for anything. Both lanes still EXIT SUCCESS.
///
/// The "never a neighbour's body" invariant is asserted in BOTH lanes: the symbols lane's name-only
/// fallback only ever searches for the QUERIED name, so `other` can never surface under `moved`'s
/// header regardless of which line it occupies; the light lane derives no extent at all. The
/// site-header and body-presence expectations differ per lane (symbols heals to the live line and
/// shows the real body; light shows no body from the still-recorded stale line), so those are
/// asserted per lane below.
#[test]
fn graph_show_never_presents_a_neighbours_body_when_the_line_drifted() {
    let dir = temp_project();
    let root = dir.path();
    seed_rigger_dir(root);

    // drift.rs: an UNRELATED `other` really sits at line 1 (carrying a distinctive token), and the
    // queried `moved` really sits at line 4 (its own distinctive token, and unambiguous - it occurs
    // at exactly one line). The graph records `moved` at the STALE line 1 - the code was edited so
    // the definition moved down since the graph was built.
    std::fs::write(
        root.join("drift.rs"),
        "fn other() {\n\
         \x20\x20\x20\x20let neighbour_body_never_shown = 1;\n\
         }\n\
         fn moved() {\n\
         \x20\x20\x20\x20let the_real_moved_body = 2;\n\
         }\n",
    )
    .unwrap();
    {
        let p = open_graph(root);
        seed_def(&p, 1, "drift.rs", "moved", "function", 1);
    }

    let (out, err, ok) = run_rigger(root, &["graph", "--show", "moved"]);

    // NEVER AN ERROR: a name/line drift exits SUCCESS, in BOTH lanes.
    assert!(
        ok,
        "a line-drifted location degrades or heals gracefully (exit SUCCESS, never an error); \
         stderr: {err}"
    );

    // NEVER A NEIGHBOUR'S BODY (the criterion's permanent crux, LANE-INDEPENDENT): the neighbour
    // `other` that really occupies the recorded line is NEVER shown under the `moved` header - the
    // symbols lane's fallback only ever searches for the name `moved`, and the light lane derives no
    // extent from any line at all.
    assert!(
        !out.contains("neighbour_body_never_shown"),
        "the definition sitting at the recorded line is NEVER shown under the drifted entity; got:\n{out}"
    );

    // The symbols lane HEALS (spec 92): the live line is shown, the recorded one noted, and the
    // moved entity's OWN real body is shown - no longer refused.
    #[cfg(feature = "symbols")]
    {
        assert!(
            out.contains("drift.rs:4"),
            "the symbols lane heals the site header to the live line; got:\n{out}"
        );
        assert!(
            out.contains("recorded line 1") && out.contains("now 4"),
            "the healed header notes the recorded line differs from the live one; got:\n{out}"
        );
        assert!(
            out.contains("the_real_moved_body"),
            "the symbols lane shows the moved entity's own LIVE body once healed; got:\n{out}"
        );
        assert_eq!(body_line_count(&out), 3, "got:\n{out}");
    }
    // The light lane still cannot derive any extent (no grammar linked), so it still shows no body
    // for anything, from the still-recorded (stale) site.
    #[cfg(not(feature = "symbols"))]
    {
        assert!(
            out.contains("drift.rs:1"),
            "the light lane still shows the recorded (unhealed) site header; got:\n{out}"
        );
        assert!(
            !out.contains("the_real_moved_body"),
            "the light lane never derives an extent, so it shows no body at all; got:\n{out}"
        );
        assert_eq!(body_line_count(&out), 0, "got:\n{out}");
        assert!(
            out.contains("code-extraction grammar") || out.contains("`symbols` feature"),
            "the light lane replaces the body with an extent-unavailable note; got:\n{out}"
        );
    }
}
