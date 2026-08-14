//! Periphery tests for `rigger graph --show <entity>` (spec 58, criterion 1) - the BOUNDARY
//! faces the happy-path surface test does not exercise. Where `graph_show_surface.rs` proves
//! resolution (by unique name, by id, and the ambiguous listing), this layer drives the COMPILED
//! `rigger` binary against the show surface's honesty and graceful-degrade contract:
//!
//!   - NONE: an unknown query prints a not-found note and NO body, and EXITS SUCCESS (never errors).
//!   - DEGRADE: when the recorded graph location no longer resolves to source (the file is missing,
//!     the line is past end-of-file, or the recorded line is 0 - a location that never named a real
//!     source line), the site header is still printed with a stale-location note in place of a body,
//!     and the command EXITS SUCCESS - the recorded graph facts survive a drifted working tree; only
//!     the body is unavailable. Two further degrade arms live inside extent derivation (the `symbols`
//!     lane, past the pre-read guards): the file's EXTENSION has no registered grammar, or the current
//!     tree holds no definition of that name STARTING at the recorded line (a name/line drift) - both
//!     degrade to a note with NO body (and never a wrong body from a neighbor at that line).
//!   - EXTENT: a definition's line-numbered body is bounded by the definition's OWN extent, derived
//!     through the shared multi-grammar symbols authority (the grammar's own tree-sitter node
//!     boundary) - so a nested `fn`, a brace inside a string/comment/char, a signature that itself
//!     carries a brace (a destructuring parameter), and a non-Rust grammar (Python, JS) are all
//!     bounded by the parser, never a hand-rolled lexer. These faces are asserted under `symbols`.
//!   - CLAMP: a body longer than the max window is bounded to it AND carries an explicit omitted-line
//!     note, so the surface never dumps an unbounded body and a clamp is never read as the whole.
//!   - LIGHT LANE: a build without the `symbols` feature links no grammar, so a located entity
//!     degrades to the site plus an explicit extent-unavailable note and NO body - the honesty
//!     contract for a feature lane that cannot derive the extent.
//!   - ARG EDGE: `graph` with neither `--around` nor `--show` errors and names BOTH selectors.
//!
//! The graph is seeded by folding the ALWAYS-compiled `CodeEntityExtracted` events (no `symbols`
//! extractor), so RESOLUTION and the degrade/none/arg faces run identically in BOTH feature lanes.
//! The BODY extent is bounded through the extraction grammar, so the extent/clamp faces are gated to
//! the `symbols` lane and the extent-unavailable note is asserted in the light lane. `definition_body`
//! is a private binary fn reachable only through the CLI, so this integration layer is the only place
//! its degrade/extent boundary can be proven.

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
/// would - so this seeding is feature-lane independent (no `symbols` extractor required). The
/// `lang` is stored as the node attr the real extraction records; the show surface re-resolves the
/// grammar from the file EXTENSION when it derives the extent, so `lang` only needs to be truthful.
fn seed_def_lang(
    p: &Projector,
    pos: u64,
    file: &str,
    name: &str,
    kind: &str,
    line: u32,
    lang: &str,
) {
    let payload = format!(
        r#"{{"file":"{file}","name":"{name}","kind":"{kind}","line":{line},"lang":"{lang}"}}"#
    );
    let mut e = Event::new(TYPE_CODE_ENTITY_EXTRACTED, payload.into_bytes());
    e.position = pos;
    p.apply(&e).unwrap();
}

/// Seed a Rust code-entity definition (the common case). Delegates to [`seed_def_lang`].
fn seed_def(p: &Projector, pos: u64, file: &str, name: &str, kind: &str, line: u32) {
    seed_def_lang(p, pos, file, name, kind, line, "rust");
}

/// Open the seeded `graph.db` under `root`'s `.rigger/`, scoped to the identity the binary reads.
fn open_graph(root: &Path) -> Projector {
    let id = run_stream_identity(root);
    Projector::open(root.join(".rigger").join("graph.db").to_str().unwrap(), &id).unwrap()
}

/// Count the line-numbered body lines in a `--show` output. Each body line is printed as
/// `  <n> | <text>` (a right-padded 1-based line number, then ` | `, then the source), so a line
/// whose text BEFORE the first ` | ` parses as a number is a body line; the site/kind/degree header
/// and the not-found / stale / extent-unavailable notes never carry that shape.
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

/// In a build WITHOUT the `symbols` feature (the light `--no-default-features` lane), a located
/// entity's body extent cannot be derived (no grammar is linked). The show surface must degrade to
/// the site header plus an explicit extent-unavailable note and NO body - never a hand-rolled lexer
/// that would mis-read the grammars the graph ingests. Asserts exit-success, the note, and no body.
#[cfg(not(feature = "symbols"))]
fn assert_light_lane_extent_note(out: &str, ok: bool) {
    assert!(
        ok,
        "a located entity in the light lane still EXITS SUCCESS (a note, never an error); got:\n{out}"
    );
    assert!(
        out.contains("code-extraction grammar") || out.contains("`symbols` feature"),
        "the light lane names the missing extraction grammar in the extent note; got:\n{out}"
    );
    assert_eq!(
        body_line_count(out),
        0,
        "the light lane prints NO line-numbered body (extent unavailable); got:\n{out}"
    );
}

/// NONE face: an unknown query is not an error. The show surface prints a one-line not-found note
/// (never guessing, never a stack trace), prints NO body, and EXITS SUCCESS - mirroring
/// `--around`'s empty result. Guards `cmd_graph_show`'s `Located::None` arm through the real CLI.
/// Lane-independent (resolution is always compiled).
#[test]
fn graph_show_unknown_entity_reports_not_found() {
    let dir = temp_project();
    let root = dir.path();
    seed_rigger_dir(root);

    // A graph that HAS entities, so a miss is a genuine no-match and not merely an empty db.
    {
        let p = open_graph(root);
        seed_def(&p, 1, "a.rs", "alpha", "function", 1);
    }

    let (out, err, ok) = run_rigger(root, &["graph", "--show", "does_not_exist"]);
    assert!(
        ok,
        "an unknown --show query exits SUCCESS (a not-found note, never an error); stderr: {err}"
    );
    assert!(
        out.contains("no such entity"),
        "prints a one-line not-found note; got:\n{out}"
    );
    assert!(
        out.contains("does_not_exist"),
        "the not-found note names the queried entity; got:\n{out}"
    );
    // No body: nothing that looks like the line-numbered body gutter the site face prints.
    assert!(
        !out.contains(" | "),
        "an unknown entity prints NO line-numbered body; got:\n{out}"
    );
}

/// DEGRADE face: when the recorded graph location no longer resolves to working-tree source, the
/// show surface still prints the site header (the graph facts survive) with a stale-location note
/// in place of the body, and EXITS SUCCESS - never an error. Two ways a location drifts:
///   (a) the file is gone entirely; (b) the recorded line is past the file's end.
/// Both degrade BEFORE any extent derivation, so this is lane-independent (holds in BOTH lanes).
/// This is the only layer that can prove it: `definition_body` is a private binary fn.
#[test]
fn graph_show_degrades_to_stale_note_when_location_drifted() {
    let dir = temp_project();
    let root = dir.path();
    seed_rigger_dir(root);

    // (a) `ghost` is recorded at gone.rs:5, but gone.rs is never written to the working tree.
    // (b) `beta` is recorded at short.rs:9, but short.rs holds only two lines.
    std::fs::write(root.join("short.rs"), "fn beta() {}\n// eof\n").unwrap();
    {
        let p = open_graph(root);
        seed_def(&p, 1, "gone.rs", "ghost", "function", 5);
        seed_def(&p, 2, "short.rs", "beta", "function", 9);
    }

    // (a) Missing file -> the site header is shown, the body degrades to the stale note, exit 0.
    let (out, err, ok) = run_rigger(root, &["graph", "--show", "ghost"]);
    assert!(
        ok,
        "a missing working-tree file degrades gracefully (exit SUCCESS); stderr: {err}"
    );
    assert!(
        out.contains("gone.rs:5"),
        "the recorded site header survives a missing file; got:\n{out}"
    );
    assert!(
        out.contains("degree"),
        "the graph facts (kind/degree) are still printed when the body is unavailable; got:\n{out}"
    );
    assert!(
        out.contains("source unavailable") && out.contains("stale"),
        "a stale-location note replaces the body for a missing file; got:\n{out}"
    );
    assert!(
        !out.contains(" | "),
        "no line-numbered body is printed for a missing file; got:\n{out}"
    );

    // (b) Recorded line past end-of-file -> the same graceful degrade, never a panic or an error.
    let (out2, err2, ok2) = run_rigger(root, &["graph", "--show", "beta"]);
    assert!(
        ok2,
        "a recorded line past end-of-file degrades gracefully (exit SUCCESS); stderr: {err2}"
    );
    assert!(
        out2.contains("short.rs:9"),
        "the recorded site header survives a line past end-of-file; got:\n{out2}"
    );
    assert!(
        out2.contains("source unavailable") && out2.contains("stale"),
        "a stale-location note replaces the body when the line is past EOF; got:\n{out2}"
    );
    assert!(
        !out2.contains(" | "),
        "no line-numbered body is printed when the line is past EOF; got:\n{out2}"
    );
}

/// DEGRADE (recorded line 0): a definition whose recorded line is 0 - a location that never named a
/// real source line (a bare cross-file placeholder, or a drifted attr) - degrades to the
/// stale-location note with NO body, and still EXITS SUCCESS. Guards `definition_body`'s `start == 0`
/// arm: the one degrade return the missing-file / past-EOF cases cannot reach, because it
/// short-circuits BEFORE any file read (and BEFORE extent derivation, so it is lane-independent). The
/// source file EXISTS here (with a real body at line 1), so the clean degrade is attributable to the
/// line-0 guard alone - without it the read would run from line 0 and misbehave.
#[test]
fn graph_show_degrades_to_stale_note_when_recorded_line_is_zero() {
    let dir = temp_project();
    let root = dir.path();
    seed_rigger_dir(root);

    // The file is present with a real body at line 1, but the graph records line 0 for the entity -
    // so site_of parses the "line" attr to 0 and definition_body is entered with start == 0.
    std::fs::write(
        root.join("ghost.rs"),
        "fn ghost() {\n    let real_body_at_line_1 = 1;\n}\n",
    )
    .unwrap();
    {
        let p = open_graph(root);
        seed_def(&p, 1, "ghost.rs", "ghost", "function", 0);
    }

    let (out, err, ok) = run_rigger(root, &["graph", "--show", "ghost"]);
    assert!(
        ok,
        "graph --show of a line-0 entity must succeed (a degrade, never an error); stderr: {err}"
    );
    // The graph facts survive: the located site header is still printed.
    assert!(
        out.contains("ghost"),
        "the located site header is still printed; got:\n{out}"
    );
    // The stale-location note stands in for the body...
    assert!(
        out.contains("stale") || out.contains("source unavailable"),
        "a line-0 location degrades to the stale-location note; got:\n{out}"
    );
    // ...and NO body is printed - the real line-1 source is never shown for a line-0 location.
    assert!(
        !out.contains("real_body_at_line_1"),
        "the working-tree body is not shown for a degraded (line 0) location; got:\n{out}"
    );
    assert_eq!(
        body_line_count(&out),
        0,
        "no line-numbered body is printed for a degraded (line 0) location; got:\n{out}"
    );
}

/// ARG EDGE: `graph` with neither `--around` nor `--show` is a usage error whose message names
/// BOTH selectors (the show flag joined the existing structural one). Guards the changed guard in
/// `cmd_graph` through the real CLI. Lane-independent.
#[test]
fn graph_requires_around_or_show() {
    let dir = temp_project();
    let root = dir.path();
    seed_rigger_dir(root);

    let (_out, err, ok) = run_rigger(root, &["graph"]);
    assert!(
        !ok,
        "`graph` with no selector must fail; it printed nothing to stderr? err: {err:?}"
    );
    assert!(
        err.contains("--around") && err.contains("--show"),
        "the usage error names BOTH selectors (--around and --show); got stderr:\n{err}"
    );
}

/// LIGHT-LANE face: a build without the `symbols` feature links no extraction grammar, so a located
/// entity whose file and line ARE present in the working tree still cannot have its body extent
/// derived. The surface degrades to the site header plus an explicit extent-unavailable note and NO
/// body, and EXITS SUCCESS - the honesty contract for a feature lane that cannot derive the extent
/// (in place of the hand-rolled lexer that would mis-read the grammars the graph ingests). Only
/// compiled in the light lane; the `symbols` lane proves the body IS shown for the same shape.
#[cfg(not(feature = "symbols"))]
#[test]
fn graph_show_light_lane_degrades_to_extent_unavailable_note() {
    let dir = temp_project();
    let root = dir.path();
    seed_rigger_dir(root);

    // A present, readable definition at a valid line: the ONLY reason a body cannot be shown is the
    // missing extraction grammar, so the degrade is attributable to the light lane alone.
    std::fs::write(
        root.join("present.rs"),
        "fn present() {\n    let body = 1;\n}\n",
    )
    .unwrap();
    {
        let p = open_graph(root);
        seed_def(&p, 1, "present.rs", "present", "function", 1);
    }

    let (out, _err, ok) = run_rigger(root, &["graph", "--show", "present"]);
    // The recorded site header survives (resolution is always compiled)...
    assert!(
        out.contains("present.rs:1"),
        "the located site header is printed in the light lane; got:\n{out}"
    );
    // ...and the body degrades to the explicit extent-unavailable note, never a silent truncation.
    assert_light_lane_extent_note(&out, ok);
    // The real body line is never shown without the grammar (no silent partial body).
    assert!(
        !out.contains("let body = 1"),
        "the light lane shows no body text without the extraction grammar; got:\n{out}"
    );
}

/// EXTENT face: a definition's body is bounded by its OWN extent - the grammar's tree-sitter node
/// boundary - so it ends on the definition's own last line and never bleeds into a following item or
/// its preamble (a blank line then the successor's doc comment). Under the symbols authority the
/// extent is the whole `function_item` node, whose range does not include a following sibling's
/// leading comment. Gated to the `symbols` lane (the light lane shows no body).
#[cfg(feature = "symbols")]
#[test]
fn graph_show_bounds_body_at_the_definitions_own_extent() {
    let dir = temp_project();
    let root = dir.path();
    seed_rigger_dir(root);

    // c.rs: `one` (lines 1-3) then a blank + a doc comment (the separator that attaches to `two`),
    // then `two` at line 6. `one`'s tree-sitter node spans lines 1-3, so its body is 1-3; the
    // trailing blank (4) and doc comment (5) belong to `two`, not `one`.
    std::fs::write(
        root.join("c.rs"),
        "fn one() {\n\
         \x20\x20\x20\x20let body_of_one = 1;\n\
         }\n\
         \n\
         // doc for the next item\n\
         fn two() {\n\
         \x20\x20\x20\x20let body_of_two = 2;\n\
         }\n",
    )
    .unwrap();
    {
        let p = open_graph(root);
        seed_def(&p, 1, "c.rs", "one", "function", 1);
        seed_def(&p, 2, "c.rs", "two", "function", 6);
    }

    let (out, err, ok) = run_rigger(root, &["graph", "--show", "one"]);
    assert!(ok, "graph --show one must succeed; stderr: {err}");
    // The definition's own body is shown, line-numbered from its recorded line.
    assert!(
        out.lines().any(|l| l.contains("fn one") && l.contains('1')),
        "the definition body starts at its recorded line 1; got:\n{out}"
    );
    assert!(
        out.contains("body_of_one"),
        "the definition's own body line is shown; got:\n{out}"
    );
    // The extent stops at one's own closing brace: none of `two`'s lines appear.
    assert!(
        !out.contains("fn two") && !out.contains("body_of_two"),
        "the body is bounded by the definition's own extent (never bleeds into `two`); got:\n{out}"
    );
    // The trailing separator (blank + the following item's doc comment) is not part of one's node.
    assert!(
        !out.contains("doc for the next item"),
        "the following item's preamble is not part of the body; got:\n{out}"
    );
    assert_eq!(
        body_line_count(&out),
        3,
        "the body is exactly one's own three lines (1-3); got:\n{out}"
    );
}

/// NESTED-DEFINITION face: a definition that CONTAINS a nested `fn`/item shows its FULL body past
/// that nested definition - never truncated to its signature. The graph records the next definition
/// BY LINE, which for a container is its nested child; bounding by that child would silently print
/// only the lines before it (just the signature when the child is on the very next line). The
/// symbols authority bounds the definition by the grammar's own node range, which SPANS the nested
/// child, so the child is part of the shown body and the extent stops at the definition's own end -
/// never bleeding into a following sibling. This is the criterion-1 OUTPUT truncation the round-1
/// defect exposed; here it is bounded through the shared multi-grammar authority. Gated to `symbols`.
#[cfg(feature = "symbols")]
#[test]
fn graph_show_shows_full_body_past_nested_definition() {
    let dir = temp_project();
    let root = dir.path();
    seed_rigger_dir(root);

    // outer.rs: `outer` (lines 1-6) CONTAINS a nested `helper` (lines 2-4) and calls it (line 5);
    // then a blank + doc-comment separator, then a top-level `sibling` at line 9. The graph records
    // outer@1, helper@2 (the next definition by line - nested inside outer), sibling@9. A
    // next-def-by-line bound would truncate outer's body to line 1 alone (`fn outer() {`) because
    // helper begins on line 2; the node-range extent spans outer's whole body through line 6.
    std::fs::write(
        root.join("outer.rs"),
        "fn outer() {\n\
         \x20\x20\x20\x20fn helper() {\n\
         \x20\x20\x20\x20\x20\x20\x20\x20let inner_body = 1;\n\
         \x20\x20\x20\x20}\n\
         \x20\x20\x20\x20let outer_body = helper();\n\
         }\n\
         \n\
         // doc for the sibling\n\
         fn sibling() {\n\
         \x20\x20\x20\x20let sibling_body = 2;\n\
         }\n",
    )
    .unwrap();
    {
        let p = open_graph(root);
        seed_def(&p, 1, "outer.rs", "outer", "function", 1);
        seed_def(&p, 2, "outer.rs", "helper", "function", 2);
        seed_def(&p, 3, "outer.rs", "sibling", "function", 9);
    }

    // `--show outer`: the FULL body is shown, INCLUDING the nested `helper`, not truncated to the
    // `fn outer` signature line.
    let (out, err, ok) = run_rigger(root, &["graph", "--show", "outer"]);
    assert!(ok, "graph --show outer must succeed; stderr: {err}");
    assert!(
        out.contains("fn outer"),
        "the outer definition's signature is shown; got:\n{out}"
    );
    assert!(
        out.contains("fn helper"),
        "the NESTED definition is part of the shown body, not truncated away; got:\n{out}"
    );
    assert!(
        out.contains("inner_body"),
        "the nested definition's own body line is shown inside outer; got:\n{out}"
    );
    assert!(
        out.contains("outer_body"),
        "outer's body AFTER the nested definition is shown (the extent spans past it); got:\n{out}"
    );
    // The extent stops at outer's own closing brace: the following sibling never bleeds in.
    assert!(
        !out.contains("fn sibling") && !out.contains("sibling_body"),
        "the body stops at outer's own closing brace (never bleeds into the sibling); got:\n{out}"
    );
    assert!(
        !out.contains("doc for the sibling"),
        "the following sibling's preamble is not part of outer's body; got:\n{out}"
    );

    // `--show helper`: the nested child shows its OWN body (lines 2-4), bounded by its own node -
    // it does NOT over-extend to outer's tail just because the next definition BY LINE (`sibling`)
    // lies far below.
    let (hout, herr, hok) = run_rigger(root, &["graph", "--show", "helper"]);
    assert!(hok, "graph --show helper must succeed; stderr: {herr}");
    assert!(
        hout.contains("fn helper") && hout.contains("inner_body"),
        "the nested definition shows its own body; got:\n{hout}"
    );
    assert!(
        !hout.contains("outer_body") && !hout.contains("sibling_body"),
        "the nested definition's extent is its own node, not the parent's tail; got:\n{hout}"
    );
}

/// SIGNATURE-BRACE face: a signature that itself carries a `{ }` - a struct-destructuring parameter
/// (`fn f(Point { x, y }: Point) {`) or an `= {}` default - must NOT early-close the body. A lexer
/// that counts the FIRST `{` on the signature line opens and immediately closes the extent ON the
/// signature, silently truncating the body to that one line. The symbols authority bounds by the
/// grammar's own node range, which spans the WHOLE function, so the full body is shown. This is the
/// exact round-2 reject (adv-u58c1-signature-brace-early-close) proven fixed through the real CLI.
/// Gated to `symbols`.
#[cfg(feature = "symbols")]
#[test]
fn graph_show_shows_full_body_of_a_destructuring_signature() {
    let dir = temp_project();
    let root = dir.path();
    seed_rigger_dir(root);

    // sig.rs: `config` (lines 1-4) destructures a `Point { x, y }` PARAMETER - a brace ON the
    // signature line - then a blank + a top-level `sibling` at line 6. A first-`{` lexer truncates
    // config's body to line 1 (`fn config(Point { x, y }: Point) -> u32 {`); the node-range extent
    // spans config's full body through line 4.
    std::fs::write(
        root.join("sig.rs"),
        "fn config(Point { x, y }: Point) -> u32 {\n\
         \x20\x20\x20\x20let sum = x + y;\n\
         \x20\x20\x20\x20sum\n\
         }\n\
         \n\
         fn sibling() {\n\
         \x20\x20\x20\x20let after = 1;\n\
         }\n",
    )
    .unwrap();
    {
        let p = open_graph(root);
        seed_def(&p, 1, "sig.rs", "config", "function", 1);
        seed_def(&p, 2, "sig.rs", "sibling", "function", 6);
    }

    let (out, err, ok) = run_rigger(root, &["graph", "--show", "config"]);
    assert!(ok, "graph --show config must succeed; stderr: {err}");
    assert!(
        out.contains("fn config"),
        "the destructuring signature is shown; got:\n{out}"
    );
    // The body PAST the signature brace is shown - the exact line a first-`{` lexer would truncate.
    assert!(
        out.contains("let sum = x + y"),
        "the body past the signature's destructuring brace is shown, not signature-only; got:\n{out}"
    );
    assert_eq!(
        body_line_count(&out),
        4,
        "the FULL body (lines 1-4) is shown, never truncated to the signature line alone; got:\n{out}"
    );
    // The extent stops at config's own closing brace: the following sibling never bleeds in.
    assert!(
        !out.contains("fn sibling") && !out.contains("after"),
        "the body stops at config's own extent (never over-reads into the sibling); got:\n{out}"
    );
}

/// LEXICAL face: the extent counts only STRUCTURAL braces - a `}` inside a string literal, a line
/// comment, or a char literal does NOT close the body. A naive brace counter would stop at the first
/// such `}` and silently truncate the body to a fragment; the grammar knows the `}` is inside a
/// string / comment / char, so the extent spans the whole definition. Gated to `symbols`.
#[cfg(feature = "symbols")]
#[test]
fn graph_show_extent_ignores_braces_in_strings_comments_and_chars() {
    let dir = temp_project();
    let root = dir.path();
    seed_rigger_dir(root);

    // tricky.rs: `tricky` (lines 1-6) whose body carries a `}` inside a string (line 2), inside a
    // line comment (line 3), and as a `'}'` char literal (line 4) - each a false close a naive
    // counter would trip on - before its real closing brace at line 6, then a top-level `after`.
    std::fs::write(
        root.join("tricky.rs"),
        "fn tricky() {\n\
         \x20\x20\x20\x20let s = \"a } brace in a string\";\n\
         \x20\x20\x20\x20// a } brace in a comment\n\
         \x20\x20\x20\x20let c = '}';\n\
         \x20\x20\x20\x20let real_tail = 1;\n\
         }\n\
         \n\
         fn after() {}\n",
    )
    .unwrap();
    {
        let p = open_graph(root);
        seed_def(&p, 1, "tricky.rs", "tricky", "function", 1);
        seed_def(&p, 2, "tricky.rs", "after", "function", 8);
    }

    let (out, err, ok) = run_rigger(root, &["graph", "--show", "tricky"]);
    assert!(ok, "graph --show tricky must succeed; stderr: {err}");
    // The body spans PAST every false-close `}` to the real closing brace at line 6.
    assert!(
        out.contains("real_tail"),
        "the body spans past the string/comment/char braces to its real tail; got:\n{out}"
    );
    assert!(
        out.contains("a } brace in a string") && out.contains("let c = '}';"),
        "the lines carrying the false-close braces are themselves shown; got:\n{out}"
    );
    // The extent stops at tricky's own closing brace and never bleeds into the sibling.
    assert!(
        !out.contains("fn after"),
        "the body stops at tricky's own closing brace (never bleeds into after); got:\n{out}"
    );
}

/// MULTI-GRAMMAR face (Python, a BRACELESS grammar): the extent authority is the grammar's own node
/// range, so a Python nested `def` inside an outer `def` is bounded by the block's DEDENT - the
/// outer's body spans past the nested child, and the nested child is bounded by its own block, not
/// the outer's tail. A Rust brace lexer finds no `{` and falls back to a next-def-by-line truncation
/// on this grammar; the shared symbols authority is correct. Proves the extent generalizes across
/// the grammars the graph ingests (adv-u58c1-multigrammar-mislex). Gated to `symbols`.
#[cfg(feature = "symbols")]
#[test]
fn graph_show_shows_full_body_of_a_python_nested_def() {
    let dir = temp_project();
    let root = dir.path();
    seed_rigger_dir(root);

    // nested.py: `outer` (lines 1-5) CONTAINS a nested `inner` (lines 2-3); the blocks are bounded
    // by indentation, not braces. The graph records outer@1, inner@2.
    std::fs::write(
        root.join("nested.py"),
        "def outer(n):\n\
         \x20\x20\x20\x20def inner(k):\n\
         \x20\x20\x20\x20\x20\x20\x20\x20return k + 1\n\
         \x20\x20\x20\x20total = inner(n)\n\
         \x20\x20\x20\x20return total\n",
    )
    .unwrap();
    {
        let p = open_graph(root);
        seed_def_lang(&p, 1, "nested.py", "outer", "function", 1, "python");
        seed_def_lang(&p, 2, "nested.py", "inner", "function", 2, "python");
    }

    // `--show outer`: the FULL indented block is shown, INCLUDING the nested `inner`, then outer's
    // own body after it - never truncated at the nested child.
    let (out, err, ok) = run_rigger(root, &["graph", "--show", "outer"]);
    assert!(
        ok,
        "graph --show outer (python) must succeed; stderr: {err}"
    );
    assert!(
        out.contains("def outer") && out.contains("def inner"),
        "the outer python def and its nested inner def are both shown; got:\n{out}"
    );
    assert!(
        out.contains("return total"),
        "outer's body AFTER the nested def is shown (the extent spans the whole block); got:\n{out}"
    );
    assert_eq!(
        body_line_count(&out),
        5,
        "the outer def spans its whole indented block (lines 1-5), not truncated at inner; got:\n{out}"
    );

    // `--show inner`: the nested def is bounded by its OWN block (lines 2-3), not the outer's tail.
    let (iout, ierr, iok) = run_rigger(root, &["graph", "--show", "inner"]);
    assert!(
        iok,
        "graph --show inner (python) must succeed; stderr: {ierr}"
    );
    assert!(
        iout.contains("def inner") && iout.contains("return k + 1"),
        "the nested python def shows its own block; got:\n{iout}"
    );
    assert!(
        !iout.contains("return total"),
        "the nested def's extent is its own block, not the outer's tail; got:\n{iout}"
    );
    assert_eq!(
        body_line_count(&iout),
        2,
        "the nested def spans exactly its own two lines (2-3); got:\n{iout}"
    );
}

/// MULTI-GRAMMAR face (JS, a single-quote-string body): a function body holding a single-quote
/// string that carries a lone `{` must NOT over-read into the NEXT function. A Rust brace lexer that
/// does not treat a single quote as a string delimiter counts that `{` as a body open and swallows
/// the following function (presenting an adjacent definition as this body - worse than truncation).
/// The grammar knows the quote is a string, so the extent stops at the function's own close. Proves
/// the extent generalizes to JS (adv-u58c1-multigrammar-mislex, the over-read case). Gated to
/// `symbols`.
#[cfg(feature = "symbols")]
#[test]
fn graph_show_does_not_overread_a_js_single_quote_brace_body() {
    let dir = temp_project();
    let root = dir.path();
    seed_rigger_dir(root);

    // widget.js: `open` (lines 1-4) whose body holds a `'{'` single-quote string (line 2), then a
    // separate `next` (lines 5-7). The graph records open@1, next@5.
    std::fs::write(
        root.join("widget.js"),
        "function open() {\n\
         \x20\x20\x20\x20const brace = '{';\n\
         \x20\x20\x20\x20return brace;\n\
         }\n\
         function next() {\n\
         \x20\x20\x20\x20return 2;\n\
         }\n",
    )
    .unwrap();
    {
        let p = open_graph(root);
        seed_def_lang(&p, 1, "widget.js", "open", "function", 1, "javascript");
        seed_def_lang(&p, 2, "widget.js", "next", "function", 5, "javascript");
    }

    let (out, err, ok) = run_rigger(root, &["graph", "--show", "open"]);
    assert!(ok, "graph --show open (js) must succeed; stderr: {err}");
    assert!(
        out.contains("function open") && out.contains("const brace = '{'"),
        "open's body (including the single-quote brace line) is shown; got:\n{out}"
    );
    // The extent stops at open's own closing brace: the single-quote `{` never over-reads into next.
    assert!(
        !out.contains("function next") && !out.contains("return 2"),
        "the single-quote `{{` never over-reads the extent into the next function; got:\n{out}"
    );
    assert_eq!(
        body_line_count(&out),
        4,
        "open's body is exactly its own four lines (1-4), never swallowing next; got:\n{out}"
    );
}

/// CLAMP: a body longer than the max window (`SHOW_MAX_BODY_LINES`, 60) is bounded to that many
/// lines AND carries an explicit omitted-line note - the show surface never dumps an unbounded body,
/// even when the definition's OWN closing brace sits far past the cap, and a clamp is never read as
/// the whole definition. Guards the `window_cap` (`start + SHOW_MAX_BODY_LINES - 1`) and the clamp
/// note through the real CLI: it is the one bound that survives a definition whose extent would
/// otherwise run to end-of-file. (`SHOW_MAX_BODY_LINES` is a private binary const, so 60 is asserted
/// by value here.) Gated to `symbols` (the extent is grammar-derived).
#[cfg(feature = "symbols")]
#[test]
fn graph_show_clamps_body_to_the_max_window() {
    let dir = temp_project();
    let root = dir.path();
    seed_rigger_dir(root);

    // big.rs: `fn big` opens at line 1; 58 brace-free filler lines carry it well past the cap; a
    // distinctive token sits AT the 60th line (the cap) and the 61st (one past it); the definition's
    // own closing brace is line 62. The node-range extent is 62, but `window_cap` = 1 + 60 - 1 = 60
    // clamps the printed body to line 60 and the surface announces the omitted lines.
    let mut lines = vec!["fn big() {".to_string()];
    for n in 2..=59 {
        lines.push(format!("    let filler_{n} = {n};"));
    }
    lines.push("    let clamp_boundary_60 = 60;".to_string()); // line 60: the cap itself
    lines.push("    let beyond_clamp_61 = 61;".to_string()); // line 61: one line past the cap
    lines.push("}".to_string()); // line 62: the definition's real closing brace
    std::fs::write(root.join("big.rs"), format!("{}\n", lines.join("\n"))).unwrap();

    {
        let p = open_graph(root);
        seed_def(&p, 1, "big.rs", "big", "function", 1);
    }

    let (out, err, ok) = run_rigger(root, &["graph", "--show", "big"]);
    assert!(ok, "graph --show big must succeed; stderr: {err}");
    // The line AT the cap is shown; the line just past it - and the real closing brace beyond it -
    // are not: the body is clamped, not run to the definition's own end.
    assert!(
        out.contains("clamp_boundary_60"),
        "the body extends up to the 60th line (the cap); got:\n{out}"
    );
    assert!(
        !out.contains("beyond_clamp_61"),
        "the body is clamped at the max window and never dumps past the 60-line cap; got:\n{out}"
    );
    assert_eq!(
        body_line_count(&out),
        60,
        "the printed body is clamped to exactly 60 lines (SHOW_MAX_BODY_LINES); got:\n{out}"
    );
    // The clamp is ANNOUNCED: an explicit note names the omitted lines through the extent's true end
    // (line 62), so a clamped body is never silently read as the whole definition.
    assert!(
        out.contains("clamped") && out.contains("62"),
        "a clamp carries an explicit omitted-line note through the extent's true end; got:\n{out}"
    );
}

/// UNREGISTERED-GRAMMAR degrade (the symbols lane): a located definition whose file EXTENSION has no
/// registered extraction grammar cannot have its extent derived - the symbols registry resolves no
/// grammar for it. The show surface degrades to the site header plus an explicit note (naming the
/// missing grammar) in place of the body, and EXITS SUCCESS - never an error, never a guessed body.
/// This is a DISTINCT degrade arm from the light lane: here the build HAS the `symbols` feature and a
/// present, readable file at a valid line, yet the extent is still unavailable because the extension
/// is not one the grammar registry covers. It exercises `derive_extent_end`'s `registry::for_path`
/// -> `None` branch, reachable only in the `symbols` lane (the light lane short-circuits before it).
/// Only compiled under `symbols`; the extent faces above cover the registered-grammar happy path.
#[cfg(feature = "symbols")]
#[test]
fn graph_show_degrades_when_no_grammar_registered_for_the_file_extension() {
    let dir = temp_project();
    let root = dir.path();
    seed_rigger_dir(root);

    // `notes.txt` is present and readable, with a real definition-shaped line at the recorded site
    // line - so the ONLY reason a body cannot be derived is that `.txt` has no registered grammar
    // (the registry covers rs/cs/ts/tsx/js/mjs/cjs/jsx/go/py; `.txt` resolves to nothing). The
    // degrade is thus attributable to the unregistered extension alone, not a missing file or a
    // drifted line.
    std::fs::write(
        root.join("notes.txt"),
        "define widget as a thing\nwith a second line\n",
    )
    .unwrap();
    {
        let p = open_graph(root);
        // A truthful `lang` attr is irrelevant to the extent path: the show surface re-resolves the
        // grammar from the file EXTENSION, and `.txt` is unregistered regardless of the recorded lang.
        seed_def_lang(&p, 1, "notes.txt", "widget", "function", 1, "text");
    }

    let (out, err, ok) = run_rigger(root, &["graph", "--show", "widget"]);
    assert!(
        ok,
        "a file with an unregistered extension degrades gracefully (exit SUCCESS); stderr: {err}"
    );
    // The recorded site header survives (resolution is grammar-independent).
    assert!(
        out.contains("notes.txt:1"),
        "the recorded site header is printed for an unregistered-extension file; got:\n{out}"
    );
    assert!(
        out.contains("degree"),
        "the graph facts (kind/degree) are still printed when the extent is unavailable; got:\n{out}"
    );
    // The note names the missing extraction grammar for THIS file, in place of the body - the
    // distinctive unregistered-grammar phrasing (not a pre-read "source unavailable" note).
    assert!(
        out.contains("code-extraction grammar") && out.contains("notes.txt"),
        "an unregistered-extension file degrades to a note naming the missing grammar; got:\n{out}"
    );
    // No body: neither a line-numbered gutter nor the file's own text is shown - never a guess from a
    // build that cannot bound the extent.
    assert_eq!(
        body_line_count(&out),
        0,
        "no line-numbered body is printed for an unregistered-extension file; got:\n{out}"
    );
    assert!(
        !out.contains("define widget as a thing"),
        "the file's text is never shown without a grammar to bound the extent; got:\n{out}"
    );
}

/// NAME/LINE-DRIFT degrade (the symbols lane): a definition whose file IS present and whose recorded
/// line IS a real line of that file, but where the CURRENT working tree holds no definition of that
/// name STARTING at the recorded line (the code was edited so the definition moved off its recorded
/// site). `derive_extent_end` matches on BOTH name and site line, so the drifted entity finds no
/// extent and the surface degrades to a stale-location note with NO body - never a WRONG body lifted
/// from whatever definition happens to sit at the recorded line. This is a distinct arm from the
/// missing-file / past-EOF / line-0 degrades (all of which short-circuit BEFORE extent derivation):
/// here every earlier guard passes and the miss is the `definition_extents` name+start_line filter
/// returning empty. Reachable only in the `symbols` lane (the light lane never derives an extent).
#[cfg(feature = "symbols")]
#[test]
fn graph_show_degrades_when_no_definition_of_that_name_at_the_recorded_line() {
    let dir = temp_project();
    let root = dir.path();
    seed_rigger_dir(root);

    // drift.rs: an UNRELATED `other` really sits at line 1 (with a distinctive body token), and the
    // queried `moved` really sits at line 3. The graph records `moved` at line 1 (a stale site: the
    // definition was edited upward since the graph was built). Every pre-extent guard passes - the
    // file exists, line 1 is a real line, it is not 0 and not past EOF - so the degrade is entirely
    // the name+start_line filter miss: no definition NAMED `moved` STARTS at line 1.
    std::fs::write(
        root.join("drift.rs"),
        "fn other() {\n\
         \x20\x20\x20\x20let wrong_body_shown = 1;\n\
         }\n\
         fn moved() {\n\
         \x20\x20\x20\x20let the_real_moved_body = 2;\n\
         }\n",
    )
    .unwrap();
    {
        let p = open_graph(root);
        // Record `moved` at the STALE line 1 (its real current site is line 3).
        seed_def(&p, 1, "drift.rs", "moved", "function", 1);
    }

    let (out, err, ok) = run_rigger(root, &["graph", "--show", "moved"]);
    assert!(
        ok,
        "a name/line-drifted location degrades gracefully (exit SUCCESS); stderr: {err}"
    );
    // The recorded site header survives (the graph facts stand even when the body cannot be bounded).
    assert!(
        out.contains("drift.rs:1"),
        "the recorded (stale) site header survives the drift; got:\n{out}"
    );
    // A stale-location note stands in for the body - the recorded line no longer names that def.
    // Pin the DISTINCTIVE filter-miss phrasing so this guards the derive_extent_end name+line arm,
    // not a pre-read guard (missing-file / past-EOF / line-0 notes share the word "stale" but never
    // say "no definition named ..." - and all three short-circuit before extent derivation anyway).
    assert!(
        out.contains("no definition named") && out.contains("stale"),
        "a name/line drift degrades to the derive-extent stale-location note; got:\n{out}"
    );
    // CRITICAL: no WRONG body. The `other` definition that really sits at line 1 must NOT be shown
    // under the `moved` header - the name+line match is exactly what prevents lifting a neighbor's
    // body. Nor is the real `moved` body shown (its recorded line is stale).
    assert!(
        !out.contains("wrong_body_shown"),
        "the definition that sits at the recorded line is NOT shown under the drifted entity's header; got:\n{out}"
    );
    assert!(
        !out.contains("the_real_moved_body"),
        "the drifted definition's own body is not shown from its stale recorded line; got:\n{out}"
    );
    assert_eq!(
        body_line_count(&out),
        0,
        "no line-numbered body is printed for a name/line-drifted location; got:\n{out}"
    );
}
