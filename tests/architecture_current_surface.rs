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
//!
//! # The port-signature pin (spec 60)
//!
//! The two pins above compare the document against LITERAL tokens written into this file,
//! which is the right shape for a configuration surface whose names are chosen once. It is
//! the wrong shape for a PORT the crate asks external adapter authors to implement: the
//! document renders `EventStore` as as-built Rust, so its rendering can be falsified by any
//! change to the trait itself, and a literal token here would have to be updated by the
//! same hand that let the document go stale. The third pin below therefore compares the
//! document against THE SOURCE: it extracts `EventStore::append`'s return type from
//! `src/eventstore/mod.rs` and from the document's own snippet and requires them to be the
//! same text. Nothing in it is a copy of either side, so it stays true across future
//! changes to the port and fails RED the moment the front door and the code disagree
//! about the contract a consumer must implement.

use std::path::PathBuf;

/// The committed architecture document, resolved from the manifest dir so the test does
/// not depend on the process CWD (integration tests may run from anywhere).
fn architecture_text() -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("docs")
        .join("architecture.md");
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()))
}

/// The committed event-store port, resolved the same CWD-independent way.
fn eventstore_source() -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("src")
        .join("eventstore")
        .join("mod.rs");
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()))
}

/// The return type `EventStore::append` is declared with inside `text`, extracted rather
/// than matched: find the trait, then its `append` arm, then the arrow that closes the
/// arm's signature, and take everything up to the `;` that ends the declaration, with all
/// whitespace collapsed so a multi-line rendering and a one-line one compare equal.
///
/// The same extraction runs over the source and over the document's fenced snippet, which
/// is what makes the pin below a comparison of two independent renderings rather than a
/// comparison of either one against a literal in this file.
fn append_return_type(text: &str) -> String {
    let trait_at = text
        .find("pub trait EventStore")
        .expect("no `pub trait EventStore` declaration to read an append arm out of");
    let rest = &text[trait_at..];
    let append_at = rest
        .find("fn append(")
        .expect("`EventStore` declares no `fn append(` arm");
    let rest = &rest[append_at..];
    let arrow_at = rest
        .find("->")
        .expect("`EventStore::append` declares no return type");
    let rest = &rest[arrow_at + 2..];
    let end = rest
        .find(';')
        .expect("`EventStore::append`'s declaration is not terminated");
    rest[..end].split_whitespace().collect::<Vec<_>>().join(" ")
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
    // The grounder configuration surface (spec 57): `symbols` (the structural grounder) is
    // the unset DEFAULT - an unset `defaults.grounder` resolves to it and it ships in the
    // default build; `grep` and `nop` are the explicit, named-only opt-outs. The vector
    // engine (`turbovec`) and its composite mode (`hybrid`) were RETIRED, so the front-door
    // document must name `symbols` as the default. `main::select_grounder` /
    // `grounder::grounder_for` are the resolution authority. A regression to a retired-engine
    // default or a "grep (default)" inversion fails RED (the negative guard below is the
    // other half of this pin).
    (
        "the symbols-default grounder selection",
        "symbols (default)",
    ),
];

/// Phrasings that name a WRONG default grounder or a RETIRED engine, checked
/// case-insensitively. `symbols` is the unset default; `grep` and `nop` are the explicit,
/// named-only opt-outs; the vector engine `turbovec` and its composite mode `hybrid` were
/// RETIRED (spec 57) and have no place in a description of the system that exists. Each
/// phrasing INVERTS the real configuration surface, so none may appear in the front-door
/// document. This guard is the negative half of the grounder-default pin: it makes a
/// re-regression at §11 (the module map or ADR-0001 R4) - or a stray "grep (default)" claim
/// or a lingering retired-engine mention anywhere - fail RED even when a correct
/// "symbols (default)" claim survives elsewhere.
const WRONG_DEFAULT_OR_RETIRED_PHRASINGS: &[&str] = &[
    // grep is the explicit opt-out, never the default.
    "grep` (default)",
    "grep (default)",
    "grep default",
    // the retired vector engine and its composite mode (spec 57) - gone from the front door,
    // whether named as a default or merely offered as a live grounder choice.
    "turbovec",
    "hybrid",
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

#[test]
fn architecture_renders_the_event_store_port_the_source_declares() {
    let declared = append_return_type(&eventstore_source());
    let rendered = append_return_type(&architecture_text());

    assert_eq!(
        rendered, declared,
        "docs/architecture.md renders `EventStore` as AS-BUILT Rust and is where an external \
         adapter author reads the contract they must implement, so its `append` arm must be \
         the arm the source declares. The document renders `-> {rendered}`; \
         src/eventstore/mod.rs declares `-> {declared}`. Re-render the snippet's append arm \
         and the doc line above it to the contract the code actually ships."
    );
}

#[test]
fn architecture_names_no_retired_or_wrong_default_grounder() {
    let text = architecture_text().to_lowercase();

    let inversions: Vec<&str> = WRONG_DEFAULT_OR_RETIRED_PHRASINGS
        .iter()
        .copied()
        .filter(|phrasing| text.contains(phrasing))
        .collect();

    assert!(
        inversions.is_empty(),
        "docs/architecture.md must describe the grounder surface that EXISTS (spec 57): \
         `symbols` is the unset default (an unset `defaults.grounder` resolves to it and it \
         ships in the default build), and `grep` / `nop` are the explicit, named-only \
         opt-outs. The vector engine `turbovec` and its `hybrid` composite were RETIRED - they \
         are neither the default nor a live choice, so the front-door document must not name \
         them, and must never call `grep` the default. Every grounder enumeration (the seams \
         diagram, the seams table, the config example, the module map, and ADR-0001 R4) must \
         name `symbols` as the default. Inverting or retired phrasings still present in the \
         document: {inversions:#?}"
    );
}
