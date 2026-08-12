//! The KurrentDB EventStore adapter: it maps the async KurrentDB gRPC client onto
//! the (sync) eventstore port via a tokio runtime, so a project can swap the
//! embedded SQLite store for a shared KurrentDB server with no change to the rest
//! of Rigger. It passes the same contract suite SQLite does (proxy fidelity).
//!
//! KurrentDB owns the event id and recorded time; Rigger's `meta` and bi-temporal
//! `valid_from` ride in the event's custom metadata (an envelope), and the
//! per-stream `revision` maps to KurrentDB's event number.
//!
//! This backend implements NO content-identity suppression - it has no index over
//! event metadata to seek - so it appends every event through, which is the fail-safe
//! direction; it owns the port's HONESTY obligation in full, and reports positions the
//! server issued rather than any it could derive.
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
    AppendToStreamOptions, Client, ClientSettings, CurrentRevision, EventData,
    Position as KdbPosition, ReadAllOptions, ReadStreamOptions, RecordedEvent, ResolvedEvent,
    StreamPosition, StreamState, SubscribeToAllOptions, SubscribeToStreamOptions,
    SubscriptionFilter,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::{
    Appended, Direction, Error, Event, EventStore, ExpectedRevision, Filter, Position, Revision,
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
    ///
    /// The connection string is a SECRET wherever it appears (§48, secrets discipline): every error
    /// this function can surface names WHICH server it concerns for a useful diagnostic, but the
    /// message is scrubbed through the single [`redact_conn`](super::redact_conn) authority first, so
    /// the `user:password@` userinfo NEVER reaches an output path. That guards both the diagnostic we
    /// add (the redacted address) AND anything the underlying parse error might itself echo of the
    /// raw string. The string handed to the client is the verbatim `conn_string`, untouched -
    /// redaction lives on the error path only.
    pub fn open(conn_string: &str) -> Result<Self, Error> {
        // Scrub every message that could echo the connection string through the one redaction
        // authority, so a credential can never leak from a forgotten branch.
        let backend = |msg: String| Error::Backend(super::redact_conn(&msg));
        // The connection string is the adapter's ENTIRE topology input; it reaches the client
        // verbatim through `client_settings`, which injects no topology of its own (§48).
        let settings = Self::client_settings(conn_string)?;
        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .map_err(|e| Error::Backend(format!("kurrentdb: runtime: {e}")))?;
        // The client spawns background tasks on creation, so it must be built
        // inside the runtime context.
        let client = {
            let _guard = rt.enter();
            Client::new(settings)
                .map_err(|e| backend(format!("kurrentdb: client {conn_string}: {e}")))?
        };
        let store = Store { client, rt };
        // Fail fast on an unreachable server (§8): a trivial $all read forces the
        // lazy gRPC channel to connect now, not on the first append.
        store
            .read_all(0, Direction::Forward, &Filter::default())
            .map_err(|e| backend(format!("kurrentdb: connect to {conn_string}: {e}")))?;
        Ok(store)
    }

    /// Parse `conn_string` into the client's [`ClientSettings`] VERBATIM (§48, no topology
    /// opinions). The connection string is the adapter's ENTIRE topology input - host, port, TLS
    /// mode, credentials, and discovery all ride the string and reach the client through the
    /// settings this returns. The ONLY transformation is [`str::parse`]: the adapter injects
    /// nothing of its own - no default host, no localhost fallback, no forced-insecure downgrade,
    /// no dropped credential - so a centrally hosted deployment's remote, TLS-secured, credentialed
    /// address is honored exactly. On a parse failure the message is scrubbed through the single
    /// [`redact_conn`](super::redact_conn) authority, so a credential in the string never reaches
    /// an output path.
    fn client_settings(conn_string: &str) -> Result<ClientSettings, Error> {
        conn_string.parse().map_err(|e| {
            Error::Backend(super::redact_conn(&format!(
                "kurrentdb: parse connection string {conn_string}: {e}"
            )))
        })
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

impl Store {
    /// Read a stream forward from `from` (inclusive revision), stopping once `limit`
    /// events have been collected. This is the ONE read this adapter drives: the port's
    /// `read_stream` passes `usize::MAX` (no bound) and the append's position read-back
    /// passes the batch size, so a read-back after a big append never walks a whole
    /// stream.
    fn read_forward(
        &self,
        stream: &str,
        from: Revision,
        limit: usize,
    ) -> Result<Vec<Event>, Error> {
        // `from` is an inclusive lower bound on revision and the direction only
        // controls order (matching the SQLite sibling and the trait convention),
        // so a backward read is the forward set reversed. Reading forward from
        // `from` and reversing honors `from` in both directions; KurrentDB's
        // native `.backwards()` from End would discard `from` entirely.
        let opts = ReadStreamOptions::default()
            .position(stream_position(from))
            .forwards();
        self.rt.block_on(async {
            let mut rs = match self.client.read_stream(stream, &opts).await {
                Ok(rs) => rs,
                Err(kurrentdb::Error::ResourceNotFound) => return Ok(Vec::new()),
                Err(e) => return Err(Error::Backend(format!("kurrentdb: read stream: {e}"))),
            };
            let mut out = Vec::new();
            while out.len() < limit {
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
        })
    }

    /// The global positions the server issued for the `n` events a just-committed
    /// append landed at revisions `first ..= first + n - 1`, read back from the stream.
    ///
    /// A read that comes back short is a replica that has not caught up yet, so the
    /// read is retried within a short bound. If it still cannot be resolved the append
    /// is reported as an error that SAYS the write landed: fabricating the positions
    /// instead would stamp a fold at locations the server never issued, and the
    /// projection's applied ledger is keyed by position - a wrong one is permanent.
    fn read_back_positions(
        &self,
        stream: &str,
        first: Revision,
        n: usize,
    ) -> Result<Vec<Position>, Error> {
        let deadline = std::time::Instant::now() + Duration::from_secs(2);
        loop {
            let got = self.read_forward(stream, first, n)?;
            if got.len() == n {
                return Ok(got.into_iter().map(|e| e.position).collect());
            }
            if std::time::Instant::now() >= deadline {
                return Err(Error::Backend(format!(
                    "kurrentdb: append to {stream:?} COMMITTED {n} event(s) at revisions \
                     {first}..={} but only {} could be read back, so the positions the server \
                     issued cannot be reported; the events are durable in the log",
                    first + n as Revision - 1,
                    got.len()
                )));
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }
}

impl EventStore for Store {
    fn append(
        &self,
        stream: &str,
        expected: ExpectedRevision,
        events: &[Event],
    ) -> Result<Appended, Error> {
        if events.is_empty() {
            return Ok(Appended::default());
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
            // This backend runs no content-identity guard - it has no index over event
            // metadata to seek - so it APPENDS THROUGH: every handed event is written
            // and reported written, which is the fail-safe direction (it can only ever
            // write more, never drop). Only the HONESTY half of the port is owed here,
            // and it is owed in full: each reported position must be one the server
            // ISSUED for that event.
            //
            // A single-event append gets that for free (the write's own commit
            // position IS that event's). A multi-event append does not: KurrentDB's
            // `$all` position is a byte offset, so the earlier events' positions are
            // not derivable from the last one by any arithmetic. They are READ BACK
            // from the stream - the batch occupies the `n` revisions ending at
            // `next_expected_version` - never invented.
            Ok(w) if events.len() == 1 => Ok(Appended::all(vec![w.position.commit as Position])),
            Ok(w) => {
                let n = events.len();
                let first = (w.next_expected_version as Revision) - (n as Revision - 1);
                let written = self.read_back_positions(stream, first, n)?;
                Ok(Appended::all(written))
            }
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
        let mut out = self.read_forward(stream, from, usize::MAX)?;
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

    /// Spec 48 - NO TOPOLOGY OPINIONS. The connection string is the adapter's ENTIRE topology
    /// input: host, port, TLS mode, and credentials all ride the string and reach the client
    /// VERBATIM through `client_settings` (the exact step `Store::open` uses to turn the string
    /// into the client's settings). The adapter injects nothing of its own - no default host, no
    /// localhost fallback, no forced-insecure downgrade, no dropped credential. This drives that
    /// seam with a REMOTE host, TLS explicitly on, a non-default port, and real credentials - none
    /// of which a local-container assumption would produce - and asserts every field survives
    /// unaltered. Hermetic: `client_settings` only parses, so no server or container is needed, and
    /// it runs identically in both feature lanes.
    #[test]
    fn client_settings_pass_host_tls_and_credentials_through_verbatim() {
        use kurrentdb::Credentials;
        let conn = "kurrentdb://app:s3cr3t@events.internal.example:2113?tls=true";
        let settings = Store::client_settings(conn).expect("a well-formed conn parses");

        let hosts = settings.hosts();
        assert_eq!(
            hosts.len(),
            1,
            "the single named host reaches the client, nothing added"
        );
        assert_eq!(
            hosts[0].host, "events.internal.example",
            "the remote host reaches the client verbatim - no localhost injected"
        );
        assert_eq!(hosts[0].port, 2113, "the port reaches the client verbatim");
        assert!(
            settings.is_secure_mode_enabled(),
            "TLS reaches the client verbatim - no insecure downgrade injected"
        );
        assert_eq!(
            settings.default_authenticated_user(),
            &Some(Credentials::new("app".to_string(), "s3cr3t".to_string())),
            "the credentials reach the client verbatim - none dropped or rewritten"
        );
    }

    /// Spec 48 - NO TOPOLOGY OPINIONS, the "no insecure assumption injected" clause specifically.
    /// When the connection string is SILENT on TLS, the adapter adds no opinion of its own: it does
    /// not append `?tls=false` or otherwise downgrade the connection. The parsed settings keep the
    /// client's own secure-by-default posture, proving the adapter injects no insecure default. A
    /// regression that forced insecurity before parsing would flip this assertion to false.
    #[test]
    fn client_settings_inject_no_insecure_default_when_the_string_is_silent_on_tls() {
        let conn = "kurrentdb://events.internal.example:2113";
        let settings = Store::client_settings(conn).expect("a well-formed conn parses");
        assert!(
            settings.is_secure_mode_enabled(),
            "a TLS-silent conn stays secure - the adapter injects no insecure downgrade"
        );
    }
}
