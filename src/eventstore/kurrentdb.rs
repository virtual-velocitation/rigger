//! The KurrentDB EventStore adapter: it maps the async KurrentDB gRPC client onto
//! the (sync) eventstore port via a tokio runtime, so a project can swap the
//! embedded SQLite store for a shared KurrentDB server with no change to the rest
//! of Rigger. It passes the same contract suite SQLite does (proxy fidelity).
//!
//! KurrentDB owns the event id and recorded time; Rigger's `meta` and bi-temporal
//! `valid_from` ride in the event's custom metadata (an envelope), and the
//! per-stream `revision` maps to KurrentDB's event number.
//!
//! ## Boundary normalization
//!
//! The [`EventStore`] trait fixes the `from` boundary convention (see its doc):
//! stream-scoped reads/subscriptions are inclusive of `from`, `$all`-scoped ones
//! are exclusive. KurrentDB's native boundaries differ from that and from each
//! other - a `read_*` from a position is inclusive while a `subscribe_*` from a
//! position is exclusive - so this adapter normalizes both onto the trait
//! convention rather than leaking KurrentDB's raw semantics.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::channel;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use kurrentdb::{
    AppendToStreamOptions, Client, CurrentRevision, EventData, Position as KdbPosition,
    ReadAllOptions, ReadStreamOptions, RecordedEvent, ResolvedEvent, StreamPosition, StreamState,
    SubscribeToAllOptions, SubscribeToStreamOptions, SubscriptionFilter,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::{
    Direction, Error, Event, EventStore, ExpectedRevision, Filter, Position, Revision,
    Subscription, NO_STREAM,
};

/// The envelope carrying Rigger's metadata and valid-time in KurrentDB's custom
/// event metadata (KurrentDB owns the id and recorded time).
#[derive(Serialize, Deserialize, Default)]
struct Envelope {
    #[serde(default)]
    meta: BTreeMap<String, String>,
    #[serde(default)]
    valid_from_nanos: i64,
}

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
        let store = Store { client, rt };
        // Fail fast on an unreachable server (§8): a trivial $all read forces the
        // lazy gRPC channel to connect now, not on the first append.
        store
            .read_all(0, Direction::Forward, &Filter::default())
            .map_err(|e| Error::Backend(format!("kurrentdb: connect: {e}")))?;
        Ok(store)
    }
}

fn to_nanos(t: SystemTime) -> i64 {
    t.duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as i64)
        .unwrap_or(0)
}

fn from_nanos(n: i64) -> SystemTime {
    UNIX_EPOCH + Duration::from_nanos(n.max(0) as u64)
}

/// Map the server's authoritative `CurrentRevision` (from a conflict payload)
/// onto Rigger's [`Revision`]: an existing stream's last event number, or
/// [`NO_STREAM`] for a stream that does not exist.
fn current_revision_to_actual(current: CurrentRevision) -> Revision {
    match current {
        CurrentRevision::Current(n) => n as Revision,
        CurrentRevision::NoStream => NO_STREAM,
    }
}

fn to_stream_state(e: ExpectedRevision) -> StreamState {
    match e {
        ExpectedRevision::Any => StreamState::Any,
        ExpectedRevision::NoStream => StreamState::NoStream,
        ExpectedRevision::Exact(v) => StreamState::StreamRevision(v.max(0) as u64),
    }
}

/// Start position for a `$all` read from `from`. KurrentDB's `$all` position is a
/// `(commit, prepare)` pair; Rigger's [`Position`] carries the commit half (see
/// the [`Position`] doc for why that round-trips), so we rebuild the pair as
/// `(from, from)`. KurrentDB's read-from-position is *inclusive*, so callers that
/// need the trait's exclusive `$all` boundary drop the boundary event afterward.
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

/// Start position for an *inclusive*-`from` stream read: KurrentDB's
/// read-from-position is inclusive, so this points right at `from`.
fn stream_position(from: Revision) -> StreamPosition<u64> {
    if from <= 0 {
        StreamPosition::Start
    } else {
        StreamPosition::Position(from as u64)
    }
}

/// Start position for an *inclusive*-`from` stream subscription. KurrentDB's
/// subscribe-from-position is *exclusive* (it resumes after the checkpoint), so
/// to include `from` we anchor one revision earlier. This makes a catch-up
/// subscription replay the same boundary event a `read_stream(.., from, ..)`
/// returns, per the trait's inclusive stream-scope convention.
fn stream_subscribe_position(from: Revision) -> StreamPosition<u64> {
    if from <= 0 {
        StreamPosition::Start
    } else {
        StreamPosition::Position((from - 1) as u64)
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
    let env: Envelope = serde_json::from_slice(&rec.custom_metadata).unwrap_or_default();
    Some(Event {
        id: rec.id.to_string(),
        stream: stream.to_string(),
        type_: rec.event_type.clone(),
        data: rec.data.to_vec(),
        meta: env.meta,
        valid_from: from_nanos(env.valid_from_nanos),
        recorded_at: SystemTime::from(rec.created),
        position: rec.position.commit as Position,
        revision: rec.revision as Revision,
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
                let env = Envelope {
                    meta: e.meta.clone(),
                    valid_from_nanos: to_nanos(e.valid_from),
                };
                let meta_bytes = serde_json::to_vec(&env).unwrap_or_default();
                EventData::binary(e.type_.clone(), e.data.clone().into())
                    .id(id)
                    .metadata(meta_bytes.into())
            })
            .collect();
        let opts = AppendToStreamOptions::default().stream_state(to_stream_state(expected));
        match self
            .rt
            .block_on(self.client.append_to_stream(stream, &opts, data))
        {
            Ok(w) => Ok(w.position.commit as Position),
            // The server already reports the stream's authoritative current
            // revision in the conflict payload; use it directly rather than
            // racing a second network read that could observe a newer (or, on a
            // delete, vanished) revision than the one the append conflicted with.
            Err(kurrentdb::Error::WrongExpectedVersion { current, .. }) => Err(Error::Conflict {
                stream: stream.to_string(),
                expected,
                actual: current_revision_to_actual(current),
            }),
            Err(e) => Err(Error::Backend(format!("kurrentdb: append: {e}"))),
        }
    }

    fn read_stream(
        &self,
        stream: &str,
        from: Revision,
        dir: Direction,
    ) -> Result<Vec<Event>, Error> {
        // `from` is an inclusive lower bound on revision and the direction only
        // controls order (matching the SQLite sibling and the trait convention),
        // so a backward read is the forward set reversed. Reading forward from
        // `from` and reversing honors `from` in both directions; KurrentDB's
        // native `.backwards()` from End would discard `from` entirely.
        let opts = ReadStreamOptions::default()
            .position(stream_position(from))
            .forwards();
        let mut out = self.rt.block_on(async {
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
                                if e.revision >= from {
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
            Ok::<_, Error>(out)
        })?;
        if matches!(dir, Direction::Backward) {
            out.reverse();
        }
        Ok(out)
    }

    fn read_all(
        &self,
        from: Position,
        dir: Direction,
        filter: &Filter,
    ) -> Result<Vec<Event>, Error> {
        // `$all` `from` is an exclusive lower bound on position and the direction
        // only controls order, so a backward read is the forward set reversed.
        // KurrentDB's read-from-position is *inclusive*, so we start the read at
        // `from` and drop the boundary event (`position > from`) to honor the
        // trait's exclusive `$all` convention. Reading forward and reversing
        // honors `from` in both directions; KurrentDB's native `.backwards()`
        // from End would discard `from`.
        let opts = ReadAllOptions::default()
            .position(all_position(from))
            .forwards();
        let mut out = self.rt.block_on(async {
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
                                if from == 0 || e.position > from {
                                    out.push(e);
                                }
                            }
                        }
                    }
                    Ok(None) => break,
                    Err(e) => return Err(Error::Backend(format!("kurrentdb: read all: {e}"))),
                }
            }
            Ok::<_, Error>(out)
        })?;
        if matches!(dir, Direction::Backward) {
            out.reverse();
        }
        Ok(out)
    }

    fn subscribe_all(&self, from: Position, filter: &Filter) -> Result<Subscription, Error> {
        let client = self.client.clone();
        let filter = filter.clone();
        let (tx, rx) = channel();
        let err = Arc::new(Mutex::new(None));
        let stop = Arc::new(AtomicBool::new(false));
        let (stop_t, err_t) = (Arc::clone(&stop), Arc::clone(&err));
        let handle = std::thread::spawn(move || {
            let rt = match current_thread_rt(&err_t) {
                Some(rt) => rt,
                None => return,
            };
            rt.block_on(async {
                // KurrentDB's subscribe-from-position is *exclusive*, which is
                // already the trait's `$all` convention - no boundary adjustment
                // needed. So `subscribe_all(p)` and `read_all(p, ..)` from the
                // same `p` replay the identical set (events after `p`).
                let opts = SubscribeToAllOptions::default()
                    .position(all_position(from))
                    .filter(all_filter(&filter));
                let mut sub = client.subscribe_to_all(&opts).await;
                forward_loop(&mut sub, &stop_t, &tx, &err_t, &filter).await;
            });
        });
        Ok(Subscription::new(rx, err, stop, handle))
    }

    fn subscribe_stream(&self, stream: &str, from: Revision) -> Result<Subscription, Error> {
        let client = self.client.clone();
        let stream = stream.to_string();
        let (tx, rx) = channel();
        let err = Arc::new(Mutex::new(None));
        let stop = Arc::new(AtomicBool::new(false));
        let (stop_t, err_t) = (Arc::clone(&stop), Arc::clone(&err));
        let handle = std::thread::spawn(move || {
            let rt = match current_thread_rt(&err_t) {
                Some(rt) => rt,
                None => return,
            };
            rt.block_on(async {
                // Anchor one revision before `from` because KurrentDB's
                // subscribe-from-position is exclusive but the trait's
                // stream-scope convention is inclusive - so the subscription
                // replays the same boundary event `read_stream(.., from, ..)`
                // returns.
                let opts =
                    SubscribeToStreamOptions::default().start_from(stream_subscribe_position(from));
                let mut sub = client.subscribe_to_stream(stream.as_str(), &opts).await;
                forward_loop(&mut sub, &stop_t, &tx, &err_t, &Filter::default()).await;
            });
        });
        Ok(Subscription::new(rx, err, stop, handle))
    }
}

fn current_thread_rt(err: &Arc<Mutex<Option<String>>>) -> Option<tokio::runtime::Runtime> {
    match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(rt) => Some(rt),
        Err(e) => {
            *err.lock().unwrap() = Some(e.to_string());
            None
        }
    }
}

/// Drive a KurrentDB subscription until stopped, converting and forwarding events.
async fn forward_loop(
    sub: &mut kurrentdb::Subscription,
    stop: &Arc<AtomicBool>,
    tx: &std::sync::mpsc::Sender<Event>,
    err: &Arc<Mutex<Option<String>>>,
    filter: &Filter,
) {
    while !stop.load(Ordering::Relaxed) {
        match tokio::time::timeout(Duration::from_millis(200), sub.next()).await {
            Ok(Ok(ev)) => {
                if let Some(rec) = original(&ev) {
                    if let Some(e) = to_event(rec, filter) {
                        if tx.send(e).is_err() {
                            return;
                        }
                    }
                }
            }
            Ok(Err(e)) => {
                *err.lock().unwrap() = Some(e.to_string());
                return;
            }
            Err(_) => {} // timeout; re-check stop
        }
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
        // Wait for readiness before Store::open (which now connects eagerly).
        std::thread::sleep(Duration::from_secs(2));
        let conn = "kurrentdb://localhost:21133?tls=false".to_string();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            // open retries via the readiness loop using a short-lived raw client check
            let mut store = Store::open(&conn);
            let deadline = Instant::now() + Duration::from_secs(60);
            while store.is_err() && Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(500));
                store = Store::open(&conn);
            }
            let store = store.expect("KurrentDB never became ready");
            wait_ready(&store);
            crate::eventstore::contract::assert_contract(&store);
        }));
        let _ = rt.block_on(container.rm());
        if let Err(e) = result {
            std::panic::resume_unwind(e);
        }
    }
}
