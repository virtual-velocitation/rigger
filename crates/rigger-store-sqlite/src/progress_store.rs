//! The store half of [`crate::progress`] (spec 93, criterion 1: THE CORE LANE IS PURE):
//! recording one live progress report to a LIVE progress store. [`crate::progress`] keeps
//! the model (`AgentProgress`, `AgentActivity`) and the pure fold (`consolidate`) -
//! everything computable from already-read `&[Event]` slices, with no store at all. This
//! module is the other half: `record`, the typed writers beside it and the [`append_stamped`]
//! they share, the functions that append to a `&dyn EventStore`, with [`latest_launch_records`],
//! [`latest_launch`] and [`next_launch`], the reads a launch decides on, split into their own file
//! (never a function-by-function `#[cfg]` inside `progress.rs`) so the pure half can compile for
//! `wasm32-unknown-unknown`.

use crate::eventstore::{Error, EventStore, ExpectedRevision, Position, TypeSelection};
use crate::progress::{
    stream_of, AgentProgress, RunStamped, SpawnLaunched, StopFailure, TYPE_SPAWN_LAUNCHED,
};

/// Record one progress report to the progress `store`, stamped with `run_id`. Append-only
/// and side-effect-free beyond the one event: a pure write, cheap to call after every
/// significant step. Returns the global position the store issued for the report, or the
/// failure a store that wrote nothing has earned ([`crate::eventstore::Appended::one`]).
pub fn record(
    store: &dyn EventStore,
    run_id: &str,
    id: &str,
    activity: &str,
) -> Result<Position, Error> {
    let progress = AgentProgress {
        id: id.to_string(),
        activity: activity.to_string(),
        revision: None,
    };
    append_stamped(
        store,
        run_id,
        &progress,
        &format!("the progress report of {id}"),
    )
}

/// Record one [`SpawnLaunched`] open-launch report to the progress `store`, stamped with
/// `run_id` (spec 104 criterion 1: THE LAUNCH IS TYPED). The caller's ordering - append
/// this BEFORE starting the child - is what makes the record trustworthy; this function
/// only guarantees the append itself is durable (the position the store issued, or the
/// failure a store that wrote nothing has earned), same contract as [`record`].
pub fn record_launch(
    store: &dyn EventStore,
    run_id: &str,
    launch: &SpawnLaunched,
) -> Result<Position, Error> {
    append_stamped(
        store,
        run_id,
        launch,
        &format!(
            "the launch record of {} (launch {})",
            launch.spawn, launch.launch
        ),
    )
}

/// The ordinal the next launch of `spawn` takes (spec 105 criterion 1): the ordinal of its
/// [`latest_launch`] plus one, or `first`, the ordinal the caller names, when the run holds no
/// record of it. A failed read is the caller's to surface, since a launch that cannot name its
/// ordinal could overwrite a prior launch's transcript.
pub fn next_launch(
    store: &dyn EventStore,
    run_id: &str,
    spawn: &str,
    first: u32,
) -> Result<u32, Error> {
    Ok(latest_launch(store, run_id, spawn)?.map_or(first, |latest| latest.launch + 1))
}

/// The latest [`SpawnLaunched`] record of `spawn` in the run `run_id` (spec 105, RESUME BY
/// SESSION): the last of its [`latest_launch_records`], so its close once that launch closed and
/// its open record until then, or `None` when the run holds none.
pub fn latest_launch(
    store: &dyn EventStore,
    run_id: &str,
    spawn: &str,
) -> Result<Option<SpawnLaunched>, Error> {
    Ok(latest_launch_records(store, run_id, spawn)?.pop())
}

/// The [`SpawnLaunched`] records of the latest launch of `spawn` in the run `run_id` (spec 105,
/// RESUME BY SESSION), in stream order: those of the highest ordinal the run's records hold for
/// that id, its open record first unless it was lost, or none when they hold none. Opening and
/// closing records both count, so a launch whose open record was lost still names its ordinal. A
/// record that does not decode names nothing.
pub fn latest_launch_records(
    store: &dyn EventStore,
    run_id: &str,
    spawn: &str,
) -> Result<Vec<SpawnLaunched>, Error> {
    let recorded = store.read_stream_typed(
        &stream_of(run_id),
        0,
        TypeSelection::Only(&[TYPE_SPAWN_LAUNCHED]),
    )?;
    let mut records: Vec<SpawnLaunched> = recorded
        .iter()
        .filter_map(|event| serde_json::from_slice::<SpawnLaunched>(&event.data).ok())
        .filter(|record| record.spawn == spawn)
        .collect();
    let latest = records.iter().map(|record| record.launch).max();
    records.retain(|record| Some(record.launch) == latest);
    Ok(records)
}

/// Record one [`StopFailure`] hook firing to the progress `store`, stamped with `run_id`
/// (spec 104 criterion 5: A FAILURE HAS A CLASS - THE HOOKS' "record" half). Called by
/// `rigger hook stop-failure --spawn <id> --class <category>`, the installed hook command
/// itself; same append contract as [`record`]/[`record_launch`].
pub fn record_stop_failure(
    store: &dyn EventStore,
    run_id: &str,
    stop_failure: &StopFailure,
) -> Result<Position, Error> {
    append_stamped(
        store,
        run_id,
        stop_failure,
        &format!("the stop-failure record of {}", stop_failure.spawn),
    )
}

/// Append `report`, stamped with `run_id`, as the one event of its run's progress stream
/// ([`stream_of`]):
/// the append contract every record above shares, and the writer a record with no wrapper of its
/// own calls directly, as the Claude Code host does for [`crate::progress::ApiRetry`]. `what`
/// names the record if the store wrote nothing ([`crate::eventstore::Appended::one`]).
pub fn append_stamped<R: RunStamped>(
    store: &dyn EventStore,
    run_id: &str,
    report: &R,
    what: &str,
) -> Result<Position, Error> {
    let ev = report
        .to_stamped_event(run_id)
        .map_err(|e| Error::Backend(format!("serialize {}: {e}", R::EVENT_TYPE)))?;
    store
        .append(
            &stream_of(run_id),
            ExpectedRevision::Any,
            std::slice::from_ref(&ev),
        )?
        .one(what)
}

#[cfg(test)]
mod tests {
    use super::*;
    // The run stream, which the conductor re-exports as `conductor::STREAM`.
    use crate::eventstore::sqlite::Store;
    use crate::eventstore::{Direction, Event};
    use crate::progress::{STREAM, TYPE_AGENT_PROGRESS};
    use crate::run as conductor;
    use crate::run::META_RUN_ID;

    #[test]
    fn progress_lands_in_its_own_store_never_the_run_stream() {
        // spec 14, criterion 1: `rigger progress` records to the progress store, NEVER the
        // run stream - the isolation is the file boundary. A separate store cannot enter a
        // run fold, so the run stream and its projection are byte-identical with or without
        // progress. Two distinct stores stand in for `.rigger/events.db` and
        // `.rigger/progress.db`.
        let run = Store::open(":memory:").unwrap();
        let progress = Store::open(":memory:").unwrap();

        // A run stream with a lifecycle event.
        run.append(
            conductor::STREAM,
            ExpectedRevision::Any,
            &[Event::new("UnitStarted", b"{\"id\":\"u\"}".to_vec())],
        )
        .unwrap();
        let before = run
            .read_stream(conductor::STREAM, 0, Direction::Forward)
            .unwrap();

        // Record progress to the SEPARATE store.
        record(
            &progress,
            "run-1",
            "u/implementer#0",
            "grep #3: conductor.rs",
        )
        .unwrap();

        // The run stream did not grow, and the run store holds NO AgentProgress anywhere -
        // progress never entered the replay-authoritative log.
        let after = run
            .read_stream(conductor::STREAM, 0, Direction::Forward)
            .unwrap();
        assert_eq!(
            before.len(),
            after.len(),
            "recording progress must not touch the run stream"
        );
        assert!(
            run.read_stream(&stream_of("run-1"), 0, Direction::Forward)
                .unwrap()
                .is_empty(),
            "no AgentProgress may land in the run store"
        );

        // The report landed in the progress store's progress stream, stamped with its run.
        let p = progress
            .read_stream(&stream_of("run-1"), 0, Direction::Forward)
            .unwrap();
        assert_eq!(p.len(), 1);
        assert_eq!(p[0].type_, TYPE_AGENT_PROGRESS);
        assert_eq!(
            p[0].meta.get(META_RUN_ID).map(String::as_str),
            Some("run-1"),
            "the report is scoped to the run that emitted it"
        );
        let ap: AgentProgress = serde_json::from_slice(&p[0].data).unwrap();
        assert_eq!(ap.id, "u/implementer#0");
        assert_eq!(ap.activity, "grep #3: conductor.rs");
    }

    #[test]
    fn an_empty_run_id_records_without_a_run_stamp() {
        // Before any run has started (a legacy or bootstrapping store) progress still
        // records - it simply carries no run scope.
        let progress = Store::open(":memory:").unwrap();
        record(&progress, "", "u/implementer#0", "starting").unwrap();
        let p = progress.read_stream(STREAM, 0, Direction::Forward).unwrap();
        assert_eq!(p.len(), 1);
        assert!(
            !p[0].meta.contains_key(META_RUN_ID),
            "no run stamp when no run has started"
        );
    }

    /// A PROGRESS REPORT THE STORE DID NOT WRITE IS NOT A REPORT, and this seam is the one
    /// an agent calls after every step to prove it is not stalled.
    ///
    /// The absence used to come back as `Ok(None)` - a success the caller had to interpret,
    /// and the command above it interpreted it by printing that the report was recorded.
    /// A live view built on reports that were never written is the exact blackout this
    /// feature exists to close, so the seam asks the one authority instead: the position,
    /// or the failure.
    #[test]
    fn a_progress_report_the_store_did_not_write_is_reported_as_lost() {
        let err = record(
            &crate::eventstore::SilentStore,
            "run-1",
            "u1/implementer#0",
            "did a thing",
        )
        .expect_err("a progress report nobody can find was not recorded");
        let message = err.to_string();
        assert!(
            message.contains("nothing"),
            "the failure says the store wrote nothing: {message}"
        );
        assert!(
            message.contains("u1/implementer#0"),
            "and names the spawn whose report was lost: {message}"
        );
    }

    #[test]
    fn record_launch_lands_in_the_progress_store_never_the_run_stream() {
        // spec 104 criterion 1: SpawnLaunched is a progress-store record (design
        // decision u104c1-spawnlaunched-shape), not a run-stream event type - same
        // isolation `record` above already proves for AgentProgress.
        let run = Store::open(":memory:").unwrap();
        let progress = Store::open(":memory:").unwrap();

        record_launch(
            &progress,
            "run-1",
            &crate::progress::SpawnLaunched {
                spawn: "u104-launch/implementer#0".into(),
                launch: 0,
                session_id: "sess-1".into(),
                started: 1_700_000_000,
                ..Default::default()
            },
        )
        .unwrap();

        assert!(
            run.read_stream(&stream_of("run-1"), 0, Direction::Forward)
                .unwrap()
                .is_empty(),
            "no SpawnLaunched may land in the run store"
        );
        let p = progress
            .read_stream(&stream_of("run-1"), 0, Direction::Forward)
            .unwrap();
        assert_eq!(p.len(), 1);
        assert_eq!(p[0].type_, crate::progress::TYPE_SPAWN_LAUNCHED);
        assert_eq!(
            p[0].meta.get(META_RUN_ID).map(String::as_str),
            Some("run-1")
        );
        let sl: crate::progress::SpawnLaunched = serde_json::from_slice(&p[0].data).unwrap();
        assert_eq!(sl.spawn, "u104-launch/implementer#0");
        assert_eq!(sl.session_id, "sess-1");
    }

    #[test]
    fn record_launch_a_report_the_store_did_not_write_is_reported_as_lost() {
        let err = record_launch(
            &crate::eventstore::SilentStore,
            "run-1",
            &crate::progress::SpawnLaunched {
                spawn: "u1/implementer#0".into(),
                launch: 0,
                session_id: "s".into(),
                started: 1,
                ..Default::default()
            },
        )
        .expect_err("a launch record nobody can find was not recorded");
        let message = err.to_string();
        assert!(message.contains("u1/implementer#0"), "message: {message}");
    }

    #[test]
    fn record_stop_failure_lands_in_the_progress_store_never_the_run_stream() {
        // spec 104 criterion 5: StopFailure is a progress-store record, same isolation
        // `record`/`record_launch` above already prove for their own types.
        let run = Store::open(":memory:").unwrap();
        let progress = Store::open(":memory:").unwrap();

        record_stop_failure(
            &progress,
            "run-1",
            &StopFailure {
                spawn: "u104-fail-class/implementer#0".into(),
                class: "rate_limit".into(),
            },
        )
        .unwrap();

        assert!(
            run.read_stream(&stream_of("run-1"), 0, Direction::Forward)
                .unwrap()
                .is_empty(),
            "no StopFailure may land in the run store"
        );
        let p = progress
            .read_stream(&stream_of("run-1"), 0, Direction::Forward)
            .unwrap();
        assert_eq!(p.len(), 1);
        assert_eq!(p[0].type_, crate::progress::TYPE_STOP_FAILURE);
        assert_eq!(
            p[0].meta.get(META_RUN_ID).map(String::as_str),
            Some("run-1")
        );
        let sf: StopFailure = serde_json::from_slice(&p[0].data).unwrap();
        assert_eq!(sf.spawn, "u104-fail-class/implementer#0");
        assert_eq!(sf.class, "rate_limit");
    }

    #[test]
    fn record_stop_failure_a_report_the_store_did_not_write_is_reported_as_lost() {
        let err = record_stop_failure(
            &crate::eventstore::SilentStore,
            "run-1",
            &StopFailure {
                spawn: "u1/implementer#0".into(),
                class: "unknown".into(),
            },
        )
        .expect_err("a stop-failure record nobody can find was not recorded");
        let message = err.to_string();
        assert!(message.contains("u1/implementer#0"), "message: {message}");
    }

    #[test]
    fn append_stamped_appends_the_one_stamped_record_of_its_run() {
        // spec 105 LIVENESS: ApiRetry is a progress-store record with no writer of its own,
        // the one event this append adds to the run's own progress stream.
        let progress = Store::open(":memory:").unwrap();
        let retry = crate::progress::ApiRetry {
            spawn: "u105/implementer#0".into(),
            delay_ms: 618,
            category: "rate_limit".into(),
        };

        append_stamped(&progress, "run-1", &retry, "the api-retry record").unwrap();

        let p = crate::progress::read_run(&progress, "run-1").unwrap();
        assert_eq!(p.len(), 1);
        assert_eq!(p[0].type_, crate::progress::TYPE_API_RETRY);
        assert_eq!(
            p[0].meta.get(META_RUN_ID).map(String::as_str),
            Some("run-1")
        );
        assert_eq!(
            serde_json::from_slice::<crate::progress::ApiRetry>(&p[0].data).unwrap(),
            retry
        );
    }

    /// A record that does not decode names no ordinal, so it moves nothing and stops nothing:
    /// a store holding one would otherwise refuse every later launch of its run.
    fn an_undecodable_launch_record_is_skipped(seeded: &[u32], next: u32) {
        let progress = Store::open(":memory:").unwrap();
        for launch in seeded {
            let record = SpawnLaunched::closed("u/implementer#0", *launch, "s", "fault", "");
            record_launch(&progress, "run-1", &record).unwrap();
        }
        progress
            .append(
                &stream_of("run-1"),
                ExpectedRevision::Any,
                &[Event::new(
                    crate::progress::TYPE_SPAWN_LAUNCHED,
                    b"not a record".to_vec(),
                )],
            )
            .unwrap();

        assert_eq!(
            next_launch(&progress, "run-1", "u/implementer#0", 6).unwrap(),
            next
        );
    }

    crate::test_cases! {
        an_undecodable_record_alone_leaves_the_first_ordinal:
            an_undecodable_launch_record_is_skipped(&[], 6);
        an_undecodable_record_leaves_the_decoded_ones_deciding:
            an_undecodable_launch_record_is_skipped(&[2], 3);
    }

    /// One seeded launch record: its run, its spawn, its ordinal and its `ended`, an open record
    /// when `None`.
    type Seed = (&'static str, &'static str, u32, Option<&'static str>);

    /// The record `seed` appends, its session naming its place so an open and a close differ.
    fn seeded_launch(&(_, spawn, launch, ended): &Seed, place: usize) -> SpawnLaunched {
        let session = format!("s{place}");
        match ended {
            Some(ended) => SpawnLaunched::closed(spawn, launch, session, ended, ""),
            None => SpawnLaunched {
                spawn: spawn.into(),
                launch,
                session_id: session,
                started: 1,
                ..Default::default()
            },
        }
    }

    /// The latest record of `u/implementer#0`'s highest ordinal in `run-1`: the last record of
    /// that ordinal in stream order, its close once it closed, whatever the records of a lower
    /// ordinal, another spawn or another run say. `latest` is the seed's place, `None` when the
    /// run holds no record of the spawn.
    fn the_latest_launch_is_the_last_record_of_its_highest_ordinal(
        seeded: &[Seed],
        latest: Option<usize>,
    ) {
        let progress = Store::open(":memory:").unwrap();
        for (place, seed) in seeded.iter().enumerate() {
            record_launch(&progress, seed.0, &seeded_launch(seed, place)).unwrap();
        }

        assert_eq!(
            latest_launch(&progress, "run-1", "u/implementer#0").unwrap(),
            latest.map(|place| seeded_launch(&seeded[place], place))
        );
    }

    const SPAWN: &str = "u/implementer#0";

    crate::test_cases! {
        no_record_names_no_latest_launch:
            the_latest_launch_is_the_last_record_of_its_highest_ordinal(
                &[("run-0", SPAWN, 4, Some("interrupted"))],
                None,
            );
        another_spawns_records_name_no_latest_launch:
            the_latest_launch_is_the_last_record_of_its_highest_ordinal(
                &[("run-1", "u/reviewer#0", 0, None), ("run-1", "u/implementer#1", 1, None)],
                None,
            );
        an_open_launch_is_its_open_record:
            the_latest_launch_is_the_last_record_of_its_highest_ordinal(
                &[("run-1", SPAWN, 0, None)],
                Some(0),
            );
        a_closed_launch_is_its_close:
            the_latest_launch_is_the_last_record_of_its_highest_ordinal(
                &[("run-1", SPAWN, 0, None), ("run-1", SPAWN, 0, Some("interrupted"))],
                Some(1),
            );
        a_lower_ordinals_later_close_leaves_the_highest_deciding:
            the_latest_launch_is_the_last_record_of_its_highest_ordinal(
                &[
                    ("run-1", SPAWN, 1, None),
                    ("run-1", SPAWN, 1, Some("interrupted")),
                    ("run-1", SPAWN, 0, Some("fault")),
                ],
                Some(1),
            );
        a_higher_ordinal_opened_past_a_close_is_open:
            the_latest_launch_is_the_last_record_of_its_highest_ordinal(
                &[
                    ("run-1", SPAWN, 0, None),
                    ("run-1", SPAWN, 0, Some("interrupted")),
                    ("run-1", "u/reviewer#0", 3, Some("fault")),
                    ("run-1", SPAWN, 1, None),
                    ("run-0", SPAWN, 1, Some("interrupted")),
                    ("run-1", "u/reviewer#0", 3, Some("completed")),
                ],
                Some(3),
            );
    }

    #[test]
    fn append_stamped_a_report_the_store_did_not_write_is_reported_as_lost() {
        let err = append_stamped(
            &crate::eventstore::SilentStore,
            "run-1",
            &crate::progress::ApiRetry::default(),
            "the api-retry record",
        )
        .expect_err("a retry record nobody can find was not recorded");
        assert_eq!(
            err.to_string(),
            "event store: reported writing nothing for the api-retry record"
        );
    }
}
