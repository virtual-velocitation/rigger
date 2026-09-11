//! Periphery for spec 88, criterion 3 - ESCALATION RESUMES: `rigger resume-unit <unit>
//! [--attempts N]` appends `UnitResumed { unit, attempts_granted, by }` (one new event
//! type, `ledger::TYPE_UNIT_RESUMED`), which the ledger folds back to a mid-remediation
//! `Failed` unit carrying a widened per-unit remediation ceiling and a display grant
//! `rigger status` (and the dashboard) render as "resumed by operator (N attempt(s)
//! granted)" - overriding the unit's raw status kind until a fresh escalation retires it.
//!
//! WHY THIS FILE, DISTINCT FROM THE IMPLEMENTER'S OWN TESTS. Three implementer-owned
//! suites already cover this feature IN-PROCESS, in the SAME compilation unit, against
//! hand-built inputs:
//!   - `ledger.rs`'s own `mod tests` proves the fold: `UnitResumed` re-enters `Failed`
//!     (not terminal), sets `resume_bound = attempts + attempts_granted`, and a later
//!     `UnitEscalated` clears `resumed` back to `None`.
//!   - `blocker.rs`'s own `mod tests` proves `classify` renders the grant and that it
//!     overrides the raw status kind, and that a re-escalation reads plain again.
//!   - `conductor.rs`'s own `mod tests` proves the WIDENED BOUND itself (private
//!     `RunCtx::max_retries_for`, unreachable outside the crate): a granted unit gets
//!     exactly its extra attempts, keyed per-unit, never lowering the plain bound.
//!
//! And `tests/cli.rs` ALREADY drives the compiled binary end to end for the command's own
//! happy path (default and explicit `--attempts`, and its three named refusals: unknown
//! unit, not-escalated, missing branch) through a REAL git-backed escalation.
//!
//! None of those close every periphery gap this criterion's surface actually has:
//!   1. Every implementer fixture reaches `UnitResumed` ONLY via `cmd_resume_unit` itself.
//!      Nothing proves the EVENT'S WIRE CONTRACT independent of that one producer - that
//!      any correctly-shaped `UnitResumed`, however it reaches the log, folds and renders
//!      correctly through a REAL `Store::open` / SQLite round trip (never an in-memory
//!      `Event` literal, unlike every implementer fixture above).
//!   2. Nothing proves the BACK-COMPAT contract `#[serde(default)]` promises on
//!      `attempts_granted`/`by`: a minimal/legacy-shaped `UnitResumed` (missing both
//!      optional fields) must not crash `rigger status` through a real store round trip.
//!   3. `main.rs`'s own `status_and_dashboard_render_the_same_current_blocker_lines` test,
//!      the ONE in-process proof that `rigger status` and the dashboard's
//!      `dash::build_state` never drift, was not extended with a `Resumed` case, so
//!      nothing proves the dashboard (a public API this criterion never touched, `src/
//!      dash.rs` is absent from this unit's diff entirely) actually renders the new kind
//!      it picks up only generically, through `blocker::Kind`'s closed match.
//!   4. Nothing proves the override survives while the resumed unit is genuinely MID
//!      re-parked attempt (a fresh `UnitStatus` past the resume, still short of a new
//!      `UnitFailed`/`UnitEscalated`) - every implementer CLI fixture only checks status
//!      immediately after the resume and again only after the SECOND escalation, never
//!      while the re-parked implementer's own attempt is still in flight.
//!   5. Nothing drives `rigger resume-unit`'s OWN argument-parsing edges (a non-numeric or
//!      zero `--attempts`, a dangling `--attempts` with no value, an unknown flag, a
//!      missing unit id) or its explicitly-named "already-landed unit" refusal case (the
//!      docstring's own words) - `tests/cli.rs` only drives the happy defaults and a
//!      mid-remediation (never-escalated) unit for the status refusal, not `Integrated`.
//!      The unknown-unit and missing-branch refusals ARE already periphery-tested by
//!      `tests/cli.rs` through a real git escalation; this file adds independent, lighter
//!      seeded-store versions of both rather than resting the accounting on an
//!      implementer-authored fixture alone.
//!
//! This file closes all five, through the compiled binary and (for the dashboard case)
//! the public `rigger::dash::build_state` API, against real on-disk SQLite stores.
//!
//! NOT OWNED HERE: the widened-bound arithmetic itself (`max_retries_for`'s `max(...)`,
//! private to `conductor.rs` and exhaustively covered by its own colocated tests), and the
//! full real-git-worktree re-park lifecycle (`tests/cli.rs`'s own end-to-end happy path
//! already drives that through a real escalation). This file only proves that whatever a
//! `UnitResumed` fact says reaches an operator - on EITHER surface - correctly and safely,
//! and that the command refusing to append one does so on every input this criterion's own
//! Done-when and docstring name.

mod common;

use std::collections::HashMap;
use std::path::Path;
use std::process::Command;

use rigger::contextgraph::Graph;
use rigger::eventstore::namespace::Namespaced;
use rigger::eventstore::sqlite::Store;
use rigger::eventstore::{Event, EventStore, ExpectedRevision};

/// A throwaway project: its own git repo (so `project_identity()` resolves
/// deterministically), with no `.rigger` dir yet. Mirrors `tests/cli.rs`'s `temp_project`
/// and `tests/cause_wire_periphery.rs`'s identically-named helper.
fn temp_project() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("create temp project");
    let _ = Command::new("git")
        .args(["init", "-q"])
        .current_dir(dir.path())
        .status();
    dir
}

/// Seed an initialized, empty `.rigger/events.db` under `root` - stands in for the store a
/// prior `rigger run`/`step` would have created. Mirrors `tests/cause_wire_periphery.rs`.
fn seed_store(root: &Path) {
    let rigger = root.join(".rigger");
    std::fs::create_dir_all(&rigger).unwrap();
    std::fs::File::create(rigger.join("events.db")).unwrap();
}

/// The project identity the binary resolves for `root` - mirrors
/// `tests/cause_wire_periphery.rs`'s identically-named helper, itself mirroring
/// `StoreLocation::identity`'s precedence.
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
    if let Ok(raw) = std::fs::read_to_string(base.join(".rigger").join("project.id")) {
        let id = raw.trim();
        if !id.is_empty() {
            return id.to_string();
        }
    }
    base.file_name()
        .and_then(|n| n.to_str())
        .filter(|s| !s.is_empty())
        .map(String::from)
        .unwrap_or_else(|| "rigger".to_string())
}

/// Append `events` directly to `root`'s namespaced run stream through a REAL `Store::open`
/// / SQLite round trip - standing in for the conductor (or `cmd_resume_unit`) minting them,
/// or for a run that predates this criterion. Mirrors `tests/cause_wire_periphery.rs`.
fn seed_run_events(root: &Path, events: &[(&str, &str)]) {
    let db = root.join(".rigger").join("events.db");
    let backend = Store::open(db.to_str().unwrap()).unwrap();
    let store = Namespaced::new(&backend, &run_stream_identity(root));
    for &(ty, body) in events {
        store
            .append(
                rigger::conductor::STREAM,
                ExpectedRevision::Any,
                &[Event::new(ty, body.as_bytes().to_vec())],
            )
            .unwrap();
    }
}

/// Read `root`'s namespaced run stream back through a REAL `Store::open` round trip, for
/// the dashboard-consistency test below to feed into `dash::build_state` exactly as the
/// serving path would.
fn read_run_events(root: &Path) -> Vec<Event> {
    let db = root.join(".rigger").join("events.db");
    let backend = Store::open(db.to_str().unwrap()).unwrap();
    let store = Namespaced::new(&backend, &run_stream_identity(root));
    store
        .read_stream(
            rigger::conductor::STREAM,
            0,
            rigger::eventstore::Direction::Forward,
        )
        .unwrap()
}

/// Run `rigger <args...>` in `cwd` and return (stdout, stderr, success). Mirrors
/// `tests/cause_wire_periphery.rs`'s identically-named helper.
fn run_rigger(cwd: &Path, args: &[&str]) -> (String, String, bool) {
    let mut cmd = common::rigger_courier();
    cmd.args(args).current_dir(cwd);
    cmd.env("RIGGER_NO_DASH", "1");
    let state = tempfile::tempdir().expect("create a temp XDG_STATE_HOME");
    cmd.env("XDG_STATE_HOME", state.path());
    let out = cmd.output().expect("failed to spawn the rigger binary");
    (
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
        out.status.success(),
    )
}

// ---------------------------------------------------------------------------------------
// Gap 1: the event's wire contract, independent of `cmd_resume_unit`.
// ---------------------------------------------------------------------------------------

/// A `UnitResumed` shaped exactly as `cmd_resume_unit` would append one, but seeded
/// DIRECTLY through a real store round trip - never through the command - reaches
/// `rigger status` correctly. Proves the FOLD + RENDER contract holds for any correctly-
/// shaped producer, not merely the one shipped command that happens to be the only one
/// today.
#[test]
fn a_unit_resumed_event_seeded_directly_through_a_real_store_reaches_status_without_the_command() {
    let proj = temp_project();
    let root = proj.path();
    seed_store(root);
    seed_run_events(
        root,
        &[
            ("UnitStarted", r#"{"id":"u"}"#),
            ("UnitFailed", r#"{"id":"u","attempts":3}"#),
            ("UnitEscalated", r#"{"id":"u"}"#),
            (
                "UnitResumed",
                r#"{"unit":"u","attempts_granted":2,"by":"operator"}"#,
            ),
        ],
    );

    let (out, err, ok) = run_rigger(root, &["status"]);
    assert!(ok, "rigger status must succeed on a resumed unit: {err}");
    assert!(
        out.contains("u: resumed by operator (2 attempt(s) granted)"),
        "a UnitResumed event reaching the log through ANY producer must render the grant \
         through the real binary/store; got:\n{out}"
    );
}

// ---------------------------------------------------------------------------------------
// Gap 2: the back-compat contract on the new event's optional fields.
// ---------------------------------------------------------------------------------------

/// A minimal/legacy-shaped `UnitResumed` - missing BOTH `#[serde(default)]` fields
/// (`attempts_granted`, `by`) - must not crash `rigger status` through a real store round
/// trip. This is the exact shape a future producer that only knows the required `unit`
/// key (or a pre-criterion replay tool unaware of the newer fields) would write.
#[test]
fn a_legacy_shaped_unit_resumed_event_missing_both_optional_fields_survives_a_real_store_round_trip(
) {
    let proj = temp_project();
    let root = proj.path();
    seed_store(root);
    seed_run_events(
        root,
        &[
            ("UnitStarted", r#"{"id":"u"}"#),
            ("UnitFailed", r#"{"id":"u","attempts":1}"#),
            ("UnitEscalated", r#"{"id":"u"}"#),
            ("UnitResumed", r#"{"unit":"u"}"#),
        ],
    );

    let (out, err, ok) = run_rigger(root, &["status"]);
    assert!(
        ok,
        "a legacy causeless-of-optionals UnitResumed must not break rigger status: {err}"
    );
    assert!(
        out.contains("u: resumed by"),
        "the unit must still read as resumed, defaults notwithstanding; got:\n{out}"
    );
    assert!(
        out.contains("0 attempt(s) granted"),
        "a missing attempts_granted must default to 0 (serde default), never error or \
         silently substitute a nonzero value; got:\n{out}"
    );
}

// ---------------------------------------------------------------------------------------
// Gap 3: the dashboard's public `build_state` picks up the new kind, matching status.
// ---------------------------------------------------------------------------------------

/// `rigger status` and the dashboard's public `dash::build_state` must render the SAME
/// line for a resumed unit - the ONE case `main.rs`'s own
/// `status_and_dashboard_render_the_same_current_blocker_lines` test never added, and the
/// SAME real-store-round-trip gap `tests/cause_wire_periphery.rs` closed for the cause
/// wire criterion before it. `src/dash.rs` is untouched by this unit's diff; this proves
/// its generic `blocker::Kind` match (via `kind_tag()`/`full_line()`, never a per-variant
/// `match`) actually carries the new variant through, rather than merely compiling.
#[test]
fn status_and_the_dashboards_build_state_render_the_same_resumed_line_through_a_real_store_round_trip(
) {
    let proj = temp_project();
    let root = proj.path();
    seed_store(root);
    seed_run_events(
        root,
        &[
            ("UnitStarted", r#"{"id":"u"}"#),
            ("UnitFailed", r#"{"id":"u","attempts":3}"#),
            ("UnitEscalated", r#"{"id":"u"}"#),
            (
                "UnitResumed",
                r#"{"unit":"u","attempts_granted":2,"by":"operator"}"#,
            ),
        ],
    );

    let (out, err, ok) = run_rigger(root, &["status"]);
    assert!(ok, "rigger status must succeed: {err}");
    let want = "u: resumed by operator (2 attempt(s) granted)";
    assert!(
        out.contains(want),
        "expected the real binary's rigger status to print {want:?}; got:\n{out}"
    );

    // The SAME events, read back through a real store round trip, fed into the
    // dashboard's public production render.
    let events = read_run_events(root);
    let state = rigger::dash::build_state(
        &events,
        &Graph::default(),
        false,
        &[],
        &HashMap::new(),
        3,
        "rigger-run",
        "main",
    )
    .expect("build_state must succeed over a resumed unit");
    let dash_lines: Vec<String> = state.blockers.iter().map(|b| b.line.clone()).collect();
    assert!(
        dash_lines.iter().any(|l| l == want),
        "the dashboard's build_state must render the identical resumed line as rigger \
         status; dash lines: {dash_lines:?}"
    );
    // The kind tag the dashboard uses for styling/grouping must also be the new, distinct
    // "resumed" tag - not silently falling back to "reject-recurrence" or "escalated".
    let resumed = state
        .blockers
        .iter()
        .find(|b| b.subject == "u")
        .expect("the resumed unit must appear in the dashboard's blocker list");
    assert_eq!(
        resumed.kind, "resumed",
        "the dashboard's kind tag for a resumed unit must be \"resumed\", not folded into \
         another kind's styling"
    );
}

// ---------------------------------------------------------------------------------------
// Gap 4: the override survives a genuinely in-flight re-parked attempt.
// ---------------------------------------------------------------------------------------

/// Once resumed, the unit's re-parked implementer picks the work back up and makes fresh
/// progress (a `UnitStatus` moving it into `green`, the last state before review) WITHOUT
/// having failed or escalated again yet. The grant must still be the line an operator
/// sees - "resumed by operator ..." - never reverting to "building (attempt N)" mid-
/// flight, which is exactly what the raw-status `Building` arm would otherwise render for
/// a `Green` unit. This is the override `blocker::classify`'s own comment documents
/// ("Overrides whatever building/reject-recurrence kind... once its implementer has
/// picked the resume back up") but no implementer fixture exercises while genuinely
/// mid-attempt (every CLI fixture only checks status immediately after the resume, and
/// again only after a SECOND failure/escalation).
#[test]
fn the_resumed_banner_survives_a_genuinely_in_flight_re_parked_attempt_not_yet_resolved() {
    let proj = temp_project();
    let root = proj.path();
    seed_store(root);
    seed_run_events(
        root,
        &[
            ("UnitStarted", r#"{"id":"u"}"#),
            ("UnitFailed", r#"{"id":"u","attempts":1}"#),
            ("UnitEscalated", r#"{"id":"u"}"#),
            (
                "UnitResumed",
                r#"{"unit":"u","attempts_granted":1,"by":"operator"}"#,
            ),
            // The re-parked implementer's own fresh progress - gates green, nothing failed
            // or escalated again yet.
            ("UnitStatus", r#"{"id":"u","status":"green"}"#),
        ],
    );

    let (out, err, ok) = run_rigger(root, &["status"]);
    assert!(ok, "rigger status must succeed: {err}");
    assert!(
        out.contains("u: resumed by operator (1 attempt(s) granted)"),
        "the resumed banner must survive the re-parked implementer's own in-flight \
         progress, not revert to a raw \"building\" line; got:\n{out}"
    );
    assert!(
        !out.contains("building"),
        "a mid-attempt resumed unit must never ALSO read as plain \"building\"; got:\n{out}"
    );
}

// ---------------------------------------------------------------------------------------
// Gap 5: `resume-unit`'s own argument-parsing edges and the "already-landed" refusal.
// ---------------------------------------------------------------------------------------

/// A non-numeric `--attempts` value refuses loudly, naming the bad value - no store is
/// ever touched (argument parsing fails first), so an arbitrary empty directory suffices.
#[test]
fn resume_unit_rejects_a_non_numeric_attempts_value() {
    let dir = tempfile::tempdir().unwrap();
    let (out, err, ok) = run_rigger(dir.path(), &["resume-unit", "u", "--attempts", "abc"]);
    assert!(!ok, "a non-numeric --attempts must refuse; stdout: {out:?}");
    assert!(
        err.contains("abc") && err.to_lowercase().contains("positive integer"),
        "the refusal must name the bad value and explain the constraint; stderr: {err:?}"
    );
}

/// `--attempts 0` refuses: a grant must add at least one real attempt.
#[test]
fn resume_unit_rejects_a_zero_attempts_value() {
    let dir = tempfile::tempdir().unwrap();
    let (out, err, ok) = run_rigger(dir.path(), &["resume-unit", "u", "--attempts", "0"]);
    assert!(!ok, "--attempts 0 must refuse; stdout: {out:?}");
    assert!(
        err.contains('0') && err.to_lowercase().contains("positive integer"),
        "the refusal must name the value and explain the constraint; stderr: {err:?}"
    );
}

/// A dangling `--attempts` with no following value refuses with a usage-shaped message,
/// rather than panicking on an out-of-bounds arg read.
#[test]
fn resume_unit_rejects_a_dangling_attempts_flag_with_no_value() {
    let dir = tempfile::tempdir().unwrap();
    let (out, err, ok) = run_rigger(dir.path(), &["resume-unit", "u", "--attempts"]);
    assert!(!ok, "a dangling --attempts must refuse; stdout: {out:?}");
    assert!(
        err.contains("--attempts") && err.to_lowercase().contains("expects a number"),
        "the refusal must name the flag and what it expects; stderr: {err:?}"
    );
}

/// An unrecognized flag refuses, rather than being silently swallowed or misread as a
/// second unit id.
#[test]
fn resume_unit_rejects_an_unknown_flag() {
    let dir = tempfile::tempdir().unwrap();
    let (out, err, ok) = run_rigger(dir.path(), &["resume-unit", "u", "--bogus"]);
    assert!(!ok, "an unknown flag must refuse; stdout: {out:?}");
    assert!(
        err.contains("--bogus") && err.to_lowercase().contains("unknown argument"),
        "the refusal must name the offending argument; stderr: {err:?}"
    );
}

/// No unit id at all refuses with a usage-shaped message.
#[test]
fn resume_unit_rejects_when_no_unit_id_is_given() {
    let dir = tempfile::tempdir().unwrap();
    let (out, err, ok) = run_rigger(dir.path(), &["resume-unit"]);
    assert!(
        !ok,
        "a bare resume-unit with no id must refuse; stdout: {out:?}"
    );
    assert!(
        err.to_lowercase().contains("expected a unit id"),
        "the refusal must say a unit id was expected; stderr: {err:?}"
    );
}

/// An unknown unit id - absent from the current run entirely - refuses by name. A
/// deliberately independent, lighter-weight seeded-store proof alongside `tests/cli.rs`'s
/// own real-git-escalation version of the same refusal (`resume_unit_refuses_an_unknown_
/// unit`), rather than resting the accounting on that implementer-authored fixture alone.
#[test]
fn resume_unit_rejects_an_unknown_unit_absent_from_the_run() {
    let proj = temp_project();
    let root = proj.path();
    seed_store(root);
    seed_run_events(
        root,
        &[
            ("UnitStarted", r#"{"id":"a"}"#),
            ("UnitEscalated", r#"{"id":"a"}"#),
        ],
    );

    let (out, err, ok) = run_rigger(root, &["resume-unit", "no-such-unit"]);
    assert!(
        !ok,
        "resume-unit on an id absent from the run must refuse; stdout: {out:?}"
    );
    assert!(
        err.contains("no-such-unit"),
        "the refusal must name the unknown unit id; stderr: {err:?}"
    );
}

/// A recorded durable branch that does not exist in the repo refuses, naming the branch
/// and hinting at `git reflog` - here via a branch NAME the repo never created (rather
/// than `tests/cli.rs`'s create-then-delete), an equally valid, independent exercise of
/// `branch_exists`' false path through the real binary and a real git repo.
#[test]
fn resume_unit_refuses_when_the_recorded_branch_was_never_created() {
    let proj = temp_project();
    let root = proj.path();
    seed_store(root);
    seed_run_events(
        root,
        &[
            (
                "UnitStarted",
                r#"{"id":"u","branch":"rigger/u/never-created"}"#,
            ),
            ("UnitEscalated", r#"{"id":"u"}"#),
        ],
    );

    let (out, err, ok) = run_rigger(root, &["resume-unit", "u"]);
    assert!(
        !ok,
        "resume-unit must refuse when the recorded branch does not exist; stdout: {out:?}"
    );
    assert!(
        err.contains("rigger/u/never-created") && err.to_lowercase().contains("reflog"),
        "the refusal must name the missing branch and hint at git reflog; stderr: {err:?}"
    );
}

/// The explicitly-named "already-landed unit has nothing to resume FROM" case from the
/// command's own docstring: an `Integrated` unit - never merely a mid-remediation one, the
/// distinct scenario `tests/cli.rs`'s own `resume_unit_refuses_a_unit_that_has_not_
/// escalated` already covers - refuses by name.
#[test]
fn resume_unit_refuses_an_already_integrated_unit() {
    let proj = temp_project();
    let root = proj.path();
    seed_store(root);
    seed_run_events(
        root,
        &[
            ("UnitStarted", r#"{"id":"u"}"#),
            ("UnitStatus", r#"{"id":"u","status":"reviewed"}"#),
            ("UnitIntegrated", r#"{"id":"u"}"#),
        ],
    );

    let (out, err, ok) = run_rigger(root, &["resume-unit", "u"]);
    assert!(
        !ok,
        "resume-unit on an already-integrated unit must refuse; stdout: {out:?}"
    );
    assert!(
        err.contains("\"u\"") && err.to_lowercase().contains("integrated"),
        "the refusal must name the unit and its landed status, distinct from the plain \
         \"not escalated\" mid-remediation case; stderr: {err:?}"
    );
}
