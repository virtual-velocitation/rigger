//! The process half of [`crate::run`] (spec 93, criterion 1: THE CORE LANE IS PURE):
//! starting, adopting and re-pinning a LIVE run against a real event store.
//! [`crate::run`] keeps the model (`RunStarted`, `RunOf`, `RunStart`) and the pure folds
//! (`current_run`, `current_run_id`, `current_run_base`, `current_run_base_tip`,
//! `run_attribution`, `effective_definition`) - everything computable from an
//! already-read `&[Event]` slice. This module is the OTHER half: the functions that
//! mint a fresh run id (`uuid::Uuid::new_v4`, a banned `core` import) and append to a
//! `&dyn EventStore`, split into its own file (never a function-by-function `#[cfg]`
//! inside `run.rs`) so the pure half can compile for `wasm32-unknown-unknown`.
//! `crate::run` stays the single vocabulary owner (the event type, the metadata keys,
//! the `RunStarted`/`RunStart` shapes); this module only ever appends what `run` already
//! knows how to serialize.

use crate::contextgraph::TYPE_DECISION_MADE;
use crate::eventstore::{Error, Event, EventStore, ExpectedRevision};
use crate::run::read::read_run;
use crate::run::{
    adopted_run, current_run, effective_definition, RunStart, RunStarted, META_DEFINITION,
    META_DEFINITION_PRIOR, META_RUN_ID, STREAM, TYPE_RUN_STARTED,
};

/// Record a `--rebase-definition` supersession on the current run (spec 13, unit 1): the
/// operator explicitly accepted the on-disk definition drift, so the run re-pins from `old`
/// to `new` and continues. It rides the existing `DecisionMade` vocabulary (no new event
/// type - the spec-13 global constraint) so it folds into the context graph as a decision,
/// and stamps the re-pinned hash in [`META_DEFINITION`] (and the superseded hash in
/// [`META_DEFINITION_PRIOR`]) so [`effective_definition`] advances and subsequent steps see
/// no drift. Scoped to the run via [`META_RUN_ID`], appended after the run's `RunStarted`.
fn record_rebase(store: &dyn EventStore, run: &str, old: &str, new: &str) -> Result<(), Error> {
    let decision = serde_json::json!({
        "id": format!("definition-rebase-{new}"),
        "summary": format!(
            "rigger --rebase-definition: accepted definition drift on run {run}; re-pinned {old} -> {new}"
        ),
    });
    let data = serde_json::to_vec(&decision)
        .map_err(|e| Error::Backend(format!("serialize rebase: {e}")))?;
    let ev = Event::new(TYPE_DECISION_MADE, data)
        .with_meta(META_RUN_ID, run)
        .with_meta(META_DEFINITION, new)
        .with_meta(META_DEFINITION_PRIOR, old);
    store
        .append(STREAM, ExpectedRevision::Any, std::slice::from_ref(&ev))?
        .one(&format!("the definition re-pin of run {run}"))?;
    Ok(())
}

/// Ensure a run is active for `criteria`, returning its run id.
///
/// If the latest run in the store was started for the SAME criteria, that run is
/// ADOPTED (resumed): nothing is appended, so a resume, an idle re-run, and a replay
/// step are all idempotent - no duplicate RunStarted fragments the run, and the
/// driver's `done` detection is preserved. Otherwise a FRESH run BEGINS: a new uuid
/// RunStarted stamped with `criteria` is appended and its id returned.
///
/// The criteria are the per-campaign fingerprint. They are the one signal derivable
/// from the log alone that distinguishes "continue the campaign in flight" from "a new
/// campaign over the same store", without re-minting on every step (which would split
/// one campaign across many runs). A legacy store with no RunStarted begins its first
/// run here; its prior events stay before that boundary and never become live work
/// (Gap 11: a new run no longer resurrects history's zombies).
pub fn ensure_started(store: &dyn EventStore, criteria: &[String]) -> Result<String, Error> {
    // The UNPINNED run start: an empty definition never drifts, so this only ever adopts or
    // mints - the historical behavior. The conductor calls this (definition pinning is
    // enforced once at the CLI boundary via [`ensure_started_pinned`]), so the two never
    // fight over the boundary: the CLI ensures the pinned run, the conductor adopts it. The
    // conductor does not resolve the run-branch base, its tip, or spec path (all three are
    // the CLI's concern), so it passes them empty here: in every real run entry the CLI has
    // already minted the RunStarted WITH its resolved base/tip and spec via
    // [`ensure_started_pinned`], and this call then ADOPTS it, so a mint here (empty
    // base/base_tip/spec) only happens on a path that has none of them to persist.
    Ok(
        ensure_started_pinned(store, criteria, "", false, "", "", "")?
            .run()
            .to_string(),
    )
}

/// Ensure a run is active for `criteria` AND enforce its definition pin (spec 13, unit 1).
///
/// This is the single adopt-or-mint authority; [`ensure_started`] is the unpinned
/// convenience over it (`definition == ""`, `rebase == false`). `definition` is the current
/// on-disk definition hash (`main::definition_hash`); an empty `definition` disables pinning
/// (the drift check is skipped and nothing is pinned), so an un-pinned caller behaves exactly
/// as before this feature.
///
/// - Same-criteria run in the store (a RESUME / live-run step): the run is ADOPTED and its
///   [`effective_definition`] is compared to `definition`. Agreement (or either side empty -
///   an unpinned run, or pinning disabled) is [`RunStart::Ready`]. A mismatch is definition
///   DRIFT: with `rebase` it records the supersession ([`record_rebase`]) and re-pins
///   ([`RunStart::Rebased`]); without `rebase` it is [`RunStart::Drifted`] and the caller
///   HALTS loudly. The mid-campaign prompt edit that silently changes replay semantics - the
///   sharpest exposure spec 13 names - can no longer pass unnoticed.
/// - No matching run (a NEW campaign / empty store): a FRESH run is minted pinning
///   `definition` and returned as [`RunStart::Ready`]. New runs are always free - only a LIVE
///   run pins (R1's edit-to-reconfigure holds for run boundaries).
///
/// `base` is the resolved run-branch base to persist on a freshly-minted RunStarted (spec 38,
/// criterion 3): it flows through to [`start_fresh`] and is stamped as `META_BASE` so
/// status/dash later name the run's actual base. It is used ONLY on the mint path - an ADOPTED
/// run keeps the base its original start stamped, so a resume never re-stamps or overwrites it.
///
/// `base_tip` is the run branch's tip commit sha AT THE MOMENT this run started (spec 91): it
/// flows through to [`start_fresh`] and is persisted in the RunStarted BODY as
/// `RunStarted::base_tip`, read back by `current_run_base_tip` and exported as
/// `$RIGGER_RUN_BASE` to every gate command. Unlike `base` (an operator-chosen REF such as
/// `origin/main`, meaningful only at anchor time), this is the resolved commit the run branch
/// actually anchored the run branch's tip at - what the checkin stage's `mutation` gate diffs
/// the whole spec against, never a `git merge-base` with the run branch (already HEAD there,
/// once the checkin stage's own worktree branches off it after every implement unit has
/// integrated). Mint-only, exactly like `base`: an ADOPTED run keeps its original stamp.
///
/// `spec_path` is the spec file path this run was launched with (spec 82, criterion 1): it
/// flows through to [`start_fresh`] and is persisted in the RunStarted body, mirroring `base`'s
/// mint-only persistence - an ADOPTED run keeps its original start's spec, never re-stamped.
pub fn ensure_started_pinned(
    store: &dyn EventStore,
    criteria: &[String],
    definition: &str,
    rebase: bool,
    base: &str,
    base_tip: &str,
    spec_path: &str,
) -> Result<RunStart, Error> {
    let events = read_run(store, STREAM)?;
    // THE MINT DECISION (spec 112): adopt the latest run when its criteria match, else mint.
    if let Some(run) = adopted_run(&events, criteria, false) {
        let pinned = effective_definition(current_run(&events));
        // Free when pinning is disabled (`definition` empty), the run is unpinned
        // (`pinned` empty - a legacy start), or the pin agrees with what is on disk.
        if definition.is_empty() || pinned.is_empty() || pinned == definition {
            return Ok(RunStart::Ready(run.run));
        }
        // Definition drift on a LIVE run.
        if rebase {
            record_rebase(store, &run.run, &pinned, definition)?;
            return Ok(RunStart::Rebased {
                run: run.run,
                pinned,
                current: definition.to_string(),
            });
        }
        return Ok(RunStart::Drifted {
            run: run.run,
            pinned,
            current: definition.to_string(),
        });
    }
    // A new campaign / empty store: a fresh run is always free - it pins the current definition
    // and persists the resolved run-branch base, its tip, and the spec path.
    Ok(RunStart::Ready(start_fresh(
        store, criteria, definition, base, base_tip, spec_path,
    )?))
}

/// Begin a FRESH run for `criteria`, UNCONDITIONALLY: mint a new uuid `RunStarted` and
/// append it, returning the new run id.
///
/// Unlike [`ensure_started`], this never adopts the latest run even when its criteria
/// match. It is the operator's explicit "start over" (`rigger run --fresh`) - the evented
/// recovery from a run wedged in a terminal state whose spec is UNCHANGED. A plan-critique
/// escalation is terminal within its run slice (the resume short-circuit holds the fan-out
/// forever), and the escalation-recovery every other case relies on - fix the spec, so its
/// criteria change and `ensure_started` mints a fresh run - does not apply when the spec is
/// correct and the escalation was a defect since fixed. Without this, `ensure_started`
/// would adopt the wedged run on every relaunch.
///
/// This is additive, not destructive: the prior run stays in the log as history and
/// cross-run context (its decisions and findings remain visible through the whole-stream
/// graph and `rigger peers`); the new boundary simply begins a clean slice AFTER it, so
/// the conductor folds ready work from an empty prior state (`current_run` scopes to the
/// new boundary) and the wedged gate runs anew. It does not touch the git run branch, so a
/// fresh run starts over atop whatever that branch already holds.
///
/// `definition` is the on-disk definition hash the new run PINS (spec 13, unit 1): a fresh
/// boundary is always free (it never drifts against a prior pin), and pinning the current
/// definition here is what lets a later live-run step detect a mid-campaign edit.
///
/// `base` is the resolved run-branch base to persist as `META_BASE` metadata on the new
/// RunStarted (spec 38, criterion 3), so status/dash/the end-of-run summary all read one base.
/// Pass `""` from a path with no run-branch base (an offline replay, the conductor's unpinned
/// adopt-or-mint); an empty base is simply not stamped and the reader falls back to live
/// resolution.
///
/// `base_tip` is the run branch's tip commit sha AT THE MOMENT this run started (spec 91),
/// persisted in the new RunStarted's BODY as `RunStarted::base_tip` - read back by
/// `current_run_base_tip` and exported to every gate command as `$RIGGER_RUN_BASE`, the
/// coordinate the checkin stage's `mutation` gate diffs the whole spec against. Pass `""` from
/// a path with no real run branch (an offline replay, the conductor's unpinned adopt-or-mint);
/// an empty tip decodes back as `None` and that gate's own `test -n "$RIGGER_RUN_BASE"` guard
/// refuses loud rather than sweeping nothing.
///
/// `spec_path` is the spec file path to persist in the new RunStarted's body (spec 82,
/// criterion 1), so `rigger status`/`rigger dash` - which run without the launching `--spec`
/// argv - can derive this run's per-run-unique PR head name. Pass `""` from a path with no
/// spec (an offline replay, a no-spec workflow run); the head-name derivation then degrades to
/// the run-short-id alone.
pub fn start_fresh(
    store: &dyn EventStore,
    criteria: &[String],
    definition: &str,
    base: &str,
    base_tip: &str,
    spec_path: &str,
) -> Result<String, Error> {
    let started = RunStarted {
        run: uuid::Uuid::new_v4().to_string(),
        criteria: criteria.to_vec(),
        definition: definition.to_string(),
        base: base.to_string(),
        base_tip: base_tip.to_string(),
        spec: spec_path.to_string(),
    };
    let ev = started
        .to_event()
        .map_err(|e| Error::Backend(format!("serialize RunStarted: {e}")))?;
    store
        .append(STREAM, ExpectedRevision::Any, std::slice::from_ref(&ev))?
        .one(&format!("the {TYPE_RUN_STARTED} of run {}", started.run))?;
    Ok(started.run)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eventstore::sqlite::Store;
    use crate::eventstore::Direction;
    use crate::ingest::DERIVED_INDEX_TYPES;
    use crate::run::read::CARRY_OVER_TYPES;
    use crate::run::{current_run_base, current_run_base_tip, current_run_id};
    use crate::test_support::{ev, seed_one_shot_fixture, ReadCountingStore};

    /// ONE-SHOT COMMANDS READ FROM THE BOUNDARY (spec 101): over a log holding 200,000 derived
    /// events and two superseded runs before the boundary, one read of the run costs exactly the
    /// boundary lookup, the carried-over knowledge by type and the run slice from the boundary
    /// with the derived types refused at the store - and a fold over it sees the current run
    /// exactly, each event once and in revision order, with every run's carried-over knowledge.
    #[test]
    fn read_run_costs_the_runs_own_events_plus_the_typed_carry_over() {
        let inner = Store::open(":memory:").unwrap();
        let fixture = seed_one_shot_fixture(&inner, STREAM, &[]);
        let store = ReadCountingStore::new(&inner);
        let events = read_run(&store, STREAM).unwrap();
        assert_eq!(store.reads(), fixture.read(STREAM));
        assert_eq!(store.materialized(), fixture.cost());
        assert_eq!(
            CARRY_OVER_TYPES,
            ["DecisionMade", "LessonLearned", "ReviewFinding"]
        );
        let types: Vec<&str> = events.iter().map(|e| e.type_.as_str()).collect();
        assert_eq!(
            types,
            [
                "DecisionMade",
                "LessonLearned",
                "ReviewFinding",
                "RunStarted",
                "RunNote",
                "DecisionMade",
                "ReviewFinding",
                "RunNote",
            ]
        );
        let revisions: Vec<i64> = events.iter().map(|e| e.revision).collect();
        let mut sorted = revisions.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(revisions, sorted, "each event once, in log order");
        assert_eq!(current_run(&events).len(), 5);
        assert_eq!(current_run(&events)[0].revision, fixture.boundary);
        assert_eq!(current_run_id(&events).as_deref(), Some("run-c"));
    }

    /// A READ OF THE RUN IS A LOG PREFIX (spec 101): a writer that appends a decision and then a
    /// spawn result between the read's two typed reads leaves the read holding both (the
    /// decision is never missing beside the later result), and one that appends after the read's
    /// last call leaves it holding neither - either way the read is every non-derived event of the
    /// run up to one head, in log order.
    #[test]
    fn read_run_is_a_log_prefix_whatever_a_concurrent_writer_appends_between_its_reads() {
        let late = || {
            vec![
                ev("DecisionMade", r#"{"id":"d-late"}"#),
                ev("SpawnResult", r#"{"id":"s-late"}"#),
            ]
        };
        let read_with_writer_after = |call: usize| -> Vec<String> {
            let inner = Store::open(":memory:").unwrap();
            seed_one_shot_fixture(&inner, STREAM, &[]);
            let store = ReadCountingStore::new(&inner).interleaving(call, STREAM, late());
            let events = read_run(&store, STREAM).unwrap();
            let positions: Vec<u64> = events.iter().map(|e| e.position).collect();
            let mut ordered = positions.clone();
            ordered.sort_unstable();
            ordered.dedup();
            assert_eq!(positions, ordered, "each event once, in log order");
            events.into_iter().map(|e| e.type_).collect()
        };
        let before = [
            "DecisionMade",
            "LessonLearned",
            "ReviewFinding",
            "RunStarted",
            "RunNote",
            "DecisionMade",
            "ReviewFinding",
            "RunNote",
        ];
        assert_eq!(
            read_with_writer_after(1),
            [&before[..], &["DecisionMade", "SpawnResult"]].concat(),
            "a write between the two typed reads is read whole"
        );
        assert_eq!(
            read_with_writer_after(2),
            before,
            "a write after the last read is not read at all"
        );
    }

    /// THE CURRENT RUN IN ONE READ (spec 101): [`read_current_run`] hands back the current run's
    /// slice and its id from exactly one read of the run - the boundary lookup, the carried-over
    /// knowledge by type and the slice - never a prior run's carried-over events.
    #[test]
    fn read_current_run_is_the_current_run_and_its_id_from_one_read() {
        let inner = Store::open(":memory:").unwrap();
        let fixture = seed_one_shot_fixture(&inner, STREAM, &[]);
        let store = ReadCountingStore::new(&inner);
        let (events, run_id) = crate::run::read::read_current_run(&store, STREAM).unwrap();
        assert_eq!(store.reads(), fixture.read(STREAM));
        assert_eq!(run_id, "run-c");
        assert_eq!(
            events.iter().map(|e| e.type_.as_str()).collect::<Vec<_>>(),
            [
                "RunStarted",
                "RunNote",
                "DecisionMade",
                "ReviewFinding",
                "RunNote"
            ]
        );

        let empty = Store::open(":memory:").unwrap();
        empty
            .append(
                STREAM,
                ExpectedRevision::NoStream,
                &[ev("UnitStarted", "{}")],
            )
            .unwrap();
        let (events, run_id) = crate::run::read::read_current_run(&empty, STREAM).unwrap();
        assert_eq!(run_id, "", "no run started names no run");
        assert_eq!(
            events.len(),
            1,
            "with no run started the whole stream is the run"
        );
    }

    /// With no run started the whole stream is the run: one typed read from revision 0 refusing
    /// only the derived types, after the boundary lookup found nothing.
    #[test]
    fn read_run_with_no_run_started_reads_every_non_derived_event() {
        let inner = Store::open(":memory:").unwrap();
        inner
            .append(
                STREAM,
                ExpectedRevision::NoStream,
                &[
                    ev("UnitStarted", "{}"),
                    ev("EdgeInferred", "{}"),
                    ev("DecisionMade", "{}"),
                    ev("CodeEntityExtracted", "{}"),
                ],
            )
            .unwrap();
        let store = ReadCountingStore::new(&inner);
        let types: Vec<String> = read_run(&store, STREAM)
            .unwrap()
            .into_iter()
            .map(|e| e.type_)
            .collect();
        assert_eq!(types, ["UnitStarted", "DecisionMade"]);
        assert_eq!(
            store.reads(),
            [
                crate::test_support::CountedRead::LastPosition {
                    stream: STREAM.to_string(),
                    event_type: TYPE_RUN_STARTED.to_string(),
                },
                crate::test_support::CountedRead::Typed {
                    stream: STREAM.to_string(),
                    from: 0,
                    only: false,
                    types: DERIVED_INDEX_TYPES.iter().map(|t| t.to_string()).collect(),
                    materialized: 2,
                },
            ]
        );
    }

    /// The run reads in LOG order, with or without a boundary: rows a stale writer left at
    /// revisions out of position order come back as the log recorded them (so `rigger watch`
    /// still sees the disorder), never re-sorted into revision order.
    #[test]
    fn read_run_hands_the_run_back_in_log_order_with_and_without_a_boundary() {
        let dir = tempfile::tempdir().unwrap();
        let revisions_read = |seed: &[&str], tail: [(&str, i64); 2]| -> Vec<i64> {
            let path = dir.path().join(format!("{}.db", seed.len()));
            let path = path.to_str().unwrap();
            let store = Store::open(path).unwrap();
            let events: Vec<Event> = seed.iter().map(|t| ev(t, "{}")).collect();
            store
                .append(STREAM, ExpectedRevision::NoStream, &events)
                .unwrap();
            let conn = rusqlite::Connection::open(path).unwrap();
            for (type_, revision) in tail {
                conn.execute(
                    "INSERT INTO events (stream, type, id, data, meta, valid_from, recorded_at, \
                     revision) VALUES (?1, ?2, ?3, X'7b7d', '{}', 0, 0, ?4)",
                    rusqlite::params![STREAM, type_, format!("tail-{revision}"), revision],
                )
                .unwrap();
            }
            read_run(&store, STREAM)
                .unwrap()
                .iter()
                .map(|e| e.revision)
                .collect()
        };
        assert_eq!(
            revisions_read(
                &["UnitStarted", "UnitStatus"],
                [("Note", 1005), ("Note", 1001)]
            ),
            [0, 1, 1005, 1001],
            "no boundary: the whole non-derived stream, in log order"
        );
        assert_eq!(
            revisions_read(
                &["RunStarted", "DecisionMade", "Note"],
                [("DecisionMade", 1005), ("Note", 1001)]
            ),
            [0, 1, 2, 1005, 1001],
            "a boundary: the carry-over and the run slice merged in log order"
        );
    }

    /// How many `RunStarted` boundaries `events` holds.
    fn run_started_count(events: &[Event]) -> usize {
        events
            .iter()
            .filter(|e| e.type_ == TYPE_RUN_STARTED)
            .count()
    }

    /// One `(base, base_tip, spec)` triple a run is started with.
    type Launch<'a> = (&'a str, &'a str, &'a str);

    /// A field of the run's `RunStarted` is stamped ONCE, at mint: a run minted with `mint`
    /// reads back `stamped` through `read` (`stamped_why`); a same-criteria resume passing the
    /// DIFFERENT `adopt` values adopts that run, appends no second boundary and keeps `stamped`
    /// (`kept_why`); and a mint with every value empty reads back `bare` (`bare_why`).
    #[allow(clippy::too_many_arguments)]
    fn assert_stamped_once_at_mint(
        mint: Launch,
        adopt: Launch,
        read: fn(&[Event]) -> Option<String>,
        stamped: &str,
        stamped_why: &str,
        kept_why: &str,
        bare: Option<&str>,
        bare_why: &str,
    ) {
        let store = Store::open(":memory:").unwrap();
        let crit = ["crit".to_string()];
        let (base, tip, spec) = mint;
        let minted = start_fresh(&store, &crit, "def", base, tip, spec).unwrap();
        let events = store.read_stream(STREAM, 0, Direction::Forward).unwrap();
        assert_eq!(read(&events).as_deref(), Some(stamped), "{stamped_why}");

        let (base, tip, spec) = adopt;
        let out = ensure_started_pinned(&store, &crit, "def", false, base, tip, spec).unwrap();
        assert_eq!(
            out.run(),
            minted,
            "the same-criteria resume adopts the minted run rather than re-minting"
        );
        let events = store.read_stream(STREAM, 0, Direction::Forward).unwrap();
        assert_eq!(
            run_started_count(&events),
            1,
            "adopt appends no second RunStarted"
        );
        assert_eq!(read(&events).as_deref(), Some(stamped), "{kept_why}");

        let bare_store = Store::open(":memory:").unwrap();
        start_fresh(&bare_store, &crit, "def", "", "", "").unwrap();
        let bare_events = bare_store
            .read_stream(STREAM, 0, Direction::Forward)
            .unwrap();
        assert_eq!(read(&bare_events).as_deref(), bare, "{bare_why}");
    }

    crate::test_cases! {
        /// Spec 38, criterion 3: the run-branch base a run anchors on is stamped as `META_BASE`
        /// on its RunStarted at mint, so `rigger status`/`rigger dash` - which cannot see the
        /// run's `--base` flag - read the run's ACTUAL base from the log. It is stamped ONCE (on
        /// the mint) and an adopting resume keeps it, so the base never drifts across steps. A
        /// mint with an EMPTY base (a repo-less path, an offline replay) stamps nothing, so a
        /// reader falls back to live resolution rather than reading an empty string.
        the_resolved_base_is_persisted_on_the_run_start_and_survives_adopt:
            assert_stamped_once_at_mint(
                ("origin/develop", "", ""),
                ("origin/other", "", ""),
                current_run_base,
                "origin/develop",
                "start_fresh stamps the resolved base as RunStarted metadata",
                "adopt keeps the base the original mint stamped; it never re-stamps",
                None,
                "an empty base is not stamped; the reader reports no persisted base",
            );
        /// Spec 91: the run branch's tip commit sha AT run start is persisted in the
        /// RunStarted BODY (unlike `base`, a ref, which lives only in metadata) at mint, so
        /// the checkin stage's `mutation` gate can diff the whole spec against the tree BEFORE
        /// any implement unit began - never a `git merge-base` with the run branch, which has
        /// already moved past this point by the time that stage's own worktree exists. It is
        /// stamped ONCE (on the mint) and an adopting resume keeps it, so the tip never drifts
        /// across steps even as the real run branch advances underneath. A mint with an EMPTY
        /// tip (a repo-less path, an offline replay) decodes back empty, so the reader reports
        /// no persisted tip and the mutation gate's own `test -n` guard refuses loud rather
        /// than sweeping an empty diff.
        the_base_tip_is_persisted_on_the_run_start_and_survives_adopt:
            assert_stamped_once_at_mint(
                ("origin/develop", "abc123deadbeef", ""),
                ("origin/other", "111222deadbeef", ""),
                current_run_base_tip,
                "abc123deadbeef",
                "start_fresh persists the run branch's tip in the RunStarted body",
                "adopt keeps the tip the original mint stamped; it never re-stamps",
                None,
                "an empty tip is not stamped; the reader reports no persisted base tip",
            );
        /// Spec 82, criterion 1: the spec file path a run was launched with is persisted in
        /// the RunStarted BODY at mint (unlike `base`, which lives only in metadata), so
        /// `ledger::RunState::apply`'s plain decode of the event picks it up like `run`/
        /// `criteria`/`definition` - no new parameter threaded through `release_ready`'s
        /// callers. It is stamped ONCE (on the mint) and an adopting resume keeps it,
        /// mirroring `base`'s mint-only persistence. A mint with an EMPTY spec path (an
        /// offline replay, a no-spec workflow run) decodes back as empty - the reader
        /// (`ledger::RunState`) then degrades the PR head-name derivation to the run-short-id
        /// alone.
        the_spec_path_is_persisted_on_the_run_start_and_survives_adopt:
            assert_stamped_once_at_mint(
                ("", "", "specs/82-unique-pr-heads.md"),
                ("", "", "specs/99-other.md"),
                |events| adopted_run(events, &["crit".to_string()], false).map(|run| run.spec),
                "specs/82-unique-pr-heads.md",
                "start_fresh persists the launching spec path in the RunStarted body",
                "adopt keeps the spec path the original mint stamped; it never re-stamps",
                Some(""),
                "",
            );
    }

    #[test]
    fn ensure_started_mints_a_fresh_run_on_an_empty_store() {
        let store = Store::open(":memory:").unwrap();
        let run = ensure_started(&store, &["crit".to_string()]).unwrap();
        assert!(!run.is_empty(), "a fresh run id is minted");

        let events = store.read_stream(STREAM, 0, Direction::Forward).unwrap();
        let starts: Vec<_> = events
            .iter()
            .filter(|e| e.type_ == TYPE_RUN_STARTED)
            .collect();
        assert_eq!(starts.len(), 1, "exactly one RunStarted is appended");
        assert_eq!(
            starts[0].meta.get(META_RUN_ID).map(String::as_str),
            Some(run.as_str()),
            "the RunStarted carries its own run id in metadata"
        );
        assert_eq!(
            adopted_run(&events, &["crit".to_string()], false).map(|started| started.run),
            Some(run),
            "the minted run is the one a same-criteria step adopts"
        );
    }

    #[test]
    fn ensure_started_adopts_the_same_criteria_run_without_re_minting() {
        // Resume / idle re-run / replay step: the same criteria adopt the existing run and
        // append NOTHING, so a run is never fragmented across step processes.
        let store = Store::open(":memory:").unwrap();
        let first = ensure_started(&store, &["crit".to_string()]).unwrap();
        let again = ensure_started(&store, &["crit".to_string()]).unwrap();
        assert_eq!(first, again, "the same criteria adopt the same run id");

        let events = store.read_stream(STREAM, 0, Direction::Forward).unwrap();
        assert_eq!(
            run_started_count(&events),
            1,
            "adopting a run appends no second RunStarted"
        );
    }

    #[test]
    fn start_fresh_mints_a_new_run_even_when_the_criteria_match() {
        // `rigger run --fresh`: the operator's explicit "start over". Where `ensure_started`
        // ADOPTS a same-criteria run, `start_fresh` ALWAYS appends a new boundary, so a run
        // wedged in a terminal state (e.g. an escalated plan-critique) whose spec is
        // unchanged can be re-run cleanly. The prior run stays in the log; the new slice
        // begins after it.
        let store = Store::open(":memory:").unwrap();
        let first = ensure_started(&store, &["crit".to_string()]).unwrap();
        // Some residue lands in the first run (a terminal escalation, say).
        store
            .append(
                STREAM,
                ExpectedRevision::Any,
                &[ev(
                    "UnitStatus",
                    r#"{"id":"plan-critique","status":"escalated"}"#,
                )],
            )
            .unwrap();

        let fresh = start_fresh(&store, &["crit".to_string()], "", "", "", "").unwrap();
        assert_ne!(
            first, fresh,
            "start_fresh mints a distinct run even though the criteria are identical"
        );

        let events = store.read_stream(STREAM, 0, Direction::Forward).unwrap();
        assert_eq!(
            run_started_count(&events),
            2,
            "--fresh appended a second RunStarted rather than adopting the wedged run"
        );
        // The current slice is the fresh boundary onward - the prior run's escalated residue
        // sits BEFORE it and can never seed live work, so the gate runs anew.
        let slice = current_run(&events);
        assert_eq!(current_run_id(&events).as_deref(), Some(fresh.as_str()));
        assert!(
            !slice
                .iter()
                .any(|e| String::from_utf8_lossy(&e.data).contains("escalated")),
            "the prior run's terminal residue is excluded from the fresh run's slice"
        );
        // A subsequent ensure_started (as the conductor calls internally) ADOPTS the fresh
        // boundary - so `rigger run --fresh` drives the clean run it just began.
        let adopted = ensure_started(&store, &["crit".to_string()]).unwrap();
        assert_eq!(
            adopted, fresh,
            "the conductor adopts the freshly-started run"
        );
    }

    #[test]
    fn ensure_started_mints_a_new_run_when_the_criteria_change() {
        // A new campaign (different acceptance criteria) begins a fresh run, so the prior
        // campaign's residue is left behind the new boundary.
        let store = Store::open(":memory:").unwrap();
        let first = ensure_started(&store, &["old".to_string()]).unwrap();
        let second = ensure_started(&store, &["new".to_string()]).unwrap();
        assert_ne!(first, second, "changed criteria begin a distinct run");

        let events = store.read_stream(STREAM, 0, Direction::Forward).unwrap();
        assert_eq!(
            run_started_count(&events),
            2,
            "the new campaign appended its own RunStarted"
        );
        assert_eq!(current_run_id(&events).as_deref(), Some(second.as_str()));
    }

    #[test]
    fn ensure_started_over_legacy_events_leaves_them_before_the_boundary() {
        // The Gap 11 case: a store holding stale non-terminal units of an aborted
        // pre-run-id run. The first run begins here; the stale units are before the
        // RunStarted, so the current run's slice never contains them.
        let store = Store::open(":memory:").unwrap();
        store
            .append(
                STREAM,
                ExpectedRevision::Any,
                &[
                    ev("UnitStarted", r#"{"id":"u-zombie"}"#),
                    ev(
                        "SpawnRequested",
                        r#"{"id":"u-zombie/implementer#0","unit":"u-zombie"}"#,
                    ),
                ],
            )
            .unwrap();

        ensure_started(&store, &["crit".to_string()]).unwrap();

        let events = store.read_stream(STREAM, 0, Direction::Forward).unwrap();
        let slice = current_run(&events);
        assert_eq!(slice.len(), 1, "only the RunStarted is in the fresh run");
        assert_eq!(slice[0].type_, TYPE_RUN_STARTED);
        assert!(
            !slice
                .iter()
                .any(|e| String::from_utf8_lossy(&e.data).contains("zombie")),
            "the aborted prior run's zombies are before the boundary and never live"
        );
    }

    // --- Definition pinning (spec 13, unit 1) ---

    #[test]
    fn a_fresh_run_pins_its_definition_hash_at_start() {
        // A new campaign pins the current definition hash on its RunStarted - the anchor a
        // later live-run step re-checks. This is the "a run pins its definition hash at
        // start" done-when, and the fresh-run-is-free path (no prior pin, no halt).
        let store = Store::open(":memory:").unwrap();
        let out = ensure_started_pinned(&store, &["crit".to_string()], "hash-A", false, "", "", "")
            .unwrap();
        assert!(
            matches!(out, RunStart::Ready(_)),
            "a fresh run is always free"
        );

        let events = store.read_stream(STREAM, 0, Direction::Forward).unwrap();
        let started = adopted_run(&events, &["crit".to_string()], false).unwrap();
        assert_eq!(
            started.definition, "hash-A",
            "the RunStarted pins the current definition hash"
        );
        assert_eq!(effective_definition(current_run(&events)), "hash-A");
    }

    #[test]
    fn adopting_a_run_whose_definition_is_unchanged_is_free() {
        // A plain step over an unchanged definition adopts the run and appends nothing - the
        // steady-state resume, unaffected by pinning.
        let store = Store::open(":memory:").unwrap();
        ensure_started_pinned(&store, &["crit".to_string()], "hash-A", false, "", "", "").unwrap();
        let out = ensure_started_pinned(&store, &["crit".to_string()], "hash-A", false, "", "", "")
            .unwrap();
        assert!(
            matches!(out, RunStart::Ready(_)),
            "an unchanged definition adopts, does not drift"
        );
        assert_eq!(
            run_started_count(&store.read_stream(STREAM, 0, Direction::Forward).unwrap()),
            1,
            "adopting appends no second RunStarted"
        );
    }

    #[test]
    fn a_live_run_under_a_drifted_definition_reports_drift_and_appends_nothing() {
        // The sharpest exposure spec 13 names: a mid-campaign definition edit. A live-run
        // step whose on-disk hash differs from the pinned hash is RunStart::Drifted (the CLI
        // then HALTS loudly), and - crucially - drift is a pure READ: nothing is appended, so
        // re-running the drifted step re-surfaces the same halt every time until it is resolved.
        let store = Store::open(":memory:").unwrap();
        ensure_started_pinned(&store, &["crit".to_string()], "hash-A", false, "", "", "").unwrap();
        let before = store
            .read_stream(STREAM, 0, Direction::Forward)
            .unwrap()
            .len();

        let out = ensure_started_pinned(&store, &["crit".to_string()], "hash-B", false, "", "", "")
            .unwrap();
        match out {
            RunStart::Drifted {
                pinned, current, ..
            } => {
                assert_eq!(pinned, "hash-A", "drift names the pinned hash");
                assert_eq!(current, "hash-B", "drift names the on-disk hash");
            }
            other => panic!("expected Drifted, got {other:?}"),
        }
        assert_eq!(
            store
                .read_stream(STREAM, 0, Direction::Forward)
                .unwrap()
                .len(),
            before,
            "a drift halt appends nothing - it re-surfaces on every step until resolved"
        );
    }

    #[test]
    fn rebase_definition_records_the_supersession_and_subsequent_steps_are_free() {
        // `--rebase-definition` records the supersession (old -> new) and continues; a plain
        // step AFTER the rebase sees the effective pin advanced to the new hash and no longer
        // halts. This is the "records the supersession and continues" done-when.
        let store = Store::open(":memory:").unwrap();
        ensure_started_pinned(&store, &["crit".to_string()], "hash-A", false, "", "", "").unwrap();

        let out = ensure_started_pinned(&store, &["crit".to_string()], "hash-B", true, "", "", "")
            .unwrap();
        match out {
            RunStart::Rebased {
                pinned, current, ..
            } => {
                assert_eq!((pinned.as_str(), current.as_str()), ("hash-A", "hash-B"));
            }
            other => panic!("expected Rebased, got {other:?}"),
        }

        // The supersession is recorded on the log (old and new hashes both legible) without a
        // second RunStarted moving the run boundary.
        let events = store.read_stream(STREAM, 0, Direction::Forward).unwrap();
        let rebase = events
            .iter()
            .find(|e| e.type_ == TYPE_DECISION_MADE)
            .expect("the rebase is recorded as a DecisionMade (no new event type)");
        assert_eq!(
            rebase.meta.get(META_DEFINITION).map(String::as_str),
            Some("hash-B")
        );
        assert_eq!(
            rebase.meta.get(META_DEFINITION_PRIOR).map(String::as_str),
            Some("hash-A")
        );
        assert_eq!(
            run_started_count(&events),
            1,
            "a rebase does NOT append a second RunStarted (the run boundary is unchanged)"
        );
        assert_eq!(
            effective_definition(current_run(&events)),
            "hash-B",
            "the effective pin advances to the rebased-to hash"
        );

        // A plain step on the (now new) definition is free - the rebase is not re-litigated.
        let after =
            ensure_started_pinned(&store, &["crit".to_string()], "hash-B", false, "", "", "")
                .unwrap();
        assert!(
            matches!(after, RunStart::Ready(_)),
            "after a rebase, a plain step on the new definition no longer drifts"
        );
    }

    #[test]
    fn an_unpinned_definition_never_drifts() {
        // The back-compat guard: a run started with no pin (empty definition - a legacy run,
        // or the conductor's own unpinned `ensure_started`), and a caller passing no
        // definition (pinning disabled), both take the free path unconditionally.
        let store = Store::open(":memory:").unwrap();
        // A legacy/unpinned run start.
        ensure_started_pinned(&store, &["crit".to_string()], "", false, "", "", "").unwrap();
        // A pinned caller against an unpinned run: free (the run pinned nothing to drift from).
        assert!(matches!(
            ensure_started_pinned(&store, &["crit".to_string()], "hash-Z", false, "", "", "")
                .unwrap(),
            RunStart::Ready(_)
        ));
        // A pin exists but the caller passes no definition (pinning disabled): free.
        let store2 = Store::open(":memory:").unwrap();
        ensure_started_pinned(&store2, &["crit".to_string()], "hash-A", false, "", "", "").unwrap();
        assert!(matches!(
            ensure_started_pinned(&store2, &["crit".to_string()], "", false, "", "", "").unwrap(),
            RunStart::Ready(_)
        ));
    }

    #[test]
    fn ensure_started_is_the_unpinned_convenience_and_still_adopts() {
        // The 2-arg `ensure_started` the conductor calls is unpinned: it delegates to
        // `ensure_started_pinned` with an empty definition, so it never drifts and adopts a
        // same-criteria run exactly as before pinning existed.
        let store = Store::open(":memory:").unwrap();
        let first = ensure_started(&store, &["crit".to_string()]).unwrap();
        let again = ensure_started(&store, &["crit".to_string()]).unwrap();
        assert_eq!(first, again, "unpinned ensure_started adopts the same run");
    }

    #[test]
    fn effective_definition_folds_the_last_rebase_over_the_pinned_start() {
        // The fold rule: the current pin is the RunStarted's definition advanced by the LAST
        // rebase record in the slice - so a run rebased A->B->C is effectively pinned at C.
        let store = Store::open(":memory:").unwrap();
        let run = start_fresh(&store, &["crit".to_string()], "hash-A", "", "", "").unwrap();
        record_rebase(&store, &run, "hash-A", "hash-B").unwrap();
        record_rebase(&store, &run, "hash-B", "hash-C").unwrap();
        let events = store.read_stream(STREAM, 0, Direction::Forward).unwrap();
        assert_eq!(effective_definition(current_run(&events)), "hash-C");
    }

    #[test]
    fn a_pin_is_scoped_to_its_run_a_fresh_boundary_repins_free() {
        // The new-campaign / --fresh path: a fresh boundary over a DIFFERENT definition is
        // always free (it pins the current hash, drifting against no prior run), and the drift
        // check reads only the CURRENT run's pin - a prior run's pin never leaks across.
        let store = Store::open(":memory:").unwrap();
        // Run 1 pins hash-A.
        ensure_started_pinned(&store, &["crit".to_string()], "hash-A", false, "", "", "").unwrap();
        // A NEW campaign (different criteria) begins its own fresh run pinning the current def.
        let out =
            ensure_started_pinned(&store, &["other".to_string()], "hash-B", false, "", "", "")
                .unwrap();
        assert!(
            matches!(out, RunStart::Ready(_)),
            "a fresh boundary for a new campaign is free even against a different definition"
        );
        let events = store.read_stream(STREAM, 0, Direction::Forward).unwrap();
        assert_eq!(
            run_started_count(&events),
            2,
            "the new campaign appended its own pinned RunStarted"
        );
        assert_eq!(
            effective_definition(current_run(&events)),
            "hash-B",
            "the current run's pin is the fresh boundary's, not run 1's"
        );
    }

    /// A RUN BOUNDARY NOBODY CAN LOCATE IS NOT A BOUNDARY, and this module writes the two
    /// events every later read of the log is partitioned by.
    ///
    /// Both writes used to hand their append report to `?;` and throw it away, which
    /// compiled unchanged when the port stopped promising a position. A store that wrote
    /// nothing then handed `ensure_started` a run id for a run the log does not contain,
    /// and every later `current_run_id` / `current_run_base` / attribution read would
    /// partition the stream against a boundary that was never recorded - silently, and for
    /// the whole run. So both ask the same authority the other single-event seams ask.
    #[test]
    fn a_run_boundary_the_store_did_not_write_is_never_reported_as_started() {
        let silent = crate::eventstore::SilentStore;

        let err = start_fresh(
            &silent,
            &["build the thing".to_string()],
            "hash-A",
            "",
            "",
            "",
        )
        .expect_err("a run whose RunStarted was never written has not started");
        let message = err.to_string();
        assert!(
            message.contains("nothing"),
            "the failure says the store wrote nothing rather than handing back a run id \
             for a boundary the log does not hold: {message}"
        );
        assert!(
            message.contains(TYPE_RUN_STARTED),
            "and names the event whose write was lost: {message}"
        );

        let err = record_rebase(&silent, "run-1", "hash-A", "hash-B")
            .expect_err("a supersession nobody can locate has not superseded anything");
        let message = err.to_string();
        assert!(
            message.contains("nothing") && message.contains("run-1"),
            "the re-pin that was never recorded says so, and names the run whose definition \
             would otherwise read as advanced: {message}"
        );
    }
}
