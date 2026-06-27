//! Per-project segregation for any EventStore: a single decorator over the port
//! that prefixes every stream a project writes and scopes every global read and
//! subscription to that namespace, so one backend (a shared SQLite file or a
//! shared KurrentDB instance) can hold many projects without their streams ever
//! mixing. Callers use plain, unprefixed stream names and never see the namespace.
//!
//! Because it depends only on the port, it is written once and wraps every
//! backend - dependency inversion buying the single implementation.

use super::{
    Direction, Error, Event, EventStore, ExpectedRevision, Filter, Position, Subscription,
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
        from: Position,
        dir: Direction,
    ) -> Result<Vec<Event>, Error> {
        self.inner.read_stream(&self.scoped(stream), from, dir)
    }

    fn read_all(
        &self,
        from: Position,
        dir: Direction,
        filter: &Filter,
    ) -> Result<Vec<Event>, Error> {
        self.inner.read_all(from, dir, &self.scope_filter(filter))
    }

    fn subscribe_all(&self, from: Position, filter: &Filter) -> Result<Subscription, Error> {
        self.inner.subscribe_all(from, &self.scope_filter(filter))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eventstore::sqlite::Store;

    #[test]
    fn segregates_projects_in_one_backend() {
        let backend = Store::open(":memory:").unwrap();
        let alpha = Namespaced::new(&backend, "alpha");
        let beta = Namespaced::new(&backend, "beta");

        // Both projects write to a stream named "run".
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

        // Each project's global read sees only its own events.
        let a = alpha
            .read_all(0, Direction::Forward, &Filter::default())
            .unwrap();
        assert_eq!(
            a.iter().map(|e| e.type_.as_str()).collect::<Vec<_>>(),
            ["A1"]
        );
        let b = beta
            .read_all(0, Direction::Forward, &Filter::default())
            .unwrap();
        assert_eq!(
            b.iter().map(|e| e.type_.as_str()).collect::<Vec<_>>(),
            ["B1"]
        );

        // The same stream name in different projects does not collide.
        let a_run = alpha.read_stream("run", 0, Direction::Forward).unwrap();
        assert_eq!(a_run.len(), 1);
        assert_eq!(a_run[0].type_, "A1");
    }

    #[test]
    fn passes_the_contract() {
        let backend = Store::open(":memory:").unwrap();
        let scoped = Namespaced::new(&backend, "contract");
        crate::eventstore::contract::assert_contract(&scoped);
    }
}
