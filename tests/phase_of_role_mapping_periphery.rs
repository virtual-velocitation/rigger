//! Spec 67, criterion 1 (PHASE MAPPING) - PERIPHERY: `workflows/rigger.js`'s `roleOf` and
//! `phaseOf` are a cross-module seam this unit's mechanical enumeration turned up - a new JS
//! reader that must stay in sync with the id grammar `spawn::spawn_id` / `spawn::spawn_role`
//! (`src/spawn.rs`) own on the Rust side (`{unit}/{role}#{attempt}`, an optional `~retry{n}`
//! respawn suffix riding the role, spec 18).
//!
//! WHY THIS FILE, DISTINCT FROM THE IMPLEMENTER'S OWN `mod tests` (`src/main.rs`'s
//! `phase_of_maps_wave_items_to_the_meta_phase_by_role_and_stage`). That test asserts the
//! mapping structurally over the IN-MEMORY `RIGGER_WORKFLOW` constant: it checks that the
//! right SUBSTRINGS exist in the right relative order in the source text. It never actually
//! RUNS `phaseOf`, so a behavioral bug - `startsWith` swapped for an exact match, the `||`
//! short-circuiting on the wrong operand, `split` given the wrong separator character, an
//! off-by-one in which array index `roleOf` reads - would pass every existing structural
//! assertion untouched as long as the right words appear somewhere in the function body.
//!
//! `roleOf` and `phaseOf` depend on NEITHER `agent` nor `parallel` nor even `log` - pure data
//! transforms of the `req` object - so, per the convention `tests/step_attention_periphery.rs`
//! established for `relayAttention`, they can be extracted VERBATIM from the real on-disk
//! source (never hand-copied, so a future edit to either is exercised here without a separate
//! update) and run for real under `node` against wave-item shapes built from the EXACT string
//! literals `src/spawn.rs`'s own doctests pin for `spawn_id` / `spawn_retry_id` / `spawn_role` -
//! never a re-derived or hand-typed id, so a change to the Rust-side grammar that this JS
//! reader silently stops mirroring fails HERE, at the seam, rather than only inside whichever
//! module owns the Rust-side grammar's own tests.
//!
//! NOT OWNED here: the mapping's own decision content (which role goes to which group) - that
//! is the Design's call, pinned by the implementer's structural test cited above and repeated
//! here only as executable inputs/outputs, never re-litigated; courier placement (the Drive
//! lane, criterion 3) and the persona-led work label (criterion 2), separate functions in the
//! same file this unit does not touch.

use std::path::Path;
use std::process::Command;

/// Read `workflows/rigger.js` at test time from the crate manifest dir - mirrors `tests/
/// cli.rs`'s and `tests/step_attention_periphery.rs`'s identical `rigger_js_source` helper
/// (the established per-file duplication convention for this small fixture).
fn rigger_js_source() -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("workflows")
        .join("rigger.js");
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

/// Extract a top-level `function <name>(...) { ... }` declaration VERBATIM from
/// `start_marker` through its brace-matched close, inclusive. The same brace-counting
/// `tests/step_attention_periphery.rs::js_declaration` uses (this file's own copy, per that
/// file's documented per-file duplication convention) - kept here rather than shared so
/// neither file's fixture depends on the other's existence or internal layout.
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

/// Run the REAL `phaseOf` - extracted verbatim from the shipped `workflows/rigger.js` together
/// with the `roleOf` helper it calls, never hand-copied - against one `req` shape, under a
/// real `node` subprocess. Neither function reads any harness-injected global (`agent`,
/// `parallel`, `log`), so nothing needs stubbing. Returns the real return value, or `None` when
/// `node` is not on PATH - the same graceful-absence contract `tests/step_attention_periphery
/// .rs::run_relay_attention` and `src/main.rs`'s own `node --check` test already establish for
/// this crate (missing node is an environment fact, never a test failure).
fn run_phase_of(id: &str, unit: &str) -> Option<String> {
    let src = rigger_js_source();
    let role_of_fn = js_declaration(&src, "function roleOf(req) {");
    let phase_of_fn = js_declaration(&src, "function phaseOf(req) {");
    let req = serde_json::json!({ "id": id, "unit": unit, "stage": "irrelevant" }).to_string();

    let script = format!(
        "{role_of_fn}\n{phase_of_fn}\nprocess.stdout.write(String(phaseOf(JSON.parse(process.argv[2]))))\n"
    );

    let node = std::env::var("RIGGER_NODE").unwrap_or_else(|_| "node".to_string());
    let mut f = tempfile::NamedTempFile::new().unwrap();
    std::io::Write::write_all(&mut f, script.as_bytes()).unwrap();

    match Command::new(&node).arg(f.path()).arg(&req).output() {
        Ok(out) => {
            assert!(
                out.status.success(),
                "the real phaseOf must run without throwing on req {req}: {}",
                String::from_utf8_lossy(&out.stderr)
            );
            Some(String::from_utf8_lossy(&out.stdout).into_owned())
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

/// The two run-wide meta-stages resolve to `Plan` by UNIT, regardless of the role the id
/// carries - the exact behavior this criterion introduces to fix the split-across-Build/Review
/// bug the planner (role `implementer`) and the critique gate (roles `adversary`/`adjudicator`)
/// would otherwise cause under a role-first mapping. Every id here is a real shape
/// `spawn::spawn_id` mints (`{unit}/{role}#{attempt}`); a structural pin over source text
/// cannot tell a role-first `phaseOf` (which would mis-route the adversary/adjudicator rows to
/// Review) from the correct unit-first one - only running it, with a review-tier role on a
/// meta-stage unit, distinguishes them.
#[test]
fn plan_and_plan_critique_resolve_to_plan_regardless_of_role() {
    for (unit, id) in [
        ("plan", "plan/implementer#0"),
        ("plan-critique", "plan-critique/adversary#0"),
        ("plan-critique", "plan-critique/adjudicator#0"),
    ] {
        let Some(out) = run_phase_of(id, unit) else {
            return; // node unavailable; graceful absence.
        };
        assert_eq!(
            out, "Plan",
            "phaseOf({{unit: {unit:?}, id: {id:?}}}) must resolve to 'Plan' - the run-wide \
             meta-stage special case must win over the id's own role"
        );
    }
}

/// The three review-tier roles - `adversary`, `adjudicator`, and `lens:*` by PREFIX (not an
/// exact-match list a new lens agent id would fall through) - resolve to `Review` on an
/// ordinary per-criterion unit. `lens:sdet` and the doctest respawn shape
/// `u1/adjudicator#0~retry2` are the EXACT strings `src/spawn.rs`'s own doctests pin for
/// `spawn_id`/`spawn_role`, so this also proves the `~retry{n}` suffix `roleOf` trims is
/// trimmed correctly end to end, not merely that the substring `adjudicator` appears somewhere
/// in the id.
#[test]
fn review_tier_roles_resolve_to_review() {
    for (unit, id) in [
        ("u1", "u1/adversary#0"),
        ("u1", "u1/adjudicator#0~retry2"),
        ("u1", "u1/lens:sdet#1"),
        ("u1", "u1/lens:architecture-reviewer#0"),
    ] {
        let Some(out) = run_phase_of(id, unit) else {
            return; // node unavailable; graceful absence.
        };
        assert_eq!(
            out, "Review",
            "phaseOf({{unit: {unit:?}, id: {id:?}}}) must resolve to 'Review'"
        );
    }
}

/// `implementer`, and any role this mapping does not recognize (a future role added on the
/// Rust side with no matching JS branch yet, and the degenerate case of an id carrying no `/`
/// at all, which `roleOf` reads as an empty role) all fall to the fail-visible `Build` default -
/// proven by actually calling `phaseOf`, not by checking that the literal `'Build'` merely
/// appears in the source (which the implementer's own structural test already does, and which
/// would stay green even if the branch above it accidentally caught these roles first).
#[test]
fn implementer_and_any_unrecognized_role_resolve_to_the_fail_visible_build_default() {
    for (unit, id) in [
        ("u1", "u1/implementer#0"),
        ("u1", "u1/some-future-role#0"),
        ("u1", "no-slash-in-this-id"),
    ] {
        let Some(out) = run_phase_of(id, unit) else {
            return; // node unavailable; graceful absence.
        };
        assert_eq!(
            out, "Build",
            "phaseOf({{unit: {unit:?}, id: {id:?}}}) must fall to the fail-visible 'Build' \
             default, not drop the row (undefined) or misroute it to 'Review'"
        );
    }
}
