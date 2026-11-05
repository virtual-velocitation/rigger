//! THE HELD RUN'S DRIVER (spec 105): the [`AgentDriver`] decorator that launches nothing while the
//! run is held and carries a session the run cannot use across the hold.
//!
//! The run's hold is a fold of its log ([`ledger::project`]), read here before every launch, so a
//! session already running is left alone and a gate already running finishes: only the next
//! launch waits. A wait ends on the run's `RunReleased` or, on `rigger serve`, on the session's
//! close, with a park, and a close beats a release landing with it, since nothing would serve the
//! launch. An error of a hold or wait class, or any error but a store fault while the run is held,
//! waits the same way and launches the same spawn id again, its next ordinal coming from the host's own read of
//! the spawn stream, so the conductor sees no failure and writes no infra mark. The hold is read
//! after the host returns, because the host appends a third-fault hold before it returns its
//! error. An error the host marked interrupted
//! ([`INTERRUPTED_MARKER`](rigger_domain::agent::INTERRUPTED_MARKER)) relaunches on its mark
//! without that read: the host decided the run held once, and a release landing after its read
//! relaunches the interrupted session instead of reaching the conductor.
//!
//! Four errors reach the conductor unchanged, held or not: one carrying a conductor marker, a store
//! fault ([`STORE_FAULT_MARKER`](rigger_domain::agent::STORE_FAULT_MARKER): a store read that
//! failed before the launch, the host's or this decorator's own hold read), one for a spawn the
//! current run's slice has ended, and the next error of a spawn id that has relaunched
//! [`RELAUNCH_BOUND`] times with no non-error outcome between. An unclassed or
//! fault-class error on an unheld run reaches it too, unless the host marked it interrupted, its
//! spawn site deciding what follows.
//!
//! On `rigger run`, an error it hands the conductor for a spawn the current run parked and has
//! not ended is recorded as that spawn's result, so a restarted run answers the spawn from the
//! log instead of launching it again: any error but a park, an emit refusal or a store fault, and
//! an interrupted one only once its spawn id has no relaunch left. A store fault ended no launched
//! spawn, so it records nothing and a restart relaunches the spawn. It writes no other event: a hold,
//! a release and a spawn's close each have their author. Its count of relaunches lives in memory
//! over one supervisor's life, so a restarted supervisor starts it again.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use rigger_domain::agent::{
    failure_class, parked_spawn, store_fault, AgentDriver, AgentResult, Error, SpawnOpts,
    EMIT_REFUSED_MARKER,
};
use serde_json::Value;

use crate::conductor::{is_interrupted, is_parked_or_refused, is_store_fault};
use crate::config::AgentDef;
use crate::eventstore::{Event, EventStore};
use crate::ledger;
use crate::run::{
    read::{hold_stands, read_current_run},
    STREAM,
};
use crate::spawn;
use crate::spawn_store;

/// How many times one spawn id relaunches with no non-error outcome of that id between: its next
/// error passes through, whatever the error's class and whether or not the run is held.
const RELAUNCH_BOUND: u32 = 2;

/// Where the error a [`HeldDriver`] hands the conductor ends up: whether it is also recorded as
/// the spawn's result.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Results {
    /// `rigger run`: the error is recorded as the spawn's result, so a restarted run answers the
    /// spawn from the log.
    Recorded,
    /// `rigger serve`: the shim hands each result back in process and none is logged.
    HandedBack,
}

/// An [`AgentDriver`] that holds `inner`'s launches back while the run is held and relaunches a
/// session a hold or a wait class ended, up to [`RELAUNCH_BOUND`].
pub struct HeldDriver<'a> {
    inner: &'a dyn AgentDriver,
    store: &'a dyn EventStore,
    results: Results,
    session_closed: Option<&'a AtomicBool>,
    pause: &'a (dyn Fn() + Sync),
    /// Each spawn id's relaunches since its last non-error outcome.
    relaunch_counts: Mutex<HashMap<String, u32>>,
}

impl<'a> HeldDriver<'a> {
    /// A decorator over `inner`, reading the run from `store` and, under [`Results::Recorded`],
    /// recording the errors it hands the conductor there, so `store` is the one every other write
    /// of the run goes through. A resume fallback launches inside
    /// `inner`'s spawn, past this gate, so an `inner` with one reads the run's hold itself
    /// ([`AgentDriver`]'s contract). `session_closed` is `rigger
    /// serve`'s flag, `None` on `rigger run`, which has no session to end a wait. `pause` runs
    /// between two reads of a standing hold.
    pub fn new(
        inner: &'a dyn AgentDriver,
        store: &'a dyn EventStore,
        results: Results,
        session_closed: Option<&'a AtomicBool>,
        pause: &'a (dyn Fn() + Sync),
    ) -> Self {
        HeldDriver {
            inner,
            store,
            results,
            session_closed,
            pause,
            relaunch_counts: Mutex::new(HashMap::new()),
        }
    }

    /// The current run's slice of the log.
    fn run_slice(&self) -> Result<Vec<Event>, Error> {
        let (slice, _) = read_current_run(self.store, STREAM)?;
        Ok(slice)
    }

    /// Return once the run is not held. A closed session ends a held read, or a wait that paused
    /// whatever its last read found, with `id`'s park: the flag is read after the hold, so a close
    /// beats a release landing with it and a launch that did not wait reads no close. A failed
    /// read of the run ends the wait with `relaunched`, the error of the spawn this wait
    /// relaunches, or with the read's own error marked a store fault ([`store_fault`]) before a
    /// first launch, since a run not proved unheld launches nothing, unless the wait paused and its session closed: that wait parks,
    /// whatever its read answered, since nothing would serve the spawn either way.
    fn wait_out_the_hold(&self, id: &str, mut relaunched: Option<Error>) -> Result<(), Error> {
        let mut paused = false;
        loop {
            let held = hold_stands(self.store, STREAM);
            let closed = self
                .session_closed
                .is_some_and(|closed| closed.load(Ordering::SeqCst));
            if (paused || matches!(held, Ok(true))) && closed {
                return Err(parked_spawn(id));
            }
            if !held.map_err(|read| {
                relaunched
                    .take()
                    .unwrap_or_else(|| store_fault(Error::from(read)))
            })? {
                return Ok(());
            }
            (self.pause)();
            paused = true;
        }
    }

    /// Whether `e`, the error of spawn `id`, is one this decorator carries across instead of its
    /// spawn site. A store fault is never carried, held or not: no launch ran, and the
    /// conductor's infra retry answers it. A run it cannot read leaves `e` to the conductor, the
    /// spawn's own failure never masked by the read's. An error the host marked interrupted is carried across without
    /// a read of the hold, since the host's own read decided it. Spends nothing: the relaunch it
    /// answers true for is spent by [`Self::try_spend_relaunch`].
    fn carries_across(&self, e: &Error, id: &str) -> bool {
        if is_parked_or_refused(e, EMIT_REFUSED_MARKER) || is_store_fault(e) {
            return false;
        }
        let Ok(slice) = self.run_slice() else {
            return false;
        };
        if !matches!(spawn::ended_by(&slice, id), Ok(None)) {
            return false;
        }
        if is_interrupted(e) {
            return true;
        }
        let Ok(run) = ledger::project(&slice) else {
            return false;
        };
        let class_waits = failure_class(e).is_some_and(|class| class.row().waits());
        run.hold.is_some() || class_waits
    }

    /// Spend one of spawn `id`'s relaunches: false, spending nothing, once `id` has used the
    /// [`RELAUNCH_BOUND`] since its last non-error outcome.
    fn try_spend_relaunch(&self, id: &str) -> bool {
        let mut counts = self.relaunch_counts.lock().expect("relaunch count lock");
        if !relaunch_owed(&counts, id) {
            return false;
        }
        *counts.entry(id.to_string()).or_insert(0) += 1;
        true
    }

    /// THE ERROR WRITE (spec 105, RESUME BY SESSION): record `e`, the error this decorator hands
    /// the conductor for spawn `id`, as the spawn's result, so a restarted `rigger run` answers
    /// the spawn from the log with that error instead of launching it again. Only under
    /// [`Results::Recorded`], `rigger run`'s mode: under [`Results::HandedBack`] nothing is
    /// recorded. A
    /// park, an emit refusal and a store fault end no launched spawn, so their marked errors
    /// record nothing; neither does
    /// an interrupted error while `id` has a relaunch owed, nor a spawn the current run never
    /// parked. The write lands over no result or a liveness fault alone
    /// ([`spawn_store::record_result_unless_ended`]), never beside a result that ended the spawn.
    /// Best-effort: a failed read or write leaves `e` to reach the conductor unchanged.
    fn record_error(&self, e: &Error, id: &str) {
        if self.results == Results::HandedBack
            || is_parked_or_refused(e, EMIT_REFUSED_MARKER)
            || is_store_fault(e)
        {
            return;
        }
        if is_interrupted(e)
            && relaunch_owed(
                &self.relaunch_counts.lock().expect("relaunch count lock"),
                id,
            )
        {
            return;
        }
        let Ok(slice) = self.run_slice() else {
            return;
        };
        if spawn::is_recorded(&slice, id) {
            let _ = spawn_store::record_result_unless_ended(
                self.store,
                &spawn::SpawnResult::failed(id, e.0.clone()),
            );
        }
    }
}

/// Whether spawn `id` has a relaunch left under [`RELAUNCH_BOUND`] in `counts`.
fn relaunch_owed(counts: &HashMap<String, u32>, id: &str) -> bool {
    counts.get(id).is_none_or(|count| *count < RELAUNCH_BOUND)
}

impl AgentDriver for HeldDriver<'_> {
    fn spawn(
        &self,
        agent: &AgentDef,
        prompt: &str,
        opts: &SpawnOpts,
        emit: &dyn Fn(&str, Value) -> Result<(), Error>,
    ) -> Result<AgentResult, Error> {
        let mut relaunched = None;
        loop {
            self.wait_out_the_hold(&opts.id, relaunched.take())?;
            match self.inner.spawn(agent, prompt, opts, emit) {
                Ok(result) => {
                    self.relaunch_counts
                        .lock()
                        .expect("relaunch count lock")
                        .remove(&opts.id);
                    return Ok(result);
                }
                Err(e)
                    if self.carries_across(&e, &opts.id) && self.try_spend_relaunch(&opts.id) =>
                {
                    relaunched = Some(e)
                }
                Err(e) => {
                    self.record_error(&e, &opts.id);
                    return Err(e);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Mutex;

    use rigger_domain::agent::{
        launch_interrupted, no_result_error, parked_spawn, store_fault, AgentDriver, AgentFailure,
        AgentResult, Error, SpawnOpts, API_UNSTABLE, EMIT_REFUSED_MARKER,
    };
    use serde_json::Value;

    use super::{HeldDriver, Results};
    use crate::config::AgentDef;
    use crate::eventstore::sqlite::Store;
    use crate::eventstore::{Direction, Event, EventStore, ExpectedRevision, Position};
    use crate::run::{HOLD_BY_HOST, HOLD_BY_OPERATOR, STREAM, TYPE_RUN_HELD};
    use crate::run_store;
    use crate::spawn::{SpawnEvent, SpawnRequest, SpawnResult, TYPE_SPAWN_RESULT};
    use crate::test_support::{
        agent, ev, held_event, implementer_opts, no_emit, placed_positions, released_event,
        run_started, spawn_result_rows, LandsInTheNextRead, ReadsFailWhenArmed,
    };

    /// The spawn each row launches, and a second spawn id for the rows the bound keeps apart.
    const ID: &str = "solo/implementer#0";
    const SIBLING: &str = "other/implementer#0";
    /// What a launch that ends well answers.
    const OUTPUT: &str = "the worker's output";
    /// What the store says when its reads are broken: [`ReadsFailWhenArmed`]'s backend error.
    const READ_FAILED: &str = "event store: the run read failed";
    /// A `RunHeld` body the fold refuses, and what the hold read says of it.
    const MALFORMED_HOLD: &str = r#"{"by":1}"#;
    const MALFORMED_HOLD_SAID: &str = "event store: fold the run's hold: \
        invalid type: integer `1`, expected a string at line 1 column 7";

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

    /// The error the host returns for a launch it closed `interrupted` on a FAULT class, a run it
    /// read held deciding that close.
    fn interrupted() -> Error {
        launch_interrupted(FAULT, "the session was interrupted".into())
    }

    /// The error of a store read failing before a launch: the read's own error, marked a store
    /// fault.
    fn faulted(read: &str) -> Error {
        store_fault(Error(read.to_string()))
    }

    /// The hold the host appends on the third failed close, before it returns its error.
    fn third_fault_hold() -> Event {
        held_event(API_UNSTABLE, HOLD_BY_HOST)
    }

    /// The real result that ended spawn [`ID`].
    fn ended() -> Event {
        SpawnResult::ok(ID, OUTPUT).to_event().unwrap()
    }

    /// The request that parked spawn [`ID`] in the run.
    fn requested() -> Event {
        SpawnRequest {
            id: ID.to_string(),
            ..SpawnRequest::default()
        }
        .to_event()
        .unwrap()
    }

    /// The result row ([`spawn_result_rows`]) of spawn [`ID`] ending on `e`.
    fn ended_on(e: Error) -> (String, String, String, bool) {
        (ID.to_string(), String::new(), e.0, false)
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
    /// it returns, as the real host appends its hold before its error, adding where they landed
    /// to `authored`, and panics on a launch past the script, so a relaunch no row expects fails
    /// that row.
    struct Scripted<'a> {
        store: &'a Store,
        armed: &'a AtomicBool,
        script: Mutex<VecDeque<Launch>>,
        launched: Mutex<Vec<String>>,
        authored: &'a Mutex<Vec<Position>>,
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
            let appended = self
                .store
                .append(STREAM, ExpectedRevision::Any, &launch.appends)
                .unwrap();
            self.authored
                .lock()
                .unwrap()
                .extend(placed_positions(&appended));
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
        /// The session closes, then the hold is released, both inside this one pause.
        CloseThenRelease,
        /// The hold is released, then the session closes, both inside this one pause.
        ReleaseThenClose,
        /// The store's reads start failing.
        BreakReads,
        /// The session closes and the store's reads start failing, both inside this one pause.
        CloseAndBreakReads,
        /// The session closes, then the hold is released, both inside the decorator's next hold
        /// read and ahead of that read's own calls, so only a flag read after that read sees the
        /// close.
        CloseThenReleaseInTheNextRead,
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
        /// Each result the decorator recorded, in order ([`spawn_result_rows`]): a result on
        /// the stream that neither the row's seed nor a launch of its script appended.
        results: Vec<(String, String, String, bool)>,
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
                results: Vec::new(),
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
    /// spawn's output or error byte for byte, the ordered launches, the launches standing at
    /// each pause, and the results the decorator recorded.
    fn check(rows: Vec<Row>) {
        for row in rows {
            let name = row.name;
            let store = Store::open(":memory:").unwrap();
            run_store::ensure_started(&store, &["c".to_string()]).unwrap();
            let seeded = store
                .append(STREAM, ExpectedRevision::Any, &row.seeded)
                .unwrap();
            let authored = Mutex::new(placed_positions(&seeded));
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
                authored: &authored,
            };
            let closed = AtomicBool::new(matches!(row.session, Session::Closed));
            let session = match row.session {
                Session::Absent => None,
                Session::Open | Session::Closed => Some(&closed),
            };
            let release = || {
                store
                    .append(
                        STREAM,
                        ExpectedRevision::Any,
                        &[released_event(HOLD_BY_OPERATOR, 0)],
                    )
                    .unwrap();
            };
            let close = || closed.store(true, Ordering::SeqCst);
            let lands_armed = AtomicBool::new(false);
            let land = || {
                close();
                release();
            };
            let lands = LandsInTheNextRead {
                inner: &reads,
                armed: &lands_armed,
                land: &land,
            };
            let paused_at = Mutex::new(Vec::new());
            let pause = || {
                let mut paused = paused_at.lock().unwrap();
                paused.push(host.launched.lock().unwrap().len());
                match row.at_pause {
                    AtPause::Release if paused.len() % row.polls == 0 => release(),
                    AtPause::Release => {}
                    AtPause::CloseSession => close(),
                    AtPause::CloseThenRelease => {
                        close();
                        release();
                    }
                    AtPause::ReleaseThenClose => {
                        release();
                        close();
                    }
                    AtPause::BreakReads => armed.store(true, Ordering::SeqCst),
                    AtPause::CloseAndBreakReads => {
                        close();
                        armed.store(true, Ordering::SeqCst);
                    }
                    AtPause::CloseThenReleaseInTheNextRead => {
                        lands_armed.store(true, Ordering::SeqCst)
                    }
                }
            };
            // Each command's mode: `rigger run`, with no session, records; `rigger serve` does not.
            let results = match row.session {
                Session::Absent => Results::Recorded,
                Session::Open | Session::Closed => Results::HandedBack,
            };
            let held = HeldDriver::new(&host, &lands, results, session, &pause);
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
            let authored = authored.into_inner().unwrap();
            let recorded: Vec<Event> = store
                .read_stream(STREAM, 0, Direction::Forward)
                .unwrap()
                .into_iter()
                .filter(|e| !authored.contains(&e.position))
                .collect();
            assert_eq!(
                spawn_result_rows(&recorded),
                row.results,
                "{name}: the results the decorator recorded"
            );
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
                seeded: vec![third_fault_hold(), released_event(HOLD_BY_OPERATOR, 0)],
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
                seeded: vec![third_fault_hold(), released_event(HOLD_BY_OPERATOR, 0)],
                script: vec![fails(classed(FAULT))],
                calls: one_call(Err(classed(FAULT))),
                launched: vec![ID],
                ..Row::default()
            },
        ]);
    }

    /// THE HOST'S DECISION TRAVELS WITH ITS ERROR: an error the host marked interrupted relaunches
    /// on its mark with no read of the hold, so a release landing between the host's held read and
    /// this decision relaunches the spawn with nothing recorded; a spawn its run has ended and a
    /// failed read still pass it, and the bound passes the next one, recording it.
    #[test]
    fn an_error_the_host_marked_interrupted_relaunches_on_its_mark_alone() {
        check(vec![
            Row {
                name: "a release landing behind the host's held read relaunches, recording nothing",
                seeded: vec![requested()],
                script: vec![
                    held_then_fails(interrupted()).appending(released_event(HOLD_BY_OPERATOR, 0)),
                    done(),
                ],
                calls: one_call(Ok(OUTPUT)),
                launched: vec![ID, ID],
                ..Row::default()
            },
            Row {
                name: "the bound passes the third and records it",
                seeded: vec![requested()],
                script: vec![
                    fails(interrupted()),
                    fails(interrupted()),
                    fails(interrupted()),
                ],
                calls: one_call(Err(interrupted())),
                launched: vec![ID, ID, ID],
                results: vec![ended_on(interrupted())],
                ..Row::default()
            },
            Row {
                name: "a spawn its run has ended passes",
                seeded: vec![requested()],
                script: vec![fails(interrupted()).appending(ended())],
                calls: one_call(Err(interrupted())),
                launched: vec![ID],
                ..Row::default()
            },
            Row {
                name: "a store failing after the error passes it",
                seeded: vec![requested()],
                script: vec![fails(interrupted()).breaking_reads()],
                calls: one_call(Err(interrupted())),
                launched: vec![ID],
                ..Row::default()
            },
        ]);
    }

    /// THE PASS-THROUGHS: a conductor marker, a store fault and a spawn the current run's slice has
    /// ended pass unchanged, held or not, ahead of the class, and none spends the bound.
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
                name: "a store fault while held passes",
                script: vec![held_then_fails(faulted(READ_FAILED))],
                calls: one_call(Err(faulted(READ_FAILED))),
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
    /// waited, at either wait, whichever way round a release landed with the close in one pause,
    /// and when both land inside the hold read, since the flag is read after the hold; it reads
    /// as nothing on a launch that did not wait.
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
                name: "a close then a release in one pause before a launch parks unlaunched",
                seeded: vec![third_fault_hold()],
                session: Session::Open,
                at_pause: AtPause::CloseThenRelease,
                calls: one_call(Err(parked_spawn(ID))),
                paused_at: vec![0],
                ..Row::default()
            },
            Row {
                name: "a release then a close in one pause before a launch parks unlaunched",
                seeded: vec![third_fault_hold()],
                session: Session::Open,
                at_pause: AtPause::ReleaseThenClose,
                calls: one_call(Err(parked_spawn(ID))),
                paused_at: vec![0],
                ..Row::default()
            },
            Row {
                name: "a close and a release inside the paused hold read park unlaunched",
                seeded: vec![third_fault_hold()],
                session: Session::Open,
                at_pause: AtPause::CloseThenReleaseInTheNextRead,
                calls: one_call(Err(parked_spawn(ID))),
                paused_at: vec![0],
                ..Row::default()
            },
            Row {
                name: "a close then a release in one pause after an error parks that spawn",
                session: Session::Open,
                at_pause: AtPause::CloseThenRelease,
                script: vec![held_then_fails(classed(FAULT))],
                calls: one_call(Err(parked_spawn(ID))),
                launched: vec![ID],
                paused_at: vec![1],
                ..Row::default()
            },
            Row {
                name: "a release then a close in one pause after an error parks that spawn",
                session: Session::Open,
                at_pause: AtPause::ReleaseThenClose,
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

    /// A FAILED READ: a run not proved unheld launches nothing and answers the read's error marked a
    /// store fault, never a hang, unless the wait paused and its session closed, which parks the spawn; a read
    /// failing while the decorator decides on a spawn's error, or in the wait after it relaunched
    /// that spawn, hands that error back, not its own.
    #[test]
    fn a_failed_read_launches_nothing_and_never_masks_the_spawn_error() {
        check(vec![
            Row {
                name: "a store failing before the launch",
                reads_fail: true,
                calls: one_call(Err(faulted(READ_FAILED))),
                ..Row::default()
            },
            Row {
                name: "a store failing during the wait",
                seeded: vec![third_fault_hold()],
                at_pause: AtPause::BreakReads,
                calls: one_call(Err(faulted(READ_FAILED))),
                paused_at: vec![0],
                ..Row::default()
            },
            Row {
                name:
                    "a store failing during the wait after a relaunch hands back the spawn's error",
                script: vec![held_then_fails(classed(FAULT))],
                at_pause: AtPause::BreakReads,
                calls: one_call(Err(classed(FAULT))),
                launched: vec![ID],
                paused_at: vec![1],
                ..Row::default()
            },
            Row {
                name: "a session closing as the store fails during the wait parks unlaunched",
                seeded: vec![third_fault_hold()],
                session: Session::Open,
                at_pause: AtPause::CloseAndBreakReads,
                calls: one_call(Err(parked_spawn(ID))),
                paused_at: vec![0],
                ..Row::default()
            },
            Row {
                name: "a hold the fold refuses before the launch",
                seeded: vec![ev(TYPE_RUN_HELD, MALFORMED_HOLD)],
                calls: one_call(Err(faulted(MALFORMED_HOLD_SAID))),
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

    /// THE ERROR WRITE (spec 105, RESUME BY SESSION): on `rigger run`, an error the decorator
    /// hands the conductor is recorded as the result of a spawn its run parked and has not ended,
    /// the bound's pass-through included and a liveness fault no bar, so a restart answers the
    /// spawn from the log. A marked error, a store fault, a relaunched one, an open or closed
    /// `rigger serve` session, a spawn this run never parked, one its run has ended and a run the decorator
    /// cannot read each record nothing, and the error returns unchanged.
    #[test]
    fn an_error_passing_on_rigger_run_is_recorded_for_a_parked_spawn_its_run_has_not_ended() {
        check(vec![
            Row {
                name: "an unclassed error of a parked spawn is recorded",
                seeded: vec![requested()],
                script: vec![fails(unclassed())],
                calls: one_call(Err(unclassed())),
                launched: vec![ID],
                results: vec![ended_on(unclassed())],
                ..Row::default()
            },
            Row {
                name: "the bound's pass-through records the error that passed, none relaunched",
                seeded: vec![requested()],
                script: vec![
                    fails(classed(WAIT)),
                    fails(classed(WAIT)),
                    fails(classed(WAIT)),
                ],
                calls: one_call(Err(classed(WAIT))),
                launched: vec![ID, ID, ID],
                results: vec![ended_on(classed(WAIT))],
                ..Row::default()
            },
            Row {
                name: "an error over a liveness fault is recorded",
                seeded: vec![requested()],
                script: vec![fails(unclassed()).appending(
                    SpawnResult::liveness_fault(ID, "silent", "stale")
                        .to_event()
                        .unwrap(),
                )],
                calls: one_call(Err(unclassed())),
                launched: vec![ID],
                results: vec![ended_on(unclassed())],
                ..Row::default()
            },
            Row {
                name: "a relaunched error records nothing",
                seeded: vec![requested()],
                script: vec![fails(classed(WAIT)), done()],
                calls: one_call(Ok(OUTPUT)),
                launched: vec![ID, ID],
                ..Row::default()
            },
            Row {
                name: "a park records nothing",
                seeded: vec![requested()],
                script: vec![fails(parked_spawn(ID))],
                calls: one_call(Err(parked_spawn(ID))),
                launched: vec![ID],
                ..Row::default()
            },
            Row {
                name: "an emit refusal records nothing",
                seeded: vec![requested()],
                script: vec![fails(refused())],
                calls: one_call(Err(refused())),
                launched: vec![ID],
                ..Row::default()
            },
            Row {
                name: "a store fault of a parked spawn records nothing",
                seeded: vec![requested()],
                script: vec![fails(faulted(READ_FAILED))],
                calls: one_call(Err(faulted(READ_FAILED))),
                launched: vec![ID],
                ..Row::default()
            },
            Row {
                name: "an open rigger serve session records nothing",
                seeded: vec![requested()],
                session: Session::Open,
                script: vec![fails(unclassed())],
                calls: one_call(Err(unclassed())),
                launched: vec![ID],
                ..Row::default()
            },
            Row {
                name: "a closed rigger serve session records nothing",
                seeded: vec![requested()],
                session: Session::Closed,
                script: vec![fails(unclassed())],
                calls: one_call(Err(unclassed())),
                launched: vec![ID],
                ..Row::default()
            },
            Row {
                name: "a spawn its run never parked records nothing",
                script: vec![fails(unclassed())],
                calls: one_call(Err(unclassed())),
                launched: vec![ID],
                ..Row::default()
            },
            Row {
                name: "a request of an earlier run records nothing",
                seeded: vec![requested(), run_started("r2", &["c"])],
                script: vec![fails(unclassed())],
                calls: one_call(Err(unclassed())),
                launched: vec![ID],
                ..Row::default()
            },
            Row {
                name: "a spawn its run has ended records nothing more",
                seeded: vec![requested()],
                script: vec![fails(classed(WAIT)).appending(ended())],
                calls: one_call(Err(classed(WAIT))),
                launched: vec![ID],
                ..Row::default()
            },
            Row {
                name: "a run read failing after the error records nothing",
                seeded: vec![requested()],
                script: vec![fails(classed(WAIT)).breaking_reads()],
                calls: one_call(Err(classed(WAIT))),
                launched: vec![ID],
                ..Row::default()
            },
        ]);
    }
}
