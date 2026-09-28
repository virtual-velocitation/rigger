//! Live cross-agent awareness: a read of the run's decisions, lessons and findings from the
//! shared event log, so no agent works blind to its peers. It never crosses the
//! file-isolation boundary - worktrees isolate the files, the event stream shares the
//! decisions.

use serde::de::DeserializeOwned;
use serde::Deserialize;

use crate::contextgraph;
use crate::eventstore::{self, Event, EventStore};
use crate::run;

/// A peer's decision, as the side-car surfaces it to an agent.
#[derive(Clone, Debug, Deserialize)]
pub struct PeerDecision {
    pub id: String,
    #[serde(default)]
    pub summary: String,
    #[serde(default)]
    pub governs: Vec<String>,
    /// LIVE when this decision belongs to the ACTIVE run, HISTORICAL (a superseded run,
    /// or pre-boundary) otherwise (spec 21, unit 3). A DERIVED VIEW, not part of the
    /// event body: the side-car sets it from the single c1 run attribution
    /// ([`run::run_attribution`] + [`run::current_run_id`] over the side-car's read)
    /// so provenance is legible without scoping grounding to the active run. `#[serde(skip)]`
    /// keeps it out of (de)serialization and defaults it to `false` - the conservative
    /// HISTORICAL default - when a decision is decoded from an event body.
    #[serde(skip)]
    pub live: bool,
}

/// A peer reviewer's finding, as the side-car surfaces it to a concurrent reviewer.
/// This is how concurrent lenses see each other's findings LIVE: a lens emits a
/// ReviewFinding, the side-car's catch-up subscription picks it up, and a fellow
/// lens re-checking `rigger_peers` scoped to its files reads it back - the same
/// channel that surfaces peer decisions, scoped on the finding's `about` files.
#[derive(Clone, Debug, Deserialize)]
pub struct PeerFinding {
    pub id: String,
    #[serde(default)]
    pub by: String,
    #[serde(default)]
    pub summary: String,
    #[serde(default)]
    pub about: Vec<String>,
}

/// A lesson a prior run's escalation recorded, as the side-car surfaces it. Lessons
/// fold ABOUT the files the failed unit touched, so they scope on `about` exactly like
/// a [`PeerFinding`]. This is the recovery surface behind the lessons half of the
/// prompt-budget elision note: when a hot file's lessons are trimmed from a prompt,
/// `rigger peers <file>` returns the full set here (adj-u1gap17).
#[derive(Clone, Debug, Deserialize)]
pub struct PeerLesson {
    pub id: String,
    #[serde(default)]
    pub summary: String,
    #[serde(default)]
    pub about: Vec<String>,
}

/// A peer record the side-car surfaces: the event type it folds from, and the files it
/// scopes on for [`Sidecar::peers_for`].
pub trait Peer: DeserializeOwned {
    /// The event type every record of this kind is decoded from.
    const EVENT_TYPE: &'static str;
    /// The files a record of this kind is scoped on.
    const SCOPE: fn(&Self) -> &[String];

    /// Every record of this kind in `seen`, in stream order - each matching event's body
    /// decoded, an undecodable one skipped.
    fn collect(seen: &[Event]) -> Vec<Self> {
        seen.iter()
            .filter(|e| e.type_ == Self::EVENT_TYPE)
            .filter_map(|e| serde_json::from_slice(&e.data).ok())
            .collect()
    }
}

impl Peer for PeerDecision {
    const EVENT_TYPE: &'static str = contextgraph::TYPE_DECISION_MADE;
    const SCOPE: fn(&Self) -> &[String] = |d| &d.governs;

    /// Each decision carries a LIVE/HISTORICAL provenance label (spec 21, unit 3),
    /// derived from the SINGLE c1 run attribution - [`run::run_attribution`] keyed by
    /// event index plus [`run::current_run_id`] - over the WHOLE `seen` stream. The same
    /// slice feeds both the attribution and the active-run id, and `.enumerate()` maps a
    /// decision's index back onto that same slice, so the index contract holds (a
    /// filtered/partial slice would misalign the keys). Grounding is NOT scoped here: the
    /// label only makes provenance legible; `graph_context` still surfaces cross-run
    /// decisions unchanged.
    fn collect(seen: &[Event]) -> Vec<Self> {
        let attribution = run::run_attribution(seen);
        let active = run::current_run_id(seen);
        seen.iter()
            .enumerate()
            .filter(|(_, e)| e.type_ == Self::EVENT_TYPE)
            .filter_map(|(i, e)| {
                let mut d: PeerDecision = serde_json::from_slice(&e.data).ok()?;
                d.live = attribution
                    .get(&i)
                    .is_some_and(|run_of| run_of.is_live(active.as_deref()));
                Some(d)
            })
            .collect()
    }
}

impl Peer for PeerFinding {
    const EVENT_TYPE: &'static str = contextgraph::TYPE_REVIEW_FINDING;
    const SCOPE: fn(&Self) -> &[String] = |f| &f.about;
}

impl Peer for PeerLesson {
    const EVENT_TYPE: &'static str = contextgraph::TYPE_LESSON_LEARNED;
    const SCOPE: fn(&Self) -> &[String] = |l| &l.about;
}

/// Sidecar is ONE read of the run's peer records (spec 101): the run's own events from its
/// boundary and every run's carried-over decisions, lessons and findings, exactly as
/// [`crate::run::read::read_run`] hands them back - never a replay of the whole log from position 0, so
/// a one-shot `rigger peers` or an MCP `rigger_peers` call costs the run and its carry-over, not
/// the project's history. A caller that wants what peers recorded since reads again: each call
/// sees every record committed before it.
pub struct Sidecar {
    seen: Vec<Event>,
}

impl Sidecar {
    /// Read the peer records of the current run on `stream`.
    pub fn read(store: &dyn EventStore, stream: &str) -> Result<Self, eventstore::Error> {
        Ok(Sidecar {
            seen: run::read::read_run(store, stream)?,
        })
    }

    /// Every peer record of kind `T` this read holds ([`Peer::collect`] over the whole read):
    /// the concurrent decisions an agent should be aware of before it acts, the findings a
    /// concurrent reviewer should be aware of before it renders its own, or the lessons a prior
    /// run's escalations recorded about the files an agent is touching.
    pub fn peers<T: Peer>(&self) -> Vec<T> {
        T::collect(&self.seen)
    }

    /// The peer records of kind `T` scoped to an agent's blast-radius (§5.3): a peer record is
    /// relevant only when its [`Peer::SCOPE`] files (a decision's `governs`, a finding's or
    /// lesson's `about`) intersect the agent's blast-radius. An empty `blast_radius` means "no
    /// scope" and returns every record (the unscoped [`Self::peers`] behavior), so a caller that
    /// does not know its files still sees its peers.
    pub fn peers_for<T: Peer>(&self, blast_radius: &[String]) -> Vec<T> {
        let all = self.peers::<T>();
        if blast_radius.is_empty() {
            return all;
        }
        let scope: std::collections::HashSet<&str> =
            blast_radius.iter().map(String::as_str).collect();
        all.into_iter()
            .filter(|p| (T::SCOPE)(p).iter().any(|f| scope.contains(f.as_str())))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eventstore::sqlite::Store;
    use crate::eventstore::ExpectedRevision;
    use crate::test_support::{seed_one_shot_fixture, ReadCountingStore};

    /// THE SIDECAR BEHIND `rigger peers` READS FROM THE BOUNDARY (spec 101): over a log holding
    /// 200,000 derived events and two superseded runs before the boundary, one read costs exactly
    /// the run's events plus the typed carry-over, and still surfaces every run's decisions,
    /// lessons and findings - the current run's decision LIVE, a superseded run's HISTORICAL.
    #[test]
    fn the_sidecar_reads_the_run_from_its_boundary_and_the_carry_over_by_type() {
        let inner = Store::open(":memory:").unwrap();
        let fixture = seed_one_shot_fixture(&inner, "run", &[]);
        let store = ReadCountingStore::new(&inner);
        let sidecar = Sidecar::read(&store, "run").unwrap();
        assert_eq!(store.reads(), fixture.read("run"));
        assert_eq!(store.materialized(), fixture.cost());

        let decisions: Vec<(String, bool)> = sidecar
            .peers::<PeerDecision>()
            .into_iter()
            .map(|d| (d.id, d.live))
            .collect();
        assert_eq!(
            decisions,
            [("d-a".to_string(), false), ("d-c".to_string(), true)]
        );
        let lessons: Vec<String> = sidecar
            .peers::<PeerLesson>()
            .into_iter()
            .map(|l| l.id)
            .collect();
        assert_eq!(lessons, ["l-a"]);
        let findings: Vec<String> = sidecar
            .peers::<PeerFinding>()
            .into_iter()
            .map(|f| f.id)
            .collect();
        assert_eq!(findings, ["f-b", "f-c"]);
    }

    #[test]
    fn surfaces_a_decision_recorded_before_the_read() {
        let store = Store::open(":memory:").unwrap();
        let data =
            serde_json::to_vec(&serde_json::json!({"id": "d1", "summary": "chose X"})).unwrap();
        store
            .append(
                "run",
                ExpectedRevision::Any,
                &[Event::new(contextgraph::TYPE_DECISION_MADE, data)],
            )
            .unwrap();
        let decisions = Sidecar::read(&store, "run")
            .unwrap()
            .peers::<PeerDecision>();
        assert_eq!(decisions.len(), 1);
        assert_eq!(decisions[0].id, "d1");
        assert_eq!(decisions[0].summary, "chose X");
    }

    /// Appends two `type_` events built by `payload(id, file)` - `ids[0]` about a.rs, `ids[1]`
    /// about b.rs - reads them back as `P`, asserts an empty blast-radius returns both, and
    /// returns the peers scoped to a.rs.
    fn peers_scoped_to_a_rs<P: Peer>(
        type_: &str,
        ids: [&str; 2],
        payload: impl Fn(&str, &str) -> serde_json::Value,
    ) -> Vec<P> {
        let store = Store::open(":memory:").unwrap();
        for (id, file) in ids.into_iter().zip(["a.rs", "b.rs"]) {
            let data = serde_json::to_vec(&payload(id, file)).unwrap();
            store
                .append("run", ExpectedRevision::Any, &[Event::new(type_, data)])
                .unwrap();
        }
        let sidecar = Sidecar::read(&store, "run").unwrap();

        // An empty blast-radius returns every record.
        let all = sidecar.peers_for::<P>(&[]);
        assert_eq!(all.len(), 2);

        sidecar.peers_for::<P>(&["a.rs".into()])
    }

    crate::test_cases! {
        /// One decision governs a.rs, another governs b.rs: scoped to a.rs, only the a.rs
        /// decision comes back.
        decisions_for_scopes_to_the_blast_radius: {
            let scoped = peers_scoped_to_a_rs::<PeerDecision>(
                contextgraph::TYPE_DECISION_MADE,
                ["da", "db"],
                |id, governs| serde_json::json!({"id": id, "summary": "x", "governs": [governs]}),
            );
            assert_eq!(scoped.len(), 1);
            assert_eq!(scoped[0].id, "da");
        };
        /// A peer reviewer's ReviewFinding is surfaced by the side-car and scoped to a
        /// reviewer's blast-radius the same way decisions are, so concurrent lenses see each
        /// other's findings on their next read.
        findings_for_scopes_to_the_blast_radius: {
            let scoped = peers_scoped_to_a_rs::<PeerFinding>(
                contextgraph::TYPE_REVIEW_FINDING,
                ["fa", "fb"],
                |id, about| {
                    serde_json::json!({"id": id, "by": "lens", "summary": "x", "about": [about]})
                },
            );
            assert_eq!(
                scoped.len(),
                1,
                "a finding about a.rs is returned scoped to a.rs"
            );
            assert_eq!(scoped[0].id, "fa");
        };
        /// A prior run's LessonLearned is surfaced by the side-car and scoped to a
        /// blast-radius the same way decisions and findings are, so `rigger peers` can
        /// recover the lessons elided from a capped prompt section (adj-u1gap17).
        lessons_for_scopes_to_the_blast_radius: {
            let scoped = peers_scoped_to_a_rs::<PeerLesson>(
                contextgraph::TYPE_LESSON_LEARNED,
                ["la", "lb"],
                |id, about| {
                    serde_json::json!({"id": id, "summary": "do not repeat x", "about": [about]})
                },
            );
            assert_eq!(
                scoped.len(),
                1,
                "a lesson about a.rs is returned scoped to a.rs"
            );
            assert_eq!(scoped[0].id, "la");
            assert_eq!(scoped[0].summary, "do not repeat x");
        };
    }
}
