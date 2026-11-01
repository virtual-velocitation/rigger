#[cfg(test)]
mod tests {
    use std::collections::VecDeque;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Mutex;

    use rigger_domain::agent::{
        no_result_error, parked_spawn, AgentDriver, AgentFailure, AgentResult, Error, SpawnOpts,
        API_UNSTABLE, EMIT_REFUSED_MARKER,
    };
    use serde_json::Value;

    use super::HeldDriver;
    use crate::config::AgentDef;
    use crate::eventstore::sqlite::Store;
    use crate::eventstore::{Event, EventStore, ExpectedRevision};
    use crate::run::{HOLD_BY_HOST, HOLD_BY_OPERATOR, STREAM, TYPE_RUN_HELD};
    use crate::run_store;
    use crate::spawn::{SpawnEvent, SpawnResult, TYPE_SPAWN_RESULT};
    use crate::test_support::{
        agent, ev, held_event, implementer_opts, no_emit, released_event, run_started,
        ReadsFailWhenArmed,
    };

    /// The spawn each row launches, and a second spawn id for the rows the bound keeps apart.
    const ID: &str = "solo/implementer#0";
    const SIBLING: &str = "other/implementer#0";
    /// What a launch that ends well answers.
    const OUTPUT: &str = "the worker's output";
    /// What the store says when its reads are broken: [`ReadsFailWhenArmed`]'s backend error.
    const READ_FAILED: &str = "event store: the run read failed";
    /// A `RunHeld` body the fold refuses, and what the fold says of it.
    const MALFORMED_HOLD: &str = r#"{"by":1}"#;
    const MALFORMED_HOLD_SAID: &str =
        "invalid type: integer `1`, expected a string at line 1 column 7";

    /// A category of each disposition.
    const FAULT: AgentFailure = AgentFailure::InvalidRequest;
    const WAIT: AgentFailure = AgentFailure::RateLimit;
    const HOLD: AgentFailure = AgentFailure::BillingError;

    /// The error a session ending on `class` returns.
    fn classed(class: AgentFailure) -> Error {
        no_result_error(class, "the session ended on its class".into())
    }

    /// An error carrying no class, as the host's wall-clock stop returns.
    fn unclassed() -> Error {
        Error("the session stopped".into())
    }

    /// The error the emit guard's refusal returns.
    fn refused() -> Error {
        Error(format!("{EMIT_REFUSED_MARKER}the guard refused the record"))
    }

    /// The hold the host appends on the third failed close, before it returns its error.
    fn third_fault_hold() -> Event {
        held_event(API_UNSTABLE, HOLD_BY_HOST)
    }

    /// The real result that ended spawn [`ID`].
    fn ended() -> Event {
        SpawnResult::ok(ID, OUTPUT).to_event().unwrap()
    }

    /// One scripted launch: the run-stream events the host appends before it returns, then what
    /// it returns.
    struct Launch {
        appends: Vec<Event>,
        outcome: Result<&'static str, Error>,
        breaks_reads: bool,
    }

    impl Launch {
        /// This launch appends `event` too, ahead of what it returns.
        fn appending(mut self, event: Event) -> Self {
            self.appends.push(event);
            self
        }

        /// The store's reads fail from this launch on.
        fn breaking_reads(mut self) -> Self {
            self.breaks_reads = true;
            self
        }
    }

    /// A launch that ends well.
    fn done() -> Launch {
        Launch {
            appends: Vec::new(),
            outcome: Ok(OUTPUT),
            breaks_reads: false,
        }
    }

    /// A launch that returns `e` and appends nothing.
    fn fails(e: Error) -> Launch {
        Launch {
            outcome: Err(e),
            ..done()
        }
    }

    /// A launch that appends the third-fault hold, then returns `e`.
    fn held_then_fails(e: Error) -> Launch {
        fails(e).appending(third_fault_hold())
    }

    /// A host double playing one scripted launch per spawn. It appends a launch's events before
    /// it returns, as the real host appends its hold before its error, and panics on a launch
    /// past the script, so a relaunch no row expects fails that row.
    struct Scripted<'a> {
        store: &'a Store,
        armed: &'a AtomicBool,
        script: Mutex<VecDeque<Launch>>,
        launched: Mutex<Vec<String>>,
    }

    impl AgentDriver for Scripted<'_> {
        fn spawn(
            &self,
            _agent: &AgentDef,
            _prompt: &str,
            opts: &SpawnOpts,
            _emit: &dyn Fn(&str, Value) -> Result<(), Error>,
        ) -> Result<AgentResult, Error> {
            self.launched.lock().unwrap().push(opts.id.clone());
            let launch = self
                .script
                .lock()
                .unwrap()
                .pop_front()
                .unwrap_or_else(|| panic!("{} launched past the script", opts.id));
            self.store
                .append(STREAM, ExpectedRevision::Any, &launch.appends)
                .unwrap();
            if launch.breaks_reads {
                self.armed.store(true, Ordering::SeqCst);
            }
            launch.outcome.map(|output| AgentResult {
                output: output.to_string(),
                ..Default::default()
            })
        }
    }

    /// The session flag a row's decorator runs under.
    #[derive(Clone, Copy, Default)]
    enum Session {
        /// `rigger run`: no session, so only a release ends a wait.
        #[default]
        Absent,
        /// A `rigger serve` session still open.
        Open,
        /// A `rigger serve` session already closed.
        Closed,
    }

    /// What a row's pause does, standing in for the world moving while the decorator sleeps.
    #[derive(Clone, Copy, Default)]
    enum AtPause {
        /// The hold is released once the row has polled it [`Row::polls`] times.
        #[default]
        Release,
        /// The session closes.
        CloseSession,
        /// The store's reads start failing.
        BreakReads,
    }

    /// One scenario: a run stream, a scripted host, the spawn calls made in order, and every
    /// launch and pause they must add up to.
    struct Row {
        name: &'static str,
        seeded: Vec<Event>,
        session: Session,
        at_pause: AtPause,
        polls: usize,
        reads_fail: bool,
        script: Vec<Launch>,
        /// Each spawn call in order, with the id it asks for and what it must return.
        calls: Vec<(&'static str, Result<&'static str, Error>)>,
        /// The id of every launch the host saw, in order.
        launched: Vec<&'static str>,
        /// How many launches had happened at each pause, so a pause before the first launch
        /// reads 0 and a wait after the first error reads 1.
        paused_at: Vec<usize>,
    }

    impl Default for Row {
        fn default() -> Self {
            Row {
                name: "",
                seeded: Vec::new(),
                session: Session::default(),
                at_pause: AtPause::default(),
                polls: 1,
                reads_fail: false,
                script: Vec::new(),
                calls: Vec::new(),
                launched: Vec::new(),
                paused_at: Vec::new(),
            }
        }
    }

    /// One spawn of [`ID`] that must return `wants`.
    fn one_call(
        wants: Result<&'static str, Error>,
    ) -> Vec<(&'static str, Result<&'static str, Error>)> {
        vec![(ID, wants)]
    }

    /// Run `rows` through the real decorator over a real store and assert each exactly: every
    /// spawn's output or error byte for byte, the ordered launches, and the launches standing at
    /// each pause.
    fn check(rows: Vec<Row>) {
        for row in rows {
            let name = row.name;
            let store = Store::open(":memory:").unwrap();
            run_store::ensure_started(&store, &["c".to_string()]).unwrap();
            store
                .append(STREAM, ExpectedRevision::Any, &row.seeded)
                .unwrap();
            let armed = AtomicBool::new(row.reads_fail);
            let reads = ReadsFailWhenArmed {
                inner: &store,
                armed: &armed,
            };
            let host = Scripted {
                store: &store,
                armed: &armed,
                script: Mutex::new(row.script.into()),
                launched: Mutex::new(Vec::new()),
            };
            let closed = AtomicBool::new(matches!(row.session, Session::Closed));
            let session = match row.session {
                Session::Absent => None,
                Session::Open | Session::Closed => Some(&closed),
            };
            let paused_at = Mutex::new(Vec::new());
            let pause = || {
                let mut paused = paused_at.lock().unwrap();
                paused.push(host.launched.lock().unwrap().len());
                match row.at_pause {
                    AtPause::Release if paused.len() % row.polls == 0 => {
                        store
                            .append(
                                STREAM,
                                ExpectedRevision::Any,
                                &[released_event(HOLD_BY_OPERATOR)],
                            )
                            .unwrap();
                    }
                    AtPause::Release => {}
                    AtPause::CloseSession => closed.store(true, Ordering::SeqCst),
                    AtPause::BreakReads => armed.store(true, Ordering::SeqCst),
                }
            };
            let held = HeldDriver::new(&host, &reads, session, &pause);
            for (call, (id, wants)) in row.calls.into_iter().enumerate() {
                let answer = held
                    .spawn(
                        &agent("implementer"),
                        "task",
                        &implementer_opts(id),
                        &no_emit,
                    )
                    .map(|result| result.output)
                    .map_err(|e| e.0);
                assert_eq!(
                    answer,
                    wants.map(str::to_string).map_err(|e| e.0),
                    "{name}: call {call}"
                );
            }
            assert_eq!(
                *host.launched.lock().unwrap(),
                row.launched,
                "{name}: launches"
            );
            assert_eq!(*paused_at.lock().unwrap(), row.paused_at, "{name}: pauses");
        }
    }

    /// HELD MEANS NO LAUNCH: a hold standing at a launch keeps the host unlaunched through every
    /// poll, and only an open hold of this run does.
    #[test]
    fn a_held_run_launches_nothing_until_its_release() {
        check(vec![
            Row {
                name: "an unheld run launches at once",
                script: vec![done()],
                calls: one_call(Ok(OUTPUT)),
                launched: vec![ID],
                ..Row::default()
            },
            Row {
                name: "a standing hold stands through every poll with nothing launched",
                seeded: vec![third_fault_hold()],
                polls: 3,
                script: vec![done()],
                calls: one_call(Ok(OUTPUT)),
                launched: vec![ID],
                paused_at: vec![0, 0, 0],
                ..Row::default()
            },
            Row {
                name: "an open session waits as a run with no session does",
                seeded: vec![third_fault_hold()],
                session: Session::Open,
                polls: 2,
                script: vec![done()],
                calls: one_call(Ok(OUTPUT)),
                launched: vec![ID],
                paused_at: vec![0, 0],
                ..Row::default()
            },
            Row {
                name: "a released hold waits for nothing",
                seeded: vec![third_fault_hold(), released_event(HOLD_BY_OPERATOR)],
                script: vec![done()],
                calls: one_call(Ok(OUTPUT)),
                launched: vec![ID],
                ..Row::default()
            },
            Row {
                name: "a hold of an earlier run waits for nothing",
                seeded: vec![third_fault_hold(), run_started("r2", &["c"])],
                script: vec![done()],
                calls: one_call(Ok(OUTPUT)),
                launched: vec![ID],
                ..Row::default()
            },
        ]);
    }

    /// THE DISPOSITION: an error relaunches the same id when its class holds or waits, or when
    /// the run is held whatever its class or none, the read following the host's return; any
    /// other error passes to its spawn site's path.
    #[test]
    fn an_error_relaunches_as_its_class_and_the_hold_say() {
        check(vec![
            Row {
                name: "a FAULT-class error returned with the third-fault hold waits and relaunches",
                script: vec![held_then_fails(classed(FAULT)), done()],
                calls: one_call(Ok(OUTPUT)),
                launched: vec![ID, ID],
                paused_at: vec![1],
                ..Row::default()
            },
            Row {
                name: "an unclassed error while held waits and relaunches",
                script: vec![held_then_fails(unclassed()), done()],
                calls: one_call(Ok(OUTPUT)),
                launched: vec![ID, ID],
                paused_at: vec![1],
                ..Row::default()
            },
            Row {
                name: "a HOLD-class error while held waits and relaunches",
                script: vec![held_then_fails(classed(HOLD)), done()],
                calls: one_call(Ok(OUTPUT)),
                launched: vec![ID, ID],
                paused_at: vec![1],
                ..Row::default()
            },
            Row {
                name: "a HOLD-class error on an unheld run relaunches at once",
                script: vec![fails(classed(HOLD)), done()],
                calls: one_call(Ok(OUTPUT)),
                launched: vec![ID, ID],
                ..Row::default()
            },
            Row {
                name: "a WAIT-class error on an unheld run relaunches at once",
                script: vec![fails(classed(WAIT)), done()],
                calls: one_call(Ok(OUTPUT)),
                launched: vec![ID, ID],
                ..Row::default()
            },
            Row {
                name: "an unclassed error on an unheld run passes",
                script: vec![fails(unclassed())],
                calls: one_call(Err(unclassed())),
                launched: vec![ID],
                ..Row::default()
            },
            Row {
                name: "a FAULT-class error on an unheld run passes",
                script: vec![fails(classed(FAULT))],
                calls: one_call(Err(classed(FAULT))),
                launched: vec![ID],
                ..Row::default()
            },
            Row {
                name: "a hold released before the error leaves the run unheld",
                seeded: vec![third_fault_hold(), released_event(HOLD_BY_OPERATOR)],
                script: vec![fails(classed(FAULT))],
                calls: one_call(Err(classed(FAULT))),
                launched: vec![ID],
                ..Row::default()
            },
        ]);
    }

    /// THE TWO PASS-THROUGHS: a conductor marker and a spawn the current run's slice has ended
    /// pass unchanged, held or not, ahead of the class, and neither spends the bound.
    #[test]
    fn a_marked_error_or_an_ended_spawn_passes_held_or_not() {
        check(vec![
            Row {
                name: "a park while held passes",
                script: vec![held_then_fails(parked_spawn(ID))],
                calls: one_call(Err(parked_spawn(ID))),
                launched: vec![ID],
                ..Row::default()
            },
            Row {
                name: "an emit refusal while held passes",
                script: vec![held_then_fails(refused())],
                calls: one_call(Err(refused())),
                launched: vec![ID],
                ..Row::default()
            },
            Row {
                name: "a marker wrapped in context passes",
                script: vec![held_then_fails(Error(format!("stage: {}", refused().0)))],
                calls: one_call(Err(Error(format!("stage: {}", refused().0)))),
                launched: vec![ID],
                ..Row::default()
            },
            Row {
                name: "a marker beats a WAIT class on an unheld run",
                script: vec![fails(no_result_error(
                    AgentFailure::RateLimit,
                    parked_spawn(ID).0,
                ))],
                calls: one_call(Err(no_result_error(
                    AgentFailure::RateLimit,
                    parked_spawn(ID).0,
                ))),
                launched: vec![ID],
                ..Row::default()
            },
            Row {
                name: "a spawn its run has ended passes while held",
                script: vec![held_then_fails(classed(FAULT)).appending(ended())],
                calls: one_call(Err(classed(FAULT))),
                launched: vec![ID],
                ..Row::default()
            },
            Row {
                name: "a spawn its run has ended beats a WAIT class on an unheld run",
                script: vec![fails(classed(WAIT)).appending(ended())],
                calls: one_call(Err(classed(WAIT))),
                launched: vec![ID],
                ..Row::default()
            },
            Row {
                name: "another spawn's result ends nothing of this one",
                seeded: vec![SpawnResult::ok(SIBLING, OUTPUT).to_event().unwrap()],
                script: vec![held_then_fails(classed(FAULT)), done()],
                calls: one_call(Ok(OUTPUT)),
                launched: vec![ID, ID],
                paused_at: vec![1],
                ..Row::default()
            },
            Row {
                name: "a liveness fault ends nothing, so the wait still relaunches",
                script: vec![
                    held_then_fails(classed(FAULT)).appending(
                        SpawnResult::liveness_fault(ID, "silent", "stale")
                            .to_event()
                            .unwrap(),
                    ),
                    done(),
                ],
                calls: one_call(Ok(OUTPUT)),
                launched: vec![ID, ID],
                paused_at: vec![1],
                ..Row::default()
            },
            Row {
                name: "a result of an earlier run ends nothing of the current slice",
                seeded: vec![ended(), run_started("r2", &["c"])],
                script: vec![held_then_fails(classed(FAULT)), done()],
                calls: one_call(Ok(OUTPUT)),
                launched: vec![ID, ID],
                paused_at: vec![1],
                ..Row::default()
            },
        ]);
    }

    /// THE BOUND: two relaunches of a spawn id with no non-error outcome between, whatever the
    /// errors' classes, counted per id; the next error and each later one pass, held or not,
    /// until a non-error outcome of that id clears the count.
    #[test]
    fn a_spawn_id_relaunches_twice_and_the_next_error_passes() {
        check(vec![
            Row {
                name: "the second relaunch is allowed and its success returns",
                script: vec![
                    held_then_fails(classed(FAULT)),
                    held_then_fails(classed(FAULT)),
                    done(),
                ],
                calls: one_call(Ok(OUTPUT)),
                launched: vec![ID, ID, ID],
                paused_at: vec![1, 2],
                ..Row::default()
            },
            Row {
                name: "the third error passes while held",
                script: vec![
                    held_then_fails(classed(FAULT)),
                    held_then_fails(classed(FAULT)),
                    held_then_fails(classed(FAULT)),
                ],
                calls: one_call(Err(classed(FAULT))),
                launched: vec![ID, ID, ID],
                paused_at: vec![1, 2],
                ..Row::default()
            },
            Row {
                name: "the third error passes unheld",
                script: vec![
                    fails(classed(WAIT)),
                    fails(classed(WAIT)),
                    fails(classed(WAIT)),
                ],
                calls: one_call(Err(classed(WAIT))),
                launched: vec![ID, ID, ID],
                ..Row::default()
            },
            Row {
                name: "the classes of the errors between do not matter",
                script: vec![
                    fails(classed(WAIT)),
                    held_then_fails(unclassed()),
                    fails(classed(HOLD)),
                ],
                calls: one_call(Err(classed(HOLD))),
                launched: vec![ID, ID, ID],
                paused_at: vec![2],
                ..Row::default()
            },
            Row {
                name: "a later error of a spent id passes while held, after the wait before it",
                script: vec![
                    held_then_fails(classed(FAULT)),
                    held_then_fails(classed(FAULT)),
                    held_then_fails(classed(FAULT)),
                    held_then_fails(unclassed()),
                ],
                calls: vec![(ID, Err(classed(FAULT))), (ID, Err(unclassed()))],
                launched: vec![ID, ID, ID, ID],
                paused_at: vec![1, 2, 3],
                ..Row::default()
            },
            Row {
                name: "a non-error outcome of a spent id clears its count",
                script: vec![
                    fails(classed(WAIT)),
                    fails(classed(WAIT)),
                    fails(classed(WAIT)),
                    done(),
                    fails(classed(WAIT)),
                    done(),
                ],
                calls: vec![(ID, Err(classed(WAIT))), (ID, Ok(OUTPUT)), (ID, Ok(OUTPUT))],
                launched: vec![ID, ID, ID, ID, ID, ID],
                ..Row::default()
            },
            Row {
                name: "a non-error outcome clears a count short of the bound",
                script: vec![
                    fails(classed(WAIT)),
                    fails(classed(WAIT)),
                    done(),
                    fails(classed(WAIT)),
                    fails(classed(WAIT)),
                    done(),
                ],
                calls: vec![(ID, Ok(OUTPUT)), (ID, Ok(OUTPUT))],
                launched: vec![ID, ID, ID, ID, ID, ID],
                ..Row::default()
            },
            Row {
                name: "each id counts alone",
                script: vec![
                    fails(classed(WAIT)),
                    fails(classed(WAIT)),
                    fails(classed(WAIT)),
                    fails(classed(WAIT)),
                    done(),
                ],
                calls: vec![(ID, Err(classed(WAIT))), (SIBLING, Ok(OUTPUT))],
                launched: vec![ID, ID, ID, SIBLING, SIBLING],
                ..Row::default()
            },
            Row {
                name: "a pass-through spends nothing and clears nothing",
                script: vec![
                    fails(unclassed()),
                    fails(classed(WAIT)),
                    fails(classed(WAIT)),
                    fails(parked_spawn(ID)),
                    fails(classed(WAIT)),
                ],
                calls: vec![
                    (ID, Err(unclassed())),
                    (ID, Err(parked_spawn(ID))),
                    (ID, Err(classed(WAIT))),
                ],
                launched: vec![ID, ID, ID, ID, ID],
                ..Row::default()
            },
        ]);
    }

    /// THE SESSION: a closed `rigger serve` session ends a wait with the park of the spawn that
    /// waited, at either wait, and reads as nothing while the run is unheld.
    #[test]
    fn a_closed_session_ends_a_wait_with_a_park() {
        check(vec![
            Row {
                name:
                    "a session closing during the wait before a launch parks with nothing launched",
                seeded: vec![third_fault_hold()],
                session: Session::Open,
                at_pause: AtPause::CloseSession,
                calls: one_call(Err(parked_spawn(ID))),
                paused_at: vec![0],
                ..Row::default()
            },
            Row {
                name: "a session closing during the wait after an error parks that spawn",
                session: Session::Open,
                at_pause: AtPause::CloseSession,
                script: vec![held_then_fails(classed(FAULT))],
                calls: one_call(Err(parked_spawn(ID))),
                launched: vec![ID],
                paused_at: vec![1],
                ..Row::default()
            },
            Row {
                name: "a session already closed parks a held run with no pause",
                seeded: vec![third_fault_hold()],
                session: Session::Closed,
                calls: one_call(Err(parked_spawn(ID))),
                ..Row::default()
            },
            Row {
                name: "a closed session on an unheld run launches",
                session: Session::Closed,
                script: vec![done()],
                calls: one_call(Ok(OUTPUT)),
                launched: vec![ID],
                ..Row::default()
            },
        ]);
    }

    /// A FAILED READ: a run not proved unheld launches nothing and answers the read's error,
    /// never a hang; a read failing after a spawn's error hands that error back, not its own.
    #[test]
    fn a_failed_read_launches_nothing_and_never_masks_the_spawn_error() {
        check(vec![
            Row {
                name: "a store failing before the launch",
                reads_fail: true,
                calls: one_call(Err(Error(READ_FAILED.to_string()))),
                ..Row::default()
            },
            Row {
                name: "a store failing during the wait",
                seeded: vec![third_fault_hold()],
                at_pause: AtPause::BreakReads,
                calls: one_call(Err(Error(READ_FAILED.to_string()))),
                paused_at: vec![0],
                ..Row::default()
            },
            Row {
                name: "a hold the fold refuses before the launch",
                seeded: vec![ev(TYPE_RUN_HELD, MALFORMED_HOLD)],
                calls: one_call(Err(Error(MALFORMED_HOLD_SAID.to_string()))),
                ..Row::default()
            },
            Row {
                name: "a store failing after the spawn's error",
                script: vec![fails(classed(WAIT)).breaking_reads()],
                calls: one_call(Err(classed(WAIT))),
                launched: vec![ID],
                ..Row::default()
            },
            Row {
                name: "a result the read refuses after the spawn's error",
                script: vec![fails(classed(WAIT)).appending(ev(TYPE_SPAWN_RESULT, "["))],
                calls: one_call(Err(classed(WAIT))),
                launched: vec![ID],
                ..Row::default()
            },
            Row {
                name: "a hold the fold refuses after the spawn's error",
                script: vec![fails(classed(WAIT)).appending(ev(TYPE_RUN_HELD, MALFORMED_HOLD))],
                calls: one_call(Err(classed(WAIT))),
                launched: vec![ID],
                ..Row::default()
            },
        ]);
    }
}
