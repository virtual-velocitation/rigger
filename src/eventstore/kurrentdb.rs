//! The KurrentDB EventStore adapter: it maps the async KurrentDB gRPC client onto
//! the (sync) eventstore port via a tokio runtime, so a project can swap the
//! embedded SQLite store for a shared KurrentDB server with no change to the rest
//! of Rigger. It passes the same contract suite SQLite does (proxy fidelity).

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::channel;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use kurrentdb::{
    AppendToStreamOptions, Client, EventData, Position as KdbPosition, ReadAllOptions,
    ReadStreamOptions, RecordedEvent, ResolvedEvent, StreamPosition, StreamState,
    SubscribeToAllOptions, SubscriptionFilter,
};
use uuid::Uuid;

use super::{
    Direction, Error, Event, EventStore, ExpectedRevision, Filter, Position, Subscription,
};

/// Store is the KurrentDB-backed EventStore.
pub struct Store {
    client: Client,
    rt: tokio::runtime::Runtime,
}

impl Store {
    /// Connect to KurrentDB, e.g. "kurrentdb://localhost:2113?tls=false".
    pub fn open(conn_string: &str) -> Result<Self, Error> {
        let settings = conn_string
            .parse()
            .map_err(|e| Error::Backend(format!("kurrentdb: parse connection string: {e}")))?;
        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .map_err(|e| Error::Backend(format!("kurrentdb: runtime: {e}")))?;
        // The client spawns background tasks on creation, so it must be built
        // inside the runtime context.
        let client = {
            let _guard = rt.enter();
            Client::new(settings).map_err(|e| Error::Backend(format!("kurrentdb: client: {e}")))?
        };
        Ok(Store { client, rt })
    }
}

fn to_stream_state(e: ExpectedRevision) -> StreamState {
    match e {
        ExpectedRevision::Any => StreamState::Any,
        ExpectedRevision::NoStream => StreamState::NoStream,
        ExpectedRevision::Exact(v) => StreamState::StreamRevision(v),
    }
}

fn all_position(from: Position) -> StreamPosition<KdbPosition> {
    if from == 0 {
        StreamPosition::Start
    } else {
        StreamPosition::Position(KdbPosition {
            commit: from,
            prepare: from,
        })
    }
}

fn all_filter(filter: &Filter) -> SubscriptionFilter {
    let base = SubscriptionFilter::on_stream_name();
    match &filter.stream_prefix {
        Some(p) => base.add_prefix(p),
        None => base.regex("^[^$].*"), // exclude system ($) streams
    }
}

fn original(ev: &ResolvedEvent) -> Option<&RecordedEvent> {
    ev.event.as_ref().or(ev.link.as_ref())
}

/// Convert a recorded event, skipping system streams and applying the prefix filter.
fn to_event(rec: &RecordedEvent, filter: &Filter) -> Option<Event> {
    let stream = rec.stream_id();
    if stream.starts_with('$') {
        return None;
    }
    if let Some(p) = &filter.stream_prefix {
        if !stream.starts_with(p.as_str()) {
            return None;
        }
    }
    Some(Event {
        id: rec.id.to_string(),
        type_: rec.event_type.clone(),
        data: rec.data.to_vec(),
        recorded_at: SystemTime::from(rec.created),
        position: rec.position.commit as Position,
    })
}

impl EventStore for Store {
    fn append(
        &self,
        stream: &str,
        expected: ExpectedRevision,
        events: &[Event],
    ) -> Result<Position, Error> {
        if events.is_empty() {
            return Ok(0);
        }
        let data: Vec<EventData> = events
            .iter()
            .map(|e| {
                let id = Uuid::parse_str(&e.id).unwrap_or_else(|_| Uuid::new_v4());
                EventData::binary(e.type_.clone(), e.data.clone().into()).id(id)
            })
            .collect();
        let opts = AppendToStreamOptions::default().stream_state(to_stream_state(expected));
        match self
            .rt
            .block_on(self.client.append_to_stream(stream, &opts, data))
        {
            Ok(w) => Ok(w.position.commit as Position),
            Err(kurrentdb::Error::WrongExpectedVersion { .. }) => Err(Error::Conflict {
                stream: stream.to_string(),
            }),
            Err(e) => Err(Error::Backend(format!("kurrentdb: append: {e}"))),
        }
    }

    fn read_stream(
        &self,
        stream: &str,
        from: Position,
        dir: Direction,
    ) -> Result<Vec<Event>, Error> {
        let opts = match dir {
            Direction::Forward => ReadStreamOptions::default()
                .position(StreamPosition::Start)
                .forwards(),
            Direction::Backward => ReadStreamOptions::default()
                .position(StreamPosition::End)
                .backwards(),
        };
        self.rt.block_on(async {
            let mut rs = match self.client.read_stream(stream, &opts).await {
                Ok(rs) => rs,
                Err(kurrentdb::Error::ResourceNotFound) => return Ok(Vec::new()),
                Err(e) => return Err(Error::Backend(format!("kurrentdb: read stream: {e}"))),
            };
            let mut out = Vec::new();
            loop {
                match rs.next().await {
                    Ok(Some(ev)) => {
                        if let Some(rec) = original(&ev) {
                            if let Some(e) = to_event(rec, &Filter::default()) {
                                if e.position > from {
                                    out.push(e);
                                }
                            }
                        }
                    }
                    Ok(None) => break,
                    Err(kurrentdb::Error::ResourceNotFound) => break,
                    Err(e) => return Err(Error::Backend(format!("kurrentdb: read stream: {e}"))),
                }
            }
            Ok(out)
        })
    }

    fn read_all(
        &self,
        from: Position,
        dir: Direction,
        filter: &Filter,
    ) -> Result<Vec<Event>, Error> {
        let opts = match dir {
            Direction::Forward => ReadAllOptions::default()
                .position(all_position(from))
                .forwards(),
            Direction::Backward => ReadAllOptions::default()
                .position(StreamPosition::End)
                .backwards(),
        };
        self.rt.block_on(async {
            let mut rs = self
                .client
                .read_all(&opts)
                .await
                .map_err(|e| Error::Backend(format!("kurrentdb: read all: {e}")))?;
            let mut out = Vec::new();
            loop {
                match rs.next().await {
                    Ok(Some(ev)) => {
                        if let Some(rec) = original(&ev) {
                            if let Some(e) = to_event(rec, filter) {
                                out.push(e);
                            }
                        }
                    }
                    Ok(None) => break,
                    Err(e) => return Err(Error::Backend(format!("kurrentdb: read all: {e}"))),
                }
            }
            Ok(out)
        })
    }

    fn subscribe_all(&self, from: Position, filter: &Filter) -> Result<Subscription, Error> {
        let client = self.client.clone();
        let filter = filter.clone();
        let (tx, rx) = channel();
        let stop = Arc::new(AtomicBool::new(false));
        let stop_thread = Arc::clone(&stop);
        let handle = std::thread::spawn(move || {
            let rt = match tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            {
                Ok(rt) => rt,
                Err(_) => return,
            };
            rt.block_on(async {
                let opts = SubscribeToAllOptions::default()
                    .position(all_position(from))
                    .filter(all_filter(&filter));
                let mut sub = client.subscribe_to_all(&opts).await;
                while !stop_thread.load(Ordering::Relaxed) {
                    match tokio::time::timeout(Duration::from_millis(200), sub.next()).await {
                        Ok(Ok(ev)) => {
                            if let Some(rec) = original(&ev) {
                                if let Some(e) = to_event(rec, &filter) {
                                    if tx.send(e).is_err() {
                                        return;
                                    }
                                }
                            }
                        }
                        Ok(Err(_)) => return,
                        Err(_) => {} // timeout; re-check stop
                    }
                }
            });
        });
        Ok(Subscription::new(rx, stop, handle))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;
    use testcontainers::core::{IntoContainerPort, WaitFor};
    use testcontainers::runners::AsyncRunner;
    use testcontainers::{GenericImage, ImageExt};

    fn wait_ready(store: &Store) {
        let deadline = Instant::now() + Duration::from_secs(60);
        while Instant::now() < deadline {
            if store
                .read_all(0, Direction::Forward, &Filter::default())
                .is_ok()
            {
                return;
            }
            std::thread::sleep(Duration::from_millis(500));
        }
        panic!("KurrentDB never became ready");
    }

    // Runs the backend-agnostic contract suite against a real KurrentDB in a
    // container. Skips if no container runtime is available.
    #[test]
    fn passes_the_contract() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let image = GenericImage::new("kurrentplatform/kurrentdb", "latest")
            .with_wait_for(WaitFor::message_on_stdout("IS LEADER"))
            .with_mapped_port(21133, 2113.tcp())
            .with_env_var("KURRENTDB_INSECURE", "true")
            .with_env_var("KURRENTDB_MEM_DB", "true")
            .with_env_var("KURRENTDB_RUN_PROJECTIONS", "None")
            .with_env_var("KURRENTDB_NODE_PORT", "2113");
        let container = match rt.block_on(image.start()) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("skipping KurrentDB contract test (no container runtime?): {e}");
                return;
            }
        };
        let conn = "kurrentdb://localhost:21133?tls=false".to_string();
        let store = Store::open(&conn).unwrap();
        // Run the contract, capturing any failure, so we always remove the
        // container (rm consumes it, avoiding a panicking Drop) before re-raising.
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            wait_ready(&store);
            crate::eventstore::contract::assert_contract(&store);
        }));
        drop(store);
        let _ = rt.block_on(container.rm());
        if let Err(e) = result {
            std::panic::resume_unwind(e);
        }
    }
}
