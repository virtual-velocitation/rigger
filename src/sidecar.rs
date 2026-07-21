//! Live cross-agent awareness: a filtered catch-up subscription over the shared
//! event log collects the decisions other agents make while one agent works, so
//! no agent works blind to its peers. A background thread drains the subscription
//! into `seen`. It never crosses the file-isolation boundary - worktrees isolate
//! the files, the event stream shares the decisions.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

use serde::Deserialize;

use crate::contextgraph;
use crate::eventstore::{self, Event, EventStore, Filter, Position};
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
    /// ([`run::run_attribution`] + [`run::current_run_id`] over the whole event stream)
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

/// Sidecar collects the events on a filtered catch-up subscription in the
/// background while one agent works.
pub struct Sidecar {
    seen: Arc<Mutex<Vec<Event>>>,
    stop: Arc<AtomicBool>,
    collector: Option<JoinHandle<()>>,
}

impl Sidecar {
    /// Open a filtered catch-up subscription from a position and begin collecting
    /// matching events in the background.
    pub fn start(
        store: &dyn EventStore,
        from: Position,
        filter: Filter,
    ) -> Result<Self, eventstore::Error> {
        let sub = store.subscribe_all(from, &filter)?;
        let seen = Arc::new(Mutex::new(Vec::new()));
        let stop = Arc::new(AtomicBool::new(false));
        let seen_thread = Arc::clone(&seen);
        let stop_thread = Arc::clone(&stop);
        let collector = std::thread::spawn(move || {
            // The subscription is owned by this thread; it stops when the thread ends.
            while !stop_thread.load(Ordering::Relaxed) {
                if let Some(e) = sub.recv_timeout(Duration::from_millis(50)) {
                    seen_thread.lock().unwrap().push(e);
                }
            }
        });
        Ok(Sidecar {
            seen,
            stop,
            collector: Some(collector),
        })
    }

    /// The DecisionMade events seen so far - the concurrent decisions an agent
    /// should be aware of before it acts.
    ///
    /// Each decision carries a LIVE/HISTORICAL provenance label (spec 21, unit 3),
    /// derived from the SINGLE c1 run attribution - [`run::run_attribution`] keyed by
    /// event index plus [`run::current_run_id`] - over the WHOLE `seen` stream. The same
    /// slice feeds both the attribution and the active-run id, and `.enumerate()` maps a
    /// decision's index back onto that same slice, so the index contract holds (a
    /// filtered/partial slice would misalign the keys). Grounding is NOT scoped here: the
    /// label only makes provenance legible; `graph_context` still surfaces cross-run
    /// decisions unchanged.
    pub fn decisions(&self) -> Vec<PeerDecision> {
        let seen = self.seen.lock().unwrap();
        let attribution = run::run_attribution(&seen);
        let active = run::current_run_id(&seen);
        seen.iter()
            .enumerate()
            .filter(|(_, e)| e.type_ == contextgraph::TYPE_DECISION_MADE)
            .filter_map(|(i, e)| {
                let mut d: PeerDecision = serde_json::from_slice(&e.data).ok()?;
                d.live = attribution
                    .get(&i)
                    .is_some_and(|run_of| run_of.is_live(active.as_deref()));
                Some(d)
            })
            .collect()
    }

    /// The concurrent decisions scoped to an agent's blast-radius (§5.3). The
    /// side-car's catch-up subscription is filtered by stream prefix, but blast-radius
    /// scoping lives in the decision CONTENT: a peer decision is relevant only when its
    /// `governs` files intersect the agent's blast-radius. An empty `blast_radius`
    /// means "no scope" and returns every decision (the historical `decisions()`
    /// behavior), so a caller that does not know its files still sees its peers.
    pub fn decisions_for(&self, blast_radius: &[String]) -> Vec<PeerDecision> {
        if blast_radius.is_empty() {
            return self.decisions();
        }
        let scope: std::collections::HashSet<&str> =
            blast_radius.iter().map(String::as_str).collect();
        self.decisions()
            .into_iter()
            .filter(|d| d.governs.iter().any(|f| scope.contains(f.as_str())))
            .collect()
    }

    /// The ReviewFinding events seen so far - the findings a concurrent reviewer
    /// should be aware of before it renders its own. The side-car collects these the
    /// same way it collects decisions, so concurrent lenses see each other's findings
    /// live (the later tiers retrieve them via the graph once they ground; the
    /// side-car covers reviewers running AT THE SAME TIME, before any of them grounds
    /// again).
    pub fn findings(&self) -> Vec<PeerFinding> {
        let seen = self.seen.lock().unwrap();
        seen.iter()
            .filter(|e| e.type_ == contextgraph::TYPE_REVIEW_FINDING)
            .filter_map(|e| serde_json::from_slice(&e.data).ok())
            .collect()
    }

    /// The concurrent findings scoped to a reviewer's blast-radius (§5.3), mirroring
    /// [`decisions_for`]: a peer finding is relevant only when its `about` files
    /// intersect the reviewer's blast-radius. An empty `blast_radius` returns every
    /// finding (the unscoped behavior), so a caller that does not know its files still
    /// sees its peers' findings.
    pub fn findings_for(&self, blast_radius: &[String]) -> Vec<PeerFinding> {
        if blast_radius.is_empty() {
            return self.findings();
        }
        let scope: std::collections::HashSet<&str> =
            blast_radius.iter().map(String::as_str).collect();
        self.findings()
            .into_iter()
            .filter(|f| f.about.iter().any(|file| scope.contains(file.as_str())))
            .collect()
    }

    /// The LessonLearned events seen so far - the lessons a prior run's escalations
    /// recorded about the files an agent is touching. The side-car collects these the
    /// same way it collects decisions and findings, so `rigger peers` can recover the
    /// lessons a capped prompt section elided.
    pub fn lessons(&self) -> Vec<PeerLesson> {
        let seen = self.seen.lock().unwrap();
        seen.iter()
            .filter(|e| e.type_ == contextgraph::TYPE_LESSON_LEARNED)
            .filter_map(|e| serde_json::from_slice(&e.data).ok())
            .collect()
    }

    /// The lessons scoped to an agent's blast-radius (§5.3), mirroring [`decisions_for`]
    /// and [`findings_for`]: a lesson is relevant only when its `about` files intersect
    /// the blast-radius. An empty `blast_radius` returns every lesson (the unscoped
    /// behavior), so a caller that does not know its files still recovers its lessons.
    pub fn lessons_for(&self, blast_radius: &[String]) -> Vec<PeerLesson> {
        if blast_radius.is_empty() {
            return self.lessons();
        }
        let scope: std::collections::HashSet<&str> =
            blast_radius.iter().map(String::as_str).collect();
        self.lessons()
            .into_iter()
            .filter(|l| l.about.iter().any(|file| scope.contains(file.as_str())))
            .collect()
    }

    /// How many events the side-car has collected so far.
    pub fn len(&self) -> usize {
        self.seen.lock().unwrap().len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl Drop for Sidecar {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(h) = self.collector.take() {
            let _ = h.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eventstore::sqlite::Store;
    use crate::eventstore::ExpectedRevision;
    use std::time::Instant;

    #[test]
    fn surfaces_decisions_from_the_subscription() {
        let store = Store::open(":memory:").unwrap();
        let sidecar = Sidecar::start(&store, 0, Filter::default()).unwrap();

        let data =
            serde_json::to_vec(&serde_json::json!({"id": "d1", "summary": "chose X"})).unwrap();
        store
            .append(
                "run",
                ExpectedRevision::Any,
                &[Event::new(contextgraph::TYPE_DECISION_MADE, data)],
            )
            .unwrap();

        // The subscription delivers the live append; wait for the collector.
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            if sidecar
                .decisions()
                .iter()
                .any(|d| d.id == "d1" && d.summary == "chose X")
            {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "the side-car never surfaced the decision"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    #[test]
    fn decisions_for_scopes_to_the_blast_radius() {
        let store = Store::open(":memory:").unwrap();
        let sidecar = Sidecar::start(&store, 0, Filter::default()).unwrap();

        // One decision governs a.rs, another governs b.rs.
        for (id, governs) in [("da", "a.rs"), ("db", "b.rs")] {
            let data = serde_json::to_vec(&serde_json::json!({
                "id": id, "summary": "x", "governs": [governs],
            }))
            .unwrap();
            store
                .append(
                    "run",
                    ExpectedRevision::Any,
                    &[Event::new(contextgraph::TYPE_DECISION_MADE, data)],
                )
                .unwrap();
        }

        // Wait until both decisions have surfaced through the subscription.
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            if sidecar.decisions().len() >= 2 {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "the side-car never surfaced both decisions"
            );
            std::thread::sleep(Duration::from_millis(10));
        }

        // Scoped to a.rs: only the a.rs decision comes back.
        let scoped = sidecar.decisions_for(&["a.rs".into()]);
        assert_eq!(scoped.len(), 1);
        assert_eq!(scoped[0].id, "da");

        // An empty blast-radius returns every decision.
        let all = sidecar.decisions_for(&[]);
        assert_eq!(all.len(), 2);
    }

    #[test]
    fn findings_for_scopes_to_the_blast_radius() {
        // A peer reviewer's ReviewFinding is surfaced by the side-car and scoped to a
        // reviewer's blast-radius the same way decisions are: a finding about a file
        // is returned by the blast-radius-scoped peers query (item 4), so concurrent
        // lenses see each other's findings live.
        let store = Store::open(":memory:").unwrap();
        let sidecar = Sidecar::start(&store, 0, Filter::default()).unwrap();

        // One finding about a.rs, another about b.rs.
        for (id, about) in [("fa", "a.rs"), ("fb", "b.rs")] {
            let data = serde_json::to_vec(&serde_json::json!({
                "id": id, "by": "lens", "summary": "x", "about": [about],
            }))
            .unwrap();
            store
                .append(
                    "run",
                    ExpectedRevision::Any,
                    &[Event::new(contextgraph::TYPE_REVIEW_FINDING, data)],
                )
                .unwrap();
        }

        // Wait until both findings have surfaced through the subscription.
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            if sidecar.findings().len() >= 2 {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "the side-car never surfaced both findings"
            );
            std::thread::sleep(Duration::from_millis(10));
        }

        // Scoped to a.rs: only the a.rs finding comes back.
        let scoped = sidecar.findings_for(&["a.rs".into()]);
        assert_eq!(
            scoped.len(),
            1,
            "a finding about a.rs is returned scoped to a.rs"
        );
        assert_eq!(scoped[0].id, "fa");

        // An empty blast-radius returns every finding.
        let all = sidecar.findings_for(&[]);
        assert_eq!(all.len(), 2);
    }

    #[test]
    fn lessons_for_scopes_to_the_blast_radius() {
        // A prior run's LessonLearned is surfaced by the side-car and scoped to a
        // blast-radius the same way decisions and findings are: a lesson about a file
        // comes back from the blast-radius-scoped peers query, so `rigger peers` can
        // recover the lessons elided from a capped prompt section (the recovery the
        // elision note names). Without this surface `rigger peers` would return zero
        // lessons and that note would be a dead promise (adj-u1gap17).
        let store = Store::open(":memory:").unwrap();
        let sidecar = Sidecar::start(&store, 0, Filter::default()).unwrap();

        // One lesson about a.rs, another about b.rs.
        for (id, about) in [("la", "a.rs"), ("lb", "b.rs")] {
            let data = serde_json::to_vec(&serde_json::json!({
                "id": id, "summary": "do not repeat x", "about": [about],
            }))
            .unwrap();
            store
                .append(
                    "run",
                    ExpectedRevision::Any,
                    &[Event::new(contextgraph::TYPE_LESSON_LEARNED, data)],
                )
                .unwrap();
        }

        // Wait until both lessons have surfaced through the subscription.
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            if sidecar.lessons().len() >= 2 {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "the side-car never surfaced both lessons"
            );
            std::thread::sleep(Duration::from_millis(10));
        }

        // Scoped to a.rs: only the a.rs lesson comes back.
        let scoped = sidecar.lessons_for(&["a.rs".into()]);
        assert_eq!(
            scoped.len(),
            1,
            "a lesson about a.rs is returned scoped to a.rs"
        );
        assert_eq!(scoped[0].id, "la");
        assert_eq!(scoped[0].summary, "do not repeat x");

        // An empty blast-radius returns every lesson.
        let all = sidecar.lessons_for(&[]);
        assert_eq!(all.len(), 2);
    }

    #[test]
    fn decisions_are_labeled_live_from_the_active_run_and_historical_from_a_superseded_run() {
        // spec 21, unit 3: `rigger peers` must tell a live decision from dead-run noise.
        // Reusing the SINGLE c1 run attribution, a decision inside the ACTIVE run's
        // [RunStarted, next RunStarted) window is LIVE; one from a superseded (earlier)
        // run is HISTORICAL. The side-car derives the label over the WHOLE event stream,
        // so the RunStarted boundaries must be present - they are, because Filter::default
        // applies no type filter, exactly as production `cmd_peers` replays from position 0.
        let store = Store::open(":memory:").unwrap();
        let sidecar = Sidecar::start(&store, 0, Filter::default()).unwrap();

        let run_started = |run: &str| {
            Event::new(
                run::TYPE_RUN_STARTED,
                serde_json::to_vec(&serde_json::json!({ "run": run })).unwrap(),
            )
        };
        let decision = |id: &str| {
            Event::new(
                contextgraph::TYPE_DECISION_MADE,
                serde_json::to_vec(&serde_json::json!({ "id": id, "summary": "x" })).unwrap(),
            )
        };
        // The stream: run r1 opens and records a decision, then run r2 (the ACTIVE run)
        // opens and records its own decision.
        for e in [
            run_started("r1"),
            decision("d_old"),
            run_started("r2"),
            decision("d_new"),
        ] {
            store.append("run", ExpectedRevision::Any, &[e]).unwrap();
        }

        // Wait until both decisions have surfaced through the subscription. Delivery is in
        // append order, so once d_new is seen its preceding r2 boundary is seen too.
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            if sidecar.decisions().len() >= 2 {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "the side-car never surfaced both decisions"
            );
            std::thread::sleep(Duration::from_millis(10));
        }

        let decisions = sidecar.decisions();
        let by_id = |id: &str| {
            decisions
                .iter()
                .find(|d| d.id == id)
                .unwrap_or_else(|| panic!("decision {id} must surface"))
        };
        assert!(
            by_id("d_new").live,
            "a decision from the active run (r2) must be LIVE"
        );
        assert!(
            !by_id("d_old").live,
            "a decision from a superseded run (r1) must be HISTORICAL"
        );
    }
}
