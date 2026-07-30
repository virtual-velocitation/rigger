# 48 - The event store is configuration: one resolution authority behind the port

**Goal:** which event-store backend a project uses is pure CONFIGURATION, with identical, reproducible
behavior regardless of the choice. Rigger's write and read sides already speak one stable interface -
the `EventStore` port (`src/eventstore/mod.rs`): every emitter hands the same `Event` envelope to the
same `append`, and the backend-agnostic contract suite (`src/eventstore/contract.rs`) pins every
adapter to identical semantics. What is NOT uniform today is the WIRING: only `rigger run` resolves a
backend through the port (`open_store`); every other command - `emit`, `result`, `status`, `step`,
`graph build` - hardcodes the embedded sqlite adapter (`Store::open(&db_path("events.db"))`). The
lethal case: a run started against a shared server-backed store has its workers self-report via bare
`rigger result <id>`, which writes to LOCAL sqlite - the run's state fractures across two stores and
the conductor never sees the result. Store selection must be PROJECT STATE resolved by one authority
that every command uses, never a per-command flag that only one command honors.

## Design

### One resolution authority

A single `resolve_store()` in `src/main.rs` is the ONLY place a concrete backend is constructed.
Every command that touches the event log calls it; no other call site may name `Store::open` for the
event log or the server adapter's constructor. It resolves, in precedence order:

1. **Explicit flags** (`--eventstore <sqlite|kurrentdb>`, `--conn <url>`) - kept on `run`, and now
   accepted anywhere store selection is meaningful; highest precedence for bootstrap and override.
2. **Environment** (`KURRENTDB_CONN`) - carries the full connection string, including credentials;
   the primary secret channel (CI secret stores, shell profiles).
3. **Local secret file** - `.rigger/store.conn`: one line, the full connection string. The gitignored,
   per-machine fallback for a developer box where exporting an env var every shell is friction. It is
   git-ignored BY CONSTRUCTION: the setup scaffold's gitignore patterns (the same mechanism that
   ignores the dash breadcrumbs) include it, and the resolver warns when the file is readable by
   other users. Credentials never ride a committed file.
4. **Project config** - the committed project configuration (the same file that already carries the
   project's workflow settings) gains a `store:` selection (`sqlite` default, or `kurrentdb` with an
   optional NON-SECRET URL - host/port only). This is how a TEAM pins its shared store: the CHOICE
   rides the repo so every member's rigger - and every worker's bare `rigger result` - resolves the
   same store with no flags, while each member's CREDENTIALS come from their env or secret file.
5. **Default** - the embedded sqlite store under `.rigger/`, exactly today's behavior. A project that
   configures nothing changes in nothing.

Selecting the server backend anywhere in the chain without a resolvable connection string errors
clearly, naming all three credential sources. And the connection string is a SECRET wherever it
appears: any error, log line, or status output that would echo it must REDACT the credential portion
(scheme and host may print; userinfo never does).

The resolved backend is the boxed port (`Box<dyn EventStore>`, wrapped in the same `Namespaced`
project scoping used today), so command code is backend-blind by construction.

### No topology opinions

The server-backed adapter's entire interface is its connection string, passed VERBATIM to the client:
remote hosts, TLS, credentials - whatever a centrally hosted deployment needs. Nothing in the shipping
code may assume a local container, a localhost address, or an insecure connection; containers appear
only in the test harness (the contract test, which keeps its graceful skip when no container runtime
is reachable).

### Projections stay local

`graph.db` and `progress.db` are per-machine, rebuildable PROJECTIONS of the log, not the log: they
remain embedded sqlite regardless of the event-store choice. The log is the shared truth; each
machine projects it locally. `resolve_store()` governs the EVENT LOG only.

## Global constraints

- Hyphens, not em dashes (a gate checks the diff; U+2014 fails it). No references to any external
  tool or project in code, comments, or commit messages.
- Both feature lanes stay green: `cargo fmt --check`; `cargo clippy --all-targets -D warnings`;
  `cargo test` - on default features AND `--no-default-features`.
- The event log stays the source of truth; the contract suite stays the reproducibility gate every
  backend must pass. This spec changes WIRING and CONFIG only - no store semantics, no event type, no
  envelope change.
- Backward compatible by default: with no config, no env, and no flags, every command behaves exactly
  as today (embedded sqlite under `.rigger/`).
- Secrets discipline: credentials ride the flag, the env, or the gitignored `.rigger/store.conn`
  only; the committed project config never requires a secret, and no output path echoes an
  unredacted connection string.

## Done when

- [ ] a test proves the SINGLE AUTHORITY: every event-log-touching command constructs its backend
  through `resolve_store()` - structurally, the sqlite event-log constructor appears at exactly one
  call site (the resolver), and a command invoked in a project configured for the server-backed store
  resolves that store (verified with the contract-test container harness, gracefully skipped without
  a runtime). This criterion OWNS the uniform wiring.
- [ ] a test proves PRECEDENCE: flag beats env beats the local secret file beats project config beats
  default, with a clear error naming all three credential sources when the server backend is selected
  without a resolvable connection string. This criterion OWNS resolution order.
- [ ] a test proves SECRETS DISCIPLINE: the connection string resolves from `.rigger/store.conn` when
  env and flags are absent; the setup scaffold's gitignore patterns include `store.conn`; and an error
  or status line that would echo the connection string redacts the credential portion (userinfo never
  prints). This criterion OWNS the secret channel; it does NOT own resolution order (the precedence
  criterion).
- [ ] a test proves NO TOPOLOGY OPINIONS: the connection string reaches the server client verbatim
  (host, TLS, credentials preserved; no localhost or insecure assumption injected). This criterion
  OWNS pass-through.
- [ ] a test proves PROJECTIONS STAY LOCAL: with the server-backed store configured, `graph.db` and
  `progress.db` still open under the local `.rigger/` (the store choice governs the event log only).
  This criterion OWNS the projection boundary.
- [ ] both feature lanes green (fmt, clippy, test on default and `--no-default-features`).
