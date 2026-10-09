//! The append-only, bi-temporal event store: an immutable log of facts the ledger
//! and context graph are projected from. `EventStore` is the port; `sqlite` is the
//! default adapter. The trait mirrors KurrentDB's primitives - per-stream append
//! with optimistic concurrency, a global $all order, per-stream revisions, and
//! catch-up subscriptions - so a backend swaps without changing the rest of Rigger.

pub mod namespace;
// THE CORE LANE IS PURE (spec 93, criterion 1): `EventStore` (below) and `Event`/
// `Position`/`Direction`/`ExpectedRevision`/`Error`/`Appended` are the PORT and stay
// ungated - `contextgraph` and every other `core` module needs them. `sqlite` and
// `kurrentdb` are the ADAPTERS (the concrete, I/O-touching implementations Design names
// explicitly): gated behind `any(feature = "store", not(feature = "core"))`, same
// predicate as every other store-gated module (see `lib.rs`'s own doc).
#[cfg(any(feature = "store", not(feature = "core")))]
pub mod sqlite;

// The KurrentDB adapter is always compiled in on every NATIVE lane (spec 47): the
// shared-store backend is a first-class product capability reachable in the default
// build via a runtime flag, never a recompile. It is still excluded from the `core`
// lane (an adapter, not the port) and from the `wasm32` target (its `tokio`/`kurrentdb`
// dependencies are target-gated out in Cargo.toml regardless).
#[cfg(any(feature = "store", not(feature = "core")))]
pub mod kurrentdb;

#[cfg(all(test, any(feature = "store", not(feature = "core"))))]
pub mod contract;

pub use rigger_domain::eventstore::*;

/// Hand `sink` what `items` yields, in order, in batches of at most `batch`, holding no more than
/// one batch at a time: the one batching of an adapter's [`EventStore::read_stream_positions`] and
/// [`EventStore::read_stream_batched`], and of the sqlite store's live-selection reads. The first
/// error - the backend's or the sink's - ends the read with that error.
#[cfg(any(feature = "store", not(feature = "core")))]
fn in_batches<T>(
    items: impl Iterator<Item = Result<T, Error>>,
    batch: usize,
    sink: &mut dyn FnMut(&[T]) -> Result<(), Error>,
) -> Result<(), Error> {
    let mut held = Vec::with_capacity(batch);
    for item in items {
        held.push(item?);
        if held.len() == batch {
            sink(&held)?;
            held.clear();
        }
    }
    if held.is_empty() {
        Ok(())
    } else {
        sink(&held)
    }
}

/// THE STORE REFUSES A DERIVED APPEND (spec 107): the one refusal both adapters' `append` open
/// with, before anything is written. A batch holding an event whose type
/// [`crate::retention::class_of`] answers derived is refused whole, naming the first such type
/// in batch order; any other batch, the empty one included, passes.
#[cfg(any(feature = "store", not(feature = "core")))]
fn refuse_derived(events: &[Event]) -> Result<(), Error> {
    use crate::retention::{class_of, Class};
    match events.iter().find(|e| class_of(&e.type_) == Class::Derived) {
        Some(derived) => Err(Error::DerivedAppend {
            type_: derived.type_.clone(),
        }),
        None => Ok(()),
    }
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

/// Reduce a connection string to a CREDENTIAL-FREE endpoint label safe to PERSIST and print: keep
/// the scheme and `host[:port]`, DROP any `[userinfo@]` credential AND any `/path`, `?query`, or
/// `#fragment` (a connection string can smuggle a credential in the userinfo OR the query). So
/// `kurrentdb://admin:secret@db.example:2113?tls=true` reduces to `kurrentdb://db.example:2113`.
///
/// This is the discovery-metadata sibling of [`redact_conn`] and shares its ONE hardened authority
/// parse ([`authority_end`]/[`is_host_port`]) - it is NOT a second redaction authority. Where
/// [`redact_conn`] masks the userinfo IN PLACE for a human-facing message (keeping the host and the
/// query), `endpoint_label` STRIPS the userinfo (and the query/path/fragment) entirely, for a value
/// that is written to disk (the instance registry's credential-free store identity, §50).
///
/// Routing through [`authority_end`] is load-bearing for the secrets invariant: a password may carry
/// an unencoded `/`, `?`, or `#` BEFORE its terminating `@` (malformed per RFC 3986, yet handed to
/// the parser verbatim). A naive parse that ends the authority at that first delimiter would slice
/// the credential off before its `@`, so `rfind('@')` would find nothing and the `user:pass` head
/// would land in the persisted label - a leak. The shared parse runs the credential on to its
/// terminating `@` instead, so `kurrentdb://user:pa/ss@host:2113` reduces to `kurrentdb://host:2113`,
/// never `kurrentdb://user:pa`. Pure, so the "the registry never holds a credential" invariant is
/// unit-tested with no store.
pub fn endpoint_label(conn: &str) -> String {
    let conn = conn.trim();
    // Split off the scheme, preserving it in the output (it names the backend). A conn with no
    // `scheme://` is treated as a bare authority so a malformed `user:pass@host` is still scrubbed.
    let (scheme, tail) = match conn.find("://") {
        Some(i) => (&conn[..i + "://".len()], &conn[i + "://".len()..]),
        None => ("", conn),
    };
    // The AUTHORITY, bounded by the shared hardened parse (which keeps a credential's unencoded
    // delimiter on the credential side of its terminating `@`), then with any `[userinfo@]` dropped:
    // the host begins at the LAST `@` of the authority (userinfo cannot contain an unencoded `@`).
    let authority = &tail[..authority_end(tail)];
    let host_port = match authority.rfind('@') {
        Some(at) => &authority[at + 1..],
        None => authority,
    };
    format!("{scheme}{host_port}")
}

#[cfg(test)]
mod endpoint_label_tests {
    use super::endpoint_label;

    crate::test_cases! {
        /// Userinfo AND query are stripped; only scheme+host:port survives.
        strips_userinfo_and_query_keeps_scheme_host_port: assert_eq!(
            endpoint_label("kurrentdb://admin:secret@db.example:2113?tls=true"),
            "kurrentdb://db.example:2113"
        );
        strips_a_bare_user_with_no_password: assert_eq!(
            endpoint_label("esdb+discover://user@cluster.internal:2113"),
            "esdb+discover://cluster.internal:2113"
        );
        an_already_credential_free_endpoint_is_unchanged: assert_eq!(
            endpoint_label("kurrentdb://db.example:2113"),
            "kurrentdb://db.example:2113"
        );
        /// A `?user=&password=` query is dropped with the path.
        a_credential_smuggled_after_the_path_is_dropped_with_the_path: assert_eq!(
            endpoint_label("kurrentdb://host:2113/stream?user=u&password=p"),
            "kurrentdb://host:2113"
        );
    }

    /// The GROUND-1 regression: a password carrying an unencoded delimiter BEFORE its terminating
    /// `@`. A naive parse ends the authority at the first `/`, so the `@` (and thus the host) is
    /// lost and the `user:pa` HEAD of the credential is what gets persisted - a leak. The shared
    /// hardened `authority_end` runs the credential on to its `@`, so the persisted label is the
    /// pure host and NO credential fragment survives.
    #[test]
    fn a_delimiter_inside_the_userinfo_never_leaks_the_credential_head() {
        assert_eq!(
            endpoint_label("kurrentdb://user:pa/ss@host:2113"),
            "kurrentdb://host:2113",
            "an unencoded `/` inside the password must not slice the authority before the `@`"
        );
        assert_eq!(
            endpoint_label("kurrentdb://user:pa?ss@host:2113"),
            "kurrentdb://host:2113",
            "an unencoded `?` inside the password is handled the same way"
        );
        // A no-scheme, malformed conn that still hides a credential is scrubbed to the bare host.
        assert_eq!(
            endpoint_label("user:pa/ss@host:2113"),
            "host:2113",
            "no scheme is no excuse to leak: a bare `user:pass@host` is still stripped"
        );
    }

    /// The persisted label must NEVER contain a credential fragment, whatever the (malformed) shape.
    #[test]
    fn no_credential_fragment_ever_survives() {
        for conn in [
            "kurrentdb://admin:hunter2@db.example:2113?tls=true",
            "kurrentdb://admin:hun/ter2@db.example:2113",
            "esdb+discover://admin@cluster.internal:2113",
            "kurrentdb://host:2113/s?user=admin&password=hunter2",
        ] {
            let label = endpoint_label(conn);
            assert!(
                !label.contains("admin") && !label.contains("hunter2") && !label.contains("hun"),
                "no credential fragment may reach the persisted label for {conn:?}; got {label}"
            );
        }
    }
}

#[cfg(test)]
mod redact_tests {
    use super::redact_conn;

    crate::test_cases! {
        /// Scheme, host, port, and query print; only the credential (user AND password) is
        /// scrubbed - the exact output carries neither half.
        strips_user_and_password_but_keeps_scheme_host_and_query: assert_eq!(
            redact_conn("kurrentdb://myuser:supersecret@db.internal:2113?tls=true"),
            "kurrentdb://<redacted>@db.internal:2113?tls=true"
        );
        strips_a_userinfo_with_no_password: assert_eq!(
            redact_conn("kurrentdb://alice@db.internal:2113"),
            "kurrentdb://<redacted>@db.internal:2113"
        );
        /// No credential means nothing to scrub.
        leaves_a_conn_with_no_userinfo_unchanged: assert_eq!(
            redact_conn("kurrentdb://127.0.0.1:2113?tls=false"),
            "kurrentdb://127.0.0.1:2113?tls=false"
        );
        /// The `@` sits in the query, not the authority, so it is NOT a userinfo separator.
        an_at_sign_in_the_query_is_not_a_credential: assert_eq!(
            redact_conn("kurrentdb://db.internal:2113?user=a@b"),
            "kurrentdb://db.internal:2113?user=a@b"
        );
        plain_text_with_no_url_is_untouched: assert_eq!(
            redact_conn("no rigger store found"),
            "no rigger store found"
        );
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
}

/// The append-only double the store halves' tests share, defined once with the fixtures.
#[cfg(all(test, any(feature = "store", not(feature = "core"))))]
pub(crate) use crate::test_support::SilentStore;
