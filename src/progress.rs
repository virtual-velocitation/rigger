//! The progress telemetry's tests: [`rigger_domain::progress`] reached through the facade,
//! beside the crate's spawn-request test fixture they build on.

pub use rigger_domain::progress::*;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eventstore::Event;
    use crate::run::META_RUN_ID;
    use crate::spawn::SpawnEvent;
    use std::collections::HashMap;
    use std::time::SystemTime;

    #[test]
    fn consolidate_joins_frontier_progress_liveness_and_milestone() {
        // spec 14, criterion 2: for each in-flight spawn the consolidator yields its stage +
        // latest activity + activity-age + liveness-age + last milestone (and the milestone's
        // age - the blackout). The clock is passed in, so the ages are deterministic.
        use std::time::Duration;

        let now = SystemTime::UNIX_EPOCH + Duration::from_secs(10_000);

        // The unit started 5 min ago; its implementer is parked (in-flight, no result).
        let mut started = Event::new("UnitStarted", b"{\"id\":\"u\"}".to_vec());
        started.recorded_at = now - Duration::from_secs(300);
        let req = crate::spawn::test_request("u", "u", "implementer", 0, "do it");
        let run_events = vec![started, req.to_event().unwrap()];

        // Two progress reports; the later one (20s ago) is the current activity.
        let mut p1 = AgentProgress {
            id: req.id.clone(),
            activity: "grep #1".into(),
        }
        .to_stamped_event("run-1")
        .unwrap();
        p1.recorded_at = now - Duration::from_secs(200);
        let mut p2 = AgentProgress {
            id: req.id.clone(),
            activity: "grep #12: conductor.rs".into(),
        }
        .to_stamped_event("run-1")
        .unwrap();
        p2.recorded_at = now - Duration::from_secs(20);
        let progress_events = vec![p1, p2];

        let liveness = HashMap::from([(req.id.clone(), 20u64)]);

        let view = consolidate(&run_events, &progress_events, &liveness, now).unwrap();
        assert_eq!(view.len(), 1, "one in-flight spawn");
        let a = &view[0];
        assert_eq!(a.id, req.id);
        assert_eq!(a.unit, "u");
        assert_eq!(a.stage, "u");
        assert_eq!(
            a.latest_activity.as_deref(),
            Some("grep #12: conductor.rs"),
            "the LATEST report wins"
        );
        assert_eq!(a.activity_age_s, Some(20));
        assert_eq!(a.liveness_age_s, Some(20));
        assert_eq!(a.last_milestone.as_deref(), Some("UnitStarted"));
        assert_eq!(
            a.milestone_age_s,
            Some(300),
            "the blackout: 5 min since the last store event, vs 20s of live activity"
        );
    }

    #[test]
    fn consolidate_reports_none_for_a_spawn_that_has_not_yet_progressed() {
        // An in-flight spawn with no progress and no marker yet: it still appears (from the
        // frontier), with the activity/liveness fields absent rather than fabricated.
        let now = SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1);
        let req = crate::spawn::test_request("u", "u", "adjudicator", 0, "judge");
        let view = consolidate(&[req.to_event().unwrap()], &[], &HashMap::new(), now).unwrap();
        assert_eq!(view.len(), 1);
        assert_eq!(view[0].latest_activity, None);
        assert_eq!(view[0].activity_age_s, None);
        assert_eq!(view[0].liveness_age_s, None);
        assert_eq!(view[0].last_milestone, None);
    }

    #[test]
    fn spawn_launched_to_event_carries_the_type_and_run_stamp() {
        // spec 104 criterion 1: the launch record serializes as SpawnLaunched, stamped
        // with its run exactly like AgentProgress - the event boundary this store's
        // isolation depends on, not a new mechanism.
        let launched = SpawnLaunched {
            spawn: "u104-launch/implementer#0".into(),
            launch: 0,
            session_id: "11111111-1111-4111-8111-111111111111".into(),
            resumed_from: None,
            started: 1_700_000_000,
            ended: None,
            class: None,
        };
        let ev = launched.to_stamped_event("run-9").unwrap();
        assert_eq!(ev.type_, TYPE_SPAWN_LAUNCHED);
        assert_eq!(ev.meta.get(META_RUN_ID).map(String::as_str), Some("run-9"));
        let back: SpawnLaunched = serde_json::from_slice(&ev.data).unwrap();
        assert_eq!(back, launched);
    }

    #[test]
    fn spawn_launched_omits_resumed_from_when_none() {
        // A fresh launch (every launch spec 104 itself performs) must not serialize a
        // `resumed_from` field at all - the future resume reader (spec 105) can then
        // treat "field absent" and "field null" the same way, and today's payload stays
        // minimal.
        let launched = SpawnLaunched {
            spawn: "u/implementer#0".into(),
            launch: 0,
            session_id: "s".into(),
            resumed_from: None,
            started: 1,
            ended: None,
            class: None,
        };
        let ev = launched.to_stamped_event("").unwrap();
        let v: serde_json::Value = serde_json::from_slice(&ev.data).unwrap();
        assert!(
            v.get("resumed_from").is_none(),
            "resumed_from must be omitted, not null: {v:?}"
        );
        assert!(
            v.get("ended").is_none(),
            "ended must be omitted while open, not null: {v:?}"
        );
        assert!(
            v.get("class").is_none(),
            "class must be omitted while open, not null: {v:?}"
        );
        assert!(
            !ev.meta.contains_key(META_RUN_ID),
            "an empty run_id carries no stamp, same as AgentProgress"
        );
    }

    #[test]
    fn spawn_launched_carries_a_relaunchs_resumed_from() {
        let launched = SpawnLaunched {
            spawn: "u/implementer#0".into(),
            launch: 1,
            session_id: "new-session".into(),
            resumed_from: Some("old-session".into()),
            started: 2,
            ended: None,
            class: None,
        };
        let ev = launched.to_stamped_event("run-1").unwrap();
        let back: SpawnLaunched = serde_json::from_slice(&ev.data).unwrap();
        assert_eq!(back.resumed_from.as_deref(), Some("old-session"));
        assert_eq!(back.launch, 1);
    }

    // ---- ended/class closing (spec 104 criteria 5/6) ----

    /// `SpawnLaunched::closed` over `ended`/`class` records `ended` and the class as `class`
    /// (an empty class stored as `None`); the record is returned for further checks.
    fn assert_closed(ended: &str, class: &str, want_class: Option<&str>) -> SpawnLaunched {
        let closing = SpawnLaunched::closed("u/implementer#0", 0, "sess-1", ended, class);
        assert_eq!(closing.ended.as_deref(), Some(ended));
        assert_eq!(closing.class.as_deref(), want_class);
        closing
    }

    crate::test_cases! {
        spawn_launched_closed_builds_a_closing_record_with_ended_and_class: {
            let closing = assert_closed("stopped", "infra", Some("infra"));
            assert_eq!(closing.spawn, "u/implementer#0");
            assert_eq!(closing.launch, 0);
            assert_eq!(closing.session_id, "sess-1");
        };
        // completed/interrupted carry no failure class.
        spawn_launched_closed_stores_an_empty_class_as_none: assert_closed("completed", "", None);
    }

    #[test]
    fn spawn_launched_closed_round_trips_through_json() {
        let closing = SpawnLaunched::closed("u/implementer#0", 2, "sess-9", "fault", "rate_limit");
        let ev = closing.to_stamped_event("run-1").unwrap();
        let back: SpawnLaunched = serde_json::from_slice(&ev.data).unwrap();
        assert_eq!(back, closing);
    }

    // ---- StopFailure (spec 104 criterion 5: A FAILURE HAS A CLASS) ----

    #[test]
    fn stop_failure_to_event_carries_the_type_and_run_stamp() {
        let sf = StopFailure {
            spawn: "u104-fail-class/implementer#0".into(),
            class: "authentication_failed".into(),
        };
        let ev = sf.to_stamped_event("run-9").unwrap();
        assert_eq!(ev.type_, TYPE_STOP_FAILURE);
        assert_eq!(ev.meta.get(META_RUN_ID).map(String::as_str), Some("run-9"));
        let back: StopFailure = serde_json::from_slice(&ev.data).unwrap();
        assert_eq!(back, sf);
    }

    #[test]
    fn stop_failure_omits_the_run_stamp_when_run_id_is_empty() {
        let sf = StopFailure {
            spawn: "u/implementer#0".into(),
            class: "rate_limit".into(),
        };
        let ev = sf.to_stamped_event("").unwrap();
        assert!(
            !ev.meta.contains_key(META_RUN_ID),
            "an empty run_id carries no stamp, same as SpawnLaunched/AgentProgress"
        );
    }

    #[test]
    fn latest_stop_failure_class_returns_none_when_no_record_names_the_spawn() {
        let events = vec![StopFailure {
            spawn: "other/implementer#0".into(),
            class: "rate_limit".into(),
        }
        .to_stamped_event("run-1")
        .unwrap()];
        assert_eq!(latest_stop_failure_class(&events, "u/implementer#0"), None);
    }

    #[test]
    fn latest_stop_failure_class_the_latest_record_for_the_spawn_wins() {
        // Two firings for the SAME spawn (a relaunch can each carry its own) - append order
        // breaks the tie, "latest wins", exactly like AgentProgress's own per-id fold.
        let events = vec![
            StopFailure {
                spawn: "u/implementer#0".into(),
                class: "rate_limit".into(),
            }
            .to_stamped_event("run-1")
            .unwrap(),
            StopFailure {
                spawn: "other/implementer#0".into(),
                class: "overloaded".into(),
            }
            .to_stamped_event("run-1")
            .unwrap(),
            StopFailure {
                spawn: "u/implementer#0".into(),
                class: "authentication_failed".into(),
            }
            .to_stamped_event("run-1")
            .unwrap(),
        ];
        assert_eq!(
            latest_stop_failure_class(&events, "u/implementer#0").as_deref(),
            Some("authentication_failed")
        );
    }

    #[test]
    fn latest_stop_failure_class_ignores_a_differently_typed_event() {
        let mut events = vec![AgentProgress {
            id: "u/implementer#0".into(),
            activity: "not a stop failure".into(),
        }
        .to_stamped_event("run-1")
        .unwrap()];
        events.push(
            StopFailure {
                spawn: "u/implementer#0".into(),
                class: "billing_error".into(),
            }
            .to_stamped_event("run-1")
            .unwrap(),
        );
        assert_eq!(
            latest_stop_failure_class(&events, "u/implementer#0").as_deref(),
            Some("billing_error")
        );
    }
}
