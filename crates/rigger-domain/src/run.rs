//! Run scoping (spec 06, unit 1 - Gap 11): a run is the slice of the append-only
//! `run` stream that begins with a [`TYPE_RUN_STARTED`] event carrying a fresh run id.
//! The conductor folds ready work from ONLY the current run's slice, so a fresh run
//! never resurrects the non-terminal residue of an aborted prior run; prior runs stay
//! visible as memory (decisions, findings, `rigger peers`) but can never become live
//! work.
//!
//! Three read-models fold run state from the one stream - the [`crate::ledger`]
//! (durable run state), the [`crate::spawn`] frontier (the stepwise wave), and the
//! [`crate::metrics`] (`rigger stats`) - and all three scope through [`current_run`].
//! The run vocabulary (the `RunStarted` event type and the `run_id` metadata key every
//! conductor-emitted event carries) lives here so those modules share one source of
//! truth rather than re-declaring it.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::contextgraph::{TYPE_DECISION_MADE, TYPE_LESSON_LEARNED, TYPE_REVIEW_FINDING};
use crate::eventstore::Event;

/// The one read a one-shot command folds (spec 101): the run from its boundary and the
/// carried-over knowledge by type, through the store port. Gated with [`crate::ingest`], whose
/// derived types it refuses.
#[cfg(any(feature = "store", not(feature = "core")))]
pub mod read;

/// The run's event stream name. Defined here (spec 93, criterion 1) rather than in
/// [`crate::conductor`] (which re-exports it): this module is `core`, `conductor` is the
/// `store`-gated orchestration use case excluded from `core`, and this module needs the
/// constant for [`current_run`] and friends.
pub const STREAM: &str = "run";

/// The event a run opens with: a fresh run id plus the acceptance criteria the run
/// satisfies. Deliberately NOT one of the lifecycle events the ledger folds - the
/// ledger and the context graph ignore an unknown type - so only the run-scoping
/// helpers here read it, exactly as [`crate::spawn::TYPE_SPAWN_REQUESTED`] is read only
/// by the spawn fold.
pub const TYPE_RUN_STARTED: &str = "RunStarted";

/// The metadata key that stamps every conductor-emitted event with the id of the run it
/// belongs to (spec 06, unit 1). Its value is the current run's uuid; a pre-run-id
/// (legacy) event carries no such key and so belongs to no run - it folds as "before the
/// first RunStarted" and never becomes live work.
pub const META_RUN_ID: &str = "run_id";

/// The metadata key carrying a definition hash on the events that pin one (spec 13, unit 1).
/// On a `--rebase-definition` supersession record it is the NEW (re-pinned) hash; the pin a
/// run opens with lives in the [`RunStarted::definition`] body. [`effective_definition`] reads
/// both through this one key + the body, so the current pin is a single fold over the run slice.
pub const META_DEFINITION: &str = "definition";

/// The metadata key carrying the OLD (superseded) definition hash on a `--rebase-definition`
/// record (spec 13, unit 1), so the supersession `old -> new` is legible in the log itself.
pub const META_DEFINITION_PRIOR: &str = "definition_prior";

/// The metadata key carrying the resolved run-branch base a run anchored on (spec 38,
/// criterion 3), stamped on the run's [`TYPE_RUN_STARTED`] event at mint. Persisting the base
/// here - stamped ONCE, when the run starts, from the same resolution `rigger run/step/workflow`
/// anchors the run branch with - is what lets a later `rigger status`/`rigger dash` name the
/// run's ACTUAL base in its ready-to-release handoff. Those surfaces run without the run's
/// `--base` flag on their argv and so cannot re-resolve it; reading this one persisted value
/// ([`current_run_base`]) keeps status, the dash, and the end-of-run summary on ONE base. It is
/// metadata, not a new event type, and empty on a run started before base persistence existed
/// (a legacy start), where the reader falls back to the live env/default resolution.
pub const META_BASE: &str = "base";

/// The body of a [`TYPE_RUN_STARTED`] event: the run id, the criteria fingerprint, and the
/// pinned definition hash.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunStarted {
    /// The fresh run id (a uuid) every event in this run is scoped by.
    pub run: String,
    /// The acceptance criteria this run satisfies - the per-campaign fingerprint that
    /// tells a RESUME (same criteria: adopt this run) from a NEW campaign (different
    /// criteria: begin a fresh run). Empty for a no-spec workflow run.
    #[serde(default)]
    pub criteria: Vec<String>,
    /// The definition hash pinned at run start (spec 13, unit 1): a stable digest over the
    /// on-disk workflow.yml + agent-prompt set (see `main::definition_hash`). A live-run step
    /// whose recomputed on-disk hash differs from the run's EFFECTIVE pin
    /// ([`effective_definition`]) HALTS loudly - so a mid-campaign prompt edit can never
    /// silently change replay semantics - unless `--rebase-definition` records the supersession
    /// and re-pins. Empty on a legacy run started before pinning existed, and on any unpinned
    /// start; an empty pin never drifts. New runs are always free: a fresh boundary just pins
    /// the current hash.
    #[serde(default)]
    pub definition: String,
    /// The resolved run-branch base this run anchored on (spec 38, criterion 3): stamped as
    /// [`META_BASE`] metadata on the emitted event, NOT serialized into the body (so it never
    /// enters the criteria/definition fingerprint a resume compares). Read back only via the
    /// event's metadata ([`current_run_base`]), so this field is write-only - it carries the
    /// base into [`Self::to_event`] at mint and is left empty (`#[serde(skip)]`) on decode.
    #[serde(skip)]
    pub base: String,
    /// The run branch's tip commit sha AT THE MOMENT this run started (spec 91): stamped
    /// once, at mint, from the same anchored run branch `base` was resolved against - never
    /// re-derived later. Persisted in the BODY (unlike `base`, which is metadata-only so it
    /// never enters the criteria/definition resume fingerprint): this field never affects run
    /// identity either (a resume compares only `criteria`), so body persistence is simply the
    /// plain decode convenience `spec` below already established, read back via
    /// [`current_run_base_tip`] and exported to every gate command as `$RIGGER_RUN_BASE`. The
    /// checkin stage's `mutation` gate diffs the whole spec against THIS commit, never a `git
    /// merge-base` with the run branch: that stage's own worktree branches off the run branch
    /// AFTER every implement unit has already integrated, so a merge-base there is already
    /// HEAD and would diff nothing (see
    /// `specs/91-mutation-runs-once-at-the-check-in-seam.md`). `#[serde(default)]` so a
    /// legacy RunStarted (predating this field) or a repo-less run decodes empty;
    /// [`current_run_base_tip`] then reports `None` and the mutation gate's own `test -n
    /// "$RIGGER_RUN_BASE"` refuses loud rather than sweeping an empty diff.
    #[serde(default)]
    pub base_tip: String,
    /// The spec file path this run was launched with (spec 82, criterion 1) - e.g.
    /// `specs/82-unique-pr-heads.md`. Persisted in the BODY (unlike [`Self::base`]), so a
    /// plain decode of this event picks it up exactly like `run`/`criteria`/`definition`:
    /// [`crate::ledger::RunState::apply`] folds it from here to derive this run's
    /// per-run-unique PR head name (`pr/<spec-stem>-<run-short-id>`, spec 82) without a new
    /// parameter threaded through every `release_ready` caller. `#[serde(default)]` so a
    /// legacy RunStarted (predating this field) or a no-spec workflow run decodes with an
    /// empty spec, and the head-name derivation degrades to the run-short-id alone.
    #[serde(default)]
    pub spec: String,
}

impl RunStarted {
    /// Build the appendable event for this run start, with the run id stamped in
    /// [`META_RUN_ID`] so the RunStarted itself belongs to its own run's slice, and the
    /// resolved run-branch base stamped in [`META_BASE`] (only when non-empty) so status/dash
    /// can name the run's actual base (spec 38, criterion 3). `pub` because its one caller
    /// outside this module, the impure mint path (`run_store::start_fresh`), lives in the
    /// `rigger-store-sqlite` adapter crate.
    #[cfg_attr(all(feature = "core", not(feature = "store")), allow(dead_code))] // only called from the *_store sibling under core-only
    pub fn to_event(&self) -> Result<Event, serde_json::Error> {
        let mut ev = Event::new(TYPE_RUN_STARTED, serde_json::to_vec(self)?)
            .with_meta(META_RUN_ID, &self.run);
        if !self.base.is_empty() {
            ev = ev.with_meta(META_BASE, &self.base);
        }
        Ok(ev)
    }
}

/// The latest [`RunStarted`] in `events`, decoded, or `None` when no run has started.
fn latest(events: &[Event]) -> Option<RunStarted> {
    events
        .iter()
        .rev()
        .find(|e| e.type_ == TYPE_RUN_STARTED)
        .and_then(Event::decode::<RunStarted>)
}

/// The current run's slice of `events`: the contiguous suffix from the LAST
/// [`TYPE_RUN_STARTED`] onward. When no run has started (a legacy store, or one this
/// feature has never scoped), the WHOLE slice is returned - so a store predating run
/// scoping folds exactly as before, and every fold that runs directly over raw events
/// is unchanged until a run actually begins.
///
/// A run's events are exactly this suffix: events are appended in order and a new run's
/// `RunStarted` is always appended after every prior run's events, so scoping by the
/// last boundary is scoping by run. Everything before that boundary - a prior run's
/// events, or pre-run-id legacy events - is excluded and can never become live work.
pub fn current_run(events: &[Event]) -> &[Event] {
    match events.iter().rposition(|e| e.type_ == TYPE_RUN_STARTED) {
        Some(i) => &events[i..],
        None => events,
    }
}

/// The id of the current (latest) run, or `None` when no run has started.
pub fn current_run_id(events: &[Event]) -> Option<String> {
    latest(events).map(|r| r.run)
}

/// The spec path the current (latest) run was launched with, its [`RunStarted::spec`] exactly as
/// recorded, or empty when no run has started or the run carries none.
pub fn current_run_spec_path(events: &[Event]) -> String {
    latest(events).map(|r| r.spec).unwrap_or_default()
}

/// The resolved run-branch base the current (latest) run anchored on, read from its
/// [`TYPE_RUN_STARTED`] event's [`META_BASE`] metadata (spec 38, criterion 3). `None` when no
/// run has started, or when the latest run was started before base persistence existed (a
/// legacy start carries no `META_BASE`) or without a real repo (an empty base is not stamped).
///
/// This is the SINGLE persisted-base authority: `rigger status`, `rigger dash`, and the
/// end-of-run summary all read the run's actual base from here, so they name ONE base even
/// though status/dash run without the run's `--base` flag on their argv. A caller falls back
/// to the live env/default resolution ([`crate::resolve_run_base`] in `main`) only when this
/// returns `None`, preserving legacy behavior for a run that never persisted its base.
pub fn current_run_base(events: &[Event]) -> Option<String> {
    events
        .iter()
        .rev()
        .find(|e| e.type_ == TYPE_RUN_STARTED)
        .and_then(|e| e.meta.get(META_BASE))
        .filter(|b| !b.is_empty())
        .cloned()
}

/// The run branch's tip commit sha the current (latest) run started AT, read from
/// [`RunStarted::base_tip`] in the BODY of its [`TYPE_RUN_STARTED`] event (spec 91). `None`
/// when no run has started, or when the latest run predates this field (a legacy start
/// decodes an empty `base_tip`) or ran without a real repo (an empty tip is never stamped).
///
/// This is the SINGLE authority this value is exported to the checkin stage's `mutation` gate
/// command through, as `$RIGGER_RUN_BASE` (spec 91, THE GATE ENVIRONMENT - see
/// [`crate::conductor`]'s gate-env assembly): the run branch's tip AT THE MOMENT the run
/// started, never a live re-resolution and never a `git merge-base` with the run branch
/// (which has already moved past this point by the time the checkin stage's own worktree
/// exists). `None` here is exactly the "this run has nothing to diff the whole spec against"
/// case the gate's own `test -n "$RIGGER_RUN_BASE"` guard refuses loud on.
pub fn current_run_base_tip(events: &[Event]) -> Option<String> {
    latest(events).map(|r| r.base_tip).filter(|b| !b.is_empty())
}

/// How a provenance-bearing event is attributed to a run (spec 21, unit 1).
///
/// The context graph spans runs by design (a unit inherits prior decisions), so a
/// decision or finding must be traced to the run whose `[RunStarted, next RunStarted)`
/// window contains the event that produced it - that provenance is what tells a live
/// decision from dead-run noise. This is the SINGLE authority for run attribution:
/// `rigger reset --runs` and `rigger peers` both derive their disposition from this one
/// window rule, never a second inline boundary scan.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RunOf {
    /// A decision or finding that falls inside a run's window: it belongs to this run id.
    Run(String),
    /// A decision or finding recorded BEFORE the first [`TYPE_RUN_STARTED`]. It belongs to
    /// no run (pre-boundary): while no run has started there is no closed run and it is live,
    /// and once a run has started it is neither the active run nor a lesson, so it is dead-run
    /// noise that `reset --runs` drops and `rigger peers` labels historical ([`RunOf::is_live`]).
    PreBoundary,
    /// A [`TYPE_LESSON_LEARNED`]: durable cross-run value, EXEMPT from attribution. A lesson
    /// is never placed in a run and so is never pruned as dead-run noise, regardless of its
    /// position (even one recorded before the first boundary). This is the "never attributed
    /// away" guarantee - a lesson is kept by its own rule, not by belonging to the active run.
    Lesson,
}

impl RunOf {
    /// Whether this node is live against the ACTIVE run `active` (the id from
    /// [`current_run_id`], `None` while no run has started). This is the single
    /// `not-live => historical` rule both read paths reuse: `rigger peers` labels a decision LIVE
    /// when this holds and HISTORICAL otherwise, and `reset --runs` keeps a node when this holds
    /// OR it is a lesson - one rule, one answer. A run's node is live while its run is the active
    /// one; a [`RunOf::PreBoundary`] node is live exactly while no run has started (a log with no
    /// `RunStarted` has no closed run, so nothing in it is dead - spec 101) and historical once one
    /// has; a [`RunOf::Lesson`] is deliberately NOT "live" (it is exempt, kept by a different rule).
    pub fn is_live(&self, active: Option<&str>) -> bool {
        match self {
            RunOf::Run(run) => active == Some(run.as_str()),
            RunOf::PreBoundary => active.is_none(),
            RunOf::Lesson => false,
        }
    }
}

/// Attribute every provenance-bearing event in the ordered stream to its run (spec 21,
/// unit 1) - the single authority the prune and the peers labels both reuse.
///
/// Returns a map from an event's index in `events` to its [`RunOf`], with an entry ONLY
/// for the three provenance types ([`TYPE_DECISION_MADE`], [`TYPE_REVIEW_FINDING`],
/// [`TYPE_LESSON_LEARNED`]); a [`TYPE_RUN_STARTED`] or any lifecycle event is absent.
/// Keying by index (not the event's own metadata) is deliberate: an agent-emitted
/// decision carries no run id in its metadata, so only its POSITION relative to the
/// `RunStarted` boundaries can attribute it - the window is the one uniform source.
///
/// `events` MUST be the whole [`STREAM`] in forward (append) order, exactly as
/// [`current_run`] expects; the window boundaries are read from it. The rule is a single
/// forward fold: the "current run" starts empty (no boundary seen yet) and advances to
/// each `RunStarted`'s id as the fold passes it, so a decision/finding is attributed to
/// the last boundary at or before it ([`RunOf::Run`]) or to [`RunOf::PreBoundary`] when
/// none precedes it. A `LessonLearned` is always [`RunOf::Lesson`] regardless of window -
/// it is exempt and never attributed away. The `BTreeMap` iterates in index order, so the
/// derivation is deterministic (a spec-21 global constraint).
pub fn run_attribution(events: &[Event]) -> BTreeMap<usize, RunOf> {
    let mut attribution = BTreeMap::new();
    let mut current: Option<String> = None;
    for (i, e) in events.iter().enumerate() {
        match e.type_.as_str() {
            TYPE_RUN_STARTED => {
                // Advance the window: every event after this boundary (until the next
                // RunStarted) belongs to this run. Decoded from the body, the same source
                // `current_run_id` reads, never the event's own metadata.
                if let Some(rs) = e.decode::<RunStarted>() {
                    current = Some(rs.run);
                }
            }
            TYPE_LESSON_LEARNED => {
                // Exempt regardless of window - a lesson is never attributed away.
                attribution.insert(i, RunOf::Lesson);
            }
            TYPE_DECISION_MADE | TYPE_REVIEW_FINDING => {
                let run_of = match &current {
                    Some(run) => RunOf::Run(run.clone()),
                    None => RunOf::PreBoundary,
                };
                attribution.insert(i, run_of);
            }
            _ => {}
        }
    }
    attribution
}

/// The definition hash currently in force for a run (spec 13, unit 1): the hash the run
/// pinned at start ([`RunStarted::definition`]), advanced by any `--rebase-definition`
/// supersession recorded since (each carries the re-pinned hash in [`META_DEFINITION`]).
/// The LAST authority wins, so a rebased run's effective pin is the rebased-to hash and a
/// plain step after a rebase no longer re-halts. Empty when the run pinned nothing (a
/// legacy/unpinned start), which the drift check reads as "unpinned - never drifts".
///
/// `run_slice` must be the CURRENT run's slice ([`current_run`]): it opens with the run's
/// one [`TYPE_RUN_STARTED`] and any rebase records for this run follow it.
pub fn effective_definition(run_slice: &[Event]) -> String {
    let mut pinned = String::new();
    for e in run_slice {
        if e.type_ == TYPE_RUN_STARTED {
            if let Some(rs) = e.decode::<RunStarted>() {
                pinned = rs.definition;
            }
        } else if let Some(rebased) = e.meta.get(META_DEFINITION) {
            pinned = rebased.clone();
        }
    }
    pinned
}

/// The outcome of ensuring a definition-pinned run (spec 13, unit 1).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RunStart {
    /// The free path: a fresh run was minted (empty store / new campaign / `--fresh`) pinning
    /// the current definition, OR an in-force run was adopted whose effective pin agrees with
    /// the on-disk definition (or is unpinned). `.0` is the run id; nothing halts.
    Ready(String),
    /// A live run was adopted but its effective pinned definition DRIFTED from the on-disk
    /// hash and no rebase was requested: the caller must HALT loudly. Carries the run id, the
    /// pinned (old) hash, and the current on-disk (new) hash for the halt message.
    Drifted {
        run: String,
        pinned: String,
        current: String,
    },
    /// A live run drifted and `--rebase-definition` recorded the supersession
    /// (`pinned -> current`); the run continues on the re-pinned definition.
    Rebased {
        run: String,
        pinned: String,
        current: String,
    },
}

impl RunStart {
    /// The run id, whichever outcome this is.
    pub fn run(&self) -> &str {
        match self {
            RunStart::Ready(run) => run,
            RunStart::Drifted { run, .. } | RunStart::Rebased { run, .. } => run,
        }
    }
}

/// THE TYPES THE RUN-CLOSURE RULE READS (spec 21, spec 101): [`superseded_graph_nodes`] and
/// [`superseded_edge_boundary`] answer the same over a log and over that log's events of these types
/// alone, in order - so a cold rebuild that keeps only these while it folds re-derives the prune
/// `rigger reset --runs` derives from the whole log, in the same ordered pass. A lesson is exempt
/// from the rule, so it is not among them.
pub const RUN_CLOSURE_TYPES: [&str; 3] =
    [TYPE_RUN_STARTED, TYPE_DECISION_MADE, TYPE_REVIEW_FINDING];

/// The retention cutoff `rigger reset --runs` reclaims superseded structural edges beneath
/// (spec 41): the nanosecond `valid_from` of the ACTIVE run's `RunStarted` - the SAME run boundary
/// [`superseded_graph_nodes`] keeps the active run's decision/finding nodes by. A superseded edge
/// (`valid_to IS NOT NULL`) retired BEFORE this cutoff belongs to a prior run and is dead cruft the
/// log can re-derive; one retired at or after it is recent history kept inside the window. `None`
/// when no run has started (a legacy store) - then nothing is reclaimed, so LIVE and recent history
/// are both untouched.
///
/// Pure over the whole forward stream, reusing the SINGLE run-boundary authority
/// ([`current_run`]) - never a second inline boundary scan. The cutoff is the graph's own
/// `to_nanos` time base (nanoseconds since the Unix epoch), so it compares directly to an edge's
/// stored `valid_to`.
pub fn superseded_edge_boundary(events: &[Event]) -> Option<i64> {
    current_run(events)
        .first()
        .filter(|e| e.type_ == TYPE_RUN_STARTED)
        .map(|e| {
            e.valid_from
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos() as i64)
                .unwrap_or(0)
        })
}

/// The decision and finding graph-node ids `rigger reset --runs` drops (spec 21, unit 2):
/// every provenance node that is NEITHER live ([`RunOf::is_live`]) NOR a lesson - a superseded
/// run's decision/finding, or a pre-boundary one recorded before the first `RunStarted` once a run
/// has started. A decision or finding is dead only against an active run: a log with no
/// `RunStarted` has no closed run, so the shared rule calls every node in it live and the drop set
/// is empty (spec 101), exactly as [`superseded_edge_boundary`] reclaims no edge from it. Pure over
/// the whole run stream, reusing the SINGLE run-attribution authority
/// (`run::run_attribution` + `run::current_run_id`) - never a second inline boundary scan.
///
/// `events` MUST be the whole [`STREAM`] in forward order - or its events of the
/// [`RUN_CLOSURE_TYPES`], the only ones this reads - exactly as
/// `run_attribution` and `current_run_id` require: the attribution keys by an event's INDEX
/// in this slice, so each node id is read back from `events[i]`'s own body (the `id` field
/// the projector folds the node under) - one whole-stream read feeds both the attribution and
/// the id lookup, never two different slices.
///
/// The keep invariant is enforced by SUBTRACTION, not by skipping live indices: a decision or
/// finding id can be recorded in BOTH a dead run AND the active run (id reuse across runs), so
/// the same graph node has one index attributed dead and another attributed live. We collect
/// the active run's node ids into a keep set and return `drop_candidates` MINUS that keep set,
/// so a reused id is PRESERVED (closes the active-node-pruned-on-cross-run-id-reuse hazard) -
/// dropping a candidate index alone would delete the shared node the active run still needs.
/// Returns a sorted, de-duplicated list (determinism is a spec-21 constraint), leaving every
/// `LessonLearned` (exempt) and every active-run node out of the drop set.
pub fn superseded_graph_nodes(events: &[Event]) -> Vec<String> {
    use std::collections::BTreeSet;
    let active = current_run_id(events);
    let attribution = run_attribution(events);
    let mut drop_candidates: BTreeSet<String> = BTreeSet::new();
    let mut keep: BTreeSet<String> = BTreeSet::new();
    for (&i, run_of) in &attribution {
        // A lesson is exempt (kept by its own rule, never "live" and never dropped).
        if matches!(run_of, RunOf::Lesson) {
            continue;
        }
        // Skip a malformed / empty-id body exactly as the projector's own fold skips it, so a
        // corrupt event never contributes a bogus id to either set.
        let Some(id) = graph_node_id(&events[i]) else {
            continue;
        };
        // A live node id - the active run's, or any recorded before a run has started - must be
        // KEPT; everything else - a superseded run's node, or a pre-boundary one once a run has
        // started - is a drop CANDIDATE. A single id can land in both sets when it is reused
        // across a dead run and the active run.
        if run_of.is_live(active.as_deref()) {
            keep.insert(id);
        } else {
            drop_candidates.insert(id);
        }
    }
    // Subtract the active run's kept ids: a node id present in BOTH a dead run and the active
    // run must be PRESERVED (dropping its dead-run index alone would delete the shared node the
    // active run still needs). The difference of two `BTreeSet`s iterates sorted, so the result
    // is deterministic (a spec-21 constraint).
    drop_candidates.difference(&keep).cloned().collect()
}

/// The graph-node id the projector folds a `DecisionMade` / `ReviewFinding` event under: the
/// `id` field of its JSON body (the exact key `contextgraph`'s fold reads, verbatim - the
/// decision/finding id is never alias-resolved). `None` for a malformed body or a
/// missing/empty id, so a corrupt event is skipped exactly as the projector's own fold skips
/// it, never dropping an unrelated node.
fn graph_node_id(e: &Event) -> Option<String> {
    let body: serde_json::Value = serde_json::from_slice(&e.data).ok()?;
    let id = body.get("id")?.as_str()?;
    (!id.is_empty()).then(|| id.to_string())
}

#[cfg(test)]
mod tests {
    /// The run-closure rule reads only the types [`super::RUN_CLOSURE_TYPES`] names: over a log
    /// mixing them with every other kind of event, it answers exactly what it answers over that
    /// log's events of those types alone - so a rebuild that keeps only those types while it folds
    /// derives the drop set and the edge boundary `rigger reset --runs` derives from the whole log.
    #[test]
    fn the_run_closure_rule_reads_only_the_run_closure_types() {
        use super::*;
        let at = |t: &str, body: &str, secs: u64| {
            let mut e = Event::new(t, body.as_bytes().to_vec());
            e.valid_from = std::time::UNIX_EPOCH + std::time::Duration::from_secs(secs);
            e
        };
        let decision =
            |id: &str| format!(r#"{{"id":"{id}","summary":"s","governs":[],"supersedes":""}}"#);
        let log = vec![
            at(TYPE_DECISION_MADE, &decision("d-pre"), 1),
            at("UnitStarted", r#"{"id":"u1"}"#, 2),
            at(TYPE_RUN_STARTED, r#"{"run":"r1","spec":"s.md"}"#, 3),
            at(TYPE_DECISION_MADE, &decision("d-dead"), 4),
            at(TYPE_REVIEW_FINDING, r#"{"id":"f-dead","about":[]}"#, 5),
            at(TYPE_DECISION_MADE, &decision("shared"), 6),
            at(TYPE_LESSON_LEARNED, r#"{"id":"l1","about":[]}"#, 7),
            at("EdgeInferred", r#"{"file":"a.rs","name":"f"}"#, 8),
            at(TYPE_RUN_STARTED, r#"{"run":"r2","spec":"s.md"}"#, 9),
            at("UnitIntegrated", r#"{"id":"u1"}"#, 10),
            at(TYPE_DECISION_MADE, &decision("d-live"), 11),
            at(TYPE_DECISION_MADE, &decision("shared"), 12),
        ];
        let kept: Vec<Event> = log
            .iter()
            .filter(|e| RUN_CLOSURE_TYPES.contains(&e.type_.as_str()))
            .cloned()
            .collect();
        let rule = |events: &[Event]| {
            (
                superseded_graph_nodes(events),
                superseded_edge_boundary(events),
            )
        };
        assert_eq!(
            (rule(&log), rule(&kept), kept.len()),
            (
                (
                    vec![
                        "d-dead".to_string(),
                        "d-pre".to_string(),
                        "f-dead".to_string()
                    ],
                    Some(9_000_000_000)
                ),
                (
                    vec![
                        "d-dead".to_string(),
                        "d-pre".to_string(),
                        "f-dead".to_string()
                    ],
                    Some(9_000_000_000)
                ),
                8
            ),
            "the closed runs' and pre-boundary nodes drop, the reused id and the lesson stay, and \
             the kept types alone answer the same"
        );
    }

    use super::*;
    use crate::test_support::ev;

    fn run_started(run: &str, criteria: &[&str]) -> Event {
        RunStarted {
            run: run.to_string(),
            criteria: criteria.iter().map(|s| s.to_string()).collect(),
            ..Default::default()
        }
        .to_event()
        .unwrap()
    }

    fn decision(id: &str) -> Event {
        ev(TYPE_DECISION_MADE, &format!(r#"{{"id":"{id}"}}"#))
    }
    fn finding(id: &str) -> Event {
        ev(TYPE_REVIEW_FINDING, &format!(r#"{{"id":"{id}"}}"#))
    }
    fn lesson(id: &str) -> Event {
        ev(TYPE_LESSON_LEARNED, &format!(r#"{{"id":"{id}"}}"#))
    }

    #[test]
    fn run_attribution_maps_decisions_to_their_window_and_never_attributes_lessons_away() {
        // Spec 21, unit 1 done-when: a decision/finding is attributed to the run whose
        // [RunStarted, next RunStarted) window contains its producing event, and a
        // LessonLearned is NEVER attributed away - it is exempt, even before the first
        // boundary. One store, two runs, plus pre-boundary residue.
        let events = vec![
            // Pre-boundary: recorded before any RunStarted - belongs to no run.
            decision("pre-d"),                      // 0
            finding("pre-f"),                       // 1
            lesson("pre-lesson"), // 2  a lesson before any boundary - still exempt
            run_started("r1", &["crit"]), // 3
            decision("d1"),       // 4
            finding("f1"),        // 5
            lesson("lesson-1"),   // 6  a lesson inside r1's window - exempt, not Run(r1)
            ev("UnitStarted", r#"{"id":"noise"}"#), // 7  not a provenance event
            run_started("r2", &["crit"]), // 8
            decision("d2"),       // 9
            lesson("lesson-2"),   // 10
        ];

        let attr = run_attribution(&events);

        // Pre-boundary decision/finding => PreBoundary (dead-run noise, belongs to no run).
        assert_eq!(attr.get(&0), Some(&RunOf::PreBoundary));
        assert_eq!(attr.get(&1), Some(&RunOf::PreBoundary));
        // A lesson is exempt regardless of position - even a pre-boundary one is Lesson.
        assert_eq!(attr.get(&2), Some(&RunOf::Lesson));
        // r1's decision and finding are attributed to r1's window.
        assert_eq!(attr.get(&4), Some(&RunOf::Run("r1".into())));
        assert_eq!(attr.get(&5), Some(&RunOf::Run("r1".into())));
        // A lesson INSIDE a run's window is still exempt - Lesson, never Run("r1").
        assert_eq!(attr.get(&6), Some(&RunOf::Lesson));
        // r2's decision is attributed to r2's window.
        assert_eq!(attr.get(&9), Some(&RunOf::Run("r2".into())));
        assert_eq!(attr.get(&10), Some(&RunOf::Lesson));
        // Boundaries and non-provenance events carry no attribution entry.
        assert!(!attr.contains_key(&3), "a RunStarted is not attributed");
        assert!(
            !attr.contains_key(&7),
            "a lifecycle event is not attributed"
        );
        assert!(!attr.contains_key(&8), "a RunStarted is not attributed");
        // Exactly the provenance events (3 decisions + 2 findings + 3 lessons), and only
        // those - the two boundaries and the lifecycle event carry no entry.
        assert_eq!(
            attr.len(),
            8,
            "every decision/finding/lesson event, and only those"
        );

        // The shared not-active => historical rule (c2 and c3 reuse it): with r2 the active
        // run, r2's decision is LIVE and r1's is HISTORICAL; a lesson is never "live" (kept
        // by its own exempt rule) and a pre-boundary node never matches the active run.
        let active = current_run_id(&events);
        assert_eq!(active.as_deref(), Some("r2"));
        assert!(
            attr[&9].is_live(active.as_deref()),
            "active-run decision is live"
        );
        assert!(
            !attr[&4].is_live(active.as_deref()),
            "superseded-run decision is historical"
        );
        assert!(
            !attr[&10].is_live(active.as_deref()),
            "a lesson is exempt, not live"
        );
        assert!(
            !attr[&0].is_live(active.as_deref()),
            "a pre-boundary node is historical"
        );
    }

    #[test]
    fn current_run_is_the_whole_slice_when_no_run_has_started() {
        // A legacy store (pre-run-id events, no RunStarted) folds whole-stream, exactly
        // as before run scoping - so every existing direct fold is untouched.
        let events = vec![
            ev("UnitStarted", r#"{"id":"a"}"#),
            ev("UnitStarted", r#"{"id":"b"}"#),
        ];
        assert_eq!(current_run(&events).len(), 2);
        assert!(current_run_id(&events).is_none());
    }

    #[test]
    fn current_run_is_the_suffix_from_the_last_run_started() {
        // Two prior units, then a run begins, then one live unit. The current run is the
        // RunStarted and everything after it - never the prior units.
        let events = vec![
            ev("UnitStarted", r#"{"id":"zombie-1"}"#),
            ev("UnitStarted", r#"{"id":"zombie-2"}"#),
            run_started("r1", &["crit"]),
            ev("UnitStarted", r#"{"id":"live"}"#),
        ];
        let slice = current_run(&events);
        assert_eq!(
            slice.len(),
            2,
            "RunStarted + the one live unit, never the zombies"
        );
        assert_eq!(slice[0].type_, TYPE_RUN_STARTED);
        assert_eq!(current_run_id(&events).as_deref(), Some("r1"));
        // None of the prior zombie units are in the current run's slice.
        assert!(
            !slice
                .iter()
                .any(|e| String::from_utf8_lossy(&e.data).contains("zombie")),
            "prior-run residue is excluded from the current run"
        );
    }

    /// The spec path the current run was launched with (spec 112, criterion 5: the plan-critique
    /// stop names it): the LATEST run's `RunStarted.spec` exactly as recorded - never an earlier
    /// run's, never stemmed - and empty when no run has started or the run carries no spec.
    #[test]
    fn current_run_spec_path_is_the_latest_runs_spec_as_recorded() {
        let launched = |run: &str, spec: &str| {
            RunStarted {
                run: run.to_string(),
                spec: spec.to_string(),
                ..Default::default()
            }
            .to_event()
            .unwrap()
        };
        let two_runs = vec![
            launched("r1", "specs/1-old.md"),
            ev("UnitStarted", r#"{"id":"u"}"#),
            launched("r2", "./specs/2-new.md"),
        ];
        let no_spec = vec![launched("r1", "specs/1-old.md"), run_started("r2", &[])];
        assert_eq!(
            (
                current_run_spec_path(&two_runs),
                current_run_spec_path(&no_spec),
                current_run_spec_path(&[ev("UnitStarted", r#"{"id":"u"}"#)]),
            ),
            ("./specs/2-new.md".to_string(), String::new(), String::new()),
            "the latest run's spec as recorded; empty for a run with no spec or no run at all"
        );
    }

    #[test]
    fn current_run_is_the_suffix_from_the_latest_of_several_runs() {
        let events = vec![
            run_started("r1", &["a"]),
            ev("UnitStarted", r#"{"id":"r1-unit"}"#),
            run_started("r2", &["b"]),
            ev("UnitStarted", r#"{"id":"r2-unit"}"#),
        ];
        let slice = current_run(&events);
        assert_eq!(slice.len(), 2);
        assert_eq!(current_run_id(&events).as_deref(), Some("r2"));
        assert!(String::from_utf8_lossy(&slice[1].data).contains("r2-unit"));
    }

    /// A log with no `RunStarted` has no closed run, so nothing in it is dead: the run-closure
    /// rule drops no decision or finding and reclaims no edge. Once a run starts, what was
    /// recorded before it is pre-boundary and drops, and the lesson never does.
    #[test]
    fn a_log_without_a_run_boundary_has_nothing_dead() {
        let before_any_run = vec![decision("d1"), finding("f1"), lesson("l1")];
        let mut started = before_any_run.clone();
        started.push(run_started("r1", &["crit"]));
        assert_eq!(
            (
                superseded_graph_nodes(&before_any_run),
                superseded_edge_boundary(&before_any_run),
                superseded_graph_nodes(&started),
            ),
            (
                Vec::<String>::new(),
                None,
                vec!["d1".to_string(), "f1".to_string()]
            ),
            "no run, nothing dead; a run started, the pre-boundary decision and finding drop"
        );
    }

    /// The no-run rule lives in the one attribution every consumer shares: with no active run a
    /// pre-boundary decision or finding is live - there is no closed run for it to belong to -
    /// and once a run is active it is historical; a run's own node is live only while that run is
    /// the active one, and a lesson is never live, whatever runs.
    #[test]
    fn a_pre_boundary_node_is_live_exactly_while_no_run_is_active() {
        let run = RunOf::Run("r1".to_string());
        assert_eq!(
            [
                RunOf::PreBoundary.is_live(None),
                RunOf::PreBoundary.is_live(Some("r1")),
                run.is_live(Some("r1")),
                run.is_live(Some("r2")),
                run.is_live(None),
                RunOf::Lesson.is_live(None),
                RunOf::Lesson.is_live(Some("r1")),
            ],
            [true, false, true, false, false, false, false],
            "live before any run, historical once one is active"
        );
    }
}
