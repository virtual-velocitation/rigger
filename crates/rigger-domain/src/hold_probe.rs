//! THE PROBE'S DELAY (spec 105 RELEASE): when the hold controller owes the open hold its next
//! probe, read from the run's hold events and the progress store's probe lines alone, so no other
//! probe state is stored.

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event_fixtures::at;
    use crate::eventstore::Event;
    use crate::progress::{AgentProgress, RunStamped};
    use crate::run::{
        RunHeld, RunReleased, HOLD_BY_HOST, HOLD_BY_OPERATOR, HOLD_BY_PROBE, TYPE_RUN_HELD,
        TYPE_RUN_RELEASED,
    };
    use std::time::{Duration, SystemTime};

    /// One run-stream entry of a row: a `RunHeld` of a cause by a writer, a `RunReleased`, or an
    /// event no hold reads, each at its valid time.
    enum Entry {
        Held(&'static str, &'static str, u64),
        Released(u64),
        Other(u64),
    }
    use Entry::{Held, Other, Released};

    /// The run slice `log` spells, each event at its index's revision.
    fn run_log(log: &[Entry]) -> Vec<Event> {
        log.iter()
            .enumerate()
            .map(|(revision, entry)| {
                let (type_, data, secs) = match entry {
                    Held(cause, by, secs) => {
                        let body = RunHeld {
                            cause: cause.to_string(),
                            by: by.to_string(),
                            model: format!("model of {cause}"),
                            ..RunHeld::default()
                        };
                        (TYPE_RUN_HELD, serde_json::to_vec(&body).unwrap(), *secs)
                    }
                    Released(secs) => {
                        let body = RunReleased::default();
                        (TYPE_RUN_RELEASED, serde_json::to_vec(&body).unwrap(), *secs)
                    }
                    Other(secs) => ("UnitStarted", br#"{"id":"u"}"#.to_vec(), *secs),
                };
                let mut event = Event::new(type_, data).with_valid_from(at(secs));
                event.revision = revision as i64;
                event
            })
            .collect()
    }

    /// A progress line of `id` stamped with `revision`, written at `secs`.
    fn line_of(id: &str, revision: Option<i64>, secs: u64) -> Event {
        AgentProgress {
            id: id.to_string(),
            activity: "a probe".to_string(),
            revision,
        }
        .to_stamped_event("r")
        .unwrap()
        .with_valid_from(at(secs))
    }

    /// The hold-probe line of a probe of the hold at `revision`, written at `secs`.
    fn probed(revision: i64, secs: u64) -> Event {
        line_of(HOLD_PROBE_ID, Some(revision), secs)
    }

    /// THE DELAY (spec 105 RELEASE), one pure function: no probe of an unheld run, an operator
    /// hold or a `model_not_found` hold; otherwise the delay is 60 s doubled per probe line inside
    /// the hold's chain, capped at 900 s, and the probe falls one delay after the later of the
    /// latest such line and the hold's latest `RunHeld`, clamped to one delay from now. A released
    /// hold joins the chain when the next hold's first `RunHeld` falls at most 600 s after its
    /// release and it is no operator hold, and the walk ends at the first hold that does not join.
    #[test]
    fn the_owed_probe_doubles_its_delay_per_probe_of_the_holds_chain() {
        let now = at(10_000);
        let host = |cause, secs| Held(cause, HOLD_BY_HOST, secs);
        let rate = |secs| host("rate_limit", secs);
        // (name, run log, probe lines, now, expected (cause, revision, delay s, due s))
        type Row = (
            &'static str,
            Vec<Entry>,
            Vec<Event>,
            SystemTime,
            Option<(&'static str, i64, u64, u64)>,
        );
        let rows: Vec<Row> =
            vec![
            ("an unheld run", vec![Other(100)], vec![], now, None),
            (
                "a released hold",
                vec![rate(100), Released(200)],
                vec![],
                now,
                None,
            ),
            (
                "an operator hold",
                vec![Held("operator", HOLD_BY_OPERATOR, 100)],
                vec![],
                now,
                None,
            ),
            (
                "a host hold an operator RunHeld landed on",
                vec![rate(100), Held("operator", HOLD_BY_OPERATOR, 200)],
                vec![],
                now,
                None,
            ),
            (
                "a model_not_found hold",
                vec![host("model_not_found", 100)],
                vec![],
                now,
                None,
            ),
            (
                "a fresh hold: 60 s after its RunHeld",
                vec![Other(50), rate(100)],
                vec![],
                now,
                Some(("rate_limit", 1, 60, 160)),
            ),
            (
                "one probe: 120 s after the line",
                vec![rate(100)],
                vec![probed(0, 200)],
                now,
                Some(("rate_limit", 0, 120, 320)),
            ),
            (
                "two probes: 240 s",
                vec![rate(100)],
                vec![probed(0, 200), probed(0, 300)],
                now,
                Some(("rate_limit", 0, 240, 540)),
            ),
            (
                "three probes: 480 s, the last under the cap",
                vec![rate(100)],
                (1..=3).map(|n| probed(0, 100 * n)).collect(),
                now,
                Some(("rate_limit", 0, 480, 780)),
            ),
            (
                "four probes: capped at 900 s, not 960",
                vec![rate(100)],
                (1..=4).map(|n| probed(0, 100 * n)).collect(),
                now,
                Some(("rate_limit", 0, 900, 1_300)),
            ),
            (
                "five probes: still 900 s",
                vec![rate(100)],
                (1..=5).map(|n| probed(0, 100 * n)).collect(),
                now,
                Some(("rate_limit", 0, 900, 1_400)),
            ),
            (
                "a RunHeld later than the latest line times the probe, the latest naming it",
                vec![rate(100), Held("billing_error", HOLD_BY_PROBE, 300)],
                vec![probed(0, 200)],
                now,
                Some(("billing_error", 1, 120, 420)),
            ),
            (
                "a line at a later revision of the open hold counts",
                vec![rate(100), Other(150)],
                vec![probed(1, 200)],
                now,
                Some(("rate_limit", 0, 120, 320)),
            ),
            (
                "a hold re-held exactly 600 s after its release joins the chain",
                vec![rate(100), Released(200), rate(800)],
                vec![probed(0, 150)],
                now,
                Some(("rate_limit", 2, 120, 920)),
            ),
            (
                "a hold re-held 601 s after its release starts a chain of its own",
                vec![rate(100), Released(200), rate(801)],
                vec![probed(0, 150)],
                now,
                Some(("rate_limit", 2, 60, 861)),
            ),
            (
                "an operator hold ends the walk, however quick the re-hold",
                vec![
                    Held("operator", HOLD_BY_OPERATOR, 100),
                    Released(200),
                    rate(300),
                ],
                vec![probed(0, 150)],
                now,
                Some(("rate_limit", 2, 60, 360)),
            ),
            (
                "the chain runs back through every joining hold",
                vec![
                    rate(100),
                    Released(200),
                    rate(300),
                    Released(400),
                    rate(500),
                ],
                vec![probed(0, 150), probed(2, 350)],
                now,
                Some(("rate_limit", 4, 240, 740)),
            ),
            (
                "the walk ends at the first hold that does not join, a joining pair behind it too",
                vec![
                    rate(100),
                    Released(200),
                    rate(300),
                    Released(400),
                    rate(1_100),
                ],
                vec![probed(0, 150), probed(2, 350)],
                now,
                Some(("rate_limit", 4, 60, 1_160)),
            ),
            (
                "a line at a joined hold's release revision is outside it",
                vec![rate(100), Released(200), rate(300)],
                vec![probed(1, 250)],
                now,
                Some(("rate_limit", 2, 60, 360)),
            ),
            (
                "a line of another id or with no revision counts nothing",
                vec![rate(100)],
                vec![line_of("u/implementer#0", Some(0), 200), line_of(HOLD_PROBE_ID, None, 300)],
                now,
                Some(("rate_limit", 0, 60, 160)),
            ),
            (
                "a hold dated now falls one delay from now",
                vec![rate(5_000)],
                vec![],
                at(5_000),
                Some(("rate_limit", 0, 60, 5_060)),
            ),
            (
                "a hold dated ahead of now is clamped to one delay from now",
                vec![rate(5_500)],
                vec![],
                at(5_000),
                Some(("rate_limit", 0, 60, 5_060)),
            ),
        ];
        for (name, log, lines, now, expected) in rows {
            let owed = owed_probe(&run_log(&log), &lines, now);
            let expected = expected.map(|(cause, revision, delay, due)| OwedProbe {
                held: RunHeld {
                    cause: cause.to_string(),
                    by: if cause == "billing_error" {
                        HOLD_BY_PROBE
                    } else {
                        HOLD_BY_HOST
                    }
                    .to_string(),
                    model: format!("model of {cause}"),
                    ..RunHeld::default()
                },
                revision,
                delay: Duration::from_secs(delay),
                due: at(due),
            });
            assert_eq!(owed, expected, "{name}");
        }
    }

    /// A probe of a credential cause reads the login first; any other cause runs the canary alone.
    #[test]
    fn only_a_credential_cause_checks_the_login() {
        let rows = [
            ("authentication_failed", true),
            ("oauth_org_not_allowed", true),
            ("cloud_credential_error", false),
            ("billing_error", false),
            ("rate_limit", false),
            (crate::agent::API_UNSTABLE, false),
        ];
        for (cause, expected) in rows {
            let log = run_log(&[Held(cause, HOLD_BY_HOST, 100)]);
            let owed = owed_probe(&log, &[], at(10_000)).unwrap();
            assert_eq!(owed.checks_login(), expected, "{cause}");
        }
    }

    /// The probe line: the constant id, the cause and the delay that bound it, and the revision
    /// of the probed hold's latest `RunHeld`.
    #[test]
    fn a_probe_line_names_its_hold_and_delay_at_the_probed_revision() {
        let log = run_log(&[Held("billing_error", HOLD_BY_HOST, 100), Other(150)]);
        let owed = owed_probe(&log, &[probed(0, 200)], at(10_000)).unwrap();
        assert_eq!(
            owed.line(),
            AgentProgress {
                id: "hold-probe".to_string(),
                activity: "probing the billing_error hold within 120 s".to_string(),
                revision: Some(0),
            }
        );
    }
}
