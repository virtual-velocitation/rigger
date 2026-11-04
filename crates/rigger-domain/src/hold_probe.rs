//! THE PROBE'S DELAY (spec 105 RELEASE): when the hold controller owes the open hold its next
//! probe, read from the run's hold events and the progress store's probe lines alone, so no other
//! probe state is stored.
//!
//! A probe's delay is 60 s doubled per probe already made of the hold's chain, capped at 900 s: a
//! probe that finds the account still failing waits longer before the next. The chain is the open
//! hold and every hold behind it that a quick re-hold continued: a hold released and held again
//! within 600 s keeps its probes' count, so a hold the host re-declares straight after a probe
//! released it does not start over at 60 s. An operator hold breaks the chain, since the operator
//! ended it, and is never probed itself; a `model_not_found` hold is never probed either, its
//! action being a configuration a probe cannot change.

use std::time::{Duration, SystemTime};

use crate::agent::{cause_row, AgentFailure};
use crate::eventstore::{Event, Revision};
use crate::ledger::{holds, Hold, HoldSpan};
use crate::progress::{AgentProgress, TYPE_AGENT_PROGRESS};
use crate::run::RunHeld;

/// The [`AgentProgress::id`] of the line the hold controller writes for each probe it makes.
pub const HOLD_PROBE_ID: &str = "hold-probe";

/// The delay of a hold's first probe.
const FIRST_DELAY: Duration = Duration::from_secs(60);

/// The delay no doubling exceeds.
const MAX_DELAY: Duration = Duration::from_secs(900);

/// The longest a released hold's release may fall before the next hold's first `RunHeld` and
/// still have that hold continue its chain.
const QUICK_RE_HOLD: Duration = Duration::from_secs(600);

/// The probe the open hold is owed: the hold's latest `RunHeld` and its run-stream revision, the
/// delay that bounds the probe and the instant it falls due.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OwedProbe {
    /// The hold's latest `RunHeld`: its cause is what the probe tests, its model the canary's.
    pub held: RunHeld,
    /// The run-stream revision of [`OwedProbe::held`]: the probe releases the hold only while
    /// its latest `RunHeld` is still this one.
    pub revision: Revision,
    /// The delay since the chain's latest probe or `RunHeld`, which also bounds the probe.
    pub delay: Duration,
    /// When the probe falls due.
    pub due: SystemTime,
}

impl OwedProbe {
    /// Whether the probe reads the login before its canary: a credential cause's.
    pub fn checks_login(&self) -> bool {
        cause_row(&self.held.cause).credential
    }

    /// The progress line the hold controller writes as it makes this probe.
    pub fn line(&self) -> AgentProgress {
        AgentProgress {
            id: HOLD_PROBE_ID.to_string(),
            activity: format!(
                "probing the {} hold within {} s",
                self.held.cause,
                self.delay.as_secs()
            ),
            revision: Some(self.revision),
        }
    }
}

/// Whether a probe can release `hold`: neither an operator hold nor a `model_not_found` hold, the
/// two [`owed_probe`] owes no probe, so `rigger status` says only of a probed hold that the run
/// resumes by itself.
pub fn is_probed(hold: &Hold) -> bool {
    !hold.operator && hold.latest.cause != AgentFailure::ModelNotFound.as_str()
}

/// The probe the run's open hold is owed at `now`, read from the current run's events
/// (`run_events`) through [`holds`] and the run's progress lines (`progress_events`): `None` for
/// an unheld run, an operator hold, a `model_not_found` hold or a run slice whose `RunHeld` does
/// not decode. Otherwise the delay is [`FIRST_DELAY`] doubled per hold-probe line stamped inside
/// the revisions of the hold's chain, capped at [`MAX_DELAY`], and the probe falls due one delay
/// after the later of the chain's latest such line and the hold's latest `RunHeld`, and never
/// later than one delay from `now`.
pub fn owed_probe(
    run_events: &[Event],
    progress_events: &[Event],
    now: SystemTime,
) -> Option<OwedProbe> {
    let spans = holds(run_events);
    // Each hold beside its span, `None` when any of the run's holds does not decode.
    let decoded: Vec<(&HoldSpan, &Hold)> = spans
        .iter()
        .map(|span| span.hold.as_ref().map(|hold| (span, hold)))
        .collect::<Option<_>>()?;
    let (&(open, hold), behind) = decoded.split_last()?;
    if open.release.is_some() || !is_probed(hold) {
        return None;
    }
    // Each hold of the chain as the revision of its first `RunHeld` and of its release, the open
    // hold's none.
    let mut chain: Vec<(Revision, Option<Revision>)> = vec![(open.first, None)];
    let mut since = open.since;
    for &(span, behind_hold) in behind.iter().rev() {
        // Every hold behind the last is released: the walk opens a hold only while none is open.
        let Some(release) = &span.release else { break };
        let gap = since.duration_since(release.at).unwrap_or_default();
        if behind_hold.operator || gap > QUICK_RE_HOLD {
            break;
        }
        chain.push((span.first, Some(release.revision)));
        since = span.since;
    }
    let probes: Vec<SystemTime> = progress_events
        .iter()
        .filter(|e| e.type_ == TYPE_AGENT_PROGRESS)
        .filter_map(|e| {
            let line: AgentProgress = serde_json::from_slice(&e.data).ok()?;
            let revision = line.revision.filter(|_| line.id == HOLD_PROBE_ID)?;
            chain
                .iter()
                .any(|&(first, release)| {
                    first <= revision && release.is_none_or(|release| revision < release)
                })
                .then_some(e.valid_from)
        })
        .collect();
    let delay = delay_after(probes.len());
    let last = probes.into_iter().fold(open.latest_at, SystemTime::max);
    Some(OwedProbe {
        held: hold.latest.clone(),
        revision: hold.revision,
        delay,
        due: (last + delay).min(now + delay),
    })
}

/// The delay after `probes` probes: [`FIRST_DELAY`] doubled per probe, capped at [`MAX_DELAY`].
fn delay_after(probes: usize) -> Duration {
    let doublings = u32::try_from(probes).unwrap_or(u32::MAX);
    let factor = 1u32.checked_shl(doublings).unwrap_or(u32::MAX);
    FIRST_DELAY.saturating_mul(factor).min(MAX_DELAY)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event_fixtures::at;
    use crate::hold_probe_fixtures::probe_line;
    use crate::progress::RunStamped;
    use crate::run::{
        RunHeld, RunReleased, HOLD_BY_HOST, HOLD_BY_OPERATOR, HOLD_BY_PROBE, TYPE_RUN_HELD,
        TYPE_RUN_RELEASED,
    };
    use std::time::{Duration, SystemTime};

    /// One run-stream entry of a row: a `RunHeld` of a cause by a writer, a `RunHeld` whose body
    /// does not decode, a `RunReleased`, or an event no hold reads, each at its valid time.
    enum Entry {
        Held(&'static str, &'static str, u64),
        BadHeld(u64),
        Released(u64),
        Other(u64),
    }
    use Entry::{BadHeld, Held, Other, Released};

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
                    BadHeld(secs) => (TYPE_RUN_HELD, br#"{"by":1}"#.to_vec(), *secs),
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
                "an undecodable RunHeld",
                vec![rate(100), BadHeld(200)],
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
        assert_eq!(owed.line(), probe_line("billing_error", 120, 0));
    }
}
