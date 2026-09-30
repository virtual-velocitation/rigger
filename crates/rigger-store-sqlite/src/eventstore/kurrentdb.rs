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
//! ## Group streams
//!
//! The group lookup ([`EventStore::latest_in_group`], spec 101) is answered from one GROUP STREAM
//! per identity (`rigger-group/<stream>/<group>`) holding KurrentDB link events (`$>`, the server's
//! own link type). An append whose events carry a group gives every event its id first, then links,
//! in each group's stream, the revision that group's newest event in the append will take together
//! with that event's id, then appends the events expecting exactly the revision it linked against.
//! A link answers only when its revision holds the very event it reserved: a crash, or a writer
//! that lost its revision to another, leaves a link whose revision holds nothing or another write -
//! even an older member of the same group - and the lookup skips it. Because every recording is
//! linked before it is written, and a later recording reads the stream only after an earlier one
//! landed, the newest link that holds its own event names the group's newest member. No link
//! reaches a caller: every read and subscription skips a record whose type is a system (`$`) type.
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
use std::time::{Duration, SystemTime};

use kurrentdb::{
    AppendToStreamOptions, Client, ClientSettings, CurrentRevision, EventData,
    Position as KdbPosition, ReadAllOptions, ReadStreamOptions, RecordedEvent, ResolvedEvent,
    StreamPosition, StreamState, SubscribeToAllOptions, SubscribeToStreamOptions,
    SubscriptionFilter,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::{
    from_nanos, to_nanos, Appended, Direction, Error, Event, EventBatchSink, EventStore,
    ExpectedRevision, Filter, GroupHead, Position, Revision, Subscription, TypeSelection,
    META_GROUP, NO_STREAM,
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

/// The fields of a recorded event [`to_event`] decodes, as plain values: the one view of a server
/// record, so the decode and its delivery rules hold without a server.
struct Record<'a> {
    stream: &'a str,
    id: Uuid,
    event_type: &'a str,
    data: &'a [u8],
    custom_metadata: &'a [u8],
    created: SystemTime,
    commit: u64,
    revision: u64,
}

impl<'a> From<&'a RecordedEvent> for Record<'a> {
    fn from(rec: &'a RecordedEvent) -> Self {
        Record {
            stream: rec.stream_id(),
            id: rec.id,
            event_type: &rec.event_type,
            data: &rec.data,
            custom_metadata: &rec.custom_metadata,
            created: SystemTime::from(rec.created),
            commit: rec.position.commit,
            revision: rec.revision,
        }
    }
}

/// The envelope a record's custom metadata carries; an unreadable one carries nothing.
fn envelope(custom_metadata: &[u8]) -> Envelope {
    serde_json::from_slice(custom_metadata).unwrap_or_default()
}

/// Whether a record of `stream` of type `event_type` reaches a caller under `filter`: never a
/// system stream's record nor a system (`$`) record - a group stream's links among them - and only
/// a record of a stream the filter's prefix admits.
fn delivered(stream: &str, event_type: &str, filter: &Filter) -> bool {
    !stream.starts_with('$')
        && !event_type.starts_with('$')
        && filter
            .stream_prefix
            .as_ref()
            .is_none_or(|p| stream.starts_with(p.as_str()))
}

/// Convert a recorded event, skipping every record [`delivered`] refuses.
fn to_event<'a>(rec: impl Into<Record<'a>>, filter: &Filter) -> Option<Event> {
    let rec = rec.into();
    delivered(rec.stream, rec.event_type, filter).then(|| {
        let env = envelope(rec.custom_metadata);
        Event {
            id: rec.id.to_string(),
            stream: rec.stream.to_string(),
            type_: rec.event_type.to_string(),
            data: rec.data.to_vec(),
            meta: env.meta,
            valid_from: from_nanos(env.valid_from_nanos),
            recorded_at: rec.created,
            position: rec.commit as Position,
            revision: rec.revision as Revision,
        }
    })
}

/// The server-side `$all` event-type filter a typed read hands the server (spec 101): `Only`
/// matches exactly the named types, `Except` every type but the named ones and never a system
/// (`$`) type, so a refused event - a derived one, or a link or metadata record - never leaves
/// the server. Rigger's type names are plain identifiers, so each matches only itself.
fn typed_read_filter(selection: TypeSelection) -> String {
    match selection {
        TypeSelection::Only(types) => format!("^(?:{})$", types.join("|")),
        TypeSelection::Except(types) => format!("^(?!\\$)(?!(?:{})$)", types.join("|")),
    }
}

/// The `$all` read a typed read drives from `start`: forward, filtered by the server on event
/// type ([`typed_read_filter`]).
fn typed_read_options(
    start: StreamPosition<KdbPosition>,
    selection: TypeSelection,
) -> ReadAllOptions {
    ReadAllOptions::default()
        .position(start)
        .forwards()
        .filter(SubscriptionFilter::on_event_type().regex(typed_read_filter(selection)))
}

/// Where a typed read from revision `from` starts in `$all`: the log's start for a `from` of 0 (or
/// below), else the position of the event at that revision, which `anchor` reads back (one event)
/// - `None` when the stream holds no such event, so the read has nothing to hand back.
fn typed_read_start(
    from: Revision,
    anchor: impl FnOnce() -> Result<Option<Position>, Error>,
) -> Result<Option<StreamPosition<KdbPosition>>, Error> {
    if from > 0 {
        Ok(anchor()?.map(all_position))
    } else {
        Ok(Some(StreamPosition::Start))
    }
}

/// The events of `stream` among a typed read's `(stream id, record)` pulls, in pull order: a
/// record of another stream is refused on its stream id BEFORE `decode` ever sees it, and a pull
/// the server failed ends the read in that failure.
fn records_of<R>(
    pulls: impl IntoIterator<Item = Result<(String, R), kurrentdb::Error>>,
    stream: &str,
    decode: impl Fn(&R) -> Option<Event>,
) -> Result<Vec<Event>, Error> {
    let mut out = Vec::new();
    for pull in pulls {
        let (id, record) =
            pull.map_err(|e| Error::Backend(format!("kurrentdb: read typed: {e}")))?;
        if id == stream {
            out.extend(decode(&record));
        }
    }
    Ok(out)
}

/// The group stream holding `group`'s links for `stream` (spec 101).
fn group_stream(stream: &str, group: &str) -> String {
    format!("rigger-group/{stream}/{group}")
}

/// The metadata key under which a group link carries the id of the event it reserves.
const META_LINKED_ID: &str = "linked-id";

/// The link event naming the event at `revision` of `stream`: the server's own `$>` link, whose
/// data is `<revision>@<stream>`, carrying the `id` of the event it reserves that revision for.
fn link_event(stream: &str, revision: Revision, id: &str) -> Event {
    Event::new("$>", format!("{revision}@{stream}").into_bytes()).with_meta(META_LINKED_ID, id)
}

/// The id a group link reserved, read from the link record's custom metadata.
fn linked_id(custom_metadata: &[u8]) -> Option<String> {
    envelope(custom_metadata).meta.remove(META_LINKED_ID)
}

/// The uuid an event with id `id` is written under: its own when `id` is one, a fresh one
/// otherwise.
fn event_id(id: &str) -> Uuid {
    Uuid::parse_str(id).unwrap_or_else(|_| Uuid::new_v4())
}

/// `events`, each carrying the id it will be written under ([`event_id`]), so a group link can
/// name the exact event it reserves before the write.
fn with_ids(events: &[Event]) -> Vec<Event> {
    events
        .iter()
        .map(|event| {
            let mut event = event.clone();
            event.id = event_id(&event.id).to_string();
            event
        })
        .collect()
}

/// The links an append of `events` after revision `last` takes: for every distinct group among
/// the events, in first-appearance order, the revision and id of that group's NEWEST event in the
/// append - the member the lookup must answer.
fn group_links(last: Revision, events: &[Event]) -> Vec<(&str, Revision, &str)> {
    let mut links: Vec<(&str, Revision, &str)> = Vec::new();
    for (revision, event) in (last + 1..).zip(events) {
        if let Some(group) = event.meta.get(META_GROUP) {
            let link = (group.as_str(), revision, event.id.as_str());
            match links.iter_mut().find(|(linked, _, _)| linked == group) {
                Some(slot) => *slot = link,
                None => links.push(link),
            }
        }
    }
    links
}

/// The expectation an append pinned to `last` writes under: the stream must still end at `last`
/// (or still not exist).
fn pinned(last: Revision) -> ExpectedRevision {
    if last == NO_STREAM {
        ExpectedRevision::NoStream
    } else {
        ExpectedRevision::Exact(last)
    }
}

/// THE GROUP-LINK PROTOCOL of an append whose events carry a group (spec 101), over the three
/// server calls it makes: read the stream's `last_revision`, `link` each group's newest revision
/// and event id in its group stream, then `write` the events pinned to that revision. An expectation the stream
/// does not meet is refused before anything is linked; a pinned write that conflicts is retried
/// from a fresh read when the caller expected `Any`, and is the caller's conflict otherwise.
fn append_linked(
    stream: &str,
    expected: ExpectedRevision,
    events: &[Event],
    mut last_revision: impl FnMut() -> Result<Revision, Error>,
    mut link: impl FnMut(&str, Revision, &str) -> Result<(), Error>,
    mut write: impl FnMut(ExpectedRevision) -> Result<Appended, Error>,
) -> Result<Appended, Error> {
    loop {
        let last = last_revision()?;
        if !expected.admits(last) {
            return Err(Error::Conflict {
                stream: stream.to_string(),
                expected,
                actual: last,
            });
        }
        for (group, revision, id) in group_links(last, events) {
            link(group, revision, id)?;
        }
        match write(pinned(last)) {
            Err(Error::Conflict { .. }) if expected == ExpectedRevision::Any => continue,
            written => return written,
        }
    }
}

/// THE FIRST-MATCH SCAN over a newest-first read: the first record `pick` accepts, pulling
/// nothing past it. A stream the server does not know - at the open or mid-read - has no match;
/// any other server failure is an error naming `step`.
fn first_match<X, T>(
    records: impl IntoIterator<Item = Result<X, kurrentdb::Error>>,
    step: &str,
    mut pick: impl FnMut(X) -> Option<T>,
) -> Result<Option<T>, Error> {
    for record in records {
        match record.map(&mut pick) {
            Ok(Some(found)) => return Ok(Some(found)),
            Ok(None) => {}
            Err(kurrentdb::Error::ResourceNotFound) => return Ok(None),
            Err(e) => return Err(Error::Backend(format!("kurrentdb: {step}: {e}"))),
        }
    }
    Ok(None)
}

/// The group lookup's scan over a group stream's newest-first links, each as the id it reserved
/// and the event its revision holds: the first link whose revision holds exactly the event it
/// reserved. A link holding nothing or another write is skipped; a group stream the server does
/// not know has no member.
fn newest_in_group(
    links: impl IntoIterator<Item = Result<(Option<String>, Option<Event>), kurrentdb::Error>>,
) -> Result<Option<GroupHead>, Error> {
    first_match(links, "latest in group", |(linked, event)| {
        event
            .filter(|event| linked.as_deref() == Some(event.id.as_str()))
            .map(|event| GroupHead {
                position: event.position,
                type_: event.type_,
                meta: event.meta,
            })
    })
}

/// The revision of a newest-first read's first record (each its revision, `None` for a record
/// carrying none), or [`NO_STREAM`] for a stream with none.
fn newest_revision(
    records: impl IntoIterator<Item = Result<Option<u64>, kurrentdb::Error>>,
) -> Result<Revision, Error> {
    Ok(first_match(records, "last revision", |revision| revision)?
        .map_or(NO_STREAM, |revision| revision as Revision))
}

impl Store {
    /// THE NEWEST-FIRST READ: `stream` read backward from its end with links resolved (a stream
    /// holding none reads as it is), each record pulled from the server only when the consumer
    /// asks for it, so a scan that stops at its first match reads nothing past it. A failure to
    /// open the read is its one item.
    fn newest_first(
        &self,
        stream: &str,
    ) -> impl Iterator<Item = Result<ResolvedEvent, kurrentdb::Error>> + '_ {
        let opts = ReadStreamOptions::default()
            .position(StreamPosition::End)
            .backwards()
            .resolve_link_tos();
        let mut read = self
            .rt
            .block_on(self.client.read_stream(stream, &opts))
            .map_err(Some);
        std::iter::from_fn(move || match &mut read {
            Ok(rs) => self.rt.block_on(rs.next()).transpose(),
            Err(opened) => opened.take().map(Err),
        })
    }

    /// The revision of `stream`'s last event ([`newest_revision`] over the newest-first read), or
    /// [`NO_STREAM`] for a stream that does not exist.
    fn last_revision(&self, stream: &str) -> Result<Revision, Error> {
        newest_revision(
            self.newest_first(stream)
                .map(|pulled| pulled.map(|ev| original(&ev).map(|rec| rec.revision))),
        )
    }

    /// THE FORWARD READ: the ONE forward read of a stream this adapter drives - `stream`'s
    /// records from revision `from` (inclusive) on, each pulled from the server only when the
    /// consumer asks for it, so a consumer holds no more than it keeps: the events of the port's
    /// `read_stream`, `read_stream_batched` and the few the append's position read-back and a
    /// typed read's anchor need ([`Store::forward_events`]), and the positions of
    /// `read_stream_positions`. A stream the server does not know, at the open or mid-stream,
    /// reads as ending there; any other failure is the read's one error, its last item.
    fn forward_records(
        &self,
        stream: &str,
        from: Revision,
    ) -> impl Iterator<Item = Result<ResolvedEvent, Error>> + '_ {
        let failed = |e: kurrentdb::Error| Error::Backend(format!("kurrentdb: read stream: {e}"));
        let opts = ReadStreamOptions::default()
            .position(stream_position(from))
            .forwards();
        let mut read = match self.rt.block_on(self.client.read_stream(stream, &opts)) {
            Ok(rs) => Some(Ok(rs)),
            Err(kurrentdb::Error::ResourceNotFound) => None,
            Err(e) => Some(Err(Some(failed(e)))),
        };
        std::iter::from_fn(move || match read.as_mut()? {
            Ok(rs) => match self.rt.block_on(rs.next()) {
                Ok(Some(ev)) => Some(Ok(ev)),
                Ok(None) | Err(kurrentdb::Error::ResourceNotFound) => None,
                Err(e) => Some(Err(failed(e))),
            },
            Err(opened) => opened.take().map(Err),
        })
    }

    /// The events of `stream` from revision `from` (inclusive) on, each built from its record as
    /// the forward read ([`Store::forward_records`]) pulls it. `from` is an inclusive lower bound
    /// on revision and a read's direction only controls order (matching the SQLite sibling and the
    /// trait convention), so a backward read is this set reversed; KurrentDB's native
    /// `.backwards()` from End would discard `from` entirely.
    fn forward_events(
        &self,
        stream: &str,
        from: Revision,
    ) -> impl Iterator<Item = Result<Event, Error>> + '_ {
        self.forward_records(stream, from)
            .filter_map(move |pulled| {
                pulled
                    .map(|ev| {
                        original(&ev)
                            .and_then(|rec| to_event(rec, &Filter::default()))
                            .filter(|e| e.revision >= from)
                    })
                    .transpose()
            })
    }

    /// The first `limit` events of `stream` from revision `from` (inclusive), pulled no further
    /// ([`Store::forward_events`]): the port's `read_stream` passes `usize::MAX` (no bound), and
    /// the append's position read-back and a typed read's anchor pass the few events they need,
    /// so neither ever walks a whole stream.
    fn read_forward(
        &self,
        stream: &str,
        from: Revision,
        limit: usize,
    ) -> Result<Vec<Event>, Error> {
        self.forward_events(stream, from).take(limit).collect()
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

/// Where a successful append ack lets the adapter say the batch landed - and nothing more
/// than that.
#[derive(Debug, PartialEq)]
enum AckPlacement {
    /// The ONE event's own commit position, issued by the server and reportable as is.
    Issued(Position),
    /// The batch occupies the `n` revisions ENDING at the ack's revision; the positions
    /// must be read back from the stream starting at this revision.
    ReadBackFrom(Revision),
}

/// Read a successful append ack for what the SERVER ISSUED, refusing anything it did not.
///
/// The client renders an ABSENT value as a zero in both fields of the ack it hands back:
/// its "no position" case maps to the start of the log (commit 0) and its "no stream"
/// case maps to revision 0. A zero is therefore ambiguous by construction - the server
/// saying "none" is spelled exactly like the first location in the log - and the adapter
/// must never resolve that ambiguity in its own favour, because both fabrications are
/// permanent once folded:
///
/// - a fabricated commit position of 0 is folded at position 0, which the projection's
///   applied ledger then marks applied FOREVER, swallowing the genuine event recorded
///   there;
/// - a fabricated revision under-runs the batch's first revision into the NEGATIVE, and
///   `read_forward`'s `revision >= from` test admits a negative `from` from the stream's
///   start - so the read-back would return the stream's FIRST `n` events and report some
///   earlier append's placements as this one's.
///
/// Neither is ever resolved by GUESSING. The batch case resolves it by ASKING - the
/// positions are read back from the stream - and the single-event case does exactly the
/// same thing rather than answering the ambiguity on its own: an ack with no position is
/// read back from the revision the ack names, so the FIRST event ever written to a fresh
/// log (whose commit position genuinely is 0, and whose append would otherwise be
/// refused for succeeding) reports the position the server issued, like every other
/// append. One resolution for both shapes, and it is the server's.
///
/// What remains refused is the shape no read-back can resolve: an ack whose revision is
/// absent under a BATCH under-runs the batch's first revision into the NEGATIVE, and
/// `read_forward`'s `revision >= from` test admits a negative `from` from the stream's
/// start - so the read would return the stream's FIRST `n` events and report some
/// earlier append's placements as this one's. The refusal says the write LANDED and only
/// the reporting failed - the same shape [`Store::read_back_positions`] uses for a
/// read-back it cannot resolve. A refusal is recoverable; a fabricated position is not.
fn placement_of_ack(
    stream: &str,
    n: usize,
    commit: u64,
    next_expected_version: u64,
) -> Result<AckPlacement, Error> {
    if n == 1 && commit != 0 {
        return Ok(AckPlacement::Issued(commit as Position));
    }
    let first = (next_expected_version as Revision) - (n as Revision - 1);
    if first < 0 {
        return Err(Error::Backend(format!(
            "kurrentdb: append to {stream:?} COMMITTED {n} event(s) but the ack reports \
             stream revision {next_expected_version}, which places the batch before \
             revision 0, so the positions the server issued cannot be reported; the \
             events are durable in the log"
        )));
    }
    Ok(AckPlacement::ReadBackFrom(first))
}

impl Store {
    /// Write `events` to `stream` under `expected` exactly as handed, reporting the positions the
    /// server issued: the one write every append - and every group link - goes through.
    fn append_raw(
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
                let id = event_id(&e.id);
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
            // This backend recognises nothing as already recorded, so it APPENDS THROUGH: every handed event is written
            // and reported written, which is the fail-safe direction (it can only ever
            // write more, never drop). Only the HONESTY half of the port is owed here,
            // and it is owed in full: each reported position must be one the server
            // ISSUED for that event.
            //
            // A single-event append can take that from the write's own commit position,
            // but ONLY when the server actually issued one. The client renders an
            // absent position as a ZERO commit (its `NoPosition` case maps to the start
            // of the log), so a zero is the server saying it issued none - not a
            // location. Reporting it would hand the projection position 0, which its
            // applied ledger then marks applied forever, swallowing the genuine event
            // recorded there. So a zero is not answered here at all: it is READ BACK
            // from the revision the ack names, exactly as a batch's positions are. That
            // is also what keeps the first append to a fresh log - whose commit position
            // genuinely is 0 - from failing for having succeeded.
            //
            // A multi-event append cannot use the commit position at all: KurrentDB's
            // `$all` position is a byte offset, so the earlier events' positions are
            // not derivable from the last one by any arithmetic. They are READ BACK
            // from the stream - the batch occupies the `n` revisions ending at
            // `next_expected_version` - never invented. That arithmetic has the same
            // absent-value hazard: an ack carrying no stream revision renders as zero,
            // which for a batch computes a NEGATIVE first revision, and a negative
            // `from` is admitted by `read_forward`'s `revision >= from` test - so the
            // read-back would return the stream's FIRST n events and report another
            // append's placements as this one's. A first revision below zero is
            // therefore refused rather than read.
            Ok(w) => {
                match placement_of_ack(
                    stream,
                    events.len(),
                    w.position.commit,
                    w.next_expected_version,
                )? {
                    AckPlacement::Issued(position) => Ok(Appended::all(vec![position])),
                    AckPlacement::ReadBackFrom(first) => Ok(Appended::all(
                        self.read_back_positions(stream, first, events.len())?,
                    )),
                }
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
}

impl EventStore for Store {
    /// An append holding a grouped event gives every event its id ([`with_ids`]) and runs the
    /// group-link protocol ([`append_linked`]) around the write; any other is written as is.
    fn append(
        &self,
        stream: &str,
        expected: ExpectedRevision,
        events: &[Event],
    ) -> Result<Appended, Error> {
        if events.iter().any(|e| e.meta.contains_key(META_GROUP)) {
            let events = with_ids(events);
            append_linked(
                stream,
                expected,
                &events,
                || self.last_revision(stream),
                |group, revision, id| {
                    self.append_raw(
                        &group_stream(stream, group),
                        ExpectedRevision::Any,
                        &[link_event(stream, revision, id)],
                    )
                    .map(|_| ())
                },
                |pinned| self.append_raw(stream, pinned, &events),
            )
        } else {
            self.append_raw(stream, expected, events)
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

    /// The newest-first read ([`Store::newest_first`]) scanned for the first event of
    /// `event_type` ([`newest_of_type`]): the server has no index over event types, so the bound
    /// is that first match, and only the events after it are ever read.
    fn last_position(&self, stream: &str, event_type: &str) -> Result<Option<Revision>, Error> {
        let records = self.newest_first(stream).filter_map(|pulled| {
            pulled
                .map(|ev| original(&ev).map(|rec| (rec.event_type.clone(), rec.revision)))
                .transpose()
        });
        newest_of_type(records, event_type)
    }

    /// A `$all` read the SERVER filters on event type ([`typed_read_filter`]), so a type the
    /// selection refuses never leaves it, started at the `$all` position of the event at
    /// revision `from` ([`typed_read_start`]) so the read is anchored on that event; another
    /// stream's record is refused on its stream id before it is decoded ([`records_of`]).
    fn read_stream_typed(
        &self,
        stream: &str,
        from: Revision,
        selection: TypeSelection,
    ) -> Result<Vec<Event>, Error> {
        let anchor = || {
            Ok(self
                .read_forward(stream, from, 1)?
                .first()
                .map(|event| event.position))
        };
        let Some(start) = typed_read_start(from, anchor)? else {
            return Ok(Vec::new());
        };
        let mut rs = self
            .rt
            .block_on(self.client.read_all(&typed_read_options(start, selection)))
            .map_err(|e| Error::Backend(format!("kurrentdb: read typed: {e}")))?;
        let pulls =
            std::iter::from_fn(|| self.rt.block_on(rs.next()).transpose()).filter_map(|pulled| {
                pulled
                    .map(|ev| {
                        original(&ev)
                            .map(|rec| rec.stream_id().to_string())
                            .map(|id| (id, ev))
                    })
                    .transpose()
            });
        records_of(pulls, stream, |ev| {
            original(ev).and_then(|rec| to_event(rec, &Filter::default()))
        })
    }

    /// The forward read of the whole stream ([`Store::forward_records`]), each record reduced to
    /// the position the server issued for it as it is pulled: no event is built and no payload or
    /// metadata is decoded or handed on. The server's read protocol has no positions-only form, so
    /// each record's bytes still cross the wire; they are dropped as the record is read, never held
    /// past it. A stream the server does not know hands nothing.
    fn read_stream_positions(
        &self,
        stream: &str,
        batch: usize,
        sink: &mut dyn FnMut(&[Position]) -> Result<(), Error>,
    ) -> Result<(), Error> {
        let positions = self.forward_records(stream, 0).filter_map(|pulled| {
            pulled
                .map(|ev| original(&ev).map(|rec| rec.position.commit as Position))
                .transpose()
        });
        super::in_batches(positions, batch, sink)
    }

    /// The stream's head is its newest event's position, read newest-first and stopping at the
    /// first; then the forward read from `from` ([`Store::forward_events`]), each record pulled
    /// from the server only as the batch needs it, so no more than one batch of events is ever
    /// held.
    fn read_stream_batched(
        &self,
        stream: &str,
        from: Revision,
        batch: usize,
        sink: &mut EventBatchSink,
    ) -> Result<(), Error> {
        let head = first_match(
            self.newest_first(stream)
                .map(|pulled| pulled.map(|ev| original(&ev).map(|rec| rec.position.commit))),
            "stream head",
            |position| position,
        )?;
        let Some(head) = head else {
            return Ok(());
        };
        super::in_batches(self.forward_events(stream, from), batch, &mut |events| {
            sink(events, head as Position)
        })
    }
    /// The newest-first read ([`Store::newest_first`]) of the identity's group stream, each link
    /// paired with the event its revision holds, ending at the newest link that holds the event
    /// it reserved ([`newest_in_group`]); the project stream itself is never read.
    fn latest_in_group(&self, stream: &str, group: &str) -> Result<Option<GroupHead>, Error> {
        newest_in_group(
            self.newest_first(&group_stream(stream, group))
                .map(|pulled| {
                    pulled.map(|ev| {
                        (
                            ev.link
                                .as_ref()
                                .and_then(|link| linked_id(&link.custom_metadata)),
                            ev.event
                                .as_ref()
                                .and_then(|rec| to_event(rec, &Filter::default())),
                        )
                    })
                }),
        )
    }
}

/// The boundary scan over a newest-first read's `(event type, revision)` records: the revision
/// of the FIRST record of `event_type`, pulling nothing past it. A stream the server does not
/// know - at the open or mid-read - has no boundary; any other server failure is an error.
fn newest_of_type<I>(records: I, event_type: &str) -> Result<Option<Revision>, Error>
where
    I: IntoIterator<Item = Result<(String, u64), kurrentdb::Error>>,
{
    first_match(records, "last position", |(t, revision)| {
        (t == event_type).then_some(revision as Revision)
    })
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

/// The append ack is read WITHOUT a server, because that is the only way this can be
/// covered at all: every other test in this module needs a live KurrentDB in a container
/// and skips itself when there is none, so the honesty obligation would otherwise be
/// pinned by nothing on an ordinary run.
#[cfg(test)]
mod ack {
    use super::*;

    /// Holds each `(n, commit, next_expected_version, placed)` case: an append of `n` events
    /// whose ack carries that commit position and next revision is placed exactly as `placed`.
    fn assert_placements(cases: &[(usize, u64, u64, AckPlacement)]) {
        for (n, commit, next, placed) in cases {
            assert_eq!(
                &placement_of_ack("run", *n, *commit, *next).unwrap(),
                placed
            );
        }
    }

    crate::test_cases! {
        a_single_event_reports_the_position_the_server_issued:
            assert_placements(&[(1, 4096, 7, AckPlacement::Issued(4096))]);
        a_batch_reports_the_revision_span_the_ack_names: assert_placements(&[
            (3, 4096, 9, AckPlacement::ReadBackFrom(7)),
            // The whole stream, starting at its very first revision, is a legitimate span.
            (3, 4096, 2, AckPlacement::ReadBackFrom(0)),
        ]);
    }

    /// An absent position is ASKED ABOUT, never answered from here. A zero commit is
    /// ambiguous by construction - "the server said none" is spelled exactly like "the
    /// start of the log" - and the batch path already resolves that ambiguity by reading
    /// the stream back. The single-event path takes the SAME route rather than a
    /// judgement of its own, which is also what stops the very first append to a fresh
    /// log (whose commit position genuinely is 0) from failing for having succeeded.
    #[test]
    fn a_single_event_whose_ack_carries_no_position_is_read_back_not_answered_here() {
        assert!(
            matches!(
                placement_of_ack("run", 1, 0, 7),
                Ok(AckPlacement::ReadBackFrom(7))
            ),
            "the position is read back from the revision the ack names"
        );
        // Nothing here can be folded at a fabricated 0.
        assert!(!matches!(
            placement_of_ack("run", 1, 0, 7),
            Ok(AckPlacement::Issued(_))
        ));
        // The stream's very first revision is a legitimate span for one event too.
        assert!(matches!(
            placement_of_ack("run", 1, 0, 0),
            Ok(AckPlacement::ReadBackFrom(0))
        ));
    }

    #[test]
    fn a_batch_whose_ack_carries_no_revision_is_refused_not_read_back_from_a_negative() {
        // The "no stream" ack renders as revision 0; for a batch of 3 that arithmetic
        // yields -2, and a negative `from` is admitted by the stream read, which would
        // report the stream's FIRST three positions as this append's.
        let err = placement_of_ack("run", 3, 4096, 0).expect_err("a batch cannot end at 0");
        let message = err.to_string();
        assert!(
            message.contains("COMMITTED 3 event") && message.contains("durable"),
            "the refusal must say the write LANDED and only the reporting failed: {message}"
        );
    }
}

/// The server-side filter a typed read hands KurrentDB is pinned WITHOUT a server: the exact
/// pattern for each selection shape, since the server - never this adapter - applies it.
#[cfg(test)]
mod typed_filter {
    use super::*;

    crate::test_cases! {
        only_matches_exactly_the_named_types: assert_eq!(
            typed_read_filter(TypeSelection::Only(&["DecisionMade", "LessonLearned"])),
            "^(?:DecisionMade|LessonLearned)$"
        );
        except_refuses_the_named_types_and_every_system_type: assert_eq!(
            typed_read_filter(TypeSelection::Except(&["EdgeInferred", "DocLinkExtracted"])),
            "^(?!\\$)(?!(?:EdgeInferred|DocLinkExtracted)$)"
        );
    }

    /// A read from revision 0 (or below) starts at the log's start without reading any anchor;
    /// a later revision starts at its event's `$all` position, and a revision the stream never
    /// reached starts nowhere.
    #[test]
    fn a_typed_read_starts_at_the_log_start_or_at_its_anchor_event() {
        let unread = || -> Result<Option<Position>, Error> {
            panic!("a read from the stream's start reads no anchor")
        };
        assert_eq!(
            typed_read_start(0, unread).unwrap(),
            Some(StreamPosition::Start)
        );
        assert_eq!(
            typed_read_start(-1, unread).unwrap(),
            Some(StreamPosition::Start)
        );
        assert_eq!(
            typed_read_start(3, || Ok(Some(4096))).unwrap(),
            Some(StreamPosition::Position(KdbPosition {
                commit: 4096,
                prepare: 4096
            }))
        );
        assert_eq!(typed_read_start(3, || Ok(None)).unwrap(), None);
        assert!(typed_read_start(3, || Err(Error::Backend("down".into()))).is_err());
    }

    /// Only the named stream's records are decoded and kept, in pull order; another stream's
    /// record never reaches the decoder, and a failed pull ends the read in that failure.
    #[test]
    fn a_typed_read_decodes_only_the_named_streams_records() {
        let pull = |stream: &str, n: u8| -> Result<(String, u8), kurrentdb::Error> {
            Ok((stream.to_string(), n))
        };
        let decode = |n: &u8| -> Option<Event> {
            assert_ne!(*n, 9, "another stream's record reached the decoder");
            Some(Event::new("E", vec![*n]))
        };
        let kept = records_of(
            vec![
                pull("s", 1),
                pull("other", 9),
                pull("s", 2),
                pull("s-longer", 9),
            ],
            "s",
            decode,
        )
        .unwrap();
        assert_eq!(
            kept.iter().map(|e| e.data.clone()).collect::<Vec<_>>(),
            [vec![1], vec![2]]
        );
        let failed = records_of(
            vec![pull("s", 1), Err(kurrentdb::Error::ResourceNotFound)],
            "s",
            decode,
        );
        assert!(
            failed
                .unwrap_err()
                .to_string()
                .contains("kurrentdb: read typed"),
            "the failure names the typed read"
        );
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
            the_server_hands_a_typed_read_only_the_selected_types(&store);
            a_link_whose_revision_holds_another_event_is_skipped(&store);
        }));
        let _ = rt.block_on(container.rm());
        if let Err(e) = result {
            std::panic::resume_unwind(e);
        }
    }

    /// WHAT THE BACKEND HANDS OVER (spec 101): the `$all` read a typed read drives is filtered by
    /// the SERVER, so every record that leaves it - on any stream - is of a type the selection
    /// admits and never a system (`$`) type; the refused types never cross the wire, and the
    /// adapter then keeps only the named stream's share.
    fn the_server_hands_a_typed_read_only_the_selected_types(store: &Store) {
        let at = |t: &str| Event::new(t, b"{}".to_vec());
        store
            .append(
                "k-handed",
                ExpectedRevision::Any,
                &[at("KLesson"), at("KDerived"), at("KWork"), at("KDerived")],
            )
            .unwrap();
        store
            .append(
                "k-handed-sibling",
                ExpectedRevision::Any,
                &[at("KWork"), at("KDerived"), at("KLesson")],
            )
            .unwrap();
        let handed_over = |selection: TypeSelection| -> Vec<(String, String)> {
            store.rt.block_on(async {
                let mut rs = store
                    .client
                    .read_all(&typed_read_options(StreamPosition::Start, selection))
                    .await
                    .unwrap();
                let mut out = Vec::new();
                while let Some(ev) = rs.next().await.unwrap() {
                    let rec = original(&ev).unwrap();
                    if rec.stream_id().starts_with("k-handed") {
                        out.push((rec.stream_id().to_string(), rec.event_type.clone()));
                    }
                    assert!(
                        !rec.event_type.starts_with('$'),
                        "no system record leaves the server: {}",
                        rec.event_type
                    );
                }
                out
            })
        };
        let pairs = |v: &[(&str, &str)]| -> Vec<(String, String)> {
            v.iter()
                .map(|(s, t)| (s.to_string(), t.to_string()))
                .collect()
        };
        assert_eq!(
            handed_over(TypeSelection::Only(&["KLesson", "KWork"])),
            pairs(&[
                ("k-handed", "KLesson"),
                ("k-handed", "KWork"),
                ("k-handed-sibling", "KWork"),
                ("k-handed-sibling", "KLesson"),
            ]),
            "Only: the server hands over the named types alone"
        );
        let except = handed_over(TypeSelection::Except(&["KDerived"]));
        assert_eq!(
            except,
            pairs(&[
                ("k-handed", "KLesson"),
                ("k-handed", "KWork"),
                ("k-handed-sibling", "KWork"),
                ("k-handed-sibling", "KLesson"),
            ]),
            "Except: the server never hands over a refused type"
        );
        let kept: Vec<String> = store
            .read_stream_typed("k-handed", 1, TypeSelection::Except(&["KDerived"]))
            .unwrap()
            .into_iter()
            .map(|e| e.type_)
            .collect();
        assert_eq!(
            kept,
            ["KWork"],
            "the adapter keeps the named stream's share from the anchor event on"
        );
    }

    /// The records a backward read hands back, newest first, as `(event type, revision)`.
    fn newest_first<'a>(
        records: &'a [(&'a str, u64)],
    ) -> impl Iterator<Item = Result<(String, u64), kurrentdb::Error>> + 'a {
        records.iter().map(|(t, r)| Ok((t.to_string(), *r)))
    }

    /// Spec 101 criterion 1, the KurrentDB half: the boundary scan answers the FIRST match of the
    /// newest-first read and pulls nothing past it - a record after the match panics - so the
    /// server read it drives stops there instead of walking the stream.
    #[test]
    fn the_boundary_scan_answers_the_first_match_and_pulls_nothing_past_it() {
        let read = newest_first(&[("Work", 7), ("Work", 6), ("RunStarted", 5)]).chain(
            std::iter::from_fn(|| -> Option<Result<(String, u64), kurrentdb::Error>> {
                panic!("the scan pulled a record past the first match")
            }),
        );
        assert_eq!(newest_of_type(read, "RunStarted").unwrap(), Some(5));
        assert_eq!(
            newest_of_type(newest_first(&[("RunStarted", 9)]), "RunStarted").unwrap(),
            Some(9),
            "the newest record itself is a match"
        );
        assert_eq!(
            newest_of_type(
                newest_first(&[("Work", 4), ("RunStarted", 3), ("RunStarted", 0)]),
                "Work"
            )
            .unwrap(),
            Some(4),
            "any type answers its own newest revision"
        );
    }

    /// A stream that never recorded the type, an empty stream, and a stream the server does not
    /// know (at the open or mid-read) all have no boundary; any other server failure is an error
    /// naming the lookup, never a fabricated boundary.
    #[test]
    fn the_boundary_scan_answers_none_for_an_absent_type_or_stream_and_errors_on_a_failure() {
        assert_eq!(
            newest_of_type(newest_first(&[("Work", 1), ("Work", 0)]), "RunStarted").unwrap(),
            None
        );
        assert_eq!(
            newest_of_type(newest_first(&[]), "RunStarted").unwrap(),
            None
        );
        assert_eq!(
            newest_of_type([Err(kurrentdb::Error::ResourceNotFound)], "RunStarted").unwrap(),
            None
        );
        let mid_read_gone = newest_first(&[("Work", 3)])
            .chain([Err(kurrentdb::Error::ResourceNotFound)])
            .chain(newest_first(&[("RunStarted", 1)]));
        assert_eq!(newest_of_type(mid_read_gone, "RunStarted").unwrap(), None);
        let failed = newest_first(&[("Work", 3)])
            .chain([Err(kurrentdb::Error::AccessDenied)])
            .chain(newest_first(&[("RunStarted", 1)]));
        match newest_of_type(failed, "RunStarted") {
            Err(Error::Backend(msg)) => {
                assert_eq!(msg, "kurrentdb: last position: Access denied error")
            }
            other => panic!("a server failure must be a backend error, got {other:?}"),
        }
    }

    /// A store over a server that never answers.
    fn unreachable_store() -> Store {
        // One thread, not a worker per core: the test runner caps the address space.
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let settings = Store::client_settings(
            "kurrentdb://127.0.0.1:1?tls=false&maxDiscoverAttempts=1&discoveryInterval=10&gossipTimeout=200&defaultDeadline=2000",
        )
        .unwrap();
        let client = {
            let _guard = rt.enter();
            Client::new(settings).unwrap()
        };
        Store { client, rt }
    }

    /// `result` is the backend error an unreachable server surfaces, naming `step`.
    fn assert_names_step<T: std::fmt::Debug>(what: &str, result: Result<T, Error>, step: &str) {
        match result {
            Err(Error::Backend(msg)) => assert!(
                msg.starts_with(step),
                "{what}: the error names its step {step:?}: {msg}"
            ),
            other => panic!("{what}: an unreachable server must be a backend error, got {other:?}"),
        }
    }

    /// The adapter's lookup reaches the server and reports its failure: over a server that never
    /// answers it is an error naming the lookup, never a fabricated `None` or revision.
    #[test]
    fn the_boundary_lookup_reports_an_unreachable_server_as_an_error() {
        assert_names_step(
            "the boundary lookup",
            unreachable_store().last_position("run", "RunStarted"),
            "kurrentdb: last position: ",
        );
    }

    /// Spec 101: every group-lookup shell reaches the server through its own step. An append with
    /// no grouped event is the plain write; one holding a grouped event - wherever in the batch -
    /// first reads the stream's last revision; the group lookup reads the group stream. Each reports
    /// the unreachable server as an error naming that step, never a fabricated answer.
    #[test]
    fn each_group_shell_reports_an_unreachable_server_as_an_error_naming_its_step() {
        let store = unreachable_store();
        let plain = Event::new("X", Vec::new());
        let grouped = plain.clone().with_meta(META_GROUP, "gc/a.rs");
        assert_names_step(
            "an ungrouped append",
            store.append("s", ExpectedRevision::Any, &[plain.clone(), plain.clone()]),
            "kurrentdb: append: ",
        );
        assert_names_step(
            "a grouped append",
            store.append("s", ExpectedRevision::Any, &[plain, grouped]),
            "kurrentdb: last revision: ",
        );
        assert_names_step(
            "the group lookup",
            store.latest_in_group("s", "gc/a.rs"),
            "kurrentdb: latest in group: ",
        );
    }

    /// Spec 101, on the server: a group link whose revision holds an event other than the one it
    /// reserved - here the group's OLDER member - is skipped, so the lookup answers the newest
    /// member; the link record itself carries the reserved id through the server's resolution.
    fn a_link_whose_revision_holds_another_event_is_skipped(store: &Store) {
        let stream = "stale-link-probe";
        let member = |generation: &str| {
            Event::new("X", Vec::new())
                .with_meta(META_GROUP, "gc/a.rs")
                .with_meta("gen", generation)
        };
        store
            .append(stream, ExpectedRevision::Any, &[member("h3"), member("h4")])
            .unwrap();
        let newest = store.latest_in_group(stream, "gc/a.rs").unwrap().unwrap();
        assert_eq!(newest.meta.get("gen").map(String::as_str), Some("h4"));
        // A stalled writer's link: it reserved revision 0 for an event that was never written.
        store
            .append_raw(
                &group_stream(stream, "gc/a.rs"),
                ExpectedRevision::Any,
                &[link_event(stream, 0, &Uuid::new_v4().to_string())],
            )
            .unwrap();
        assert_eq!(
            store.latest_in_group(stream, "gc/a.rs").unwrap(),
            Some(newest),
            "the stale link resolves to the older h3 and is skipped"
        );
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

/// The group-link protocol and the group lookup's scan are pinned WITHOUT a server (spec 101):
/// which revision each group links, the order the three server calls run in, when a conflict is
/// retried and when it is the caller's, and which resolved link answers the lookup.
#[cfg(test)]
mod group_links {
    use super::*;
    use std::cell::RefCell;

    fn member(group: Option<&str>) -> Event {
        let event = Event::new("X", Vec::new());
        match group {
            Some(group) => event.with_meta(META_GROUP, group),
            None => event,
        }
    }

    /// `member` carrying the fixed id `id`.
    fn member_id(group: Option<&str>, id: &str) -> Event {
        let mut event = member(group);
        event.id = id.to_string();
        event
    }

    const A1: &str = "00000000-0000-0000-0000-0000000000a1";
    const A2: &str = "00000000-0000-0000-0000-0000000000a2";
    const B1: &str = "00000000-0000-0000-0000-0000000000b1";

    #[test]
    fn each_group_links_the_revision_and_id_of_its_newest_event_in_first_appearance_order() {
        let events = [
            member_id(Some("gc/b.rs"), B1),
            member_id(None, A1),
            member_id(Some("gc/a.rs"), A1),
            member_id(Some("gc/b.rs"), A2),
        ];
        assert_eq!(
            group_links(4, &events),
            [("gc/b.rs", 8, A2), ("gc/a.rs", 7, A1)],
            "revisions count on from the stream's last one; a group's later member moves its link \
             and the id it names"
        );
        assert_eq!(
            group_links(NO_STREAM, &events[..1]),
            [("gc/b.rs", 0, B1)],
            "a stream that does not exist starts at revision 0"
        );
        assert!(group_links(4, &[member(None)]).is_empty());
    }

    #[test]
    fn a_pinned_write_expects_exactly_the_revision_it_linked_against() {
        assert_eq!(pinned(NO_STREAM), ExpectedRevision::NoStream);
        assert_eq!(pinned(0), ExpectedRevision::Exact(0));
        assert_eq!(pinned(7), ExpectedRevision::Exact(7));
    }

    #[test]
    fn the_group_stream_and_the_link_name_the_event_they_resolve_to() {
        assert_eq!(
            group_stream("proj-x-rigger", "gc/a.rs"),
            "rigger-group/proj-x-rigger/gc/a.rs"
        );
        let link = link_event("proj-x-rigger", 12, A1);
        assert_eq!(link.type_, "$>");
        assert_eq!(link.data, b"12@proj-x-rigger".to_vec());
        assert_eq!(
            link.meta,
            BTreeMap::from([(META_LINKED_ID.to_string(), A1.to_string())]),
            "the link names the id of the event it reserves, and nothing else"
        );
        assert!(
            !delivered("rigger-group/s/gc/a.rs", &link.type_, &Filter::default()),
            "a link never reaches a caller, whatever stream holds it"
        );
    }

    #[test]
    fn a_link_records_its_reserved_id_in_the_envelope_the_lookup_reads_back() {
        let written = serde_json::to_vec(&Envelope {
            meta: link_event("s", 3, A1).meta,
            valid_from_nanos: 0,
        })
        .unwrap();
        assert_eq!(linked_id(&written), Some(A1.to_string()));
        assert_eq!(
            linked_id(b""),
            None,
            "a record with no envelope reserves nothing"
        );
        let unlinked = serde_json::to_vec(&Envelope::default()).unwrap();
        assert_eq!(linked_id(&unlinked), None, "an envelope naming no id");
    }

    #[test]
    fn every_event_of_a_grouped_append_carries_the_uuid_it_is_written_under() {
        assert_eq!(event_id(A1).to_string(), A1, "an event's own uuid is kept");
        assert_eq!(
            event_id("00000000-0000-0000-0000-0000000000A1").to_string(),
            A1,
            "and written in the one spelling the read decodes"
        );
        let (fresh, other) = (event_id(""), event_id("not-a-uuid"));
        assert_ne!(fresh, Uuid::nil(), "an event without one gets a fresh uuid");
        assert_ne!(fresh, other, "each fresh uuid is distinct");

        let handed = [member_id(Some("gc/a.rs"), A1), member(None)];
        let ided = with_ids(&handed);
        assert_eq!(ided.len(), 2);
        assert_eq!(ided[0].id, A1);
        assert_eq!(ided[0].meta, handed[0].meta);
        assert_eq!(
            Uuid::parse_str(&ided[1].id).map(|u| u.to_string()).ok(),
            Some(ided[1].id.clone())
        );
        assert_ne!(ided[1].id, Uuid::nil().to_string());
    }

    #[test]
    fn only_a_non_system_record_of_an_admitted_stream_is_delivered() {
        let all = Filter::default();
        let scoped = Filter {
            stream_prefix: Some("proj-a-".into()),
        };
        assert!(delivered("rigger", "X", &all));
        assert!(!delivered("$stats", "X", &all), "a system stream");
        assert!(!delivered("rigger", "$>", &all), "a system record");
        assert!(delivered("proj-a-rigger", "X", &scoped));
        assert!(
            !delivered("proj-b-rigger", "X", &scoped),
            "a stream outside the prefix"
        );
        assert!(!delivered("proj-a-rigger", "$metadata", &scoped));
    }

    /// A recorded event as the server hands it, in the plain view [`to_event`] decodes.
    fn record<'a>(stream: &'a str, event_type: &'a str, custom_metadata: &'a [u8]) -> Record<'a> {
        Record {
            stream,
            id: Uuid::parse_str(A1).unwrap(),
            event_type,
            data: b"{\"k\":1}",
            custom_metadata,
            created: SystemTime::UNIX_EPOCH + Duration::from_secs(5),
            commit: 42,
            revision: 3,
        }
    }

    #[test]
    fn a_delivered_record_decodes_to_the_event_it_carries_and_any_other_to_nothing() {
        let envelope = serde_json::to_vec(&Envelope {
            meta: BTreeMap::from([(META_GROUP.to_string(), "gc/a.rs".to_string())]),
            valid_from_nanos: 7_000,
        })
        .unwrap();
        let event = to_event(
            record("proj-a-rigger", "Work", &envelope),
            &Filter::default(),
        )
        .expect("an ordinary record is delivered");
        assert_eq!(
            (
                event.id.as_str(),
                event.stream.as_str(),
                event.type_.as_str(),
                event.data.as_slice(),
                event.position,
                event.revision
            ),
            (A1, "proj-a-rigger", "Work", b"{\"k\":1}".as_slice(), 42, 3)
        );
        assert_eq!(
            event.meta,
            BTreeMap::from([(META_GROUP.to_string(), "gc/a.rs".to_string())])
        );
        assert_eq!(event.valid_from, from_nanos(7_000));
        assert_eq!(
            event.recorded_at,
            SystemTime::UNIX_EPOCH + Duration::from_secs(5)
        );
        assert!(
            to_event(
                record("proj-a-rigger", "Work", b"not json"),
                &Filter::default()
            )
            .is_some_and(|e| e.meta.is_empty()),
            "an unreadable envelope decodes to no metadata, never to a dropped event"
        );
        let scoped = Filter {
            stream_prefix: Some("proj-a-".into()),
        };
        assert!(to_event(record("proj-a-rigger", "Work", b""), &scoped).is_some());
        for (stream, type_, filter) in [
            ("rigger-group/s/gc/a.rs", "$>", &Filter::default()),
            ("$stats", "Work", &Filter::default()),
            ("proj-b-rigger", "Work", &scoped),
        ] {
            assert!(
                to_event(record(stream, type_, b""), filter).is_none(),
                "{type_} on {stream} never reaches a caller"
            );
        }
    }

    /// The protocol's calls, in order, over a scripted server: `lasts` answers each read of the
    /// stream's last revision, `writes` each pinned write.
    fn drive(
        expected: ExpectedRevision,
        lasts: Vec<Revision>,
        writes: Vec<Result<Appended, Error>>,
    ) -> (Result<Appended, Error>, Vec<String>) {
        let calls = RefCell::new(Vec::new());
        let lasts = RefCell::new(lasts.into_iter());
        let writes = RefCell::new(writes.into_iter());
        let events = [
            member_id(Some("gc/a.rs"), B1),
            member_id(Some("gc/a.rs"), A1),
        ];
        let result = append_linked(
            "s",
            expected,
            &events,
            || {
                calls.borrow_mut().push("last".to_string());
                Ok(lasts.borrow_mut().next().expect("a scripted last revision"))
            },
            |group, revision, id| {
                assert_eq!(id, A1, "the link names the group's newest event");
                calls.borrow_mut().push(format!("link {group} {revision}"));
                Ok(())
            },
            |pinned| {
                calls.borrow_mut().push(format!("write {pinned:?}"));
                writes.borrow_mut().next().expect("a scripted write")
            },
        );
        (result, calls.into_inner())
    }

    fn conflict() -> Result<Appended, Error> {
        Err(Error::Conflict {
            stream: "s".into(),
            expected: ExpectedRevision::Exact(3),
            actual: 4,
        })
    }

    #[test]
    fn the_link_is_written_before_the_events_it_names() {
        let (result, calls) = drive(
            ExpectedRevision::Any,
            vec![3],
            vec![Ok(Appended::all(vec![9, 10]))],
        );
        assert_eq!(result.unwrap().last(), Some(10));
        assert_eq!(calls, ["last", "link gc/a.rs 5", "write Exact(3)"]);
    }

    #[test]
    fn an_any_append_that_loses_its_pinned_revision_re_reads_and_re_links() {
        let (result, calls) = drive(
            ExpectedRevision::Any,
            vec![3, 4],
            vec![conflict(), Ok(Appended::all(vec![11, 12]))],
        );
        assert_eq!(result.unwrap().last(), Some(12));
        assert_eq!(
            calls,
            [
                "last",
                "link gc/a.rs 5",
                "write Exact(3)",
                "last",
                "link gc/a.rs 6",
                "write Exact(4)"
            ]
        );
    }

    #[test]
    fn a_pinned_callers_conflict_is_the_callers_and_is_not_retried() {
        let (result, calls) = drive(ExpectedRevision::Exact(3), vec![3], vec![conflict()]);
        assert!(matches!(result, Err(Error::Conflict { actual: 4, .. })));
        assert_eq!(calls, ["last", "link gc/a.rs 5", "write Exact(3)"]);
    }

    #[test]
    fn an_expectation_the_stream_does_not_meet_is_refused_before_anything_is_linked() {
        for (expected, last) in [
            (ExpectedRevision::Exact(2), 3),
            (ExpectedRevision::NoStream, 0),
        ] {
            let (result, calls) = drive(expected, vec![last], Vec::new());
            match result {
                Err(Error::Conflict {
                    stream,
                    expected: named,
                    actual,
                }) => {
                    assert_eq!((stream.as_str(), named, actual), ("s", expected, last));
                }
                other => panic!("{expected:?} over {last} must conflict: {other:?}"),
            }
            assert_eq!(calls, ["last"], "nothing is linked or written");
        }
        let (result, calls) = drive(
            ExpectedRevision::NoStream,
            vec![NO_STREAM],
            vec![Ok(Appended::all(vec![1, 2]))],
        );
        assert!(result.is_ok());
        assert_eq!(calls, ["last", "link gc/a.rs 1", "write NoStream"]);
    }

    #[test]
    fn a_failed_revision_read_or_link_ends_the_append_before_the_write() {
        let failed = append_linked(
            "s",
            ExpectedRevision::Any,
            &[member(Some("gc/a.rs"))],
            || Err(Error::Backend("down".into())),
            |_, _, _| panic!("nothing is linked"),
            |_| panic!("nothing is written"),
        );
        assert!(matches!(failed, Err(Error::Backend(m)) if m == "down"));
        let failed = append_linked(
            "s",
            ExpectedRevision::Any,
            &[member(Some("gc/a.rs"))],
            || Ok(0),
            |_, _, _| Err(Error::Backend("link down".into())),
            |_| panic!("an unlinked recording is never written"),
        );
        assert!(matches!(failed, Err(Error::Backend(m)) if m == "link down"));
    }

    /// A group link as the lookup reads it: the id it reserves and the event its revision holds.
    fn link(
        linked: Option<&str>,
        holds: Option<(&str, u64)>,
    ) -> Result<(Option<String>, Option<Event>), kurrentdb::Error> {
        Ok((
            linked.map(str::to_string),
            holds.map(|(id, position)| {
                let mut event = member_id(Some("gc/a.rs"), id);
                event.position = position;
                event
            }),
        ))
    }

    #[test]
    fn the_lookup_answers_the_newest_link_whose_revision_holds_the_event_it_reserved() {
        let links = vec![
            link(Some(A1), None),           // a dangling link: its revision holds nothing
            link(Some(A1), Some((B1, 90))), // a stale link: its revision holds another write
            link(None, Some((A2, 80))),     // a record reserving no id
            link(Some(A2), Some((A2, 60))), // the answer
            Err(kurrentdb::Error::ResourceNotFound), // never pulled
        ];
        let head = newest_in_group(links).unwrap().unwrap();
        assert_eq!((head.position, head.type_.as_str()), (60, "X"));
        assert_eq!(
            head.meta.get(META_GROUP).map(String::as_str),
            Some("gc/a.rs")
        );
    }

    #[test]
    fn a_group_with_no_resolving_link_or_no_group_stream_has_no_member() {
        assert_eq!(newest_in_group(vec![link(Some(A1), None)]).unwrap(), None);
        assert_eq!(
            newest_in_group(vec![Err(kurrentdb::Error::ResourceNotFound)]).unwrap(),
            None
        );
        match newest_in_group(vec![
            link(Some(A1), None),
            Err(kurrentdb::Error::AccessDenied),
        ]) {
            Err(Error::Backend(msg)) => {
                assert_eq!(msg, "kurrentdb: latest in group: Access denied error")
            }
            other => panic!("a server failure must be a backend error, got {other:?}"),
        }
    }

    #[test]
    fn the_last_revision_is_the_newest_records_and_a_missing_stream_has_none() {
        let read = [Ok(None), Ok(Some(7)), Ok(Some(6))]
            .into_iter()
            .chain(std::iter::from_fn(
                || -> Option<Result<Option<u64>, kurrentdb::Error>> {
                    panic!("the scan pulled a record past the newest one")
                },
            ));
        assert_eq!(newest_revision(read).unwrap(), 7);
        assert_eq!(newest_revision([Ok(Some(0))]).unwrap(), 0);
        assert_eq!(newest_revision(std::iter::empty()).unwrap(), NO_STREAM);
        assert_eq!(
            newest_revision([Err(kurrentdb::Error::ResourceNotFound)]).unwrap(),
            NO_STREAM
        );
        match newest_revision([Err(kurrentdb::Error::AccessDenied)]) {
            Err(Error::Backend(msg)) => {
                assert_eq!(msg, "kurrentdb: last revision: Access denied error")
            }
            other => panic!("a server failure must be a backend error, got {other:?}"),
        }
    }

    /// An in-memory server running the group-link protocol: one project stream, and its group
    /// links as `(group, revision, reserved id)` in the order they were written.
    #[derive(Default)]
    struct Server {
        stream: RefCell<Vec<Event>>,
        links: RefCell<Vec<(String, Revision, String)>>,
    }

    impl Server {
        fn last(&self) -> Revision {
            self.stream.borrow().len() as Revision - 1
        }

        /// `events` appended under `expected` through [`append_linked`], `stall` running after
        /// each read of the stream's last revision - where a slow writer loses the race.
        fn append(
            &self,
            expected: ExpectedRevision,
            events: &[Event],
            mut stall: impl FnMut(),
        ) -> Result<Appended, Error> {
            let events = with_ids(events);
            append_linked(
                "s",
                expected,
                &events,
                || {
                    let last = self.last();
                    stall();
                    Ok(last)
                },
                |group, revision, id| {
                    self.links
                        .borrow_mut()
                        .push((group.to_string(), revision, id.to_string()));
                    Ok(())
                },
                |pinned| {
                    let last = self.last();
                    if !pinned.admits(last) {
                        return Err(Error::Conflict {
                            stream: "s".into(),
                            expected: pinned,
                            actual: last,
                        });
                    }
                    let mut stream = self.stream.borrow_mut();
                    let mut positions = Vec::new();
                    for event in &events {
                        let mut event = event.clone();
                        event.revision = stream.len() as Revision;
                        event.position = 100 + stream.len() as Position;
                        positions.push(event.position);
                        stream.push(event);
                    }
                    Ok(Appended::all(positions))
                },
            )
        }

        /// What the lookup answers for `group`: its links newest first, each resolved to the
        /// event its revision holds.
        fn latest(&self, group: &str) -> Option<GroupHead> {
            let stream = self.stream.borrow();
            let links = self.links.borrow();
            newest_in_group(
                links
                    .iter()
                    .rev()
                    .filter(|(linked, _, _)| linked == group)
                    .map(|(_, revision, id)| {
                        Ok((Some(id.clone()), stream.get(*revision as usize).cloned()))
                    }),
            )
            .unwrap()
        }
    }

    fn generation(generation: &str) -> Event {
        member(Some("gc/a.rs")).with_meta("gen", generation)
    }

    /// THE STALE LINKER (spec 101): P reads the stream's end and stalls; Q records `gc/a.rs` at
    /// h3 and R at h4; P then links the revision it read - which now holds Q's older member of the
    /// SAME group - and its pinned write conflicts, so P's link is the group's newest and dangles
    /// onto Q's event. The lookup must still answer R's h4: a link answers only when its revision
    /// holds the very event it reserved.
    #[test]
    fn a_stale_linker_whose_write_conflicts_never_answers_an_older_member_of_its_group() {
        let server = Server::default();
        server
            .append(ExpectedRevision::Any, &vec![member(None); 5], || {})
            .unwrap();
        let mut raced = false;
        let stalled = server.append(ExpectedRevision::Exact(4), &[generation("h2")], || {
            if !std::mem::replace(&mut raced, true) {
                for gen in ["h3", "h4"] {
                    server
                        .append(
                            ExpectedRevision::Any,
                            &[generation(gen), generation(gen)],
                            || {},
                        )
                        .unwrap();
                }
            }
        });
        assert!(
            matches!(stalled, Err(Error::Conflict { actual: 8, .. })),
            "the stalled writer's pinned write conflicts: {stalled:?}"
        );
        assert_eq!(
            server
                .links
                .borrow()
                .iter()
                .map(|(_, revision, _)| *revision)
                .collect::<Vec<_>>(),
            [6, 8, 5],
            "the stalled writer's link is the newest, and names Q's first event"
        );
        let head = server.latest("gc/a.rs").expect("the group has a member");
        assert_eq!(
            (head.position, head.meta.get("gen").map(String::as_str)),
            (108, Some("h4")),
            "the newest member is R's last event"
        );

        // The same writer expecting `Any` re-reads, re-links and lands; the lookup then answers it.
        server
            .append(ExpectedRevision::Any, &[generation("h2")], || {})
            .unwrap();
        let head = server.latest("gc/a.rs").unwrap();
        assert_eq!(
            (head.position, head.meta.get("gen").map(String::as_str)),
            (109, Some("h2"))
        );
    }
}
