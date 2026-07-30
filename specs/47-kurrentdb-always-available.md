# 47 - KurrentDB is always available: retire the build-time feature flag

**Goal:** the KurrentDB event-store backend must be available in EVERY rigger binary, not behind a
build-time cargo feature. The shared-store capability - multiple users' rigger instances appending to
one KurrentDB so agent context is shared across a team - is a first-class product capability, and a
consumer who installs the default build must be able to point at a shared store with a flag, never a
recompile. A backend behind `-F kurrentdb` does not ship: `--eventstore kurrentdb` in the default
binary today errors with "requires the `kurrentdb` cargo feature", which is a dead end for exactly the
user the capability exists for. Retire the flag; the adapter compiles into every build.

## Design

The adapter (`src/eventstore/kurrentdb.rs`) is complete and contract-verified (its `passes_the_contract`
test runs the backend-agnostic store contract against a real KurrentDB container). What gates it is
small and explicit:

- **`Cargo.toml`:** the `kurrentdb` feature currently activates `dep:kurrentdb`, `dep:tokio`, and
  `dep:testcontainers`. Make `kurrentdb` and `tokio` UNCONDITIONAL `[dependencies]` (the adapter's
  gRPC client and its runtime are part of the product), and move `testcontainers` to
  `[dev-dependencies]` - it drives the contract TEST only and must never sit in the production
  dependency tree. Delete the `kurrentdb` feature from `[features]` and from the `check-cfg` values;
  nothing in the tree may reference `feature = "kurrentdb"` afterwards.
- **`src/eventstore/mod.rs`:** the `#[cfg(feature = "kurrentdb")]` module gate comes off; the module
  is always compiled and exported.
- **`src/main.rs`:** the gated `open_kurrentdb` pair collapses to the real implementation only; the
  `#[cfg(not(...))]` stub and its "requires the `kurrentdb` cargo feature" error are deleted (that
  error can no longer happen), along with the stub's companion test. `--eventstore kurrentdb` without
  a connection string keeps its existing clear error (`--conn <url>` or `KURRENTDB_CONN`).
- **The contract test** compiles in every lane and keeps its existing graceful skip when no container
  runtime is reachable (it prints the skip and returns), so a CI box without a container runtime stays
  green while a box with one exercises the real backend.

Feature-lane note: the two CI lanes (default and `--no-default-features`) both now compile the
adapter - that is the point of "always available". The lanes continue to differ only in the grounding
features (`symbols`/`turbovec`), which are orthogonal to the store.

## Global constraints

- Hyphens, not em dashes (a gate checks the diff; U+2014 fails it). No references to any external tool
  or project in code, comments, or commit messages.
- Both feature lanes stay green: `cargo fmt --check`; `cargo clippy --all-targets -D warnings`;
  `cargo test` - on default features AND `--no-default-features` - and BOTH lanes now compile the
  KurrentDB adapter.
- The event log stays the source of truth; this spec changes packaging/gating only - no store
  semantics, no event type, no projection change.
- `testcontainers` must not appear in the production dependency graph (dev-dependencies only).
- The sqlite backend remains the default; `--eventstore kurrentdb` remains explicit opt-in at runtime.

## Done when

- [ ] a test proves ALWAYS-AVAILABLE: in the default build (no feature flags), selecting the KurrentDB
  backend without a connection string fails with the missing-`--conn` error - never a missing-cargo-
  feature error - proving the adapter is compiled in and reachable. This criterion OWNS the
  unconditional availability.
- [ ] a test proves DEP HYGIENE: the `kurrentdb` cargo feature no longer exists (building with
  `-F kurrentdb` is rejected by cargo as an unknown feature) and `testcontainers` is a dev-dependency
  only. This criterion OWNS the packaging change.
- [ ] the existing KurrentDB contract test compiles and runs (or gracefully skips without a container
  runtime) in BOTH lanes. This criterion OWNS the test surface.
- [ ] both feature lanes green (fmt, clippy, test on default and `--no-default-features`).
