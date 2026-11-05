#[cfg(test)]
mod tests {
    use std::sync::atomic::AtomicBool;
    use std::sync::Mutex;
    use std::time::{Duration, SystemTime};

    use rigger_domain::hold_probe::{HoldProbe, OwedProbe, ProbeOutcome, HOLD_PROBE_ID};
    use rigger_domain::progress::{decoded_progress, read_run};

    use super::probe_an_owed_hold;
    use crate::eventstore::sqlite::Store;
    use crate::eventstore::{Direction, Event, EventStore, ExpectedRevision};
    use crate::run::{
        RunHeld, RunReleased, HOLD_BY_HOST, HOLD_BY_OPERATOR, STREAM, TYPE_RUN_HELD,
        TYPE_RUN_RELEASED,
    };
    use crate::run_store;
    use crate::test_support::{held_event, ReadsFailWhenArmed};

    /// The project root the controller hands the probe.
    const ROOT: &str = "the/project/root";
    /// What the store says when its reads are broken: [`ReadsFailWhenArmed`]'s backend error.
    const READ_FAILED: &str = "event store: the run read failed";
    /// How far past the wall clock a due row polls: past a first probe's 60 s delay.
    const PAST_THE_DELAY: Duration = Duration::from_secs(120);
    /// A hold's cause that needs no person, one that does and one a login fixes.
    const WAIT: &str = "rate_limit";
    const HOLD: &str = "billing_error";
    const LOGIN: &str = "authentication_failed";
    /// The line a first probe of a [`WAIT`] or [`LOGIN`] hold records.
    const WAIT_LINE: &str = "probing the rate_limit hold within 60 s";
    const LOGIN_LINE: &str = "probing the authentication_failed hold within 60 s";

    /// One poll of the controller over a run started fresh, then `seeded`.
    struct Row {
        name: &'static str,
        seeded: Vec<Event>,
        /// How far past the wall clock the poll's `now` falls.
        ahead: Duration,
        /// Whether every run read fails from the start.
        reads_fail: bool,
        /// What the probe answers.
        outcome: ProbeOutcome,
        /// What lands on the run stream while the probe runs.
        lands: Option<Event>,
        want: Want,
    }

    /// What one poll must leave.
    struct Want {
        /// The cause of the hold the poll probed, `None` for a poll that made no probe.
        probed: Option<&'static str>,
        /// The hold-probe lines on the run's progress stream after the poll, the line of a probe
        /// already there as the probe starts.
        lines: Vec<&'static str>,
        /// The run-stream events past the seeded ones, each as [`said`] spells it.
        appended: Vec<&'static str>,
        /// What the poll returns.
        returned: Result<(), &'static str>,
    }

    /// A probe double: it records the root it probes in, the cause of the hold it probes and the
    /// hold-probe lines the progress store holds as it starts, then appends `lands` to the run
    /// stream and answers `outcome`.
    struct FakeProbe<'a> {
        run_store: &'a dyn EventStore,
        progress: &'a dyn EventStore,
        run: &'a str,
        outcome: ProbeOutcome,
        lands: Option<Event>,
        probes: Mutex<Vec<(String, String, Vec<String>)>>,
    }

    impl HoldProbe for FakeProbe<'_> {
        fn probe(&self, root: &str, owed: &OwedProbe) -> ProbeOutcome {
            let seen = probe_lines(self.progress, self.run);
            self.probes
                .lock()
                .unwrap()
                .push((root.to_string(), owed.held.cause.clone(), seen));
            if let Some(event) = &self.lands {
                self.run_store
                    .append(STREAM, ExpectedRevision::Any, std::slice::from_ref(event))
                    .unwrap();
            }
            self.outcome.clone()
        }
    }

    /// The activities of the hold-probe lines on run `run`'s progress stream, in order.
    fn probe_lines(progress: &dyn EventStore, run: &str) -> Vec<String> {
        decoded_progress(&read_run(progress, run).unwrap())
            .filter(|(_, line)| line.id == HOLD_PROBE_ID)
            .map(|(_, line)| line.activity)
            .collect()
    }

    /// A run-stream event as a row spells it: a hold by its cause and writer, a release by its
    /// writer, anything else by its type.
    fn said(event: &Event) -> String {
        match event.type_.as_str() {
            TYPE_RUN_HELD => {
                let held: RunHeld = event.decode().unwrap();
                format!("RunHeld {} by {}", held.cause, held.by)
            }
            TYPE_RUN_RELEASED => {
                let released: RunReleased = event.decode().unwrap();
                format!("RunReleased by {}", released.by)
            }
            other => other.to_string(),
        }
    }

    /// Poll each row once through the real controller over real stores and the fake probe, and
    /// assert what it returned, the probe it made, the lines it recorded and what it appended.
    fn check(rows: Vec<Row>) {
        for row in rows {
            let name = row.name;
            let store = Store::open(":memory:").unwrap();
            let progress = Store::open(":memory:").unwrap();
            let run = run_store::ensure_started(&store, &["c".to_string()]).unwrap();
            if !row.seeded.is_empty() {
                store
                    .append(STREAM, ExpectedRevision::Any, &row.seeded)
                    .unwrap();
            }
            let head = store
                .read_stream(STREAM, 0, Direction::Forward)
                .unwrap()
                .len();
            let armed = AtomicBool::new(row.reads_fail);
            let reads = ReadsFailWhenArmed {
                inner: &store,
                armed: &armed,
            };
            let probe = FakeProbe {
                run_store: &store,
                progress: &progress,
                run: &run,
                outcome: row.outcome,
                lands: row.lands,
                probes: Mutex::new(Vec::new()),
            };
            let now = SystemTime::now() + row.ahead;
            let returned = probe_an_owed_hold(&reads, &progress, &probe, ROOT, now);
            let want = row.want;
            assert_eq!(
                returned,
                want.returned.map_err(str::to_string),
                "{name}: what the poll returns"
            );
            let made: Vec<(String, String, Vec<String>)> = want
                .probed
                .into_iter()
                .map(|cause| {
                    let lines = want.lines.iter().map(|line| line.to_string()).collect();
                    (ROOT.to_string(), cause.to_string(), lines)
                })
                .collect();
            assert_eq!(
                probe.probes.into_inner().unwrap(),
                made,
                "{name}: the probes made, each in the root with its line already recorded"
            );
            assert_eq!(
                probe_lines(&progress, &run),
                want.lines,
                "{name}: the hold-probe lines"
            );
            let tail: Vec<String> = store.read_stream(STREAM, 0, Direction::Forward).unwrap()
                [head..]
                .iter()
                .map(said)
                .collect();
            assert_eq!(
                tail, want.appended,
                "{name}: the run-stream events appended"
            );
        }
    }

    /// A poll that makes no probe and writes nothing.
    fn nothing() -> Want {
        Want {
            probed: None,
            lines: Vec::new(),
            appended: Vec::new(),
            returned: Ok(()),
        }
    }

    /// A poll of `ahead` over `seeded` whose probe would answer `outcome`, with nothing landing
    /// while it runs and every read answering.
    fn row(
        name: &'static str,
        seeded: Vec<Event>,
        ahead: Duration,
        outcome: ProbeOutcome,
        want: Want,
    ) -> Row {
        Row {
            name,
            seeded,
            ahead,
            reads_fail: false,
            outcome,
            lands: None,
            want,
        }
    }

    /// THE HOLD CONTROLLER'S POLL (spec 105 RELEASE), over a fake probe: a probe falls due only
    /// for a host's hold one delay past its `RunHeld`, its line recorded before it starts; a
    /// passing probe releases the hold it probed and no hold re-declared since, a failed probe
    /// moves the cause only to a person-needing category, a logged-out session leaves the hold as
    /// it stands, and a read that fails ends the poll with its error before any line or probe.
    #[test]
    fn a_poll_probes_a_due_hold_once_its_line_is_recorded_and_writes_what_the_outcome_decides() {
        let passed = || ProbeOutcome::Passed;
        let wait_hold = || vec![held_event(WAIT, HOLD_BY_HOST)];
        let probed = |cause, appended| Want {
            probed: Some(cause),
            lines: vec![if cause == LOGIN {
                LOGIN_LINE
            } else {
                WAIT_LINE
            }],
            appended,
            returned: Ok(()),
        };
        check(vec![
            row(
                "an unheld run owes no probe",
                Vec::new(),
                PAST_THE_DELAY,
                passed(),
                nothing(),
            ),
            row(
                "a hold not yet one delay old makes no probe",
                wait_hold(),
                Duration::ZERO,
                passed(),
                nothing(),
            ),
            row(
                "an operator hold is never probed",
                vec![held_event(WAIT, HOLD_BY_OPERATOR)],
                PAST_THE_DELAY,
                passed(),
                nothing(),
            ),
            row(
                "a passing probe releases the hold it probed",
                wait_hold(),
                PAST_THE_DELAY,
                passed(),
                probed(WAIT, vec!["RunReleased by probe"]),
            ),
            row(
                "a failed probe's person-needing category moves the cause",
                wait_hold(),
                PAST_THE_DELAY,
                ProbeOutcome::Failed(HOLD.to_string()),
                probed(WAIT, vec!["RunHeld billing_error by probe"]),
            ),
            row(
                "a failed probe on the hold's own cause appends nothing",
                wait_hold(),
                PAST_THE_DELAY,
                ProbeOutcome::Failed(WAIT.to_string()),
                probed(WAIT, Vec::new()),
            ),
            row(
                "a logged-out session leaves the hold as it stands",
                vec![held_event(LOGIN, HOLD_BY_HOST)],
                PAST_THE_DELAY,
                ProbeOutcome::LoggedOut,
                probed(LOGIN, Vec::new()),
            ),
            Row {
                lands: Some(held_event(WAIT, HOLD_BY_HOST)),
                ..row(
                    "a hold re-declared while a passing probe runs is not released by it",
                    wait_hold(),
                    PAST_THE_DELAY,
                    passed(),
                    probed(WAIT, vec!["RunHeld rate_limit by host"]),
                )
            },
            Row {
                reads_fail: true,
                ..row(
                    "a run read that fails ends the poll with its error, no line or probe made",
                    wait_hold(),
                    PAST_THE_DELAY,
                    passed(),
                    Want {
                        returned: Err(READ_FAILED),
                        ..nothing()
                    },
                )
            },
        ]);
    }
}
