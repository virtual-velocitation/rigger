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

/// A peer's decision, as the side-car surfaces it to an agent.
#[derive(Clone, Debug, Deserialize)]
pub struct PeerDecision {
    pub id: String,
    #[serde(default)]
    pub summary: String,
    #[serde(default)]
    pub governs: Vec<String>,
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
    pub fn decisions(&self) -> Vec<PeerDecision> {
        let seen = self.seen.lock().unwrap();
        seen.iter()
            .filter(|e| e.type_ == contextgraph::TYPE_DECISION_MADE)
            .filter_map(|e| serde_json::from_slice(&e.data).ok())
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
}
