//! Spec 56, criterion 1 - CURRENT-SURFACE ACCURACY of the front-door architecture
//! document.
//!
//! `docs/architecture.md` is the top of the documentation tree: a newcomer reading it
//! must learn the system THAT EXISTS. Two surfaces drifted the furthest from the code
//! and are the ones this criterion pins to their real names:
//!
//!   1. The event store is now pure CONFIGURATION behind one resolution authority
//!      (`main::store_selection` / `store_selection_at`, the §48 precedence chain). The
//!      backend is chosen by the committed project `store:` selection
//!      (`config::StoreConfig`, whose `store.backend` field the resolver reads), and its
//!      credentials ride one of the higher-precedence channels: the `KURRENTDB_CONN`
//!      environment variable or the per-machine `.rigger/store.conn` secret file. The
//!      document must name that store-resolution and configuration surface.
//!
//!   2. The knowledge-graph inspector is a parameterized `/api/graph` view with three
//!      lenses (`dash::Lens`: `lens=concepts` / `lens=code` / `lens=files`, an
//!      abstraction ladder) and directed call queries (`view=calls` with `dir=down` for
//!      the execution path and `dir=up` for the call sites - `dash::CallDir`). The
//!      document must name that real query surface.
//!
//! This test reads the committed `docs/architecture.md` (resolved from
//! `CARGO_MANIFEST_DIR`, so it does not depend on the process CWD) and asserts every
//! current-surface token is present. It is deliberately NOT feature-gated: it parses a
//! text file and touches no backend symbol, so it runs identically in both feature lanes.
//!
//! It OWNS the present-tense accuracy pins (spec 56 criterion 1). Staleness (criterion 2),
//! document integrity (criterion 3), and both-lanes-green (criterion 4) are pinned by
//! their own tests and are not this test's concern.

use std::path::PathBuf;

/// The committed architecture document, resolved from the manifest dir so the test does
/// not depend on the process CWD (integration tests may run from anywhere).
fn architecture_text() -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("docs")
        .join("architecture.md");
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()))
}

/// Every (surface, token) the front-door document must name to describe the system as it
/// exists today. Each token is a literal string the current code actually uses, so the
/// pin fails loudly if the document ever describes a shape the code does not have.
const CURRENT_SURFACE_TOKENS: &[(&str, &str)] = &[
    // The store-resolution and configuration surface (§48, "one resolution authority").
    (
        "the committed project store selection (config::StoreConfig)",
        "store.backend",
    ),
    (
        "the KURRENTDB_CONN environment credential channel",
        "KURRENTDB_CONN",
    ),
    (
        "the per-machine secret-file credential channel",
        ".rigger/store.conn",
    ),
    // The knowledge-graph inspector's real query surface (the /api/graph three-lens ladder
    // and the directed call queries).
    ("the concepts lens", "lens=concepts"),
    ("the code lens", "lens=code"),
    ("the files lens", "lens=files"),
    ("the directed call-query view", "view=calls"),
    ("the execution-path (forward) call direction", "dir=down"),
    ("the call-sites (reverse) call direction", "dir=up"),
];

#[test]
fn architecture_names_the_current_store_and_inspector_surface() {
    let text = architecture_text();

    let missing: Vec<String> = CURRENT_SURFACE_TOKENS
        .iter()
        .filter(|(_, token)| !text.contains(token))
        .map(|(surface, token)| format!("{surface}  (missing token: {token:?})"))
        .collect();

    assert!(
        missing.is_empty(),
        "docs/architecture.md must describe the system that exists today (spec 56, \
         criterion 1): it must name the store-resolution and configuration surface (the \
         committed `store:` selection, the `KURRENTDB_CONN` environment variable, and the \
         per-machine `.rigger/store.conn` secret file) and the graph inspector's real query \
         surface (the three `lens=` names and the directed `view=calls` `dir=` views). \
         Surfaces the document fails to name: {missing:#?}"
    );
}
