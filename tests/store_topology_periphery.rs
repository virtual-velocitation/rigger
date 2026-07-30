//! Spec 48, criterion 4 - NO TOPOLOGY OPINIONS - the PERIPHERY layer for the KurrentDB adapter's
//! PUBLIC `Store::open` boundary.
//!
//! The criterion OWNS pass-through: the connection string reaches the server client VERBATIM, and
//! nothing in the shipping adapter may assume a local container, a localhost address, or an insecure
//! connection. The verbatim survival of each individual FIELD (host, port, TLS, credentials) is
//! proven where it is actually OBSERVABLE:
//!
//!   * white-box, against the parsed `ClientSettings` the client receives, by the `client_settings_*`
//!     tests in `src/eventstore/kurrentdb.rs` - the correct layer, since the settings ARE what the
//!     client is handed; and
//!   * end-to-end against a REAL backend on a NON-DEFAULT mapped port by the contract test
//!     `passes_the_contract` - a default-port or localhost rewrite would make it dial the wrong
//!     address and fail (it gracefully skips with no container runtime).
//!
//! The public `Store::open` boundary exposes no getter for the client's parsed topology, and its
//! error text merely re-echoes the INPUT string - so an assertion that a field appears there would
//! prove nothing about what the client parsed (a false witness). This file therefore does NOT lean on
//! that echo. It pins, robustly and OFFLINE, the one boundary property no other test covers: the
//! adapter ACCEPTS the shape a centrally hosted deployment needs - a TLS-secured, credentialed,
//! non-default-port address - and PROCEEDS to dial it, rather than rejecting or choking on anything
//! but a localhost/insecure address. A well-formed but unreachable address is refused at the network,
//! so `open` reaches its CONNECT stage rather than its PARSE stage; the two error paths are worded
//! distinctly, and telling them apart is exactly what proves the address was accepted, not rejected.
//! And it pins the credential scrub on that CONNECT path at the LIBRARY boundary - a path the sibling
//! `store_secrets_periphery.rs` reaches only at PARSE, and `store_secrets.rs` only through the binary
//! with TLS off. No server is needed, so the guard holds on every machine.

use rigger::eventstore::kurrentdb::Store;

/// A TLS-secured, credentialed address on a NON-DEFAULT port, at a loopback host with nothing
/// listening: the exact SHAPE a centrally hosted, secured deployment configures - TLS on, real
/// credentials, an operator-chosen port that is not the 2113 default - yet unreachable, so the
/// connection is refused IMMEDIATELY and the test needs no server. `spy:hunter2` is the credential
/// that must never reach any output path.
const TLS_CREDENTIALED_UNREACHABLE: &str = "kurrentdb://spy:hunter2@127.0.0.1:65533?tls=true";
/// The username half of the credential in [`TLS_CREDENTIALED_UNREACHABLE`].
const SECRET_USER: &str = "spy";
/// The password half of the credential in [`TLS_CREDENTIALED_UNREACHABLE`].
const SECRET_PASSWORD: &str = "hunter2";

/// The public `Store::open` ACCEPTS a TLS-secured, credentialed, non-default-port address and dials
/// it - it injects no localhost/insecure-only assumption that would refuse the address before ever
/// connecting. The address is unreachable, so `open` fails at the NETWORK (its CONNECT stage), never
/// at PARSE: the connect wording is present and the parse wording is absent, which together prove the
/// shape was accepted. The credential is scrubbed on that connect path, at the library boundary.
#[test]
fn open_accepts_a_tls_credentialed_address_and_reaches_connect_never_rejecting_the_shape() {
    let err = match Store::open(TLS_CREDENTIALED_UNREACHABLE) {
        Ok(_) => panic!("an unreachable server must not open a store"),
        Err(e) => e.to_string(),
    };

    // ACCEPTED, NOT REJECTED. A TLS-on, credentialed, non-default-port address parses cleanly and
    // `open` proceeds to the CONNECT stage - it fails at the NETWORK, not by rejecting the address
    // shape at parse. `open`'s two error paths are worded distinctly ("connect to ..." for the dial
    // vs "parse connection string ..." for a malformed address), so the PRESENCE of the connect
    // wording together with the ABSENCE of the parse wording prove the adapter injected no
    // localhost/insecure-only assumption that would have refused this shape before dialing it.
    assert!(
        err.contains("connect"),
        "open must reach the CONNECT stage for a well-formed TLS+credentialed address - it fails at \
         the network, not by rejecting the address shape: {err}"
    );
    assert!(
        !err.contains("parse connection string"),
        "a TLS-secured, credentialed, non-default-port address must NOT be rejected at parse - the \
         adapter assumes no localhost-only or insecure-only shape: {err}"
    );
    // DISPATCH TO THE SERVER. The failure arises inside the KurrentDB backend, proving `open` acted
    // as the server adapter and dialed the address rather than short-circuiting to another store.
    assert!(
        err.contains("kurrentdb"),
        "the failure must arise inside the server backend, proving open dialed the server: {err}"
    );

    // SECRETS DISCIPLINE ON THE CONNECT PATH, at the LIBRARY boundary. The sibling
    // `store_secrets_periphery.rs` pins redaction on the PARSE path of this same public `open`;
    // `store_secrets.rs` pins the CONNECT path only through the built binary and only with TLS off.
    // This is the CONNECT path of the public `Store::open` itself, TLS on: neither half of the
    // credential may survive, and the marker shows it was deliberately scrubbed, not merely absent.
    assert!(
        !err.contains(SECRET_PASSWORD) && !err.contains(SECRET_USER),
        "the credential must never reach the connect-error text: {err}"
    );
    assert!(
        err.contains("<redacted>"),
        "the credential must be replaced by the deliberate redaction marker, not merely absent: {err}"
    );
}
