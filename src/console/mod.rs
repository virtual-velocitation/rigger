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

/// The code lens's map engine (spec 84, criterion 1: "THE MAP LANDS LABELLED" - districts,
/// semantic zoom, label placement). `core`, like this module itself - see [`map`]'s own doc.
pub mod map;

use std::collections::BTreeMap;

use crate::blocker::{self, Blocker};
use crate::eventstore::{Event, Position};
use crate::ledger::{self, AttentionEntry, RunState, Status};
use crate::spawn;

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

/// The conventional unit id this codebase's own injected DAG-critique / plan-critique
/// gate carries (mirrors `conductor::critique_gate_name`'s ROLE-based recognition,
/// which `console` cannot reach - `conductor` is not a `core` module, only its own
/// hardcoded default stage name is stable across this codebase's runs). A workflow
/// that names its own critique gate differently is out of scope for the `"plan"` mark:
/// its round simply reads as an ordinary per-unit `"verdict"` mark instead, never a
/// crash or a misattributed mark.
const PLAN_CRITIQUE_UNIT: &str = "plan-critique";

/// One mark on the scrubber's track (spec 94 c3 Design, "6.9 The scrubber, replay and
/// the palette": "its marks are verdicts (red reject, green approve), integrations
/// (accent, taller) and the plan approval, each with a tooltip"). `kind` is
/// `"verdict"` (a per-unit review round's adjudicator disposition), `"integration"`
/// (a unit landed), or `"plan"` (the SAME adjudicator-disposition shape as
/// `"verdict"`, but for [`PLAN_CRITIQUE_UNIT`] - the plan-critique's own round, kept
/// as its own kind so a page renders it with the plan-review colour rather than an
/// ordinary unit's).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct ScrubMark {
    pub position: Position,
    pub kind: &'static str,
    pub color: &'static str,
    pub tall: bool,
    pub tooltip: String,
}

/// One wall-clock hour boundary the run's recorded events crossed (Design, "6.9": "its
/// ticks are wall-clock hours"), pinned to the position of the first event AT OR AFTER
/// that boundary - never the boundary's own (unrecorded) position, so dragging to a
/// tick always lands on a position the fold can actually render.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct ScrubTick {
    pub position: Position,
    pub label: String,
}

/// The scrubber's whole track for a recorded stream (spec 94 c3: `scrub_track`): the
/// marks and the hour ticks, both already position-ordered - a page renders this list
/// directly, with no fold or sort of its own.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize)]
pub struct ScrubTrack {
    pub marks: Vec<ScrubMark>,
    pub ticks: Vec<ScrubTick>,
}

/// One adjudicator `SpawnResult`'s mark, or `None` for every other event: not a
/// `SpawnResult`, not an adjudicator (`SpawnResult::is_adjudicator`'s own self-gate -
/// only an adjudicator disposes, mirroring `SpawnResult::adjudication`), or a verdict
/// line whose literal this codebase's vocabulary does not recognize - never a guessed
/// colour for an unrecognized token.
fn verdict_mark(e: &Event) -> Option<ScrubMark> {
    if e.type_ != spawn::TYPE_SPAWN_RESULT {
        return None;
    }
    let res = spawn::SpawnResult::from_event(e).ok()?;
    if !res.is_adjudicator() {
        return None;
    }
    let verdict = res.adjudication()?.verdict?;
    let unit = spawn::unit_of(&res.id).unwrap_or("").to_string();
    let color = match verdict.as_str() {
        "approve" => "green",
        "reject" => "red",
        _ => return None,
    };
    if unit == PLAN_CRITIQUE_UNIT {
        Some(ScrubMark {
            position: e.position,
            kind: "plan",
            color: "review",
            tall: false,
            tooltip: format!("plan critique: {verdict}"),
        })
    } else {
        Some(ScrubMark {
            position: e.position,
            kind: "verdict",
            color,
            tall: false,
            tooltip: format!("{unit}: {verdict}"),
        })
    }
}

/// A `UnitIntegrated` event's mark, or `None` for every other event / a malformed one
/// (never fabricating an id for a body that carries none).
fn integration_mark(e: &Event) -> Option<ScrubMark> {
    if e.type_ != ledger::TYPE_UNIT_INTEGRATED {
        return None;
    }
    let id = serde_json::from_slice::<serde_json::Value>(&e.data)
        .ok()?
        .get("id")?
        .as_str()?
        .to_string();
    Some(ScrubMark {
        position: e.position,
        kind: "integration",
        color: "accent",
        tall: true,
        tooltip: format!("{id}: integrated"),
    })
}

/// [`Event::recorded_at`] as Unix seconds, `0` on a pre-epoch time (never produced by a
/// real store, but never trusted blindly) - the same degrade-not-panic conversion
/// [`crate::dash::unix_seconds`] keeps for the identical field, kept as this module's
/// own copy because `dash` is a `store`-gated module `console` (a `core` one) cannot
/// depend on.
fn unix_seconds(t: std::time::SystemTime) -> u64 {
    t.duration_since(std::time::SystemTime::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Wall-clock hour ticks: one per HOUR BOUNDARY the run's events cross, pinned to the
/// position of the first event at/after that boundary. Walks `events` in POSITION
/// order regardless of input order (pure, no dependence on caller discipline). An
/// event whose `recorded_at` converts to `0` (the sentinel [`Event::mint_time`]'s own
/// core-lane epoch fallback returns when a caller never set a real time - see its own
/// doc) contributes no boundary crossing, so a wasm-decoded stream carrying no wire
/// timestamp degrades to no ticks rather than a wall of spurious ones piled at the
/// epoch. The very first REAL timestamp seen only seeds the baseline hour - it never
/// ticks itself, since it crosses nothing yet.
fn hour_ticks(events: &[Event]) -> Vec<ScrubTick> {
    const HOUR_SECS: u64 = 3600;
    const HOURS_PER_DAY: u64 = 24;
    let mut ordered: Vec<&Event> = events.iter().collect();
    ordered.sort_by_key(|e| e.position);
    let mut ticks = Vec::new();
    let mut last_hour: Option<u64> = None;
    for e in ordered {
        let secs = unix_seconds(e.recorded_at);
        if secs == 0 {
            continue;
        }
        let hour = secs / HOUR_SECS;
        if last_hour.is_some_and(|prev| hour > prev) {
            let hour_of_day = (hour % HOURS_PER_DAY) as u32;
            ticks.push(ScrubTick {
                position: e.position,
                label: format!("{hour_of_day:02}:00"),
            });
        }
        last_hour = Some(hour);
    }
    ticks
}

/// The scrubber's whole track for a recorded stream (spec 94 c3: `scrub_track`):
/// verdict/integration/plan marks plus the wall-clock hour ticks, both position-
/// ordered. Pure over `events` - no clock, no store, no process (`core`'s own purity
/// rule): every time it needs is already on [`Event::recorded_at`], never read fresh.
pub fn scrub_track(events: &[Event]) -> ScrubTrack {
    let mut marks: Vec<ScrubMark> = events
        .iter()
        .filter_map(|e| verdict_mark(e).or_else(|| integration_mark(e)))
        .collect();
    marks.sort_by_key(|m| m.position);
    ScrubTrack {
        marks,
        ticks: hour_ticks(events),
    }
}

/// One entry of the command palette (spec 94 c4, THE PALETTE): a target the page's own
/// routing already understands. `kind` names which of the five sections
/// [`palette_commands`] fills (`"view"`, `"courtroom"`, `"agent"`, `"live"`,
/// `"replay"`); `id` is the exact token the page routes on (a `data-view` slug, a unit
/// id, or a spawn id) - never a second id scheme the page must translate; `label` is
/// the rendered text a person filters against as they type.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct PaletteCommand {
    pub kind: &'static str,
    pub id: String,
    pub label: String,
}

/// The seven views the tab bar renders, left to right (Design §1's own row; §6.9: "the
/// digits 0-6 switch views") - the SAME `data-view` slugs `src/console.html`'s tab
/// markup already carries, so a palette pick needs no second slug table kept in step by
/// hand.
pub const VIEWS: [(&str, &str); 7] = [
    ("fleet", "Fleet"),
    ("theater", "Theater"),
    ("agents", "Agents"),
    ("court", "Courtroom"),
    ("map", "Knowledge"),
    ("plan", "Plan"),
    ("brief", "Briefing"),
];

/// The command palette's whole entry list for a recorded stream (spec 94 c4:
/// `palette_commands`): the seven views, every unit's courtroom (one per id already in
/// `units` - [`fold`]'s own [`ConsoleState::units`], never a second unit enumeration),
/// every agent (one per distinct, well-formed spawn id a [`spawn::TYPE_SPAWN_REQUESTED`]
/// event names), jump to live, and replay from the start (both left for the page to
/// execute with its OWN cursor/replay functions - this module invents no second copy of
/// those). Pure over `events` and `units` - no clock, no store, no process (`core`'s own
/// purity rule) - and, like [`scrub_track`], NEVER FAILS: the agent section folds through
/// [`spawn::recorded_lenient`] - not a second, inlined copy of its filter-parse loop -
/// which skips (never propagates) any `TYPE_SPAWN_REQUESTED` body that doesn't
/// deserialize, the SAME degrade-not-fail pattern `scrub_track`'s own `filter_map` and
/// `dash.rs::console_snapshot_json`'s `if let Ok(...)` already use for this identical
/// older-run scenario - CONSTRAINTS WALK: "An older run lacking a field - the fold
/// renders the blank, never fails." [`spawn::recorded`] stays the right tool for its
/// OTHER callers (the replay driver's park-or-replay decision, the budget breaker's hard
/// count), where a malformed spawn is a genuine invariant violation; [`spawn::recorded_lenient`]
/// is the sibling this read-only display caller needs instead, so one malformed or
/// older-run spawn entry loses only its own agent row, never the views/courtroom/live/
/// replay entries that don't depend on it at all.
///
/// CURSOR CONTRACT: this function trusts its caller to have already scoped `events` and
/// `units` to the SAME position - it reconciles nothing itself, matching [`scrub_track`]'s
/// own "no clock, no store" purity. `crates/console-core`'s `op_palette_commands` is the
/// one caller today; it takes an explicit `position` argument (spec 94 c4's own REQUIRED
/// FIX, adj-u94c4-r4-verdict-reject-push-while-scrubbed-desync - an earlier round scoped
/// both through an AMBIENT session cursor instead, which a `fold_push`/`fold_reset`
/// legitimately (and silently) reset to live between a caller's scrub and its later read)
/// and independently re-derives both `events` and `units` from that ONE position via a
/// fresh `console::fold` call, so the courtroom section and the agent section always agree
/// on "the state of the run after N events" (spec 94 Goal) - a mismatched pair here is a
/// caller bug, not a case this function degrades on the way it degrades a malformed spawn.
pub fn palette_commands(events: &[Event], units: &UnitStatuses) -> Vec<PaletteCommand> {
    let mut commands: Vec<PaletteCommand> = VIEWS
        .iter()
        .map(|(id, label)| PaletteCommand {
            kind: "view",
            id: (*id).to_string(),
            label: (*label).to_string(),
        })
        .collect();

    for id in units.keys() {
        commands.push(PaletteCommand {
            kind: "courtroom",
            id: id.clone(),
            label: format!("Courtroom: {id}"),
        });
    }

    for id in spawn::recorded_lenient(events).into_keys() {
        commands.push(PaletteCommand {
            kind: "agent",
            id: id.clone(),
            label: format!("Agent: {id}"),
        });
    }

    commands.push(PaletteCommand {
        kind: "live",
        id: "live".to_string(),
        label: "Jump to live".to_string(),
    });
    commands.push(PaletteCommand {
        kind: "replay",
        id: "replay".to_string(),
        label: "Replay from start".to_string(),
    });

    commands
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ev(type_: &str, json: &str) -> Event {
        Event::new(type_, json.as_bytes().to_vec())
    }

    /// An adjudicator `SpawnResult` event for `id`, its verdict line `output`, at `position`.
    fn adjudicator_result(id: &str, output: &str, position: Position) -> Event {
        let mut e = spawn::SpawnResult::ok(id, output).to_event().unwrap();
        e.position = position;
        e
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

    /// SCRUB_TRACK, integration marks (spec 94 c3 Design, "6.9"): a `UnitIntegrated` event
    /// is an "accent, taller" mark naming the landed unit - mutation-discriminating (dropping
    /// the `tall`/`color` literals, or the id, changes this assertion).
    #[test]
    fn scrub_track_marks_an_integration_accent_and_tall() {
        let mut e0 = ev(
            ledger::TYPE_UNIT_INTEGRATED,
            r#"{"id":"u1","commit":"abc"}"#,
        );
        e0.position = 5;
        let track = scrub_track(&[e0]);
        assert_eq!(track.marks.len(), 1, "{:?}", track.marks);
        let m = &track.marks[0];
        assert_eq!(m.position, 5);
        assert_eq!(m.kind, "integration");
        assert_eq!(m.color, "accent");
        assert!(m.tall, "an integration mark is taller than a verdict");
        assert_eq!(m.tooltip, "u1: integrated");
    }

    /// SCRUB_TRACK, verdict marks: a per-unit adjudicator APPROVE is green, a REJECT is red -
    /// the exact "verdicts (red reject, green approve)" wording, read from the SAME
    /// `Adjudication::verdict` a review-quality/finding-expiry consumer already reads (never a
    /// second, re-derived approve/reject classification).
    #[test]
    fn scrub_track_marks_unit_verdicts_red_and_green() {
        let events = vec![
            adjudicator_result(
                "u1/adjudicator#0",
                r#"{"verdict":"reject","cause":"genuine-defect"}"#,
                3,
            ),
            adjudicator_result("u1/adjudicator#1", r#"{"verdict":"approve"}"#, 9),
        ];
        let track = scrub_track(&events);
        assert_eq!(track.marks.len(), 2, "{:?}", track.marks);
        assert_eq!(track.marks[0].position, 3);
        assert_eq!(track.marks[0].kind, "verdict");
        assert_eq!(track.marks[0].color, "red");
        assert!(!track.marks[0].tall);
        assert_eq!(track.marks[0].tooltip, "u1: reject");
        assert_eq!(track.marks[1].position, 9);
        assert_eq!(track.marks[1].kind, "verdict");
        assert_eq!(track.marks[1].color, "green");
        assert_eq!(track.marks[1].tooltip, "u1: approve");
    }

    /// SCRUB_TRACK, the plan approval: an adjudicator result for the `plan-critique` unit is
    /// its OWN mark kind (`"plan"`), distinct from an ordinary per-unit `"verdict"` mark even
    /// though it rides the identical event shape - proven by mixing one of each in one stream.
    #[test]
    fn scrub_track_marks_the_plan_critique_approval_distinctly() {
        let events = vec![
            adjudicator_result("plan-critique/adjudicator#0", r#"{"verdict":"approve"}"#, 1),
            adjudicator_result("u1/adjudicator#0", r#"{"verdict":"approve"}"#, 4),
        ];
        let track = scrub_track(&events);
        assert_eq!(track.marks.len(), 2, "{:?}", track.marks);
        assert_eq!(track.marks[0].kind, "plan");
        assert_eq!(track.marks[0].tooltip, "plan critique: approve");
        assert_eq!(track.marks[1].kind, "verdict");
    }

    /// SCRUB_TRACK never marks a non-adjudicator result even when its output is
    /// verdict-shaped (a lens echoing the vocabulary) - only an adjudicator disposes,
    /// mirroring `SpawnResult::adjudication`'s own self-gate.
    #[test]
    fn scrub_track_ignores_a_non_adjudicator_result() {
        let events = vec![adjudicator_result(
            "u1/sdet#0",
            r#"{"verdict":"approve"}"#,
            2,
        )];
        let track = scrub_track(&events);
        assert!(track.marks.is_empty(), "{:?}", track.marks);
    }

    /// SCRUB_TRACK, the old-contract sentinel: an adjudicator result whose verdict line
    /// carries `upheld`/`discarded` but NO `verdict` key at all (`Adjudication::verdict`'s
    /// own doc comment: "an old-contract line naming only upheld/discarded" - a run recorded
    /// before this criterion existed) parses to `Some(Adjudication)` with `verdict: None`,
    /// not `None` outright - a DIFFERENT sentinel than `scrub_track_ignores_a_non_adjudicator_result`
    /// (non-adjudicator) or the "no verdict line at all" case `adjudication()`'s own test
    /// covers. `verdict_mark` must degrade to no mark here too, never a guessed colour for
    /// the CONSTRAINTS WALK's "an older run lacking a field - the fold renders the blank,
    /// never fails" - mutation-discriminating against a permissive default match arm (e.g.
    /// `_ => "gray"` in place of `_ => return None`), which this test would catch but an
    /// empty-string-verdict probe could not (an empty string already falls through the
    /// existing match's `_` arm harmlessly either way).
    #[test]
    fn scrub_track_marks_nothing_for_an_old_contract_line_with_no_verdict_key() {
        let events = vec![adjudicator_result(
            "u1/adjudicator#0",
            r#"{"upheld":["f1"],"discarded":[]}"#,
            6,
        )];
        // Confirm the test's own premise against the real parser, not just the mark logic:
        // this line DOES parse to Some(Adjudication) with verdict: None (the exact shape
        // verdict_mark's `?` chain must degrade on), never the "no verdict line" None case.
        let res = spawn::SpawnResult::from_event(&events[0]).expect("parses as a SpawnResult");
        let adj = res
            .adjudication()
            .expect("upheld/discarded alone still parses as a verdict-shaped line");
        assert_eq!(adj.verdict, None, "test setup: {adj:?}");

        let track = scrub_track(&events);
        assert!(
            track.marks.is_empty(),
            "an old-contract line with no verdict key must yield no mark, never a guessed \
             colour: {:?}",
            track.marks
        );
    }

    /// SCRUB_TRACK, hour ticks: consecutive events whose `recorded_at` crosses an hour
    /// boundary produce one tick per crossing, pinned to the position of the first event
    /// AT OR AFTER that boundary, labelled by the UTC hour - never a tick at the very first
    /// event (which only seeds the baseline, crossing nothing yet).
    #[test]
    fn scrub_track_ticks_one_per_hour_boundary_crossed() {
        use std::time::{Duration, SystemTime};
        let based = |secs: u64, pos: Position| {
            let mut e = ev(ledger::TYPE_UNIT_STARTED, r#"{"id":"u1"}"#);
            e.position = pos;
            e.recorded_at = SystemTime::UNIX_EPOCH + Duration::from_secs(secs);
            e
        };
        let events = vec![
            based(0, 1),        // 00:00 - baseline, no tick
            based(30 * 60, 2),  // 00:30 - same hour, no tick
            based(70 * 60, 3),  // 01:10 - crosses into hour 1: tick at position 3
            based(3 * 3600, 4), // 03:00 - crosses straight through hour 2 into hour 3
        ];
        let track = scrub_track(&events);
        assert_eq!(
            track.ticks,
            vec![
                ScrubTick {
                    position: 3,
                    label: "01:00".to_string()
                },
                ScrubTick {
                    position: 4,
                    label: "03:00".to_string()
                },
            ],
            "{:?}",
            track.ticks
        );
    }

    /// SCRUB_TRACK, hour ticks, the `>` vs `>=` boundary: once a REAL (non-epoch-sentinel)
    /// timestamp has seeded `last_hour`, a SECOND event still within that SAME hour must
    /// tick NEVER - only a STRICTLY greater hour crosses a boundary. The prior test's own
    /// same-hour event (`based(30 * 60, 2)`) cannot pin this: it is the run's very FIRST
    /// real timestamp, so `last_hour` is still `None` entering that check and the
    /// `is_some_and` short-circuits to `false` regardless of `>` vs `>=` - it exercises
    /// only the epoch-sentinel `continue` above, never this comparison. Here the baseline
    /// event carries a genuine non-zero timestamp, so the second, same-hour event actually
    /// reaches the `>`/`>=` comparison with `last_hour` already `Some(_)`.
    #[test]
    fn hour_ticks_never_double_ticks_a_second_event_within_the_seeded_hour() {
        use std::time::{Duration, SystemTime};
        let based = |secs: u64, pos: Position| {
            let mut e = ev(ledger::TYPE_UNIT_STARTED, r#"{"id":"u1"}"#);
            e.position = pos;
            e.recorded_at = SystemTime::UNIX_EPOCH + Duration::from_secs(secs);
            e
        };
        let events = vec![
            based(60, 1),   // 00:01 - the first REAL timestamp: seeds hour 0, no tick
            based(1800, 2), // 00:30 - still hour 0: no tick (pins `>`, not `>=`)
            based(3660, 3), // 01:01 - crosses into hour 1: tick at position 3
        ];
        let track = scrub_track(&events);
        assert_eq!(
            track.ticks,
            vec![ScrubTick {
                position: 3,
                label: "01:00".to_string()
            }],
            "a same-hour event right after the seeded baseline must never tick: {:?}",
            track.ticks
        );
    }

    /// An event with the sentinel `recorded_at = UNIX_EPOCH` (`Event::mint_time`'s own
    /// core-lane fallback when a caller never set a real time - see its doc) contributes no
    /// boundary crossing: a wasm-decoded stream with no wire timestamp degrades to no ticks,
    /// never a wall of spurious ones all piled at the epoch.
    #[test]
    fn scrub_track_ticks_are_silent_on_events_with_no_real_timestamp() {
        let events = vec![ev(ledger::TYPE_UNIT_STARTED, r#"{"id":"u1"}"#)];
        let track = scrub_track(&events);
        assert!(track.ticks.is_empty(), "{:?}", track.ticks);
    }

    /// SCRUB_TRACK's marks are position-ordered regardless of the input slice's own order -
    /// a page renders this list directly, with no sort of its own.
    #[test]
    fn scrub_track_marks_are_position_ordered_regardless_of_input_order() {
        let mut later = ev(ledger::TYPE_UNIT_INTEGRATED, r#"{"id":"u2","commit":"d"}"#);
        later.position = 8;
        let mut earlier = ev(ledger::TYPE_UNIT_INTEGRATED, r#"{"id":"u1","commit":"c"}"#);
        earlier.position = 2;
        let track = scrub_track(&[later, earlier]);
        assert_eq!(
            track.marks.iter().map(|m| m.position).collect::<Vec<_>>(),
            vec![2, 8]
        );
    }

    /// A `SpawnRequested` event for `unit`/`role`#`attempt` at `position` - built through
    /// [`spawn::SpawnRequest`]'s own `to_event`, never a hand-assembled JSON literal, so
    /// these tests exercise the exact wire shape `spawn::recorded` reads in production.
    fn spawn_requested(unit: &str, role: &str, attempt: u32, position: Position) -> Event {
        let mut e = spawn::SpawnRequest::new(unit, "implement", role, attempt, "do the thing")
            .to_event()
            .unwrap();
        e.position = position;
        e
    }

    /// PALETTE_COMMANDS, views: the seven views, in the tab bar's own left-to-right
    /// order, for even a completely empty session - a palette must still let a person
    /// jump to a view (or live/replay) when no run has been folded yet.
    #[test]
    fn palette_commands_lists_the_seven_views_in_tab_order() {
        let commands = palette_commands(&[], &UnitStatuses::new());
        let views: Vec<(&str, &str)> = commands
            .iter()
            .filter(|c| c.kind == "view")
            .map(|c| (c.id.as_str(), c.label.as_str()))
            .collect();
        assert_eq!(views, VIEWS.to_vec(), "{commands:?}");
    }

    /// PALETTE_COMMANDS, courtroom: one entry per unit id already in `units` - the SAME
    /// map [`fold`] produces, never a second unit enumeration - labelled `Courtroom:
    /// <id>` so it reads and filters the same way the mock's own entries do.
    #[test]
    fn palette_commands_lists_every_units_courtroom() {
        let mut units = UnitStatuses::new();
        units.insert("u90c1".to_string(), "integrated");
        units.insert("u90c2".to_string(), "reviewed");
        let commands = palette_commands(&[], &units);
        let courtrooms: Vec<(&str, &str)> = commands
            .iter()
            .filter(|c| c.kind == "courtroom")
            .map(|c| (c.id.as_str(), c.label.as_str()))
            .collect();
        assert_eq!(
            courtrooms,
            vec![("u90c1", "Courtroom: u90c1"), ("u90c2", "Courtroom: u90c2"),],
            "{commands:?}"
        );
    }

    /// PALETTE_COMMANDS, agents: one entry per DISTINCT spawn id a recorded
    /// `SpawnRequested` names - proven with two units' worth of spawns AND a repeated
    /// round (the same id parked twice, exactly what a courier retry or a replayed
    /// step can do) to prove this dedups by id itself, sorted, rather than emitting a
    /// row per event.
    #[test]
    fn palette_commands_lists_every_distinct_recorded_agent() {
        let events = vec![
            spawn_requested("u90c1", "implementer", 0, 1),
            spawn_requested("u90c2", "implementer", 0, 2),
            spawn_requested("u90c1", "implementer", 0, 3), // re-parked: same id again
        ];
        let commands = palette_commands(&events, &UnitStatuses::new());
        let agents: Vec<&str> = commands
            .iter()
            .filter(|c| c.kind == "agent")
            .map(|c| c.id.as_str())
            .collect();
        assert_eq!(
            agents,
            vec!["u90c1/implementer#0", "u90c2/implementer#0"],
            "{commands:?}"
        );
        let labels: Vec<&str> = commands
            .iter()
            .filter(|c| c.kind == "agent")
            .map(|c| c.label.as_str())
            .collect();
        assert_eq!(
            labels,
            vec!["Agent: u90c1/implementer#0", "Agent: u90c2/implementer#0"],
            "{commands:?}"
        );
    }

    /// PALETTE_COMMANDS, live and replay: exactly one of each, present even for a
    /// completely empty session, and last in the list (after every view/courtroom/
    /// agent entry) - the page renders them, it never invents them.
    #[test]
    fn palette_commands_includes_jump_to_live_and_replay_from_start() {
        let commands = palette_commands(&[], &UnitStatuses::new());
        let last_two: Vec<(&str, &str)> = commands
            .iter()
            .rev()
            .take(2)
            .rev()
            .map(|c| (c.kind, c.label.as_str()))
            .collect();
        assert_eq!(
            last_two,
            vec![("live", "Jump to live"), ("replay", "Replay from start")],
            "{commands:?}"
        );
    }

    /// PALETTE_COMMANDS degrades, never fails, on a malformed `SpawnRequested` (spec 94's
    /// own CONSTRAINTS WALK: "An older run lacking a field - the fold renders the blank,
    /// never fails") - the bad entry loses only its own agent row; the views/live/replay
    /// sections it doesn't depend on still answer in full.
    #[test]
    fn palette_commands_omits_only_a_malformed_spawn_requested_entry() {
        let bad = Event::new(spawn::TYPE_SPAWN_REQUESTED, b"not json".to_vec());
        let commands = palette_commands(&[bad], &UnitStatuses::new());
        assert!(!commands.is_empty(), "{commands:?}");
        assert!(
            commands.iter().any(|c| c.kind == "view" && c.id == "fleet"),
            "{commands:?}"
        );
        assert!(commands.iter().any(|c| c.kind == "live"), "{commands:?}");
        assert!(commands.iter().any(|c| c.kind == "replay"), "{commands:?}");
        assert!(
            !commands.iter().any(|c| c.kind == "agent"),
            "a malformed spawn must never produce an agent row: {commands:?}"
        );
    }

    /// PALETTE_COMMANDS, mixed log: a valid spawn survives alongside a malformed one in
    /// the SAME event log - proving the skip is per-event, not an all-or-nothing guard
    /// over the whole stream.
    #[test]
    fn palette_commands_keeps_a_valid_spawn_alongside_a_malformed_one() {
        let events = vec![
            spawn_requested("u90c1", "implementer", 0, 1),
            Event::new(spawn::TYPE_SPAWN_REQUESTED, b"not json".to_vec()),
        ];
        let commands = palette_commands(&events, &UnitStatuses::new());
        let agents: Vec<&str> = commands
            .iter()
            .filter(|c| c.kind == "agent")
            .map(|c| c.id.as_str())
            .collect();
        assert_eq!(agents, vec!["u90c1/implementer#0"], "{commands:?}");
    }
}
