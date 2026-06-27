//! Per-project segregation for any EventStore: a single decorator over the port
//! that prefixes every stream a project writes and scopes every global read and
//! subscription to that namespace, **stripping the prefix from returned events so
//! callers see clean stream names**. One backend (a shared SQLite file or a shared
//! KurrentDB instance) can hold many projects without their streams ever mixing.
//!
//! Because it depends only on the port, it is written once and wraps every
//! backend - dependency inversion buying the single implementation.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::channel;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use super::{
    Direction, Error, Event, EventStore, ExpectedRevision, Filter, Position, Revision, Subscription,
};

/// Namespaced wraps an EventStore so all of its data is scoped to one project.
pub struct Namespaced<'a> {
    inner: &'a dyn EventStore,
    prefix: String,
}

impl<'a> Namespaced<'a> {
    /// Scope inner to the named project.
    pub fn new(inner: &'a dyn EventStore, project: &str) -> Self {
        Namespaced {
            inner,
            prefix: format!("proj-{project}-"),
        }
    }

    fn scoped(&self, stream: &str) -> String {
        format!("{}{stream}", self.prefix)
    }

    /// Force the project namespace, composing it with any caller prefix
    /// (interpreted within the namespace).
    fn scope_filter(&self, filter: &Filter) -> Filter {
        let caller = filter.stream_prefix.as_deref().unwrap_or("");
        Filter {
            stream_prefix: Some(format!("{}{caller}", self.prefix)),
        }
    }

    fn strip(&self, mut events: Vec<Event>) -> Vec<Event> {
        for e in &mut events {
            if let Some(rest) = e.stream.strip_prefix(&self.prefix) {
                e.stream = rest.to_string();
            }
        }
        events
    }
}

impl EventStore for Namespaced<'_> {
    fn append(
        &self,
        stream: &str,
        expected: ExpectedRevision,
        events: &[Event],
    ) -> Result<Position, Error> {
        self.inner.append(&self.scoped(stream), expected, events)
    }

    fn read_stream(
        &self,
        stream: &str,
        from: Revision,
        dir: Direction,
    ) -> Result<Vec<Event>, Error> {
        let events = self.inner.read_stream(&self.scoped(stream), from, dir)?;
        Ok(self.strip(events))
    }

    fn read_all(
        &self,
        from: Position,
        dir: Direction,
        filter: &Filter,
    ) -> Result<Vec<Event>, Error> {
        let events = self.inner.read_all(from, dir, &self.scope_filter(filter))?;
        Ok(self.strip(events))
    }

    fn subscribe_all(&self, from: Position, filter: &Filter) -> Result<Subscription, Error> {
        let inner = self.inner.subscribe_all(from, &self.scope_filter(filter))?;
        Ok(strip_subscription(inner, self.prefix.clone()))
    }

    fn subscribe_stream(&self, stream: &str, from: Revision) -> Result<Subscription, Error> {
        let inner = self.inner.subscribe_stream(&self.scoped(stream), from)?;
        Ok(strip_subscription(inner, self.prefix.clone()))
    }
}

/// Wrap a subscription so each delivered event has the namespace prefix stripped
/// from its stream. Owns the inner subscription; dropping the wrapper stops both.
fn strip_subscription(inner: Subscription, prefix: String) -> Subscription {
    let (tx, rx) = channel();
    let err = Arc::new(Mutex::new(None));
    let stop = Arc::new(AtomicBool::new(false));
    let stop_thread = Arc::clone(&stop);
    let err_thread = Arc::clone(&err);
    let handle = std::thread::spawn(move || {
        // The inner subscription is owned here; it stops when this thread ends.
        while !stop_thread.load(Ordering::Relaxed) {
            match inner.recv_timeout(Duration::from_millis(50)) {
                Some(mut e) => {
                    if let Some(rest) = e.stream.strip_prefix(&prefix) {
                        e.stream = rest.to_string();
                    }
                    if tx.send(e).is_err() {
                        return;
                    }
                }
                None => {
                    if let Some(msg) = inner.err() {
                        *err_thread.lock().unwrap() = Some(msg);
                        return;
                    }
                    // a quiet timeout: the inner is still live; re-check stop
                }
            }
        }
    });
    Subscription::new(rx, err, stop, handle)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eventstore::sqlite::Store;

    #[test]
    fn segregates_projects_and_strips_the_prefix() {
        let backend = Store::open(":memory:").unwrap();
        let alpha = Namespaced::new(&backend, "alpha");
        let beta = Namespaced::new(&backend, "beta");

        alpha
            .append(
                "run",
                ExpectedRevision::Any,
                &[Event::new("A1", b"a".to_vec())],
            )
            .unwrap();
        beta.append(
            "run",
            ExpectedRevision::Any,
            &[Event::new("B1", b"b".to_vec())],
        )
        .unwrap();

        let a = alpha
            .read_all(0, Direction::Forward, &Filter::default())
            .unwrap();
        assert_eq!(
            a.iter().map(|e| e.type_.as_str()).collect::<Vec<_>>(),
            ["A1"]
        );
        // the returned stream name is clean (prefix stripped), not "proj-alpha-run"
        assert_eq!(a[0].stream, "run");

        let b = beta
            .read_all(0, Direction::Forward, &Filter::default())
            .unwrap();
        assert_eq!(
            b.iter().map(|e| e.type_.as_str()).collect::<Vec<_>>(),
            ["B1"]
        );

        let a_run = alpha.read_stream("run", 0, Direction::Forward).unwrap();
        assert_eq!(a_run.len(), 1);
        assert_eq!(a_run[0].type_, "A1");
        assert_eq!(a_run[0].stream, "run");
    }

    #[test]
    fn passes_the_contract() {
        let backend = Store::open(":memory:").unwrap();
        let scoped = Namespaced::new(&backend, "contract");
        crate::eventstore::contract::assert_contract(&scoped);
    }
}
