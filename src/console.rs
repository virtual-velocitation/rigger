//! The console's own fold (spec 93, criterion 4: "ONE FOLD"): the pure,
//! dependency-free reconstruction of "what is happening" from a recorded event
//! stream, shared between `rigger status` (the terminal) and the Mission
//! Control console page (specs 94-98) so both read the SAME projection -
//! never a second parallel fold. This module is `core` (no `store`, no clock,
//! no I/O): [`fold`] takes only the events already read and the configured
//! remediation bound, so it compiles for `wasm32-unknown-unknown` and gives
//! the identical answer wherever it runs (spec 93 Design, "THE PURITY RULE").
//!
//! [`fold`] delegates to the SAME authorities `rigger status` and the
//! dashboard already share for two of its four facts - [`ledger::project`]
//! for unit statuses, [`blocker::from_events`]/[`blocker::lines`] for the
//! current-blocker lines (already the one classifier both surfaces render,
//! spec 19a) - and adds the two new ones this criterion introduces: [`dock`]
//! (the needs-you list) and [`statusline`] (the one-line summary). `rigger
//! status` (`cmd_status` in the binary) calls [`fold`] directly and prints
//! its `statusline` as the first line and its dock's needs-you lines,
//! rather than composing its own copy of any of these four facts - the "CLI's
//! use of the core" half of this criterion.

use std::collections::BTreeMap;

use crate::blocker::{self, Blocker};
use crate::eventstore::Event;
use crate::ledger::{self, AttentionEntry, RunState, Status};

/// One unit's status word, as the console renders it: the projected
/// [`Status`]'s wire string, keyed by unit id. [`RunState::units`] is already
/// a [`BTreeMap`], so this is lexically ordered for a deterministic render -
/// the SAME order `ledger::project` itself produces, never re-sorted.
pub type UnitStatuses = BTreeMap<String, &'static str>;

/// The dock's needs-you list (spec 93 Design, "6.8 The dock, the health strip
/// and the statusline"): every [`AttentionEntry`] CURRENTLY outstanding, read
/// fresh from the log on every call.
///
/// This is deliberately a different computation from `rigger step`'s push-side
/// [`ledger::RunState::attention`] crossings: that field fires ONCE, on the
/// transition a single live `conductor::run` call observed (spec 69's "once
/// per threshold crossing", for a driver's narrator) and cannot be
/// reconstructed from a static read of the log alone (`RunState::attention`'s
/// own doc: "NOT folded from the log by `project`"). The dock instead answers
/// "what, right now, still needs a human" - a snapshot an operator can read at
/// any time (a `rigger status` call, a page render, a replay scrub) and see
/// exactly the same list until the condition resolves, never a one-shot
/// notification that vanishes the moment it has been read once.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Dock {
    pub needs_you: Vec<AttentionEntry>,
}

impl Dock {
    /// The rendered needs-you lines, one per entry, in [`ledger::attention_kind_rank`]
    /// order - the same "subject: prose" shape the current-blocker lines use
    /// ([`AttentionEntry::line`]).
    pub fn lines(&self) -> Vec<String> {
        self.needs_you.iter().map(AttentionEntry::line).collect()
    }
}

/// Derive the dock's needs-you list from a projected [`RunState`] plus the raw
/// `events` (needed only for the durable budget fact, exactly as
/// [`blocker::from_state`] already takes both). Three CURRENT-STATE signals,
/// a snapshot subset of spec 69's five crossing signals - the two that are
/// genuinely crossings with no persisting "still true" state of their own
/// (the budget's final-tenth threshold, a frontier stall reported only once)
/// stay push-side, on `rigger step`'s wire, not here:
///
/// - **escalated** - every unit whose [`Status`] is currently [`Status::Escalated`].
/// - **halted** - the run's spawn budget is currently spent
///   ([`blocker::budget_halt`], the same durable fact [`blocker::classify`]'s
///   run-level [`blocker::Kind::Budget`] line already surfaces).
/// - **worker-death-recurred** - a unit currently [`Status::Failed`] (mid-remediation,
///   still parked awaiting its next attempt) with two or more recorded
///   attempts AND no in-effect resume grant ([`ledger::Unit::resumed`] is
///   `None`) - the same threshold, and the same resumed guard,
///   [`blocker::classify`]'s `RejectRecurrence`/`Resumed` arms use, so a unit
///   on the "needs you" list is always also visible on the current-blocker
///   list as `reject-recurrence` (never a surprise entry with nothing else to
///   point at), and a unit an operator has just resumed reads as "the
///   operator already acted", never as still needing one.
///
/// Deterministically ordered by [`ledger::attention_kind_rank`], lexical by
/// unit within a kind (both `run.units` and the resulting `Vec` walk are
/// `BTreeMap`-ordered), so two folds of the same log agree byte-for-byte.
pub fn dock(run: &RunState, events: &[Event]) -> Dock {
    let mut needs_you = Vec::new();

    for (id, u) in &run.units {
        if u.status == Status::Escalated {
            needs_you.push(AttentionEntry::unit_scoped(
                ledger::ATTENTION_ESCALATED,
                id.clone(),
                "escalated after exhausting remediation",
            ));
        }
    }

    if let Some(b) = blocker::budget_halt(events) {
        needs_you.push(AttentionEntry::run_scoped(
            ledger::ATTENTION_HALTED,
            format!("budget spent {}/{}", b.spent, b.cap),
        ));
    }

    for (id, u) in &run.units {
        if u.status == Status::Failed && u.attempts >= 2 && u.resumed.is_none() {
            needs_you.push(AttentionEntry::unit_scoped(
                ledger::ATTENTION_WORKER_DEATH_RECURRED,
                id.clone(),
                format!("{} attempts, still parked", u.attempts),
            ));
        }
    }

    needs_you.sort_by_key(|e| ledger::attention_kind_rank(e.kind));
    Dock { needs_you }
}

/// The one-line statusline (spec 93 Design, "6.8"): `rigger status` prints
/// this as its first line, and a later criterion's `rigger status --line`
/// prints exactly this text for an editor status bar - one authority for
/// both. `focus` is the first unit, in the run's own lexical order, that has
/// NOT reached a terminal state (mirrors the theater's own leftmost lane);
/// `-` when every unit is terminal or the run holds none. The health word is
/// the two facts a pure fold over the log can actually see - the deeper
/// five-signal health strip (heartbeats, frontier, churn, dash, store) needs
/// LIVE process state (liveness ages, a stream connection, a store probe)
/// no static event slice carries, and is specs 94-98's page-side job:
/// - `"needs-you"` - the dock is non-empty.
/// - `"working"` - clean of needs-you, but a unit is still mid-flight
///   (a current-blocker line exists).
/// - `"done"` - every unit integrated (a non-empty run with no blockers left).
/// - `"healthy"` - no units yet, or nothing to report either way.
pub fn statusline(run: &RunState, blockers: &[Blocker], dock: &Dock) -> String {
    let total = run.units.len();
    let landed = run
        .units
        .values()
        .filter(|u| u.status == Status::Integrated)
        .count();
    let focus = run
        .units
        .values()
        .find(|u| !matches!(u.status, Status::Integrated | Status::Escalated))
        .map(|u| u.id.as_str())
        .unwrap_or("-");
    let health = if !dock.needs_you.is_empty() {
        "needs-you"
    } else if !blockers.is_empty() {
        "working"
    } else if total > 0 && landed == total {
        "done"
    } else {
        "healthy"
    };
    format!("{focus} . {landed}/{total} units . {health}")
}

/// The whole console fold (spec 93, criterion 4): unit statuses, the
/// current-blocker lines, the dock, and the statusline - everything `rigger
/// status` prints, from one call over one projected [`RunState`]. Pure over
/// `events` plus the configured remediation bound - no clock, no store, no
/// process.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ConsoleState {
    pub units: UnitStatuses,
    pub blockers: Vec<String>,
    pub dock: Dock,
    pub statusline: String,
}

/// Fold a recorded run stream into the [`ConsoleState`] both `rigger status`
/// and the console page render (spec 93 Design, "ONE FOLD" - "so the terminal
/// and the page are one authority"). Calls [`blocker::from_events`] (rather
/// than [`blocker::from_state`] over the `run` this function projects anyway)
/// so this stays the module's own production consumer of that entry point -
/// blocker.rs's own doc names it "the SINGLE authority both operator surfaces
/// render", and a status/console read is not the hot, incremental per-event
/// path (that is the member crate's `fold_push` ABI op, spec 93 criterion 2's
/// own budget) where a second projection would matter.
pub fn fold(
    events: &[Event],
    configured_max_retries: u32,
) -> Result<ConsoleState, serde_json::Error> {
    let run = ledger::project(events)?;
    let blockers = blocker::from_events(events, configured_max_retries)?;
    let dock = dock(&run, events);
    let statusline = statusline(&run, &blockers, &dock);
    let units = run
        .units
        .iter()
        .map(|(id, u)| (id.clone(), u.status.as_str()))
        .collect();
    Ok(ConsoleState {
        units,
        blockers: blocker::lines(&blockers),
        dock,
        statusline,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ev(type_: &str, json: &str) -> Event {
        Event::new(type_, json.as_bytes().to_vec())
    }

    /// ONE FOLD, unit statuses: `console::fold`'s `units` map is exactly
    /// `ledger::project`'s own status word per unit - not a second,
    /// independently-derived copy - proven non-vacuous by covering every
    /// status the projection can produce.
    #[test]
    fn fold_units_agree_with_ledger_project() {
        let events = vec![
            ev(ledger::TYPE_UNIT_STARTED, r#"{"id":"u-pending"}"#),
            ev(ledger::TYPE_UNIT_STARTED, r#"{"id":"u-green"}"#),
            ev(
                ledger::TYPE_UNIT_STATUS,
                r#"{"id":"u-green","status":"green"}"#,
            ),
            ev(ledger::TYPE_UNIT_STARTED, r#"{"id":"u-done"}"#),
            ev(
                ledger::TYPE_UNIT_INTEGRATED,
                r#"{"id":"u-done","commit":"abc"}"#,
            ),
            ev(ledger::TYPE_UNIT_STARTED, r#"{"id":"u-esc"}"#),
            ev(ledger::TYPE_UNIT_ESCALATED, r#"{"id":"u-esc"}"#),
        ];
        let run = ledger::project(&events).unwrap();
        let state = fold(&events, 3).unwrap();
        let want: UnitStatuses = run
            .units
            .iter()
            .map(|(id, u)| (id.clone(), u.status.as_str()))
            .collect();
        assert_eq!(
            state.units, want,
            "console::fold's unit statuses must be ledger::project's own, byte-identical"
        );
        assert_eq!(state.units.get("u-done"), Some(&"integrated"));
        assert_eq!(state.units.get("u-esc"), Some(&"escalated"));
    }

    /// ONE FOLD, blockers: `console::fold`'s `blockers` are exactly
    /// `blocker::from_events`'s own lines - the SAME shared classifier
    /// `rigger status` and the dashboard already render (spec 19a), routed
    /// through the fold rather than duplicated.
    #[test]
    fn fold_blockers_agree_with_blocker_from_events() {
        let events = vec![
            ev(ledger::TYPE_UNIT_STARTED, r#"{"id":"u-build"}"#),
            ev(ledger::TYPE_UNIT_STARTED, r#"{"id":"u-fail"}"#),
            ev(
                ledger::TYPE_UNIT_FAILED,
                r#"{"id":"u-fail","attempts":2,"cause":"reject"}"#,
            ),
        ];
        let want = blocker::lines(&blocker::from_events(&events, 3).unwrap());
        let state = fold(&events, 3).unwrap();
        assert_eq!(state.blockers, want);
        assert!(!want.is_empty(), "fixture must actually exercise a blocker");
    }

    /// The dock lists a currently-escalated unit, and nothing else, for an
    /// otherwise clean run - mutation-discriminating: dropping the escalated
    /// arm empties this list.
    #[test]
    fn dock_lists_a_currently_escalated_unit() {
        let events = vec![
            ev(ledger::TYPE_UNIT_STARTED, r#"{"id":"u-esc"}"#),
            ev(ledger::TYPE_UNIT_ESCALATED, r#"{"id":"u-esc"}"#),
        ];
        let run = ledger::project(&events).unwrap();
        let d = dock(&run, &events);
        assert_eq!(d.needs_you.len(), 1, "{:?}", d.needs_you);
        assert_eq!(d.needs_you[0].kind, ledger::ATTENTION_ESCALATED);
        assert_eq!(d.needs_you[0].unit, "u-esc");
        assert_eq!(
            d.lines(),
            vec!["u-esc: escalated after exhausting remediation"]
        );
    }

    /// The dock lists a currently-spent budget, run-scoped (no `unit`), from
    /// the SAME durable `BudgetExhausted` fact `blocker::classify`'s
    /// run-level line already reads.
    #[test]
    fn dock_lists_a_currently_spent_budget() {
        let events = vec![ev(
            blocker::TYPE_BUDGET_EXHAUSTED,
            r#"{"budget":10,"spawns":10}"#,
        )];
        let run = ledger::project(&events).unwrap();
        let d = dock(&run, &events);
        assert_eq!(d.needs_you.len(), 1, "{:?}", d.needs_you);
        assert_eq!(d.needs_you[0].kind, ledger::ATTENTION_HALTED);
        assert_eq!(d.needs_you[0].unit, "");
        assert_eq!(d.lines(), vec!["run: budget spent 10/10"]);
    }

    /// A budget halt already RESOLVED (a later unit-lifecycle event followed
    /// it - the exact "operator raised the budget and work resumed" case
    /// `blocker::budget_halt`'s own doc names) leaves the dock silent on it,
    /// exactly as `blocker::classify`'s run-level line does.
    #[test]
    fn dock_drops_a_resolved_budget_halt() {
        let mut events = vec![
            ev(
                blocker::TYPE_BUDGET_EXHAUSTED,
                r#"{"budget":10,"spawns":10}"#,
            ),
            ev(ledger::TYPE_UNIT_STARTED, r#"{"id":"u-new"}"#),
        ];
        for (i, e) in events.iter_mut().enumerate() {
            e.position = (i + 1) as u64;
        }
        let run = ledger::project(&events).unwrap();
        let d = dock(&run, &events);
        assert!(
            d.needs_you.is_empty(),
            "a resolved budget halt must not appear: {:?}",
            d.needs_you
        );
    }

    /// A unit an operator has just resumed (`rigger resume-unit`, spec 88) is
    /// re-parked at `Status::Failed` with `resumed = Some` - the exact state
    /// `blocker::classify`'s own resumed guard already excludes from
    /// `RejectRecurrence`. The dock's worker-death-recurred arm must agree: a
    /// just-resumed unit reads "the operator already acted", not "still needs
    /// you" - mirrors `dock_drops_a_resolved_budget_halt`'s "operator already
    /// acted -> dock silent" pattern for the budget arm.
    #[test]
    fn dock_drops_a_resumed_unit_from_worker_death_recurred() {
        let events = vec![
            ev(ledger::TYPE_UNIT_STARTED, r#"{"id":"u-resumed"}"#),
            ev(
                ledger::TYPE_UNIT_FAILED,
                r#"{"id":"u-resumed","attempts":3,"cause":"reject"}"#,
            ),
            ev(ledger::TYPE_UNIT_ESCALATED, r#"{"id":"u-resumed"}"#),
            ev(
                ledger::TYPE_UNIT_RESUMED,
                r#"{"unit":"u-resumed","attempts_granted":2,"by":"operator"}"#,
            ),
        ];
        let run = ledger::project(&events).unwrap();
        assert!(
            run.units["u-resumed"].resumed.is_some(),
            "fixture must actually resume the unit"
        );
        let d = dock(&run, &events);
        assert!(
            d.needs_you.is_empty(),
            "a just-resumed unit must not read as still-needs-you: {:?}",
            d.needs_you
        );
    }

    /// The dock lists a unit past the recurrence threshold that is STILL
    /// failed (mid-remediation, parked awaiting its next attempt) - never an
    /// escalated or integrated one, which have left `Failed` for good.
    #[test]
    fn dock_lists_a_unit_still_failed_past_the_recurrence_threshold() {
        let events = vec![
            ev(ledger::TYPE_UNIT_STARTED, r#"{"id":"u-churn"}"#),
            ev(
                ledger::TYPE_UNIT_FAILED,
                r#"{"id":"u-churn","attempts":3,"cause":"reject"}"#,
            ),
        ];
        let run = ledger::project(&events).unwrap();
        let d = dock(&run, &events);
        assert_eq!(d.needs_you.len(), 1, "{:?}", d.needs_you);
        assert_eq!(d.needs_you[0].kind, ledger::ATTENTION_WORKER_DEATH_RECURRED);
        assert_eq!(d.needs_you[0].unit, "u-churn");
    }

    /// A unit's FIRST failure (attempts == 1) is not a recurrence yet, so the
    /// dock stays silent on it - mirrors `blocker::classify`'s own
    /// `RejectRecurrence` numbering (`#1/max` still prints, but the dock's
    /// bar is "recurred", i.e. failed more than once).
    #[test]
    fn dock_is_silent_on_a_units_first_failure() {
        let events = vec![
            ev(ledger::TYPE_UNIT_STARTED, r#"{"id":"u-once"}"#),
            ev(
                ledger::TYPE_UNIT_FAILED,
                r#"{"id":"u-once","attempts":1,"cause":"reject"}"#,
            ),
        ];
        let run = ledger::project(&events).unwrap();
        let d = dock(&run, &events);
        assert!(d.needs_you.is_empty(), "{:?}", d.needs_you);
    }

    /// A clean run (no units at all) needs nobody: an empty dock, and the
    /// statusline reads `healthy`.
    #[test]
    fn dock_and_statusline_are_clean_on_an_empty_run() {
        let events: Vec<Event> = vec![];
        let run = ledger::project(&events).unwrap();
        let d = dock(&run, &events);
        assert!(d.needs_you.is_empty());
        let blockers = blocker::from_state(&run, &events, 3);
        assert_eq!(statusline(&run, &blockers, &d), "- . 0/0 units . healthy");
    }

    /// The statusline names the focus unit (the first non-terminal one, in
    /// lexical id order), the landed/total count, and the `working` health
    /// word for a unit still building with no needs-you condition.
    #[test]
    fn statusline_reports_focus_landed_and_working() {
        let events = vec![
            ev(ledger::TYPE_UNIT_STARTED, r#"{"id":"u1"}"#),
            ev(
                ledger::TYPE_UNIT_INTEGRATED,
                r#"{"id":"u1","commit":"abc"}"#,
            ),
            ev(ledger::TYPE_UNIT_STARTED, r#"{"id":"u2"}"#),
        ];
        let run = ledger::project(&events).unwrap();
        let blockers = blocker::from_state(&run, &events, 3);
        let d = dock(&run, &events);
        assert_eq!(statusline(&run, &blockers, &d), "u2 . 1/2 units . working");
    }

    /// The statusline reports `done` once every unit has landed and nothing
    /// else needs attention.
    #[test]
    fn statusline_reports_done_when_every_unit_landed() {
        let events = vec![
            ev(ledger::TYPE_UNIT_STARTED, r#"{"id":"u1"}"#),
            ev(
                ledger::TYPE_UNIT_INTEGRATED,
                r#"{"id":"u1","commit":"abc"}"#,
            ),
        ];
        let run = ledger::project(&events).unwrap();
        let blockers = blocker::from_state(&run, &events, 3);
        let d = dock(&run, &events);
        assert_eq!(statusline(&run, &blockers, &d), "- . 1/1 units . done");
    }

    /// The statusline's health word is `needs-you` whenever the dock is
    /// non-empty, even if that same unit also carries a current-blocker line
    /// (an escalated unit is both) - needs-you takes precedence over working.
    #[test]
    fn statusline_reports_needs_you_over_working() {
        let events = vec![
            ev(ledger::TYPE_UNIT_STARTED, r#"{"id":"u-esc"}"#),
            ev(ledger::TYPE_UNIT_ESCALATED, r#"{"id":"u-esc"}"#),
        ];
        let run = ledger::project(&events).unwrap();
        let blockers = blocker::from_state(&run, &events, 3);
        let d = dock(&run, &events);
        assert!(!blockers.is_empty(), "an escalated unit is also a blocker");
        assert_eq!(statusline(&run, &blockers, &d), "- . 0/1 units . needs-you");
    }

    /// Determinism: folding the same stream twice yields byte-identical
    /// `ConsoleState`s (no `HashMap`, no clock, no randomness in the fold).
    #[test]
    fn fold_is_deterministic() {
        let events = vec![
            ev(ledger::TYPE_UNIT_STARTED, r#"{"id":"u-churn"}"#),
            ev(
                ledger::TYPE_UNIT_FAILED,
                r#"{"id":"u-churn","attempts":2,"cause":"reject"}"#,
            ),
            ev(blocker::TYPE_BUDGET_EXHAUSTED, r#"{"budget":5,"spawns":5}"#),
        ];
        assert_eq!(fold(&events, 3).unwrap(), fold(&events, 3).unwrap());
    }
}
