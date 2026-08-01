//! Periphery tests for `rigger graph --show <entity>` (spec 58, criterion 1) - the BOUNDARY
//! faces the happy-path surface test does not exercise. Where `graph_show_surface.rs` proves
//! resolution (by unique name, by id, and the ambiguous listing), this layer drives the COMPILED
//! `rigger` binary against the show surface's honesty and graceful-degrade contract:
//!
//!   - NONE: an unknown query prints a not-found note and NO body, and EXITS SUCCESS (never errors).
//!   - DEGRADE: when the recorded graph location no longer resolves to source (the file is missing,
//!     or the line is past end-of-file), the site header is still printed with a stale-location
//!     note in place of a body, and the command EXITS SUCCESS - the recorded graph facts survive a
//!     drifted working tree; only the body is unavailable.
//!   - EXTENT: a definition's line-numbered body is bounded by the NEXT definition in the file and
//!     its trailing preamble (the following item's blank/doc-comment separator) is trimmed, so the
//!     body never bleeds into the next item.
//!   - ARG EDGE: `graph` with neither `--around` nor `--show` errors and names BOTH selectors.
//!
//! The graph is seeded by folding the ALWAYS-compiled `CodeEntityExtracted` events (no `symbols`
//! extractor), so every test here runs identically in BOTH feature lanes (default and
//! `--no-default-features`). `read_definition_body` is a private binary fn reachable only through
//! the CLI, so this integration layer is the only place its degrade/extent boundary can be proven.

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

/// Open the seeded `graph.db` under `root`'s `.rigger/`, scoped to the identity the binary reads.
fn open_graph(root: &Path) -> Projector {
    let id = run_stream_identity(root);
    Projector::open(root.join(".rigger").join("graph.db").to_str().unwrap(), &id).unwrap()
}

/// NONE face: an unknown query is not an error. The show surface prints a one-line not-found note
/// (never guessing, never a stack trace), prints NO body, and EXITS SUCCESS - mirroring
/// `--around`'s empty result. Guards `cmd_graph_show`'s `Located::None` arm through the real CLI.
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
/// This is the only layer that can prove it: `read_definition_body` is a private binary fn.
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

/// EXTENT face: a definition's body is bounded by the NEXT definition in the same file, and the
/// trailing separator (a blank line then the following item's doc comment / attributes) is trimmed
/// so the body ends on the definition's own last line, never bleeding into the next item. Drives
/// `read_definition_body`'s `next_def_line` bound and `is_trailing_non_body` trim through the CLI.
#[test]
fn graph_show_bounds_body_at_next_definition_and_trims_trailing_preamble() {
    let dir = temp_project();
    let root = dir.path();
    seed_rigger_dir(root);

    // c.rs: `one` (lines 1-3) then a blank + a doc comment (the separator that attaches to `two`),
    // then `two` at line 6. `one`'s recorded next-definition line is 6, so its raw window is 1..=5;
    // the trailing blank (4) and doc comment (5) are trimmed, leaving the body at lines 1..=3.
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
    // The extent stops before the next definition: none of `two`'s lines appear.
    assert!(
        !out.contains("fn two") && !out.contains("body_of_two"),
        "the body is bounded by the next definition (never bleeds into `two`); got:\n{out}"
    );
    // The trailing separator (blank + the following item's doc comment) is trimmed, so the body
    // ends on `one`'s own closing brace rather than the next item's preamble.
    assert!(
        !out.contains("doc for the next item"),
        "the trailing doc comment (the successor's preamble) is trimmed from the body; got:\n{out}"
    );
}

/// NESTED-DEFINITION face: a definition that CONTAINS a nested `fn`/item shows its FULL body past
/// that nested definition - never truncated to its signature. The graph records the next definition
/// BY LINE, which for a container is its nested child; bounding by that child would silently print
/// only the lines before it (just the signature when the child is on the very next line). The show
/// surface bounds a braced definition by its OWN brace-balanced close instead, so the nested child
/// is part of the shown body and the extent stops at the definition's own closing brace - never
/// bleeding into a following sibling. Guards `read_definition_body`'s brace-balanced extent (the fix
/// for the criterion-1 OUTPUT truncation) through the real CLI, in BOTH feature lanes.
#[test]
fn graph_show_shows_full_body_past_nested_definition() {
    let dir = temp_project();
    let root = dir.path();
    seed_rigger_dir(root);

    // outer.rs: `outer` (lines 1-6) CONTAINS a nested `helper` (lines 2-4) and calls it (line 5);
    // then a blank + doc-comment separator, then a top-level `sibling` at line 9. The graph records
    // outer@1, helper@2 (the next definition by line - nested inside outer), sibling@9. The buggy
    // bound (next-def-by-line) would truncate outer's body to line 1 alone (`fn outer() {`) because
    // helper begins on line 2; the brace-balanced bound spans outer's whole body through line 6.
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

    // `--show helper`: the nested child shows its OWN body (lines 2-4), bounded by its own closing
    // brace - it does NOT over-extend to outer's tail just because the next definition BY LINE
    // (`sibling`) lies far below.
    let (hout, herr, hok) = run_rigger(root, &["graph", "--show", "helper"]);
    assert!(hok, "graph --show helper must succeed; stderr: {herr}");
    assert!(
        hout.contains("fn helper") && hout.contains("inner_body"),
        "the nested definition shows its own body; got:\n{hout}"
    );
    assert!(
        !hout.contains("outer_body") && !hout.contains("sibling_body"),
        "the nested definition's extent is its own closing brace, not the parent's tail; got:\n{hout}"
    );
}

/// LEXICAL face: the brace-balanced extent counts only STRUCTURAL braces - a `}` inside a string
/// literal, a line comment, or a char literal does NOT close the body. A naive brace counter would
/// stop at the first such `}` and silently truncate the body to a fragment; the show surface must
/// span the whole definition. Guards `brace_balanced_end`'s string / comment / char skipping.
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

/// BRACE-LESS face: a definition with NO `{ }` body (a `const`, a type alias, a trait-method
/// declaration) nests nothing, so it is bounded by the NEXT definition in the file and its trailing
/// separator (the following item's blank/doc-comment preamble) is trimmed. This is the fallback path
/// the brace-balanced extent does not cover; it keeps the show surface honest for statement-form
/// definitions. Drives `read_definition_body`'s brace-less bound and `is_trailing_non_body` trim.
#[test]
fn graph_show_bounds_brace_less_definition_at_next_and_trims_preamble() {
    let dir = temp_project();
    let root = dir.path();
    seed_rigger_dir(root);

    // d.rs: `ALPHA` (line 1, a brace-less const) then a blank + doc-comment separator (attaching to
    // `BETA`), then `BETA` at line 4. ALPHA has no `{ }` body, so its extent is bounded by the next
    // definition (line 4); the trailing blank (2) and doc comment (3) are trimmed to ALPHA's own
    // line 1.
    std::fs::write(
        root.join("d.rs"),
        "const ALPHA: u32 = 1;\n\
         \n\
         // doc for beta\n\
         const BETA: u32 = 2;\n",
    )
    .unwrap();
    {
        let p = open_graph(root);
        seed_def(&p, 1, "d.rs", "ALPHA", "const", 1);
        seed_def(&p, 2, "d.rs", "BETA", "const", 4);
    }

    let (out, err, ok) = run_rigger(root, &["graph", "--show", "ALPHA"]);
    assert!(ok, "graph --show ALPHA must succeed; stderr: {err}");
    assert!(
        out.contains("const ALPHA"),
        "the brace-less definition's own line is shown; got:\n{out}"
    );
    // Bounded by the next definition, with the trailing separator trimmed: BETA and its preamble
    // never appear.
    assert!(
        !out.contains("const BETA"),
        "the body is bounded by the next definition (never bleeds into BETA); got:\n{out}"
    );
    assert!(
        !out.contains("doc for beta"),
        "the trailing separator (BETA's preamble) is trimmed from ALPHA's body; got:\n{out}"
    );
}

/// ARG EDGE: `graph` with neither `--around` nor `--show` is a usage error whose message names
/// BOTH selectors (the show flag joined the existing structural one). Guards the changed guard in
/// `cmd_graph` through the real CLI.
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
