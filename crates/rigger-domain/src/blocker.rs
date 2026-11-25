//! The current-blocker classifier (spec 19a, unit 1): a pure read-model over the
//! run's projected state that answers, for each unfinished unit, ONE line saying why
//! it is where it is - so an operator sees a run's decisive state without grepping
//! `ps` and journals.
//!
//! This is the SINGLE authority both operator surfaces render: `rigger status`
//! (`cmd_status` in the binary) and the dashboard ([`crate::dash`]) each call
//! [`from_events`] and print the SAME [`Blocker::full_line`], so the two surfaces
//! cannot drift. It reads only existing run state (the [`ledger`] projection) - no new event
//! type, no control flow.
//!
//! The kinds are a fixed, closed set (spec 19a): `building`, `reviewing`,
//! `reject-recurrence`, `approved-not-integrated`, `escalated` and `resumed`. Determinism is
//! by construction: units are folded from a [`BTreeMap`](std::collections::BTreeMap)
//! (lexical).
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

use crate::config::{Defaults, Stage};
use crate::eventstore::Event;
use crate::ledger::{self, RunState, Status};
use crate::safety;

/// The reject-recurrence diagnose threshold (spec 69 Design: "reject-recurrence at the diagnose
/// threshold (>= 3 ...")): the count at which a failing unit stops reading as remediating on its
/// own and needs an operator's look. Fixed by the spec text, independent of the bound the
/// conductor escalates a unit at (its stage's or fan-out template's own `max_retries`, else
/// `defaults.max_retries`: a different bound, for a different purpose - when the conductor
/// escalates, not when an operator should look). It lives here, beside the `reject-recurrence`
/// kind, rather than in the store-gated `watch` module whose reject-recurrence-trend signal
/// alerts on it, so the console's always-compiled needs-you dock reads the same constant.
pub const REJECT_RECURRENCE_DIAGNOSE_THRESHOLD: u32 = 3;

/// Whether `unit` awaits an operator's finding audit: it is [`Status::Failed`] (so the loop is
/// still remediating it on its own, not escalated and not yet green), its recorded attempts are
/// at or past [`REJECT_RECURRENCE_DIAGNOSE_THRESHOLD`], and no resume grant is in effect
/// ([`ledger::Unit::resumed`] is `None`; a grant is the operator's look already taken). The one
/// test the console's needs-you dock lists a unit on and the conductor's reject-recurrence
/// attention stamp crosses into, so both surfaces tap the operator on the same failed attempt.
pub fn awaits_finding_audit(unit: &ledger::Unit) -> bool {
    unit.status == Status::Failed
        && unit.attempts >= REJECT_RECURRENCE_DIAGNOSE_THRESHOLD
        && unit.resumed.is_none()
}

/// The closed set of current-blocker kinds (spec 19a, unit 1). Each unfinished unit
/// maps to exactly one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Kind {
    /// The implementer is building this unit (grounding/red/green), on attempt `n`.
    Building { attempt: u32 },
    /// Gates are green and the unit is under review (adjudication pending).
    Reviewing,
    /// The unit failed and is remediating: this is failure `n` of the `max` bound it
    /// escalates at ([`ledger::Unit::max_retries`], stamped on its failure). `cause` is the
    /// conductor's own closed-vocabulary tag for WHY (spec 69, criterion 3: the cause wire) -
    /// `reject`, `gate:<name>`, `integrate-conflict`, `git-refused`, or `infra:<kind>` - defaulted to
    /// `"unknown"` when the folded [`ledger::Unit::cause`] is empty (a cause-less prior event).
    RejectRecurrence { n: u32, max: u32, cause: String },
    /// The review approved (on the result channel) but the unit is not yet integrated.
    ApprovedNotIntegrated,
    /// The unit gave up at the remediation bound and is awaiting a human.
    Escalated,
    /// Spec 88, criterion 3 (ESCALATION RESUMES): an operator ran `rigger resume-unit`
    /// on this unit and its grant is still in effect (no fresh escalation has retired
    /// it yet) - [`ledger::Unit::resumed`] is `Some`. Overrides whatever building/
    /// reject-recurrence kind the unit's raw status would otherwise map to, so the
    /// operator sees confirmation of their own grant rather than losing it in the
    /// ordinary remediation noise.
    Resumed { by: String, attempts_granted: u32 },
}

/// One current-blocker line: the unit `subject` it is about and its [`Kind`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Blocker {
    /// The unit id this blocker describes.
    pub subject: String,
    pub kind: Kind,
}

impl Blocker {
    /// The subject label: the unit id.
    pub fn subject(&self) -> &str {
        &self.subject
    }

    /// The kind's one-line description, without the subject prefix. Hyphens only (a
    /// gate rejects em dashes).
    pub fn line(&self) -> String {
        match &self.kind {
            Kind::Building { attempt } => format!("building (attempt {attempt})"),
            Kind::Reviewing => "reviewing (gates green, awaiting adjudication)".to_string(),
            Kind::RejectRecurrence { n, max, cause } => {
                format!("reject-recurrence #{n}/{max} ({cause})")
            }
            Kind::ApprovedNotIntegrated => {
                "approved, not yet integrated (review passed; integration pending)".to_string()
            }
            Kind::Escalated => "escalated (awaiting a human)".to_string(),
            Kind::Resumed {
                by,
                attempts_granted,
            } => format!("resumed by {by} ({attempts_granted} attempt(s) granted)"),
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
            Kind::Resumed { .. } => "resumed",
        }
    }
}

/// Resolve the effective remediation bound of a configured `defaults.max_retries`: the key, or
/// [`safety::MAX_RETRIES`] when unset (`0`) ([`defaults_max_retries`]). The
/// `reject-recurrence #n/max` line reads it only for a failure with no stamped bound, one an
/// older binary wrote: the conductor stamps every failure with the bound it escalates that unit
/// at ([`ledger::Unit::max_retries`]), which a stage's own key or a resume grant can set apart
/// from the defaults.
pub fn effective_max_retries(configured: u32) -> u32 {
    defaults_max_retries(configured).0
}

/// A `max_retries` key as set: `None` when unset (`0`), which inherits.
fn set_key(max_retries: u32) -> Option<u32> {
    (max_retries != 0).then_some(max_retries)
}

/// The bound a configured `defaults.max_retries` gives with its source: the key when set, else
/// the built-in [`safety::MAX_RETRIES`]. The one home of that fallback.
fn defaults_max_retries(configured: u32) -> (u32, RetriesSource) {
    match set_key(configured) {
        Some(default) => (default, RetriesSource::Defaults),
        None => (safety::MAX_RETRIES, RetriesSource::BuiltIn),
    }
}

/// Where a stage's effective `max_retries` came from (spec 123, THE CEILING): its own key, the
/// workflow's `defaults.max_retries`, or the built-in bound [`safety::MAX_RETRIES`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RetriesSource {
    Stage,
    Defaults,
    BuiltIn,
}

/// The effective remediation bound of `stage` (spec 123, THE CEILING, and spec 91's stage
/// override) with its source: the stage's own non-zero `max_retries`, else the non-zero
/// `defaults.max_retries`, else [`safety::MAX_RETRIES`]. No stage reads the defaults alone. The
/// conductor's per-unit bound and the spec critic's ceiling both read it, so a unit and a
/// critique escalate at one bound.
pub fn stage_max_retries(stage: Option<&Stage>, defaults: &Defaults) -> (u32, RetriesSource) {
    match stage.and_then(|st| set_key(st.max_retries)) {
        Some(own) => (own, RetriesSource::Stage),
        None => defaults_max_retries(defaults.max_retries),
    }
}

/// Classify the current blocker for each unfinished unit of a run. PURE over the projected
/// [`RunState`] and the already-resolved run-wide `max_retries` bound - no I/O, no clock. A failed unit's
/// line names the bound stamped on its latest failure ([`ledger::Unit::max_retries`], the one
/// the conductor escalates it at), and the run-wide bound only when that failure carries none.
///
/// One [`Blocker`] per unit that has NOT integrated (an integrated unit is done, not a
/// blocker), keyed by its [`Status`]:
/// - `Pending`/`Grounding`/`Red`/`Green` -> [`Kind::Building`] on attempt `attempts + 1`;
/// - `Verified` -> [`Kind::Reviewing`];
/// - `Reviewed` -> [`Kind::ApprovedNotIntegrated`];
/// - `Failed` -> [`Kind::RejectRecurrence`] `#attempts/max` (mid-remediation), `max` the
///   unit's stamped bound;
/// - `Escalated` -> [`Kind::Escalated`].
///
/// The blockers follow the [`RunState::units`] `BTreeMap`'s lexical order, so the whole list
/// is deterministic.
pub fn classify(run: &RunState, max_retries: u32) -> Vec<Blocker> {
    let mut out = Vec::new();
    for (id, u) in &run.units {
        if u.status == Status::Integrated {
            continue;
        }
        // Spec 88, criterion 3: a still-in-effect resume grant overrides the unit's
        // raw status kind (Failed while re-parked, or Grounding/Red/Green once its
        // implementer has picked the resume back up) - the operator's own action
        // takes precedence over the ordinary remediation noise until either a fresh
        // escalation retires it (`Unit::resumed` folds back to `None`, the ledger's
        // own job) or the unit integrates (excluded just above).
        if let Some(grant) = &u.resumed {
            out.push(Blocker {
                subject: id.clone(),
                kind: Kind::Resumed {
                    by: grant.by.clone(),
                    attempts_granted: grant.attempts_granted,
                },
            });
            continue;
        }
        let kind = match u.status {
            Status::Integrated => continue,
            Status::Escalated => Kind::Escalated,
            Status::Failed => Kind::RejectRecurrence {
                n: u.attempts,
                max: set_key(u.max_retries).unwrap_or(max_retries),
                cause: if u.cause.is_empty() {
                    ledger::CAUSE_UNKNOWN.to_string()
                } else {
                    u.cause.clone()
                },
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

/// The convenience entry both operator surfaces call: project the run and [`classify`] - so `rigger status` and the dashboard render the SAME lines
/// from ONE call. `configured_max_retries` is `defaults.max_retries` (unresolved);
/// [`effective_max_retries`] applies the `0 -> MAX_RETRIES` fallback, and a failed unit's
/// stamped bound takes precedence over it ([`classify`]).
pub fn from_events(
    events: &[Event],
    configured_max_retries: u32,
) -> Result<Vec<Blocker>, serde_json::Error> {
    let run = ledger::project(events)?;
    Ok(classify(
        &run,
        effective_max_retries(configured_max_retries),
    ))
}

/// A helper for callers that already hold a projected [`RunState`] (the dashboard folds
/// one for its unit view): classify from that state without re-projecting.
pub fn from_state(run: &RunState, configured_max_retries: u32) -> Vec<Blocker> {
    classify(run, effective_max_retries(configured_max_retries))
}

/// Render a classified list to its shared one-line strings (what each surface prints).
pub fn lines(blockers: &[Blocker]) -> Vec<String> {
    blockers.iter().map(Blocker::full_line).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::test_support::ev;
    use crate::test_support::positioned;

    #[test]
    fn effective_max_retries_falls_back_when_unset() {
        assert_eq!(effective_max_retries(0), safety::MAX_RETRIES);
        assert_eq!(effective_max_retries(6), 6);
    }

    /// Spec 123, THE CEILING: a stage's effective `max_retries` is its own non-zero key, else
    /// `defaults.max_retries` when non-zero, else the built-in bound, each with the source it came
    /// from.
    #[test]
    fn a_stages_max_retries_is_its_own_else_the_defaults_else_the_built_in_bound() {
        let stage = |max_retries| Stage {
            max_retries,
            ..Stage::default()
        };
        let defaults = |max_retries| Defaults {
            max_retries,
            ..Defaults::default()
        };
        let cases = [
            (
                "a stage's own key over a larger default",
                Some(stage(5)),
                defaults(4),
                (5, RetriesSource::Stage),
            ),
            (
                "a stage's own key under a larger default",
                Some(stage(1)),
                defaults(4),
                (1, RetriesSource::Stage),
            ),
            (
                "a stage's own key under unset defaults",
                Some(stage(5)),
                defaults(0),
                (5, RetriesSource::Stage),
            ),
            (
                "an unset stage inherits the defaults' key",
                Some(stage(0)),
                defaults(4),
                (4, RetriesSource::Defaults),
            ),
            (
                "no stage reads the defaults' key",
                None,
                defaults(4),
                (4, RetriesSource::Defaults),
            ),
            (
                "an unset stage under unset defaults takes the built-in bound",
                Some(stage(0)),
                defaults(0),
                (3, RetriesSource::BuiltIn),
            ),
            (
                "no stage under unset defaults takes the built-in bound",
                None,
                defaults(0),
                (3, RetriesSource::BuiltIn),
            ),
        ];
        for (why, stage, defaults, want) in cases {
            assert_eq!(stage_max_retries(stage.as_ref(), &defaults), want, "{why}");
        }
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
        // Lexical unit order; no integrated unit.
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
                    Kind::RejectRecurrence {
                        n: 2,
                        max: 5,
                        cause: "unknown".to_string()
                    }
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

    /// A unit that started and failed once per `failures` payload (in order), under a
    /// configured bound of `max_retries`, renders exactly `line`.
    fn assert_reject_recurrence_line(failures: &[&str], max_retries: u32, line: &str) {
        let events = positioned(
            failures
                .iter()
                .flat_map(|failure| {
                    [
                        ev(ledger::TYPE_UNIT_STARTED, r#"{"id":"u"}"#),
                        ev(ledger::TYPE_UNIT_FAILED, failure),
                    ]
                })
                .collect(),
        );
        let blockers = from_events(&events, max_retries).unwrap();
        assert_eq!(blockers[0].full_line(), line);
    }

    crate::test_cases! {
        // configured 0 -> resolves to MAX_RETRIES (3). No cause on the event (a
        // cause-less prior event, spec 69 criterion 3) reads as "unknown", never the
        // old generic "remediating" wording.
        reject_recurrence_line_shows_n_over_max: assert_reject_recurrence_line(
            &[r#"{"id":"u","attempts":1}"#],
            0,
            "u: reject-recurrence #1/3 (unknown)",
        );
        // spec 69, criterion 3 (the cause wire): "status blocker lines carry it" - the
        // line names the conductor's own recorded cause instead of the generic
        // "remediating" wording.
        reject_recurrence_line_carries_the_recorded_cause: assert_reject_recurrence_line(
            &[r#"{"id":"u","attempts":4,"cause":"integrate-conflict"}"#],
            6,
            "u: reject-recurrence #4/6 (integrate-conflict)",
        );
        // A changed cause is progress (spec 69 Design): the line names the MOST
        // RECENT failure's cause, not the first.
        reject_recurrence_line_carries_the_latest_of_several_causes: assert_reject_recurrence_line(
            &[
                r#"{"id":"u","attempts":1,"cause":"gate:fmt"}"#,
                r#"{"id":"u","attempts":2,"cause":"reject"}"#,
            ],
            6,
            "u: reject-recurrence #2/6 (reject)",
        );
        // The bound stamped on the failure (its stage's own 2) is the unit's, over the
        // run-wide 6 a surface passes; the rows above, whose failures carry none (an older
        // binary's), read the run-wide bound.
        reject_recurrence_line_names_the_units_stamped_bound: assert_reject_recurrence_line(
            &[r#"{"id":"u","attempts":1,"cause":"gate:fmt","max_retries":2}"#],
            6,
            "u: reject-recurrence #1/2 (gate:fmt)",
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
    fn a_resumed_units_grant_is_named_and_overrides_the_raw_status_kind() {
        // Spec 88, criterion 3: `rigger status` names the grant, even while the
        // resumed unit's raw status (Failed, re-parked-not-yet-picked-up) would
        // otherwise read as reject-recurrence.
        let events = positioned(vec![
            ev(ledger::TYPE_UNIT_STARTED, r#"{"id":"u"}"#),
            ev(ledger::TYPE_UNIT_FAILED, r#"{"id":"u","attempts":3}"#),
            ev(ledger::TYPE_UNIT_ESCALATED, r#"{"id":"u"}"#),
            ev(
                ledger::TYPE_UNIT_RESUMED,
                r#"{"unit":"u","attempts_granted":2,"by":"operator"}"#,
            ),
        ]);
        let blockers = from_events(&events, 3).unwrap();
        assert_eq!(blockers.len(), 1);
        assert_eq!(
            blockers[0].kind,
            Kind::Resumed {
                by: "operator".to_string(),
                attempts_granted: 2,
            }
        );
        assert_eq!(
            blockers[0].full_line(),
            "u: resumed by operator (2 attempt(s) granted)"
        );
    }

    #[test]
    fn a_re_escalation_after_a_resume_reads_as_plain_escalated_again() {
        // "a second escalation after the grant is final again until the next
        // resume": once the widened bound is spent and the unit escalates a second
        // time, the line must NOT still say "resumed" - it reads exactly like a
        // never-resumed escalation.
        let events = positioned(vec![
            ev(ledger::TYPE_UNIT_STARTED, r#"{"id":"u"}"#),
            ev(ledger::TYPE_UNIT_FAILED, r#"{"id":"u","attempts":3}"#),
            ev(ledger::TYPE_UNIT_ESCALATED, r#"{"id":"u"}"#),
            ev(
                ledger::TYPE_UNIT_RESUMED,
                r#"{"unit":"u","attempts_granted":1,"by":"operator"}"#,
            ),
            ev(ledger::TYPE_UNIT_FAILED, r#"{"id":"u","attempts":4}"#),
            ev(ledger::TYPE_UNIT_ESCALATED, r#"{"id":"u"}"#),
        ]);
        let blockers = from_events(&events, 4).unwrap();
        assert_eq!(blockers.len(), 1);
        assert_eq!(blockers[0].kind, Kind::Escalated);
        assert_eq!(blockers[0].full_line(), "u: escalated (awaiting a human)");
    }

    /// A log written while the spawn budget existed still carries its `BudgetExhausted` and
    /// `TaskAborted` records: they fold to no blocker, and the unit reads as it stands.
    #[test]
    fn a_retired_budget_halt_in_an_old_log_folds_to_no_blocker() {
        let events = positioned(vec![
            ev(ledger::TYPE_UNIT_STARTED, r#"{"id":"u"}"#),
            ev(
                ledger::TYPE_BUDGET_EXHAUSTED,
                r#"{"budget":200,"spawns":200}"#,
            ),
            ev(
                ledger::TYPE_TASK_ABORTED,
                r#"{"reason":"spawn budget exhausted"}"#,
            ),
        ]);
        assert_eq!(
            lines(&from_events(&events, 3).unwrap()),
            ["u: building (attempt 1)"]
        );
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
        assert_eq!(from_state(&run, 3), from_events(&events, 3).unwrap());
    }
}
