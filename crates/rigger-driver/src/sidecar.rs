//! Live cross-agent awareness: a read of the run's decisions, lessons and findings from the
//! shared event log, so no agent works blind to its peers. It never crosses the
//! file-isolation boundary - worktrees isolate the files, the event stream shares the
//! decisions.

use serde::de::DeserializeOwned;
use serde::Deserialize;

use crate::contextgraph;
use crate::contextgraph::query::referenced_id;
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
/// channel that surfaces peer decisions, scoped on the files the finding's `about` names.
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
/// a [`PeerFinding`], and `rigger peers <file>` lists them beside the file's decisions and
/// findings (the newest under the lessons section's budget).
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
    /// The references a record of this kind is scoped on, each naming the node id
    /// [`referenced_id`] reads from it.
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
    /// relevant only when the files its [`Peer::SCOPE`] references name (a decision's `governs`,
    /// a finding's or lesson's `about`) intersect the files the agent's blast-radius names, each
    /// side read through [`referenced_id`] - so a record citing `a.rs:12` is scoped on `a.rs`. The
    /// record comes back with its references as written. An empty `blast_radius` means "no
    /// scope" and returns every record (the unscoped [`Self::peers`] behavior), so a caller that
    /// does not know its files still sees its peers.
    pub fn peers_for<T: Peer>(&self, blast_radius: &[String]) -> Vec<T> {
        let all = self.peers::<T>();
        if blast_radius.is_empty() {
            return all;
        }
        let scope: std::collections::HashSet<&str> =
            blast_radius.iter().map(|f| referenced_id(f)).collect();
        all.into_iter()
            .filter(|p| {
                (T::SCOPE)(p)
                    .iter()
                    .any(|f| scope.contains(referenced_id(f)))
            })
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
    /// 200,000 ledger entries and two superseded runs before the boundary, one read costs exactly
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
        peers_scoped_to(type_, ids, ["a.rs", "b.rs"], "a.rs", payload)
    }

    /// Appends two `type_` events built by `payload(id, file)`, `ids[i]` naming `files[i]`, reads
    /// them back as `P`, asserts an empty blast-radius returns both, and returns the peers scoped
    /// to `blast_radius`.
    fn peers_scoped_to<P: Peer>(
        type_: &str,
        ids: [&str; 2],
        files: [&str; 2],
        blast_radius: &str,
        payload: impl Fn(&str, &str) -> serde_json::Value,
    ) -> Vec<P> {
        let store = Store::open(":memory:").unwrap();
        for (id, file) in ids.into_iter().zip(files) {
            let data = serde_json::to_vec(&payload(id, file)).unwrap();
            store
                .append("run", ExpectedRevision::Any, &[Event::new(type_, data)])
                .unwrap();
        }
        let sidecar = Sidecar::read(&store, "run").unwrap();

        // An empty blast-radius returns every record.
        let all = sidecar.peers_for::<P>(&[]);
        assert_eq!(all.len(), 2);

        sidecar.peers_for::<P>(&[blast_radius.into()])
    }

    /// The ids of the `P` records the side-car scopes to `blast_radius`, out of two built by
    /// `payload(id, file)`: `on-a` citing `a.rs:12` and `on-b` citing `b.rs:12-20`.
    fn ids_citing_lines_scoped_to<P: Peer>(
        type_: &str,
        blast_radius: &str,
        payload: impl Fn(&str, &str) -> serde_json::Value,
        id: fn(&P) -> &str,
    ) -> Vec<String> {
        peers_scoped_to::<P>(
            type_,
            ["on-a", "on-b"],
            ["a.rs:12", "b.rs:12-20"],
            blast_radius,
            payload,
        )
        .iter()
        .map(|p| id(p).to_string())
        .collect()
    }

    /// A record citing a line of a file, or a span of its lines, is scoped on that file: the
    /// decision, finding and lesson citing `a.rs:12` come back for the blast-radius `a.rs` and for
    /// another line of it, `a.rs:40`, while those citing `b.rs:12-20` do not.
    #[test]
    fn a_record_citing_lines_of_a_file_is_scoped_on_that_file() {
        for blast_radius in ["a.rs", "a.rs:40"] {
            let scoped = [
                ids_citing_lines_scoped_to::<PeerDecision>(
                    contextgraph::TYPE_DECISION_MADE,
                    blast_radius,
                    |id, file| serde_json::json!({"id": id, "summary": "x", "governs": [file]}),
                    |d| &d.id,
                ),
                ids_citing_lines_scoped_to::<PeerFinding>(
                    contextgraph::TYPE_REVIEW_FINDING,
                    blast_radius,
                    |id, file| serde_json::json!({"id": id, "by": "lens", "summary": "x", "about": [file]}),
                    |f| &f.id,
                ),
                ids_citing_lines_scoped_to::<PeerLesson>(
                    contextgraph::TYPE_LESSON_LEARNED,
                    blast_radius,
                    |id, file| serde_json::json!({"id": id, "summary": "x", "about": [file]}),
                    |l| &l.id,
                ),
            ];
            assert_eq!(
                scoped,
                [["on-a"], ["on-a"], ["on-a"]],
                "the decision, finding and lesson scoped to {blast_radius}"
            );
        }
    }

    /// One `type_` event carrying `body`, stamped with each `(key, value)` of `meta`.
    fn stamped(type_: &str, body: serde_json::Value, meta: &[(&str, &str)]) -> Event {
        meta.iter().fold(
            Event::new(type_, serde_json::to_vec(&body).unwrap()),
            |e, (key, value)| e.with_meta(*key, *value),
        )
    }

    /// A log holding a record of every rank tier, oldest first: `d-prior`, unit u1's ruling from
    /// before the run; the run's `RunStarted`; `d-verdict`, u1's adjudicator verdict; `d-rebase`, a
    /// definition re-pin; `d-own` and `f-own`, u1's ruling and lens finding; `d-both`, u2's ruling
    /// on a.rs and a line of b.rs; `d-peer`, u2's ruling on a.rs; `l-a`, a lesson about a.rs; and
    /// `d-off`, a ruling on c.rs alone. Ranked for `caller` over `files`, as `(id, live)` pairs.
    fn ranked_ids(files: &[&str], caller: Option<&str>) -> Vec<(String, Option<bool>)> {
        use serde_json::json;
        let decision = |id: &str, governs: &[&str], meta: &[(&str, &str)]| {
            stamped(
                contextgraph::TYPE_DECISION_MADE,
                json!({"id": id, "summary": id, "governs": governs}),
                meta,
            )
        };
        let events = [
            decision("d-prior", &["a.rs"], &[(META_SPAWN, "u1/implementer#0")]),
            crate::test_support::run_started("r1", &[]),
            decision("d-verdict", &["a.rs"], &[(META_SPAWN, "u1/adjudicator#0")]),
            decision("d-rebase", &["a.rs"], &[(run::META_DEFINITION, "h2")]),
            decision("d-own", &["a.rs"], &[(META_SPAWN, "u1/implementer#0")]),
            stamped(
                contextgraph::TYPE_REVIEW_FINDING,
                json!({"id": "f-own", "by": "lens:sdet", "summary": "f", "about": ["a.rs"]}),
                &[(META_SPAWN, "u1/lens:sdet#0")],
            ),
            decision(
                "d-both",
                &["a.rs", "b.rs:3"],
                &[(META_SPAWN, "u2/implementer#0")],
            ),
            decision("d-peer", &["a.rs"], &[(META_SPAWN, "u2/implementer#0")]),
            stamped(
                contextgraph::TYPE_LESSON_LEARNED,
                json!({"id": "l-a", "summary": "l", "about": ["a.rs"]}),
                &[],
            ),
            decision("d-off", &["c.rs"], &[]),
        ];
        let store = Store::open(":memory:").unwrap();
        store.append("run", ExpectedRevision::Any, &events).unwrap();
        let files: Vec<String> = files.iter().map(|f| f.to_string()).collect();
        Sidecar::read(&store, "run")
            .unwrap()
            .ranked(&files, caller)
            .into_iter()
            .map(|p| (p.id, p.live))
            .collect()
    }

    /// Asserts [`ranked_ids`] over `files` for `caller` is `expected`, each id's `live` label
    /// being HISTORICAL for `d-prior`, none for the lesson `l-a`, and LIVE for the rest.
    fn assert_ranked(files: &[&str], caller: Option<&str>, expected: &[&str]) {
        let live = |id: &str| match id {
            "d-prior" => Some(false),
            "l-a" => None,
            _ => Some(true),
        };
        let expected: Vec<(String, Option<bool>)> = expected
            .iter()
            .map(|id| (id.to_string(), live(id)))
            .collect();
        assert_eq!(ranked_ids(files, caller), expected);
    }

    crate::test_cases! {
        /// For a spawn of unit u1: u1's own records first (live before its prior run's), then the
        /// rest by the files they name and then by age, the lesson ranking with the live records,
        /// and every judging seat's verdict and definition re-pin last, u1's own verdict first
        /// among them; the c.rs ruling is out of scope.
        ranked_puts_the_callers_own_unit_first_and_ceremony_last: assert_ranked(
            &["a.rs", "b.rs"],
            Some("u1/implementer#1"),
            &["f-own", "d-own", "d-prior", "d-both", "l-a", "d-peer", "d-verdict", "d-rebase"],
        );
        /// With no calling spawn, no unit is raised: the record naming both files leads, then
        /// the live records and the lesson newest first, the prior run's ruling after them and
        /// the ceremony last, newest first.
        ranked_without_a_caller_orders_by_files_named_then_age: assert_ranked(
            &["a.rs", "b.rs"],
            None,
            &["d-both", "l-a", "d-peer", "f-own", "d-own", "d-prior", "d-rebase", "d-verdict"],
        );
        /// Unscoped, every record is in and none names a passed file, so age orders each tier.
        ranked_unscoped_holds_every_record_newest_first_within_a_tier: assert_ranked(
            &[],
            None,
            &[
                "d-off", "l-a", "d-peer", "d-both", "f-own", "d-own", "d-prior", "d-rebase",
                "d-verdict",
            ],
        );
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
