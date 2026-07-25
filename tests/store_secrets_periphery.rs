//! Spec 48, SECRETS DISCIPLINE - the PERIPHERY layer for the connection-string secret channel.
//!
//! This file complements `store_secrets.rs` (which drives the built binary end-to-end) with the
//! two boundary surfaces that a binary-driven test cannot reach directly:
//!
//!   * THE PUBLIC API of the single redaction authority, `rigger::eventstore::redact_conn`. It is a
//!     `pub fn` (an added public API item), so a downstream crate that embeds the harness can and
//!     will call it directly. Naming the fully-qualified path here forces the compiler to resolve it
//!     as a public library symbol (its reachability is proven by compilation), and the cases below
//!     hold it to the contract its own doc states - "redact the credential portion of EVERY URL in
//!     `s`" - at the API edges the in-crate unit tests do not exercise: a path `@` (not a query `@`),
//!     an authority with several `@`, the empty string, idempotence, and - critically - a message
//!     that names MORE THAN ONE server.
//!
//!   * THE PARSE-ERROR BRANCH of `rigger::eventstore::kurrentdb::Store::open`. `open` gained
//!     redaction on its error path (a changed public API). The binary-driven file exercises the
//!     CONNECT branch (a well-formed but unreachable address), where the conn string appears once.
//!     The PARSE branch is different and offline: a malformed connection string is rejected before
//!     any runtime or network, and its error embeds the conn string TWICE - once in the diagnostic
//!     `open` adds and once more in the backend parser's own message, which re-echoes the whole
//!     string. That double occurrence is the exact shape the redaction must survive, and no
//!     server-dependent test can reach it. This is the LIBRARY boundary, distinct from the courier
//!     path the binary test drives.
//!
//! Every case is deterministic and server-free, so the secret channel stays regression-locked on
//! every machine.

use rigger::eventstore::redact_conn;

/// A credentialed, well-formed address whose userinfo is the secret under test. `spy:hunter2` is the
/// credential that must NEVER survive any redaction, in either half.
const SECRET_USER: &str = "spy";
const SECRET_PASSWORD: &str = "hunter2";

/// Assert neither half of the `spy:hunter2` credential survives in `text`, naming the channel in the
/// failure so a leak points straight at the branch that let it through.
fn assert_no_credential(text: &str, why: &str) {
    assert!(
        !text.contains(SECRET_PASSWORD),
        "{why}: the password must NEVER reach an output path: {text}"
    );
    assert!(
        !text.contains(SECRET_USER),
        "{why}: the username must NEVER reach an output path: {text}"
    );
}

/// The public API resolves from an external crate and scrubs the userinfo while keeping the scheme,
/// host, and query - the reachability of `rigger::eventstore::redact_conn` is proven by this file
/// compiling, and its core contract is proven at the crate boundary (not only from inside the crate).
#[test]
fn redact_conn_is_a_public_symbol_that_scrubs_userinfo_but_keeps_scheme_host_and_query() {
    let out = redact_conn("kurrentdb://spy:hunter2@db.internal:2113?tls=true");
    assert_no_credential(&out, "public API basic scrub");
    assert_eq!(
        out, "kurrentdb://<redacted>@db.internal:2113?tls=true",
        "the scheme, host, port, and query print; only the credential is replaced by the marker"
    );
}

/// An `@` in the URL PATH (after the authority, before any query) is not a userinfo separator, so it
/// is left untouched. This is the path-position sibling of the in-crate query-`@` case, and it pins
/// that the authority boundary is the first `/` (path start), not merely the first `?`.
#[test]
fn redact_conn_leaves_an_at_sign_in_the_path_alone() {
    let conn = "kurrentdb://db.internal:2113/tenants/a@b/stream";
    assert_eq!(
        redact_conn(conn),
        conn,
        "an `@` past the authority (in the path) is not a credential and must not be touched"
    );
}

/// When the authority itself contains several `@`, the host begins at the LAST one, so everything
/// before it (the whole userinfo, embedded `@` and all) is scrubbed and the host survives. Pins the
/// `rfind`-based host boundary the doc promises.
#[test]
fn redact_conn_scrubs_the_whole_userinfo_when_the_authority_has_several_at_signs() {
    let out = redact_conn("kurrentdb://spy:hun@ter2@db.internal:2113");
    assert!(
        !out.contains("spy") && !out.contains("hun@ter2"),
        "the entire userinfo up to the last `@` must be scrubbed: {out}"
    );
    assert_eq!(
        out, "kurrentdb://<redacted>@db.internal:2113",
        "the host begins at the last `@` of the authority; everything before it is the credential"
    );
}

/// The empty string is a boundary input: no URL, nothing to scrub, no panic.
#[test]
fn redact_conn_on_the_empty_string_is_empty() {
    assert_eq!(redact_conn(""), "");
}

/// Redacting an already-redacted string is stable: the marker carries no `user:pass@`, so a second
/// pass is a no-op. A redaction that is not idempotent would corrupt a string scrubbed twice on its
/// way through two output layers.
#[test]
fn redact_conn_is_idempotent() {
    let once = redact_conn("kurrentdb://spy:hunter2@db.internal:2113?tls=true");
    let twice = redact_conn(&once);
    assert_eq!(
        once, twice,
        "a second redaction of an already-scrubbed string changes nothing"
    );
    assert_no_credential(&twice, "idempotent redaction");
}

/// THE MULTI-URL CONTRACT. `redact_conn`'s own doc promises it scrubs the credential of EVERY URL in
/// the string, and it is the SINGLE authority precisely so no site has to worry about how many times
/// a connection string appears in a message. A diagnostic that names two servers - or, far more
/// commonly, one server named twice (see the parse-error test below) - must leave NO credential
/// behind. Both userinfo halves must be gone.
#[test]
fn redact_conn_scrubs_every_url_when_a_message_names_more_than_one_server() {
    let out = redact_conn(
        "primary kurrentdb://spy:hunter2@db1.internal:2113 replica kurrentdb://mole:s3cr3t@db2.internal:2113 done",
    );
    assert_no_credential(&out, "first credentialed URL in a two-server message");
    assert!(
        !out.contains("mole") && !out.contains("s3cr3t"),
        "the SECOND URL's credential must be scrubbed too - redaction covers every URL: {out}"
    );
    assert!(
        out.contains("db1.internal") && out.contains("db2.internal"),
        "both hosts still name which servers the message concerns: {out}"
    );
}

/// THE MULTI-URL CONTRACT AT ITS MOST HOSTILE SHAPE: two credentialed URLs run together with NO
/// whitespace and NO path/query/fragment delimiter between them - a comma abuts the first host
/// directly against the next `scheme://`. This is exactly the shape a diagnostic that lists servers
/// (or the parse error's double-embed) can emit, and it defeats any redactor that leans on
/// whitespace to find URL boundaries: only bounding each authority at the NEXT `://` scrubs both
/// credentials. It pins the delimiter-free sibling of the whitespace-separated case above, so the
/// single-authority guarantee is proven independent of what - if anything - separates the URLs.
#[test]
fn redact_conn_scrubs_both_urls_when_they_abut_with_no_delimiter_between_them() {
    let out = redact_conn(
        "kurrentdb://spy:hunter2@db1.internal:2113,kurrentdb://mole:s3cr3t@db2.internal:2113",
    );
    assert_no_credential(&out, "first credential in a delimiter-free two-URL string");
    assert!(
        !out.contains("mole") && !out.contains("s3cr3t"),
        "the second URL's credential must be scrubbed even with no separator before its scheme: {out}"
    );
    assert_eq!(
        out, "kurrentdb://<redacted>@db1.internal:2113,kurrentdb://<redacted>@db2.internal:2113",
        "each URL's userinfo is replaced by the marker; the comma and both hosts print verbatim"
    );
}

/// THE PARSE-ERROR BOUNDARY of the KurrentDB adapter's public `open`. A malformed connection string
/// carrying a credential is rejected at PARSE - before any runtime or network - and the resulting
/// `Error::Backend` message embeds the connection string TWICE: once in the diagnostic `open` adds
/// and once more in the backend parser's own error, which re-echoes the whole string. Every
/// occurrence of the credential must be scrubbed: the security property is that neither half of the
/// userinfo ever reaches the error text, while the host still names which address failed. This
/// branch is unreachable from a server-driven test (which fails later, at connect, with the conn
/// embedded only once), so it is guarded here at the library boundary.
#[test]
fn open_with_a_malformed_credentialed_conn_never_leaks_the_credential_it_echoes() {
    // A non-numeric port makes the address malformed, so `open` fails at parse (offline) rather than
    // at connect; `spy:hunter2` is the credential that must not survive the error text.
    let text = match rigger::eventstore::kurrentdb::Store::open(
        "kurrentdb://spy:hunter2@127.0.0.1:notaport",
    ) {
        Ok(_) => panic!("a malformed connection string must not open a store"),
        Err(e) => e.to_string(),
    };
    assert_no_credential(&text, "kurrentdb::Store::open parse-error path");
    assert!(
        text.contains("<redacted>"),
        "the credential must be replaced by the deliberate redaction marker, not merely absent: {text}"
    );
    assert!(
        text.contains("127.0.0.1"),
        "the host must still name which address failed to parse - redaction removes only the \
         userinfo: {text}"
    );
}

/// A password with an UNENCODED reserved char (`/`, `?`, or `#`) is malformed per RFC 3986 - those
/// chars belong percent-encoded - yet real credentials routinely carry them and the raw string is
/// still handed to the parser verbatim. Such a char sits INSIDE the userinfo, BEFORE the credential's
/// terminating `@`, so a redactor that ends the authority at the first `/?#` truncates the slice
/// before the `@` and prints the whole `user:pass` verbatim. Spec 48 is absolute - userinfo NEVER
/// reaches an output path - and carves out no exception for a malformed conn (its sibling test
/// `open_with_a_malformed_credentialed_conn...` already treats a malformed credentialed conn as
/// exactly what redaction must survive), so each of these must scrub the whole credential while the
/// host still prints.
#[test]
fn redact_conn_scrubs_a_userinfo_that_carries_an_unencoded_reserved_char() {
    for raw in ['#', '/', '?'] {
        // `spy:s3cr<raw>et` is the credential; the raw char splits the password so neither `spy` nor
        // the `s3cr...` fragment may survive, and the exact form pins that ONLY the userinfo is gone.
        let conn = format!("kurrentdb://spy:s3cr{raw}et@db.internal:2113");
        let out = redact_conn(&conn);
        assert!(
            !out.contains("spy") && !out.contains("s3cr"),
            "a raw {raw:?} inside the password must not stop the scrub before the terminating `@`: {out}"
        );
        assert_eq!(
            out, "kurrentdb://<redacted>@db.internal:2113",
            "the whole userinfo is replaced by the marker; the host and port still print: {out}"
        );
    }
}

/// THE PARSE-ERROR BOUNDARY with a SPECIAL-CHAR credential. `spy:s3cr#et` carries a raw `#`, so the
/// connection string is rejected at PARSE (before any runtime or network) and the resulting error
/// double-embeds the whole string - once in the diagnostic `open` adds and once more in the backend
/// parser's own re-echo. This is the one-char sibling of the `:notaport` case above (same offline
/// parse-rejection class), reached on the REAL `kurrentdb::Store::open` path; the credential must
/// never survive either embed, while the host still names the address that failed to parse.
#[test]
fn open_with_a_special_char_credentialed_conn_never_leaks_it() {
    let text = match rigger::eventstore::kurrentdb::Store::open(
        "kurrentdb://spy:s3cr#et@db.internal:2113",
    ) {
        Ok(_) => panic!("a malformed connection string must not open a store"),
        Err(e) => e.to_string(),
    };
    assert!(
        !text.contains("spy") && !text.contains("s3cr"),
        "neither half of a special-char credential may survive the parse-error text: {text}"
    );
    assert!(
        text.contains("<redacted>"),
        "the credential must be replaced by the deliberate redaction marker, not merely absent: {text}"
    );
    assert!(
        text.contains("db.internal"),
        "the host must still name which address failed to parse: {text}"
    );
}
