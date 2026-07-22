//! The conductor's durable run state, projected from the event log: rebuildable
//! by replay, so a crashed or resumed run continues from the truth rather than
//! from conversation. Unknown event types are ignored, so the same log feeds both
//! this projection and the context graph.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::eventstore::Event;

/// Status of a unit of work, over its lifecycle
/// pending -> grounding -> red -> green -> verified -> reviewed -> integrated.
/// `failed` is TRANSIENT (the unit is mid-remediation and retries / resumes); the
/// terminal states are `integrated` (it landed) and `escalated` (it gave up at the
/// remediation bound).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    Pending,
    Grounding,
    Red,
    Green,
    Verified,
    Reviewed,
    Integrated,
    Failed,
    Escalated,
}

impl Status {
    pub fn parse(s: &str) -> Option<Status> {
        Some(match s {
            "pending" => Status::Pending,
            "grounding" => Status::Grounding,
            "red" => Status::Red,
            "green" => Status::Green,
            "verified" => Status::Verified,
            "reviewed" => Status::Reviewed,
            "integrated" => Status::Integrated,
            "failed" => Status::Failed,
            "escalated" => Status::Escalated,
            _ => return None,
        })
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Status::Pending => "pending",
            Status::Grounding => "grounding",
            Status::Red => "red",
            Status::Green => "green",
            Status::Verified => "verified",
            Status::Reviewed => "reviewed",
            Status::Integrated => "integrated",
            Status::Failed => "failed",
            Status::Escalated => "escalated",
        }
    }
}

/// Unit is one unit of work in the run.
#[derive(Clone, Debug)]
pub struct Unit {
    pub id: String,
    pub spec_criterion: String,
    pub depends_on: Vec<String>,
    pub status: Status,
    pub worktree: String,
    pub branch: String,
    /// red / green / verify / review summaries.
    pub evidence: BTreeMap<String, String>,
    pub attempts: u32,
    pub commit: String,
}

/// RunState is the projected run state.
#[derive(Default)]
pub struct RunState {
    pub units: BTreeMap<String, Unit>,
    /// Whether the run flagged a spec defect (an uncovered criterion, §4.4). Folded
    /// from the conductor's SpecDefect event; gates `fully_done`.
    pub spec_defect: bool,
    /// Whether a deferred gate failed at the run's phase boundary. Folded from the
    /// conductor's DeferredGateFailed event; gates both `done` and `fully_done` so a
    /// deferred failure can never be reported as a finished run.
    pub deferred_gate_failed: bool,
    /// The run's live HALT reason when the spawn-budget breaker stopped this run process
    /// with ready work unscheduled (Gap 13) - e.g. `"budget exhausted: 200/200 spawns"` -
    /// or `None` on a clean fixpoint. Unlike the other fields this is NOT folded from the
    /// log by [`project`]: a halt is a condition of the CURRENT run process, so
    /// `conductor::run` stamps it from its in-process breaker state after projecting. Folding
    /// the durable `BudgetExhausted` event would falsely re-report a halt the operator has
    /// since resolved by raising the budget (a resume then schedules the work and never
    /// trips), so `project` deliberately leaves this `None` and only the live run sets it.
    /// `rigger step` copies it onto its printed `Step` so the thin driver stops loudly on a
    /// halt instead of reading convergence.
    pub budget_halt: Option<String>,
    /// Unit ids currently awaiting a human: a `ManualReview` was emitted for the unit and
    /// the run does not (yet) class it terminal - the manual-review half of the
    /// action-needed inbox. Deduped and lexically ordered for a stable render. Folded by
    /// [`project`] in a second pass (see [`RunState::fold_manual_review_inbox`]) because the
    /// terminal exclusion needs the FINAL folded state: a unit that is manual-reviewed and
    /// then integrated must leave the inbox.
    pub manual_review: Vec<String>,
}

/// The ready-to-release handoff (spec 38, criterion 3): the human-facing summary the loop
/// surfaces on a DONE run so the operator can open the release PR. It is a pure projection
/// over the run state plus the run's branch/base config - NO new event and no auto-merge, so
/// a resume-by-replay re-reaches the identical summary and the loop stops at "ready to open a
/// PR" (the human owns release). Built only by [`RunState::release_ready`], which returns
/// `None` for any run that is not [`RunState::done`], so an unfinished run surfaces nothing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ReleaseReady {
    /// The run branch the approved units were integrated onto - the PR's head.
    pub run_branch: String,
    /// The release-target branch the run integrates toward - the PR's base. This is the
    /// resolved base ref with a leading `origin/` remote prefix stripped (`origin/main` ->
    /// `main`), so the surfaced PR command targets the branch and not the tracking ref.
    pub base: String,
    /// How many units landed on the run branch (every unit, since the run is done).
    pub integrated_units: usize,
    /// The exact command a human runs to open the release PR - `base..run_branch` is exactly
    /// this run's work, so the PR always applies.
    pub pr_command: String,
}

impl ReleaseReady {
    /// The status-surface render: the human-readable lines `rigger status` and the run's
    /// end-of-run summary print, naming the run branch, the base, the integrated-unit count,
    /// and the PR command. ONE render authority so every surface reads identically.
    pub fn lines(&self) -> Vec<String> {
        let plural = if self.integrated_units == 1 { "" } else { "s" };
        vec![
            format!(
                "release-ready: run branch {:?} is ready to open a PR to {:?} \
                 ({} unit{} integrated)",
                self.run_branch, self.base, self.integrated_units, plural
            ),
            format!("  {}", self.pr_command),
        ]
    }
}

// Run-event types the conductor emits (folded here into run state).
pub const TYPE_UNIT_STARTED: &str = "UnitStarted";
pub const TYPE_UNIT_STATUS: &str = "UnitStatus";
pub const TYPE_UNIT_FAILED: &str = "UnitFailed";
pub const TYPE_UNIT_ESCALATED: &str = "UnitEscalated";
pub const TYPE_UNIT_INTEGRATED: &str = "UnitIntegrated";
/// The conductor's SpecDefect event (kept in sync with `conductor::TYPE_SPEC_DEFECT`):
/// an uncovered criterion the run flagged rather than deviating around (§4.4).
pub const TYPE_SPEC_DEFECT: &str = "SpecDefect";
/// The conductor's DeferredGateFailed event (kept in sync with
/// `conductor::TYPE_DEFERRED_GATE_FAILED`): a deferred gate that failed when it ran
/// at the run's phase boundary. A deferred failure is surfaced truthfully - it gates
/// both `done` and `fully_done` so the run never reports finished with a red
/// phase-boundary gate.
pub const TYPE_DEFERRED_GATE_FAILED: &str = "DeferredGateFailed";
/// The conductor's ManualReview event (the conductor re-exports this as
/// `conductor::TYPE_MANUAL_REVIEW`, the single source of the string): a Manual-autonomy
/// gate paused its unit awaiting human review (§4.3). Folded here into
/// [`RunState::manual_review`] so the action-needed inbox is owned by the projection, not
/// re-derived by any adapter that only reads it.
pub const TYPE_MANUAL_REVIEW: &str = "ManualReview";

#[derive(Deserialize)]
struct UnitStarted {
    id: String,
    #[serde(default)]
    spec_criterion: String,
    #[serde(default)]
    needs: Vec<String>,
    #[serde(default)]
    worktree: String,
    #[serde(default)]
    branch: String,
}
#[derive(Deserialize)]
struct UnitStatus {
    id: String,
    status: String,
    #[serde(default)]
    evidence: BTreeMap<String, String>,
}
#[derive(Deserialize)]
struct UnitFailed {
    id: String,
    #[serde(default)]
    attempts: u32,
}
#[derive(Deserialize)]
struct UnitEscalated {
    id: String,
}
#[derive(Deserialize)]
struct UnitIntegrated {
    id: String,
    #[serde(default)]
    commit: String,
}
#[derive(Deserialize)]
struct ManualReview {
    /// The paused unit's id. The conductor emits `unit` and `id` both equal to the unit
    /// name; `unit` takes precedence and `id` is the fallback for any producer that carries
    /// only the generic id.
    #[serde(default)]
    unit: String,
    #[serde(default)]
    id: String,
}

impl RunState {
    pub fn new() -> Self {
        RunState::default()
    }

    fn unit(&mut self, id: &str) -> &mut Unit {
        self.units.entry(id.to_string()).or_insert_with(|| Unit {
            id: id.to_string(),
            spec_criterion: String::new(),
            depends_on: Vec::new(),
            status: Status::Pending,
            worktree: String::new(),
            branch: String::new(),
            evidence: BTreeMap::new(),
            attempts: 0,
            commit: String::new(),
        })
    }

    /// Fold one run event into the state.
    pub fn apply(&mut self, e: &Event) -> Result<(), serde_json::Error> {
        match e.type_.as_str() {
            TYPE_UNIT_STARTED => {
                let p: UnitStarted = serde_json::from_slice(&e.data)?;
                let u = self.unit(&p.id);
                u.spec_criterion = p.spec_criterion;
                u.depends_on = p.needs;
                u.worktree = p.worktree;
                u.branch = p.branch;
                u.status = Status::Grounding;
            }
            TYPE_UNIT_STATUS => {
                let p: UnitStatus = serde_json::from_slice(&e.data)?;
                let u = self.unit(&p.id);
                if let Some(s) = Status::parse(&p.status) {
                    u.status = s;
                }
                u.evidence.extend(p.evidence);
            }
            TYPE_UNIT_FAILED => {
                let p: UnitFailed = serde_json::from_slice(&e.data)?;
                let u = self.unit(&p.id);
                u.status = Status::Failed;
                u.attempts = p.attempts;
            }
            TYPE_UNIT_ESCALATED => {
                let p: UnitEscalated = serde_json::from_slice(&e.data)?;
                self.unit(&p.id).status = Status::Escalated;
            }
            TYPE_UNIT_INTEGRATED => {
                let p: UnitIntegrated = serde_json::from_slice(&e.data)?;
                let u = self.unit(&p.id);
                u.status = Status::Integrated;
                u.commit = p.commit;
            }
            TYPE_SPEC_DEFECT => {
                self.spec_defect = true;
            }
            TYPE_DEFERRED_GATE_FAILED => {
                self.deferred_gate_failed = true;
            }
            _ => {}
        }
        Ok(())
    }

    /// Done reports whether the run is complete: at least one unit, all integrated,
    /// and no deferred gate failed at the phase boundary. (Coverage and inline
    /// gate-green are enforced by the conductor's coverage gate and per-unit gates; a
    /// unit reaches Integrated only after its inline gates pass. A deferred gate runs
    /// ONCE at end-of-run, after every unit integrated, so its failure is folded in
    /// here rather than at any single unit.)
    pub fn done(&self) -> bool {
        !self.deferred_gate_failed
            && !self.units.is_empty()
            && self.units.values().all(|u| u.status == Status::Integrated)
    }

    /// The full "done" predicate (§4.1, R6): every criterion covered + every unit
    /// integrated + every gate green.
    ///
    /// The three conjuncts collapse to two checks here because the conductor already
    /// enforces the others by construction:
    /// - **every gate green** is implied by **every unit integrated**: a unit reaches
    ///   `Integrated` only after `run_gates` returns all-pass (and an adjudicator
    ///   verdict, when present, approves), so all-integrated already means gate-green.
    /// - **every criterion covered** is enforced as a gate at the start of the run
    ///   (and again after planning for a `produces` workflow); a remaining gap halts
    ///   the run with a SpecDefect rather than reaching here. So the live witness that
    ///   coverage held is the *absence* of a flagged spec defect.
    ///
    /// Hence: when there are criteria to satisfy, the run is fully done iff no spec
    /// defect was flagged and every unit integrated. With no criteria there is nothing
    /// to converge against, so this defers to the plain `done` predicate.
    pub fn fully_done(&self, criteria: &[String]) -> bool {
        if self.spec_defect || self.deferred_gate_failed {
            return false;
        }
        if criteria.is_empty() {
            return self.done();
        }
        !self.units.is_empty() && self.units.values().all(|u| u.status == Status::Integrated)
    }

    /// The ready-to-release handoff for this run (spec 38, criterion 3), or `None` when the
    /// run is not [`Self::done`] - so an unfinished run (a unit still un-integrated, an empty
    /// run, or a failed deferred phase-boundary gate) surfaces NO release-ready signal. On a
    /// done run it names `run_branch` as the PR head and the release-target `base` (the
    /// resolved base ref with a leading `origin/` remote prefix stripped) as the PR base, so
    /// the derived `gh pr create` command targets the branch a PR can actually apply to.
    /// Purely derived (no new event, no auto-merge): a resume-by-replay re-reaches it.
    pub fn release_ready(&self, run_branch: &str, base: &str) -> Option<ReleaseReady> {
        if !self.done() {
            return None;
        }
        let integrated_units = self
            .units
            .values()
            .filter(|u| u.status == Status::Integrated)
            .count();
        // The PR base is the release-target BRANCH, not a remote tracking ref: `gh pr create
        // --base` takes a branch name, so strip the default remote's `origin/` prefix
        // (`origin/main` -> `main`). A base that is already a plain branch, or one on another
        // remote, is left intact for the human to adjust.
        let base = base.strip_prefix("origin/").unwrap_or(base).to_string();
        let pr_command = format!("gh pr create --base {base} --head {run_branch}");
        Some(ReleaseReady {
            run_branch: run_branch.to_string(),
            base,
            integrated_units,
            pr_command,
        })
    }

    /// Whether a unit has reached a terminal state (integrated or escalated).
    ///
    /// `Failed` is deliberately NOT terminal. A `Failed` unit has exhausted ONE
    /// remediation attempt but has NOT yet hit `MAX_RETRIES` (which would have folded
    /// to `Escalated`): it is mid-remediation, not done. A window that ends with the
    /// unit `Failed` must let the next window RESUME and continue remediating from the
    /// recorded attempt count - not seed it into `terminal` and skip it forever. Only
    /// `Integrated` (it landed) and `Escalated` (it gave up at the bound, which is
    /// final) are truly terminal; both halt the unit for good.
    pub fn is_terminal(&self, id: &str) -> bool {
        matches!(
            self.units.get(id).map(|u| u.status),
            Some(Status::Integrated) | Some(Status::Escalated)
        )
    }

    /// Whether a unit has been integrated (used by resume to skip completed work).
    pub fn is_integrated(&self, id: &str) -> bool {
        matches!(
            self.units.get(id).map(|u| u.status),
            Some(Status::Integrated)
        )
    }

    /// The ids of every unit that ESCALATED - it exhausted remediation and went terminal
    /// WITHOUT integrating (§4.6, spec 19c unit 1). Lexically ordered (the `units` map is a
    /// [`BTreeMap`] keyed by id) so the set is deterministic for the serialized wire.
    ///
    /// This is the honest wedge set: a fixpoint reached with any escalated unit is NOT a
    /// clean completion, so `rigger step` copies this onto its printed `Step` and the thin
    /// driver stops loudly on a wedged terminus, exactly as it does for a budget halt. It is
    /// deliberately `Escalated`-ONLY, not "every unit that never integrated": a
    /// terminal-by-design unit (`on_pass: none`) rests unintegrated at a clean fixpoint by
    /// intent and must NOT be surfaced as a wedge (finding
    /// adv-u1-approved-not-integrated-false-positive-on-onpass-none) - only a unit that gave
    /// up at the retry bound is a genuine wedge.
    pub fn escalated_units(&self) -> Vec<String> {
        self.units
            .values()
            .filter(|u| u.status == Status::Escalated)
            .map(|u| u.id.clone())
            .collect()
    }

    /// Fold the manual-review inbox into [`RunState::manual_review`]: distinct unit ids that
    /// have a [`TYPE_MANUAL_REVIEW`] event and are NOT (yet) terminal.
    ///
    /// This is a SECOND pass over the events, run once by [`project`] after the per-event
    /// fold, because the terminal exclusion needs the FINAL state: a unit that is
    /// manual-reviewed and later integrated (or escalated) must leave the inbox, and event
    /// order does not guarantee the terminal transition follows the pause. The candidate id
    /// is the event's `unit` field, falling back to `id`; empties are skipped; the result is
    /// deduped and lexically ordered (via [`BTreeSet`]) for a stable render.
    fn fold_manual_review_inbox(&mut self, events: &[Event]) {
        let mut inbox: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
        for e in events {
            if e.type_ != TYPE_MANUAL_REVIEW {
                continue;
            }
            let Ok(p) = serde_json::from_slice::<ManualReview>(&e.data) else {
                continue;
            };
            let id = if p.unit.is_empty() { p.id } else { p.unit };
            if !id.is_empty() && !self.is_terminal(&id) {
                inbox.insert(id);
            }
        }
        self.manual_review = inbox.into_iter().collect();
    }
}

/// Project rebuilds run state from an ordered slice of events.
pub fn project(events: &[Event]) -> Result<RunState, serde_json::Error> {
    let mut r = RunState::new();
    for e in events {
        r.apply(e)?;
    }
    r.fold_manual_review_inbox(events);
    Ok(r)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ev(type_: &str, data: &str) -> Event {
        Event::new(type_, data.as_bytes().to_vec())
    }

    #[test]
    fn projects_unit_lifecycle() {
        let events = vec![
            ev(
                TYPE_UNIT_STARTED,
                r#"{"id":"u","needs":["x"],"worktree":"/wt","branch":"b"}"#,
            ),
            ev(
                TYPE_UNIT_STATUS,
                r#"{"id":"u","status":"green","evidence":{"green":"54 passed"}}"#,
            ),
            ev(TYPE_UNIT_INTEGRATED, r#"{"id":"u","commit":"abc"}"#),
        ];
        let r = project(&events).unwrap();
        assert_eq!(r.units["u"].status, Status::Integrated);
        assert_eq!(r.units["u"].commit, "abc");
        assert_eq!(r.units["u"].depends_on, ["x"]);
        assert_eq!(r.units["u"].branch, "b");
        assert_eq!(
            r.units["u"].evidence.get("green").map(String::as_str),
            Some("54 passed")
        );
        assert!(r.done());
        assert!(r.is_integrated("u"));
    }

    #[test]
    fn folds_intermediate_lifecycle_states() {
        let events = vec![
            ev(TYPE_UNIT_STARTED, r#"{"id":"u"}"#),
            ev(TYPE_UNIT_STATUS, r#"{"id":"u","status":"red"}"#),
            ev(TYPE_UNIT_STATUS, r#"{"id":"u","status":"verified"}"#),
        ];
        let r = project(&events).unwrap();
        assert_eq!(r.units["u"].status, Status::Verified);
        assert!(!r.done());
        assert!(!r.is_terminal("u"));
    }

    #[test]
    fn not_done_with_an_escalated_unit() {
        let r = project(&[ev(TYPE_UNIT_ESCALATED, r#"{"id":"u"}"#)]).unwrap();
        assert_eq!(r.units["u"].status, Status::Escalated);
        assert!(!r.done());
        assert!(r.is_terminal("u"));
    }

    #[test]
    fn a_failed_unit_is_not_terminal() {
        // A unit that FAILED a review/gate but has NOT yet escalated (attempts < the
        // bound) is mid-remediation, not done. It must NOT be treated as terminal -
        // otherwise resume seeds it into `terminal` and skips it forever, and it never
        // finishes its remediation across windows.
        let r = project(&[ev(TYPE_UNIT_FAILED, r#"{"id":"u","attempts":1}"#)]).unwrap();
        assert_eq!(r.units["u"].status, Status::Failed);
        assert_eq!(r.units["u"].attempts, 1);
        assert!(
            !r.is_terminal("u"),
            "a Failed-but-not-escalated unit is transient (mid-remediation), not terminal"
        );
        // The folded attempt count is preserved so resume can continue remediating from
        // it rather than restarting the counter.
        assert!(
            !r.done(),
            "a Failed unit is not Integrated, so the run is not done"
        );
        assert!(!r.is_integrated("u"));
    }

    #[test]
    fn fully_done_holds_for_a_clean_run_and_fails_on_escalation() {
        let criteria = vec!["crit".into()];

        // Clean run: every unit integrated, no spec defect -> fully done (§4.1, R6).
        let clean = project(&[
            ev(TYPE_UNIT_STARTED, r#"{"id":"u","spec_criterion":"crit"}"#),
            ev(TYPE_UNIT_INTEGRATED, r#"{"id":"u","commit":"abc"}"#),
        ])
        .unwrap();
        assert!(clean.fully_done(&criteria));

        // An escalated unit is not integrated -> not fully done.
        let escalated = project(&[
            ev(TYPE_UNIT_STARTED, r#"{"id":"u","spec_criterion":"crit"}"#),
            ev(TYPE_UNIT_ESCALATED, r#"{"id":"u"}"#),
        ])
        .unwrap();
        assert!(!escalated.fully_done(&criteria));

        // A flagged spec defect gates fully_done even if every unit integrated.
        let defect = project(&[
            ev(TYPE_UNIT_STARTED, r#"{"id":"u","spec_criterion":"crit"}"#),
            ev(TYPE_UNIT_INTEGRATED, r#"{"id":"u","commit":"abc"}"#),
            ev(TYPE_SPEC_DEFECT, r#"{"reason":"gap"}"#),
        ])
        .unwrap();
        assert!(defect.spec_defect);
        assert!(!defect.fully_done(&criteria));
    }

    #[test]
    fn a_failing_deferred_gate_gates_done_and_fully_done() {
        // A deferred gate that failed at the phase boundary must gate BOTH `done` and
        // `fully_done`, even when every unit integrated - a deferred failure is never
        // reported as a finished run.
        let criteria = vec!["crit".into()];
        let r = project(&[
            ev(TYPE_UNIT_STARTED, r#"{"id":"u","spec_criterion":"crit"}"#),
            ev(TYPE_UNIT_INTEGRATED, r#"{"id":"u","commit":"abc"}"#),
            ev(TYPE_DEFERRED_GATE_FAILED, r#"{"gate":"itest"}"#),
        ])
        .unwrap();
        assert!(r.deferred_gate_failed);
        // Every unit integrated, yet the run is not done because a deferred gate failed.
        assert!(r.units.values().all(|u| u.status == Status::Integrated));
        assert!(!r.done(), "a failing deferred gate must gate `done`");
        assert!(
            !r.fully_done(&criteria),
            "a failing deferred gate must gate `fully_done` with criteria"
        );
        assert!(
            !r.fully_done(&[]),
            "a failing deferred gate must gate `fully_done` with no criteria"
        );
    }

    #[test]
    fn release_ready_surfaces_only_a_done_run() {
        // Spec 38 criterion 3 (the ready-to-release handoff): on a DONE run the projection
        // yields the summary naming the run branch, the release-target base, the
        // integrated-unit count, and the exact PR command; a run that is NOT done yields
        // None so no release-ready signal is ever surfaced for unfinished work.

        // A done run (one integrated unit) IS release-ready.
        let done = project(&[
            ev(TYPE_UNIT_STARTED, r#"{"id":"u1"}"#),
            ev(TYPE_UNIT_INTEGRATED, r#"{"id":"u1","commit":"abc"}"#),
        ])
        .unwrap();
        let rr = done
            .release_ready("rigger-run", "origin/main")
            .expect("a done run is release-ready");
        assert_eq!(rr.run_branch, "rigger-run");
        // The release target is the base ref with the `origin/` remote prefix stripped, so
        // the PR command targets the branch (`main`), not the tracking ref (`origin/main`).
        assert_eq!(rr.base, "main");
        assert_eq!(rr.integrated_units, 1);
        assert_eq!(rr.pr_command, "gh pr create --base main --head rigger-run");
        // The human render names all four facts on the status surface.
        let text = rr.lines().join("\n");
        assert!(text.contains("rigger-run"), "{text}");
        assert!(text.contains("main"), "{text}");
        assert!(text.contains("1 unit"), "{text}");
        assert!(
            text.contains("gh pr create --base main --head rigger-run"),
            "{text}"
        );

        // A base that is already a plain branch name is passed through unchanged.
        let plain = done.release_ready("rigger-run", "develop").unwrap();
        assert_eq!(plain.base, "develop");
        assert_eq!(
            plain.pr_command,
            "gh pr create --base develop --head rigger-run"
        );

        // A run with a not-yet-integrated unit surfaces NO release-ready signal.
        let running = project(&[
            ev(TYPE_UNIT_STARTED, r#"{"id":"u1"}"#),
            ev(TYPE_UNIT_INTEGRATED, r#"{"id":"u1","commit":"abc"}"#),
            ev(TYPE_UNIT_STARTED, r#"{"id":"u2"}"#),
        ])
        .unwrap();
        assert!(running.release_ready("rigger-run", "origin/main").is_none());

        // An empty run (nothing landed) is not release-ready.
        assert!(RunState::new()
            .release_ready("rigger-run", "origin/main")
            .is_none());

        // A failing deferred phase-boundary gate is never release-ready, even with every
        // unit integrated - it must not be reported as a finished, releasable run.
        let deferred_failed = project(&[
            ev(TYPE_UNIT_STARTED, r#"{"id":"u1"}"#),
            ev(TYPE_UNIT_INTEGRATED, r#"{"id":"u1","commit":"abc"}"#),
            ev(TYPE_DEFERRED_GATE_FAILED, r#"{"gate":"itest"}"#),
        ])
        .unwrap();
        assert!(deferred_failed
            .release_ready("rigger-run", "origin/main")
            .is_none());
    }

    #[test]
    fn manual_review_inbox_folds_fallback_dedup_and_drops_terminal() {
        // The action-needed inbox: distinct, non-terminal units with a ManualReview.
        // Exercises every arm of `fold_manual_review_inbox` in one projection:
        //   a - `unit` field, emitted TWICE  -> deduped to one entry, not terminal
        //   b - only the `id` field present  -> picked up via the id fallback
        //   c - manual-reviewed then INTEGRATED -> dropped (terminal exclusion)
        //   d - manual-reviewed then ESCALATED  -> dropped (terminal exclusion)
        let events = vec![
            ev(TYPE_UNIT_STARTED, r#"{"id":"a"}"#),
            ev(TYPE_MANUAL_REVIEW, r#"{"id":"a","unit":"a"}"#),
            // Duplicate ManualReview for the same unit must not double-list it.
            ev(TYPE_MANUAL_REVIEW, r#"{"id":"a","unit":"a"}"#),
            // Only the generic `id` field, no `unit` - the fallback must still list it.
            ev(TYPE_MANUAL_REVIEW, r#"{"id":"b"}"#),
            // Manual-reviewed then integrated: the terminal transition drops it.
            ev(TYPE_UNIT_STARTED, r#"{"id":"c"}"#),
            ev(TYPE_MANUAL_REVIEW, r#"{"id":"c","unit":"c"}"#),
            ev(TYPE_UNIT_INTEGRATED, r#"{"id":"c","commit":"abc"}"#),
            // Manual-reviewed then escalated: escalation is terminal too, so it drops.
            ev(TYPE_MANUAL_REVIEW, r#"{"id":"d","unit":"d"}"#),
            ev(TYPE_UNIT_ESCALATED, r#"{"id":"d"}"#),
        ];
        let r = project(&events).unwrap();
        // Deduped, lexically ordered, terminal units excluded: only a and b remain.
        assert_eq!(r.manual_review, vec!["a".to_string(), "b".to_string()]);
    }

    #[test]
    fn manual_review_inbox_is_empty_without_manual_review_events() {
        // A run with no ManualReview leaves the inbox empty (no spurious entries).
        let r = project(&[
            ev(TYPE_UNIT_STARTED, r#"{"id":"u"}"#),
            ev(TYPE_UNIT_INTEGRATED, r#"{"id":"u","commit":"abc"}"#),
        ])
        .unwrap();
        assert!(r.manual_review.is_empty());
    }
}
