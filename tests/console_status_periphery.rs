//! Periphery for spec 93, criterion 4 ("ONE FOLD"): `rigger status` (`cmd_status` in the
//! binary) prints its statusline and its needs-you lines through `rigger::console`'s
//! [`console::fold`], never a second, independently-composed copy of either. The statusline is
//! the first line of a run that is neither held nor started and records no dash URL; a started
//! run's goes under its run header, and a held run's status opens with its HELD line (spec 105,
//! WHAT A PERSON SEES), which this file drives through the real hold verbs.
//!
//! WHY THIS FILE. `cmd_status` is a PRIVATE free function in the `rigger` BINARY crate
//! (`src/main.rs`), unreachable from an integration-test crate under `tests/` other than by
//! spawning the compiled binary (mirrors `tests/cause_wire_periphery.rs`'s identical situation
//! for the same command's blocker lines). `console.rs`'s own colocated `mod tests` already
//! proves the fold itself (unit statuses agree with `ledger::project`, blockers with
//! `blocker::from_events`, the dock's needs-you arms, the statusline's health words) against
//! hand-built in-memory `Event`s; this file proves the OTHER half - that the real compiled
//! `rigger status`, reading a real on-disk store, prints exactly what that fold computes,
//! through a real `argv` -> `main()` -> `cmd_status` dispatch and a real SQLite round trip.

mod common;

use std::path::Path;
use std::time::Duration;

use common::cli::read_run_events;
use common::cli::run_rigger;
use common::cli::seed_run_events;
use common::cli::temp_store_project;
use common::cli::{
    backdate_store_file_by, held_line_parts, seed_a_run, seed_a_supervisor_beat, seed_on_the_run,
};
use common::fixtures::{dated_ago, hold_naming, run_held_event};
use rigger::ledger::{AttentionEntry, ATTENTION_HELD, ATTENTION_RELEASED};
use rigger::run::HOLD_BY_HOST;

/// The reason the operator holds a run with in this file.
const REASON: &str = "a deploy is landing";

/// The operator's `rigger hold` on the current run, for [`REASON`].
const HOLD: &[&str] = &["hold", "--reason", REASON];

/// The operator's `rigger release` of the current run's hold.
const RELEASE: &[&str] = &["release"];

/// Hold `root`'s current run as the host does on a rate limit, needing no person.
fn seed_a_host_hold(root: &Path) {
    let hold = hold_naming("rate_limit", "wait it out", HOLD_BY_HOST);
    seed_on_the_run(root, run_held_event(&hold));
}

/// ONE FOLD, the CLI's use of the core: for a run with an escalated unit, `rigger status`'s
/// FIRST printed line is exactly `console::fold`'s `statusline` computed independently by this
/// test from a real store round trip, and the needs-you section names that unit through
/// `console::fold`'s own dock line - not a second, hand-composed rendering.
#[test]
fn rigger_status_first_line_and_needs_you_section_are_the_consoles_own_fold() {
    let proj = temp_store_project();
    let root = proj.path();
    seed_run_events(
        root,
        &[
            ("UnitStarted", r#"{"id":"u-esc"}"#),
            ("UnitEscalated", r#"{"id":"u-esc"}"#),
        ],
    );

    let (out, err, ok) = run_rigger(root, &["status"]);
    assert!(ok, "rigger status must succeed: {err}");

    let events = read_run_events(root);
    let want = rigger::console::fold(&events, 3).expect("console::fold over the real read-back");

    let first_line = out.lines().next().unwrap_or("");
    assert_eq!(
        first_line, want.statusline,
        "the first printed line must be console::fold's own statusline; got:\n{out}"
    );

    assert!(
        out.contains("needs you:"),
        "a non-empty dock must print a needs-you section; got:\n{out}"
    );
    for line in want.dock.lines() {
        assert!(
            out.contains(&line),
            "expected the needs-you section to carry {line:?}; got:\n{out}"
        );
    }

    // ConsoleState.blockers (this criterion's third fold fact, alongside the statusline and
    // the dock): the escalated unit is ALSO a current blocker (spec 19a's classifier already
    // covers `Escalated`), so the SAME real run must print a "current blockers:" section
    // carrying console::fold's own blocker line - proving the field the "ONE FOLD" Done-when
    // names ("blockers") agrees with the real binary too, not only the statusline and the dock.
    assert!(
        out.contains("current blockers:"),
        "an escalated unit is also a current blocker; got:\n{out}"
    );
    for line in &want.blockers {
        assert!(
            out.contains(line.as_str()),
            "expected the current-blockers section to carry {line:?}; got:\n{out}"
        );
    }
}

/// The dock's OTHER two needs-you arms - the budget halt and reject-recurrence, INCLUDING
/// its resumed guard - proven through the same real `argv` -> `cmd_status` -> real-store round
/// trip the escalated arm already gets above. Before this test, only the escalated arm was
/// wired-proven through the real binary (sdet-u93c4-dock-budget-and-recurrence-arms-not-periphery-proven);
/// the other two were proven only against hand-built in-memory `Event`s in `console.rs`'s own
/// `mod tests`. Also pins the fix for adv-u93c4-c4-dock-resumed-unit-false-needs-you (dock's
/// reject-recurrence arm now mirrors `blocker::classify`'s resumed guard) through the real
/// binary, not only through `console.rs`'s own colocated unit test.
#[test]
fn rigger_status_needs_you_covers_budget_halt_and_reject_recurrence_arms() {
    let proj = temp_store_project();
    let root = proj.path();
    let threshold = rigger::blocker::REJECT_RECURRENCE_DIAGNOSE_THRESHOLD;
    let failed = format!(r#"{{"id":"u-fail","attempts":{threshold},"cause":"reject"}}"#);
    seed_run_events(
        root,
        &[
            // A unit at the reject-recurrence diagnose threshold, never resumed: must appear.
            ("UnitStarted", r#"{"id":"u-fail"}"#),
            ("UnitFailed", failed.as_str()),
            // A unit an operator has since resumed: must NOT appear - the resumed guard
            // dock's reject-recurrence arm now shares with blocker::classify.
            ("UnitStarted", r#"{"id":"u-resumed"}"#),
            (
                "UnitFailed",
                r#"{"id":"u-resumed","attempts":3,"cause":"reject"}"#,
            ),
            ("UnitEscalated", r#"{"id":"u-resumed"}"#),
            (
                "UnitResumed",
                r#"{"unit":"u-resumed","attempts_granted":2,"by":"operator"}"#,
            ),
            // The run-level budget halt, seeded last so it is the CURRENT blocker
            // (`blocker::budget_halt` reports only when no later unit-lifecycle event has
            // superseded it).
            ("BudgetExhausted", r#"{"budget":10,"spawns":10}"#),
        ],
    );

    let (out, err, ok) = run_rigger(root, &["status"]);
    assert!(ok, "rigger status must succeed: {err}");

    let events = read_run_events(root);
    let want = rigger::console::fold(&events, 3).expect("console::fold over the real read-back");
    assert_eq!(
        want.dock.needs_you.len(),
        2,
        "fixture must exercise exactly the budget-halt and u-fail reject-recurrence arms, \
         with u-resumed excluded: {:?}",
        want.dock.needs_you
    );

    assert!(
        out.contains("needs you:"),
        "a non-empty dock must print a needs-you section; got:\n{out}"
    );
    for line in want.dock.lines() {
        assert!(
            out.contains(&line),
            "expected the needs-you section to carry {line:?}; got:\n{out}"
        );
    }

    // Pin the two literal facts directly, independent of `want`: a regression in
    // console::fold's own dock logic (not only in cmd_status's wiring of it) would also
    // break these.
    assert!(
        out.contains("run: budget spent 10/10"),
        "expected the budget-halt line; got:\n{out}"
    );
    assert!(
        out.contains(&format!(
            "u-fail: {threshold} failed attempts, the loop is still remediating - audit the \
             findings (rigger-diagnose-churn)"
        )),
        "expected the reject-recurrence line for u-fail; got:\n{out}"
    );

    // The regression this round fixes, pinned through the real binary: a just-resumed unit
    // must not read as still-needs-you.
    assert!(
        !out.contains(&rigger::ledger::AttentionEntry::reject_recurrence("u-resumed", 3).line()),
        "a just-resumed unit must not appear as reject-recurrence; got:\n{out}"
    );
}

/// A clean run (no units at all) prints the console's `healthy` statusline as its first line,
/// and the needs-you section says so explicitly rather than rendering an empty header - so an
/// operator reading a quiet run's status never wonders whether the needs-you check even ran.
#[test]
fn rigger_status_reports_nothing_needs_you_on_a_clean_run() {
    let proj = temp_store_project();
    let root = proj.path();

    let (out, err, ok) = run_rigger(root, &["status"]);
    assert!(ok, "rigger status must succeed on an empty store: {err}");

    let first_line = out.lines().next().unwrap_or("");
    assert_eq!(first_line, "- . 0/0 units . healthy");
    assert!(
        out.contains("Nothing needs a human right now"),
        "an empty dock must say so explicitly; got:\n{out}"
    );
}

/// `--json` is unaffected by this criterion: the console's needs-you/statusline surface is
/// human-table-only, so the machine-readable array shape (spec 10/14's liveness contract) stays
/// exactly what it was.
#[test]
fn rigger_status_json_is_unaffected_by_the_console_surface() {
    let proj = temp_store_project();
    let root = proj.path();
    seed_run_events(
        root,
        &[
            ("UnitStarted", r#"{"id":"u-esc"}"#),
            ("UnitEscalated", r#"{"id":"u-esc"}"#),
        ],
    );

    let (out, err, ok) = run_rigger(root, &["status", "--json"]);
    assert!(ok, "rigger status --json must succeed: {err}");
    let value: serde_json::Value = serde_json::from_str(out.trim()).expect("valid JSON");
    assert!(
        value.is_array(),
        "the top level must stay a bare array (spec 10/14's liveness contract): {out}"
    );
    assert!(
        !out.contains("needs you:"),
        "the console's human rendering must not leak into --json: {out}"
    );
}

/// `console::dock` and `console::statusline` are independently reachable PUBLIC library API,
/// not merely private helpers [`console::fold`] composes internally - any external consumer
/// (this test crate today, the WASM member crate criterion 2 builds tomorrow) can call them
/// directly and get the exact facts `fold` embeds. Proven over events a REAL SQLite
/// append/read-back round trip produced, not the hand-built in-memory `Event`s `console.rs`'s
/// own colocated `mod tests` uses - so a real store/serialization defect reaching either
/// function's inputs would show here even where it cannot show there. Also closes the two
/// `ConsoleState` fields (`units`, `blockers`) neither periphery test above independently
/// asserts, and the `UnitStatuses` type alias they are keyed by.
#[test]
fn console_dock_and_statusline_are_reachable_directly_over_a_real_store_round_trip() {
    let proj = temp_store_project();
    let root = proj.path();
    seed_run_events(
        root,
        &[
            ("UnitStarted", r#"{"id":"u-esc"}"#),
            ("UnitEscalated", r#"{"id":"u-esc"}"#),
            ("UnitStarted", r#"{"id":"u-building"}"#),
        ],
    );

    let events = read_run_events(root);
    let run = rigger::ledger::project(&events).expect("project a real read-back stream");
    let blockers =
        rigger::blocker::from_events(&events, 3).expect("classify blockers over the real stream");

    // dock(): called directly, bypassing fold entirely.
    let dock = rigger::console::dock(&run, &events);
    assert_eq!(dock.needs_you.len(), 1, "{:?}", dock.needs_you);
    assert_eq!(dock.needs_you[0].unit, "u-esc");
    assert_eq!(
        dock.lines(),
        vec!["u-esc: escalated after exhausting remediation"]
    );

    // statusline(): likewise called directly.
    let line = rigger::console::statusline(&run, &blockers, &dock);
    assert_eq!(line, "u-building . 0/2 units . needs-you");

    // fold() over the SAME real read-back stream must equal the direct calls above,
    // byte-for-byte - the ONE FOLD guarantee, proven this time on the two fields
    // (`units`, `blockers`) this file's other tests check only through their printed text.
    let state = rigger::console::fold(&events, 3).expect("fold the same real read-back stream");
    assert_eq!(state.units.get("u-esc"), Some(&"escalated"));
    assert_eq!(state.units.get("u-building"), Some(&"grounding"));
    assert_eq!(state.blockers, rigger::blocker::lines(&blockers));
    assert_eq!(state.dock, dock);
    assert_eq!(state.statusline, line);
}

/// Spec 105's WHAT A PERSON SEES through the compiled `rigger status`: given a started run the host
/// or the operator (through `rigger hold`) holds, when the operator asks for its status, then it
/// opens with the HELD line, ending that the run resumes by itself only for a host hold a
/// supervisor beat watched within 90 s; the statusline under the run header reads `held` while
/// the dock lists nothing but the hold, and `needs-you` beside any other entry; and the needs-you
/// section lists the hold first for a hold whose latest `RunHeld` needs a person, and nothing for
/// one that needs none. A hold the operator released prints no HELD line and lists nothing.
/// `rigger status --line` prints that same statusline alone, and the statusline, the needs-you and
/// the current-blocker lines are `console::fold`'s own.
#[test]
fn rigger_status_shows_a_person_the_hold_in_its_held_line_statusline_and_needs_you_section() {
    struct Case {
        name: &'static str,
        /// The run's history past its start, before the operator acts.
        seed: fn(&Path),
        /// The verbs the operator runs, in order, before asking for the status.
        verbs: &'static [&'static [&'static str]],
        /// How long before the status the one supervisor beat landed, `None` for no beat.
        beat: Option<Duration>,
        /// The HELD line past its duration, `None` for no HELD line.
        held: Option<String>,
        statusline: &'static str,
        needs_you: Vec<String>,
    }
    let nothing: fn(&Path) = |_| {};
    let operator_held = format!("operator - {REASON}; rigger release ends it");
    let operator_entry = format!("run: held - operator - {REASON}");
    let host_released = "rate_limit - wait it out; rigger release ends it".to_string();
    let threshold = rigger::blocker::REJECT_RECURRENCE_DIAGNOSE_THRESHOLD;
    let cases = vec![
        Case {
            name: "a host hold needing no person, under a beat just landed",
            seed: seed_a_host_hold,
            verbs: &[],
            beat: Some(Duration::ZERO),
            held: Some("rate_limit - wait it out; the run resumes by itself".to_string()),
            statusline: "- . 0/0 units . held",
            needs_you: vec![],
        },
        Case {
            name: "a host hold needing no person, under a beat two minutes old",
            seed: seed_a_host_hold,
            verbs: &[],
            beat: Some(Duration::from_secs(120)),
            held: Some(host_released.clone()),
            statusline: "- . 0/0 units . held",
            needs_you: vec![],
        },
        Case {
            name: "an operator hold",
            seed: nothing,
            verbs: &[HOLD],
            beat: None,
            held: Some(operator_held.clone()),
            statusline: "- . 0/0 units . held",
            needs_you: vec![operator_entry.clone()],
        },
        Case {
            name: "a host hold the operator holds over",
            seed: seed_a_host_hold,
            verbs: &[HOLD],
            beat: None,
            held: Some(operator_held.clone()),
            statusline: "- . 0/0 units . held",
            needs_you: vec![operator_entry.clone()],
        },
        Case {
            name: "a hold needing a person the host holds over needing none",
            seed: |root| {
                let login = rigger::run::RunHeld {
                    needs_person: true,
                    ..hold_naming("auth", "log in again", HOLD_BY_HOST)
                };
                seed_on_the_run(root, run_held_event(&login));
                seed_a_host_hold(root);
            },
            verbs: &[],
            beat: None,
            held: Some(host_released),
            statusline: "- . 0/0 units . held",
            needs_you: vec![],
        },
        Case {
            name: "an operator hold beside a unit failing past the recurrence threshold",
            seed: |root| {
                let attempts = rigger::blocker::REJECT_RECURRENCE_DIAGNOSE_THRESHOLD;
                let failed = format!(r#"{{"id":"u-fail","attempts":{attempts},"cause":"reject"}}"#);
                let started = r#"{"id":"u-fail"}"#;
                seed_run_events(
                    root,
                    &[("UnitStarted", started), ("UnitFailed", failed.as_str())],
                );
            },
            verbs: &[HOLD],
            beat: None,
            held: Some(operator_held),
            statusline: "u-fail . 0/1 units . needs-you",
            needs_you: vec![
                operator_entry,
                AttentionEntry::reject_recurrence("u-fail", threshold).line(),
            ],
        },
        Case {
            name: "an operator hold the operator released",
            seed: nothing,
            verbs: &[HOLD, RELEASE],
            beat: None,
            held: None,
            statusline: "- . 0/0 units . healthy",
            needs_you: vec![],
        },
    ];
    for case in cases {
        let name = case.name;
        let dir = temp_store_project();
        let root = dir.path();
        let run_id = seed_a_run(root);
        (case.seed)(root);
        for verb in case.verbs {
            let (_, err, ok) = run_rigger(root, verb);
            assert_eq!((ok, err.as_str()), (true, ""), "{name}: rigger {verb:?}");
        }
        if let Some(ago) = case.beat {
            seed_a_supervisor_beat(root);
            backdate_store_file_by(root, "progress.db", ago);
        }

        let want = rigger::console::fold(&read_run_events(root), 3).unwrap();
        assert_eq!(
            (want.statusline.as_str(), want.dock.lines()),
            (case.statusline, case.needs_you.clone()),
            "{name}: the console's own fold"
        );

        let (out, err, ok) = run_rigger(root, &["status"]);
        assert_eq!((ok, err.as_str()), (true, ""), "{name}: rigger status");
        let mut expected = Vec::new();
        if let Some(held) = &case.held {
            // The HELD line's duration is the wall clock's, every hold here raised moments ago.
            let first = out.lines().next().unwrap_or_default();
            let (held_for, _) = held_line_parts(first)
                .unwrap_or_else(|| panic!("{name}: the status opens with the HELD line:\n{out}"));
            let secs: f64 = held_for.parse().unwrap();
            assert!(
                (0.0..60.0).contains(&secs),
                "{name}: held since the hold was raised; got {held_for}s"
            );
            expected.push(format!("HELD {held_for}s - {held}"));
        }
        expected.push(format!("run {}", rigger::ledger::short_run_id(&run_id)));
        expected.push(case.statusline.to_string());
        expected.push("needs you:".to_string());
        if case.needs_you.is_empty() {
            expected.push(
                "  Nothing needs a human right now. Walk away - you will be tapped.".to_string(),
            );
        }
        expected.extend(case.needs_you.iter().map(|line| format!("  {line}")));
        if !want.blockers.is_empty() {
            expected.push("current blockers:".to_string());
            expected.extend(want.blockers.iter().map(|line| format!("  {line}")));
        }
        assert_eq!(
            out.lines().collect::<Vec<_>>(),
            expected,
            "{name}: rigger status"
        );
        assert_eq!(
            run_rigger(root, &["status", "--line"]),
            (format!("{}\n", case.statusline), String::new(), true),
            "{name}: rigger status --line"
        );
    }
}

/// The attention fold the dock lists a hold from, `ledger::hold_attention`, over what the hold
/// verbs write to a real store: given a host hold raised ten minutes back that the operator holds
/// over with `rigger hold` and ends with `rigger release`, then a second hold the operator raises
/// and releases at once, the fold yields one `held` entry per hold, naming the reason of its
/// latest `RunHeld`, and one `released` entry per release, naming the `held_for_s` that release
/// recorded.
#[test]
fn hold_attention_folds_what_the_hold_verbs_write_into_one_held_and_one_released_entry_per_hold() {
    let dir = temp_store_project();
    let root = dir.path();
    seed_a_run(root);
    let host = hold_naming("rate_limit", "wait it out", HOLD_BY_HOST);
    seed_on_the_run(root, dated_ago(run_held_event(&host), 600));
    for verb in [HOLD, RELEASE, HOLD, RELEASE] {
        let (_, err, ok) = run_rigger(root, verb);
        assert_eq!((ok, err.as_str()), (true, ""), "rigger {verb:?}");
    }

    let held_for: Vec<u64> = common::cli::run_payloads(root, rigger::run::TYPE_RUN_RELEASED)
        .iter()
        .map(|release| release["held_for_s"].as_u64().unwrap())
        .collect();
    assert!(
        matches!(held_for[..], [first, second] if (600..660).contains(&first) && second < 60),
        "each release records how long its own hold stood, the first ten minutes: {held_for:?}"
    );
    let held = || AttentionEntry::run_scoped(ATTENTION_HELD, format!("held - operator - {REASON}"));
    let released = |secs: u64| {
        AttentionEntry::run_scoped(ATTENTION_RELEASED, format!("released after {secs}s"))
    };
    assert_eq!(
        rigger::ledger::hold_attention(&read_run_events(root)),
        Some(vec![
            held(),
            released(held_for[0]),
            held(),
            released(held_for[1])
        ])
    );
}
