//! Spec 67, criterion 5 (META MATCHES REALITY) - PERIPHERY.
//!
//! Mechanical enumeration for this unit (diff BASE e956561..HEAD 2594928): the four probes
//! for new/changed public API, trait impls, CLI subcommands/flags, and event types all
//! return NOTHING against `*.rs` - the change is confined to `workflows/rigger.js`, and
//! every non-comment line touched sits inside the `export const meta` object literal (three
//! `phases` entries: `Integrate` dropped, `Drive` added, `Build`/`Review` detail text
//! updated). There is no cross-module Rust call to enumerate, but `meta` IS a serialized
//! contract in the sense the probe table's "event type / serialized form" row means: the
//! Workflow runtime statically extracts it (never runs it - `meta` must be a pure literal,
//! per the comment directly above it in the source) and renders `/workflows` progress groups
//! from exactly the `title`s it declares. That external consumer is this criterion's real
//! boundary.
//!
//! WHY THIS FILE, DISTINCT FROM THE IMPLEMENTER'S OWN `mod tests`
//! (`src/main.rs`'s `meta_matches_reality_drops_integrate_and_the_unit_stage_construction`).
//! That test asserts over the RAW TEXT of the `meta` object body: it checks that the
//! substrings `title: 'Plan'`, `title: 'Build'`, `title: 'Review'`, `title: 'Drive'` each
//! appear somewhere in it, and that `title: 'Integrate'` and a few retired phrases do not. A
//! text scan proves presence and absence of substrings; it cannot prove the array those
//! titles live in is shaped the way the criterion actually requires: EXACTLY four phases (a
//! stray fifth phase alongside the required four would sail through untouched), each of
//! those four carrying a non-empty `detail` (an empty or missing `detail` on any phase would
//! also sail through untouched), and the phases declared in the fixed lifecycle order the
//! Design section states (Plan, Build, Review, Drive). A behavioral bug there - one entry's
//! `detail` accidentally left `''`, a leftover fifth phase never removed, two phases in the
//! wrong order - would pass every existing structural assertion as long as the four required
//! words appear somewhere in the object body.
//!
//! So this file extracts the REAL `meta` object literal verbatim from the on-disk source
//! (never hand-copied, so a future edit is exercised here without a separate update) and
//! evaluates it for real under `node` - the exact object the external Workflow runtime would
//! statically extract - then inspects the actual parsed `phases` array, not its source text.
//! `meta` depends on no harness-injected global (`args`, `agent`, `parallel`, `log`, `phase`
//! are all read or called elsewhere in the file, never inside the `meta` literal itself - the
//! same "pure literal" property the source comment above it requires), so it evaluates
//! standalone with nothing to stub, the same way `tests/phase_of_role_mapping_periphery.rs`
//! runs `roleOf`/`phaseOf` standalone.
//!
//! NOT OWNED here: the wording of each `detail` string (the implementer's own content,
//! pinned by its structural test cited above and never re-litigated); `phaseOf`'s role
//! mapping (criterion 1, covered by `tests/phase_of_role_mapping_periphery.rs`); courier
//! placement onto the `Drive` lane (criterion 3, a separate unit - as of this diff the
//! courier spawn sites still pass `phase: 'Plan'` and the global `phase('Plan')` marker
//! still exists, both untouched by this criterion and out of scope here).

use std::path::Path;
use std::process::Command;

/// Read `workflows/rigger.js` at test time from the crate manifest dir - the same
/// `rigger_js_source` helper `tests/phase_of_role_mapping_periphery.rs` and `tests/cli.rs`
/// each keep their own copy of (the established per-file duplication convention for this
/// small fixture).
fn rigger_js_source() -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("workflows")
        .join("rigger.js");
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

/// Extract a top-level brace-delimited declaration VERBATIM from `start_marker` (which must
/// end at the declaration's opening brace) through its brace-matched close, inclusive. This
/// file's own copy of the same brace-counting `tests/phase_of_role_mapping_periphery.rs` and
/// `tests/step_attention_periphery.rs` each keep, per that convention.
fn js_declaration<'a>(src: &'a str, start_marker: &str) -> &'a str {
    let start = src
        .find(start_marker)
        .unwrap_or_else(|| panic!("workflow must contain `{start_marker}`"));
    let open = start
        + src[start..]
            .find('{')
            .expect("declaration must open a brace");
    let mut depth = 0usize;
    for (i, c) in src[open..].char_indices() {
        match c {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return &src[start..=open + i];
                }
            }
            _ => {}
        }
    }
    panic!("`{start_marker}` is not brace-balanced");
}

/// One declared phase, as `meta.phases` ACTUALLY parses under node - never re-derived from
/// source text.
#[derive(serde::Deserialize, Debug)]
struct Phase {
    title: String,
    detail: serde_json::Value,
}

/// Evaluate the REAL `export const meta = { ... }` literal - extracted verbatim from the
/// shipped `workflows/rigger.js`, never hand-copied - under a real `node` subprocess, and
/// return the actual parsed `phases` array. `None` when `node` is not on PATH: the same
/// graceful-absence contract `tests/phase_of_role_mapping_periphery.rs::run_phase_of` and
/// `src/main.rs`'s own `node --check` test already establish for this crate (missing node is
/// an environment fact, never a test failure).
fn run_meta_phases() -> Option<Vec<Phase>> {
    let src = rigger_js_source();
    let meta_decl = js_declaration(&src, "export const meta = {");
    // Drop the ESM `export` keyword so the extracted literal runs as a plain top-level
    // `const` under a script invoked directly by `node` (no --input-type=module, no other
    // export in scope) - the value itself is untouched.
    let meta_decl = meta_decl
        .strip_prefix("export ")
        .expect("declaration must start with `export `");

    let script = format!("{meta_decl}\nprocess.stdout.write(JSON.stringify(meta.phases))\n");

    let node = std::env::var("RIGGER_NODE").unwrap_or_else(|_| "node".to_string());
    let mut f = tempfile::NamedTempFile::new().unwrap();
    std::io::Write::write_all(&mut f, script.as_bytes()).unwrap();

    match Command::new(&node).arg(f.path()).output() {
        Ok(out) => {
            assert!(
                out.status.success(),
                "the real meta object literal must evaluate without throwing:\n{}",
                String::from_utf8_lossy(&out.stderr)
            );
            let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
            Some(
                serde_json::from_str(&stdout)
                    .unwrap_or_else(|e| panic!("meta.phases must be a JSON array: {e}: {stdout}")),
            )
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

/// The criterion's own words are "declares EXACTLY Plan, Build, Review, Drive" - proven here
/// against the REAL parsed array (length AND order), not a substring scan that would miss a
/// stray fifth phase riding alongside the required four, or the two build/review phases
/// swapped with review/drive.
#[test]
fn meta_phases_is_exactly_the_four_lifecycle_groups_in_order() {
    let Some(phases) = run_meta_phases() else {
        return; // node unavailable; graceful absence.
    };
    let titles: Vec<&str> = phases.iter().map(|p| p.title.as_str()).collect();
    assert_eq!(
        titles,
        vec!["Plan", "Build", "Review", "Drive"],
        "meta.phases must declare EXACTLY these four lifecycle groups, in this order - no \
         retired 'Integrate', no stray extra phase, no reordering: got {titles:?}"
    );
}

/// The Design section requires "each with a detail line" - a property the implementer's
/// substring pin never binds to a specific title (it only checks that SOME `title: '...'`
/// substring exists somewhere in the object body). Proven here against the real parsed
/// value: every phase's `detail` must be a non-empty string, not merely present as a key.
#[test]
fn every_declared_phase_carries_a_non_empty_detail_line() {
    let Some(phases) = run_meta_phases() else {
        return; // node unavailable; graceful absence.
    };
    assert_eq!(phases.len(), 4, "expected exactly four phases: {phases:?}");
    for phase in &phases {
        let detail = phase.detail.as_str().unwrap_or_else(|| {
            panic!(
                "phase {:?}'s detail must be a string, got {:?}",
                phase.title, phase.detail
            )
        });
        assert!(
            !detail.trim().is_empty(),
            "phase {:?} must carry a non-empty detail line",
            phase.title
        );
    }
}
