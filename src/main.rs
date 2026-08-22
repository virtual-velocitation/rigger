//! The harness CLI and the single composition root: it constructs the concrete
//! adapters (event store, agent driver, grounder, projector) and injects them into
//! the conductor, which depends only on ports. `rigger run` executes the configured
//! workflow - the agent driver (`--driver cli|workflow`) and the event store
//! (`--eventstore sqlite|kurrentdb`) are selected by flag; `rigger graph` inspects
//! the context graph; `rigger init`/`setup` scaffold a project.

use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use rigger::blocker;
use rigger::budget::BuildBudget;
use rigger::canary;
use rigger::community;
use rigger::concepts;
use rigger::conductor::{self, Deps};
use rigger::config;
use rigger::contextgraph::{
    self,
    sqlite::{Located, Projector, PruneStats},
    Projection,
};
use rigger::dash;
use rigger::driver::cli;
use rigger::driver::replay::{spawn_scratch_path, ReplayDriver};
use rigger::eventstore::namespace::Namespaced;
use rigger::eventstore::{
    sqlite::{PrunedDerived, Store},
    Direction, Event, EventStore, ExpectedRevision, Filter,
};
use rigger::gate::{
    resolve_build_layer, resolve_mutation_layer, resolved_cache_dir, BuildEnv, ExecRunner, Gate,
    GateResult, Runner, STORE_FENCE_ENV,
};
use rigger::grounder::Grounder;
use rigger::ledger::{self, RunState};
use rigger::metrics::{self, Metrics};
use rigger::run as runscope;
use rigger::sidecar::{PeerDecision, Sidecar};
use rigger::worktree::{RunBranchSetup, Worktree};
use rigger::{hooks, mcpserver, playbooks, progress, spawn, spec, watch};

// Spec 74, criterion 2: the SAME derivation seam `build.rs` embeds at compile time
// (`build/gitsemver.rs`, already `#[path]`-included into `build.rs` and the two
// derivation-seam test binaries - see that file's own module doc comment: "that is the
// ONE derivation seam ... never a parallel reimplementation of it") is included here a
// third time so `cmd_validate` can re-derive the CHECKOUT's current version for the
// behind-the-tree comparison. This is a live, runtime invocation of `go-gitsemver` (and
// git) - but only from `validate`'s comparison path, never from `rigger version`'s own
// self-report, which stays the compile-time-only `GITSEMVER_VERSION` const below. The
// Global constraint ("version values are computed at COMPILE time only - no runtime git
// or tool invocation") reads narrowly, scoped to that self-report: `git_is_ancestor`
// (below) already shells to git live inside this exact command for the analogous
// spec-18-criterion-9 workflow-drift ordering diagnostic, an established precedent for a
// runtime tool/git call from `validate` for comparison purposes.
#[path = "../build/gitsemver.rs"]
#[allow(dead_code)]
mod gitsemver;

const RIGGER_DIR: &str = ".rigger";

/// The breadcrumb file, under [`RIGGER_DIR`], where a run driver records the URL of the
/// dashboard it auto-started (spec 19b, unit 1), so `rigger status` - a separate process -
/// can surface it. Read-only discoverability; a stale file after a finished run is a
/// lifecycle concern owned by unit 3's reaping, not this unit's start + discoverability.
const DASH_URL_FILE: &str = "dash.url";

/// The per-project dash marker file, under [`RIGGER_DIR`] alongside [`DASH_URL_FILE`]: the
/// port + pid of the run dashboard currently serving this project (a [`dash::DashMarker`]).
/// The step drive path reads it before spawning and writes it after, so the first `step` of
/// a run starts a dashboard and every later `step` finds it recorded and starts none - at
/// most one run dashboard per project (spec 39, criterion 1: idempotent start on step).
const DASH_MARKER_FILE: &str = "dash.marker";

/// The per-project breadcrumb, under [`RIGGER_DIR`], naming the run id whose OWN step path most
/// recently attempted a dash ensure (spec 69, round-8 fix for
/// adv-u69c1r7-mint-order-bug-is-structural-not-a-coverage-gap). Written by
/// [`record_dash_attempt`] on EVERY [`ensure_run_dashboard`] / [`start_run_dashboard`] call that
/// is not itself suppressed by the opt-out - regardless of whether that attempt started a new
/// dash, found one already serving, or failed - because all three outcomes mean the same thing
/// for this breadcrumb's purpose: THIS run's own step path just vouched for the dash, so a probe
/// that later finds it dead is this run's concern to report, not a maybe-inherited artifact.
///
/// This exists because [`watch::WatchInputs::dash_breadcrumb_written_at`] /
/// [`watch::WatchInputs::run_started_at`]'s wall-clock `written < started` comparison, though
/// empirically correct against the real call order in every one of the three drivers (RunStarted
/// mints via [`enforce_definition_pin`] / [`fresh_run_if_requested`] BEFORE the dash-ensure call
/// in the very same function - never the other way around, verified against the compiled binary,
/// not merely read from source), is still reasoning about ORDER to infer OWNERSHIP - exactly the
/// kind of proxy round 3-7's own reject history shows is easy to get backwards (round 7's own
/// review asserted the opposite order from what the binary actually does). This breadcrumb
/// replaces that inference with the fact itself for the common case: an EXPLICIT run-id match,
/// immune to clock skew or a future refactor that reorders the mint relative to the dash-ensure
/// call. It only ever WIDENS reporting (see `detect`'s `breadcrumb_predates_this_run`): a match
/// forces reporting regardless of what the timestamps say; a miss (absent file, or one naming an
/// older/different run - including every existing seeded-event test, which never calls the real
/// dash-ensure path at all and so never writes this file) falls back to the pre-existing
/// timestamp comparison unchanged, so no established suppression regresses.
const DASH_ATTEMPT_FILE: &str = "dash.attempt";

/// Record that THIS run's own step path just attempted a dash ensure (spec 69, round-8 fix; see
/// [`DASH_ATTEMPT_FILE`]'s doc for the full rationale). Best-effort like every other dash
/// breadcrumb write in this module - a failed write only risks a later false suppression of an
/// anomaly this run's own dash-liveness probe should catch anyway via the timestamp fallback,
/// never a broken run. `run_id` empty (no run started yet in this project) writes nothing: there
/// is no run for the breadcrumb to name, so a later watch poll correctly falls back to the
/// existing unknown-run handling rather than matching an empty string against another empty one.
fn record_dash_attempt(run_id: &str) {
    if !run_id.is_empty() {
        let _ = std::fs::write(db_path(DASH_ATTEMPT_FILE), run_id);
    }
}

/// Environment opt-out for the step path's always-on dashboard: when
/// [`DASH_DISABLE_ENV`] is set (to any value) the step does NOT auto-start a run
/// dashboard. Production leaves it unset (the dash is always-on, no opt-in flag - spec
/// 19b); a headless CI or the crate's own integration tests set it so a short-lived
/// `rigger step` never spawns a real dashboard process.
const DASH_DISABLE_ENV: &str = "RIGGER_NO_DASH";

/// Env override for the PORT the step-path always-on dashboard binds. Absent (the production
/// default) resolves to [`dash::DEFAULT_PORT`] with NO free-port search, so the singleton's
/// stable fixed-address contract (spec 50, criterion 4) is unchanged. It exists for the case the
/// fixed address otherwise makes untestable and unusable: a machine where a rigger dash already
/// holds the default (the self-hosting dev box always does) or a non-rigger process owns 7420 -
/// there the ensure path needs the same port seam the manual `rigger dash --port` already has.
/// The crate's own step-path dash integration tests set it to an ephemeral loopback port so they
/// exercise the ensure path WITHOUT fighting a real machine dash on the fixed default, exactly as
/// the direct-`rigger dash` singleton test injects `free_loopback_port`. A malformed value falls
/// back to the default (a bad knob never breaks a run's observability).
const DASH_PORT_ENV: &str = "RIGGER_DASH_PORT";

/// Env override (milliseconds) for the self-reap watcher's poll interval (spec 39, criterion 3),
/// and its production default. The crate's own integration test sets the env small so the
/// detached dash's self-reap is observable within the test, without changing the shipped cadence.
const DASH_REAP_POLL_ENV: &str = "RIGGER_DASH_REAP_POLL_MS";
const DASH_REAP_POLL_DEFAULT_MS: u64 = 5_000;

/// Env override (seconds) for the self-reap watcher's IDLE WINDOW (spec 50, criterion 5): a
/// registered instance whose heartbeat is older than this counts as no longer live. Absent, the
/// window defaults to the registry's own idle bound ([`rigger::registry::DEFAULT_IDLE_MS`]) so the
/// reader and the reaper share one staleness authority; the crate's own integration test sets it
/// small so the singleton's self-reap is observable within the test rather than on the shipped
/// multi-minute cadence.
const DASH_REAP_STALE_ENV: &str = "RIGGER_DASH_REAP_STALE_SECS";

/// The tracked file under `.rigger/` that carries the durable project identity (spec 09,
/// Gap 20): one trimmed line committed to git, so the identity survives directory renames
/// and machine moves instead of tracking the volatile directory basename.
const PROJECT_ID_FILE: &str = "project.id";

/// The run branch the stepwise driver accumulates a run on: every unit worktree is
/// branched from it and every approved unit is merged back into it. Mirrored by
/// `RUN` in `workflows/rigger.js` (the JS driver); the two names must agree.
const RUN_BRANCH: &str = "rigger-run";

/// The default ref the run branch is anchored to when `rigger step` (or the driver)
/// is not given `--base`, and ONLY when the run branch does not exist yet - once
/// [`RUN_BRANCH`] exists it is reused as the run's anchor and the base is not consulted
/// (see [`Worktree::ensure_run_branch`]). If this default does not resolve (a repo with
/// no remote, a `master`-default repo, or a pre-fetch clone) the run branch is created
/// off the current HEAD instead, so isolation is still established. Mirrored by the
/// driver's own default.
const DEFAULT_BASE_REF: &str = "origin/main";

/// The JS-driver RUNTIME files, embedded in the binary so `rigger setup` can
/// provision a per-project shim without the user cloning the repo. Only the three
/// runtime files ship: `shim.mjs` (the driver), `package.json`, and the
/// `package-lock.json` (so `npm ci` installs the exact locked tree). The dev-only
/// `mock-*`/`*.test.mjs` files are deliberately NOT embedded - they are for the
/// repo's own tests + CI, not the runtime a user runs.
const SHIM_MJS: &str = include_str!("../shim/shim.mjs");
const SHIM_PACKAGE_JSON: &str = include_str!("../shim/package.json");
const SHIM_PACKAGE_LOCK_JSON: &str = include_str!("../shim/package-lock.json");

/// The three embedded shim runtime files as (filename, contents) pairs, written
/// verbatim into `<project>/.rigger/shim/` by `provision_shim`.
const SHIM_FILES: &[(&str, &str)] = &[
    ("shim.mjs", SHIM_MJS),
    ("package.json", SHIM_PACKAGE_JSON),
    ("package-lock.json", SHIM_PACKAGE_LOCK_JSON),
];

/// The native Claude Code workflow, embedded in the binary so `rigger setup` can
/// install it into a project without the user cloning the repo. A saved Claude Code
/// workflow is a single self-contained `.js` file: Claude Code auto-discovers any
/// `.js` under `<project>/.claude/workflows/`, so writing this there makes the
/// `/rigger <spec>` workflow runnable immediately, with no registration step. The
/// workflow drives its agents through the Workflow tool and grounds / persists their
/// reasoning via `rigger ground`, `rigger emit`, and `rigger peers`.
const RIGGER_WORKFLOW: &str = include_str!("../workflows/rigger.js");

/// Where the native `/rigger` workflow is installed, relative to the project root:
/// `<root>/.claude/workflows/rigger.js`. Claude Code auto-discovers `.js` files in
/// this directory, so the workflow is runnable as `/rigger <spec>` the moment it is
/// written - no registration. Rooted at `root` so it is testable against a temp dir.
fn workflow_path(root: &Path) -> std::path::PathBuf {
    root.join(".claude").join("workflows").join("rigger.js")
}

/// Where `rigger docs` writes the rendered handbook discipline chapter, relative to the
/// project root. It lives beside the other handbook chapters and is drift-checked against
/// a fresh render (spec 20, unit 2). The single source of this path.
const HANDBOOK_DISCIPLINE_REL: &str = "docs/handbook/using-rigger.md";

/// The default location this repo keeps its specs, surfaced in the rendered discipline as
/// a project specific a repo overlay (spec 20, unit 3) can override without editing the
/// shared discipline source.
const DEFAULT_SPECS_LOCATION: &str = "specs/";

type Res = Result<(), Box<dyn std::error::Error>>;

/// The build-provenance identifier (a git commit/describe id) that `build.rs` embeds at
/// compile time, so a running binary can report WHICH source it was built from. Always
/// non-empty (the build script falls back to a sentinel outside a git checkout). This is the
/// single authority for the value: the workflow-drift diagnostic reads the SAME const to name
/// which side is stale, rather than re-deriving provenance a second way.
const BUILD_PROVENANCE: &str = env!("RIGGER_BUILD_PROVENANCE");

/// The version `go-gitsemver` derives for the built commit under the committed
/// `go-gitsemver.yml` (spec 74): `FullSemVer` with `ShortSha` folded into its build
/// metadata, embedded by `build.rs` at COMPILE time (the binary never invokes the tool,
/// or git, again at runtime - see `build/gitsemver.rs`, the single derivation seam
/// shared with its test). Falls back to the bare crate semver plus an explicit
/// `+unversioned` marker whenever `go-gitsemver` could not run: never fabricated, never
/// a failed build.
const GITSEMVER_VERSION: &str = env!("RIGGER_GITSEMVER_VERSION");

/// The one-line version identity: the derived semver plus the embedded build provenance.
/// Sole source of the version string, so `rigger version` and `rigger --version` cannot
/// drift.
fn version_line() -> String {
    format!("rigger {} (build {})", GITSEMVER_VERSION, BUILD_PROVENANCE)
}

/// `rigger version` (and `rigger --version` / `-V`): print the crate version and the build
/// provenance, so any agent can identify the exact binary without guessing.
fn cmd_version() -> Res {
    println!("{}", version_line());
    Ok(())
}

/// Which agent driver a `run` uses (§10): `cli` is the standalone `claude`
/// subprocess path; `workflow` is the in-Claude-Code MCP-server path.
#[derive(Clone, Copy, PartialEq, Eq)]
enum DriverKind {
    Cli,
    Workflow,
}

/// Which event-store backend a run uses (§10): `sqlite` is the embedded default;
/// `kurrentdb` is the server backend, compiled into every build (spec 47) and
/// selected at runtime by `--eventstore kurrentdb`.
#[derive(Clone, Copy, PartialEq, Eq)]
enum StoreKind {
    Sqlite,
    KurrentDb,
}

/// The parsed flags shared by `run` (and the `--driver workflow` path): which
/// driver, which event store, the connection string for the server backend, the
/// positional spec path, and whether to force a fresh run.
struct RunArgs {
    driver: DriverKind,
    store: Option<StoreKind>,
    conn: Option<String>,
    spec: Option<String>,
    /// `--fresh`: begin a NEW run for the spec's criteria even when the latest run in the
    /// store already matches them (which `ensure_started` would otherwise adopt). The
    /// evented recovery from a run wedged in a terminal state - e.g. a plan-critique
    /// escalation - whose spec is unchanged; see [`rigger::run::start_fresh`].
    fresh: bool,
    /// `--rebase-definition` (spec 13, unit 1): on a live run whose on-disk definition drifted
    /// from the hash pinned at start, record the supersession (old hash, new hash) and continue
    /// on the new definition, instead of HALTING loudly. The operator's explicit "I meant to
    /// edit the definition mid-campaign" escape.
    rebase_definition: bool,
    /// `--base <ref>` (spec 18, criterion 6): the run-branch base a run entry anchors on,
    /// exactly as `rigger step --base` does. `None` when the flag is absent, so the effective
    /// base resolves (via [`resolve_run_base`]) to the `RIGGER_BASE` env override or the
    /// load-bearing [`DEFAULT_BASE_REF`] (`origin/main`) - the default stays unchanged.
    base: Option<String>,
}

/// Parse `rigger run`'s flags: `--driver <cli|workflow>`, `--eventstore
/// <sqlite|kurrentdb>`, `--conn <url>`, `--base <ref>` (the run-branch base, spec 18
/// criterion 6), and a single positional spec path. Unknown flags and a second positional
/// are rejected (§10).
fn parse_run_args(args: &[String]) -> Result<RunArgs, Box<dyn std::error::Error>> {
    let mut driver = DriverKind::Cli;
    let mut store = None;
    let mut conn = None;
    let mut spec = None;
    let mut fresh = false;
    let mut rebase_definition = false;
    let mut base = None;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--fresh" => fresh = true,
            "--rebase-definition" => rebase_definition = true,
            "--base" => {
                i += 1;
                base = match args.get(i) {
                    Some(r) => Some(r.clone()),
                    None => return Err("run: --base expects a ref".into()),
                };
            }
            "--driver" => {
                i += 1;
                driver = match args.get(i).map(String::as_str) {
                    Some("cli") => DriverKind::Cli,
                    Some("workflow") => DriverKind::Workflow,
                    other => {
                        return Err(
                            format!("run: --driver expects cli|workflow, got {other:?}").into()
                        )
                    }
                };
            }
            "--eventstore" => {
                i += 1;
                store = match args.get(i).map(String::as_str) {
                    Some("sqlite") => Some(StoreKind::Sqlite),
                    Some("kurrentdb") => Some(StoreKind::KurrentDb),
                    other => {
                        return Err(format!(
                            "run: --eventstore expects sqlite|kurrentdb, got {other:?}"
                        )
                        .into())
                    }
                };
            }
            "--conn" => {
                i += 1;
                conn = match args.get(i) {
                    Some(c) => Some(c.clone()),
                    None => return Err("run: --conn expects a connection url".into()),
                };
            }
            flag if flag.starts_with("--") => {
                return Err(format!("run: unknown flag {flag:?}").into());
            }
            positional => {
                if spec.is_some() {
                    return Err(format!(
                        "run: unexpected second positional argument {positional:?}"
                    )
                    .into());
                }
                spec = Some(positional.to_string());
            }
        }
        i += 1;
    }
    Ok(RunArgs {
        driver,
        store,
        conn,
        spec,
        fresh,
        rebase_definition,
        base,
    })
}

/// Resolve the effective run-branch base for a run entry (spec 18, criterion 6), and
/// whether it was chosen explicitly. Precedence: an explicit `--base <ref>` on the argv
/// (`argv_base`), then the `RIGGER_BASE` environment override (`env_base`) - the channel
/// `rigger workflow` threads its `--base` down through the shim to the served `rigger
/// serve`, since the shim spawns the child with the inherited environment (the same
/// mechanism it already uses for `RIGGER_BIN`) - then the load-bearing [`DEFAULT_BASE_REF`]
/// (`origin/main`). An empty override is treated as unset so a run never anchors on "".
/// The bool is `true` when the base came from the flag or the env (used only to warn when
/// an operator's chosen base is ignored because the run branch already exists).
fn resolve_run_base(argv_base: Option<&str>, env_base: Option<&str>) -> (String, bool) {
    let chosen = argv_base
        .filter(|s| !s.is_empty())
        .or_else(|| env_base.filter(|s| !s.is_empty()));
    match chosen {
        Some(b) => (b.to_string(), true),
        None => (DEFAULT_BASE_REF.to_string(), false),
    }
}

/// Which event-log backend a command resolves to (§48, "one resolution authority"): the
/// embedded sqlite default, or the server backend addressed by a connection string. Produced
/// by [`store_selection`] (which owns the precedence among the configuration sources) and
/// consumed by [`resolve_store`] (which owns the construction) and the courier locator
/// [`require_store_dir`] (which needs to know whether a local `events.db` is even required).
#[derive(Clone, Debug, PartialEq, Eq)]
enum StoreSelection {
    /// The embedded sqlite event log. Its store is the file the caller resolves; the isolated
    /// replay store and the local identity migration are sqlite by construction and pass this.
    Sqlite,
    /// The server backend, addressed by this verbatim connection string.
    Server(String),
}

impl StoreSelection {
    /// Whether this selection is the embedded sqlite backend (whose store is a local file).
    fn is_sqlite(&self) -> bool {
        matches!(self, StoreSelection::Sqlite)
    }
}

/// Open the embedded sqlite event log at `path`. This is the ONE sqlite event-log constructor
/// (§48, the single authority): [`resolve_store`] boxes it as the port for every command, and
/// the local identity migration - which needs the concrete [`Store`] for its stream-rename
/// maintenance - constructs through here too, so the sqlite backend is built at exactly one
/// call site. The structural test in `tests/store_resolution.rs` pins that.
fn open_sqlite_store(path: &str) -> Result<Store, Box<dyn std::error::Error>> {
    Ok(Store::open(path)?)
}

/// The `KURRENTDB_CONN` connection string from the environment, treating an empty value as
/// unset so a stray `KURRENTDB_CONN=` never selects the server with no address.
fn env_conn() -> Option<String> {
    std::env::var("KURRENTDB_CONN")
        .ok()
        .filter(|s| !s.is_empty())
}

/// The connection string from the per-machine secret file `<rigger_dir>/store.conn` (§48 rung 3),
/// treating an ABSENT file (or a blank one) as unset - `Ok(None)`, "no opinion", so the resolver
/// falls through to the next rung. One line: the full connection string, credentials included - the
/// gitignored developer-box fallback for a machine where exporting the env var every shell is
/// friction. This is the READ that positions the secret file in the precedence chain (between the
/// environment and the committed config); the SECRETS criterion layers the `.gitignore` pattern, the
/// world-readable-permission warning, and connection-string redaction ONTO this one reader rather
/// than adding a second parallel one.
///
/// A PRESENT-but-unreadable file (a permission or IO fault, distinct from a genuinely absent one)
/// surfaces LOUDLY as an error, never collapsing into the same `None` an absent file returns - the
/// exact NotFound-vs-other split [`config::read_store_config`] makes one rung down
/// (d-u2-config-unreadable-loud / d-u2-conn-file-unreadable-loud). Swallowing it (the old
/// `read_to_string(...).ok()`) let an unreadable secret file fall silently through to the sqlite
/// default: a courier on a server-pinning box whose `store.conn` it cannot read - the different-user
/// / permission edge §48 explicitly contemplates - would self-report to LOCAL sqlite while the
/// conductor uses the server, fracturing the run's state across two stores.
fn store_conn_file(rigger_dir: &Path) -> Result<Option<String>, Box<dyn std::error::Error>> {
    let path = rigger_dir.join("store.conn");
    match std::fs::read_to_string(&path) {
        Ok(body) => {
            let conn = body.trim().to_string();
            if conn.is_empty() {
                return Ok(None);
            }
            // A real connection string was read: nudge the operator if the secret file is exposed
            // to other users (a hygiene warning only - resolution proceeds regardless).
            warn_if_conn_file_is_exposed(&path);
            Ok(Some(conn))
        }
        // ABSENT is "no opinion" (fall through); any OTHER IO error is a present-but-unreadable
        // secret file and surfaces LOUDLY - never the silent wrong-store fallback of the old `.ok()`.
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(format!("read store connection file: {e}").into()),
    }
}

/// Whether a secret file's Unix `mode` grants READ to group or other - i.e. someone besides the
/// owner can see the connection-string credential it holds. `0o044` masks the group-read and
/// other-read bits; a credential file should be owner-only (`0o600`), so any of those bits set
/// means the secret is exposed. Pure so the threshold is unit-testable without touching the fs.
#[cfg(unix)]
fn conn_file_is_group_or_other_readable(mode: u32) -> bool {
    mode & 0o044 != 0
}

/// Warn (NEVER fail) when the per-machine secret file is readable by users other than its owner:
/// it carries the connection string's credential, so a group- or world-readable file exposes that
/// secret (§48, secrets discipline). This is a hygiene nudge on the ONE secret-file reader
/// ([`store_conn_file`]) - store resolution proceeds regardless, and the connection string itself is
/// never printed (only the file path and its mode). Unix-only: the mode bits are a POSIX concept
/// with no cross-platform equivalent; elsewhere it is a no-op.
fn warn_if_conn_file_is_exposed(path: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(meta) = std::fs::metadata(path) {
            let mode = meta.permissions().mode();
            if conn_file_is_group_or_other_readable(mode) {
                eprintln!(
                    "warning: {} is readable by other users (mode {:o}); it holds the store \
                     connection credential - restrict it to your account (chmod 600 {}) so the \
                     secret is not exposed",
                    path.display(),
                    mode & 0o777,
                    path.display()
                );
            }
        }
    }
    #[cfg(not(unix))]
    {
        let _ = path;
    }
}

/// The `.rigger` directory whose committed config (`workflow.yml`) and per-machine secret file
/// (`store.conn`) the store resolver reads for the lower-precedence rungs (§48 rungs 3-4). Anchored
/// at the OWNING repo root (`main_repo_root`, whose `git-common-dir` resolves the main checkout even
/// from a nested git worktree), so a courier's `rigger result` reads the SAME secret file and config
/// the conductor does - the gitignored `store.conn` lives only in the main checkout, never in a
/// linked worktree, so a cwd-anchored read would miss it and fracture the store selection. Falls
/// back to the cwd's `.rigger` outside any git context.
fn config_rigger_dir() -> PathBuf {
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    main_repo_root(&cwd).unwrap_or(cwd).join(RIGGER_DIR)
}

/// Interpret the committed config's `store.backend` (§48 rung 4): an empty value is "no opinion"
/// (the resolver falls through to the next rung, the default), `sqlite` / `kurrentdb` select a
/// backend, and anything else is a clear configuration error naming the accepted values - never a
/// silent fallback that would hide a typo behind today's default.
fn store_backend_kind(
    cfg: &config::StoreConfig,
) -> Result<Option<StoreKind>, Box<dyn std::error::Error>> {
    match cfg.backend.trim() {
        "" => Ok(None),
        "sqlite" => Ok(Some(StoreKind::Sqlite)),
        "kurrentdb" => Ok(Some(StoreKind::KurrentDb)),
        other => Err(format!(
            "the project config's store.backend is {other:?}; only \"sqlite\" or \"kurrentdb\" \
             are valid"
        )
        .into()),
    }
}

/// Resolve the server connection string from the available credential sources, in precedence order:
/// the explicit `--conn` flag, then the `KURRENTDB_CONN` environment value, then the per-machine
/// `.rigger/store.conn` secret file. When the server backend is selected with NONE of them, the
/// error names ALL THREE channels (§48 criterion 2) so the fix is unambiguous. `env_conn` is the
/// already-resolved environment value (empty is treated as unset).
fn resolve_conn(
    flag_conn: Option<&str>,
    env_conn: Option<&str>,
    rigger_dir: &Path,
) -> Result<String, Box<dyn std::error::Error>> {
    if let Some(conn) = flag_conn.filter(|s| !s.is_empty()) {
        return Ok(conn.to_string());
    }
    if let Some(conn) = env_conn.filter(|s| !s.is_empty()) {
        return Ok(conn.to_string());
    }
    // The secret file, distinguishing absent (fall through to the error below) from
    // present-but-unreadable (a LOUD read error, propagated with `?` - never swallowed into the
    // "no connection string" case, which would misdiagnose an unreadable file as a missing one).
    if let Some(conn) = store_conn_file(rigger_dir)? {
        return Ok(conn);
    }
    Err(
        "the server event store is selected but no connection string is set - provide one via \
         --conn <url>, the KURRENTDB_CONN environment variable, or the .rigger/store.conn \
         secret file"
            .into(),
    )
}

/// Resolve WHICH event-log backend a command uses (§48, "one resolution authority") - the PURE
/// core of the single selection authority, over an explicit `.rigger` directory and an explicit
/// environment value so the full precedence is testable with temp dirs and no process-env
/// mutation. The public [`store_selection`] wraps this with the real environment and the owning
/// repo's `.rigger`. Precedence, highest first (§48 criterion 2, "resolution order"):
///
///   1. an explicit `--eventstore`/`--conn` flag (`flag_store` / `flag_conn`), kept on `run`. A
///      non-empty `--conn` alone SELECTS the server addressed by it (a bare `--conn` is never
///      dropped to a lower rung); `--eventstore sqlite` wins outright even against a `--conn`;
///   2. the `KURRENTDB_CONN` environment value (`env_conn`, the full connection string);
///   3. the local secret file `<rigger_dir>/store.conn` (the gitignored per-machine credential);
///   4. the committed project config's `store:` selection (the CHOICE pinned in the repo, with an
///      optional non-secret host/port URL - credentials never ride the committed file);
///   5. the embedded sqlite store as the default when nothing selects otherwise (so a project that
///      configures nothing behaves exactly as today - the backward-compatibility bar).
///
/// Selecting the server (by any rung) without a resolvable connection string is a clear error
/// naming all three credential channels ([`resolve_conn`]).
fn store_selection_at(
    flag_store: Option<StoreKind>,
    flag_conn: Option<&str>,
    env_conn: Option<String>,
    rigger_dir: &Path,
) -> Result<StoreSelection, Box<dyn std::error::Error>> {
    // The environment value, normalized (an empty `KURRENTDB_CONN=` is unset, never a server with
    // no address). Threaded on to `resolve_conn` so the flag rung's fallback order stays uniform.
    let env = env_conn.as_deref().filter(|s| !s.is_empty());
    // A non-empty `--conn` flag, normalized (a stray `--conn ''` is unset, never a server with no
    // address). It SELECTS the server on its own (below), so it is normalized once here.
    let flag = flag_conn.filter(|s| !s.is_empty());
    // 1. an explicit flag is the highest-precedence, unambiguous override.
    match flag_store {
        Some(StoreKind::KurrentDb) => {
            return Ok(StoreSelection::Server(resolve_conn(flag, env, rigger_dir)?))
        }
        // `--eventstore sqlite` wins OUTRIGHT: the named backend is the unambiguous override, so it
        // beats even a `--conn` present alongside it (contradictory flags resolve to the backend).
        Some(StoreKind::Sqlite) => return Ok(StoreSelection::Sqlite),
        // No `--eventstore`, but a bare `--conn <url>` SELECTS the server addressed verbatim by it
        // (§48 rung 1; d-u2-conn-flag-selects-server). A non-empty `--conn` is a first-class
        // highest-precedence source - dropping it to a lower rung was the store-fracture footgun
        // (`rigger run --conn kurrentdb://prod <spec>` silently resolving LOCAL sqlite).
        None => {
            if let Some(conn) = flag {
                return Ok(StoreSelection::Server(conn.to_string()));
            }
        }
    }
    // 2. the environment carries the full connection string, so a bare command (no flag) in a
    //    shell or CI configured for the server resolves it - the wiring that keeps a worker's
    //    `rigger result` on the same store the run uses, instead of a local sqlite fracture.
    if let Some(conn) = env {
        return Ok(StoreSelection::Server(conn.to_string()));
    }
    // 3. the per-machine gitignored secret file: a developer box that pins the shared server
    //    without exporting the env var every shell. An absent file is "no opinion" (fall through);
    //    a present-but-unreadable one surfaces LOUDLY here (`?`), never a silent drop to the sqlite
    //    default that would fracture a server-pinned run's store (d-u2-conn-file-unreadable-loud).
    if let Some(conn) = store_conn_file(rigger_dir)? {
        return Ok(StoreSelection::Server(conn));
    }
    // 4. the committed project config: the CHOICE the team pins in the repo. Its optional
    //    non-secret URL is the address; absent, the address must come from a credential source -
    //    all of which rungs 1-3 already found empty, so `resolve_conn` names all three.
    let cfg = config::read_store_config(rigger_dir)?;
    match store_backend_kind(&cfg)? {
        Some(StoreKind::KurrentDb) => {
            let conn = if cfg.url.trim().is_empty() {
                // No committed url: the address comes from a credential source. Thread the real
                // `--conn` flag in (not `None`) so the SELECTED store and the credential that OPENS
                // it can never drift - a non-empty flag already returned at rung 1, so this honours
                // it defensively for any future path that reaches rung 4 with a flag live.
                resolve_conn(flag, env, rigger_dir)?
            } else {
                cfg.url.trim().to_string()
            };
            return Ok(StoreSelection::Server(conn));
        }
        Some(StoreKind::Sqlite) => return Ok(StoreSelection::Sqlite),
        None => {}
    }
    // 5. the default: the embedded sqlite store (backward compatible - a project that configures
    //    nothing changes in nothing).
    Ok(StoreSelection::Sqlite)
}

/// Resolve WHICH event-log backend a command uses, from the real environment and the owning repo's
/// `.rigger` (§48, "one resolution authority"). This is the SINGLE place the selection is decided,
/// so every command - and every worker's bare `rigger result` - agrees on the store without a
/// per-command flag. The precedence and its error surface live in the pure [`store_selection_at`];
/// this wrapper supplies the two ambient inputs (the `KURRENTDB_CONN` environment value and the
/// resolved `.rigger` directory).
fn store_selection(
    flag_store: Option<StoreKind>,
    flag_conn: Option<&str>,
) -> Result<StoreSelection, Box<dyn std::error::Error>> {
    let rigger_dir = config_rigger_dir();
    store_selection_at(flag_store, flag_conn, env_conn(), &rigger_dir)
}

/// Construct the selected event-log backend as a boxed port (§48, "one resolution authority").
/// This is the ONLY place a concrete event-log backend is handed to a command: every command
/// routes its backend through here (the isolated replay store and the local identity migration
/// pass an explicit [`StoreSelection::Sqlite`], being local by construction), so store selection
/// is uniform. `sqlite_path` is where the embedded sqlite log lives when sqlite is selected; it
/// is ignored for the server backend, whose entire address is its connection string.
fn resolve_store(
    sel: &StoreSelection,
    sqlite_path: &str,
) -> Result<Box<dyn EventStore>, Box<dyn std::error::Error>> {
    match sel {
        StoreSelection::Sqlite => Ok(Box::new(open_sqlite_store(sqlite_path)?)),
        StoreSelection::Server(conn) => {
            Ok(Box::new(rigger::eventstore::kurrentdb::Store::open(conn)?))
        }
    }
}

/// The CREDENTIAL-FREE store identity for the machine-global instance registry (spec 50),
/// derived from the run's resolved [`StoreSelection`]: the local sqlite path (local by
/// construction, no credential) or the shared server's `scheme://host:port` with any
/// `user:password@` userinfo and any `?query` stripped by the crate's SINGLE redaction authority
/// ([`rigger::eventstore::endpoint_label`], which shares `redact_conn`'s hardened authority parse -
/// not a second, weaker parser). The credential the shared store OPENS with never reaches the
/// registry - the dash re-resolves it through the store-resolution authority exactly as every
/// command does.
fn registry_store_identity(sel: &StoreSelection, root: &Path) -> rigger::registry::StoreIdentity {
    match sel {
        StoreSelection::Sqlite => rigger::registry::StoreIdentity::Local {
            path: root
                .join(RIGGER_DIR)
                .join("events.db")
                .to_string_lossy()
                .into_owned(),
        },
        StoreSelection::Server(conn) => rigger::registry::StoreIdentity::Shared {
            endpoint: rigger::eventstore::endpoint_label(conn),
        },
    }
}

/// How often a held [`RunRegistration`] refreshes its heartbeat: a THIRD of the registry's idle
/// window ([`rigger::registry::DEFAULT_IDLE_MS`]), so a live in-process run is re-stamped at least
/// three times before a reader would consider its entry stale - comfortably inside the window even
/// under scheduling jitter or one very long in-process gate.
fn registry_heartbeat_interval() -> std::time::Duration {
    std::time::Duration::from_millis(rigger::registry::DEFAULT_IDLE_MS / 3)
}

/// A LIVE registration in the machine-global discovery registry (spec 50), held for as long as the
/// run it represents is in flight. Its initial entry is written synchronously by
/// [`register_run_instance`]; while this guard is held a background thread REFRESHES the heartbeat
/// every [`registry_heartbeat_interval`], and dropping it (the run scope ends, on success OR error)
/// signals that thread to stop and joins it. The periodic refresh is what keeps a run driven WHOLLY
/// in-process (`rigger run`/`serve` - a single `conductor::run` call that can drive for hours, or a
/// `rigger step` whose one gate runs longer than the idle window) from aging out of discovery
/// MID-RUN: a one-shot register alone would let a reader prune the live entry after the window.
struct RunRegistration {
    /// Dropping this disconnects the channel, so the heartbeat thread's `recv_timeout` returns at
    /// once (never waiting out a full interval) and the join below completes promptly.
    tx: Option<std::sync::mpsc::Sender<()>>,
    handle: Option<std::thread::JoinHandle<()>>,
}

impl RunRegistration {
    /// An inert guard that holds no thread - the homeless/unwritable degrade path, so the caller
    /// binds one uniform type whether or not a registry entry was actually written.
    fn inert() -> Self {
        RunRegistration {
            tx: None,
            handle: None,
        }
    }
}

impl Drop for RunRegistration {
    fn drop(&mut self) {
        // Disconnect first so the sleeping heartbeat thread wakes immediately, THEN reap it. Both
        // are best-effort: a poisoned/panicked thread never blocks the run's teardown.
        drop(self.tx.take());
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

/// Register THIS invocation's instance in the machine-global discovery registry (spec 50, criterion
/// 2): the project root, the credential-free store identity, and a fresh heartbeat, keyed so a
/// re-registration refreshes one entry in place. This is the SINGLE registration writer - every run
/// entry point (`rigger step`, and the in-process `rigger run`/`serve`/`workflow` drivers) requests
/// through it, so the machine-global registry sees every invocation that starts or advances a run,
/// not just the stepwise loop path.
///
/// Returns a [`RunRegistration`] the CALLER MUST HOLD for the life of the run (`let _reg = ...;`):
/// the initial entry is written here, synchronously, and the returned guard's background thread
/// keeps its heartbeat fresh until the guard drops. BEST-EFFORT and warn-only throughout: the
/// registry is discovery metadata whose loss is harmless (live instances repopulate it), so a
/// homeless environment or a write error degrades to an inert guard and NEVER fails the run.
#[must_use = "hold the RunRegistration for the life of the run so its heartbeat stays fresh"]
fn register_run_instance(repo: &str, selection: &StoreSelection) -> RunRegistration {
    let Some(dir) = rigger::registry::default_dir() else {
        return RunRegistration::inert(); // homeless environment: degrade to no registration
    };
    let root = if repo.is_empty() {
        std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
    } else {
        PathBuf::from(repo)
    };
    let inst = rigger::registry::Instance {
        project: project_identity(),
        root: root.to_string_lossy().into_owned(),
        store: registry_store_identity(selection, &root),
        heartbeat_ms: rigger::registry::now_ms(),
    };
    // The initial, synchronous registration. On failure, degrade to an inert guard rather than
    // spawn a heartbeat thread that could only fail the same way.
    if let Err(e) = rigger::registry::write(&dir, &inst) {
        eprintln!("rigger: instance registry write skipped ({e}); discovery is unaffected");
        return RunRegistration::inert();
    }
    // The heartbeat thread re-stamps `inst` every interval until the guard drops. `recv_timeout`
    // makes the sleep interruptible: each `Timeout` refreshes the entry; a `Disconnected` (the guard
    // dropped) ends the `while let` at once, so teardown never waits out a full interval.
    let (tx, rx) = std::sync::mpsc::channel::<()>();
    let interval = registry_heartbeat_interval();
    let handle = std::thread::spawn(move || {
        while let Err(std::sync::mpsc::RecvTimeoutError::Timeout) = rx.recv_timeout(interval) {
            let refreshed = rigger::registry::Instance {
                heartbeat_ms: rigger::registry::now_ms(),
                ..inst.clone()
            };
            let _ = rigger::registry::write(&dir, &refreshed); // best-effort refresh
        }
    });
    RunRegistration {
        tx: Some(tx),
        handle: Some(handle),
    }
}

/// The project identity that scopes the event streams and context graph (§5.1.1,
/// R9): the basename of the git repo top-level, falling back to the current
/// directory's name, falling back to "rigger". Never empty.
///
/// Anchored at the process cwd, which is correct for the RUN DRIVER (`run`/`step`/
/// `serve`): it creates the store under the cwd's `.rigger/`, so the cwd's git
/// top-level is the identity that scopes it. The store-opening COURIERS must NOT use
/// this - a courier can run from a cwd that is not the store's owner (a nested git
/// worktree) - so they bind identity to the RESOLVED store root instead, via
/// [`StoreLocation::identity`] / [`project_identity_at`].
fn project_identity() -> String {
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    project_identity_at(&cwd)
}

/// The project identity, anchored at an explicit `root` rather than the process cwd. In
/// precedence order (spec 09): the tracked `.rigger/project.id` file when present, else the
/// legacy basename identity ([`legacy_identity_at`]). Never empty.
///
/// The tracked id file survives directory renames, machine moves, and shared backends - a
/// `mv` of the checkout no longer orphans the project's history, because the identity is a
/// committed line, not the volatile directory basename (Gap 20). A pre-spec-09 checkout with
/// no `project.id` behaves EXACTLY as before (the legacy basename), until `rigger init`/
/// `setup` mints the file, so backward compatibility is a hard bar.
///
/// The id file is resolved relative to the git top-level (where `.rigger` conventionally
/// lives, like `.git`), so it is found no matter which subdirectory the command ran from,
/// falling back to `root` itself outside any git context.
///
/// Anchoring at an explicit root is load-bearing for the store-opening couriers. When a
/// courier walks UP from a git-linked worktree nested inside the repo (the Gap-14 default
/// scratch root `<repo>/.rigger/tmp/...`) to the repo's real store, `git rev-parse
/// --show-toplevel` run from the cwd returns the LINKED-WORKTREE path, so the append would
/// misfile under `proj-<worktree>-run` while the spawn the conductor is waiting on stays
/// parked forever (spec 05's exact charter defect). Running git anchored at the resolved
/// store root instead returns the repo root, so it reads THAT root's `project.id` first and
/// the write lands in the `proj-<repo>-run` stream the conductor reads - identical to the
/// identity the conductor computed when it created that store from the same root.
fn project_identity_at(root: &Path) -> String {
    let toplevel = git_repo_at(root);
    let base: &Path = if toplevel.is_empty() {
        root
    } else {
        Path::new(&toplevel)
    };
    if let Some(id) = read_project_id(base) {
        return id;
    }
    legacy_identity_from(&toplevel, root)
}

/// The LEGACY basename identity, anchored at an explicit `root`: the basename of the git
/// top-level containing `root`, falling back to `root`'s own basename, then to "rigger".
/// Never empty. This is the pre-spec-09 behavior, unchanged - it is what identity resolves
/// to when no `.rigger/project.id` is present, and the "before" namespace the spec-09
/// migration renames a project's history AWAY from once the file is minted.
fn legacy_identity_at(root: &Path) -> String {
    legacy_identity_from(&git_repo_at(root), root)
}

/// The legacy basename identity given an already-resolved git `toplevel` (empty outside a
/// repo) and the `root` it was resolved from - so [`project_identity_at`] resolves the git
/// top-level exactly once and reuses it for the fallback.
fn legacy_identity_from(toplevel: &str, root: &Path) -> String {
    let from_repo = Path::new(toplevel)
        .file_name()
        .and_then(|n| n.to_str())
        .filter(|s| !s.is_empty());
    if let Some(name) = from_repo {
        return name.to_string();
    }
    root.file_name()
        .and_then(|n| n.to_str())
        .map(String::from)
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "rigger".to_string())
}

/// The trimmed contents of the tracked `<base>/.rigger/project.id`, or `None` when the file
/// is absent, unreadable, or blank. A present, non-empty line IS the project identity
/// (spec 09): clones and checkouts inherit it through git, so one logical project shares a
/// single namespace across machines and paths.
fn read_project_id(base: &Path) -> Option<String> {
    let path = base.join(RIGGER_DIR).join(PROJECT_ID_FILE);
    let raw = std::fs::read_to_string(path).ok()?;
    let id = raw.trim();
    if id.is_empty() {
        None
    } else {
        Some(id.to_string())
    }
}

/// Whether the tracked `.rigger/project.id` is present (and non-blank) for the project at
/// `root`, resolved relative to the git top-level (else `root`) - the same anchoring
/// [`project_identity_at`] uses. `false` means identity falls back to the volatile basename,
/// which `rigger validate` surfaces as a rename-orphans-history hazard.
fn has_tracked_project_id(root: &Path) -> bool {
    let toplevel = git_repo_at(root);
    let base: &Path = if toplevel.is_empty() {
        root
    } else {
        Path::new(&toplevel)
    };
    read_project_id(base).is_some()
}

/// A stable, deterministic 64-bit FNV-1a hash. The project id derived from a remote must
/// be the SAME on every clone, machine, and rigger version, so this uses the fixed FNV
/// constants rather than `std::collections::hash_map::DefaultHasher` (whose output is
/// explicitly NOT guaranteed stable across builds).
fn fnv1a_64(bytes: &[u8]) -> u64 {
    const OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0000_0100_0000_01b3;
    let mut hash = OFFSET;
    for &b in bytes {
        hash ^= b as u64;
        hash = hash.wrapping_mul(PRIME);
    }
    hash
}

/// Canonicalize definition text for hashing (spec 13, unit 1): normalize CRLF -> LF and
/// strip trailing whitespace from each line, so a checkout's line-ending or trailing-space
/// noise never reads as a definition change while any real edit does.
fn canonical_definition_text(s: &str) -> String {
    s.replace("\r\n", "\n")
        .lines()
        .map(str::trim_end)
        .collect::<Vec<_>>()
        .join("\n")
}

/// The definition hash a run PINS (spec 13, unit 1): a stable FNV-1a digest over the on-disk
/// definition - the `.rigger/workflow.yml` plus the FULL agent-prompt set (every
/// `.rigger/agents/*.md`, which carries each agent's prompt and frontmatter) - canonicalized
/// ([`canonical_definition_text`]) and folded in sorted-filename order. So the same definition
/// hashes identically across machines and checkouts (the [`fnv1a_64`] idiom is fixed-seed and
/// build-stable), while ANY content change - a mid-campaign prompt edit above all - changes it.
///
/// This is the hash a run pins at start and a live-run step re-checks; a mismatch on a live run
/// HALTS loudly (see [`enforce_definition_pin`]). Hashing the on-disk files directly (not the
/// parsed `Config`) is faithful to the design's "workflow.yml + the full agent-prompt set" and
/// conservative: it needs no serialization of the config and errs toward halting, and the
/// `--rebase-definition` escape makes an intended edit a one-flag continue.
fn definition_hash(dir: &str) -> Result<String, Box<dyn std::error::Error>> {
    let base = Path::new(dir).join(RIGGER_DIR);
    let mut buf = String::new();
    // workflow.yml first, tagged so an (impossible-here) empty agents set is still distinct
    // from an empty workflow.
    let workflow = std::fs::read_to_string(base.join("workflow.yml"))
        .map_err(|e| format!("definition hash: read {RIGGER_DIR}/workflow.yml: {e}"))?;
    buf.push_str("workflow.yml\n");
    buf.push_str(&canonical_definition_text(&workflow));
    buf.push('\n');
    // Every agent definition, folded in sorted-filename order so the hash is independent of
    // directory iteration order.
    let agents_dir = base.join("agents");
    let mut agents: Vec<(String, String)> = Vec::new();
    for entry in std::fs::read_dir(&agents_dir)
        .map_err(|e| format!("definition hash: read {}: {e}", agents_dir.display()))?
    {
        let path = entry?.path();
        if path.extension().and_then(|x| x.to_str()) != Some("md") {
            continue;
        }
        let name = path
            .file_name()
            .and_then(|x| x.to_str())
            .unwrap_or_default()
            .to_string();
        let content = std::fs::read_to_string(&path)
            .map_err(|e| format!("definition hash: read {}: {e}", path.display()))?;
        agents.push((name, content));
    }
    agents.sort();
    for (name, content) in agents {
        buf.push_str("agent:");
        buf.push_str(&name);
        buf.push('\n');
        buf.push_str(&canonical_definition_text(&content));
        buf.push('\n');
    }
    Ok(format!("{:016x}", fnv1a_64(buf.as_bytes())))
}

/// Enforce the run's definition pin (spec 13, unit 1) at the CLI boundary, BEFORE the
/// conductor drives: adopt-or-mint the run for `criteria` with `definition` pinned, and act on
/// the outcome. A fresh or unchanged run continues silently; `--rebase-definition` on a drifted
/// run records the supersession and continues with a notice; a drifted run WITHOUT the flag
/// returns a loud error, so `rigger step`/`run` HALTS naming the drift instead of driving a
/// campaign whose replay semantics silently changed. The conductor's own (unpinned)
/// `ensure_started` then simply ADOPTS the run this ensured.
///
/// `base` is the resolved run-branch base to persist on a freshly-minted RunStarted (spec 38,
/// criterion 3), so `rigger status`/`rigger dash` later read the run's actual base from the
/// log rather than re-resolving without the run's `--base` flag. On an ADOPTED (resumed) run
/// it is ignored - the base its original start stamped stands.
fn enforce_definition_pin(
    store: &dyn EventStore,
    criteria: &[String],
    definition: &str,
    rebase: bool,
    base: &str,
) -> Res {
    match runscope::ensure_started_pinned(store, criteria, definition, rebase, base)? {
        runscope::RunStart::Ready(_) => Ok(()),
        runscope::RunStart::Rebased {
            run,
            pinned,
            current,
        } => {
            eprintln!(
                "rigger: --rebase-definition: recorded the definition supersession \
                 ({pinned} -> {current}) on run {run}; continuing on the new definition."
            );
            Ok(())
        }
        runscope::RunStart::Drifted {
            run,
            pinned,
            current,
        } => Err(format!(
            "definition drift - the on-disk workflow/agent definition (hash {current}) differs \
             from the hash run {run} pinned at start ({pinned}). A live run pins its definition so \
             replay semantics cannot silently change mid-campaign. Re-run with --rebase-definition \
             to record the supersession ({pinned} -> {current}) and continue, or restore the \
             definition to match the pin."
        )
        .into()),
    }
}

/// Canonicalize a git remote URL so the ssh, https, and `.git`-suffixed forms of ONE repo
/// all reduce to the SAME string (spec 09): strip the scheme (`https://`, `ssh://`,
/// `git://`) and any `user@` credential, lowercase the host, drop a trailing `.git` and
/// surrounding slashes, and normalize the scp-style `host:path` separator to `/`. So
/// `git@github.com:Acme/Repo.git`, `https://github.com/Acme/Repo.git`, and
/// `ssh://git@github.com/Acme/Repo` all normalize to `github.com/Acme/Repo`, minting one
/// identity. Pure, so the "ssh/https/.git forms mint identical ids" invariant is unit-tested.
fn normalize_origin_url(url: &str) -> String {
    let mut s = url.trim();
    // Strip the scheme (everything up to and including "://").
    if let Some(idx) = s.find("://") {
        s = &s[idx + 3..];
    }
    // Strip any "user@" credential prefix (e.g. the ssh `git@`).
    if let Some(idx) = s.find('@') {
        s = &s[idx + 1..];
    }
    // Split the host from the path on the first ':' (scp-style) or '/'.
    let (host, path) = match s.find([':', '/']) {
        Some(i) => (&s[..i], &s[i + 1..]),
        None => (s, ""),
    };
    let host = host.to_ascii_lowercase();
    // Drop surrounding slashes and a single trailing `.git` from the path.
    let path = path.trim_matches('/');
    let path = path.strip_suffix(".git").unwrap_or(path);
    let path = path.trim_end_matches('/');
    if path.is_empty() {
        host
    } else {
        format!("{host}/{path}")
    }
}

/// The `origin` remote URL configured at `root`, or `None` when there is no `origin` remote
/// (or git is unavailable). Read via `git config --get remote.origin.url`, which needs no
/// network and no newer git than the rest of rigger already assumes.
fn origin_url_at(root: &Path) -> Option<String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["config", "--get", "remote.origin.url"])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let url = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if url.is_empty() {
        None
    } else {
        Some(url)
    }
}

/// Mint a fresh durable project id for `root` (spec 09): deterministically from the
/// normalized `origin` URL when a remote exists (so every clone of one repo mints the same
/// id, and the ssh/https/`.git` forms agree), else a random id when there is no remote to
/// anchor on. The result is a compact hex token, safe as a stream-namespace component.
fn mint_project_id(root: &Path) -> String {
    match origin_url_at(root) {
        Some(url) => format!("{:016x}", fnv1a_64(normalize_origin_url(&url).as_bytes())),
        None => uuid::Uuid::new_v4().simple().to_string(),
    }
}

/// What the spec-09 open-time identity migration should do, given whether each namespace
/// holds history. Pure over the two facts, so the decision is unit-testable without a store.
#[derive(Debug, PartialEq, Eq)]
enum MigrationOutcome {
    /// Nothing to migrate: no minted identity distinct from the basename, already migrated
    /// (minted populated), or a fresh project (both empty).
    NoOp,
    /// Legacy history with an empty minted namespace: rename the legacy streams once.
    Rename,
    /// BOTH namespaces hold history: ambiguous, refuse loudly (never guess).
    Ambiguous,
}

/// Decide the migration from the minted vs legacy identities and whether each namespace is
/// populated (spec 09). When the minted identity is not distinct from the legacy basename
/// (no `project.id`, or it equals the basename) there is nothing to migrate. Otherwise the
/// only case that renames is legacy-populated + minted-empty; a populated minted namespace
/// means it already migrated (or is a fresh mint), and both populated is ambiguous.
fn decide_migration(
    minted: &str,
    legacy: &str,
    minted_has: bool,
    legacy_has: bool,
) -> MigrationOutcome {
    if minted == legacy {
        return MigrationOutcome::NoOp;
    }
    match (legacy_has, minted_has) {
        (true, true) => MigrationOutcome::Ambiguous,
        (true, false) => MigrationOutcome::Rename,
        _ => MigrationOutcome::NoOp,
    }
}

/// Perform the one-time spec-09 identity migration on an already-opened sqlite `backend`,
/// renaming a project's legacy-namespace history to the `minted` identity and recording the
/// move as a `DecisionMade` (no new event types). Returns `Some(n)` with the stream count
/// when it migrated, `None` when there was nothing to do (idempotent on re-open), and an
/// `Err` naming BOTH identities when the store is ambiguous (history under both namespaces).
/// Takes the identities as arguments so it is unit-testable against an in-memory store.
fn migrate_project_identity(
    backend: &Store,
    minted: &str,
    legacy: &str,
    graph: Option<&Projector>,
) -> Result<Option<usize>, Box<dyn std::error::Error>> {
    let legacy_ns = format!("proj-{legacy}-");
    let minted_ns = format!("proj-{minted}-");
    let legacy_has = backend.has_stream_prefix(&legacy_ns)?;
    let minted_has = backend.has_stream_prefix(&minted_ns)?;
    match decide_migration(minted, legacy, minted_has, legacy_has) {
        MigrationOutcome::NoOp => Ok(None),
        MigrationOutcome::Ambiguous => Err(format!(
            "ambiguous project identity: the event store holds history under BOTH the minted \
             identity {minted:?} and the legacy identity {legacy:?}. Refusing to guess which \
             is authoritative - resolve it manually (keep one namespace) before running again."
        )
        .into()),
        MigrationOutcome::Rename => {
            // Re-key the graph the SAME way the streams are renamed (spec 28 GC5 backward-compat):
            // the migration renames event streams, but the graph folds incrementally so the
            // renamed streams are never re-folded - its pre-mint rows keep the legacy scope and,
            // once the read filter scopes reads to the minted identity, that history would be
            // silently orphaned. Re-scope the graph rows legacy -> minted so a single-project
            // deployment reads EXACTLY as before across the mint. Skipped when no graph is wired
            // (a store-only unit test).
            //
            // ORDER MATTERS for crash-safety: the graph and the streams live in two separate
            // databases with no shared transaction, so re-key the graph FIRST and rename the
            // streams LAST. `decide_migration` returns `Rename` ONLY while the legacy namespace
            // still holds streams, and `rename_stream_prefix` is the SOLE step that clears it - so
            // the stream rename is the irreversible commit point. If the process dies (or the
            // re-key errors: a composite (id, project) collision, or a locked shared backend) after
            // the re-key but before the rename, the legacy namespace is still populated, a re-open
            // decides `Rename` again, and the idempotent re-key (which moves 0 rows once done)
            // replays cleanly to completion. Renaming first would empty the legacy namespace, so a
            // failed re-key would NoOp forever and orphan the pre-mint graph rows. Re-keying first
            // also keeps the minted graph scope empty until the DecisionMade fold below, so the
            // composite (id, project) key never collides.
            if let Some(g) = graph {
                g.migrate_project(legacy, minted)?;
            }
            let n = backend.rename_stream_prefix(&legacy_ns, &minted_ns)?;
            // Record the migration as a DecisionMade in the MINTED namespace (spec 09: the
            // migration is recorded with the existing DecisionMade, NO new event type) - old
            // identity, new identity, and stream count - so the audit trail carries it and a
            // re-open finds the legacy namespace already empty (a no-op).
            let store = Namespaced::new(backend, minted);
            let data = serde_json::json!({
                "id": format!("identity-migration-{minted}"),
                "summary": format!(
                    "migrated project history to the durable identity: renamed {n} stream(s) \
                     from the legacy namespace {legacy:?} to the minted identity {minted:?} \
                     (.rigger/{PROJECT_ID_FILE})"
                ),
                "governs": [format!("{RIGGER_DIR}/{PROJECT_ID_FILE}")],
            });
            let args = serde_json::json!({
                "type": contextgraph::TYPE_DECISION_MADE,
                "data": data,
            });
            mcpserver::emit_event(
                &store,
                conductor::STREAM,
                graph.map(|g| g as &dyn Projection),
                &args,
            )
            .map_err(|e| -> Box<dyn std::error::Error> { e.into() })?;
            Ok(Some(n))
        }
    }
}

/// Run the spec-09 open-time identity migration against the LOCAL sqlite store
/// (`.rigger/events.db` under the cwd), before the run driver opens its own backend. A
/// no-op when there is no local store yet (a fresh project), or when the minted identity is
/// not distinct from the legacy basename (no `project.id` minted). Refuses loudly (Err) when
/// both namespaces hold history. Self-contained: it opens its own short-lived store + graph
/// connections and drops them before the caller opens the real ones, so it wires into any
/// run-driver entry point in a single call and never touches the injected backend.
fn migrate_local_identity() -> Res {
    let cwd = std::env::current_dir()?;
    migrate_identity_at(&StoreLocation {
        dir: cwd.join(RIGGER_DIR),
    })
}

/// The spec-09 open-time identity migration against an ALREADY-RESOLVED store
/// ([`StoreLocation`]) - the one implementation [`migrate_local_identity`] is the cwd-anchored
/// entry to.
///
/// It is anchored at the store's OWNING ROOT (the parent of the resolved `.rigger/`), the same
/// anchor [`StoreLocation::identity`] binds the namespace to, so the migration and the streams it
/// renames can never be computed from two different roots. A command that has already resolved
/// which store it is about to touch - a courier, or a maintenance prune walked up from a nested
/// worktree - calls THIS rather than re-deriving the store from the process cwd, which is how a
/// walked-up command would otherwise migrate one store and mutate another.
fn migrate_identity_at(loc: &StoreLocation) -> Res {
    let store_path = loc.file("events.db");
    if !Path::new(&store_path).is_file() {
        return Ok(()); // a fresh project: no history to migrate
    }
    // ONE root for both halves of the comparison. A `.rigger` with no parent is pathological (the
    // resolved dir is always `<root>/.rigger`), and it falls back to the cwd exactly as
    // [`StoreLocation::identity`] does, so the two can never disagree about which root they mean.
    let root = match loc.dir.parent() {
        Some(root) => root.to_path_buf(),
        None => std::env::current_dir()?,
    };
    let minted = project_identity_at(&root);
    let legacy = legacy_identity_at(&root);
    if minted == legacy {
        return Ok(()); // no minted identity distinct from the basename
    }
    let backend = open_sqlite_store(&store_path)?;
    let graph = Projector::open(&loc.file("graph.db"), &minted)?;
    if let Some(n) = migrate_project_identity(&backend, &minted, &legacy, Some(&graph))? {
        eprintln!(
            "rigger: migrated project identity - renamed {n} stream(s) from the legacy \
             namespace {legacy:?} to the minted identity {minted:?} (.rigger/{PROJECT_ID_FILE})"
        );
    }
    Ok(())
}

/// The canonical command surface, in dispatch order. This is the SINGLE list of
/// subcommand names: the runtime reads it (the unknown-command help below names the
/// known commands from it) and `rigger docs` reads it so the generated discipline's
/// command list is code-derived, not hand-copied. Keep it in step with the `main`
/// dispatch match below - the same must-agree discipline [`RUN_BRANCH`] keeps with the
/// JS driver. `docs_context` and the `commands_registry_agrees_with_dispatch` test guard
/// it against drift.
const SUBCOMMANDS: &[&str] = &[
    "run",
    "step",
    "reported",
    "prompt",
    "serve",
    "workflow",
    "graph",
    "stats",
    "canary",
    "playbooks",
    "replay",
    "status",
    "dash",
    "watch",
    "ground",
    "reindex",
    "symbols-index",
    "emit",
    "progress",
    "result",
    "peers",
    "reset",
    "validate",
    "init",
    "setup",
    "docs",
    "prime",
    "version",
    "help",
];

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        usage();
        std::process::exit(2);
    }
    let result = match args[1].as_str() {
        "run" => cmd_run(&args[2..]),
        "step" => cmd_step(&args[2..]),
        "reported" => cmd_reported(&args[2..]),
        "prompt" => cmd_prompt(&args[2..]),
        "serve" => cmd_serve(&args[2..]),
        "workflow" => cmd_workflow(&args[2..]),
        "graph" => cmd_graph(&args[2..]),
        "stats" => cmd_stats(&args[2..]),
        "canary" => cmd_canary(&args[2..]),
        "playbooks" => cmd_playbooks(&args[2..]),
        "replay" => cmd_replay(&args[2..]),
        "status" => cmd_status(&args[2..]),
        "dash" => cmd_dash(&args[2..]),
        "watch" => cmd_watch(&args[2..]),
        "ground" => cmd_ground(&args[2..]),
        "reindex" => cmd_reindex(&args[2..]),
        "symbols-index" => cmd_symbols_index(&args[2..]),
        "emit" => cmd_emit(&args[2..]),
        "progress" => cmd_progress(&args[2..]),
        "result" => cmd_result(&args[2..]),
        "peers" => cmd_peers(&args[2..]),
        "reset" => cmd_reset(&args[2..]),
        "validate" => cmd_validate(&args[2..]),
        "init" => cmd_init(),
        "setup" => cmd_setup(&args[2..]),
        "docs" => cmd_docs(&args[2..]),
        "prime" => cmd_prime(),
        "version" | "--version" | "-V" => cmd_version(),
        "help" | "-h" | "--help" => {
            usage();
            Ok(())
        }
        other => {
            eprintln!("rigger: unknown command {other:?}");
            eprintln!("known commands: {}", SUBCOMMANDS.join(", "));
            usage();
            std::process::exit(2);
        }
    };

    let code = match result {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("rigger: {e}");
            1
        }
    };

    std::process::exit(code);
}

fn usage() {
    eprint!(
        "rigger - a config-driven, event-sourced multi-agent dev-loop harness\n\n\
usage:\n  \
rigger run [spec] [opts]    run the workflow (opts below)\n  \
rigger step [--spec <path>]      advance the run one frontier via the replay driver\n            \
[--base <ref>]        and print the newly parked spawn wave + a done flag\n                              \
as JSON. --base (default origin/main) anchors a NEW run\n                              \
branch; if it is unresolvable the branch is created off\n                              \
HEAD. An existing run branch is reused, never reset.\n                              \
--fresh begins a NEW run for the spec even if the latest\n                              \
matches (pass on the first step to restart a wedged run);\n                              \
--rebase-definition accepts a drifted definition and\n                              \
continues, else a live-run step HALTS on definition drift\n  \
rigger reported <id>        exit 0 iff spawn <id> already has a recorded result in\n                              \
this project's run stream (else non-zero). A read-only check\n                              \
of whether a spawn reported yet; the death courier records\n                              \
atomically instead via `rigger result --if-absent`\n  \
rigger prompt <id>          print the parked spawn's full prompt (persona + task).\n                              \
The step wave is a slim manifest; each worker fetches its\n                              \
own prompt from the log by spawn id (spawn-by-reference)\n  \
rigger workflow [spec]      turn-key: launch the per-project Node driver, which\n                              \
spawns `rigger serve`, runs each agent via the Agent\n                              \
SDK, and drives the loop (one command; run `rigger\n                              \
setup` first - it provisions the driver in .rigger/shim/)\n  \
rigger serve [opts]         run as an MCP server the driver connects to\n  \
rigger graph --around <id>  print the context subgraph around a node\n  \
rigger graph --show <entity> print an entity's definition site + line-numbered body\n                              \
(the text half of lookup; resolves a full id or a bare name)\n  \
rigger graph build          fold the project's source into the graph from a cold\n                              \
checkout (no run required)\n  \
rigger graph communities    derive the code lens's coupling communities offline and\n                              \
[--resolution <r>]          record them as events (deterministic; default r=1.0)\n  \
rigger graph concepts       derive the concepts lens's intent-layer grouping offline\n                              \
[--resolution <r>]          and record them as events (deterministic; default r=1.0)\n  \
rigger stats                print the run's operator metrics: first-pass yield,\n                              \
per-gate remediation counts, escalation rate, and\n                              \
review approve/reject counts. --canary reports the\n                              \
latest canary run's judge-the-judges recall scorecard\n  \
rigger canary               run the review panel against the seeded-defect corpus\n            \
[--corpus <dir>]         (default ./canaries) and score per-tier catch rate,\n                              \
adjudicator correctness, and verdict stability under\n                              \
finding-order shuffle, into the project's canary stream\n                              \
(read back with `rigger stats --canary`)\n  \
rigger playbooks --rebuild  reconstruct the distilled playbook pool under\n                              \
.rigger/playbooks/ from the recorded LessonLearned\n                              \
stream: deduplicated, trigger-scoped agent-files the\n                              \
lessons injector ranks by blast-radius relevance (a\n                              \
rebuildable projection of the log, never hand-edited)\n  \
rigger replay <run|latest>  re-drive a completed run's recorded trajectory under a\n            \
--against <rev>          candidate config (workflow + prompts at git <rev>) in an\n                              \
isolated scratch namespace, and print the stats diff\n                              \
vs the recorded baseline. Never writes the real run\n                              \
stream - past runs become a regression corpus for a\n                              \
config edit (\"did that change regress first-pass yield?\")\n  \
rigger status [--json]      present the live per-agent view of the current run: for\n                              \
each in-flight agent, what it is doing (latest progress),\n                              \
its heartbeat age, and how long since its last store event\n                              \
(the blackout). --json prints the shim/dash machine shape\n  \
rigger dash [--port <n>]    serve the read-only observability page on 127.0.0.1\n                              \
(default port 7420) with live past/present/future views;\n                              \
--export <path> writes the equivalent static snapshot\n  \
rigger watch [--interval <s>] the driver-independent watchdog: polls the store,\n            \
[--once]                  process table, and status for the five rigger-watch-a-run\n                              \
signals (escalated blockers, heartbeat staleness, dash\n                              \
liveness, reject-recurrence trend, frontier progress) plus\n                              \
store integrity, printing one line per anomaly naming\n                              \
signal, subject, and response skill. --once prints standing\n                              \
anomalies and exits (cron/CI); default streams (poll every\n                              \
180s, dedup'd) and never talks to the driver - it works\n                              \
with the driver dead\n  \
rigger ground <query> [k]   print up to k (default 8) repo references the project's\n                              \
configured grounder finds for <query>, as `file:line: text`\n  \
rigger reindex <file>...    incrementally re-index the named files in the project's\n                              \
persisted grounding index (the grounder's reindex), so a\n                              \
later `rigger ground` reflects just-landed changes\n  \
rigger symbols-index [dir]  build + persist the structural symbol index over [dir]\n                              \
(default .) and print its path + file count - the fresh-\n                              \
process determinism harness for the symbols grounder (spec 15)\n  \
rigger emit <type> <json>   append {{type, data:<json>}} to the event store and fold\n                              \
it into the context graph (the CLI form of rigger_emit)\n  \
rigger progress <id> <act>  record one live progress line for spawn <id> to the\n                              \
separate .rigger/progress.db (never the run stream), so an\n                              \
observer can see what a working agent is doing between\n                              \
milestones - `rigger status` and the dash present it\n  \
rigger result <id> [out]    record a parked spawn's outcome to the run log so the next\n                              \
step advances past it: <out> (or stdin) is the agent's output\n                              \
(with --error, its failure message); --if-absent records only\n                              \
if the id has no result; --meta <json> adds bookkeeping\n  \
rigger peers [file ...]     print peer decisions, lessons, and findings from the\n                              \
context graph, scoped to the given files (the CLI form of\n                              \
rigger_peers)\n  \
rigger reset --runs         drop every superseded / dead run's decisions and\n                              \
findings from the context graph, keeping every lesson and\n                              \
the active run's own decisions/findings. Sheds dead-run\n                              \
grounding noise without wiping the store: it deletes no\n                              \
event. reset itself does write the log once, on a store\n                              \
still under the legacy basename namespace: the one-time\n                              \
identity migration renames those streams and records one\n                              \
DecisionMade before either mode prunes\n  \
rigger reset --derived      compact the EVENT LOG: keep the latest event per\n                              \
replay key of each derived index type, delete the\n                              \
superseded re-recordings, and vacuum so the file shrinks\n                              \
on disk. Every other event survives. Sheds the\n                              \
duplication a log accreted before the ingest dedup;\n                              \
composes with --runs (each prunes its own accumulation).\n                              \
Refuses while run machinery looks live (a held step\n                              \
lock, a non-terminal unit, an in-flight spawn, or a live\n                              \
driver registration), naming what is live: compaction\n                              \
leaves revision gaps by design, and a stale writer can\n                              \
reissue one and reorder the log - the corruption this\n                              \
guard exists to prevent. --force-live skips the check\n                              \
entirely and checks nothing: pass it only once you are\n                              \
certain no writer is using this store, since forcing\n                              \
past a genuinely live writer is exactly that corruption\n  \
rigger validate             load and validate the workflow + agents\n  \
rigger init                 set up a project: scaffold .rigger/ (workflow.yml +\n                              \
an agents/ folder) and install the Claude Code\n                              \
SessionStart hook (it runs `rigger prime`)\n  \
rigger setup                full setup: everything `init` does, PLUS install the\n                              \
native /rigger Claude Code workflow (.claude/workflows/\n                              \
rigger.js) and provision the JS driver (.rigger/shim/ +\n                              \
npm install). After it: run `/rigger <spec>` in Claude\n                              \
Code (primary), or `rigger workflow` as a fallback\n  \
rigger prime                print recent decisions (what the hook runs)\n  \
rigger version              print the crate version and the build-provenance id\n                              \
(a git commit/describe embedded at build time) so an\n                              \
agent can identify the exact binary. Also `--version`\n\n\
run/serve options:\n  \
--driver <cli|workflow>          cli (default): standalone claude subprocess;\n                                   \
workflow: in-Claude-Code MCP server\n  \
--eventstore <sqlite|kurrentdb>  sqlite (default): embedded file in .rigger/;\n                                   \
kurrentdb: shared server backend, always available\n  \
--conn <url>                     KurrentDB connection url (or set KURRENTDB_CONN)\n  \
--fresh                          begin a NEW run even if the latest run matches this\n                                   \
spec (which is otherwise adopted/resumed). The evented\n                                   \
restart for a run wedged in a terminal state (e.g. an\n                                   \
escalated plan-critique) whose spec is unchanged; the\n                                   \
prior run stays in the log as history and context\n  \
--rebase-definition              accept an on-disk definition (workflow.yml + agent\n                                   \
prompts) that drifted from what this live run pinned at\n                                   \
start: record the supersession and continue instead of\n                                   \
halting. The explicit mid-campaign-edit escape (a live\n                                   \
run otherwise HALTS loudly on definition drift)\n\n\
storage and graph live in ./.rigger/ (per project, like .git/), scoped to the\n\
project identity so one backend can hold many projects without their data mixing.\n"
    );
}

fn db_path(name: &str) -> String {
    Path::new(RIGGER_DIR)
        .join(name)
        .to_string_lossy()
        .into_owned()
}

/// The bounded store walk's outcome: the CHOSEN store (the OUTERMOST `.rigger/events.db`
/// within scope) and any NEARER shadow stores it bypassed (nearest first).
///
/// Outermost wins (spec 08 item 6): a courier deep in the tree - inside a unit worktree or
/// an agent-scratch dir that happens to carry its own `.rigger/events.db` - must bind the
/// repo root's REAL run stream, never a nearer shadow that would eclipse it. So the walk
/// does not stop at the first store it finds; it collects every store in scope and keeps
/// the OUTERMOST, recording the bypassed nearer ones so the caller can warn (naming both).
struct StoreWalk {
    /// The `.rigger` dir of the OUTERMOST store in scope, or `None` when scope holds none.
    dir: Option<PathBuf>,
    /// The `.rigger` dirs of NEARER stores bypassed in favor of `dir` (nearest first);
    /// empty unless a shadow was eclipsed.
    shadows: Vec<PathBuf>,
}

/// Walk up from `start` (inclusive) collecting every `.rigger/events.db` in scope, and
/// return the OUTERMOST as the chosen store together with any nearer shadows it bypassed
/// (see [`StoreWalk`]).
///
/// The walk is BOUNDED at the main-repo root governing `start` (the parent of its git
/// common dir): the sanctioned walk-up case is a courier inside a nested git worktree
/// of THIS project, and an unbounded walk lets a courier in a storeless nested repo (an
/// agent-scratch probe under `<repo>/.rigger/tmp`, say) bind to a PARENT project's
/// store and write into a foreign run stream with exit-0 success (adversary finding
/// adv9-walkup-cross-project, empirically proven). Outside any git context there is no
/// sanctioned walk at all: only `start` itself counts. This unit changes only WHICH store
/// within that unchanged scope is chosen (the outermost, not the nearest), never the
/// boundary itself (landed unit-9 behavior).
fn walk_stores_from(start: &Path) -> StoreWalk {
    let boundary = main_repo_root(start);
    let mut found: Vec<PathBuf> = Vec::new();
    let mut cur = Some(start);
    while let Some(dir) = cur {
        let rigger = dir.join(RIGGER_DIR);
        if rigger.join("events.db").is_file() {
            found.push(rigger);
        }
        match &boundary {
            Some(root) if dir == root => break, // reached the sanctioned bound (inclusive)
            None => break,                      // no git context: only `start` counts
            _ => {}
        }
        cur = dir.parent();
    }
    // `found` is nearest-first, so the LAST entry is the outermost store in scope; the
    // earlier (nearer) ones are the bypassed shadows, kept nearest-first for the warning.
    let dir = found.pop();
    StoreWalk {
        dir,
        shadows: found,
    }
}

/// The OUTERMOST store directory within the bounded walk scope from `start`, or `None`
/// when scope holds none. Thin wrapper over [`walk_stores_from`] for the read-only callers
/// (residue/validate) that only need the chosen store, not the bypassed-shadow report.
fn find_store_dir_from(start: &Path) -> Option<PathBuf> {
    walk_stores_from(start).dir
}

/// The MAIN repo root governing `start`: the parent of `git rev-parse --git-common-dir`
/// run from `start`. For a linked worktree the common dir is the main repo's `.git`, so
/// this resolves to the main checkout's root - exactly the outermost directory the
/// store walk-up is sanctioned to reach. `None` when `start` is not inside any git repo.
fn main_repo_root(start: &Path) -> Option<PathBuf> {
    let out = std::process::Command::new("git")
        .arg("-C")
        .arg(start)
        .args(["rev-parse", "--git-common-dir"])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let common = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if common.is_empty() {
        return None;
    }
    let common_path = Path::new(&common);
    let abs = if common_path.is_absolute() {
        common_path.to_path_buf()
    } else {
        start.join(common_path)
    };
    abs.parent().map(|p| p.to_path_buf())
}

/// A resolved rigger store, as a store-opening COURIER (`emit`/`result`/`peers`/
/// `reported`) must see it: the `.rigger` directory that actually holds the store (found
/// by walking UP from the cwd, never fabricated), together with the identity that scopes
/// its namespaced streams - bound to the store's OWNING ROOT, not the process cwd.
///
/// Binding identity to the owning root is the whole point of this type. Walking up already
/// finds the real store file when a courier runs from a nested git worktree; but the
/// STREAM the write lands in is chosen by the identity, and `project_identity()` reads the
/// cwd's git top-level, which inside a git-linked worktree is the WORKTREE path (basename
/// `rigger-wt-...`), not the repo. So a walked-up write would silently misfile under
/// `proj-<worktree>-run` while the conductor keeps reading `proj-<repo>-run` - the spawn
/// stays parked (spec 05's charter defect). [`identity`](Self::identity) anchors identity
/// at the resolved root instead, so the write lands in the stream the conductor reads.
struct StoreLocation {
    /// The `.rigger` store directory (`<root>/.rigger`) resolved by walking up the cwd.
    dir: PathBuf,
}

impl StoreLocation {
    /// A store file path (`events.db` / `graph.db`) under the resolved `.rigger/`, as the
    /// `&str` the sqlite `Store` / `Projector` opens.
    fn file(&self, name: &str) -> String {
        store_file(&self.dir, name)
    }

    /// The identity scoping this store's namespaced streams, bound to the store's OWNING
    /// ROOT (the parent of the resolved `.rigger/`), NOT the process cwd - so a courier
    /// walked up from a nested git worktree records into the same `proj-<repo>-run` stream
    /// the conductor reads, never a `proj-<worktree>-run` misfile (spec 05).
    fn identity(&self) -> String {
        match self.dir.parent() {
            Some(root) => project_identity_at(root),
            // A `.rigger` with no parent is pathological (the resolved dir is always
            // `<root>/.rigger` from an absolute cwd); fall back to the cwd-anchored identity.
            None => project_identity(),
        }
    }
}

/// The [`StoreLocation`] a SERVER-backed project resolves to, anchored at `cwd`. The server
/// holds ONE remote store, so there is no local `events.db` to walk to: identity binds to the
/// OWNING root (the main repo root - correct even from a nested worktree, exactly as the sqlite
/// walk's identity does), and [`resolve_store`] reaches the server by its connection string (the
/// resolved dir's `events.db` path is ignored for the server backend). This is the ONE
/// server-location authority every server-backed store access shares - the store-opening couriers
/// ([`require_store_dir`]) that WRITE and the residue scan's run-liveness read ([`read_run_units`])
/// that READS - so a server-configured project resolves the SAME store from every path (spec 48,
/// "one resolution authority"), never a second parallel mapping that could drift.
fn server_store_location(cwd: &Path) -> StoreLocation {
    let dir = main_repo_root(cwd)
        .unwrap_or_else(|| cwd.to_path_buf())
        .join(RIGGER_DIR);
    StoreLocation { dir }
}

/// Resolve the `.rigger` store a store-opening COURIER command (`emit`/`result`/`peers`/
/// `reported`) must use, REFUSING rather than fabricating a fresh empty store when neither
/// the current directory nor any ancestor holds one (spec 05, done-when: "store-opening
/// commands refuse (or walk up) instead of fabricating a fresh `.rigger/events.db` when run
/// from a cwd with no existing store").
///
/// The defect this closes: a courier run from the WRONG cwd - most plausibly a unit
/// worktree, which carries the tracked `.rigger/workflow.yml` + agents but NOT the
/// machine-local, gitignored `.rigger/events.db` - used to `create_dir_all(.rigger)` +
/// `Store::open` a brand-new empty store there, record into that dead store, and print
/// success while the real spawn stayed parked forever in the project's actual run stream.
/// Walking up finds the real store when the cwd is a SUBDIRECTORY (or a nested worktree) of
/// the project root; refusing (when no ancestor has one) surfaces the wrong-cwd mistake
/// instead of silently swallowing the write. The returned [`StoreLocation`] additionally
/// binds identity to the resolved root, so a walked-up write lands in the stream the
/// conductor reads (see [`StoreLocation::identity`]). The run driver (`run`/`step`/`serve`)
/// is deliberately NOT routed through here: it legitimately BOOTSTRAPS the store on the
/// first step of a fresh project.
fn require_store_dir() -> Result<(StoreLocation, StoreSelection), Box<dyn std::error::Error>> {
    // The gate store fence (spec 70 criterion 3): the gate runner (`gate::ExecRunner::run`)
    // pins STORE_FENCE_ENV around a unit-worktree gate's spawned process so a store-opening
    // courier command IT runs (a test invoking `rigger emit`/`result`/`peers`/`reported`)
    // resolves to the fenced scratch dir named here, never walking up into the repo's LIVE
    // run stream. Checked BEFORE store_selection/the walk so a fenced courier never even
    // reads the real backend config or touches the ambient filesystem above `cwd` - a
    // fenced gate sees strictly less ambient state, never more. Additive and defaulted
    // off: unset (every caller that is not a gate's own spawned process), resolution is
    // byte-identical to before this fence existed.
    if let Ok(fenced) = std::env::var(STORE_FENCE_ENV) {
        let fenced = fenced.trim();
        if !fenced.is_empty() {
            let dir = PathBuf::from(fenced);
            // The fenced location is a scratch sibling ExecRunner names but never
            // creates (it must not create it INSIDE target_dir, which cargo owns and
            // may wipe - see ExecRunner::run). A store-opening courier that resolves
            // here is about to `Store::open` a path inside it; sqlite/rusqlite refuses
            // to create a database file in a directory that does not yet exist, so an
            // uncreated fence would fail every fenced courier outright instead of
            // landing it in an isolated EMPTY store - the opposite of "isolate more".
            // Creating it here (the one store-resolution authority every courier
            // funnels through) covers every current and future caller of this env var
            // uniformly, not just ExecRunner's specific choice of path.
            std::fs::create_dir_all(&dir)?;
            return Ok((StoreLocation { dir }, StoreSelection::Sqlite));
        }
    }
    let sel = store_selection(None, None)?;
    let cwd = std::env::current_dir()?;
    // A server-backed project shares ONE remote store; there is no local `events.db` to walk to,
    // so a courier binds identity to the OWNING root (the main repo root, correct even from a
    // nested worktree, exactly as the sqlite walk's identity does) and lets `resolve_store` reach
    // the server - closing the state-fracture where a worker's bare `rigger result` wrote to local
    // sqlite while the run lived on the server. The same [`server_store_location`] the residue
    // scan's read resolves through, so write and read agree on the one server store.
    if !sel.is_sqlite() {
        return Ok((server_store_location(&cwd), sel));
    }
    let walk = walk_stores_from(&cwd);
    let dir = walk.dir.ok_or_else(|| -> Box<dyn std::error::Error> {
        format!(
            "no rigger store found: neither {} nor any parent directory has an initialized \
             {RIGGER_DIR}/events.db. This usually means the command ran from the wrong \
             directory (e.g. a unit worktree, whose {RIGGER_DIR} is not the run's store). \
             Run it from the project root that owns the run; refusing to fabricate a fresh \
             empty store here.",
            cwd.display()
        )
        .into()
    })?;
    // Outermost store wins (spec 08 item 6): a NEARER shadow `events.db` (inside a unit
    // worktree or a scratch dir) must never SILENTLY eclipse the repo root's real run
    // stream. When the bounded walk bypassed one, name BOTH the bypassed shadow and the
    // chosen outermost store on stderr so the misfiling hazard is seen, not discovered.
    // (`validate`'s residue scan keeps its own shadow-store warning; this is the
    // courier-time notice at the exact moment a write is about to be routed.)
    for shadow in &walk.shadows {
        eprintln!(
            "store: warning: bypassing a nearer shadow store at {} in favor of the outermost \
             store at {} (a shadow store never eclipses the real run stream)",
            shadow.display(),
            dir.display()
        );
    }
    Ok((StoreLocation { dir }, sel))
}

/// The path to a database file (`events.db` / `graph.db`) inside a resolved store
/// directory, as the `&str` the sqlite `Store` / `Projector` opens.
fn store_file(dir: &Path, name: &str) -> String {
    dir.join(name).to_string_lossy().into_owned()
}

/// The stderr advisories `rigger result` prints from a single pre-write read of the run
/// stream, BEFORE it records (spec 05, done-when: "`rigger result` prints stderr
/// advisories for an orphan id and for superseding an existing result"). Two independent
/// notes, both purely advisory - the record still lands, because pre-recording a result
/// before its spawn request is parked is legitimate and re-recording deliberately
/// supersedes (results are last-write-wins). ORPHAN: no `SpawnRequested` with this id is
/// in the stream, so nothing is parked under it - a typoed id would otherwise silently
/// strand the real spawn while the orphan result records against an id the run never
/// requested. SUPERSEDE: a `SpawnResult` for this id is already recorded (at position N),
/// so this write replaces the earlier outcome.
///
/// Pure over the already-read events (no I/O) so both rules are unit-testable without a
/// store, mirroring the other `rigger result` seams ([`parse_result_args`]/[`build_result`]).
/// `will_supersede` is false on the `--if-absent` path (weave with unit-10): the CAS
/// refuses to overwrite, so a supersede note would claim a replacement that never
/// happens - only the orphan rule applies there.
fn result_advisories(events: &[Event], id: &str, will_supersede: bool) -> Vec<String> {
    let mut notes = Vec::new();
    if !spawn::is_recorded(events, id) {
        // The orphan note never claims a recording it might not make (spec 08 item 5). On
        // the plain (unconditional) path - `will_supersede` is true, since that path always
        // overwrites - the record always lands, so it states the recording. On the
        // `--if-absent` path (`will_supersede` is false) the CAS records ONLY if the spawn
        // is still unanswered, so it states that condition rather than asserting a recording
        // an already-answered spawn would leave untouched.
        notes.push(if will_supersede {
            format!(
                "result: note: no spawn request is recorded for {id:?}; recording an orphan \
                 result (nothing is parked under this id)"
            )
        } else {
            format!(
                "result: note: no spawn request is recorded for {id:?}; --if-absent records \
                 only if the spawn is unanswered"
            )
        });
    }
    // The LATEST already-recorded result for this id (last-write-wins), and the log
    // position it currently sits at, so the advisory can name it.
    let prior = events.iter().rev().find(|e| {
        e.type_ == spawn::TYPE_SPAWN_RESULT
            && spawn::SpawnResult::from_event(e).is_ok_and(|r| r.id == id)
    });
    if !will_supersede {
        return notes;
    }
    if let Some(e) = prior {
        notes.push(format!(
            "result: note: {id:?} already has a recorded result at position {}; this \
             record supersedes it",
            e.position
        ));
    }
    notes
}

/// Load the config a RUN will drive, refusing to start when a gating persona guarantees an
/// integration-gate stall (spec 18, unit 2). This is the single load seam every run entry
/// (`cmd_step`, `run_cli`, `run_workflow`) shares, so the run-start refusal cannot be present
/// at one entry and silently missing at another.
///
/// The integration gate reads a gating agent's RESULT channel for a `{"verdict":...}` line and
/// never reads emitted events (a deliberate load-bearing decision); a gating persona (a review
/// adjudicator on any tier, or a plan-critique adjudicator) that records its verdict ONLY via
/// `rigger_emit` is therefore a guaranteed stall - the gate finds no verdict, folds it as a
/// non-approval, and the unit remediates until it escalates. Rather than begin that doomed run,
/// `rigger run`/`workflow`/`step` refuse up front with the SAME deterministic fix message
/// `rigger validate` gives. The check itself has ONE authority - `config::lint_gating_verdict_lines`
/// (spec 18, unit 1) - reused here, never re-derived; this seam only wires it onto the run path.
fn load_run_config(dir: &str) -> Result<config::Config, Box<dyn std::error::Error>> {
    let cfg = config::load(dir)?;
    config::lint_gating_verdict_lines(&cfg)?;
    Ok(cfg)
}

fn cmd_run(args: &[String]) -> Res {
    let parsed = parse_run_args(args)?;
    // `--driver workflow` is the equivalent of `rigger serve`: the in-Claude-Code
    // MCP-server path. `cli` (the default) keeps the standalone subprocess path.
    match parsed.driver {
        DriverKind::Workflow => run_workflow(&parsed),
        DriverKind::Cli => run_cli(&parsed),
    }
}

/// `rigger step [--spec <path>]` - advance the run one frontier (§4, spec 04).
///
/// Drives `conductor::run` with the REPLAY driver over this project's namespaced run
/// stream: every already-recorded spawn is replayed from the log and every unrecorded
/// one at the frontier is parked as a `SpawnRequested` event. When every in-flight
/// spawn is parked the conductor unwinds cleanly and returns, so the process ends with
/// the run's whole state in the log - a later step, after a courier records results via
/// `rigger result`, replays past them.
///
/// It then prints ONE line of JSON on stdout: the WAVE it newly parked plus a `done`
/// flag (`{"wave":[<SpawnRequest>...],"done":<bool>}`), computed by the pure
/// [`spawn::step_result`] seam from the stream read before and after the run (decision
/// `d-step-wave-delta`). Two ready units with disjoint blast radii - which the
/// conductor's blast-radius partition keeps in one wave - park their spawns together and
/// appear in the same wave, so fan-out falls out of the run structure. The thin driver
/// runs the wave's agents in parallel and steps again until `done`.
///
/// Composition mirrors `run_cli` (the per-project namespaced sqlite run stream, the
/// grounder from `defaults.grounder`, the context-graph projector) so a step sees
/// exactly the state a `rigger run` would.
///
/// `--base <ref>` (default `origin/main`) anchors the run branch. Before driving the
/// conductor - which branches every unit worktree off HEAD and merges every approved
/// unit back into the current branch - the step ensures [`RUN_BRANCH`] exists AND is
/// checked out, so that isolation boundary is the run branch and never the operator's
/// own branch. On the native path `cmd_step` IS the driver (there is no separate setup
/// step), so this cannot be skipped when the base is missing: if [`RUN_BRANCH`] does not
/// exist yet it is created off `--base`, or off the current HEAD when `--base` does not
/// resolve (a repo with no remote, a `master`-default repo, or a pre-fetch clone) - a
/// fallback that keeps isolation and mirrors the JS driver. A step will therefore switch
/// the repo's checkout to [`RUN_BRANCH`] as a deliberate side effect; if that checkout
/// fails (e.g. a dirty tree, or the run branch is checked out in another worktree) the
/// step aborts with a clear error BEFORE it prints any JSON - run-branch setup is a
/// precondition, not something to proceed past.
///
/// An EXISTING run branch is reused, never reset (see [`Worktree::ensure_run_branch`]),
/// so prior steps' integrations survive and the run continues from where it left off.
/// Because of that, `--base` only takes effect when the run branch is first created;
/// once [`RUN_BRANCH`] exists, an explicit `--base` is ignored (re-anchoring would orphan
/// the integrated units), and the step says so on stderr rather than silently. A
/// repo-less invocation skips run-branch setup entirely.
/// The busy-refusal token a second concurrent `rigger step` prints (see
/// [`acquire_step_lock`]). A DRIVER couriering steps keys on this exact substring to tell a
/// benign "wait, another step holds the lock" from a real step failure - so it backs off
/// and retries `rigger step` instead of tearing the run down. Kept as a named constant so
/// the conductor side and the driver prompt can never drift apart.
const STEP_BUSY_TOKEN: &str = "another `rigger step` is already running";

/// Acquire the exclusive advisory lock that SERIALIZES `rigger step`, returning the held
/// [`File`](std::fs::File) as an RAII guard (the OS releases the flock when it drops or the
/// process dies). A NON-blocking `try_lock`: if another step already holds it, refuse fast
/// and loudly ([`STEP_BUSY_TOKEN`]) rather than blocking - a driver whose courier gets the
/// refusal backs off and retries, which keeps the run flowing without ever running two
/// steps at once. See the call site for why concurrent steps corrupt the run.
///
/// `rigger_dir` is the `.rigger` directory the lock file (`step.lock`) lives under, so a caller
/// resolves it exactly as it resolves every other store file: `cmd_step` always runs against the
/// CWD-relative [`RIGGER_DIR`] (it just ensured that directory exists), while a probe run from a
/// nested worktree - [`refuse_derived_reset_if_live`]'s live-writer guard (spec 71, criterion 2) -
/// passes the STORE'S resolved [`StoreLocation::dir`] instead. A caller that instead hardcoded
/// [`RIGGER_DIR`] here would probe the wrong (or nonexistent) `.rigger` under its own cwd and
/// misread "not this repo's `.rigger`" as "the lock is held" - the false refusal a nested-worktree
/// caller must never produce.
fn acquire_step_lock(rigger_dir: &Path) -> Result<std::fs::File, Box<dyn std::error::Error>> {
    use fs2::FileExt;
    let path = rigger_dir.join("step.lock");
    let f = std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(false)
        .open(&path)?;
    f.try_lock_exclusive()
        .map_err(|_| -> Box<dyn std::error::Error> {
            format!(
                "rigger step: {STEP_BUSY_TOKEN} in this repo (lock {}). Refusing to run \
             concurrently: two steps would race the run-branch checkout and the unit \
             worktrees branched off HEAD, corrupting the run. \
             Wait for the running step to finish (or kill it) and retry.",
                path.display()
            )
            .into()
        })?;
    Ok(f)
}

fn cmd_step(args: &[String]) -> Res {
    let args = parse_step_args(args)?;
    // Refuse a doomed run up front: a gating persona that never puts its verdict on the result
    // channel would stall the integration gate (spec 18, unit 2). This reuses unit 1's lint at
    // the run's config-load seam, before any unit is parked.
    let cfg = load_run_config(".")?;
    let criteria = load_criteria(args.spec.as_deref())?;
    std::fs::create_dir_all(RIGGER_DIR)?;

    // Serialize concurrent `rigger step` invocations so the run advances ONE step at a time
    // (spec 51 relies on that invariant). A step checks out the run branch and branches unit
    // worktrees off HEAD, then integrates units and appends events (see just below); two
    // steps at once would race that shared checkout/HEAD and interleave their integrations,
    // corrupting the run. The overlap arises when a driver re-couriers a step while the
    // first's minutes-long gate still runs. Held for the whole step and released when this
    // process exits (even on crash/kill), so a dead step never wedges the run. The guard
    // binds a name so it is not dropped early.
    let _step_lock = acquire_step_lock(Path::new(RIGGER_DIR))?;

    // Anchor + check out the run branch before the conductor branches any unit worktree
    // off HEAD. Guarded on a real repo so the repo-less unit-test path is untouched. A
    // failure here aborts the step (with a clear, actionable error) rather than driving
    // the conductor on the wrong branch - isolation is a precondition, not best-effort.
    let repo = git_repo();
    if !repo.is_empty() {
        // Refuse an obviously-wrong base BEFORE the run branch is anchored (spec 18, criterion
        // 7). Gating on the PLANNED anchor (a side-effect-free peek) - not on the created branch
        // - means a refused first step leaves NO wrong-base run branch behind, so the corrected
        // `--base` retry re-runs this check and re-anchors fresh instead of reusing (and thus
        // self-disarming on) the wrong-base branch.
        let planned = Worktree::planned_run_branch_setup(&repo, RUN_BRANCH, &args.base);
        // Loop-readiness gate (spec 38, criterion 2): refuse a run with no reachable base (an
        // unresolvable base AND no HEAD to fall back to) loudly rather than minting a run branch
        // that branches from nowhere.
        refuse_when_base_unreachable(&repo, "rigger step", &args.base, planned)?;
        refuse_when_base_lacks_spec_paths(&repo, "rigger step", &args.base, planned, &criteria)?;
        let setup = Worktree::ensure_run_branch(&repo, RUN_BRANCH, &args.base).map_err(|e| {
            format!(
                "rigger step: could not prepare the run branch {RUN_BRANCH:?} (base {:?}): {e}. \
                 The step did not run; resolve the git state (e.g. commit or stash a dirty tree) and retry.",
                args.base
            )
        })?;
        warn_on_run_branch_divergence("rigger step", setup, &args.base, args.base_explicit);
    }

    // Migrate a pre-spec-09 store's legacy-namespace history to the minted identity once,
    // before opening the run backend (spec 09, Gap 20). A no-op unless `.rigger/project.id`
    // was minted with an id distinct from the basename and the legacy namespace still holds
    // the history; refuses loudly if both namespaces are populated.
    migrate_local_identity()?;

    let selection = store_selection(None, None)?;
    // Register this instance in the machine-global discovery registry (spec 50, criterion 2):
    // every step "advances a run". Held (`_registration`) for the whole step so its heartbeat
    // thread keeps the entry fresh even if THIS step's in-process gate runs longer than the idle
    // window; dropped when the step returns. Best-effort and warn-only - it never blocks the step.
    let _registration = register_run_instance(&repo, &selection);
    let backend = resolve_store(&selection, &db_path("events.db"))?;
    let store = Namespaced::new(backend.as_ref(), &project_identity());

    // The definition hash this step pins / re-checks (spec 13, unit 1): the digest of the
    // on-disk workflow.yml + agent-prompt set. Computed once and used for both the `--fresh`
    // pinned boundary and the drift check below.
    let definition = definition_hash(".")?;

    // `--fresh`: begin a NEW run BEFORE this step (and before the liveness sweep reads the
    // current run), so the conductor's own `ensure_started` adopts this just-minted
    // boundary instead of the latest (possibly wedged) run. A one-shot the DRIVER passes
    // on the first step of an explicit restart; plain steps after it adopt the boundary it
    // began. The notice goes to STDERR - stdout carries only the `{wave,done}` JSON the
    // driver parses. See `runscope::start_fresh`.
    if args.fresh {
        // Persist the resolved run-branch base on the fresh boundary (spec 38, criterion 3):
        // `args.base` is the base this step anchored the run branch on, so `rigger status`/dash
        // name the same base in the ready-to-release handoff.
        let run = runscope::start_fresh(&store, &criteria, &definition, &args.base)?;
        eprintln!("rigger step: --fresh: began a new run {run} (the prior run stays in the log)");
    }

    // Captured before `repo` moves into Deps: the fixpoint/terminal teardown below needs it, and
    // computed BEFORE the definition-pin check so a definition-drift HALT can reclaim run-level
    // scratch on its way out (spec 34, criterion 3).
    let scratch_root = if repo.is_empty() {
        None
    } else {
        Some(rigger::worktree::scratch_root_from_env(
            &repo,
            &cfg.workflow.defaults.workdir,
        ))
    };

    // The maintenance half of Gap 14, made liveness-aware (spec 64, criterion 4): every step
    // starts by sweeping the scratch root's terminal worktrees (integrated units, review
    // scaffolding), so leaks from crashed or superseded step processes are reclaimed by the
    // loop itself instead of accumulating until a human notices a full disk. Placed here (after
    // the store opens and any `--fresh` boundary is settled, but still BEFORE the
    // definition-pin HALT below) so it keeps running at step start on every step, including one
    // that goes on to halt on drift - exactly as before this criterion. The merged-only git
    // ancestry rule alone is not sufficient: a PARKED unit whose attempt produced an EMPTY diff
    // has a branch tip that IS an ancestor of the run branch (trivially - it never advanced past
    // it) while the unit is still live in review, so `sweep_terminal` is handed the CURRENT
    // run's live branches (the same `current_run_units` fold `reclaim_orphan_scratch` below
    // reads - one liveness authority, not a parallel notion) and spares any of them outright,
    // even one that would otherwise pass the ancestry test.
    //
    // `live_branches_for_sweep` fails CLOSED on an unreadable stream (`None`): liveness can only
    // be UNDER- not OVER-determined, so a `store.read_stream` error skips the sweep call
    // OUTRIGHT below, never runs it with a live set silently degraded to empty (which would
    // revert to the pre-c4 ancestry-only rule this criterion exists to close). Its own doc
    // comment carries the full rationale and is where its unit test lives; the `sweep_terminal`
    // call itself stays INLINE here (not pulled into that helper) because
    // `worktree_sweep_completes_before_any_add_within_one_step` (spec 51, criterion 5) pins its
    // presence and lock->sweep->add ordering directly in `cmd_step`'s own source text.
    if let Some(root) = &scratch_root {
        if let Some(live_branches) =
            live_branches_for_sweep(store.read_stream(conductor::STREAM, 0, Direction::Forward))
        {
            match rigger::worktree::sweep_terminal(&repo, root, RUN_BRANCH, &live_branches) {
                Ok(0) => {}
                Ok(n) => eprintln!("rigger step: swept {n} terminal worktree(s) from {root}"),
                Err(e) => eprintln!("rigger step: scratch sweep skipped: {e}"),
            }
        }
    }

    // Definition pinning (spec 13, unit 1): pin this run's definition (a fresh run) or enforce
    // it (a live run). A drifted live-run definition WITHOUT `--rebase-definition` HALTS here,
    // loudly and before any worktree work, so a mid-campaign prompt edit can never silently
    // change replay semantics; `--rebase-definition` records the supersession and continues.
    if let Err(e) = enforce_definition_pin(
        &store,
        &criteria,
        &definition,
        args.rebase_definition,
        &args.base,
    ) {
        // A definition-drift HALT is a terminal state for this run process (spec 34, criterion
        // 3): reclaim the run-level shared scratch before propagating the loud halt, so a halted
        // run leaves no shared build cache or agent scratch behind - the same run-teardown a
        // clean fixpoint gets. Gated on the SAME `terminal_and_no_live_worker` predicate the
        // terminal-fixpoint teardown below uses (ONE authority, so the two sites can never
        // diverge): the run must be at a terminal state with NO live worker - an empty pending
        // frontier, no hung-but-possibly-alive spawn, AND no still-pending manual-review pause. A
        // still-in-flight worker, OR a hung spawn whose liveness fault counts as "answered" yet
        // leaves a worker the operator may still resume with `--rebase-definition`, OR a unit
        // paused awaiting a human (its persisted `ManualReview` from an EARLIER step folds into the
        // inbox this predicate reads from the full stream), is STILL ADVANCING - so its scratch is
        // never pulled out from under it (the never-delete-live-owned rail). An unreadable/malformed
        // stream reads as NOT safe, so uncertainty never reclaims. Best-effort; the halt is
        // surfaced regardless.
        if let Some(root) = &scratch_root {
            if let Ok(events) = store.read_stream(conductor::STREAM, 0, Direction::Forward) {
                if terminal_and_no_live_worker(&events).unwrap_or(false) {
                    reclaim_run_scratch(root);
                }
            }
        }
        return Err(e);
    }

    let graph = Projector::open(&db_path("graph.db"), &project_identity())?;
    let grounder = select_grounder(&cfg.workflow.defaults.grounder)?;
    // The store state BEFORE this step's own liveness sweep runs (spec 69, criterion 5):
    // used below to scope the sweep's marker reads to THIS run (a slug-colliding re-run
    // never reads a prior run's leftover mtime) and, since `run_id` never changes within a
    // step, reused verbatim for the hung-attention cursor path too (see `pre_hung_ids`
    // below). Read ONCE here, before the sweep mutates the log.
    let pre = store.read_stream(conductor::STREAM, 0, Direction::Forward)?;
    // Empty before the first RunStarted (the first step, where nothing is in-flight to sweep
    // and nothing has ever been surfaced anyway).
    let run_id = runscope::current_run_id(&pre).unwrap_or_default();
    // The hung-attention CROSSING BOUNDARY (spec 69, criterion 5; review u69c5 round 3,
    // cause genuine-defect): the spawn ids already surfaced on the `attention` wire as of
    // the end of the PREVIOUS `rigger step` invocation, read from the persisted cursor
    // (see `liveness::hung_cursor_path`'s own doc comment for why a fresh read taken at
    // THIS process's own start - the prior approach - can never see a fault an out-of-band
    // driver call recorded strictly between two step invocations: that write already
    // predates every read this process could take). `conductor::compute_attention`'s own
    // doc comment covers why this specific signal cannot be computed from inside
    // `conductor::run` at all, in-process or otherwise. Absent scratch (the repo-less
    // unit-test path only - guarded the same way every other scratch-dependent read in this
    // function already is) has nowhere to persist a cursor, so this reads as empty - see the
    // `newly_hung` computation below (review u69c5 round 5, cause genuine-defect) for why an
    // always-empty read here is made SAFE (never a false "newly hung" every step) rather than
    // read at face value the way it is in the scratch-present path.
    let pre_hung_ids: std::collections::BTreeSet<String> = scratch_root
        .as_deref()
        .map(|root| rigger::liveness::read_hung_cursor(root, &run_id))
        .unwrap_or_default();

    // Liveness sweep (spec 10, unit 3): BEFORE the conductor replays the frontier, classify
    // any IN-FLIGHT spawn whose per-spawn heartbeat marker went stale beyond its
    // `max_wall_clock` as an infrastructure fault (a HUNG agent) and record it on the
    // spawn's id. The conductor then re-parks that fault (charging no remediation attempt -
    // the unit's code is not at fault), and it surfaces as a halt below. Best-effort and
    // scoped to the current run; a sweep failure never blocks the step.
    if let Some(root) = &scratch_root {
        match cfg.workflow.failure_taxonomy() {
            Ok(taxonomy) => {
                match rigger::liveness::sweep(
                    &store,
                    runscope::current_run(&pre),
                    root,
                    &run_id,
                    &taxonomy,
                    std::time::SystemTime::now(),
                ) {
                    Ok(stale) if !stale.is_empty() => eprintln!(
                        "rigger step: liveness swept {} hung spawn(s) (classified infra, no attempt charged): {}",
                        stale.len(),
                        stale.iter().map(|s| s.id.clone()).collect::<Vec<_>>().join(", ")
                    ),
                    Ok(_) => {}
                    Err(e) => eprintln!("rigger step: liveness sweep skipped: {e}"),
                }
            }
            Err(e) => eprintln!("rigger step: liveness sweep skipped (taxonomy: {e})"),
        }
    }

    // Orphan-sweep backstop (spec 34, criterion 2): reclaim any scratch under the root that no
    // LIVE unit of the CURRENT run owns - a prior run's stranded worktree/build cache, or a
    // `cargo-target-<slug>` an agent wrote outside its assigned path (the unbounded per-agent
    // leak). Keyed on liveness ownership (the SAME `worktree_belongs_to_live` predicate the
    // `rigger validate` residue report reads), so it can never remove a worktree an in-flight
    // reviewer is reading or a cache a live unit is building, and it deliberately spares the
    // shared `agent-scratch`/`agent-live`/bare-`cargo-target` areas a running spawn may still
    // be writing into. This runs AFTER `enforce_definition_pin` above so a `--fresh` restart's
    // just-superseded prior-run scratch reads as non-live and is reclaimed; it re-runs
    // idempotently each step (an already-clean root sweeps nothing). Broader than the git-only
    // `sweep_terminal` above, which reclaims only integrated worktrees. Best-effort - a sweep
    // failure only warns and never blocks the step.
    if let Some(root) = &scratch_root {
        match store.read_stream(conductor::STREAM, 0, Direction::Forward) {
            Ok(events) => {
                let run_units = current_run_units(&events);
                let removed = reclaim_orphan_scratch(&repo, root, &run_units);
                if removed > 0 {
                    eprintln!(
                        "rigger step: reclaimed {removed} orphaned scratch entr{} under {root}",
                        if removed == 1 { "y" } else { "ies" }
                    );
                }
            }
            Err(e) => eprintln!("rigger step: orphan sweep skipped: {e}"),
        }
    }

    // Always-on dash on the native step path, retargeted at the machine SINGLETON (spec 39,
    // criterion 1; spec 50, criterion 4): ENSURE the one machine-level dash at the fixed default
    // address is up, so the loop the driver advances through many short-lived `rigger step`
    // invocations is never invisible. The first step of a run starts it at `dash::DEFAULT_PORT`
    // (never a drifting port); every later step finds it serving and starts none (never a second
    // dash). Started DETACHED so it survives across the run's many step processes, and best-effort
    // so a start failure only warns - the run proceeds headless. Suppressed entirely by the
    // opt-out (`RIGGER_NO_DASH` OR `dash: off`): a headless/CI run then binds no port at all. The
    // config opt-out is read off the already-loaded `cfg`, not re-loaded. Placed just before the
    // conductor advances the frontier so the dash is serving while this step's gates and spawns
    // are in flight.
    ensure_run_dashboard(cfg.workflow.dash_enabled(), &store);

    let driver = ReplayDriver::new(&store);
    let deps = Deps {
        store: &store,
        driver: &driver,
        gates: &ExecRunner,
        repo,
        grounder: Some(grounder.as_ref()),
        graph: Some(&graph),
        criteria,
    };
    let rs = conductor::run(&cfg, &deps)?;

    let events = store.read_stream(conductor::STREAM, 0, Direction::Forward)?;
    // The printed wave is the FULL pending frontier (every parked spawn without a
    // result), so a killed or re-run step process orphans nothing and a relaunched
    // driver resumes the in-flight wave (see spawn::step_result). Scoped to the CURRENT
    // run's slice (spec 06, unit 1): a prior run's unanswered spawns sit before this
    // run's RunStarted, so they never reappear in this run's wave (Gap 11).
    let mut step = spawn::step_result(runscope::current_run(&events)).map_err(|e| e.to_string())?;
    // Stamp each bounded wave item with the RESOLVED absolute path of its liveness marker
    // (spec 10, unit 3, BLOCKER-1): the thin driver frames both the worker's heartbeat
    // `touch` and its staleness watchdog around THIS path, never re-deriving a scratch root
    // of its own. Derived from the SINGLE authority `liveness::marker_path` over the same
    // resolved scratch root (`RIGGER_TMPDIR` > `defaults.workdir` > repo default) the sweep
    // above reads and this run's id - so the worker-write path is byte-identical to the
    // sweep-read path under ANY scratch config. Only a bounded spawn carries a marker.
    if let Some(root) = &scratch_root {
        let run_id = runscope::current_run_id(&events).unwrap_or_default();
        for item in step.wave.iter_mut() {
            if item.max_wall_clock.is_some() {
                item.marker_path = Some(
                    rigger::liveness::marker_path(root, &run_id, &item.id)
                        .to_string_lossy()
                        .into_owned(),
                );
            }
        }
    }
    // Surface a spawn-budget HALT (Gap 13) distinct from convergence: the conductor sets
    // `budget_halt` from its in-process breaker when a trip left ready work unscheduled, so
    // the printed `Step` carries a halt reason (`{"...","done":true,"halted":"..."}`) the
    // thin driver stops LOUDLY on - instead of reading a starved run as a clean completion.
    //
    // Surface a WEDGED terminus (spec 19c, unit 1) distinct from a clean completion, ALONGSIDE
    // the budget halt: the set of units that escalated (exhausted remediation and went
    // terminal without integrating), taken from the conductor's projected run state - the
    // single authority for the escalated set, reusing the folded `UnitEscalated` status.
    // Omitted from the wire when empty, so a clean run's `{"wave":[],"done":true}` shape is
    // unchanged; when non-empty the driver treats a `done` fixpoint carrying it as a LOUD stop
    // (exactly as for a budget halt), so a unit that can never pass review no longer
    // masquerades as a clean "run complete". Escalation-and-continue MID-run is untouched -
    // only the driver's read of the final terminus changes, and it gates on `step.done`.
    // Stamped BEFORE the `halted` move below (which consumes `rs.budget_halt`), as it borrows
    // `rs`.
    step.escalated = rs.escalated_units();
    // Surface the push-side ATTENTION array (spec 69, criterion 5): the conductor already
    // computed it as a before/after diff of this call's own transition (see
    // `conductor::compute_attention`), so this is a plain move of the live state onto the
    // wire - exactly like `escalated` and `halted`, and like them omitted when empty so a
    // clean step's `{"wave":[],"done":true}` shape stays byte-for-byte unchanged. Rendering
    // each entry as a narrator log line is a later criterion's job (spec 69, "the driver
    // relays it"); this step only stamps the wire.
    step.attention = rs.attention;
    step.halted = rs.budget_halt;
    // Hung agents (spec 10, unit 3): any spawn whose LATEST result is a liveness fault is a
    // hung, unrecovered agent whose worker may STILL be alive and writing under the shared
    // scratch. Surfaced as a loud halt so the driver stops on a named reason instead of reading
    // a stalled wave as a clean fixpoint. A budget halt already on the channel takes precedence
    // for the surfaced REASON (it is the harder global rail), so the hung reason is only stamped
    // when no budget halt is set. (The teardown's never-delete-live guard reads the same hung set
    // through `terminal_and_no_live_worker` below, so a hung-but-alive worker is spared under any
    // halt - not just when its reason is the one surfaced here.)
    let hung =
        rigger::liveness::hung_spawns(runscope::current_run(&events)).map_err(|e| e.to_string())?;
    if step.halted.is_none() && !hung.is_empty() {
        // Recovery: record a real result on the named spawn (last-write-wins supersedes the
        // fault), then re-drive.
        step.halted = Some(rigger::liveness::halt_reason(&hung));
    }
    // The hung-liveness half of `attention`'s `halted` signal (spec 69, criterion 5; review
    // u69c5 round 3, cause genuine-defect): computed HERE, not inside `conductor::run`, because
    // its crossing boundary is `pre_hung_ids` (the boundary computed above - the PERSISTED
    // cursor whenever scratch is available, see `liveness::hung_cursor_path`'s own doc comment
    // for why - never a fresh read taken at this process's own start) - see
    // `conductor::compute_attention`'s own doc comment for why that boundary cannot live
    // inside `run()` itself either way. A spawn hung in `hung` that
    // was NOT already hung as of `pre_hung_ids` is a genuine NEW crossing this step; one
    // already hung as of `pre_hung_ids` is a still-true restamp and does not fire again.
    // `merge_hung_attention` (pulled out for unit-testability - see its own doc comment) owns
    // the precedence-and-ordering mechanics.
    //
    // Gated on `scratch_root.is_some()` (review u69c5 round 5, cause genuine-defect, findings
    // sdet-u69c5r4-repoless-cursor-restamps-every-step /
    // adv-u69c5r4-confirm-sdet-repoless-restamp-empirically-proven): absent scratch,
    // `pre_hung_ids` above has nowhere to persist a cursor and always reads empty, while
    // `hung` (unlike `pre_hung_ids`) is derived purely from the event log and is NOT itself
    // gated on `scratch_root` - so comparing the two directly would read every step as a fresh
    // crossing and restamp `attention` forever instead of once, the opposite failure direction
    // from the crash-window gap the persisted cursor exists to close. `step.halted` just above
    // is UNCHANGED by this gate - it stays sourced from the full, ungated `hung` set, so a
    // hung spawn still halts loudly on every step regardless of scratch (this only scopes the
    // separate crossing-tracked `attention` entry). Without a persisted cursor there is no
    // boundary available at all to tell a genuine new crossing from a still-true restamp in
    // this path (every repo-less-reachable fault is recorded by a wholly separate `rigger
    // result --error` process call, which by construction always predates this step's own
    // reads - there is no in-process moment that precedes it the way the scratch-present
    // sweep's pre-sweep read does), so gating BOTH halves of the comparison to the SAME
    // scratch-presence keeps them consistent in every path: `attention` simply carries no
    // hung-liveness entry when repo-less, rather than guessing wrong in either direction.
    let newly_hung = scratch_root.is_some() && hung.iter().any(|h| !pre_hung_ids.contains(&h.id));
    step.attention = merge_hung_attention(step.attention, newly_hung, || {
        rigger::liveness::halt_reason(&hung)
    });
    // `step` is now fully finalized - nothing below this point mutates it further.
    //
    // DELIVER the step BEFORE persisting the hung-attention cursor (spec 69, criterion 5;
    // review u69c5 round 5, cause genuine-defect, finding
    // adv-u69c5r4-cursor-write-outruns-its-own-delivery): the `println!` below is `attention`'s
    // SOLE delivery channel - it is never appended to the event log (see `Step`'s own
    // `attention` field: `skip_serializing_if = "Vec::is_empty"`, live-wire-only by design) -
    // so persisting the cursor first would let a process death strictly between the two calls
    // (SIGKILL, OOM, a broken pipe to the driver) durably mark a crossing "already surfaced"
    // even though no observer ever actually saw it: the next invocation's `pre_hung_ids` would
    // already contain the id, so `newly_hung` reads false forever after - silently swallowing a
    // genuine crossing, the exact failure mode this whole mechanism exists to close. Printing
    // FIRST means a crash in that window instead costs one harmless extra restamp next step
    // (the cursor never got updated, so the unchanged crossing is detected again) - the same
    // "fail toward one extra harmless re-notification, never toward silently swallowing a
    // genuine new crossing" direction `write_hung_cursor`'s own doc comment already commits to.
    println!("{}", serde_json::to_string(&step)?);
    // Persist the hung-attention cursor (spec 69, criterion 5; review u69c5 round 3, cause
    // genuine-defect): overwrite it with THIS step's own full `hung` set - the exact set
    // `pre_hung_ids` above just diffed against - so the NEXT `rigger step` invocation (this
    // process's own next step, OR the very next one after an out-of-band driver fault) reads
    // an up-to-date boundary rather than the one this process itself started with. Best-effort;
    // a write failure only risks one extra re-stamp later, never fails this step (see
    // `liveness::write_hung_cursor`'s own doc comment). Runs AFTER the print above, never
    // before it (see that comment for why).
    if let Some(root) = &scratch_root {
        let current_hung_ids: std::collections::BTreeSet<String> =
            hung.iter().map(|h| h.id.clone()).collect();
        if let Err(e) = rigger::liveness::write_hung_cursor(root, &run_id, &current_hung_ids) {
            eprintln!("rigger step: could not persist the hung-attention cursor: {e}");
        }
    }
    // RUN TEARDOWN at a terminal run state (spec 34, criterion 3): reclaim the run's run-level
    // shared scratch - `agent-scratch` (probe repos + verification builds a worker parks under
    // <scratch-root>/agent-scratch per the driver's scratch policy), `agent-live` (per-spawn
    // liveness markers, spec 10 unit 3), and the SHARED build cache (`cargo-target`/`target`
    // directly under the root, the driver's `CARGO_TARGET_DIR` - the unbounded multi-GB leak
    // spec 34 names). These exist only to serve in-flight spawns, so once the run is terminal
    // with no spawn live they are pure residue; leaving them is how a wedged/halted run leaks
    // gigabytes of build debris (Gap 14). The orphan-sweep backstop (criterion 2) deliberately
    // SPARES these shared areas while the run steps (a live spawn may still be building into
    // them), so their reclamation is exactly this run-level teardown - fired for EVERY terminal
    // state, not just a clean fixpoint: a wedge/escalation and a budget halt reclaim too.
    //
    // Gated on the SINGLE `terminal_and_no_live_worker` predicate (the never-delete-live-owned
    // rail): the pending frontier is empty, no liveness-fault spawn may still be alive, AND no
    // manual-review pause is still pending. The SAME predicate gates the definition-drift teardown
    // above, so EVERY still-advancing condition is inherited by both sites and none can drift
    // between them. It generalizes the former clean-fixpoint-only guard (`step.done &&
    // halted.is_none()`) to also fire on a budget halt / escalation while still sparing a liveness
    // halt or a manual-review pause. Best-effort - never fails the step. `?` here can never
    // actually err: all three sub-reads (`step_result`, `hung_spawns`, `ledger::project`) already
    // succeeded above on this same `events` (the last inside `conductor::run`, which produced
    // `rs`), so the predicate is pure recomputation over an in-memory slice.
    //
    // The frontier+hung core is NECESSARY but not SUFFICIENT for run terminality: a manual-review
    // PAUSE (`autonomy: manual` on a gated stage, §4.3) emits `ManualReview` and returns its unit
    // pending WITHOUT ever parking an implementer spawn, so it leaves an EMPTY frontier and no hung
    // spawn - the core reads terminal - yet the run is manual-review-pending, i.e. NOT converged
    // and STILL ADVANCING (a human will approve+integrate it on a later step). That is exactly a
    // run this rail must SPARE. The manual-review exclusion is FOLDED INTO the shared predicate
    // (it projects the `manual_review` inbox from the scoped events), so both this terminal site
    // and the drift early-return above spare a paused run without any per-caller guard to keep in
    // sync. (A budget halt / escalation IS terminal per criterion 3 and leaves the inbox empty, so
    // those still reclaim - only a non-terminal manual-review pause is excluded.)
    if terminal_and_no_live_worker(&events)? {
        if let Some(root) = &scratch_root {
            reclaim_run_scratch(root);
        }
    }
    Ok(())
}

/// Merge the hung-liveness half of signal 2 (`halted`) into `attention` (spec 69, criterion
/// 5; review u69c5 round 3, cause genuine-defect) and restore the canonical kind order -
/// pulled out of [`cmd_step`] (not reachable through the crate API - `main.rs` is a binary)
/// purely so the PRECEDENCE-AND-ORDERING mechanics are independently unit-testable, separate
/// from the CROSSING decision (`newly_hung`, computed at the call site from `pre_hung_ids` -
/// see `conductor::compute_attention`'s own doc comment for why that decision cannot live
/// inside `conductor::run` itself).
///
/// `attention` already carries whatever `compute_attention` built (escalated /
/// budget-halted / worker-death-recurred / budget-final-tenth / stalled-frontier, in THAT
/// canonical order). Pushes ONE run-scoped `halted` entry - built lazily via `reason` only
/// when actually needed, since `liveness::halt_reason` walks the whole hung set - when
/// `newly_hung` is true AND no `halted` entry is already present (a budget halt this same
/// call takes precedence, mirroring the SAME precedence the `halted` wire field itself
/// already gives the budget breaker over the hung fallback, just above this function's call
/// site). A STABLE sort by [`ledger::attention_kind_rank`] afterward only ever needs to
/// relocate the ONE entry just appended - `compute_attention`'s own entries are already in
/// canonical order, and a stable sort never disturbs their relative order (e.g. two
/// `stalled-frontier` units stay lexical) - so the merged array is byte-identical to what
/// `compute_attention` alone would have produced had it been able to see this crossing.
fn merge_hung_attention(
    mut attention: Vec<ledger::AttentionEntry>,
    newly_hung: bool,
    reason: impl FnOnce() -> String,
) -> Vec<ledger::AttentionEntry> {
    if newly_hung && attention.iter().all(|e| e.kind != ledger::ATTENTION_HALTED) {
        attention.push(ledger::AttentionEntry::run_scoped(
            ledger::ATTENTION_HALTED,
            reason(),
        ));
        attention.sort_by_key(|e| ledger::attention_kind_rank(e.kind));
    }
    attention
}

/// The step-start sweep's liveness decision (spec 64, criterion 4 fix): given the outcome of
/// reading the CURRENT run's stream, decide the live branches `cmd_step` hands to
/// `worktree::sweep_terminal` - or that the sweep must not run at all. Pulled out of [`cmd_step`]
/// (which is not reachable through the crate API - `main.rs` is a binary) purely so this
/// decision is independently unit-testable; the `sweep_terminal` call itself stays inline in
/// `cmd_step` (see the call site's comment for why).
///
/// Fails CLOSED (`None`) on an unreadable stream, mirroring the fail-closed convention
/// `reclaim_orphan_scratch` and `terminal_and_no_live_worker` already use elsewhere in
/// `cmd_step`: liveness can only be UNDER- not OVER-determined, so when `store.read_stream`
/// errors (a real failure class under WAL-mode concurrent writers - see `SQLITE_BUSY_SNAPSHOT` in
/// `eventstore/sqlite.rs`) this returns `None` and prints a warning, which the caller reads as
/// "skip the sweep entirely" - never as an empty live set. An empty live set means "nobody is
/// live" (still runs the sweep, reclaiming everything the ancestry rule would), the OPPOSITE of
/// "we don't know who is live" - conflating the two is exactly the rejected bug this closes: it
/// would silently revert to the pre-c4 ancestry-only rule that force-removes a live unit's
/// empty-diff worktree mid-review.
fn live_branches_for_sweep(
    read: Result<Vec<Event>, rigger::eventstore::Error>,
) -> Option<std::collections::HashSet<String>> {
    match read {
        Ok(events) => Some(current_run_units(&events).live_branches),
        Err(e) => {
            eprintln!("rigger step: scratch sweep skipped (liveness unreadable): {e}");
            None
        }
    }
}

/// The NO-STILL-ADVANCING-WORK core of the never-delete-live-owned rail as ONE predicate (spec 34,
/// criterion 3): true when the current run has NO worker that may still be alive under the shared
/// scratch AND no unit still awaiting a human. Both run-teardown sites - the definition-drift
/// early-return in [`cmd_step`] and the terminal-fixpoint teardown after `conductor::run` - gate on
/// THIS function, so every still-advancing condition is inherited by both and none can drift into a
/// divergent per-caller copy (the divergence that once let the drift path reclaim on an empty
/// frontier ALONE - first omitting the hung check, then the manual-review check).
///
/// Three conditions, all required:
/// - the pending frontier is EMPTY (`spawn::step_result(...).done`): every recorded spawn has a
///   result, so no in-flight wave and no obviously-live worker; and
/// - NO spawn is HUNG (`liveness::hung_spawns(...)` is empty): a liveness-fault result counts as
///   "answered" (so it does NOT keep the frontier non-empty) yet leaves a worker that may still
///   be alive and writing under the shared scratch - and which the operator may yet recover - so
///   its presence must still block reclamation; and
/// - NO manual-review PAUSE is pending (`ledger::project(...).manual_review` is empty): a
///   `autonomy: manual` gate (§4.3) emits a PERSISTED `ManualReview` and returns its unit pending
///   WITHOUT parking any spawn, so it leaves an empty frontier and no hung spawn - the frontier+hung
///   core alone reads terminal - yet the run is manual-review-pending, i.e. NON-terminal and STILL
///   ADVANCING (a human will approve+integrate it on a later step). That persisted pause is a
///   property of the LOG, not of whether `conductor::run` ran this step, so it is folded in HERE
///   rather than at a caller: the drift early-return runs BEFORE `conductor::run`, but it reads the
///   full stream (which already carries a prior step's `ManualReview`), so it needs the exclusion
///   too. Folding it into this shared core keeps a single authority for "no still-advancing work"
///   and closes the never-delete-live breach a per-caller guard re-opened.
///
/// Scoped to the CURRENT run only (`runscope::current_run`), so a prior run's unanswered spawns or
/// paused units never gate this run's teardown. Errs only if a malformed stored event cannot be
/// replayed; callers treat an `Err` as "not safe to reclaim" (never delete on uncertainty).
fn terminal_and_no_live_worker(events: &[Event]) -> Result<bool, String> {
    let scoped = runscope::current_run(events);
    let frontier_empty = spawn::step_result(scoped).map_err(|e| e.to_string())?.done;
    let no_hung = rigger::liveness::hung_spawns(scoped)
        .map_err(|e| e.to_string())?
        .is_empty();
    // The manual-review inbox, projected from the SAME scoped slice - the single authority for
    // which units still await a human. A non-terminal manual-review PAUSE leaves an empty frontier
    // and no hung spawn (it parks no spawn), so the frontier+hung core alone reads terminal even
    // though the run is still advancing. Folding the exclusion HERE - not at each caller - means
    // both teardown sites inherit it structurally and the guard can never diverge between them.
    let no_manual_review = ledger::project(scoped)
        .map_err(|e| e.to_string())?
        .manual_review
        .is_empty();
    Ok(frontier_empty && no_hung && no_manual_review)
}

/// Reclaim the run's run-level shared scratch at a terminal run state (spec 34, criterion 3):
/// `agent-scratch` (probe repos + verify builds a worker parks there), `agent-live` (per-spawn
/// liveness markers), and the SHARED build cache - `cargo-target` and `target` directly under
/// the scratch root, the driver's `CARGO_TARGET_DIR`, the unbounded multi-GB leak. These are the
/// run-level areas the orphan-sweep backstop deliberately spares while the run is stepping (a
/// live spawn may still be building into them); once the run is terminal and no spawn is live
/// they are pure residue, so this teardown - and only this teardown - reclaims them. The two
/// build-cache names mirror exactly what `rigger validate`'s residue report flags as a shared
/// cache (`scan_residue`), so validate-reports and step-reclaims stay in lockstep.
///
/// Each area is reaped-then-removed (spec 23): any process still rooted in it is reaped BEFORE
/// the dir is removed so nothing outlives a dir holding a now-deleted cwd. Scoped to the EXACT
/// dir removed under the resolved scratch `root` (`RIGGER_TMPDIR` > `defaults.workdir` > repo
/// default), never a hardcoded `.rigger/tmp`, so a relocated scratch root stays safe and only
/// rigger's own scratch is ever reaped. Every half is best-effort - a missing area is a graceful
/// no-op (platform-tolerant, idempotent), never an error that fails the step. Per-unit worktrees
/// and their `cargo-target-<slug>` caches are NOT this function's concern - those are reclaimed
/// when their unit goes terminal (`Worktree::remove` / `sweep_terminal` / the orphan-sweep),
/// never while a later stage of the same unit still needs them.
///
/// The bare `cargo-target`/`target` removals are the first code to delete those names directly
/// under the root (the orphan-sweep and `rigger validate` only ever touch the per-unit
/// `cargo-target-<slug>` siblings), so this is safe ONLY because `root` is the RESOLVED rigger
/// scratch root (`RIGGER_TMPDIR` > `defaults.workdir` > the repo default `.rigger/tmp`), a
/// directory rigger owns end to end - never the repo root. An operator who misconfigures the
/// scratch root TO the repo root would have rigger park every worker's scratch there too, so the
/// misconfiguration is self-evident long before this teardown; this function does not re-derive
/// or second-guess `root`, it trusts the one resolution authority all scratch paths share.
fn reclaim_run_scratch(root: &str) {
    let base = std::path::Path::new(root);
    reap_then_remove_dir(&base.join("agent-scratch"));
    reap_then_remove_dir(&base.join(rigger::liveness::MARKER_SUBDIR));
    reap_then_remove_dir(&base.join("cargo-target"));
    reap_then_remove_dir(&base.join("target"));
}

/// Reap any process rooted in `dir` (spec 23), then remove the dir. The reap runs BEFORE the
/// removal so no process outlives the dir holding a now-deleted cwd; both halves are
/// best-effort and never fail the step. The reap is scoped to the EXACT `dir` (the scan
/// canonicalizes it) and only ever reaches processes rooted strictly inside it, so it is safe
/// on any relocated scratch root and never touches a process outside rigger's own dir. Off a
/// platform without `/proc` the reap is a graceful no-op and only the removal runs. This is the
/// shared teardown for the fixpoint scratch-area sweep in [`cmd_step`]; the worktree-removal
/// reap point is [`rigger::worktree::Worktree::remove`].
fn reap_then_remove_dir(dir: &std::path::Path) {
    rigger::reap::reap_processes_rooted_under(dir);
    let _ = std::fs::remove_dir_all(dir);
}

/// Reap any process rooted in a leftover unit worktree `dir` (spec 23), then reclaim the dir -
/// the worktree half of the spec-34 orphan-sweep, the analog of [`reap_then_remove_dir`] for a
/// scratch entry git may still track. A killed step can leave a `rigger-wt-<slug>` worktree
/// still REGISTERED, so a plain `remove_dir_all` would strand a dangling admin entry; `git
/// worktree remove --force` deregisters it (and tolerates a dirty tree). A bare leftover dir
/// git never tracked makes that command fail, so it falls through to the plain removal, and any
/// dangling admin entry a partial removal leaves is pruned by [`rigger::worktree::sweep_terminal`]
/// at the next step start. Best-effort - a failed reclaim never aborts the sweep.
fn reap_then_remove_worktree(repo: &str, dir: &std::path::Path) {
    rigger::reap::reap_processes_rooted_under(dir);
    let deregistered = !repo.is_empty()
        && Command::new("git")
            .arg("-C")
            .arg(repo)
            .args(["worktree", "remove", "--force"])
            .arg(dir)
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);
    if !deregistered {
        let _ = std::fs::remove_dir_all(dir);
    }
}

/// `rigger reported <id>` - exit 0 iff spawn `<id>` already has a recorded result in this
/// project's run stream, and non-zero (a clear error) when it does not.
///
/// A read-only "has this spawn reported yet?" query - it never writes. It was originally
/// the READ half of the driver's two-process check-then-record death-report guard
/// (decision `thin-driver-death-guard`): the courier ran `rigger reported <id> || rigger
/// result <id> --error <why>` so the `--error` landed ONLY when no result existed yet,
/// because recording UNCONDITIONALLY would clobber a self-report ([`spawn::result_of`] is
/// last-write-wins) and force-fail a genuinely successful/approved unit on the next replay.
/// That read-then-write pair left a TOCTOU window (a self-report landing between the check
/// and the record was still clobbered), so the death courier now records atomically via a
/// single `rigger result <id> --if-absent --error <why>` instead (spec 05; the write path
/// is [`spawn::record_result_if_absent`]). This command is retained as a standalone check -
/// e.g. an operator asking whether a spawn is answered - not as the courier's guard.
///
/// Composition mirrors [`cmd_result`]: the store is RESOLVED by walking up to the owning
/// root and scoped by that root's identity (via [`require_store_dir`]), the SAME per-project
/// namespaced sqlite run stream the write half lands in - so the guard and the self-report
/// can never disagree about which store/stream is authoritative (a cwd-relative or cwd-git-
/// worktree read could see "not reported" off-root and clobber a real self-report). The
/// stream is read forward from revision 0 and projected through [`spawn::result_of`] - the
/// exact boundary and projection the replay driver uses to decide answer-vs-park, so this
/// check agrees with the conductor by construction. The namespace-scoped read and its
/// absent/unreported edges live in the testable [`result_of_at`] seam.
fn cmd_reported(args: &[String]) -> Res {
    let id = match args {
        [id] => id.as_str(),
        _ => return Err("reported: expected exactly one spawn id: rigger reported <id>".into()),
    };
    // Resolve the store the SAME way `cmd_result` does - walk UP to the owning root and
    // bind identity to THAT root - so the death-report guard reads the exact namespaced
    // stream a self-report landed in. Reading a cwd-relative store (or the cwd's git-
    // worktree identity) could see "not reported" off-root and clobber a real self-report
    // with an `--error` (arch-reported-result-store-asym). When no store exists up-tree,
    // nothing could have been reported: treat it as unreported (the guard proceeds), the
    // same outcome as `result_of_at`'s absent-db edge, without fabricating a store.
    let reported = match require_store_dir() {
        Ok((loc, sel)) => result_of_at(&loc.file("events.db"), &loc.identity(), id, &sel)?,
        Err(_) => None,
    };
    match reported {
        // Already answered: print a one-line summary (for the courier's log) and exit 0, so
        // the guard's `|| rigger result <id> --error` is SKIPPED and the existing result -
        // the worker's own report - stands untouched.
        Some(res) => {
            println!(
                "{} {}",
                res.id,
                if res.is_error() { "failed" } else { "ok" }
            );
            Ok(())
        }
        // No result yet: exit non-zero (a clear error) so a caller can tell the spawn is still
        // unanswered.
        None => Err(format!("reported: spawn {id:?} has no recorded result yet").into()),
    }
}

/// `rigger prompt <spawn-id>` - print the parked spawn's full prompt (persona + task)
/// on stdout. The thin driver's waves are SLIM manifests (spawn-by-reference): a
/// review-round prompt can run to hundreds of kilobytes, which cannot survive a
/// model-relayed structured output verbatim, so the worker fetches its own prompt
/// straight from the log.
///
/// A store-opening COURIER, invoked BY THE WORKER from inside its unit worktree, so it
/// resolves the store the SAME way `cmd_reported`/`cmd_result` do - walk UP to the owning
/// root and scope by that root's identity (via [`require_store_dir`] /
/// [`StoreLocation::identity`]) - reading the `proj-<repo>-run` stream the conductor parked
/// the spawn in. A cwd-relative `Store::open(&db_path("events.db"))` would instead FABRICATE
/// a fresh empty `.rigger/events.db` inside the worktree (which carries the tracked
/// `.rigger/` but never the gitignored store) and then report "no spawn request recorded"
/// for every id, stranding the worker that ran it - the exact store-opening defect spec 05
/// closes, and the reason this sibling of `cmd_reported` must not stay a parallel un-hardened
/// store-opener.
fn cmd_prompt(args: &[String]) -> Res {
    let id = match args {
        [id] => id.as_str(),
        _ => return Err("prompt: expected exactly one spawn id: rigger prompt <id>".into()),
    };
    let (loc, selection) = require_store_dir()?;
    let backend = resolve_store(&selection, &loc.file("events.db"))?;
    let store = Namespaced::new(backend.as_ref(), &loc.identity());
    let events = store.read_stream(conductor::STREAM, 0, Direction::Forward)?;
    match spawn::prompt_for(&events, id).map_err(|e| e.to_string())? {
        Some(p) => {
            println!("{p}");
            Ok(())
        }
        None => Err(format!("prompt: no spawn request recorded for {id:?}").into()),
    }
}

/// The pure read-model core of `rigger reported`: open the embedded `events.db` at `path`,
/// read `project`'s run stream through the per-project [`Namespaced`] decorator, and return
/// the LATEST recorded result for `id` (or `None` when the spawn is still unreported).
///
/// Split from [`cmd_reported`] (which owns only the I/O boundary and the exit-code decision)
/// so the namespace-scoped read and its absent-db / unreported edges are unit-testable
/// against any backing file, project name, and id - without depending on the process cwd or
/// a real git repo for identity (mirrors [`stats_lines`], decision `d-stats-read-seam`).
///
/// An absent `events.db` (a never-run project) reads as `None` - guarded BEFORE
/// [`Store::open`], which would otherwise create the file - so the guard treats a spawn with
/// no store exactly like a spawn with no result: unreported. The [`Namespaced`] read scopes
/// to `proj-<project>-run`, so a result another project wrote never masks this one.
fn result_of_at(
    path: &str,
    project: &str,
    id: &str,
    sel: &StoreSelection,
) -> Result<Option<spawn::SpawnResult>, Box<dyn std::error::Error>> {
    if sel.is_sqlite() && !Path::new(path).exists() {
        return Ok(None);
    }
    let backend = resolve_store(sel, path)?;
    let store = Namespaced::new(backend.as_ref(), project);
    let events = store.read_stream(conductor::STREAM, 0, Direction::Forward)?;
    Ok(spawn::result_of(&events, id).map_err(|e| e.to_string())?)
}

/// The parsed flags of a `rigger step` invocation.
struct StepArgs {
    /// The spec whose Done-when criteria drive the deterministic decomposition, or
    /// None for an unconstrained step (exactly as `rigger run` uses `--spec`).
    spec: Option<String>,
    /// The ref the run branch is anchored to (`--base`, default [`DEFAULT_BASE_REF`]).
    base: String,
    /// Whether `--base` was passed explicitly (vs. the default). Used to warn only when
    /// an operator's EXPLICIT base is ignored because the run branch already exists -
    /// the steady-state default reuse is silent, an explicit-but-ignored base is not.
    base_explicit: bool,
    /// `--fresh`: begin a NEW run for the spec's criteria before this step, even when the
    /// latest run matches (which the conductor's `ensure_started` would adopt). A ONE-SHOT
    /// the DRIVER passes on the first step of an explicit restart - the evented recovery
    /// from a run wedged in a terminal state whose spec is unchanged; see
    /// [`rigger::run::start_fresh`]. Plain steps after it adopt the boundary it began.
    fresh: bool,
    /// `--rebase-definition` (spec 13, unit 1): on a live-run step whose on-disk definition
    /// drifted from the hash pinned at start, record the supersession and continue on the new
    /// definition instead of HALTING loudly. The operator's explicit mid-campaign-edit escape.
    rebase_definition: bool,
}

/// Parse `rigger step`'s flags: an optional `--spec <path>` (the spec whose Done-when
/// criteria drive the deterministic decomposition, exactly as `rigger run` uses it) and
/// an optional `--base <ref>` (the run-branch base, default [`DEFAULT_BASE_REF`]). Each
/// flag requires its value, and an unknown flag or a bare positional is a clear error,
/// so a typo never silently runs an unconstrained step.
fn parse_step_args(args: &[String]) -> Result<StepArgs, Box<dyn std::error::Error>> {
    let mut spec = None;
    let mut base = None;
    let mut fresh = false;
    let mut rebase_definition = false;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--fresh" => fresh = true,
            "--rebase-definition" => rebase_definition = true,
            "--spec" => {
                i += 1;
                spec = match args.get(i) {
                    Some(p) => Some(p.clone()),
                    None => return Err("step: --spec expects a path".into()),
                };
            }
            "--base" => {
                i += 1;
                base = match args.get(i) {
                    Some(r) => Some(r.clone()),
                    None => return Err("step: --base expects a ref".into()),
                };
            }
            flag if flag.starts_with("--") => {
                return Err(format!("step: unknown flag {flag:?}").into());
            }
            positional => {
                return Err(format!(
                    "step: unexpected positional argument {positional:?}; pass the spec via --spec <path>"
                )
                .into());
            }
        }
        i += 1;
    }
    Ok(StepArgs {
        spec,
        base_explicit: base.is_some(),
        base: base.unwrap_or_else(|| DEFAULT_BASE_REF.to_string()),
        fresh,
        rebase_definition,
    })
}

/// Warn on stderr when the run branch was anchored somewhere OTHER than the base the
/// operator asked for, so a divergence is never silent (the old behavior silently
/// no-op'd an unresolvable base and silently ignored `--base` on every run after the
/// first). `cmd` names the invoking command (e.g. `"rigger step"`, `"rigger run"`) so the
/// advisory reads true for whichever run entry anchored. Any primary output (the step's
/// `{wave,done}` JSON) still goes to stdout untouched; these are stderr advisories, not
/// errors - isolation is intact in every case, only the anchor differs.
///
/// A [`RunBranchSetup::CreatedFromHead`] here is a HEAD fallback with a VALID HEAD: the
/// configured base did not resolve, but the current HEAD is a real commit, so the run branch
/// descends from a reachable base (the operator's own branch) that a PR still applies to - it
/// proceeds and is merely advised here. The genuinely baseless case (an unborn HEAD, nothing
/// to branch from) never reaches this function: it is refused loudly upstream by the spec 38
/// loop-readiness gate ([`refuse_when_base_unreachable`]).
fn warn_on_run_branch_divergence(
    cmd: &str,
    setup: RunBranchSetup,
    base: &str,
    base_explicit: bool,
) {
    match setup {
        RunBranchSetup::CreatedFromHead => eprintln!(
            "{cmd}: base {base:?} did not resolve, so the run branch {RUN_BRANCH:?} was anchored \
             on the current HEAD instead (unit isolation is intact, but not anchored on {base:?}). \
             Fetch the base or pass an existing ref as --base to anchor there."
        ),
        // The run branch already exists and was reused. Reusing the default base every
        // run is the expected steady state and stays silent; only an EXPLICIT --base
        // that got ignored (because re-anchoring would orphan integrated work) is worth a
        // word, so the operator is not left thinking their re-anchor took effect.
        RunBranchSetup::Reused if base_explicit => eprintln!(
            "{cmd}: the run branch {RUN_BRANCH:?} already exists and was reused (its \
             integrated work is preserved); --base {base:?} was NOT applied. Re-anchoring an existing \
             run branch would discard integrated units; to anchor a run on {base:?}, start it on a \
             repo without {RUN_BRANCH:?} (or delete that branch first)."
        ),
        RunBranchSetup::Reused | RunBranchSetup::CreatedFromBase => {}
    }
}

/// Anchor the run branch off `base` before a run entry drives the conductor, so every
/// unit worktree branches off [`RUN_BRANCH`] and integration merges never land on the
/// operator's own branch (spec 18, criterion 6 threads `--base` here). Creates and checks
/// out [`RUN_BRANCH`] off `base` (or off HEAD when `base` does not resolve - the same
/// fallback `cmd_step` uses), reusing an existing run branch untouched. `cmd` labels the
/// command in the error and the divergence advisory. A failure aborts the run with an
/// actionable error rather than driving the conductor on the wrong branch - isolation is a
/// precondition, not best-effort. Callers guard this on a real repo (a repo-less invocation
/// skips run-branch setup entirely). The missing-files base check (criterion 7) runs BEFORE
/// this, gated on [`Worktree::planned_run_branch_setup`], so a wrong-base run is refused
/// without ever creating a branch to anchor here.
fn anchor_run_branch(repo: &str, cmd: &str, base: &str, base_explicit: bool) -> Res {
    let setup = Worktree::ensure_run_branch(repo, RUN_BRANCH, base).map_err(|e| {
        format!(
            "{cmd}: could not prepare the run branch {RUN_BRANCH:?} (base {base:?}): {e}. \
             The run did not start; resolve the git state (e.g. commit or stash a dirty tree) and retry."
        )
    })?;
    warn_on_run_branch_divergence(cmd, setup, base, base_explicit);
    Ok(())
}

/// Loop-readiness gate for run-branch basing (spec 38, criterion 2): REFUSE a run that has NO
/// REACHABLE BASE - no configured base that resolves AND no HEAD commit to fall back to - so the
/// run branch would "branch from nowhere" (an orphan history a pull request cannot apply to).
/// The run stops loudly here instead of silently minting a baseless run branch.
///
/// The refusal is DELIBERATELY narrow. It fires ONLY on a would-be
/// [`RunBranchSetup::CreatedFromHead`] (an absent run branch with an unresolvable base) whose
/// HEAD ALSO does not resolve - a genuinely empty / unborn-HEAD repo. When the configured base
/// does not resolve but the current HEAD IS a real commit, the HEAD fallback anchors the run
/// branch on the operator's own branch: a REACHABLE base the run branch descends from, so a PR
/// still applies. That case proceeds (and [`warn_on_run_branch_divergence`] advises it) - the
/// established CLI contract (`step_creates_run_branch_off_head_when_base_unresolvable`) depends
/// on it. A would-be [`RunBranchSetup::CreatedFromBase`] has a resolvable base and proceeds; a
/// would-be [`RunBranchSetup::Reused`] means the run already exists (its base was vetted at
/// creation), so it is NEVER refused - re-refusing on resume-by-replay would wedge a live run.
///
/// Gates on the side-effect-free PLANNED anchor and a read-only HEAD probe, so a refused run
/// creates no branch: the operator who commits a base (or passes a reachable `--base`) and
/// retries anchors the run FRESH. `cmd` labels the command in the refusal (matching
/// [`refuse_when_base_lacks_spec_paths`]). Run BEFORE [`refuse_when_base_lacks_spec_paths`]:
/// reachability is the more fundamental precondition (a base with no tree cannot have its paths
/// inspected at all).
fn refuse_when_base_unreachable(repo: &str, cmd: &str, base: &str, setup: RunBranchSetup) -> Res {
    if matches!(setup, RunBranchSetup::CreatedFromHead)
        && !rigger::worktree::ref_resolves(repo, "HEAD")
    {
        return Err(format!(
            "{cmd}: no reachable base for the run branch {RUN_BRANCH:?}: the base {base:?} does not \
             resolve and this repo has no commit to fall back to (an unborn HEAD), so the run branch \
             would branch from nowhere - an orphan history a pull request cannot apply to. No run \
             branch was created; commit a base first, or fetch/pass --base <a reachable ref>, then \
             re-run so the run branch is based on the branch it integrates toward."
        )
        .into());
    }
    Ok(())
}

/// Before a run parks its first unit, guard against an operator anchoring the run on the
/// WRONG base: extract the path-like tokens the spec's `criteria` reference and check them
/// against `base`. When the criteria name paths but NONE of them resolve in `base`, that is
/// a strong wrong-base signal - the files the units must edit live on another branch - so
/// REFUSE with an error naming a missing path and the `--base` fix, rather than driving a
/// doomed run whose unit worktrees branch off a tree that lacks those very files. A PARTIAL
/// match only WARNS and proceeds: a spec legitimately names to-be-created files, so the
/// absence of SOME paths is not a wrong-base signal. No path tokens means nothing to check.
///
/// This runs BEFORE the run branch is anchored, gated on the PLANNED anchor
/// ([`Worktree::planned_run_branch_setup`], a side-effect-free peek) rather than an
/// already-created branch. That ordering is what makes the refusal actionable: a refused step
/// creates no run branch, so the operator who obeys the message and retries with a corrected
/// `--base` re-runs this check (which then passes) and anchors the run FRESH on the right base -
/// it can never end up stuck on the wrong-base branch a post-anchor check would have left behind.
///
/// `setup` (the planned anchor) gates WHEN this runs. Only a run branch that WOULD be freshly
/// [`RunBranchSetup::CreatedFromBase`] is at "before a run parks its first unit" with a base that
/// is known to resolve. A would-be REUSED branch means one already exists - a real run is already
/// under way (re-checking every step would spuriously refuse a spec of not-yet-created files
/// mid-run) - and a would-be HEAD fallback ([`RunBranchSetup::CreatedFromHead`]) has no resolvable
/// base to look paths up in. Both skip. `cmd` labels the command in the refusal and the advisory
/// (matching [`anchor_run_branch`] / [`warn_on_run_branch_divergence`]). Spec 18, criterion 7.
fn refuse_when_base_lacks_spec_paths(
    repo: &str,
    cmd: &str,
    base: &str,
    setup: RunBranchSetup,
    criteria: &[String],
) -> Res {
    if !matches!(setup, RunBranchSetup::CreatedFromBase) {
        return Ok(());
    }
    let tokens = spec::path_tokens(criteria);
    if tokens.is_empty() {
        return Ok(());
    }
    // `partition` preserves token order, so `absent[0]` (the path named in either message)
    // is deterministic - the first path-like token the criteria reference, in order.
    let (present, absent): (Vec<&String>, Vec<&String>) = tokens
        .iter()
        .partition(|t| rigger::worktree::path_in_ref(repo, base, t));
    if present.is_empty() {
        // Total absence: the strong wrong-base signal (`absent` is non-empty here because
        // `tokens` was non-empty and none of them are present).
        return Err(format!(
            "{cmd}: the spec's criteria reference {n} path(s) - e.g. {first:?} - but NONE of them \
             exist in the base ref {base:?}. This usually means the base is wrong (the files live \
             on another branch). No run branch was created, so just re-run with --base <your-branch> \
             pointing where these paths exist to anchor the run there.",
            n = absent.len(),
            first = absent[0],
        )
        .into());
    }
    if !absent.is_empty() {
        eprintln!(
            "{cmd}: {n} spec-referenced path(s) are absent from the base ref {base:?} (e.g. \
             {first:?}); proceeding because others are present. If the base is wrong, delete the \
             {RUN_BRANCH} branch and re-run with --base <your-branch>.",
            n = absent.len(),
            first = absent[0],
        );
    }
    Ok(())
}

/// The standalone CLI path: ground, spawn agents as `claude` subprocesses, drive
/// the DAG to integration. The store is selected by flag and wrapped in the
/// per-project namespace decorator before it is injected (§5.1.1, R9).
fn run_cli(parsed: &RunArgs) -> Res {
    // Refuse before starting if a gating persona would stall the integration gate (spec 18,
    // unit 2); `load_run_config` reuses unit 1's lint at this run's config-load seam.
    let cfg = load_run_config(".")?;
    let criteria = load_criteria(parsed.spec.as_deref())?;
    std::fs::create_dir_all(RIGGER_DIR)?;
    // Anchor + check out the run branch off `--base` (spec 18, criterion 6) BEFORE the
    // conductor branches any unit worktree off HEAD, so machine-generated units never
    // branch/merge onto the operator's own branch. `--base` threads here exactly as it does
    // for `rigger step`; the effective base is the flag, then the `RIGGER_BASE` env override
    // (how `rigger workflow` threads its `--base` through the shim), then `origin/main`.
    // Guarded on a real repo, so the repo-less path is untouched.
    let repo = git_repo();
    if !repo.is_empty() {
        let (base, base_explicit) = resolve_run_base(
            parsed.base.as_deref(),
            std::env::var("RIGGER_BASE").ok().as_deref(),
        );
        // Refuse an obviously-wrong base BEFORE anchoring (spec 18, criterion 7), gating on the
        // side-effect-free planned anchor so no wrong-base run branch is ever created and the
        // corrected `--base` retry re-anchors fresh.
        let planned = Worktree::planned_run_branch_setup(&repo, RUN_BRANCH, &base);
        // Loop-readiness gate (spec 38, criterion 2): refuse a run with no reachable base (an
        // unresolvable base AND no HEAD to fall back to) loudly rather than minting a run branch
        // that branches from nowhere.
        refuse_when_base_unreachable(&repo, "rigger run", &base, planned)?;
        refuse_when_base_lacks_spec_paths(&repo, "rigger run", &base, planned, &criteria)?;
        anchor_run_branch(&repo, "rigger run", &base, base_explicit)?;
    }
    // The boxed backend and its namespaced wrapper both live here, in this stack
    // frame, for the whole run: the decorator borrows the concrete store, and both
    // outlive the `conductor::run` call below.
    // Migrate a pre-spec-09 store's legacy-namespace history to the minted identity once,
    // before opening the run backend (spec 09). Local-sqlite only - the migration renames
    // streams in the local `.rigger/events.db`; a shared KurrentDB backend is out of scope.
    let selection = store_selection(parsed.store, parsed.conn.as_deref())?;
    if selection.is_sqlite() {
        migrate_local_identity()?;
    }
    // Register this instance in the machine-global discovery registry (spec 50, criterion 2).
    // `rigger run` drives the WHOLE run in-process through a single `conductor::run` below, so the
    // held guard's heartbeat thread (not a one-shot register) is what keeps the entry from aging out
    // of discovery mid-run; it is dropped when `run_cli` returns. Best-effort - it never blocks the
    // run. Registered here, off the same resolved `selection` `rigger step` uses, so a native
    // `rigger run` is as discoverable as the stepwise loop path.
    let _registration = register_run_instance(&repo, &selection);
    let backend = resolve_store(&selection, &db_path("events.db"))?;
    let store = Namespaced::new(backend.as_ref(), &project_identity());
    // `--fresh`: begin a NEW run before driving, so the conductor's own `ensure_started`
    // adopts this just-minted boundary instead of the (possibly wedged) latest run. See
    // `runscope::start_fresh` - the evented restart for a terminal escalation on an
    // unchanged spec.
    fresh_run_if_requested(parsed, &store, &criteria)?;
    let graph = Projector::open(&db_path("graph.db"), &project_identity())?;
    let driver = cli::Driver::default();
    let grounder = select_grounder(&cfg.workflow.defaults.grounder)?;
    let deps = Deps {
        store: &store,
        driver: &driver,
        gates: &ExecRunner,
        repo,
        grounder: Some(grounder.as_ref()),
        graph: Some(&graph),
        criteria,
    };
    // Always-on dash (spec 19b, unit 1): auto-start a `rigger dash` serving this run before
    // the loop begins, so an active harness is never invisible. Held for the whole run - the
    // guard reaps the dash when this scope ends (unit 3's reaping mechanism).
    let _dash = start_run_dashboard(&store);
    let rs = conductor::run(&cfg, &deps)?;
    // The release-target base the ready-to-release handoff names (spec 38, criterion 3): read
    // the base PERSISTED on this run's RunStarted, so the end-of-run summary, `rigger status`,
    // and `rigger dash` all name the ONE base the run anchored on. A run started before base
    // persistence existed (or without a repo) carries none, so fall back to the same
    // flag/env/default resolution the run branch was anchored with.
    let release_base = store
        .read_stream(conductor::STREAM, 0, Direction::Forward)
        .ok()
        .and_then(|events| runscope::current_run_base(&events))
        .unwrap_or_else(|| {
            resolve_run_base(
                parsed.base.as_deref(),
                std::env::var("RIGGER_BASE").ok().as_deref(),
            )
            .0
        });
    print_run_state(&rs, &release_base);
    // spec 17 criterion 4c: a silently-serializing fleet must WARN during a run, not only when the
    // operator later runs `rigger stats`. Re-project this run's metrics from the log and, if the
    // parallelism-retention floor was breached under structural grounding, log the SAME line the
    // stats row shows (one authority) to stderr. Best-effort: a read hiccup never fails a run that
    // already succeeded, and on the shipped non-symbols default retention is unmeasured so nothing
    // prints and the default run output is unchanged.
    if let Ok(events) = store.read_stream(conductor::STREAM, 0, Direction::Forward) {
        let m = metrics::project(runscope::current_run(&events));
        if let Some(line) = parallelism_retention_line(&m) {
            if m.parallelism_retention_warns() {
                eprintln!("rigger: {line}");
            }
        }
    }
    Ok(())
}

/// Begin (or adopt) and definition-PIN the run both `run` drivers drive (spec 13, unit 1).
/// When `--fresh` is set it appends a new pinned `RunStarted` for `criteria` so the run starts
/// a clean slice even if the latest run already matches (which `ensure_started` would adopt),
/// printing the new run id. It then enforces the definition pin ([`enforce_definition_pin`]):
/// a drifted live-run definition HALTS loudly unless `--rebase-definition` records the
/// supersession and continues. A fresh or unchanged run continues silently.
fn fresh_run_if_requested(
    parsed: &RunArgs,
    store: &dyn EventStore,
    criteria: &[String],
) -> Result<(), Box<dyn std::error::Error>> {
    let definition = definition_hash(".")?;
    // The resolved run-branch base to persist on the RunStarted this mints (spec 38, criterion
    // 3), resolved from the SAME precedence the run branch is anchored with (the `--base` flag,
    // then the `RIGGER_BASE` env override, then the default) so the persisted base matches the
    // branch the run actually targets. Persisted only on a mint (a `--fresh` boundary or a new
    // campaign); an adopted resume keeps its original stamp.
    let (base, _) = resolve_run_base(
        parsed.base.as_deref(),
        std::env::var("RIGGER_BASE").ok().as_deref(),
    );
    if parsed.fresh {
        let run = runscope::start_fresh(store, criteria, &definition, &base)?;
        println!("rigger: --fresh: began a new run {run} (the prior run stays in the log)");
    }
    enforce_definition_pin(
        store,
        criteria,
        &definition,
        parsed.rebase_definition,
        &base,
    )?;
    Ok(())
}

/// The in-Claude-Code MCP-server path (`rigger serve` / `rigger run --driver
/// workflow`): the conductor orchestrates on a background thread and this thread
/// serves the MCP bridge over stdio. The store is selected by flag and wrapped in
/// the per-project namespace decorator before it is injected into BOTH the
/// conductor and the side-car (§5.1.1, R9).
fn run_workflow(parsed: &RunArgs) -> Res {
    // Refuse before starting if a gating persona would stall the integration gate (spec 18,
    // unit 2); `load_run_config` reuses unit 1's lint at this run's config-load seam.
    let cfg = load_run_config(".")?;
    let criteria = load_criteria(parsed.spec.as_deref())?;
    std::fs::create_dir_all(RIGGER_DIR)?;
    // Anchor + check out the run branch off `--base` (spec 18, criterion 6) before the
    // conductor branches any unit worktree off HEAD, mirroring `rigger step`. `rigger
    // workflow` threads its `--base` here through the shim via the inherited `RIGGER_BASE`
    // env (the shim spawns this `rigger serve` with the inherited environment); an explicit
    // `--base` on `rigger serve` / `rigger run --driver workflow` takes precedence. Guarded
    // on a real repo, so the repo-less path is untouched.
    {
        let repo = git_repo();
        if !repo.is_empty() {
            let (base, base_explicit) = resolve_run_base(
                parsed.base.as_deref(),
                std::env::var("RIGGER_BASE").ok().as_deref(),
            );
            // Refuse an obviously-wrong base BEFORE anchoring (spec 18, criterion 7), gating on
            // the side-effect-free planned anchor so no wrong-base run branch is ever created and
            // the corrected `--base` retry re-anchors fresh.
            let planned = Worktree::planned_run_branch_setup(&repo, RUN_BRANCH, &base);
            // Loop-readiness gate (spec 38, criterion 2): refuse a run with no reachable base
            // (an unresolvable base AND no HEAD to fall back to) loudly rather than minting a run
            // branch that branches from nowhere.
            refuse_when_base_unreachable(&repo, "rigger workflow", &base, planned)?;
            refuse_when_base_lacks_spec_paths(&repo, "rigger workflow", &base, planned, &criteria)?;
            anchor_run_branch(&repo, "rigger workflow", &base, base_explicit)?;
        }
    }
    // One-time spec-09 identity migration before opening the run backend (local-sqlite only).
    let selection = store_selection(parsed.store, parsed.conn.as_deref())?;
    if selection.is_sqlite() {
        migrate_local_identity()?;
    }
    // Register this instance in the machine-global discovery registry (spec 50, criterion 2). Like
    // `rigger run`, the served conductor drives the whole run in-process (on the background thread
    // in the scope below), so the held guard's heartbeat thread keeps the entry live for the whole
    // MCP session; it is dropped when `run_workflow` returns. `repo` was resolved in a scoped block
    // above, so read it once more here for the registration root. Best-effort - it never blocks.
    let _registration = register_run_instance(&git_repo(), &selection);
    let backend = resolve_store(&selection, &db_path("events.db"))?;
    let store = Namespaced::new(backend.as_ref(), &project_identity());
    // `--fresh`: begin a NEW run before the conductor thread starts, so its `ensure_started`
    // adopts this boundary rather than the latest (possibly wedged) run.
    fresh_run_if_requested(parsed, &store, &criteria)?;
    let graph = Projector::open(&db_path("graph.db"), &project_identity())?;
    let driver = rigger::driver::workflow::Driver::new();
    let grounder = select_grounder(&cfg.workflow.defaults.grounder)?;
    let peers = rigger::sidecar::Sidecar::start(&store, 0, Filter::default())?;

    // Spec 14: the SEPARATE progress store + scratch root, so the MCP `rigger_activity` tool
    // presents the live per-agent view (this run's progress joined with the frontier and the
    // liveness-marker ages rigger reads in Rust) to the shim over its existing connection -
    // the shim never touches the filesystem. Progress is always the local sqlite sibling of
    // the run store, regardless of the run store's backend.
    let prog_backend = Store::open(&db_path("progress.db"))?;
    let prog_store = Namespaced::new(&prog_backend, &project_identity());
    let scratch_root = {
        let repo = git_repo();
        if repo.is_empty() {
            String::new()
        } else {
            rigger::worktree::scratch_root_from_env(&repo, &cfg.workflow.defaults.workdir)
        }
    };

    // Always-on dash (spec 19b, unit 1): auto-start a `rigger dash` serving this run for the
    // whole MCP session, so an active harness is never invisible. Held here (not inside the
    // scope) so it is reaped when `run_workflow` returns - after the session ends - by unit
    // 3's guard.
    let _dash = start_run_dashboard(&store);

    // The conductor orchestrates in the background; this thread serves the MCP
    // bridge over stdio. The shim drains spawns via rigger_next/result; closing
    // stdin ends the session.
    std::thread::scope(|s| {
        s.spawn(|| {
            let deps = Deps {
                store: &store,
                driver: &driver,
                gates: &ExecRunner,
                repo: git_repo(),
                grounder: Some(grounder.as_ref()),
                graph: Some(&graph),
                criteria,
            };
            if let Err(e) = conductor::run(&cfg, &deps) {
                eprintln!("rigger: conductor: {e}");
            }
            // Signal the run is over so an empty rigger_next reports done:true and the
            // shim exits cleanly. Set on BOTH success and error: a conductor error
            // still ends the run, and the shim must not poll forever.
            driver.finish();
        });
        // Wire the graph into the MCP server too, so a ReviewFinding (or DecisionMade)
        // an agent emits via rigger_emit folds into the graph as it lands - the
        // adversary / adjudicator, which ground afterwards, then retrieve it through
        // `graph_context` (the cross-agent memory the review tiers communicate
        // through), not via the conductor hand-threading prompts.
        let server = rigger::mcpserver::Server::new(&driver, &store, conductor::STREAM, &peers)
            .with_graph(&graph)
            .with_progress(&prog_store, &scratch_root);
        let _ = server.run(std::io::stdin().lock(), std::io::stdout().lock());
    });
    Ok(())
}

fn cmd_serve(args: &[String]) -> Res {
    // `rigger serve` is the equivalent of `rigger run --driver workflow`, so it
    // shares the same flag surface (the event store and its connection string) and
    // the same composition path - it just forces the workflow driver.
    let mut parsed = parse_run_args(args)?;
    parsed.driver = DriverKind::Workflow;
    run_workflow(&parsed)
}

/// Parse `rigger workflow`'s arguments: an optional positional spec path and an optional
/// `--base <ref>` (the run-branch base, spec 18 criterion 6). A second positional, an
/// unknown flag, and a valueless `--base` are clear errors, so a typo never silently
/// changes what runs or which base a run anchors on.
fn parse_workflow_args(
    args: &[String],
) -> Result<(Option<String>, Option<String>), Box<dyn std::error::Error>> {
    let mut spec = None;
    let mut base = None;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--base" => {
                i += 1;
                base = match args.get(i) {
                    Some(r) => Some(r.clone()),
                    None => return Err("workflow: --base expects a ref".into()),
                };
            }
            flag if flag.starts_with("--") => {
                return Err(format!("workflow: unknown flag {flag:?}").into());
            }
            positional => {
                if spec.is_some() {
                    return Err(format!(
                        "workflow: expected at most one spec path, got a second {positional:?}"
                    )
                    .into());
                }
                spec = Some(positional.to_string());
            }
        }
        i += 1;
    }
    Ok((spec, base))
}

/// `rigger workflow [spec] [--base <ref>]` is the turn-key one-command activation of the
/// workflow driver: it execs the Node shim (`shim/shim.mjs`), which spawns `rigger serve`
/// (this same binary, via `RIGGER_BIN`), connects an MCP client to it, and drives the agent
/// loop via the Claude Agent SDK. The user runs ONE command instead of hand-wiring `rigger
/// serve` into an MCP host. `--base` (spec 18, criterion 6) threads to the served run's
/// branch anchor through the inherited `RIGGER_BASE` environment.
fn cmd_workflow(args: &[String]) -> Res {
    // `rigger workflow [spec] [--base <ref>]`: an optional spec path and the run-branch base
    // (spec 18, criterion 6). A second positional or a valueless --base is a clear error.
    let (spec, base) = parse_workflow_args(args)?;
    let shim = locate_shim(Path::new("."))?;
    // The shim spawns `rigger serve` itself; point it at THIS binary so the driver
    // and the served conductor are always the same build (no PATH ambiguity).
    let rigger_bin = std::env::current_exe()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|_| "rigger".to_string());

    let node = std::env::var("RIGGER_NODE").unwrap_or_else(|_| "node".to_string());
    let mut cmd = Command::new(&node);
    cmd.arg(&shim);
    if let Some(spec) = &spec {
        cmd.arg(spec);
    }
    cmd.env("RIGGER_BIN", &rigger_bin);
    // Thread --base to the served `rigger serve` the shim spawns: the shim inherits this
    // process's environment (the same channel it uses for RIGGER_BIN), so RIGGER_BASE reaches
    // `run_workflow`'s run-branch anchor, where `resolve_run_base` reads it. Set only when the
    // operator passed --base, so the no-flag default (origin/main) is unchanged.
    if let Some(base) = &base {
        cmd.env("RIGGER_BASE", base);
    }

    let status = cmd.status().map_err(|e| {
        format!(
            "workflow: failed to launch the Node driver ({node} {shim}): {e}. \
             Is Node installed and on your PATH? Run `rigger setup` if the JS driver's \
             dependencies are not yet installed."
        )
    })?;
    if !status.success() {
        return Err(format!("workflow: the Node driver exited unsuccessfully ({status})").into());
    }
    Ok(())
}

/// Locate the JS driver's `shim.mjs` to run, rooted at the project `root`.
///
/// `rigger workflow` runs the PER-PROJECT shim that `rigger setup` provisions
/// (`<root>/.rigger/shim/shim.mjs`), so the driver and its installed `node_modules`
/// travel with the project, not the binary. Search order:
///   1. the `RIGGER_SHIM` env override (an explicit path) - the escape hatch for a
///      custom or dev shim;
///   2. the provisioned per-project shim at `<root>/.rigger/shim/shim.mjs`.
///
/// When neither exists the error tells the user to run `rigger setup` (which
/// provisions `.rigger/shim/` and installs its deps), rather than leaving them to
/// hand-wire a shim. A `RIGGER_SHIM` override that points at a missing path is a
/// clear error, never a silent fallthrough.
fn locate_shim(root: &Path) -> Result<String, Box<dyn std::error::Error>> {
    if let Ok(explicit) = std::env::var("RIGGER_SHIM") {
        if Path::new(&explicit).exists() {
            return Ok(explicit);
        }
        return Err(format!("workflow: RIGGER_SHIM={explicit} does not exist").into());
    }
    let provisioned = shim_dir(root).join("shim.mjs");
    if provisioned.exists() {
        return Ok(provisioned.to_string_lossy().into_owned());
    }
    Err(format!(
        "workflow: the per-project JS driver is not provisioned (looked for {}). \
         Run `rigger setup` to write the shim into .rigger/shim/ and install its \
         dependencies, then re-run `rigger workflow`.",
        provisioned.display()
    )
    .into())
}

/// Extract the spec's acceptance criteria, enforcing the loop-ready gate (§8): a
/// spec with no enumerable Done-when criteria blocks until a human adds them; no
/// spec path means an unconstrained run (empty criteria).
fn load_criteria(spec_path: Option<&str>) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let Some(spec_path) = spec_path else {
        return Ok(Vec::new());
    };
    let text =
        std::fs::read_to_string(spec_path).map_err(|e| format!("read spec {spec_path}: {e}"))?;
    let criteria = spec::extract_criteria(&text);
    if criteria.is_empty() {
        return Err(format!(
            "loop-ready: spec {spec_path} has no enumerable Done-when criteria (checkbox items); add them before running"
        )
        .into());
    }
    Ok(criteria)
}

fn cmd_graph(args: &[String]) -> Res {
    // `rigger graph build` is a distinct verb (populate) from the default `--around` inspector
    // read: fold the project's source into the graph from a cold checkout, no run required.
    if args.first().map(String::as_str) == Some("build") {
        return cmd_graph_build(&args[1..]);
    }
    // `rigger graph communities` is the OFFLINE detection pass (spec 53): derive the code lens's
    // coupling communities over the already-folded structure layer and record them as events.
    if args.first().map(String::as_str) == Some("communities") {
        return cmd_graph_communities(&args[1..]);
    }
    // `rigger graph concepts` is the OFFLINE intent-derivation pass (spec 54): derive the concepts
    // lens's grouping over the already-folded intent layer and record them as events.
    if args.first().map(String::as_str) == Some("concepts") {
        return cmd_graph_concepts(&args[1..]);
    }
    let mut around = String::new();
    let mut show = String::new();
    let mut depth: i64 = 2;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--around" => {
                i += 1;
                around = args.get(i).cloned().unwrap_or_default();
            }
            "--show" => {
                i += 1;
                show = args.get(i).cloned().unwrap_or_default();
            }
            "--depth" => {
                i += 1;
                depth = args.get(i).and_then(|d| d.parse().ok()).unwrap_or(2);
            }
            _ => {}
        }
        i += 1;
    }
    // `rigger graph --show <entity>` is the TEXT half of lookup (spec 58): the definition site and
    // body, beside `--around`'s structural neighborhood. Dispatched before the `--around` guard so
    // a `--show` query needs no `--around`.
    if !show.is_empty() {
        return cmd_graph_show(&show);
    }
    if around.is_empty() {
        return Err("graph: --around <id> or --show <entity> is required".into());
    }
    let gp = Projector::open(&db_path("graph.db"), &project_identity())?;
    let g = gp.subgraph(&[around.clone()], depth)?;
    println!("subgraph around {around:?} (depth {depth}):");
    for n in &g.nodes {
        println!("  node {:<24} {}", n.id, n.kind);
    }
    for e in &g.edges {
        println!("  edge {} -{}-> {}", e.from, e.rel, e.to);
    }
    if g.nodes.is_empty() {
        println!("  (nothing found; has `rigger run` been run yet?)");
    }
    Ok(())
}

/// The upper bound on how many body lines `rigger graph --show` prints (spec 58): the definition's
/// extent comes from the grammar's own node boundary, but a very long body is clamped to this window
/// so the surface never dumps an unbounded body. A clamp is announced with an explicit note (the
/// omitted-line count), so a bounded body is never mistaken for the whole definition.
const SHOW_MAX_BODY_LINES: u32 = 60;

/// `rigger graph --show <entity>` (spec 58, criterion 1) - the TEXT half of graph lookup, beside
/// `--around`'s structural neighborhood. It resolves the entity through [`Projector::locate`] (a
/// full `<file>::<name>` id, or a bare name via the pinned name-suffix match), then:
///
/// - ONE match: prints the definition SITE (`file:line`), the entity's KIND and one-hop DEGREE, and
///   the definition BODY read from the WORKING TREE at that location - line-numbered and bounded by
///   the extent the shared multi-grammar symbols authority derives (the grammar's own node
///   boundary), clamped to a max window. A missing file, a recorded line past end-of-file, a drifted
///   location the current tree no longer matches, or a build without the extraction grammar degrades
///   to the site plus an explicit note, never an error (the recorded graph facts are still shown;
///   only the body is unavailable, and the surface says so).
/// - MANY matches (an ambiguous bare name): LISTS the sorted candidates, each with its file, and
///   prints NO body - the graph's honesty rule is never to guess among candidates.
/// - NONE: a one-line not-found note (never an error), mirroring `--around`'s empty result.
///
/// Read-only over the projection and the working tree; deterministic for a given tree and graph.
fn cmd_graph_show(entity: &str) -> Res {
    let gp = Projector::open(&db_path("graph.db"), &project_identity())?;
    match gp.locate(entity)? {
        Located::None => {
            println!(
                "show {entity:?}: no such entity in the graph (has it been built? try `rigger graph build`)"
            );
        }
        Located::Many(cands) => {
            println!(
                "show {entity:?}: {} candidates - the name is ambiguous, re-run --show on one id:",
                cands.len()
            );
            for c in &cands {
                println!("  {}   ({})", c.id, c.file);
            }
        }
        Located::One(site) => print_entity_site(&site),
    }
    Ok(())
}

/// Print one located entity for `rigger graph --show` (spec 58): the site/kind/degree header, then
/// the line-numbered body bounded through the shared multi-grammar symbols authority - or an
/// explicit note (a drifted location, or a build without the extraction grammar) in place of the
/// body, so the surface is never silently wrong (a graceful degrade, never an error).
fn print_entity_site(site: &contextgraph::sqlite::EntitySite) {
    let kind = if site.kind.is_empty() {
        "?"
    } else {
        site.kind.as_str()
    };
    // The definition name is the id's suffix after the first `::` (a file path never contains one),
    // the twin of the `<file>::<name>` id `locate` resolved - used to match the working-tree extent.
    let name = site
        .id
        .split_once("::")
        .map(|(_, n)| n)
        .unwrap_or(site.id.as_str());
    println!("show {}", site.id);
    println!(
        "  site: {}:{}   kind {}   degree {}",
        site.file, site.line, kind, site.degree
    );
    match definition_body(&site.file, site.line, name) {
        ShowBody::Lines {
            lines,
            omitted,
            extent_end,
        } => {
            for (n, text) in lines {
                println!("  {n:>6} | {text}");
            }
            if omitted > 0 {
                // The extent ran past the max window: print an explicit clamp note (the omitted
                // count and the extent's true last line) so a bounded body is never read as whole.
                println!(
                    "  (body clamped to {SHOW_MAX_BODY_LINES} lines; {omitted} more line(s) omitted, through line {extent_end})"
                );
            }
        }
        ShowBody::Note(reason) => println!("  ({reason})"),
    }
}

/// The outcome of bounding a located definition's body for `rigger graph --show` (spec 58).
enum ShowBody {
    /// The line-numbered body window `[start, end]`: `omitted` is how many lines were dropped past
    /// the [`SHOW_MAX_BODY_LINES`] clamp (`0` when the whole extent fit), and `extent_end` is the
    /// extent's true last line, so the caller can print an honest clamp note when `omitted > 0`.
    Lines {
        lines: Vec<(u32, String)>,
        omitted: u32,
        extent_end: u32,
    },
    /// No body could be shown; the string is the human reason (a drifted working-tree location, or
    /// a build compiled without the extraction grammar). Printed in place of the body so the show
    /// surface degrades honestly, never guessing or silently truncating.
    Note(String),
}

/// Bound and read a located definition's body from the WORKING TREE for `rigger graph --show`
/// (spec 58). The file is read relative to the git top-level (so a `--show` launched from a
/// subdirectory still finds it), falling back to the cwd outside a git context.
///
/// The extent is derived through the SHARED multi-grammar symbols authority, not a hand-rolled
/// per-language lexer: [`derive_extent_end`] resolves the file's grammar via the symbols registry
/// and reads the definition's END line from the grammar's OWN tree-sitter node boundary. So a
/// braced language's closing brace, a Python block's dedent, a Go backtick raw string, and a JS
/// single-quote string carrying a lone `{` are all bounded correctly by the parser - including a
/// signature that itself carries a brace (a struct-destructuring parameter, an `= {}` default) and
/// a definition that CONTAINS a nested `fn`/item (its extent spans the child, never truncates at
/// it). The window is `[start, extent]`, clamped by [`SHOW_MAX_BODY_LINES`]; a clamp reports its
/// omitted-line count so a bounded body is never read as whole.
///
/// Returns [`ShowBody::Note`] - the caller prints it in place of the body, never an error - when the
/// body cannot be shown honestly: the recorded `start` line is `0` or past end-of-file, the file
/// cannot be read (a drifted or unknown location), the current tree no longer holds a definition of
/// that name at that line (a stale location), or this build has no extraction grammar (the light,
/// `--no-default-features` lane). It never GUESSES a body from a structural next-definition bound.
fn definition_body(file: &str, start: u32, name: &str) -> ShowBody {
    // A recorded line of 0 never named a real source line: degrade before any read.
    if start == 0 {
        return ShowBody::Note(format!(
            "source unavailable at {file}:{start}; the recorded location may be stale"
        ));
    }
    let root = git_repo();
    let path = if root.is_empty() {
        std::path::PathBuf::from(file)
    } else {
        std::path::Path::new(&root).join(file)
    };
    let Ok(text) = std::fs::read_to_string(&path) else {
        return ShowBody::Note(format!(
            "source unavailable at {file}:{start}; the recorded location may be stale"
        ));
    };
    let all: Vec<&str> = text.lines().collect();
    let total = all.len() as u32;
    if start > total {
        // The recorded line is past end-of-file: the location drifted.
        return ShowBody::Note(format!(
            "source unavailable at {file}:{start}; the recorded location may be stale"
        ));
    }
    // Derive the extent's end line through the ONE multi-grammar authority. A miss (a drifted
    // location, or a light-lane build with no grammar) is an explicit note, never a guessed body.
    let extent_end = match derive_extent_end(file, &text, start, name) {
        Ok(end) => end.min(total),
        Err(why) => return ShowBody::Note(why),
    };
    // The max window: never dump an unbounded body. A clamp keeps the extent's true end so the
    // caller can announce the omitted lines.
    let window_cap = start.saturating_add(SHOW_MAX_BODY_LINES).saturating_sub(1);
    let printed_end = extent_end.max(start).min(window_cap);
    let omitted = extent_end.saturating_sub(printed_end);
    let lines = (start..=printed_end)
        .map(|n| (n, all[(n - 1) as usize].to_string()))
        .collect();
    ShowBody::Lines {
        lines,
        omitted,
        extent_end,
    }
}

/// The 1-based, inclusive END line of the definition named `name` at site line `start` in `source`,
/// derived through the shared multi-grammar symbols authority (spec 58). It resolves the file's
/// grammar via the symbols registry and reads the extent from [`definition_extents`], the SAME
/// tree-sitter tag mechanism the code graph is extracted with - so ONE extent authority generalizes
/// across every ingested grammar rather than a Rust-only brace lexer in this composition root.
///
/// Matches on BOTH name and site line (a definition that has moved off `start` no longer matches, so
/// a drifted location degrades to a note rather than a wrong body); when several definitions share
/// the name and line, the widest extent (the outermost construct) wins. Returns `Err` with a human
/// reason - the caller degrades to a note - when no grammar is registered for the file's extension,
/// the grammar cannot tag it, or the current tree holds no such definition at that line.
#[cfg(feature = "symbols")]
fn derive_extent_end(file: &str, source: &str, start: u32, name: &str) -> Result<u32, String> {
    use rigger::grounder::symbols::{extract, registry};
    let Some(entry) = registry::for_path(file, None) else {
        return Err(format!(
            "no code-extraction grammar is registered for {file}; the body extent is unavailable"
        ));
    };
    let extents = extract::definition_extents(source, &entry.language, entry.tags_query)?;
    extents
        .into_iter()
        .filter(|d| d.name == name && d.start_line == start)
        .map(|d| d.end_line)
        .max()
        .ok_or_else(|| {
            format!(
                "no definition named {name:?} at line {start} in the current working tree; the recorded location may be stale"
            )
        })
}

/// Light-lane [`derive_extent_end`]: a build WITHOUT the `symbols` feature links no grammar, so the
/// extent cannot be derived. It returns an explicit reason the caller prints as a note - the show
/// surface stays honest ("the body needs the extraction grammar this build omits") rather than
/// falling back to a hand-rolled lexer that would mis-read the very grammars the graph ingests.
#[cfg(not(feature = "symbols"))]
fn derive_extent_end(_file: &str, _source: &str, _start: u32, _name: &str) -> Result<u32, String> {
    Err(
        "the body extent needs the code-extraction grammar; this build was compiled without the `symbols` feature"
            .to_string(),
    )
}

/// `rigger graph build` - fold the project's source into `.rigger/graph.db` from a COLD checkout
/// (spec 45): no run, no `RunStarted`, no event beyond the code-ingest events the fold already
/// emits, so the graph exists on any repo the tool has merely cloned - not only ones a run has
/// driven. It reuses the SAME walk-and-content-key ingest authority ([`rigger::ingest::ingest_project`])
/// the live run uses; only this standalone entry is new, so a build and a run can never fork the
/// key an event is deduped under.
///
/// Store lifecycle mirrors the RUN DRIVER, not the couriers: it CREATES the store under the cwd's
/// `.rigger/` when absent (a cold checkout legitimately has none yet - this command's whole point
/// is to populate it) rather than the courier walk-up that refuses a missing store. On an EXISTING
/// store it refreshes incrementally through the ONE shared suppression predicate
/// ([`rigger::ingest::project_scoped_replay_keys`]) the live run also seeds from, never a second
/// copy here. The seen-key set lives in two phases: it is SEEDED with the keys of each file's
/// LATEST recorded derived-index batch and no earlier one, then EXTENDED with every key this build
/// appends, retiring nothing - so "latest generation per file" describes the seed, not the set
/// after the first batch. The SEED is what every suppression decision is taken against, because one
/// walk hands this command each batch identity (`gc`/`gd` per file) exactly once and this command
/// walks once. On that seed an unchanged file's batch
/// is already wholly recorded and re-ingests nothing, while a file whose content AS THE WALK LOWERED
/// IT differs from its latest recorded batch re-emits every event the walk extracted for it. That
/// includes a file REVERTED to content it held at an earlier generation - its keys are byte-identical
/// to records the log still carries, and it re-emits precisely because those records are no longer
/// that file's latest generation. The qualifier is load-bearing and the two halves differ on it: the
/// design half reads the LIVE tree, while the code half lowers from the PERSISTED symbols index when
/// the project has one, so on such a project the decision is taken against what that index holds.
/// Both halves of that are claims about what this command APPENDS, over the files the walk emits a
/// batch for: a file the walk hands over NO batch for - one the walk no longer sees, or one whose
/// extraction the walk lowered to nothing - reaches no suppression decision here at all and retires
/// nothing, whereas a path the tree has DELETED that the persisted index still lists IS handed over
/// and does reach one. And a batch whose append lands but whose fold does not leaves the log right
/// and the graph behind (`append_and_fold_batch` folds best-effort by contract). What a re-emitted batch RETIRES is the FOLD's doing and reaches the code half only:
/// a code batch carries a `fresh` head whose 29a mechanism supersedes that file's prior structural
/// edges, while a design batch sets no `fresh` head, so re-emitting one adds edges without retiring
/// the ones its earlier generation left live. The light lane compiles no extraction pass, so
/// `graph build` there degrades to an empty graph (it still creates the store) and exits 0, never
/// an error.
fn cmd_graph_build(_args: &[String]) -> Res {
    // Bootstrap the store like `run`/`step` do (create-or-open under the cwd's `.rigger/`), NOT the
    // courier `require_store_dir` walk-up that refuses when none exists.
    std::fs::create_dir_all(RIGGER_DIR)?;
    let selection = store_selection(None, None)?;
    let backend = resolve_store(&selection, &db_path("events.db"))?;
    let store = Namespaced::new(backend.as_ref(), &project_identity());
    let graph = Projector::open(&db_path("graph.db"), &project_identity())?;

    // The tree to fold: the git top-level, so a build launched from a subdirectory still ingests
    // the WHOLE project (the same root a run's `deps.repo` carries), falling back to the cwd
    // outside any git context.
    let root = {
        let top = git_repo();
        if top.is_empty() {
            ".".to_string()
        } else {
            top
        }
    };

    // Seed the seen-keys from the existing log so a re-build refreshes incrementally (spec 45: "on
    // an existing one it refreshes incrementally"), through the ONE predicate that owns the
    // content-key format ([`rigger::ingest::project_scoped_replay_keys`]) - the same predicate the
    // run's `replayed_keys` seeding calls, never a second copy here, so a build and a run can never
    // disagree about which recorded key a fresh emit is redundant against.
    //
    // The predicate is TYPE-FIRST and LATEST-PER-FILE, which is what this seeding used to get
    // wrong in both directions: it collected EVERY event's replay key with no type test (so a
    // non-derived key could suppress a derived emit that happened to share its spelling) and it
    // treated a key as redundant whenever it had EVER been recorded (so a file reverted to content
    // it held at an earlier generation re-emitted nothing and stranded the graph on the superseded
    // version). Only the LATEST recorded generation of each file suppresses now.
    let prior = store.read_stream(conductor::STREAM, 0, Direction::Forward)?;
    let mut seen: std::collections::HashSet<String> =
        rigger::ingest::project_scoped_replay_keys(&prior);

    // The walk and content key are the shared authority; the cold build's emit SINK appends each
    // file's WHOLE batch to the run stream in ONE append and folds it into the graph in ONE
    // transaction (the code-ingest fold), skipping any key already seen. This is spec 49's
    // batched-fold cadence: the measured cold-build throughput was transaction-cadence bound, so a
    // per-file batch pays the store's transaction cost ONCE, not once per event. There is no run to
    // stamp, so the events carry no run id - matching the run's own ingest events when no run id is
    // set. The batched append-and-fold + position assignment is the SAME shared authority the run's
    // keyed sink uses ([`rigger::ingest::append_and_fold_batch`]), so a build and a run fold a file's
    // batch identically.
    let mut appended = 0usize;
    rigger::ingest::ingest_project_batched(&root, |keyed| {
        // Keep only the not-yet-seen events of this file's batch, stamping each survivor with its
        // replay key (the same content-keyed dedup a run seeds from the log). A batch already wholly
        // recorded (an unchanged file on a re-build) survives to nothing and appends nothing.
        // `insert` both TESTS and EXTENDS `seen`: every key this build keeps joins the set and no
        // superseded generation is retired from it, so from here on `seen` is the log-derived seed
        // PLUS this build's own emissions. That is harmless because the walk yields each batch
        // identity (`gc`/`gd` per file) once and this command walks once, so no later batch is ever
        // weighed against a key an earlier one added.
        let survivors: Vec<Event> = keyed
            .iter()
            .filter(|(key, _)| seen.insert(key.clone()))
            .map(|(key, ev)| {
                (*ev)
                    .clone()
                    .with_meta(conductor::META_REPLAY_KEY, key.as_str())
            })
            .collect();
        // Fold best-effort, exactly as the run's batched append-and-fold does: a fold failure must
        // not fail the ingest, which already landed durably in the log.
        match rigger::ingest::append_and_fold_batch(
            &store,
            Some(&graph as &dyn Projection),
            conductor::STREAM,
            &survivors,
        ) {
            Ok(_) => appended += survivors.len(),
            Err(e) => eprintln!("graph build: skipping a batch that failed to append: {e}"),
        }
    });

    println!(
        "graph build: ingested {appended} code-ingest event(s) into {}",
        db_path("graph.db")
    );
    Ok(())
}

/// `rigger graph communities [--resolution <r>]` - the OFFLINE, DETERMINISTIC community-detection
/// pass (spec 53, the CODE lens). It reads the project's already-folded coupling layer (the live
/// `CALLS` / `REFERENCES` / `CONTAINS` edges among code-entity / file nodes), runs modularity-based
/// detection over it at the given `--resolution` grain (default [`community::DEFAULT_RESOLUTION`]),
/// and RECORDS the result as `CommunityAssigned` events - so the derived grouping is event-sourced
/// (the `IN_COMMUNITY` membership edges are a rebuildable fold of the log), never computed at request
/// time. Re-running at a resolution with a NON-empty result supersedes only that grain's prior
/// assignments (the fold's `fresh` boundary), so distinct grains coexist and the lens reads one live
/// set per grain.
///
/// Store lifecycle mirrors `graph build` (the composition root, not the courier walk-up): it
/// CREATES the store under the cwd's `.rigger/` when absent, then reads the WHOLE projection and
/// appends-and-folds the pass's events in ONE batch. Detection is always-compiled and reads only
/// folded edges, so this runs identically in both feature lanes; a graph with no coupling edges
/// detects nothing and records nothing (exit 0, never an error). Because an empty result records NO
/// events, an empty re-run is KEEP-LAST-GOOD: it does NOT clear a grain's prior assignment - the last
/// NON-empty pass at that resolution stays live (see `community::events`, decision
/// d-u53c2-empty-rerun-keep-last-good). A real subsystem removal is a SHRINK - a smaller NON-empty
/// result - which DOES supersede via the `fresh` boundary and drops the emptied community's node.
fn cmd_graph_communities(args: &[String]) -> Res {
    let mut resolution = community::DEFAULT_RESOLUTION;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--resolution" => {
                i += 1;
                let raw = args.get(i).cloned().unwrap_or_default();
                resolution = raw.parse::<f64>().map_err(|_| {
                    format!("graph communities: --resolution expects a number, got {raw:?}")
                })?;
                if !(resolution.is_finite() && resolution > 0.0) {
                    return Err(format!(
                        "graph communities: --resolution must be a positive finite number, got {resolution}"
                    )
                    .into());
                }
            }
            other => {
                return Err(format!("graph communities: unknown argument {other:?}").into());
            }
        }
        i += 1;
    }

    // Bootstrap the store like `graph build`/`run` do (create-or-open under the cwd's `.rigger/`).
    std::fs::create_dir_all(RIGGER_DIR)?;
    let selection = store_selection(None, None)?;
    let backend = resolve_store(&selection, &db_path("events.db"))?;
    let store = Namespaced::new(backend.as_ref(), &project_identity());
    let graph = Projector::open(&db_path("graph.db"), &project_identity())?;

    // Read the WHOLE live projection and detect communities over its coupling layer. `whole()` is
    // the same direct, project-scoped, sorted read the dash's `/api/graph` provider consults.
    let whole = graph.whole()?;
    let coupling = community::Coupling::from_graph(&whole);
    let assignment = community::detect(&coupling, resolution);
    let events = community::events(&assignment);

    // Append the pass's events in ONE store append and fold them in ONE transaction (the shared
    // batched append-and-fold authority). The `fresh` head supersedes this grain's prior
    // memberships; the rest re-add, so a re-run REPLACES this resolution's assignment set.
    rigger::ingest::append_and_fold_batch(
        &store,
        Some(&graph as &dyn Projection),
        conductor::STREAM,
        &events,
    )?;

    println!(
        "graph communities: detected {} communit{} over {} coupled node(s) at resolution {} ({} membership event(s) recorded into {})",
        assignment.num_communities,
        if assignment.num_communities == 1 { "y" } else { "ies" },
        coupling.len(),
        resolution,
        events.len(),
        db_path("graph.db")
    );
    Ok(())
}

/// `rigger graph concepts [--resolution <r>]` - the OFFLINE, DETERMINISTIC intent-derivation pass
/// (spec 54, the CONCEPTS lens). It reads the project's already-folded INTENT layer (the live
/// `SPECIFIES` / `CONSTRAINS` / `GOVERNS` / `explains` / `references` edges among design docs,
/// handbook rules, specs, rationale, and the code they attach to), runs the SAME deterministic
/// community detection the code lens ships over it at the given `--resolution` grain (default
/// [`concepts::DEFAULT_RESOLUTION`]), and RECORDS the result as `ConceptDerived` / `ConceptRealized`
/// events - so the derived grouping is event-sourced (the `REALIZES` membership edges are a
/// rebuildable fold of the log), never computed at request time. Re-running at a resolution with a
/// NON-empty result supersedes only that grain's prior grouping (the fold's `fresh` boundary), so
/// distinct grains coexist and the lens reads one live set per grain.
///
/// Store lifecycle mirrors `graph communities` (the composition root, not the courier walk-up): it
/// CREATES the store under the cwd's `.rigger/` when absent, then reads the WHOLE projection and
/// appends-and-folds the pass's events in ONE batch. The derivation is always-compiled and reads only
/// folded edges, so this runs identically in both feature lanes; a graph with no intent edges derives
/// nothing and records nothing (exit 0, never an error). Because an empty result records NO events, an
/// empty re-run is KEEP-LAST-GOOD: it does NOT clear a grain's prior grouping - the last NON-empty
/// pass at that resolution stays live (see `concepts::events`).
fn cmd_graph_concepts(args: &[String]) -> Res {
    let mut resolution = concepts::DEFAULT_RESOLUTION;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--resolution" => {
                i += 1;
                let raw = args.get(i).cloned().unwrap_or_default();
                resolution = raw.parse::<f64>().map_err(|_| {
                    format!("graph concepts: --resolution expects a number, got {raw:?}")
                })?;
                if !(resolution.is_finite() && resolution > 0.0) {
                    return Err(format!(
                        "graph concepts: --resolution must be a positive finite number, got {resolution}"
                    )
                    .into());
                }
            }
            other => {
                return Err(format!("graph concepts: unknown argument {other:?}").into());
            }
        }
        i += 1;
    }

    // Bootstrap the store like `graph communities` does (create-or-open under the cwd's `.rigger/`).
    std::fs::create_dir_all(RIGGER_DIR)?;
    let selection = store_selection(None, None)?;
    let backend = resolve_store(&selection, &db_path("events.db"))?;
    let store = Namespaced::new(backend.as_ref(), &project_identity());
    let graph = Projector::open(&db_path("graph.db"), &project_identity())?;

    // Read the WHOLE live projection and derive concepts over its intent layer. `whole()` is the same
    // direct, project-scoped, sorted read the dash's `/api/graph` provider consults.
    let whole = graph.whole()?;
    let layer = concepts::intent_layer(&whole);
    let derivation = concepts::derive(&whole, &layer, resolution);
    let events = concepts::events(&derivation);

    // Append the pass's events in ONE store append and fold them in ONE transaction (the shared
    // batched append-and-fold authority). The `fresh` head supersedes this grain's prior grouping;
    // the rest re-add, so a re-run REPLACES this resolution's concept set.
    rigger::ingest::append_and_fold_batch(
        &store,
        Some(&graph as &dyn Projection),
        conductor::STREAM,
        &events,
    )?;

    println!(
        "graph concepts: derived {} concept{} over {} intent-linked node(s) at resolution {} ({} event(s) recorded into {})",
        derivation.num_concepts,
        if derivation.num_concepts == 1 { "" } else { "s" },
        layer.len(),
        resolution,
        events.len(),
        db_path("graph.db")
    );
    Ok(())
}

/// `rigger stats` - print the operator metrics for the current project's run: the
/// implement -> review loop's first-pass yield, per-gate remediation (pass/fail)
/// counts, escalation rate, and review approve/reject counts.
///
/// Composition mirrors `run_cli` (decision `d-stats-namespace`): resolve this project's
/// identity and `.rigger/events.db` path, then delegate to [`stats_lines`], which opens
/// the db via [`Store`], wraps it in the per-project [`Namespaced`] decorator, reads the
/// conductor's run stream ([`conductor::STREAM`]) forward - the same stream and boundary
/// the conductor itself replays its run state from - and folds it through the pure
/// [`metrics::project`] read-model.
///
/// Both no-run edges (absent db, empty namespaced run stream) come back from
/// [`stats_lines`] as `None` and print the same clear "no runs yet" message instead of
/// an empty table or a panic (decision `d-stats-absent-guard`); see that function for
/// the per-edge rationale.
///
/// `rigger stats` takes no arguments; any extra argument is a clear error.
fn cmd_stats(args: &[String]) -> Res {
    // `rigger stats` reports the LATEST run; `rigger stats --all` reports the historical
    // aggregate over every run in the store (spec 06, unit 1); `rigger stats --canary`
    // reports the judge-the-judges scorecard of the latest canary run (spec 13, unit 5).
    // No other argument is accepted.
    if let [flag] = args {
        if flag == "--canary" {
            return cmd_stats_canary();
        }
    }
    let all = match args {
        [] => false,
        [flag] if flag == "--all" => true,
        _ => {
            return Err(format!(
                "stats: expected no arguments, --all, or --canary, got {}",
                args.join(" ")
            )
            .into())
        }
    };

    // Resolve the project identity and db path the same way every CLI command does,
    // then delegate the namespace-scoped read + no-runs decision to `stats_lines`. This
    // wrapper owns only the I/O boundary (which file, which project, and the printing);
    // the read-model edges live in the testable seam below.
    let selection = store_selection(None, None)?;
    match stats_lines(&db_path("events.db"), &project_identity(), all, &selection)? {
        Some(lines) => {
            for line in lines {
                println!("{line}");
            }
        }
        // No run to report on - absent db (never-run project) or an empty namespaced
        // run stream. One clear message for both edges.
        None => println!("{NO_RUNS_MESSAGE}"),
    }
    Ok(())
}

/// The pure read-model core of `rigger stats`: open the embedded `events.db` at `path`,
/// read `project`'s `run` stream through the per-project [`Namespaced`] decorator, and
/// fold it into the printable metric lines - returning `None` for the two "no runs yet"
/// edges so [`cmd_stats`] prints one clear message for both (decision `d-stats-read-seam`).
///
/// Split out from [`cmd_stats`] so the namespace-scoped read and its empty/absent edges
/// are unit-testable against any backing file and project name, without depending on the
/// process cwd or a real git repo for identity (which `project_identity` derives).
///
/// `None` is returned for two edges (decision `d-stats-absent-guard`):
///   1. **absent db** - a project that has never run has no `events.db`. We guard BEFORE
///      [`Store::open`], which (via `Connection::open`) would create the file and mask a
///      never-run project as an empty one. This mirrors [`cmd_prime`]'s absent-db guard.
///   2. **empty run stream** - the db exists (some other command, or another project
///      sharing the backend, created it) but *this* project's namespaced `run` stream
///      holds no events. The [`Namespaced`] read scopes to `proj-<project>-run`, so an
///      event another project wrote, or one this project wrote to a different stream,
///      does not leak into the count.
fn stats_lines(
    path: &str,
    project: &str,
    all: bool,
    sel: &StoreSelection,
) -> Result<Option<Vec<String>>, Box<dyn std::error::Error>> {
    if sel.is_sqlite() && !Path::new(path).exists() {
        return Ok(None);
    }

    let backend = resolve_store(sel, path)?;
    let store = Namespaced::new(backend.as_ref(), project);
    // The conductor projects its run state from STREAM read forward from revision 0
    // (inclusive); read the same stream the same way so the metrics fold sees exactly
    // the run the conductor drove, scoped to this project's namespace.
    let events = store.read_stream(conductor::STREAM, 0, Direction::Forward)?;
    if events.is_empty() {
        return Ok(None);
    }

    // Default to the LATEST run's slice; `--all` folds the whole stream for the
    // historical aggregate (spec 06, unit 1). `metrics::project` stays a pure fold over
    // whichever slice it is handed - the run choice lives here, at the read boundary.
    let scoped = if all {
        &events[..]
    } else {
        runscope::current_run(&events)
    };
    Ok(Some(format_stats(&metrics::project(scoped))))
}

/// The message printed when there is no run to report on - either the project has
/// never run (no `events.db`) or its run stream is empty. Single-sourced so both
/// edges in [`cmd_stats`] stay in lock-step.
const NO_RUNS_MESSAGE: &str =
    "# Rigger: no runs recorded yet (run `rigger run` to start a run, then `rigger stats`).";

/// The operator-facing parallelism-retention line for a run (spec 17 criterion 4c), or `None`
/// when the metric was NOT measured: [`parallelism_retention`](Metrics::parallelism_retention) is
/// `None` because no `BlastRadiusComputed` audit was recorded, which is the shipped non-symbols
/// default. Both operator surfaces then omit the line, so default output is byte-for-byte unchanged.
///
/// When measured it reports the share of grounded units that stay co-schedulable (the
/// wave-parallelism the fleet retained), and a fleet that has quietly serialized itself - a
/// retention below [`metrics::PARALLELISM_RETENTION_WARN`], per
/// [`parallelism_retention_warns`](Metrics::parallelism_retention_warns) - gets a loud inline
/// `WARN` naming the floor.
///
/// Single-sourced so the `rigger stats` retention row and the end-of-`rigger run` stderr notice
/// render IDENTICALLY: the warn text and its firing condition have ONE authority and cannot drift.
fn parallelism_retention_line(m: &Metrics) -> Option<String> {
    let retention = m.parallelism_retention?;
    let mut line = format!(
        "{:.1}% of grounded units stay co-schedulable (wave-parallelism retained)",
        retention * 100.0,
    );
    if m.parallelism_retention_warns() {
        line.push_str(&format!(
            " - WARN: below the {:.1}% floor, the fleet is largely serializing (most units \
             alone in their partition batch)",
            metrics::PARALLELISM_RETENTION_WARN * 100.0,
        ));
    }
    Some(line)
}

/// Render a [`Metrics`] value into the lines `rigger stats` prints, one metric group
/// per line. Split from [`cmd_stats`] (which does the I/O) so the formatting is a
/// pure function of the metrics and can be asserted in a unit test without touching
/// the filesystem.
///
/// The output reports the four required metrics:
///   - **first-pass yield** as a percentage with the clean/started fraction;
///   - **per-gate remediation counts** - one line per gate, `pass`/`fail`/`total`,
///     where `fail` is the remediation signal (sorted by gate id, stable);
///   - **escalation rate** as a percentage with the escalated/started fraction;
///   - **review approve/reject** counts.
fn format_stats(m: &Metrics) -> Vec<String> {
    let mut lines = Vec::new();
    lines.push("run stats:".to_string());
    lines.push(format!(
        "  first-pass yield   {:.1}% ({}/{} units clean on the first pass)",
        m.first_pass_yield() * 100.0,
        m.first_pass_clean,
        m.units_started,
    ));
    lines.push(format!(
        "  escalation rate    {:.1}% ({}/{} units escalated to a human)",
        m.escalation_rate() * 100.0,
        m.units_escalated,
        m.units_started,
    ));
    lines.push(format!(
        "  review             {} approved / {} rejected",
        m.review_approve, m.review_reject,
    ));
    // The runtime parallelism-retention row (spec 17 criterion 4c): shown only when structural
    // grounding measured it (`Some`); omitted on the shipped non-symbols default so that output
    // is unchanged. A below-floor share carries a loud inline WARN (single-sourced via
    // `parallelism_retention_line`, shared with the end-of-run stderr notice).
    if let Some(body) = parallelism_retention_line(m) {
        lines.push(format!("  parallelism        {body}"));
    }
    if m.gates.is_empty() {
        lines.push("  gates              (no gate runs recorded)".to_string());
    } else {
        lines.push("  per-gate runs (fail = remediation):".to_string());
        for (gate, counts) in &m.gates {
            lines.push(format!(
                "    {gate:<16} {} pass / {} fail / {} total",
                counts.pass,
                counts.fail,
                counts.total(),
            ));
        }
    }
    append_review_quality(&mut lines, m);
    lines
}

fn append_review_quality(lines: &mut Vec<String>, m: &Metrics) {
    let rq = &m.review_quality;
    lines.push("  review quality:".to_string());
    // Disclose an UNFED upheld numerator honestly (spec 11 remediation): the upheld-based
    // folds - finding survival, adversary precision, cost per upheld - only take a non-zero
    // value when a finding's attribution AND the adjudicator's recorded verdict meet on this
    // log. An all-zero-upheld panel is therefore ambiguous: it can mean the review tier
    // genuinely upheld nothing, OR that the numerator was never fed here. Distinguish and
    // disclose the UNFED case so a reader never misreads "0 upheld" as proven reviewer
    // failure. Two unfed shapes leave the folded upheld total at 0 while findings/spawns
    // exist:
    //   - NO verdict recorded on this run's driver (the in-process cli path records none), or
    //   - a verdict WAS recorded but the findings it upheld carry no attribution to fold onto
    //     (`upheld_unattributed > 0` - the empty-actor sentinel dropped them). This is the
    //     dominant case on a real aggregate store, which the adjudications==0 guard missed.
    // A verdict that recorded and genuinely upheld nothing (upheld set empty, so
    // `upheld_unattributed == 0`) is NOT unfed - its 0% is honest, so it stays silent.
    let upheld_folded: u64 = rq.finding_survival.values().map(|c| c.upheld).sum();
    let has_upheld_panel = !rq.finding_survival.is_empty() || !rq.tier_cost.is_empty();
    if has_upheld_panel
        && upheld_folded == 0
        && (rq.adjudications == 0 || rq.upheld_unattributed > 0)
    {
        let why = if rq.adjudications == 0 {
            "no adjudicator verdict recorded on this run's driver - the upheld set rides the courier SpawnResult the in-process cli path never writes".to_string()
        } else {
            format!(
                "a verdict WAS recorded, but {} upheld finding(s) carry no attribution to fold onto (unattributed on this log)",
                rq.upheld_unattributed,
            )
        };
        lines.push(format!(
            "    (unfed upheld numerator: the folds below - survival, adversary precision, cost per upheld - render 0/- and do NOT mean the review tier upheld nothing; {why})"
        ));
    }
    lines.push(format!(
        "    flip-flop rate     {:.1}% ({}/{} rejects reversed on the same sha)",
        m.flip_flop_rate() * 100.0,
        rq.flip_flops,
        m.review_reject,
    ));
    lines.push(format!(
        "    lens overlap       {:.1}% ({}/{} flagged files hit by 2+ actors)",
        rq.lens_overlap_rate() * 100.0,
        rq.overlap_files,
        rq.finding_files,
    ));
    lines.push(format!(
        "    adversary precision {:.1}% ({}/{} adversary-only findings upheld)",
        rq.adversary_precision() * 100.0,
        rq.adversary_only.upheld,
        rq.adversary_only.raised,
    ));
    if rq.finding_survival.is_empty() {
        lines.push("    finding survival   (no review findings recorded)".to_string());
    } else {
        lines.push("    finding survival per actor (upheld/raised):".to_string());
        for (actor, c) in &rq.finding_survival {
            lines.push(format!(
                "      {actor:<20} {}/{} ({:.0}%)",
                c.upheld,
                c.raised,
                c.survival() * 100.0,
            ));
        }
    }
    if rq.rejections_by_cause.is_empty() {
        lines.push("    rejections by cause (none recorded)".to_string());
    } else {
        lines.push("    rejections by cause:".to_string());
        for (cause, n) in &rq.rejections_by_cause {
            lines.push(format!("      {cause:<24} {n}"));
        }
    }
    // A rejection's cause rides a RECORDED adjudicator reject verdict; the in-process cli
    // path records none, so on that path - and on any aggregate store mixing the two - the
    // folded causes account for FEWER rejects than review_reject. Disclose the unfed
    // remainder so the cause panel is never misread as the full reject breakdown (the count
    // never underflows: each cause fold is paired with a review_reject in the same arm).
    let causes_folded: u64 = rq.rejections_by_cause.values().sum();
    if causes_folded < m.review_reject {
        lines.push(format!(
            "    (cause folded for {}/{} review rejects; the other {} carry no recorded verdict cause on this log)",
            causes_folded,
            m.review_reject,
            m.review_reject - causes_folded,
        ));
    }
    if !rq.escalations_by_cause.is_empty() {
        lines.push("    escalations by cause:".to_string());
        for (cause, n) in &rq.escalations_by_cause {
            lines.push(format!("      {cause:<24} {n}"));
        }
    }
    if rq.tier_cost.is_empty() {
        lines.push("    tier cost          (no review spawns recorded)".to_string());
    } else {
        lines.push("    cost per upheld finding per tier (spawns/upheld):".to_string());
        for (tier, tc) in &rq.tier_cost {
            let ratio = if tc.upheld == 0 {
                "-".to_string()
            } else {
                format!("{:.1}", tc.cost_per_upheld())
            };
            lines.push(format!(
                "      {tier:<12} {} spawns / {} upheld ({ratio})",
                tc.spawns, tc.upheld,
            ));
        }
    }
}

/// The message `rigger stats --canary` prints when no canary run has been recorded yet -
/// either the project has never run (no `events.db`) or its canary stream is empty.
const NO_CANARY_MESSAGE: &str =
    "# Rigger: no canary run recorded yet (run `rigger canary` to score the review panel \
     against the corpus, then `rigger stats --canary`).";

/// `rigger stats --canary` (spec 13, unit 5): report the judge-the-judges scorecard of the
/// LATEST canary run - per-tier catch rate, adjudicator correctness, and finding-order
/// stability - folded from the project's DISTINCT canary stream (never the run stream).
fn cmd_stats_canary() -> Res {
    match canary_stats_lines(&db_path("events.db"), &project_identity())? {
        Some(lines) => {
            for line in lines {
                println!("{line}");
            }
        }
        None => println!("{NO_CANARY_MESSAGE}"),
    }
    Ok(())
}

/// The pure read-model core of `rigger stats --canary`: open the embedded `events.db`,
/// read `project`'s namespaced `canary` stream, and fold it into the printable canary
/// scorecard - `None` for the two "no canary run yet" edges (absent db / empty stream),
/// so [`cmd_stats_canary`] prints one clear message for both. Split out for the same
/// reason [`stats_lines`] is: the namespace-scoped read is unit-testable off the process
/// cwd.
fn canary_stats_lines(
    path: &str,
    project: &str,
) -> Result<Option<Vec<String>>, Box<dyn std::error::Error>> {
    let sel = store_selection(None, None)?;
    if sel.is_sqlite() && !Path::new(path).exists() {
        return Ok(None);
    }
    let backend = resolve_store(&sel, path)?;
    let store = Namespaced::new(backend.as_ref(), project);
    let events = store.read_stream(canary::STREAM, 0, Direction::Forward)?;
    if events.is_empty() {
        return Ok(None);
    }
    // `project_canary` scopes internally to the latest canary run (its batch marker).
    Ok(Some(format_canary_stats(&metrics::project_canary(&events))))
}

/// Render a [`metrics::CanaryMetrics`] scorecard into the lines `rigger stats --canary`
/// prints. Pure over the metrics so it is asserted without touching the filesystem, and
/// shared with `rigger canary`'s own post-run summary so the two agree.
fn format_canary_stats(m: &metrics::CanaryMetrics) -> Vec<String> {
    let mut lines = Vec::new();
    lines.push("canary stats (judge-the-judges recall):".to_string());
    lines.push(format!(
        "  items scored       {} ({} planted, {} defect class(es) cataloged)",
        m.items,
        m.planted,
        m.defect_classes.len(),
    ));
    lines.push("  catch rate by tier (planted defects each tier caught):".to_string());
    for (tier, tc) in &m.tier_catch {
        lines.push(format!(
            "    {tier:<16} {}/{} ({:.1}%)",
            tc.caught,
            tc.planted,
            tc.rate() * 100.0,
        ));
    }
    lines.push(format!(
        "  adjudicator        {}/{} correct ({:.1}%)",
        m.adjudicator_correct,
        m.items,
        m.adjudicator_accuracy() * 100.0,
    ));
    lines.push(format!(
        "  verdict stability  {}/{} stable ({:.1}%) under finding-order shuffle",
        m.stable,
        m.items,
        m.stability_rate() * 100.0,
    ));
    if !m.defect_classes.is_empty() {
        lines.push(format!(
            "  defect classes     {}",
            m.defect_classes
                .iter()
                .cloned()
                .collect::<Vec<_>>()
                .join(", "),
        ));
    }
    lines
}

/// Read the project's cross-run resolved-model drift (spec 13b, unit 1) from the embedded
/// `events.db` at `path`, namespaced by `project`, folding the run stream via
/// [`metrics::model_drift`]. Returns an EMPTY (no-drift) [`metrics::ModelDrift`] when there
/// is no store yet - so a never-run project and a no-drift project are treated the same. It
/// reads the SAME namespaced run stream `rigger stats` folds, so the `rigger validate`
/// warning and the `rigger canary --if-model-changed` trigger fold ONE source of truth for
/// what "the model changed" means - they can never disagree. Split off (path + project
/// explicit) so the read is unit-testable off the process cwd, exactly like [`stats_lines`].
fn read_model_drift(
    path: &str,
    project: &str,
) -> Result<metrics::ModelDrift, Box<dyn std::error::Error>> {
    let sel = store_selection(None, None)?;
    if sel.is_sqlite() && !Path::new(path).exists() {
        return Ok(metrics::ModelDrift::default());
    }
    let backend = resolve_store(&sel, path)?;
    let store = Namespaced::new(backend.as_ref(), project);
    let events = store.read_stream(conductor::STREAM, 0, Direction::Forward)?;
    Ok(metrics::model_drift(&events))
}

/// The `rigger validate` model-drift advisory (spec 13b, unit 1): a stderr warning naming
/// each tier whose resolved model id re-pointed since the previous run and recommending the
/// drift-gated canary, or `None` when nothing drifted. Pure over the [`metrics::ModelDrift`]
/// so it is asserted without touching the filesystem (like [`format_stats`]); the caller
/// prints it without changing the exit status, exactly like the other validate advisories.
fn model_drift_advisory(drift: &metrics::ModelDrift) -> Option<String> {
    if !drift.changed() {
        return None;
    }
    let mut msg = String::from(
        "warning: a tier's resolved model id changed since the previous run (a silent alias \
         re-point):",
    );
    for c in &drift.changes {
        let alias = if c.alias.is_empty() {
            "(unnamed tier)"
        } else {
            c.alias.as_str()
        };
        msg.push_str(&format!("\n  - {alias}: {} -> {}", c.previous, c.current));
    }
    msg.push_str(
        "\nRun `rigger canary --if-model-changed` to re-measure the review panel against the \
         seeded-defect corpus before trusting a run under the new model.",
    );
    Some(msg)
}

/// The `rigger validate` order-signature advisories (spec 71, VALIDATE DETECTS THE
/// SIGNATURE): one warning per affected stream naming the row count, the affected position
/// range, and [`watch::ORDER_SIGNATURE_REPAIR_DOC_REF`] - or an empty vec on a clean log. Pure
/// over already-detected signatures, mirroring [`model_drift_advisory`]'s split between
/// reading and formatting. Report-only like every validate advisory: printed to stderr, never
/// changing the exit status - repair stays a documented operator procedure, never a command
/// this binary performs (spec 71 Notes: fail-safe directions only).
fn order_signature_advisories(signatures: &[watch::OrderSignature]) -> Vec<String> {
    signatures
        .iter()
        .map(|s| {
            format!(
                "warning: stream {} has {} row(s) where position order and revision order \
                 disagree (positions {}..={}): a write likely landed in a revision hole a \
                 compaction opened. This is report-only - `rigger validate` never repairs it; \
                 see {} for the repair procedure.",
                s.stream,
                s.rows,
                s.first_position,
                s.last_position,
                watch::ORDER_SIGNATURE_REPAIR_DOC_REF
            )
        })
        .collect()
}

/// Read [`watch::OrderSignature`]s (spec 71) from the embedded `events.db` at `path`,
/// namespaced by `project`, scanning the FULL log in position order (mirrors
/// [`cmd_prime`]'s full-log read - the store defends its own order for every stream, not one
/// distinguished stream, so there is no narrower slice to scan). Returns an empty vec when
/// there is no store yet, like [`read_model_drift`]. Detection itself
/// ([`watch::order_signatures`]) is the SAME shared algorithm `rigger watch`'s own store-
/// integrity signal calls - one implementation, not two kept in sync by hand.
fn read_order_signatures(
    path: &str,
    project: &str,
) -> Result<Vec<watch::OrderSignature>, Box<dyn std::error::Error>> {
    let sel = store_selection(None, None)?;
    if sel.is_sqlite() && !Path::new(path).exists() {
        return Ok(Vec::new());
    }
    let backend = resolve_store(&sel, path)?;
    let store = Namespaced::new(backend.as_ref(), project);
    let events = store.read_all(0, Direction::Forward, &Filter::default())?;
    Ok(watch::order_signatures(&events))
}

/// `rigger canary [--corpus <dir>] [--if-model-changed]` (spec 13, unit 5; drift trigger spec
/// 13b, unit 1): run the review panel against every item in the seeded-defect corpus (default
/// `./canaries`) and record the scored outcomes to the project's canary stream, then print the
/// scorecard. This is the loop's only RECALL measurement - it judges the judges against known
/// ground truth. The scores land in a DISTINCT stream from the run's, so a canary run never
/// perturbs the project's operator metrics; `rigger stats --canary` re-reports them.
///
/// With `--if-model-changed` the run is GATED on model drift: the canary runs ONLY when a
/// tier's resolved model id re-pointed since the previous run (the same drift `rigger
/// validate` warns about), and an unchanged model runs no canary - the automatic monitor for
/// silent alias re-points. Without the flag the canary always runs.
fn cmd_canary(args: &[String]) -> Res {
    let mut corpus_dir = "canaries".to_string();
    let mut if_model_changed = false;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--corpus" => {
                corpus_dir = args
                    .get(i + 1)
                    .ok_or("canary: --corpus needs a directory path")?
                    .clone();
                i += 2;
            }
            "--if-model-changed" => {
                if_model_changed = true;
                i += 1;
            }
            other => {
                return Err(format!(
                    "canary: unexpected argument {other:?} (usage: rigger canary [--corpus <dir>] \
                     [--if-model-changed])"
                )
                .into())
            }
        }
    }

    // The drift gate (spec 13b, unit 1): with `--if-model-changed`, run the canary ONLY when a
    // tier's resolved model re-pointed since the previous run; an unchanged model runs no
    // canary (and needs no corpus, so the gate precedes the corpus load). The detection reads
    // the SAME namespaced run stream `rigger validate`'s drift advisory folds, so the warning
    // and this trigger can never disagree on what "the model changed" means.
    if if_model_changed {
        let drift = read_model_drift(&db_path("events.db"), &project_identity())?;
        if !drift.changed() {
            println!(
                "canary: no resolved-model change since the previous run - skipping (run \
                 `rigger canary` to force a run)."
            );
            return Ok(());
        }
        for c in &drift.changes {
            let alias = if c.alias.is_empty() {
                "(unnamed tier)"
            } else {
                c.alias.as_str()
            };
            println!(
                "canary: resolved model changed for {alias} ({} -> {}) since the previous run - \
                 running the panel.",
                c.previous, c.current,
            );
        }
    }

    let corpus = canary::load_corpus(Path::new(&corpus_dir))?;
    if corpus.is_empty() {
        return Err(format!(
            "canary: the corpus at {corpus_dir:?} has no items (add `*.md` canary files)"
        )
        .into());
    }

    let cfg = config::load(".")?;
    let panel = cfg.workflow.defaults.review.clone();
    if panel.is_empty() {
        return Err("canary: defaults.review declares no review panel to measure".into());
    }

    std::fs::create_dir_all(RIGGER_DIR)?;
    // Sqlite is the canary's local measurement store; migrate a pre-spec-09 namespace once
    // so the canary stream lands under the same identity `stats --canary` reads.
    let selection = store_selection(None, None)?;
    if selection.is_sqlite() {
        migrate_local_identity()?;
    }
    let backend = resolve_store(&selection, &db_path("events.db"))?;
    let store = Namespaced::new(backend.as_ref(), &project_identity());
    let driver = cli::Driver::default();

    let report = canary::run_canary(&store, &driver, &cfg, &panel, &corpus)?;
    println!(
        "canary run {}: scored {} corpus item(s) against the review panel",
        report.batch,
        report.outcomes.len(),
    );
    // Re-read and fold from the store so the printed scorecard is exactly what
    // `rigger stats --canary` will report from the same events.
    let events = store.read_stream(canary::STREAM, 0, Direction::Forward)?;
    for line in format_canary_stats(&metrics::project_canary(&events)) {
        println!("{line}");
    }
    Ok(())
}

/// `rigger playbooks --rebuild` (spec 13b, unit 2) - reconstruct the distilled playbook pool
/// under `.rigger/playbooks/` from this project's recorded `LessonLearned` stream. The pool is
/// a rebuildable PROJECTION of the log (never hand-edited state): [`playbooks::rebuild`] clears
/// the rigger-managed pool files and re-derives every deduplicated, trigger-scoped playbook, so
/// this command is the operator's way to regenerate the pool after new lessons land (or to
/// recover a hand-corrupted pool). It only READS the run stream (never writes it), scoped to
/// this project's namespace exactly as `rigger stats`/`rigger canary` read it; an absent store
/// (a never-run project) has no lessons, so the pool rebuilds empty rather than fabricating one.
fn cmd_playbooks(args: &[String]) -> Res {
    match args {
        [flag] if flag == "--rebuild" => {}
        _ => {
            return Err("playbooks: expected --rebuild (usage: rigger playbooks --rebuild)".into())
        }
    }

    // Migrate a pre-spec-09 namespace once so the lessons stream lands under the same
    // identity the conductor wrote, then READ (never fabricate) this project's run stream.
    let selection = store_selection(None, None)?;
    if selection.is_sqlite() {
        migrate_local_identity()?;
    }
    let db = db_path("events.db");
    let events = if !selection.is_sqlite() || Path::new(&db).exists() {
        let backend = resolve_store(&selection, &db)?;
        let store = Namespaced::new(backend.as_ref(), &project_identity());
        store.read_stream(conductor::STREAM, 0, Direction::Forward)?
    } else {
        Vec::new()
    };

    let pool_dir = Path::new(RIGGER_DIR).join(playbooks::POOL_SUBDIR);
    let pool = playbooks::rebuild(&events, &pool_dir)?;
    let lessons = events
        .iter()
        .filter(|e| e.type_ == contextgraph::TYPE_LESSON_LEARNED)
        .count();
    println!(
        "playbooks: rebuilt {} playbook(s) under {} from {} recorded lesson event(s)",
        pool.len(),
        pool_dir.display(),
        lessons,
    );
    Ok(())
}

/// `rigger replay <run-id|latest> --against <rev>` - trajectory replay / config eval
/// (spec 13, unit 2). Re-drive a COMPLETED run's recorded trajectory under a CANDIDATE
/// config (the `workflow.yml` + agent prompts committed at git `<rev>`) in a fully
/// ISOLATED scratch namespace, then print the stats DIFF against the run's recorded
/// baseline metrics. Past runs become a regression corpus for config edits - "did that
/// prompt/tier/budget change regress first-pass yield?" gets an answer with no live
/// campaign, because unit 1's pinned definition makes the baseline citable.
///
/// The re-drive answers every agent spawn from the baseline's recorded `SpawnResult`s (the
/// [`ReplayDriver`]) and every gate the candidate still declares from its recorded
/// `GateVerdict`s (the conductor's gate-verdict replay), so it runs NO agent and NO gate
/// command - it re-derives only the run's SHAPE (which stages, which review tier, which
/// budget, WHICH gates the CANDIDATE config dictates) over the same recorded behaviour. A
/// spawn the candidate config introduces that the trajectory never recorded simply parks, so
/// the re-drive stops where the recorded behaviour runs out rather than fabricating one.
///
/// The "gate runs" column is re-scoped to the candidate accordingly: the trajectory seeds
/// every recorded gate verdict, but only the gates the candidate config still declares are
/// re-reached, so a config edit that REMOVES or renames a gate lowers the candidate "gate
/// runs" (a removed gate's seeded verdict is dropped by [`candidate_reaches_gate`] before the
/// candidate fold), while an added gate the baseline never ran runs FAIL-SAFE (never a
/// fabricated pass, see [`ReplayRunner`]). The one gate boundary the offline replay does not
/// reproduce is the git-merge-specific POST-MERGE re-gate (d13-u2), whose recorded verdict is
/// left as-is.
///
/// ISOLATION (never the real project streams): the re-drive writes to a FRESH sqlite file
/// under the scratch root, opened as a distinct [`Namespaced`] project - the real
/// `.rigger/events.db` is only ever READ (to lift the baseline) and never opened for write.
/// The candidate config is read from a throwaway detached `git worktree` of `<rev>` that is
/// removed after loading. Both scratch artifacts live under the project scratch root, never
/// the OS temp partition.
fn cmd_replay(args: &[String]) -> Res {
    let (run_id, rev) = parse_replay_args(args)?;

    // The candidate config lives at a git rev, so a replay needs a repo. The baseline is
    // read from THIS project's namespaced run stream (read-only).
    let repo = git_repo();
    if repo.is_empty() {
        return Err(
            "rigger replay: needs a git repo - the candidate config is read at the \
                    git rev given to --against, and this project is not inside one"
                .into(),
        );
    }

    // 1. Lift the baseline: read (never write) this project's run stream and slice the
    //    requested run. `metrics::project` folds it into the recorded baseline.
    let db = db_path("events.db");
    let selection = store_selection(None, None)?;
    if selection.is_sqlite() && !Path::new(&db).exists() {
        return Err(format!(
            "rigger replay: no runs recorded yet for this project (no {db}); run `rigger run` first"
        )
        .into());
    }
    let backend = resolve_store(&selection, &db)?;
    let real = Namespaced::new(backend.as_ref(), &project_identity());
    let events = real.read_stream(conductor::STREAM, 0, Direction::Forward)?;
    let baseline = baseline_run_slice(&events, &run_id).ok_or_else(|| {
        format!(
            "rigger replay: no run {run_id:?} in this project's stream (use a run id from \
             `rigger stats`, or `latest`)"
        )
    })?;
    let baseline_metrics = metrics::project(baseline);
    // The baseline run's acceptance criteria: the SPEC the candidate config is re-driven
    // against, so the isolated run adopts the same campaign fingerprint. The resolved run id
    // (never the literal `latest`) names the baseline in the diff header.
    let baseline_started = serde_json::from_slice::<runscope::RunStarted>(&baseline[0].data).ok();
    let criteria: Vec<String> = baseline_started
        .as_ref()
        .map(|r| r.criteria.clone())
        .unwrap_or_default();
    let baseline_id = baseline_started
        .map(|r| r.run)
        .filter(|r| !r.is_empty())
        .unwrap_or_else(|| run_id.clone());

    // 2. Materialize the candidate config at <rev> in a throwaway checkout.
    let workdir = config::load(".")
        .map(|c| c.workflow.defaults.workdir)
        .unwrap_or_default();
    let scratch_root = rigger::worktree::scratch_root_from_env(&repo, &workdir);
    std::fs::create_dir_all(&scratch_root)?;
    let (candidate_cfg, candidate_definition) =
        materialize_config_at_rev(&repo, &rev, &scratch_root)?;

    // 3. Seed the ISOLATED store (a separate scratch db + namespace) with a fresh RunStarted
    //    for the candidate criteria/definition, then the baseline's replayable trajectory.
    //    The db lives in a THROWAWAY subdir removed wholesale below, so the WAL/SHM sidecars
    //    a live WAL-mode sqlite opens beside the .db never leak under the scratch root.
    let replay_dir =
        Path::new(&scratch_root).join(format!("rigger-replay-{}", uuid::Uuid::new_v4().simple()));
    std::fs::create_dir_all(&replay_dir)?;
    let replay_db = replay_dir.join("events.db");

    // The store (and everything borrowing it - the namespaced view, the driver, the deps)
    // is confined to this scope so it is DROPPED before the scratch subdir is removed: a
    // WAL-mode sqlite only releases its `.db-wal`/`.db-shm` sidecars on close, so cleaning
    // up while the connection is still open would leak them (adv-u13r-replay-scratch-wal-shm-leak).
    let (candidate_metrics, drive_err) = {
        let iso_backend = resolve_store(
            &StoreSelection::Sqlite,
            replay_db.to_str().unwrap_or_default(),
        )?;
        let iso = Namespaced::new(iso_backend.as_ref(), "rigger-replay");
        // An offline replay re-fold over an isolated store: no run branch, no PR, so no base
        // to persist (spec 38, criterion 3).
        runscope::start_fresh(&iso, &criteria, &candidate_definition, "")?;
        let trajectory = conductor::replay_trajectory(baseline);
        iso.append(conductor::STREAM, ExpectedRevision::Any, &trajectory)?;

        // 4. Re-drive the candidate config over the isolated store. Repo-less and grounder-less
        //    (a pure offline re-fold), the ReplayDriver answers each spawn from the seeded
        //    results, and ReplayRunner guarantees a candidate-config-only gate never shells out.
        let driver = ReplayDriver::new(&iso);
        let deps = Deps {
            store: &iso,
            driver: &driver,
            gates: &ReplayRunner,
            repo: String::new(),
            grounder: None,
            graph: None,
            criteria,
        };
        let drive = conductor::run(&candidate_cfg, &deps);

        // 5. Fold the candidate metrics from the isolated run. The re-drive's own result is
        //    reported but never fatal: a candidate config that parks (an uncovered spawn) still
        //    yields a partial, honestly-labelled candidate column.
        //
        //    "gate runs" must reflect the CANDIDATE config, not echo the seeded baseline: the
        //    trajectory seeds every recorded GateVerdict, but the re-drive only RE-REACHES the
        //    gates the candidate config still declares (`run_gates` iterates the candidate's
        //    `st.gates`), so a removed/renamed gate is never touched. Filter the isolated
        //    current-run through `candidate_reaches_gate` before folding, so a seeded verdict
        //    the candidate no longer reaches is dropped from the candidate "gate runs" count
        //    (adv-u13r-gate-runs-echoes-seed-not-candidate). Every non-gate event folds
        //    unchanged, so only the gate column is re-scoped.
        let iso_events = iso.read_stream(conductor::STREAM, 0, Direction::Forward)?;
        let current = runscope::current_run(&iso_events);
        let started = started_units(current);
        let candidate_view: Vec<Event> = current
            .iter()
            .filter(|e| candidate_reaches_gate(e, &candidate_cfg, &started))
            .cloned()
            .collect();
        (metrics::project(&candidate_view), drive.err())
    };

    // 6. The isolated store is now dropped (closed): remove the whole throwaway db subdir -
    //    the `.db` plus its `.db-wal` / `.db-shm` sidecars - in one call, so no sqlite file
    //    leaks under the scratch root. Best-effort (the diff is already computed), so a
    //    cleanup failure never fails the command.
    let _ = std::fs::remove_dir_all(&replay_dir);

    for line in format_stats_diff(&baseline_id, &rev, &baseline_metrics, &candidate_metrics) {
        println!("{line}");
    }
    if let Some(e) = drive_err {
        eprintln!(
            "rigger replay: the candidate re-drive did not complete ({e}); the candidate \
             column reflects the run up to where the recorded trajectory ran out"
        );
    }
    Ok(())
}

/// The set of unit ids the re-drive actually STARTED (emitted a `UnitStarted` for) in the
/// isolated `events` slice. The seeded trajectory carries only SpawnResults + GateVerdicts
/// ([`conductor::replay_trajectory`] strips the lifecycle), so every `UnitStarted` here is
/// one the re-drive emitted for a unit the CANDIDATE config reached - the signal that lets
/// [`candidate_reaches_gate`] drop the seeded gate verdicts of a stage the candidate removed
/// (or a unit its DAG never reached), which the re-drive never re-started.
fn started_units(events: &[Event]) -> std::collections::HashSet<String> {
    events
        .iter()
        .filter(|e| e.type_ == ledger::TYPE_UNIT_STARTED)
        .filter_map(|e| {
            serde_json::from_slice::<serde_json::Value>(&e.data)
                .ok()
                .and_then(|v| v.get("id").and_then(|i| i.as_str()).map(String::from))
        })
        .collect()
}

/// Whether the candidate config still REACHES the gate a recorded `GateVerdict` scored, so it
/// counts toward the candidate "gate runs" column of a `rigger replay` diff. Every non-gate
/// event passes through unchanged (only the gate column is re-scoped to the candidate); a
/// gate verdict is KEPT only when the offline re-drive would genuinely re-reach it:
///
/// - its stage/unit was STARTED in the re-drive (`started`) - a stage the candidate removed,
///   or a unit its DAG never reached, is never re-driven, so its seeded verdicts do not count;
/// - AND the candidate config's stage still DECLARES this gate - `run_gates` iterates the
///   candidate's `st.gates`, so a static stage that dropped or renamed the gate never runs it,
///   and its seeded verdict is not reached. A kept gate replays (counted), an added gate runs
///   fail-safe (a fresh verdict, also for a declared gate, so counted), a removed/renamed gate
///   drops out - exactly the set the re-drive reaches.
///
/// A verdict whose replay key carries no `/gate:` infix (an integrate-time GATED_BY artifact
/// verdict, already excluded by [`metrics::project`]; or a post-merge re-gate keyed apart -
/// the git-merge-specific boundary the offline replay never reproduces, per d13-u2) is left as
/// recorded. A gate verdict on a started unit that is NOT a static workflow stage (a
/// planner-proposed unit whose gate list cannot be re-scoped from the config) is likewise kept
/// as recorded - the re-scoping never over-drops a verdict it cannot confidently place.
fn candidate_reaches_gate(
    e: &Event,
    cfg: &config::Config,
    started: &std::collections::HashSet<String>,
) -> bool {
    if e.type_ != contextgraph::TYPE_GATE_VERDICT {
        return true;
    }
    // A verdict with no gate-RUN replay key (artifact / post-merge / skip) is not a re-scopable
    // pre-merge gate run; leave it as recorded.
    let Some(stage) = e
        .meta
        .get(conductor::META_REPLAY_KEY)
        .and_then(|k| conductor::unit_of_gate_key(k))
    else {
        return true;
    };
    // The re-drive must have re-started this stage's unit; a removed stage is never re-driven.
    if !started.contains(stage) {
        return false;
    }
    let Some(gate) = serde_json::from_slice::<serde_json::Value>(&e.data)
        .ok()
        .and_then(|v| v.get("gate").and_then(|g| g.as_str()).map(String::from))
    else {
        return true;
    };
    // A static candidate stage that no longer lists this gate never runs it (removed/renamed);
    // a non-static unit (no such stage) is kept as recorded rather than over-dropped.
    match cfg.workflow.stages.get(stage) {
        Some(st) => st.gates.iter().any(|g| g == &gate),
        None => true,
    }
}

/// Parse `rigger replay <run-id|latest> --against <rev>`. Exactly the run selector and the
/// `--against <rev>` pair are accepted, in either order for the flag; anything else is a
/// loud usage error rather than a silently-ignored argument.
fn parse_replay_args(args: &[String]) -> Result<(String, String), Box<dyn std::error::Error>> {
    let mut run_id: Option<String> = None;
    let mut rev: Option<String> = None;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--against" => {
                rev = Some(
                    args.get(i + 1)
                        .ok_or("rigger replay: --against needs a git rev")?
                        .clone(),
                );
                i += 2;
            }
            flag if flag.starts_with("--") => {
                return Err(format!("rigger replay: unknown flag {flag:?}").into());
            }
            positional if run_id.is_none() => {
                run_id = Some(positional.to_string());
                i += 1;
            }
            extra => {
                return Err(format!("rigger replay: unexpected argument {extra:?}").into());
            }
        }
    }
    let run_id = run_id.ok_or(
        "rigger replay: expected a run id (or `latest`) and `--against <rev>`; \
         see `rigger --help`",
    )?;
    let rev = rev.ok_or("rigger replay: missing --against <rev> (the candidate config rev)")?;
    Ok((run_id, rev))
}

/// The slice of `events` belonging to `run_id`: the contiguous window from that run's
/// `RunStarted` up to (but excluding) the next one, so a MIDDLE run in a multi-run store is
/// sliced exactly like the current one - not just the latest. `latest` selects the current
/// run ([`runscope::current_run`]). `None` when no such run exists (an unknown id, or an
/// empty stream).
fn baseline_run_slice<'a>(events: &'a [Event], run_id: &str) -> Option<&'a [Event]> {
    if run_id == "latest" {
        let slice = runscope::current_run(events);
        return (!slice.is_empty()).then_some(slice);
    }
    let start = events.iter().position(|e| {
        e.type_ == runscope::TYPE_RUN_STARTED && run_started_id(e).as_deref() == Some(run_id)
    })?;
    let end = events[start + 1..]
        .iter()
        .position(|e| e.type_ == runscope::TYPE_RUN_STARTED)
        .map(|off| start + 1 + off)
        .unwrap_or(events.len());
    Some(&events[start..end])
}

/// The run id carried in a `RunStarted` event body, or `None` if it is malformed.
fn run_started_id(e: &Event) -> Option<String> {
    serde_json::from_slice::<runscope::RunStarted>(&e.data)
        .ok()
        .map(|r| r.run)
}

/// Load the candidate [`Config`](config) and its definition hash from git `<rev>` via a
/// throwaway DETACHED worktree under `scratch_root`, removed once loaded. Reading the config
/// through a real checkout (rather than piping `git show`) reuses the exact [`config::load`]
/// / [`definition_hash`] readers the live path uses, so a replay evaluates precisely the
/// config a run at `<rev>` would.
fn materialize_config_at_rev(
    repo: &str,
    rev: &str,
    scratch_root: &str,
) -> Result<(config::Config, String), Box<dyn std::error::Error>> {
    let checkout = Path::new(scratch_root).join(format!(
        "rigger-replay-cfg-{}",
        uuid::Uuid::new_v4().simple()
    ));
    let checkout_str = checkout
        .to_str()
        .ok_or("rigger replay: non-utf8 scratch path")?;
    let add = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(["worktree", "add", "--detach"])
        .arg(checkout_str)
        .arg(rev)
        .output()?;
    if !add.status.success() {
        return Err(format!(
            "rigger replay: could not check out --against {rev:?}: {}",
            String::from_utf8_lossy(&add.stderr).trim()
        )
        .into());
    }
    // Load BEFORE removing the checkout; both readers return owned values, so the worktree
    // can be torn down immediately after.
    let loaded = config::load(checkout_str)
        .map_err(|e| format!("rigger replay: candidate config at {rev:?} is invalid: {e}"))
        .and_then(|cfg| {
            definition_hash(checkout_str)
                .map(|def| (cfg, def))
                .map_err(|e| format!("rigger replay: candidate definition hash at {rev:?}: {e}"))
        });
    let _ = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(["worktree", "remove", "--force"])
        .arg(checkout_str)
        .output();
    Ok(loaded?)
}

/// Render the baseline-vs-candidate stats diff `rigger replay` prints: a header naming the
/// baseline run and the candidate rev, a column head, then one aligned row per headline
/// metric from [`metrics::diff_rows`], each changed row flagged with `*` so a config edit's
/// effect jumps out. Pure over the two [`Metrics`], so it is asserted without any I/O.
fn format_stats_diff(run_id: &str, rev: &str, base: &Metrics, cand: &Metrics) -> Vec<String> {
    let mut lines = vec![
        format!("replay stats diff (baseline run {run_id} vs candidate config @ {rev}):"),
        format!("  {:<20} {:>10} {:>10}", "metric", "baseline", "candidate"),
    ];
    for (label, b, c) in metrics::diff_rows(base, cand) {
        let flag = if b != c { "  *" } else { "" };
        lines.push(format!("  {label:<20} {b:>10} {c:>10}{flag}"));
    }
    lines
}

/// A [`Runner`] for `rigger replay` that NEVER executes a gate command. The re-drive
/// replays every gate outcome the recorded trajectory carries (the conductor's gate-verdict
/// replay answers them before any runner is consulted), so this is reached ONLY for a gate
/// the CANDIDATE config introduced that the baseline never ran - which cannot be scored from
/// recorded behaviour. It therefore FAILS SAFE (never a fabricated pass) and runs no shell,
/// keeping the replay a pure offline re-fold of recorded facts.
struct ReplayRunner;

impl Runner for ReplayRunner {
    fn run(
        &self,
        g: &Gate,
        _dir: &str,
        _target_dir: &str,
        _store_fence: &str,
        _build_env: &BuildEnv,
        _budget: &BuildBudget,
    ) -> GateResult {
        GateResult {
            pass: false,
            evidence: format!(
                "FAIL\ngate {}: not covered by the replayed trajectory (a candidate-config gate \
                 with no recorded verdict); rigger replay never executes a gate command",
                g.id
            ),
        }
    }
}

/// `rigger dash` - serve or export the embedded observability page (spec 11, unit 2).
///
/// A READ-ONLY window over the existing projections: the conductor stays the sole mutation
/// authority, so the dash has no write or control surface (enforced in [`dash::route`],
/// which answers only `GET`). `rigger dash` serves the live-polling single-file page on
/// loopback (`127.0.0.1`, default [`dash::DEFAULT_PORT`], override with `--port`);
/// `rigger dash --export <path>` writes the equivalent static, shareable snapshot.
///
/// Composition mirrors the sibling operator reads (`stats`, `graph`): it resolves this
/// project's `.rigger/events.db` + `.rigger/graph.db` by cwd (via [`db_path`] /
/// [`project_identity`]) and re-reads them on EACH request, so the page reflects the run
/// as it advances. An ABSENT `events.db` reads as an empty run (guarded BEFORE
/// [`Store::open`], which would otherwise create it), so an operator can launch the dash
/// first and watch the run populate it. The context graph is best-effort: a grep-only run
/// never builds one, and an absent or unreadable `graph.db` yields an empty graph rather
/// than failing the whole page.
/// Auto-start a read-only `rigger dash` for the run a driver is about to drive, so an active
/// harness is never invisible (spec 19b, unit 1: always-on, no opt-in flag). The dash binds
/// [`dash::DEFAULT_PORT`] or the next free loopback port (so two concurrent harnesses each
/// get their OWN); its URL is printed at run start and recorded in `.rigger/`[`DASH_URL_FILE`]
/// so `rigger status` can surface it.
///
/// Returns the [`dash::ReapedChild`] guard the DRIVER holds for the whole run: dropping it
/// (on a normal finish OR an unwinding panic) reaps the dash. That guard is unit 3's reaping
/// mechanism, reused here as the single reaper - THIS unit owns only start + discoverability,
/// never stopping. Best-effort: if the dash cannot be started the run still proceeds (the
/// dash is observability, not the deliverable), so a port-starved or spawn-refused
/// environment degrades to a headless run rather than aborting one.
///
/// `store` is read once here to stamp [`DASH_ATTEMPT_FILE`] with the current run's id via
/// [`record_dash_attempt`] (spec 69, round-8 fix), mirroring [`ensure_run_dashboard`]'s own
/// stamp on the step path - by the time this runs, `fresh_run_if_requested` has already
/// ensured/minted the run, so the id is always available for a real run.
fn start_run_dashboard(store: &dyn EventStore) -> Option<dash::ReapedChild> {
    if let Ok(events) = store.read_stream(conductor::STREAM, 0, Direction::Forward) {
        record_dash_attempt(&runscope::current_run_id(&events).unwrap_or_default());
    }
    match spawn_run_dashboard() {
        Ok((guard, url)) => {
            // Stderr, not stdout: in the workflow driver (`rigger serve`) stdout is the MCP
            // transport, which the run-start pointer must never corrupt.
            eprintln!("rigger dash: serving this run at {url}");
            Some(guard)
        }
        Err(e) => {
            eprintln!(
                "rigger: could not auto-start the dashboard ({e}); the run continues headless"
            );
            None
        }
    }
}

/// Pick a free port, spawn `rigger dash --port <n>` as a child of the current executable,
/// and record its URL in `.rigger/`[`DASH_URL_FILE`] for `rigger status`. The child's stdout
/// is silenced (in the MCP `rigger serve` driver the parent's stdout is the protocol
/// transport, which the dash child must never write to) and its stdin is closed; the dash
/// logs only to its own stderr. Returns the guard plus the URL for the run-start pointer.
fn spawn_run_dashboard() -> Result<(dash::ReapedChild, String), Box<dyn std::error::Error>> {
    let port = dash::free_port_from(dash::DEFAULT_PORT)?;
    let exe = std::env::current_exe()?;
    let child = Command::new(exe)
        .arg("dash")
        .arg("--port")
        .arg(port.to_string())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .spawn()?;
    let url = format!("http://127.0.0.1:{port}/");
    // Discoverability breadcrumb for `rigger status`; best-effort and overwritten each run.
    // `.rigger/` already exists (the driver created it before reaching here).
    let _ = std::fs::write(db_path(DASH_URL_FILE), &url);
    Ok((dash::ReapedChild::new(child), url))
}

/// The outcome of the step path's idempotent dash-start attempt (spec 39, criterion 1).
#[derive(Debug, PartialEq, Eq)]
enum DashStart {
    /// No dash was serving this project, so this step STARTED one on the given port.
    Started(u16),
    /// A dash was already serving on the given port, so this step started NONE (the
    /// idempotent no-op that makes the second and every later `step` of a run a no-op).
    AlreadyServing(u16),
    /// The best-effort start failed; the run proceeds headless (a start failure never
    /// fails the step - the dash is observability, not the deliverable).
    Failed,
}

/// The recorded-serving predicate BOTH the step path's idempotent-start decision
/// ([`ensure_run_dashboard_at`], via [`dash::dash_start_needed`]) and `rigger status`'s
/// truthful presentation ([`cmd_status`], via [`dash::dash_status`]) verify a marker's port
/// against - ONE named symbol, not two independently duplicated literal closures, so the two
/// surfaces provably share the same probe rather than merely claiming to (arch-u69c4-parity-
/// claim-rests-on-stale-unlinked-docs-not-a-shared-symbol). A REAL network probe of the port,
/// never a bare pid-liveness check: a marker left by a self-reaped or pid-recycled dash must
/// never masquerade as still serving just because its pid happens to be alive (possibly reused
/// by an unrelated process).
fn dash_marker_serving(m: dash::DashMarker) -> bool {
    dash::dash_serving_on(m.port)
}

/// Idempotently ensure a run dashboard serves the project whose marker lives at
/// `marker_path` (spec 39, criterion 1: idempotent start on step). Reads the per-project
/// [`dash::DashMarker`]; if it names a still-serving dash (per `still_serving`), returns
/// [`DashStart::AlreadyServing`] WITHOUT spawning a second one - the marker/pid short-circuit
/// that makes the second and every later `step` of a run a no-op, never a port fight.
/// Otherwise calls `start` to spawn one, records its marker, and returns [`DashStart::Started`].
///
/// `still_serving` and `start` are INJECTED so the start-once behavior is provable without a
/// real dashboard process; the production caller ([`ensure_run_dashboard`]) passes
/// [`dash_marker_serving`] (a real port probe, not a bare pid check - see its own doc) and
/// [`spawn_run_dashboard_detached`].
fn ensure_run_dashboard_at(
    marker_path: &Path,
    still_serving: impl Fn(dash::DashMarker) -> bool,
    start: impl FnOnce() -> std::io::Result<dash::DashMarker>,
) -> DashStart {
    let existing = dash::DashMarker::read(marker_path);
    if !dash::dash_start_needed(existing, &still_serving) {
        // `dash_start_needed` only returns false when `existing` is a still-serving marker,
        // so this port is that live dash's port; the `unwrap_or` is unreachable-but-safe.
        return DashStart::AlreadyServing(existing.map(|m| m.port).unwrap_or_default());
    }
    match start() {
        Ok(marker) => {
            // Record the marker so the NEXT step of this run discovers this dash and does not
            // start a second. Best-effort: a failed write only risks a later duplicate start,
            // never a broken step.
            let _ = marker.write(marker_path);
            DashStart::Started(marker.port)
        }
        Err(_) => DashStart::Failed,
    }
}

/// Place `cmd`'s spawned child in its OWN process group (a new group whose PGID equals the
/// child's PID, via `process_group(0)`), detached from the parent command's process group.
/// This is the session-detachment that makes the always-on dash actually survive across steps
/// (spec 44): WITHOUT it a detached dash inherits `rigger step`'s process group, and when the
/// workflow courier runs `rigger step` as a foreground command the harness tears down that
/// command's process group on completion and reaps the dash with it - the spec-39 always-on
/// dash then dies the instant every step returns. `process_group(0)` makes the child a group
/// leader in a group the parent's teardown never reaches. Std-only (no `libc`), so it compiles
/// and holds identically on BOTH the default and `--no-default-features` lanes. Non-Unix builds
/// keep the group-inheriting behavior (the always-on dash is a Unix-path feature).
#[cfg(unix)]
fn detach_process_group(cmd: &mut Command) {
    use std::os::unix::process::CommandExt;
    // `process_group(0)` runs a `setpgid(0, 0)`-equivalent in the child before exec, making it a
    // group leader whose PGID equals its own PID - a brand-new process group the parent command's
    // group teardown never reaches.
    cmd.process_group(0);
}

#[cfg(not(unix))]
fn detach_process_group(_cmd: &mut Command) {}

/// Spawn `rigger dash --port <n>` as a DETACHED child - NO [`dash::ReapedChild`] guard is
/// held - returning its port + pid as a [`dash::DashMarker`] the caller records. Detached is
/// deliberate: the step path that starts it returns per frontier, so a guard-bound child would
/// be reaped on that return and the very next step would find no live dash and start another.
/// Not reaping here is what lets the dash keep serving across the run's many `step`
/// invocations (spec 39). Also records the dash-url breadcrumb `rigger status` reads.
///
/// ALL THREE of the child's standard streams are closed (`stdin`/`stdout`/`stderr` ->
/// [`Stdio::null`]). This matters precisely BECAUSE it is detached and outlives the `step`:
/// an inherited stderr (or stdout) would keep the step process's pipe write-end open after
/// the step exits, so any parent capturing the step's output (the thin driver, or a plain
/// `rigger step 2>&1 | ...`) would BLOCK on EOF until the long-lived dash died. A guard-bound
/// dash can inherit stderr safely because it is reaped when the driver exits; a detached one
/// must hold no inherited descriptor. The dash's own startup errors are therefore silent -
/// acceptable for a best-effort, self-contained observability process whose logs nothing reads.
fn spawn_run_dashboard_detached() -> std::io::Result<dash::DashMarker> {
    // The machine singleton binds the FIXED default address (spec 50, criterion 4) - no free-port
    // search, so the address never drifts. If a dash is already serving it, the spawned `rigger
    // dash` recognizes the singleton and exits 0 without binding a second (criterion 1's
    // `bind_singleton`), so this is safe even when a concurrent run races to start one.
    //
    // The default is overridable via [`DASH_PORT_ENV`] for exactly the case the fixed address
    // otherwise makes untestable and unusable: a machine where a rigger dash already holds the
    // default (the self-hosting dev box always does), or where a non-rigger process owns 7420.
    // Unset (the production default) resolves to [`dash::DEFAULT_PORT`] with NO free-port search,
    // so the singleton's stable-address contract is unchanged; the ensure path just gains the same
    // port seam the manual `rigger dash --port` already has, which lets the step-path dash tests
    // inject an ephemeral port and never fight a real machine dash.
    let port = dash_ensure_port();
    let exe = std::env::current_exe()?;
    let mut cmd = Command::new(exe);
    cmd.arg("dash")
        .arg("--port")
        .arg(port.to_string())
        // Self-reap on run-idle (spec 39, criterion 3): a DETACHED dash holds no `ReapedChild`,
        // so it must watch the run's own liveness and exit once the run completes or its heartbeat
        // goes stale - the backstop the process-bound guard provides on the `rigger run` paths.
        .arg("--reap-on-idle")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    // Session-detach BEFORE spawning: put the dash in its own process group so the teardown of
    // the foreground `rigger step` command's process group does not reap it (spec 44). Without
    // this the "detached" child still shares step's group and dies the instant the step returns.
    detach_process_group(&mut cmd);
    let child = cmd.spawn()?;
    let pid = child.id();
    // Detach: `child` is dropped at the end of this function. A std `Child`'s Drop neither
    // waits nor kills, so the dash process keeps running after this step returns. (Contrast
    // `dash::ReapedChild`, whose Drop reaps - deliberately NOT used on the step path.)
    let url = format!("http://127.0.0.1:{port}/");
    let _ = std::fs::write(db_path(DASH_URL_FILE), &url);
    Ok(dash::DashMarker { port, pid })
}

/// Whether the always-on dash auto-ensure is SUPPRESSED for this run (spec 50, criterion 4) -
/// the single opt-out authority. It is suppressed by EITHER opt-out: the environment disable
/// ([`DASH_DISABLE_ENV`] set to any value, `env_disabled`) OR the config opt-out (`dash: off`,
/// surfaced here as `config_dash_enabled == false`). Pure over its two resolved inputs so both
/// opt-out paths - and the fact that either alone suffices - are provable without mutating the
/// process environment or cwd; the real caller resolves `env_disabled` from the environment and
/// `config_dash_enabled` from the already-loaded workflow.
fn dash_ensure_suppressed(env_disabled: bool, config_dash_enabled: bool) -> bool {
    env_disabled || !config_dash_enabled
}

/// The port the step-path always-on dashboard binds: [`DASH_PORT_ENV`] when set to a valid
/// `u16`, else [`dash::DEFAULT_PORT`]. The real caller resolves the raw value from the process
/// environment; the resolution itself is [`dash_ensure_port_from`], pure over that resolved input
/// so both the override and the fixed-default fallback are provable without mutating the process
/// environment (the same "pure over resolved inputs" discipline as [`dash_ensure_suppressed`]).
fn dash_ensure_port() -> u16 {
    dash_ensure_port_from(std::env::var(DASH_PORT_ENV).ok().as_deref())
}

/// Resolve the step-path ensure port from an already-read [`DASH_PORT_ENV`] value: a valid `u16`
/// overrides, anything else (absent, empty, or malformed) falls back to [`dash::DEFAULT_PORT`] -
/// so production (env unset) gets the fixed default with no free-port search, and a bad knob never
/// breaks a run's observability.
fn dash_ensure_port_from(raw: Option<&str>) -> u16 {
    raw.and_then(|v| v.trim().parse().ok())
        .unwrap_or(dash::DEFAULT_PORT)
}

/// Ensure the machine-level SINGLETON dashboard is up for the step drive path (spec 39,
/// criterion 1; spec 50, criterion 4), unless opted out. The always-on promise retargeted at the
/// singleton: the first `step` of a run starts the one dash at the FIXED [`dash::DEFAULT_PORT`]
/// (never a drifting port); every later step finds it serving and starts none (never a second
/// dash). `config_dash_enabled` is the workflow's resolved [`config::Workflow::dash_enabled`],
/// passed in from the already-loaded config so this never re-reads it.
///
/// OPT-OUT (criterion 4): the ensure is skipped entirely when EITHER the [`DASH_DISABLE_ENV`]
/// environment disable OR the config `dash: off` is set (resolved through
/// [`dash_ensure_suppressed`]) - a headless or CI run then binds NO port at all and proceeds
/// normally. Best-effort and headless-degrading otherwise: a failed start only warns. The started
/// dash is DETACHED so it survives across the run's many short-lived `step` processes.
///
/// `store` is read once here to stamp [`DASH_ATTEMPT_FILE`] with the current run's id via
/// [`record_dash_attempt`] (spec 69, round-8 fix) - by the time this runs, `enforce_definition_pin`
/// has already ensured/minted the run for this step, so the id is always available for a real
/// run. Skipped entirely on the opt-out path above: an opted-out step attempts no dash at all, so
/// there is nothing this run to vouch for.
fn ensure_run_dashboard(config_dash_enabled: bool, store: &dyn EventStore) {
    let env_disabled = std::env::var_os(DASH_DISABLE_ENV).is_some();
    if dash_ensure_suppressed(env_disabled, config_dash_enabled) {
        return;
    }
    if let Ok(events) = store.read_stream(conductor::STREAM, 0, Direction::Forward) {
        record_dash_attempt(&runscope::current_run_id(&events).unwrap_or_default());
    }
    let marker_path = std::path::PathBuf::from(db_path(DASH_MARKER_FILE));
    match ensure_run_dashboard_at(
        &marker_path,
        dash_marker_serving,
        spawn_run_dashboard_detached,
    ) {
        DashStart::Started(port) => {
            // Stderr, not stdout: in the workflow driver (`rigger serve`) stdout is the MCP
            // transport; in the step driver stdout carries only the `{wave,done}` JSON.
            eprintln!("rigger dash: serving this run at http://127.0.0.1:{port}/");
        }
        // A dash is already serving the singleton address - the idempotent no-op, nothing to
        // announce (this run started it earlier, a prior run left it up, or another project did).
        DashStart::AlreadyServing(_) => {}
        DashStart::Failed => {
            eprintln!("rigger: could not auto-start the dashboard; the run continues headless");
        }
    }
}

/// The URL of the dash a driver auto-started for THIS run, recorded in `.rigger/`[`DASH_URL_FILE`]
/// (spec 19b, unit 1 discoverability). Absent when no driver started one (e.g. `rigger status`
/// run before any run began), in which case `rigger status` shows no dashboard line. Purely a
/// read: `rigger status` never starts or stops a dash.
fn recorded_dash_url(loc: &StoreLocation) -> Option<String> {
    let url = std::fs::read_to_string(loc.file(DASH_URL_FILE)).ok()?;
    let url = url.trim().to_string();
    (!url.is_empty()).then_some(url)
}

/// Render `rigger status`'s dashboard line (spec 69, criterion 4: "`rigger status` never lies
/// about the dash"). `None` when nothing was ever recorded ([`dash::DashStatus::Absent`]) - the
/// unchanged silent case. A trusted URL prints EXACTLY as before
/// ([`dash::DashStatus::Serving`]). A probe PROVED the recorded URL dead prints the truthful
/// line instead - naming the matching marker's pid when one is known, or naming none when only
/// a mismatched marker (or none at all) was recorded (round 3: a mismatched marker's pid
/// belongs to an unrelated dash and must never be printed as though it were this URL's) - plus
/// both self-heal paths, so an operator is never sent chasing a URL nothing answers.
fn dash_status_line(status: &dash::DashStatus) -> Option<String> {
    match status {
        dash::DashStatus::Absent => None,
        dash::DashStatus::Serving(url) => Some(format!("dashboard: {url}")),
        dash::DashStatus::NotServing { pid: Some(pid) } => Some(format!(
            "dashboard: not serving (marker names dead pid {pid}) - run 'rigger dash' or the \
             next step restarts it"
        )),
        dash::DashStatus::NotServing { pid: None } => Some(
            "dashboard: not serving (recorded url is unreachable) - run 'rigger dash' or the \
             next step restarts it"
                .to_string(),
        ),
    }
}

/// Render `rigger status --json`'s dashboard entry (spec 69, criterion 4's third clause:
/// "`--json` carries the same truth"), the JSON sibling of [`dash_status_line`]. `None` for
/// [`dash::DashStatus::Absent`] - nothing to append, mirroring the text render's silent case.
/// A trusted URL and a proven-dead marker each render as a small, self-describing object
/// under a `"dashboard"` key - never a bare string or a shape that could be mistaken for an
/// [`progress::AgentActivity`] entry - so the caller can tell it apart from an agent entry by
/// its keys alone. `pid` serializes as `null` when no matching marker named one (round 3).
fn dash_status_json(status: &dash::DashStatus) -> Option<serde_json::Value> {
    let dashboard = match status {
        dash::DashStatus::Absent => return None,
        dash::DashStatus::Serving(url) => serde_json::json!({"status": "serving", "url": url}),
        dash::DashStatus::NotServing { pid } => {
            serde_json::json!({"status": "not_serving", "pid": pid})
        }
    };
    Some(serde_json::json!({ "dashboard": dashboard }))
}

fn cmd_dash(args: &[String]) -> Res {
    // `--export <path>` and/or `--port <n>`; loopback only (no host flag by design).
    // `--reap-on-idle` makes this dash SELF-REAP when the run it serves goes idle/complete
    // (spec 39, criterion 3) - passed only by the DETACHED step-path spawn, never the
    // guard-bound `rigger run` / `run_workflow` dash (which keeps its `ReapedChild`).
    let mut export: Option<String> = None;
    let mut port: u16 = dash::DEFAULT_PORT;
    let mut reap_on_idle = false;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--export" => {
                i += 1;
                export = Some(
                    args.get(i)
                        .cloned()
                        .ok_or("dash: --export expects a path")?,
                );
            }
            "--port" => {
                i += 1;
                port = args
                    .get(i)
                    .and_then(|p| p.parse().ok())
                    .ok_or("dash: --port expects a port number (1-65535)")?;
            }
            "--reap-on-idle" => reap_on_idle = true,
            other => return Err(format!("dash: unknown argument {other:?}").into()),
        }
        i += 1;
    }

    let events_db = db_path("events.db");
    let graph_db = db_path("graph.db");
    let progress_db = db_path("progress.db");
    let identity = project_identity();
    // The scratch root whose markers rigger stats to present each agent's liveness age (spec
    // 14). Resolved once; a repo-less invocation leaves it empty and the view omits ages.
    // The configured remediation bound (same config) sets the `#n/max` on a current-blocker
    // `reject-recurrence` line so the dashboard and `rigger status` agree.
    let (workdir, max_retries) = config::load(".")
        .map(|c| (c.workflow.defaults.workdir, c.workflow.defaults.max_retries))
        .unwrap_or_default();
    let scratch_root = {
        let repo = git_repo();
        if repo.is_empty() {
            String::new()
        } else {
            rigger::worktree::scratch_root_from_env(&repo, &workdir)
        }
    };
    // The FALLBACK release-target base the ready-to-release handoff (spec 38, criterion 3) names
    // on the dash. `build_state` reads the base PERSISTED on this run's RunStarted from the
    // events it projects each request, so the live dash names the base the run actually anchored
    // on even though it inherits only the environment (never the run's `--base` flag). This
    // env/default resolution is used ONLY when the run predates base persistence (or ran with no
    // repo) and carries no persisted base, keeping the dash and `rigger status` on one handoff.
    let (release_base, _) = resolve_run_base(None, std::env::var("RIGGER_BASE").ok().as_deref());

    // The machine-global instance registry the singleton's self-reap watcher polls (spec 50,
    // criterion 5) - resolved ONLY when this is the detached, self-reaping singleton
    // (`--reap-on-idle`) and the environment has a state home (a homeless environment holds no
    // registry, so the watcher never starts and the dash simply serves). `None` for the guard-bound
    // `rigger run` / `run_workflow` dash, which never starts a watcher (it keeps its `ReapedChild`).
    // Retargets spec 39's per-run liveness snapshot at the singleton: the reap trigger is now the
    // registry (every instance on the machine), not this one project's run, so no per-run inputs are
    // captured here.
    let reap_registry_dir = reap_on_idle.then(rigger::registry::default_dir).flatten();

    // The SEPARATE, lazy graph provider for `/api/graph` (spec 45, criteria 1+2). It opens the
    // projection and reads the graph ONLY when a graph request arrives - never on the 1.5s state
    // poll - so a whole-graph read never rides the poll. It reads the WHOLE projection DIRECTLY
    // (`dash_read_whole_graph`), NOT the run-seeded `subgraph(graph_seeds(events), 2)` the polled
    // provider uses, so the overview and the seeded neighborhood reach any node the projection
    // holds - fixing the never-built-repo dead-end where an empty run-seed set collapsed the graph
    // to `Graph::default()`. Read-only and per-request-open like the polled provider: the dash
    // still starts before the store exists, and an absent/empty graph degrades to an empty result,
    // never an error. Its own clones of the db path + identity, captured before the polled
    // `provider` below moves the originals.
    // The machine-global instance registry (spec 50), for the landing list AND the ATTACH
    // resolver (criterion 3). `None` in a homeless environment: the landing is then empty and no
    // `?instance=` selector can resolve, but the dash still serves its own local project.
    let registry_dir = rigger::registry::default_dir();

    let graph_provider = {
        let graph_db = graph_db.clone();
        let identity = identity.clone();
        let registry_dir = registry_dir.clone();
        // The selected instance (spec 50, criterion 3) chooses WHICH store's whole graph is read:
        // the dash's own project by default, an attached instance's LOCAL graph projection when
        // one is selected, or an empty graph for a since-gone selector - never an error.
        move |instance: Option<&str>| -> contextgraph::Graph {
            match dash_resolve_attach(instance, registry_dir.as_deref()) {
                DashAttach::Local => dash_read_whole_graph(&graph_db, &identity),
                DashAttach::Instance(inst) => dash_attach_graph(&inst),
                DashAttach::Empty => contextgraph::Graph::default(),
            }
        }
    };

    // The DIRECTED-CALL provider for `/api/graph?view=calls` (spec 52, criterion 4): the SAME lazy
    // direct-projection provider the whole-graph views use, but running the store-side directed
    // traversal `Projection::calls` (the execution path / call sites of a seed) instead of reading
    // the whole graph. Opened only on a call request, never on the state poll; per-request-open and
    // best-effort like `graph_provider`, so an absent / empty graph or a seed with no calls degrades
    // to an empty `CallGraph`, never an error. Chooses WHICH store to walk from the same spec-50
    // attach selector.
    let calls_provider = {
        let graph_db = graph_db.clone();
        let identity = identity.clone();
        let registry_dir = registry_dir.clone();
        move |instance: Option<&str>,
              seed: &[String],
              direction: contextgraph::Direction,
              depth: i64,
              tier_floor: &str|
              -> contextgraph::CallGraph {
            match dash_resolve_attach(instance, registry_dir.as_deref()) {
                DashAttach::Local => {
                    dash_read_calls(&graph_db, &identity, seed, direction, depth, tier_floor)
                }
                DashAttach::Instance(inst) => {
                    dash_attach_calls(&inst, seed, direction, depth, tier_floor)
                }
                DashAttach::Empty => contextgraph::CallGraph::default(),
            }
        }
    };

    // Fresh projection inputs on every request. Reading (not holding an open handle) is
    // what lets the dash start before the store exists and pick the run up once it does. The
    // selected instance (spec 50, criterion 3) chooses WHICH store is opened per request: the
    // dash's own project by default, an attached instance's stores when one is selected, or an
    // empty state for a since-gone selector (an empty store renders empty, never an error).
    let provider = {
        let registry_dir = registry_dir.clone();
        move |instance: Option<&str>| -> Result<dash::DashInputs, String> {
            match dash_resolve_attach(instance, registry_dir.as_deref()) {
                DashAttach::Local => {
                    let events = dash_read_run(&events_db, &identity).map_err(|e| e.to_string())?;
                    let graph = dash_read_graph(&graph_db, &identity, &events);
                    let run_id = runscope::current_run_id(&events).unwrap_or_default();
                    let progress = dash_read_progress(&progress_db, &identity, &run_id);
                    let liveness = dash_read_liveness(&events, &scratch_root, &run_id);
                    Ok((events, graph, progress, liveness))
                }
                DashAttach::Instance(inst) => Ok(dash_attach_inputs(&inst)),
                DashAttach::Empty => Ok((
                    Vec::new(),
                    contextgraph::Graph::default(),
                    Vec::new(),
                    std::collections::HashMap::new(),
                )),
            }
        }
    };

    // The LANDING provider (spec 50, criterion 3): the machine-global registry projected into the
    // credential-free instance list, freshly read (and stale-pruned) on each `/api/instances` poll.
    let instances_provider = {
        let registry_dir = registry_dir.clone();
        move || -> Vec<dash::InstanceView> {
            match registry_dir.as_deref() {
                Some(dir) => {
                    let now = rigger::registry::now_ms();
                    let live =
                        rigger::registry::read_live(dir, now, rigger::registry::DEFAULT_IDLE_MS);
                    dash::instance_views(&live, now)
                }
                None => Vec::new(),
            }
        }
    };

    match export {
        Some(path) => {
            // An exported snapshot is always of the dash's own local project (no attach selector).
            let (events, graph, progress, liveness) =
                provider(None).map_err(|e| -> Box<dyn std::error::Error> { e.into() })?;
            let html = dash::render_export(
                &events,
                &graph,
                &progress,
                &liveness,
                max_retries,
                RUN_BRANCH,
                &release_base,
            )?;
            std::fs::write(&path, html)?;
            println!("wrote dash snapshot to {path}");
            Ok(())
        }
        None => {
            // Fixed address + singleton (spec 50, criterion 1): bind the RESOLVED address
            // DIRECTLY - no free-port search. If a rigger dash is already serving that address,
            // report it and exit 0 (never a second dash, never a drifted port); a non-dash
            // holder is a genuine conflict the bind error surfaces (resolve it with `--port`).
            let addr = SocketAddr::from(([127, 0, 0, 1], port));
            match dash::bind_singleton(addr)? {
                dash::SingletonBind::AlreadyServing(existing) => {
                    // The singleton is the point: a second invocation reports the ONE known
                    // address and exits cleanly instead of starting a rival dash.
                    println!("rigger dash: already serving on http://{existing}/");
                    Ok(())
                }
                dash::SingletonBind::Bound(listener) => {
                    // Self-reap on machine idle (spec 50, criterion 5): the detached step-path
                    // SINGLETON is started with `--reap-on-idle`, so a background thread polls the
                    // machine-global instance registry and exits this process once NOTHING is
                    // registered-and-alive - leaving no orphaned dash on a quiet machine, while
                    // surviving one project's run ending as long as another's is still live. This
                    // retargets spec 39's per-run liveness trigger at the machine-level singleton;
                    // it is driven by the registry, NOT by any single `step` process exiting.
                    // Read-only: the watcher only reads the registry. The guard-bound `rigger run` /
                    // `run_workflow` dash omits the flag and keeps its `ReapedChild` reaping
                    // instead. Started ONLY on the branch that actually binds and serves - a
                    // short-circuited singleton invocation serves nothing and starts no watcher.
                    if let Some(registry_dir) = reap_registry_dir {
                        let poll = dash_reap_poll();
                        let idle_window = dash_reap_idle_window();
                        std::thread::spawn(move || {
                            watch_and_self_reap_on_idle(registry_dir, idle_window, poll)
                        });
                    }
                    dash::serve_on(
                        listener,
                        provider,
                        graph_provider,
                        calls_provider,
                        instances_provider,
                        max_retries,
                        RUN_BRANCH,
                        &release_base,
                    )?;
                    Ok(())
                }
            }
        }
    }
}

/// The dash self-reap watcher's poll interval (spec 50, criterion 5): how often the detached
/// singleton dash re-checks the machine-global instance registry for a live instance. Env-tunable
/// via [`DASH_REAP_POLL_ENV`] (milliseconds) - the crate's own integration test sets it small so
/// the self-reap is observable quickly - defaulting to [`DASH_REAP_POLL_DEFAULT_MS`]. Clamped to at
/// least 1ms so a `0` override never spins the watcher into a busy loop.
fn dash_reap_poll() -> std::time::Duration {
    let ms = std::env::var(DASH_REAP_POLL_ENV)
        .ok()
        .and_then(|v| v.trim().parse::<u64>().ok())
        .unwrap_or(DASH_REAP_POLL_DEFAULT_MS)
        .max(1);
    std::time::Duration::from_millis(ms)
}

/// The dash self-reap watcher's IDLE WINDOW (spec 50, criterion 5): a registered instance whose
/// heartbeat is older than this counts as no longer live, so once EVERY registered instance has
/// aged past it (and none was refreshed within it) the singleton reaps. Defaults to the registry's
/// own idle window ([`rigger::registry::DEFAULT_IDLE_MS`]) so the reader and the reaper share ONE
/// staleness bound; env-tunable via [`DASH_REAP_STALE_ENV`] (seconds) so the crate's own
/// integration test can make the reap observable on a short window without the shipped multi-minute
/// cadence.
fn dash_reap_idle_window() -> std::time::Duration {
    match std::env::var(DASH_REAP_STALE_ENV)
        .ok()
        .and_then(|v| v.trim().parse::<u64>().ok())
    {
        Some(secs) => std::time::Duration::from_secs(secs),
        None => std::time::Duration::from_millis(rigger::registry::DEFAULT_IDLE_MS),
    }
}

/// The dash self-reap watcher loop (spec 50, criterion 5), run on a background thread inside the
/// detached step-path SINGLETON dash. On each `poll` tick it reads the machine-global instance
/// registry read-only ([`rigger::registry::read_live`], which prunes entries whose heartbeat has
/// aged past `idle_window`); when [`dash::should_reap_singleton`] says the machine is quiet - no
/// registered instance is live and at least one has been seen - it exits the process, terminating
/// the blocked [`dash::serve`] accept loop. This RETARGETS spec 39's per-run liveness watch at the
/// machine-level singleton: the dash serves every registered instance and outlives any single run,
/// so it SURVIVES one project's run ending while another's is still live (that instance keeps
/// `read_live` non-empty) and reaps only on a genuinely idle machine - driven by the registry, NOT
/// by any single `step` process exiting. The `ever_seen_live` latch is the startup-race guard (a
/// just-ensured singleton must not reap before its ensuring run writes its entry). Never returns
/// (it either loops or exits the process).
fn watch_and_self_reap_on_idle(
    registry_dir: PathBuf,
    idle_window: std::time::Duration,
    poll: std::time::Duration,
) -> ! {
    let ttl_ms = u64::try_from(idle_window.as_millis()).unwrap_or(u64::MAX);
    let mut ever_seen_live = false;
    loop {
        std::thread::sleep(poll);
        // Read-only scan of the machine-global registry: every instance whose heartbeat is fresher
        // than the idle window. `read_live` also prunes the entries that have aged out, so a dead
        // run's stale entry cannot keep the singleton alive.
        let live = rigger::registry::read_live(&registry_dir, rigger::registry::now_ms(), ttl_ms);
        if !live.is_empty() {
            // Latch: once any live instance has been seen, a later return to zero is genuine machine
            // idle (not the startup gap before the ensuring run's entry has landed).
            ever_seen_live = true;
        }
        if dash::should_reap_singleton(live.len(), ever_seen_live) {
            // Self-reap: exit the whole process so the detached singleton leaves no orphan on a
            // quiet machine. The stale `.rigger/dash.marker` this leaves behind is deliberately NOT
            // removed - a next run's first `step` already tolerates it (`dash_start_needed` probes
            // the recorded pid, sees it dead, and starts a fresh dash), and removing it here would
            // race a successor dash that may already have rewritten the marker with its own live pid.
            std::process::exit(0);
        }
    }
}

/// Read this project's CURRENT-run events from `events_db` under `identity`, scoped to the
/// latest run exactly as [`stats_lines`] does. An absent db is an empty run and NO file is
/// created (the guard precedes [`Store::open`], which would otherwise fabricate one).
fn dash_read_run(
    events_db: &str,
    identity: &str,
) -> Result<Vec<Event>, Box<dyn std::error::Error>> {
    // Resolve WHICH backend through the one authority, and SURFACE a genuine selection failure
    // (an unreadable `.rigger/store.conn`, an unreadable/malformed `workflow.yml`, an invalid
    // `store.backend`) with `?` - matching every other real-run-stream read (`canary_stats_lines`,
    // `read_model_drift`, `read_run_units`). Swallowing it into a silent local-sqlite default would
    // read the WRONG store on a server-pinned box whose secret file this user cannot read (the
    // different-user / permission edge §48 contemplates), so the dashboard read reports an empty run
    // against a live server (d-u2rr-observer-selection-loud, spec-19c loud-failure-surfacing).
    let sel = store_selection(None, None)?;
    if sel.is_sqlite() && !Path::new(events_db).exists() {
        return Ok(Vec::new());
    }
    let backend = resolve_store(&sel, events_db)?;
    let store = Namespaced::new(backend.as_ref(), identity);
    let all = store.read_stream(conductor::STREAM, 0, Direction::Forward)?;
    Ok(runscope::current_run(&all).to_vec())
}

/// Build the context subgraph around the run's own units/decisions/findings from
/// `graph_db` (seeds via [`dash::graph_seeds`]). Best-effort: an absent graph (a grep-only
/// run never builds one) or any query error yields an empty graph, so the rest of the dash
/// still serves.
fn dash_read_graph(graph_db: &str, identity: &str, events: &[Event]) -> contextgraph::Graph {
    if !Path::new(graph_db).exists() {
        return contextgraph::Graph::default();
    }
    let seeds = dash::graph_seeds(events);
    if seeds.is_empty() {
        return contextgraph::Graph::default();
    }
    match Projector::open(graph_db, identity) {
        Ok(p) => p.subgraph(&seeds, 2).unwrap_or_default(),
        Err(_) => contextgraph::Graph::default(),
    }
}

/// The WHOLE projection for the `/api/graph` provider (spec 45, criterion 2): the direct-projection
/// read the dedicated, lazy graph provider consults on a graph request. Unlike [`dash_read_graph`]
/// it does NOT go through `graph_seeds` - it reads the entire live projection ([`Projector::whole`]),
/// so the seeded neighborhood and whole-graph overview reach ANY node the projection holds. That
/// fixes the never-built-repo dead-end: on a graph populated by code ingest with no run
/// decisions/findings, `graph_seeds` is empty and the run-seeded read collapses to
/// `Graph::default()`, whereas this read still returns the code nodes and their edges. Best-effort
/// like the run-seeded read: an absent graph (a grep-only run never builds one), an open error, or a
/// query error all degrade to an empty graph, never an error, so the rest of the dash still serves.
fn dash_read_whole_graph(graph_db: &str, identity: &str) -> contextgraph::Graph {
    if !Path::new(graph_db).exists() {
        return contextgraph::Graph::default();
    }
    match Projector::open(graph_db, identity) {
        Ok(p) => p.whole().unwrap_or_default(),
        Err(_) => contextgraph::Graph::default(),
    }
}

/// The DIRECTED-CALL walk for the `/api/graph?view=calls` provider (spec 52, criterion 4): the
/// store-side `Projection::calls` traversal (the seed's execution path or call sites) read through
/// the SAME lazy direct-projection open as [`dash_read_whole_graph`], never the polled read. Opens
/// the projection per request and runs the walk `direction`/`depth`/`tier_floor` select. Best-effort
/// exactly like the whole-graph read: an absent graph (a grep-only run never builds one), an open
/// error, or a walk error all degrade to an empty [`contextgraph::CallGraph`], never an error, so a
/// call request over a never-built or empty graph renders an empty view instead of failing.
fn dash_read_calls(
    graph_db: &str,
    identity: &str,
    seed: &[String],
    direction: contextgraph::Direction,
    depth: i64,
    tier_floor: &str,
) -> contextgraph::CallGraph {
    if !Path::new(graph_db).exists() {
        return contextgraph::CallGraph::default();
    }
    match Projector::open(graph_db, identity) {
        Ok(p) => p
            .calls(seed, direction, depth, tier_floor)
            .unwrap_or_default(),
        Err(_) => contextgraph::CallGraph::default(),
    }
}

/// The directed-call walk over an ATTACHED instance's graph store (spec 52 c4 + spec 50 c3): the
/// [`dash_read_calls`] analogue of [`dash_attach_graph`], opening the selected instance's `graph.db`
/// read-only. Best-effort - a since-gone or never-built instance graph degrades to an empty
/// [`contextgraph::CallGraph`], never an error.
fn dash_attach_calls(
    inst: &rigger::registry::Instance,
    seed: &[String],
    direction: contextgraph::Direction,
    depth: i64,
    tier_floor: &str,
) -> contextgraph::CallGraph {
    let graph_db = instance_rigger_dir(inst)
        .join("graph.db")
        .to_string_lossy()
        .into_owned();
    dash_read_calls(&graph_db, &inst.project, seed, direction, depth, tier_floor)
}

/// This run's progress from the SEPARATE progress store (spec 14), for the dash's live
/// per-agent view. Absent/empty is fine (the store is created lazily by the first
/// `rigger progress`), and only the current run's reports (by `run_id`) are returned.
fn dash_read_progress(progress_db: &str, identity: &str, run_id: &str) -> Vec<Event> {
    if !Path::new(progress_db).exists() {
        return Vec::new();
    }
    let Ok(backend) = Store::open(progress_db) else {
        return Vec::new();
    };
    let store = Namespaced::new(&backend, identity);
    store
        .read_stream(progress::STREAM, 0, Direction::Forward)
        .unwrap_or_default()
        .into_iter()
        .filter(|e| {
            run_id.is_empty()
                || e.meta.get(runscope::META_RUN_ID).map(String::as_str) == Some(run_id)
        })
        .collect()
}

/// The liveness-marker age (whole seconds since last touch) for each in-flight spawn in
/// `events` (the current run's slice), read HERE in Rust so the dash PRESENTS it (spec 14) -
/// the same stat the retired probe did, done by rigger rather than a spawned agent. Empty
/// when there is no scratch root (a repo-less invocation).
fn dash_read_liveness(
    events: &[Event],
    scratch_root: &str,
    run_id: &str,
) -> std::collections::HashMap<String, u64> {
    let mut ages = std::collections::HashMap::new();
    if scratch_root.is_empty() {
        return ages;
    }
    let Ok(step) = spawn::step_result(events) else {
        return ages;
    };
    let now = std::time::SystemTime::now();
    for w in &step.wave {
        let path = rigger::liveness::marker_path(scratch_root, run_id, &w.id);
        if let Ok(age) = std::fs::metadata(&path)
            .and_then(|md| md.modified())
            .map(|mtime| now.duration_since(mtime).map(|d| d.as_secs()).unwrap_or(0))
        {
            ages.insert(w.id.clone(), age);
        }
    }
    ages
}

/// Which registered instance a dash request ATTACHES to (spec 50, criterion 3), resolved from the
/// request's `?instance=<id>` selector against the machine-global registry.
enum DashAttach {
    /// No instance selected: serve the dash's OWN local project (backward compatible - today's
    /// single-project dash).
    Local,
    /// Serve this registered instance's stores, read-only.
    Instance(rigger::registry::Instance),
    /// An instance was requested but is unknown or has aged out of the registry: serve an EMPTY
    /// state - never the local default (a since-gone selection must not silently show the local
    /// run) and never an error (an empty store renders an empty state, spec 50).
    Empty,
}

/// Resolve which instance a dash request attaches to (spec 50, criterion 3). An absent/empty
/// selector keeps the dash on its own local project; a selector names a registry entry by its
/// stable id, resolved through a fresh [`rigger::registry::read_live`] (which also prunes stale
/// entries) so a per-request open always lands on a currently-live instance. An unresolvable
/// selector (homeless environment, unknown id, or a pruned-stale entry) degrades to [`DashAttach::Empty`].
fn dash_resolve_attach(instance: Option<&str>, dir: Option<&Path>) -> DashAttach {
    let Some(id) = instance.filter(|s| !s.is_empty()) else {
        return DashAttach::Local;
    };
    let Some(dir) = dir else {
        return DashAttach::Empty;
    };
    let live = rigger::registry::read_live(
        dir,
        rigger::registry::now_ms(),
        rigger::registry::DEFAULT_IDLE_MS,
    );
    match live.into_iter().find(|i| i.id() == id) {
        Some(inst) => DashAttach::Instance(inst),
        None => DashAttach::Empty,
    }
}

/// A registered instance's `.rigger` directory, where its LOCAL knowledge-graph and progress
/// projections live regardless of whether its EVENT store is local sqlite or a shared server (the
/// KG is built locally per project - spec 50: "the knowledge-graph views open that instance's
/// local graph projection").
fn instance_rigger_dir(inst: &rigger::registry::Instance) -> PathBuf {
    Path::new(&inst.root).join(RIGGER_DIR)
}

/// Read a registered instance's current-run events, read-only (spec 50, criterion 3). A Local
/// instance is opened directly as sqlite at its registered log path; a Shared instance resolves
/// its connection through the store-resolution authority at the instance's OWN `.rigger` (the same
/// config that lets a worker report to that shared store lets the dash read it), read under the
/// instance's namespace identity. Best-effort: an absent, unreachable, or unreadable store degrades
/// to an empty run - "an empty store renders an empty state, never an error" - because the selected
/// instance is discovery metadata, not a source of truth.
///
/// Read an instance's `run` stream from an embedded sqlite event log at `path`, READ-ONLY: an
/// ABSENT file degrades to an empty run (`Ok(Vec::new())`) rather than opening it, because
/// [`open_sqlite_store`] -> [`Store::open`] CREATES the file AND its schema, and a dash attach is a
/// read-only projection that MUST NEVER write a store under a foreign project (spec 50, the
/// read-only global constraint). This is the ONE read-only sqlite attach reader: the Local arm and
/// the Shared arm's Sqlite-degrade BOTH route through it, so the existence guard lives in exactly
/// one place and no attach path can open-create a phantom `events.db`.
fn dash_read_sqlite_stream_readonly(
    path: &str,
    project: &str,
) -> Result<Vec<Event>, Box<dyn std::error::Error>> {
    if !Path::new(path).exists() {
        return Ok(Vec::new());
    }
    let backend = open_sqlite_store(path)?;
    let store = Namespaced::new(&backend, project);
    Ok(store.read_stream(conductor::STREAM, 0, Direction::Forward)?)
}

fn dash_attach_run(inst: &rigger::registry::Instance) -> Vec<Event> {
    let read = || -> Result<Vec<Event>, Box<dyn std::error::Error>> {
        let all = match &inst.store {
            rigger::registry::StoreIdentity::Local { path } => {
                dash_read_sqlite_stream_readonly(path, &inst.project)?
            }
            rigger::registry::StoreIdentity::Shared { .. } => {
                let rigger_dir = instance_rigger_dir(inst);
                // Resolve through the ATTACHED instance's OWN `.rigger` with NO ambient environment
                // (`None`, never `env_conn()`): the dash process's own `KURRENTDB_CONN` addresses a
                // DIFFERENT project's store, so letting it win (§48 rung 2) would attach the wrong
                // store. The instance's own secret file / committed choice (rungs 3-4) is the
                // authority for reading THAT instance (adv-u50c3-uphold-sdet-env-precedence).
                let sel = store_selection_at(None, None, None, &rigger_dir)?;
                let events_db = rigger_dir.join("events.db");
                let events_db_path = events_db.to_string_lossy();
                match sel {
                    // The instance registered as Shared but its own config no longer resolves a
                    // server (its secret file / config is gone): a Sqlite DEGRADE. Guard existence
                    // EXACTLY like the Local arm - a read-only attach must NEVER open-create the
                    // store, so an absent `events.db` renders an empty run, never a phantom store
                    // file written under a foreign project (adv-u50c3-shared-attach-creates-phantom-store).
                    StoreSelection::Sqlite => {
                        dash_read_sqlite_stream_readonly(&events_db_path, &inst.project)?
                    }
                    StoreSelection::Server(_) => {
                        let backend = resolve_store(&sel, &events_db_path)?;
                        let store = Namespaced::new(backend.as_ref(), &inst.project);
                        store.read_stream(conductor::STREAM, 0, Direction::Forward)?
                    }
                }
            }
        };
        Ok(runscope::current_run(&all).to_vec())
    };
    read().unwrap_or_default()
}

/// The cheap per-request inputs for an ATTACHED instance (spec 50, criterion 3): its run events,
/// the run-seeded context subgraph, and this-run progress - all from that instance's stores, read
/// read-only. The graph and progress are ALWAYS the instance's LOCAL projections under its
/// `.rigger`; only the event store may be remote. Liveness ages are the LOCAL run's scratch, which
/// a possibly-remote instance has none of reachable here, so its per-agent ages are simply absent.
fn dash_attach_inputs(inst: &rigger::registry::Instance) -> dash::DashInputs {
    let events = dash_attach_run(inst);
    let rigger_dir = instance_rigger_dir(inst);
    let graph_db = rigger_dir.join("graph.db").to_string_lossy().into_owned();
    let progress_db = rigger_dir
        .join("progress.db")
        .to_string_lossy()
        .into_owned();
    let graph = dash_read_graph(&graph_db, &inst.project, &events);
    let run_id = runscope::current_run_id(&events).unwrap_or_default();
    let progress = dash_read_progress(&progress_db, &inst.project, &run_id);
    (events, graph, progress, std::collections::HashMap::new())
}

/// The WHOLE knowledge-graph projection for an ATTACHED instance (spec 50, criterion 3), the
/// `/api/graph` view's lazy read: that instance's LOCAL `graph.db` under its `.rigger`, read
/// directly ([`dash_read_whole_graph`]) so it reaches any node the projection holds even when the
/// run seeded none. Best-effort/empty-degrade like the local read.
fn dash_attach_graph(inst: &rigger::registry::Instance) -> contextgraph::Graph {
    let graph_db = instance_rigger_dir(inst)
        .join("graph.db")
        .to_string_lossy()
        .into_owned();
    dash_read_whole_graph(&graph_db, &inst.project)
}

/// `rigger ground "<query>" [<k>]` - run the project's configured grounder (the
/// same one the `run`/`serve` paths build from `defaults.grounder` via
/// [`select_grounder`]) over the repo and print up to `k` (default 8) relevant
/// references, one per line as `file:line: <text>`. Empty output when nothing is
/// relevant. This is the CLI surface a native-workflow agent (which has Bash, not
/// the MCP grounding tool) uses to ground.
fn cmd_ground(args: &[String]) -> Res {
    let query = args
        .first()
        .ok_or("ground: expected a query: rigger ground \"<query>\" [<k>]")?;
    let k: usize = match args.get(1) {
        Some(s) => s
            .parse()
            .map_err(|_| format!("ground: <k> must be a non-negative integer, got {s:?}"))?,
        None => 8,
    };
    if args.len() > 2 {
        return Err(format!(
            "ground: expected at most a query and k, got {} arguments",
            args.len()
        )
        .into());
    }
    // Honor the project's configured `defaults.grounder` when a config is present;
    // a project with no `.rigger/workflow.yml` yet falls back to the default grounder
    // (the empty name -> symbols, the scaffold default), so an agent can ground before
    // a workflow is authored rather than hitting a config error.
    let name = config::load(".")
        .map(|cfg| cfg.workflow.defaults.grounder)
        .unwrap_or_default();
    let grounder = select_grounder(&name)?;
    for r in grounder.ground(query, k) {
        println!("{}:{}: {}", r.file, r.line, r.text);
    }
    Ok(())
}

/// `rigger reindex <file>...` - incrementally re-index the named files in the
/// project's persisted grounding index. It resolves the grounder from
/// `defaults.grounder` via [`select_reindex_grounder`] (rooted at `.`) - which, after
/// turbovec's retirement, resolves IDENTICALLY to [`select_grounder`]: the `symbols`
/// grounder's `open` only LOADS the persisted index (it does not freshen the whole
/// tree), so the named files are re-parsed exactly ONCE here rather than once by a
/// load-time freshen and again by the reindex. It then calls [`Grounder::reindex`] on
/// the changed files, so the `symbols` grounder drops each file's old symbols, re-parses
/// its current content, and persists the delta to `.rigger/symbols/` - a later `rigger
/// ground` (and the review tier the workflow runs after a unit lands) then reflects the
/// just-integrated code WITHOUT re-indexing the whole repo. For the grep / nop
/// grounders `reindex` is a no-op (they re-read the tree each call), so this command is
/// harmless there. Files are repo-relative, matching how the grounder records and
/// grounds them. At least one file is required.
fn cmd_reindex(args: &[String]) -> Res {
    if args.is_empty() {
        return Err("reindex: expected at least one file: rigger reindex <file>...".into());
    }
    // Same selection path as `cmd_ground`: honor `defaults.grounder` when a config
    // is present, else the unset default (symbols). The grounder is rooted at `.`,
    // so the persisted index it loads/updates is this project's `.rigger/symbols/`.
    let name = config::load(".")
        .map(|cfg| cfg.workflow.defaults.grounder)
        .unwrap_or_default();
    // Use the reindex-specific constructor: it loads the persisted index WITHOUT a
    // whole-tree freshen, so `reindex` re-parses ONLY the named files - never those
    // files twice (once by a load-time freshen, once by the reindex below).
    let grounder = select_reindex_grounder(&name)?;
    grounder.reindex(".", args);
    println!(
        "reindexed {} file(s) in the grounding index: {}",
        args.len(),
        args.join(", ")
    );
    Ok(())
}

/// `rigger symbols-index [<dir>]` - the criterion-3 fresh-process determinism harness for the
/// `symbols` structural index (spec 15, unit 3). It builds the whole-project symbol index over
/// `<dir>` (default `.`) via [`rigger::grounder::symbols::build_index`] and persists it with
/// [`rigger::grounder::symbols::store::save`], then prints the persisted path and file count.
///
/// It is DELIBERATELY independent of [`select_grounder`] / `defaults.grounder`: it drives unit
/// 3's own build+persist path directly, so a determinism test can re-index the SAME tree in two
/// SEPARATE `rigger` processes and diff the persisted `index.json` byte-for-byte - the
/// cross-process check the in-process lib test structurally cannot make, since one process
/// shares a single hash seed. Keeping this off the grounder-selection wiring also keeps the
/// spec-15 unit DAG acyclic (this harness needs only unit 3's code, never unit 4's selection).
///
/// Feature-gated on `symbols`: a build without it has no structural index, so the command
/// errors loudly rather than pretending to build one (the same no-silent-degrade rule the
/// grounder selection follows).
fn cmd_symbols_index(args: &[String]) -> Res {
    #[cfg(feature = "symbols")]
    {
        if args.len() > 1 {
            return Err(format!(
                "symbols-index: expected at most a directory, got {} arguments",
                args.len()
            )
            .into());
        }
        let dir = args.first().map(String::as_str).unwrap_or(".");
        let idx = rigger::grounder::symbols::build_index(dir, None);
        rigger::grounder::symbols::store::save(&idx, dir)?;
        println!(
            "symbols index: {} file(s) -> {}",
            idx.files().len(),
            rigger::grounder::symbols::store::index_path(dir).display()
        );
        Ok(())
    }
    #[cfg(not(feature = "symbols"))]
    {
        let _ = args;
        Err(
            "symbols-index requires the `symbols` feature; rebuild with the default features"
                .into(),
        )
    }
}

/// `rigger emit <type> '<json-object>'` - append an event `{type: <type>, data:
/// <parsed json>}` to the project's event store AND fold it into the context graph,
/// EXACTLY as the MCP `rigger_emit` tool does (both call [`mcpserver::emit_event`]).
/// The store and graph are opened the way `serve` opens them - the namespaced
/// per-project event store and the `graph.db` projector on the `conductor::STREAM`.
/// A bad / non-object JSON payload is a clear error to stderr with a non-zero exit.
fn cmd_emit(args: &[String]) -> Res {
    // Optional `--spawn <id>`: stamp the emit with the EMITTING spawn's id
    // ([`META_SPAWN`](conductor::META_SPAWN)) at RECORD time. A native courier's `rigger emit`
    // is otherwise unattributable once the conductor replays it (the conductor never touched
    // it), so the verdict-channel-mismatch backstop (spec 18, unit 3) could not tell a GATING
    // adjudicator's OWN approve from a concurrent sibling's by position alone. The workflow
    // threads the worker's own spawn id here, exactly as the cli emit callback and the workflow
    // MCP server stamp their emits, so the recording the ReplayDriver later folds already
    // names its emitting spawn and is correlated by identity, never a shared-stream position.
    let mut spawn: Option<&str> = None;
    let mut positional: Vec<&String> = Vec::new();
    let mut it = args.iter();
    while let Some(a) = it.next() {
        if a == "--spawn" {
            spawn = Some(
                it.next()
                    .ok_or("emit: --spawn expects a spawn id: rigger emit --spawn <id> <type> '<json-object>'")?
                    .as_str(),
            );
        } else {
            positional.push(a);
        }
    }
    let typ = positional
        .first()
        .ok_or("emit: expected a type: rigger emit [--spawn <id>] <type> '<json-object>'")?;
    let json_arg = positional
        .get(1)
        .ok_or("emit: expected a JSON object: rigger emit [--spawn <id>] <type> '<json-object>'")?;
    if positional.len() > 2 {
        return Err(format!(
            "emit: expected a type and a single JSON object, got {} arguments",
            positional.len()
        )
        .into());
    }
    let data: serde_json::Value = serde_json::from_str(json_arg)
        .map_err(|e| format!("emit: <json-object> is not valid JSON: {e}"))?;
    if !data.is_object() {
        return Err(format!(
            "emit: <json-object> must be a JSON object, got {}",
            json_type_name(&data)
        )
        .into());
    }

    // Resolve the EXISTING store (walk up; refuse if none) rather than fabricating one
    // in the wrong cwd, and scope it by the RESOLVED root's identity (not the cwd's), so
    // a walked-up write lands in the stream the conductor reads - see [`require_store_dir`].
    let (loc, selection) = require_store_dir()?;
    let backend = resolve_store(&selection, &loc.file("events.db"))?;
    let store = Namespaced::new(backend.as_ref(), &loc.identity());
    let graph = Projector::open(&loc.file("graph.db"), &loc.identity())?;

    // Same args shape the MCP tool receives, so emit_event - the shared core both
    // surfaces call - behaves identically here and over MCP. A non-empty `--spawn <id>`
    // rides in `meta.spawn`, the same key the MCP server's `stamp_current_spawn` writes.
    let mut tool_args = serde_json::json!({ "type": typ, "data": data });
    if let Some(spawn) = spawn.filter(|s| !s.is_empty()) {
        let mut meta = serde_json::Map::new();
        meta.insert(
            conductor::META_SPAWN.to_string(),
            serde_json::Value::String(spawn.to_string()),
        );
        tool_args
            .as_object_mut()
            .expect("json! built an object")
            .insert("meta".to_string(), serde_json::Value::Object(meta));
    }
    let pos = mcpserver::emit_event(&store, conductor::STREAM, Some(&graph), &tool_args)?;
    println!("emitted {typ} (position {pos}) and folded it into the context graph");
    Ok(())
}

/// `rigger progress <id> "<activity>"` - record one live progress report for spawn `<id>`
/// to the SEPARATE progress store (`.rigger/progress.db`), stamped with the current run
/// (spec 14, Gap 27). `<activity>` is a short one-line description of what the agent just
/// did (a grep, a build, a commit, a decision). The report NEVER touches the run stream -
/// it lands in its own store, so replay stays byte-identical - and rigger reads it back
/// (the consolidator) to PRESENT a live per-agent view. A pure append: the run is resolved
/// read-only from the run store to scope the report, and only the progress store is written.
/// Routed through [`require_store_dir`] like the other courier commands, so a worker running
/// it from a nested worktree records into the project's real store, never a misfiled one.
fn cmd_progress(args: &[String]) -> Res {
    let id = args
        .first()
        .ok_or("progress: expected a spawn id: rigger progress <id> \"<activity>\"")?;
    let activity = args
        .get(1)
        .ok_or("progress: expected an activity: rigger progress <id> \"<activity>\"")?;
    if args.len() > 2 {
        return Err(format!(
            "progress: expected an id and a single activity string, got {} arguments",
            args.len()
        )
        .into());
    }
    if activity.trim().is_empty() {
        return Err("progress: <activity> must be non-empty".into());
    }

    let (loc, selection) = require_store_dir()?;
    // Resolve the current run READ-ONLY from the run store, only to scope the report.
    let run_backend = resolve_store(&selection, &loc.file("events.db"))?;
    let run_store = Namespaced::new(run_backend.as_ref(), &loc.identity());
    let events = run_store.read_stream(conductor::STREAM, 0, Direction::Forward)?;
    let run_id = runscope::current_run_id(&events).unwrap_or_default();
    // Append to the SEPARATE progress store - never the run stream.
    let prog_backend = Store::open(&loc.file("progress.db"))?;
    let prog_store = Namespaced::new(&prog_backend, &loc.identity());
    let pos = rigger::progress::record(&prog_store, &run_id, id, activity)?;
    println!("progress recorded for {id} (position {pos})");
    Ok(())
}

/// `rigger status [--json]` - present the live per-agent view of the current run (spec 14,
/// unit 2). Rigger CONSOLIDATES its three signals for every in-flight spawn - the run-stream
/// milestone, the latest progress report, and the liveness-marker age it reads in Rust here
/// (so no consumer stats a file) - into one view: what each agent is at, what it is doing,
/// how long since its last activity and heartbeat, and how long since its last store event
/// (the blackout this closes). `--json` prints the machine shape the shim and the dash also
/// consume; the default is a readable table. Read-only over the run store, the separate
/// progress store, and the liveness markers.
fn cmd_status(args: &[String]) -> Res {
    let mut json = false;
    for a in args {
        match a.as_str() {
            "--json" => json = true,
            other => return Err(format!("status: unknown argument {other:?} (only --json)").into()),
        }
    }
    let (loc, selection) = require_store_dir()?;
    let now = std::time::SystemTime::now();

    // The current run's slice of the run stream, and its id.
    let run_backend = resolve_store(&selection, &loc.file("events.db"))?;
    let run_store = Namespaced::new(run_backend.as_ref(), &loc.identity());
    let all = run_store.read_stream(conductor::STREAM, 0, Direction::Forward)?;
    let run_events = runscope::current_run(&all);
    let run_id = runscope::current_run_id(&all).unwrap_or_default();

    // This run's progress, from the SEPARATE store (absent/empty is fine - the store is
    // created lazily by the first `rigger progress`).
    let prog_events: Vec<Event> = match Store::open(&loc.file("progress.db")) {
        Ok(backend) => {
            let store = Namespaced::new(&backend, &loc.identity());
            store
                .read_stream(progress::STREAM, 0, Direction::Forward)
                .unwrap_or_default()
                .into_iter()
                .filter(|e| {
                    run_id.is_empty()
                        || e.meta.get(runscope::META_RUN_ID).map(String::as_str)
                            == Some(run_id.as_str())
                })
                .collect()
        }
        Err(_) => Vec::new(),
    };

    // Liveness ages: rigger stats each in-flight spawn's marker IN RUST here (this is what
    // the JS driver's haiku probe was reconstructing by proxy - unit 3 retires it). The
    // configured remediation bound is read from the SAME config so the current-blocker
    // classifier's `#n/max` line matches the depth the run actually escalates at.
    let (workdir, max_retries) = config::load(".")
        .map(|c| (c.workflow.defaults.workdir, c.workflow.defaults.max_retries))
        .unwrap_or_default();
    let repo = git_repo();
    let mut liveness_ages: std::collections::HashMap<String, u64> =
        std::collections::HashMap::new();
    if !repo.is_empty() {
        let root = rigger::worktree::scratch_root_from_env(&repo, &workdir);
        for w in &spawn::step_result(run_events)?.wave {
            let path = rigger::liveness::marker_path(&root, &run_id, &w.id);
            if let Ok(age) = std::fs::metadata(&path)
                .and_then(|md| md.modified())
                .map(|mtime| now.duration_since(mtime).map(|d| d.as_secs()).unwrap_or(0))
            {
                liveness_ages.insert(w.id.clone(), age);
            }
        }
    }

    let view = progress::consolidate(run_events, &prog_events, &liveness_ages, now)?;

    // Spec 69, criterion 4: never printed (or, under `--json`, ever wired) on trust alone.
    // [`dash::dash_status`] probes with [`dash::dash_serving_on`] directly - the SAME
    // underlying probe [`dash_marker_serving`] wraps for the step path's own idempotent-start
    // decision, so this surface and the step path can never disagree about whether a recorded
    // dash is alive. A marker-less recorded URL (the guard-bound `rigger run` / `rigger
    // serve` dash writes none) is unverifiable, not dead, and stays trusted exactly as before
    // this criterion; a marker naming a DIFFERENT dash than the recorded URL (round 3,
    // adv-u69c4r2-mismatched-marker-still-trusts-a-dead-url) is NOT "nothing to check" either -
    // `dash_status` probes the URL's own port directly in that case, never the marker's
    // unrelated one. Computed HERE, before the `--json` early return below, so `--json`
    // carries the same truth as the human table (round 2: the prior placement after the
    // `--json` return left `--json` structurally unable to compute it at all).
    let dash_marker = dash::DashMarker::read(Path::new(&loc.file(DASH_MARKER_FILE)));
    let dash_status =
        dash::dash_status(recorded_dash_url(&loc), dash_marker, dash::dash_serving_on);

    if json {
        // The dashboard truth is APPENDED to the in-flight-agent array rather than wrapping
        // it in a new top-level shape: the liveness courier (workflows/rigger.js:218) does
        // `Array.isArray(arr) ? arr.find(a => a.id === id) : null` over this exact line, so
        // the top level MUST stay a bare array - wrapping it in an object would silently and
        // permanently blind spec 10/14's liveness watchdog (`find` never runs, every
        // liveness read becomes `null`, "conservatively" treated as never-stale forever).
        // Absent (nothing ever recorded) appends nothing, so a project with no dash history
        // gets byte-identical `--json` output to before this criterion; the appended object
        // carries no `id` field, so it can never collide with a real spawn id.
        let mut value = serde_json::to_value(&view)?;
        if let (Some(arr), Some(dashboard)) = (value.as_array_mut(), dash_status_json(&dash_status))
        {
            arr.push(dashboard);
        }
        println!("{}", serde_json::to_string(&value)?);
        return Ok(());
    }

    // The current-blocker line per unfinished unit (spec 19a, unit 1), from the shared
    // classifier the dashboard also renders - so `rigger status` and the dashboard show
    // the SAME lines. Computed even when no agent is parked, so an escalated unit or a
    // budget halt (which have no live spawn) is still surfaced.
    let blocker_lines = status_blocker_lines(run_events, max_retries)?;

    // The ready-to-release handoff (spec 38, criterion 3): surfaced on this status surface
    // when the run is DONE (every unit integrated, no failed deferred gate), naming the run
    // branch, the release-target base, the integrated-unit count, and the PR command. Empty
    // for a run that is not done, so an unfinished run surfaces NO release-ready signal. The
    // base is the one PERSISTED on this run's RunStarted, so status names the SAME base the run
    // anchored on - `rigger status` runs without the run's `--base` flag on its argv and so
    // cannot re-resolve it. A run predating base persistence (or with no repo) carries none, so
    // fall back to the `RIGGER_BASE` env / load-bearing default. A done run has no live spawns,
    // so this must also print in the no-agents-in-flight branch below, never only in the
    // agents-in-flight path.
    let release_base = runscope::current_run_base(run_events)
        .unwrap_or_else(|| resolve_run_base(None, std::env::var("RIGGER_BASE").ok().as_deref()).0);
    let release_lines = release_ready_lines(run_events, RUN_BRANCH, &release_base);

    // The auto-started dash's URL for this run (spec 19b, unit 1 discoverability): shown
    // whenever a driver recorded one, even for an otherwise-quiet run, so an operator can
    // always find the live observability page. Printed before the run summary so it appears
    // in the "no agents in flight" case too.
    if let Some(line) = dash_status_line(&dash_status) {
        println!("{line}");
    }

    // Readable table. The blackout is visible as `last store event` age >> activity age.
    let short = |s: &str| s.chars().take(12).collect::<String>();
    if view.is_empty() && blocker_lines.is_empty() {
        println!("run {}: no agents in flight", short(&run_id));
        for line in &release_lines {
            println!("{line}");
        }
        return Ok(());
    }
    if view.is_empty() {
        println!("run {}: no agents in flight", short(&run_id));
    } else {
        let age = |s: Option<u64>| s.map(|s| format!("{s}s ago")).unwrap_or_else(|| "-".into());
        println!("run {}: {} agent(s) in flight", short(&run_id), view.len());
        for a in &view {
            println!("  {} [{}]", a.id, a.stage);
            println!(
                "      doing: {} ({}) | heartbeat {} | last store event: {} ({})",
                a.latest_activity
                    .as_deref()
                    .unwrap_or("(none reported yet)"),
                age(a.activity_age_s),
                a.liveness_age_s
                    .map(|s| format!("{s}s ago"))
                    .unwrap_or_else(|| "-".into()),
                a.last_milestone.as_deref().unwrap_or("-"),
                age(a.milestone_age_s),
            );
        }
    }
    if !blocker_lines.is_empty() {
        println!("current blockers:");
        for line in &blocker_lines {
            println!("  {line}");
        }
    }
    // Non-empty only when the run is done; a no-op otherwise, so an in-flight run prints
    // nothing here (the done case has no live spawns and is handled in the branch above).
    for line in &release_lines {
        println!("{line}");
    }
    Ok(())
}

/// The current-blocker lines `rigger status` prints (spec 19a, unit 1): one line per
/// unfinished unit, plus the run-level budget halt, from the SHARED
/// [`blocker`](rigger::blocker) classifier the dashboard also renders. Pure over the
/// run's event slice and the configured remediation bound, so it renders identically to
/// the dashboard (which calls the same [`blocker::from_events`]) and is unit-testable
/// without a store.
fn status_blocker_lines(
    run_events: &[Event],
    configured_max_retries: u32,
) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    Ok(blocker::lines(&blocker::from_events(
        run_events,
        configured_max_retries,
    )?))
}

/// The ready-to-release handoff lines `rigger status` prints (spec 38, criterion 3): empty
/// for any run that is NOT done, else the summary naming the run branch, the release-target
/// base, the integrated-unit count, and the exact PR command. Pure over the run's event
/// slice plus the resolved run branch/base, so it is unit-testable without a store and
/// renders identically wherever it is surfaced - the single authority is
/// [`ledger::RunState::release_ready`] + [`ledger::ReleaseReady::lines`], never a second
/// derivation. A projection hiccup yields no lines rather than failing the status read.
fn release_ready_lines(run_events: &[Event], run_branch: &str, base: &str) -> Vec<String> {
    ledger::project(run_events)
        .ok()
        .and_then(|rs| rs.release_ready(run_branch, base))
        .map(|rr| rr.lines())
        .unwrap_or_default()
}

/// Parsed `rigger watch` arguments (see [`parse_watch_args`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct WatchArgs {
    /// `--once`: print standing anomalies and exit, rather than streaming.
    once: bool,
    /// `--interval <s>`: the streaming poll period, in seconds.
    interval_secs: u64,
}

/// Parse `rigger watch`'s two flags, extracted from [`cmd_watch`] so the loop and every
/// arm is directly testable with plain string-slice inputs - no store, no cwd, no clock.
/// `--once` is a bare flag; `--interval <s>` takes the next argument, parsed as an
/// integer number of seconds; an unrecognized argument refuses naming the usage.
fn parse_watch_args(args: &[String]) -> Result<WatchArgs, Box<dyn std::error::Error>> {
    let mut once = false;
    let mut interval_secs = watch::DEFAULT_INTERVAL_SECS;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--once" => {
                once = true;
                i += 1;
            }
            "--interval" => {
                let v = args.get(i + 1).ok_or("watch: --interval expects seconds")?;
                interval_secs = v.parse::<u64>().map_err(|_| {
                    format!("watch: --interval expects an integer number of seconds, got {v:?}")
                })?;
                i += 2;
            }
            other => {
                return Err(format!(
                    "watch: unknown argument {other:?} (usage: rigger watch [--interval <s>] \
                     [--once])"
                )
                .into())
            }
        }
    }
    Ok(WatchArgs {
        once,
        interval_secs,
    })
}

/// `rigger watch [--interval <s>] [--once]` (spec 69, criterion 2): the
/// driver-independent watchdog. Gathers the store, process-table, and status truth
/// [`watch::detect`] needs and prints one line per anomaly - naming signal, subject,
/// and response - so an orchestrator armed on this command sees exactly what a
/// manual `rigger-watch-a-run` look would, without polling anything by hand.
///
/// `--once` prints the CURRENT standing anomalies and exits (the cron/CI shape): a poll
/// failure here (no store, a genuinely unreadable one, a bad flag upstream) propagates and
/// exits non-zero, exactly like every other one-shot courier command.
///
/// Without it, this STREAMS: poll, print only what is new or has worsened since the last poll
/// (in-process [`watch::Dedup`] - spec 69 Design: "dedup state lives in process memory
/// only"), sleep `--interval` seconds (default [`watch::DEFAULT_INTERVAL_SECS`]), and repeat
/// forever - the harness's background monitor is the intended host for this loop. A poll
/// failure while streaming is FAIL-SOFT, not fatal: reported on stderr and retried on the
/// next tick, never propagated out of the process. This is the whole point named by this
/// command's own Design text ("it must work with the driver dead" - the watchdog reads only
/// store, process table, and status, never the driver, exactly the process that may be dead)
/// carried one step further: a watchdog armed unattended must also outlive a TRANSIENT fault
/// in the very store it reads (a torn read racing a concurrent writer, a momentarily locked
/// file) rather than itself becoming the thing that silently stops monitoring. Matches every
/// other fallible read [`watch_poll`] already performs beyond the store (`config::load`, the
/// step-lock probe, the liveness-marker stat, the dash-marker read) - all deliberately
/// fail-soft; only the store reads used to be the exception.
fn cmd_watch(args: &[String]) -> Res {
    let WatchArgs {
        once,
        interval_secs,
    } = parse_watch_args(args)?;

    let mut dedup = watch::Dedup::new();
    loop {
        let anomalies =
            match require_store_dir().and_then(|(loc, selection)| watch_poll(&loc, &selection)) {
                Ok(anomalies) => anomalies,
                Err(e) if once => return Err(e),
                Err(e) => {
                    eprintln!("rigger: watch: poll failed, will retry: {e}");
                    std::thread::sleep(std::time::Duration::from_secs(interval_secs.max(1)));
                    continue;
                }
            };
        let to_print = if once {
            anomalies
        } else {
            dedup.step(anomalies)
        };
        for a in &to_print {
            println!("{}", a.line());
        }
        if once {
            return Ok(());
        }
        std::thread::sleep(std::time::Duration::from_secs(interval_secs.max(1)));
    }
}

/// One poll's worth of I/O for [`cmd_watch`]: reads the run's event slice (scoped
/// exactly as `rigger status` scopes it) AND the whole log across every stream (the
/// scope store-integrity needs, mirroring `rigger validate`'s own order-signature
/// detector, spec 71), tries the step lock non-blocking (free = no `rigger step` is
/// running right now), gathers each currently-parked spawn's heartbeat-marker age
/// exactly as `cmd_status` does, and probes the dash's liveness with a real serve check
/// (the recorded marker's port when one exists, else the recorded `dash.url`'s own port;
/// see the dash-probe comment inline below for why both breadcrumbs matter), then hands
/// all of it to [`watch::detect`], the pure core. Never talks to the driver: every input
/// here is store, process-table, or status truth.
///
/// `loc`/`selection` are INJECTED rather than read ambiently in here (mirrors
/// [`refuse_derived_reset_if_live`]'s same shape): the composition root
/// (`cmd_watch`) resolves them via [`require_store_dir`], so this function - and the
/// test that seeds a [`StoreLocation`] pointing at a tempdir - never depends on the
/// process's actual cwd.
fn watch_poll(
    loc: &StoreLocation,
    selection: &StoreSelection,
) -> Result<Vec<watch::Anomaly>, Box<dyn std::error::Error>> {
    let now = std::time::SystemTime::now();

    let run_backend = resolve_store(selection, &loc.file("events.db"))?;
    let run_store = Namespaced::new(run_backend.as_ref(), &loc.identity());
    let all_in_project = run_store.read_stream(conductor::STREAM, 0, Direction::Forward)?;
    let run_events = runscope::current_run(&all_in_project).to_vec();
    let run_id = runscope::current_run_id(&all_in_project).unwrap_or_default();
    let last_event_at = run_events.last().map(|e| e.recorded_at);
    // When THIS run began - its own leading `RunStarted`'s `recorded_at` (`current_run`
    // always slices from that event onward), or `None` when no run has started yet in
    // this scope. Used ONLY to scope Signal 3 (dash liveness); see the dash-probe block
    // below for why.
    let run_started_at = run_events.first().map(|e| e.recorded_at);

    // Store integrity reads the WHOLE log across every stream (spec 71's own scope: a
    // disordered stream is a store-wide fault, not a per-run one), reusing the same
    // open connection rather than a second backend handle.
    let full_events = run_store.read_all(0, Direction::Forward, &Filter::default())?;

    // No step process running right now: a non-blocking try-lock that succeeds means
    // free. Dropped immediately either way, so this probe never holds the lock.
    let step_lock_free = acquire_step_lock(&loc.dir).is_ok();

    // Each currently-parked spawn's heartbeat-marker age, exactly as `cmd_status`
    // computes `liveness_ages` (spec 19a) - the SAME "live agent processes" reading
    // both surfaces show, so they can never disagree on who is still working.
    let (workdir, _max_retries) = config::load(".")
        .map(|c| (c.workflow.defaults.workdir, c.workflow.defaults.max_retries))
        .unwrap_or_default();
    let repo = git_repo();
    let mut wave_liveness_ages: std::collections::BTreeMap<String, u64> =
        std::collections::BTreeMap::new();
    if !repo.is_empty() {
        let root = rigger::worktree::scratch_root_from_env(&repo, &workdir);
        for w in &spawn::step_result(&run_events)?.wave {
            let path = rigger::liveness::marker_path(&root, &run_id, &w.id);
            if let Ok(age) = std::fs::metadata(&path)
                .and_then(|md| md.modified())
                .map(|mtime| now.duration_since(mtime).map(|d| d.as_secs()).unwrap_or(0))
            {
                wave_liveness_ages.insert(w.id.clone(), age);
            }
        }
    }

    // Dash liveness: prefer the per-project MARKER (port + pid) when one exists,
    // verified with the same real serve probe `dash_serving_on` uses - a marker naming
    // a dead or hung-holder pid never reads as serving, exactly like c4's status truth.
    //
    // Only ONE of the three real dash-launching drivers (`rigger step`, via
    // `ensure_run_dashboard`) ever writes a marker; `rigger run` and `rigger serve`
    // (`spawn_run_dashboard`/`spawn_run_dashboard_detached`) record ONLY the
    // `dash.url` breadcrumb. Without a fallback, a marker-absent project always read
    // as `NotRecorded` regardless of whether a dash was ever actually up - silently
    // blind for 2 of the 3 real drivers (round-3 reject cause
    // adv-u69c1r3-watch-once-inherits-marker-absent-blindspot). So when no marker
    // exists, probe the PORT EMBEDDED IN THE RECORDED URL directly instead - the same
    // safe, timeout-bounded `dash_serving_on` probe, just without a pid to name. Only
    // when NEITHER breadcrumb is recorded at all (`dash: off` / `RIGGER_NO_DASH`, or
    // watched before any run began) does the DashProbe VALUE constructed here read as
    // "never started". A `NotServing` value built here does not by itself guarantee an
    // anomaly, though: both breadcrumb files are project-level singletons never removed
    // once their dash exits, so `watch::detect` additionally gates this signal on the
    // run being unfinished (`!run.done()`) AND, since round-6 (round-5 reject cause
    // adv2-u69c1-r5-uphold-sdet-second-run-stale-marker), on the breadcrumb file NOT
    // being DEFINITIVELY older than `run_started_at` above - a done run's stale
    // breadcrumb is success, and a FRESH run that never touched the dash must not
    // inherit an EARLIER run's dead one either; both are suppressed in `detect`, not
    // here. The burden of proof runs toward reporting: unknown mtime or run-start info
    // (e.g. a dash breadcrumb with no run ever recorded in this project yet) still
    // reports, exactly as before this fix - only a PROVEN-older breadcrumb suppresses.
    // `dash_breadcrumb_written_at` is the mtime of WHICHEVER file actually backed the
    // classification below (the marker when read, else the url file), gathered
    // alongside it so the two can never point at different files.
    let marker_path = std::path::PathBuf::from(loc.file(DASH_MARKER_FILE));
    let url_path = std::path::PathBuf::from(loc.file(DASH_URL_FILE));
    let mtime_of = |p: &Path| std::fs::metadata(p).ok()?.modified().ok();
    let marker = dash::DashMarker::read(&marker_path);
    let recorded = recorded_dash_url(loc);
    let (dash, dash_breadcrumb_written_at) = match (recorded, marker) {
        // A recorded URL with a parseable port is the canonical authority, exactly as
        // `dash::dash_status` (src/dash.rs) decides it for the mismatched-marker case
        // sibling criterion u69c4 hardened: the probe targets the URL'S OWN port, and a
        // marker's pid is named ONLY when its port matches the url's - a mismatched
        // marker's pid belongs to some other dash and is never printed as this url's.
        // Deliberately NOT a bare call to `dash_status`: that surface PRESENTS (an
        // absent marker leaves the url "unverifiable but trusted" - never falsely dead),
        // while this probe DETECTS (it always probes, marker or not - the
        // url-only-dead-dash contract pinned in tests/cli.rs). The mismatch RULE is
        // shared; the trust-without-probing rule is dash_status's alone.
        (Some(url), Some(m)) => match dash::url_port(&url) {
            Some(url_port) => {
                // Round-9 escalation-remedy reject (adv-u69c1-mismatched-marker-suppression-
                // borrows-wrong-files-mtime): the probe always targets the URL's OWN port
                // (below), but ONLY when the marker's port matches it did the marker actually
                // back that classification (its pid is named); on a mismatch the marker played
                // no part - the url alone decided - so the mtime gathered here must follow
                // suit, exactly as the doc comment above this whole match requires ("of
                // WHICHEVER file actually backed the classification"). Sourcing the marker's
                // mtime unconditionally let a stale, mismatched marker that predates this run's
                // own RunStarted wrongly suppress a fresh, currently-dead url written after it.
                // `pid`/`port_matches` come from the SAME shared rule `dash_status` uses
                // (`dash::pid_if_port_matches`, round 11 architecture/adversary review) rather
                // than a second hand-rolled copy - `pid.is_some()` iff the marker's port matched.
                let pid = dash::pid_if_port_matches(&m, url_port);
                let port_matches = pid.is_some();
                let written_at = if port_matches {
                    mtime_of(&marker_path)
                } else {
                    mtime_of(&url_path)
                };
                if dash::dash_serving_on(url_port) {
                    (watch::DashProbe::Serving, written_at)
                } else {
                    (
                        watch::DashProbe::NotServing {
                            pid,
                            port: url_port,
                        },
                        written_at,
                    )
                }
            }
            // An unparseable recorded URL (foreign or malformed - the same ambiguous
            // input `dash_status` treats as unverifiable): the marker is the only
            // checkable breadcrumb left, so probe it as the marker-only arm does.
            None => {
                if dash::dash_serving_on(m.port) {
                    (watch::DashProbe::Serving, mtime_of(&marker_path))
                } else {
                    (
                        watch::DashProbe::NotServing {
                            pid: Some(m.pid),
                            port: m.port,
                        },
                        mtime_of(&marker_path),
                    )
                }
            }
        },
        // URL recorded, no marker at all: probe the url's own port (detection, not
        // presentation - a dead url-only dash must still be reported; pinned by the
        // url-breadcrumb-only test in tests/cli.rs). No marker, no pid to name.
        (Some(url), None) => match dash::url_port(&url) {
            Some(port) if dash::dash_serving_on(port) => {
                (watch::DashProbe::Serving, mtime_of(&url_path))
            }
            Some(port) => (
                watch::DashProbe::NotServing { pid: None, port },
                mtime_of(&url_path),
            ),
            None => (watch::DashProbe::NotRecorded, None),
        },
        // No URL recorded: a marker alone stays this probe's own authority. `dash_status`
        // deliberately reads url-first and would call this Absent, but a marker-only dash
        // is real (the step path writes a marker; the dead-marker contract in
        // `rigger-restore-the-dash` pins that `rigger watch --once` reports it), so
        // suppressing it here would hide a genuinely dead dash.
        (None, Some(m)) => {
            if dash::dash_serving_on(m.port) {
                (watch::DashProbe::Serving, mtime_of(&marker_path))
            } else {
                (
                    watch::DashProbe::NotServing {
                        pid: Some(m.pid),
                        port: m.port,
                    },
                    mtime_of(&marker_path),
                )
            }
        }
        (None, None) => (watch::DashProbe::NotRecorded, None),
    };

    // Round-8 fix (spec 69, adv-u69c1r7-mint-order-bug-is-structural-not-a-coverage-gap): whether
    // THIS project's CURRENTLY WATCHED run's own step path attempted a dash ensure THIS run - an
    // explicit run-id match against `DASH_ATTEMPT_FILE` ([`record_dash_attempt`]'s write site),
    // not an inference from timestamps. A match means Signal 3 must never suppress: this run's
    // own step path just vouched for the dash. An absent file or one naming a different run (every
    // existing seeded-event test included, since none of them drive the real dash-ensure call)
    // means this signal alone is silent and `detect` falls back to the pre-existing
    // `dash_breadcrumb_written_at`/`run_started_at` comparison unchanged.
    let dash_attempted_this_run = std::fs::read_to_string(loc.file(DASH_ATTEMPT_FILE))
        .ok()
        .map(|s| s.trim().to_string())
        .is_some_and(|attempted_run| !attempted_run.is_empty() && attempted_run == run_id);

    let inputs = watch::WatchInputs {
        run_events: &run_events,
        full_events: &full_events,
        now,
        last_event_at,
        step_lock_free,
        wave_liveness_ages: &wave_liveness_ages,
        dash,
        run_started_at,
        dash_breadcrumb_written_at,
        dash_attempted_this_run,
    };
    Ok(watch::detect(&inputs))
}

/// `rigger reset` - the supported prunes, one flag per accumulation.
///
/// Each mode sheds a DIFFERENT pile, so they name themselves explicitly and compose in one
/// invocation.
///
///   - `--runs` (spec 21, unit 2) prunes the CONTEXT GRAPH: see [`reset_runs`].
///   - `--derived` (spec 60, criterion 5) compacts the EVENT LOG: see [`reset_derived`].
///
/// A BARE `rigger reset` (no flags at all) is a MENU, not an error (spec 68, "the reset
/// surface"): see [`reset_menu`]. Only a TRULY empty `args` takes that path - any non-empty
/// args that select no mode (an unknown flag, or `--force-live` alone) fall through to
/// [`reset_modes`]'s existing refusal exactly as before this menu existed, so that refusal and
/// its tests are untouched.
///
/// PRECHECKS FIRST, and exactly what they promise. The flags are parsed and the backend
/// requirement of every requested mode is settled BEFORE the first prune runs, so a composed
/// invocation never starts work it is already known to be unable to finish - the shape that used
/// to leave the graph pruned and the log untouched because the log's backend was refused second.
/// Each mode's own mutation is atomic (each is one transaction over one file), and the modes run
/// in order: if a prune fails on a genuine IO or lock fault after an earlier one committed, the
/// earlier prune HAS happened and is reported on stdout above the error. That is the honest
/// statement of the composition, and it is deliberately not called all-or-nothing: two files
/// cannot be committed together, and claiming otherwise would tell an operator not to look.
fn cmd_reset(args: &[String]) -> Res {
    if args.is_empty() {
        let (loc, selection) = require_store_dir()?;
        if selection.is_sqlite() {
            migrate_identity_at(&loc)?;
        }
        return reset_menu(&loc, &selection);
    }

    let modes = reset_modes(args)?;

    let (loc, selection) = require_store_dir()?;
    // Before ANY prune reads a stream name: run the one-time spec-09 identity migration, exactly
    // as `run` / `step` / `workflow` / `playbooks` do before they open their store. Both prunes
    // address this project's history BY ITS CURRENT IDENTITY, and a store bloated enough to need
    // compacting is by construction an OLD store whose history was written under the pre-identity
    // basename namespace. Without this, `reset` would match no stream at all on exactly the log it
    // exists for and report a perfectly successful prune of zero rows - the silent no-op this
    // command's whole design refuses. Anchored at the RESOLVED store root, not the process cwd, so
    // a reset run from a nested worktree migrates the store it is about to prune.
    if selection.is_sqlite() {
        migrate_identity_at(&loc)?;
    }
    // Decided up front, before anything is pruned: deleting rows and reclaiming the file are
    // mechanics of the embedded log, not port operations, so `--derived` names the backend it
    // needs rather than quietly doing nothing on one that cannot compact.
    if modes.derived && !selection.is_sqlite() {
        return Err(format!(
            "reset --derived: the derived-index compaction deletes rows from the event log and \
             vacuums the file, which is a mechanic of the embedded {RIGGER_DIR}/events.db store; \
             this project is configured for the server-backed store, which rigger cannot compact. \
             Re-run it against a project on the sqlite backend, or prune the server store with \
             its own retention tooling. Refusing rather than reporting a prune that did not happen."
        )
        .into());
    }
    if modes.runs {
        reset_runs(&loc, &selection)?;
    }
    if modes.derived {
        // COMPACTION REFUSES LIVE WRITERS (spec 71, criterion 2): `--derived` leaves revision
        // gaps by design, and a writer built before this compaction ran can reissue one of those
        // gaps and reorder the log (the incident spec 71 records) if the log changes under it.
        // `--force-live` is the explicit, named escape hatch that skips this check entirely (it
        // verifies nothing - the operator owns that risk once they pass it). The registry read
        // is resolved HERE, at the composition root, and handed in - the guard itself never
        // reads the ambient environment (see `refuse_derived_reset_if_live`'s docs).
        //
        // Deliberately checked AFTER `--runs` (not alongside the STATIC backend-mismatch
        // precheck above): the live-writer guard reads a DYNAMIC snapshot that is scoped
        // entirely to the event log, so a composed `reset --runs --derived` under a live signal
        // still completes `--runs`'s OWN, independent, already-safe prune of the graph
        // (`graph.db` - a different file, never at risk from a live event-log writer); only
        // `--derived` itself refuses. This is the same "each mode sheds only its own
        // accumulation, and an earlier prune's completion is reported before a later refusal"
        // honesty this function's own docs already commit to for a genuine IO/lock fault -
        // extended here to a live-writer refusal, not just an unexpected error.
        if !modes.force_live {
            refuse_derived_reset_if_live(
                &loc,
                &selection,
                rigger::registry::default_dir().as_deref(),
            )?;
        }
        reset_derived(&loc)?;
    }
    Ok(())
}

/// Bare `rigger reset` (spec 68, "the reset surface"): a MENU, not an error. Prints one line per
/// prunable accumulation `--runs` / `--derived` would act on, each with a MEASURED count and the
/// flag that acts on it, then exits 0. Read-only by construction - every number here comes from a
/// `SELECT`, never from running a prune, so invoking the bare command is always safe to do "just
/// to look".
///
/// WHY A COUNT, NOT A DISK-BYTE FORECAST. The flagged reports name bytes RECLAIMED
/// (`derived_prune_report`, `reset_runs`'s own line) because they measure a real before/after
/// across the mutation that just ran - `PrunedDerived::reclaimed_bytes`'s own docs are explicit
/// that this is "MEASURED, NOT DERIVED" over the actual rewrite, and `Projector::compact`'s docs
/// say the same of `VACUUM`: a page-count delta is only meaningful once the rewrite has happened.
/// There is no honest byte figure to preview BEFORE that rewrite runs - printing one here would
/// be exactly the fabricated number this whole command's design otherwise refuses to print. A
/// COUNT of what would be removed is the real, read-only measurement the preview CAN make
/// ([`contextgraph::sqlite::Projector::count_prunable`] /
/// [`eventstore::sqlite::Store::count_derived_duplicates`], each the read-only twin of the
/// predicate its flagged prune deletes by), so that is what this menu reports.
fn reset_menu(loc: &StoreLocation, selection: &StoreSelection) -> Res {
    // --runs: works over ANY backend, exactly like a real `--runs` does (the context graph is
    // always a local file; only the EVENT log may be server-backed) - so this reads the whole run
    // stream through the resolved backend precisely as `reset_runs` does.
    let backend = resolve_store(selection, &loc.file("events.db"))?;
    let store = Namespaced::new(backend.as_ref(), &loc.identity());
    let events = store.read_stream(conductor::STREAM, 0, Direction::Forward)?;
    let drop = superseded_graph_nodes(&events);
    let boundary = superseded_edge_boundary(&events);
    let graph = Projector::open(&loc.file("graph.db"), &loc.identity())?;
    let stats = graph.count_prunable(&drop, boundary)?;
    println!("{}", runs_menu_line(&stats));

    // --derived: a mechanic of the embedded sqlite store (see `reset_derived`'s own doc) - honest
    // per backend rather than a number a server-backed project could never actually reclaim.
    if selection.is_sqlite() {
        let es = open_sqlite_store(&loc.file("events.db"))?;
        let duplicates = es.count_derived_duplicates(
            &Namespaced::prefix_for(&loc.identity()),
            &rigger::ingest::derived_index_identity(),
        )?;
        println!("{}", derived_menu_line(selection, Some(&duplicates)));
    } else {
        println!("{}", derived_menu_line(selection, None));
    }
    Ok(())
}

/// The `--runs` line of [`reset_menu`], pure over the already-measured [`PruneStats`] so the
/// wording is unit-testable without a store.
fn runs_menu_line(stats: &PruneStats) -> String {
    format!(
        "--runs: {} dead-run node(s) and {} superseded edge(s) prunable from the context graph; \
         rerun `rigger reset --runs` to reclaim them",
        stats.nodes, stats.superseded_edges
    )
}

/// The `--derived` line of [`reset_menu`], pure over the already-measured per-type duplicate
/// counts (or their absence, on a backend that cannot compact) so both branches are
/// unit-testable without a store or a live server: `duplicates` is `Some` on the sqlite backend
/// (`selection.is_sqlite()`) and `None` on any other, and this reads `selection` only to name the
/// backend it is honest about.
fn derived_menu_line(selection: &StoreSelection, duplicates: Option<&[(String, usize)]>) -> String {
    match duplicates {
        Some(counts) => {
            let total: usize = counts.iter().map(|(_, n)| n).sum();
            format!(
                "--derived: {total} duplicate event(s) prunable from the event log across {} \
                 derived type(s); rerun `rigger reset --derived` to compact them",
                counts.len()
            )
        }
        None => {
            debug_assert!(
                !selection.is_sqlite(),
                "derived_menu_line: a `None` count on the sqlite backend would hide a real \
                 measurement the caller could have taken"
            );
            "--derived: unavailable on this backend - compaction deletes rows from the event log \
             and vacuums the file, a mechanic of the embedded sqlite events.db store; this \
             project is configured for the server-backed store, which rigger cannot compact"
                .to_string()
        }
    }
}

/// Which prunes one `rigger reset` invocation was asked for.
struct ResetModes {
    runs: bool,
    derived: bool,
    /// The override for `--derived`'s live-writer guard (spec 71, criterion 2): skips
    /// [`refuse_derived_reset_if_live`] entirely rather than acting on what it would have found -
    /// the operator asked to compact WHATEVER the run machinery looks like, and this flag owns
    /// that risk (see its help text and [`live_writer_refusal`]). Meaningless on its own; only
    /// `--derived` ever reads it. Not itself a mode - a bare `--force-live` with no
    /// `--runs`/`--derived` still falls through the "at least one mode" refusal below exactly as
    /// before this flag existed.
    force_live: bool,
}

/// Parse `rigger reset`'s flags: any combination of the named modes, in any order, each at most
/// once, and at least one of them; `--force-live` composes with either and is at most once too.
///
/// Every mode is explicit and an unrecognized argument is REFUSED rather than ignored, because
/// both failure modes here are silent: a bare `reset` that guessed a mode would prune something
/// the operator did not ask for, and a tolerated typo would report success for work it never did.
fn reset_modes(args: &[String]) -> Result<ResetModes, Box<dyn std::error::Error>> {
    let mut modes = ResetModes {
        runs: false,
        derived: false,
        force_live: false,
    };
    for arg in args {
        let slot = match arg.as_str() {
            "--runs" => &mut modes.runs,
            "--derived" => &mut modes.derived,
            "--force-live" => &mut modes.force_live,
            other => {
                return Err(format!(
                    "reset: expected --runs and/or --derived (with an optional --force-live), \
                     got {other}: rigger reset --runs | rigger reset --derived [--force-live]"
                )
                .into())
            }
        };
        if *slot {
            return Err(format!("reset: {arg} was given more than once").into());
        }
        *slot = true;
    }
    if !modes.runs && !modes.derived {
        return Err(
            "reset: expected at least one mode: rigger reset --runs (prune the context \
                    graph) and/or rigger reset --derived (compact the event log)"
                .into(),
        );
    }
    Ok(modes)
}

/// `rigger reset --derived` (spec 60, criterion 5) - SUPPORTED COMPACTION of an event log that
/// accumulated derived-index duplication before the project-scoped ingest dedup existed.
///
/// For each of the four derived index types it keeps the LATEST event per distinct replay key,
/// deletes every earlier recording of that key, and vacuums so the file shrinks on disk. Every
/// non-derived event survives byte-for-byte; the graph projection stays consistent, because it is
/// an upsert projection in which all recordings of a key fold to the same rows.
///
/// It is orchestration over ONE store-mutation primitive
/// ([`rigger::eventstore::sqlite::Store::prune_derived_index`]), handed the ONE derived-index
/// content-identity policy [`rigger::ingest::derived_index_identity`] owns (the replay-key
/// metadata name, the four derived types, where a key's content generation lies, and which of
/// those types re-assert a fact in place rather than superseding it), and the
/// SAME stream-prefix spelling every namespaced read and write of this project uses
/// ([`Namespaced::prefix_for`]) - so a change to the namespace's wire form can never leave the
/// compaction addressing streams that no longer exist. That prefix is a string match, with the
/// property a string match has: a project whose id is a prefix of another's shares its slice,
/// exactly as `read_all`, `subscribe_all` and the identity migration already do. The boundary is
/// inherited, not tightened here. The two are the same PREFIX, not the same predicate: those
/// reads match it with SQL `LIKE` and no `ESCAPE`, so an `_` or `%` in a project id is a wildcard
/// there, while the prune matches literally and so reaches a SUBSET of the streams the project's
/// own reads reach - the safe direction for a command that deletes.
///
/// The sqlite store is constructed through [`open_sqlite_store`], the one sqlite event-log
/// constructor (§48), exactly as the local identity migration does when it needs the concrete
/// store for a maintenance operation the port does not carry.
fn reset_derived(loc: &StoreLocation) -> Res {
    let store = open_sqlite_store(&loc.file("events.db"))?;
    let pruned = store.prune_derived_index(
        &Namespaced::prefix_for(&loc.identity()),
        &rigger::ingest::derived_index_identity(),
    )?;
    println!("{}", derived_prune_report(&pruned));
    Ok(())
}

/// The one line `rigger reset --derived` prints, rendered from what the prune actually did.
///
/// A pure function of the report, and separate from the command, because FOUR of its five
/// compaction states are unreachable from a happy-path run of the binary - a reclamation the
/// truncating checkpoint declined, a rewrite that failed after the deletes committed, a rewrite
/// that was deliberately not run, and a database with no file behind it (which `rigger reset
/// --derived` never opens at all, though the store this renders is a published entry point that
/// does) - and each of them is a state whose whole purpose is to be READ correctly by an
/// operator. A report only the lucky path renders is a report nothing pins.
fn derived_prune_report(pruned: &PrunedDerived) -> String {
    let per_type = pruned
        .removed
        .iter()
        .map(|(t, n)| format!("{t} {n}"))
        .collect::<Vec<_>>()
        .join(", ");
    // WHAT HAPPENED TO THE FILE, in the four states the prune can leave it. The deletes have
    // committed before any of this is decided, so none of them is a failure of the prune:
    //   - the rewrite failed: the rows are gone anyway, so the operator gets the counts, the
    //     failure by name, and the two facts that follow from the ordering (the deletes are
    //     durable; a re-run is safe, and it retries the reclamation because what triggers the
    //     rewrite is the free space still in the file). Anything less is an "error" about a log
    //     that WAS pruned.
    //   - the file had no free space to reclaim: it is deliberately not rewritten, because a full
    //     rewrite there holds the write lock for a whole scan and stages a second copy of the log
    //     in the temporary directory to reclaim nothing. Zero bytes is the measurement, not a
    //     missing one. Read from `compaction_ran`, never inferred from a zero count: a pass that
    //     deleted nothing still rewrites a file that HAS space to reclaim, and telling an
    //     operator their log was left alone while it was being rewritten is the misreport this
    //     whole line exists to avoid.
    //   - the reclamation was measured: report the bytes the log lost on disk.
    //   - the truncating checkpoint was declined by a concurrent reader: the freed pages stay in
    //     the write-ahead log, so it is reported as unmeasured rather than as a byte count the
    //     operator's own `ls` would contradict.
    //   - the database has no file behind it at all: there was never a before-measurement to
    //     take, so the same "rewritten, no number" shape arrives from a different cause and says
    //     so. Read from `on_disk_measured`, never folded into the declined-checkpoint arm: those
    //     two are the ONLY producers of that shape, and naming a concurrent reader for the second
    //     asserts a cause this function was not handed - it would send a reader looking for a
    //     writer that does not exist and promise pages at a checkpoint that will never put a byte
    //     on a disk this database does not use.
    let compaction = match (
        &pruned.compaction_error,
        pruned.compaction_ran,
        pruned.reclaimed_bytes,
        pruned.on_disk_measured,
    ) {
        (Some(err), _, _, _) => format!(
            "the log file could NOT be compacted afterwards: {err}. The deletes are committed and \
             durable, so nothing was lost and re-running the command is safe - and it retries the \
             reclamation, because the space this run could not reclaim is still free in the file"
        ),
        (None, false, _, _) => "the log file was holding no reclaimable free page, so it was left \
                                exactly as it stands rather than rewritten to reclaim nothing"
            .to_string(),
        (None, true, Some(bytes), _) => {
            format!("then compacted the log file and reclaimed {bytes} byte(s) on disk")
        }
        (None, true, None, true) => "then compacted the log file, but the freed pages could not \
                                     be folded back into the file: a concurrent reader held the \
                                     write-ahead log, so they land at the next checkpoint and \
                                     this run reclaimed an unmeasured amount"
            .to_string(),
        (None, true, None, false) => "then compacted the log, which has no file behind it (an \
                                      in-memory or temporary database): there are no bytes on \
                                      disk to have been reclaimed, so the reclamation is \
                                      unmeasured rather than zero"
            .to_string(),
    };
    // WHAT THE COUNT MEANS, both ways round, because each direction is misread in its own way.
    //
    // ZERO is the expected report on a log whose derived index holds one recording per distinct
    // key, and an operator who reads "pruned 0" as a failure goes looking for a defect that is not
    // there. It is justified by WHAT THIS LOG HOLDS and never by WHEN it was written: a log
    // written since the ingest dedup existed still re-records a file's whole batch whenever that
    // file's content returns to a generation the log already recorded, so "written after the
    // dedup" implies nothing about the count.
    //
    // NON-ZERO on such a log is therefore NOT evidence the dedup is broken - it is that
    // by-design duplication being shed - and saying so is the same sentence's other half: an
    // operator who has just been told zero is normal will otherwise read a non-zero prune as the
    // dedup having failed.
    let what_the_count_means = if pruned.total_removed() == 0 {
        " - a log whose derived index already holds one recording per distinct key has no \
         redundancy to shed, so this is the expected report on such a log, not a failed prune"
    } else {
        " - a non-zero count is not a sign the ingest dedup is broken: a file whose content \
         RETURNS to a generation the log already recorded (a revert, a branch switch, a checkout \
         back) re-records its whole batch by design, because a dedup that suppressed it would \
         strand the graph on the version the file has since moved past, and this is that \
         duplication being shed"
    };
    format!(
        "reset --derived: pruned {} redundant derived-index event(s) from the event log \
         ({per_type}), {compaction} - every non-derived event and the latest recording of every \
         content key are preserved{what_the_count_means}",
        pruned.total_removed(),
    )
}

/// The reasons `rigger reset --derived` must refuse (spec 71, criterion 2), from four
/// already-gathered facts - the recorded incident this guard exists to prevent is a compaction
/// that ran WHILE a writer was still appending, so each fact covers a different shape that writer
/// can take and none alone covers every shape:
///   - `step_lock_held`: a `rigger step` is running right now, possibly mid-wave before it has
///     even parked a spawn (the narrowest, most immediate signal - see [`acquire_step_lock`]).
///   - `live_units`: the CURRENT run's non-terminal unit branches ([`current_run_units`], the
///     SAME authority `cmd_step`'s orphan-sweep and `validate`'s residue scan already fold on) -
///     catches a unit that is live BETWEEN spawn rounds (its last spawn answered, its next not
///     parked yet), which an in-flight-spawn check alone would miss.
///   - `in_flight_spawn_ids`: a recorded spawn request in the current run with no result yet
///     ([`spawn::step_result`]'s wave) - catches a pre-unit spawn (a plan/canary round the ledger
///     has not folded into a unit yet) that `live_units` alone would miss, AND a worker (an agent
///     process running its own `rigger emit`/`rigger result` couriers) that may be appending even
///     with no `step`/`run`/`serve` process alive right now.
///   - `driver_registrations`: a live entry in the machine-global instance registry (spec 50) for
///     THIS project's exact store - an in-process `rigger run`/`serve` (which never touches
///     `step.lock`, and whose next spawn may not be parked yet either) elsewhere on this machine.
///
/// Pure (no IO) so the composition - list EVERY applicable reason, never just the first, so an
/// operator sees the whole picture in one refusal instead of clearing one and retrying into the
/// next - is unit-tested without any of the four. Empty means quiet: safe to compact.
fn live_writer_reasons(
    step_lock_held: bool,
    live_units: &std::collections::HashSet<String>,
    in_flight_spawn_ids: &[String],
    driver_registrations: usize,
) -> Vec<String> {
    let mut reasons = Vec::new();
    if step_lock_held {
        reasons
            .push("a `rigger step` is running right now (it holds .rigger/step.lock)".to_string());
    }
    if !live_units.is_empty() {
        let mut slugs: Vec<&str> = live_units
            .iter()
            .map(|b| b.strip_prefix("rigger/u/").unwrap_or(b.as_str()))
            .collect();
        slugs.sort_unstable();
        reasons.push(format!(
            "{} unit(s) in the current run are not yet terminal: {}",
            slugs.len(),
            slugs.join(", "),
        ));
    }
    if !in_flight_spawn_ids.is_empty() {
        reasons.push(format!(
            "{} spawn(s) in the current run have no recorded result yet: {}",
            in_flight_spawn_ids.len(),
            in_flight_spawn_ids.join(", "),
        ));
    }
    if driver_registrations > 0 {
        reasons.push(format!(
            "{driver_registrations} driver registration(s) for this project's store are still \
             live in the machine-global instance registry (spec 50) - a `run`/`serve`/`step` may \
             be advancing this run elsewhere on this machine"
        ));
    }
    reasons
}

/// The loud refusal `rigger reset --derived` prints for a non-empty [`live_writer_reasons`]:
/// names every reason, the concrete risk, and the override. `--force-live` is named here and in
/// its own help text (spec 71: "an explicit override flag whose help text owns the risk") so an
/// operator reads the same risk-owning sentence wherever they meet the flag.
fn live_writer_refusal(reasons: &[String]) -> String {
    format!(
        "reset --derived: refusing to compact the event log while run machinery looks live - {}. \
         Compaction keeps only the latest event per replay key, which leaves REVISION GAPS by \
         design; a writer whose append cursor was built before this compaction ran can reissue \
         one of those gap revisions, and every later event then sorts BELOW it in revision order \
         - the incident this guard exists to prevent, and the corruption forcing past a genuinely \
         live writer would risk. Stop the run machinery named above and retry, or pass \
         --force-live to compact anyway if you are certain no writer is using this store \
         (--force-live checks nothing; it trusts you with that risk).",
        reasons.join("; "),
    )
}

/// Gather [`live_writer_reasons`]'s four facts and refuse `rigger reset --derived` (spec 71,
/// criterion 2) when any applies. IMPURE (a lock probe, a store read, an optional registry read)
/// so the decision composition itself stays pure and unit-tested without any of the four.
///
/// `registry_dir` is INJECTED (mirrors [`dash_resolve_attach`]'s existing DI shape) rather than
/// read ambiently in here: the composition root (`cmd_reset`) resolves it once via
/// [`rigger::registry::default_dir`], exactly as every other ambient-environment read in this
/// crate is pushed to a caller rather than repeated inside a callee. `None` (a homeless
/// environment) degrades to zero registrations - the same degrade `register_run_instance` itself
/// takes for the identical reason: the registry's loss is harmless discovery metadata, never a
/// signal this guard can invent.
///
/// FAIL-SAFE in two different ways for two different faults:
///   - a run-stream read failure propagates as a command error rather than folding into "no
///     in-flight spawns" - this guard may only REFUSE a prune, never approve one it could not
///     actually verify was safe (mirrors [`terminal_and_no_live_worker`]'s convention on the
///     opposite rail: an unreadable stream is never read as "nobody is here").
///   - a step-lock probe error that is NOT the lock actually being held (e.g. a permission
///     fault) also propagates as a command error rather than being misread as "a step is
///     running": only [`STEP_BUSY_TOKEN`] in the probe's own error names a genuinely held lock,
///     so an operator troubleshooting an unrelated IO fault gets that fault's own message
///     instead of a misdiagnosis pointing them at a `rigger step` that is not actually running.
fn refuse_derived_reset_if_live(
    loc: &StoreLocation,
    selection: &StoreSelection,
    registry_dir: Option<&Path>,
) -> Res {
    // A non-blocking probe of the SAME advisory lock `rigger step` holds for its whole duration,
    // resolved at THIS STORE's own directory (never the process cwd) - `reset --derived` is run
    // from a nested worktree just as every other courier is (see `require_store_dir`), and a
    // cwd-relative probe would open a `.rigger/step.lock` under the WRONG (or nonexistent)
    // directory there. Acquiring (then immediately dropping) it proves nobody else holds it right
    // now. A failure whose message names the busy token proves a step IS running; any OTHER
    // failure (a permission fault, a read-only filesystem) is a real fault this command cannot
    // silently misdiagnose as "held", so it propagates instead.
    let step_lock_held = match acquire_step_lock(&loc.dir) {
        Ok(_) => false,
        Err(e) if e.to_string().contains(STEP_BUSY_TOKEN) => true,
        Err(e) => return Err(e),
    };

    let backend = resolve_store(selection, &loc.file("events.db"))?;
    let store = Namespaced::new(backend.as_ref(), &loc.identity());
    let events = store.read_stream(conductor::STREAM, 0, Direction::Forward)?;
    let live_units = current_run_units(&events).live_branches;
    let in_flight_spawn_ids: Vec<String> = spawn::step_result(runscope::current_run(&events))?
        .wave
        .into_iter()
        .map(|w| w.id)
        .collect();

    let driver_registrations = registry_dir
        .map(|dir| {
            let root = loc
                .dir
                .parent()
                .map(Path::to_path_buf)
                .unwrap_or_else(|| PathBuf::from("."));
            let expected = registry_store_identity(selection, &root);
            rigger::registry::read_live(
                dir,
                rigger::registry::now_ms(),
                rigger::registry::DEFAULT_IDLE_MS,
            )
            .into_iter()
            .filter(|inst| inst.store == expected)
            .count()
        })
        .unwrap_or(0);

    let reasons = live_writer_reasons(
        step_lock_held,
        &live_units,
        &in_flight_spawn_ids,
        driver_registrations,
    );
    if reasons.is_empty() {
        return Ok(());
    }
    Err(live_writer_refusal(&reasons).into())
}

/// `rigger reset --runs` (spec 21, unit 2) - drop the decisions and findings of every
/// SUPERSEDED / dead run from the context graph, PRESERVING every `LessonLearned` and the
/// active run's decisions and findings. It is the supported way to shed dead-run noise
/// without deleting the whole store: this prune DELETES NO EVENT, so `rigger stats`, replay,
/// and cross-run history stay intact - only the graph the grounder reads is pruned (there is
/// no way to shed the noise today short of wiping `graph.db` wholesale).
///
/// WHAT THE COMMAND AROUND IT DOES WRITE TO THE LOG, stated here because this function's own
/// report used to promise an untouched log and no longer can: [`cmd_reset`] runs the one-time
/// spec-09 identity migration before EITHER mode, so on a store still filed under the legacy
/// basename namespace that migration renames its streams to the minted identity and appends one
/// `DecisionMade` recording the rename. No event is dropped, reordered, or altered in content by
/// it, and it prints its own line when it fires - but "the event log is untouched" is not true of
/// a `rigger reset --runs` on the one class of store the migration exists for, so the printed
/// report says what IS true instead. The migration is deliberately not gated on `--derived`:
/// `reset_runs` reads through `Namespaced::new(backend, &loc.identity())`, so skipping it on an
/// unmigrated store would read an empty stream and report a confident prune of zero dead-run
/// nodes - the silent no-op the migration is there to prevent, moved from one mode to the other.
///
/// This is pure orchestration over two single authorities: the disposition comes from the
/// run-attribution primitive ([`superseded_graph_nodes`] over `run::run_attribution` +
/// `run::current_run_id`), and the deletion is the graph-mutation primitive
/// ([`Projector::prune`]). ONE whole-stream forward read feeds the attribution AND the
/// node-id lookup (the index-keying contract `run_attribution` documents - a filtered slice
/// would misattribute); the derived node ids are then handed to the prune.
fn reset_runs(loc: &StoreLocation, selection: &StoreSelection) -> Res {
    let backend = resolve_store(selection, &loc.file("events.db"))?;
    let store = Namespaced::new(backend.as_ref(), &loc.identity());
    // ONE whole-stream forward read: it feeds BOTH the attribution and the per-index node-id
    // lookup inside `superseded_graph_nodes`, honoring run_attribution's whole-stream contract.
    let events = store.read_stream(conductor::STREAM, 0, Direction::Forward)?;
    let drop = superseded_graph_nodes(&events);
    // Spec 41: the retention cutoff for the superseded-edge reclamation - the active run's start,
    // the SAME run boundary the node drop-set is derived against. `None` (a legacy store with no
    // run) reclaims no edge, so LIVE and recent history are both untouched.
    let boundary = superseded_edge_boundary(&events);

    let graph = Projector::open(&loc.file("graph.db"), &loc.identity())?;
    let removed = graph.prune(&drop, boundary)?;
    // Compact the projection file so the prune reclaims DISK, not just rows (spec 46, criterion 3):
    // the deletes free pages inside graph.db that SQLite retains on a freelist, so without a VACUUM
    // the file stays as LARGE on disk as before even though the dead rows are gone. VACUUM reclaims
    // disk ONLY - it changes no query result and gives no query or fold speedup; it rebuilds only
    // the rebuildable projection and the event log is untouched.
    let reclaimed_bytes = graph.compact()?;
    println!(
        "reset --runs: pruned {} dead-run node(s) and reclaimed {} superseded edge(s) from the \
         context graph, then compacted the graph file (reclaimed {} byte(s) on disk) - every \
         lesson, the active run, and every live edge are preserved; this prune deletes no event \
         from the log. The one thing `rigger reset` writes there is the one-time identity \
         migration it runs first: on a store still under the legacy basename namespace that \
         renames its streams and records one DecisionMade, and it prints its own line when it \
         does",
        removed.nodes, removed.superseded_edges, reclaimed_bytes
    );
    Ok(())
}

/// The retention cutoff `rigger reset --runs` reclaims superseded structural edges beneath
/// (spec 41): the nanosecond `valid_from` of the ACTIVE run's `RunStarted` - the SAME run boundary
/// [`superseded_graph_nodes`] keeps the active run's decision/finding nodes by. A superseded edge
/// (`valid_to IS NOT NULL`) retired BEFORE this cutoff belongs to a prior run and is dead cruft the
/// log can re-derive; one retired at or after it is recent history kept inside the window. `None`
/// when no run has started (a legacy store) - then nothing is reclaimed, so LIVE and recent history
/// are both untouched.
///
/// Pure over the whole forward stream, reusing the SINGLE run-boundary authority
/// ([`runscope::current_run`]) - never a second inline boundary scan. The cutoff is the graph's own
/// `to_nanos` time base (nanoseconds since the Unix epoch), so it compares directly to an edge's
/// stored `valid_to`.
fn superseded_edge_boundary(events: &[Event]) -> Option<i64> {
    runscope::current_run(events)
        .first()
        .filter(|e| e.type_ == runscope::TYPE_RUN_STARTED)
        .map(|e| {
            e.valid_from
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos() as i64)
                .unwrap_or(0)
        })
}

/// The decision and finding graph-node ids `rigger reset --runs` drops (spec 21, unit 2):
/// every provenance node that is NEITHER the active run's NOR a lesson - a superseded run's
/// decision/finding, or a pre-boundary one recorded before the first `RunStarted`. Pure over
/// the whole run stream, reusing the SINGLE run-attribution authority
/// (`run::run_attribution` + `run::current_run_id`) - never a second inline boundary scan.
///
/// `events` MUST be the whole [`conductor::STREAM`] in forward order, exactly as
/// `run_attribution` and `current_run_id` require: the attribution keys by an event's INDEX
/// in this slice, so each node id is read back from `events[i]`'s own body (the `id` field
/// the projector folds the node under) - one whole-stream read feeds both the attribution and
/// the id lookup, never two different slices.
///
/// The keep invariant is enforced by SUBTRACTION, not by skipping live indices: a decision or
/// finding id can be recorded in BOTH a dead run AND the active run (id reuse across runs), so
/// the same graph node has one index attributed dead and another attributed live. We collect
/// the active run's node ids into a keep set and return `drop_candidates` MINUS that keep set,
/// so a reused id is PRESERVED (closes the active-node-pruned-on-cross-run-id-reuse hazard) -
/// dropping a candidate index alone would delete the shared node the active run still needs.
/// Returns a sorted, de-duplicated list (determinism is a spec-21 constraint), leaving every
/// `LessonLearned` (exempt) and every active-run node out of the drop set.
fn superseded_graph_nodes(events: &[Event]) -> Vec<String> {
    use std::collections::BTreeSet;
    let attribution = runscope::run_attribution(events);
    let active = runscope::current_run_id(events);
    let mut drop_candidates: BTreeSet<String> = BTreeSet::new();
    let mut keep: BTreeSet<String> = BTreeSet::new();
    for (&i, run_of) in &attribution {
        // A lesson is exempt (kept by its own rule, never "live" and never dropped).
        if matches!(run_of, runscope::RunOf::Lesson) {
            continue;
        }
        // Skip a malformed / empty-id body exactly as the projector's own fold skips it, so a
        // corrupt event never contributes a bogus id to either set.
        let Some(id) = graph_node_id(&events[i]) else {
            continue;
        };
        // The active run's node ids are live and must be KEPT; everything else - a superseded
        // run's node, or a pre-boundary one - is a drop CANDIDATE. A single id can land in both
        // sets when it is reused across a dead run and the active run.
        if run_of.is_live(active.as_deref()) {
            keep.insert(id);
        } else {
            drop_candidates.insert(id);
        }
    }
    // Subtract the active run's kept ids: a node id present in BOTH a dead run and the active
    // run must be PRESERVED (dropping its dead-run index alone would delete the shared node the
    // active run still needs). The difference of two `BTreeSet`s iterates sorted, so the result
    // is deterministic (a spec-21 constraint).
    drop_candidates.difference(&keep).cloned().collect()
}

/// The graph-node id the projector folds a `DecisionMade` / `ReviewFinding` event under: the
/// `id` field of its JSON body (the exact key `contextgraph`'s fold reads, verbatim - the
/// decision/finding id is never alias-resolved). `None` for a malformed body or a
/// missing/empty id, so a corrupt event is skipped exactly as the projector's own fold skips
/// it, never dropping an unrelated node.
fn graph_node_id(e: &Event) -> Option<String> {
    let body: serde_json::Value = serde_json::from_slice(&e.data).ok()?;
    let id = body.get("id")?.as_str()?;
    (!id.is_empty()).then(|| id.to_string())
}

/// `rigger peers [<file> ...]` - print the peer decisions, lessons, and review findings
/// from the context graph scoped to the given files (or all if none), EXACTLY as the MCP
/// `rigger_peers` tool does (both render through [`mcpserver::peers_json`]). The store
/// is RESOLVED by walking up to the project's existing `.rigger` (refusing to fabricate
/// one, spec 05 - see [`require_store_dir`]); a side-car replays the `conductor::STREAM`
/// backlog and this command waits for it to catch up before rendering one readable
/// line per decision / lesson / finding. Rendering the lessons here is what makes the
/// capped prompt sections' "recover the full set with `rigger peers <file>`" note honest
/// for the lessons section, not just decisions and findings (adj-u1gap17).
fn cmd_peers(args: &[String]) -> Res {
    let files: Vec<String> = args.to_vec();

    let (loc, selection) = require_store_dir()?;
    let backend = resolve_store(&selection, &loc.file("events.db"))?;
    let store = Namespaced::new(backend.as_ref(), &loc.identity());

    // The side-car replays the whole backlog from position 0; wait until it has
    // drained every event currently in the store before reading, so a one-shot CLI
    // call sees the full picture (the long-running serve path catches up live).
    let peers = Sidecar::start(&store, 0, Filter::default())?;
    let total = store
        .read_all(0, Direction::Forward, &Filter::default())?
        .len();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while peers.len() < total && std::time::Instant::now() < deadline {
        std::thread::sleep(std::time::Duration::from_millis(10));
    }

    let result = mcpserver::peers_json(&peers, &files);
    let decisions = result["decisions"].as_array().cloned().unwrap_or_default();
    let lessons = result["lessons"].as_array().cloned().unwrap_or_default();
    let findings = result["findings"].as_array().cloned().unwrap_or_default();
    for d in &decisions {
        println!("{}", peer_decision_line(d));
    }
    for l in &lessons {
        let id = l["id"].as_str().unwrap_or_default();
        let summary = l["summary"].as_str().unwrap_or_default();
        let about = json_str_array(&l["about"]);
        println!("lesson {id} | {summary} | about: {about}");
    }
    for f in &findings {
        let id = f["id"].as_str().unwrap_or_default();
        let by = f["by"].as_str().unwrap_or_default();
        let summary = f["summary"].as_str().unwrap_or_default();
        let about = json_str_array(&f["about"]);
        println!("finding {id} | by {by} | {summary} | about: {about}");
    }
    Ok(())
}

/// Render one `rigger peers` decision line, labeling its provenance LIVE (from the
/// active run) or HISTORICAL (a superseded run, or pre-boundary) from the `live` flag the
/// side-car derived via the single c1 run attribution (spec 21, unit 3). The label makes
/// a prior run's decision legible instead of alarming; grounding still surfaces cross-run
/// decisions unchanged. A missing/false `live` flag renders HISTORICAL - the conservative
/// default that matches the side-car's own default.
fn peer_decision_line(d: &serde_json::Value) -> String {
    let id = d["id"].as_str().unwrap_or_default();
    let summary = d["summary"].as_str().unwrap_or_default();
    let governs = json_str_array(&d["governs"]);
    let provenance = if d["live"].as_bool().unwrap_or(false) {
        "LIVE"
    } else {
        "HISTORICAL"
    };
    format!("decision {id} | {provenance} | {summary} | governs: {governs}")
}

/// Join a JSON array of strings into a comma-separated list for a `rigger peers`
/// line (the `governs` / `about` files). A non-array or empty value renders as `-`.
fn json_str_array(v: &serde_json::Value) -> String {
    match v.as_array() {
        Some(a) if !a.is_empty() => a
            .iter()
            .filter_map(|x| x.as_str())
            .collect::<Vec<_>>()
            .join(", "),
        _ => "-".to_string(),
    }
}

/// A human-readable name for a JSON value's type, for the `rigger emit` error that
/// rejects a non-object payload.
fn json_type_name(v: &serde_json::Value) -> &'static str {
    match v {
        serde_json::Value::Null => "null",
        serde_json::Value::Bool(_) => "a boolean",
        serde_json::Value::Number(_) => "a number",
        serde_json::Value::String(_) => "a string",
        serde_json::Value::Array(_) => "an array",
        serde_json::Value::Object(_) => "an object",
    }
}

/// A parsed `rigger result` invocation (see [`cmd_result`]): the spawn `id`, the
/// optional outcome `text` (`None` means "read it from stdin"), whether `--error`
/// marks it a failure, whether `--if-absent` makes the record conditional, and the
/// optional `--meta` courier bookkeeping.
struct ResultArgs {
    id: String,
    text: Option<String>,
    is_error: bool,
    if_absent: bool,
    meta: Option<serde_json::Value>,
}

/// Parse `rigger result <id> [<output>] [--error] [--if-absent] [--meta '<json>']`.
///
/// `<id>` is the required deterministic spawn id (`{unit}/{role}#{attempt}`). The
/// outcome payload is an OPTIONAL second positional; when omitted, [`cmd_result`]
/// reads it from stdin (spec 04: "record a spawn's outcome (stdin or arg)"). `--error`
/// is a bare flag that turns the payload into the failure message rather than the
/// agent's output. `--if-absent` is a bare flag that makes the record CONDITIONAL: the
/// outcome is written only when the spawn has no result yet, atomically and without
/// clobbering an existing one (the thin driver's death courier uses it - spec 05).
/// `--meta` takes a JSON OBJECT (mirroring `rigger emit`'s payload contract) carrying
/// courier bookkeeping (e.g. the resolved model id, spec 05). Unknown flags, a
/// missing/empty id, a third positional, and a non-object/invalid `--meta` are all
/// rejected with a clear message.
fn parse_result_args(args: &[String]) -> Result<ResultArgs, Box<dyn std::error::Error>> {
    let mut id: Option<String> = None;
    let mut text: Option<String> = None;
    let mut is_error = false;
    let mut if_absent = false;
    let mut meta: Option<serde_json::Value> = None;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--error" => is_error = true,
            "--if-absent" => if_absent = true,
            "--meta" => {
                let raw = args.get(i + 1).ok_or(
                    "result: --meta needs a JSON object: rigger result <id> --meta '<json>'",
                )?;
                let value: serde_json::Value = serde_json::from_str(raw)
                    .map_err(|e| format!("result: --meta is not valid JSON: {e}"))?;
                if !value.is_object() {
                    return Err(format!(
                        "result: --meta must be a JSON object, got {}",
                        json_type_name(&value)
                    )
                    .into());
                }
                meta = Some(value);
                i += 1;
            }
            flag if flag.starts_with("--") => {
                return Err(format!("result: unknown flag {flag:?}").into());
            }
            positional => {
                if id.is_none() {
                    id = Some(positional.to_string());
                } else if text.is_none() {
                    text = Some(positional.to_string());
                } else {
                    return Err(format!(
                        "result: unexpected extra argument {positional:?}; usage: rigger result <id> [<output>] [--error] [--meta '<json>']"
                    )
                    .into());
                }
            }
        }
        i += 1;
    }
    let id = id.ok_or(
        "result: expected a spawn id: rigger result <id> [<output>] [--error] [--meta '<json>']",
    )?;
    if id.is_empty() {
        return Err("result: the spawn id must not be empty".into());
    }
    Ok(ResultArgs {
        id,
        text,
        is_error,
        if_absent,
        meta,
    })
}

/// Build the [`spawn::SpawnResult`] a `rigger result` invocation records, from its
/// parsed pieces and the already-resolved outcome `text` (positional arg or stdin).
///
/// Split from [`cmd_result`] (which does the stdin + store I/O) so the outcome-shaping
/// rules are a pure, unit-testable function. `--error` needs a NON-EMPTY message: a
/// blank error would leave [`spawn::SpawnResult::is_error`] false, so the replay driver
/// would answer the spawn AS a success and silently swallow the failure the courier
/// meant to record. A success may carry empty output (an agent that finished with no
/// final message is a valid outcome).
fn build_result(
    id: &str,
    text: &str,
    is_error: bool,
    meta: Option<serde_json::Value>,
) -> Result<spawn::SpawnResult, Box<dyn std::error::Error>> {
    let mut res = if is_error {
        if text.trim().is_empty() {
            return Err(format!(
                "result: --error for {id:?} needs a non-empty message (a blank error would replay as a success)"
            )
            .into());
        }
        spawn::SpawnResult::failed(id, text)
    } else {
        spawn::SpawnResult::ok(id, text)
    };
    if let Some(m) = meta {
        res = res.with_meta(m);
    }
    Ok(res)
}

/// Read the outcome payload from stdin when it was not given as an argument. A pipe /
/// heredoc conventionally appends a trailing newline (e.g. `echo "$out" | rigger
/// result ...`), so a SINGLE trailing `\n` (and a preceding `\r`) is stripped, leaving
/// exactly the payload rather than the shell's line terminator. Reading from an
/// interactive terminal with no argument would block forever, so that is a clear error
/// instead.
fn read_outcome_from_stdin() -> Result<String, Box<dyn std::error::Error>> {
    use std::io::{IsTerminal, Read};
    if std::io::stdin().is_terminal() {
        return Err("result: no outcome given - pass it as an argument (rigger result <id> <output>) or pipe it on stdin".into());
    }
    let mut buf = String::new();
    std::io::stdin().read_to_string(&mut buf)?;
    if buf.ends_with('\n') {
        buf.pop();
        if buf.ends_with('\r') {
            buf.pop();
        }
    }
    Ok(buf)
}

/// `rigger result <id> [<output>] [--error] [--if-absent] [--meta '<json>']` - record a
/// parked spawn's OUTCOME to the run log, so the conductor's replay driver answers that
/// spawn from the log instead of re-parking it and the next `rigger step` / `rigger run`
/// advances past it (spec 04). The courier that ran the parked agent reports its final
/// message as `<output>` (or on stdin); a worker that died is reported with `--error
/// <message>`; `--meta` attaches optional bookkeeping (e.g. the resolved model id).
///
/// `--if-absent` makes the write CONDITIONAL and atomic: the outcome is recorded only
/// when the spawn has no result yet, and an already-recorded result is left UNTOUCHED
/// (still exit 0). The thin driver's death courier uses it to record a died-worker
/// failure without clobbering a self-report that landed first - one atomic operation
/// closing the TOCTOU window the old two-process `rigger reported <id> || rigger result
/// <id> --error` guard left open (spec 05). See [`spawn::record_result_if_absent`].
///
/// The [`spawn::SpawnResult`] is appended to the SAME per-project [`Namespaced`] `run`
/// stream the conductor drives, so the write lands exactly where the replay driver reads.
/// A recorded failure replays AS a failure - the conductor remediates it just as it would
/// a live one. The store is RESOLVED by walking up to the project's existing `.rigger`
/// (refusing to fabricate one in the wrong cwd, spec 05 - see [`require_store_dir`]); and
/// before recording, a single pre-write read of the stream prints stderr advisories for
/// an ORPHAN id (no matching spawn request) or for SUPERSEDING an existing result (see
/// [`result_advisories`]).
fn cmd_result(args: &[String]) -> Res {
    let parsed = parse_result_args(args)?;
    // The outcome text comes from the positional arg when given, else stdin. Resolving
    // it here keeps `build_result` a pure function of already-resolved pieces.
    let text = match parsed.text {
        Some(t) => t,
        None => read_outcome_from_stdin()?,
    };
    let res = build_result(&parsed.id, &text, parsed.is_error, parsed.meta)?;

    // Resolve the EXISTING store (walk up; refuse if none) rather than fabricating one
    // in the wrong cwd, scoped by the RESOLVED root's identity: a courier run from a unit
    // worktree would otherwise record into a fresh dead store (no store) or misfile under
    // the worktree's own namespace (walked-up store) while the real spawn stays parked
    // forever - both fixed here (see [`require_store_dir`] / [`StoreLocation::identity`]).
    let (loc, selection) = require_store_dir()?;
    let backend = resolve_store(&selection, &loc.file("events.db"))?;
    let store = Namespaced::new(backend.as_ref(), &loc.identity());

    // One cheap pre-write read of the run stream, to advise (on stderr) about an orphan
    // id or about superseding an existing result BEFORE the append. Advisory only: the
    // record still lands, since pre-recording and deliberate re-recording are both
    // legitimate (see [`result_advisories`]). Weave with unit-10: under `--if-absent`
    // nothing can supersede (the CAS refuses), so the supersede note is suppressed -
    // the "left it untouched" line below reports that case honestly.
    let prior = store.read_stream(conductor::STREAM, 0, Direction::Forward)?;
    for note in result_advisories(&prior, &res.id, !parsed.if_absent) {
        eprintln!("{note}");
    }

    let kind = if res.is_error() {
        "error result"
    } else {
        "result"
    };
    // The position an append actually landed at, or `None` when `--if-absent` was a no-op
    // (a result already stood, so a prior `rigger result` already folded it - see the fold
    // below). Only a real append is folded into the graph.
    let recorded = if parsed.if_absent {
        // Conditional atomic record: write only if the spawn is still unanswered, never
        // overwriting an existing result. A no-op (a result already stood) is a success,
        // so the courier's `|| ...`-free single command always exits 0.
        match spawn::record_result_if_absent(&store, &res)? {
            Some(pos) => {
                println!("recorded {kind} for {} (position {pos})", res.id);
                Some(pos)
            }
            None => {
                println!(
                    "{} already has a result; --if-absent left it untouched",
                    res.id
                );
                None
            }
        }
    } else {
        let pos = spawn::record_result(&store, &res)?;
        println!("recorded {kind} for {} (position {pos})", res.id);
        Some(pos)
    };

    // Disposition-expiry (spec 25, criterion 1): fold the just-recorded result into this run's
    // context graph, EXACTLY as `rigger emit` folds an emitted event (see [`cmd_emit`] /
    // [`mcpserver::emit_event`]). The adjudicator's recorded `SpawnResult` is the ONLY place a
    // review's findings are disposed: the `TYPE_SPAWN_RESULT` fold arm reads its verdict line's
    // `discarded` ids (through the single [`spawn::SpawnResult::adjudication`] authority) and
    // invalidates those findings' graph edges, so grounding stops surfacing them. Without this
    // fold the arm is inert in production - the courier appends the verdict to `events.db` but
    // nothing ever folds a `SpawnResult` into the persistent `graph.db`, so a discarded finding
    // is never pruned. Only an adjudicator result disposes anything (`adjudication` self-gates
    // on the adjudicator role and returns `None` otherwise), so folding EVERY recorded result
    // is safe: a plain worker/courier result folds to nothing.
    //
    // Best-effort, and AFTER the durable append (mirroring `emit_event`): the record already
    // landed in the log, so a graph open/fold failure must NEVER fail a result the log holds -
    // the graph is a rebuildable projection, the log is the source of truth. A `--if-absent`
    // no-op appended nothing, so there is nothing new to fold (the prior record already did).
    if let Some(pos) = recorded {
        fold_recorded_result_into_graph(&loc, &res, pos);
    }

    // Per-spawn scratch reclamation (spec 34, criterion 1): the moment this spawn's result is
    // recorded - for ANY outcome (a success, a reject verdict, an `--error`, or a
    // liveness/infra fault, all of which reach the store through THIS courier) - reclaim the
    // dedicated scratch dir rigger assigned it under `.rigger/tmp`. `cmd_result` only ever
    // runs for the spawn being reported, so a spawn with no recorded result is never touched
    // (the "keeps its scratch" half of the criterion, by construction). Reclaimed even on a
    // `--if-absent` no-op: a result already stood, so a prior `rigger result` already
    // reclaimed it and reclaiming an already-gone path is a graceful no-op. Best-effort - the
    // record already landed durably, so scratch reclamation may never fail a recorded result.
    reclaim_spawn_scratch(&loc, &prior, &res.id);
    Ok(())
}

/// Reclaim the per-spawn scratch dir [`spawn_scratch_path`] assigned spawn `spawn_id`,
/// resolving the scratch root and run id the SAME way the assignment (the replay driver's
/// park) did so the reclaim targets the exact path the run created (spec 34, criterion 1).
///
/// Entirely best-effort and platform-tolerant: the result already landed durably in
/// `events.db`, so neither resolving the root nor removing the dir may surface an error that
/// fails a recorded result, and an already-gone path is a graceful no-op.
/// [`reap_then_remove_dir`] reaps any process still rooted under the scratch (spec 23) before
/// removing it, so a build a hung worker left running never outlives its now-deleted cwd.
fn reclaim_spawn_scratch(loc: &StoreLocation, prior: &[Event], spawn_id: &str) {
    let Some(repo) = loc.dir.parent().and_then(|p| p.to_str()) else {
        return;
    };
    // The run's scratch root by the SAME precedence the run assigned the path with
    // (`scratch_root_from_env`: RIGGER_TMPDIR > `defaults.workdir` > the `<repo>/.rigger/tmp`
    // default). The courier inherits the run's `RIGGER_TMPDIR`; `workdir` loads best-effort,
    // falling back to the repo default when the config is momentarily unreadable (the
    // overwhelming common placement). The read-only `_path_` resolver never conjures a root.
    let workdir = config::load(repo)
        .map(|c| c.workflow.defaults.workdir)
        .unwrap_or_default();
    let scratch_root = rigger::worktree::scratch_root_path_from_env(repo, &workdir);
    let run_id = runscope::current_run_id(prior).unwrap_or_default();
    reap_then_remove_dir(&spawn_scratch_path(&scratch_root, &run_id, spawn_id));
}

/// Fold a just-recorded [`spawn::SpawnResult`] into the run's context graph at its recorded
/// `position`, so an adjudicator verdict that disposes a review's findings invalidates their
/// graph edges (the `contextgraph` `TYPE_SPAWN_RESULT` fold arm). This is the result-channel
/// analogue of the emit-channel fold [`mcpserver::emit_event`] performs: rebuild the appended
/// event, stamp it with the position the append returned, and `apply` it to the SAME `graph.db`
/// the resolved store owns (`loc.file("graph.db")`, exactly as [`cmd_emit`] co-locates it).
///
/// Entirely best-effort: the record already landed durably in `events.db`, so neither opening
/// the projector nor applying the fold may surface an error that fails a recorded result. A
/// serialize failure (unreachable for a result that just serialized to append) or a graph I/O
/// failure is swallowed - the log stays the source of truth and the graph re-derives on the
/// next fold or rebuild.
fn fold_recorded_result_into_graph(
    loc: &StoreLocation,
    res: &spawn::SpawnResult,
    pos: rigger::eventstore::Position,
) {
    let Ok(mut event) = res.to_event() else {
        return;
    };
    event.position = pos;
    if let Ok(graph) = Projector::open(&loc.file("graph.db"), &loc.identity()) {
        let _ = graph.apply(&event);
    }
}

fn cmd_validate(args: &[String]) -> Res {
    let root = Path::new(".");
    // Optional `<spec>` path (spec 18, Unit 4): emit heuristic spec-shape advisories that
    // name the rule and recommend the fix. These are ADVISORY - they never change the exit
    // status - so a badly-shaped criterion is surfaced, not refused. Run before config
    // validation so a spec can be linted from a fresh checkout whose rigger config is not
    // yet valid; an unreadable spec path is still an input error (the lint is heuristic,
    // but "you named a spec that does not exist" is not).
    if let Some(spec_path) = args.first() {
        let text = std::fs::read_to_string(spec_path)
            .map_err(|e| format!("read spec {spec_path}: {e}"))?;
        for advisory in spec::spec_shape_advisories(&text) {
            eprintln!("warning: spec {spec_path}: {advisory}");
        }
    }
    let cfg = config::load(".")?;
    // Static verdict-line lint (spec 18, unit 1): a gating adjudicator whose persona only
    // records its verdict via `rigger_emit` - never on its result output - is a guaranteed
    // stall, because the integration gate reads the result channel, not emitted events. This
    // is a HARD error (deterministic hang) that names the fix, so `rigger validate` refuses a
    // config that would silently ferment into an escalation loop.
    config::lint_gating_verdict_lines(&cfg)?;
    // Surface the running binary's version + build provenance (spec 18) so an agent driving
    // `rigger validate` can identify the exact binary - the same provenance the drift
    // advisory below uses to name which side is stale.
    println!("{}", version_line());
    println!(
        "config valid: {} agents, {} stages, {} gates",
        cfg.agents.len(),
        cfg.workflow.stages.len(),
        cfg.workflow.gates.len()
    );
    // Build-environment SURFACES report (spec 65 units 2 and 5, NO SILENT DEGRADE /
    // HONEST SURFACES): a named-but-absent `build.wrapper`, or a named wrapper whose cache
    // dir cannot be created, already failed above (`config::load`'s `Config::validate`
    // rejects both at run start, before `cfg` could exist), so by this point resolution
    // can only succeed - this SURFACES what it resolved to (wrapper, cache dir, budget) so
    // an `auto` probe that quietly found nothing (or found a wrapper whose cache dir turned
    // out unusable) is SEEN as "none" here rather than silently doing nothing invisibly.
    // Reads through the SAME `resolve_build_layer` authority `Config::validate` and the
    // conductor's build-environment authority use - never a second, independently
    // re-derived report; the formatting itself lives in the pure, unit-tested
    // `build_environment_report` below so this edge stays a thin resolve-then-print.
    let wrapper =
        match resolve_build_layer(&cfg.workflow.build.wrapper, &cfg.workflow.build.cache_dir) {
            Ok(w) => w,
            Err(e) => return Err(e.to_string().into()),
        };
    // Mutation-efficacy step SURFACE (spec 73): a `build.mutation: on` with no `cargo-mutants`
    // resolvable already failed above (`config::load`'s `Config::validate` rejects it at run
    // start, before `cfg` could exist), so by this point resolution can only succeed - reads
    // through the SAME `resolve_mutation_layer` authority `Config::validate` uses, never a
    // second, independently re-derived check.
    let mutation_enabled = match resolve_mutation_layer(&cfg.workflow.build.mutation) {
        Ok(m) => m,
        Err(e) => return Err(e.to_string().into()),
    };
    for line in build_environment_report(wrapper.as_deref(), &cfg.workflow.build, mutation_enabled)
    {
        println!("{line}");
    }
    // Non-fatal advisories (spec 05:55): surface config/install drift so it is seen,
    // not discovered by accident. Each is a stderr warning that never changes the exit
    // status - `rigger validate` still succeeds so long as the config itself is valid.
    for advisory in validate_advisories(root) {
        eprintln!("{advisory}");
    }
    // Unbounded wall-clock advisory (spec 19c, unit 3): warn when `defaults.max_wall_clock`
    // is unbounded and a gating role carries no per-agent bound, so a hung gating agent that
    // is never swept - a silent stall - is visible at author time. Non-fatal like the others;
    // reuses the single `config::gating_agent_ids` authority the verdict-line lint uses.
    if let Some(advisory) = config::unbounded_wall_clock_advisory(&cfg) {
        eprintln!("{advisory}");
    }
    // Residue surfacing (spec 06, unit 6 / Gap 14d): report leftover scratch worktrees,
    // orphaned build caches, shadow stores, and dead `rigger/u/*` branches - with sizes -
    // so residue is seen before a disk fills. Warnings only; validate NEVER fails or
    // deletes anything (cleanup stays with the step-start sweep).
    // A genuine store-SELECTION failure here (unreadable `.rigger/store.conn`, malformed
    // `workflow.yml`, invalid `store.backend`) SURFACES loudly - `?` fails validate - rather than
    // degrading to a wrong-store read that would misreport live worktrees/branches as residue
    // (d-u2rr-observer-selection-loud). The residue FINDINGS themselves stay warning-only below;
    // this only makes an inability to even resolve the run store loud, never silent.
    for advisory in residue_advisories(root, &cfg)? {
        eprintln!("{advisory}");
    }
    // Model-drift advisory (spec 13b, unit 1): warn when a tier's resolved model id
    // re-pointed since the previous run and recommend `rigger canary --if-model-changed`.
    // A store-read failure just skips the advisory (never fails validate), exactly like the
    // git-backed advisories above swallow a missing/erroring git.
    if let Ok(drift) = read_model_drift(&db_path("events.db"), &project_identity()) {
        if let Some(advisory) = model_drift_advisory(&drift) {
            eprintln!("{advisory}");
        }
    }
    // Order-signature advisory (spec 71, VALIDATE DETECTS THE SIGNATURE): warn when a
    // stream's position order and revision order disagree - the tail a write leaves when it
    // lands at a revision a compaction opened as a hole. Report-only, like every other
    // validate advisory: this NEVER repairs, reorders, or changes the exit status (fail-safe
    // direction only - the spec's repair stays a documented operator procedure, never a
    // command). A store-read failure just skips the advisory (never fails validate), exactly
    // like the model-drift advisory above.
    if let Ok(signatures) = read_order_signatures(&db_path("events.db"), &project_identity()) {
        for advisory in order_signature_advisories(&signatures) {
            eprintln!("{advisory}");
        }
    }
    // INDEX STALENESS advisory (spec 68, VALIDATE ADVISORIES): warn when the persisted
    // `symbols` grounding index has drifted from the tree and name `rigger reindex`. Cost-
    // bounded and ungated (Design) - see `grounder::symbols::staleness`'s own docs for the
    // measurement itself. `None` when there is no persisted index (nothing to compare
    // against) or no disagreement; this never fails validate.
    if let Some(drift) = rigger::grounder::symbols::staleness(root.to_str().unwrap_or(".")) {
        eprintln!("{}", index_staleness_message(&drift));
    }
    // LOG BLOAT advisory (spec 68, VALIDATE ADVISORIES): warn when the event log's derived
    // index is duplicated above threshold and name `rigger reset --derived`. Reuses the
    // store's OWN aggregate ([`rigger::eventstore::sqlite::Store::measure_derived_duplication`],
    // the same key/type/prefix authority the compaction itself uses - no shadow accounting).
    // `None` on a server-backed project (a sqlite-only mechanic, exactly like `reset --derived`
    // itself), on a project with no events.db yet, or on any read failure; this never fails
    // validate and never creates a store that does not already exist.
    if let Some(advisory) = bloat_advisory_for(&db_path("events.db"), &project_identity()) {
        eprintln!("{advisory}");
    }
    // Docs-drift GATE (spec 20, unit 2): the committed `using-rigger` skill and handbook
    // discipline chapter are generated by `rigger docs` from the same code facts this binary
    // runs on. When a source fact or a template changes, a fresh render diverges from the
    // committed copy - so re-render here and, UNLIKE the warning advisories above, FAIL
    // LOUDLY (a non-zero exit, surfaced by `main`) when the committed docs no longer match,
    // naming the drifted files and the `rigger docs` fix. This is what makes the discipline
    // STAY accurate rather than merely start accurate. Runs last so the config summary and the
    // soft advisories are still seen before the hard failure. Absent files are skipped, so an
    // operator project that never carries rigger's own committed docs still passes validate.
    if let Some(failure) = docs_drift_failure(root) {
        return Err(failure.into());
    }
    Ok(())
}

/// The build-environment lines `rigger validate` prints (spec 65 unit 5, HONEST SURFACES):
/// given the wrapper ALREADY resolved by [`resolve_build_layer`] (the same authority
/// `Config::validate`'s run-start check and the conductor's build-environment authority
/// read - this never re-derives it), render:
/// - the wrapper name, or `none` when the layer is inactive;
/// - the cache dir it resolved to (via [`resolved_cache_dir`], the SAME ternary
///   [`BuildEnv::resolve`] and the cache-dir probe use) - ONLY when a wrapper is actually
///   active, since an inactive layer touches no cache dir and claiming one would fabricate
///   a surface nothing backs;
/// - the machine-wide build budget, ALWAYS: `build.max_concurrent` gates every compiler
///   invocation this loop runs (spec 65 unit 3) regardless of whether a wrapper is
///   configured, so an operator sees it even with the wrapper off. `0` is the documented
///   unlimited convention (mirrors `defaults.budget`), reported in words rather than a
///   bare, easily-misread `0`.
/// - the mutation-efficacy step setting, ALWAYS (spec 73): `on` or `off`, given the ALREADY-
///   RESOLVED `mutation_enabled` (through the SAME `resolve_mutation_layer` authority
///   `Config::validate`'s run-start check uses - a `build.mutation: on` with no
///   `cargo-mutants` on PATH already failed before this could be reached, so by the time
///   this prints, `on` in config and `mutation_enabled: true` always agree).
///
/// Pure formatting over already-resolved values, so it is unit-tested without touching
/// PATH or the filesystem; the effectful wrapper/mutation resolution stays at the
/// `cmd_validate` edge that calls this.
fn build_environment_report(
    wrapper: Option<&str>,
    build: &config::BuildConfig,
    mutation_enabled: bool,
) -> Vec<String> {
    let mut lines = Vec::new();
    match wrapper {
        Some(w) => {
            lines.push(format!("build wrapper: {w}"));
            lines.push(format!(
                "build cache dir: {}",
                resolved_cache_dir(&build.cache_dir)
            ));
        }
        None => lines.push("build wrapper: none".to_string()),
    }
    lines.push(format!(
        "build budget: {}",
        if build.max_concurrent == 0 {
            "unlimited".to_string()
        } else {
            build.max_concurrent.to_string()
        }
    ));
    lines.push(format!(
        "build mutation: {}",
        if mutation_enabled { "on" } else { "off" }
    ));
    lines
}

/// The non-fatal `rigger validate` advisories (spec 05:55), in report order:
///   (a) the installed `/rigger` workflow has drifted from this binary's embedded copy;
///   (b) tracked `.rigger/` files carry uncommitted modifications.
/// Both are warnings only - they are collected here and printed to stderr by the caller
/// without affecting the exit status. Rooted at `root` so the seam is testable against a
/// temp dir without mutating the process-wide current directory.
fn validate_advisories(root: &Path) -> Vec<String> {
    let mut advisories = Vec::new();
    // Identity durability (spec 09): without a tracked project.id, identity is the volatile
    // directory basename, so a rename away orphans this project's run history. Warn (like
    // the other drift advisories) so it is seen before a rename loses the log.
    if !has_tracked_project_id(root) {
        advisories.push(format!(
            "warning: no tracked {RIGGER_DIR}/{PROJECT_ID_FILE}; this project's identity falls \
             back to the directory basename, so renaming the checkout orphans its run history. \
             Run `rigger setup` (or `rigger init`) to mint a durable id, then commit it."
        ));
    }
    // Workflow-drift diagnostic (spec 18, criterion 9): when the installed workflow differs
    // from this binary's embedded copy, name WHICH side is stale (the installed workflow vs
    // the binary) using the embedded build provenance and give the directive fix, rather
    // than an ambiguous "they differ". The binary's provenance and the git ancestry oracle
    // are wired here at the edge; the decision itself is the pure [`drift_side`].
    if let Some(advisory) =
        workflow_drift_advisory(root, BUILD_PROVENANCE, |a, b| git_is_ancestor(root, a, b))
    {
        advisories.push(advisory);
    }
    if let Some(dirty) = uncommitted_rigger_advisory(root) {
        advisories.push(dirty);
    }
    // Spec 74, criterion 2: the missing-go-gitsemver-binary advisory (fires whenever
    // THIS binary's own embedded version carries the `+unversioned` marker, regardless
    // of cause) and the behind-the-tree advisory (fires when the checkout's freshly
    // re-derived version is genuinely ahead of it) are independent - both wired here,
    // never mutually exclusive in code even though in practice at most one condition
    // tends to hold at a time (an unversioned installed side already silences the
    // behind-the-tree comparison on its own, inside `behind_the_tree_message`).
    if let Some(advisory) = missing_gitsemver_binary_advisory(GITSEMVER_VERSION) {
        advisories.push(advisory);
    }
    if let Some(advisory) = behind_the_tree_advisory(root, GITSEMVER_VERSION, BUILD_PROVENANCE) {
        advisories.push(advisory);
    }
    advisories
}

/// The INDEX STALENESS advisory line (spec 68, VALIDATE ADVISORIES), rendered from an already-
/// computed [`rigger::grounder::symbols::IndexDrift`] (the pure formatting stays separate from
/// the gathering in [`rigger::grounder::symbols::staleness`], exactly like
/// [`build_environment_report`] above). Names counts per kind of disagreement - never just a
/// bare "it drifted" - and the fix, `rigger reindex`.
fn index_staleness_message(drift: &rigger::grounder::symbols::IndexDrift) -> String {
    let mut parts = Vec::new();
    if !drift.added.is_empty() {
        parts.push(format!(
            "{} file(s) on disk not yet in the index",
            drift.added.len()
        ));
    }
    if !drift.removed.is_empty() {
        parts.push(format!(
            "{} indexed file(s) no longer on disk",
            drift.removed.len()
        ));
    }
    if !drift.changed.is_empty() {
        parts.push(format!(
            "{} sampled file(s) whose content changed",
            drift.changed.len()
        ));
    }
    format!(
        "warning: the symbols grounding index ({}) has drifted from the tree ({}). Run \
         `rigger reindex <file>...` to refresh it.",
        rigger::grounder::symbols::store::index_path(".").display(),
        parts.join(", "),
    )
}

/// The derived-index duplication FACTOR (rows per distinct key) above which `rigger validate`
/// warns of log bloat (Design: "derived-type duplication factor above threshold"). `1.5` means
/// at least half again as many recordings as distinct keys survive in the log - a real
/// redundancy signal, not the occasional legitimate re-recording (a revert, a branch switch) a
/// small, healthy log can carry without ever being worth an operator's attention.
const BLOAT_DUPLICATION_THRESHOLD: f64 = 1.5;

/// The LOG BLOAT advisory line (spec 68, VALIDATE ADVISORIES), rendered from an already-measured
/// [`rigger::eventstore::sqlite::DerivedDuplication`] - pure formatting, separate from the
/// gathering in [`bloat_advisory_for`]. `None` when the measured factor does not clear
/// [`BLOAT_DUPLICATION_THRESHOLD`].
fn bloat_advisory(measured: &rigger::eventstore::sqlite::DerivedDuplication) -> Option<String> {
    let factor = measured.factor();
    if factor <= BLOAT_DUPLICATION_THRESHOLD {
        return None;
    }
    Some(format!(
        "warning: the event log's derived index is duplicated {factor:.1}x ({} row(s) recording \
         only {} distinct key(s)); run `rigger reset --derived` to compact it.",
        measured.rows, measured.distinct_keys
    ))
}

/// Gather + measure the LOG BLOAT advisory's input (spec 68): open the sqlite event log at
/// `path`, scoped to `project`'s stream prefix, and run
/// [`rigger::eventstore::sqlite::Store::measure_derived_duplication`] - the ONE read-only
/// aggregate the compaction's own `key_expr`/`type_list` authority backs (Design: "no shadow
/// accounting"). `None`, never an error, on every reason there is nothing honest to measure:
/// a server-backed project (this is a sqlite-only mechanic, exactly like `reset --derived`
/// itself refuses there - see [`cmd_reset`]), or a project with no `events.db` file YET - checked
/// BEFORE opening anything, because [`open_sqlite_store`] (like [`Store::open`] under it) creates
/// a missing file, and a read-only advisory must never have that side effect. Any read error
/// after that point (a malformed store, a lock) is likewise swallowed, exactly like the model-
/// drift and order-signature advisories above.
fn bloat_advisory_for(path: &str, project: &str) -> Option<String> {
    let sel = store_selection(None, None).ok()?;
    if !sel.is_sqlite() || !Path::new(path).exists() {
        return None;
    }
    let store = open_sqlite_store(path).ok()?;
    let prefix = Namespaced::prefix_for(project);
    let measured = store
        .measure_derived_duplication(&prefix, &rigger::ingest::derived_index_identity())
        .ok()?;
    bloat_advisory(&measured)
}

/// Whether the `/rigger` workflow installed at `<root>/.claude/workflows/rigger.js` has
/// DRIFTED from the embedded [`RIGGER_WORKFLOW`] this binary ships. `false` when the file
/// is absent (nothing installed, so nothing to drift) or byte-identical to the embedded
/// copy; `true` only when an installed file differs. This is the single source of truth
/// for the "installed vs embedded workflow" comparison - it reuses the same
/// [`workflow_path`] and [`RIGGER_WORKFLOW`] that [`install_workflow`] writes, so the
/// drift check and the install can never disagree on what "the workflow" is.
fn installed_workflow_drifted(root: &Path) -> bool {
    match std::fs::read(workflow_path(root)) {
        Ok(bytes) => bytes != RIGGER_WORKFLOW.as_bytes(),
        Err(_) => false, // absent or unreadable: no installed workflow to surface drift for
    }
}

/// The sidecar that records WHICH build's `rigger setup` last wrote the installed
/// workflow, stored beside it as `.claude/workflows/.rigger-workflow-provenance`. The
/// drift diagnostic reads it (see [`workflow_drift_advisory`]) to name which side of a
/// workflow drift is stale. Absent for a workflow written by a build that predates this
/// recording - the diagnostic then falls back to the refresh directive.
fn workflow_provenance_path(root: &Path) -> std::path::PathBuf {
    workflow_path(root).with_file_name(".rigger-workflow-provenance")
}

/// The build provenance recorded for the installed workflow (the build whose `rigger
/// setup` last wrote it), or `None` when no sidecar is present (an older install, or none).
/// Trimmed so a trailing newline never defeats the comparison against [`BUILD_PROVENANCE`].
fn installed_workflow_provenance(root: &Path) -> Option<String> {
    let raw = std::fs::read_to_string(workflow_provenance_path(root)).ok()?;
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

/// Whether commit `ancestor` is an ancestor of commit `descendant` in the git repository
/// rooted at `root`: `Some(true)`/`Some(false)` when git can decide, `None` when it cannot
/// (git unavailable, not a repo, or either id unresolvable - e.g. an operator project that
/// does not carry rigger's history). Uses `git merge-base --is-ancestor`, whose exit status
/// is 0 for an ancestor and 1 otherwise; any other status is treated as undecidable. This
/// is the ordering oracle the [`drift_side`] decision injects, so the pure decision stays
/// testable in both directions without a live repo.
///
/// Spec 74, criterion 2 periphery finding: captures the child's stdout/stderr (`.output()`)
/// rather than inheriting the parent's (`.status()`), so an unresolvable `ancestor` - the
/// exact "does not carry rigger's history" case this doc comment already calls out as
/// normal and silently handled via `None` - never leaks git's own `fatal: Not a valid
/// object name ...` onto the CALLER's stderr. Before this fix `behind_the_tree_advisory`
/// (below) called this UNCONDITIONALLY on every `rigger validate` invocation, so the leak
/// fired on nearly every non-self-hosting target project, not merely the rare drifted-
/// workflow-with-recorded-provenance case [`workflow_drift_advisory`] alone would reach;
/// mirrors [`git_commit_distance`]'s already-correct `.output()` pattern immediately below.
fn git_is_ancestor(root: &Path, ancestor: &str, descendant: &str) -> Option<bool> {
    let out = Command::new("git")
        .args(["merge-base", "--is-ancestor", ancestor, descendant])
        .current_dir(root)
        .output()
        .ok()?;
    match out.status.code() {
        Some(0) => Some(true),
        Some(1) => Some(false),
        _ => None,
    }
}

/// Number of commits `descendant` carries beyond `ancestor` (`git rev-list --count
/// ancestor..descendant`), or `None` when git cannot decide (unavailable, not a repo, or
/// either id unresolvable - the same undecidable cases [`git_is_ancestor`] reports).
/// Meaningful only once the caller already knows `ancestor` truly is a (proper) ancestor
/// of `descendant`: this just counts, it never itself verifies order (spec 74, criterion
/// 2's commit-distance figure for the behind-the-tree advisory).
fn git_commit_distance(root: &Path, ancestor: &str, descendant: &str) -> Option<u64> {
    let out = Command::new("git")
        .args(["rev-list", "--count", &format!("{ancestor}..{descendant}")])
        .current_dir(root)
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    String::from_utf8(out.stdout).ok()?.trim().parse().ok()
}

/// The missing-`go-gitsemver`-binary advisory (spec 74, criterion 2): whenever THIS
/// installed binary's own embedded version ([`GITSEMVER_VERSION`]) carries the
/// [`gitsemver::UNVERSIONED_SUFFIX`] marker - REGARDLESS OF CAUSE (the tool missing from
/// PATH at build time, the build not run inside a git checkout, or any other reason
/// [`gitsemver::derive_version`] fell back). One uniform advisory for every cause,
/// mirroring `derive_version`'s own one-uniform-fallback contract (see
/// `build/gitsemver.rs`'s module doc: the embedded marker itself cannot distinguish its
/// cause, so every cause folds into one signal - here too, into one advisory). `None`
/// when the version was genuinely derived.
fn missing_gitsemver_binary_advisory(installed_version: &str) -> Option<String> {
    if !installed_version.ends_with(gitsemver::UNVERSIONED_SUFFIX) {
        return None;
    }
    Some(format!(
        "warning: this rigger binary's version ({installed_version}) could not be derived by \
         go-gitsemver at build time (the tool may be missing from PATH, or the build did not \
         run inside a git checkout - the embedded marker cannot distinguish which). Install \
         go-gitsemver (github.com/MyCarrier-DevOps/go-gitsemver) and rebuild so future builds \
         report a real, comparable version instead of the crate's bare semver."
    ))
}

/// The pure "behind-the-tree" decision (spec 74, criterion 2): given the installed
/// binary's version, the checkout's freshly re-derived version, and how many commits (if
/// decidable) the checkout is ahead of the installed build, decide the advisory text.
/// `None` whenever there is nothing actionable: the two version strings already agree
/// (nothing that would change the derived order actually changed), either side carries
/// the `+unversioned` marker (derivation unavailable is a DIFFERENT condition, reported
/// separately by [`missing_gitsemver_binary_advisory`] - a string comparison against a
/// fallback marker would be meaningless noise here), the git order was undecidable, or
/// the checkout is not (or no longer) ahead. `commit_distance` is injected so this stays
/// pure and testable without a live repo - mirrors [`drift_side`]'s injected-oracle shape.
fn behind_the_tree_message(
    installed_version: &str,
    checkout_version: &str,
    commit_distance: Option<u64>,
) -> Option<String> {
    if installed_version == checkout_version
        || installed_version.ends_with(gitsemver::UNVERSIONED_SUFFIX)
        || checkout_version.ends_with(gitsemver::UNVERSIONED_SUFFIX)
    {
        return None;
    }
    let distance = commit_distance?;
    if distance == 0 {
        return None;
    }
    Some(format!(
        "warning: this checkout's derived version ({checkout_version}) is {distance} commit(s) \
         ahead of the installed rigger binary's version ({installed_version}); rebuild rigger \
         so the binary matches the tree."
    ))
}

/// Gather the behind-the-tree advisory (spec 74, criterion 2) at `root`, given
/// `installed_version` and `installed_commit` (the composition root wires
/// [`GITSEMVER_VERSION`] and [`BUILD_PROVENANCE`] - the SAME commit-determined identity
/// [`workflow_drift_advisory`] already uses, per the spec's global constraint that
/// stored provenance keeps the build hash as its identity key). Re-derives the
/// CHECKOUT's current version through the SAME derivation seam criterion 1 embeds at
/// compile time ([`gitsemver::derive_version`], reused here rather than reimplemented -
/// a live, runtime invocation of `go-gitsemver`/git for VALIDATE's comparison
/// specifically, never for `rigger version`'s own compile-time-only self-report; see the
/// `mod gitsemver` doc comment above for why this is in-bounds). "Ahead" is decided by
/// real git ANCESTRY of `installed_commit` in `HEAD` (mirrors [`git_is_ancestor`]'s
/// existing role in [`drift_side`]) rather than a hand-rolled semver comparison: under
/// this project's Mainline config the derived order is monotonic with commit order along
/// one line of history, and ancestry is the more literal reading of the Problem
/// statement's own "behind the tree" framing, with no new dependency. Only when
/// `installed_commit` truly is a proper ancestor of `HEAD` is a distance even computed;
/// the actual decision is the pure [`behind_the_tree_message`].
fn behind_the_tree_advisory(
    root: &Path,
    installed_version: &str,
    installed_commit: &str,
) -> Option<String> {
    let checkout_version = gitsemver::derive_version("go-gitsemver", root);
    let distance = if git_is_ancestor(root, installed_commit, "HEAD") == Some(true) {
        git_commit_distance(root, installed_commit, "HEAD")
    } else {
        None
    };
    behind_the_tree_message(installed_version, &checkout_version, distance)
}

/// Which side of an installed-vs-embedded workflow drift is stale (spec 18, criterion 9).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DriftSide {
    /// The installed workflow is from a NEWER build than this binary: the binary is stale
    /// (rebuild it).
    BinaryStale,
    /// The installed workflow is older than - or was hand-edited away from - this binary's
    /// embedded copy: the workflow is stale (`rigger setup` to refresh it).
    WorkflowStale,
}

/// Decide which side of a workflow drift is stale from the two builds' provenance, using an
/// injected ancestry oracle so the decision is pure and testable in both directions. Names
/// the BINARY as stale ONLY when the installed workflow's build is provably newer (this
/// binary's build is a proper ancestor of it). Every other case (the installed build equals
/// this binary from a local hand-edit, no recorded provenance, or an undecidable order)
/// resolves to the actionable refresh directive, so the diagnostic is never the ambiguous
/// "they differ".
fn drift_side(
    installed_provenance: Option<&str>,
    binary_provenance: &str,
    is_ancestor: impl Fn(&str, &str) -> Option<bool>,
) -> DriftSide {
    match installed_provenance {
        Some(installed)
            if installed != binary_provenance
                && is_ancestor(binary_provenance, installed) == Some(true) =>
        {
            DriftSide::BinaryStale
        }
        _ => DriftSide::WorkflowStale,
    }
}

/// The workflow-drift advisory (spec 18, criterion 9): when the installed `/rigger` workflow
/// differs from this binary's embedded copy, name WHICH side is stale using the build
/// provenance and give the directive fix (rebuild the binary vs `rigger setup`), never an
/// ambiguous "they differ". `None` when there is no drift. `binary_provenance` and the
/// ancestry oracle are injected so the message is testable for both drift directions without
/// a live git repo; the composition root wires [`BUILD_PROVENANCE`] and [`git_is_ancestor`].
fn workflow_drift_advisory(
    root: &Path,
    binary_provenance: &str,
    is_ancestor: impl Fn(&str, &str) -> Option<bool>,
) -> Option<String> {
    if !installed_workflow_drifted(root) {
        return None;
    }
    let path = workflow_path(root);
    let installed_provenance = installed_workflow_provenance(root);
    Some(
        match drift_side(
            installed_provenance.as_deref(),
            binary_provenance,
            is_ancestor,
        ) {
            DriftSide::BinaryStale => format!(
                "warning: the installed /rigger workflow ({}) is from a newer build ({}) than \
                 this rigger binary (build {}); the binary is stale. Rebuild rigger so the \
                 workflow and the binary that drives it are the same build.",
                path.display(),
                installed_provenance.as_deref().unwrap_or("a newer build"),
                binary_provenance,
            ),
            DriftSide::WorkflowStale => format!(
                "warning: the installed /rigger workflow ({}) has drifted from this rigger \
                 binary's embedded copy (build {}); the installed workflow is stale. Run \
                 `rigger setup` to refresh it so the workflow and the binary that drives it \
                 are the same build.",
                path.display(),
                binary_provenance,
            ),
        },
    )
}

/// Advisory naming the tracked `.rigger/` files that carry uncommitted modifications, or
/// `None` when the tracked `.rigger/` tree is clean (or the project is not a git repo, or
/// git is unavailable - in which case there is nothing to flag). Runs `git status
/// --porcelain -- .rigger` rooted at `root` and folds its output through the pure
/// [`dirty_tracked_paths`] seam.
fn uncommitted_rigger_advisory(root: &Path) -> Option<String> {
    let out = Command::new("git")
        .args(["status", "--porcelain", "--", RIGGER_DIR])
        .current_dir(root)
        .output()
        .ok()?;
    if !out.status.success() {
        return None; // not a git repo / git absent: nothing to flag
    }
    let porcelain = String::from_utf8_lossy(&out.stdout);
    let dirty = dirty_tracked_paths(&porcelain);
    if dirty.is_empty() {
        return None;
    }
    let mut msg = String::from("warning: tracked .rigger/ files have uncommitted modifications:");
    for path in &dirty {
        msg.push_str("\n  - ");
        msg.push_str(path);
    }
    msg.push_str("\nCommit or discard them so a run starts from a clean, reproducible state.");
    Some(msg)
}

/// Given `git status --porcelain` output already scoped to `.rigger/`, return the paths
/// of TRACKED files with uncommitted modifications. Untracked (`??`) and ignored (`!!`)
/// entries are excluded - the criterion flags TRACKED files, and a machine-local
/// untracked/ignored file (e.g. `.rigger/events.db`, `.rigger/shim/`) is not a drift the
/// operator must commit. A porcelain line is `XY <path>` (two status columns, a space,
/// then the path); rename entries (`R  old -> new`) are reported verbatim.
fn dirty_tracked_paths(porcelain: &str) -> Vec<String> {
    porcelain
        .lines()
        .filter_map(|line| {
            // A well-formed porcelain line is at least "XY " followed by the path.
            if line.len() < 4 {
                return None;
            }
            let status = &line[..2];
            if status == "??" || status == "!!" {
                return None; // untracked or ignored: not a tracked modification
            }
            Some(line[3..].to_string())
        })
        .collect()
}

// ---- `rigger validate` residue report (spec 06, unit 6 / Gap 14d) -----------------
//
// `rigger validate` surfaces the run's leftover disk - scratch worktrees whose unit is
// no longer live, orphaned build caches, shadow `events.db` stores (the misfiling hazard
// proven by adversary finding adv9-shadow-store-reopens-defect), and dead `rigger/u/*`
// branches - as warnings that NEVER fail validation and NEVER delete anything. Cleanup
// stays with the step-start sweep (`worktree::sweep_terminal`); this half only reports.

/// The leftover artifacts a `rigger validate` residue scan found under the scratch root
/// (plus dead `rigger/u/*` branches), each with a size where one is meaningful. Held as
/// data so the scan is unit-testable apart from its stderr rendering ([`format_residue`]).
#[derive(Debug, Default, PartialEq, Eq)]
struct ResidueReport {
    /// Scratch-root worktrees (`rigger-wt-*`) whose unit is not live: (dir name, bytes).
    worktrees: Vec<(String, u64)>,
    /// Orphaned build caches directly under the scratch root: (dir name, bytes).
    caches: Vec<(String, u64)>,
    /// Shadow `events.db` stores anywhere under the scratch root: (relative path, bytes).
    shadow_stores: Vec<(String, u64)>,
    /// Local `rigger/u/*` branches with no live unit.
    branches: Vec<String>,
}

impl ResidueReport {
    fn is_empty(&self) -> bool {
        self.worktrees.is_empty()
            && self.caches.is_empty()
            && self.shadow_stores.is_empty()
            && self.branches.is_empty()
    }
}

/// The stderr advisory (spec 06:60) naming the run's residue, or empty when nothing is
/// leftover. Reuses the two impure seams a courier uses - the run store (for the LIVE
/// unit set) and git (for local `rigger/u/*` branches) - then folds the pure
/// [`scan_residue`]. Anchored at `root`'s owning store so the scanned scratch root is the
/// SAME `<repo>/.rigger/tmp` the run uses; the path is resolved WITHOUT creating it, so
/// validate stays read-only.
fn residue_advisories(
    root: &Path,
    cfg: &config::Config,
) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let cwd = std::env::current_dir().unwrap_or_else(|_| root.to_path_buf());
    // The repo whose `<repo>/.rigger/tmp` the run uses: the store's OWNING root when a
    // store exists (walking up as the couriers do), else the cwd's git top-level, else the
    // cwd itself. Keeps the scanned scratch root aligned with the run's actual one.
    let repo = find_store_dir_from(&cwd)
        .and_then(|d| d.parent().map(|p| p.to_string_lossy().into_owned()))
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| {
            let top = git_repo_at(&cwd);
            if top.is_empty() {
                cwd.to_string_lossy().into_owned()
            } else {
                top
            }
        });
    let scratch = PathBuf::from(rigger::worktree::scratch_root_path_from_env(
        &repo,
        &cfg.workflow.defaults.workdir,
    ));
    // A genuine store-SELECTION failure (unreadable secret file / malformed config / invalid
    // backend) SURFACES here rather than silently reading the wrong store - which, folding zero live
    // units, would misreport every LIVE `rigger/u/*` worktree/branch as removable residue
    // (d-u2rr-observer-selection-loud). The benign no-run/no-store and store-access-miss cases still
    // yield an empty live set inside `read_run_units`, so an unconfigured or never-run project scans
    // cleanly.
    let run_units = read_run_units(&cwd)?;
    let slugs = live_slugs(&run_units.live_branches);
    let local_branches = local_unit_branches(&cwd);
    let report = scan_residue(
        &scratch,
        &slugs,
        &run_units.dead_slugs,
        &local_branches,
        &run_units.live_branches,
    );
    let mut advisories = format_residue(&report);
    // Leaked-process advisory (spec 23, unit 2): any process still rooted under the SAME
    // resolved scratch root, warning-only like the residue block above. Reuses the `scratch`
    // path already resolved here and the shared scan authority - no second resolver, no second
    // scan - so a process left holding a now-deleted (or soon-to-be-removed) scratch dir is
    // visible even when no teardown is running.
    advisories.extend(leaked_process_advisories(&scratch));
    Ok(advisories)
}

/// The warning-only `rigger validate` advisories (spec 23, unit 2) naming every process still
/// rooted under the scratch root: a leak the teardown reap missed, or a process left running
/// while no teardown is active. ONE advisory per process, each naming its pid and command, so
/// an operator can see and reclaim it - surfaced only, like the residue block, never a hard
/// failure and never a kill (the teardown reap in `src/worktree.rs` / `cmd_step` is the only
/// kill). Empty when nothing is rooted there; and because the shared scan authority
/// ([`rigger::reap::processes_rooted_under`] - the SAME one the teardown reap consumes) returns
/// empty where the dir or `/proc` is absent, this is a graceful no-op (empty, never an error)
/// on a platform without `/proc` too.
fn leaked_process_advisories(scratch_root: &Path) -> Vec<String> {
    rigger::reap::processes_rooted_under(scratch_root)
        .into_iter()
        .map(|(pid, command)| {
            let named = if command.is_empty() {
                format!("pid {pid}")
            } else {
                format!("pid {pid} ({command})")
            };
            format!(
                "warning: process rooted under the scratch root (surfaced only - validate \
                 never reaps it): {named} - its cwd is under {}; it outlives a dir rigger owns \
                 until the next teardown or step reaps it.",
                scratch_root.display()
            )
        })
        .collect()
}

/// The CURRENT run's unit liveness, read from the run store the SAME way the couriers do
/// (walk UP to the owning store, scope by its identity). No store (a project that never
/// ran) means no live units, so every scratch worktree and `rigger/u/*` branch reads as
/// residue.
///
/// This reads the DURABLE real run stream, so it resolves WHICH backend through the one
/// authority ([`store_selection`]) exactly as every other real-run-stream read does
/// (`dash_read_run`, `canary_stats_lines`, `read_model_drift`): a project configured for the
/// server backend reads the SERVER's run (spec 48 criterion 1, "a command invoked in a project
/// configured for the server-backed store resolves that store"), never a stale local sqlite
/// file. It is NOT local-by-construction like the isolated replay store, so it must not pin
/// [`StoreSelection::Sqlite`].
///
/// A genuine selection FAILURE off a PRESENT source - an unreadable `.rigger/store.conn`, an
/// unreadable/malformed `workflow.yml`, an invalid `store.backend`, or the server selected with no
/// resolvable connection string - SURFACES as an `Err` here (propagated with `?`), never a silent
/// degrade to the local sqlite default: reading the wrong (empty local) store would fold zero live
/// units and misreport every LIVE `rigger/u/<slug>` worktree/branch as residue (via
/// [`residue_advisories`], spec 06 line 60) - the exact silent-wrong-store fracture spec 48's one
/// resolution authority and spec 19c's loud-failure-surfacing forbid (d-u2rr-observer-selection-loud).
/// Only the BENIGN "no run ever happened / nothing selected" cases degrade to `Ok(RunUnits::default())`
/// (no live units): a sqlite selection whose local store was never created, and a store that resolves
/// but cannot be opened or read (e.g. an unreachable configured server) - a store-ACCESS miss, distinct
/// from a selection FAILURE.
fn read_run_units(cwd: &Path) -> Result<RunUnits, Box<dyn std::error::Error>> {
    let sel = store_selection(None, None)?;
    // Resolve the store's OWNING root and identity. For sqlite the durable log is a LOCAL file,
    // so walk UP to it (as the couriers do); its absence means no run ever happened => no live
    // units. For the server backend there is no local `events.db` to walk to, so resolve through
    // the shared [`server_store_location`] (the SAME authority the store-opening couriers'
    // [`require_store_dir`] server branch uses), binding identity to the main repo root and
    // letting [`resolve_store`] reach the server.
    let loc = if sel.is_sqlite() {
        let Some(dir) = find_store_dir_from(cwd) else {
            return Ok(RunUnits::default());
        };
        StoreLocation { dir }
    } else {
        server_store_location(cwd)
    };
    // A store-ACCESS miss (an unreachable configured server, a corrupt local log) degrades to no
    // live units - best-effort, distinct from the selection FAILURE surfaced above: the store WAS
    // resolved, it just cannot be reached, so the residue scan stays warning-only rather than
    // failing validate on a transient outage.
    let Ok(backend) = resolve_store(&sel, &loc.file("events.db")) else {
        return Ok(RunUnits::default());
    };
    let store = Namespaced::new(backend.as_ref(), &loc.identity());
    match store.read_stream(conductor::STREAM, 0, Direction::Forward) {
        Ok(events) => Ok(current_run_units(&events)),
        Err(_) => Ok(RunUnits::default()),
    }
}

/// The branches/slugs of the CURRENT run's units. The branch (`rigger/u/<slug>`) is the
/// durable per-unit key the conductor records on `UnitStarted`; it does NOT record the
/// worktree dir (a per-process path), so the slug carried in the branch is the only stable
/// handle back to a unit.
#[derive(Default)]
struct RunUnits {
    /// `rigger/u/<slug>` of every non-terminal (in-flight) unit - these are LIVE, so their
    /// worktrees and branches are spared from residue.
    live_branches: std::collections::HashSet<String>,
    /// `<slug>` of every terminal (integrated/escalated) unit. A DEAD unit's leftover
    /// deterministic `rigger-wt-<slug>` worktree is itself residue, and its slug must not
    /// be mistaken for a live unit's per-process `-<8hex>` tail (adv-u6res-uuid8-tail).
    dead_slugs: std::collections::HashSet<String>,
}

/// Fold the CURRENT run's units from `events`. Scoping to the current run's slice via
/// `runscope::current_run` BEFORE `ledger::project` (exactly as `conductor.rs` folds the
/// run state it returns) is what makes a PRIOR run's abandoned non-terminal unit read as
/// residue instead of live: this CONSUMES the one "what is a live unit" authority rather
/// than defining a parallel notion of liveness (spec 06 unit 1, Gap 11).
fn current_run_units(events: &[Event]) -> RunUnits {
    let run = ledger::project(runscope::current_run(events)).unwrap_or_default();
    let mut out = RunUnits::default();
    for u in run.units.values() {
        if run.is_terminal(&u.id) {
            if let Some(slug) = u.branch.strip_prefix("rigger/u/") {
                if !slug.is_empty() {
                    out.dead_slugs.insert(slug.to_string());
                }
            }
        } else if !u.branch.is_empty() {
            out.live_branches.insert(u.branch.clone());
        }
    }
    out
}

/// The `<slug>` of each live unit (the shared token in `rigger/u/<slug>` and
/// `rigger-wt-<slug>`), derived from the live branch names.
fn live_slugs(
    live_branches: &std::collections::HashSet<String>,
) -> std::collections::HashSet<String> {
    live_branches
        .iter()
        .filter_map(|b| b.strip_prefix("rigger/u/").map(str::to_string))
        .collect()
}

/// Orphan-sweep backstop (spec 34, criterion 2): reclaim every scratch entry under `root`
/// that NO live unit of the current run owns - the ownership backstop that makes the
/// clean-up guarantee independent of agent goodwill. Two shapes are reclaimed: a
/// `rigger-wt-<slug>` worktree and a `cargo-target-<slug>` per-unit build cache (Gap 19)
/// whose `<slug>` names no live unit - a prior run's killed-process leftover, or an ad-hoc
/// `cargo-target-<slug>` an agent wrote outside its assigned path (the unbounded per-agent
/// build-cache leak spec 34 names). Both are removed only when they are NOT live-owned,
/// decided by the SAME [`worktree_belongs_to_live`] predicate `rigger validate`'s residue
/// report reads over the current run's [`RunUnits`] - one definition of "live-owned", not a
/// parallel notion.
///
/// The never-delete-live-owned invariant (spec 34 Global Constraint) is what the liveness key
/// buys: a LIVE unit's worktree/cache is spared, and so are the shared live-spawn areas this
/// backstop deliberately never touches - `agent-scratch` (probe repos and verify builds an
/// in-flight worker parks there), `agent-live` (per-spawn liveness markers), and the bare
/// shared `cargo-target`/`target` a live spawn may still be building into (the driver's
/// `CARGO_TARGET_DIR`). Those are run-level scratch reclaimed by the run's fixpoint/teardown
/// once no spawn is live, never by this per-step backstop, so it can never delete a target a
/// running build is writing. Best-effort per entry: a failed reclaim never aborts the sweep.
/// Returns how many entries were reclaimed.
fn reclaim_orphan_scratch(repo: &str, root: &str, run_units: &RunUnits) -> usize {
    let live = live_slugs(&run_units.live_branches);
    let mut removed = 0;
    let Ok(entries) = std::fs::read_dir(root) else {
        return 0;
    };
    for entry in entries.flatten() {
        let Ok(ft) = entry.file_type() else { continue };
        if !ft.is_dir() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        let path = entry.path();
        if name.starts_with(rigger::worktree::UNIT_WORKTREE_PREFIX) {
            // A leftover unit worktree no live unit owns. Reap any process still rooted in it
            // (a leaked build) BEFORE removing it, and deregister it from git if a killed step
            // left it registered.
            if !worktree_belongs_to_live(&name, &live, &run_units.dead_slugs) {
                reap_then_remove_worktree(repo, &path);
                removed += 1;
            }
        } else if let Some(slug) = name.strip_prefix(rigger::worktree::UNIT_CACHE_PREFIX) {
            // A per-unit / ad-hoc `cargo-target-<slug>` cache. Mirror the worktree liveness
            // check on the reconstructed `rigger-wt-<slug>` name so a cache stays in lockstep
            // with its unit's liveness (a live unit's cache is in use, not residue). A bare
            // `cargo-target` (no `-<slug>` tail) never matches this prefix and is spared.
            let wt = format!("{}{slug}", rigger::worktree::UNIT_WORKTREE_PREFIX);
            if !worktree_belongs_to_live(&wt, &live, &run_units.dead_slugs) {
                reap_then_remove_dir(&path);
                removed += 1;
            }
        }
        // Any other entry (agent-scratch, agent-live, a bare cargo-target/target, a review
        // worktree) is either a live-shared area or not rigger's slug-keyed scratch: spared
        // here and reclaimed, if ever, by the run-level fixpoint/teardown - never this backstop.
    }
    removed
}

/// The local `rigger/u/*` branches in the repo governing `cwd`, via `git for-each-ref`.
/// Empty when git is unavailable or `cwd` is not a repo (nothing to flag then).
fn local_unit_branches(cwd: &Path) -> Vec<String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(cwd)
        .args([
            "for-each-ref",
            "--format=%(refname:short)",
            "refs/heads/rigger/u/",
        ])
        .output();
    match out {
        Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout)
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .map(str::to_string)
            .collect(),
        _ => Vec::new(),
    }
}

/// Scan `scratch_root` (a filesystem read, no mutation) plus the given local `rigger/u/*`
/// branches for residue no live unit owns. `live_slugs` are the `<slug>` of live units and
/// `live_branches` their full branch names; `dead_slugs` are the `<slug>` of terminal units
/// (used only to disambiguate a `<live-slug>-<8hex>`-shaped worktree, see
/// `worktree_belongs_to_live`). Pure over its inputs, so it is testable against a temp
/// scratch dir with synthetic worktrees, caches, and shadow stores.
fn scan_residue(
    scratch_root: &Path,
    live_slugs: &std::collections::HashSet<String>,
    dead_slugs: &std::collections::HashSet<String>,
    local_unit_branches: &[String],
    live_branches: &std::collections::HashSet<String>,
) -> ResidueReport {
    let mut report = ResidueReport::default();
    if let Ok(entries) = std::fs::read_dir(scratch_root) {
        for entry in entries.flatten() {
            let Ok(ft) = entry.file_type() else { continue };
            if !ft.is_dir() {
                continue;
            }
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with("rigger-wt-") {
                if !worktree_belongs_to_live(&name, live_slugs, dead_slugs) {
                    report.worktrees.push((name, dir_size_bytes(&entry.path())));
                }
            } else if name == "target" || name == "cargo-target" {
                // A build cache directly under the scratch root - a shared/leftover target
                // dir the run never reclaims (Gap 14: orphaned build caches until a disk fills).
                report.caches.push((name, dir_size_bytes(&entry.path())));
            } else if let Some(slug) = name.strip_prefix(rigger::worktree::UNIT_CACHE_PREFIX) {
                // A per-unit build cache (`cargo-target-<slug>`, Gap 19). It is reclaimed with
                // its unit's worktree on BOTH the graceful (`Worktree::remove`) and crash
                // (`sweep_terminal`) paths, so it is residue ONLY when that worktree is no
                // longer live - a leftover a crash stranded between removing the worktree and
                // reclaiming the cache, or from an older run. A LIVE unit's cache is in use,
                // not residue. Mirror the worktree liveness check on the reconstructed
                // `rigger-wt-<slug>` name so the cache and its worktree stay in lockstep.
                let wt_name = format!("{}{slug}", rigger::worktree::UNIT_WORKTREE_PREFIX);
                if !worktree_belongs_to_live(&wt_name, live_slugs, dead_slugs) {
                    report.caches.push((name, dir_size_bytes(&entry.path())));
                }
            }
        }
    }
    // Shadow stores: any `events.db` anywhere under the scratch root (including inside a
    // worktree) - a store a misdirected courier can silently record into. Reported
    // regardless of the containing worktree's liveness, because the hazard is the store
    // itself (adv9-shadow-store-reopens-defect), not whether its worktree is in flight.
    for path in find_shadow_stores(scratch_root) {
        let rel = path
            .strip_prefix(scratch_root)
            .unwrap_or(&path)
            .to_string_lossy()
            .into_owned();
        let size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
        report.shadow_stores.push((rel, size));
    }
    for b in local_unit_branches {
        if !live_branches.contains(b) {
            report.branches.push(b.clone());
        }
    }
    report.worktrees.sort();
    report.caches.sort();
    report.shadow_stores.sort();
    report.branches.sort();
    report
}

/// Whether a scratch worktree dir named `name` (a `rigger-wt-...` basename) belongs to a
/// LIVE unit (so it is NOT residue). Matches BOTH the deterministic `rigger-wt-<slug>`
/// shape (spec 06 unit 4) and the legacy per-process `rigger-wt-<slug>-<8hex>` shape.
///
/// The per-process shape is ambiguous with a DEAD unit whose slug is itself
/// `<live-slug>-<8hex>`: e.g. a dead `foo-deadbeef` while `foo` is live owns a
/// deterministic `rigger-wt-foo-deadbeef` worktree that would otherwise decompose as
/// live-`foo` + uuid-`deadbeef` and be spared. `dead_slugs` (the current run's terminal
/// units) resolves it - an exact dead slug is its OWN (dead) unit's worktree, never a live
/// unit's per-process tail (adv-u6res-uuid8-tail-false-match), so it stays residue.
fn worktree_belongs_to_live(
    name: &str,
    live_slugs: &std::collections::HashSet<String>,
    dead_slugs: &std::collections::HashSet<String>,
) -> bool {
    let Some(rest) = name.strip_prefix("rigger-wt-") else {
        return false;
    };
    if dead_slugs.contains(rest) {
        return false;
    }
    live_slugs.iter().any(|slug| {
        rest == slug.as_str()
            || rest
                .strip_prefix(slug.as_str())
                .and_then(|s| s.strip_prefix('-'))
                .is_some_and(is_uuid8)
    })
}

/// Whether `s` is exactly 8 hex digits - the `uuid[..8]` suffix the conductor appends to a
/// per-process worktree dir name.
fn is_uuid8(s: &str) -> bool {
    s.len() == 8 && s.bytes().all(|b| b.is_ascii_hexdigit())
}

/// Every `events.db` under `root` (recursively) - the shadow stores Gap 14d surfaces. The
/// walk prunes build-cache / vcs / node dirs (which never hold an `events.db`) so it stays
/// cheap even beside a multi-gigabyte target dir, and it does not follow symlinks (an
/// `entry.file_type()` reflects the dirent, so a symlinked dir is neither descended nor
/// counted - no cycles).
fn find_shadow_stores(root: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let Ok(ft) = entry.file_type() else { continue };
            let name = entry.file_name();
            if ft.is_dir() {
                let n = name.to_string_lossy();
                // A per-unit build cache (`cargo-target-<slug>`, Gap 19) is pruned like the
                // shared `cargo-target`: it never holds a real `events.db`, and descending a
                // leaked multi-gigabyte cache would defeat this walk's cheap-beside-a-target
                // guarantee (adv-u3gap19-shadow-walk-descends-per-unit-caches).
                let pruned = matches!(
                    n.as_ref(),
                    "target" | "cargo-target" | "node_modules" | ".git"
                ) || n.starts_with(rigger::worktree::UNIT_CACHE_PREFIX);
                if !pruned {
                    stack.push(entry.path());
                }
            } else if ft.is_file() && name == std::ffi::OsStr::new("events.db") {
                found.push(entry.path());
            }
        }
    }
    found
}

/// Total size in bytes of every regular file under `path` (recursively). Best-effort: an
/// unreadable dir/entry is skipped so a residue size can never fail the report, and
/// symlinks are not followed (so no cycles). A non-existent path is `0`.
fn dir_size_bytes(path: &Path) -> u64 {
    let mut total = 0;
    let mut stack = vec![path.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let Ok(ft) = entry.file_type() else { continue };
            if ft.is_dir() {
                stack.push(entry.path());
            } else if ft.is_file() {
                if let Ok(md) = entry.metadata() {
                    total += md.len();
                }
            }
        }
    }
    total
}

/// A short human-readable size (`5.5G`, `12.0M`, `340.0K`, `18B`) for a residue line.
fn human_size(bytes: u64) -> String {
    const GB: u64 = 1 << 30;
    const MB: u64 = 1 << 20;
    const KB: u64 = 1 << 10;
    if bytes >= GB {
        format!("{:.1}G", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1}M", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1}K", bytes as f64 / KB as f64)
    } else {
        format!("{bytes}B")
    }
}

/// Render a [`ResidueReport`] as `rigger validate` stderr advisory lines - empty when
/// there is no residue (validate stays silent), otherwise a single `warning:`-prefixed
/// block with one indented, sized line per leftover so an operator sees what to reclaim.
fn format_residue(report: &ResidueReport) -> Vec<String> {
    if report.is_empty() {
        return Vec::new();
    }
    let mut msg = String::from(
        "warning: residue found under the scratch root (surfaced only - validate never \
         removes it):",
    );
    for (name, bytes) in &report.worktrees {
        msg.push_str(&format!(
            "\n  worktree with no live unit: {name} ({})",
            human_size(*bytes)
        ));
    }
    for (name, bytes) in &report.caches {
        msg.push_str(&format!(
            "\n  orphaned build cache: {name} ({})",
            human_size(*bytes)
        ));
    }
    for (path, bytes) in &report.shadow_stores {
        msg.push_str(&format!(
            "\n  shadow store: {path} ({})",
            human_size(*bytes)
        ));
    }
    for b in &report.branches {
        msg.push_str(&format!("\n  branch with no live unit: {b}"));
    }
    vec![msg]
}

/// What [`init_project`] did, PER ARTIFACT, so `rigger setup` / `rigger init` can
/// narrate exactly what changed and stay a silent no-op on a rerun that changed nothing
/// (spec 05, criterion 4: setup is re-runnable with no surprising output). The summary
/// is built from these fields ([`scaffold_summary_lines`]) so it can never claim a
/// scaffold action that was not performed - a gitignore-only repair reports only the
/// gitignore change (the honest-summary bar the loop already enforced on unit-5).
#[derive(Debug, Default)]
struct ScaffoldReport {
    /// True when this run newly wrote `.rigger/workflow.yml` (it was absent).
    wrote_workflow: bool,
    /// Agent files this run newly wrote (empty when they already existed).
    new_agents: Vec<String>,
    /// True when this run installed or updated the SessionStart hook in
    /// `.claude/settings.json` (false when the hook was already present unchanged).
    wrote_hook: bool,
    /// `.gitignore` patterns this run newly appended (empty when every machine-local
    /// pattern was already ignored or tracked).
    gitignore_added: Vec<String>,
    /// The durable project id this run newly MINTED into `.rigger/project.id` (spec 09),
    /// or `None` when the file already existed and was left untouched.
    minted_id: Option<String>,
}

impl ScaffoldReport {
    /// True when this run created or modified ANY scaffold artifact. False means the
    /// scaffold was already complete and this run left the tree byte-for-byte identical.
    fn changed(&self) -> bool {
        self.wrote_workflow
            || !self.new_agents.is_empty()
            || self.wrote_hook
            || !self.gitignore_added.is_empty()
            || self.minted_id.is_some()
    }
}

/// Scaffold a project idempotently, returning a [`ScaffoldReport`] of what actually
/// changed. Every step is a no-op when its artifact already exists and matches, so a
/// rerun on an initialized project changes nothing and reports `changed: false`.
fn init_project(root: &Path) -> Result<ScaffoldReport, Box<dyn std::error::Error>> {
    // 1. Scaffold .rigger/.
    let rigger_dir = root.join(RIGGER_DIR);
    let agents_dir = rigger_dir.join("agents");
    std::fs::create_dir_all(&agents_dir)?;
    let wrote_workflow = write_if_absent(&rigger_dir.join("workflow.yml"), SCAFFOLD_WORKFLOW)?;

    // 1b. Mint the durable project identity when absent (spec 09, Gap 20): a tracked
    // `.rigger/project.id` line so the identity survives directory renames and machine
    // moves instead of tracking the volatile directory basename. Deterministic from the
    // normalized `origin` URL when a remote exists (every clone mints the same id), random
    // otherwise. A present file is left untouched (`minted_id` stays `None`), so a rerun
    // never re-mints. A genuine write failure escalates (naming the artifact), never a
    // silent omission - identity is load-bearing.
    let id_path = rigger_dir.join(PROJECT_ID_FILE);
    let minted_id = if id_path.exists() {
        None
    } else {
        let id = mint_project_id(root);
        write_if_absent(&id_path, &format!("{id}\n"))?;
        Some(id)
    };

    // 2. Load the workflow to determine which agents are referenced, then only
    // scaffold those agents. This allows setup to skip scaffolding when the
    // workflow's referenced agents already exist (§05 setup hygiene).
    let referenced_agents = get_referenced_agent_ids(root).unwrap_or_default();

    // If the workflow references agents, scaffold only those. If it references
    // nothing (should not happen with a valid workflow), scaffold all defaults
    // for backward compatibility (empty repo case).
    let agents_to_scaffold: Vec<(&str, &str)> = if referenced_agents.is_empty() {
        SCAFFOLD_AGENTS.to_vec()
    } else {
        SCAFFOLD_AGENTS
            .iter()
            .filter(|(_, content)| {
                // Extract the agent id from the YAML frontmatter (id: xxx)
                if let Ok(def) = rigger::config::parse_agent(content.as_bytes()) {
                    referenced_agents.contains(&def.id)
                } else {
                    // If we can't parse it, skip it to avoid scaffolding invalid agents
                    false
                }
            })
            .copied()
            .collect()
    };

    let mut new_agents = Vec::new();
    for (file, content) in &agents_to_scaffold {
        // Report only NEWLY-written agents; an existing agent is kept silently, so a
        // rerun scaffolds nothing new (the skip-scaffolding hygiene of §05). A genuine
        // write failure escalates (naming the artifact), never a silent omission.
        if write_if_absent(&agents_dir.join(file), content)? {
            new_agents.push(file.to_string());
        }
    }

    // 3. Install the SessionStart hook, merging into any existing settings. Write ONLY
    // when the merge actually changes settings.json, so a rerun (the hook already
    // present) leaves the file - and its mtime - untouched.
    let claude_dir = root.join(".claude");
    std::fs::create_dir_all(&claude_dir)?;
    let settings_path = claude_dir.join("settings.json");
    let existing = std::fs::read(&settings_path).unwrap_or_default();
    let merged = hooks::install_session_start(&existing, "rigger prime")?;
    let wrote_hook = merged != existing;
    if wrote_hook {
        std::fs::write(&settings_path, &merged)?;
    }

    // 4. Write .gitignore entries for machine-local installs, the always-on dash's runtime
    // breadcrumbs, and the per-machine store-connection secret file, when they are not already
    // ignored or tracked. `.claude/` and `.rigger/shim/` are the machine-local installs;
    // `.rigger/dash.url` and `.rigger/dash.marker` are the dash's discoverability breadcrumbs
    // (spec 39) - left untracked-and-not-ignored they get swept into a unit worktree's commit by
    // `git add` and then collide with the live dash's rewrites when the conductor merges the unit
    // ("untracked working tree files would be overwritten"). `.rigger/dash.attempt` (spec 69,
    // round-8 fix) is a THIRD runtime breadcrumb of the identical shape - `ensure_run_dashboard`
    // / `start_run_dashboard` rewrite it on every dash-ensure call exactly as they rewrite the
    // other two - so it collides the same way if left untracked-and-not-ignored, and is ignored
    // here for the same reason. `.rigger/store.conn` is the store resolver's per-machine secret
    // file (spec 48 rung 3): it carries the connection string's credentials, so it is git-ignored
    // BY CONSTRUCTION - a developer's credentials can never ride a committed file. Record WHICH
    // patterns were appended so the summary reports the real gitignore change and nothing it did
    // not do.
    let mut gitignore_added = Vec::new();
    for pattern in [
        ".claude/",
        ".rigger/shim/",
        ".rigger/dash.url",
        ".rigger/dash.marker",
        ".rigger/dash.attempt",
        ".rigger/store.conn",
    ] {
        if write_gitignore_entries(root, pattern)? {
            gitignore_added.push(pattern.to_string());
        }
    }

    Ok(ScaffoldReport {
        wrote_workflow,
        new_agents,
        wrote_hook,
        gitignore_added,
        minted_id,
    })
}

/// Print the empty-repo scaffold pointer: where to get a real starting agent fleet
/// (the agency-agents collection) and how to author agents (the handbook chapter).
/// `rigger init` / `rigger setup` call this ONLY when the default fleet was actually
/// scaffolded this run - per the weave of units 4 and 8, the signal is a non-empty
/// [`ScaffoldReport::new_agents`] (spec 05 done-when line 57, clause 2) - never on a
/// re-run that keeps an existing fleet.
fn print_scaffold_pointer() {
    println!(
        "next: this scaffolded a minimal starter fleet. For a fuller set, clone the \
         agency-agents collection from https://github.com/msitarzewski/agency-agents and \
         import it with `rigger setup --agents <dir>`, or author your own following the \
         handbook chapter at docs/handbook/authoring-agents.md"
    );
}

/// Print the end-of-setup orientation block: the three ways to drive a run, so an operator
/// who just provisioned the project discovers them without grepping the docs (spec 19a unit
/// 2). Names the blessed native `/rigger <spec>` path (chosen from the `/workflows` menu),
/// the read-only dashboard (`rigger dash`, on `127.0.0.1:<dash::DEFAULT_PORT>` - the port is
/// single-sourced from the constant so this line and the fixture that asserts it cannot
/// drift), and `rigger workflow` / `rigger run` as the headless twins that drive the same
/// loop without an editor. Output text only; the dashboard's runtime behavior is spec 19b's.
fn print_orientation() {
    println!("to drive a run, three ways:");
    println!(
        "  /rigger <spec>                 the blessed native path (choose it from /workflows)"
    );
    println!(
        "  rigger dash                    a read-only live dashboard at http://127.0.0.1:{}",
        dash::DEFAULT_PORT
    );
    println!(
        "  rigger workflow / rigger run   the headless twins (the same loop without an editor)"
    );
}

/// Write a .gitignore entry for the given pattern if it is not already an explicit line
/// or a tracked path, returning whether it APPENDED an entry (`true`) or left `.gitignore`
/// untouched (`false`). Idempotent: a rerun finds the exact line already present and is a
/// no-op, so setup never pollutes `.gitignore` with duplicates.
///
/// Deliberately does NOT consult `git check-ignore` to skip a path a BROADER rule already
/// covers. `git check-ignore` resolves ignores against machine-local global sources
/// (`core.excludesFile`, `~/.config/git/ignore`, `.git/info/exclude`), so letting it decide
/// what to append would make the COMMITTED `.gitignore` contingent on the setup-runner's
/// machine: an operator whose global excludes already list `.rigger/` would ship a
/// `.gitignore` MISSING the `.rigger/dash.url` / `.rigger/dash.marker` lines, and a teammate
/// or CI cloning with a clean HOME would then let `git add` sweep the dash breadcrumbs into a
/// unit commit - the exact collision spec 46 criterion 1 exists to prevent. The committed
/// file must be self-contained and portable, so we append the explicit line whenever it is
/// absent. A redundant-but-correct per-file line in a repo whose OWN rules already ignore a
/// broader path (e.g. `.rigger/`) is harmless; the exact-line check above still guarantees
/// idempotency, and the file stays machine-independent.
fn write_gitignore_entries(root: &Path, pattern: &str) -> Result<bool, Box<dyn std::error::Error>> {
    let gitignore_path = root.join(".gitignore");
    let normalized_pattern = pattern.trim_end_matches('/');

    // Already an explicit line in .gitignore: a no-op. This exact-line check is the
    // idempotency guarantee, and it reads ONLY the repo's own committed `.gitignore` (never
    // machine-local global git config), so it holds even OUTSIDE a git repo and never makes
    // the committed file depend on the runner's machine.
    let current = std::fs::read_to_string(&gitignore_path).unwrap_or_default();
    if current
        .lines()
        .any(|line| line.trim() == normalized_pattern)
    {
        return Ok(false); // Already in .gitignore
    }

    // Check if the path is tracked in git (it should not be, as .claude/ and .rigger/shim/
    // are machine-local and should never be committed). This is just a safety check.
    let is_tracked = Command::new("git")
        .args(["ls-files"])
        .current_dir(root)
        .output()
        .ok()
        .map(|output| {
            String::from_utf8_lossy(&output.stdout)
                .lines()
                .any(|line| line.starts_with(&format!("{}/", normalized_pattern)))
        })
        .unwrap_or(false);

    if is_tracked {
        return Ok(false); // Path is tracked, don't ignore it
    }

    // Append to .gitignore
    use std::io::Write;
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&gitignore_path)?;

    // Add a newline before the entry if the file is not empty and doesn't end with newline
    if !current.is_empty() && !current.ends_with('\n') {
        writeln!(file)?;
    }

    writeln!(file, "{}", normalized_pattern)?;

    Ok(true)
}

/// Get all agent IDs referenced in the workflow at <root>/.rigger/workflow.yml.
/// Returns an empty set if the workflow cannot be loaded or parsed.
fn get_referenced_agent_ids(
    root: &Path,
) -> Result<std::collections::HashSet<String>, Box<dyn std::error::Error>> {
    use std::collections::HashSet;

    let workflow_path = root.join(RIGGER_DIR).join("workflow.yml");
    if !workflow_path.exists() {
        return Ok(HashSet::new());
    }

    let content = std::fs::read_to_string(&workflow_path)?;
    let workflow: rigger::config::Workflow = serde_yaml::from_str(&content)?;

    let mut ids = HashSet::new();

    // Add agents from defaults.review
    for agent_id in workflow.defaults.review.agent_ids() {
        ids.insert(agent_id);
    }

    // Add agents from all stages
    for stage in workflow.stages.values() {
        for agent_id in stage.agent_ids() {
            ids.insert(agent_id);
        }
    }

    Ok(ids)
}

/// The per-artifact summary lines for a scaffold run: ONE line for each artifact this
/// run actually (re)wrote, and nothing for artifacts left untouched. This is the single
/// authority for the setup/init summary, so it can never emit a blanket "scaffolded
/// workflow + agents + hook" claim on a run that only repaired one artifact - a
/// gitignore-only repair yields only the gitignore line (spec 05, criterion 4: prints
/// nothing surprising; the honest-summary bar of adj-unit5). Pure so it is unit-testable
/// without capturing stdout.
fn scaffold_summary_lines(report: &ScaffoldReport) -> Vec<String> {
    let mut lines = Vec::new();
    if let Some(id) = &report.minted_id {
        lines.push(format!(
            "minted the durable project identity in .rigger/{PROJECT_ID_FILE}: {id} \
             (commit it so a rename never orphans this project's history)"
        ));
    }
    if report.wrote_workflow {
        lines.push("scaffolded .rigger/workflow.yml".to_string());
    }
    if !report.new_agents.is_empty() {
        lines.push(format!(
            "scaffolded .rigger/agents/{{{}}}",
            report.new_agents.join(", ")
        ));
    }
    if report.wrote_hook {
        lines.push(
            "installed a Claude Code SessionStart hook in .claude/settings.json (it runs \
             `rigger prime`)"
                .to_string(),
        );
    }
    if !report.gitignore_added.is_empty() {
        lines.push(format!(
            "added .gitignore entries so machine-local installs stay untracked: {}",
            report.gitignore_added.join(", ")
        ));
    }
    lines
}

fn cmd_init() -> Res {
    let report = init_project(Path::new("."))?;
    let lines = scaffold_summary_lines(&report);
    if lines.is_empty() {
        // Re-runnable: an already-initialized project is a silent no-op with a plain
        // confirmation, never a re-narration of every file left in place.
        println!("rigger init: already initialized; nothing to scaffold");
    } else {
        for line in lines {
            println!("{line}");
        }
    }
    // The starter-fleet pointer fires exactly when default agents were NEWLY
    // scaffolded (the empty-repo path): units 4 + 8 woven - the per-artifact report's
    // `new_agents` IS the scaffolded-new signal.
    if !report.new_agents.is_empty() {
        print_scaffold_pointer();
    }
    Ok(())
}

/// The directory the per-project JS driver is provisioned into, relative to the
/// project root: `<root>/.rigger/shim/`. `rigger setup` writes the embedded runtime
/// files here and installs their npm deps; `rigger workflow` runs `shim.mjs` from
/// here.
fn shim_dir(root: &Path) -> std::path::PathBuf {
    root.join(RIGGER_DIR).join("shim")
}

/// What an install step did to a file it manages under the project root (the `/rigger`
/// workflow or the `using-rigger` skill), so `rigger setup` can REPORT a refresh but stay
/// a silent no-op when nothing drifted (spec 05, criterion 4: setup is re-runnable - it
/// detects and refreshes a drifted installed file, reports the refresh, and changes
/// nothing when the file is already current).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InstallOutcome {
    /// No file was installed before; the managed copy was written fresh.
    Installed,
    /// An installed file had DRIFTED from the managed copy (e.g. an older `rigger` build
    /// wrote it, or a hand-edit) and was refreshed to match this binary.
    Refreshed,
    /// The installed file already matched the managed copy byte-for-byte, so nothing was
    /// written - a rerun changes nothing (not even the file's mtime, which the grounder
    /// keys off).
    AlreadyCurrent,
}

/// Write `contents` to `path` ONLY when it is absent or differs, returning [what it
/// did](InstallOutcome). A byte-identical file is left untouched so a `rigger setup` rerun
/// is a true no-op: rewriting identical content would still bump the file's mtime, an
/// observable side effect (the grounder's staleness gate keys off mtime). Parent
/// directories are created so a fresh checkout installs cleanly. This is the SINGLE
/// authority for the compare-then-write-if-changed install step, shared by
/// [`install_workflow`] and [`install_skills`] so every install cannot drift in how it
/// detects and reports a no-op.
fn install_file_if_changed(
    path: &Path,
    contents: &[u8],
) -> Result<InstallOutcome, Box<dyn std::error::Error>> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let existed = path.exists();
    if existed && std::fs::read(path)? == contents {
        return Ok(InstallOutcome::AlreadyCurrent);
    }
    std::fs::write(path, contents)?;
    Ok(if existed {
        InstallOutcome::Refreshed
    } else {
        InstallOutcome::Installed
    })
}

/// Install (or refresh) the native `/rigger` Claude Code workflow at
/// `<root>/.claude/workflows/rigger.js`, returning [what it did](InstallOutcome).
///
/// It COMPARES the installed file against the embedded [`RIGGER_WORKFLOW`] (via
/// [`install_file_if_changed`]) and writes ONLY when the file is absent (a fresh install)
/// or has drifted (a stale copy from an older `rigger` build): an up-to-date workflow is
/// left untouched so a `rigger setup` rerun is a true no-op. A drifted file is overwritten
/// so an upgrade refreshes the workflow to match the binary - the workflow and the
/// conductor / CLI it drives stay the same build. Claude Code auto-discovers `.js` here,
/// so the user can run `/rigger <spec>` immediately, with no registration. Rooted at
/// `root` so it is testable against a temp dir. The installed path is
/// [`workflow_path`]`(root)`.
fn install_workflow(root: &Path) -> Result<InstallOutcome, Box<dyn std::error::Error>> {
    let outcome = install_file_if_changed(&workflow_path(root), RIGGER_WORKFLOW.as_bytes())?;
    // Record which build wrote this workflow so the drift diagnostic can later name WHICH
    // side is stale (spec 18, criterion 9). Written beside the workflow and ONLY when the
    // workflow itself was (re)written, so an `AlreadyCurrent` rerun stays a true no-op that
    // does not even touch the file's mtime.
    if outcome != InstallOutcome::AlreadyCurrent {
        std::fs::write(workflow_provenance_path(root), BUILD_PROVENANCE)?;
    }
    Ok(outcome)
}

/// Where `rigger docs` writes a registry skill's rendered content, relative to the project
/// root: `skills/<name>/SKILL.md`. Committed and drift-checked (spec 20, unit 2; spec 68,
/// criterion 1) and installed into `.claude/skills/` by `rigger setup` (see
/// [`skill_install_path`]) - one naming convention, one function, shared by `rigger docs`,
/// the docs-drift gate, and the CI-lane guard, so the three can never disagree on where a
/// skill's committed source lives.
fn skill_source_rel(name: &str) -> String {
    format!("skills/{name}/SKILL.md")
}

/// Where `rigger setup` installs a registry skill, relative to the project root:
/// `<root>/.claude/skills/<name>/SKILL.md`. Claude Code auto-discovers skills under
/// `.claude/skills/`, so the installed file is loadable the moment it is written - a file
/// DISTINCT from the `/rigger` workflow at [`workflow_path`] (the workflow RUNS the loop;
/// a skill tells an agent WHEN and HOW) and from the committed, drift-checked source at
/// [`skill_source_rel`] (which `rigger docs` renders and `rigger validate` re-renders
/// against). Rooted at `root` so it is testable against a temp dir.
fn skill_install_path(root: &Path, name: &str) -> std::path::PathBuf {
    root.join(".claude")
        .join("skills")
        .join(name)
        .join("SKILL.md")
}

/// Where a repo declares its skill-registry project overlay, relative to the project root:
/// `<root>/.rigger/docs-overlay.yml`. Optional - an absent file means every installed skill
/// carries only the shared defaults.
fn docs_overlay_path(root: &Path) -> std::path::PathBuf {
    root.join(RIGGER_DIR).join("docs-overlay.yml")
}

/// A per-repo overlay that adds THIS repository's specifics to every INSTALLED registry
/// skill WITHOUT editing the shared discipline source (overlay honored per entry - spec
/// 68, criterion 1). The two drift-prone facts a downstream project may differ on - the
/// base branch a run anchors on and where the repo keeps its specs - are read from
/// [`docs_overlay_path`] and merged onto the code-derived [`docs_context`] before each
/// skill is rendered and installed. Both fields are OPTIONAL:
/// an absent overlay file, or an absent field, leaves the shared default in place, so the
/// overlay only ever ADDS repo specifics and never restates the shared discipline. Unknown
/// keys are rejected so a typo fails loudly rather than being silently ignored.
#[derive(Debug, Default, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct DocsOverlay {
    /// This repo's base branch, overriding [`DEFAULT_BASE_REF`] in the rendered skill.
    #[serde(default)]
    base_ref: Option<String>,
    /// Where this repo keeps its specs, overriding [`DEFAULT_SPECS_LOCATION`].
    #[serde(default)]
    specs_location: Option<String>,
}

impl DocsOverlay {
    /// Merge this overlay onto `ctx`, overriding ONLY the fields the overlay declares, so a
    /// repo customizes just the facts it differs on and inherits the shared defaults for
    /// the rest.
    fn apply(&self, ctx: &mut rigger::docs::DocsContext) {
        if let Some(base) = &self.base_ref {
            ctx.base_ref = base.clone();
        }
        if let Some(specs) = &self.specs_location {
            ctx.specs_location = specs.clone();
        }
    }
}

/// Read the project's [`DocsOverlay`] from [`docs_overlay_path`]. An ABSENT file is the
/// common case and yields an empty overlay (no overrides), so a repo that wants only the
/// shared discipline writes no overlay. A PRESENT but malformed overlay is a LOUD error
/// naming the file, never a silent skip that would install a skill missing the repo
/// specifics the author asked for.
fn read_docs_overlay(root: &Path) -> Result<DocsOverlay, Box<dyn std::error::Error>> {
    let path = docs_overlay_path(root);
    let raw = match std::fs::read_to_string(&path) {
        Ok(raw) => raw,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(DocsOverlay::default()),
        Err(e) => return Err(format!("setup: reading {}: {e}", path.display()).into()),
    };
    serde_yaml::from_str(&raw)
        .map_err(|e| format!("setup: {} is not a valid docs overlay: {e}", path.display()).into())
}

/// Install (or refresh) EVERY skill in [`rigger::docs::skill_registry`] under `root`,
/// returning each entry's `(name, outcome)` in registry order (spec 68, criterion 1:
/// generalizes the single-skill `install_skill` seam over the whole registry). ONE
/// code-derived context is built and this repo's [overlay](read_docs_overlay) merged onto
/// it ONCE, then EVERY entry renders against that same context and installs via
/// [`install_file_if_changed`] - so a downstream repo's base branch and specs location
/// appear in every installed skill without anyone editing the shared discipline source,
/// and adding an entry to the registry is the ONLY step needed to make a new skill
/// install; this loop needs no per-skill edit. Like [`install_workflow`], each entry
/// writes ONLY when its file is absent or has drifted, so a `rigger setup` rerun on an
/// up-to-date repo is a true no-op that does not even move a file's mtime.
fn install_skills(
    root: &Path,
) -> Result<Vec<(&'static str, InstallOutcome)>, Box<dyn std::error::Error>> {
    let mut ctx = docs_context();
    read_docs_overlay(root)?.apply(&mut ctx);
    let mut outcomes = Vec::new();
    for entry in rigger::docs::skill_registry() {
        let rendered = entry.render(&ctx);
        let outcome =
            install_file_if_changed(&skill_install_path(root, entry.name), rendered.as_bytes())?;
        outcomes.push((entry.name, outcome));
    }
    Ok(outcomes)
}

/// The comment line that OPENS rigger's managed block inside a `pre-commit` hook. It is a
/// shell comment (inert) AND the sentinel [`compose_precommit`] uses to find its own block
/// so a rerun refreshes exactly that block and never duplicates it - and so a chained
/// hook's own lines, which live outside the sentinels, are never disturbed.
const PRECOMMIT_BEGIN: &str = "# >>> BEGIN rigger docs pre-commit (managed - do not edit) >>>";
/// The comment line that CLOSES rigger's managed block (see [`PRECOMMIT_BEGIN`]).
const PRECOMMIT_END: &str = "# <<< END rigger docs pre-commit (managed) <<<";

/// Render rigger's managed `pre-commit` block: the sentinel-bounded shell that checks the
/// code-derived docs (`rigger docs`) against what is ALREADY STAGED, so a commit that changes
/// a documented code fact can only land carrying freshly rendered docs - never a silently
/// rewritten stand-in for them. Four hard safety invariants are baked into the SCRIPT:
///
/// - COMPARISON SCOPE: it reads and compares ONLY the `using-rigger` skill and the handbook
///   chapter by explicit path (built from [`skill_source_rel`]`("using-rigger")` /
///   [`HANDBOOK_DISCIPLINE_REL`] so the scope can never drift from what [`write_docs`]
///   writes for those two outputs), never any other working-tree file. The hook's
///   self-hosting scope stays these two (spec 68, criterion 1 generalizes the REGISTRY,
///   not this pre-existing fast commit-time check); every registry entry, including
///   `planning-a-spec`, is still covered by the docs-drift GATE below (`rigger validate`).
/// - NEVER REWRITES SILENTLY (spec 70): it never runs `git add` on the docs - staging what a
///   commit carries is the operator's job, always. When the fresh render DIFFERS from what is
///   already staged, the block REFUSES the commit (`exit 1`), naming the drifted files, the
///   rendering binary's path AND its build provenance (`command -v rigger` / `rigger
///   version`), and the two remedies (re-render with the tree-built binary, or reinstall). A
///   binary that is older than the tree - whatever happens to be first on PATH - can then
///   never launder a stale re-render into a commit by staging it over the operator's correctly
///   staged content; the worst it can do is block the commit and name itself as the suspect.
///   A MATCHING render changes nothing and the block falls through to `true`, exactly as
///   before this invariant existed.
/// - SELF-HOSTING SCOPE: it checks ONLY when the repo ALREADY TRACKS these docs (`git
///   ls-files --error-unmatch`), i.e. rigger's own self-hosting repo. These are rigger's OWN
///   committed docs and an operator project never carries them (see [`docs_drift`]: their
///   absence is not drift), so in an operator repo the block is INERT - it does not even run
///   `rigger docs`, creates nothing, and refuses nothing, so an ordinary operator commit is
///   never forced to carry rigger's internal discipline docs. The same hook is installed
///   everywhere (it cannot know at install time whether the repo tracks the docs); this
///   commit-time tracked check is what keeps it correct in both a self-hosting and an
///   operator repo.
/// - GRACEFUL DEGRADE ON UNAVAILABILITY: a missing or failing `rigger` (it cannot even attempt
///   a render, so it has nothing to compare) warns to stderr and lets the commit proceed - the
///   spec-20 `rigger validate` / CI drift check is the hard backstop for that case. This is
///   the ONLY case the block lets a drifted commit through; once a render succeeds, a mismatch
///   is a hard stop, not a warning.
///
/// The trailing `true` on the no-drift path (rather than `exit 0`) keeps the block cooperative:
/// it contributes a zero exit when it is the last block, without hard-terminating any block a
/// future tool might append after it. The `exit 1` on a detected drift is deliberately NOT
/// cooperative - it must abort the whole hook (including anything chained after it), because a
/// commit that is about to be refused should not go on to run further gates. The hook invokes
/// `rigger` BY NAME (relying on PATH), like the SessionStart hook runs `rigger prime`, so it
/// stays portable across a team's clones (no absolute path to one developer's binary).
fn precommit_block() -> String {
    // A raw-string template so the shell indentation is exact and readable; the two doc
    // paths and the sentinels are injected from their single-source consts.
    const TEMPLATE: &str = r#"__BEGIN__
# Check rigger's code-derived docs against a fresh render before THIS commit lands, so a
# commit that changes a documented code fact can only land carrying freshly rendered docs.
# SAFE to share: it reads and compares ONLY the two rendered outputs (never other working-tree
# files) and NEVER stages anything itself - staging is always the operator's own act. It acts
# ONLY where the repo already tracks those docs (inert in an operator project that does not
# carry them). A missing or failing `rigger` warns and lets the commit proceed (`rigger
# validate` / the CI drift check is the hard backstop for that case) - but once a render
# succeeds and DIFFERS from what is already staged, the commit is REFUSED rather than
# silently rewritten: a stale binary on PATH must never launder its own re-render into a
# commit over the operator's correctly staged content.
#
# BINARY SELECTION (spec 75): prefer a `rigger` BUILT FROM THIS TREE over whatever happens to
# sit first on PATH, so a worktree whose code legitimately changes a rendered fact renders with
# a binary that actually reflects that change instead of deadlocking against a stale PATH
# install. Candidate order, most authoritative first: the env-provided cargo target dir
# (release then debug), this working tree's own local target (release then debug), this
# worktree's own unit-derived scratch cargo-target - its unit is read from the worktree
# directory name, `rigger-wt-<unit>` (release then debug), the run's shared step-cache target
# (debug only - unit gates build the debug profile only), and finally PATH. SAFE-CLOSED: a
# wrong candidate can only ever convert a false refusal into a pass when its render genuinely
# matches what is already staged - never the reverse - so this can only make the hook MORE
# correct, never less.
git_common_dir=$(git rev-parse --git-common-dir 2>/dev/null)
worktree_top=$(git rev-parse --show-toplevel 2>/dev/null)
worktree_base=$(basename "$worktree_top" 2>/dev/null)
unit=
case "$worktree_base" in
    rigger-wt-*) unit="${worktree_base#rigger-wt-}" ;;
esac
unit_release=
unit_debug=
shared_debug=
if [ -n "$git_common_dir" ]; then
    if [ -n "$unit" ]; then
        unit_release="$git_common_dir/../.rigger/tmp/cargo-target-$unit/release/rigger"
        unit_debug="$git_common_dir/../.rigger/tmp/cargo-target-$unit/debug/rigger"
    fi
    shared_debug="$git_common_dir/../.rigger/tmp/cargo-target/debug/rigger"
fi
rigger_bin=
for candidate in \
    "${CARGO_TARGET_DIR:+$CARGO_TARGET_DIR/release/rigger}" \
    "${CARGO_TARGET_DIR:+$CARGO_TARGET_DIR/debug/rigger}" \
    "./target/release/rigger" \
    "./target/debug/rigger" \
    "$unit_release" \
    "$unit_debug" \
    "$shared_debug" \
; do
    if [ -n "$candidate" ] && [ -x "$candidate" ]; then
        rigger_bin="$candidate"
        break
    fi
done
if [ -z "$rigger_bin" ] && command -v rigger >/dev/null 2>&1; then
    rigger_bin=$(command -v rigger)
fi
if [ -n "$rigger_bin" ]; then
    # Only check in a repo that ALREADY TRACKS these rendered docs (rigger's own self-hosting
    # repo). An operator project never carries them, so leave it untouched.
    tracked=
    untracked=
    for doc in "__SKILL__" "__HANDBOOK__"; do
        if git ls-files --error-unmatch -- "$doc" >/dev/null 2>&1; then
            tracked="${tracked:+$tracked }$doc"
        else
            untracked=1
        fi
    done
    # Check ONLY when EVERY rendered output is already tracked (rigger's own self-hosting
    # repo). If any is untracked - an operator project, or a partial-tracking state - stay inert
    # so `rigger docs` never runs and never creates a stray untracked doc file the operator did
    # not ask for.
    if [ -z "$untracked" ] && [ -n "$tracked" ]; then
        if "$rigger_bin" docs >/dev/null 2>&1; then
            # `rigger docs` just wrote a fresh render into the working tree. Compare it against
            # what is ALREADY STAGED (the index) - never stage the fresh render itself. A doc
            # that differs is drifted: either the staged content is genuinely stale, or this
            # invocation's `rigger` is stale relative to the tree; the hook cannot tell which,
            # so it refuses rather than guessing.
            drifted=
            for doc in $tracked; do
                if [ -f "$doc" ] && ! git diff --quiet -- "$doc" 2>/dev/null; then
                    drifted="${drifted:+$drifted }$doc"
                fi
            done
            if [ -n "$drifted" ]; then
                rigger_prov=$("$rigger_bin" version 2>/dev/null)
                echo "rigger: pre-commit: refusing to commit - the committed docs have drifted from a fresh render: $drifted" 1>&2
                echo "rigger: pre-commit: rendering binary: $rigger_bin ($rigger_prov)" 1>&2
                echo 'rigger: pre-commit: nothing was staged. Fix by either re-rendering with the tree-built binary (rigger docs, then git add the result), or reinstalling rigger so PATH points at a binary built from this tree' 1>&2
                exit 1
            fi
        else
            echo 'rigger: pre-commit: rigger docs failed; committing without regenerated docs (rigger validate is the backstop)' 1>&2
        fi
    fi
else
    echo 'rigger: pre-commit: no rigger binary found (checked the tree build output and PATH); skipping docs regeneration (rigger validate is the backstop)' 1>&2
fi
# Best-effort on unavailability only (the drift check is the hard backstop for that case): a
# matching render (or a `rigger` that could not even attempt one) falls through to here and
# never blocks a commit. A DETECTED drift already `exit 1`'d above and never reaches this line.
true
__END__
"#;
    TEMPLATE
        .replace("__BEGIN__", PRECOMMIT_BEGIN)
        .replace("__END__", PRECOMMIT_END)
        .replace("__SKILL__", &skill_source_rel("using-rigger"))
        .replace("__HANDBOOK__", HANDBOOK_DISCIPLINE_REL)
}

/// Find the byte offset of the first occurrence of `needle` in `haystack`, or `None`. Lets the
/// byte-level composer locate the ASCII sentinels (and the shebang's newline) inside a
/// pre-commit hook that may not be valid UTF-8, so it can refresh/chain at the byte level
/// without ever clobbering the existing hook.
fn find_bytes(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || needle.len() > haystack.len() {
        return None;
    }
    haystack.windows(needle.len()).position(|w| w == needle)
}

/// Byte-level core of the pre-commit composer (`compose_precommit` is a thin, test-only UTF-8
/// wrapper over it). Composing on BYTES (not `str`) is what keeps the
/// non-clobbering guarantee for a pre-existing pre-commit hook that is NOT valid UTF-8 (a
/// compiled/binary hook, or one carrying non-UTF-8 bytes): its bytes are preserved verbatim and
/// rigger's block is chained onto them, never replaced by a fresh script
/// (d24-2-nonutf8-byte-compose-no-clobber). Given the CURRENT hook bytes (or `None` when absent):
///
/// - ABSENT -> a fresh `#!/bin/sh` script carrying rigger's managed block.
/// - EXISTING WITH the sentinel-marked block -> the sentinel-bounded region is REPLACED in place
///   with the current block, every byte outside the sentinels preserved. This makes composing
///   IDEMPOTENT (re-composing an installed hook is a fixed point) and refreshes a stale block
///   from an older build without duplicating it.
/// - EXISTING WITHOUT the block -> rigger's block is inserted right AFTER the shebang line (or
///   at the very top when there is no shebang), i.e. BEFORE the existing hook body, so it runs
///   first. rigger's block ends in a bare `true` (never `exit`), so the existing hook still runs
///   after it and BOTH run - even when the existing hook ends in a terminal `exit 0` (the modal
///   hand-written/sample shape). Appending AFTER such a hook would silently shadow rigger's
///   block and skip the docs regeneration (d24-11 / d24-2-prepend-fixes-terminal-shadow).
fn compose_precommit_bytes(existing: Option<&[u8]>) -> Vec<u8> {
    let block = precommit_block();
    let block = block.as_bytes();
    let Some(existing) = existing else {
        let mut out = b"#!/bin/sh\n".to_vec();
        out.extend_from_slice(block);
        return out;
    };
    // Refresh the managed region in place when a well-formed (begin-before-end) block is
    // already present, preserving every byte on both sides of the sentinels.
    if let (Some(start), Some(end_start)) = (
        find_bytes(existing, PRECOMMIT_BEGIN.as_bytes()),
        find_bytes(existing, PRECOMMIT_END.as_bytes()),
    ) {
        if start < end_start {
            let end = end_start + PRECOMMIT_END.len();
            let before = &existing[..start];
            // `block` already ends with a newline, so drop the newline that followed the old
            // end sentinel to avoid a blank line creeping in on each refresh.
            let after = existing[end..]
                .strip_prefix(b"\n")
                .unwrap_or(&existing[end..]);
            let mut out = Vec::with_capacity(before.len() + block.len() + after.len());
            out.extend_from_slice(before);
            out.extend_from_slice(block);
            out.extend_from_slice(after);
            return out;
        }
    }
    // No block yet: chain by inserting rigger's block right after the shebang line, so a
    // terminal existing hook cannot shadow it.
    let insert_at = if existing.starts_with(b"#!") {
        match find_bytes(existing, b"\n") {
            Some(nl) => nl + 1,
            // A shebang with no trailing newline (degenerate single line): the whole file is the
            // shebang, so append the block after a newline - there is no body to shadow it.
            None => {
                let mut out = existing.to_vec();
                out.push(b'\n');
                out.extend_from_slice(block);
                return out;
            }
        }
    } else {
        // No shebang: prepend the block at the very top, preserving the existing content after.
        0
    };
    let mut out = Vec::with_capacity(existing.len() + block.len());
    out.extend_from_slice(&existing[..insert_at]);
    out.extend_from_slice(block);
    out.extend_from_slice(&existing[insert_at..]);
    out
}

/// Compose the `pre-commit` hook to install, given the CURRENT hook content (or `None` when
/// absent). PURE and filesystem-free (mirroring [`hooks::install_session_start`]) so the
/// idempotency and non-clobbering-chaining behavior is unit-testable without a real `.git`. A
/// thin UTF-8 wrapper over [`compose_precommit_bytes`], which holds the single composing
/// authority (production, including the non-UTF-8 install path, goes straight through the
/// byte core). Test-only: it exists so the idempotency / non-clobbering / prepend-chaining
/// behavior reads cleanly as `str` in the unit tests.
#[cfg(test)]
fn compose_precommit(existing: Option<&str>) -> String {
    let bytes = compose_precommit_bytes(existing.map(str::as_bytes));
    // UTF-8 in (a `str` existing hook plus the `str` block) yields UTF-8 out: every split point
    // is an ASCII sentinel/newline offset (a char boundary) or a slice endpoint, so this holds.
    String::from_utf8(bytes).expect("composing UTF-8 hook parts yields UTF-8")
}

/// The git hooks directory for `root`, resolved robustly via `git rev-parse --git-path
/// hooks` (which honors `core.hooksPath` and a worktree's `.git`-file indirection) and
/// falling back to `<root>/.git/hooks` when git cannot be consulted. A relative path git
/// prints is resolved against `root` so the caller gets an absolute-enough path to write to.
fn git_hooks_dir(root: &Path) -> std::path::PathBuf {
    let resolved = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["rev-parse", "--git-path", "hooks"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .filter(|s| !s.is_empty());
    match resolved {
        Some(p) => {
            let p = Path::new(&p);
            if p.is_absolute() {
                p.to_path_buf()
            } else {
                root.join(p)
            }
        }
        None => root.join(".git").join("hooks"),
    }
}

/// Install (or refresh) rigger's docs-checking `pre-commit` hook under `root`,
/// returning [what it did](InstallOutcome). The FS-facing wrapper around the pure
/// [`compose_precommit_bytes`]: it reads the current `pre-commit` (if any) AS BYTES, composes
/// the merged hook, and writes it ONLY when the merge changes something - so a `rigger setup`
/// rerun on an already-installed hook is a true no-op that does not even move the file's mtime
/// (the no-op-when-unchanged discipline of [`install_file_if_changed`], applied to a composer
/// that CHAINS rather than overwrites). The written hook is marked executable so git will
/// run it. Non-destructive by construction: an existing pre-commit hook is preserved - rigger's
/// block is chained in (inserted after the shebang, before the existing body), never clobbered -
/// and reading BYTES rather than a UTF-8 string keeps that guarantee even for a non-UTF-8 hook.
fn install_precommit_hook(root: &Path) -> Result<InstallOutcome, Box<dyn std::error::Error>> {
    let hooks_dir = git_hooks_dir(root);
    std::fs::create_dir_all(&hooks_dir)?;
    let hook_path = hooks_dir.join("pre-commit");
    let existed = hook_path.exists();
    // Read the current hook as BYTES so a non-UTF-8 or otherwise unreadable existing hook is
    // preserved and chained rather than clobbered by a fresh script (the compose is byte-level;
    // d24-2-nonutf8-byte-compose-no-clobber).
    let existing = std::fs::read(&hook_path).ok();
    let merged = compose_precommit_bytes(existing.as_deref());
    if existing.as_deref() == Some(merged.as_slice()) {
        return Ok(InstallOutcome::AlreadyCurrent);
    }
    std::fs::write(&hook_path, &merged)?;
    // Mark the hook executable so git runs it. A hook without the execute bit is silently
    // ignored, which would defeat the whole feature.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&hook_path)?.permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&hook_path, perms)?;
    }
    Ok(if existed {
        InstallOutcome::Refreshed
    } else {
        InstallOutcome::Installed
    })
}

/// Provision the per-project JS driver under `<root>/.rigger/shim/`: write the three
/// embedded runtime files (`shim.mjs`, `package.json`, `package-lock.json`) and
/// install their npm dependencies so `node_modules` is ready and `rigger workflow`
/// is zero-setup. Rooted at `root` so it is testable against a temp dir.
///
/// Provisioning is a silent no-op when the shim is already up to date: the three
/// runtime files match the embedded copies AND `node_modules` is present (see
/// [`shim_is_current`]). Skipping then avoids re-touching the files' mtimes and
/// re-running npm on every `rigger setup` (spec 05, criterion 4: setup is re-runnable
/// and changes nothing when nothing drifted). Otherwise the files are (re)written from
/// the embedded copies (so a `rigger` upgrade refreshes the driver to match the binary)
/// and npm install runs: `npm ci` when the lockfile is present (a reproducible, locked
/// install), else `npm install`. A missing `npm` is a CLEAR error (naming the directory
/// it would have installed in), never a silent skip - the user must know the driver is
/// not ready. Returns whether it actually (re)provisioned.
fn provision_shim(root: &Path) -> Result<bool, Box<dyn std::error::Error>> {
    let dir = shim_dir(root);
    if shim_is_current(&dir) {
        return Ok(false);
    }
    write_shim_files(root)?;
    run_npm_install(&dir)?;
    Ok(true)
}

/// Whether the provisioned shim in `dir` is up to date: every embedded runtime file is
/// present with byte-identical content AND npm's install is COMPLETE. Used by
/// [`provision_shim`] to make a `rigger setup` rerun a no-op instead of re-writing the
/// files and re-running npm.
///
/// Completeness is gated on `node_modules/.package-lock.json` - the hidden lockfile npm
/// writes as the FINAL step of a successful `npm ci` / `npm install` - not on the mere
/// PRESENCE of a `node_modules` directory. A torn/partial install (an interrupted `npm
/// ci`, which `rm -rf`s `node_modules` then repopulates incrementally) leaves the
/// directory present-but-incomplete and WITHOUT the marker; gating on the marker makes
/// setup re-run npm and SELF-HEAL it rather than treating the broken tree as current and
/// refusing to repair it forever (spec 05, criterion 4: setup is re-runnable).
fn shim_is_current(dir: &Path) -> bool {
    dir.join("node_modules")
        .join(".package-lock.json")
        .is_file()
        && SHIM_FILES.iter().all(|(name, contents)| {
            std::fs::read(dir.join(name))
                .map(|on_disk| on_disk == contents.as_bytes())
                .unwrap_or(false)
        })
}

/// Write the three embedded shim runtime files into `<root>/.rigger/shim/`,
/// returning that directory. Split out from [`provision_shim`] (which also runs npm
/// install) so the file-provisioning step is testable without invoking npm.
fn write_shim_files(root: &Path) -> Result<std::path::PathBuf, Box<dyn std::error::Error>> {
    let dir = shim_dir(root);
    std::fs::create_dir_all(&dir)?;
    for (name, contents) in SHIM_FILES {
        std::fs::write(dir.join(name), contents)?;
    }
    Ok(dir)
}

/// Install the shim's npm dependencies in `dir`. Uses `npm ci` when a
/// `package-lock.json` is present (a clean, lockfile-exact install) and `npm
/// install` otherwise. `npm` not being on PATH is a clear, actionable error naming
/// the directory - the JS driver is unusable without its deps, so this never
/// silently succeeds.
fn run_npm_install(dir: &Path) -> Res {
    let npm = std::env::var("RIGGER_NPM").unwrap_or_else(|_| "npm".to_string());
    let subcmd = if dir.join("package-lock.json").exists() {
        "ci"
    } else {
        "install"
    };
    let status = Command::new(&npm)
        .arg(subcmd)
        .current_dir(dir)
        .status()
        .map_err(|e| {
            format!(
                "setup: could not run `{npm} {subcmd}` in {}: {e}. \
                 Is Node's npm installed and on your PATH? The JS driver needs its \
                 dependencies before `rigger workflow` can run.",
                dir.display()
            )
        })?;
    if !status.success() {
        return Err(format!(
            "setup: `{npm} {subcmd}` failed in {} ({status}); the JS driver's \
             dependencies were not installed",
            dir.display()
        )
        .into());
    }
    Ok(())
}

/// `rigger setup` is the FULL project setup: it does everything `rigger init` does
/// (scaffold `.rigger/` + install the Claude Code hook), installs the native
/// `/rigger` Claude Code workflow at `.claude/workflows/rigger.js`, AND provisions
/// the JS driver (writes the embedded shim runtime into `.rigger/shim/` and runs `npm
/// install`). After it runs the user can drive the loop with the native workflow
/// (`/rigger <spec>`) with zero manual setup; the standalone `rigger workflow` shim
/// remains as a fallback.
fn cmd_setup(args: &[String]) -> Res {
    let opts = parse_setup_args(args)?;
    let root = Path::new(".");
    // Each step is drift-aware and reports whether it changed anything, so setup is
    // safely re-runnable: it refreshes a drifted workflow and reports it, and a rerun
    // on an up-to-date repo changes nothing and prints nothing surprising (spec 05,
    // criterion 4).
    let scaffold = init_project(root)?;
    let workflow = install_workflow(root)?;
    // Install EVERY skill in the registry (spec 20, unit 3; spec 68, criterion 1): each a
    // loadable front-door DISTINCT from the `/rigger` workflow, with this repo's project
    // overlay (base branch, specs location) merged into the render. Drift-aware like the
    // workflow, so a rerun on an up-to-date repo changes nothing.
    let skills = install_skills(root)?;
    // Install the docs-regenerating git pre-commit hook (spec 24): on `git commit` it runs
    // `rigger docs` and stages any changed rendered outputs into the SAME commit, so a commit
    // that changes a documented code fact carries its freshly rendered docs. Drift-aware and
    // non-destructive like the installs above - a rerun on an already-installed hook changes
    // nothing, and any pre-existing pre-commit hook is chained, never clobbered.
    let hook = install_precommit_hook(root)?;
    let provisioned = provision_shim(root)?;

    // The --agents import (units 4 + 8 woven) is itself a REQUESTED change: it runs
    // before the silent-no-op check and always reports its outcome, so an import onto
    // an otherwise up-to-date repo is never silently skipped.
    let imported = if let Some(src) = &opts.agents_dir {
        let summary = import_agents(root, src)?;
        println!(
            "imported {} agent {} from {} into .rigger/agents/ ({} kept - already present)",
            summary.imported,
            if summary.imported == 1 {
                "file"
            } else {
                "files"
            },
            src.display(),
            summary.skipped,
        );
        true
    } else {
        false
    };

    let workflow_changed = workflow != InstallOutcome::AlreadyCurrent;
    let skill_changed = skills
        .iter()
        .any(|(_, outcome)| *outcome != InstallOutcome::AlreadyCurrent);
    let hook_changed = hook != InstallOutcome::AlreadyCurrent;
    if !scaffold.changed()
        && !workflow_changed
        && !skill_changed
        && !hook_changed
        && !provisioned
        && !imported
    {
        // A silent no-op: nothing drifted, so there is nothing to report.
        return Ok(());
    }

    // Surface the running binary's version + build provenance (spec 18) whenever setup
    // actually reports a change, so an agent can see which binary just (re)provisioned the
    // project. Printed AFTER the silent-no-op early return above, so a rerun that changed
    // nothing stays silent.
    println!("{}", version_line());

    // Narrate ONLY the scaffold artifacts this run actually (re)wrote - never a blanket
    // claim, so a gitignore-only repair reports the gitignore change alone.
    for line in scaffold_summary_lines(&scaffold) {
        println!("{line}");
    }
    if provisioned {
        println!(
            "provisioned the JS driver in .rigger/shim/ (wrote shim.mjs + package.json + \
             package-lock.json and ran npm install)"
        );
    }
    match workflow {
        InstallOutcome::Installed => println!(
            "installed the /rigger workflow (.claude/workflows/rigger.js) - run it with: /rigger \
             <spec-path>"
        ),
        InstallOutcome::Refreshed => println!(
            "refreshed the drifted /rigger workflow (.claude/workflows/rigger.js) to match this \
             rigger build"
        ),
        InstallOutcome::AlreadyCurrent => {}
    }
    for (name, outcome) in &skills {
        match outcome {
            InstallOutcome::Installed => {
                println!("installed the {name} skill (.claude/skills/{name}/SKILL.md)")
            }
            InstallOutcome::Refreshed => println!(
                "refreshed the drifted {name} skill (.claude/skills/{name}/SKILL.md) to match \
                 this rigger build"
            ),
            InstallOutcome::AlreadyCurrent => {}
        }
    }
    match hook {
        InstallOutcome::Installed => println!(
            "installed the docs pre-commit hook - each commit now regenerates the using-rigger \
             docs and stages any change into that same commit"
        ),
        InstallOutcome::Refreshed => {
            println!("refreshed the docs pre-commit hook to match this rigger build")
        }
        InstallOutcome::AlreadyCurrent => {}
    }
    // The starter-fleet pointer fires exactly when default agents were NEWLY
    // scaffolded (spec 05 line 57 clause 2): the per-artifact report's `new_agents`
    // is the scaffolded-new signal.
    if !scaffold.new_agents.is_empty() {
        print_scaffold_pointer();
    }
    // The orientation block closes the reported-change path: because it lives after the
    // silent-no-op early return above, a fully up-to-date rerun that changed nothing stays
    // quiet and never re-prints it (spec 05 crit 4: a rerun prints nothing surprising).
    print_orientation();
    Ok(())
}

/// Parsed `rigger setup` options. Setup takes no positional arguments; the only
/// flag is `--agents <dir>`, the local directory a starting agent fleet is imported
/// from (spec 05).
#[derive(Debug, Default)]
struct SetupOpts {
    agents_dir: Option<std::path::PathBuf>,
}

/// Parse `rigger setup`'s arguments: only `--agents <dir>` is recognized. An unknown
/// flag or a missing `--agents` value is a clear error rather than a silent skip.
fn parse_setup_args(args: &[String]) -> Result<SetupOpts, Box<dyn std::error::Error>> {
    let mut opts = SetupOpts::default();
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--agents" => {
                let dir = it.next().ok_or(
                    "setup: --agents needs a directory argument (a local checkout of an \
                     agent collection)",
                )?;
                opts.agents_dir = Some(std::path::PathBuf::from(dir));
            }
            other => return Err(format!("setup: unknown argument {other:?}").into()),
        }
    }
    Ok(opts)
}

/// The outcome of an agent import: how many `.md` files were newly written into
/// `.rigger/agents/` and how many were kept untouched because a file of that name
/// already existed (import never overwrites).
#[derive(Debug, Default, PartialEq, Eq)]
struct ImportSummary {
    imported: usize,
    skipped: usize,
}

/// Import a starting agent fleet from a local collection directory into
/// `<root>/.rigger/agents/` (spec 05: offline - no network access in setup; the user
/// clones the collection themselves). For each `.md` file in `src`, the identity
/// frontmatter field is normalized to Rigger's `id:` and the file is copied under its
/// own name into `.rigger/agents/`. A file whose name already exists is KEPT untouched
/// (import never overwrites, so a re-run - or importing over the scaffolded fleet - is
/// safe) and counted as skipped. The result is validated by the SAME `config::load`
/// `rigger validate` runs, so a malformed agent fails the import loudly rather than
/// being written and breaking a later load. Rooted at `root` so it is testable against
/// a temp dir.
fn import_agents(root: &Path, src: &Path) -> Result<ImportSummary, Box<dyn std::error::Error>> {
    let dest = root.join(RIGGER_DIR).join("agents");
    std::fs::create_dir_all(&dest)?;

    // Collect the source `.md` files, SURFACING (never silently dropping) any directory
    // entry that fails to stat - a collection with an unreadable file must fail the
    // import loudly, not import a short count under a success message. Sorted so the log
    // and any first-error are stable across filesystems.
    let mut md_files: Vec<std::path::PathBuf> = Vec::new();
    for entry in std::fs::read_dir(src)
        .map_err(|e| format!("setup --agents: cannot read {}: {e}", src.display()))?
    {
        let entry = entry.map_err(|e| {
            format!(
                "setup --agents: reading an entry under {}: {e}",
                src.display()
            )
        })?;
        let path = entry.path();
        if path.extension().and_then(|x| x.to_str()) == Some("md") {
            md_files.push(path);
        }
    }
    md_files.sort();

    // The prospective fleet: the agents already on disk plus the ones this import would
    // add. `rigger setup` scaffolds the default fleet before this runs, and a foreign
    // collection can carry an id that collides with a scaffolded agent (or with another
    // file in the same import) under a DIFFERENT filename - past the filename-only
    // overwrite guard, but a duplicate id `config::load` rejects. We therefore validate
    // the whole prospective fleet BEFORE writing anything (below), so a collision aborts
    // the import atomically instead of leaving half the files on disk to brick every
    // later load.
    let mut fleet: Vec<(String, config::AgentDef)> = config::read_agents_dir(&dest)
        .map_err(|e| format!("setup --agents: reading the existing fleet: {e}"))?;

    // Pass 1: normalize, parse, and STAGE each file to write - writing nothing yet.
    let mut summary = ImportSummary::default();
    let mut to_write: Vec<(String, String, String)> = Vec::new(); // (name, content, id)
    for path in md_files {
        let name = path
            .file_name()
            .and_then(|x| x.to_str())
            .ok_or_else(|| {
                format!(
                    "setup --agents: non-UTF-8 file name under {}",
                    src.display()
                )
            })?
            .to_string();
        if dest.join(&name).exists() {
            println!("kept existing .rigger/agents/{name} (import never overwrites)");
            summary.skipped += 1;
            continue;
        }
        let raw = std::fs::read_to_string(&path)
            .map_err(|e| format!("setup --agents: read {name}: {e}"))?;
        let normalized =
            normalize_identity(&raw).map_err(|e| format!("setup --agents: {name}: {e}"))?;
        // Parse structurally as we stage, so a malformed file's error names it (the same
        // parse the loader uses). The id invariant (non-blank, unique) is enforced once
        // for the whole fleet by `config::index_agents` below - the SAME rule the loader
        // applies, not a second copy of it.
        let parsed = config::parse_agent(normalized.as_bytes())
            .map_err(|e| format!("setup --agents: {name}: {e}"))?;
        let id = parsed.id.clone();
        fleet.push((name.clone(), parsed));
        to_write.push((name, normalized, id));
    }

    // Validate the prospective fleet by the SAME rule `config::load` enforces - a
    // non-blank, unique id per agent - before a single byte is written, so a blank or
    // colliding id fails the import loudly and leaves `.rigger/agents/` untouched.
    config::index_agents(fleet)?;

    // Pass 2: every staged file validated - commit the writes.
    for (name, content, id) in &to_write {
        std::fs::write(dest.join(name), content)
            .map_err(|e| format!("setup --agents: write {name}: {e}"))?;
        println!("imported .rigger/agents/{name} (id: {id})");
        summary.imported += 1;
    }

    // Full referential validation of the resulting project (workflow -> agent
    // references, the review panel, gates) via the same load `rigger validate` runs.
    let root_str = root
        .to_str()
        .ok_or("setup --agents: project root path is not valid UTF-8")?;
    config::load(root_str)?;

    Ok(summary)
}

/// Return `content` with the agent's identity frontmatter key normalized to Rigger's
/// `id:`. Collections such as agency-agents / Claude Code sub-agents name the identity
/// field `name:`, while Rigger's [`config::AgentDef`] requires `id:`. If the
/// frontmatter already declares a top-level `id:`, the content is returned unchanged;
/// otherwise the FIRST top-level `name:` key is renamed to `id:`, preserving its value,
/// every other frontmatter line, and the prompt body verbatim. A file with no YAML
/// frontmatter is an error (the same shape the loader rejects).
fn normalize_identity(content: &str) -> Result<String, Box<dyn std::error::Error>> {
    // Parse the frontmatter through the SAME seam the loader uses
    // (`config::split_frontmatter`), not a second private copy of the delimiter logic:
    // `front` is the frontmatter text, `body` the prompt after the closing `---`. A file
    // with no (or unterminated) frontmatter fails here exactly as the loader's parse does.
    let (front, body) = config::split_frontmatter(content)?;

    // A top-level `id:` already present -> nothing to normalize.
    if front.lines().any(|l| top_level_key(l) == Some("id")) {
        return Ok(content.to_string());
    }

    // Rename the FIRST top-level `name:` key to `id:`, preserving its value and every other
    // frontmatter line; the prompt body is reattached verbatim.
    let mut renamed = false;
    let new_front = front
        .lines()
        .map(|line| {
            if !renamed && top_level_key(line) == Some("name") {
                renamed = true;
                let colon = line.find(':').expect("a top-level key implies a colon");
                format!("id{}", &line[colon..])
            } else {
                line.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n");

    Ok(format!("---\n{new_front}\n---\n{body}"))
}

/// The top-level YAML key a frontmatter line declares, or `None` when the line is
/// blank, indented (a nested value), a comment, or carries no `key:`. Frontmatter is
/// flat, so a non-indented `key:` line is a top-level field.
fn top_level_key(line: &str) -> Option<&str> {
    if line.is_empty() || line.starts_with([' ', '\t']) || line.starts_with('#') {
        return None;
    }
    let (key, _rest) = line.split_once(':')?;
    Some(key.trim_end())
}

/// Build the [`rigger::docs::DocsContext`] from the code definitions the runtime uses,
/// so no discipline fact is hand-copied into the rendered document: each field is read
/// from the same const / enum / registry the binary runs on, and changing that source
/// changes the render. This is the composition-root wiring for the render pipeline (spec
/// 20, unit 1) - the pure render lives in `rigger::docs`, and this function is where the
/// concrete facts are injected. A project overlay (unit 3) overrides fields on the
/// returned context BEFORE rendering, so repo specifics and the shared discipline share
/// this one pipeline.
fn docs_context() -> rigger::docs::DocsContext {
    use rigger::spec::ShapeRule;
    rigger::docs::DocsContext {
        base_ref: DEFAULT_BASE_REF.to_string(),
        dash_port: dash::DEFAULT_PORT,
        max_retries: rigger::safety::MAX_RETRIES,
        verdict_approve: conductor::VERDICT_APPROVE.to_string(),
        // Enumerate the lint rules explicitly so the render reads their real `name()` and
        // a removed variant breaks THIS build, not the rendered document at runtime.
        spec_shape_rules: [
            ShapeRule::MultiBehavior,
            ShapeRule::SubBulletAsUnit,
            ShapeRule::OverLong,
        ]
        .iter()
        .map(|r| r.name().to_string())
        .collect(),
        spec_shape_recommendation: spec::SHAPE_RECOMMENDATION.to_string(),
        subcommands: SUBCOMMANDS.iter().map(|c| c.to_string()).collect(),
        specs_location: DEFAULT_SPECS_LOCATION.to_string(),
        // Spec 69, criterion 1: the five `rigger watch` signals, in Design order, read from
        // the SAME `watch::Signal::name()`/`response()` `rigger watch` itself prints on an
        // anomaly line - so `rigger-watch-a-run`'s render can never silently drift from the
        // command's real signal set.
        watch_signals: [
            watch::Signal::Escalated,
            watch::Signal::DeadDriver,
            watch::Signal::DashNotServing,
            watch::Signal::RejectRecurrence,
            watch::Signal::FrontierStall,
        ]
        .map(|signal| rigger::docs::WatchSignalFact {
            name: signal.name().to_string(),
            response: signal.response().to_string(),
        }),
        watch_poll_interval_secs: watch::DEFAULT_INTERVAL_SECS,
        reject_recurrence_diagnose_threshold: watch::REJECT_RECURRENCE_DIAGNOSE_THRESHOLD,
    }
}

/// Render EVERY [registry skill](rigger::docs::skill_registry) plus the handbook
/// discipline chapter from [`docs_context`], and write them under `root`: each skill at
/// [`skill_source_rel`]`(entry.name)`, the handbook at [`HANDBOOK_DISCIPLINE_REL`]. Returns
/// the paths written, in registry order followed by the handbook - a stable order. Rooted
/// at `root` so it is testable against a temp dir without touching the process cwd; parent
/// directories are created so a fresh checkout renders every file (spec 68, criterion 1:
/// generalizes over the whole registry - adding an entry needs no edit here).
fn write_docs(root: &Path) -> Result<Vec<std::path::PathBuf>, Box<dyn std::error::Error>> {
    let ctx = docs_context();
    let mut outputs: Vec<(std::path::PathBuf, String)> = rigger::docs::skill_registry()
        .into_iter()
        .map(|entry| (root.join(skill_source_rel(entry.name)), entry.render(&ctx)))
        .collect();
    outputs.push((
        root.join(HANDBOOK_DISCIPLINE_REL),
        rigger::docs::render_handbook_discipline(&ctx),
    ));
    let mut written = Vec::with_capacity(outputs.len());
    for (path, contents) in &outputs {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, contents)?;
        written.push(path.clone());
    }
    Ok(written)
}

/// `rigger docs` renders the operating discipline from the code the binary runs on into
/// its committed outputs - every registry skill plus the handbook discipline chapter - so
/// the discipline stays in lock-step with behavior instead of drifting from it. Re-run it
/// after changing a source fact or a template and commit the result; `rigger validate`
/// (spec 20, unit 2; spec 68, criterion 1) fails loudly if a committed copy drifts from a
/// fresh render.
fn cmd_docs(_args: &[String]) -> Res {
    for path in write_docs(Path::new("."))? {
        println!("rendered {}", path.display());
    }
    Ok(())
}

/// The committed discipline outputs under `root` that have DRIFTED from a fresh render of
/// the current code-derived context (spec 20, unit 2; spec 68, criterion 1: the docs-drift
/// gate covers EVERY registry entry), in report order (registry order, then the handbook).
/// A path is reported only when the committed file EXISTS and its BYTES differ from a
/// fresh render; an ABSENT (or unreadable) file is skipped - these are rigger's OWN
/// committed docs, and an operator project never carries them, so their absence is not
/// drift (the same "nothing installed, nothing to drift" rule
/// [`installed_workflow_drifted`] applies to the workflow). Reuses the SINGLE render
/// authority ([`docs_context`] + `rigger::docs::skill_registry`/`render_handbook_discipline`)
/// and the same [`skill_source_rel`] / [`HANDBOOK_DISCIPLINE_REL`] paths [`write_docs`]
/// writes, so the drift check and the write can never disagree on what "the docs" are.
/// Rooted at `root` so the seam is testable against a temp dir without touching the cwd.
fn docs_drift(root: &Path) -> Vec<std::path::PathBuf> {
    let ctx = docs_context();
    let mut checks: Vec<(std::path::PathBuf, String)> = rigger::docs::skill_registry()
        .into_iter()
        .map(|entry| (root.join(skill_source_rel(entry.name)), entry.render(&ctx)))
        .collect();
    checks.push((
        root.join(HANDBOOK_DISCIPLINE_REL),
        rigger::docs::render_handbook_discipline(&ctx),
    ));
    let mut drifted = Vec::new();
    for (path, fresh) in checks {
        // Byte comparison (not `read_to_string`): a committed file corrupted to non-UTF-8 is
        // genuinely drifted, and comparing bytes catches it rather than silently skipping it.
        match std::fs::read(&path) {
            Ok(bytes) if bytes != fresh.as_bytes() => drifted.push(path),
            _ => {} // absent/unreadable (not our committed docs here), or byte-identical
        }
    }
    drifted
}

/// The `rigger validate` docs-drift FAILURE (spec 20, unit 2; spec 68, criterion 1): when a
/// committed discipline output has drifted from a fresh render, a single loud message
/// naming EVERY drifted file and the one-command fix, or `None` when the committed docs
/// are in sync (or absent). Unlike the warning advisories, the caller surfaces this as a
/// HARD, non-zero exit - a changed const/template/hand-edit is a definition drift that
/// must be regenerated, not a soft nudge - so the discipline STAYS in lock-step with the
/// code the binary runs on.
fn docs_drift_failure(root: &Path) -> Option<String> {
    let drifted = docs_drift(root);
    if drifted.is_empty() {
        return None;
    }
    let names = drifted
        .iter()
        .map(|p| p.display().to_string())
        .collect::<Vec<_>>()
        .join(", ");
    Some(format!(
        "the committed rigger skill/discipline docs have drifted from a fresh render \
         ({names}): a source fact or template changed but the committed copy was not \
         regenerated, so the discipline no longer matches the code it describes. Run \
         `rigger docs` and commit the result so they are in lock-step again."
    ))
}

fn cmd_prime() -> Res {
    let path = db_path("events.db");
    let selection = store_selection(None, None)?;
    if selection.is_sqlite() && !Path::new(&path).exists() {
        println!("# Rigger: no decisions recorded yet (run `rigger run` to start).");
        return Ok(());
    }
    let store = resolve_store(&selection, &path)?;
    let events = store.read_all(0, Direction::Backward, &Filter::default())?;
    println!("# Rigger: recent decisions");
    let mut shown = 0;
    for e in &events {
        if e.type_ != contextgraph::TYPE_DECISION_MADE {
            continue;
        }
        if let Ok(d) = serde_json::from_slice::<PeerDecision>(&e.data) {
            println!("- {}: {}", d.id, d.summary);
            shown += 1;
            if shown >= 10 {
                break;
            }
        }
    }
    if shown == 0 {
        println!("(none yet)");
    }
    Ok(())
}

/// Build the grounder named by `defaults.grounder` (§3.2, §5.4, R4). The structural
/// `symbols` grounder is the DEFAULT: an UNSET / empty `defaults.grounder` AND the explicit
/// name `symbols` resolve to it. `grep` and `nop` resolve via `grounder::grounder_for` and
/// are reachable ONLY when named explicitly.
///
/// When the binary is built WITHOUT the `symbols` feature, resolving to symbols is a LOUD
/// error (a clear message + non-zero exit) via `grounder_for`, never a silent degrade to
/// grep. Grep runs ONLY when the user writes `grounder: grep`.
///
/// This is c2's mechanical default-resolution after turbovec's retirement; c1 is the
/// authority that PROVES the accepted-name contract (the exact `symbols`/`grep`/`nop` set
/// and the loud migration error for the retired `turbovec` / `hybrid` names).
fn select_grounder(name: &str) -> Result<Box<dyn Grounder>, Box<dyn std::error::Error>> {
    // `symbols` (and the UNSET / empty default) resolve to the real structural grounder when the
    // feature is built (it opens or builds+persists the index over the repo root); a build WITHOUT
    // the feature falls through to `grounder_for`, whose `symbols` arm is the loud no-silent-degrade
    // error.
    #[cfg(feature = "symbols")]
    {
        let n = name.trim();
        if n.is_empty() || n.eq_ignore_ascii_case("symbols") {
            return Ok(Box::new(
                rigger::grounder::symbols::grounder::Symbols::open(".", None),
            ));
        }
    }
    Ok(rigger::grounder::grounder_for(name, ".")?)
}

/// The grounder for `rigger reindex`. After turbovec's retirement it resolves IDENTICALLY to
/// [`select_grounder`]: the only case that ever differed was turbovec (whose freshening `new`
/// had to be swapped for `new_for_reindex` to avoid a double-embed). The surviving grounders
/// have no such distinction - `Symbols::open` only LOADS the persisted index (it does not
/// freshen the whole tree), and grep / nop have no index at all - so there is one selection
/// authority, not two to keep in sync by hand.
fn select_reindex_grounder(name: &str) -> Result<Box<dyn Grounder>, Box<dyn std::error::Error>> {
    select_grounder(name)
}

fn git_repo() -> String {
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    git_repo_at(&cwd)
}

/// The git top-level directory *containing `root`*, resolved with `git -C <root>` so the
/// answer is anchored at `root` rather than the process cwd - empty when `root` is not in
/// a git repo. Running git anchored at an explicit directory is what lets the couriers
/// derive a store's identity from the RESOLVED store root (which git reports as the repo
/// root) instead of the cwd (which, inside a git-linked worktree, git reports as the
/// worktree path) - see [`project_identity_at`].
fn git_repo_at(root: &Path) -> String {
    Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["rev-parse", "--show-toplevel"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default()
}

fn print_run_state(rs: &RunState, base: &str) {
    println!("run state:");
    for (name, u) in &rs.units {
        println!("  {:<20} {}", name, u.status.as_str());
    }
    if rs.done() {
        println!("done: every unit integrated");
    } else {
        println!("incomplete: not every unit integrated");
    }
    // The ready-to-release handoff (spec 38, criterion 3): on a DONE run, surface the run
    // branch, the release-target base, the integrated-unit count, and the exact PR command
    // the human runs to open the release PR - the same one-authority render `rigger status`
    // shows. The loop STOPS here: it surfaces the handoff, it never merges to the base.
    if let Some(rr) = rs.release_ready(RUN_BRANCH, base) {
        for line in rr.lines() {
            println!("{line}");
        }
    }
}

/// Write `content` to `path` only when it does not already exist, returning `Ok(true)`
/// when it WROTE the file and `Ok(false)` when it KEPT an existing one. Keeping is silent
/// (a `rigger setup` / `rigger init` rerun must not narrate every file it left untouched),
/// so the boolean is how callers report only what a run actually created. A genuine write
/// FAILURE is an ERROR naming the artifact, not a swallowed `false`: setup/init must exit
/// nonzero rather than drop an artifact it could not create from the summary while still
/// exiting 0 (an honest-reporting hole on the error path). Only an already-present file is
/// a silent success; a real I/O failure escalates.
fn write_if_absent(path: &Path, content: &str) -> Result<bool, Box<dyn std::error::Error>> {
    if path.exists() {
        return Ok(false);
    }
    std::fs::write(path, content)
        .map_err(|e| format!("rigger: could not write {}: {e}", path.display()))?;
    Ok(true)
}

/// The scaffolded workflow (§3.2): a worked plan -> implement pipeline where the
/// review is PER UNIT. It demonstrates the documented shape - a `defaults:` block
/// (autonomy + grounder + the three-tier `review` panel), a reusable `gates:`
/// library, and an implement stage that runs each unit's complete lifecycle
/// (implement -> gates -> three-tier review of THIS unit -> integrate). It loads
/// through `config::load` against the agents scaffolded alongside it.
const SCAFFOLD_WORKFLOW: &str =
    "# Scaffolded by `rigger init`. A worked plan -> implement pipeline where the\n\
# review is PER UNIT: each unit implements, three-tier-reviews ITSELF (lenses ->\n\
# adversary -> adjudicator via defaults.review), and integrates in one lifecycle.\n\
# Replace the gate commands with your own.\n\
name: example\n\
\n\
defaults:\n  \
autonomy: auto_notify   # manual | auto_notify | silent\n  \
grounder: symbols       # symbols (default; the structural symbol index) | grep | nop\n  \
# The spawn-budget circuit-breaker: the hard cap on agent spawns one unattended\n  \
# run may make. At the cap the breaker emits BudgetExhausted and aborts the run,\n  \
# so a runaway can never spawn unboundedly. NON-ZERO on purpose - 0 = unlimited.\n  \
budget: 60\n  \
# The remediation depth: how many attempts a failed unit gets before it escalates\n  \
# to a human. This is the REFINEMENT-depth knob, not a review-rigor one - raise it\n  \
# to give a subtle unit room to CONVERGE under the full strict review instead of\n  \
# escalating prematurely. It loosens the depth limit, never the review bar. Absent\n  \
# falls back to 3 (the historical default); bounded by `budget` above.\n  \
max_retries: 3\n  \
# The three-tier review panel applied to EVERY implement unit. Declared once\n  \
# here, inherited by the implement stage and every planner-proposed unit.\n  \
review:\n    \
lenses: [architecture-reviewer, sdet]   # tier 1: the expert lenses\n    \
adversary: adversary           # tier 2: reviews the lenses and refutes them\n    \
adjudicator: adjudicator   # tier 3: neutral judge; its verdict gates the unit\n\
\n\
# The compilation-cache wrapper (spec 65): `auto` probes PATH for a known wrapper\n\
# (sccache, ccache) and uses it when present, so a machine that already has one\n\
# installed benefits with no further config; `off` disables the shared-cache\n\
# layer entirely. See `rigger validate` for the resolved wrapper/cache dir/budget.\n\
build:\n  \
wrapper: auto\n\
\n\
gates:                    # a reusable library of commands, referenced by name\n  \
build: { run: \"echo build ok; true\", kind: core }\n  \
test:  { run: \"echo test ok; true\",  kind: core }\n  \
lint:  { run: \"echo lint ok; true\",  kind: elevated }\n\
\n\
stages:\n  \
# The conductor creates one baseline implement unit per acceptance criterion (the\n  \
# deterministic decomposition); this planner REFINES that baseline via UnitProposed.\n  \
# A produces stage decomposes the whole spec, so it has no single coverage criterion\n  \
# - it grounds on the spec's acceptance criteria, not a `coverage` label.\n  \
plan:\n    \
agent: planner\n    \
produces: dag           # refine the spec's unit DAG at runtime\n\
\n  \
# The adversarial plan-critique gate: BEFORE any implementer spawns, the adversary +\n  \
# adjudicator review the PROPOSED unit DAG for the cross-unit hazards per-unit review\n  \
# cannot see: ambiguous mitigation ownership and open dispositions (a shared blast\n  \
# radius is informational only - partition: by-blast-radius serializes it). A reject\n  \
# feeds back to the\n  \
# planner (bounded by max_retries); an approve releases the fan-out. Review-only (no\n  \
# agent) - it critiques the plan, it does not implement.\n  \
plan-critique:\n    \
needs: [plan]\n    \
adversary: adversary        # tier 2: reviews the DAG and refutes it\n    \
adjudicator: adjudicator    # tier 3: its approve/reject gates the fan-out\n\
\n  \
# Each unit implements, three-tier-reviews ITSELF (via defaults.review), and\n  \
# integrates in one lifecycle. A reject or a gate failure feeds back into that\n  \
# same unit's remediation loop; it does NOT integrate until approved + green.\n  \
implement:\n    \
needs: [plan-critique]\n    \
agent: rust-engineer\n    \
strategy: fan-out       # one worker per ready unit, in isolated worktrees\n    \
partition: by-blast-radius\n    \
gates: [build, test, lint]  # red -> green enforced around the change\n    \
on_pass: merge          # land + reindex + record, per unit, once reviewed\n    \
coverage: \"each unit is implemented, reviews itself, and integrates green\"\n";

/// The agents the scaffolded workflow references - a fresh-repo SEED template, not a
/// frozen canonical fleet. Every entry is referenced by [`SCAFFOLD_WORKFLOW`] and every
/// referenced id is seeded here (the two stay in lockstep so a fresh `rigger init` seeds
/// no stray, unreferenced agent). The ids match this project's own canonical personas
/// (planner, rust-engineer, architecture-reviewer, sdet, adversary, adjudicator); the
/// four generic placeholder personas (implementer, devils-advocate, reviewer.architecture,
/// reviewer.technical) deliberately do NOT appear. Model tiers are a conscious seed
/// default: the implementer ships on a cheap-first `model_ladder` (`[sonnet, opus]`, spec 10
/// unit 4) so its first attempt is cheap and a persistently-failing unit escalates to the
/// strong model under remediation; the lenses stay on `sonnet` and the adversary and
/// adjudicator on a fixed `opus` (judgment is not laddered). Each is a
/// markdown-with-frontmatter definition `config::load` parses; filenames are arbitrary, the
/// `id` is what the workflow binds to.
const SCAFFOLD_AGENTS: &[(&str, &str)] = &[
    (
        "planner.md",
        "---\n\
id: planner\n\
model: sonnet\n\
tools: [Read, Grep, Glob]\n\
isolation: none\n\
---\n\
You decompose the spec into a DAG of small, independently-verifiable units, one\n\
per acceptance criterion. Emit each as a UnitProposed decision. Do not write code.\n",
    ),
    (
        "rust-engineer.md",
        "---\n\
id: rust-engineer\n\
model_ladder: [sonnet, opus]\n\
tools: [Read, Edit, Write, Grep, Glob, Bash]\n\
isolation: worktree\n\
recurse: false\n\
---\n\
You implement ONE fully-specified unit inside your worktree, in idiomatic Rust.\n\
Write the failing test first, confirm RED, implement minimally, confirm GREEN, run\n\
the named gates, commit. Report the final line as JSON: {\"id\",\"pass\",\"evidence\"}.\n",
    ),
    (
        "architecture-reviewer.md",
        "---\n\
id: architecture-reviewer\n\
model: sonnet\n\
tools: [Read, Grep, Glob, Bash]\n\
isolation: none\n\
---\n\
You review a diff for architectural defects ONLY. Quote the rule or doc violated.\n\
Output the REVIEW schema: {verdict, issues:[{title,file_line,reason}]}.\n",
    ),
    (
        "sdet.md",
        "---\n\
id: sdet\n\
model: sonnet\n\
tools: [Read, Grep, Glob, Bash]\n\
isolation: none\n\
---\n\
You review a diff for correctness, error-handling, test coverage, and idiomatic\n\
defects ONLY. Output the REVIEW schema: {verdict, issues:[{title,file_line,reason}]}.\n",
    ),
    (
        "adversary.md",
        "---\n\
id: adversary\n\
model: opus\n\
tools: [Read, Grep, Glob, Bash]\n\
isolation: none\n\
---\n\
You are the adversary (tier 2). You run AFTER the lenses and review THEIR findings\n\
AND the diff, trying to PROVE THE LENSES WRONG: hold them to a higher bar, surface\n\
the substantive issues they all missed, and refute lens overreach. You review the\n\
reviews - not a parallel lens - and you do NOT render the final verdict. Default to\n\
skepticism; cite file:line. Record findings with rigger_emit.\n",
    ),
    (
        "adjudicator.md",
        "---\n\
id: adjudicator\n\
model: opus\n\
tools: [Read, Grep, Glob, Bash]\n\
isolation: none\n\
---\n\
You are the adjudicator (tier 3), the neutral final judge. Weigh the expert lenses\n\
against the adversary and decide who wins. Be neutral in tone but EXTREMELY strict\n\
on design / architecture / ADR adherence: any deviation or cut corner is a reject,\n\
no matter which side flagged it. When you reject, say exactly what must change. End\n\
with a single JSON line {\"verdict\":\"approve\"} or {\"verdict\":\"reject\"} - reject\n\
blocks integration no matter what the static gates say.\n",
    ),
];

#[cfg(test)]
mod tests {
    use super::*;

    // --- Spec 39, criterion 1: idempotent start of the run dashboard on the step path ---

    /// The flagship criterion-1 proof: a `step` with NO dash serving starts exactly one, and
    /// a later `step` while it is serving starts NONE - the marker/pid check short-circuits.
    /// `start` is injected (counting spawns and recording a marker owned by THIS process, a
    /// guaranteed-live pid), so the real `pid_is_alive` predicate finds the recorded dash
    /// serving on the second call - proving idempotency without a real dashboard process.
    #[test]
    fn ensure_run_dashboard_at_starts_once_then_short_circuits_on_a_live_marker() {
        use std::cell::Cell;
        let dir = tempfile::tempdir().unwrap();
        let marker_path = dir.path().join(DASH_MARKER_FILE);
        let starts = Cell::new(0u32);
        let live = dash::DashMarker {
            port: 54321,
            pid: std::process::id(),
        };

        // First step of the run: no marker yet -> start one and record its marker.
        let first = ensure_run_dashboard_at(
            &marker_path,
            |m| dash::pid_is_alive(m.pid),
            || {
                starts.set(starts.get() + 1);
                Ok(live)
            },
        );
        assert_eq!(
            first,
            DashStart::Started(54321),
            "the first step starts a dash"
        );
        assert_eq!(starts.get(), 1, "exactly one dash was started");
        assert_eq!(
            dash::DashMarker::read(&marker_path),
            Some(live),
            "the started dash is recorded for later steps to discover"
        );

        // A later step WHILE it is serving: the marker names a live dash -> NO second start.
        let second = ensure_run_dashboard_at(
            &marker_path,
            |m| dash::pid_is_alive(m.pid),
            || {
                starts.set(starts.get() + 1);
                Ok(dash::DashMarker {
                    port: 60000,
                    pid: std::process::id(),
                })
            },
        );
        assert_eq!(
            second,
            DashStart::AlreadyServing(54321),
            "a later step is a no-op while a dash is serving"
        );
        assert_eq!(
            starts.get(),
            1,
            "no second dash was started - the start is idempotent across steps"
        );
    }

    /// [`dash_marker_serving`] is the ONE named predicate `ensure_run_dashboard`'s real
    /// production wiring passes to [`ensure_run_dashboard_at`] - pinned directly, not only
    /// through an injected test double, so a mutant that made it unconditionally trust any
    /// marker (round 3, part of closing adv-u69c4r2-mismatched-marker-still-trusts-a-dead-url's
    /// remediation) cannot slip through untested. A port nothing binds must read as NOT
    /// serving - the real network probe, never a bare presence check.
    #[test]
    fn dash_marker_serving_reports_false_when_nothing_answers_the_markers_port() {
        let port = dash::free_port_from(40000).expect("a free loopback port must be available");
        assert!(
            !dash_marker_serving(dash::DashMarker { port, pid: 1 }),
            "a marker naming a port nothing serves must read as not serving"
        );
    }

    /// A marker left by a crashed/reaped dash (recorded but NOT serving) does not suppress a
    /// fresh start: the step starts a new dash and overwrites the stale marker.
    #[test]
    fn ensure_run_dashboard_at_restarts_when_the_recorded_dash_is_gone() {
        let dir = tempfile::tempdir().unwrap();
        let marker_path = dir.path().join(DASH_MARKER_FILE);
        dash::DashMarker {
            port: 40000,
            pid: 123,
        }
        .write(&marker_path)
        .unwrap();
        let outcome = ensure_run_dashboard_at(
            &marker_path,
            |_| false, // the recorded dash is gone
            || {
                Ok(dash::DashMarker {
                    port: 40001,
                    pid: 456,
                })
            },
        );
        assert_eq!(
            outcome,
            DashStart::Started(40001),
            "a dead marker -> start a fresh dash"
        );
        assert_eq!(
            dash::DashMarker::read(&marker_path),
            Some(dash::DashMarker {
                port: 40001,
                pid: 456
            }),
            "the fresh dash replaces the stale marker"
        );
    }

    /// A best-effort start failure degrades to headless (a warning, `DashStart::Failed`) and
    /// records no marker - never a panic or a failed step.
    #[test]
    fn ensure_run_dashboard_at_reports_failed_when_the_start_errors() {
        let dir = tempfile::tempdir().unwrap();
        let marker_path = dir.path().join(DASH_MARKER_FILE);
        let outcome = ensure_run_dashboard_at(
            &marker_path,
            |_| false,
            || Err(std::io::Error::other("no free port")),
        );
        assert_eq!(
            outcome,
            DashStart::Failed,
            "a start failure degrades to headless, not a panic"
        );
        assert_eq!(
            dash::DashMarker::read(&marker_path),
            None,
            "a failed start records no marker"
        );
    }

    /// Spec 50, criterion 4 (opt-out): the always-on ensure is suppressed by EITHER opt-out - the
    /// environment disable OR the config `dash: off` (surfaced as `config_dash_enabled == false`) -
    /// and only proceeds when NEITHER is set. Pins that the two opt-out paths are independent (each
    /// alone suffices) and that a run with no opt-out still ensures the dash.
    #[test]
    fn dash_ensure_is_suppressed_by_either_opt_out_and_proceeds_when_neither_is_set() {
        // Neither opt-out: the ensure proceeds (the always-on default).
        assert!(
            !dash_ensure_suppressed(false, true),
            "with no env disable and the config dash ON, the ensure proceeds"
        );
        // The ENV opt-out alone suppresses it (even with the config dash ON).
        assert!(
            dash_ensure_suppressed(true, true),
            "RIGGER_NO_DASH alone suppresses the ensure"
        );
        // The CONFIG opt-out alone suppresses it (even with no env disable).
        assert!(
            dash_ensure_suppressed(false, false),
            "`dash: off` alone suppresses the ensure"
        );
        // Both set: still suppressed.
        assert!(
            dash_ensure_suppressed(true, false),
            "both opt-outs set stays suppressed"
        );
    }

    /// Spec 69, criterion 4: `dash_status_line` renders each [`dash::DashStatus`] outcome to
    /// the EXACT text `rigger status` prints - `Absent` prints nothing (today's silent
    /// no-recorded-dash case), `Serving` prints the unchanged `dashboard: <url>` line, a
    /// `NotServing` with a known pid (a MATCHING marker proven dead) prints the truthful line
    /// naming that pid and both self-heal paths, and a `NotServing` with no pid (round 3: a
    /// mismatched-or-absent marker, the url's own port directly proven dead) prints the
    /// truthful line WITHOUT fabricating a pid - never a stale URL either way.
    #[test]
    fn dash_status_line_renders_each_outcome_to_its_exact_text() {
        assert_eq!(
            dash_status_line(&dash::DashStatus::Absent),
            None,
            "no recorded dash prints nothing"
        );
        assert_eq!(
            dash_status_line(&dash::DashStatus::Serving("http://127.0.0.1:7420/".into())),
            Some("dashboard: http://127.0.0.1:7420/".to_string()),
            "a trusted URL prints exactly as before this criterion"
        );
        assert_eq!(
            dash_status_line(&dash::DashStatus::NotServing { pid: Some(4242) }),
            Some(
                "dashboard: not serving (marker names dead pid 4242) - run 'rigger dash' or \
                 the next step restarts it"
                    .to_string()
            ),
            "a matching marker proven dead names the pid and both self-heal paths, never a \
             stale URL"
        );
        assert_eq!(
            dash_status_line(&dash::DashStatus::NotServing { pid: None }),
            Some(
                "dashboard: not serving (recorded url is unreachable) - run 'rigger dash' or \
                 the next step restarts it"
                    .to_string()
            ),
            "a directly-proven-dead url with no matching marker names both self-heal paths \
             without fabricating a pid"
        );
    }

    /// Spec 69, criterion 4's third clause ("`--json` carries the same truth"): the JSON
    /// sibling of the text-render test above, over the same [`dash::DashStatus`] outcomes.
    /// `Absent` appends nothing (round 2: this is what closes the "vacuously true" gap - before
    /// round 2, `--json` never even reached a computed [`dash::DashStatus`], so this branch and
    /// the informative ones below were indistinguishable).
    #[test]
    fn dash_status_json_renders_each_outcome_to_its_exact_shape() {
        assert_eq!(
            dash_status_json(&dash::DashStatus::Absent),
            None,
            "no recorded dash appends nothing to the --json array"
        );
        assert_eq!(
            dash_status_json(&dash::DashStatus::Serving("http://127.0.0.1:7420/".into())),
            Some(serde_json::json!({
                "dashboard": {"status": "serving", "url": "http://127.0.0.1:7420/"}
            })),
            "a trusted URL carries its status and url"
        );
        assert_eq!(
            dash_status_json(&dash::DashStatus::NotServing { pid: Some(4242) }),
            Some(serde_json::json!({
                "dashboard": {"status": "not_serving", "pid": 4242}
            })),
            "a matching marker proven dead carries its status and the dead pid, never a URL"
        );
        assert_eq!(
            dash_status_json(&dash::DashStatus::NotServing { pid: None }),
            Some(serde_json::json!({
                "dashboard": {"status": "not_serving", "pid": null}
            })),
            "a directly-proven-dead url with no matching marker carries null, never a \
             fabricated pid"
        );
    }

    /// Spec 50, criterion 4 (stable fixed address): the step-path ensure port resolves to the
    /// FIXED [`dash::DEFAULT_PORT`] when the override env is unset - production's no-free-port-search
    /// singleton contract - and only a VALID `u16` override relocates it; an empty or malformed
    /// value degrades to the default so a bad knob never breaks a run's observability. Pure over the
    /// already-read env value so both branches are provable without mutating the process
    /// environment (the same discipline as `dash_ensure_suppressed`).
    #[test]
    fn dash_ensure_port_defaults_to_the_fixed_address_and_only_a_valid_override_relocates_it() {
        // Unset: the fixed default, with no free-port search - the production singleton address.
        assert_eq!(
            dash_ensure_port_from(None),
            dash::DEFAULT_PORT,
            "an unset override binds the fixed DEFAULT_PORT (the stable singleton address)"
        );
        // A valid u16 relocates it (the seam the step-path dash tests use to inject an ephemeral
        // port and never fight a real machine dash on the fixed default).
        assert_eq!(
            dash_ensure_port_from(Some("54321")),
            54321,
            "a valid u16 override relocates the ensure port"
        );
        assert_eq!(
            dash_ensure_port_from(Some("  8080  ")),
            8080,
            "surrounding whitespace is trimmed before parsing"
        );
        // Absent-shaped or malformed values degrade to the default, never a panic or a break.
        for bad in ["", "   ", "not-a-port", "70000", "-1", "80.5"] {
            assert_eq!(
                dash_ensure_port_from(Some(bad)),
                dash::DEFAULT_PORT,
                "a malformed override ({bad:?}) falls back to the fixed default"
            );
        }
    }

    /// spec 24/70, crit 1: `compose_precommit` is the PURE, filesystem-free composer for the
    /// docs pre-commit hook. A FRESH install (no existing hook) yields a runnable `/bin/sh`
    /// script carrying rigger's sentinel-marked managed block: it checks the docs (`rigger
    /// docs`) against ONLY the two rendered outputs by path (never a blanket `git add` - it
    /// never stages anything at all, spec 70), acts ONLY where those docs are already tracked
    /// (inert in an operator repo), guards `rigger` presence for graceful degrade, and ends
    /// with `true` on the no-drift path so a matching render can never block a commit.
    #[test]
    fn compose_precommit_fresh_install_carries_the_managed_block() {
        let hook = compose_precommit(None);
        assert!(
            hook.starts_with("#!/bin/sh\n"),
            "a fresh hook is a runnable sh script; got:\n{hook}"
        );
        assert!(
            hook.contains(PRECOMMIT_BEGIN) && hook.contains(PRECOMMIT_END),
            "carries the sentinel-marked managed block; got:\n{hook}"
        );
        assert!(hook.contains("rigger docs"), "checks the docs");
        assert!(
            hook.contains("command -v rigger"),
            "guards rigger presence (graceful degrade)"
        );
        assert!(
            hook.contains(skill_source_rel("using-rigger").as_str())
                && hook.contains(HANDBOOK_DISCIPLINE_REL),
            "compares exactly the two rendered outputs by path; got:\n{hook}"
        );
        assert!(
            !hook.contains("add -A") && !hook.contains("add .") && !hook.contains("git add --"),
            "never stages anything - it may only ever REFUSE more, not stage more (spec 70)"
        );
        assert!(
            hook.contains("ls-files --error-unmatch"),
            "the check is gated on the docs already being TRACKED, so the hook stays inert in \
             an operator repo that does not carry them; got:\n{hook}"
        );
        assert!(
            hook.contains("\ntrue\n"),
            "the managed block ends with `true` so it never blocks a commit"
        );
    }

    /// spec 70, crit 1 (the hook REFUSES instead of REWRITING, pure/structural): the managed
    /// block must never silently launder a stale re-render into the commit by staging it. It
    /// compares the fresh render against what is already staged (`git diff`, not `git add`) and
    /// hard-fails (`exit 1`) naming the drifted files, the rendering binary's path AND its build
    /// provenance (`rigger version`), and the two remedies - so a stale binary on PATH can never
    /// silently overwrite correctly staged docs (the bug that cost three rejected attempts on
    /// one unit). A matching render still ends in a bare `true` and never touches the index.
    #[test]
    fn precommit_block_refuses_on_drift_instead_of_staging() {
        let hook = compose_precommit(None);
        assert!(
            !hook.contains("git add --"),
            "the block must never invoke `git add` on the docs itself - it may only ever \
             REFUSE more, not stage more (a human-facing remedy may still name `git add` as \
             the operator's own next step); got:\n{hook}"
        );
        assert!(
            hook.contains("git diff"),
            "the block detects drift by comparing the fresh render against what is already \
             staged, not by unconditionally rewriting it; got:\n{hook}"
        );
        assert!(
            hook.contains("exit 1"),
            "a detected drift must hard-fail the commit, not warn-and-proceed; got:\n{hook}"
        );
        assert!(
            hook.contains("command -v rigger") && hook.contains("\"$rigger_bin\" version"),
            "the refusal names the rendering binary's path AND its build provenance; got:\n{hook}"
        );
        assert!(
            hook.to_lowercase().contains("reinstall")
                && (hook.contains("tree-built") || hook.contains("tree built")),
            "the refusal names the two remedies - re-render with the tree-built binary, or \
             reinstall; got:\n{hook}"
        );
    }

    /// spec 75, crit 1 (BINARY SELECTION, pure): the managed block must prefer a `rigger`
    /// built FROM THIS TREE over whatever happens to be first on PATH, so a worktree whose
    /// code legitimately changes a rendered fact renders with a binary that actually reflects
    /// it (rather than deadlocking against a stale PATH install). Proves, at the
    /// `compose_precommit_bytes` seam, that the block tries every candidate in the spec's
    /// exact order - env target dir (release then debug), local target (release then debug),
    /// this worktree's own unit-derived scratch cargo-target (release then debug), the shared
    /// step-cache target (debug only), PATH last - and that BOTH the render call and the
    /// provenance line invoke the RESOLVED binary, not a bare unqualified `rigger`. This
    /// criterion OWNS the candidate order and its rendering in the template (c2 owns the
    /// end-to-end fixture-driven behavior, not this test).
    #[test]
    fn precommit_block_resolves_a_tree_built_binary_before_path() {
        let hook = compose_precommit(None);

        // The actual TRY order is the order of the `for candidate in ...` list (shell
        // variables like `$unit_release` are computed once, above the loop, for POSIX
        // correctness - their VALUES, not their computation site, are what matters), bounded
        // between "for candidate in" and its closing "; do". Slicing to exactly that region
        // means a candidate's VALUE text appearing earlier (in its own assignment) can never
        // be mistaken for its position in the try order.
        let loop_start = hook
            .find("for candidate in")
            .expect("a for-loop iterates the candidates in order");
        let loop_body_start = loop_start + "for candidate in".len();
        let loop_end = hook[loop_body_start..]
            .find("; do")
            .map(|i| loop_body_start + i)
            .expect("the candidate for-loop is closed with `; do`");
        let candidate_list = &hook[loop_body_start..loop_end];
        let idx_env_release = candidate_list
            .find("$CARGO_TARGET_DIR/release/rigger")
            .expect("env target dir release candidate is tried");
        let idx_env_debug = candidate_list
            .find("$CARGO_TARGET_DIR/debug/rigger")
            .expect("env target dir debug candidate is tried");
        let idx_local_release = candidate_list
            .find("./target/release/rigger")
            .expect("local target release candidate is tried");
        let idx_local_debug = candidate_list
            .find("./target/debug/rigger")
            .expect("local target debug candidate is tried");
        let idx_unit_release = candidate_list
            .find("$unit_release")
            .expect("unit-derived cargo-target release candidate is tried");
        let idx_unit_debug = candidate_list
            .find("$unit_debug")
            .expect("unit-derived cargo-target debug candidate is tried");
        let idx_shared_debug = candidate_list
            .find("$shared_debug")
            .expect("shared step-cache debug candidate is tried");
        // PATH is consulted only as the fallback AFTER the candidate loop closes.
        let idx_path_fallback = hook[loop_end..]
            .find("command -v rigger")
            .map(|i| loop_end + i)
            .expect("PATH fallback candidate is tried after the loop");
        assert!(
            idx_env_release < idx_env_debug
                && idx_env_debug < idx_local_release
                && idx_local_release < idx_local_debug
                && idx_local_debug < idx_unit_release
                && idx_unit_release < idx_unit_debug
                && idx_unit_debug < idx_shared_debug
                && idx_shared_debug < idx_path_fallback,
            "candidates must be TRIED in the spec's exact order (env dir release/debug, local \
             target release/debug, unit-derived cargo-target release/debug, shared step-cache \
             debug, PATH last); got:\n{hook}"
        );

        // The unit-derived candidates ($unit_release/$unit_debug) are POINTED at paths keyed
        // by the worktree directory name `rigger-wt-<unit>`, and the shared step-cache
        // candidate ($shared_debug) is a DISTINCT debug-only path with no `-$unit` segment.
        assert!(
            hook.contains("rigger-wt-*")
                && hook.contains("cargo-target-$unit/release/rigger")
                && hook.contains("cargo-target-$unit/debug/rigger"),
            "the unit-derived candidates are keyed off the worktree directory name \
             `rigger-wt-<unit>`; got:\n{hook}"
        );
        assert!(
            hook.contains("/.rigger/tmp/cargo-target/debug/rigger")
                && !hook.contains("/.rigger/tmp/cargo-target/release/rigger"),
            "the shared step-cache candidate is DEBUG ONLY (unit gates build the debug profile \
             only); got:\n{hook}"
        );

        // Both the render call and the provenance line invoke the RESOLVED binary, never a
        // bare unqualified `rigger` (spec 75 done-when 1: "invokes the resolved binary for
        // both the render and the provenance line").
        assert!(
            hook.contains("\"$rigger_bin\" docs"),
            "the docs render must invoke the resolved candidate binary; got:\n{hook}"
        );
        assert!(
            hook.contains("\"$rigger_bin\" version"),
            "the provenance line must invoke the resolved candidate binary; got:\n{hook}"
        );
        assert!(
            !hook.contains("if rigger docs"),
            "the old bare-unqualified render invocation must be gone; got:\n{hook}"
        );
        assert!(
            !hook.contains("$(rigger version"),
            "the old bare-unqualified provenance invocation must be gone; got:\n{hook}"
        );

        // The top-level availability gate now covers EVERY candidate (including PATH), not
        // PATH alone - a wrong/stale candidate can only ever convert a false refusal into a
        // pass when the render genuinely matches (safe-closed), never the reverse.
        assert!(
            hook.contains("[ -n \"$rigger_bin\" ]"),
            "the block gates on a RESOLVED binary (any candidate), not PATH alone; got:\n{hook}"
        );
    }

    /// spec 24, crit 1 (idempotency, pure): re-composing an already-installed hook is a
    /// fixed point - the sentinel-marked block appears exactly once, so a `rigger setup`
    /// rerun never duplicates it (the property `install_precommit_hook` reports as
    /// `AlreadyCurrent`).
    #[test]
    fn compose_precommit_is_idempotent() {
        let once = compose_precommit(None);
        let twice = compose_precommit(Some(&once));
        assert_eq!(
            once, twice,
            "re-composing an installed hook changes nothing"
        );
        assert_eq!(
            once.matches(PRECOMMIT_BEGIN).count(),
            1,
            "the managed block is never duplicated"
        );
    }

    /// spec 24, crit 2 (non-clobbering chaining, pure): composing onto a pre-existing hook
    /// PRESERVES the existing commands and inserts rigger's block right after the shebang -
    /// BEFORE the existing hook body, not after it. Prepending is what keeps rigger's block
    /// reachable when the existing hook ends in a terminal `exit 0` (see
    /// `compose_precommit_prepends_before_a_terminal_exit_existing_hook`): rigger's block ends
    /// in a bare `true` (never `exit`), so the existing hook still runs after it and BOTH run.
    /// Re-composing the chained form stays a fixed point (block appears once). Supersedes the
    /// crit-1 append-after ordering (d24-11 / d24-2-prepend-fixes-terminal-shadow).
    #[test]
    fn compose_precommit_chains_without_clobbering_an_existing_hook() {
        let existing = "#!/bin/sh\necho existing-hook-ran\nmake lint\n";
        let chained = compose_precommit(Some(existing));
        assert!(
            chained.contains("echo existing-hook-ran") && chained.contains("make lint"),
            "the existing hook's commands are preserved; got:\n{chained}"
        );
        assert!(chained.contains(PRECOMMIT_BEGIN), "rigger's block is added");
        assert!(
            chained.starts_with("#!/bin/sh\n"),
            "the shebang stays on line 1 so git still runs the hook; got:\n{chained}"
        );
        let user_pos = chained.find("echo existing-hook-ran").unwrap();
        let block_pos = chained.find(PRECOMMIT_BEGIN).unwrap();
        assert!(
            block_pos < user_pos,
            "rigger's block is PREPENDED after the shebang, before the existing hook body, so a \
             terminal existing hook cannot shadow it; got:\n{chained}"
        );
        let again = compose_precommit(Some(&chained));
        assert_eq!(
            chained, again,
            "re-composing the chained hook is a fixed point"
        );
        assert_eq!(
            again.matches(PRECOMMIT_BEGIN).count(),
            1,
            "no duplicate block on a chained rerun"
        );
    }

    /// spec 24, crit 2 (non-clobbering chaining defeats a terminal existing hook, pure): the
    /// modal hand-written / sample pre-commit hook ends in a terminal `exit 0`. If rigger's
    /// block were APPENDED after such a hook it would never be reached and the docs would
    /// silently not regenerate (adv-u24-1r-chained-terminal-hook-shadows-rigger-block-silently
    /// / d24-11). Prepending after the shebang puts rigger's block BEFORE the terminal `exit 0`
    /// so it always runs, and rigger's own block ends in a bare `true` so the existing hook
    /// (including its `exit 0`) still runs after it - BOTH run.
    #[test]
    fn compose_precommit_prepends_before_a_terminal_exit_existing_hook() {
        let terminal = "#!/bin/sh\necho user-hook-ran\nexit 0\n";
        let chained = compose_precommit(Some(terminal));
        let block_pos = chained.find(PRECOMMIT_BEGIN).unwrap();
        let exit_pos = chained.find("exit 0").unwrap();
        assert!(
            block_pos < exit_pos,
            "rigger's block must come BEFORE the existing hook's terminal `exit 0`, or it would \
             be shadowed and never run; got:\n{chained}"
        );
        assert!(
            chained.contains("echo user-hook-ran"),
            "the existing terminal hook is preserved in full; got:\n{chained}"
        );
    }

    /// spec 24, crit 2 (idempotency + non-clobbering, byte level): a pre-existing pre-commit
    /// hook that is NOT valid UTF-8 (e.g. a compiled/binary hook, or one carrying non-UTF-8
    /// bytes) must NEVER be clobbered by a fresh script (sdet-u24-1r-nonutf8-clobber-persists /
    /// d24-2-nonutf8-byte-compose-no-clobber). `install_precommit_hook` reads the hook as BYTES
    /// and composes at the byte level, so the original bytes are preserved and rigger's block
    /// is chained, and a rerun is a fixed point (no duplicate block).
    #[test]
    fn install_precommit_hook_preserves_a_non_utf8_existing_hook() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        // git_hooks_dir falls back to <root>/.git/hooks when git cannot be consulted.
        let hooks = root.join(".git").join("hooks");
        std::fs::create_dir_all(&hooks).unwrap();
        let hook_path = hooks.join("pre-commit");
        // A valid ASCII shebang line then deliberately non-UTF-8 bytes in the body.
        let mut original: Vec<u8> = b"#!/bin/sh\n".to_vec();
        original.extend_from_slice(&[0xff, 0xfe, b'\n']);
        std::fs::write(&hook_path, &original).unwrap();

        let outcome = install_precommit_hook(root).unwrap();
        assert_eq!(
            outcome,
            InstallOutcome::Refreshed,
            "an existing hook is refreshed, not freshly installed"
        );
        let written = std::fs::read(&hook_path).unwrap();
        assert!(
            written
                .windows(original.len())
                .any(|w| w == original.as_slice())
                || written.windows(3).any(|w| w == [0xff, 0xfe, b'\n']),
            "the original non-UTF-8 hook bytes must be preserved, never clobbered"
        );
        assert!(
            written
                .windows(PRECOMMIT_BEGIN.len())
                .any(|w| w == PRECOMMIT_BEGIN.as_bytes()),
            "rigger's managed block is chained onto the non-UTF-8 hook"
        );

        // Idempotent: re-installing over the chained non-UTF-8 hook changes nothing and never
        // duplicates the block.
        let again = install_precommit_hook(root).unwrap();
        assert_eq!(
            again,
            InstallOutcome::AlreadyCurrent,
            "a rerun over the chained non-UTF-8 hook is a true no-op"
        );
        let rewritten = std::fs::read(&hook_path).unwrap();
        assert_eq!(
            written, rewritten,
            "the non-UTF-8 chained hook is a fixed point"
        );
        let begins = rewritten
            .windows(PRECOMMIT_BEGIN.len())
            .filter(|w| *w == PRECOMMIT_BEGIN.as_bytes())
            .count();
        assert_eq!(begins, 1, "the managed block is never duplicated");
    }

    /// spec 24, crit 1 (refresh-in-place, pure): a stale managed block (an older rigger
    /// build wrote it, or a hand-edit) is REPLACED with the current block, bounded by its
    /// sentinels, so refresh never leaks stale lines and never disturbs a chained hook's own
    /// commands on either side of the block.
    #[test]
    fn compose_precommit_refreshes_a_stale_block_in_place() {
        let stale = format!(
            "#!/bin/sh\necho keep-me\n{PRECOMMIT_BEGIN}\nstale garbage a new build no longer \
             emits\n{PRECOMMIT_END}\necho trailing-keep\n"
        );
        let refreshed = compose_precommit(Some(&stale));
        assert!(
            !refreshed.contains("stale garbage"),
            "the stale block body is gone; got:\n{refreshed}"
        );
        assert!(
            refreshed.contains("rigger docs"),
            "the current block body is present"
        );
        assert!(
            refreshed.contains("echo keep-me") && refreshed.contains("echo trailing-keep"),
            "surrounding hook lines on both sides of the block are preserved; got:\n{refreshed}"
        );
        assert_eq!(
            refreshed.matches(PRECOMMIT_BEGIN).count(),
            1,
            "still exactly one managed block"
        );
    }

    /// spec 21, unit 3: the exact line `rigger peers` PRINTS for a decision must label its
    /// provenance - LIVE for a decision from the active run, HISTORICAL for one from a
    /// superseded run - from the `live` flag the shared `peers_json` core threads through.
    /// Asserting on the rendered line (not just the JSON) closes the gap where the printed
    /// label was untested (sdet-u21peers-cmdpeers-render-label-untested).
    #[test]
    fn cmd_peers_prints_live_or_historical_per_decision_provenance() {
        let live = peer_decision_line(&serde_json::json!({
            "id": "d_new", "summary": "chose X", "governs": ["a.rs"], "live": true,
        }));
        assert_eq!(live, "decision d_new | LIVE | chose X | governs: a.rs");

        let historical = peer_decision_line(&serde_json::json!({
            "id": "d_old", "summary": "chose Y", "governs": ["b.rs"], "live": false,
        }));
        assert_eq!(
            historical,
            "decision d_old | HISTORICAL | chose Y | governs: b.rs"
        );

        // A missing `live` flag renders HISTORICAL - the conservative default.
        let defaulted = peer_decision_line(&serde_json::json!({
            "id": "d_bare", "summary": "z", "governs": [],
        }));
        assert_eq!(defaulted, "decision d_bare | HISTORICAL | z | governs: -");
    }

    /// Spec 21, unit 2: the drop-set derivation `rigger reset --runs` hands to the prune. It
    /// reuses the SINGLE run-attribution authority (`run_attribution` + `current_run_id`), so a
    /// SUPERSEDED run's decision/finding AND a PRE-BOUNDARY one (recorded before the first
    /// `RunStarted`) are both dropped, while every `LessonLearned` (even a pre-boundary one) and
    /// the ACTIVE run's decisions/findings are preserved. Index-keyed off the whole forward
    /// stream, mapping each dropped index back to its event body's id; sorted + de-duplicated.
    ///
    /// Two hazards are pinned here that the naive skip-live-index derivation gets wrong:
    /// - KEEP INVARIANT under cross-run id reuse: `shared-d` is recorded in BOTH the dead run
    ///   r1 AND the active run r2, so its dead-run index is a drop candidate while its
    ///   active-run index is live. The one graph node must be PRESERVED - the drop set is the
    ///   candidates MINUS the active run's node ids, not merely the non-live indices.
    /// - SENTINEL arm: a dead-run decision with an EMPTY id and one with a MALFORMED (non-JSON)
    ///   body must be SKIPPED (`graph_node_id` returns `None`), never panicking and never
    ///   contributing a bogus id to the drop set.
    #[test]
    fn superseded_graph_nodes_drops_dead_runs_and_preboundary_keeping_lessons_active_and_reused_ids(
    ) {
        fn ev(type_: &str, data: &str) -> Event {
            Event::new(type_, data.as_bytes().to_vec())
        }
        fn run_started(run: &str) -> Event {
            ev(
                runscope::TYPE_RUN_STARTED,
                &format!(r#"{{"run":"{run}","criteria":["crit"]}}"#),
            )
        }
        let events = vec![
            // Pre-boundary (before any RunStarted): decision + finding DROP, lesson KEEPS.
            ev(contextgraph::TYPE_DECISION_MADE, r#"{"id":"pre-d"}"#),
            ev(contextgraph::TYPE_REVIEW_FINDING, r#"{"id":"pre-f"}"#),
            ev(contextgraph::TYPE_LESSON_LEARNED, r#"{"id":"pre-lesson"}"#),
            run_started("r1"),
            // Superseded run r1: decision + finding DROP, lesson KEEPS.
            ev(contextgraph::TYPE_DECISION_MADE, r#"{"id":"r1-d"}"#),
            ev(contextgraph::TYPE_REVIEW_FINDING, r#"{"id":"r1-f"}"#),
            ev(contextgraph::TYPE_LESSON_LEARNED, r#"{"id":"r1-lesson"}"#),
            // A decision id reused across runs, recorded here in the DEAD run r1 first.
            ev(contextgraph::TYPE_DECISION_MADE, r#"{"id":"shared-d"}"#),
            // Sentinel arms in the dead run: an empty id and a malformed (non-JSON) body must
            // be skipped, never dropped and never panicking.
            ev(contextgraph::TYPE_DECISION_MADE, r#"{"id":""}"#),
            ev(contextgraph::TYPE_REVIEW_FINDING, "not json at all"),
            run_started("r2"),
            // Active run r2: decision + finding KEEP, lesson KEEPS.
            ev(contextgraph::TYPE_DECISION_MADE, r#"{"id":"r2-d"}"#),
            ev(contextgraph::TYPE_REVIEW_FINDING, r#"{"id":"r2-f"}"#),
            ev(contextgraph::TYPE_LESSON_LEARNED, r#"{"id":"r2-lesson"}"#),
            // The SAME reused id recorded again in the ACTIVE run r2: the node must survive.
            ev(contextgraph::TYPE_DECISION_MADE, r#"{"id":"shared-d"}"#),
        ];

        let drop = superseded_graph_nodes(&events);
        assert_eq!(
            drop,
            vec!["pre-d", "pre-f", "r1-d", "r1-f"],
            "exactly the dead-run + pre-boundary decisions/findings, sorted; lessons, the active \
             run (r2), a cross-run-reused id, and malformed/empty-id events are all preserved"
        );
    }

    /// Spec 41: the retention cutoff `rigger reset --runs` hands to the extended prune for the
    /// superseded-edge reclamation. It is the ACTIVE run's `RunStarted` `valid_from` in the graph's
    /// nanosecond-since-epoch time base (an edge's stored `valid_to`), derived from the SAME
    /// `run::current_run` boundary the node drop-set uses - so a superseded edge retired before the
    /// active run is reclaimed and one retired during it is kept. With NO run started (a legacy
    /// store) it is `None`, so nothing is reclaimed and LIVE plus recent history are both untouched.
    #[test]
    fn superseded_edge_boundary_is_the_active_runs_start_or_none_without_a_run() {
        use std::time::{Duration, UNIX_EPOCH};
        fn run_started_at(run: &str, secs: u64) -> Event {
            Event::new(
                runscope::TYPE_RUN_STARTED,
                format!(r#"{{"run":"{run}","criteria":["crit"]}}"#).into_bytes(),
            )
            .with_valid_from(UNIX_EPOCH + Duration::from_secs(secs))
        }
        fn decision(id: &str, secs: u64) -> Event {
            Event::new(
                contextgraph::TYPE_DECISION_MADE,
                format!(r#"{{"id":"{id}"}}"#).into_bytes(),
            )
            .with_valid_from(UNIX_EPOCH + Duration::from_secs(secs))
        }

        // No RunStarted at all (a legacy store): no boundary, so the reclamation is skipped entirely.
        let legacy = vec![decision("d0", 50)];
        assert_eq!(
            superseded_edge_boundary(&legacy),
            None,
            "with no run started there is no boundary - nothing is reclaimed"
        );

        // Two runs: the cutoff is the LATEST (active) run's start (300s), NOT the prior run's (100s),
        // so an edge superseded during run r1 (before 300s) is reclaimable and r2's own is retained.
        let events = vec![
            run_started_at("r1", 100),
            decision("r1-d", 150),
            run_started_at("r2", 300),
            decision("r2-d", 350),
        ];
        assert_eq!(
            superseded_edge_boundary(&events),
            Some(Duration::from_secs(300).as_nanos() as i64),
            "the boundary is the active run's RunStarted valid_from in the edge time base (nanos)"
        );
    }

    /// Spec 43, criterion 4 (CONSUMERS ARE UNAFFECTED). The graph fold de-noises to the target
    /// project: it stopped projecting the loop's own run machinery (the agent / unit / gate NODES,
    /// the `agent --TOUCHES--> file` edge, and the agent-attribution edges). This test proves the
    /// three named functional consumers produce the SAME result before and after that de-noise,
    /// because NONE of them reads a dropped node - each reads a substrate the fold change never
    /// touches:
    ///
    ///   * `metrics::project` folds the EVENT LOG. The de-noise removed the graph NODES for
    ///     `GateVerdict` / `UnitStarted` / `UnitIntegrated`, but those EVENTS still stand in the
    ///     log, and metrics reads them from there (it never receives a `Projection`), so its counts
    ///     are unmoved.
    ///   * run pruning (`superseded_graph_nodes` -> `Projector::prune`) derives its drop set from
    ///     the EVENT LOG through the one `run::run_attribution` authority, which attributes ONLY
    ///     decisions / findings / lessons - never a `UnitStarted` / `FileTouched` / `GateVerdict`.
    ///     So no machinery id can enter the drop set, the derivation is byte-identical whether or
    ///     not the machinery events are present, and pruning the de-noised graph (which has no
    ///     machinery nodes) still drops exactly the dead-run content and keeps the active run's.
    ///   * blast radius grounds over the CODE cross-reference (here the always-available `Grep`
    ///     default over the source tree; the `symbols` grounder likewise reads its symbol index) -
    ///     never the context graph - so a graph-only change cannot alter a radius.
    ///
    /// This owns the safe-consumer guarantee; it deliberately does NOT re-assert content survival
    /// (criterion 2 owns that). The event stream carries the machinery in its RAW production shape
    /// (a `UnitStarted` with both `id` and `unit`, a `GateVerdict` with `pass`, a `FileTouched`
    /// with `by`) - the exact payloads the log records and the de-noise now ignores.
    #[test]
    fn the_denoise_leaves_metrics_run_pruning_and_blast_radius_unaffected() {
        use rigger::grounder::{Grep, Grounder};

        // One positioned event in raw on-log JSON. Distinct positions are required: the graph fold
        // dedups on position (`INSERT OR IGNORE INTO applied`), and metrics / attribution key by
        // index, so a monotonic position per event models the real append order.
        fn ev(pos: u64, type_: &str, json: serde_json::Value) -> Event {
            let mut e = Event::new(type_, serde_json::to_vec(&json).unwrap());
            e.position = pos;
            e
        }

        // A whole run stream spanning a DEAD run r1 and the ACTIVE run r2, each interleaving the
        // machinery the de-noise dropped (FileTouched / UnitStarted / GateVerdict / UnitIntegrated)
        // with the content (DecisionMade / ReviewFinding) and the unit lifecycle metrics folds.
        let stream = vec![
            // --- Dead run r1 ---
            ev(
                1,
                runscope::TYPE_RUN_STARTED,
                serde_json::json!({ "run": "r1", "criteria": ["c"] }),
            ),
            ev(
                2,
                contextgraph::TYPE_FILE_TOUCHED,
                serde_json::json!({ "path": "src/combat.rs", "by": "rust-engineer" }),
            ),
            ev(
                3,
                ledger::TYPE_UNIT_STARTED,
                serde_json::json!({ "id": "u_r1", "unit": "u_r1", "criterion": "c1", "agent": "rust-engineer", "needs": [] }),
            ),
            ev(
                4,
                contextgraph::TYPE_GATE_VERDICT,
                serde_json::json!({ "gate": "build", "pass": true }),
            ),
            ev(
                5,
                contextgraph::TYPE_DECISION_MADE,
                serde_json::json!({ "id": "d_r1", "summary": "dead-run decision", "governs": ["src/combat.rs"], "supersedes": "" }),
            ),
            ev(
                6,
                contextgraph::TYPE_REVIEW_FINDING,
                serde_json::json!({ "id": "f_r1", "by": "tech-lens", "unit": "u_r1", "summary": "dead-run finding", "about": ["src/combat.rs"] }),
            ),
            ev(
                7,
                ledger::TYPE_UNIT_STATUS,
                serde_json::json!({ "id": "u_r1", "status": "verified" }),
            ),
            ev(
                8,
                ledger::TYPE_UNIT_STATUS,
                serde_json::json!({ "id": "u_r1", "status": "reviewed" }),
            ),
            ev(
                9,
                ledger::TYPE_UNIT_INTEGRATED,
                serde_json::json!({ "id": "u_r1", "commit": "abc1" }),
            ),
            // --- Active run r2 ---
            ev(
                10,
                runscope::TYPE_RUN_STARTED,
                serde_json::json!({ "run": "r2", "criteria": ["c"] }),
            ),
            ev(
                11,
                contextgraph::TYPE_FILE_TOUCHED,
                serde_json::json!({ "path": "src/combat.rs", "by": "rust-engineer" }),
            ),
            ev(
                12,
                ledger::TYPE_UNIT_STARTED,
                serde_json::json!({ "id": "u_r2", "unit": "u_r2", "criterion": "c1", "agent": "rust-engineer", "needs": [] }),
            ),
            ev(
                13,
                contextgraph::TYPE_GATE_VERDICT,
                serde_json::json!({ "gate": "clippy", "pass": true }),
            ),
            ev(
                14,
                contextgraph::TYPE_DECISION_MADE,
                serde_json::json!({ "id": "d_r2", "summary": "active-run decision", "governs": ["src/combat.rs"], "supersedes": "" }),
            ),
            ev(
                15,
                ledger::TYPE_UNIT_STATUS,
                serde_json::json!({ "id": "u_r2", "status": "verified" }),
            ),
            ev(
                16,
                ledger::TYPE_UNIT_STATUS,
                serde_json::json!({ "id": "u_r2", "status": "reviewed" }),
            ),
            ev(
                17,
                ledger::TYPE_UNIT_INTEGRATED,
                serde_json::json!({ "id": "u_r2", "commit": "abc2" }),
            ),
        ];

        // ===================================================================================
        // CONSUMER 1 - metrics::project folds the EVENT LOG, machinery events and all.
        // ===================================================================================
        // The de-noise stopped projecting `GateVerdict` / `UnitStarted` / `UnitIntegrated` as graph
        // nodes, but metrics reads those events from the log - so it still tallies two started
        // units, two clean first passes, both gates, and two review approvals. Asserting the
        // headline fields (not the whole struct) keeps the pin focused on the de-noised event types
        // without coupling to the unrelated review-quality fold.
        let m = metrics::project(&stream);
        assert_eq!(
            m.units_started, 2,
            "both UnitStarted events fold from the log"
        );
        assert_eq!(
            m.first_pass_clean, 2,
            "both units integrated with no failure - metrics reads the lifecycle from the log, not the graph"
        );
        assert_eq!(
            m.gates.get("build").map(|g| (g.pass, g.fail)),
            Some((1, 0)),
            "the build GateVerdict is still folded from the log though it is no longer a graph node"
        );
        assert_eq!(
            m.gates.get("clippy").map(|g| (g.pass, g.fail)),
            Some((1, 0)),
            "the clippy GateVerdict is still folded from the log"
        );
        assert_eq!(m.units_escalated, 0, "no unit escalated");
        assert_eq!(
            m.review_approve, 2,
            "both `reviewed` statuses count as approvals"
        );
        assert_eq!(m.review_reject, 0, "no review rejected");

        // ===================================================================================
        // CONSUMER 2 - run pruning derives its drop set from the EVENT LOG.
        // ===================================================================================
        // `superseded_graph_nodes` reuses `run::run_attribution`, which attributes ONLY
        // decision / finding / lesson events - so the machinery events (a `UnitStarted` carrying an
        // `id`, a `FileTouched`, a `GateVerdict`, a `UnitIntegrated`) contribute NOTHING to the drop
        // set, and it is exactly the dead run's decision and finding.
        let drop = superseded_graph_nodes(&stream);
        assert_eq!(
            drop,
            vec!["d_r1", "f_r1"],
            "the drop set is precisely the dead run's content - no machinery id (u_r1, u_r2, build, clippy, src/combat.rs) leaks in"
        );

        // The derivation is UNAFFECTED by the machinery events' presence: stripping every
        // FileTouched / UnitStarted / GateVerdict / UnitStatus / UnitIntegrated from the stream (the
        // events the de-noise stopped projecting) leaves the drop set byte-identical. This is the
        // "same result before and after the de-noise" guarantee for the pruning consumer.
        let content_only: Vec<Event> = stream
            .iter()
            .filter(|e| {
                e.type_ == runscope::TYPE_RUN_STARTED
                    || e.type_ == contextgraph::TYPE_DECISION_MADE
                    || e.type_ == contextgraph::TYPE_REVIEW_FINDING
                    || e.type_ == contextgraph::TYPE_LESSON_LEARNED
            })
            .cloned()
            .collect();
        assert_eq!(
            superseded_graph_nodes(&content_only),
            drop,
            "removing the machinery events does not change the pruning drop set - it reads only the run windows and the content ids"
        );

        // End-to-end: fold the whole run into the DE-NOISED graph (which projects no machinery
        // node), then run the real prune. It still drops exactly the dead-run content and keeps the
        // active run's decision plus the code it governs.
        let graph = Projector::open(":memory:", "test").unwrap();
        for e in &stream {
            graph.apply(e).unwrap();
        }
        let boundary = superseded_edge_boundary(&stream);
        graph.prune(&drop, boundary).unwrap();

        let g = graph
            .subgraph(
                &[
                    "d_r1".to_string(),
                    "f_r1".to_string(),
                    "d_r2".to_string(),
                    "src/combat.rs".to_string(),
                ],
                2,
            )
            .unwrap();
        assert!(
            g.nodes
                .iter()
                .any(|n| n.id == "d_r2" && n.kind == contextgraph::KIND_DECISION),
            "the active run's decision survives the prune; got {:?}",
            g.nodes.iter().map(|n| (&n.id, &n.kind)).collect::<Vec<_>>()
        );
        assert!(
            g.edges.iter().any(|e| e.rel == contextgraph::REL_GOVERNS
                && e.from == "d_r2"
                && e.to == "src/combat.rs"),
            "and its GOVERNS edge to the code it concerns survives"
        );
        assert!(
            !g.nodes.iter().any(|n| n.id == "d_r1"),
            "the dead run's decision is pruned"
        );
        assert!(
            !g.nodes.iter().any(|n| n.id == "f_r1"),
            "the dead run's finding is pruned"
        );

        // ===================================================================================
        // CONSUMER 3 - blast radius grounds over the CODE cross-reference, never the graph.
        // ===================================================================================
        // A blast radius is a function of the source tree (via the `Grep` default here, or the
        // `symbols` grounder's cross-reference index), never of the context graph - so removing
        // machinery graph nodes cannot move it. The radius of `apply_damage` covers the file that
        // defines it and the file that references it, and excludes an unrelated file.
        let repo = tempfile::tempdir().unwrap();
        std::fs::write(
            repo.path().join("combat.rs"),
            "pub fn apply_damage(target: &mut Enemy) { target.hp -= 1; }\n",
        )
        .unwrap();
        std::fs::write(
            repo.path().join("enemy.rs"),
            "fn hit(e: &mut Enemy) { apply_damage(e); }\n",
        )
        .unwrap();
        std::fs::write(repo.path().join("audio.rs"), "pub fn play_sound() {}\n").unwrap();
        let grep = Grep {
            root: repo.path().to_string_lossy().into_owned(),
        };
        let br = grep.blast_radius("apply_damage", 8);
        assert!(
            br.safe.iter().any(|f| f == "combat.rs"),
            "the safe radius covers the file that DEFINES apply_damage; got {:?}",
            br.safe
        );
        assert!(
            br.safe.iter().any(|f| f == "enemy.rs"),
            "and the file that REFERENCES it; got {:?}",
            br.safe
        );
        assert!(
            !br.safe.iter().any(|f| f == "audio.rs"),
            "an unrelated file is not in the radius; got {:?}",
            br.safe
        );
        assert!(
            !br.serialize,
            "apply_damage is not a hub, so the radius does not serialize"
        );
    }

    /// The single-source version line must carry BOTH the go-gitsemver-derived version
    /// (spec 74) and the (non-empty) embedded build provenance, so `rigger version` /
    /// `--version` can identify the exact binary. Pins the format helper both invocation
    /// arms print. The derived-version VALUE (successful derivation vs. the
    /// `+unversioned` fallback) is proven at the derivation seam by
    /// `tests/gitsemver_derivation.rs` against fixture repositories and the real
    /// binary; this test pins only that `version_line` routes through it.
    #[test]
    fn version_line_carries_the_derived_version_and_a_non_empty_build_provenance() {
        assert!(
            !GITSEMVER_VERSION.is_empty(),
            "build.rs must embed a non-empty go-gitsemver-derived version"
        );
        assert!(
            !BUILD_PROVENANCE.is_empty(),
            "build.rs must embed a non-empty build-provenance id"
        );
        let line = version_line();
        assert!(
            line.contains(GITSEMVER_VERSION),
            "version line must report the derived version; got: {line}"
        );
        assert!(
            line.contains(BUILD_PROVENANCE),
            "version line must report the build-provenance id; got: {line}"
        );
    }

    /// Spec 20, unit 1: the render pipeline's context is populated FROM the code the
    /// runtime uses, so no discipline fact is hand-copied. Each field must equal the
    /// SAME const / enum / registry the binary runs on - the wiring that makes changing
    /// a source fact change the render.
    #[test]
    fn docs_context_reads_every_fact_from_code() {
        let ctx = docs_context();
        assert_eq!(ctx.base_ref, DEFAULT_BASE_REF);
        assert_eq!(ctx.dash_port, dash::DEFAULT_PORT);
        assert_eq!(ctx.max_retries, rigger::safety::MAX_RETRIES);
        assert_eq!(ctx.verdict_approve, conductor::VERDICT_APPROVE);
        assert_eq!(ctx.spec_shape_recommendation, spec::SHAPE_RECOMMENDATION);
        assert_eq!(
            ctx.spec_shape_rules,
            vec![
                spec::ShapeRule::MultiBehavior.name(),
                spec::ShapeRule::SubBulletAsUnit.name(),
                spec::ShapeRule::OverLong.name()
            ]
        );
        assert_eq!(
            ctx.subcommands,
            SUBCOMMANDS
                .iter()
                .map(|c| c.to_string())
                .collect::<Vec<_>>()
        );
        // Spec 69, criterion 1: the watch facts `rigger-watch-a-run`/`rigger-diagnose-churn`
        // interpolate must be the SAME values `rigger watch` itself uses, not a hand copy.
        let expected_signals = [
            watch::Signal::Escalated,
            watch::Signal::DeadDriver,
            watch::Signal::DashNotServing,
            watch::Signal::RejectRecurrence,
            watch::Signal::FrontierStall,
        ]
        .map(|signal| rigger::docs::WatchSignalFact {
            name: signal.name().to_string(),
            response: signal.response().to_string(),
        });
        assert_eq!(ctx.watch_signals, expected_signals);
        assert_eq!(ctx.watch_poll_interval_secs, watch::DEFAULT_INTERVAL_SECS);
        assert_eq!(
            ctx.reject_recurrence_diagnose_threshold,
            watch::REJECT_RECURRENCE_DIAGNOSE_THRESHOLD
        );
    }

    /// Spec 20, unit 1 (the golden fact test): known code facts appear VERBATIM in BOTH
    /// rendered outputs, read live from the consts. A render that hard-copied a different
    /// literal instead of interpolating the context would diverge from the live const and
    /// fail here - so this ties the rendered document to the code, not a hand-copy.
    #[test]
    fn docs_render_surfaces_known_code_facts_verbatim() {
        let ctx = docs_context();
        let skill = rigger::docs::render_using_rigger_skill(&ctx);
        let handbook = rigger::docs::render_handbook_discipline(&ctx);
        for out in [&skill, &handbook] {
            assert!(
                out.contains(DEFAULT_BASE_REF),
                "base ref not verbatim in render"
            );
            assert!(
                out.contains(&dash::DEFAULT_PORT.to_string()),
                "dash port not verbatim in render"
            );
            assert!(
                out.contains(&rigger::safety::MAX_RETRIES.to_string()),
                "retry bound not verbatim in render"
            );
            assert!(
                out.contains(conductor::VERDICT_APPROVE),
                "verdict word not verbatim in render"
            );
            assert!(
                out.contains(spec::ShapeRule::MultiBehavior.name()),
                "spec-shape rule not verbatim in render"
            );
        }
        // The two outputs render from the ONE context: the skill also carries its loadable
        // frontmatter (distinguishing it from the handbook chapter).
        assert!(skill.starts_with("---\nname: using-rigger\n"));
        assert!(handbook.starts_with("# Using rigger: the operating discipline"));
    }

    /// Spec 20, unit 1: the `SUBCOMMANDS` registry is the single command surface the docs
    /// read - it must be non-empty, unique, name the commands the docs pipeline references,
    /// and stay in step with the dispatch (its own `docs` arm and the pre-existing ones).
    #[test]
    fn commands_registry_is_well_formed_and_covers_dispatch() {
        assert!(!SUBCOMMANDS.is_empty());
        let mut sorted = SUBCOMMANDS.to_vec();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(
            sorted.len(),
            SUBCOMMANDS.len(),
            "SUBCOMMANDS has a duplicate"
        );
        for cmd in SUBCOMMANDS {
            assert!(!cmd.is_empty(), "a command name is empty");
        }
        for expected in ["run", "step", "validate", "setup", "docs", "version"] {
            assert!(
                SUBCOMMANDS.contains(&expected),
                "SUBCOMMANDS must name the {expected:?} dispatch arm"
            );
        }
    }

    /// Spec 20, unit 1; spec 68, criterion 1: `rigger docs` renders EVERY registry skill
    /// plus the handbook and writes them to their committed paths under the project root.
    /// Proven against a temp root so it needs no process-cwd change: every file lands at
    /// its single-source path with the code facts (and, for a skill, the operator-binary
    /// prohibition) in it.
    #[test]
    fn write_docs_writes_every_registry_skill_plus_the_handbook() {
        let dir = tempfile::tempdir().unwrap();
        let written = write_docs(dir.path()).unwrap();
        let mut expected: Vec<std::path::PathBuf> = rigger::docs::skill_registry()
            .iter()
            .map(|e| dir.path().join(skill_source_rel(e.name)))
            .collect();
        expected.push(dir.path().join(HANDBOOK_DISCIPLINE_REL));
        assert_eq!(written, expected);

        let skill_path = dir.path().join(skill_source_rel("using-rigger"));
        let handbook_path = dir.path().join(HANDBOOK_DISCIPLINE_REL);
        let skill = std::fs::read_to_string(&skill_path).unwrap();
        let handbook = std::fs::read_to_string(&handbook_path).unwrap();
        assert!(skill.contains(DEFAULT_BASE_REF) && skill.contains("name: using-rigger"));
        assert!(skill.contains(rigger::docs::OPERATOR_BINARY_PROHIBITION));
        assert!(handbook.contains(DEFAULT_BASE_REF));
        // Byte-stable: a second render writes identical bytes (the drift check needs this).
        write_docs(dir.path()).unwrap();
        assert_eq!(std::fs::read_to_string(&skill_path).unwrap(), skill);
        assert_eq!(std::fs::read_to_string(&handbook_path).unwrap(), handbook);
    }

    /// Spec 68, criterion 1 (the structural pin): the SET of names `rigger setup` installs
    /// and the SET of skill paths `rigger docs` renders are each computed as EXACTLY
    /// `rigger::docs::skill_registry()`'s own names - never a second, hand-maintained list
    /// either surface could fall out of step with. Because both [`install_skills`] and
    /// [`write_docs`] loop over the registry directly (proven above and by this equality),
    /// adding an entry to the registry is the ONLY step that can make a skill install and
    /// render; neither surface can be updated without the other.
    #[test]
    fn install_and_docs_each_cover_exactly_the_registry_no_more_no_less() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let registry_names: Vec<&str> = rigger::docs::skill_registry()
            .iter()
            .map(|e| e.name)
            .collect();
        assert!(
            registry_names.len() >= 2,
            "the registry must carry at least using-rigger and planning-a-spec"
        );

        let installed_names: std::collections::BTreeSet<&str> = install_skills(root)
            .expect("install must succeed")
            .into_iter()
            .map(|(name, _)| name)
            .collect();
        let expected: std::collections::BTreeSet<&str> = registry_names.iter().copied().collect();
        assert_eq!(
            installed_names, expected,
            "rigger setup must install EXACTLY the registry's skills, no more, no less"
        );

        let written: std::collections::BTreeSet<std::path::PathBuf> =
            write_docs(root).unwrap().into_iter().collect();
        let mut expected_written: std::collections::BTreeSet<std::path::PathBuf> = registry_names
            .iter()
            .map(|name| root.join(skill_source_rel(name)))
            .collect();
        expected_written.insert(root.join(HANDBOOK_DISCIPLINE_REL));
        assert_eq!(
            written, expected_written,
            "rigger docs must render EXACTLY the registry's skill paths plus the (non-registry) \
             handbook, no more, no less"
        );
    }

    /// Spec 68, criterion 2 (the accuracy pin): every bare `rigger <cmd>` a per-operation
    /// skill teaches names a REAL entry in [`SUBCOMMANDS`] - the one dispatch registry the
    /// runtime and `rigger docs` both read. `rigger::docs` cannot see `SUBCOMMANDS` (it
    /// lives in the binary crate), so this pin lives here: if a command a skill teaches
    /// were ever dropped from dispatch, this test - not just an operator hitting a dead
    /// command - would catch it.
    #[test]
    fn per_operation_skills_reference_only_real_subcommands() {
        let ctx = docs_context();
        let cases: &[(&str, &[&str])] = &[
            ("rigger-reset-store", &["reset", "validate", "status"]),
            ("rigger-build-graph", &["graph"]),
            (
                "rigger-reindex",
                &["reindex", "graph", "ground", "validate"],
            ),
            (
                "rigger-resume-a-run",
                &["status", "run", "serve", "workflow", "step"],
            ),
            (
                "rigger-handle-an-escalation",
                &["status", "peers", "run", "serve"],
            ),
        ];
        let registry = rigger::docs::skill_registry();
        for (name, commands) in cases {
            let entry = registry
                .iter()
                .find(|e| e.name == *name)
                .unwrap_or_else(|| panic!("{name} must be in the registry"));
            let rendered = entry.render(&ctx);
            for cmd in *commands {
                assert!(
                    SUBCOMMANDS.contains(cmd),
                    "{name} references `rigger {cmd}`, but {cmd:?} is not in SUBCOMMANDS - \
                     the binary has no such command"
                );
                let literal = format!("rigger {cmd}");
                assert!(
                    rendered.contains(&literal),
                    "{name} must actually reference `{literal}` somewhere in its rendered \
                     content, not just claim to via this test's own table"
                );
            }
        }
    }

    /// Spec 69, criterion 1 (the accuracy pin, extending the spec-68 sibling
    /// [`per_operation_skills_reference_only_real_subcommands`] to the three watch-discipline
    /// skills): every bare `rigger <cmd>` `rigger-watch-a-run` / `rigger-restore-the-dash` /
    /// `rigger-diagnose-churn` teach names a REAL entry in [`SUBCOMMANDS`], and is literally
    /// present in the rendered output - so a dropped or renamed `rigger dash`, `rigger
    /// status`, `rigger watch`, or `rigger emit` reference in this family fails here, not
    /// just misleads an operator.
    #[test]
    fn watching_discipline_skills_reference_only_real_subcommands() {
        let ctx = docs_context();
        let cases: &[(&str, &[&str])] = &[
            ("rigger-watch-a-run", &["status", "watch"]),
            ("rigger-restore-the-dash", &["dash", "status", "watch"]),
            ("rigger-diagnose-churn", &["emit", "watch"]),
        ];
        let registry = rigger::docs::skill_registry();
        for (name, commands) in cases {
            let entry = registry
                .iter()
                .find(|e| e.name == *name)
                .unwrap_or_else(|| panic!("{name} must be in the registry"));
            let rendered = entry.render(&ctx);
            for cmd in *commands {
                assert!(
                    SUBCOMMANDS.contains(cmd),
                    "{name} references `rigger {cmd}`, but {cmd:?} is not in SUBCOMMANDS - \
                     the binary has no such command"
                );
                let literal = format!("rigger {cmd}");
                assert!(
                    rendered.contains(&literal),
                    "{name} must actually reference `{literal}` somewhere in its rendered \
                     content, not just claim to via this test's own table"
                );
            }
        }
    }

    /// Spec 20, unit 2 (the drift seam, at the unit level); spec 68, criterion 1 (the gate
    /// covers EVERY registry entry): `docs_drift` flags a committed output whose bytes
    /// differ from a fresh render, is SILENT when the committed copies are in sync, and
    /// SKIPS an absent file (an operator project that never carries rigger's own committed
    /// docs must not be flagged). Proven against a temp root so it needs no cwd.
    #[test]
    fn docs_drift_flags_a_changed_file_and_skips_absent_or_in_sync_ones() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let skill_path = root.join(skill_source_rel("using-rigger"));
        let other_skill_path = root.join(skill_source_rel("planning-a-spec"));
        let handbook_path = root.join(HANDBOOK_DISCIPLINE_REL);

        // Absent (nothing rendered yet) -> no drift, no failure: these are rigger's OWN docs,
        // which an operator project never carries, so their absence must not fail validate.
        assert!(
            docs_drift(root).is_empty(),
            "absent committed docs are not drift"
        );
        assert!(docs_drift_failure(root).is_none());

        // Rendered from code -> in sync -> still no drift.
        write_docs(root).unwrap();
        assert!(
            docs_drift(root).is_empty(),
            "freshly rendered docs must be in sync with a fresh render"
        );
        assert!(docs_drift_failure(root).is_none());

        // Hand-edit the using-rigger skill the render would never produce -> ONLY that
        // skill drifts (planning-a-spec stays in sync), and the failure names it plus the
        // `rigger docs` fix.
        std::fs::write(&skill_path, "hand-edited, not a render\n").unwrap();
        assert_eq!(
            docs_drift(root),
            vec![skill_path.clone()],
            "only the changed committed file is flagged"
        );
        let failure = docs_drift_failure(root).expect("a drifted skill must produce a failure");
        assert!(
            failure.contains(skill_source_rel("using-rigger").as_str())
                && failure.contains("rigger docs"),
            "the drift failure must name the drifted file and the `rigger docs` fix; got: {failure}"
        );

        // Drift the OTHER registry skill too -> both are reported, in registry order.
        std::fs::write(&other_skill_path, "hand-edited, not a render\n").unwrap();
        assert_eq!(
            docs_drift(root),
            vec![skill_path.clone(), other_skill_path.clone()],
            "both drifted skills are flagged, in registry order"
        );

        // Drift the handbook too -> it reports LAST, after every registry skill.
        std::fs::write(&handbook_path, "hand-edited handbook, not a render\n").unwrap();
        assert_eq!(
            docs_drift(root),
            vec![skill_path, other_skill_path, handbook_path]
        );
    }

    /// Spec 20, unit 2; spec 68, criterion 1 (the CI-lane guard, generalized over the whole
    /// registry): EVERY REAL committed registry skill plus the handbook discipline chapter
    /// must be byte-identical to a fresh render of the current code facts, so a changed
    /// const/template/registry entry that was NOT followed by `rigger docs` reddens `cargo
    /// test` in CI - not only `rigger validate` on a live checkout (the validate fixture
    /// renders fresh in a temp project, so it is always in sync THERE and cannot catch real
    /// repo drift). Reads the committed files from the crate manifest dir.
    #[test]
    fn committed_registry_docs_are_in_sync_with_a_fresh_render() {
        let ctx = docs_context();
        let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
        let mut checks: Vec<(std::path::PathBuf, String)> = rigger::docs::skill_registry()
            .into_iter()
            .map(|entry| {
                (
                    manifest.join(skill_source_rel(entry.name)),
                    entry.render(&ctx),
                )
            })
            .collect();
        checks.push((
            manifest.join(HANDBOOK_DISCIPLINE_REL),
            rigger::docs::render_handbook_discipline(&ctx),
        ));
        for (path, fresh) in checks {
            let committed = std::fs::read_to_string(&path)
                .unwrap_or_else(|e| panic!("read committed {}: {e}", path.display()));
            assert_eq!(
                committed,
                fresh,
                "the committed {} has drifted from a fresh render; run `rigger docs` and \
                 commit the result so the discipline matches the code",
                path.display()
            );
        }
    }

    /// Spec 19a, unit 1 (the shared current-blocker classifier): `rigger status` and the
    /// dashboard render the SAME one-line current-blocker per unfinished unit, from ONE
    /// classifier - covering building, reject-recurrence (#n/max), approved-not-integrated,
    /// escalated, and the run-level budget halt. Proven over the PRODUCTION render of each
    /// surface: the exact `Vec<String>` `cmd_status` prints (via `status_blocker_lines`)
    /// versus the `line` field the dashboard serializes into its `/api/state` snapshot (via
    /// `dash::build_state`). Byte-identical lines are the structural proof there is one
    /// shared classifier, not two that can drift.
    #[test]
    fn status_and_dashboard_render_the_same_current_blocker_lines() {
        use rigger::contextgraph::Graph;
        use std::collections::HashMap;

        // A run holding a unit in every classifier arm, plus a live budget halt. The
        // BudgetExhausted is LAST (highest position) so it is the current run-level blocker,
        // not a stale one a resume progressed past.
        let mut events = vec![
            Event::new(ledger::TYPE_UNIT_STARTED, br#"{"id":"u-build"}"#.to_vec()),
            Event::new(ledger::TYPE_UNIT_STARTED, br#"{"id":"u-fail"}"#.to_vec()),
            Event::new(
                ledger::TYPE_UNIT_FAILED,
                br#"{"id":"u-fail","attempts":2}"#.to_vec(),
            ),
            Event::new(ledger::TYPE_UNIT_STARTED, br#"{"id":"u-appr"}"#.to_vec()),
            Event::new(
                ledger::TYPE_UNIT_STATUS,
                br#"{"id":"u-appr","status":"reviewed"}"#.to_vec(),
            ),
            Event::new(ledger::TYPE_UNIT_STARTED, br#"{"id":"u-esc"}"#.to_vec()),
            Event::new(ledger::TYPE_UNIT_ESCALATED, br#"{"id":"u-esc"}"#.to_vec()),
            Event::new(
                conductor::TYPE_BUDGET_EXHAUSTED,
                br#"{"budget":200,"spawns":200}"#.to_vec(),
            ),
        ];
        for (i, e) in events.iter_mut().enumerate() {
            e.position = (i + 1) as u64;
        }
        let max_retries = 6;

        // The `rigger status` production render: the exact lines cmd_status prints.
        let status_lines = status_blocker_lines(&events, max_retries).unwrap();

        // The dashboard production render: the `line` fields in the /api/state snapshot.
        let state = dash::build_state(
            &events,
            &Graph::default(),
            false,
            &[],
            &HashMap::new(),
            max_retries,
            RUN_BRANCH,
            DEFAULT_BASE_REF,
        )
        .unwrap();
        let dash_lines: Vec<String> = state.blockers.iter().map(|b| b.line.clone()).collect();

        // One shared classifier: byte-identical lines on both surfaces.
        assert_eq!(
            status_lines, dash_lines,
            "rigger status and the dashboard must render identical current-blocker lines"
        );

        // Every required kind is covered, deterministically ordered (run-level budget first,
        // then units lexically).
        assert_eq!(
            status_lines,
            vec![
                "run: budget spent 200/200 (raise defaults.budget and resume)".to_string(),
                "u-appr: approved, not yet integrated (review passed; integration pending)"
                    .to_string(),
                "u-build: building (attempt 1)".to_string(),
                "u-esc: escalated (awaiting a human)".to_string(),
                "u-fail: reject-recurrence #2/6 (unknown)".to_string(),
            ]
        );
    }

    /// Spec 38, criterion 3 (the ready-to-release handoff): the exact lines the `rigger
    /// status` surface prints are non-empty and name the run branch, the release-target base,
    /// the integrated-unit count, and the PR command ONLY when the run is done; a run that is
    /// NOT done surfaces no release-ready signal. Proven over the production render seam
    /// (`release_ready_lines`) `cmd_status` prints, so the surface cannot silently drift from
    /// the one authority.
    #[test]
    fn release_ready_lines_surface_only_on_a_done_run() {
        // A done run: one integrated unit, no failed deferred gate.
        let done = [
            Event::new(ledger::TYPE_UNIT_STARTED, br#"{"id":"u1"}"#.to_vec()),
            Event::new(
                ledger::TYPE_UNIT_INTEGRATED,
                br#"{"id":"u1","commit":"abc"}"#.to_vec(),
            ),
        ];
        let lines = release_ready_lines(&done, RUN_BRANCH, DEFAULT_BASE_REF);
        assert!(
            !lines.is_empty(),
            "a done run surfaces the release-ready handoff"
        );
        let text = lines.join("\n");
        assert!(text.contains(RUN_BRANCH), "names the run branch: {text}");
        assert!(
            text.contains("1 unit"),
            "names the integrated-unit count: {text}"
        );
        // `origin/main` is stripped to the release-target branch in the PR command.
        assert!(
            text.contains("gh pr create --base main --head rigger-run"),
            "names the PR command: {text}"
        );

        // A run with a still-un-integrated unit surfaces NO release-ready signal.
        let running = [
            Event::new(ledger::TYPE_UNIT_STARTED, br#"{"id":"u1"}"#.to_vec()),
            Event::new(
                ledger::TYPE_UNIT_INTEGRATED,
                br#"{"id":"u1","commit":"abc"}"#.to_vec(),
            ),
            Event::new(ledger::TYPE_UNIT_STARTED, br#"{"id":"u2"}"#.to_vec()),
        ];
        assert!(
            release_ready_lines(&running, RUN_BRANCH, DEFAULT_BASE_REF).is_empty(),
            "an unfinished run surfaces no release-ready signal"
        );
    }

    #[test]
    fn status_and_dash_read_the_runs_persisted_base_not_a_re_resolution() {
        // The base-asymmetry fix (spec 38, criterion 3): a run started with an explicit
        // `--base` and NO `RIGGER_BASE` in the environment persists that base as `META_BASE`
        // on its RunStarted. `rigger status` and `rigger dash` run WITHOUT the run's `--base`
        // flag on their argv, so before this fix they re-resolved via `resolve_run_base(None,
        // env)` and named the WRONG default base for a `rigger run --base X` run. Now every
        // surface reads the ONE persisted base, so all of status/dash/print_run_state name the
        // base the run actually anchored on.

        // `rigger run --base release/2.0` with no `RIGGER_BASE` resolves to the flag base...
        let (flag_base, explicit) = resolve_run_base(Some("release/2.0"), None);
        assert_eq!(flag_base, "release/2.0");
        assert!(explicit, "an explicit --base is flagged explicit");

        // ...and that resolved base is stamped as `META_BASE` on the run's RunStarted, exactly
        // as `start_fresh`/`ensure_started_pinned` now stamp it at mint. A done run follows.
        let events = [
            Event::new(
                runscope::TYPE_RUN_STARTED,
                br#"{"run":"r1","criteria":[]}"#.to_vec(),
            )
            .with_meta(runscope::META_RUN_ID, "r1")
            .with_meta(runscope::META_BASE, &flag_base),
            Event::new(ledger::TYPE_UNIT_STARTED, br#"{"id":"u1"}"#.to_vec()),
            Event::new(
                ledger::TYPE_UNIT_INTEGRATED,
                br#"{"id":"u1","commit":"abc"}"#.to_vec(),
            ),
        ];

        // The persisted base is read straight back from the log - the single authority.
        assert_eq!(
            runscope::current_run_base(&events).as_deref(),
            Some("release/2.0")
        );

        // The status/dash read pattern (persisted base, else the env/default fallback) names
        // the flag base even though the surface's own argv has no `--base` and the env is
        // empty here - the parity the fix restores...
        let status_base =
            runscope::current_run_base(&events).unwrap_or_else(|| resolve_run_base(None, None).0);
        assert_eq!(status_base, "release/2.0");

        // ...whereas the OLD asymmetric re-resolution (the defect) named the DEFAULT, not the
        // run's base - documenting exactly the wrong PR command the persisted base eliminates.
        assert_eq!(resolve_run_base(None, None).0, DEFAULT_BASE_REF);
        assert_ne!(status_base, DEFAULT_BASE_REF);

        // Every surface renders through `release_ready`, so the PR command names the run's
        // actual base - not `main`.
        let text = release_ready_lines(&events, RUN_BRANCH, &status_base).join("\n");
        assert!(
            text.contains("gh pr create --base release/2.0 --head rigger-run"),
            "the PR command targets the run's persisted base: {text}"
        );

        // A run started BEFORE base persistence carries no `META_BASE`, so a surface falls back
        // to the live env/default resolution - the legacy behavior is preserved untouched.
        let legacy = [Event::new(
            runscope::TYPE_RUN_STARTED,
            br#"{"run":"r0","criteria":[]}"#.to_vec(),
        )
        .with_meta(runscope::META_RUN_ID, "r0")];
        assert_eq!(runscope::current_run_base(&legacy), None);
    }

    // ---- `rigger validate` advisories (spec 05:55): pure seams + drift compare ----

    #[test]
    fn dirty_tracked_paths_keeps_tracked_modifications_and_drops_untracked_and_ignored() {
        // A mix of porcelain status codes scoped to `.rigger/`: modified-in-worktree,
        // staged, added, deleted (all TRACKED), plus untracked (`??`) and ignored (`!!`).
        let porcelain = " M .rigger/workflow.yml\n\
                         M  .rigger/agents/sdet.md\n\
                         A  .rigger/agents/new.md\n\
                         D  .rigger/agents/gone.md\n\
                         ?? .rigger/events.db\n\
                         !! .rigger/shim/node_modules\n";
        let dirty = dirty_tracked_paths(porcelain);
        assert_eq!(
            dirty,
            vec![
                ".rigger/workflow.yml".to_string(),
                ".rigger/agents/sdet.md".to_string(),
                ".rigger/agents/new.md".to_string(),
                ".rigger/agents/gone.md".to_string(),
            ],
            "only TRACKED+modified paths are flagged; untracked `??` and ignored `!!` \
             entries are excluded"
        );
    }

    #[test]
    fn dirty_tracked_paths_on_a_clean_tree_is_empty() {
        assert!(dirty_tracked_paths("").is_empty());
    }

    #[test]
    fn installed_workflow_drifted_is_false_when_absent_or_identical_and_true_on_drift() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();

        // Absent: nothing installed, so there is no drift to surface.
        assert!(
            !installed_workflow_drifted(root),
            "an absent installed workflow is not drift"
        );

        // Identical to the embedded copy: not drift.
        let path = workflow_path(root);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, RIGGER_WORKFLOW).unwrap();
        assert!(
            !installed_workflow_drifted(root),
            "an installed workflow byte-identical to the embedded copy is not drift"
        );

        // Differs from the embedded copy: drift.
        std::fs::write(&path, "// stale installed workflow\n").unwrap();
        assert!(
            installed_workflow_drifted(root),
            "an installed workflow differing from the embedded copy IS drift"
        );
    }

    // ---- spec 18, criterion 9: workflow-drift "which side is stale" diagnostic --------

    #[test]
    fn drift_side_names_the_binary_stale_only_when_the_installed_workflow_is_provably_newer() {
        // This binary's build is a PROPER ANCESTOR of the build that wrote the installed
        // workflow: the installed workflow is newer, so the BINARY is stale.
        let binary_is_ancestor = |ancestor: &str, descendant: &str| -> Option<bool> {
            Some(ancestor == "binary" && descendant == "installed")
        };
        assert_eq!(
            drift_side(Some("installed"), "binary", binary_is_ancestor),
            DriftSide::BinaryStale,
            "a provably-newer installed workflow makes the binary stale"
        );

        // The installed workflow's build is OLDER (this binary is not its ancestor): the
        // WORKFLOW is stale.
        assert_eq!(
            drift_side(Some("installed"), "binary", |_, _| Some(false)),
            DriftSide::WorkflowStale,
            "an older installed workflow makes the workflow stale"
        );

        // Undecidable order (git cannot resolve one of the ids, e.g. an operator project
        // that lacks rigger's history): fall back to the actionable refresh directive.
        assert_eq!(
            drift_side(Some("installed"), "binary", |_, _| None),
            DriftSide::WorkflowStale,
            "an undecidable order falls back to refreshing the workflow"
        );

        // No recorded provenance (an older install with no sidecar): refresh directive, and
        // the ancestry oracle is never consulted.
        assert_eq!(
            drift_side(None, "binary", |_, _| panic!(
                "ancestry must not be consulted without a recorded provenance"
            )),
            DriftSide::WorkflowStale,
            "a missing provenance falls back to refreshing the workflow"
        );

        // The installed build EQUALS this binary but the content drifted (a local
        // hand-edit): refresh directive, ancestry never consulted.
        assert_eq!(
            drift_side(Some("binary"), "binary", |_, _| panic!(
                "ancestry must not be consulted for a same-build hand-edit"
            )),
            DriftSide::WorkflowStale,
            "a same-build content edit falls back to refreshing the workflow"
        );
    }

    #[test]
    fn workflow_drift_advisory_names_which_side_is_stale_and_never_says_they_differ() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let path = workflow_path(root);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();

        // No drift (byte-identical to the embedded copy): no advisory at all.
        std::fs::write(&path, RIGGER_WORKFLOW).unwrap();
        assert!(
            workflow_drift_advisory(root, "binary", |_, _| None).is_none(),
            "no advisory when the installed workflow matches the embedded copy"
        );

        // Drift the installed workflow and record it as written by a NEWER build.
        std::fs::write(&path, "// drifted installed workflow\n").unwrap();
        std::fs::write(workflow_provenance_path(root), "installed-newer\n").unwrap();
        let binary_stale = workflow_drift_advisory(root, "binary-old", |anc, desc| {
            Some(anc == "binary-old" && desc == "installed-newer")
        })
        .expect("a drifted workflow yields an advisory");
        assert!(
            binary_stale.contains("the binary is stale")
                && binary_stale.to_lowercase().contains("rebuild")
                && binary_stale.contains("installed-newer")
                && binary_stale.contains("binary-old"),
            "the binary-stale advisory names the binary as stale, says rebuild, and cites \
             both provenances; got: {binary_stale}"
        );
        assert!(
            !binary_stale.contains("they differ"),
            "the advisory must never be the ambiguous 'they differ'; got: {binary_stale}"
        );

        // Same drifted file, but recorded as an OLDER build: the WORKFLOW is stale.
        std::fs::write(workflow_provenance_path(root), "installed-old\n").unwrap();
        let workflow_stale = workflow_drift_advisory(root, "binary-new", |_, _| Some(false))
            .expect("a drifted workflow yields an advisory");
        assert!(
            workflow_stale.contains("the installed workflow is stale")
                && workflow_stale.contains("rigger setup")
                && workflow_stale.contains(".claude/workflows/rigger.js"),
            "the workflow-stale advisory names the workflow as stale, says `rigger setup`, \
             and names the file; got: {workflow_stale}"
        );
        assert!(
            !workflow_stale.contains("they differ"),
            "the advisory must never be the ambiguous 'they differ'; got: {workflow_stale}"
        );
    }

    #[test]
    fn git_is_ancestor_decides_commit_order_in_a_real_repo() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let git = |args: &[&str]| {
            let out = std::process::Command::new("git")
                .args(args)
                .current_dir(root)
                .env("GIT_AUTHOR_NAME", "t")
                .env("GIT_AUTHOR_EMAIL", "t@e")
                .env("GIT_COMMITTER_NAME", "t")
                .env("GIT_COMMITTER_EMAIL", "t@e")
                .output()
                .unwrap();
            assert!(
                out.status.success(),
                "git {args:?} failed: {}",
                String::from_utf8_lossy(&out.stderr)
            );
            String::from_utf8(out.stdout).unwrap().trim().to_string()
        };
        git(&["init", "-q"]);
        std::fs::write(root.join("a"), "1").unwrap();
        git(&["add", "."]);
        git(&["commit", "-q", "-m", "one"]);
        let first = git(&["rev-parse", "HEAD"]);
        std::fs::write(root.join("a"), "2").unwrap();
        git(&["commit", "-q", "-am", "two"]);
        let second = git(&["rev-parse", "HEAD"]);

        assert_eq!(
            git_is_ancestor(root, &first, &second),
            Some(true),
            "the parent commit is an ancestor of the child"
        );
        assert_eq!(
            git_is_ancestor(root, &second, &first),
            Some(false),
            "the child commit is not an ancestor of the parent"
        );
        assert_eq!(
            git_is_ancestor(root, &"0".repeat(40), &second),
            None,
            "an unresolvable id makes the order undecidable"
        );
    }

    #[test]
    fn git_commit_distance_counts_commits_ahead_in_a_real_repo() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let git = |args: &[&str]| {
            let out = std::process::Command::new("git")
                .args(args)
                .current_dir(root)
                .env("GIT_AUTHOR_NAME", "t")
                .env("GIT_AUTHOR_EMAIL", "t@e")
                .env("GIT_COMMITTER_NAME", "t")
                .env("GIT_COMMITTER_EMAIL", "t@e")
                .output()
                .unwrap();
            assert!(
                out.status.success(),
                "git {args:?} failed: {}",
                String::from_utf8_lossy(&out.stderr)
            );
            String::from_utf8(out.stdout).unwrap().trim().to_string()
        };
        git(&["init", "-q"]);
        std::fs::write(root.join("a"), "1").unwrap();
        git(&["add", "."]);
        git(&["commit", "-q", "-m", "one"]);
        let first = git(&["rev-parse", "HEAD"]);
        std::fs::write(root.join("a"), "2").unwrap();
        git(&["commit", "-q", "-am", "two"]);
        std::fs::write(root.join("a"), "3").unwrap();
        git(&["commit", "-q", "-am", "three"]);
        let third = git(&["rev-parse", "HEAD"]);

        assert_eq!(
            git_commit_distance(root, &first, &third),
            Some(2),
            "two commits separate first and third"
        );
        assert_eq!(
            git_commit_distance(root, &first, &first),
            Some(0),
            "a commit is zero commits ahead of itself"
        );
        assert_eq!(
            git_commit_distance(root, &"0".repeat(40), &third),
            None,
            "an unresolvable id makes the distance undecidable"
        );
    }

    #[test]
    fn missing_gitsemver_binary_advisory_fires_only_on_the_unversioned_marker() {
        assert!(
            missing_gitsemver_binary_advisory("1.2.3+abcdef").is_none(),
            "a genuinely derived version names no missing-binary advisory"
        );
        for installed in ["0.3.0+unversioned", "9.9.9+unversioned"] {
            let advisory = missing_gitsemver_binary_advisory(installed).unwrap_or_else(|| {
                panic!("every +unversioned-marked version must yield an advisory (cause is irrelevant); got None for {installed}")
            });
            assert!(
                advisory.contains("go-gitsemver") && advisory.contains(installed),
                "the advisory must name go-gitsemver and the installed version; got: {advisory}"
            );
        }
    }

    #[test]
    fn behind_the_tree_message_is_silent_when_versions_already_match() {
        assert!(
            behind_the_tree_message("1.2.3+abc", "1.2.3+abc", Some(5)).is_none(),
            "identical versions carry nothing actionable, regardless of a nonzero distance"
        );
    }

    #[test]
    fn behind_the_tree_message_is_silent_when_either_side_is_unversioned() {
        assert!(
            behind_the_tree_message("0.3.0+unversioned", "1.0.0+abc", Some(3)).is_none(),
            "an unversioned installed side is reported by the missing-binary advisory, not \
             this one"
        );
        assert!(
            behind_the_tree_message("1.0.0+abc", "0.3.0+unversioned", Some(3)).is_none(),
            "an unversioned checkout side has nothing comparable to report"
        );
    }

    #[test]
    fn behind_the_tree_message_is_silent_on_an_undecidable_or_zero_distance() {
        assert!(
            behind_the_tree_message("1.0.0+abc", "1.1.0+def", None).is_none(),
            "an undecidable git order (diverged history, an unresolvable id) reports nothing"
        );
        assert!(
            behind_the_tree_message("1.0.0+abc", "1.1.0+def", Some(0)).is_none(),
            "zero commits ahead is not behind the tree"
        );
    }

    #[test]
    fn behind_the_tree_message_names_both_versions_and_the_commit_distance() {
        let msg = behind_the_tree_message("1.0.0+abc123", "1.1.0+def456", Some(4))
            .expect("a genuine ahead-by-N case must yield an advisory");
        assert!(
            msg.contains("1.0.0+abc123") && msg.contains("1.1.0+def456") && msg.contains('4'),
            "the advisory must name both versions and the commit distance; got: {msg}"
        );
    }

    /// Skip (rather than fail) when `go-gitsemver` is not on PATH in this environment -
    /// mirrors `tests/gitsemver_derivation.rs`'s own `gitsemver_available` guard: these
    /// tests prove `behind_the_tree_advisory`'s WIRING to the real derivation seam given
    /// the tool, which the pure `behind_the_tree_message` tests above already prove
    /// independently of it.
    fn gitsemver_available() -> bool {
        Command::new("go-gitsemver")
            .arg("version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    /// `git <args>` in `root` with a fixed committer identity, panicking with stderr on
    /// failure - mirrors `tests/gitsemver_derivation.rs`'s own `git` fixture helper.
    fn behind_the_tree_git(root: &Path, args: &[&str]) {
        let out = Command::new("git")
            .args(args)
            .current_dir(root)
            .env("GIT_AUTHOR_NAME", "t")
            .env("GIT_AUTHOR_EMAIL", "t@e")
            .env("GIT_COMMITTER_NAME", "t")
            .env("GIT_COMMITTER_EMAIL", "t@e")
            .output()
            .unwrap_or_else(|e| panic!("spawning git {args:?} failed: {e}"));
        assert!(
            out.status.success(),
            "git {args:?} failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }

    fn behind_the_tree_git_output(root: &Path, args: &[&str]) -> String {
        let out = Command::new("git")
            .args(args)
            .current_dir(root)
            .output()
            .unwrap_or_else(|e| panic!("spawning git {args:?} failed: {e}"));
        assert!(
            out.status.success(),
            "git {args:?} failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8(out.stdout).unwrap().trim().to_string()
    }

    #[test]
    fn behind_the_tree_advisory_names_the_real_derived_version_ahead_of_the_installed_commit() {
        if !gitsemver_available() {
            eprintln!("skipping: go-gitsemver not on PATH");
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        behind_the_tree_git(root, &["init", "-q"]);
        std::fs::write(
            root.join("go-gitsemver.yml"),
            "mode: Mainline\ntag-prefix: v\n",
        )
        .unwrap();
        behind_the_tree_git(root, &["add", "."]);
        behind_the_tree_git(root, &["commit", "-q", "-m", "chore: initial"]);
        behind_the_tree_git(root, &["tag", "v1.0.0"]);
        let installed_commit = behind_the_tree_git_output(root, &["rev-parse", "HEAD"]);
        let installed_version = gitsemver::derive_version("go-gitsemver", root);

        std::fs::write(root.join("file.txt"), "second\n").unwrap();
        behind_the_tree_git(root, &["add", "."]);
        behind_the_tree_git(root, &["commit", "-q", "-m", "feat: add a thing"]);

        let advisory = behind_the_tree_advisory(root, &installed_version, &installed_commit)
            .expect("a checkout genuinely ahead of the installed commit must yield an advisory");
        assert!(
            advisory.contains(&installed_version),
            "the advisory must name the installed version; got: {advisory}"
        );
        assert!(
            advisory.contains('1'),
            "the checkout is exactly one commit ahead; got: {advisory}"
        );
        assert!(
            advisory.contains("1.1.0"),
            "the feat: commit must bump the minor in the reported checkout version; got: \
             {advisory}"
        );
    }

    #[test]
    fn behind_the_tree_advisory_is_silent_when_the_checkout_has_not_moved() {
        if !gitsemver_available() {
            eprintln!("skipping: go-gitsemver not on PATH");
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        behind_the_tree_git(root, &["init", "-q"]);
        std::fs::write(
            root.join("go-gitsemver.yml"),
            "mode: Mainline\ntag-prefix: v\n",
        )
        .unwrap();
        behind_the_tree_git(root, &["add", "."]);
        behind_the_tree_git(root, &["commit", "-q", "-m", "chore: initial"]);
        let installed_commit = behind_the_tree_git_output(root, &["rev-parse", "HEAD"]);
        let installed_version = gitsemver::derive_version("go-gitsemver", root);

        assert!(
            behind_the_tree_advisory(root, &installed_version, &installed_commit).is_none(),
            "the installed commit equals HEAD, so there is nothing to report"
        );
    }

    #[test]
    fn install_workflow_records_the_build_provenance_beside_the_workflow() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        assert!(
            installed_workflow_provenance(root).is_none(),
            "no recorded provenance before any install"
        );
        install_workflow(root).expect("a fresh install must succeed");
        assert_eq!(
            installed_workflow_provenance(root).as_deref(),
            Some(BUILD_PROVENANCE),
            "a fresh install records THIS binary's build provenance beside the workflow"
        );
    }

    #[test]
    fn validate_advisories_warns_on_workflow_drift_naming_the_file() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let path = workflow_path(root);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "// drifted\n").unwrap();

        let advisories = validate_advisories(root);
        assert!(
            advisories
                .iter()
                .any(|a| a.contains("drifted") && a.contains(".claude/workflows/rigger.js")),
            "a drifted installed workflow yields a drift advisory naming the file; got: \
             {advisories:?}"
        );
    }

    // ---- `rigger validate` residue report (spec 06:60 / Gap 14d): pure seams --------

    use std::collections::HashSet;

    fn slugs<const N: usize>(xs: [&str; N]) -> HashSet<String> {
        xs.iter().map(|s| s.to_string()).collect()
    }

    fn write_file(path: &Path, bytes: &[u8]) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, bytes).unwrap();
    }

    /// `git init` a repo at `root` and commit a single file `rel` with `contents`, so a
    /// base ref like `HEAD` resolves and `rel` is present in its tree (for the
    /// missing-files base-refusal tests, spec 18 criterion 7).
    fn init_committed_repo(root: &Path, rel: &str, contents: &str) {
        for args in [
            &["init", "-q"][..],
            &["config", "user.email", "t@example.com"],
            &["config", "user.name", "t"],
        ] {
            assert!(
                Command::new("git")
                    .args(args)
                    .current_dir(root)
                    .status()
                    .unwrap()
                    .success(),
                "git {args:?} must succeed"
            );
        }
        write_file(&root.join(rel), contents.as_bytes());
        for args in [&["add", rel][..], &["commit", "-q", "-m", "seed"]] {
            assert!(
                Command::new("git")
                    .args(args)
                    .current_dir(root)
                    .status()
                    .unwrap()
                    .success(),
                "git {args:?} must succeed"
            );
        }
    }

    #[test]
    fn refuse_when_base_lacks_spec_paths_refuses_on_total_absence_and_names_a_path() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        init_committed_repo(root, "src/main.rs", "fn main() {}\n");
        let repo = root.to_str().unwrap();
        // The spec's only path token is absent from HEAD => refuse, naming it AND --base.
        let criteria = vec!["the file crates/foo/src/bar.rs exports Zed".to_string()];
        let err = refuse_when_base_lacks_spec_paths(
            repo,
            "rigger step",
            "HEAD",
            RunBranchSetup::CreatedFromBase,
            &criteria,
        )
        .expect_err("a spec referencing only-absent paths must refuse");
        let msg = err.to_string();
        assert!(
            msg.contains("crates/foo/src/bar.rs"),
            "the refusal must name the missing path; got: {msg}"
        );
        assert!(
            msg.contains("--base"),
            "the refusal must suggest --base; got: {msg}"
        );
    }

    #[test]
    fn refuse_when_base_lacks_spec_paths_proceeds_when_the_base_contains_them() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        init_committed_repo(root, "src/main.rs", "fn main() {}\n");
        let repo = root.to_str().unwrap();
        let present = vec!["touches `src/main.rs`".to_string()];
        assert!(
            refuse_when_base_lacks_spec_paths(
                repo,
                "rigger step",
                "HEAD",
                RunBranchSetup::CreatedFromBase,
                &present,
            )
            .is_ok(),
            "a spec whose referenced path exists in the base must proceed"
        );
    }

    #[test]
    fn refuse_when_base_lacks_spec_paths_partial_match_warns_and_proceeds() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        init_committed_repo(root, "src/main.rs", "fn main() {}\n");
        let repo = root.to_str().unwrap();
        // One present, one absent => partial => warn + proceed, never a refusal.
        let mixed = vec!["touches src/main.rs and adds crates/new/src/lib.rs".to_string()];
        assert!(
            refuse_when_base_lacks_spec_paths(
                repo,
                "rigger step",
                "HEAD",
                RunBranchSetup::CreatedFromBase,
                &mixed,
            )
            .is_ok(),
            "a partial match must proceed (some named paths may be to-be-created)"
        );
    }

    #[test]
    fn refuse_when_base_lacks_spec_paths_skips_without_tokens_or_off_a_fresh_from_base_anchor() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        init_committed_repo(root, "src/main.rs", "fn main() {}\n");
        let repo = root.to_str().unwrap();
        // No path-like tokens => nothing to check, even on a fresh-from-base anchor.
        let no_paths = vec!["the store passes its contract suite".to_string()];
        assert!(refuse_when_base_lacks_spec_paths(
            repo,
            "rigger step",
            "HEAD",
            RunBranchSetup::CreatedFromBase,
            &no_paths,
        )
        .is_ok());
        // Only-absent paths, but a REUSED or HEAD-fallback anchor skips the check: the run
        // already began (or has no resolvable base), so it must never refuse mid-run.
        let absent = vec!["the file crates/foo/src/bar.rs".to_string()];
        assert!(
            refuse_when_base_lacks_spec_paths(
                repo,
                "rigger step",
                "HEAD",
                RunBranchSetup::Reused,
                &absent,
            )
            .is_ok(),
            "a reused run branch must not re-refuse"
        );
        assert!(
            refuse_when_base_lacks_spec_paths(
                repo,
                "rigger step",
                "HEAD",
                RunBranchSetup::CreatedFromHead,
                &absent,
            )
            .is_ok(),
            "a HEAD fallback (no resolvable base) must not refuse"
        );
    }

    #[test]
    fn refuse_when_base_unreachable_fails_loudly_only_when_no_reachable_base() {
        // Loop-readiness gate (spec 38, criterion 2): a run with NO reachable base - an
        // unresolvable base AND no HEAD commit to fall back to (an unborn / empty repo) - is
        // REFUSED loudly rather than minting a run branch that branches from nowhere. The
        // refusal is side-effect-free, so the corrected retry anchors the run fresh.
        let empty = tempfile::tempdir().unwrap();
        let empty_root = empty.path();
        for args in [
            &["init", "-q"][..],
            &["config", "user.email", "t@example.com"],
            &["config", "user.name", "t"],
        ] {
            assert!(
                Command::new("git")
                    .args(args)
                    .current_dir(empty_root)
                    .status()
                    .unwrap()
                    .success(),
                "git {args:?} must succeed"
            );
        }
        let empty_repo = empty_root.to_str().unwrap();
        let err = refuse_when_base_unreachable(
            empty_repo,
            "rigger step",
            "origin/main",
            RunBranchSetup::CreatedFromHead,
        )
        .expect_err("an unborn-HEAD repo with an unresolvable base has no reachable base");
        let msg = err.to_string();
        assert!(
            msg.contains("origin/main"),
            "the refusal must name the unresolved base; got: {msg}"
        );
        assert!(
            msg.contains("--base"),
            "the refusal must point at a reachable --base; got: {msg}"
        );

        // A repo whose base is unresolvable but whose HEAD IS a real commit: the HEAD fallback
        // anchors on the operator's own branch - a REACHABLE base a PR still applies to - so it
        // PROCEEDS (the established CLI HEAD-fallback contract), never a refusal.
        let live = tempfile::tempdir().unwrap();
        let live_root = live.path();
        init_committed_repo(live_root, "src/main.rs", "fn main() {}\n");
        let live_repo = live_root.to_str().unwrap();
        assert!(
            refuse_when_base_unreachable(
                live_repo,
                "rigger step",
                "origin/main",
                RunBranchSetup::CreatedFromHead,
            )
            .is_ok(),
            "a HEAD fallback with a real HEAD is a reachable base and must proceed"
        );

        // A resolvable base (CreatedFromBase) always has a real anchor and passes. An existing
        // run branch (Reused) is NEVER refused - its base was vetted at creation, so re-checking
        // on resume-by-replay must not wedge a live run (proven here even on the empty repo).
        assert!(
            refuse_when_base_unreachable(
                live_repo,
                "rigger step",
                "main",
                RunBranchSetup::CreatedFromBase,
            )
            .is_ok(),
            "a reachable base must pass the loop-readiness gate"
        );
        assert!(
            refuse_when_base_unreachable(
                empty_repo,
                "rigger step",
                "main",
                RunBranchSetup::Reused,
            )
            .is_ok(),
            "a reused run branch must never be refused (resume-safe), even on an empty repo"
        );
    }

    #[test]
    fn human_size_formats_bytes_through_gib() {
        assert_eq!(human_size(0), "0B");
        assert_eq!(human_size(18), "18B");
        assert_eq!(human_size(1024), "1.0K");
        assert_eq!(human_size(1536), "1.5K");
        assert_eq!(human_size(5 * (1 << 20)), "5.0M");
        assert_eq!(human_size(3 * (1 << 30) + (1 << 29)), "3.5G");
    }

    #[test]
    fn is_uuid8_accepts_exactly_eight_hex_digits() {
        assert!(is_uuid8("99dd4e29"));
        assert!(is_uuid8("deadbeef"));
        assert!(!is_uuid8("99dd4e2")); // 7
        assert!(!is_uuid8("99dd4e299")); // 9
        assert!(!is_uuid8("99dd4e2g")); // non-hex
    }

    #[test]
    fn worktree_belongs_to_live_matches_both_naming_shapes_without_prefix_false_match() {
        let live = slugs(["unit-6-rigger-validate-reports-residue-w", "unit-1"]);
        let no_dead = slugs([]);
        // Legacy per-process shape `rigger-wt-<slug>-<8hex>`.
        assert!(worktree_belongs_to_live(
            "rigger-wt-unit-6-rigger-validate-reports-residue-w-99dd4e29",
            &live,
            &no_dead
        ));
        // Deterministic shape `rigger-wt-<slug>` (spec 06 unit 4, no uuid).
        assert!(worktree_belongs_to_live(
            "rigger-wt-unit-1",
            &live,
            &no_dead
        ));
        // A dead unit's worktree is NOT live.
        assert!(!worktree_belongs_to_live(
            "rigger-wt-unit-99-ghost-12345678",
            &live,
            &no_dead
        ));
        // `unit-1` is a prefix of the longer slug but must not false-match a foreign uuid:
        // `rigger-wt-unit-1-2-abcdef12` has slug `unit-1-2`, not live.
        assert!(!worktree_belongs_to_live(
            "rigger-wt-unit-1-2-abcdef12",
            &live,
            &no_dead
        ));

        // adv-u6res-uuid8-tail-false-match: a DEAD unit `unit-1-deadbeef` (while `unit-1`
        // is live) owns a deterministic `rigger-wt-unit-1-deadbeef`. Without the dead-slug
        // set it decomposes as live-`unit-1` + uuid-`deadbeef` and is (wrongly) spared...
        assert!(worktree_belongs_to_live(
            "rigger-wt-unit-1-deadbeef",
            &live,
            &no_dead
        ));
        // ...but knowing `unit-1-deadbeef` is a terminal unit, it is its OWN dead unit's
        // worktree - residue, NOT live. (Reverting the `dead_slugs` guard reddens this.)
        let dead = slugs(["unit-1-deadbeef"]);
        assert!(!worktree_belongs_to_live(
            "rigger-wt-unit-1-deadbeef",
            &live,
            &dead
        ));
    }

    #[test]
    fn current_run_units_scopes_to_the_current_run_and_splits_live_from_dead() {
        let events = [
            // A PRIOR run left a still-non-terminal unit. Under an UNSCOPED fold it reads
            // as live; scoping to the current run's slice must EXCLUDE it (it is residue of
            // an aborted run) - this is the dispositive current-run clause (spec 06:50/30).
            Event::new(
                runscope::TYPE_RUN_STARTED,
                br#"{"run":"r0","criteria":["old"]}"#.to_vec(),
            ),
            Event::new(
                ledger::TYPE_UNIT_STARTED,
                br#"{"id":"unit-prior","branch":"rigger/u/unit-prior"}"#.to_vec(),
            ),
            // The CURRENT run begins here.
            Event::new(
                runscope::TYPE_RUN_STARTED,
                br#"{"run":"r1","criteria":["new"]}"#.to_vec(),
            ),
            Event::new(
                ledger::TYPE_UNIT_STARTED,
                br#"{"id":"unit-6","branch":"rigger/u/unit-6"}"#.to_vec(),
            ),
            Event::new(
                ledger::TYPE_UNIT_STARTED,
                br#"{"id":"unit-old","branch":"rigger/u/unit-old"}"#.to_vec(),
            ),
            // unit-old integrated -> terminal -> dead, not live.
            Event::new(
                ledger::TYPE_UNIT_INTEGRATED,
                br#"{"id":"unit-old","commit":"abc"}"#.to_vec(),
            ),
            Event::new(
                ledger::TYPE_UNIT_STARTED,
                br#"{"id":"unit-gone","branch":"rigger/u/unit-gone"}"#.to_vec(),
            ),
            // unit-gone escalated -> terminal -> dead, not live.
            Event::new(
                ledger::TYPE_UNIT_ESCALATED,
                br#"{"id":"unit-gone"}"#.to_vec(),
            ),
        ];
        let run = current_run_units(&events);
        // Only THIS run's in-flight unit is live: unit-prior is excluded by run-scoping,
        // and this run's terminal units are dead, not live.
        assert_eq!(run.live_branches, slugs(["rigger/u/unit-6"]));
        assert_eq!(live_slugs(&run.live_branches), slugs(["unit-6"]));
        assert_eq!(run.dead_slugs, slugs(["unit-old", "unit-gone"]));
    }

    /// Spec 64, criterion 4 fix (the rejected round): an unreadable run stream must make the
    /// step-start sweep decision fail CLOSED (`None`, read by `cmd_step` as "skip the sweep
    /// call entirely"), never degrade to `Some(HashSet::new())` - the rejected bug, which
    /// `sweep_terminal` would still run with an EMPTY live set, silently reverting to the
    /// pre-c4 ancestry-only rule that force-removes a live unit's empty-diff worktree mid-review.
    /// `Err` is asserted first (the exact arm the prior round shipped with zero coverage of);
    /// `Ok` is asserted too, so this also pins that a readable stream still hands back the SAME
    /// fold `current_run_units` computes elsewhere in this function - one liveness authority,
    /// not a second one reimplemented here.
    #[test]
    fn live_branches_for_sweep_fails_closed_on_an_unreadable_stream_but_folds_a_readable_one() {
        let err = live_branches_for_sweep(Err(rigger::eventstore::Error::Backend(
            "simulated read failure (e.g. SQLITE_BUSY_SNAPSHOT under a concurrent writer)"
                .to_string(),
        )));
        assert_eq!(
            err, None,
            "an unreadable run stream must decide None (skip the sweep outright), never \
             Some(empty set) - the rejected silent degrade that still runs the sweep"
        );

        let events = vec![
            Event::new(
                runscope::TYPE_RUN_STARTED,
                br#"{"run":"r1","criteria":["c"]}"#.to_vec(),
            ),
            Event::new(
                ledger::TYPE_UNIT_STARTED,
                br#"{"id":"unit-6","branch":"rigger/u/unit-6"}"#.to_vec(),
            ),
        ];
        assert_eq!(
            live_branches_for_sweep(Ok(events)),
            Some(slugs(["rigger/u/unit-6"])),
            "a readable stream decides Some(the current_run_units fold) - the sweep still runs"
        );
    }

    #[test]
    fn find_shadow_stores_finds_nested_events_db_and_prunes_build_caches() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        // A shadow store inside a worktree, and one in a scratch probe repo.
        write_file(
            &root.join("rigger-wt-x").join(".rigger").join("events.db"),
            b"shadow",
        );
        write_file(
            &root.join("probe").join(".rigger").join("events.db"),
            b"shadow2",
        );
        // A same-named file buried in a build cache must be PRUNED (never a real store).
        write_file(
            &root.join("cargo-target").join("debug").join("events.db"),
            b"not-a-store",
        );
        // A per-unit build cache (`cargo-target-<slug>`, Gap 19) is pruned the same way -
        // descending a leaked multi-gigabyte unit cache would defeat the walk's
        // cheap-beside-a-target guarantee (adv-u3gap19-shadow-walk-descends-per-unit-caches).
        write_file(
            &root
                .join("cargo-target-unit-9")
                .join("debug")
                .join("events.db"),
            b"not-a-store-either",
        );
        let mut found: Vec<String> = find_shadow_stores(root)
            .iter()
            .map(|p| p.strip_prefix(root).unwrap().to_string_lossy().into_owned())
            .collect();
        found.sort();
        assert_eq!(
            found,
            vec![
                "probe/.rigger/events.db".to_string(),
                "rigger-wt-x/.rigger/events.db".to_string(),
            ],
            "shadow-store walk finds nested events.db but prunes build caches"
        );
    }

    #[test]
    fn dir_size_bytes_sums_files_recursively() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        write_file(&root.join("a.txt"), &[0u8; 100]);
        write_file(&root.join("sub").join("b.txt"), &[0u8; 250]);
        assert_eq!(dir_size_bytes(root), 350);
        assert_eq!(
            dir_size_bytes(&root.join("nonexistent")),
            0,
            "a missing path sizes to 0, never a panic"
        );
    }

    #[test]
    fn scan_residue_reports_dead_worktrees_caches_shadows_and_branches() {
        let dir = tempfile::tempdir().unwrap();
        let scratch = dir.path();
        // A LIVE unit's worktree - must NOT be flagged.
        write_file(
            &scratch.join("rigger-wt-unit-6-99dd4e29").join("keep.txt"),
            &[0u8; 10],
        );
        // A DEAD unit's worktree - flagged, with size.
        write_file(
            &scratch
                .join("rigger-wt-unit-99-ghost-12345678")
                .join("big.bin"),
            &[0u8; 4096],
        );
        // An orphaned build cache directly under the scratch root.
        write_file(&scratch.join("cargo-target").join("x.rlib"), &[0u8; 2048]);
        // A DEAD unit's per-unit build cache (`cargo-target-<slug>`, Gap 19) - its owning
        // worktree is not live, so the leaked cache is residue and must be reported.
        write_file(
            &scratch.join("cargo-target-unit-99-ghost").join("i.rlib"),
            &[0u8; 512],
        );
        // A LIVE unit's per-unit build cache - in use, NOT residue, must be omitted.
        write_file(
            &scratch.join("cargo-target-unit-6").join("i.rlib"),
            &[0u8; 128],
        );
        // A shadow store inside the dead worktree.
        write_file(
            &scratch
                .join("rigger-wt-unit-99-ghost-12345678")
                .join(".rigger")
                .join("events.db"),
            b"shadow",
        );
        let live_slugs = slugs(["unit-6"]);
        let live_branches = slugs(["rigger/u/unit-6"]);
        let local_branches = vec![
            "rigger/u/unit-6".to_string(),        // live -> kept
            "rigger/u/unit-99-ghost".to_string(), // dead -> flagged
        ];

        let report = scan_residue(
            scratch,
            &live_slugs,
            &slugs([]),
            &local_branches,
            &live_branches,
        );

        assert_eq!(
            report.worktrees,
            vec![("rigger-wt-unit-99-ghost-12345678".to_string(), 4096 + 6)],
            "only the DEAD unit's worktree is residue, sized (payload + shadow store)"
        );
        assert_eq!(
            report.caches,
            vec![
                ("cargo-target".to_string(), 2048),
                ("cargo-target-unit-99-ghost".to_string(), 512),
            ],
            "the shared orphan cache and the DEAD unit's per-unit cache are residue; the LIVE unit's per-unit cache is omitted"
        );
        assert_eq!(
            report.shadow_stores,
            vec![(
                "rigger-wt-unit-99-ghost-12345678/.rigger/events.db".to_string(),
                6
            )],
        );
        assert_eq!(report.branches, vec!["rigger/u/unit-99-ghost".to_string()]);
        assert!(!report.is_empty());
    }

    #[test]
    fn scan_residue_is_empty_when_everything_is_live_and_no_shadow_stores() {
        let dir = tempfile::tempdir().unwrap();
        let scratch = dir.path();
        write_file(
            &scratch.join("rigger-wt-unit-6-99dd4e29").join("keep.txt"),
            &[0u8; 10],
        );
        let report = scan_residue(
            scratch,
            &slugs(["unit-6"]),
            &slugs([]),
            &["rigger/u/unit-6".to_string()],
            &slugs(["rigger/u/unit-6"]),
        );
        assert!(
            report.is_empty(),
            "a scratch root holding only the live unit's clean worktree is not residue: {report:?}"
        );
        assert!(format_residue(&report).is_empty());
    }

    #[test]
    fn format_residue_renders_a_sized_warning_block() {
        let report = ResidueReport {
            worktrees: vec![("rigger-wt-unit-99-ghost-12345678".to_string(), 4096)],
            caches: vec![("cargo-target".to_string(), 5_905_580_032)],
            shadow_stores: vec![("probe/.rigger/events.db".to_string(), 6)],
            branches: vec!["rigger/u/unit-99-ghost".to_string()],
        };
        let lines = format_residue(&report);
        assert_eq!(lines.len(), 1, "the residue report is one stderr block");
        let block = &lines[0];
        assert!(block.starts_with("warning: residue found under the scratch root"));
        assert!(
            block.contains("worktree with no live unit: rigger-wt-unit-99-ghost-12345678 (4.0K)")
        );
        assert!(block.contains("orphaned build cache: cargo-target (5.5G)"));
        assert!(block.contains("shadow store: probe/.rigger/events.db (6B)"));
        assert!(block.contains("branch with no live unit: rigger/u/unit-99-ghost"));
    }

    // ---- spec 34 (criterion 2): the orphan-sweep backstop reclaim seam --------------

    #[test]
    fn reclaim_orphan_scratch_removes_non_live_owned_scratch_and_spares_live_and_shared_areas() {
        // spec 34 (criterion 2): the ORPHAN-SWEEP reclaims every scratch entry no LIVE unit of
        // the current run owns - a prior run's stranded worktree/cache, or a `cargo-target-<slug>`
        // an agent wrote outside its assigned path - keyed on the SAME liveness-ownership
        // predicate the residue report reads. The never-delete-live-owned rail (spec 34 Global
        // Constraint): a LIVE unit's worktree/cache is spared, proving the sweep can never remove
        // scratch a live spawn/run owns; the shared live-spawn areas (`agent-scratch`,
        // `agent-live`, the bare `cargo-target` a live spawn builds into) are spared too.
        let dir = tempfile::tempdir().unwrap();
        let scratch = dir.path();

        // A LIVE unit (`rigger/u/live-unit`, non-terminal) owns a worktree + per-unit cache.
        write_file(
            &scratch.join("rigger-wt-live-unit").join("keep.txt"),
            &[0u8; 8],
        );
        write_file(
            &scratch.join("cargo-target-live-unit").join("live.rlib"),
            &[0u8; 8],
        );
        // A DEAD (terminal) unit's stranded worktree + per-unit cache - residue.
        write_file(
            &scratch.join("rigger-wt-dead-unit").join("stale.txt"),
            &[0u8; 8],
        );
        write_file(
            &scratch.join("cargo-target-dead-unit").join("dead.rlib"),
            &[0u8; 8],
        );
        // An ad-hoc `cargo-target-<slug>` an agent wrote outside its assigned path (no live
        // owner) - the unbounded per-agent build-cache leak spec 34 names.
        write_file(
            &scratch.join("cargo-target-adhoc-x1").join("junk.rlib"),
            &[0u8; 8],
        );
        // A prior run's killed-process leftover worktree (no live unit) - residue.
        write_file(
            &scratch
                .join("rigger-wt-old-run-deadbeef")
                .join("leftover.txt"),
            &[0u8; 8],
        );
        // The shared live-spawn areas a running spawn is still using - MUST be spared.
        write_file(
            &scratch
                .join("agent-scratch")
                .join("probe")
                .join("Cargo.toml"),
            b"[package]",
        );
        write_file(&scratch.join("agent-live").join("run").join("marker"), b"");
        write_file(&scratch.join("cargo-target").join("shared.rlib"), &[0u8; 8]);

        let run_units = RunUnits {
            live_branches: slugs(["rigger/u/live-unit"]),
            dead_slugs: slugs(["dead-unit"]),
        };
        // Empty repo -> the git-aware worktree deregister is skipped and a plain removal runs,
        // which is all the synthetic (non-registered) worktree dirs here need.
        let removed = reclaim_orphan_scratch("", scratch.to_str().unwrap(), &run_units);
        assert_eq!(
            removed, 4,
            "exactly the four non-live-owned entries are reclaimed"
        );

        // Live-owned scratch: spared.
        assert!(
            scratch.join("rigger-wt-live-unit").exists(),
            "the LIVE unit's worktree is spared (keyed on liveness)"
        );
        assert!(
            scratch.join("cargo-target-live-unit").exists(),
            "the LIVE unit's per-unit build cache is in use, not residue"
        );
        // Shared live-spawn areas: spared (reclaimed by the run teardown, never this backstop).
        assert!(
            scratch.join("agent-scratch").exists(),
            "agent-scratch (in-flight worker probe/build area) is spared"
        );
        assert!(
            scratch.join("agent-live").exists(),
            "agent-live (per-spawn liveness markers) is spared"
        );
        assert!(
            scratch.join("cargo-target").exists(),
            "the bare shared cargo-target a live spawn may still build into is spared"
        );
        // Non-live-owned scratch: reclaimed.
        assert!(
            !scratch.join("rigger-wt-dead-unit").exists(),
            "the DEAD unit's worktree is reclaimed"
        );
        assert!(
            !scratch.join("cargo-target-dead-unit").exists(),
            "the DEAD unit's per-unit cache is reclaimed"
        );
        assert!(
            !scratch.join("cargo-target-adhoc-x1").exists(),
            "an ad-hoc cargo-target outside a spawn's assigned path is reclaimed"
        );
        assert!(
            !scratch.join("rigger-wt-old-run-deadbeef").exists(),
            "a prior run's leftover worktree is reclaimed"
        );

        // Idempotent: a re-run over the now-clean root reclaims nothing and errors on nothing.
        assert_eq!(
            reclaim_orphan_scratch("", scratch.to_str().unwrap(), &run_units),
            0,
            "the sweep is idempotent - a clean root reclaims nothing"
        );
    }

    // ---- `rigger validate` leaked-process advisory (spec 23, unit 2) ----------------

    #[test]
    fn leaked_process_advisories_name_a_process_rooted_under_the_scratch_root() {
        // spec 23 unit 2: a process whose cwd is under the scratch root is surfaced as a
        // warning-only advisory naming its pid, so a leaked build/tool is visible even when no
        // teardown is running. Consumes the SAME scan authority the teardown reap uses.
        let dir = tempfile::tempdir().unwrap();
        let scratch = dir.path().join("tmp");
        let inside = scratch.join("probe");
        std::fs::create_dir_all(&inside).unwrap();

        let mut child = Command::new("sleep")
            .arg("300")
            .current_dir(&inside)
            .spawn()
            .expect("spawn probe child");

        // Wait until the kernel reports the child rooted under the scratch root, then capture.
        let mut advisories = Vec::new();
        for _ in 0..200 {
            let a = leaked_process_advisories(&scratch);
            if a.iter()
                .any(|line| line.contains(&format!("pid {}", child.id())))
            {
                advisories = a;
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(25));
        }

        // Reap the fixture before asserting so a failed assert never leaks the sleeper.
        let _ = child.kill();
        let _ = child.wait();

        assert!(
            advisories.iter().any(|line| line.starts_with("warning:")
                && line.contains("scratch root")
                && line.contains(&format!("pid {}", child.id()))),
            "a process rooted under the scratch root yields a warning-only advisory naming its \
             pid; got: {advisories:?}"
        );
    }

    #[test]
    fn leaked_process_advisories_is_empty_when_no_process_is_rooted_under_the_scratch_root() {
        // None rooted under the scratch root: the advisory list is empty, so validate stays
        // silent about leaked processes.
        let dir = tempfile::tempdir().unwrap();
        let scratch = dir.path().join("tmp");
        std::fs::create_dir_all(&scratch).unwrap();
        assert!(
            leaked_process_advisories(&scratch).is_empty(),
            "an empty scratch root yields no leaked-process advisory"
        );
    }

    #[test]
    fn leaked_process_advisories_is_a_graceful_no_op_when_the_scratch_root_is_absent() {
        // Platform tolerance: an absent scratch root - the stand-in for an absent `/proc`,
        // since the shared scanner short-circuits to empty in both cases - yields an empty
        // list and NEVER an error, so validate keeps working on any platform.
        let dir = tempfile::tempdir().unwrap();
        let absent = dir.path().join("never-created");
        assert!(leaked_process_advisories(&absent).is_empty());
    }

    // ---- `rigger result`: argument parsing and outcome shaping (the stepwise CLI) ----

    #[test]
    fn parse_result_takes_an_id_and_an_optional_output_arg() {
        let a = parse_result_args(&["u/implementer#0".into(), "the diff".into()]).unwrap();
        assert_eq!(a.id, "u/implementer#0");
        assert_eq!(a.text.as_deref(), Some("the diff"));
        assert!(!a.is_error);
        assert!(a.meta.is_none());
    }

    #[test]
    fn parse_result_with_no_output_defers_to_stdin() {
        // Just an id -> text is None, so cmd_result reads the outcome from stdin.
        let a = parse_result_args(&["u/implementer#0".into()]).unwrap();
        assert_eq!(a.id, "u/implementer#0");
        assert!(a.text.is_none());
    }

    // ---- store-open hardening: walk up to an existing store, never fabricate one ----

    #[test]
    fn find_store_dir_from_returns_the_dir_that_holds_the_store() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::create_dir_all(root.join(RIGGER_DIR)).unwrap();
        std::fs::File::create(root.join(RIGGER_DIR).join("events.db")).unwrap();
        assert_eq!(find_store_dir_from(root), Some(root.join(RIGGER_DIR)));
    }

    #[test]
    fn find_store_dir_from_walks_up_from_a_subdirectory() {
        // A courier run from a SUBDIR of the project root still resolves the root's
        // store. The root is a git repo: the walk is bounded at the main-repo root, so
        // only git-governed ancestry is walkable (adv9-walkup-cross-project).
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        git_init_quiet(root);
        std::fs::create_dir_all(root.join(RIGGER_DIR)).unwrap();
        std::fs::File::create(root.join(RIGGER_DIR).join("events.db")).unwrap();
        let sub = root.join("src").join("deep");
        std::fs::create_dir_all(&sub).unwrap();
        assert_eq!(find_store_dir_from(&sub), Some(root.join(RIGGER_DIR)));
    }

    /// `git init -q` a test root so the bounded store walk has a sanctioned repo scope.
    fn git_init_quiet(root: &Path) {
        std::process::Command::new("git")
            .args(["init", "-q"])
            .current_dir(root)
            .status()
            .unwrap();
    }

    #[test]
    fn find_store_dir_from_never_escapes_the_repo_into_a_parent_store() {
        // adv9-walkup-cross-project: a courier in a storeless NESTED repo (an
        // agent-scratch probe under the parent's .rigger/tmp, say) must NOT bind to the
        // parent project's store - that writes into a foreign run stream. The walk stops
        // at the nested repo's own root. And with no git context at all there is no
        // sanctioned walk: only the start dir itself counts.
        let dir = tempfile::tempdir().unwrap();
        let parent = dir.path();
        git_init_quiet(parent);
        std::fs::create_dir_all(parent.join(RIGGER_DIR)).unwrap();
        std::fs::File::create(parent.join(RIGGER_DIR).join("events.db")).unwrap();

        // A nested, storeless git repo below the parent (not a linked worktree).
        let nested = parent
            .join(".rigger")
            .join("tmp")
            .join("agent-scratch")
            .join("probe");
        std::fs::create_dir_all(&nested).unwrap();
        git_init_quiet(&nested);
        assert_eq!(
            find_store_dir_from(&nested),
            None,
            "a storeless nested repo must refuse, never bind the parent's store"
        );

        // No git context: no walk-up at all (a store AT the start dir still counts).
        let bare = tempfile::tempdir().unwrap();
        let sub = bare.path().join("deep");
        std::fs::create_dir_all(&sub).unwrap();
        std::fs::create_dir_all(bare.path().join(RIGGER_DIR)).unwrap();
        std::fs::File::create(bare.path().join(RIGGER_DIR).join("events.db")).unwrap();
        assert_eq!(
            find_store_dir_from(&sub),
            None,
            "without a git scope the walk is unsanctioned"
        );
    }

    #[test]
    fn reap_then_remove_dir_reaps_processes_rooted_inside_then_removes_the_dir() {
        // spec 23: the fixpoint scratch-area sweep (cmd_step) reaps every process rooted in a
        // scratch dir BEFORE removing it, so a build or tool a worker left running under
        // agent-scratch does not outlive the deleted dir. A process rooted OUTSIDE the swept dir
        // is untouched (the safety boundary). The inside child IGNORES SIGTERM, so only the
        // SIGKILL escalation can reap it - exercising the full SIGTERM-then-SIGKILL mechanism at
        // this second teardown point (the first is Worktree::remove).
        let root = tempfile::tempdir().unwrap();
        let swept = root.path().join("agent-scratch");
        std::fs::create_dir_all(&swept).unwrap();

        let mut inside = Command::new("sh")
            .arg("-c")
            .arg("trap '' TERM; while :; do sleep 1; done")
            .current_dir(&swept)
            .spawn()
            .expect("spawn inside child");
        let mut outside = Command::new("sleep")
            .arg("300")
            .current_dir(root.path())
            .spawn()
            .expect("spawn outside child");

        let detected = (0..200).any(|_| {
            if rigger::reap::processes_rooted_under(&swept)
                .iter()
                .any(|(pid, _)| *pid == inside.id())
            {
                return true;
            }
            std::thread::sleep(std::time::Duration::from_millis(25));
            false
        });
        assert!(
            detected,
            "precondition: the inside child is rooted in the swept dir"
        );

        reap_then_remove_dir(&swept);

        let inside_died = (0..200).any(|_| {
            if matches!(inside.try_wait(), Ok(Some(_))) {
                return true;
            }
            std::thread::sleep(std::time::Duration::from_millis(25));
            false
        });
        let outside_alive = matches!(outside.try_wait(), Ok(None));

        let _ = outside.kill();
        let _ = outside.wait();
        if !inside_died {
            let _ = inside.kill();
            let _ = inside.wait();
        }

        assert!(
            inside_died,
            "a process rooted in the swept scratch dir must be reaped before its removal"
        );
        assert!(
            outside_alive,
            "a process rooted OUTSIDE the swept dir must survive the sweep (safety boundary)"
        );
        assert!(
            !swept.exists(),
            "the swept scratch dir is removed after its rooted processes are reaped"
        );
    }

    #[test]
    fn reclaim_run_scratch_removes_the_run_level_areas_and_spares_per_unit_scratch() {
        // spec 34, criterion 3: the terminal-state run teardown reclaims EXACTLY the run-level
        // shared areas - `agent-scratch`, `agent-live`, and the SHARED build cache
        // (`cargo-target` + `target` directly under the root, the driver's `CARGO_TARGET_DIR`) -
        // and NOTHING else. Per-unit worktrees (`rigger-wt-<slug>`) and per-unit build caches
        // (`cargo-target-<slug>`) are owned by their unit's own terminal reclamation (Worktree::
        // remove / sweep_terminal / the orphan-sweep), never this run-level teardown, so they are
        // SPARED here even though `cargo-target-<slug>` shares the build-cache prefix.
        let root = tempfile::tempdir().unwrap();
        let base = root.path();

        // The four run-level areas the teardown OWNS.
        for area in ["agent-scratch", "agent-live", "cargo-target", "target"] {
            let dir = base.join(area);
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(dir.join("residue.bin"), [0u8; 32]).unwrap();
        }

        // Scratch the teardown must SPARE: a per-unit worktree, its per-unit build cache (whose
        // `cargo-target-` prefix must NOT be mistaken for the bare shared `cargo-target`), and an
        // unrelated file.
        let unit_wt = base.join("rigger-wt-some-unit");
        std::fs::create_dir_all(&unit_wt).unwrap();
        let unit_cache = base.join("cargo-target-some-unit");
        std::fs::create_dir_all(&unit_cache).unwrap();
        let unrelated = base.join("keep.txt");
        std::fs::write(&unrelated, b"durable").unwrap();

        reclaim_run_scratch(base.to_str().unwrap());

        for area in ["agent-scratch", "agent-live", "cargo-target", "target"] {
            assert!(
                !base.join(area).exists(),
                "the run teardown must reclaim the run-level {area}"
            );
        }
        assert!(
            unit_wt.exists(),
            "a per-unit worktree is owned by its unit's terminal reclamation, not the run teardown"
        );
        assert!(
            unit_cache.exists(),
            "a per-unit cargo-target-<slug> cache must be spared (prefix must not match the bare shared cache)"
        );
        assert!(unrelated.exists(), "an unrelated file must be spared");

        // Idempotent + platform-tolerant: a second call over the now-empty root is a graceful
        // no-op (the areas are already gone), never a panic or error.
        reclaim_run_scratch(base.to_str().unwrap());
    }

    #[test]
    fn find_store_dir_from_refuses_the_worktree_shape_with_no_events_db() {
        // The unit-worktree shape: a `.rigger/` (tracked workflow.yml/agents) with NO
        // machine-local events.db must NOT count as a store, so a courier there refuses
        // rather than fabricating a fresh empty store - the exact defect this unit closes.
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::create_dir_all(root.join(RIGGER_DIR)).unwrap();
        std::fs::write(root.join(RIGGER_DIR).join("workflow.yml"), "stages: []\n").unwrap();
        let sub = root.join("nested");
        std::fs::create_dir_all(&sub).unwrap();
        assert_eq!(find_store_dir_from(&sub), None);
    }

    #[test]
    fn find_store_dir_from_walks_past_a_storeless_rigger_to_the_real_store_above() {
        // The REAL production topology: a git-linked unit worktree nested under the repo
        // carries a TRACKED but storeless `.rigger/` (workflow.yml + agents, no machine-
        // local events.db), while the repo root above it holds the real store. A courier
        // run from inside that worktree must walk PAST its own storeless `.rigger/` and
        // resolve the repo's real store - not stop at (nor fabricate under) the storeless
        // one. `find_store_dir_from` keys on `.rigger/events.db` as a FILE, so the storeless
        // intermediate `.rigger/` is correctly skipped; a regression that refused at the
        // first `.rigger/` dir would strand every worker in a real rigger worktree.
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        git_init_quiet(root);
        // The repo root's real store.
        std::fs::create_dir_all(root.join(RIGGER_DIR)).unwrap();
        std::fs::File::create(root.join(RIGGER_DIR).join("events.db")).unwrap();
        // A nested worktree with a tracked-but-storeless `.rigger/` (no events.db).
        let worktree = root.join(".rigger").join("tmp").join("rigger-wt-x");
        std::fs::create_dir_all(worktree.join(RIGGER_DIR)).unwrap();
        std::fs::write(
            worktree.join(RIGGER_DIR).join("workflow.yml"),
            "stages: []\n",
        )
        .unwrap();
        // A courier running from inside the storeless worktree resolves the root's store.
        assert_eq!(
            find_store_dir_from(&worktree),
            Some(root.join(RIGGER_DIR)),
            "must walk past the storeless worktree `.rigger/` to the repo's real store"
        );
    }

    #[test]
    fn walk_stores_from_prefers_the_outermost_store_over_a_nearer_shadow() {
        // Spec 08 item 6: within the bounded walk scope the OUTERMOST store wins. A nested
        // subdir carries its own shadow `.rigger/events.db`; a courier there must bind the
        // repo root's real store, and the walk must REPORT the bypassed shadow so the
        // caller can warn. One git repo => the whole ancestry up to the root is in scope.
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        git_init_quiet(root);
        // The repo root's real store (the outermost in scope).
        std::fs::create_dir_all(root.join(RIGGER_DIR)).unwrap();
        std::fs::File::create(root.join(RIGGER_DIR).join("events.db")).unwrap();
        // A nearer SHADOW store in a nested dir under the repo.
        let nested = root.join("sub").join("deep");
        std::fs::create_dir_all(nested.join(RIGGER_DIR)).unwrap();
        std::fs::File::create(nested.join(RIGGER_DIR).join("events.db")).unwrap();

        let walk = walk_stores_from(&nested);
        assert_eq!(
            walk.dir,
            Some(root.join(RIGGER_DIR)),
            "the outermost (repo root) store must win over the nearer shadow"
        );
        assert_eq!(
            walk.shadows,
            vec![nested.join(RIGGER_DIR)],
            "the bypassed nearer shadow must be reported so the courier can warn"
        );
    }

    #[test]
    fn walk_stores_from_reports_no_shadow_for_a_single_store() {
        // The normal topology - exactly one store in scope - bypasses nothing, so no
        // warning ever fires. Guards against a spurious shadow warning on every courier.
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        git_init_quiet(root);
        std::fs::create_dir_all(root.join(RIGGER_DIR)).unwrap();
        std::fs::File::create(root.join(RIGGER_DIR).join("events.db")).unwrap();
        let sub = root.join("crate").join("src");
        std::fs::create_dir_all(&sub).unwrap();

        let walk = walk_stores_from(&sub);
        assert_eq!(walk.dir, Some(root.join(RIGGER_DIR)));
        assert!(
            walk.shadows.is_empty(),
            "a single store in scope bypasses nothing; got {:?}",
            walk.shadows
        );
    }

    // ---- gate store fence (spec 70 criterion 3): pinned store resolution, never a live store ----

    /// Mutates the process-global CWD and `STORE_FENCE_ENV`; shares the `cwd` serial key
    /// with every other cwd-sensitive test in this file so none of them observe each
    /// other's changed CWD mid-window.
    #[test]
    #[serial_test::serial(cwd)]
    fn require_store_dir_pins_to_the_fence_env_and_never_reaches_the_live_store_above_it() {
        struct Restore(std::path::PathBuf);
        impl Drop for Restore {
            fn drop(&mut self) {
                let _ = std::env::set_current_dir(&self.0);
                std::env::remove_var(STORE_FENCE_ENV);
            }
        }
        let prev = std::env::current_dir().unwrap();
        let _restore = Restore(prev);
        std::env::remove_var(STORE_FENCE_ENV);

        // The REAL production topology this defect hits (matching
        // find_store_dir_from_walks_past_a_storeless_rigger_to_the_real_store_above): a
        // git-linked unit worktree nested under `.rigger/tmp`, carrying no events.db of
        // its own, with the repo root's real (LIVE) store above it.
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        git_init_quiet(root);
        std::fs::create_dir_all(root.join(RIGGER_DIR)).unwrap();
        let live_events = root.join(RIGGER_DIR).join("events.db");
        std::fs::write(&live_events, b"LIVE-STORE-BYTES-BEFORE").unwrap();
        let live_before = std::fs::read(&live_events).unwrap();

        let worktree = root.join(RIGGER_DIR).join("tmp").join("rigger-wt-x");
        std::fs::create_dir_all(worktree.join(RIGGER_DIR)).unwrap();
        std::fs::write(
            worktree.join(RIGGER_DIR).join("workflow.yml"),
            "stages: []\n",
        )
        .unwrap();
        std::env::set_current_dir(&worktree).unwrap();

        // WITHOUT the fence: a real courier from inside the worktree walks up and binds
        // the repo's LIVE store - today's sanctioned behavior for a deliberate agent
        // command (spec 05), reconfirmed here as the baseline the fence must override.
        let (unfenced_loc, _unfenced_sel) =
            require_store_dir().expect("an unfenced courier must resolve the live store above it");
        assert_eq!(
            unfenced_loc.dir,
            root.join(RIGGER_DIR),
            "without a fence, resolution walks up to the repo's live store (the baseline)"
        );

        // WITH the fence set (as ExecRunner sets it for a unit-worktree gate): resolution
        // must land at the fenced scratch dir instead - never the live store above.
        let fence = tempfile::tempdir().unwrap();
        let fence_rigger = fence.path().join(RIGGER_DIR);
        std::env::set_var(STORE_FENCE_ENV, &fence_rigger);
        let (fenced_loc, fenced_sel) =
            require_store_dir().expect("a fenced courier must resolve the pinned scratch dir");
        std::env::remove_var(STORE_FENCE_ENV);
        assert_eq!(
            fenced_loc.dir, fence_rigger,
            "a fenced courier must resolve to the pinned scratch dir, not the live store"
        );
        assert_ne!(
            fenced_loc.dir,
            root.join(RIGGER_DIR),
            "a fenced courier must never resolve to the repo's live store dir"
        );
        assert!(fenced_sel.is_sqlite(), "the fence pins the sqlite backend");

        // The live store is byte-identical before and after the fenced resolution.
        let live_after = std::fs::read(&live_events).unwrap();
        assert_eq!(
            live_before, live_after,
            "the live store must be untouched by a fenced gate's resolution"
        );
    }

    #[test]
    #[serial_test::serial(cwd)]
    fn require_store_dir_fence_is_off_by_default() {
        // Additive, defaulted off (spec 70 Notes): with STORE_FENCE_ENV unset, an
        // unfenced courier's resolution is byte-identical to before the fence existed -
        // a plain store at the cwd resolves normally.
        struct Restore(std::path::PathBuf);
        impl Drop for Restore {
            fn drop(&mut self) {
                let _ = std::env::set_current_dir(&self.0);
            }
        }
        let prev = std::env::current_dir().unwrap();
        let _restore = Restore(prev);
        std::env::remove_var(STORE_FENCE_ENV);

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::create_dir_all(root.join(RIGGER_DIR)).unwrap();
        std::fs::File::create(root.join(RIGGER_DIR).join("events.db")).unwrap();
        std::env::set_current_dir(root).unwrap();

        let (loc, _sel) = require_store_dir().expect("a plain store must resolve normally");
        assert_eq!(loc.dir, root.join(RIGGER_DIR));
    }

    // ---- `rigger result` stderr advisories: orphan id and superseding result ----

    #[test]
    fn result_advisories_flags_an_orphan_id_with_no_spawn_request() {
        // No SpawnRequested is recorded for the id -> exactly the orphan advisory.
        let notes = result_advisories(&[], "u/implementer#0", true);
        assert_eq!(notes.len(), 1, "only the orphan note; got {notes:?}");
        assert!(notes[0].contains("no spawn request is recorded"));
        assert!(notes[0].contains("u/implementer#0"));
    }

    #[test]
    fn result_advisories_orphan_wording_is_plain_on_record_and_conditional_under_if_absent() {
        // Spec 08 item 5: the plain (unconditional) record path keeps its "recording an
        // orphan result" wording, while the `--if-absent` path (`will_supersede` false)
        // states the conditional and NEVER claims a recording it may not make.
        let plain = result_advisories(&[], "u/implementer#0", true);
        assert_eq!(plain.len(), 1, "only the orphan note; got {plain:?}");
        assert!(
            plain[0].contains("recording an orphan result"),
            "the plain path states the recording; got {plain:?}"
        );

        let if_absent = result_advisories(&[], "u/implementer#0", false);
        assert_eq!(
            if_absent.len(),
            1,
            "only the orphan note; got {if_absent:?}"
        );
        assert!(
            if_absent[0].contains("--if-absent records only if the spawn is unanswered"),
            "the --if-absent path states the conditional; got {if_absent:?}"
        );
        assert!(
            !if_absent[0].contains("recording an orphan result"),
            "the --if-absent path must NOT claim a recording; got {if_absent:?}"
        );
    }

    #[test]
    fn result_advisories_is_silent_for_a_parked_unanswered_spawn() {
        // A parked spawn (its request is recorded) with no result yet needs no advisory:
        // this is the normal courier path.
        let req = spawn::SpawnRequest::new("u", "impl", "implementer", 0, "do it");
        let ev = req.to_event().unwrap();
        let notes = result_advisories(std::slice::from_ref(&ev), &req.id, true);
        assert!(
            notes.is_empty(),
            "a parked-but-unanswered spawn needs no note; got {notes:?}"
        );
    }

    #[test]
    fn result_advisories_flags_a_supersede_with_the_prior_result_position() {
        // Request recorded (no orphan) AND a prior result at a known position -> exactly
        // the supersede advisory, naming that position.
        let req = spawn::SpawnRequest::new("u", "impl", "implementer", 0, "do it");
        let req_ev = req.to_event().unwrap();
        let mut res_ev = spawn::SpawnResult::ok(&req.id, "first").to_event().unwrap();
        res_ev.position = 7;
        let notes = result_advisories(&[req_ev, res_ev], &req.id, true);
        assert_eq!(notes.len(), 1, "only the supersede note; got {notes:?}");
        assert!(notes[0].contains("already has a recorded result at position 7"));
        assert!(notes[0].contains("supersedes"));
    }

    #[test]
    fn result_advisories_suppresses_the_supersede_note_when_not_superseding() {
        // The `--if-absent` path (weave with unit-10): the CAS never overwrites, so a
        // supersede note would claim a replacement that never happens. Only the orphan
        // rule applies; a request-and-result pair yields no note at all.
        let req = spawn::SpawnRequest::new("u", "impl", "implementer", 0, "do it");
        let req_ev = req.to_event().unwrap();
        let mut res_ev = spawn::SpawnResult::ok(&req.id, "first").to_event().unwrap();
        res_ev.position = 7;
        let notes = result_advisories(&[req_ev, res_ev], &req.id, false);
        assert!(
            notes.is_empty(),
            "no supersede note on the non-superseding path; got {notes:?}"
        );
    }

    #[test]
    fn result_advisories_flags_both_orphan_and_supersede() {
        // A result recorded against an id the run never requested: BOTH notes fire.
        let mut res_ev = spawn::SpawnResult::ok("typo/id#0", "prev")
            .to_event()
            .unwrap();
        res_ev.position = 3;
        let notes = result_advisories(std::slice::from_ref(&res_ev), "typo/id#0", true);
        assert_eq!(notes.len(), 2, "orphan + supersede; got {notes:?}");
        assert!(notes
            .iter()
            .any(|n| n.contains("no spawn request is recorded")));
        assert!(notes.iter().any(|n| n.contains("at position 3")));
    }

    #[test]
    fn parse_result_error_flag_is_order_independent() {
        // `--error` is a bare flag, so it composes with the output positional in either
        // order: `<id> --error <msg>` and `<id> <msg> --error` both mean the same thing.
        for args in [
            vec![
                "u/adjudicator#1".to_string(),
                "--error".into(),
                "boom".into(),
            ],
            vec![
                "u/adjudicator#1".to_string(),
                "boom".into(),
                "--error".into(),
            ],
        ] {
            let a = parse_result_args(&args).unwrap();
            assert_eq!(a.id, "u/adjudicator#1");
            assert_eq!(a.text.as_deref(), Some("boom"));
            assert!(a.is_error);
        }
    }

    #[test]
    fn parse_result_if_absent_is_off_by_default_and_a_bare_order_independent_flag() {
        // Absent by default (the plain `rigger result` still records unconditionally).
        let plain = parse_result_args(&["u/implementer#0".into(), "done".into()]).unwrap();
        assert!(!plain.if_absent, "--if-absent defaults off");

        // `--if-absent` is a bare flag that composes with `--error` and the output
        // positional in any order (the death courier passes `<id> --if-absent --error <msg>`).
        for args in [
            vec![
                "u/adjudicator#1".to_string(),
                "--if-absent".into(),
                "--error".into(),
                "died".into(),
            ],
            vec![
                "u/adjudicator#1".to_string(),
                "died".into(),
                "--error".into(),
                "--if-absent".into(),
            ],
        ] {
            let a = parse_result_args(&args).unwrap();
            assert_eq!(a.id, "u/adjudicator#1");
            assert_eq!(a.text.as_deref(), Some("died"));
            assert!(a.is_error);
            assert!(a.if_absent, "--if-absent must parse regardless of position");
        }
    }

    #[test]
    fn parse_result_meta_must_be_a_json_object() {
        let a = parse_result_args(&[
            "u/implementer#0".into(),
            "out".into(),
            "--meta".into(),
            r#"{"resolved_model":"claude-x"}"#.into(),
        ])
        .unwrap();
        assert_eq!(a.meta.unwrap()["resolved_model"], "claude-x");

        // A non-object JSON --meta is rejected (mirrors `rigger emit`'s object contract).
        assert!(
            parse_result_args(&[
                "u/implementer#0".into(),
                "--meta".into(),
                "\"just-a-string\"".into(),
            ])
            .is_err(),
            "a non-object --meta is rejected"
        );
        // Invalid JSON is rejected.
        assert!(
            parse_result_args(&[
                "u/implementer#0".into(),
                "--meta".into(),
                "{not json".into()
            ])
            .is_err(),
            "malformed --meta json is rejected"
        );
        // --meta with no following value is rejected.
        assert!(
            parse_result_args(&["u/implementer#0".into(), "--meta".into()]).is_err(),
            "--meta needs a value"
        );
    }

    #[test]
    fn parse_result_rejects_missing_id_extra_args_and_unknown_flags() {
        assert!(parse_result_args(&[]).is_err(), "the id is required");
        assert!(
            parse_result_args(&["".into()]).is_err(),
            "an empty id is rejected"
        );
        assert!(
            parse_result_args(&["id".into(), "out".into(), "extra".into()]).is_err(),
            "a third positional is rejected"
        );
        assert!(
            parse_result_args(&["id".into(), "--bogus".into()]).is_err(),
            "an unknown flag is rejected"
        );
    }

    #[test]
    fn build_result_shapes_success_and_failure() {
        let ok = build_result("u/implementer#0", "the diff", false, None).unwrap();
        assert!(!ok.is_error());
        assert_eq!(ok.output, "the diff");

        let failed = build_result("u/adjudicator#1", "crashed", true, None).unwrap();
        assert!(failed.is_error());
        assert_eq!(failed.error, "crashed");

        // A success may legitimately carry empty output (an agent with no final message).
        assert!(build_result("u/implementer#0", "", false, None)
            .unwrap()
            .output
            .is_empty());
    }

    #[test]
    fn build_result_rejects_a_blank_error_message() {
        // A blank --error would leave is_error() false and replay AS a success, silently
        // swallowing the failure the courier meant to record - so it is rejected.
        assert!(build_result("u/adjudicator#1", "   ", true, None).is_err());
        assert!(build_result("u/adjudicator#1", "", true, None).is_err());
    }

    #[test]
    fn build_result_attaches_meta() {
        let res = build_result(
            "u/implementer#0",
            "out",
            false,
            Some(serde_json::json!({"resolved_model": "claude-x"})),
        )
        .unwrap();
        assert_eq!(res.meta["resolved_model"], "claude-x");
    }

    #[test]
    fn a_recorded_result_lets_the_replay_driver_advance_past_the_spawn() {
        // The acceptance shape for this unit: a result recorded through the SAME seam
        // cmd_result uses (build_result -> spawn::record_result on the per-project
        // namespaced run stream) flips a PARKED spawn to one the replay driver answers -
        // i.e. the next step advances past it (spec 04, Done-when).
        use rigger::conductor::{is_parked, AgentDriver, Error, SpawnOpts};
        use rigger::config::AgentDef;
        use rigger::driver::replay::ReplayDriver;

        let backend = Store::open(":memory:").unwrap();
        let store = Namespaced::new(&backend, "proj");
        let id = spawn::spawn_id("u", spawn::ROLE_IMPLEMENTER, 0);

        let driver = ReplayDriver::new(&store);
        let agent = AgentDef::default();
        let opts = SpawnOpts {
            id: id.clone(),
            unit: "u".into(),
            stage: "u".into(),
            ..Default::default()
        };
        let no_emit = |_: &str, _: serde_json::Value| -> Result<(), Error> { Ok(()) };

        // Before any result is recorded, the frontier PARKS (it waits for the courier).
        let parked = driver
            .spawn(&agent, "do it", &opts, &no_emit)
            .expect_err("an unrecorded spawn parks the frontier");
        assert!(is_parked(&parked));

        // `rigger result u/implementer#0 "the diff"` records the outcome through the seam.
        let res = build_result(&id, "the diff", false, None).unwrap();
        spawn::record_result(&store, &res).unwrap();

        // Now the next step ADVANCES PAST it: the same spawn is answered from the log.
        let answered = driver
            .spawn(&agent, "do it", &opts, &no_emit)
            .expect("a recorded result replays instead of re-parking");
        assert_eq!(answered.output, "the diff");
    }

    #[test]
    fn a_recorded_error_result_replays_as_a_failure_not_a_fake_success() {
        // `rigger result <id> --error <msg>` must replay AS a failure so the conductor
        // remediates it exactly as a live failure, never a fabricated success.
        use rigger::conductor::{is_parked, AgentDriver, Error, SpawnOpts};
        use rigger::config::AgentDef;
        use rigger::driver::replay::ReplayDriver;

        let backend = Store::open(":memory:").unwrap();
        let store = Namespaced::new(&backend, "proj");
        let id = spawn::spawn_id("u", spawn::ROLE_IMPLEMENTER, 0);

        let res = build_result(&id, "worker died: non-zero exit", true, None).unwrap();
        spawn::record_result(&store, &res).unwrap();

        let driver = ReplayDriver::new(&store);
        let agent = AgentDef::default();
        let opts = SpawnOpts {
            id: id.clone(),
            unit: "u".into(),
            stage: "u".into(),
            ..Default::default()
        };
        let no_emit = |_: &str, _: serde_json::Value| -> Result<(), Error> { Ok(()) };

        let err = driver
            .spawn(&agent, "do it", &opts, &no_emit)
            .expect_err("a recorded failure replays as an error");
        assert_eq!(err.0, "worker died: non-zero exit");
        assert!(
            !is_parked(&err),
            "a recorded failure is a real failure, not a park"
        );
    }

    /// Write the scaffold constants into a temp `.rigger/` (the same bytes
    /// `rigger init` emits) and load them through `config::load`: the scaffold must
    /// be a valid, referentially-complete config demonstrating the full DAG shape.
    #[test]
    fn scaffold_parses_into_a_valid_config() {
        let dir = tempfile::tempdir().unwrap();
        let rigger = dir.path().join(RIGGER_DIR);
        let agents = rigger.join("agents");
        std::fs::create_dir_all(&agents).unwrap();
        std::fs::write(rigger.join("workflow.yml"), SCAFFOLD_WORKFLOW).unwrap();
        for (file, content) in SCAFFOLD_AGENTS {
            std::fs::write(agents.join(file), content).unwrap();
        }

        let cfg = config::load(dir.path().to_str().unwrap())
            .expect("the scaffolded config must load and validate");

        // Six CANONICAL agents: planner, rust-engineer, the two reviewer lenses
        // (architecture-reviewer + sdet), the adversary, the adjudicator. Integration is
        // folded into the unit lifecycle (no integrator). None of the four generic
        // placeholder personas is seeded.
        assert_eq!(cfg.agents.len(), 6, "scaffold agent count");
        // Three stages: plan -> plan-critique -> implement. The plan-critique gate
        // (spec 10, Unit 1) reviews the proposed DAG before the fan-out releases.
        assert_eq!(cfg.workflow.stages.len(), 3, "scaffold stage count");
        // Three gates in the reusable library.
        assert_eq!(cfg.workflow.gates.len(), 3, "scaffold gate count");

        // The scaffold exercises the per-unit shape: a producer, the plan-critique gate
        // between plan and implement, a fan-out implement stage that integrates on_pass:
        // merge, and a three-tier review panel declared once on defaults.review.
        let plan = &cfg.workflow.stages["plan"];
        assert_eq!(plan.produces, "dag");
        // The plan-critique gate: review-only (no agent), needs plan, its adversary +
        // adjudicator gate the fan-out.
        let critique = &cfg.workflow.stages["plan-critique"];
        assert!(critique.agent.is_empty(), "the gate implements nothing");
        assert_eq!(critique.needs, ["plan"]);
        assert_eq!(critique.adversary, "adversary");
        assert_eq!(critique.adjudicator, "adjudicator");
        let implement = &cfg.workflow.stages["implement"];
        assert_eq!(implement.strategy, "fan-out");
        assert_eq!(
            implement.needs,
            ["plan-critique"],
            "the fan-out releases only after the plan-critique gate approves"
        );
        assert_eq!(implement.on_pass, "merge");
        let review = &cfg.workflow.defaults.review;
        assert_eq!(
            review.lenses,
            ["architecture-reviewer", "sdet"],
            "tier 1: the two canonical expert lenses"
        );
        assert_eq!(review.adversary, "adversary", "tier 2: refutes the lenses");
        assert_eq!(
            review.adjudicator, "adjudicator",
            "tier 3: the neutral adjudicator gates"
        );
        // The scaffold sets symbols EXPLICITLY (visible, not implicit) - it is the
        // default grounder (the structural symbol index), so a fresh `rigger init`
        // config grounds and reindexes without hitting the retired-grounder error.
        assert_eq!(cfg.workflow.defaults.grounder, "symbols");
        // FIX 3: the scaffold ships a NON-ZERO spawn budget so an unattended `rigger
        // run` cannot spawn unboundedly - 0 would be unlimited.
        assert!(
            cfg.workflow.defaults.budget > 0,
            "the scaffold must ship a non-zero default spawn budget; was {}",
            cfg.workflow.defaults.budget
        );
        assert_eq!(cfg.workflow.defaults.budget, 60, "scaffold default budget");
    }

    /// The two checked-in workflows that ship with the repo - the self-hosted
    /// `.rigger/workflow.yml` and `examples/demo` - must each carry a NON-ZERO spawn
    /// budget (FIX 3): a shipped, unattended config must cap its own spawns. A 0
    /// (unlimited) budget here is what let a runaway loop churn for hours.
    #[test]
    // Reads relative paths (`.`, `..`) so it depends on the process CWD. Another test
    // (`cmd_stats_on_a_never_run_project...`) temporarily `set_current_dir`s to a temp
    // dir; if that runs concurrently, `config::load(".")` here resolves `.` to that
    // temp dir and fails ("read architecture-reviewer.md: No such file"). CWD is
    // process-global, so a restore guard in the other test does not close the window -
    // the two must be mutually exclusive. Both share the `cwd` serial key.
    #[serial_test::serial(cwd)]
    fn shipped_workflows_carry_a_non_zero_spawn_budget() {
        for root in ["..", "../examples/demo", ".", "examples/demo"] {
            // The test runs from the crate root in CI and from the workspace root
            // locally; probe both layouts and skip a path that does not resolve to a
            // loadable config rather than hard-failing on the working directory.
            let path = std::path::Path::new(root);
            if !path.join(RIGGER_DIR).join("workflow.yml").exists() {
                continue;
            }
            let cfg = config::load(root)
                .unwrap_or_else(|e| panic!("shipped workflow at {root:?} must load: {e}"));
            assert!(
                cfg.workflow.defaults.budget > 0,
                "shipped workflow at {root:?} must cap spawns with a non-zero budget; was {}",
                cfg.workflow.defaults.budget
            );
        }
    }

    #[test]
    fn parse_run_args_defaults_to_cli_and_an_unset_store() {
        let a = parse_run_args(&[]).unwrap();
        assert!(a.driver == DriverKind::Cli);
        // No `--eventstore` flag leaves the store UNSET, so the resolver picks it up from the
        // configuration chain (env, then default sqlite) - a flagless `run` is not pinned to
        // sqlite at parse time, which is what lets it honor a server-configured project.
        assert!(a.store.is_none());
        assert!(a.conn.is_none());
        assert!(a.spec.is_none());
        assert!(!a.fresh, "--fresh is off unless asked");
    }

    #[test]
    fn parse_run_args_reads_fresh_alongside_a_spec() {
        // `--fresh` is a bare boolean flag; it composes with a positional spec and the
        // other run flags without consuming a value.
        let a = parse_run_args(&["--fresh".to_string(), "spec.md".to_string()]).unwrap();
        assert!(a.fresh, "--fresh sets the fresh-restart flag");
        assert_eq!(a.spec.as_deref(), Some("spec.md"));
        assert!(a.driver == DriverKind::Cli, "--fresh leaves other defaults");
        assert!(
            !a.rebase_definition,
            "--rebase-definition is off unless asked"
        );
    }

    #[test]
    fn parse_run_args_reads_rebase_definition() {
        // `--rebase-definition` (spec 13, unit 1) is a bare boolean flag, off by default.
        assert!(!parse_run_args(&[]).unwrap().rebase_definition);
        let a =
            parse_run_args(&["--rebase-definition".to_string(), "spec.md".to_string()]).unwrap();
        assert!(
            a.rebase_definition,
            "--rebase-definition sets the mid-campaign-edit escape"
        );
        assert_eq!(a.spec.as_deref(), Some("spec.md"));
    }

    #[test]
    fn parse_run_args_reads_driver_eventstore_conn_and_spec() {
        let args = [
            "spec.md".to_string(),
            "--driver".to_string(),
            "workflow".to_string(),
            "--eventstore".to_string(),
            "kurrentdb".to_string(),
            "--conn".to_string(),
            "kurrentdb://localhost:2113".to_string(),
        ];
        let a = parse_run_args(&args).unwrap();
        assert!(a.driver == DriverKind::Workflow);
        assert!(a.store == Some(StoreKind::KurrentDb));
        assert_eq!(a.conn.as_deref(), Some("kurrentdb://localhost:2113"));
        assert_eq!(a.spec.as_deref(), Some("spec.md"));
    }

    #[test]
    fn parse_run_args_rejects_unknown_flags_and_values() {
        assert!(parse_run_args(&["--driver".into(), "bogus".into()]).is_err());
        assert!(parse_run_args(&["--eventstore".into(), "bogus".into()]).is_err());
        assert!(parse_run_args(&["--nope".into()]).is_err());
        assert!(parse_run_args(&["a".into(), "b".into()]).is_err());
    }

    /// `rigger run`/`rigger serve` accept `--base <ref>` (spec 18, criterion 6): it is no
    /// longer an "unknown flag". The raw argv base is captured (None when absent, so the
    /// default resolves to `origin/main`), it composes with a positional spec in any order,
    /// and a valueless `--base` is a clear error.
    #[test]
    fn parse_run_args_accepts_base_alongside_a_spec() {
        let r = |a: &[&str]| parse_run_args(&a.iter().map(|s| s.to_string()).collect::<Vec<_>>());

        // No --base: the raw base is None (it resolves to the default downstream).
        assert_eq!(r(&[]).unwrap().base, None);

        // `rigger run <spec> --base <ref>` accepts BOTH the spec and the flag, no
        // "unknown flag" / "unexpected second positional".
        let a = r(&["spec.md", "--base", "my-feature"]).unwrap();
        assert_eq!(a.spec.as_deref(), Some("spec.md"));
        assert_eq!(a.base.as_deref(), Some("my-feature"));

        // Order-free: the flag may precede the positional.
        let a = r(&["--base", "origin/next", "spec.md"]).unwrap();
        assert_eq!(a.spec.as_deref(), Some("spec.md"));
        assert_eq!(a.base.as_deref(), Some("origin/next"));

        // A valueless --base is a hard error naming the fix, never a silent default.
        let err = match r(&["--base"]) {
            Ok(_) => panic!("--base without a value must error"),
            Err(e) => e.to_string(),
        };
        assert!(
            err.contains("--base expects a ref"),
            "the error must explain --base needs a ref; got: {err:?}"
        );
    }

    /// [`resolve_run_base`] fixes the run-branch base precedence for a run entry:
    /// an explicit `--base` flag wins, then the `RIGGER_BASE` environment override (how
    /// `rigger workflow` threads its `--base` down through the shim to the served
    /// `rigger serve`), then the load-bearing [`DEFAULT_BASE_REF`]. The bool reports
    /// whether the base was chosen explicitly (flag or env) vs. defaulted.
    #[test]
    fn resolve_run_base_precedence_flag_then_env_then_default() {
        // The explicit flag wins even when the env is also set.
        assert_eq!(
            resolve_run_base(Some("flag-ref"), Some("env-ref")),
            ("flag-ref".to_string(), true)
        );
        // No flag: the RIGGER_BASE env is honored (the `rigger workflow` -> shim thread).
        assert_eq!(
            resolve_run_base(None, Some("env-ref")),
            ("env-ref".to_string(), true)
        );
        // Neither: the default, NOT flagged explicit.
        assert_eq!(
            resolve_run_base(None, None),
            (DEFAULT_BASE_REF.to_string(), false)
        );
        assert_eq!(resolve_run_base(None, None).0, "origin/main");
        // An empty env value is treated as unset (never anchors on "").
        assert_eq!(
            resolve_run_base(None, Some("")),
            (DEFAULT_BASE_REF.to_string(), false)
        );
    }

    /// `rigger workflow` accepts an optional spec AND `--base <ref>` (spec 18, criterion 6):
    /// `--base` is no longer rejected as "expected at most one spec path". Spec and flag
    /// compose in any order; a second positional and a valueless `--base` are hard errors.
    #[test]
    fn parse_workflow_args_reads_spec_and_base() {
        let w =
            |a: &[&str]| parse_workflow_args(&a.iter().map(|s| s.to_string()).collect::<Vec<_>>());

        // Bare: no spec, no base.
        let (spec, base) = w(&[]).unwrap();
        assert!(spec.is_none());
        assert!(base.is_none());

        // Just a spec (the pre-existing behavior).
        let (spec, base) = w(&["spec.md"]).unwrap();
        assert_eq!(spec.as_deref(), Some("spec.md"));
        assert!(base.is_none());

        // `rigger workflow <spec> --base <ref>` and the order-flipped form both parse.
        let (spec, base) = w(&["spec.md", "--base", "my-feature"]).unwrap();
        assert_eq!(spec.as_deref(), Some("spec.md"));
        assert_eq!(base.as_deref(), Some("my-feature"));
        let (spec, base) = w(&["--base", "my-feature", "spec.md"]).unwrap();
        assert_eq!(spec.as_deref(), Some("spec.md"));
        assert_eq!(base.as_deref(), Some("my-feature"));

        // `--base` with no spec is fine (the default spec-less workflow, re-anchored).
        let (spec, base) = w(&["--base", "my-feature"]).unwrap();
        assert!(spec.is_none());
        assert_eq!(base.as_deref(), Some("my-feature"));

        // A second spec path is still the same clear error; a valueless --base names the fix.
        let err = w(&["a.md", "b.md"]).unwrap_err().to_string();
        assert!(
            err.contains("expected at most one spec path"),
            "a second positional must be rejected; got: {err:?}"
        );
        let err = w(&["--base"]).unwrap_err().to_string();
        assert!(
            err.contains("--base expects a ref"),
            "the error must explain --base needs a ref; got: {err:?}"
        );
    }

    /// `rigger step` accepts `--spec` and `--base`: `--base` defaults to `origin/main`,
    /// both flags require a value, and an unknown flag or bare positional is rejected.
    #[test]
    fn parse_step_args_reads_spec_and_base_with_default() {
        let s = |a: &[&str]| parse_step_args(&a.iter().map(|s| s.to_string()).collect::<Vec<_>>());

        // Default base when --base is not given; no spec. The default is NOT flagged as
        // explicit, so steady-state reuse stays silent.
        let a = s(&[]).unwrap();
        assert_eq!(a.base, DEFAULT_BASE_REF);
        assert_eq!(a.base, "origin/main");
        assert!(a.spec.is_none());
        assert!(!a.base_explicit, "an unspecified --base is not explicit");

        // --base overrides the default and is flagged explicit; --spec is read
        // independently and order-free.
        let a = s(&["--base", "rigger-run-1"]).unwrap();
        assert_eq!(a.base, "rigger-run-1");
        assert!(a.spec.is_none());
        assert!(a.base_explicit, "a given --base is explicit");

        let a = s(&["--spec", "specs/04.md", "--base", "origin/next"]).unwrap();
        assert_eq!(a.spec.as_deref(), Some("specs/04.md"));
        assert_eq!(a.base, "origin/next");
        assert!(a.base_explicit);

        // An explicit --base equal to the default is still explicit (so an ignored
        // re-anchor to origin/main is reported, not swallowed as a default).
        let a = s(&["--base", "origin/main"]).unwrap();
        assert_eq!(a.base, "origin/main");
        assert!(a.base_explicit);

        // Each flag requires its value; typos and positionals are hard errors.
        assert!(s(&["--base"]).is_err(), "--base without a value must error");
        assert!(s(&["--spec"]).is_err(), "--spec without a value must error");
        assert!(s(&["--nope"]).is_err(), "an unknown flag must error");
        assert!(s(&["bare"]).is_err(), "a bare positional must error");

        // `--fresh` is a bare boolean flag (off by default), composing with the others.
        assert!(!s(&[]).unwrap().fresh, "--fresh is off unless asked");
        let a = s(&["--fresh", "--spec", "specs/12.md"]).unwrap();
        assert!(a.fresh, "--fresh sets the fresh-restart flag on a step");
        assert_eq!(a.spec.as_deref(), Some("specs/12.md"));

        // `--rebase-definition` (spec 13, unit 1) is likewise a bare boolean, off by default.
        assert!(
            !s(&[]).unwrap().rebase_definition,
            "--rebase-definition is off unless asked"
        );
        let a = s(&["--rebase-definition", "--base", "origin/next"]).unwrap();
        assert!(
            a.rebase_definition,
            "--rebase-definition sets the mid-campaign-edit escape on a step"
        );
        assert_eq!(a.base, "origin/next");
    }

    /// The definition hash (spec 13, unit 1) is a DETERMINISTIC function of the on-disk
    /// definition that CHANGES when any part of it - a prompt above all - changes, and is
    /// independent of agent-file iteration order and of trailing-whitespace / line-ending noise.
    #[test]
    fn definition_hash_is_stable_and_content_sensitive() {
        let write_def = |root: &std::path::Path, workflow: &str, prompt: &str| {
            let agents = root.join(".rigger").join("agents");
            std::fs::create_dir_all(&agents).unwrap();
            std::fs::write(root.join(".rigger").join("workflow.yml"), workflow).unwrap();
            std::fs::write(
                agents.join("worker.md"),
                format!("---\nid: worker\n---\n{prompt}\n"),
            )
            .unwrap();
        };

        let base = tempfile::tempdir().unwrap();
        write_def(base.path(), "name: w\n", "Do the unit.");
        let dir = base.path().to_str().unwrap();
        let h0 = definition_hash(dir).unwrap();
        // Deterministic: recomputing over the same on-disk definition is byte-identical.
        assert_eq!(
            h0,
            definition_hash(dir).unwrap(),
            "same definition, same hash"
        );
        // Canonicalization: trailing whitespace and CRLF do NOT change the hash.
        write_def(base.path(), "name: w\r\n", "Do the unit.   ");
        assert_eq!(
            h0,
            definition_hash(dir).unwrap(),
            "trailing-ws / CRLF noise is canonicalized away"
        );
        // A PROMPT edit changes the hash - the mid-campaign edit spec 13 must catch.
        write_def(base.path(), "name: w\n", "Do the unit differently.");
        assert_ne!(
            h0,
            definition_hash(dir).unwrap(),
            "a prompt edit changes the definition hash"
        );
        // A workflow.yml edit changes the hash too.
        let with_wf = definition_hash(dir).unwrap();
        write_def(base.path(), "name: changed\n", "Do the unit differently.");
        assert_ne!(
            with_wf,
            definition_hash(dir).unwrap(),
            "a workflow edit changes the hash"
        );
    }

    /// KurrentDB is ALWAYS AVAILABLE (spec 47): the adapter is compiled into every
    /// build, not gated behind a cargo feature. Selecting it WITHOUT a connection
    /// string fails with the missing-`--conn` error - proving the real adapter is
    /// compiled in and reachable - and NEVER with a missing-cargo-feature error
    /// (which can no longer happen). Ungated on purpose: this must hold in BOTH
    /// feature lanes (default and `--no-default-features`).
    #[test]
    fn kurrentdb_is_always_available_and_needs_a_conn() {
        // Resolve over an EMPTY `.rigger` with no credential source anywhere - no flag conn, no
        // environment (threaded as `None`), no secret file - so the flag-selected server has
        // nothing to resolve and hits the missing-connection-string guard. Hermetic: independent
        // of both the ambient repo's config and the process environment.
        let tmp = tempfile::tempdir().unwrap();
        let rigger_dir = tmp.path().join(".rigger");
        std::fs::create_dir_all(&rigger_dir).unwrap();

        let err = match store_selection_at(Some(StoreKind::KurrentDb), None, None, &rigger_dir) {
            Ok(_) => panic!("kurrentdb without a conn must not select a store"),
            Err(e) => e.to_string(),
        };

        // The real adapter's missing-conn guard names the --conn / KURRENTDB_CONN
        // channel - reaching it proves the adapter is compiled in.
        assert!(
            err.contains("--conn") || err.contains("KURRENTDB_CONN"),
            "the error must be the missing-connection-string error, proving the adapter is \
             reachable; got: {err}"
        );
        // It must NOT be the retired missing-feature error: the adapter is always
        // compiled in, so no "requires the cargo feature" dead end can occur.
        assert!(
            !err.contains("feature"),
            "the adapter is always compiled in, so no missing-feature error can occur; got: {err}"
        );
    }

    /// Spec 48, criterion 4 - NO TOPOLOGY OPINIONS, the resolver's half. The selection chain is a
    /// pure conduit for the connection string: whichever rung supplies it - an explicit `--conn`
    /// flag, the `KURRENTDB_CONN` environment, or the `.rigger/store.conn` secret file - the string
    /// reaches `StoreSelection::Server` BYTE FOR BYTE. The resolver strips no credential, normalizes
    /// no TLS parameter, and rewrites no host; the only code that ever interprets the address is the
    /// adapter's client (proven in `eventstore::kurrentdb`). Exercised with a REMOTE, TLS-secured,
    /// CREDENTIALED address - nothing a localhost default would produce - through each credential
    /// rung, asserting the resolved string is identical to the input. Hermetic: a temp `.rigger`
    /// and threaded environment, so no process-env mutation and deterministic under parallelism.
    #[test]
    fn store_selection_preserves_a_credentialed_tls_conn_verbatim() {
        use std::fs;
        let tmp = tempfile::tempdir().unwrap();
        let rigger_dir = tmp.path().join(".rigger");
        fs::create_dir_all(&rigger_dir).unwrap();

        // A remote host, TLS on, real credentials, a non-default port: every part a topology
        // opinion (a localhost default, a forced tls=false, a dropped credential) would corrupt.
        let conn = "kurrentdb://operator:s3cr3t@events.internal.example:2113?tls=true";
        let expected = StoreSelection::Server(conn.to_string());

        // Rung 1: an explicit --conn flag reaches Server verbatim.
        assert_eq!(
            store_selection_at(None, Some(conn), None, &rigger_dir).unwrap(),
            expected,
            "a --conn flag reaches the adapter verbatim - the resolver injects no topology opinion"
        );
        // Rung 2: the KURRENTDB_CONN environment value reaches Server verbatim.
        assert_eq!(
            store_selection_at(None, None, Some(conn.to_string()), &rigger_dir).unwrap(),
            expected,
            "the environment connection string reaches the adapter verbatim"
        );
        // Rung 3: the .rigger/store.conn secret file reaches Server verbatim (no surrounding
        // whitespace, so the file rung's line-trim leaves the address itself untouched).
        fs::write(rigger_dir.join("store.conn"), conn).unwrap();
        assert_eq!(
            store_selection_at(None, None, None, &rigger_dir).unwrap(),
            expected,
            "the secret-file connection string reaches the adapter verbatim"
        );
    }

    /// Spec 48, criterion 2 - PRECEDENCE. `store_selection_at` resolves the event-log backend
    /// from the configuration sources in one STRICT order every command shares: an explicit flag
    /// beats the environment beats the local secret file (`.rigger/store.conn`) beats the
    /// committed project config (`store:` in `workflow.yml`) beats the embedded-sqlite default.
    /// Proven here over the PURE core - a temp `.rigger` for the two file-backed rungs and the
    /// environment threaded as a value, so no process-env mutation is needed and the ordering is
    /// deterministic under parallel test execution. Each source carries a DISTINCT value, so the
    /// value the result carries names the winning rung unambiguously.
    #[test]
    fn store_selection_precedence_flag_env_secret_file_config_then_default() {
        use std::fs;
        let tmp = tempfile::tempdir().unwrap();
        let rigger_dir = tmp.path().join(".rigger");
        fs::create_dir_all(&rigger_dir).unwrap();

        // (Re)write the two file-backed rungs; `None` removes the file so the rung is absent.
        let write_secret = |conn: Option<&str>| match conn {
            Some(c) => fs::write(rigger_dir.join("store.conn"), c).unwrap(),
            None => {
                let _ = fs::remove_file(rigger_dir.join("store.conn"));
            }
        };
        let write_config = |body: Option<&str>| match body {
            Some(b) => fs::write(rigger_dir.join("workflow.yml"), b).unwrap(),
            None => {
                let _ = fs::remove_file(rigger_dir.join("workflow.yml"));
            }
        };
        let sel = |flag_store, flag_conn: Option<&str>, env: Option<&str>| {
            store_selection_at(flag_store, flag_conn, env.map(String::from), &rigger_dir)
        };
        let server = |host: &str| StoreSelection::Server(host.to_string());

        // The two lower file-backed sources present together, each a DISTINCT address.
        write_secret(Some("kurrentdb://secret-file:2113?tls=false"));
        write_config(Some(
            "store:\n  backend: kurrentdb\n  url: \"kurrentdb://config-host:2113?tls=false\"\n",
        ));

        // 1. The explicit flag wins over env, the secret file, and the config.
        assert_eq!(
            sel(
                Some(StoreKind::KurrentDb),
                Some("kurrentdb://flag-host:2113?tls=false"),
                Some("kurrentdb://env-host:2113?tls=false"),
            )
            .unwrap(),
            server("kurrentdb://flag-host:2113?tls=false"),
            "an explicit --conn flag is the highest-precedence source"
        );
        // A flag selecting sqlite also wins outright, even with a server env/secret/config live.
        assert_eq!(
            sel(
                Some(StoreKind::Sqlite),
                None,
                Some("kurrentdb://env-host:2113?tls=false"),
            )
            .unwrap(),
            StoreSelection::Sqlite,
            "--eventstore sqlite beats every lower source"
        );
        // A BARE --conn (flag_store=None) SELECTS the server addressed verbatim by it: a non-empty
        // --conn is a first-class highest-precedence source, never silently dropped to a lower rung
        // (the store-fracture footgun spec 48 motivates against - d-u2-conn-flag-selects-server).
        assert_eq!(
            sel(None, Some("kurrentdb://bare-conn:2113?tls=false"), None).unwrap(),
            server("kurrentdb://bare-conn:2113?tls=false"),
            "a bare --conn selects the server, beating the secret file and config beneath it"
        );
        // The bare --conn outranks the environment too (it is rung 1; KURRENTDB_CONN is rung 2).
        assert_eq!(
            sel(
                None,
                Some("kurrentdb://bare-conn:2113?tls=false"),
                Some("kurrentdb://env-host:2113?tls=false"),
            )
            .unwrap(),
            server("kurrentdb://bare-conn:2113?tls=false"),
            "a bare --conn outranks KURRENTDB_CONN"
        );
        // An explicit --eventstore sqlite still wins OUTRIGHT over a --conn: the flag-store is the
        // unambiguous backend override, so contradictory flags resolve to sqlite, never the server.
        assert_eq!(
            sel(
                Some(StoreKind::Sqlite),
                Some("kurrentdb://bare-conn:2113?tls=false"),
                None,
            )
            .unwrap(),
            StoreSelection::Sqlite,
            "--eventstore sqlite wins outright even with a --conn present"
        );
        // An EMPTY --conn is not a selection: it is unset, so the rungs beneath decide (here the
        // secret file), exactly as an absent flag would - a stray `--conn ''` never selects a
        // server with no address.
        assert_eq!(
            sel(None, Some(""), None).unwrap(),
            server("kurrentdb://secret-file:2113?tls=false"),
            "an empty --conn is unset, so the secret file wins beneath it"
        );

        // 2. No flag: the environment beats the secret file and the config.
        assert_eq!(
            sel(None, None, Some("kurrentdb://env-host:2113?tls=false")).unwrap(),
            server("kurrentdb://env-host:2113?tls=false"),
            "KURRENTDB_CONN beats the secret file and the committed config"
        );
        // An empty environment value is treated as unset (never selects a server with no address).
        assert_eq!(
            sel(None, None, Some("")).unwrap(),
            server("kurrentdb://secret-file:2113?tls=false"),
            "an empty env value is unset, so the secret file wins beneath it"
        );

        // 3. No flag, no env: the local secret file beats the config.
        assert_eq!(
            sel(None, None, None).unwrap(),
            server("kurrentdb://secret-file:2113?tls=false"),
            ".rigger/store.conn beats the committed config"
        );

        // 4. No flag, no env, no secret file: the committed config's non-secret URL is used.
        write_secret(None);
        assert_eq!(
            sel(None, None, None).unwrap(),
            server("kurrentdb://config-host:2113?tls=false"),
            "the committed store: config beats the default"
        );
        // A config pinning sqlite explicitly resolves the embedded store.
        write_config(Some("store:\n  backend: sqlite\n"));
        assert_eq!(
            sel(None, None, None).unwrap(),
            StoreSelection::Sqlite,
            "store: sqlite in the config selects the embedded backend"
        );

        // 5. Nothing configured anywhere: the embedded-sqlite default (backward compatible).
        write_config(None);
        assert_eq!(
            sel(None, None, None).unwrap(),
            StoreSelection::Sqlite,
            "no source configured resolves the sqlite default"
        );
        // A workflow.yml with no `store:` key is also "no opinion" -> the default.
        write_config(Some("name: demo\n"));
        assert_eq!(
            sel(None, None, None).unwrap(),
            StoreSelection::Sqlite,
            "a config without a store: key defaults to sqlite"
        );

        // The three-source error: the server is selected (config pins kurrentdb) with NO url and
        // no credential source anywhere - the error must name ALL THREE credential channels.
        write_config(Some("store:\n  backend: kurrentdb\n"));
        // The TWIN of the rung-1 drop (config-kurrentdb-with-no-url + a CLI --conn): the config
        // selects the server but carries no url, and the CLI --conn is the credential that resolves
        // it. The --conn is NEVER dropped here either - the exact input that previously fell through
        // to the sqlite default now resolves the server verbatim from the flag.
        assert_eq!(
            sel(None, Some("kurrentdb://flag-conn:2113?tls=false"), None).unwrap(),
            server("kurrentdb://flag-conn:2113?tls=false"),
            "a config-selected server with no url takes the --conn credential, never drops it"
        );
        let err = sel(None, None, None).unwrap_err().to_string();
        assert!(
            err.contains("--conn") && err.contains("KURRENTDB_CONN") && err.contains("store.conn"),
            "a config-selected server with no resolvable conn must name all three credential \
             sources; got: {err}"
        );
        // And the same three-source error when the flag selects the server with nothing to resolve.
        let err = sel(Some(StoreKind::KurrentDb), None, None)
            .unwrap_err()
            .to_string();
        assert!(
            err.contains("--conn") && err.contains("KURRENTDB_CONN") && err.contains("store.conn"),
            "--eventstore kurrentdb with no conn must name all three credential sources; got: {err}"
        );

        // An unknown backend in the committed config is a clear configuration error, not a silent
        // fallback to the default.
        write_config(Some("store:\n  backend: bogus\n"));
        let err = sel(None, None, None).unwrap_err().to_string();
        assert!(
            err.contains("bogus") && err.contains("sqlite") && err.contains("kurrentdb"),
            "an unknown store.backend must be rejected naming the valid values; got: {err}"
        );
    }

    #[test]
    fn project_identity_is_never_empty() {
        assert!(!project_identity().is_empty());
    }

    #[test]
    fn project_identity_reads_the_tracked_id_file_then_falls_back_to_the_basename() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        // No project.id: identity is the legacy basename (unchanged pre-spec-09 behavior).
        let basename = root.file_name().unwrap().to_str().unwrap().to_string();
        assert_eq!(project_identity_at(root), basename);
        assert_eq!(legacy_identity_at(root), basename);

        // A tracked project.id, when present (and trimmed), IS the identity - it survives a
        // directory rename because it does not track the basename.
        std::fs::create_dir_all(root.join(RIGGER_DIR)).unwrap();
        std::fs::write(
            root.join(RIGGER_DIR).join(PROJECT_ID_FILE),
            "  durable-id-42 \n",
        )
        .unwrap();
        assert_eq!(project_identity_at(root), "durable-id-42");
        // The legacy resolver ignores the file, so the migration can still name the "before".
        assert_eq!(legacy_identity_at(root), basename);
        assert!(has_tracked_project_id(root));

        // A blank id file is treated as absent (falls back), never an empty identity.
        std::fs::write(root.join(RIGGER_DIR).join(PROJECT_ID_FILE), "   \n").unwrap();
        assert_eq!(project_identity_at(root), basename);
        assert!(!has_tracked_project_id(root));
    }

    #[test]
    fn ssh_https_and_git_suffix_forms_of_one_repo_mint_identical_ids() {
        let forms = [
            "git@github.com:Acme/Repo.git",
            "https://github.com/Acme/Repo.git",
            "https://github.com/Acme/Repo",
            "ssh://git@github.com/Acme/Repo.git",
            "git://github.com/Acme/Repo.git",
            "https://GitHub.com/Acme/Repo.git/",
        ];
        // Every form canonicalizes to the same normalized URL...
        assert_eq!(normalize_origin_url(forms[0]), "github.com/Acme/Repo");
        for f in forms {
            assert_eq!(
                normalize_origin_url(f),
                "github.com/Acme/Repo",
                "form {f:?} must normalize identically"
            );
        }
        // ...so the derived stable id is identical across all forms.
        let id0 = format!(
            "{:016x}",
            fnv1a_64(normalize_origin_url(forms[0]).as_bytes())
        );
        for f in forms {
            let id = format!("{:016x}", fnv1a_64(normalize_origin_url(f).as_bytes()));
            assert_eq!(id, id0, "form {f:?} must mint the same id");
        }
    }

    #[test]
    fn normalize_origin_url_separates_distinct_repos_and_lowercases_only_the_host() {
        assert_ne!(
            normalize_origin_url("git@github.com:Acme/One.git"),
            normalize_origin_url("git@github.com:Acme/Two.git")
        );
        // Host case is normalized; path case is significant (never lowercased).
        assert_eq!(
            normalize_origin_url("https://GITHUB.com/Acme/Repo"),
            normalize_origin_url("https://github.com/Acme/Repo")
        );
        assert_ne!(
            normalize_origin_url("https://github.com/Acme/Repo"),
            normalize_origin_url("https://github.com/acme/repo")
        );
    }

    #[test]
    fn decide_migration_covers_every_case() {
        // No minted identity distinct from the basename: nothing to migrate, ever.
        assert_eq!(
            decide_migration("same", "same", false, false),
            MigrationOutcome::NoOp
        );
        assert_eq!(
            decide_migration("same", "same", true, true),
            MigrationOutcome::NoOp
        );
        // Legacy history with an empty minted namespace: rename once.
        assert_eq!(
            decide_migration("minted", "legacy", false, true),
            MigrationOutcome::Rename
        );
        // BOTH namespaces populated: ambiguous, refuse.
        assert_eq!(
            decide_migration("minted", "legacy", true, true),
            MigrationOutcome::Ambiguous
        );
        // Already migrated (minted populated, legacy empty) or fresh (both empty): no-op.
        assert_eq!(
            decide_migration("minted", "legacy", true, false),
            MigrationOutcome::NoOp
        );
        assert_eq!(
            decide_migration("minted", "legacy", false, false),
            MigrationOutcome::NoOp
        );
    }

    #[test]
    fn migrate_project_identity_renames_legacy_history_and_records_a_decision() {
        use rigger::eventstore::ExpectedRevision;
        let backend = Store::open(":memory:").unwrap();
        // Pre-spec-09 history under the legacy basename namespace.
        backend
            .append(
                "proj-oldname-run",
                ExpectedRevision::Any,
                &[Event::new("UnitStarted", b"{}".to_vec())],
            )
            .unwrap();

        let moved = migrate_project_identity(&backend, "mint123", "oldname", None).unwrap();
        assert_eq!(moved, Some(1), "one legacy stream renamed");

        // The legacy namespace is now empty; the minted namespace holds the history.
        assert!(backend
            .read_stream("proj-oldname-run", 0, Direction::Forward)
            .unwrap()
            .is_empty());
        let migrated = backend
            .read_stream("proj-mint123-run", 0, Direction::Forward)
            .unwrap();
        assert!(
            migrated.iter().any(|e| e.type_ == "UnitStarted"),
            "the original history moved to the minted namespace"
        );
        assert!(
            migrated
                .iter()
                .any(|e| e.type_ == contextgraph::TYPE_DECISION_MADE),
            "the migration is recorded as a DecisionMade in the minted namespace"
        );

        // Idempotent: a second open sees the legacy namespace empty and does nothing.
        assert_eq!(
            migrate_project_identity(&backend, "mint123", "oldname", None).unwrap(),
            None
        );
    }

    #[test]
    fn migrate_project_identity_refuses_when_both_namespaces_hold_history() {
        use rigger::eventstore::ExpectedRevision;
        let backend = Store::open(":memory:").unwrap();
        backend
            .append(
                "proj-oldname-run",
                ExpectedRevision::Any,
                &[Event::new("A", b"".to_vec())],
            )
            .unwrap();
        backend
            .append(
                "proj-mint123-run",
                ExpectedRevision::Any,
                &[Event::new("B", b"".to_vec())],
            )
            .unwrap();

        let err = migrate_project_identity(&backend, "mint123", "oldname", None).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("mint123") && msg.contains("oldname"),
            "the refusal names BOTH identities; got: {msg}"
        );
        // Nothing was renamed - both namespaces are intact.
        assert_eq!(
            backend
                .read_stream("proj-oldname-run", 0, Direction::Forward)
                .unwrap()
                .len(),
            1
        );
        assert_eq!(
            backend
                .read_stream("proj-mint123-run", 0, Direction::Forward)
                .unwrap()
                .len(),
            1
        );
    }

    #[test]
    fn migrate_project_identity_rekeys_graph_rows_so_pre_mint_history_is_not_orphaned() {
        use rigger::eventstore::ExpectedRevision;
        // Spec 28 GC5 (backward-compat): a single-project deployment behaves EXACTLY as before,
        // even across the spec-09 identity mint. The identity migration renames event STREAMS
        // (`rename_stream_prefix`), but the graph folds incrementally, so the renamed streams are
        // NEVER re-folded - its pre-mint rows keep the legacy scope. Once the read filter
        // (criterion 2) scopes reads to the minted identity, that pre-mint history would be
        // SILENTLY ORPHANED. `migrate_project_identity` must therefore re-key the graph rows the
        // same way it renames the streams, so the minted read still returns the pre-mint history.
        let dir = tempfile::tempdir().unwrap();
        let store_path = dir.path().join("events.db");
        let graph_path = dir.path().join("graph.db");
        let store_path = store_path.to_str().unwrap();
        let graph_path = graph_path.to_str().unwrap();

        // The deployment runs under its basename identity "oldname": it appends a stream under
        // the legacy namespace and folds a decision into the graph tagged "oldname".
        let backend = Store::open(store_path).unwrap();
        backend
            .append(
                "proj-oldname-run",
                ExpectedRevision::Any,
                &[Event::new("UnitStarted", b"{}".to_vec())],
            )
            .unwrap();
        {
            let legacy_graph = Projector::open(graph_path, "oldname").unwrap();
            let payload = serde_json::json!({
                "id": "pre-d", "summary": "s", "governs": ["pre.rs"], "supersedes": "",
            });
            let mut e = Event::new(
                contextgraph::TYPE_DECISION_MADE,
                serde_json::to_vec(&payload).unwrap(),
            );
            e.position = 1;
            legacy_graph.apply(&e).unwrap();
        }

        // It then mints `.rigger/project.id`: the migration opens the graph under the MINTED
        // identity and migrates. Before the re-key fix the graph rows kept the legacy scope, so
        // the minted read returned nothing - the pre-mint history was orphaned.
        let graph = Projector::open(graph_path, "mint123").unwrap();
        let moved = migrate_project_identity(&backend, "mint123", "oldname", Some(&graph)).unwrap();
        assert_eq!(
            moved,
            Some(1),
            "the one legacy stream is renamed to the minted namespace"
        );

        // Backward-compat: the minted projector still returns the pre-mint decision and its
        // governed file - the single-project deployment behaves EXACTLY as before the mint.
        let g = graph.subgraph(&["pre.rs".to_string()], 2).unwrap();
        assert!(
            g.nodes.iter().any(|n| n.id == "pre-d"),
            "the pre-mint decision is re-keyed to the minted scope and stays reachable, got {g:?}"
        );
        assert!(
            g.nodes.iter().any(|n| n.id == "pre.rs"),
            "the pre-mint governed file is re-keyed too, got {g:?}"
        );
        assert_eq!(
            graph.resolve("pre-d").unwrap().as_deref(),
            Some("pre-d"),
            "the pre-mint node resolves under the minted identity after migration"
        );
    }

    #[test]
    fn migrate_project_identity_rekeys_the_graph_before_the_irreversible_stream_rename() {
        use rigger::eventstore::ExpectedRevision;
        // Spec 28 GC5 (backward-compat), crash-safety ORDERING. The identity migration mutates
        // TWO databases with no shared transaction: it re-keys the graph (graph.db) and renames
        // the event streams (events.db). `decide_migration` returns `Rename` ONLY while the legacy
        // namespace still holds streams, and `rename_stream_prefix` is the SOLE step that clears
        // it - so the rename is the irreversible commit point and MUST run LAST. Were the rename
        // to run first, a graph re-key that then failed (a composite `(id, project)` key collision,
        // or a locked shared backend) would leave the streams renamed but the graph rows
        // un-re-keyed, and because the legacy namespace is now empty a re-open would NoOp forever,
        // permanently orphaning the pre-mint graph history under the minted read filter. Pin the
        // ordering: a FAILED re-key must leave the stream rename UNCOMMITTED, so the whole
        // migration stays retryable on recovery.
        let dir = tempfile::tempdir().unwrap();
        let store_path = dir.path().join("events.db");
        let graph_path = dir.path().join("graph.db");
        let store_path = store_path.to_str().unwrap();
        let graph_path = graph_path.to_str().unwrap();

        // Pre-mint history under the legacy basename identity "oldname": a stream plus a folded
        // decision (its node "pre-d" and its governed-file node).
        let backend = Store::open(store_path).unwrap();
        backend
            .append(
                "proj-oldname-run",
                ExpectedRevision::Any,
                &[Event::new("UnitStarted", b"{}".to_vec())],
            )
            .unwrap();
        let apply_pre_d = |g: &Projector, pos: u64, governs: &str| {
            let payload = serde_json::json!({
                "id": "pre-d", "summary": "s", "governs": [governs], "supersedes": "",
            });
            let mut e = Event::new(
                contextgraph::TYPE_DECISION_MADE,
                serde_json::to_vec(&payload).unwrap(),
            );
            e.position = pos;
            g.apply(&e).unwrap();
        };
        {
            let legacy_graph = Projector::open(graph_path, "oldname").unwrap();
            apply_pre_d(&legacy_graph, 1, "pre.rs");
        }

        // Force the graph re-key to FAIL: seed the MINTED scope with a node whose id ("pre-d")
        // collides with a legacy node, so `migrate_project`'s `UPDATE nodes SET project=minted`
        // hits the composite `(id, project)` primary key and errors (the whole re-key transaction
        // rolls back atomically). This is one of the two `migrate_project` Err paths the design
        // itself flags.
        let graph = Projector::open(graph_path, "mint123").unwrap();
        apply_pre_d(&graph, 2, "other.rs");

        // The migration must ERROR (the re-key cannot complete)...
        let err =
            migrate_project_identity(&backend, "mint123", "oldname", Some(&graph)).unwrap_err();
        assert!(
            !err.to_string().is_empty(),
            "the failed graph re-key surfaces an error"
        );

        // ...and because the re-key runs BEFORE the rename, the irreversible stream rename never
        // committed: the legacy namespace is STILL populated and the minted namespace is STILL
        // empty, so a re-open decides `Rename` again and the migration is retryable. (Under the
        // rejected ordering the rename committed first, emptying the legacy namespace and stranding
        // the graph forever.)
        assert!(
            !backend
                .read_stream("proj-oldname-run", 0, Direction::Forward)
                .unwrap()
                .is_empty(),
            "the stream rename did NOT commit when the graph re-key failed (rename must run last)"
        );
        assert!(
            backend
                .read_stream("proj-mint123-run", 0, Direction::Forward)
                .unwrap()
                .is_empty(),
            "the aborted migration moved no history into the minted namespace"
        );
    }

    #[test]
    fn migrate_project_identity_recovers_from_a_crash_between_the_rekey_and_the_rename() {
        use rigger::eventstore::ExpectedRevision;
        // Spec 28 GC5 (backward-compat), crash-safety RECOVERY. Because the graph re-key runs
        // BEFORE the irreversible stream rename, a crash in the window (graph re-key committed, the
        // rename not yet) leaves the legacy namespace still populated. Recovery therefore decides
        // `Rename` again, REPLAYS the idempotent re-key (which now moves 0 rows, never a duplicate
        // or a collision), and completes the rename - so the pre-mint history stays visible under
        // the minted read filter, exactly as before the mint.
        let dir = tempfile::tempdir().unwrap();
        let store_path = dir.path().join("events.db");
        let graph_path = dir.path().join("graph.db");
        let store_path = store_path.to_str().unwrap();
        let graph_path = graph_path.to_str().unwrap();

        let backend = Store::open(store_path).unwrap();
        backend
            .append(
                "proj-oldname-run",
                ExpectedRevision::Any,
                &[Event::new("UnitStarted", b"{}".to_vec())],
            )
            .unwrap();
        {
            let legacy_graph = Projector::open(graph_path, "oldname").unwrap();
            let payload = serde_json::json!({
                "id": "pre-d", "summary": "s", "governs": ["pre.rs"], "supersedes": "",
            });
            let mut e = Event::new(
                contextgraph::TYPE_DECISION_MADE,
                serde_json::to_vec(&payload).unwrap(),
            );
            e.position = 1;
            legacy_graph.apply(&e).unwrap();
        }

        // Reproduce the crash-window STATE the correct ordering leaves behind: the graph re-key
        // committed (both pre-mint nodes are already at minted) but the stream rename did not.
        let graph = Projector::open(graph_path, "mint123").unwrap();
        assert_eq!(
            graph.migrate_project("oldname", "mint123").unwrap(),
            2,
            "the crash lands AFTER the graph re-key: the two pre-mint nodes are already at minted"
        );
        assert!(
            !backend
                .read_stream("proj-oldname-run", 0, Direction::Forward)
                .unwrap()
                .is_empty(),
            "the crash lands BEFORE the rename: the legacy namespace is still populated"
        );

        // Recovery: re-run the migration. It decides `Rename` again (legacy still populated),
        // replays the idempotent re-key, and completes the rename that the crash interrupted.
        let moved = migrate_project_identity(&backend, "mint123", "oldname", Some(&graph)).unwrap();
        assert_eq!(
            moved,
            Some(1),
            "recovery completes the stream rename the crash interrupted"
        );
        // The re-key was a clean 0-row no-op on the recovery replay: a further replay still moves
        // nothing (idempotent), so recovery never duplicated or re-moved a row.
        assert_eq!(
            graph.migrate_project("oldname", "mint123").unwrap(),
            0,
            "the graph re-key is idempotent: once re-keyed, replays move 0 rows"
        );

        // Backward-compat holds after recovery: the minted read still returns the pre-mint history
        // (exactly one un-duplicated decision node), and the rename completed.
        let g = graph.subgraph(&["pre.rs".to_string()], 2).unwrap();
        assert_eq!(
            g.nodes.iter().filter(|n| n.id == "pre-d").count(),
            1,
            "exactly one pre-mint decision node is reachable under minted (no duplicate), got {g:?}"
        );
        assert_eq!(
            graph.resolve("pre-d").unwrap().as_deref(),
            Some("pre-d"),
            "the pre-mint node resolves under the minted identity after recovery"
        );
        assert!(
            backend
                .read_stream("proj-oldname-run", 0, Direction::Forward)
                .unwrap()
                .is_empty(),
            "the legacy namespace is empty after the recovered rename"
        );
        assert!(
            backend
                .read_stream("proj-mint123-run", 0, Direction::Forward)
                .unwrap()
                .iter()
                .any(|e| e.type_ == "UnitStarted"),
            "the pre-mint history now lives under the minted namespace"
        );
    }

    /// Spec 45, criterion 2 (DIRECT-PROJECTION REACH): the `/api/graph` provider reads the WHOLE
    /// projection directly, not the run-seeded `subgraph(graph_seeds(events), 2)`. So on an
    /// indexed-but-never-built repo - a graph populated by code ingest with NO run
    /// decisions/findings, hence an EMPTY `graph_seeds` - a seed naming a real node still returns
    /// its neighborhood and the whole-graph overview still returns its clusters, instead of the
    /// `Graph::default()` dead-end the run-seeded read produced.
    #[test]
    fn dash_graph_provider_reaches_the_whole_projection_when_run_seeds_are_empty() {
        use std::collections::HashMap;

        let dir = tempfile::tempdir().unwrap();
        let graph_path = dir.path().join("graph.db");
        let graph_path = graph_path.to_str().unwrap();
        let identity = "reachtest";

        // A projection built from CODE INGEST alone (spec 29a: CodeEntityExtracted + EdgeInferred),
        // exactly what a cold-checkout `graph build` folds. No decision, no finding - so nothing a
        // run would seed a subgraph from.
        {
            let p = Projector::open(graph_path, identity).unwrap();
            let def = serde_json::json!({
                "file": "src/combat.rs", "name": "apply_damage",
                "kind": "function", "line": 7, "lang": "rust",
            });
            let mut e = Event::new(
                contextgraph::TYPE_CODE_ENTITY_EXTRACTED,
                serde_json::to_vec(&def).unwrap(),
            );
            e.position = 1;
            p.apply(&e).unwrap();
            let refr =
                serde_json::json!({ "file": "src/combat.rs", "name": "clamp", "lang": "rust" });
            let mut e2 = Event::new(
                contextgraph::TYPE_EDGE_INFERRED,
                serde_json::to_vec(&refr).unwrap(),
            );
            e2.position = 2;
            p.apply(&e2).unwrap();
        }

        // Precondition = the never-built dead-end. The run log carries no content events, so
        // `graph_seeds` is EMPTY and the OLD run-seeded read collapses to `Graph::default()`.
        let no_run_events: Vec<Event> = Vec::new();
        assert!(
            dash::graph_seeds(&no_run_events).is_empty(),
            "the never-built repo has no run seeds"
        );
        let run_seeded = dash_read_graph(graph_path, identity, &no_run_events);
        assert!(
            run_seeded.nodes.is_empty(),
            "the run-seeded read is the empty dead-end this criterion removes, got {run_seeded:?}"
        );

        // The fix: the graph provider reads the whole projection directly, so a real code node is
        // reachable with no run seeds at all.
        let whole = dash_read_whole_graph(graph_path, identity);
        assert!(
            whole
                .nodes
                .iter()
                .any(|n| n.id == "src/combat.rs::apply_damage"),
            "the whole-projection read reaches a code node with no run seeds, got {whole:?}"
        );

        // Seeded-neighborhood reach: `/api/graph?seed=<real node>` over the whole graph returns the
        // node's neighborhood - not the empty default.
        let seed = "src/combat.rs::apply_damage";
        let resp = dash::route(
            "GET",
            &format!("/api/graph?seed={seed}"),
            &no_run_events,
            &whole,
            &[],
            &HashMap::new(),
            3,
            "rigger-run",
            "main",
            &[],
        );
        assert_eq!(resp.status, 200);
        let body = String::from_utf8(resp.body).unwrap();
        let nb: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert!(
            nb["nodes"]
                .as_array()
                .unwrap()
                .iter()
                .any(|n| n["id"] == seed),
            "the seeded neighborhood over the whole projection contains the seed node, got {body}"
        );

        // Whole-graph overview reach: `/api/graph` (no seed) returns clusters over the whole graph,
        // never an empty default.
        let resp2 = dash::route(
            "GET",
            "/api/graph",
            &no_run_events,
            &whole,
            &[],
            &HashMap::new(),
            3,
            "rigger-run",
            "main",
            &[],
        );
        assert_eq!(resp2.status, 200);
        let body2 = String::from_utf8(resp2.body).unwrap();
        let ov: serde_json::Value = serde_json::from_str(&body2).unwrap();
        assert!(
            !ov["clusters"].as_array().unwrap().is_empty() && ov["total"].as_u64().unwrap() > 0,
            "the whole-graph overview returns clusters, not an empty default, got {body2}"
        );
    }

    /// Spec 45 GLOBAL CONSTRAINT (read-only provider, L33-34 / L72-73): the direct-projection
    /// provider is READ-ONLY, and "the dash still starts before the store exists; an absent graph
    /// degrades to an empty result, never an error". `dash_read_whole_graph` over an ABSENT graph db
    /// (the grep-only / never-built repo that has no `.rigger/graph.db`) must return an EMPTY graph
    /// and MUST NOT materialize the db.
    ///
    /// This pins the load-bearing `if !Path::new(graph_db).exists()` guard: `Projector::open` opens
    /// with the default `OPEN_READWRITE | OPEN_CREATE` and runs `execute_batch(SCHEMA)`, so WITHOUT
    /// the guard a read would spuriously CREATE the file+schema on a repo that never built one - a
    /// real, user-facing WRITE that breaks the read-only contract. The file-not-created assertion is
    /// the guard's teeth: delete the guard and this test reddens.
    #[test]
    fn dash_read_whole_graph_on_an_absent_db_is_empty_and_creates_nothing() {
        let dir = tempfile::tempdir().unwrap();
        // A path that does NOT exist: the never-built repo has no graph projection at all.
        let graph_path = dir.path().join("graph.db");
        let graph_db = graph_path.to_str().unwrap();
        let identity = "reachtest";
        assert!(
            !Path::new(graph_db).exists(),
            "precondition: the never-built repo has no graph db yet"
        );

        // (a) The read degrades to an EMPTY graph, never an error.
        let whole = dash_read_whole_graph(graph_db, identity);
        assert!(
            whole.nodes.is_empty() && whole.edges.is_empty(),
            "an absent projection reads as an empty graph, got {whole:?}"
        );

        // (b) The read is READ-ONLY: it must NOT have materialized the db. Removing the
        // `if !Path::new(graph_db).exists()` guard makes `Projector::open` CREATE + SCHEMA-write the
        // file here, reddening this assertion - these are the guard's teeth.
        assert!(
            !Path::new(graph_db).exists(),
            "a read over an absent projection must NOT create {graph_db} (read-only provider)"
        );

        // The composed provider closure the dash consults on /api/graph (main.rs, the
        // `move || dash_read_whole_graph(..)` provider) honors the same read-only contract over the
        // absent path.
        let provider = || -> contextgraph::Graph { dash_read_whole_graph(graph_db, identity) };
        let via_provider = provider();
        assert!(
            via_provider.nodes.is_empty() && via_provider.edges.is_empty(),
            "the composed provider over an absent projection is empty, got {via_provider:?}"
        );
        assert!(
            !Path::new(graph_db).exists(),
            "the composed provider must NOT create the db either (read-only provider)"
        );
    }

    /// `rigger setup` must provision the per-project JS driver: write the three
    /// embedded runtime files into `.rigger/shim/` with the embedded content. (The
    /// npm-install step is asserted separately so this test does not depend on npm.)
    #[test]
    fn setup_provisions_the_shim_runtime_files() {
        let dir = tempfile::tempdir().unwrap();
        let shim = write_shim_files(dir.path()).expect("provisioning writes the shim files");
        assert_eq!(shim, shim_dir(dir.path()));

        for (name, embedded) in SHIM_FILES {
            let path = shim.join(name);
            assert!(path.exists(), "{name} must be written into .rigger/shim/");
            let on_disk = std::fs::read_to_string(&path).unwrap();
            assert_eq!(
                &on_disk, embedded,
                "{name} on disk must be byte-identical to the embedded runtime"
            );
        }

        // The dev-only mock/test files must NOT ship - only the three runtime files.
        let names: Vec<String> = std::fs::read_dir(&shim)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert!(
            !names
                .iter()
                .any(|n| n.contains("mock") || n.contains(".test.")),
            "only runtime files ship; no mock-*/*.test.mjs. found: {names:?}"
        );

        // The embedded shim.mjs is the real driver (a sanity check it is not a stub).
        assert!(
            SHIM_MJS.contains("rigger") && SHIM_MJS.contains("query"),
            "the embedded shim.mjs must be the real JS driver"
        );
    }

    /// Criterion 4 (spec 05): `rigger setup` is re-runnable. `install_workflow` installs
    /// the native `/rigger` workflow at `.claude/workflows/rigger.js` byte-identical to
    /// the embedded `RIGGER_WORKFLOW`, DETECTS and REFRESHES a drifted copy (an older
    /// `rigger` build), and is a SILENT NO-OP - not even an mtime bump - when the
    /// installed workflow already matches. The npm-install step is exercised separately,
    /// so this test does not depend on npm.
    #[test]
    fn setup_installs_refreshes_and_is_a_noop_on_the_native_rigger_workflow() {
        let dir = tempfile::tempdir().unwrap();
        let path = workflow_path(dir.path());
        assert_eq!(
            path,
            dir.path()
                .join(".claude")
                .join("workflows")
                .join("rigger.js"),
            "the workflow must be installed at .claude/workflows/rigger.js"
        );

        // 1. Absent -> a fresh install, written byte-identical to the embedded copy.
        assert_eq!(
            install_workflow(dir.path()).expect("installing writes the workflow file"),
            InstallOutcome::Installed,
            "the first install reports a fresh install"
        );
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            RIGGER_WORKFLOW,
            "the installed workflow must be byte-identical to the embedded RIGGER_WORKFLOW"
        );

        // The embedded workflow is the real driver, not a stub: it exports `meta` and
        // drives agents via the workflow runtime.
        assert!(
            RIGGER_WORKFLOW.contains("export const meta") && RIGGER_WORKFLOW.contains("agent("),
            "the embedded workflow must be the real native /rigger workflow"
        );

        // 2. Already current -> a silent no-op that changes NOTHING, not even the file's
        //    mtime (the grounder's staleness gate keys off mtime). Sleep past the clock's
        //    resolution first so a stray rewrite WOULD move the mtime we assert is stable.
        let before = std::fs::metadata(&path).unwrap().modified().unwrap();
        std::thread::sleep(std::time::Duration::from_millis(20));
        assert_eq!(
            install_workflow(dir.path()).expect("a no-op rerun must succeed"),
            InstallOutcome::AlreadyCurrent,
            "an up-to-date workflow must be detected as current"
        );
        assert_eq!(
            std::fs::metadata(&path).unwrap().modified().unwrap(),
            before,
            "an up-to-date workflow must NOT be rewritten (its mtime must not move)"
        );

        // 3. Drifted (a stale copy from an older build) -> refreshed to the embedded copy.
        std::fs::write(&path, "// stale - from an older rigger build\n").unwrap();
        assert_eq!(
            install_workflow(dir.path()).expect("re-install must succeed"),
            InstallOutcome::Refreshed,
            "a drifted workflow must be refreshed"
        );
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            RIGGER_WORKFLOW,
            "refreshing must overwrite the drifted workflow with the embedded content"
        );
    }

    /// Find `name`'s outcome in an [`install_skills`] result, panicking if the registry
    /// somehow did not install it - a test-only convenience so each assertion below reads
    /// by skill name rather than by a brittle vec index.
    fn outcome_for<'a>(
        outcomes: &'a [(&'static str, InstallOutcome)],
        name: &str,
    ) -> &'a InstallOutcome {
        &outcomes
            .iter()
            .find(|e| e.0 == name)
            .unwrap_or_else(|| panic!("{name} missing from install_skills output"))
            .1
    }

    /// The `using-rigger` entry's [`rigger::docs::SkillEntry`], for tests that need to
    /// render it directly (registry order is not pinned, so callers look it up by name).
    fn registry_entry(name: &str) -> rigger::docs::SkillEntry {
        rigger::docs::skill_registry()
            .into_iter()
            .find(|e| e.name == name)
            .unwrap_or_else(|| panic!("{name} missing from the skill registry"))
    }

    /// Spec 20, unit 3; spec 68, criterion 1: `rigger setup` installs EVERY registry
    /// skill, each as a file DISTINCT from the `/rigger` workflow. `using-rigger` lands at
    /// `.claude/skills/using-rigger/SKILL.md` (a loadable skill Claude Code
    /// auto-discovers), which is not the workflow path, and it carries the rendered skill
    /// (loadable frontmatter) PLUS the operator-binary prohibition. Install is re-runnable
    /// exactly like the workflow: absent -> Installed, unchanged -> a silent no-op that
    /// does not even move the mtime, drifted -> Refreshed.
    #[test]
    fn setup_installs_the_using_rigger_skill_distinct_from_the_workflow() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let path = skill_install_path(root, "using-rigger");
        assert_eq!(
            path,
            root.join(".claude")
                .join("skills")
                .join("using-rigger")
                .join("SKILL.md"),
            "the skill installs at .claude/skills/using-rigger/SKILL.md"
        );
        assert_ne!(
            path,
            workflow_path(root),
            "the installed skill must be a file DISTINCT from the /rigger workflow"
        );

        // 1. Absent -> a fresh install carrying the rendered skill (loadable frontmatter),
        //    byte-identical to a fresh default render (no overlay in this repo).
        let outcomes = install_skills(root).expect("installing writes every skill file");
        assert_eq!(
            *outcome_for(&outcomes, "using-rigger"),
            InstallOutcome::Installed,
            "the first install reports a fresh install"
        );
        let installed = std::fs::read_to_string(&path).unwrap();
        assert!(
            installed.starts_with("---\nname: using-rigger\n"),
            "the installed skill must open with its loadable frontmatter; got: {}",
            &installed[..installed.len().min(60)]
        );
        assert_eq!(
            installed,
            registry_entry("using-rigger").render(&docs_context()),
            "with no overlay the installed skill is the default code-derived render"
        );
        assert!(
            installed.contains(rigger::docs::OPERATOR_BINARY_PROHIBITION),
            "the installed skill must carry the operator-binary prohibition"
        );

        // 2. Already current -> a silent no-op that does not move the mtime.
        let before = std::fs::metadata(&path).unwrap().modified().unwrap();
        std::thread::sleep(std::time::Duration::from_millis(20));
        let outcomes = install_skills(root).expect("a no-op rerun must succeed");
        assert_eq!(
            *outcome_for(&outcomes, "using-rigger"),
            InstallOutcome::AlreadyCurrent,
            "an up-to-date skill must be detected as current"
        );
        assert_eq!(
            std::fs::metadata(&path).unwrap().modified().unwrap(),
            before,
            "an up-to-date skill must NOT be rewritten (its mtime must not move)"
        );

        // 3. Drifted -> refreshed to the rendered skill.
        std::fs::write(&path, "stale hand-edit\n").unwrap();
        let outcomes = install_skills(root).expect("re-install must succeed");
        assert_eq!(
            *outcome_for(&outcomes, "using-rigger"),
            InstallOutcome::Refreshed,
            "a drifted skill must be refreshed"
        );
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            registry_entry("using-rigger").render(&docs_context()),
            "refreshing must overwrite the drift with the rendered skill"
        );
    }

    /// Spec 20, unit 3; spec 68, criterion 1 (overlay honored per entry): a project
    /// overlay adds this repo's specifics - the base branch and where specs live - into
    /// EVERY installed skill WITHOUT editing the shared discipline source.
    /// `.rigger/docs-overlay.yml` declares the two repo facts; they override the
    /// code-derived context BEFORE each render, so an installed skill that carries the
    /// fact (`using-rigger`) reflects it while the shared render still defaults for a repo
    /// with no overlay.
    #[test]
    fn setup_skill_install_applies_the_project_overlay() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::create_dir_all(root.join(RIGGER_DIR)).unwrap();
        std::fs::write(
            docs_overlay_path(root),
            "base_ref: work/trunk\nspecs_location: requirements/\n",
        )
        .unwrap();

        install_skills(root).expect("installing with an overlay must succeed");
        let installed = std::fs::read_to_string(skill_install_path(root, "using-rigger")).unwrap();

        // The repo specifics appear in the installed skill...
        assert!(
            installed.contains("work/trunk"),
            "the overlay base branch must flow into the installed skill"
        );
        assert!(
            installed.contains("requirements/"),
            "the overlay specs location must flow into the installed skill"
        );
        // ...and they REPLACE the shared defaults (the override is real, not additive).
        assert!(
            !installed.contains(DEFAULT_BASE_REF),
            "the overlay base branch must REPLACE the default base ref"
        );

        // The shared discipline source is untouched: docs_context() still yields the
        // defaults, and a repo with no overlay renders those defaults.
        assert_eq!(docs_context().base_ref, DEFAULT_BASE_REF);
        assert_eq!(docs_context().specs_location, DEFAULT_SPECS_LOCATION);
        assert!(
            rigger::docs::render_using_rigger_skill(&docs_context()).contains(DEFAULT_BASE_REF),
            "the shared render is unchanged; the overlay only overrode the install"
        );
    }

    /// The overlay overrides ONLY the fields it declares: a partial overlay (base_ref
    /// only) leaves specs_location at the shared default, so a repo customizes just the
    /// facts it differs on.
    #[test]
    fn docs_overlay_overrides_only_declared_fields() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::create_dir_all(root.join(RIGGER_DIR)).unwrap();
        std::fs::write(docs_overlay_path(root), "base_ref: only-base\n").unwrap();

        let mut ctx = docs_context();
        read_docs_overlay(root)
            .expect("a valid partial overlay reads")
            .apply(&mut ctx);
        assert_eq!(ctx.base_ref, "only-base", "declared field is overridden");
        assert_eq!(
            ctx.specs_location, DEFAULT_SPECS_LOCATION,
            "an undeclared field keeps the shared default"
        );

        // An absent overlay file yields no overrides (the common case, not an error).
        let empty = tempfile::tempdir().unwrap();
        let none = read_docs_overlay(empty.path()).expect("an absent overlay is not an error");
        let mut ctx2 = docs_context();
        none.apply(&mut ctx2);
        assert_eq!(
            ctx2,
            docs_context(),
            "no overlay leaves the context unchanged"
        );
    }

    /// A PRESENT but malformed overlay is a LOUD error naming the file, never a silent
    /// skip that would install a skill missing the repo specifics the author asked for.
    #[test]
    fn docs_overlay_malformed_is_a_loud_error() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::create_dir_all(root.join(RIGGER_DIR)).unwrap();
        std::fs::write(docs_overlay_path(root), "base_ref: [not, a, string]\n").unwrap();
        let err = read_docs_overlay(root).expect_err("a malformed overlay must fail loudly");
        assert!(
            err.to_string().contains("docs-overlay.yml"),
            "the error must name the overlay file; got: {err}"
        );
    }

    /// Criterion 4: provisioning the JS driver is a silent no-op when the shim is
    /// already current - the runtime files match the embedded copies and npm's install
    /// is COMPLETE (its `node_modules/.package-lock.json` marker present) - so a `rigger
    /// setup` rerun does not rewrite the files or re-run npm. Faking a complete
    /// `node_modules` lets this assert the short-circuit WITHOUT npm: were the
    /// short-circuit broken, `provision_shim` would run npm and return `true` (or error
    /// when npm is absent), both of which fail this test.
    #[test]
    fn provision_shim_is_a_silent_noop_when_already_current() {
        let dir = tempfile::tempdir().unwrap();
        let shim = write_shim_files(dir.path()).unwrap();
        assert!(!shim_is_current(&shim), "no node_modules yet: not current");

        // A COMPLETE npm install leaves node_modules/.package-lock.json as its final
        // marker; only then is the shim current.
        let node_modules = shim.join("node_modules");
        std::fs::create_dir_all(&node_modules).unwrap();
        std::fs::write(node_modules.join(".package-lock.json"), "{}").unwrap();
        assert!(
            shim_is_current(&shim),
            "matching runtime files + a COMPLETE node_modules (marker present): current"
        );

        let provisioned = provision_shim(dir.path())
            .expect("a fully-provisioned shim must be a clean no-op (no npm needed)");
        assert!(
            !provisioned,
            "provision_shim must report no work when the shim is already current"
        );

        // A drifted runtime file makes the shim not-current again (an upgrade path).
        std::fs::write(shim.join("shim.mjs"), "// stale shim from an older build\n").unwrap();
        assert!(
            !shim_is_current(&shim),
            "a drifted runtime file must make the shim not-current"
        );
    }

    /// Criterion 4: setup SELF-HEALS a torn/partial shim install. An interrupted `npm
    /// ci` (which `rm -rf`s `node_modules` then repopulates incrementally) leaves a
    /// `node_modules` DIRECTORY that lacks npm's completeness marker
    /// (`node_modules/.package-lock.json`). `shim_is_current` must treat that as NOT
    /// current so the next `rigger setup` re-runs npm and repairs it, rather than
    /// short-circuiting on bare directory presence and permanently refusing to fix a
    /// broken install. Regression-locks adv-u4-shim-torn-install-not-self-healed.
    #[test]
    fn shim_is_not_current_when_node_modules_is_torn_missing_the_install_marker() {
        let dir = tempfile::tempdir().unwrap();
        let shim = write_shim_files(dir.path()).unwrap();

        // A torn install: node_modules exists (some deps partially unpacked) but the
        // final .package-lock.json marker a COMPLETE install writes is absent.
        std::fs::create_dir_all(shim.join("node_modules").join("some-partial-dep")).unwrap();
        assert!(
            !shim_is_current(&shim),
            "a node_modules dir lacking the .package-lock.json completeness marker is a torn \
             install and must NOT be treated as current"
        );

        // Adding the marker (as a completed npm install would) makes it current again.
        std::fs::write(shim.join("node_modules").join(".package-lock.json"), "{}").unwrap();
        assert!(
            shim_is_current(&shim),
            "once the completeness marker is present the shim is current"
        );
    }

    /// Criterion 4: scaffolding is idempotent. The first `init_project` on an empty
    /// project changes the tree and reports the agents it wrote; a second run finds
    /// everything present and is a silent no-op (`changed: false`, no new agents), so
    /// `rigger setup` / `rigger init` re-run without side effects.
    #[test]
    fn init_project_is_idempotent_reporting_new_work_only_once() {
        let dir = tempfile::tempdir().unwrap();

        let first = init_project(dir.path()).expect("first init scaffolds the project");
        assert!(
            first.changed(),
            "the first init on an empty project must change the tree"
        );
        assert!(
            !first.new_agents.is_empty(),
            "the first init scaffolds the workflow's referenced agents"
        );

        let second = init_project(dir.path()).expect("a rerun must succeed");
        assert!(
            !second.changed(),
            "a rerun on an initialized project must change nothing"
        );
        assert!(
            second.new_agents.is_empty(),
            "a rerun scaffolds no new agents"
        );
    }

    /// Criterion 4 (spec 05): the setup/init summary is HONEST per artifact - it must
    /// never claim a scaffold action it did not perform. On a gitignore-only repair (the
    /// primary Gap-9 upgrade path: `workflow.yml`, the agents, and the hook are all
    /// already present, but a `.gitignore` entry was lost and gets re-appended) the
    /// summary reports ONLY the gitignore change and does NOT emit the false "scaffolded
    /// workflow.yml / agents / installed hook" line. Regression-locks
    /// adv-u4-coarse-changed-summary-lies.
    #[test]
    fn scaffold_summary_reports_only_the_gitignore_change_on_a_gitignore_only_repair() {
        let dir = tempfile::tempdir().unwrap();

        // First init scaffolds everything AND appends the machine-local .gitignore
        // entries (a non-git temp dir is untracked, so the entries are written).
        let first = init_project(dir.path()).expect("first init scaffolds the project");
        assert!(
            first.wrote_workflow && !first.new_agents.is_empty() && first.wrote_hook,
            "the first init writes workflow.yml, the agents, and the hook"
        );
        assert!(
            !first.gitignore_added.is_empty(),
            "the first init appends the machine-local .gitignore entries"
        );

        // Simulate the Gap-9 upgrade path: only `.gitignore` needs repair; every other
        // scaffold artifact is still present and byte-identical.
        std::fs::remove_file(dir.path().join(".gitignore")).unwrap();

        let repair = init_project(dir.path()).expect("a gitignore-only repair must succeed");
        assert!(
            !repair.wrote_workflow,
            "workflow.yml already exists; it must NOT be reported as scaffolded"
        );
        assert!(
            repair.new_agents.is_empty(),
            "the agents already exist; none are newly written"
        );
        assert!(
            !repair.wrote_hook,
            "the hook is already installed; it must NOT be reported as installed"
        );
        assert!(
            !repair.gitignore_added.is_empty(),
            "the lost .gitignore entries are re-appended - the ONE real change this run made"
        );

        // The summary must report the gitignore change and NOTHING it did not do.
        let lines = scaffold_summary_lines(&repair);
        assert_eq!(
            lines.len(),
            1,
            "a gitignore-only repair reports exactly one line, got: {lines:?}"
        );
        assert!(
            lines[0].contains(".gitignore"),
            "the one line must report the gitignore change, got: {:?}",
            lines[0]
        );
        assert!(
            !lines.iter().any(|l| {
                l.contains("workflow.yml")
                    || l.contains(".rigger/agents/")
                    || l.contains("SessionStart hook")
            }),
            "a gitignore-only repair must not claim it scaffolded the workflow, agents, or \
             hook: {lines:?}"
        );
    }

    /// Spec 46, criterion 1 (CONSUMER GITIGNORE): the always-on dash writes two runtime
    /// breadcrumbs under `.rigger/` - `.rigger/dash.url` and `.rigger/dash.marker`. Left
    /// untracked-and-not-ignored in a consumer's repo they get swept into a unit worktree's
    /// commit by `git add`, then collide with the live dash's rewrites when the conductor
    /// merges the unit (`git merge` aborts with "untracked working tree files would be
    /// overwritten"). So `rigger init`/`setup` must append an ignore line for BOTH, exactly
    /// as it does for the other machine-local installs, and the append must be idempotent -
    /// a second setup adds no duplicate line.
    #[test]
    fn init_project_gitignores_the_dash_runtime_breadcrumbs_idempotently() {
        let dir = tempfile::tempdir().unwrap();

        // First scaffold on a fresh consumer repo: both dash breadcrumbs are
        // untracked-and-not-ignored, so setup appends an ignore line for each and reports it.
        let first = init_project(dir.path()).expect("first init scaffolds the project");
        assert!(
            first
                .gitignore_added
                .contains(&".rigger/dash.url".to_string())
                && first
                    .gitignore_added
                    .contains(&".rigger/dash.marker".to_string())
                && first
                    .gitignore_added
                    .contains(&".rigger/dash.attempt".to_string()),
            "the first init reports appending ALL THREE dash-artifact ignore patterns (url, \
             marker, and the round-8 attempt breadcrumb), got: {:?}",
            first.gitignore_added
        );

        let gitignore = dir.path().join(".gitignore");
        let content = std::fs::read_to_string(&gitignore).unwrap();
        assert!(
            content.lines().any(|l| l.trim() == ".rigger/dash.url"),
            "the written .gitignore ignores the dash url breadcrumb, got:\n{content}"
        );
        assert!(
            content.lines().any(|l| l.trim() == ".rigger/dash.marker"),
            "the written .gitignore ignores the dash marker breadcrumb, got:\n{content}"
        );
        assert!(
            content.lines().any(|l| l.trim() == ".rigger/dash.attempt"),
            "the written .gitignore ignores the dash attempt breadcrumb (spec 69, round-8 fix; \
             the same collision-with-a-unit-commit risk as the other two dash breadcrumbs), \
             got:\n{content}"
        );

        // Idempotent: a second setup finds all three already ignored and appends nothing new.
        let second = init_project(dir.path()).expect("a rerun must succeed");
        assert!(
            !second
                .gitignore_added
                .contains(&".rigger/dash.url".to_string())
                && !second
                    .gitignore_added
                    .contains(&".rigger/dash.marker".to_string())
                && !second
                    .gitignore_added
                    .contains(&".rigger/dash.attempt".to_string()),
            "a rerun re-appends no dash-artifact ignore pattern, got: {:?}",
            second.gitignore_added
        );

        let after = std::fs::read_to_string(&gitignore).unwrap();
        assert_eq!(
            after
                .lines()
                .filter(|l| l.trim() == ".rigger/dash.url")
                .count(),
            1,
            "exactly one .rigger/dash.url ignore line - no duplicate accrued, got:\n{after}"
        );
        assert_eq!(
            after
                .lines()
                .filter(|l| l.trim() == ".rigger/dash.marker")
                .count(),
            1,
            "exactly one .rigger/dash.marker ignore line - no duplicate accrued, got:\n{after}"
        );
        assert_eq!(
            after
                .lines()
                .filter(|l| l.trim() == ".rigger/dash.attempt")
                .count(),
            1,
            "exactly one .rigger/dash.attempt ignore line - no duplicate accrued, got:\n{after}"
        );
    }

    /// Spec 48, SECRETS DISCIPLINE: the per-machine connection-string secret file
    /// `.rigger/store.conn` (store resolver rung 3) carries credentials, so `rigger init`/`setup`
    /// must git-ignore it BY CONSTRUCTION - the same scaffold mechanism that ignores the dash
    /// breadcrumbs - so a developer who drops their credentials into it can never commit them, and
    /// the committed project config never requires a secret. The append is idempotent: a second
    /// setup adds no duplicate line.
    #[test]
    fn init_project_gitignores_the_store_conn_secret_file_idempotently() {
        let dir = tempfile::tempdir().unwrap();

        // First scaffold on a fresh consumer repo: the secret file is untracked-and-not-ignored, so
        // setup appends an ignore line for it and reports it.
        let first = init_project(dir.path()).expect("first init scaffolds the project");
        assert!(
            first
                .gitignore_added
                .contains(&".rigger/store.conn".to_string()),
            "the first init reports appending the store.conn secret-file ignore pattern, got: {:?}",
            first.gitignore_added
        );

        let gitignore = dir.path().join(".gitignore");
        let content = std::fs::read_to_string(&gitignore).unwrap();
        assert!(
            content.lines().any(|l| l.trim() == ".rigger/store.conn"),
            "the written .gitignore ignores the store.conn secret file, got:\n{content}"
        );

        // Idempotent: a second setup finds it already ignored and appends nothing new.
        let second = init_project(dir.path()).expect("a rerun must succeed");
        assert!(
            !second
                .gitignore_added
                .contains(&".rigger/store.conn".to_string()),
            "a rerun re-appends no store.conn ignore pattern, got: {:?}",
            second.gitignore_added
        );

        let after = std::fs::read_to_string(&gitignore).unwrap();
        assert_eq!(
            after
                .lines()
                .filter(|l| l.trim() == ".rigger/store.conn")
                .count(),
            1,
            "exactly one .rigger/store.conn ignore line - no duplicate accrued, got:\n{after}"
        );
    }

    /// Spec 48, SECRETS DISCIPLINE (the permission-hygiene rung): the store-connection secret file
    /// carries a credential, so the resolver flags it when it is readable by users other than its
    /// owner. Owner-only modes (`0o600` and friends) are clean; any group-read or other-read bit
    /// exposes the secret and must be flagged. Pins the exact threshold so the nudge neither
    /// false-positives on a locked-down file nor misses an exposed one.
    #[cfg(unix)]
    #[test]
    fn a_group_or_other_readable_secret_file_mode_is_flagged_owner_only_is_not() {
        use super::conn_file_is_group_or_other_readable as exposed;
        // Owner-only: the credential is not exposed.
        assert!(!exposed(0o600), "0o600 (owner rw) is owner-only");
        assert!(!exposed(0o700), "0o700 (owner rwx) is owner-only");
        assert!(!exposed(0o400), "0o400 (owner read) is owner-only");
        // Any group-read or other-read bit exposes the credential.
        assert!(exposed(0o640), "0o640 grants group read");
        assert!(exposed(0o604), "0o604 grants other read");
        assert!(
            exposed(0o644),
            "0o644 (a default umask) grants group+other read"
        );
        assert!(exposed(0o444), "0o444 is world-readable");
    }

    /// Spec 46, criterion 1 (CONSUMER GITIGNORE), the broad-rule corner: even when a
    /// consumer's OWN committed `.gitignore` already covers both dash breadcrumbs through a
    /// broader rule (`.rigger/`), setup STILL appends the explicit `.rigger/dash.url` and
    /// `.rigger/dash.marker` lines. The committed `.gitignore` must be self-contained and
    /// portable, never contingent on any ignore resolution that could differ per machine, so
    /// the required lines are always present in the artifact shipped to a teammate/CI. The
    /// redundant-but-correct per-file line is harmless; the exact-line idempotency guard still
    /// prevents any duplicate. Proves setup does NOT let a broader ignore rule suppress the
    /// explicit dash lines (the regression a machine-local `git check-ignore` skip introduced).
    #[test]
    fn init_project_still_writes_the_dash_ignore_lines_when_a_broader_rule_covers_them() {
        let dir = tempfile::tempdir().unwrap();
        git_init_quiet(dir.path());
        // The consumer's own repo already ignores the entire runtime dir through a broad rule.
        std::fs::write(dir.path().join(".gitignore"), ".rigger/\n").unwrap();

        let report = init_project(dir.path()).expect("init must scaffold");
        assert!(
            report
                .gitignore_added
                .contains(&".rigger/dash.url".to_string())
                && report
                    .gitignore_added
                    .contains(&".rigger/dash.marker".to_string())
                && report
                    .gitignore_added
                    .contains(&".rigger/dash.attempt".to_string()),
            "setup appends the explicit dash lines (including the round-8 attempt breadcrumb) \
             even when .rigger/ broadly covers them, so the committed .gitignore stays \
             self-contained, got: {:?}",
            report.gitignore_added
        );

        let content = std::fs::read_to_string(dir.path().join(".gitignore")).unwrap();
        assert!(
            content.lines().any(|l| l.trim() == ".rigger/dash.url")
                && content.lines().any(|l| l.trim() == ".rigger/dash.marker")
                && content.lines().any(|l| l.trim() == ".rigger/dash.attempt"),
            "all three explicit per-file dash ignore lines are present in the committed \
             .gitignore even though .rigger/ already covers them, got:\n{content}"
        );

        // Idempotent: a rerun re-appends nothing (the exact lines are already present), so the
        // redundant-but-correct lines never accrue a duplicate.
        let second = init_project(dir.path()).expect("a rerun must succeed");
        assert!(
            !second
                .gitignore_added
                .contains(&".rigger/dash.url".to_string())
                && !second
                    .gitignore_added
                    .contains(&".rigger/dash.marker".to_string())
                && !second
                    .gitignore_added
                    .contains(&".rigger/dash.attempt".to_string()),
            "a rerun re-appends no dash line (exact-line idempotency), got: {:?}",
            second.gitignore_added
        );
    }

    /// Spec 08 item 2: the scaffold seed and the scaffold workflow reference the SAME
    /// canonical persona set - every seeded agent is referenced by the workflow and every
    /// referenced agent is seeded (no stray, unreferenced persona on a fresh-repo init) -
    /// and that set is the canonical six, with NONE of the four generic placeholder
    /// personas. A regression re-seeding a generic stray, or seeding an agent the workflow
    /// does not reference, fails here.
    #[test]
    fn scaffold_agents_and_workflow_reference_the_same_canonical_set() {
        use std::collections::BTreeSet;

        // Every agent id the scaffolded workflow references.
        let wf: config::Workflow =
            serde_yaml::from_str(SCAFFOLD_WORKFLOW).expect("the scaffolded workflow must parse");
        let mut referenced: BTreeSet<String> = wf.defaults.review.agent_ids().into_iter().collect();
        for stage in wf.stages.values() {
            referenced.extend(stage.agent_ids());
        }

        // Every agent id the scaffold seeds.
        let seeded: BTreeSet<String> = SCAFFOLD_AGENTS
            .iter()
            .map(|(_, c)| {
                config::parse_agent(c.as_bytes())
                    .expect("every seeded agent must parse")
                    .id
            })
            .collect();

        assert_eq!(
            seeded, referenced,
            "the seed and the scaffolded workflow must reference the same persona set: \
             seeded={seeded:?} referenced={referenced:?}"
        );

        let canonical: BTreeSet<String> = [
            "planner",
            "rust-engineer",
            "architecture-reviewer",
            "sdet",
            "adversary",
            "adjudicator",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        assert_eq!(
            seeded, canonical,
            "the seed is exactly the canonical persona set"
        );

        // The four generic placeholder personas are gone for good - not a filename, not an
        // id (the strays spec 05/08 removed and must never re-scaffold).
        for stray in [
            "implementer",
            "devils-advocate",
            "reviewer.architecture",
            "reviewer.technical",
        ] {
            assert!(
                !seeded.contains(stray),
                "the generic persona {stray:?} must not be seeded"
            );
            assert!(
                !SCAFFOLD_AGENTS
                    .iter()
                    .any(|(f, _)| *f == format!("{stray}.md")),
                "the generic file {stray}.md must not be seeded"
            );
        }
    }

    /// Spec 08 item 3: the referenced-agent scaffold-skip filter. `init_project` scaffolds
    /// ONLY the seeded agents the workflow references, and skips (never writes) a seeded
    /// agent the workflow does not reference. Driven with a workflow that references just
    /// two of the six seeded agents: exactly those two are written, the other four are not.
    #[test]
    fn init_scaffolds_only_the_workflow_referenced_agents() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let rigger = root.join(RIGGER_DIR);
        let agents = rigger.join("agents");
        std::fs::create_dir_all(&agents).unwrap();

        // A pre-existing workflow that references only `planner` and `adversary`.
        // `init_project` keeps it (write_if_absent) and scaffolds against ITS references.
        std::fs::write(
            rigger.join("workflow.yml"),
            "name: t\nstages:\n  plan:\n    agent: planner\n  go:\n    agent: adversary\n",
        )
        .unwrap();

        let report = init_project(root).expect("init must scaffold the referenced agents");

        assert!(
            agents.join("planner.md").exists(),
            "referenced planner seeded"
        );
        assert!(
            agents.join("adversary.md").exists(),
            "referenced adversary seeded"
        );
        for skipped in [
            "rust-engineer.md",
            "architecture-reviewer.md",
            "sdet.md",
            "adjudicator.md",
        ] {
            assert!(
                !agents.join(skipped).exists(),
                "an unreferenced seeded agent must NOT be scaffolded: {skipped}"
            );
        }
        let mut got = report.new_agents.clone();
        got.sort();
        assert_eq!(
            got,
            ["adversary.md", "planner.md"],
            "only the workflow-referenced agents are newly written"
        );
    }

    /// Spec 08 item 3: `get_referenced_agent_ids` - the source of truth the scaffold-skip
    /// filter reads - returns exactly the agent ids the workflow references, and an empty
    /// set when there is no workflow (the empty-repo signal `init_project` uses to seed the
    /// full default fleet).
    #[test]
    fn get_referenced_agent_ids_reads_the_scaffolded_workflows_fleet() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let rigger = root.join(RIGGER_DIR);
        std::fs::create_dir_all(&rigger).unwrap();
        std::fs::write(rigger.join("workflow.yml"), SCAFFOLD_WORKFLOW).unwrap();

        let ids = get_referenced_agent_ids(root).unwrap();
        let want: std::collections::HashSet<String> = [
            "planner",
            "rust-engineer",
            "architecture-reviewer",
            "sdet",
            "adversary",
            "adjudicator",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        assert_eq!(
            ids, want,
            "the referenced fleet is exactly the scaffolded canonical six"
        );

        let empty = tempfile::tempdir().unwrap();
        assert!(
            get_referenced_agent_ids(empty.path()).unwrap().is_empty(),
            "no workflow.yml yields an empty referenced set (the empty-repo seed signal)"
        );
    }

    /// Spec 08 item 4: a FAILED scaffold write is an error naming the artifact, never a
    /// swallowed `false` that drops the artifact from the summary while setup exits 0. An
    /// already-present file is a silent `Ok(false)` (kept), a fresh path is `Ok(true)`
    /// (wrote), and a genuine write failure is `Err` naming the path.
    #[test]
    fn write_if_absent_wrote_kept_and_errors_naming_the_artifact() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();

        // Fresh path -> wrote.
        let fresh = root.join("fresh.txt");
        assert!(
            write_if_absent(&fresh, "hi").unwrap(),
            "a fresh path is newly written"
        );
        assert_eq!(std::fs::read_to_string(&fresh).unwrap(), "hi");

        // Already present -> kept, silent, and left byte-for-byte untouched.
        assert!(
            !write_if_absent(&fresh, "OVERWRITE").unwrap(),
            "an existing file is kept, not rewritten"
        );
        assert_eq!(
            std::fs::read_to_string(&fresh).unwrap(),
            "hi",
            "keeping never touches the existing bytes"
        );

        // A genuine write failure (the parent directory does not exist) is an ERROR that
        // names the artifact - not a swallowed false.
        let unwritable = root.join("no-such-dir").join("agent.md");
        let err = write_if_absent(&unwritable, "x")
            .expect_err("a failed write must be an error, not a swallowed false");
        assert!(
            err.to_string().contains("agent.md"),
            "the error must name the artifact it could not write; got: {err}"
        );
    }

    // ---- `rigger setup --agents <dir>`: importing a starting fleet from a local dir ----

    /// `rigger setup` takes only the `--agents <dir>` flag; a bare setup parses to no
    /// import, `--agents <dir>` captures the source directory, a missing value errors,
    /// and an unknown flag errors (never a silent skip).
    #[test]
    fn parse_setup_args_reads_the_agents_directory_flag() {
        assert!(parse_setup_args(&[]).unwrap().agents_dir.is_none());

        let opts = parse_setup_args(&["--agents".into(), "/some/collection".into()]).unwrap();
        assert_eq!(
            opts.agents_dir.as_deref(),
            Some(Path::new("/some/collection"))
        );

        assert!(
            parse_setup_args(&["--agents".into()]).is_err(),
            "--agents with no directory must be a clear error"
        );
        assert!(
            parse_setup_args(&["--bogus".into()]).is_err(),
            "an unknown setup flag must be a clear error"
        );
    }

    /// `import_agents` copies each `.md` from a local collection directory into
    /// `.rigger/agents/`, normalizing the collection's identity field (`name:`) to
    /// Rigger's `id:` so a foreign agent loads under Rigger's schema. The imported file
    /// parses via the same `config::parse_agent` the loader uses.
    #[test]
    fn import_agents_copies_and_normalizes_the_identity_field() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        // A valid project to validate against (workflow + the default fleet).
        init_project(root).unwrap();

        // A foreign collection whose agents use `name:` as their identity field (the
        // Claude Code / agency-agents shape), plus an extra unknown frontmatter key.
        let src = root.join("collection");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(
            src.join("researcher.md"),
            "---\nname: researcher\ndescription: digs up prior art\nmodel: sonnet\n---\n\
             You research prior art and cite sources.\n",
        )
        .unwrap();
        // A non-.md file must be ignored.
        std::fs::write(src.join("README.txt"), "not an agent").unwrap();

        let summary = import_agents(root, &src).unwrap();
        assert_eq!(
            summary,
            ImportSummary {
                imported: 1,
                skipped: 0
            }
        );

        let imported = std::fs::read_to_string(root.join(".rigger/agents/researcher.md")).unwrap();
        assert!(
            imported.contains("id: researcher"),
            "the identity field must be normalized to `id:`; got:\n{imported}"
        );
        assert!(
            !imported.contains("name: researcher"),
            "the original `name:` identity key must be renamed, not left in place"
        );
        // The extra frontmatter and the prompt body survive the normalization untouched.
        assert!(imported.contains("description: digs up prior art"));
        assert!(imported.contains("You research prior art and cite sources."));

        // It parses under Rigger's schema with the normalized id.
        let a = config::parse_agent(imported.as_bytes()).unwrap();
        assert_eq!(a.id, "researcher");
        assert_eq!(a.model, "sonnet");
    }

    /// Import never overwrites an existing agent file: a collection file whose name
    /// collides with one already in `.rigger/agents/` is kept as-is and counted as
    /// skipped, so a re-run (or importing over the scaffolded fleet) is safe.
    #[test]
    fn import_agents_refuses_to_overwrite_an_existing_agent() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        init_project(root).unwrap();

        // `planner.md` already exists (scaffolded by init_project). Capture it.
        let existing_path = root.join(".rigger/agents/planner.md");
        let original = std::fs::read_to_string(&existing_path).unwrap();

        let src = root.join("collection");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(
            src.join("planner.md"),
            "---\nname: planner\n---\nA DIFFERENT planner that must not clobber the local one.\n",
        )
        .unwrap();
        std::fs::write(
            src.join("newcomer.md"),
            "---\nid: newcomer\n---\nBrand new agent.\n",
        )
        .unwrap();

        let summary = import_agents(root, &src).unwrap();
        assert_eq!(
            summary,
            ImportSummary {
                imported: 1,
                skipped: 1
            },
            "the colliding planner.md is skipped; only newcomer.md is imported"
        );
        assert_eq!(
            std::fs::read_to_string(&existing_path).unwrap(),
            original,
            "the pre-existing agent file must be left byte-for-byte untouched"
        );
        assert!(root.join(".rigger/agents/newcomer.md").exists());
    }

    /// Import runs the same validation `rigger validate` applies: a malformed agent
    /// file (no frontmatter) fails the import loudly instead of writing a file that
    /// would later break `config::load`.
    #[test]
    fn import_agents_validates_and_rejects_a_malformed_agent() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        init_project(root).unwrap();

        let src = root.join("collection");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(src.join("broken.md"), "no frontmatter here, just prose\n").unwrap();

        assert!(
            import_agents(root, &src).is_err(),
            "an agent file with no YAML frontmatter must fail the import validation"
        );
    }

    /// Import is atomic on an id collision with an agent already on disk. A collection
    /// file whose normalized id equals a scaffolded agent's - under a DIFFERENT filename,
    /// so the filename-only overwrite guard does not catch it - is rejected BEFORE any
    /// write, leaving `.rigger/agents/` untouched. Without this, the file is written and
    /// the trailing whole-fleet load then fails on the duplicate id, bricking every later
    /// `config::load`.
    #[test]
    fn import_agents_rejects_an_id_colliding_with_an_existing_agent() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        init_project(root).unwrap();

        let src = root.join("collection");
        std::fs::create_dir_all(&src).unwrap();
        // A different filename, but its id collides with the scaffolded `planner`.
        std::fs::write(
            src.join("my-planner.md"),
            "---\nid: planner\n---\nA colliding planner under a new filename.\n",
        )
        .unwrap();

        assert!(
            import_agents(root, &src).is_err(),
            "an imported id that collides with an existing agent must fail the import"
        );
        assert!(
            !root.join(".rigger/agents/my-planner.md").exists(),
            "the colliding file must NOT be written - the import aborts atomically"
        );
    }

    /// Import is atomic on a duplicate id WITHIN one import: two collection files that
    /// normalize to the same id are rejected before either is written, so no half-import
    /// is left behind.
    #[test]
    fn import_agents_rejects_a_duplicate_id_within_one_import() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        init_project(root).unwrap();

        let src = root.join("collection");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(src.join("a-dup.md"), "---\nid: twin\n---\nFirst.\n").unwrap();
        // `name:` normalizes to the same `id: twin`.
        std::fs::write(src.join("b-dup.md"), "---\nname: twin\n---\nSecond.\n").unwrap();

        assert!(
            import_agents(root, &src).is_err(),
            "two imported files sharing an id must fail the import"
        );
        assert!(
            !root.join(".rigger/agents/a-dup.md").exists()
                && !root.join(".rigger/agents/b-dup.md").exists(),
            "neither file may be written when the batch has a duplicate id"
        );
    }

    /// Import rejects an agent whose identity field is present but blank - the empty-id
    /// arm - by the SAME rule `config::load` applies, and writes nothing. A `name:` with
    /// an empty value normalizes to a blank `id:`.
    #[test]
    fn import_agents_rejects_an_agent_with_a_blank_id() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        init_project(root).unwrap();

        let src = root.join("collection");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(
            src.join("blank.md"),
            "---\nname: \"\"\ndescription: has a blank identity\n---\nBody.\n",
        )
        .unwrap();

        assert!(
            import_agents(root, &src).is_err(),
            "a blank id must fail the import (the same rule config::load enforces)"
        );
        assert!(
            !root.join(".rigger/agents/blank.md").exists(),
            "the blank-id file must NOT be written - the import aborts before writing"
        );
    }

    /// Import runs the SAME whole-project validation `rigger validate` applies: a project
    /// whose workflow references a missing agent fails the import even when the imported
    /// file itself is well-formed. This drives the trailing `config::load` referential
    /// check.
    #[test]
    fn import_agents_runs_full_validation_and_rejects_a_broken_project() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        init_project(root).unwrap();
        // Break a workflow agent reference so the whole-project load fails referentially.
        let wf_path = root.join(".rigger/workflow.yml");
        let wf = std::fs::read_to_string(&wf_path).unwrap();
        std::fs::write(&wf_path, wf.replace("agent: rust-engineer", "agent: ghost")).unwrap();

        let src = root.join("collection");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(
            src.join("newcomer.md"),
            "---\nid: newcomer\n---\nA well-formed new agent.\n",
        )
        .unwrap();

        assert!(
            import_agents(root, &src).is_err(),
            "import must run the same validation `rigger validate` applies and reject a \
             project whose workflow references a missing agent"
        );
    }

    /// Extract the literal body of the `export const meta = { ... }` object from the
    /// embedded workflow: from `export const meta` to the matching top-level `}`. Used to
    /// assert the meta object stays a PURE LITERAL (the Workflow runtime extracts it
    /// statically, so it cannot contain computed values or interpolation).
    fn meta_object_body(src: &str) -> &str {
        let start = src
            .find("export const meta")
            .expect("workflow must export const meta");
        let open = start + src[start..].find('{').expect("meta must open a brace");
        let mut depth = 0usize;
        for (i, c) in src[open..].char_indices() {
            match c {
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        return &src[open..=open + i];
                    }
                }
                _ => {}
            }
        }
        panic!("meta object literal is not brace-balanced");
    }

    /// Extract the STRING VALUE of `meta.description` from the workflow source: the
    /// single-quoted literal that follows the `description:` key inside the meta object.
    /// `meta.description` is the tagline the skills list and the `/workflows` header show,
    /// so a test can assert it reads as user-facing prose free of the driver's internal
    /// plumbing terms. The description literal is single-quoted and contains no apostrophe,
    /// so the first `'...'` pair after the key delimits it exactly (a test-only heuristic,
    /// not a JS parser).
    fn meta_description(src: &str) -> &str {
        let meta = meta_object_body(src);
        let key = meta
            .find("description:")
            .expect("meta must declare a description");
        let after = &meta[key + "description:".len()..];
        let open = after
            .find('\'')
            .expect("meta.description must be a single-quoted string literal");
        let rest = &after[open + 1..];
        let close = rest
            .find('\'')
            .expect("meta.description string literal must be closed");
        &rest[..close]
    }

    /// Strip `//` line comments from JS source so assertions about the executable code
    /// (e.g. "the global `phase('Build')` marker is gone") are not tripped by prose that
    /// documents the removed construct. Only whole-line comments and end-of-line comments
    /// are stripped; this is a test-only heuristic, not a JS parser, and the workflow's
    /// comments never contain `//` inside a string literal on the same line.
    fn strip_line_comments(src: &str) -> String {
        src.lines()
            .map(|line| match line.find("//") {
                Some(i) => &line[..i],
                None => line,
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// The native `/rigger` workflow is a THIN driver over the Rust conductor: it couriers
    /// each frontier via `rigger step`, spawns the returned wave natively in parallel with a
    /// per-unit `opts.phase` label built from the wave item, lets each worker self-report via
    /// `rigger result`, records a dead worker's failure on its behalf via `rigger result
    /// --if-absent --error`, and loops until the step reports `done`. Because `meta` MUST be a pure literal
    /// (statically extracted by the Workflow runtime - no computed values / no interpolation)
    /// and unit ids are only known at runtime, the per-unit labels live in the runtime
    /// `opts.phase` strings while `meta.phases` keeps the fixed stage set. This test pins the
    /// thin-driver contract so a future edit cannot silently regress it; it supersedes the
    /// fat-workflow `buildUnit`/`PH` structure this workflow replaced.
    #[test]
    fn workflow_is_a_thin_courier_driver_with_per_unit_phase_labels() {
        let wf = RIGGER_WORKFLOW;
        // Code assertions run against comment-stripped source so the workflow's own prose
        // (which documents the removed fat-workflow constructs) cannot trip them; the meta
        // assertions run against the raw literal object body.
        let code = strip_line_comments(wf);

        // 1. meta.phases keeps the FIXED stage set as a pure up-front literal.
        let meta = meta_object_body(wf);
        for stage in ["Plan", "Build", "Review", "Integrate"] {
            assert!(
                meta.contains(&format!("title: '{stage}'")),
                "meta.phases must declare the fixed stage '{stage}'"
            );
        }

        // 2. meta stays a PURE LITERAL: no interpolation / computed values anywhere in the
        //    object body, so the runtime can statically extract it before the body runs.
        //    Runtime per-unit ids must never leak into meta.
        assert!(
            !meta.contains("${"),
            "meta must be a pure literal - no `${{...}}` interpolation or computed values \
             (found interpolation inside the meta object body): {meta}"
        );

        // 3. The driver COURIERS the wave via `rigger step` - it does not decompose or
        //    orchestrate the DAG itself (that lives in the conductor behind the step) - and
        //    loops on the `{wave, done}` shape the step prints.
        assert!(
            code.contains("rigger step"),
            "the thin driver must fetch each wave by having a courier run `rigger step`"
        );
        assert!(
            code.contains("step.wave") && code.contains("step.done"),
            "the driver must read the wave and loop until the step reports done"
        );

        // 4. It SPAWNS the wave natively in parallel, one agent per wave item.
        assert!(
            code.contains("parallel(") && code.contains("wave.map("),
            "the driver must spawn the wave's agents natively in parallel"
        );

        // 5. Per-unit progress groups are produced at runtime from the WAVE ITEM (unit +
        //    stage), per the spawn::SpawnRequest contract, and every worker is labelled with it.
        assert!(
            code.contains("function phaseOf(req)") && code.contains("`${req.unit}:${req.stage}`"),
            "the driver must build each worker's opts.phase label from the wave item's unit + stage"
        );
        assert!(
            code.contains("phase: ph"),
            "each spawned worker must label its progress group with the per-unit phase"
        );
        // No bare global lifecycle phase markers: Build/Review/Integrate are per-unit (inside
        // the conductor) now, so a global marker would re-imply a false "all units build, then
        // all review" order.
        for stage in ["Build", "Review", "Integrate"] {
            assert!(
                !code.contains(&format!("phase('{stage}')")),
                "the global phase('{stage}') marker must not exist - {stage} is per-unit now"
            );
            assert!(
                !code.contains(&format!("phase: '{stage}'")),
                "no agent may use the bare global `phase: '{stage}'` opts - that would collapse \
                 every unit into one global progress group"
            );
        }
        // Only Plan remains a genuine global phase marker (the orchestration/courier pass).
        assert!(
            code.contains("phase('Plan')"),
            "the single global Plan pass must keep its phase('Plan') marker"
        );

        // 6. Workers SELF-REPORT via `rigger result <id>`, and a worker that DIES without
        //    reporting has its failure recorded on its behalf via `rigger result <id>
        //    --if-absent --error` from the `agent()`-rejected (catch) branch.
        assert!(
            code.contains("rigger result ${req.id}"),
            "each worker must be told to self-report its result via `rigger result <id>`"
        );
        assert!(
            code.contains("catch") && code.contains("report-death:"),
            "a worker that dies (its agent() rejects) must be caught and its failure couriered"
        );

        // 6a. The death courier records the failure ATOMICALLY and CONDITIONALLY via a single
        //     `rigger result <id> --if-absent --error <why>`: the `--error` lands ONLY when the
        //     spawn has no result yet, and an existing result (a worker that self-reported
        //     success/approve and THEN ran to max-turns) is left untouched. It replaces the old
        //     two-process `rigger reported <id> || rigger result <id> --error` guard, whose
        //     read-then-write gap could clobber a self-report landing between the check and the
        //     record (`rigger result` / `spawn::result_of` are last-write-wins), force-failing an
        //     approved unit on replay. One atomic op closes that TOCTOU window - the primary
        //     correctness invariant the review rejected the unguarded version for.
        assert!(
            code.contains("rigger result ${req.id} --if-absent --error"),
            "the death courier must record atomically via `rigger result <id> --if-absent --error` \
             so a self-reported result is never clobbered"
        );
        assert!(
            !code.contains("rigger reported ${req.id} ||"),
            "the death courier must no longer use the two-process `rigger reported <id> || ...` \
             check-then-record guard (the atomic `--if-absent` record supersedes it)"
        );

        // 6b. Both courier `agent()` calls (the death-report courier AND the top-level `rigger
        //     step` courier) are wrapped so a courier that itself dies is a clean, loud stop
        //     rather than an uncaught rejection that aborts the driver (or, for the death
        //     courier, an abort that also leaves the spawn unreported and hangs the run). The
        //     death courier's own failure is captured in the shared `fatal` sink, not re-thrown.
        assert!(
            code.contains("fatal.push("),
            "a death-report courier that itself fails must be captured (in `fatal`), not swallowed \
             or allowed to abort parallel() mid-wave"
        );
        assert!(
            code.contains("courier agent itself failed"),
            "the top-level `rigger step` courier agent() must be wrapped so its own death is a \
             clean, loud stop, not an uncaught abort of the whole driver"
        );

        // 6c. Every anomalous (non-fixpoint) exit stops LOUDLY: `stop()` throws so a hung/failed
        //     run surfaces as a workflow failure instead of resolving as a clean completion.
        assert!(
            code.contains("function stop(") && code.contains("throw new Error"),
            "anomalous exits must stop loudly via a throwing `stop()`, never a silent success return"
        );

        // 6d. A spawn-budget HALT (Gap 13) is a LOUD stop, never a clean completion: `rigger
        //     step` reports a `halted` reason distinct from `done` convergence, and the driver
        //     routes a halted step through the throwing `stop()` (so a starved run surfaces as a
        //     workflow failure instead of the `done` fixpoint reading it as success). The STEP
        //     schema must also ADMIT the optional `halted` field - the top level rejects unknown
        //     properties, so a halted step's JSON would otherwise fail validation and be lost.
        assert!(
            code.contains("step.halted"),
            "the driver must inspect `step.halted` and stop loudly on a budget halt \
             (a halted run is never a clean completion)"
        );
        assert!(
            code.contains("halted: { type: 'string' }"),
            "the STEP schema must declare the optional `halted` field (top-level \
             additionalProperties is false, so an undeclared `halted` would be rejected)"
        );

        // 6e. A WEDGED terminus (spec 19c, unit 1) is a LOUD stop, never a clean completion:
        //     `rigger step` carries the set of escalated units, and the driver's `done` branch
        //     routes a fixpoint reached with any of them through the throwing `stop()` (so a
        //     unit that can never pass review does not masquerade as success). The STEP schema
        //     must also ADMIT the `escalated` array - the top level rejects unknown properties,
        //     so an undeclared `escalated` would fail validation and the wedge would be lost.
        assert!(
            code.contains("step.escalated"),
            "the driver must inspect `step.escalated` and stop loudly on a fixpoint reached \
             with an escalated unit (a wedged terminus is never a clean completion)"
        );
        assert!(
            code.contains("escalated: { type: 'array', items: { type: 'string' } }"),
            "the STEP schema must declare the `escalated` array (top-level \
             additionalProperties is false, so an undeclared `escalated` would be rejected)"
        );
        // The loud-stop guarantee IS the ordering: the wedge `stop()` must run BEFORE the
        // `done` fixpoint breaks the loop, or an escalated terminus would break as a clean
        // completion (the exact regression a reorder would silently reintroduce). Pin it: the
        // wedge stop's reason precedes the "run complete" break in source. Presence alone
        // (checked above) does not guarantee the position that makes the stop reachable.
        let wedge_stop = code
            .find("escalated after exhausting remediation")
            .expect("the driver must stop loudly on an escalated fixpoint, naming the units");
        let run_complete = code
            .find("run complete: the conductor reached a fixpoint")
            .expect("the driver must log a clean completion at a non-wedged fixpoint");
        assert!(
            wedge_stop < run_complete,
            "the escalated-fixpoint `stop()` must precede the `done` completion break, or a \
             wedged terminus would resolve as a clean `run complete` before the wedge is checked"
        );

        // 7. The workflow still parses: run `node --check` when node is on PATH (never a
        //    silent skip - assert the clear reason when it is not available).
        let node = std::env::var("RIGGER_NODE").unwrap_or_else(|_| "node".to_string());
        let mut f = tempfile::NamedTempFile::new().unwrap();
        std::io::Write::write_all(&mut f, wf.as_bytes()).unwrap();
        match Command::new(&node).arg("--check").arg(f.path()).output() {
            Ok(out) => assert!(
                out.status.success(),
                "node --check must pass on the embedded workflow:\n{}",
                String::from_utf8_lossy(&out.stderr)
            ),
            Err(e) => assert!(
                e.kind() == std::io::ErrorKind::NotFound,
                "node --check failed for a reason other than node being absent: {e}"
            ),
        }
    }

    /// Spec 69, criterion 5 (this unit OWNS the wire stamp): the STEP schema must ADMIT the
    /// `attention` array - the top level rejects unknown properties (`additionalProperties:
    /// false`), so a step carrying a non-empty `attention` would otherwise fail validation
    /// and the signal would be lost, exactly the `halted`/`escalated` precedent this
    /// mirrors. This unit stamps the wire ONLY; rendering an entry as a narrator log line
    /// is a later criterion's job, so this test asserts schema admission alone.
    #[test]
    fn the_step_schema_admits_the_attention_array() {
        assert!(
            RIGGER_WORKFLOW.contains("attention: {"),
            "the STEP schema must declare the optional `attention` array (top-level \
             additionalProperties is false, so an undeclared `attention` would be rejected \
             and a flagged step's signal would be lost)"
        );
        assert!(
            RIGGER_WORKFLOW.contains("kind: { type: 'string' }"),
            "the `attention` item schema must admit `kind` (the signal name)"
        );
    }

    /// Extract a top-level `function <name>(...) { ... }` body (from the opening brace after
    /// `signature` to its matching closing brace) from JS source. The same brace-counting as
    /// [`meta_object_body`], generalized to a named function so a test can pin what that
    /// function's body does (or does not) contain, rather than the whole embedded file.
    fn js_function_body<'a>(src: &'a str, signature: &str) -> &'a str {
        let start = src
            .find(signature)
            .unwrap_or_else(|| panic!("workflow must define `{signature}`"));
        let open = start
            + src[start..]
                .find('{')
                .expect("function signature must open a brace");
        let mut depth = 0usize;
        for (i, c) in src[open..].char_indices() {
            match c {
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        return &src[open..=open + i];
                    }
                }
                _ => {}
            }
        }
        panic!("`{signature}` body is not brace-balanced");
    }

    /// Spec 69, criterion 6 ("the driver relays it" - THIS unit OWNS the relay; the wire
    /// stamp is criterion 5's, pinned above by `the_step_schema_admits_the_attention_array`).
    /// Each `attention` entry the wire carries must render as ONE narrator `log()` line naming
    /// the event (`kind`), the unit, and a response - mirroring `src/watch.rs::Signal::
    /// response`'s convention for the pull-side watchdog (decision
    /// d-u69c6-attention-response-mapping): `escalated` and `halted` resolve to the two
    /// existing spec-68 skills the Design's own Notes point at by name ("resume and escalation
    /// response protocols are spec 68's skills, referenced by name"), `worker-death-recurred`
    /// to the churn skill, `budget-final-tenth` to the resume skill (a preemptive warning for
    /// the same halt), and `stalled-frontier` names the Design's own literal directive instead
    /// of inventing a sixth skill - exactly as `Signal::FrontierStall` does on the pull side.
    /// This is a RENDER-ONLY relay (spec 69: "log lines only, no new stops, no retry-rule
    /// changes"), so the function must never call `stop(`; an entry-less step must render
    /// nothing, which iterating the wire's own array (never a second anomaly list) guarantees
    /// structurally. The relay must fire "at the wave it arrived" - before that wave's own
    /// agents are spawned, not after.
    #[test]
    fn the_driver_relays_each_attention_entry_as_a_narrator_log_line() {
        let code = strip_line_comments(RIGGER_WORKFLOW);

        // The relay is a named function, both DEFINED and actually CALLED (not merely
        // declared and dead).
        assert!(
            code.contains("function relayAttention(step)"),
            "the driver must define a relayAttention(step) function that renders the wire's \
             attention array"
        );
        assert_eq!(
            code.matches("relayAttention(step)").count(),
            2,
            "relayAttention(step) must appear exactly twice: its own definition signature and \
             one call site that actually invokes it"
        );

        let body = js_function_body(&code, "function relayAttention(step) {");

        // Renders ONLY what the wire says: iterates the wire's own `attention` array (omitted
        // entirely on a clean step - criterion 5's `skip_serializing_if`), never a second,
        // independently-maintained anomaly list; an entry-less step's loop body never runs.
        assert!(
            body.contains("step.attention || []"),
            "relayAttention must iterate step.attention (guarded with || [] against the \
             omitted-on-a-clean-step shape), so an entry-less step renders nothing"
        );

        // Each entry names its event (kind) and detail in one log() line.
        assert!(
            body.contains("log(") && body.contains("a.kind") && body.contains("a.detail"),
            "each attention entry must render as a log() line naming its kind and detail"
        );

        // The five wire kinds (ledger::ATTENTION_*, the closed vocabulary criterion 5 stamps)
        // each resolve to a response - pinned against the SAME string constants the wire stamp
        // uses, so a renamed kind breaks this test rather than silently going unmapped.
        for (kind, response) in [
            (ledger::ATTENTION_ESCALATED, "rigger-handle-an-escalation"),
            (ledger::ATTENTION_HALTED, "rigger-resume-a-run"),
            (
                ledger::ATTENTION_WORKER_DEATH_RECURRED,
                "rigger-diagnose-churn",
            ),
            (ledger::ATTENTION_BUDGET_FINAL_TENTH, "rigger-resume-a-run"),
            (
                ledger::ATTENTION_STALLED_FRONTIER,
                "stop the driver and diagnose before another round spends",
            ),
        ] {
            assert!(
                code.contains(&format!("'{kind}': '{response}'"))
                    || code.contains(&format!("{kind}: '{response}'")),
                "the driver must map wire kind '{kind}' to response '{response}'"
            );
        }

        // Render-only: never a new stop path (spec 69: "log lines only, no new stops").
        assert!(
            !body.contains("stop("),
            "the attention relay must never call stop() - it is a render-only narration; the \
             wire stamp already decided what happened"
        );

        // "At the wave it arrived": the relay call must precede the wave-spawn CONDITIONAL
        // itself (`if (wave.length > 0)`), not merely its inner spawn-narration text - a
        // weaker check anchored on the log() line alone stays green even if a future edit
        // nests the call inside that block (review u69c6 round 1, cause genuine-defect:
        // moving the shipped, unconditional call to the block's first line left every
        // periphery test and this test green, because the call still textually preceded the
        // log() line while now running only when wave.length > 0). Anchoring on the `if`
        // line itself catches that exact nesting: a step whose wave is empty - the
        // escalated/halted/stalled-frontier "nothing left to spawn" case an unattended
        // operator most needs the narrator line for - must still get its attention relayed.
        let call_pos = code
            .rfind("relayAttention(step)")
            .expect("relayAttention(step) must be called");
        let wave_conditional_pos = code
            .find("if (wave.length > 0)")
            .expect("the driver must gate wave-spawning on a non-empty wave");
        assert!(
            call_pos < wave_conditional_pos,
            "attention must be relayed for the step BEFORE the wave-spawn conditional \
             (\"at the wave it arrived\"), not nested inside it - a step with an empty wave \
             (escalated/halted/stalled-frontier) must still get its attention relayed"
        );
    }

    /// Spec 69, criterion 5, signal 2's hung-liveness half (review u69c5 round 3, cause
    /// genuine-defect): `merge_hung_attention` must not fire when there is nothing newly
    /// hung, proving the crossing gate, not just the merge mechanics, since a wrong-way bug
    /// here would restamp on every call exactly like the defect this round fixes.
    #[test]
    fn merge_hung_attention_does_nothing_when_not_newly_hung() {
        let attention = vec![ledger::AttentionEntry::unit_scoped(
            ledger::ATTENTION_ESCALATED,
            "u",
            "escalated after exhausting remediation",
        )];
        let merged = merge_hung_attention(attention.clone(), false, || {
            panic!("the reason closure must not run when nothing is newly hung")
        });
        assert_eq!(
            merged, attention,
            "attention must be untouched when newly_hung is false"
        );
    }

    /// A budget halt this same call takes precedence over a co-occurring hung-liveness halt
    /// (mirroring the SAME precedence the `halted` wire field already gives the budget
    /// breaker over its own hung fallback, just above this function's call site in
    /// `cmd_step`) - proving the merge does NOT stamp a second `halted` entry, and does not
    /// evaluate the (potentially expensive) reason closure, when one is already present.
    #[test]
    fn merge_hung_attention_defers_to_an_existing_budget_halt() {
        let attention = vec![ledger::AttentionEntry::run_scoped(
            ledger::ATTENTION_HALTED,
            "budget exhausted: 1/1 spawns",
        )];
        let merged = merge_hung_attention(attention.clone(), true, || {
            panic!("the reason closure must not run when a halted entry already exists")
        });
        assert_eq!(
            merged, attention,
            "a budget halt already on the channel must not be joined by a second halted entry"
        );
    }

    /// The merge must land the hung-liveness `halted` entry in its CANONICAL position
    /// (escalated, halted, worker-death-recurred, budget-final-tenth, stalled-frontier) even
    /// when `compute_attention` already produced entries both BEFORE and AFTER that slot in
    /// the SAME step (a different unit independently escalating, and a third stalling) - a
    /// naive push-to-the-end would leave `halted` stuck last, violating the wire's
    /// documented deterministic order.
    #[test]
    fn merge_hung_attention_lands_in_canonical_position_alongside_other_signals() {
        let attention = vec![
            ledger::AttentionEntry::unit_scoped(
                ledger::ATTENTION_ESCALATED,
                "e",
                "escalated after exhausting remediation",
            ),
            ledger::AttentionEntry::unit_scoped(
                ledger::ATTENTION_STALLED_FRONTIER,
                "s",
                "3 recorded results, still parked",
            ),
        ];
        let merged =
            merge_hung_attention(attention, true, || "liveness: 1 spawn(s) hung".to_string());
        assert_eq!(
            merged,
            vec![
                ledger::AttentionEntry::unit_scoped(
                    ledger::ATTENTION_ESCALATED,
                    "e",
                    "escalated after exhausting remediation",
                ),
                ledger::AttentionEntry::run_scoped(
                    ledger::ATTENTION_HALTED,
                    "liveness: 1 spawn(s) hung",
                ),
                ledger::AttentionEntry::unit_scoped(
                    ledger::ATTENTION_STALLED_FRONTIER,
                    "s",
                    "3 recorded results, still parked",
                ),
            ],
            "the hung halted entry must be inserted BETWEEN escalated and stalled-frontier, \
             the canonical order, not appended after both"
        );
    }

    /// Spec 44, criterion 1 (this unit OWNS the courier-prompt guarantee): the step
    /// courier must run `rigger step` as ONE FOREGROUND, BLOCKING Bash call - never
    /// `run_in_background`, never watched via a Monitor / poll loop - because a foreground
    /// call blocks until the step prints its single JSON line, which is exactly the line to
    /// relay. And when it must report a failure, the `error` string must be the command's
    /// ACTUAL stderr or the fixed phrase `step did not complete within my attempts` - NEVER
    /// an invented placeholder token. This pins the exact defect that surfaced while
    /// dogfooding the loop: a courier that backgrounded the step, watched it with a Monitor,
    /// and returned `{"wave":[],"done":false,"error":"PLACEHOLDER_DO_NOT_USE"}` before the
    /// step had produced anything - a fabricated error that stopped the run after zero waves
    /// while lying about its own state. Asserted over the EMBEDDED `RIGGER_WORKFLOW` (the
    /// `include_str!` byte-source `rigger setup` writes and the drift check reads), the same
    /// structural style spec 39 used for the workflow string, because the cargo gate set
    /// runs no JS. This unit owns ONLY the courier prompt: it asserts nothing about the
    /// driver's null-step guard (criterion 2) or the dash detachment (criterion 3).
    #[test]
    fn workflow_step_courier_prompt_is_foreground_and_honest() {
        // Assert over comment-stripped source so the phrases are checked in the actual
        // courier-prompt string literal, not the file's documentation prose.
        let code = strip_line_comments(RIGGER_WORKFLOW);

        // 1. FOREGROUND, BLOCKING: the courier runs the step as one blocking Bash call - a
        //    foreground call blocks until the step prints its single JSON line, which is
        //    exactly the line the courier relays.
        assert!(
            code.contains("FOREGROUND, BLOCKING Bash"),
            "the step-courier prompt must instruct running `rigger step` as one FOREGROUND, \
             BLOCKING Bash call (a foreground call blocks until the step prints its JSON line)"
        );

        // 2. NOT backgrounded, NOT polled: the exact shape the defect ran the step in (a
        //    `run_in_background` step watched by a Monitor, returning a fabricated error
        //    before the step produced anything) is explicitly forbidden in the prompt.
        assert!(
            code.contains("NOT run_in_background"),
            "the step-courier prompt must explicitly forbid `run_in_background` (the step must \
             block in the foreground, not run detached)"
        );
        assert!(
            code.contains("NOT via a Monitor"),
            "the step-courier prompt must explicitly forbid watching the step via a Monitor / \
             poll loop (that path fabricated an error before the step produced its JSON)"
        );

        // 3. HONEST error: when the courier must report a failure, `error` is the ACTUAL
        //    stderr or the one fixed no-completion phrase - never an invented placeholder.
        assert!(
            code.contains("step did not complete within my attempts"),
            "the courier's `error` must be allowed to carry the fixed no-completion phrase"
        );
        assert!(
            code.contains("NEVER an invented placeholder"),
            "the step-courier prompt must forbid returning a fabricated placeholder token in \
             `error` (the error must be real stderr or the fixed no-completion phrase)"
        );

        // 4. Regression guard on the exact fabricated token the defect returned: it must not
        //    appear ANYWHERE in the embedded workflow (asserted on the raw source, comments
        //    included) - a courier that returns it lies that the step failed.
        assert!(
            !RIGGER_WORKFLOW.contains("PLACEHOLDER_DO_NOT_USE"),
            "the fabricated placeholder token `PLACEHOLDER_DO_NOT_USE` must never appear in the \
             embedded workflow - a courier returning it lies that the step failed after zero waves"
        );
    }

    /// Isolate the STEP-courier prompt (the agent that runs `rigger step` and relays the wave)
    /// from the surrounding driver source, so a structural assertion pins the RIGHT agent's
    /// instructions and not some other prompt that shares a word. The prompt is the template
    /// string that opens with `Advance the run one frontier` and runs up to the `{ phase: 'Plan'`
    /// options object that closes the `agent(...)` call. Asserted over comment-stripped source so
    /// the phrases are checked in the actual prompt literal, not the file's documentation prose.
    fn step_courier_prompt(code: &str) -> &str {
        let at = code
            .find("Advance the run one frontier")
            .expect("the driver must still define the step-courier prompt");
        let end = code[at..]
            .find("{ phase: 'Plan'")
            .map(|off| at + off)
            .expect("the step-courier prompt must close with the `{ phase: 'Plan' }` options");
        &code[at..end]
    }

    /// Spec 51, criterion 3 (this unit OWNS the courier amendment): the step courier keeps its
    /// foreground-blocking rule and its placeholder prohibition, and gains the ONE sanctioned
    /// exception - if the DRIVING HARNESS (not the courier) converts the running foreground step
    /// into a BACKGROUND task because it outran the harness's foreground cap, the courier must
    /// NOT return a placeholder sentinel (the defect the re-park/wait work fixes): it WAITS on
    /// that background task's OUTPUT FILE until it holds the step's single JSON line and returns
    /// that line verbatim - polling the output file is the sanctioned wait here - or, if the JSON
    /// still cannot be obtained, falls back to the existing re-run rule (recorded gate results let
    /// a re-run resume past finished work). This pins the exact gap spec 51 closes: a courier
    /// forbidden from monitors and unable to wait returned a placeholder for an auto-backgrounded
    /// step, stopping the driver. Asserted structurally over the EMBEDDED `RIGGER_WORKFLOW` (the
    /// `include_str!` byte-source `rigger setup` writes and the drift check reads), the same
    /// convention spec 44's courier tests use because the cargo gate set runs no JS. This unit
    /// owns ONLY the courier amendment: it asserts nothing about the reviewer-error re-park
    /// (criteria 1/2) or the worktree self-heal / sweep-ordering (criteria 4/5).
    #[test]
    fn workflow_step_courier_waits_on_an_auto_backgrounded_step() {
        let code = strip_line_comments(RIGGER_WORKFLOW);
        let prompt = step_courier_prompt(&code);

        // 1. The NORMAL-case foreground rule is UNCHANGED: the courier still runs the step as one
        //    foreground, blocking Bash call (the amendment adds an exception, it does not relax
        //    the default that a foreground call blocks until the step prints its JSON line).
        assert!(
            prompt.contains("FOREGROUND, BLOCKING Bash"),
            "the amended step-courier prompt must keep the FOREGROUND, BLOCKING rule for the \
             normal case; got:\n{prompt}"
        );
        assert!(
            prompt.contains("NOT run_in_background"),
            "the amended prompt must keep forbidding the courier from backgrounding the step \
             itself; got:\n{prompt}"
        );

        // 2. The exception is scoped to a HARNESS-INITIATED backgrounding, not a courier choice:
        //    the prompt states the driving harness may convert the foreground call into a
        //    background task on its own (it outran the foreground cap), a conversion the courier
        //    did not choose - so a courier reading this cannot use it to justify backgrounding.
        assert!(
            prompt.contains("harness")
                && prompt.contains("background task")
                && prompt.contains("did not choose"),
            "the amendment must scope the wait to a HARNESS-initiated conversion of the foreground \
             call into a background task (a conversion the courier did not choose), not a courier \
             decision to background the step; got:\n{prompt}"
        );

        // 3. The SANCTIONED WAIT: on that path the courier waits on the background task's OUTPUT
        //    FILE, polling it until it holds the step's single JSON line, and returns that line
        //    verbatim - the exact sanctioned exception spec 51 grants (a courier otherwise
        //    forbidden from monitors and unable to wait).
        assert!(
            prompt.contains("output file")
                && prompt.contains("poll")
                && prompt.contains("sanctioned")
                && prompt.contains("verbatim"),
            "the amendment must instruct the courier to WAIT by polling the auto-backgrounded \
             step's OUTPUT FILE for the single JSON line and return it verbatim (the sanctioned \
             wait); got:\n{prompt}"
        );

        // 4. FALL BACK to the existing re-run rule when the JSON still cannot be obtained: the
        //    step's gate results are recorded durably, so a re-run resumes past finished work -
        //    the amendment must route to that rule, never to a fabricated result.
        assert!(
            prompt.contains("re-run") && prompt.contains("resume"),
            "if the JSON cannot be obtained from the background task's output, the amendment must \
             fall back to re-running the step (a re-run resumes past durably recorded work), not \
             fabricate a result; got:\n{prompt}"
        );

        // 5. The PLACEHOLDER PROHIBITION still holds ON THIS PATH: returning a sentinel or
        //    placeholder for an auto-backgrounded step is exactly the defect spec 51 closes, so
        //    the amended prompt must keep forbidding it (never a fabricated wave / error token).
        assert!(
            prompt.contains("placeholder"),
            "the amendment must keep the placeholder prohibition on the auto-background path (a \
             sentinel / placeholder remains forbidden); got:\n{prompt}"
        );
        assert!(
            !RIGGER_WORKFLOW.contains("PLACEHOLDER_DO_NOT_USE"),
            "the fabricated placeholder token must never appear in the embedded workflow"
        );
    }

    /// Spec 44, criterion 2 (this unit OWNS the driver null-step guard): the driver must GUARD
    /// a null step BEFORE it dereferences `step.error`. `agent()` can RESOLVE to null - rather
    /// than reject - when the courier agent dies on a TERMINAL error (an expired login, an
    /// exhausted API quota): it produces no structured output, so the await yields null instead
    /// of throwing and the surrounding try/catch never fires. Dereferencing `step.error` on that
    /// null step crashes the driver uncaught - the exact defect that surfaced while dogfooding
    /// the loop (an uncaught crash instead of a clean, resumable stop). The guard turns it into a
    /// clean, loud, RESUMABLE stop that names the likely cause. This unit owns ONLY the null-step
    /// guard: it asserts nothing about the courier prompt (criterion 1) or the dash detachment
    /// (criterion 3). Asserted structurally over the EMBEDDED `RIGGER_WORKFLOW` (the
    /// `include_str!` byte-source `rigger setup` writes and the drift check reads), the same
    /// style spec 39 used for the workflow string, because the cargo gate set runs no JS.
    #[test]
    fn workflow_driver_guards_a_null_step_before_dereferencing_it() {
        // Assert over comment-stripped source so the guard is checked in the actual driver code
        // and its stop-message string literal, not the file's documentation prose.
        let code = strip_line_comments(RIGGER_WORKFLOW);

        // 1. The guard EXISTS: the driver tests `!step` (agent() resolved to null) explicitly.
        assert!(
            code.contains("if (!step)"),
            "the driver must guard a null step with `if (!step)` before touching its fields"
        );

        // 2. The guard PRECEDES the dereference: `if (!step)` must appear BEFORE the first
        //    `step.error` read, or a null step would still crash on the very dereference the
        //    guard exists to prevent (presence alone does not prove the guard is reachable in
        //    time - the wedge-stop ordering test above pins position for the same reason).
        let guard = code
            .find("if (!step)")
            .expect("the driver must guard a null step");
        let deref = code
            .find("step.error")
            .expect("the driver must read step.error after the guard");
        assert!(
            guard < deref,
            "the `if (!step)` guard must precede the `step.error` dereference, or a null step \
             (agent() resolved to null) would still crash before the guard runs"
        );

        // 3. The guard stops CLEANLY and LOUDLY: it routes the null step through the throwing
        //    `stop()` (a controlled workflow failure), never a silent return or an uncaught
        //    null-dereference crash. The stop call must live between the guard and the deref.
        assert!(
            code[guard..deref].contains("stop("),
            "the null-step guard must stop loudly via `stop(...)` (a clean, controlled failure), \
             not fall through or crash on the dereference"
        );

        // 4. The diagnostic names the LIKELY CAUSE (the courier agent died on a terminal API
        //    error - an expired login / an exhausted quota, so agent() resolved to null) and
        //    that the run is RESUMABLE - the two things spec 44 requires the message to carry so
        //    the operator knows why it stopped and that a re-run continues from this frontier.
        assert!(
            code.contains("resolved to null"),
            "the null-step diagnostic must name the cause: agent() RESOLVED TO NULL rather than \
             rejecting (the courier agent died terminally, producing no JSON)"
        );
        assert!(
            code.contains("expired login") && code.contains("quota"),
            "the null-step diagnostic must name the likely terminal cause (an expired login or \
             an exhausted API quota)"
        );
        assert!(
            code.contains("RESUMABLE"),
            "the null-step diagnostic must tell the operator the run is RESUMABLE (a re-run \
             continues from this frontier)"
        );
    }

    /// Spec 19a Unit 3 (done-when item 3): the static `meta.description` is the tagline
    /// the skills list and the `/workflows` header both show, so it must read as a
    /// jargon-free, user-useful line - what the workflow does and when to reach for it -
    /// NOT the driver's internal plumbing. The architecture explanation lives in the
    /// file's header comment; the tagline must leak NONE of the plumbing terms the old
    /// description carried ("driven THINLY", "courier", "SpawnResult"). Asserted over the
    /// EMBEDDED `RIGGER_WORKFLOW` (the `include_str!` byte-source the drift check and the
    /// thin-driver contract test also read), because the cargo gate set runs no JS. This
    /// unit owns ONLY the `meta.description` scrub; the `SpawnRequest.title` live-render
    /// is a separate unit's concern, so this test asserts nothing about it.
    #[test]
    fn workflow_meta_description_is_a_user_facing_tagline_free_of_plumbing_terms() {
        let desc = meta_description(RIGGER_WORKFLOW);

        // 1. None of the internal plumbing terms leak into the user-facing tagline. Each
        //    names the driver's mechanism (the thin courier over the conductor, the
        //    SpawnResult wire) rather than the user's outcome; that prose belongs in the
        //    file's header comment, not the skills-list / `/workflows` tagline.
        for term in ["driven THINLY", "courier", "SpawnResult"] {
            assert!(
                !desc.contains(term),
                "meta.description is the user-facing tagline; it must not leak the internal \
                 plumbing term {term:?} (that prose belongs in the file's header comment): \
                 {desc:?}"
            );
        }

        // 2. meta stays a PURE static literal (the Workflow runtime extracts it before the
        //    body runs), so the tagline carries no interpolation / computed values.
        assert!(
            !desc.contains("${"),
            "meta.description must be a pure static literal - no `${{...}}` interpolation: \
             {desc:?}"
        );

        // 3. It reads as a CONCISE tagline, not the multi-clause plumbing paragraph the old
        //    description was (~900+ chars). A one-line tagline fits a sane length bound.
        assert!(
            desc.len() < 350,
            "meta.description must read as a concise one-line tagline, not a plumbing \
             paragraph ({} chars): {desc:?}",
            desc.len()
        );

        // 4. It is USER-USEFUL: it names what the workflow acts on (a spec, so a user knows
        //    when to reach for it) AND what it DOES for them (build / implement / deliver /
        //    turn a spec into code), not only how the driver is wired internally.
        let lc = desc.to_lowercase();
        assert!(
            lc.contains("spec"),
            "the tagline must name what the workflow acts on (a spec) so a user knows when \
             to reach for it: {desc:?}"
        );
        assert!(
            [
                "build",
                "implement",
                "deliver",
                "turn",
                "ship",
                "write",
                "make"
            ]
            .iter()
            .any(|verb| lc.contains(verb)),
            "the tagline must say what the workflow DOES for the user (build / implement / \
             deliver / turn a spec into working code), not only how it is wired: {desc:?}"
        );
    }

    /// `rigger setup` runs npm install against the provisioned shim so `node_modules`
    /// is ready. When npm is available we run it FOR REAL against a temp dir and
    /// confirm `node_modules` appears; when npm is unavailable we assert the clear
    /// error path instead (never a silent skip).
    #[test]
    fn setup_runs_npm_install_or_reports_a_clear_error() {
        let dir = tempfile::tempdir().unwrap();
        let shim = write_shim_files(dir.path()).unwrap();

        if npm_available() {
            // npm is on PATH: provisioning must run it for real and leave node_modules.
            provision_shim(dir.path()).expect("provision_shim must succeed when npm is available");
            assert!(
                shim.join("node_modules").is_dir(),
                "npm install must populate node_modules in the provisioned shim dir"
            );
        } else {
            // npm is NOT on PATH: the error must be clear and actionable, not a silent
            // skip. Point RIGGER_NPM at a binary that does not exist to exercise the
            // missing-npm path deterministically.
            std::env::set_var("RIGGER_NPM", "definitely-not-a-real-npm-binary-xyz");
            let err = run_npm_install(&shim).expect_err("a missing npm must be a clear error");
            std::env::remove_var("RIGGER_NPM");
            let msg = err.to_string();
            assert!(
                msg.contains("npm") && msg.to_lowercase().contains("path"),
                "the missing-npm error must mention npm and PATH; got: {msg}"
            );
        }
    }

    /// `rigger workflow` runs the PROVISIONED per-project shim when `.rigger/shim/`
    /// exists, and otherwise reports a clear "run `rigger setup`" error rather than
    /// failing obscurely.
    #[test]
    fn workflow_locates_the_provisioned_shim_or_tells_you_to_run_setup() {
        // Guard the RIGGER_SHIM override does not leak in from the environment.
        let prior = std::env::var("RIGGER_SHIM").ok();
        std::env::remove_var("RIGGER_SHIM");

        let dir = tempfile::tempdir().unwrap();
        // Absent: a clear, actionable error naming `rigger setup`.
        let err = locate_shim(dir.path()).expect_err("an unprovisioned project must error");
        assert!(
            err.to_string().contains("rigger setup"),
            "the unprovisioned error must tell the user to run `rigger setup`; got: {err}"
        );

        // After provisioning the files, locate_shim finds the per-project shim.mjs.
        let shim = write_shim_files(dir.path()).unwrap();
        let found = locate_shim(dir.path()).expect("a provisioned shim must be located");
        assert_eq!(
            Path::new(&found),
            shim.join("shim.mjs"),
            "locate_shim must return the provisioned .rigger/shim/shim.mjs"
        );

        if let Some(v) = prior {
            std::env::set_var("RIGGER_SHIM", v);
        }
    }

    fn npm_available() -> bool {
        Command::new("npm")
            .arg("--version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    use rigger::metrics::GateCounts;
    use std::collections::BTreeMap;

    /// `format_stats` must surface ALL FOUR required metrics - first-pass yield,
    /// per-gate remediation (pass/fail) counts, escalation rate, and review
    /// approve/reject - from a fully-populated `Metrics` value. This pins the CLI
    /// contract for `rigger stats` (the spec's "stats prints the four metrics")
    /// without touching the filesystem.
    #[test]
    fn format_stats_prints_all_four_metrics() {
        let mut gates = BTreeMap::new();
        gates.insert("build".to_string(), GateCounts { pass: 4, fail: 1 });
        gates.insert("clippy".to_string(), GateCounts { pass: 3, fail: 2 });
        let m = Metrics {
            units_started: 4,
            first_pass_clean: 3,
            gates,
            units_escalated: 1,
            review_approve: 5,
            review_reject: 2,
            ..Default::default()
        };
        let out = format_stats(&m).join("\n");

        // 1. First-pass yield: 3/4 = 75.0%, with the fraction shown.
        assert!(
            out.contains("first-pass yield   75.0% (3/4 units clean on the first pass)"),
            "first-pass yield line missing/wrong:\n{out}"
        );
        // 2. Escalation rate: 1/4 = 25.0%, with the fraction shown.
        assert!(
            out.contains("escalation rate    25.0% (1/4 units escalated to a human)"),
            "escalation rate line missing/wrong:\n{out}"
        );
        // 3. Review approve/reject counts.
        assert!(
            out.contains("review             5 approved / 2 rejected"),
            "review approve/reject line missing/wrong:\n{out}"
        );
        // 4. Per-gate remediation counts: one line per gate (fail = remediation),
        // sorted by gate id (build before clippy).
        assert!(
            out.contains("build            4 pass / 1 fail / 5 total"),
            "build gate line missing/wrong:\n{out}"
        );
        assert!(
            out.contains("clippy           3 pass / 2 fail / 5 total"),
            "clippy gate line missing/wrong:\n{out}"
        );
        let build_at = out.find("build ").expect("build gate present");
        let clippy_at = out.find("clippy ").expect("clippy gate present");
        assert!(build_at < clippy_at, "gates must be sorted by id:\n{out}");
    }

    /// A zeroed `Metrics` (the shape `project(&[])` returns) must render guarded,
    /// NaN-free output: 0.0% rates and a "no gate runs" line, never `NaN%` from a
    /// divide-by-zero or an empty/blank gates section.
    #[test]
    fn format_stats_handles_zeroed_metrics_without_nan() {
        let out = format_stats(&Metrics::default()).join("\n");
        assert!(out.contains("first-pass yield   0.0%"), "{out}");
        assert!(out.contains("escalation rate    0.0%"), "{out}");
        assert!(
            out.contains("review             0 approved / 0 rejected"),
            "{out}"
        );
        assert!(
            out.contains("gates              (no gate runs recorded)"),
            "a run with no gate runs must say so, not print a blank section:\n{out}"
        );
        assert!(
            !out.to_lowercase().contains("nan"),
            "rates must be guarded, never NaN:\n{out}"
        );
    }

    /// spec 17 criterion 4c: the runtime parallelism-retention metric must REACH an operator on
    /// the production `rigger stats` render (previously it was computed by `metrics::project` but
    /// no path surfaced it). A MEASURED retention shows a row with the co-schedulable share; a
    /// retention below [`metrics::PARALLELISM_RETENTION_WARN`] adds a loud inline WARN naming the
    /// floor so a silently-serializing fleet is visible; and an UNMEASURED retention (`None` - the
    /// shipped non-symbols default records no `BlastRadiusComputed` audit) OMITS the row entirely,
    /// so the default `rigger stats` output is byte-for-byte unchanged.
    #[test]
    fn format_stats_surfaces_parallelism_retention_and_warns_below_the_floor() {
        // Measured and above the floor: a row with the share, no WARN.
        let healthy = Metrics {
            parallelism_retention: Some(0.95),
            ..Default::default()
        };
        let out = format_stats(&healthy).join("\n");
        assert!(
            out.contains("parallelism        95.0%"),
            "a measured retention must appear on an operator-visible stats row:\n{out}"
        );
        assert!(
            !out.contains("WARN"),
            "a healthy fleet at or above the floor must not warn:\n{out}"
        );

        // Measured and below the floor: the share is still shown AND a loud WARN names the floor.
        let serializing = Metrics {
            parallelism_retention: Some(0.5),
            ..Default::default()
        };
        let out = format_stats(&serializing).join("\n");
        assert!(
            out.contains("parallelism        50.0%"),
            "the below-floor retention share must still be shown:\n{out}"
        );
        assert!(
            out.contains("WARN") && out.contains("80.0% floor"),
            "a below-floor retention must warn and name the 80.0% floor:\n{out}"
        );

        // Unmeasured (the shipped non-symbols default): no retention row at all.
        let unmeasured = Metrics {
            parallelism_retention: None,
            ..Default::default()
        };
        let out = format_stats(&unmeasured).join("\n");
        assert!(
            !out.contains("parallelism"),
            "an unmeasured retention (default lane) must omit the row, keeping default stats \
             output unchanged:\n{out}"
        );
    }

    /// The parallelism-retention line is single-sourced through [`parallelism_retention_line`] so
    /// the `rigger stats` row and the end-of-`rigger run` stderr notice (spec 17 4c's "logged
    /// warning when retention drops below the threshold on a run") render IDENTICALLY and cannot
    /// drift: `None` when unmeasured, no `WARN` at or above the floor, and a `WARN` naming the
    /// floor below it.
    #[test]
    fn parallelism_retention_line_is_single_sourced_and_warns_below_the_floor() {
        assert!(
            parallelism_retention_line(&Metrics {
                parallelism_retention: None,
                ..Default::default()
            })
            .is_none(),
            "an unmeasured retention yields no line (nothing to surface)"
        );
        let healthy = parallelism_retention_line(&Metrics {
            parallelism_retention: Some(0.9),
            ..Default::default()
        })
        .expect("a measured retention yields a line");
        assert!(
            healthy.contains("90.0%") && !healthy.contains("WARN"),
            "a healthy retention shows the share without a warning: {healthy}"
        );
        let warn = parallelism_retention_line(&Metrics {
            parallelism_retention: Some(0.4),
            ..Default::default()
        })
        .expect("a measured retention yields a line");
        assert!(
            warn.contains("40.0%") && warn.contains("WARN") && warn.contains("80.0% floor"),
            "a below-floor retention warns and names the floor: {warn}"
        );
    }

    /// spec 11 remediation: an in-process (cli) run has findings but records NO adjudicator
    /// verdict (no SpawnResult), so the upheld-based folds are unfed. The render must
    /// DISCLOSE that honestly rather than let a reader misread the 0% survival as the
    /// adjudicator having discarded every finding.
    #[test]
    fn stats_discloses_when_no_verdict_was_recorded_on_this_driver() {
        let mut finding_survival = BTreeMap::new();
        finding_survival.insert(
            "lens:sdet".to_string(),
            metrics::FindingCounts {
                raised: 3,
                upheld: 0,
            },
        );
        let m = Metrics {
            review_quality: metrics::ReviewQuality {
                finding_survival,
                adjudications: 0,
                ..Default::default()
            },
            ..Default::default()
        };
        let out = format_stats(&m).join("\n");
        assert!(
            out.contains("no adjudicator verdict recorded on this run's driver"),
            "an in-process run with findings but no recorded verdict must disclose the unfed numerator:\n{out}"
        );

        // With a verdict recorded (the courier path), the disclosure is suppressed.
        let mut finding_survival = BTreeMap::new();
        finding_survival.insert(
            "lens:sdet".to_string(),
            metrics::FindingCounts {
                raised: 3,
                upheld: 2,
            },
        );
        let m = Metrics {
            review_quality: metrics::ReviewQuality {
                finding_survival,
                adjudications: 1,
                ..Default::default()
            },
            ..Default::default()
        };
        let out = format_stats(&m).join("\n");
        assert!(
            !out.contains("no adjudicator verdict recorded"),
            "a run WITH a recorded verdict must not print the disclosure:\n{out}"
        );
    }

    /// spec 11 remediation (the reject this unit fixes): a run RECORDS an adjudicator verdict
    /// (adjudications > 0) yet folds ZERO upheld per actor because the upheld findings carry
    /// no attribution on this log (the empty-actor sentinel dropped them - the dominant shape
    /// on a real aggregate store). The prior guard keyed the disclosure on `adjudications == 0`
    /// only, so this case rendered an all-zero survival / "-" cost panel with NO disclosure -
    /// the exact "review upheld nothing" misread this unit exists to prevent. The render must
    /// now DISCLOSE the unfed numerator whenever an all-zero-upheld panel hides a dropped
    /// numerator, and stay SILENT only when the adjudicator genuinely upheld nothing.
    #[test]
    fn stats_discloses_unfed_numerator_when_verdict_recorded_but_findings_unattributed() {
        let mut finding_survival = BTreeMap::new();
        finding_survival.insert(
            "lens:sdet".to_string(),
            metrics::FindingCounts {
                raised: 3,
                upheld: 0,
            },
        );
        let mut tier_cost = BTreeMap::new();
        tier_cost.insert(
            "lens".to_string(),
            metrics::TierCost {
                spawns: 2,
                upheld: 0,
            },
        );
        tier_cost.insert(
            "adjudicator".to_string(),
            metrics::TierCost {
                spawns: 1,
                upheld: 0,
            },
        );
        let m = Metrics {
            review_reject: 5,
            review_quality: metrics::ReviewQuality {
                finding_survival,
                tier_cost,
                adjudications: 1,       // a verdict WAS recorded ...
                upheld_unattributed: 2, // ... but the findings it upheld are unattributed here
                ..Default::default()
            },
            ..Default::default()
        };
        let out = format_stats(&m).join("\n");
        assert!(
            out.contains("unfed upheld numerator"),
            "an all-zero-upheld panel with a recorded verdict but unattributed upheld findings must disclose the unfed numerator:\n{out}"
        );
        assert!(
            out.contains("2 upheld finding(s) carry no attribution"),
            "the disclosure must name the count of dropped upheld findings:\n{out}"
        );
        assert!(
            !out.contains("no adjudicator verdict recorded"),
            "with a verdict recorded, the disclosure must not claim none was recorded:\n{out}"
        );

        // A verdict that recorded and GENUINELY upheld nothing (nothing dropped) is NOT unfed;
        // its 0% is honest, so the render must stay silent rather than cry an unfed numerator.
        let mut finding_survival = BTreeMap::new();
        finding_survival.insert(
            "lens:sdet".to_string(),
            metrics::FindingCounts {
                raised: 3,
                upheld: 0,
            },
        );
        let m = Metrics {
            review_quality: metrics::ReviewQuality {
                finding_survival,
                adjudications: 1,
                upheld_unattributed: 0,
                ..Default::default()
            },
            ..Default::default()
        };
        let out = format_stats(&m).join("\n");
        assert!(
            !out.contains("unfed upheld numerator"),
            "a genuine all-discard verdict (nothing upheld, nothing dropped) must not claim an unfed numerator:\n{out}"
        );
    }

    /// spec 11 remediation (adv-u1r-cause-split-folds-undisclosed-on-cli): a rejection's cause
    /// folds only from a RECORDED adjudicator reject verdict, so on a real aggregate store the
    /// cause panel accounts for far fewer rejects than `review_reject` (e.g. `spec-ambiguity 1`
    /// beside `64 rejected`). The render must disclose the unfed remainder so the cause panel
    /// is never misread as the full reject breakdown.
    #[test]
    fn stats_discloses_cause_split_remainder_when_fewer_causes_than_rejects() {
        let mut rejections_by_cause = BTreeMap::new();
        rejections_by_cause.insert("spec-ambiguity".to_string(), 1u64);
        let m = Metrics {
            review_reject: 64,
            review_quality: metrics::ReviewQuality {
                rejections_by_cause,
                ..Default::default()
            },
            ..Default::default()
        };
        let out = format_stats(&m).join("\n");
        assert!(
            out.contains("cause folded for 1/64 review rejects"),
            "a cause panel accounting for fewer rejects than review_reject must disclose the remainder:\n{out}"
        );
        assert!(
            out.contains("the other 63 carry no recorded verdict cause"),
            "the disclosure must name the unfed remainder count:\n{out}"
        );

        // When every reject carries a folded cause, no remainder disclosure fires.
        let mut rejections_by_cause = BTreeMap::new();
        rejections_by_cause.insert("genuine-defect".to_string(), 2u64);
        let m = Metrics {
            review_reject: 2,
            review_quality: metrics::ReviewQuality {
                rejections_by_cause,
                ..Default::default()
            },
            ..Default::default()
        };
        let out = format_stats(&m).join("\n");
        assert!(
            !out.contains("carry no recorded verdict cause"),
            "with every reject's cause folded, no remainder disclosure should fire:\n{out}"
        );
    }

    /// `cmd_stats` rejects any positional argument with a clear error (it takes none),
    /// mirroring the strict-arity errors the other CLI commands raise.
    #[test]
    fn cmd_stats_rejects_extra_arguments() {
        let err = cmd_stats(&["unexpected".to_string()]).expect_err("stats takes no arguments");
        assert!(
            err.to_string().contains("stats: expected no arguments"),
            "the error must explain stats takes no arguments; got: {err}"
        );
    }

    #[test]
    fn baseline_run_slice_selects_a_run_by_id_including_a_middle_run() {
        // A multi-run store. An explicit id slices THAT run's window
        // (RunStarted..next RunStarted) even for a MIDDLE run - so replaying an OLD run
        // never folds the newer runs appended after it - while `latest` selects the
        // current run and an unknown id (or empty stream) is None.
        let rs = |run: &str| {
            Event::new(
                runscope::TYPE_RUN_STARTED,
                serde_json::to_vec(&serde_json::json!({"run": run, "criteria": []})).unwrap(),
            )
        };
        let unit = |id: &str| {
            Event::new(
                ledger::TYPE_UNIT_STARTED,
                serde_json::to_vec(&serde_json::json!({"id": id, "agent": "w"})).unwrap(),
            )
        };
        let events = vec![
            rs("run-A"),
            unit("a1"),
            rs("run-B"),
            unit("b1"),
            unit("b2"),
            rs("run-C"),
            unit("c1"),
        ];

        let b = baseline_run_slice(&events, "run-B").expect("run-B exists");
        assert_eq!(b.len(), 3, "run-B is its RunStarted plus its two units");
        assert_eq!(b[0].type_, runscope::TYPE_RUN_STARTED);
        assert!(String::from_utf8_lossy(&b[1].data).contains("b1"));
        assert!(
            !b.iter()
                .any(|e| String::from_utf8_lossy(&e.data).contains("c1")),
            "run-C is excluded from run-B's slice"
        );
        assert_eq!(
            baseline_run_slice(&events, "run-A").unwrap().len(),
            2,
            "the first run is bounded by run-B's boundary"
        );
        let latest = baseline_run_slice(&events, "latest").unwrap();
        assert!(String::from_utf8_lossy(&latest[1].data).contains("c1"));
        assert!(baseline_run_slice(&events, "run-Z").is_none(), "unknown id");
        assert!(baseline_run_slice(&[], "latest").is_none(), "empty stream");
    }

    #[test]
    fn format_stats_diff_flags_only_the_changed_rows() {
        let base = Metrics {
            review_approve: 1,
            ..Default::default()
        };
        let cand = Metrics {
            review_approve: 0,
            ..Default::default()
        };
        let lines = format_stats_diff("run-X", "abc123", &base, &cand);
        assert!(
            lines[0].contains("run-X") && lines[0].contains("abc123"),
            "the header names the baseline run and the candidate rev; got: {:?}",
            lines[0]
        );
        let review = lines
            .iter()
            .find(|l| l.contains("review approved"))
            .expect("a review-approved row");
        assert!(
            review.trim_end().ends_with('*'),
            "the changed review row is flagged; got: {review:?}"
        );
        let units = lines
            .iter()
            .find(|l| l.contains("units started"))
            .expect("a units-started row");
        assert!(
            !units.trim_end().ends_with('*'),
            "an unchanged row carries no flag; got: {units:?}"
        );
    }

    // --- Spec 65 unit 5: `rigger validate` SURFACES the resolved wrapper, cache dir, and
    // budget; `rigger setup` writes `build: { wrapper: auto }` for new projects only. ---

    /// With a wrapper active, `rigger validate` reports the wrapper name, the cache dir it
    /// resolved to, AND the budget - three lines, in that order, so an operator sees the
    /// whole resolved build environment at a glance (Design: "wrapper ... cache dir, slot
    /// budget"). The cache dir line reads through the SAME [`resolved_cache_dir`] the
    /// [`BuildEnv`] resolver and the cache-dir probe use - never a second independently
    /// re-derived path.
    #[test]
    fn build_environment_report_with_a_wrapper_lists_wrapper_cache_dir_and_budget() {
        let build = config::BuildConfig {
            wrapper: "sccache".to_string(),
            cache_dir: "/tmp/example-cache".to_string(),
            jobs: 0,
            max_concurrent: 4,
            mutation: String::new(),
        };
        let lines = build_environment_report(Some("sccache"), &build, false);
        assert_eq!(
            lines,
            vec![
                "build wrapper: sccache".to_string(),
                "build cache dir: /tmp/example-cache".to_string(),
                "build budget: 4".to_string(),
                "build mutation: off".to_string(),
            ]
        );
    }

    /// With no wrapper resolved (`off`, or `auto` finding nothing), NO cache dir line is
    /// printed - an inactive layer touches no cache dir, so claiming one would be a fabricated
    /// surface - but the budget line still prints: `build.max_concurrent` gates every compiler
    /// invocation this loop runs regardless of whether a wrapper is configured.
    #[test]
    fn build_environment_report_with_no_wrapper_omits_cache_dir_but_keeps_budget() {
        let build = config::BuildConfig {
            wrapper: String::new(),
            cache_dir: String::new(),
            jobs: 0,
            max_concurrent: 8,
            mutation: String::new(),
        };
        let lines = build_environment_report(None, &build, false);
        assert_eq!(
            lines,
            vec![
                "build wrapper: none".to_string(),
                "build budget: 8".to_string(),
                "build mutation: off".to_string(),
            ]
        );
    }

    /// `max_concurrent: 0` is the documented unlimited convention (matching
    /// `defaults.budget`'s own `0` = unlimited); the report says so in words, never a bare
    /// misleading `0`.
    #[test]
    fn build_environment_report_zero_max_concurrent_reports_unlimited() {
        let build = config::BuildConfig {
            max_concurrent: 0,
            ..Default::default()
        };
        let lines = build_environment_report(None, &build, false);
        assert!(
            lines.iter().any(|l| l == "build budget: unlimited"),
            "a zero max_concurrent must report as unlimited, got: {lines:?}"
        );
    }

    /// Spec 73: `rigger validate` reports `build mutation: on` when the step is enabled -
    /// given the ALREADY-RESOLVED bool, mirroring the wrapper report's own already-resolved
    /// convention.
    #[test]
    fn build_environment_report_reports_mutation_on() {
        let build = config::BuildConfig::default();
        let lines = build_environment_report(None, &build, true);
        assert!(
            lines.iter().any(|l| l == "build mutation: on"),
            "a resolved-enabled mutation step must report on, got: {lines:?}"
        );
    }

    /// The `off` counterpart of `build_environment_report_reports_mutation_on` - the default,
    /// back-compat case for every workflow committed before this key existed.
    #[test]
    fn build_environment_report_reports_mutation_off() {
        let build = config::BuildConfig::default();
        let lines = build_environment_report(None, &build, false);
        assert!(
            lines.iter().any(|l| l == "build mutation: off"),
            "a resolved-disabled mutation step must report off, got: {lines:?}"
        );
    }

    // --- Spec 71, criterion 3: `rigger validate` detects a stream whose position order and
    // revision order disagree (the signature left by a write that lands in a compaction-
    // opened revision hole). ---

    // NOTE: `order_signatures` itself (the pure running-max-revision detector) and its own
    // boundary tests - including the duplicate-revision case - now live in
    // `src/watch.rs`'s own test module: it moved there (spec 69 u69c2 consolidation) to be the
    // ONE shared implementation `rigger watch`'s store-integrity signal also calls, rather
    // than a second parallel reimplementation. Only the advisory FORMATTING below (unique to
    // `rigger validate`) still belongs to this file.

    /// The advisory names the stream, the out-of-order row count, the affected position
    /// range, and the doc location the repair procedure lives at - an operator reading
    /// `rigger validate`'s stderr has everything needed to find and fix it, without validate
    /// performing any repair itself (report-only, like every other validate advisory).
    #[test]
    fn order_signature_advisories_names_the_stream_count_range_and_repair_doc() {
        let signatures = vec![watch::OrderSignature {
            stream: "run".to_string(),
            rows: 2,
            first_position: 5,
            last_position: 6,
        }];
        let advisories = order_signature_advisories(&signatures);
        assert_eq!(advisories.len(), 1);
        let a = &advisories[0];
        assert!(a.starts_with("warning:"), "advisory: {a}");
        assert!(
            a.contains("run") && a.contains('2') && a.contains('5') && a.contains('6'),
            "advisory names the stream, count, and position range: {a}"
        );
        assert!(
            a.contains(watch::ORDER_SIGNATURE_REPAIR_DOC_REF),
            "advisory names the repair doc location: {a}"
        );
    }

    /// A clean log's empty signature list draws no advisories at all.
    #[test]
    fn order_signature_advisories_is_empty_when_no_signatures_are_given() {
        assert!(order_signature_advisories(&[]).is_empty());
    }

    // --- Spec 68, VALIDATE ADVISORIES: the two pure formatters (INDEX STALENESS, LOG BLOAT) ---

    #[test]
    fn index_staleness_message_names_every_kind_of_disagreement_and_the_fix() {
        let drift = rigger::grounder::symbols::IndexDrift {
            added: vec!["new.rs".to_string()],
            removed: vec!["gone.rs".to_string()],
            changed: vec!["edited.rs".to_string()],
        };
        let msg = index_staleness_message(&drift);
        assert!(msg.starts_with("warning:"), "advisory: {msg}");
        assert!(
            msg.contains('1'),
            "the message must carry the per-kind counts: {msg}"
        );
        assert!(
            msg.contains("rigger reindex"),
            "the message must name the fix: {msg}"
        );
    }

    #[test]
    fn bloat_advisory_is_none_at_or_below_the_threshold_and_named_above_it() {
        // Exactly at the threshold: not yet a warning-worthy signal.
        let at_threshold = rigger::eventstore::sqlite::DerivedDuplication {
            rows: 3,
            distinct_keys: 2, // factor 1.5 == BLOAT_DUPLICATION_THRESHOLD
        };
        assert_eq!(bloat_advisory(&at_threshold), None);

        // Clearly above: a named warning carrying the measured factor and the fix.
        let above_threshold = rigger::eventstore::sqlite::DerivedDuplication {
            rows: 6,
            distinct_keys: 1, // factor 6.0
        };
        let advisory = bloat_advisory(&above_threshold).expect("must warn above threshold");
        assert!(advisory.starts_with("warning:"), "advisory: {advisory}");
        assert!(
            advisory.contains("6.0"),
            "the message must carry the measured factor: {advisory}"
        );
        assert!(
            advisory.contains("rigger reset --derived"),
            "the message must name the fix: {advisory}"
        );
    }

    #[test]
    fn bloat_advisory_for_never_fabricates_a_store_that_does_not_exist() {
        // No `.rigger/events.db` at all: `bloat_advisory_for` must skip BEFORE opening
        // anything (opening would create the file - a read-only advisory's forbidden side
        // effect), and the file must genuinely stay absent afterwards.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(".rigger").join("events.db");
        assert_eq!(
            bloat_advisory_for(path.to_str().unwrap(), "proj"),
            None,
            "no store yet is not evidence of bloat"
        );
        assert!(
            !path.exists(),
            "a read-only advisory must never create the store it found absent"
        );
    }

    /// The scaffolded workflow (`rigger init`/`setup` on a NEW project) declares
    /// `build: { wrapper: auto }` (spec 65 Design: "`rigger setup` writes the `build:`
    /// section with `wrapper: auto` for new projects") - a fresh project benefits from a
    /// machine's already-installed compilation-cache wrapper with no further config.
    #[test]
    fn scaffold_workflow_declares_build_wrapper_auto() {
        let wf: config::Workflow =
            serde_yaml::from_str(SCAFFOLD_WORKFLOW).expect("the scaffolded workflow must parse");
        assert_eq!(
            wf.build.wrapper, "auto",
            "a freshly scaffolded workflow.yml must default build.wrapper to auto"
        );
    }

    /// `rigger setup`/`init` NEVER clobbers an existing `workflow.yml` (spec 65 Design:
    /// "never clobbers an existing one") - including a committed `build:` section that
    /// differs from the fresh-project default. A project that has already opted OUT
    /// (`wrapper: off`) must stay opted out across a rerun, never silently flipped back to
    /// `auto`.
    #[test]
    fn init_project_never_clobbers_an_existing_build_section() {
        let dir = tempfile::tempdir().unwrap();
        let rigger_dir = dir.path().join(RIGGER_DIR);
        std::fs::create_dir_all(&rigger_dir).unwrap();
        let workflow_path = rigger_dir.join("workflow.yml");
        std::fs::write(&workflow_path, "name: custom\nbuild:\n  wrapper: off\n").unwrap();

        init_project(dir.path()).expect("a rerun over an existing project must succeed");

        let after = std::fs::read_to_string(&workflow_path).unwrap();
        assert_eq!(
            after, "name: custom\nbuild:\n  wrapper: off\n",
            "an existing workflow.yml's build: section must be left byte-for-byte untouched"
        );
    }

    #[test]
    fn parse_replay_args_requires_a_run_and_a_rev_in_either_order() {
        assert!(parse_replay_args(&[]).is_err(), "no args is an error");
        assert!(
            parse_replay_args(&["latest".to_string()]).is_err(),
            "missing --against is an error"
        );
        let (run, rev) =
            parse_replay_args(&["latest".into(), "--against".into(), "HEAD".into()]).unwrap();
        assert_eq!((run.as_str(), rev.as_str()), ("latest", "HEAD"));
        // The flag may lead the positional.
        let (run, rev) =
            parse_replay_args(&["--against".into(), "rev1".into(), "run-7".into()]).unwrap();
        assert_eq!((run.as_str(), rev.as_str()), ("run-7", "rev1"));
        assert!(
            parse_replay_args(&["a".into(), "b".into(), "--against".into(), "r".into()]).is_err(),
            "a second positional is an error, not silently ignored"
        );
    }

    /// On an absent `events.db` (a project that has never run) `cmd_stats` must print
    /// the clear "no runs yet" message and succeed, NOT create the db or panic. Run in
    /// a temp dir so the real project's `.rigger/` is untouched.
    #[test]
    // Mutates the process-global CWD (`set_current_dir` below). Shares the `cwd` serial
    // key with `shipped_workflows_carry_a_non_zero_spawn_budget` (which reads relative
    // paths) so the two never run concurrently: the restore guard prevents LEAKING a
    // changed CWD past this test, but only mutual exclusion prevents the other test from
    // OBSERVING the changed CWD mid-window.
    #[serial_test::serial(cwd)]
    fn cmd_stats_on_a_never_run_project_says_no_runs_and_creates_no_db() {
        let dir = tempfile::tempdir().unwrap();
        let prev = std::env::current_dir().unwrap();
        // current_dir is process-global; serialize against the other cwd-sensitive
        // path via a guard that always restores it even on a failed assertion.
        struct Restore(std::path::PathBuf);
        impl Drop for Restore {
            fn drop(&mut self) {
                let _ = std::env::set_current_dir(&self.0);
            }
        }
        let _restore = Restore(prev);
        std::env::set_current_dir(dir.path()).unwrap();

        cmd_stats(&[]).expect("stats on a never-run project must succeed");

        // The absent-db guard must run BEFORE Store::open, so no events.db is created.
        assert!(
            !dir.path().join(RIGGER_DIR).join("events.db").exists(),
            "stats on a never-run project must not create events.db"
        );
    }

    /// `rigger step` SERIALIZES: while one step holds the lock, a second concurrent step
    /// REFUSES (with the driver-recognizable busy token) instead of running - so the run
    /// advances one step at a time and two steps never race the shared run state. And the
    /// refusal is not permanent: once the first releases, a later step acquires cleanly.
    #[test]
    #[serial_test::serial(cwd)]
    fn a_second_concurrent_rigger_step_refuses_and_the_lock_frees_on_release() {
        let dir = tempfile::tempdir().unwrap();
        let prev = std::env::current_dir().unwrap();
        struct Restore(std::path::PathBuf);
        impl Drop for Restore {
            fn drop(&mut self) {
                let _ = std::env::set_current_dir(&self.0);
            }
        }
        let _restore = Restore(prev);
        std::env::set_current_dir(dir.path()).unwrap();
        std::fs::create_dir_all(RIGGER_DIR).unwrap();

        // First step holds the exclusive lock for its whole duration.
        let held =
            acquire_step_lock(Path::new(RIGGER_DIR)).expect("the first step must acquire the lock");
        // A second concurrent step must REFUSE fast (not block, not double-run) and carry the
        // token the driver keys on to back off rather than tear the run down.
        let err = acquire_step_lock(Path::new(RIGGER_DIR))
            .expect_err("a second concurrent step must refuse");
        assert!(
            err.to_string().contains(STEP_BUSY_TOKEN),
            "the refusal must carry the busy token for the driver: {err}"
        );
        // Releasing the first frees the lock so a LATER step proceeds - the refusal is
        // transient, not a wedge. Assert that eventual-acquire contract with a bounded
        // backoff, not a single instantaneous try: in a saturated parallel test binary a
        // concurrently spawned subprocess can momentarily inherit the just-released lock fd
        // across its fork/exec window (before close-on-exec fires and drops it), so an
        // immediate reacquire can still observe a spurious BUSY. That transient refusal is
        // precisely what the driver is built to ride - back off on STEP_BUSY_TOKEN and retry -
        // so the test models the same protocol rather than racing an exact instant.
        drop(held);
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        let _reacquired = loop {
            match acquire_step_lock(Path::new(RIGGER_DIR)) {
                Ok(f) => break f,
                Err(e) => {
                    assert!(
                        std::time::Instant::now() < deadline,
                        "after the first releases, a later step must acquire cleanly; still \
                         refused at the backoff deadline (last refusal: {e})"
                    );
                    std::thread::sleep(std::time::Duration::from_millis(10));
                }
            }
        };
    }

    /// The no-runs message single-sourced for both the absent-db and empty-stream
    /// edges must actually point the user at `rigger run` - a pinned, greppable
    /// contract so the two edges can never drift apart or lose the next-step hint.
    #[test]
    fn no_runs_message_points_at_rigger_run() {
        assert!(NO_RUNS_MESSAGE.contains("rigger run"), "{NO_RUNS_MESSAGE}");
        assert!(NO_RUNS_MESSAGE.contains("no runs"), "{NO_RUNS_MESSAGE}");
    }

    /// Append `events` to `project`'s namespaced `run` stream in the sqlite db at
    /// `path` - the exact stream and namespace the conductor writes its run to, so a
    /// `stats_lines` read sees them exactly as it would a real run. Returns nothing;
    /// the db file now exists with the events committed.
    fn seed_run(path: &str, project: &str, events: &[rigger::eventstore::Event]) {
        use rigger::eventstore::ExpectedRevision;
        let backend = Store::open(path).expect("open sqlite backend");
        let store = Namespaced::new(&backend, project);
        store
            .append(conductor::STREAM, ExpectedRevision::Any, events)
            .expect("append run events");
    }

    /// `stats_lines` against an absent `events.db` returns `None` (the "no runs yet"
    /// signal) and - critically - does NOT create the file. Opening would create it
    /// and mask a never-run project as an empty one, so the guard must precede the open.
    #[test]
    fn stats_lines_absent_db_returns_none_and_creates_no_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("events.db");
        let path_str = path.to_str().unwrap();

        let out = stats_lines(path_str, "proj-x", false, &StoreSelection::Sqlite)
            .expect("absent db is not an error");
        assert!(out.is_none(), "an absent db must read as no runs (None)");
        assert!(
            !path.exists(),
            "stats_lines must not create events.db when it is absent"
        );
    }

    /// `stats_lines` against an existing db whose namespaced `run` stream is empty
    /// returns `None`. This is the db-exists-but-no-run edge: another command (or
    /// another project sharing the backend) created the file, but this project has no
    /// run. It must read as "no runs yet", not a zeroed/empty table.
    #[test]
    fn stats_lines_existing_db_with_empty_run_stream_returns_none() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("events.db");
        let path_str = path.to_str().unwrap();

        // Create the db file via the real store path, but leave "proj-me"'s run stream
        // empty (append zero events still opens/creates the backing file).
        seed_run(path_str, "proj-me", &[]);
        assert!(path.exists(), "the db file must exist for this edge");

        let out = stats_lines(path_str, "proj-me", false, &StoreSelection::Sqlite)
            .expect("empty run stream is not an error");
        assert!(
            out.is_none(),
            "an existing db with an empty run stream must read as no runs (None)"
        );
    }

    /// The read is scoped to the per-project namespace: a run that ANOTHER project
    /// wrote to the SAME shared backend must not leak into this project's stats. With
    /// the backend holding `proj-other`'s run, `proj-me`'s `stats_lines` still reads
    /// `None` - proving the [`Namespaced`] decorator (`proj-<project>-run`) is on the
    /// read path, not just the write path.
    #[test]
    fn stats_lines_does_not_read_another_projects_namespaced_run() {
        use rigger::eventstore::Event;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("events.db");
        let path_str = path.to_str().unwrap();

        // proj-other has a real run in the shared backend.
        seed_run(
            path_str,
            "proj-other",
            &[Event::new("UnitStarted", b"{}".to_vec())],
        );

        // proj-me, reading the same file, sees its OWN (empty) namespace - no runs.
        let mine = stats_lines(path_str, "proj-me", false, &StoreSelection::Sqlite)
            .expect("read is not an error");
        assert!(
            mine.is_none(),
            "stats must be namespace-scoped: another project's run must not leak in"
        );

        // Sanity: the other project's run IS visible to it, so the data really is there
        // and the None above is the namespace boundary, not a read failure.
        let theirs = stats_lines(path_str, "proj-other", false, &StoreSelection::Sqlite)
            .expect("read is not an error");
        assert!(
            theirs.is_some(),
            "the project that owns the run must see its stats"
        );
    }

    /// A populated namespaced run reads back through `stats_lines` as the rendered
    /// metric lines - the positive case that pins the read-fold-format path end to end
    /// against a real on-disk db (not just the pure formatter), and that the events the
    /// fold sees came back through the namespace with their clean stream name.
    #[test]
    fn stats_lines_existing_run_renders_metric_lines() {
        use rigger::eventstore::Event;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("events.db");
        let path_str = path.to_str().unwrap();

        seed_run(
            path_str,
            "proj-me",
            &[
                Event::new("UnitStarted", br#"{"id":"u1"}"#.to_vec()),
                Event::new("UnitIntegrated", br#"{"id":"u1"}"#.to_vec()),
            ],
        );

        let lines = stats_lines(path_str, "proj-me", false, &StoreSelection::Sqlite)
            .expect("read is not an error")
            .expect("a populated run must render lines, not None");
        let out = lines.join("\n");
        assert!(
            out.contains("run stats:"),
            "a populated run must render the stats header:\n{out}"
        );
        assert!(
            out.contains("first-pass yield"),
            "a populated run must render the first-pass yield metric:\n{out}"
        );
        assert!(
            out != NO_RUNS_MESSAGE,
            "a populated run must not print the no-runs message"
        );
    }

    /// `result_of_at` (the read behind `rigger reported`, and the same latest-result read
    /// `spawn::record_result_if_absent` consults) treats an absent `events.db` as UNREPORTED
    /// (`None`) and does NOT create the file: a never-run project has no result for any spawn,
    /// and opening would create the db, masking the edge. A `None` here makes `rigger reported`
    /// exit non-zero, correctly reporting the spawn as still unanswered.
    #[test]
    fn result_of_at_absent_db_reads_as_unreported_and_creates_no_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("events.db");
        let path_str = path.to_str().unwrap();

        let got = result_of_at(path_str, "proj-x", "u/impl#0", &StoreSelection::Sqlite)
            .expect("absent db is not an error");
        assert!(got.is_none(), "an absent db must read as unreported (None)");
        assert!(
            !path.exists(),
            "result_of_at must not create events.db when it is absent"
        );
    }

    /// A spawn with no recorded result reads as UNREPORTED (`None`) even when the db exists and
    /// holds OTHER events (including other spawns' results): `result_of_at` matches on the exact
    /// spawn id, so an unanswered spawn is correctly treated as still-parked.
    #[test]
    fn result_of_at_unrecorded_spawn_reads_as_unreported() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("events.db");
        let path_str = path.to_str().unwrap();

        // A different spawn HAS a result; the one we ask about does not.
        seed_run(
            path_str,
            "proj-me",
            &[spawn::SpawnResult::ok("u/other#0", "done")
                .to_event()
                .unwrap()],
        );

        let got = result_of_at(path_str, "proj-me", "u/impl#0", &StoreSelection::Sqlite)
            .expect("read is not an error");
        assert!(
            got.is_none(),
            "a spawn with no result of its own must read as unreported (None)"
        );
    }

    /// A recorded self-report reads back as `Some` - the anti-clobber invariant the review
    /// rejected the unguarded death courier for. A worker that self-reported (success OR its own
    /// failure) is ANSWERED, so `rigger reported` exits 0 and the death courier's atomic
    /// `rigger result <id> --if-absent --error` records nothing: the worker's own result is
    /// never overwritten by a courier `--error`.
    #[test]
    fn result_of_at_reads_a_self_reported_result_so_it_is_not_clobbered() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("events.db");
        let path_str = path.to_str().unwrap();

        seed_run(
            path_str,
            "proj-me",
            &[
                spawn::SpawnResult::ok("u/impl#0", "implemented and reported")
                    .to_event()
                    .unwrap(),
            ],
        );

        let got = result_of_at(path_str, "proj-me", "u/impl#0", &StoreSelection::Sqlite)
            .expect("read is not an error")
            .expect("a recorded result must read back as Some, not None");
        assert_eq!(got.id, "u/impl#0");
        assert!(
            !got.is_error(),
            "a self-reported success must read back as a success (so the guard skips --error)"
        );
        assert_eq!(got.output, "implemented and reported");
    }

    /// The read is namespace-scoped: a result ANOTHER project wrote to the same shared backend
    /// must not make this project's spawn look reported. Proves the [`Namespaced`] decorator is
    /// on the guard's read path, so a spawn id colliding across projects cannot cross-answer.
    #[test]
    fn result_of_at_is_namespace_scoped() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("events.db");
        let path_str = path.to_str().unwrap();

        // proj-other recorded a result for an id that ALSO exists in proj-me's run.
        seed_run(
            path_str,
            "proj-other",
            &[spawn::SpawnResult::ok("u/impl#0", "theirs")
                .to_event()
                .unwrap()],
        );

        // proj-me, reading the same file, sees its OWN (empty) namespace: still unreported.
        let mine = result_of_at(path_str, "proj-me", "u/impl#0", &StoreSelection::Sqlite)
            .expect("read is not an error");
        assert!(
            mine.is_none(),
            "another project's result must not leak in - the read must be namespace-scoped"
        );

        // Sanity: the owner DOES see it, so the None above is the namespace boundary, not a miss.
        let theirs = result_of_at(path_str, "proj-other", "u/impl#0", &StoreSelection::Sqlite)
            .expect("read is not an error");
        assert!(
            theirs.is_some(),
            "the project that owns the result must see it"
        );
    }

    /// `cmd_reported` validates its arg count BEFORE any store I/O: exactly one spawn id is
    /// required, so a typo (zero args, or extra args) is a clear error rather than a silent
    /// read of the wrong thing. The single-id read path itself is covered by `result_of_at`
    /// (the testable seam), which `cmd_reported` wraps for I/O + identity + the exit decision.
    #[test]
    fn cmd_reported_requires_exactly_one_id() {
        let none = cmd_reported(&[]).expect_err("no id must be a clear error");
        assert!(
            none.to_string().contains("rigger reported <id>"),
            "the no-id error must show the usage; got: {none}"
        );
        let extra = cmd_reported(&["a".to_string(), "b".to_string()])
            .expect_err("extra args must be a clear error");
        assert!(
            extra.to_string().contains("rigger reported <id>"),
            "the extra-args error must show the usage; got: {extra}"
        );
    }

    // --- Spec 44, criterion 3: the always-on dash is SESSION-DETACHED from `rigger step` ---

    /// Read the process-group id (PGID / `pgrp`) of `pid` from `/proc/<pid>/stat`. Pure std, so
    /// it holds on BOTH feature lanes. `/proc/<pid>/stat` is `pid (comm) state ppid pgrp ...`;
    /// `comm` may itself contain spaces and parens, so we split AFTER the last `)` - the tokens
    /// that follow are then `state ppid pgrp ...`, making `pgrp` the third whitespace token. A
    /// zombie (an exited-but-unreaped child) still has a readable `stat`, so this is race-free
    /// against the child having already exited; only a fully reaped pid is gone.
    #[cfg(target_os = "linux")]
    fn pgid_of(pid: u32) -> u32 {
        let stat = std::fs::read_to_string(format!("/proc/{pid}/stat"))
            .unwrap_or_else(|e| panic!("read /proc/{pid}/stat: {e}"));
        let after_comm = stat
            .rsplit_once(')')
            .expect("/proc stat has a parenthesised comm field")
            .1;
        after_comm
            .split_whitespace()
            .nth(2)
            .expect("/proc stat has a pgrp field after comm")
            .parse()
            .expect("pgrp is a base-10 integer")
    }

    /// The load-bearing detachment proof: `detach_process_group` puts a spawned child in its OWN
    /// process group - a group whose PGID equals the child's own PID (it is the group leader) and
    /// which DIFFERS from this test process's process group. That different group is exactly what
    /// lets the detached dash survive the teardown of the parent `rigger step` command's process
    /// group (spec 44): a group-scoped teardown of the parent's group never reaches the child's
    /// own group. Uses a controlled, fully-reaped `sleep` child so the test is deterministic and
    /// leaks nothing.
    #[cfg(target_os = "linux")]
    #[test]
    fn detach_process_group_places_the_child_in_its_own_process_group() {
        let parent_pgid = pgid_of(std::process::id());

        let mut cmd = Command::new("sleep");
        cmd.arg("30")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        detach_process_group(&mut cmd);
        let mut child = cmd.spawn().expect("spawn a controlled child");
        let child_pgid = pgid_of(child.id());

        assert_eq!(
            child_pgid,
            child.id(),
            "a detached child is its OWN process-group leader (PGID == its PID)"
        );
        assert_ne!(
            child_pgid, parent_pgid,
            "a detached child is in a DIFFERENT process group than its parent - so a teardown of \
             the parent command's process group cannot reap it"
        );

        let _ = child.kill();
        let _ = child.wait();
    }

    /// The control that makes the assertion above meaningful: WITHOUT `detach_process_group`, a
    /// spawned child INHERITS the parent's process group. So the detachment is load-bearing - it
    /// is precisely what moves the child out of `rigger step`'s group. If this ever failed
    /// (child already in its own group with no detach), the detached-case assertion would prove
    /// nothing.
    #[cfg(target_os = "linux")]
    #[test]
    fn a_child_spawned_without_detachment_inherits_the_parent_process_group() {
        let parent_pgid = pgid_of(std::process::id());

        let mut child = Command::new("sleep")
            .arg("30")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn a controlled child");
        let child_pgid = pgid_of(child.id());

        assert_eq!(
            child_pgid, parent_pgid,
            "a child spawned WITHOUT detachment stays in the parent's process group - the very \
             group whose teardown would otherwise reap the dash; detach_process_group is what \
             breaks it out"
        );

        let _ = child.kill();
        let _ = child.wait();
    }

    /// End-to-end wiring: the dash actually spawned by `spawn_run_dashboard_detached` is placed
    /// in its own process group (PGID == the spawned pid, a different group than this parent), so
    /// the production step path really does session-detach the always-on dash (spec 44 criterion
    /// 3), not merely the seam in isolation. The spawned child is deliberately un-reaped - being
    /// un-reaped across steps is the whole point of "detached" - and the OS reaps this transient
    /// child when the test process exits.
    #[cfg(target_os = "linux")]
    #[test]
    fn spawn_run_dashboard_detached_session_detaches_the_dash() {
        let parent_pgid = pgid_of(std::process::id());

        let marker = spawn_run_dashboard_detached().expect("spawn the detached dash");
        let dash_pgid = pgid_of(marker.pid);

        assert_eq!(
            dash_pgid, marker.pid,
            "the spawned dash is its own process-group leader (PGID == its PID)"
        );
        assert_ne!(
            dash_pgid, parent_pgid,
            "the spawned dash is in a DIFFERENT process group than the step process that spawned \
             it - so tearing down the step command's process group does not reap the dash"
        );
    }

    // --- Spec 60, criterion 5: what `rigger reset --derived` SAYS about what it did ---

    /// A prune report with `removed` rows spread over the shipped derived types and the given
    /// reclamation state. Built from the real type list so a fifth derived type cannot leave this
    /// pinning a report shape nothing renders.
    ///
    /// `compaction_ran` is a parameter of its own rather than inferred from `reclaimed`, for the
    /// same reason the report reads it rather than inferring it: a file that was not rewritten
    /// and a rewrite that reclaimed nothing are both `Some(0)` and are different states. So is
    /// `on_disk_measured`, for the same reason again one level down: a rewrite whose bytes could
    /// not be measured because a reader declined the checkpoint and one whose bytes never existed
    /// because the database has no file are BOTH a rewritten file with no number, and only the
    /// caller of the prune knows which.
    fn report_of(
        removed: usize,
        compaction_ran: bool,
        reclaimed: Option<u64>,
        on_disk_measured: bool,
        failure: Option<&str>,
    ) -> String {
        let types = rigger::ingest::DERIVED_INDEX_TYPES;
        let pruned = PrunedDerived {
            removed: types
                .iter()
                .enumerate()
                .map(|(i, t)| (t.to_string(), if i == 0 { removed } else { 0 }))
                .collect(),
            reclaimed_bytes: reclaimed,
            compaction_ran,
            on_disk_measured,
            compaction_error: failure.map(str::to_string),
        };
        derived_prune_report(&pruned)
    }

    /// Spec 60, criterion 5: a compaction that failed AFTER the deletes committed is reported, not
    /// swallowed into an error that says only that something went wrong.
    ///
    /// The rows are gone from the log by then. An operator told only "error" cannot tell that from
    /// a prune that never ran, so they cannot know whether to run it again, and they never see the
    /// per-type counts the command exists to give them. The line therefore carries the counts, the
    /// failure by name, and the two things that follow from the ordering: the deletes are durable
    /// and a re-run is safe.
    #[test]
    fn a_compaction_that_failed_after_the_deletes_is_reported_beside_the_counts() {
        let out = report_of(7, true, None, true, Some("database or disk is full"));
        assert!(
            out.contains("pruned 7 redundant derived-index event(s)"),
            "a failed compaction must not cost the operator the counts; got {out:?}"
        );
        assert!(
            out.contains("database or disk is full"),
            "the report must NAME the failure, or an operator cannot act on it; got {out:?}"
        );
        for (fact, needle) in [
            ("say the deletes survived it", "deletes are committed"),
            ("say a re-run is safe", "re-running"),
        ] {
            assert!(
                out.contains(needle),
                "the failed-compaction report must {fact} ({needle:?}); got {out:?}"
            );
        }
        assert!(
            !out.contains("byte(s) on disk"),
            "a compaction that failed reclaimed nothing it can put a number on; got {out:?}"
        );
    }

    /// Spec 60, criterion 5: a prune that shed nothing says the file was left as it stands, and
    /// justifies itself by WHAT THIS LOG HOLDS - never by WHEN the log was written.
    ///
    /// A log written since the ingest dedup existed does NOT always prune to zero: a file whose
    /// content returns to a generation the log already recorded re-records that whole batch by
    /// design, so "written after the dedup" implies nothing about the count. Justifying the zero
    /// report that way is the sentence an operator uses to decide whether a NON-zero prune means
    /// the dedup is broken, so it has to be a statement about the log in front of them.
    #[test]
    fn a_prune_that_shed_nothing_is_justified_by_this_log_not_by_when_it_was_written() {
        let out = report_of(0, false, Some(0), true, None);
        for (fact, needle) in [
            ("say WHY nothing was shed", "no redundancy to shed"),
            ("say the report is the EXPECTED one", "expected report"),
            ("say it is not a failure", "not a failed prune"),
            (
                "say the file was not rewritten",
                "left exactly as it stands",
            ),
        ] {
            assert!(
                out.contains(needle),
                "the report on a clean log must {fact} ({needle:?}); got {out:?}"
            );
        }
        assert!(
            !out.contains("written since"),
            "the zero report must not rest on WHEN the log was written: a log written since the \
             dedup existed still re-records a file's batch whenever its content returns to a \
             generation the log already held, so that reasoning would make a perfectly correct \
             non-zero prune look like a broken dedup. Got {out:?}"
        );
    }

    /// Spec 60, criterion 5: "the file was left alone" is a statement about THE REWRITE, not about
    /// the row count - so a pass that deleted nothing and DID rewrite the file says so.
    ///
    /// This is the pass an operator reaches by following the failed-reclamation report's own
    /// advice: the first run's deletes committed and its rewrite failed, so the re-run sheds no
    /// rows and reclaims the space that was left behind. A report that read "nothing was deleted"
    /// as "nothing was rewritten" would tell that operator their log was untouched by the very
    /// run that compacted it, and would make the advice look like it had done nothing.
    #[test]
    fn a_pass_that_deleted_nothing_but_reclaimed_space_reports_the_reclamation() {
        let out = report_of(0, true, Some(8192), true, None);
        assert!(
            out.contains("reclaimed 8192 byte(s) on disk"),
            "the re-run's reclamation is what the operator was told to run for; got {out:?}"
        );
        assert!(
            !out.contains("left exactly as it stands"),
            "a run that rewrote the file must never say it left it alone - that is the sentence \
             an operator checks the advice against; got {out:?}"
        );
    }

    /// Spec 60, criterion 5: a prune that DID shed rows explains why a deduplicated log still had
    /// something to shed, and carries none of the clean-log clause.
    #[test]
    fn a_prune_that_shed_rows_explains_the_duplication_a_deduplicated_log_still_accumulates() {
        let out = report_of(12, true, Some(4096), true, None);
        for (fact, needle) in [
            ("name the shape that re-records a batch", "RETURNS"),
            ("give the operator the ordinary cause", "revert"),
            (
                "say it is not a broken dedup",
                "not a sign the ingest dedup is broken",
            ),
        ] {
            assert!(
                out.contains(needle),
                "a non-zero prune must {fact} ({needle:?}), or an operator reads it as the dedup \
                 having failed; got {out:?}"
            );
        }
        for needle in [
            "no redundancy to shed",
            "expected report",
            "not a failed prune",
        ] {
            assert!(
                !out.contains(needle),
                "the clean-log clause must not print on a prune that shed rows ({needle:?}); got \
                 {out:?}"
            );
        }
        assert!(
            out.contains("reclaimed 4096 byte(s) on disk"),
            "a measured reclamation is reported as the measurement it is; got {out:?}"
        );
    }

    /// Spec 60, criterion 5: an unmeasured reclamation has TWO causes, and the report may only
    /// name the one it was actually told about.
    ///
    /// `reclaimed_bytes: None` with the rewrite having run means either "a concurrent reader held
    /// the write-ahead log so the checkpoint was declined" or "this database has no file behind
    /// it, so there were never any bytes on disk to measure" - and the store yields the SAME
    /// `(no error, rewritten, no bytes)` triple for both. Rendering a concurrent reader for the
    /// second is the report asserting a cause it was never handed: it sends an operator looking
    /// for a reader that does not exist, and tells them pages will land at a checkpoint that will
    /// never move a byte onto a disk this database does not use. `on_disk_measured` is the fact
    /// that separates them, so it is carried beside the count rather than guessed at from it.
    #[test]
    fn an_unmeasurable_database_is_not_reported_as_a_checkpoint_a_reader_declined() {
        let no_file = report_of(5, true, None, false, None);
        let declined = report_of(5, true, None, true, None);

        assert!(
            !no_file.contains("concurrent reader"),
            "a database with no file behind it was never told a reader held anything - naming one \
             invents the cause; got {no_file:?}"
        );
        assert!(
            !no_file.contains("next checkpoint"),
            "and there is no checkpoint that will land bytes on a disk this database does not \
             write to; got {no_file:?}"
        );
        assert!(
            no_file.contains("no file behind it"),
            "the report must say WHY the figure is missing: the database has no file on disk to \
             measure; got {no_file:?}"
        );
        assert!(
            no_file.contains("pruned 5 redundant derived-index event(s)"),
            "and an unmeasurable reclamation must not cost the operator the counts; got \
             {no_file:?}"
        );

        assert!(
            declined.contains("concurrent reader"),
            "the OTHER cause of the same triple still reads as itself - this is the arm the file \
             case must not be folded into; got {declined:?}"
        );
        assert_ne!(
            no_file, declined,
            "the two causes of an unmeasured reclamation must not render to one sentence, or the \
             distinction is carried and then thrown away"
        );
    }

    // --- Spec 71, criterion 2: COMPACTION REFUSES LIVE WRITERS ---

    fn no_live_units() -> std::collections::HashSet<String> {
        std::collections::HashSet::new()
    }

    /// The pure core (spec 71, criterion 2): all four facts quiet is the only quiet state.
    #[test]
    fn live_writer_reasons_is_empty_only_when_all_four_facts_are_quiet() {
        assert!(
            live_writer_reasons(false, &no_live_units(), &[], 0).is_empty(),
            "nothing live must draw no reason"
        );
        assert!(!live_writer_reasons(true, &no_live_units(), &[], 0).is_empty());
        assert!(!live_writer_reasons(
            false,
            &std::collections::HashSet::from(["rigger/u/a".to_string()]),
            &[],
            0
        )
        .is_empty());
        assert!(
            !live_writer_reasons(false, &no_live_units(), &["a/implementer#0".to_string()], 0)
                .is_empty()
        );
        assert!(!live_writer_reasons(false, &no_live_units(), &[], 1).is_empty());
    }

    /// A held step lock is named by exactly what it is, and the refusal points at the override -
    /// with a phrase that OWNS the risk, not merely the bare flag token (spec 71: "an explicit
    /// override flag whose help text owns the risk").
    #[test]
    fn refusal_names_a_held_step_lock_and_the_force_live_override_owning_the_risk() {
        let reasons = live_writer_reasons(true, &no_live_units(), &[], 0);
        let out = live_writer_refusal(&reasons);
        assert!(
            out.contains("step.lock"),
            "must name the held lock; got {out:?}"
        );
        assert!(
            out.contains("--force-live"),
            "must name the override; got {out:?}"
        );
        assert!(
            out.contains("reset --derived"),
            "must name the refused command; got {out:?}"
        );
        assert!(
            out.contains("corruption"),
            "must OWN the risk, not just name the flag; got {out:?}"
        );
    }

    /// A unit that is still non-terminal - live BETWEEN spawn rounds, with no spawn currently in
    /// flight - is named by its slug, distinctly from an in-flight spawn id.
    #[test]
    fn refusal_names_a_non_terminal_unit_between_spawn_rounds() {
        let live_units = std::collections::HashSet::from(["rigger/u/a".to_string()]);
        let reasons = live_writer_reasons(false, &live_units, &[], 0);
        let out = live_writer_refusal(&reasons);
        assert!(
            out.contains('a') && out.contains("not yet terminal"),
            "must name the non-terminal unit; got {out:?}"
        );
    }

    /// In-flight spawns are named individually by id, and the count is stated.
    #[test]
    fn refusal_names_every_in_flight_spawn_id_and_the_count() {
        let ids = vec!["a/implementer#0".to_string(), "b/reviewer#1".to_string()];
        let reasons = live_writer_reasons(false, &no_live_units(), &ids, 0);
        let out = live_writer_refusal(&reasons);
        assert!(
            out.contains("a/implementer#0") && out.contains("b/reviewer#1"),
            "must name BOTH in-flight spawn ids; got {out:?}"
        );
        assert!(
            out.contains('2'),
            "must state the count of in-flight spawns; got {out:?}"
        );
    }

    /// A live driver registration is named by its count and the mechanism (spec 50) it comes from.
    #[test]
    fn refusal_names_the_driver_registration_count() {
        let reasons = live_writer_reasons(false, &no_live_units(), &[], 3);
        let out = live_writer_refusal(&reasons);
        assert!(
            out.contains('3') && out.contains("registration"),
            "must state the registration count; got {out:?}"
        );
    }

    /// ALL applicable reasons are named together, not just the first found - so an operator sees
    /// the whole picture in one refusal instead of clearing one and retrying into the next.
    #[test]
    fn refusal_names_every_applicable_reason_together_not_just_the_first() {
        let live_units = std::collections::HashSet::from(["rigger/u/a".to_string()]);
        let ids = vec!["b/reviewer#1".to_string()];
        let reasons = live_writer_reasons(true, &live_units, &ids, 1);
        let out = live_writer_refusal(&reasons);
        assert!(out.contains("step.lock"), "must still name the lock");
        assert!(out.contains('a'), "must still name the non-terminal unit");
        assert!(out.contains("b/reviewer#1"), "must still name the spawn");
        assert!(
            out.contains("registration"),
            "must still name the registration"
        );
    }

    /// A malformed event in the current run's slice (the `Err(_)` sentinel of the in-flight-spawn
    /// read) makes the guard REFUSE rather than silently read as quiet - the fail-safe direction
    /// spec 71 requires: an unreadable signal is never treated as "nobody is here".
    #[test]
    fn refuse_derived_reset_if_live_fails_safe_on_a_malformed_spawn_event() {
        let dir = tempfile::tempdir().unwrap();
        let rigger_dir = dir.path().join(RIGGER_DIR);
        std::fs::create_dir_all(&rigger_dir).unwrap();
        let loc = StoreLocation {
            dir: rigger_dir.clone(),
        };
        let identity = loc.identity();
        let db = rigger_dir.join("events.db").to_string_lossy().into_owned();
        let backend = rigger::eventstore::sqlite::Store::open(&db).unwrap();
        let store = Namespaced::new(&backend, &identity);
        // A malformed SpawnRequested body: valid JSON but missing the fields `spawn::recorded`
        // needs, so decoding it fails and `spawn::step_result` returns `Err`.
        store
            .append(
                conductor::STREAM,
                ExpectedRevision::Any,
                &[Event::new(spawn::TYPE_SPAWN_REQUESTED, b"{}".to_vec())],
            )
            .unwrap();
        drop(store);
        drop(backend);

        let err = refuse_derived_reset_if_live(&loc, &StoreSelection::Sqlite, None)
            .expect_err("a malformed spawn event must refuse, never read as quiet");
        assert!(
            !err.to_string().is_empty(),
            "the refusal must carry a message an operator can act on"
        );
    }

    /// `reset_modes` accepts `--force-live` alongside `--derived`, at most once, and it never
    /// implies a mode on its own - a bare `--force-live` still falls through the existing "at
    /// least one mode" refusal exactly as before this flag existed.
    #[test]
    fn reset_modes_parses_force_live_alongside_derived_rejects_duplicates_and_never_implies_a_mode()
    {
        let modes = reset_modes(&["--derived".to_string(), "--force-live".to_string()])
            .expect("--derived --force-live must parse");
        assert!(modes.derived && modes.force_live && !modes.runs);

        let err = match reset_modes(&["--force-live".to_string(), "--force-live".to_string()]) {
            Err(e) => e,
            Ok(_) => panic!("a duplicate --force-live must be refused"),
        };
        assert!(err.to_string().contains("more than once"), "got {err}");

        let err = match reset_modes(&["--force-live".to_string()]) {
            Err(e) => e,
            Ok(_) => panic!("--force-live alone names no mode"),
        };
        assert!(
            err.to_string().contains("at least one mode"),
            "a bare --force-live must fall through the same 'at least one mode' refusal as a \
             bare reset; got {err}"
        );
    }

    // --- Spec 69, criterion 2: THE WATCHDOG (`rigger watch --once`, wired end to end) ---

    /// Open a fresh sqlite store at a tempdir'd [`StoreLocation`], returning it alongside the
    /// project identity `watch_poll` will scope to. Mirrors
    /// `refuse_derived_reset_if_live_fails_safe_on_a_malformed_spawn_event`'s own setup, so
    /// `watch_poll` is exercised with an INJECTED location - never the process cwd.
    fn watch_test_store() -> (tempfile::TempDir, StoreLocation, String) {
        let dir = tempfile::tempdir().unwrap();
        let rigger_dir = dir.path().join(RIGGER_DIR);
        std::fs::create_dir_all(&rigger_dir).unwrap();
        let loc = StoreLocation {
            dir: rigger_dir.clone(),
        };
        let identity = loc.identity();
        (dir, loc, identity)
    }

    #[test]
    fn watch_once_on_a_clean_store_reports_no_anomalies() {
        let (_dir, loc, identity) = watch_test_store();
        let db = loc.file("events.db");
        {
            let backend = Store::open(&db).unwrap();
            let store = Namespaced::new(&backend, &identity);
            store
                .append(
                    conductor::STREAM,
                    ExpectedRevision::Any,
                    &[
                        Event::new(ledger::TYPE_UNIT_STARTED, br#"{"id":"u"}"#.to_vec()),
                        Event::new(
                            ledger::TYPE_UNIT_INTEGRATED,
                            br#"{"id":"u","commit":"c"}"#.to_vec(),
                        ),
                    ],
                )
                .unwrap();
        }
        let anomalies = watch_poll(&loc, &StoreSelection::Sqlite).unwrap();
        assert!(
            anomalies.is_empty(),
            "a clean store must report no anomalies: {anomalies:?}"
        );
    }

    #[test]
    fn parse_watch_args_defaults_to_streaming_with_the_default_interval() {
        let a = parse_watch_args(&[]).unwrap();
        assert!(!a.once);
        assert_eq!(a.interval_secs, watch::DEFAULT_INTERVAL_SECS);
    }

    /// The loop must run to completion over MULTIPLE arguments, in either flag order,
    /// consuming each flag's own width (`--once` is 1, `--interval <s>` is 2) - a
    /// mutated loop-continuation comparison either stops after the first flag or
    /// walks past the slice, so a single-flag input cannot tell the arms apart.
    #[test]
    fn parse_watch_args_accepts_once_and_interval_together_in_either_order() {
        let a = parse_watch_args(&["--interval".to_string(), "5".to_string()]).unwrap();
        assert!(!a.once);
        assert_eq!(a.interval_secs, 5);

        let a = parse_watch_args(&[
            "--interval".to_string(),
            "5".to_string(),
            "--once".to_string(),
        ])
        .unwrap();
        assert!(a.once, "--interval then --once must still set once");
        assert_eq!(a.interval_secs, 5);

        let a = parse_watch_args(&[
            "--once".to_string(),
            "--interval".to_string(),
            "7".to_string(),
        ])
        .unwrap();
        assert!(a.once, "--once then --interval must still set once");
        assert_eq!(a.interval_secs, 7);
    }

    #[test]
    fn parse_watch_args_rejects_a_non_integer_interval_a_missing_value_and_an_unknown_flag() {
        assert!(parse_watch_args(&["--interval".to_string(), "soon".to_string()]).is_err());
        assert!(parse_watch_args(&["--interval".to_string()]).is_err());
        assert!(parse_watch_args(&["--bogus".to_string()]).is_err());
    }

    /// The headline scenario (spec 69, Done-when "a test proves THE WATCHDOG"): a store
    /// seeded with a multi-result spawn, an escalated unit, a unit at reject-recurrence
    /// three, and an out-of-order tail. `rigger watch --once` (here, `watch_poll` - the
    /// function `cmd_watch` calls with no further logic between it and stdout) must print
    /// one line per anomaly naming signal, subject, and response.
    #[test]
    fn watch_once_on_the_seeded_store_reports_one_line_per_anomaly() {
        let (_dir, loc, identity) = watch_test_store();
        let db = loc.file("events.db");
        {
            let backend = Store::open(&db).unwrap();
            let store = Namespaced::new(&backend, &identity);
            // An escalated unit.
            store
                .append(
                    conductor::STREAM,
                    ExpectedRevision::Any,
                    &[
                        Event::new(ledger::TYPE_UNIT_STARTED, br#"{"id":"u-esc"}"#.to_vec()),
                        Event::new(ledger::TYPE_UNIT_ESCALATED, br#"{"id":"u-esc"}"#.to_vec()),
                    ],
                )
                .unwrap();
            // A unit at reject-recurrence three, same cause each time.
            for attempt in 1..=3u32 {
                store
                    .append(
                        conductor::STREAM,
                        ExpectedRevision::Any,
                        &[Event::new(
                            ledger::TYPE_UNIT_STARTED,
                            br#"{"id":"u-fail"}"#.to_vec(),
                        )],
                    )
                    .unwrap();
                store
                    .append(
                        conductor::STREAM,
                        ExpectedRevision::Any,
                        &[Event::new(
                            ledger::TYPE_UNIT_FAILED,
                            format!(r#"{{"id":"u-fail","attempts":{attempt},"cause":"gate:fmt"}}"#)
                                .into_bytes(),
                        )],
                    )
                    .unwrap();
            }
            // A spawn answered three times without the run advancing.
            for _ in 0..3 {
                store
                    .append(
                        conductor::STREAM,
                        ExpectedRevision::Any,
                        &[Event::new(
                            spawn::TYPE_SPAWN_RESULT,
                            br#"{"id":"u-stall/implementer#0"}"#.to_vec(),
                        )],
                    )
                    .unwrap();
            }
            // A healthy, unrelated stream that will be corrupted below.
            store
                .append(
                    "watch-test-ooo",
                    ExpectedRevision::Any,
                    &[
                        Event::new("E", vec![0]),
                        Event::new("E", vec![1]),
                        Event::new("E", vec![2]),
                    ],
                )
                .unwrap();
        }

        // An out-of-order tail (spec 71's own corruption signature): delete the
        // namespaced stream's revision-0 row and reissue it at the newest position -
        // exactly what a stale (pre-append-guard) writer would do, and exactly the
        // shape `Store::append` itself refuses, so it can only be reproduced by going
        // around it with a raw connection - mirrors
        // `append_refuses_a_stream_whose_position_order_and_revision_order_already_
        // disagree` (src/eventstore/sqlite.rs).
        let scoped_ooo_stream = format!(
            "{}watch-test-ooo",
            rigger::eventstore::namespace::Namespaced::prefix_for(&identity)
        );
        {
            let conn = rusqlite::Connection::open(&db).unwrap();
            conn.execute(
                "DELETE FROM events WHERE stream = ?1 AND revision = 0",
                [&scoped_ooo_stream],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO events (stream, type, id, data, meta, valid_from, recorded_at, \
                 revision) VALUES (?1, 'E', 'reissued', X'00', '{}', 0, 0, 0)",
                [&scoped_ooo_stream],
            )
            .unwrap();
        }

        let anomalies = watch_poll(&loc, &StoreSelection::Sqlite).unwrap();
        let signals: Vec<watch::Signal> = anomalies.iter().map(|a| a.signal).collect();
        assert_eq!(
            signals,
            vec![
                watch::Signal::Escalated,
                watch::Signal::RejectRecurrence,
                watch::Signal::FrontierStall,
                watch::Signal::StoreIntegrity,
            ],
            "one line per anomaly, in Design order: {anomalies:?}"
        );
        // Each line names its signal, subject, and response - never a bare fact.
        let by_signal = |s: watch::Signal| anomalies.iter().find(|a| a.signal == s).unwrap();
        let esc = by_signal(watch::Signal::Escalated).line();
        assert!(esc.contains("escalated blockers") && esc.contains("u-esc"));
        assert!(esc.contains("rigger-handle-an-escalation"));
        let rr = by_signal(watch::Signal::RejectRecurrence).line();
        assert!(rr.contains("reject-recurrence trend") && rr.contains("u-fail"));
        assert!(rr.contains("rigger-diagnose-churn"));
        let fs = by_signal(watch::Signal::FrontierStall).line();
        assert!(fs.contains("frontier progress") && fs.contains("u-stall/implementer#0"));
        assert!(fs.contains("stop the driver and diagnose"));
        let si = by_signal(watch::Signal::StoreIntegrity).line();
        assert!(si.contains("store integrity") && si.contains("watch-test-ooo"));
        assert!(si.contains(watch::ORDER_SIGNATURE_REPAIR_DOC_REF));
    }

    /// `rigger watch`'s signal set covers every signal `rigger-watch-a-run` names (spec 69
    /// Done-when), pinned against the same [`watch::SKILL_SIGNAL_NAMES`] the pure `detect`
    /// tests pin against - a superset relation (store integrity is the automation's own
    /// sixth check), never equality.
    #[test]
    fn the_watchdog_command_signal_set_covers_every_signal_the_watch_skill_names() {
        let command_signals: std::collections::BTreeSet<&str> = [
            watch::Signal::Escalated,
            watch::Signal::DeadDriver,
            watch::Signal::DashNotServing,
            watch::Signal::RejectRecurrence,
            watch::Signal::FrontierStall,
            watch::Signal::StoreIntegrity,
        ]
        .iter()
        .map(|s| s.name())
        .collect();
        for skill_signal in watch::SKILL_SIGNAL_NAMES {
            assert!(
                command_signals.contains(skill_signal),
                "the watchdog command must cover skill signal {skill_signal:?}; got \
                 {command_signals:?}"
            );
        }
    }

    /// `watch_poll` must ACTUALLY PROBE a recorded dash marker's port over a real socket
    /// (`dash::dash_serving_on`), not merely thread the marker through unexamined: a marker
    /// naming a port nothing answers on (the process is gone / the port was never bound) is
    /// reported as [`watch::Signal::DashNotServing`], naming the dead pid and port, exactly
    /// as `rigger-restore-the-dash` diagnoses.
    #[test]
    fn watch_once_reports_dash_not_serving_when_the_marker_names_a_dead_holder() {
        let (_dir, loc, _identity) = watch_test_store();
        // A port nothing listens on: bind an ephemeral port, then drop the listener,
        // freeing it - a probe against it afterward gets connection-refused, exactly
        // the "hung holder is gone" case the marker/probe split exists to catch.
        let dead_port = {
            let l = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
            l.local_addr().unwrap().port()
        };
        let marker_path = std::path::PathBuf::from(loc.file(DASH_MARKER_FILE));
        dash::DashMarker {
            port: dead_port,
            pid: 999_999,
        }
        .write(&marker_path)
        .unwrap();

        let anomalies = watch_poll(&loc, &StoreSelection::Sqlite).unwrap();
        assert_eq!(anomalies.len(), 1, "got: {anomalies:?}");
        let a = &anomalies[0];
        assert_eq!(a.signal, watch::Signal::DashNotServing);
        assert!(a.detail.contains("999999"), "detail: {}", a.detail);
        assert!(
            a.detail.contains(&dead_port.to_string()),
            "detail: {}",
            a.detail
        );
        assert!(a.line().contains("rigger-restore-the-dash"));
    }

    /// The [`watch::DashProbe::Serving`] counterpart: a marker naming a port a REAL
    /// rigger-dash-shaped listener answers on (carrying [`dash::DASH_HEADER`], the exact
    /// marker `dash_serving_on` itself checks for) reports NO anomaly - the probe's actual
    /// result gates the branch, not just whether a marker is present.
    #[test]
    fn watch_once_reports_no_anomaly_when_the_dash_marker_names_a_real_serving_holder() {
        use std::io::Write as _;
        let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        std::thread::spawn(move || {
            for mut s in listener.incoming().flatten() {
                let _ = s.write_all(
                    format!(
                        "HTTP/1.1 200 OK\r\n{}: probe\r\nConnection: close\r\n\r\n",
                        dash::DASH_HEADER
                    )
                    .as_bytes(),
                );
            }
        });

        let (_dir, loc, _identity) = watch_test_store();
        let marker_path = std::path::PathBuf::from(loc.file(DASH_MARKER_FILE));
        dash::DashMarker {
            port,
            pid: std::process::id(),
        }
        .write(&marker_path)
        .unwrap();

        let anomalies = watch_poll(&loc, &StoreSelection::Sqlite).unwrap();
        assert!(
            anomalies.is_empty(),
            "a marker naming a real serving dash must report no anomaly: {anomalies:?}"
        );
    }

    // --- Spec 68, criterion 3: the bare-menu report lines (pure, no store, no live server) ---

    #[test]
    fn runs_menu_line_names_the_measured_counts_and_the_flag() {
        let line = runs_menu_line(&PruneStats {
            nodes: 4,
            superseded_edges: 2,
        });
        assert!(
            line.contains("--runs:"),
            "must name its own flag; got {line:?}"
        );
        assert!(
            line.contains("4 dead-run node(s)") && line.contains("2 superseded edge(s)"),
            "must name the measured counts; got {line:?}"
        );
        assert!(
            line.contains("--runs"),
            "must tell the operator which flag reclaims it; got {line:?}"
        );

        let zero = runs_menu_line(&PruneStats::default());
        assert!(
            zero.contains("0 dead-run node(s)") && zero.contains("0 superseded edge(s)"),
            "an empty store must report zero, not omit the line; got {zero:?}"
        );
    }

    #[test]
    fn derived_menu_line_sums_the_measured_duplicate_counts_and_names_the_flag() {
        let counts = vec![
            ("CodeEntityExtracted".to_string(), 3usize),
            ("EdgeInferred".to_string(), 0usize),
            ("DocLinkExtracted".to_string(), 5usize),
        ];
        let line = derived_menu_line(&StoreSelection::Sqlite, Some(&counts));
        assert!(
            line.contains("--derived:"),
            "must name its own flag; got {line:?}"
        );
        assert!(
            line.contains("8 duplicate event(s)"),
            "must sum the per-type counts (3+0+5=8); got {line:?}"
        );
        assert!(
            line.contains("3 derived type(s)"),
            "must name how many types were measured; got {line:?}"
        );
        assert!(
            line.contains("--derived"),
            "must tell the operator which flag compacts it; got {line:?}"
        );

        let zero = derived_menu_line(&StoreSelection::Sqlite, Some(&[]));
        assert!(
            zero.contains("0 duplicate event(s)"),
            "an empty store must report zero, not omit the line; got {zero:?}"
        );
    }

    /// The per-backend honesty branch (spec 68 Design: "a backend where a prune is unavailable
    /// says so on that line"). `StoreSelection` is private to this module, so this is the ONE
    /// place able to construct `Server(..)` directly and prove the wording without a live
    /// server - `derived_menu_line` never opens a connection either way.
    #[test]
    fn derived_menu_line_on_a_server_backend_says_so_instead_of_a_fabricated_count() {
        let server = StoreSelection::Server("esdb://127.0.0.1:2113?tls=false".to_string());
        let line = derived_menu_line(&server, None);
        assert!(
            !line.contains("duplicate event(s)"),
            "a backend that cannot compact must never print a count it could not measure; got {line:?}"
        );
        assert!(
            line.contains("--derived:") && line.contains("unavailable"),
            "must name its own flag and say it is unavailable; got {line:?}"
        );
        assert!(
            line.contains("server-backed store"),
            "must name the backend the project is actually configured for; got {line:?}"
        );
    }

    /// Spec 73, criterion 1. The implementer persona (`.rigger/agents/rust-engineer.md`) is
    /// OPERATOR CONFIGURATION seeded by the operator, not authored by any unit (spec 73
    /// Design: "the grounder cannot ground non-code files, so no unit can own a Markdown
    /// blast radius"). So this is a DRIFT GUARD, not a feature test: it pins the seeded
    /// mutation-STEP contract - WHEN the instrument runs and HOW a missed mutant is resolved -
    /// against the committed file, so an edit that drops or weakens that contract fails the
    /// suite instead of silently drifting. The ACCOUNTING shape (the `DecisionMade` entry
    /// format, the diff base, the total, the empty-diff case) is criterion 2's drift guard,
    /// NOT this one's, and is deliberately not asserted here.
    #[test]
    fn implementer_persona_pins_the_seeded_mutation_step_contract() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join(RIGGER_DIR)
            .join("agents")
            .join("rust-engineer.md");
        let persona = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("read committed {}: {e}", path.display()));
        // Whitespace-normalize before matching (collapse newlines/indentation to single
        // spaces), matching criterion 2's established drift-guard pattern (d-u73c2-accounting-
        // drift-guard-approach): the committed persona wraps this paragraph across markdown
        // list-continuation lines, so a raw substring match is fragile to a pure reflow
        // (identical words, different line wrap) and would false-fail or false-pass around a
        // line break.
        let normalized = persona.split_whitespace().collect::<Vec<_>>().join(" ");

        // One contiguous-phrase check, not two independently-satisfiable fragments: a
        // decomposed persona that keeps "Mutation efficacy" and "build.mutation" as bare
        // substrings in unrelated sentences (destroying the gating relation - the step
        // runs only WHEN the config key is on) must fail this test, not pass it.
        assert!(
            normalized.contains("Mutation efficacy (when `build.mutation` is on)"),
            "the step must be gated on the build.mutation config key, as one contiguous \
             gating clause, not two independently-satisfiable fragments; got:\n{normalized}"
        );
        // One contiguous-phrase check, not two independently-satisfiable fragments: a
        // decomposed persona that keeps both bare substrings in unrelated sentences
        // (destroying the after-tests-green-before-pre-gate-commit placement relation
        // criterion 1's own Done-when bullet names) must fail this test, not pass it.
        assert!(
            normalized.contains("After your unit tests are green and BEFORE the pre-gate commit"),
            "the step must run after unit-green and before the pre-gate commit, as one \
             contiguous relational clause, not two independently-satisfiable fragments; \
             got:\n{normalized}"
        );
        assert!(
            normalized.contains("diff against the unit's merge-base with the run branch"),
            "the mutants run must be scoped to a diff against the unit's merge-base with the \
             run branch; got:\n{normalized}"
        );
        // One contiguous-phrase check, not two independently-satisfiable fragments: a
        // decomposed persona that keeps "cargo mutants --in-diff" and "DEFAULT feature
        // lane" as bare substrings while running the invocation on some OTHER lane (or
        // every lane) would still satisfy two independent `contains` calls, so the
        // invocation and the lane it runs on must be pinned as one relation.
        assert!(
            normalized.contains(
                "cargo mutants --in-diff unit.diff --timeout-multiplier 1.5 -j 2` on the \
                 DEFAULT feature lane"
            ),
            "the step must name the diff-scoped cargo-mutants invocation tied to running on \
             the default feature lane, as one contiguous clause, not two independently- \
             satisfiable fragments; got:\n{normalized}"
        );
        // One contiguous-phrase check naming the either-or relation itself, not two bare
        // keywords: a decomposed persona that keeps "KILLED" and "JUSTIFIED" as unrelated
        // words (e.g. "always JUSTIFIED ... and never KILLED") would still satisfy two
        // independent `contains` calls despite inverting the disjunction.
        assert!(
            normalized.contains(
                "is either KILLED by a strengthened test or JUSTIFIED with a concrete \
                 equivalence reason"
            ),
            "a missed mutant must be resolved by an explicit kill-or-justify disjunction, as \
             one contiguous either-or clause, not two independent bare keywords; \
             got:\n{normalized}"
        );
        // The consequence itself, not just the "unjustified miss" keyword: an inversion that
        // keeps the words "unjustified miss" but reverses the outcome (e.g. "is merely noted
        // in the log, and the unit may still be marked done") must fail this test.
        assert!(
            normalized.contains("an unjustified miss means the unit is not done"),
            "an unjustified missed mutant must leave the unit not done - the consequence \
             clause itself, not merely the presence of the words \"unjustified miss\"; \
             got:\n{normalized}"
        );
    }
}
