//! The append-only, bi-temporal event store: an immutable log of facts the ledger
//! and context graph are projected from. `EventStore` is the port; `sqlite` is the
//! default adapter. The trait mirrors KurrentDB's primitives - per-stream append
//! with optimistic concurrency, a global $all order, per-stream revisions, and
//! catch-up subscriptions - so a backend swaps without changing the rest of Rigger.

pub mod namespace;
pub mod sqlite;

// The KurrentDB adapter is always compiled in (spec 47): the shared-store backend is
// a first-class product capability reachable in the default build via a runtime flag,
// never a recompile - so the module is not gated behind a cargo feature.
pub mod kurrentdb;

#[cfg(test)]
pub mod contract;

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Receiver;
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, SystemTime};

use thiserror::Error;

/// Position is an event's place in the global $all order: store-assigned and only
/// ever increasing. It is a single opaque ordering value: callers compare and
/// checkpoint it, never decompose it.
///
/// SQLite assigns it directly as the 1-based row position. KurrentDB's native
/// `$all` position is a `(commit, prepare)` pair; the adapter exposes the
/// **commit position** as this `Position` and reconstructs the pair as
/// `(commit, commit)` when resuming a read or subscription. That round-trips
/// faithfully because KurrentDB orders and seeks `$all` by commit position, and
/// every record Rigger writes is a single-event append whose own commit and
/// prepare positions coincide (the prepare half of a *start* position only
/// disambiguates records that share a commit, which Rigger never produces). A
/// position returned from `read_all`/`subscribe_all` therefore resolves back to
/// the same logical location when fed into the next resume.
pub type Position = u64;

/// Revision is an event's place within its own stream: 0-based, so the first event
/// in a stream is revision 0. An empty stream sits at [`NO_STREAM`].
pub type Revision = i64;

/// The revision of a stream that does not yet exist.
pub const NO_STREAM: Revision = -1;

/// Read direction over a stream or the global log.
#[derive(Clone, Copy, Debug)]
pub enum Direction {
    Forward,
    Backward,
}

/// The optimistic-concurrency expectation for [`EventStore::append`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExpectedRevision {
    /// No concurrency check.
    Any,
    /// The stream must not yet exist (its last revision is [`NO_STREAM`]).
    NoStream,
    /// The stream's current last revision must equal this exactly.
    Exact(Revision),
}

/// A read/subscription filter over the global log.
#[derive(Clone, Debug, Default)]
pub struct Filter {
    pub stream_prefix: Option<String>,
}

/// Event is a single immutable fact. Callers populate the input fields; the store
/// stamps `recorded_at`, `position`, and `revision` on append (and `stream` to the
/// target stream). `valid_from` is the bi-temporal valid-time - when the fact
/// became true - and defaults to the append time unless the caller sets it.
#[derive(Clone, Debug)]
pub struct Event {
    pub id: String,
    pub stream: String,
    pub type_: String,
    pub data: Vec<u8>,
    /// Causation, correlation, and actor metadata.
    pub meta: BTreeMap<String, String>,
    /// When the fact became true (caller-supplied; defaults to the append time).
    pub valid_from: SystemTime,
    /// When the store ingested it (store-stamped).
    pub recorded_at: SystemTime,
    pub position: Position,
    pub revision: Revision,
}

impl Event {
    /// A new event with a fresh id. The store stamps `stream`, `recorded_at`,
    /// `position`, and `revision` on append; `valid_from` defaults to now and may
    /// be overridden with [`Event::with_valid_from`].
    pub fn new(type_: impl Into<String>, data: Vec<u8>) -> Self {
        let now = SystemTime::now();
        Event {
            id: uuid::Uuid::new_v4().to_string(),
            stream: String::new(),
            type_: type_.into(),
            data,
            meta: BTreeMap::new(),
            valid_from: now,
            recorded_at: now,
            position: 0,
            revision: NO_STREAM,
        }
    }

    /// Builder: set a metadata entry (causation / correlation / actor).
    pub fn with_meta(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.meta.insert(key.into(), value.into());
        self
    }

    /// Builder: set the valid-from time (when the fact became true).
    pub fn with_valid_from(mut self, t: SystemTime) -> Self {
        self.valid_from = t;
        self
    }
}

#[derive(Debug, Error)]
pub enum Error {
    #[error("event store: concurrency conflict on stream {stream:?}: expected {expected:?}, actual revision {actual}")]
    Conflict {
        stream: String,
        expected: ExpectedRevision,
        actual: Revision,
    },
    #[error("event store: {0}")]
    Backend(String),
}

/// A catch-up subscription: it replays the existing events from a position, then
/// streams new ones live, until it is dropped. Adapters feed it from a background
/// thread; callers consume it with the recv methods and check [`Subscription::err`]
/// for a terminal error after the stream ends.
pub struct Subscription {
    rx: Receiver<Event>,
    err: Arc<Mutex<Option<String>>>,
    stop: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
}

impl Subscription {
    /// Build a subscription from a backend's event channel, its terminal-error
    /// cell, its stop flag, and the thread feeding it.
    pub fn new(
        rx: Receiver<Event>,
        err: Arc<Mutex<Option<String>>>,
        stop: Arc<AtomicBool>,
        handle: JoinHandle<()>,
    ) -> Self {
        Subscription {
            rx,
            err,
            stop,
            handle: Some(handle),
        }
    }

    /// Block for the next event, or None once the feeding thread has stopped.
    pub fn recv(&self) -> Option<Event> {
        self.rx.recv().ok()
    }

    /// Block up to `timeout` for the next event.
    pub fn recv_timeout(&self, timeout: Duration) -> Option<Event> {
        self.rx.recv_timeout(timeout).ok()
    }

    /// Take the next event if one is ready, without blocking.
    pub fn try_recv(&self) -> Option<Event> {
        self.rx.try_recv().ok()
    }

    /// The terminal error, if the feeding thread ended in one.
    pub fn err(&self) -> Option<String> {
        self.err.lock().unwrap().clone()
    }
}

impl Drop for Subscription {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

/// EventStore is the append-only, bi-temporal log port (KurrentDB-shaped).
/// Implementations are safe to share across threads.
///
/// # The `from` boundary convention
///
/// Every read and subscription takes a `from` cursor. The boundary is the same
/// for the read and the subscription that share a scope, so a catch-up
/// subscription and a read from the same `from` replay exactly the same set (no
/// dropped or duplicated boundary event):
///
/// - **Stream-scoped** ([`read_stream`](EventStore::read_stream),
///   [`subscribe_stream`](EventStore::subscribe_stream)): `from` is a per-stream
///   revision and is **inclusive**. `from == 0` includes revision 0 (the first
///   event); resuming from the revision of the last event you saw re-delivers
///   that event.
/// - **`$all`-scoped** ([`read_all`](EventStore::read_all),
///   [`subscribe_all`](EventStore::subscribe_all)): `from` is a global
///   [`Position`] and is **exclusive**. `from == 0` includes every event;
///   resuming from the position of the last event you processed delivers only
///   what came after it - the natural checkpoint shape (store the last handled
///   position, resume from it, never see it twice).
///
/// Adapters must honor this regardless of the backend's native boundary.
/// KurrentDB's `read_*`-from-a-position is inclusive while its
/// `subscribe_*`-from-a-position is exclusive; its adapter normalizes both onto
/// the convention above.
pub trait EventStore: Send + Sync {
    /// Append events to the end of a stream under an optimistic-concurrency
    /// expectation, returning the global position of the last event written. A
    /// failed expectation yields [`Error::Conflict`] carrying the stream's actual
    /// current revision.
    fn append(
        &self,
        stream: &str,
        expected: ExpectedRevision,
        events: &[Event],
    ) -> Result<Position, Error>;

    /// Read one stream's events from a per-stream revision (**inclusive**), in a
    /// direction. Backward reads return the same set as a forward read from
    /// `from`, reversed (direction controls order, not the boundary).
    fn read_stream(
        &self,
        stream: &str,
        from: Revision,
        dir: Direction,
    ) -> Result<Vec<Event>, Error>;

    /// Read the global log from a global position (**exclusive**), in a
    /// direction, filtered. Backward reads return the same set as a forward read
    /// from `from`, reversed (direction controls order, not the boundary).
    fn read_all(
        &self,
        from: Position,
        dir: Direction,
        filter: &Filter,
    ) -> Result<Vec<Event>, Error>;

    /// Open a catch-up subscription over the global log from a position
    /// (**exclusive**): it replays the matching events after `from` in order,
    /// then delivers new ones live.
    fn subscribe_all(&self, from: Position, filter: &Filter) -> Result<Subscription, Error>;

    /// Open a catch-up subscription over one stream from a revision
    /// (**inclusive**): it replays that stream's events from `from` onward, then
    /// delivers new ones live.
    fn subscribe_stream(&self, stream: &str, from: Revision) -> Result<Subscription, Error>;
}

/// The marker that replaces a redacted credential, so a scrubbed connection string reads as
/// deliberately redacted (a human sees the credentials were removed) rather than silently
/// mangled or merely absent.
const REDACTED: &str = "<redacted>";

/// Redact the credential (userinfo) portion of every URL in `s` (§48, secrets discipline). A
/// connection string is a SECRET wherever it appears: any error, log line, or status output that
/// would echo it must scrub the `user:password@` that sits between `scheme://` and the host. The
/// scheme and host still print (they name WHICH server, which is useful in an error), but the
/// userinfo is replaced by the [`REDACTED`] marker and never reaches an output path.
///
/// This is the SINGLE redaction authority: every site that would surface a connection string in
/// user-facing text passes through here, so a credential can never leak from one forgotten branch.
/// It operates purely on the message text and never on the connection string handed to the client,
/// so verbatim pass-through to the backend is untouched.
///
/// A string with no URL userinfo returns unchanged. Redaction is confined to the URL AUTHORITY
/// (`[userinfo@]host[:port]`, between `scheme://` and the path/query/fragment): an `@` in a path or
/// query - not a credential separator - is left alone, and a userinfo with an embedded `:` (a
/// `user:password` pair) is scrubbed whole (the host begins at the last `@` of the authority). A
/// `/`, `?`, or `#` normally ENDS the authority, but a password may carry one of those chars
/// unencoded (malformed per RFC 3986, yet handed to the parser verbatim), so such a char BEFORE the
/// credential's terminating `@` does not stop the scrub - the whole userinfo is still removed.
pub fn redact_conn(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(scheme_end) = rest.find("://") {
        // Everything up to and including the "://" separator prints verbatim (scheme is not secret).
        let after = scheme_end + "://".len();
        out.push_str(&rest[..after]);
        let tail = &rest[after..];
        // Bound the current URL at the NEXT `://` so one authority never runs into the following
        // scheme. This is load-bearing: without it, a message that names two servers back-to-back
        // with no `/`, `?`, or `#` between them (`...@h1 kurrentdb://u2:p2@h2`, or comma/paren-
        // delimited `...@h1,kurrentdb://u2:p2@h2`) lets the scan swallow the next scheme, so the
        // leftover `//user:pass@host` no longer begins with a `scheme://` and is never re-scanned -
        // leaking the second credential verbatim. Bounding here scrubs EVERY URL's userinfo whatever
        // the delimiter between URLs (one server named twice - the parse error's double-embed - as
        // well as many named once).
        let seg_end = tail.find("://").unwrap_or(tail.len());
        let seg = &tail[..seg_end];
        let auth_end = authority_end(seg);
        let authority = &seg[..auth_end];
        // Userinfo is separated from the host by the LAST `@` inside the authority (a bare `@` never
        // appears in a host, and userinfo cannot contain an unencoded `@`), so anything before it is
        // the credential and is replaced by the marker; the host and beyond print unchanged.
        match authority.rfind('@') {
            Some(at) => {
                out.push_str(REDACTED);
                out.push_str(&authority[at..]); // includes the '@' and the host
            }
            None => out.push_str(authority),
        }
        rest = &tail[auth_end..];
    }
    out.push_str(rest);
    out
}

/// The byte offset in `seg` (one URL's tail, already bounded so it never reaches the next URL's
/// `scheme://`) at which the AUTHORITY ends - i.e. where `host[:port]` gives way to the path, query,
/// or fragment. The authority is `[userinfo@]host[:port]`.
///
/// The authority normally ends at the first `/`, `?`, or `#`. The subtlety this function exists for:
/// a password may carry one of those chars UNENCODED, and such a char sits inside the userinfo,
/// BEFORE the credential's terminating `@`. Ending the authority at that first delimiter would slice
/// the credential off before its `@`, so [`redact_conn`]'s `rfind('@')` would find nothing and print
/// the `user:pass` verbatim - a leak. So when the first delimiter is NOT preceded by a completed
/// authority, the credential runs on to its terminating `@` and the authority ends only at the first
/// delimiter AFTER the host.
///
/// The genuine-path/query `@` case (an `@` that a caller legitimately put in a path or query, which
/// must NOT be scrubbed) is told apart by exactly this: it follows a delimiter that DID close a
/// well-formed `host[:port]`, so that authority already ended and the `@` is post-authority.
fn authority_end(seg: &str) -> usize {
    let Some(delim) = seg.find(['/', '?', '#']) else {
        return seg.len(); // no path/query/fragment: the whole segment is the authority
    };
    let head = &seg[..delim];
    // If the userinfo `@` already precedes the delimiter (the well-formed case), or the text before
    // the delimiter is a complete `host[:port]` (the delimiter genuinely starts the path/query/
    // fragment), the authority ends right at the delimiter and any later `@` is post-authority.
    if head.contains('@') || is_host_port(head) {
        return delim;
    }
    // Otherwise the delimiter is an unencoded reserved char INSIDE the userinfo. The credential runs
    // on to its terminating `@`; the authority then ends at the first delimiter after the host (the
    // host carries no `@`, so `rfind('@')` on the returned slice still isolates the whole userinfo).
    // If there is no such `@`, the pre-delimiter text was not a credential after all - fall back to
    // the delimiter, leaving the segment untouched.
    match seg[delim..].find('@') {
        Some(rel_at) => {
            let at = delim + rel_at;
            seg[at..]
                .find(['/', '?', '#'])
                .map_or(seg.len(), |rel| at + rel)
        }
        None => delim,
    }
}

/// True when `s` is a complete URL authority with NO userinfo: a bare `host`, or `host:port` with a
/// non-empty all-digit port. Used to tell a `/`, `?`, or `#` that genuinely closes the authority
/// (starting the path/query/fragment) from one buried unencoded inside a credential's password.
fn is_host_port(s: &str) -> bool {
    if s.is_empty() {
        return false;
    }
    match s.rsplit_once(':') {
        // `host:port` - a port must be present and all ASCII digits (a non-numeric "port" like the
        // `pass` in `user:pass` is what marks this as a credential, not a host).
        Some((host, port)) => {
            !host.is_empty() && !port.is_empty() && port.bytes().all(|b| b.is_ascii_digit())
        }
        // A bare reg-name / IP host with no port.
        None => true,
    }
}

#[cfg(test)]
mod redact_tests {
    use super::redact_conn;

    #[test]
    fn strips_user_and_password_but_keeps_scheme_host_and_query() {
        let redacted = redact_conn("kurrentdb://myuser:supersecret@db.internal:2113?tls=true");
        assert!(
            !redacted.contains("supersecret") && !redacted.contains("myuser"),
            "userinfo must never survive redaction: {redacted}"
        );
        assert_eq!(
            redacted, "kurrentdb://<redacted>@db.internal:2113?tls=true",
            "scheme, host, port, and query print; only the credential is scrubbed"
        );
    }

    #[test]
    fn strips_a_userinfo_with_no_password() {
        assert_eq!(
            redact_conn("kurrentdb://alice@db.internal:2113"),
            "kurrentdb://<redacted>@db.internal:2113"
        );
    }

    #[test]
    fn leaves_a_conn_with_no_userinfo_unchanged() {
        let conn = "kurrentdb://127.0.0.1:2113?tls=false";
        assert_eq!(
            redact_conn(conn),
            conn,
            "no credential means nothing to scrub"
        );
    }

    #[test]
    fn an_at_sign_in_the_query_is_not_a_credential() {
        // The `@` sits in the query, not the authority, so it is NOT a userinfo separator.
        let conn = "kurrentdb://db.internal:2113?user=a@b";
        assert_eq!(redact_conn(conn), conn);
    }

    #[test]
    fn redacts_a_conn_embedded_in_a_longer_error_message() {
        let msg = "kurrentdb: connect to kurrentdb://joe:pw@10.0.0.5:2113?tls=false: timed out";
        let redacted = redact_conn(msg);
        assert!(
            !redacted.contains("joe") && !redacted.contains("pw@"),
            "the credential inside a wrapping message must be scrubbed: {redacted}"
        );
        assert!(
            redacted.contains("10.0.0.5:2113") && redacted.contains("timed out"),
            "the host and the surrounding message text still print: {redacted}"
        );
    }

    #[test]
    fn plain_text_with_no_url_is_untouched() {
        assert_eq!(
            redact_conn("no rigger store found"),
            "no rigger store found"
        );
    }
}
