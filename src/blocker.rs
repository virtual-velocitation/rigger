//! The current-blocker classifier (spec 19a, unit 1): a pure read-model over the
//! run's projected state that answers, for each unfinished unit, ONE line saying why
//! it is where it is - so an operator sees a run's decisive state without grepping
//! `ps` and journals.
//!
//! This is the SINGLE authority both operator surfaces render: `rigger status`
//! (`cmd_status` in the binary) and the dashboard ([`crate::dash`]) each call
//! [`from_events`] and print the SAME [`Blocker::full_line`], so the two surfaces
//! cannot drift. It reads only existing run state (the [`ledger`] projection plus the
//! durable `BudgetExhausted` fact) - no new event type, no control flow.
//!
//! The kinds are a fixed, closed set (spec 19a): `building`, `reviewing`,
//! `reject-recurrence`, `approved-not-integrated`, `escalated`, and the run-level
//! `budget`. Determinism is by construction: units are folded from a
//! [`BTreeMap`](std::collections::BTreeMap) (lexical) and the run-level budget line,
//! when present, sorts FIRST.
//!
//! ## On `approved-not-integrated`
//!
//! [`Kind::ApprovedNotIntegrated`] is detected as [`ledger::Status::Reviewed`] and
//! worded as "approved, not yet integrated" - approve landed ON the result channel,
//! integration is pending. It is NOT "verdict not on result channel": the conductor
//! emits `reviewed` ONLY after reading an approve from the result channel, and the
//! genuine verdict-not-on-channel stall hard-errors at review (the fail-fast backstop)
//! so it never reaches `Reviewed`. A persistent `Reviewed`-without-`Integrated` unit
//! therefore means the approve WAS on the channel and integration has not completed -
//! the opposite of a channel stall - so the line names that state truthfully.

use serde::Deserialize;

use crate::eventstore::Event;
use crate::ledger::{self, RunState, Status};
use crate::safety;

/// The run's spawn-budget halt, folded from the durable `BudgetExhausted` fact:
/// `spent` spawns against a `cap` (`defaults.budget`). Surfaced as the run-level
/// [`Kind::Budget`] blocker.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Budget {
    pub spent: u32,
    pub cap: u32,
}

/// The closed set of current-blocker kinds (spec 19a, unit 1). Each unfinished unit
/// maps to exactly one; [`Kind::Budget`] is the single run-level condition.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Kind {
    /// The implementer is building this unit (grounding/red/green), on attempt `n`.
    Building { attempt: u32 },
    /// Gates are green and the unit is under review (adjudication pending).
    Reviewing,
    /// The unit failed and is remediating: this is failure `n` of the `max` bound
    /// before escalation (`defaults.max_retries`).
    RejectRecurrence { n: u32, max: u32 },
    /// The review approved (on the result channel) but the unit is not yet integrated.
    ApprovedNotIntegrated,
    /// The unit gave up at the remediation bound and is awaiting a human.
    Escalated,
    /// The whole run halted on the spawn budget (run-level, not a single unit).
    Budget(Budget),
}

/// One current-blocker line: the `subject` it is about (a unit id, or empty for the
/// run-level budget condition) and its [`Kind`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Blocker {
    /// The unit id this blocker describes, or empty for a run-level condition.
    pub subject: String,
    pub kind: Kind,
}

/// The subject label shown to an operator when a run-level condition has no unit.
const RUN_SUBJECT: &str = "run";

impl Blocker {
    /// The subject label: the unit id, or `run` for a run-level condition.
    pub fn subject(&self) -> &str {
        if self.subject.is_empty() {
            RUN_SUBJECT
        } else {
            &self.subject
        }
    }

    /// The kind's one-line description, without the subject prefix. Hyphens only (a
    /// gate rejects em dashes).
    pub fn line(&self) -> String {
        match &self.kind {
            Kind::Building { attempt } => format!("building (attempt {attempt})"),
            Kind::Reviewing => "reviewing (gates green, awaiting adjudication)".to_string(),
            Kind::RejectRecurrence { n, max } => {
                format!("reject-recurrence #{n}/{max} (remediating)")
            }
            Kind::ApprovedNotIntegrated => {
                "approved, not yet integrated (review passed; integration pending)".to_string()
            }
            Kind::Escalated => "escalated (awaiting a human)".to_string(),
            Kind::Budget(b) => format!(
                "budget spent {}/{} (raise defaults.budget and resume)",
                b.spent, b.cap
            ),
        }
    }

    /// The SHARED one-line render both surfaces emit: `<subject>: <line>`.
    pub fn full_line(&self) -> String {
        format!("{}: {}", self.subject(), self.line())
    }

    /// A short, stable kind tag for grouping and styling (the dashboard uses it as a CSS
    /// class suffix; it is not shown as prose).
    pub fn kind_tag(&self) -> &'static str {
        match self.kind {
            Kind::Building { .. } => "building",
            Kind::Reviewing => "reviewing",
            Kind::RejectRecurrence { .. } => "reject-recurrence",
            Kind::ApprovedNotIntegrated => "approved-not-integrated",
            Kind::Escalated => "escalated",
            Kind::Budget(_) => "budget",
        }
    }
}

/// Resolve the effective remediation bound the way the conductor does
/// ([`crate::conductor`]'s `max_retries`): `defaults.max_retries`, or
/// [`safety::MAX_RETRIES`] when unset (`0`). Single-sourced here so the
/// `reject-recurrence #n/max` line's `max` matches the bound the run actually escalates
/// at, on either surface.
pub fn effective_max_retries(configured: u32) -> u32 {
    if configured == 0 {
        safety::MAX_RETRIES
    } else {
        configured
    }
}

#[derive(Deserialize)]
struct BudgetExhausted {
    #[serde(default)]
    budget: u32,
    #[serde(default)]
    spawns: u32,
}

/// Fold the run's spawn-budget halt from the durable `BudgetExhausted` fact, or `None`
/// when the run is not currently halted on budget.
///
/// A halt is reported ONLY when the `BudgetExhausted` is the LATEST unit-lifecycle
/// event by position: a resume that raised `defaults.budget` and scheduled the stalled
/// work records later lifecycle events, so a stale `BudgetExhausted` still sitting in
/// the log does not re-report a halt the operator has since resolved. When nothing has
/// happened since the halt, it is the current blocker and is surfaced.
pub fn budget_halt(events: &[Event]) -> Option<Budget> {
    let mut budget: Option<(u64, Budget)> = None;
    let mut last_lifecycle_pos: u64 = 0;
    for e in events {
        match e.type_.as_str() {
            crate::conductor::TYPE_BUDGET_EXHAUSTED => {
                if let Ok(p) = serde_json::from_slice::<BudgetExhausted>(&e.data) {
                    budget = Some((
                        e.position,
                        Budget {
                            spent: p.spawns,
                            cap: p.budget,
                        },
                    ));
                }
            }
            ledger::TYPE_UNIT_STARTED
            | ledger::TYPE_UNIT_STATUS
            | ledger::TYPE_UNIT_FAILED
            | ledger::TYPE_UNIT_ESCALATED
            | ledger::TYPE_UNIT_INTEGRATED => {
                last_lifecycle_pos = last_lifecycle_pos.max(e.position);
            }
            _ => {}
        }
    }
    match budget {
        Some((pos, b)) if pos >= last_lifecycle_pos => Some(b),
        _ => None,
    }
}

/// Classify the current blocker for each unfinished unit of a run, plus the run-level
/// budget halt when present. PURE over the projected [`RunState`], the folded `budget`,
/// and the already-resolved `max_retries` bound - no I/O, no clock.
///
/// One [`Blocker`] per unit that has NOT integrated (an integrated unit is done, not a
/// blocker), keyed by its [`Status`]:
/// - `Pending`/`Grounding`/`Red`/`Green` -> [`Kind::Building`] on attempt `attempts + 1`;
/// - `Verified` -> [`Kind::Reviewing`];
/// - `Reviewed` -> [`Kind::ApprovedNotIntegrated`];
/// - `Failed` -> [`Kind::RejectRecurrence`] `#attempts/max` (mid-remediation);
/// - `Escalated` -> [`Kind::Escalated`].
///
/// The run-level [`Kind::Budget`] blocker, when `budget` is `Some`, sorts FIRST; the
/// per-unit blockers follow in the [`RunState::units`] `BTreeMap`'s lexical order, so
/// the whole list is deterministic.
pub fn classify(run: &RunState, budget: Option<Budget>, max_retries: u32) -> Vec<Blocker> {
    let mut out = Vec::new();
    if let Some(b) = budget {
        out.push(Blocker {
            subject: String::new(),
            kind: Kind::Budget(b),
        });
    }
    for (id, u) in &run.units {
        let kind = match u.status {
            Status::Integrated => continue,
            Status::Escalated => Kind::Escalated,
            Status::Failed => Kind::RejectRecurrence {
                n: u.attempts,
                max: max_retries,
            },
            Status::Reviewed => Kind::ApprovedNotIntegrated,
            Status::Verified => Kind::Reviewing,
            Status::Pending | Status::Grounding | Status::Red | Status::Green => Kind::Building {
                attempt: u.attempts + 1,
            },
        };
        out.push(Blocker {
            subject: id.clone(),
            kind,
        });
    }
    out
}

/// The convenience entry both operator surfaces call: project the run, fold the budget
/// halt, and [`classify`] - so `rigger status` and the dashboard render the SAME lines
/// from ONE call. `configured_max_retries` is `defaults.max_retries` (unresolved);
/// [`effective_max_retries`] applies the `0 -> MAX_RETRIES` fallback.
pub fn from_events(
    events: &[Event],
    configured_max_retries: u32,
) -> Result<Vec<Blocker>, serde_json::Error> {
    let run = ledger::project(events)?;
    Ok(classify(
        &run,
        budget_halt(events),
        effective_max_retries(configured_max_retries),
    ))
}

/// A helper for callers that already hold a projected [`RunState`] (the dashboard folds
/// one for its unit view): classify from that state plus the raw `events` (for the
/// budget fold) without re-projecting.
pub fn from_state(run: &RunState, events: &[Event], configured_max_retries: u32) -> Vec<Blocker> {
    classify(
        run,
        budget_halt(events),
        effective_max_retries(configured_max_retries),
    )
}

/// Render a classified list to its shared one-line strings (what each surface prints).
pub fn lines(blockers: &[Blocker]) -> Vec<String> {
    blockers.iter().map(Blocker::full_line).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eventstore::Event;

    fn ev(type_: &str, json: &str) -> Event {
        Event::new(type_, json.as_bytes().to_vec())
    }

    fn positioned(mut events: Vec<Event>) -> Vec<Event> {
        for (i, e) in events.iter_mut().enumerate() {
            e.position = (i + 1) as u64;
        }
        events
    }

    #[test]
    fn effective_max_retries_falls_back_when_unset() {
        assert_eq!(effective_max_retries(0), safety::MAX_RETRIES);
        assert_eq!(effective_max_retries(6), 6);
    }

    #[test]
    fn each_status_maps_to_its_kind_and_integrated_is_not_a_blocker() {
        // One run holding a unit in every non-terminal-plus-escalated state, so the
        // classifier's whole status arm is exercised in one projection. Integrated units
        // must NOT appear (they landed - they are not a blocker).
        let events = positioned(vec![
            ev(ledger::TYPE_UNIT_STARTED, r#"{"id":"u-build"}"#),
            ev(ledger::TYPE_UNIT_STARTED, r#"{"id":"u-review"}"#),
            ev(
                ledger::TYPE_UNIT_STATUS,
                r#"{"id":"u-review","status":"verified"}"#,
            ),
            ev(ledger::TYPE_UNIT_STARTED, r#"{"id":"u-appr"}"#),
            ev(
                ledger::TYPE_UNIT_STATUS,
                r#"{"id":"u-appr","status":"reviewed"}"#,
            ),
            ev(ledger::TYPE_UNIT_STARTED, r#"{"id":"u-fail"}"#),
            ev(ledger::TYPE_UNIT_FAILED, r#"{"id":"u-fail","attempts":2}"#),
            ev(ledger::TYPE_UNIT_STARTED, r#"{"id":"u-esc"}"#),
            ev(ledger::TYPE_UNIT_ESCALATED, r#"{"id":"u-esc"}"#),
            ev(ledger::TYPE_UNIT_STARTED, r#"{"id":"u-done"}"#),
            ev(
                ledger::TYPE_UNIT_INTEGRATED,
                r#"{"id":"u-done","commit":"c"}"#,
            ),
        ]);
        let blockers = from_events(&events, 5).unwrap();
        // Lexical unit order; no budget line; no integrated unit.
        let got: Vec<(String, Kind)> = blockers
            .iter()
            .map(|b| (b.subject().to_string(), b.kind.clone()))
            .collect();
        assert_eq!(
            got,
            vec![
                ("u-appr".to_string(), Kind::ApprovedNotIntegrated),
                ("u-build".to_string(), Kind::Building { attempt: 1 }),
                ("u-esc".to_string(), Kind::Escalated),
                (
                    "u-fail".to_string(),
                    Kind::RejectRecurrence { n: 2, max: 5 }
                ),
                ("u-review".to_string(), Kind::Reviewing),
            ]
        );
        // The integrated unit produced no blocker.
        assert!(!blockers.iter().any(|b| b.subject == "u-done"));
    }

    #[test]
    fn building_attempt_counts_from_the_prior_failure() {
        // A unit that failed twice and resumed (UnitStarted after the failures) is
        // grounding again on attempt 3 - attempt = attempts + 1.
        let events = positioned(vec![
            ev(ledger::TYPE_UNIT_STARTED, r#"{"id":"u"}"#),
            ev(ledger::TYPE_UNIT_FAILED, r#"{"id":"u","attempts":2}"#),
            ev(ledger::TYPE_UNIT_STARTED, r#"{"id":"u"}"#),
        ]);
        let blockers = from_events(&events, 3).unwrap();
        assert_eq!(blockers.len(), 1);
        assert_eq!(blockers[0].kind, Kind::Building { attempt: 3 });
    }

    #[test]
    fn reject_recurrence_line_shows_n_over_max() {
        let events = positioned(vec![
            ev(ledger::TYPE_UNIT_STARTED, r#"{"id":"u"}"#),
            ev(ledger::TYPE_UNIT_FAILED, r#"{"id":"u","attempts":1}"#),
        ]);
        // configured 0 -> resolves to MAX_RETRIES (3).
        let blockers = from_events(&events, 0).unwrap();
        assert_eq!(
            blockers[0].full_line(),
            "u: reject-recurrence #1/3 (remediating)"
        );
    }

    #[test]
    fn approved_not_integrated_reads_as_integration_pending_not_channel_stall() {
        // Reviewed-but-not-Integrated: approve WAS on the result channel (reviewed is
        // emitted only then), so the line names integration-pending, never a
        // verdict-not-on-channel stall (prior finding sdet-u1-approved-not-integrated).
        let events = positioned(vec![
            ev(ledger::TYPE_UNIT_STARTED, r#"{"id":"u"}"#),
            ev(
                ledger::TYPE_UNIT_STATUS,
                r#"{"id":"u","status":"reviewed"}"#,
            ),
        ]);
        let blockers = from_events(&events, 3).unwrap();
        assert_eq!(blockers[0].kind, Kind::ApprovedNotIntegrated);
        let line = blockers[0].line();
        assert!(
            line.contains("approved, not yet integrated"),
            "line: {line}"
        );
        assert!(
            !line.contains("not on result channel") && !line.contains("verdict"),
            "must not name a verdict-channel stall: {line}"
        );
    }

    #[test]
    fn budget_halt_is_a_run_level_blocker_sorted_first() {
        // A run halted on budget: a pending unit plus the terminal BudgetExhausted, which
        // is the LAST lifecycle event, so the halt is current.
        let events = positioned(vec![
            ev(ledger::TYPE_UNIT_STARTED, r#"{"id":"u"}"#),
            ev(
                crate::conductor::TYPE_BUDGET_EXHAUSTED,
                r#"{"budget":200,"spawns":200}"#,
            ),
        ]);
        let blockers = from_events(&events, 3).unwrap();
        // Budget sorts first, then the unit.
        assert_eq!(
            blockers[0].kind,
            Kind::Budget(Budget {
                spent: 200,
                cap: 200
            })
        );
        assert_eq!(blockers[0].subject(), "run");
        assert_eq!(
            blockers[0].full_line(),
            "run: budget spent 200/200 (raise defaults.budget and resume)"
        );
        assert_eq!(blockers[1].subject(), "u");
    }

    #[test]
    fn a_resume_past_the_budget_halt_suppresses_the_stale_line() {
        // The operator raised defaults.budget and resumed: lifecycle events land AFTER
        // the BudgetExhausted, so the stale halt is NOT re-reported.
        let events = positioned(vec![
            ev(ledger::TYPE_UNIT_STARTED, r#"{"id":"u"}"#),
            ev(
                crate::conductor::TYPE_BUDGET_EXHAUSTED,
                r#"{"budget":200,"spawns":200}"#,
            ),
            // Resume scheduled the work: a later lifecycle event.
            ev(ledger::TYPE_UNIT_INTEGRATED, r#"{"id":"u","commit":"c"}"#),
        ]);
        assert_eq!(budget_halt(&events), None);
        // With u integrated and no live budget halt, there is nothing to report.
        assert!(from_events(&events, 3).unwrap().is_empty());
    }

    #[test]
    fn an_empty_run_has_no_blockers() {
        assert!(from_events(&[], 3).unwrap().is_empty());
    }

    #[test]
    fn from_state_matches_from_events() {
        // The two entry points must agree, so a caller that already holds a RunState
        // renders identically to one that folds from events.
        let events = positioned(vec![
            ev(ledger::TYPE_UNIT_STARTED, r#"{"id":"u"}"#),
            ev(ledger::TYPE_UNIT_FAILED, r#"{"id":"u","attempts":1}"#),
        ]);
        let run = ledger::project(&events).unwrap();
        assert_eq!(
            from_state(&run, &events, 3),
            from_events(&events, 3).unwrap()
        );
    }
}
