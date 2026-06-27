//! Live cross-agent awareness: while an agent works, the side-car surfaces the
//! decisions other agents have made on the shared event log, so no agent works
//! blind to its peers. It never crosses the file-isolation boundary - worktrees
//! isolate the files, the event stream shares the decisions. This reads the log
//! on demand (the equivalent of the Go background catch-up subscription).

use serde::Deserialize;

use crate::contextgraph;
use crate::eventstore::{Direction, EventStore, Filter};

/// A peer's decision, as the side-car surfaces it to an agent.
#[derive(Clone, Debug, Deserialize)]
pub struct PeerDecision {
    pub id: String,
    #[serde(default)]
    pub summary: String,
    #[serde(default)]
    pub governs: Vec<String>,
}

/// Sidecar surfaces the decisions on a filtered view of the event log.
pub struct Sidecar<'a> {
    store: &'a dyn EventStore,
    filter: Filter,
}

impl<'a> Sidecar<'a> {
    pub fn new(store: &'a dyn EventStore, filter: Filter) -> Self {
        Sidecar { store, filter }
    }

    fn events(&self) -> Vec<crate::eventstore::Event> {
        self.store
            .read_all(0, Direction::Forward, &self.filter)
            .unwrap_or_default()
    }

    /// The DecisionMade events seen so far - the concurrent decisions an agent
    /// should be aware of before it acts.
    pub fn decisions(&self) -> Vec<PeerDecision> {
        self.events()
            .iter()
            .filter(|e| e.type_ == contextgraph::TYPE_DECISION_MADE)
            .filter_map(|e| serde_json::from_slice(&e.data).ok())
            .collect()
    }

    /// How many events the side-car's view contains.
    pub fn len(&self) -> usize {
        self.events().len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eventstore::sqlite::Store;
    use crate::eventstore::{Event, ExpectedRevision};

    #[test]
    fn surfaces_decisions_from_the_log() {
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
        let sc = Sidecar::new(&store, Filter::default());
        assert!(sc
            .decisions()
            .iter()
            .any(|d| d.id == "d1" && d.summary == "chose X"));
        assert_eq!(sc.len(), 1);
    }
}
