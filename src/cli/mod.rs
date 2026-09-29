use crate::*;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::process::Stdio;

use rigger::budget::BuildBudget;
use rigger::canary;
use rigger::canary_store;
use rigger::community;
use rigger::concepts;
use rigger::conductor::{self, Deps};
use rigger::config;
use rigger::config_store;
use rigger::console;
use rigger::contextgraph::{
    self,
    sqlite::{Projector, PruneStats},
    Located, Projection,
};
use rigger::dash;
use rigger::driver::cli;
use rigger::driver::replay::{
    cache_home_from, mutation_scratch_path, mutation_scratch_root, spawn_scratch_path, ReplayDriver,
};
use rigger::eventstore::namespace::Namespaced;
use rigger::eventstore::{
    sqlite::{DerivedPreview, PrunedDerived, Store},
    Direction, Event, EventStore, ExpectedRevision, Filter, Position,
};
use rigger::gate::{
    resolve_build_layer, resolved_cache_dir, BuildEnv, ExecRunner, Gate, GateResult, Runner,
    MUTATION_GATE_ID, STORE_FENCE_ENV,
};
use rigger::grounder::Grounder;
use rigger::instructions;
use rigger::ledger::{self, RunState};
use rigger::metrics::{self, Metrics};
use rigger::playbooks::fnv1a_64;
use rigger::run as runscope;
use rigger::run_store as runscope_store;
use rigger::sidecar::{PeerDecision, Sidecar};
use rigger::spawn::SpawnEvent;
use rigger::worktree::{RunBranchSetup, Worktree};
use rigger::{hooks, mcpserver, playbooks, progress, spawn, spawn_store, spec, subprocess, watch};

use rigger::config::RIGGER_DIR;

mod dashboard;
mod eval;
mod graph;
mod guard;
mod hygiene;
mod observe;
mod run;
mod setup;
mod validate;
pub(crate) use dashboard::*;
pub(crate) use eval::*;
pub(crate) use graph::*;
pub(crate) use guard::*;
pub(crate) use hygiene::*;
pub(crate) use observe::*;
pub(crate) use run::*;
pub(crate) use setup::*;
pub(crate) use validate::*;

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
const SHIM_MJS: &str = include_str!("../../shim/shim.mjs");
const SHIM_PACKAGE_JSON: &str = include_str!("../../shim/package.json");
const SHIM_PACKAGE_LOCK_JSON: &str = include_str!("../../shim/package-lock.json");

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
const RIGGER_WORKFLOW: &str = include_str!("../../workflows/rigger.js");

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

/// Where `rigger docs` writes the rendered planning field guide handbook page, relative to
/// the project root (spec 66, criterion 2): the failure-catalog companion to
/// `authoring-loops.md`'s shape rules, cross-linked from there. Drift-checked the same way
/// as [`HANDBOOK_DISCIPLINE_REL`]. The single source of this path.
const PLANNING_FIELD_GUIDE_REL: &str = "docs/handbook/planning-field-guide.md";

/// One [`HANDBOOK_PAGES`] entry: the page's committed rel path and the pure render
/// function that produces its fresh content. Named so clippy's `type_complexity` lint
/// stays clean and so [`write_docs`]/[`docs_drift`] read as "a rel path and a renderer",
/// not an inline tuple type.
type HandbookPageEntry = (&'static str, fn(&rigger::docs::DocsContext) -> String);

/// Every handbook page `rigger docs` renders and the docs-drift gate checks, OUTSIDE the
/// skill registry (these are handbook chapters, not installable skills - see
/// [`rigger::docs::skill_registry`] for those). [`write_docs`] and [`docs_drift`] both walk
/// this ONE list, in this order, so adding a page here is the only step needed to render
/// and drift-check it (spec 66, criterion 2: generalizes the formerly-single-page
/// mechanism, mirroring the registry's "one enumeration every surface walks" shape).
const HANDBOOK_PAGES: &[HandbookPageEntry] = &[
    (
        HANDBOOK_DISCIPLINE_REL,
        rigger::docs::render_handbook_discipline,
    ),
    (
        PLANNING_FIELD_GUIDE_REL,
        rigger::docs::render_planning_field_guide,
    ),
];

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
    /// escalation - whose spec is unchanged; see [`rigger::run_store::start_fresh`].
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

/// Open this project's `graph.db` for `command`, whose answer depends on the fold (spec 101): a
/// file folded under an older fold rule is refused at once, naming `rigger setup` - the one
/// command that rebuilds it - never waited on, rebuilt or folded into here. The open itself writes
/// nothing to such a file.
fn open_graph(
    graph_db: &str,
    project: &str,
    command: &str,
) -> Result<Projector, Box<dyn std::error::Error>> {
    let graph = Projector::open(graph_db, project)?;
    if graph.rebuild_owed()? {
        return Err(format!("{command}: {}", contextgraph::REBUILD_OWED).into());
    }
    Ok(graph)
}

/// What a read-only surface (graph inspection, `rigger validate`, the dashboard) says when this
/// project's `graph.db` owes its rebuild (spec 101): it answers from the projection as it stands,
/// and names the command that pays the rebuild. `None` when there is no file (never creating
/// one), when it owes nothing, or when it cannot be read.
fn graph_rebuild_owed_note(graph_db: &str, project: &str) -> Option<String> {
    if !Path::new(graph_db).exists() {
        return None;
    }
    let owed = Projector::open(graph_db, project)
        .ok()?
        .rebuild_owed()
        .ok()?;
    owed.then(|| {
        format!(
            "note: {} - until then the context graph answers as it stands",
            contextgraph::REBUILD_OWED
        )
    })
}

/// Open this project's `graph.db` for a read-only inspection: it answers from the projection as
/// it stands, saying on stderr first when that projection owes its rebuild
/// ([`graph_rebuild_owed_note`]). The open writes nothing to such a file.
fn open_graph_to_read(
    graph_db: &str,
    project: &str,
) -> Result<Projector, Box<dyn std::error::Error>> {
    if let Some(note) = graph_rebuild_owed_note(graph_db, project) {
        eprintln!("{note}");
    }
    Ok(Projector::open(graph_db, project)?)
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
/// exact NotFound-vs-other split [`config_store::read_store_config`] makes one rung down
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
    let cwd = cwd();
    main_repo_root(&cwd).unwrap_or(cwd).join(RIGGER_DIR)
}

/// The process's working directory, or `.` when it cannot be read.
fn cwd() -> PathBuf {
    std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
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
    let cfg = config_store::read_store_config(rigger_dir)?;
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

/// Refresh THIS invocation's entry in the machine-global instance registry (spec 50) as a
/// ONE-SHOT re-stamp - the courier counterpart to [`register_run_instance`]'s heartbeat thread
/// (spec 62, "couriers count as activity"). A courier command (`progress`, `emit`, `result`)
/// advances a run but is short-lived: it has no long-lived scope to hold a [`RunRegistration`]
/// guard over, so it re-stamps the SAME entry a live driver's heartbeat thread would - same
/// file, by [`rigger::registry::Instance::id`], keyed off the resolved store's root and store
/// identity - once, right here, and returns. This is what keeps an instance whose only activity
/// for a stretch is courier traffic from aging out of discovery mid-run: every invocation that
/// starts OR advances a run now refreshes the heartbeat, not just the ones that hold a driver
/// scope.
///
/// `loc` and `selection` are the SAME resolved [`StoreLocation`] / [`StoreSelection`] the caller
/// already has in hand from [`require_store_dir`] - never re-resolved here, so a courier's
/// registry entry is always keyed to the exact store its real work just wrote to. `loc.identity()`
/// (bound to the RESOLVED root, not the process cwd) is used for the `project` label rather than
/// the ambient [`project_identity`], for the same reason [`StoreLocation::identity`]'s own doc
/// comment gives: a courier can run from a cwd that is not the store's owner (a nested worktree).
///
/// BEST-EFFORT and warn-only, mirroring [`register_run_instance`]'s degrade exactly: a homeless
/// environment (no resolvable state home) or a write error never fails, slows, or warns away the
/// courier's real work beyond a single stderr line - the registry's loss is harmless (spec 50).
///
/// FENCE-AWARE (spec 70 criterion 3): when [`STORE_FENCE_ENV`] is set - a unit-worktree gate's
/// spawned test process, the same condition [`require_store_dir`] itself checks first - this is a
/// complete no-op before [`rigger::registry::default_dir`] is even consulted. That resolver reads
/// `XDG_STATE_HOME`/`HOME` directly and is NOT scoped by `loc`/`selection`, so without this check
/// a fenced courier's registry write would still land in the real, machine-global registry even
/// though its store write correctly landed in the fenced scratch dir - reopening the exact ambient
/// side channel the fence exists to close.
fn refresh_registry_entry(loc: &StoreLocation, selection: &StoreSelection) {
    // The gate store fence (spec 70 criterion 3), mirroring `require_store_dir`'s own check
    // (same env var, same "checked first, before touching any ambient state" placement): a
    // fenced courier's STORE already resolves to the pinned scratch dir via `require_store_dir`,
    // but `rigger::registry::default_dir()` below reads `XDG_STATE_HOME`/`HOME` directly and is
    // entirely decoupled from `loc`/`selection` - so without this check, a courier a fenced
    // gate's own spawned test process runs (the exact call shape spec 70 criterion 3 names) would
    // still write a real Instance into the machine-global registry, reopening precisely the
    // ambient side channel that fence exists to close. A fenced gate sees strictly less ambient
    // state, never more - the registry is ambient state, so a fenced courier skips it entirely
    // rather than trying to refresh a fenced/scratch registry no discovery consumer ever reads.
    if let Ok(fenced) = std::env::var(STORE_FENCE_ENV) {
        if !fenced.trim().is_empty() {
            return;
        }
    }
    let Some(dir) = rigger::registry::default_dir() else {
        return; // homeless environment: degrade to a no-op, exactly like register_run_instance
    };
    // `loc.dir` is always `<root>/.rigger` (see `StoreLocation`/`require_store_dir`); a missing
    // parent is the same pathological case `StoreLocation::identity` already degrades from.
    let Some(root) = loc.dir.parent() else {
        return;
    };
    let inst = rigger::registry::Instance {
        project: loc.identity(),
        root: root.to_string_lossy().into_owned(),
        store: registry_store_identity(selection, root),
        heartbeat_ms: rigger::registry::now_ms(),
    };
    if let Err(e) = rigger::registry::write(&dir, &inst) {
        eprintln!("rigger: instance registry refresh skipped ({e}); discovery is unaffected");
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
    project_identity_at(&cwd())
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
/// definition - the `.rigger/workflow.yml`, the FULL agent-prompt set (every
/// `.rigger/agents/*.md`, which carries each agent's prompt and frontmatter) and the operator
/// instruction layer (every `.rigger/instructions/*.md`, absent means empty) - canonicalized
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
    push_definition_section(&mut buf, "workflow.yml", &workflow);
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
        push_definition_section(&mut buf, &format!("agent:{name}"), &content);
    }
    // The operator instruction layer, through the ONE loader that decides which files are
    // instructions (filename order); an absent directory contributes nothing.
    for ins in config_store::load_instructions(Path::new(dir))
        .map_err(|e| format!("definition hash: {e}"))?
    {
        push_definition_section(&mut buf, &format!("instruction:{}.md", ins.name), &ins.body);
    }
    Ok(format!("{:016x}", fnv1a_64(buf.as_bytes())))
}

/// Fold one tagged, canonicalized definition file into the [`definition_hash`] buffer.
fn push_definition_section(buf: &mut String, tag: &str, content: &str) {
    buf.push_str(tag);
    buf.push('\n');
    buf.push_str(&canonical_definition_text(content));
    buf.push('\n');
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
/// move as a `DecisionMade` (no new event types). Returns `Some((n, emitted))` with the stream
/// count and what became of that decision (its position and fold) when it migrated, `None` when there was nothing to do (idempotent on re-open), and an
/// `Err` naming BOTH identities when the store is ambiguous (history under both namespaces).
/// Takes the identities as arguments so it is unit-testable against an in-memory store.
fn migrate_project_identity(
    backend: &Store,
    minted: &str,
    legacy: &str,
    graph: Option<&Projector>,
) -> Result<Option<(usize, mcpserver::Emitted)>, Box<dyn std::error::Error>> {
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
            let emitted = mcpserver::emit_event(
                &store,
                conductor::STREAM,
                || mcpserver::wired(graph.map(|g| g as &dyn Projection)),
                &args,
            )
            .map_err(|e| -> Box<dyn std::error::Error> { e.into() })?;
            Ok(Some((n, emitted)))
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
    if let Some((n, decision)) = migrate_project_identity(&backend, &minted, &legacy, Some(&graph))?
    {
        eprintln!(
            "rigger: migrated project identity - renamed {n} stream(s) from the legacy \
             namespace {legacy:?} to the minted identity {minted:?} (.rigger/{PROJECT_ID_FILE}); \
             recorded its decision (position {}){}",
            decision.position,
            fold_clause(&decision.fold)
        );
    }
    Ok(())
}

/// The one CLI spelling of an event on the log that the context graph does not hold, and why.
fn not_folded(why: &str) -> String {
    format!("not folded into the context graph: {why}")
}

/// What a command that appended an event adds to its own report line about that event's fold,
/// so no command claims a fold that did not happen or stays silent about one that failed.
fn fold_clause(fold: &contextgraph::Fold) -> String {
    match fold {
        contextgraph::Fold::Folded => " and folded it into the context graph".to_string(),
        contextgraph::Fold::NotFolded(why) => format!("; {}", not_folded(why)),
    }
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
/// of THIS project, and an unbounded walk lets a courier in a storeless nested repo bind
/// to a PARENT project's store and write into a foreign run stream with exit-0 success
/// (adversary finding adv9-walkup-cross-project, empirically proven). Outside any git
/// context there is no sanctioned walk at all: only `start` itself counts.
///
/// TWO shapes reach the boundary (spec 89, criterion 2 - SCRATCH IS OUTSIDE THE STORE
/// TREE): `start` is a filesystem DESCENDANT of the boundary (the pre-relocation nested
/// worktree, `<repo>/.rigger/tmp/rigger-wt-<slug>`, still reachable for a caller that
/// configures scratch back under the repo) - the ORIGINAL plain `.parent()` climb,
/// terminating the instant it reaches the boundary (inclusive), unchanged; or `start` is
/// NOT a descendant at all (the relocated cache-home worktree a real spawn now runs its
/// courier calls from) - climbing `start`'s own physical ancestors in that case would
/// walk into cache-home territory with NO governing relationship to this repo (reopening
/// the exact adv9-walkup-cross-project hazard the bound exists to close: an unrelated
/// project's - or a leftover fixture's - store sitting at some ancestor of the cache
/// home). The sanctioned set there is exactly `{start, boundary}` - no ancestors between
/// them are ever consulted, mirroring the "outside git context" case's own "only `start`
/// counts" discipline for the part of the path this repo does not govern.
fn walk_stores_from(start: &Path) -> StoreWalk {
    let boundary = main_repo_root(start);
    let mut found: Vec<PathBuf> = Vec::new();
    // The nested-vs-relocated classification needs an apples-to-apples comparison, but
    // `main_repo_root` can return a path carrying literal `..` segments (a RELATIVE
    // `git rev-parse --git-common-dir` output for a plain subdirectory of the SAME repo
    // joined onto `start`, never resolved - harmless for that shape's own git-worktree
    // admin files, which always store an ABSOLUTE common-dir, but not for this component-
    // wise prefix check). Canonicalized ONLY for this decision, on throwaway copies -
    // `main_repo_root`'s own return value (every other caller's `boundary`) is untouched,
    // and the walk below still uses the original, uncanonicalized `start`/`boundary`
    // throughout. A `start` that cannot be canonicalized (does not exist on disk) falls
    // back to `nested = true`, the ORIGINAL unconditional ancestor climb every existing
    // caller already relies on.
    let nested = match (
        start.canonicalize(),
        boundary.as_deref().map(Path::canonicalize),
    ) {
        (Ok(s), Some(Ok(b))) => s.starts_with(&b),
        _ => true,
    };
    if boundary.is_none() || nested {
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
    } else {
        // `start` lives outside the boundary entirely: check it (a local shadow, e.g. a
        // tracked-but-storeless `.rigger/`) and the boundary itself, nearest-first,
        // touching nothing in between.
        let start_rigger = start.join(RIGGER_DIR);
        if start_rigger.join("events.db").is_file() {
            found.push(start_rigger);
        }
        if let Some(root) = &boundary {
            let root_rigger = root.join(RIGGER_DIR);
            if root_rigger.join("events.db").is_file() {
                found.push(root_rigger);
            }
        }
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
    let out = subprocess::git_in(start)
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

/// STEP RESOLVES THE MAIN WORKTREE (spec 89, criterion 4): `rigger step`, `rigger run`
/// (both the default CLI driver and `--driver workflow`, i.e. `run_cli` and `run_workflow` -
/// the latter also `rigger serve`'s sole implementation) and `rigger workflow` each call this
/// at their own entry so a LINKED git worktree is refused up front, naming both trees, instead
/// of silently proceeding on the linked tree's own toplevel
/// and later failing deep inside branch setup with git's own opaque "'rigger-run' is already
/// used by worktree ..." (a linked worktree cannot itself hold the run branch checked out - git
/// already holds it there in the main tree). Evidence (2026-09-11): the driver stopped after 28
/// waves for exactly this reason, when a courier's `rigger step` ran with its cwd drifted into
/// a unit worktree.
///
/// Compares [`main_repo_root`] (the git-common-dir-derived main checkout, correct even from
/// inside a linked worktree) against `cwd`'s own `git rev-parse --show-toplevel` (which returns
/// the LINKED tree when run from inside one): equal - `cwd` already IS the main tree, return it
/// unchanged; a real repo where they differ - refuse; no repo reachable at all -
/// `Ok(String::new())`, preserving the existing repo-less unit-test path every caller already
/// guards its own repo-only logic on.
fn resolve_main_worktree_or_refuse(cwd: &Path, command: &str) -> Result<String, String> {
    let toplevel = git_repo_at(cwd);
    if toplevel.is_empty() {
        return Ok(String::new());
    }
    let Some(main_root) = main_repo_root(cwd) else {
        return Ok(toplevel);
    };
    let main_canon = std::fs::canonicalize(&main_root).unwrap_or_else(|_| main_root.clone());
    let top_canon =
        std::fs::canonicalize(Path::new(&toplevel)).unwrap_or_else(|_| PathBuf::from(&toplevel));
    if main_canon == top_canon {
        return Ok(toplevel);
    }
    Err(format!(
        "{command}: refusing to run from inside a linked worktree ({linked}) - the main \
         worktree is {main}. A linked worktree cannot itself hold the run branch checked out \
         (git already holds it there in the main tree), so continuing here would fail deep \
         inside branch setup with a raw git error instead of this one; re-run `{command}` from \
         the main worktree ({main}).",
        linked = top_canon.display(),
        main = main_canon.display(),
    ))
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
    /// This store's context graph, opened for one fold: the one opener every command that appends
    /// and then folds hands [`contextgraph::Fold::of`] (directly or through
    /// [`mcpserver::emit_event`]), so each folds into the `graph.db` its store owns, under the
    /// store's identity.
    fn graph(&self) -> Result<Box<dyn Projection>, contextgraph::Error> {
        Projector::open(&self.file("graph.db"), &self.identity())
            .map(|g| Box::new(g) as Box<dyn Projection>)
    }

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

    /// The repo root this store lives under (spec 83, criterion 2): the SAME OWNING root
    /// [`identity`](Self::identity) binds to (the parent of the resolved `.rigger/`), NEVER
    /// the process's raw cwd (`git_repo()`). A courier invoked from a nested unit worktree -
    /// the documented, walk-up-supported shape [`require_store_dir`] exists for - has a
    /// DIFFERENT git toplevel than the main repo a driver's `rigger step` (always run from the
    /// repo root) stamped a spawn's liveness marker under; resolving the scratch root from
    /// that raw cwd instead of this owning root is exactly how `rigger status`/`rigger watch`
    /// used to read back "no marker" for an agent that was demonstrably alive and heartbeating
    /// (spec 83's Problem statement). Empty when `dir` has no parent (pathological - the
    /// resolved dir is always `<root>/.rigger`), mirroring [`identity`](Self::identity)'s own
    /// fallback shape.
    fn repo_root(&self) -> String {
        self.dir
            .parent()
            .and_then(|p| p.to_str())
            .map(String::from)
            .unwrap_or_default()
    }
}

/// The shared `defaults.workdir`/`defaults.max_retries` resolver EVERY production reader of
/// those two fields delegates to (spec 83 criterion 2): [`cmd_status`], [`watch_poll`],
/// [`reclaim_spawn_scratch`], and [`cmd_scratch`] (round 2 + round 3's `loc`, resolved by
/// [`require_store_dir`]'s owning-root walk), plus [`cmd_dash`] and `cmd_replay` (round 3,
/// each passing a `StoreLocation` built from ITS OWN pre-existing repo/cwd resolution -
/// `cmd_dash`'s raw process cwd and `cmd_replay`'s [`git_repo`] - unchanged by this function;
/// see each caller's own doc comment for why). Every caller reads via
/// [`config_store::read_scratch_defaults`], NEVER [`config::load`]: `config::load` additionally
/// requires a fully loadable `.rigger/agents/` fleet AND a passing [`config::Config::validate`]
/// just to learn two string/int fields - this project's own committed `.rigger/workflow.yml`
/// sets `build.mutation: on`, which `validate` rejects whenever `cargo-mutants` is off PATH,
/// so ANY environment invoking one of these commands without it on PATH used to silently lose
/// a configured `defaults.workdir` (and `defaults.max_retries`) via each call site's own
/// `.unwrap_or_default()` over `config::load`'s `Err`. Separately, for the FOUR `loc`-from-
/// `require_store_dir` callers, `loc.dir` is `<owning-root>/.rigger` - the SAME owning root
/// [`StoreLocation::repo_root`] resolves the scratch root from, never a nested unit
/// worktree's own cwd (round 1's original defect). [`config_store::read_scratch_defaults`] requires
/// neither a loadable fleet nor a passing validate, so it can never regress on either axis.
/// Absent/unreadable resolves to `("", 0)`, matching [`config_store::read_scratch_workdir`]'s own
/// tolerant-absent contract.
fn scratch_defaults(loc: &StoreLocation) -> (String, u32) {
    let d = config_store::read_scratch_defaults(&loc.dir).unwrap_or_default();
    (d.workdir, d.max_retries)
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

/// Marker error: [`require_store_dir`] found no `.rigger/events.db` at or above the cwd (spec
/// 94 criterion 5 round 2 - the checkin adjudication reject on `adv-u94c5-setup-wires-a-
/// command-that-hard-fails-with-no-run-yet`). `cmd_status`'s `--line` path downcasts on this
/// SPECIFIC type, never a string match on the message it carries, to degrade to a graceful
/// placeholder instead of the CLI's hard error; every other caller of `require_store_dir`
/// (and every OTHER failure `--line` itself can hit) still propagates it exactly as the plain
/// error it replaces - only the message text moved into a named type, byte-for-byte.
#[derive(Debug, thiserror::Error)]
#[error("{0}")]
struct NoStoreFound(String);

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
        Box::new(NoStoreFound(format!(
            "no rigger store found: neither {} nor any parent directory has an initialized \
             {RIGGER_DIR}/events.db. This usually means the command ran from the wrong \
             directory (e.g. a unit worktree, whose {RIGGER_DIR} is not the run's store). \
             Run it from the project root that owns the run; refusing to fabricate a fresh \
             empty store here.",
            cwd.display()
        )))
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

/// Reap any process rooted in `dir` (spec 23), then remove the dir. The reap runs BEFORE the
/// removal so no process outlives the dir holding a now-deleted cwd; both halves are
/// best-effort and never fail the step. `authorized_root` (spec 78 round 2, decision
/// `u78c2r2-authorized-root-caller-supplied`) is the SAME resolved root the caller already
/// used to build `dir` - never re-derived here - so this reap is safe on any relocated
/// scratch root (`RIGGER_TMPDIR`/`defaults.workdir`) or registered mutation-scratch root
/// under a cache home, and still never touches a process outside `authorized_root`. Off a
/// platform without `/proc` the reap is a graceful no-op and only the removal runs. This is
/// the shared teardown for the fixpoint scratch-area sweep in [`cmd_step`]; the
/// worktree-removal reap point is [`rigger::worktree::Worktree::remove`], which authorizes
/// its own reap differently (git identity, not containment - see its own doc comment).
fn reap_then_remove_dir(dir: &std::path::Path, authorized_root: &std::path::Path) {
    rigger::reap::reap_processes_rooted_under(dir, authorized_root);
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
/// `authorized_root` is the resolved scratch root `dir` was enumerated from (spec 78 round 2).
fn reap_then_remove_worktree(repo: &str, dir: &std::path::Path, authorized_root: &std::path::Path) {
    rigger::reap::reap_processes_rooted_under(dir, authorized_root);
    let deregistered = !repo.is_empty()
        && subprocess::git_in(repo)
            .args(["worktree", "remove", "--force"])
            .arg(dir)
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);
    if !deregistered {
        let _ = std::fs::remove_dir_all(dir);
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
    let Some((events, _)) = with_project_store(path, project, sel, |store| {
        runscope::read::read_current_run(store, conductor::STREAM)
    })?
    else {
        return Ok(None);
    };
    Ok(spawn::result_of(&events, id).map_err(|e| e.to_string())?)
}

/// Run `run_id`'s progress reports out of `project`'s namespace of the progress store
/// `backend` - its own per-run stream alone ([`progress::read_run`], spec 101), the one progress
/// read `rigger status` and the dash snapshot fold. An unreadable stream is no progress.
fn read_run_progress(backend: &dyn EventStore, project: &str, run_id: &str) -> Vec<Event> {
    progress::read_run(&Namespaced::new(backend, project), run_id).unwrap_or_default()
}

/// `project`'s namespaced `stream`, read forward from revision 0 out of the store `sel`
/// resolves at `path` - `None` when that store is an embedded sqlite file that does not exist
/// yet. The absence check runs BEFORE [`Store::open`], which (via `Connection::open`) would
/// otherwise create the file and mask a never-run project as an empty one. The
/// [`Namespaced`] read scopes to `proj-<project>-<stream>`, so an event another project wrote
/// never leaks in.
fn read_project_stream(
    path: &str,
    project: &str,
    stream: &str,
    sel: &StoreSelection,
) -> Result<Option<Vec<Event>>, Box<dyn std::error::Error>> {
    with_project_store(path, project, sel, |store| {
        store.read_stream(stream, 0, Direction::Forward)
    })
}

/// `read` over `project`'s namespaced view of the store `sel` resolves at `path` - `None` when
/// that store is an embedded sqlite file that does not exist yet, checked BEFORE [`Store::open`],
/// which would otherwise create the file and mask a never-run project as an empty one.
fn with_project_store<T>(
    path: &str,
    project: &str,
    sel: &StoreSelection,
    read: impl FnOnce(&dyn EventStore) -> Result<T, rigger::eventstore::Error>,
) -> Result<Option<T>, Box<dyn std::error::Error>> {
    if sel.is_sqlite() && !Path::new(path).exists() {
        return Ok(None);
    }
    let backend = resolve_store(sel, path)?;
    let store = Namespaced::new(backend.as_ref(), project);
    Ok(Some(read(&store)?))
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
    let provisioned = rigger_path(root, SHIM_DIR).join("shim.mjs");
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
    // The conductor projects its run state from STREAM read forward from revision 0
    // (inclusive); read the same stream the same way so the metrics fold sees exactly
    // the run the conductor drove, scoped to this project's namespace.
    let events = read_project_stream(path, project, conductor::STREAM, sel)?.unwrap_or_default();
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
    append_spawn_timing(&mut lines, m);
    lines
}

/// Render the spec-61 SPAWN TIMING panel: per-agent (spawn-role) wall-clock duration
/// aggregates - count, total, and mean - derived from pairing each recorded spawn request
/// with its result by `(run window, spawn id)`, plus how many recorded requests were
/// excluded: a dead worker (no same-window result), or a paired result whose duration was
/// non-positive (suspect - a same-batch append or clock skew) - either way excluded from
/// every aggregate above and reported here as its own count.
fn append_spawn_timing(lines: &mut Vec<String>, m: &Metrics) {
    if m.spawn_timing.is_empty() && m.unpaired_spawns == 0 {
        lines.push("  spawn timing       (no recorded spawns)".to_string());
        return;
    }
    lines.push("  spawn timing per agent (mean / count / total):".to_string());
    for (role, t) in &m.spawn_timing {
        lines.push(format!(
            "    {role:<20} {} avg / {} spawns / {} total",
            fmt_duration(t.mean()),
            t.count,
            fmt_duration(t.total),
        ));
    }
    if m.unpaired_spawns > 0 {
        lines.push(format!(
            "    ({} unpaired spawn request(s) excluded above - no recorded result (dead \
             worker), or a paired result with a suspect non-positive duration)",
            m.unpaired_spawns,
        ));
    }
}

/// Render a [`std::time::Duration`] as whole seconds with one decimal place (e.g.
/// `"12.3s"`), the single formatting authority the spawn-timing panel uses so a duration
/// never renders two different ways.
fn fmt_duration(d: std::time::Duration) -> String {
    format!("{:.1}s", d.as_secs_f64())
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

/// Render a [`metrics::CanaryMetrics`] scorecard into the lines `rigger stats --canary`
/// prints. Pure over the metrics so it is asserted without touching the filesystem, and
/// shared with `rigger canary`'s own post-run summary so the two agree.
fn format_canary_stats(m: &metrics::CanaryMetrics) -> Vec<String> {
    let mut lines = Vec::new();
    lines.push("canary stats (judge-the-judges recall):".to_string());
    // MODEL PINNING criterion (spec 61 c7): binary build, corpus hash, and every tier's
    // resolved model id - so a pinned A/B arm is auditable from the scorecard alone. Only
    // rendered once a run has actually recorded a header event (a legacy stream, or a
    // never-run project via `CanaryMetrics::default()`, leaves `binary_build` empty and
    // this block is silently omitted rather than printing a blank/misleading line).
    if !m.binary_build.is_empty() {
        lines.push(format!("  binary build       {}", m.binary_build));
        lines.push(format!(
            "  corpus hash        {}",
            if m.corpus_hash.is_empty() {
                "(unmeasured)"
            } else {
                &m.corpus_hash
            },
        ));
        lines.push("  resolved model by tier (never a configured alias):".to_string());
        for (tier, id) in &m.resolved_models {
            lines.push(format!(
                "    {tier:<16} {}",
                if id.is_empty() { "unmeasured" } else { id },
            ));
        }
    }
    lines.push(format!(
        "  items scored       {} ({} planted, {} defect class(es) cataloged)",
        m.items,
        m.planted,
        m.defect_classes.len(),
    ));
    lines.push("  catch rate by tier (planted defects each tier caught):".to_string());
    for (tier, tc) in &m.tier_catch {
        // NO FAKE ZEROS (spec 61): a tier's `0` catch count is a REAL measurement only
        // when every planted item's attribution was actually captured. When the run also
        // recorded a correctly-rejected item with an empty caught_by, that zero cannot be
        // trusted - it may be a rejection this tier truly earned but the attribution
        // mechanism failed to record, not a genuine miss - so print n/a with a reason
        // instead of the misleading 0/N (0.0%).
        if tc.caught == 0 && m.unattributed_correct_rejects > 0 {
            lines.push(format!(
                "    {tier:<16} n/a ({} correctly-rejected item(s) with no measured tier \
                 attribution)",
                m.unattributed_correct_rejects,
            ));
        } else {
            lines.push(format!(
                "    {tier:<16} {}/{} ({:.1}%)",
                tc.caught,
                tc.planted,
                tc.rate() * 100.0,
            ));
        }
    }
    // FALSE POSITIVES ARE FIRST-CLASS (spec 61): the summary reports control items as
    // their own line - approved vs rejected - visible at the same glance as the per-tier
    // catch rate above. Rejecting a known-good control burns a remediation cycle and,
    // repeated, escalates a correct unit, so a zero false-positive count still renders
    // (honestly, like the findings-volume section's own unmeasured-tier zero) rather than
    // being folded silently into the adjudicator's overall accuracy line below.
    lines.push(format!(
        "  control items      {}/{} approved ({} false positive(s): known-good rejected)",
        m.controls - m.control_false_positives,
        m.controls,
        m.control_false_positives,
    ));
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
    if !m.findings_raised.is_empty() {
        lines.push("  findings raised by tier (volume, informational):".to_string());
        for (tier, count) in &m.findings_raised {
            lines.push(format!("    {tier:<16} {count}"));
        }
    }
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
    Ok(read_project_stream(path, project, conductor::STREAM, &sel)?
        .map(|events| metrics::model_drift(&events))
        .unwrap_or_default())
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
pub(crate) fn cmd_replay(args: &[String]) -> Res {
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

    // 2. Materialize the candidate config at <rev> in a throwaway checkout. `workdir` is read
    //    via the SAME shared, validate-independent `scratch_defaults` resolver
    //    `cmd_status`/`watch_poll`/`reclaim_spawn_scratch`/`cmd_scratch`/`cmd_dash` all use
    //    (spec 83 criterion 2, round 3) - never `config::load`, which additionally requires a
    //    fully loadable `.rigger/agents/` fleet AND a passing `Config::validate` just to learn
    //    this one string field; `adv-u83c3r2-cmd-replay-fourth-unmigrated-site` found this was
    //    the fourth call site still on the old pattern, silently zeroing a configured
    //    `defaults.workdir` (this THROWAWAY scratch placement, never the candidate's own
    //    config at `<rev>`, which `materialize_config_at_rev` loads separately below) whenever
    //    `Config::validate` failed for an unrelated reason. Anchored at `repo` (the resolved
    //    git top-level, not raw cwd), matching `StoreLocation::dir`'s own convention.
    let (workdir, _max_retries) = scratch_defaults(&StoreLocation {
        dir: Path::new(&repo).join(RIGGER_DIR),
    });
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
        // or spec path to persist (spec 38, criterion 3; spec 82, criterion 1).
        runscope_store::start_fresh(&iso, &criteria, &candidate_definition, "", "", "")?;
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
    //
    //    reap-exempt (spec 79, criterion 2): `replay_dir` is created and removed entirely
    //    within this function, and the ONLY thing ever run against it in between is the
    //    offline `ReplayDriver`/`ReplayRunner` pairing constructed just above (`deps.gates:
    //    &ReplayRunner`, whose own module doc states it never shells out - a candidate-config
    //    re-drive is a pure in-process re-fold over the seeded trajectory) - no subprocess is
    //    ever spawned with a cwd inside it, so nothing can be rooted there to reap.
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
    let add = subprocess::git_in(repo)
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
    let loaded = config_store::load(checkout_str)
        .map_err(|e| format!("rigger replay: candidate config at {rev:?} is invalid: {e}"))
        .and_then(|cfg| {
            definition_hash(checkout_str)
                .map(|def| (cfg, def))
                .map_err(|e| format!("rigger replay: candidate definition hash at {rev:?}: {e}"))
        });
    // reap-exempt (spec 79, criterion 2): `checkout_str` is created and removed entirely
    // within this one function, and the ONLY things ever run against it in between are
    // `config::load` and `definition_hash` (just above) - both pure `std::fs` readers with
    // no subprocess spawned inside the checkout - so nothing can be rooted there to reap.
    let _ = subprocess::git_in(repo)
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
        _mutants_dir: &str,
        _build_cache_dir: &str,
        _build_cache_guard: &str,
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

/// The URL of the dash a driver auto-started for THIS run, recorded in `.rigger/`[`DASH_URL_FILE`]
/// (spec 19b, unit 1 discoverability). Absent when no driver started one (e.g. `rigger status`
/// run before any run began), in which case `rigger status` shows no dashboard line. Purely a
/// read: `rigger status` never starts or stops a dash.
fn recorded_dash_url(loc: &StoreLocation) -> Option<String> {
    let url = std::fs::read_to_string(loc.file(DASH_URL_FILE)).ok()?;
    let url = url.trim().to_string();
    (!url.is_empty()).then_some(url)
}

/// Every currently in-flight spawn's liveness-marker age (spec 14; spec 83 criterion 2): the
/// ONE authority `cmd_status` and `watch_poll` both call, so the two surfaces can never
/// disagree about who is still alive - no second, independently re-derived copy of this loop
/// to drift out of step with the first.
///
/// `repo` MUST be the store's resolved OWNING root ([`StoreLocation::repo_root`]), never a raw
/// `git_repo()` cwd read: a courier invoked from a nested unit worktree (the documented,
/// walk-up-supported shape [`require_store_dir`] exists for) has a DIFFERENT git toplevel than
/// the main repo a driver's `rigger step` (which always runs from the repo root) stamped the
/// marker under, so resolving the scratch root from the raw cwd silently looks in a tree
/// nothing ever wrote to - an alive, heartbeating agent then reads back with NO liveness age
/// at all, exactly spec 83's own Problem statement ("the per-spawn liveness marker the sweep
/// would consult is absent even while the agent is demonstrably alive"). An empty `repo` (no
/// owning root resolved at all) degrades to no ages, mirroring every other repo-less reader.
fn liveness_ages_for_wave(
    repo: &str,
    workdir: &str,
    run_id: &str,
    wave: &[spawn::WaveItem],
    now: std::time::SystemTime,
) -> std::collections::BTreeMap<String, u64> {
    if repo.is_empty() {
        return std::collections::BTreeMap::new();
    }
    // A read-only report resolves the root without creating it: `rigger status` must never
    // conjure a scratch root, nor run the orphan-root reclaim that creating one does.
    let root = rigger::worktree::scratch_root_path_from_env(repo, workdir);
    rigger::liveness::marker_ages(&root, run_id, wave, now)
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
    let run_backend = resolve_store(selection, &loc.file("events.db"))?;
    let run_store = Namespaced::new(run_backend.as_ref(), &loc.identity());
    watch_poll_over(loc, &run_store)
}

/// [`watch_poll`] over an already-opened run store: every store input [`watch::detect`] needs
/// comes from ONE read of the run ([`runscope::read::read_current_run`], spec 101), so a poll costs the run's
/// own events plus the carried-over knowledge, never the project's history.
fn watch_poll_over(
    loc: &StoreLocation,
    run_store: &dyn EventStore,
) -> Result<Vec<watch::Anomaly>, Box<dyn std::error::Error>> {
    let now = std::time::SystemTime::now();

    let (run_events, run_id) = runscope::read::read_current_run(run_store, conductor::STREAM)?;
    let last_event_at = run_events.last().map(|e| e.recorded_at);
    // When THIS run began - its own leading `RunStarted`'s `recorded_at` (`current_run`
    // always slices from that event onward), or `None` when no run has started yet in
    // this scope. Used ONLY to scope Signal 3 (dash liveness); see the dash-probe block
    // below for why.
    let run_started_at = run_events.first().map(|e| e.recorded_at);

    // Store integrity is judged over the run this poll already read (spec 101: a one-shot
    // command reads the run, never the whole log): a disordered tail of the run stream is
    // reported here, and `rigger validate` keeps the whole-store detector (spec 71).

    // No step process running right now: a non-blocking try-lock that succeeds means
    // free. Dropped immediately either way, so this probe never holds the lock.
    let step_lock_free = acquire_step_lock(&loc.dir).is_ok();

    // Each currently-parked spawn's heartbeat-marker age, exactly as `cmd_status`
    // computes `liveness_ages` (spec 19a) - the SAME "live agent processes" reading both
    // surfaces show, so they can never disagree on who is still working. `watch_poll` never
    // reads `max_retries` (only `cmd_status`'s blocker-line rendering needs it), so the
    // second half of the shared `scratch_defaults` pair is deliberately discarded here.
    let (workdir, _max_retries) = scratch_defaults(loc);
    let wave = spawn::step_result(&run_events)?.wave;
    let wave_liveness_ages =
        liveness_ages_for_wave(&loc.repo_root(), &workdir, &run_id, &wave, now);

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
    // safe, timeout-bounded `dash::dash_answer_on` probe, just without a pid to name. Only
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
    // The ONE probe `rigger status` also consumes ([`dash::dash_answer_on`]): a held port that
    // does not answer within [`dash::DASH_PROBE_WINDOW_MS`] is a busy dash, never a dead one.
    // `pid` is the display value (already filtered through `dash::displayable_pid`).
    let probe = |port: u16, pid: Option<u32>| match dash::dash_answer_on(port) {
        dash::DashAnswer::Serving => watch::DashProbe::Serving,
        dash::DashAnswer::Unresponsive => watch::DashProbe::Unresponsive {
            pid,
            port,
            window_ms: dash::DASH_PROBE_WINDOW_MS,
        },
        dash::DashAnswer::NotServing => watch::DashProbe::NotServing { pid, port },
    };
    let marker = dash::DashMarker::read(&marker_path);
    let recorded = recorded_dash_url(loc);
    let (dash, dash_breadcrumb_written_at) = match (recorded, marker) {
        // A recorded URL with a parseable port is the canonical authority, exactly as
        // `dash::dash_status` (crates/rigger-dash/src/dash.rs) decides it for the mismatched-marker case
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
                // Round 5 (adj-u62c1r4-verdict-reject-sentinel-pid-leaks-to-status):
                // filtered HERE, at the display value handed to the probe, never
                // by touching `pid` itself - `port_matches` above must keep reading
                // the UNFILTERED `pid.is_some()` so a genuinely port-matching
                // sentinel marker still sources `written_at` from `marker_path`, not
                // `url_path` (filtering inside `pid_if_port_matches` would flip
                // `port_matches` to false for exactly this marker and reintroduce
                // the wrong-file's-mtime defect class closed at round 9,
                // adv-u69c1-mismatched-marker-suppression-borrows-wrong-files-mtime).
                // `dash::displayable_pid` names no real process for the sentinel, so
                // it renders here exactly like the already-correct
                // no-matching-marker case.
                (probe(url_port, dash::displayable_pid(pid)), written_at)
            }
            // An unparseable recorded URL (foreign or malformed - the same ambiguous
            // input `dash_status` treats as unverifiable): the marker is the only
            // checkable breadcrumb left, so probe it as the marker-only arm does.
            None => {
                // Round 5: this arm has no url port to compare against via
                // `pid_if_port_matches`, so it always read `m.pid` directly - the
                // same sentinel-leak class the two sites above were fixed for
                // (round-4 reject's REJECT GROUND named those two by line range, but
                // the underlying defect - an unfiltered raw marker pid reaching a
                // display site - applies here identically).
                (
                    probe(m.port, dash::displayable_pid(Some(m.pid))),
                    mtime_of(&marker_path),
                )
            }
        },
        // URL recorded, no marker at all: probe the url's own port (detection, not
        // presentation - a dead url-only dash must still be reported; pinned by the
        // url-breadcrumb-only test in tests/cli.rs). No marker, no pid to name.
        (Some(url), None) => match dash::url_port(&url) {
            Some(port) => (probe(port, None), mtime_of(&url_path)),
            None => (watch::DashProbe::NotRecorded, None),
        },
        // No URL recorded: a marker alone stays this probe's own authority. `dash_status`
        // deliberately reads url-first and would call this Absent, but a marker-only dash
        // is real (the step path writes a marker; the dead-marker contract in
        // `rigger-restore-the-dash` pins that `rigger watch --once` reports it), so
        // suppressing it here would hide a genuinely dead dash.
        (None, Some(m)) => (
            // Round 5: same sentinel-leak class as the unparseable-url arm's comment
            // above - no url port to compare against, so this always read `m.pid`
            // directly until now.
            probe(m.port, dash::displayable_pid(Some(m.pid))),
            mtime_of(&marker_path),
        ),
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

/// Format `spec::spec_lint_advisories`' output (spec 18 Unit 4 / spec 66 criterion 3's
/// classification, untouched here) as the `"warning: spec {path}: {advisory}"` lines
/// `rigger validate` prints. The ONE formatter both the pre-launch `cmd_validate` below
/// AND the in-run `load_criteria` call site (spec 66 criterion 4 - reached by `rigger
/// run`, `rigger step`, and `rigger serve`/`rigger workflow` alike, since all three route
/// through that one function) build their warning lines from - never a second, parallel
/// aggregation or a re-derived wording, so the two lint surfaces cannot silently diverge.
/// Pure (no I/O), so the sharing is pinned by a direct unit test on this function, not
/// only inferred from a subprocess capture of either call site.
fn spec_lint_warning_lines(spec_path: &str, text: &str) -> Vec<String> {
    spec::spec_lint_advisories(text)
        .into_iter()
        .map(|advisory| format!("warning: spec {spec_path}: {advisory}"))
        .collect()
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

/// The branches/slugs of the CURRENT run's units, PLUS its spawns' own liveness (spec 77
/// criterion 6, `adj-u77c6-verdict-reject-unflaggable-highest-stakes-category`: registered
/// scratch roots are keyed by SPAWN id, not unit slug, so their dead-share accounting needs
/// a spawn-level liveness set alongside the unit-level one). The branch (`rigger/u/<slug>`)
/// is the durable per-unit key the conductor records on `UnitStarted`; it does NOT record
/// the worktree dir (a per-process path), so the slug carried in the branch is the only
/// stable handle back to a unit.
#[derive(Default)]
struct RunUnits {
    /// `rigger/u/<slug>` of every non-terminal (in-flight) unit - these are LIVE, so their
    /// worktrees and branches are spared from residue.
    live_branches: std::collections::HashSet<String>,
    /// `<slug>` of every terminal (integrated/escalated) unit. A DEAD unit's leftover
    /// deterministic `rigger-wt-<slug>` worktree is itself residue, and its slug must not
    /// be mistaken for a live unit's per-process `-<8hex>` tail (adv-u6res-uuid8-tail).
    dead_slugs: std::collections::HashSet<String>,
    /// The [`crate::liveness::marker_filename`]-encoded leaf name of every spawn the
    /// CURRENT run has REQUESTED but has NOT YET ANSWERED - "in-flight" in the exact sense
    /// [`crate::liveness::sweep`] already folds its own set from. A spawn drops out the
    /// instant ANY result lands on it - success, reject, `--error`, or the liveness-fault
    /// result `sweep` itself records the moment a marker goes stale - so a HUNG spawn's
    /// registered scratch leaf reads as dead here the same step `sweep` retires it, with no
    /// second, independently-derived notion of "in flight".
    live_spawn_leaf_names: std::collections::HashSet<String>,
    /// The [`crate::liveness::marker_filename`]-encoded run-id leaf of the CURRENT run
    /// itself (`None` when no run has ever started) - the exact subdir name
    /// [`crate::driver::replay::spawn_scratch_path`] nests every spawn's `agent-scratch`
    /// leaf under (`agent-scratch/<sanitized run_id>/<sanitized spawn_id>`). `dead_
    /// spawn_leaf_bytes`'s per-leaf liveness check (`live_spawn_leaf_names`) is scoped to
    /// spawns THIS run itself requested, so it must only be consulted for the run-id subdir
    /// that IS this run - a leaf name shared with an EARLIER, abandoned run's own subdir
    /// (the routine self-hosting pattern: a killed run followed by a fresh run that
    /// re-proposes the identical unit/spawn id) would otherwise be spared by name alone even
    /// though it belongs to a different, dead run
    /// (`sdet-u77c6r2-cross-run-leaf-collision-hides-the-highest-stakes-orphan`).
    current_run_scratch_leaf: Option<String>,
}

/// Fold the CURRENT run's units AND spawns from `events`. Scoping to the current run's
/// slice via `runscope::current_run` BEFORE `ledger::project`/`spawn::recorded` (exactly as
/// `conductor.rs` folds the run state it returns) is what makes a PRIOR run's abandoned
/// non-terminal unit or spawn read as residue instead of live: this CONSUMES the one "what
/// is live" authority rather than defining a parallel notion of liveness (spec 06 unit 1,
/// Gap 11; spec 77 criterion 6).
fn current_run_units(events: &[Event]) -> RunUnits {
    let scoped = runscope::current_run(events);
    let run = ledger::project(scoped).unwrap_or_default();
    let mut out = RunUnits {
        current_run_scratch_leaf: runscope::current_run_id(events)
            .and_then(|id| rigger::liveness::marker_filename(&id)),
        ..RunUnits::default()
    };
    for u in run.units.values() {
        // Spec 83, criterion 1: THE FENCE. A unit the ledger reads terminal can still have
        // a STRAGGLER spawn working the same unit id (a slower confirmatory review lens
        // still running after the deciding verdict already integrated it - the observed
        // `u81c1` bug); `spawn_fence` closes that gap by consulting the unit's LATEST
        // requested spawn directly, so `dead_slugs`/`live_branches` stay the ONE liveness
        // authority every consumer (`sweep_terminal`, `reclaim_orphan_scratch` via
        // `worktree_belongs_to_live`) already reads, rather than leaving a second notion of
        // "in flight" for callers to reconcile themselves.
        let fenced_live = run.is_terminal(&u.id)
            && matches!(
                rigger::worktree::spawn_fence(scoped, &u.id),
                rigger::worktree::SpawnFence::InFlight { .. }
            );
        if run.is_terminal(&u.id) && !fenced_live {
            if let Some(slug) = u.branch.strip_prefix("rigger/u/") {
                if !slug.is_empty() {
                    out.dead_slugs.insert(slug.to_string());
                }
            }
        } else if !u.branch.is_empty() {
            out.live_branches.insert(u.branch.clone());
        }
    }
    // Mirrors `liveness::sweep`'s own in-flight fold: every requested spawn `result_of`
    // has not yet answered. `spawn::recorded`/`spawn::result_of` failing (a malformed
    // event) is treated as "not in flight" - the conservative direction for an ADVISORY
    // surface: a read failure never overstates what is still live.
    if let Ok(requested) = spawn::recorded(scoped) {
        for id in requested.keys() {
            if matches!(spawn::result_of(scoped, id), Ok(None)) {
                if let Some(leaf) = rigger::liveness::marker_filename(id) {
                    out.live_spawn_leaf_names.insert(leaf);
                }
            }
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
///
/// `declared_units` (spec 89, criterion 1, round 2 fix) is the CURRENTLY loaded workflow's own
/// `rigger/u/<slug>` stages - config, never the event log, the SAME set `cmd_step` also hands
/// `sweep_terminal` - narrowing this backstop's own dirty-spare exception (see the worktree arm
/// below) to a unit this run's definition actually declares.
///
/// Returns how many entries were reclaimed.
fn reclaim_orphan_scratch(
    repo: &str,
    root: &str,
    run_units: &RunUnits,
    declared_units: &std::collections::HashSet<String>,
) -> usize {
    let live = live_slugs(&run_units.live_branches);
    let declared_slugs = live_slugs(declared_units);
    let root_path = std::path::Path::new(root);
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
            //
            // A HALT NEVER DISCARDS A TREE (spec 89, criterion 1), round 2 fix
            // (sdet-u89c1-sweep-terminal-discards-halted-tree, generalized): this backstop is a
            // SECOND worktree-disposition authority alongside `sweep_terminal` - both run from
            // `cmd_step`, strictly before `conductor::run` ever gets a chance to capture a
            // halted spawn's abandoned edit as its own `wip` commit - and `reap_then_remove_
            // worktree`'s own `git worktree remove --force` "also tolerates a dirty tree" (its
            // doc comment), i.e. force-discards one. "Not live-owned" alone is exactly the
            // shape a store/worktree desync (a restored snapshot, or this project's very first
            // step) leaves a genuine, not-yet-recorded unit in, so a STILL-DIRTY candidate whose
            // slug this workflow DECLARES is spared here too, regardless of liveness - mirroring
            // `sweep_terminal_logged`'s identical guard. Gated on `declared_slugs`: dirtiness
            // alone is not evidence of a halted spawn - a genuinely dead, undeclared branch that
            // happens to also carry untracked content is still reclaimed exactly as before this
            // criterion. The status read is scoped to a REAL linked worktree only
            // (`path.join(".git")` present) - a bare directory git never tracked has no `.git`
            // of its own, and running `git status` from inside one climbs to whatever repo
            // happens to enclose `root` (the "act on the enclosing repository" hazard spec 89's
            // own STEP-RESOLVES-ONE-ROOT criterion names), reading unrelated content as "dirty" -
            // falling back, for that shape, to the ORIGINAL unconditional reclaim, unchanged.
            //
            // The status read itself now goes through [`rigger::worktree::path_is_dirty`]
            // (round 3 fix, `arch-u89c1r2-dirty-check-duplicated-and-diverges-fail-direction`)
            // instead of a second, independently-hardcoded `Command::new("git")` call: round 2's
            // own inline version collapsed ANY spawn failure or non-zero git exit to `dirty =
            // false` (fail OPEN, reclaim/discard), the exact opposite of `sweep_terminal_logged`'s
            // `unwrap_or(false)` (which, negated into this same `dirty` polarity, fails CLOSED -
            // spare) on the identical unreadable-status error, despite this comment already
            // claiming the two mirror each other. Sharing the one primitive - and picking the
            // same `unwrap_or(true)` fail-closed direction the sibling call site now also picks
            // explicitly - makes that divergence structurally impossible to reintroduce.
            let slug = name.trim_start_matches(rigger::worktree::UNIT_WORKTREE_PREFIX);
            let dirty = declared_slugs.contains(slug)
                && path.join(".git").exists()
                && rigger::worktree::path_is_dirty(&path.to_string_lossy()).unwrap_or(true);
            if !worktree_belongs_to_live(&name, &live, &run_units.dead_slugs) && !dirty {
                reap_then_remove_worktree(repo, &path, root_path);
                removed += 1;
            }
        } else if let Some(slug) = name.strip_prefix(rigger::worktree::UNIT_CACHE_PREFIX) {
            // A per-unit / ad-hoc `cargo-target-<slug>` cache. Mirror the worktree liveness
            // check on the reconstructed `rigger-wt-<slug>` name so a cache stays in lockstep
            // with its unit's liveness (a live unit's cache is in use, not residue). A bare
            // `cargo-target` (no `-<slug>` tail) never matches this prefix and is spared.
            let wt = format!("{}{slug}", rigger::worktree::UNIT_WORKTREE_PREFIX);
            if !worktree_belongs_to_live(&wt, &live, &run_units.dead_slugs) {
                reap_then_remove_dir(&path, root_path);
                removed += 1;
            }
        } else if is_build_cache_tombstone(&name) {
            // A stray shared-build-cache tombstone (spec 77 criterion 5): the enumerable
            // residue an interrupted `reclaim_shared_build_cache` delete can leave behind
            // (a killed process, a filesystem error mid-`remove_dir_all`). NO liveness
            // check, unlike every other arm here - the rename that created this name is
            // the LAST thing that will ever reference it by path, so it is always safe to
            // reap unconditionally (spec 77 Design: "correct precisely because nothing can
            // ever want it back").
            reap_then_remove_dir(&path, root_path);
            removed += 1;
        }
        // Any other entry (agent-scratch, agent-live, a bare cargo-target/target, a review
        // worktree) is either a live-shared area or not rigger's slug-keyed scratch: spared
        // here and reclaimed, if ever, by the run-level fixpoint/teardown - never this backstop.
    }
    removed
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

/// What [`reclaim_shared_build_cache`] did: the exclusive, non-blocking attempt either
/// reclaimed the cache (naming the exact bytes freed - `0` when it did not exist, an
/// idempotent, honest no-op rather than an error) or found it BUSY (a rigger-launched
/// shared-cache build holds the guard SHARED right now). Distinct from an `io::Error`,
/// which this fn reserves for a genuine, unexpected filesystem failure (the caller decides
/// how to report each: a loud, non-zero-exit refusal for the operator-invoked `rigger reset
/// --build-cache`, a silent best-effort skip for the run-teardown sweep).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BuildCacheReclaim {
    Reclaimed(u64),
    Busy,
}

/// The tombstone name [`reclaim_shared_build_cache`] renames `cache` to before deleting it:
/// process- and time-qualified so concurrent or rapid-succession reclaims (of a cache a
/// build recreated in between) can never collide on one name - no lock is held across the
/// delete, so uniqueness here is what keeps two tombstones from ever fighting over the same
/// path. Named for [`is_build_cache_tombstone`] to recognize (spec 77 Design: "a failed
/// tombstone delete leaves an ENUMERABLE residue shape the orphan sweep reaps
/// unconditionally").
fn build_cache_tombstone_path(cache: &Path) -> PathBuf {
    let name = cache
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| rigger::worktree::SHARED_BUILD_CACHE_NAME.to_string());
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    cache.with_file_name(format!("{name}.tombstone-{}-{nanos}", std::process::id()))
}

/// Whether `name` (a bare filename, no path) is a [`build_cache_tombstone_path`] residue -
/// the enumerable shape a reclaim's own failed (or killed-mid-flight) delete can leave
/// behind. Nothing ever references a tombstone by path again once the rename lands (the
/// reclaim that created it either deletes it itself or is gone), so ANY match here is safe
/// to reap unconditionally - no liveness check needed, unlike the live cache name itself.
fn is_build_cache_tombstone(name: &str) -> bool {
    name.starts_with(&format!(
        "{}.tombstone-",
        rigger::worktree::SHARED_BUILD_CACHE_NAME
    ))
}

/// Reclaim the shared gate build cache at `cache` (spec 77 criterion 5, BOUNDED SHARED
/// CACHE) - the ONE mutation authority over this resource: both the operator-invoked
/// `rigger reset --build-cache` ([`reset_build_cache`]) and the best-effort run-teardown
/// sweep ([`reclaim_run_scratch`]) call this, so the safety protocol can never diverge
/// between call sites (spec 77 Global Constraint 3: fail-safe deletion only, skip anything
/// a liveness guard claims).
///
/// EXCLUSION: a reader-writer flock on [`rigger::worktree::shared_build_cache_guard_path`],
/// acquired EXCLUSIVE and NON-BLOCKING here. Every rigger-launched shared-cache build
/// ([`rigger::gate`]'s `ExecRunner::run`, via an exec-replacing `flock -s -F` wrapper around
/// the gate command itself) holds the SAME guard SHARED for as long as its cargo/rustc
/// process tree is alive, so this can only succeed when no such build is in flight - on
/// contention it returns `Ok(BuildCacheReclaim::Busy)` immediately, never queuing (spec 77
/// Design: "never waiting, so no build can queue behind the delete" - a prior, now-
/// superseded design proved a queued waiter can only ever resume into a hole, since flock
/// never gates a concurrent unlink).
///
/// RECLAIM: rename-then-delete, not delete-in-place - a successful rename onto a
/// process-unique tombstone name ([`build_cache_tombstone_path`]) IS the reclaim (cargo
/// recreates its target dir on demand, so there is nothing to restore), and the lock is
/// RELEASED the instant the rename lands, before the delete of the (potentially
/// multi-gigabyte) tombstone even starts. A build whose shared-lock acquisition was parked
/// behind this exclusive one therefore never resumes into a hole: cargo simply creates a
/// fresh directory at the original path the moment it next writes there.
///
/// A missing `cache` (never built into, or already reclaimed) reclaims 0 bytes - idempotent,
/// never an error (spec 77 Notes: "repeated reset --build-cache -> idempotent zero-report").
/// A failed rename (still holding the lock, released before returning) changes nothing on
/// disk and surfaces the `io::Error`; the delete itself is best-effort
/// (`reap_then_remove_dir`'s own swallowed-`Result` convention) - a failed or
/// killed-mid-flight delete leaves an enumerable tombstone [`is_build_cache_tombstone`]
/// recognizes, never a silently unrecoverable leak.
///
/// `scratch_root` is the resolved scratch root `cache` was itself joined onto by every real
/// caller (`cache = scratch.join(SHARED_BUILD_CACHE_NAME)`) - it authorizes the tombstone
/// reap (spec 78 round 2, decision `u78c2r2-authorized-root-caller-supplied`). Taken as an
/// explicit parameter rather than derived from `cache.parent()`: `build_cache_tombstone_path`
/// guarantees the tombstone is ALWAYS a sibling of `cache` (`Path::with_file_name`), so
/// `cache.parent()` would authorize reaping the tombstone for ANY `cache` whatsoever,
/// including one a caller bug pointed somewhere dangerous - an independently-resolved root
/// is the only check that means anything.
fn reclaim_shared_build_cache(
    cache: &Path,
    scratch_root: &Path,
) -> std::io::Result<BuildCacheReclaim> {
    let guard_path = rigger::worktree::shared_build_cache_guard_path(
        &cache
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .to_string_lossy(),
    );
    if let Some(parent) = Path::new(&guard_path).parent() {
        std::fs::create_dir_all(parent)?;
    }
    let guard = std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(false)
        .open(&guard_path)?;
    // Fully-qualified, matching `gate::build_cache_guard_is_usable`'s own reasoning: std's
    // more recently stabilized `File::try_lock`/`lock`/`unlock` share these exact names with
    // `fs2::FileExt`, and a plain method call would silently prefer the std inherent method
    // (leaving this crate's deliberate `fs2` choice unused) rather than genuinely exercising it.
    if fs2::FileExt::try_lock_exclusive(&guard).is_err() {
        return Ok(BuildCacheReclaim::Busy);
    }
    if !cache.exists() {
        let _ = fs2::FileExt::unlock(&guard);
        return Ok(BuildCacheReclaim::Reclaimed(0));
    }
    let tombstone = build_cache_tombstone_path(cache);
    if let Err(e) = std::fs::rename(cache, &tombstone) {
        let _ = fs2::FileExt::unlock(&guard);
        return Err(e);
    }
    // Release the instant the rename lands - the delete below never runs with the guard
    // held, so it can never block (or be blocked by) a build starting fresh at the now-absent
    // original path.
    let _ = fs2::FileExt::unlock(&guard);
    let bytes = dir_size_bytes(&tombstone);
    reap_then_remove_dir(&tombstone, scratch_root);
    Ok(BuildCacheReclaim::Reclaimed(bytes))
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

// ---- spec 77 criterion 6: FOOTPRINT ACCOUNTING ------------------------------------

/// One category of rigger's total on-disk footprint (spec 77 criterion 6): its human name,
/// TOTAL bytes (live and dead together), the bytes within it that are DEAD (no longer
/// needed by anything live - see each producer below for how "dead" is decided per
/// category), and the command an operator runs to reclaim it. `reclaim_hint` is `None` for
/// a category with no rigger-owned reclaim command at all (store, backups: spec 77 Global
/// Constraint 4, "the store and its backups are NEVER auto-deleted... only the operator
/// removes them") - such a category is reported but never flagged. `reclaimable` lists the
/// dead entries `rigger reset --build-cache` removes one by one ([`reclaim_dead_footprint`]) -
/// the SAME entries whose bytes `dead_bytes` counts, so the advisory and the reaper read one
/// accounting (gap 96). It is empty for a category that verb does not reclaim entry by entry:
/// store and backups (never auto-deleted), worktrees (git-registered, reclaimed by `rigger
/// step`), and the shared build cache (reclaimed whole, under its guard, by
/// [`reclaim_shared_build_cache`]).
#[derive(Debug, Clone, PartialEq)]
struct FootprintCategory {
    name: &'static str,
    total_bytes: u64,
    dead_bytes: u64,
    reclaim_hint: Option<&'static str>,
    reclaimable: Vec<DeadEntry>,
}

/// One dead entry a footprint category counts: its path, and the resolved root it was
/// enumerated under, which authorizes the reap that runs before its removal.
#[derive(Debug, Clone, PartialEq)]
struct DeadEntry {
    path: PathBuf,
    root: PathBuf,
}

/// The dead-share threshold (spec 77 criterion 6, "flags any category whose dead share
/// exceeds an advisory threshold"): a category whose `dead_bytes / total_bytes` reaches
/// this percentage is named in a footprint advisory. One fixed constant, not a config
/// knob - the Design text names a threshold, not a tunable, and every other `rigger
/// validate` residue-style advisory in this file is similarly unconditional (ANY residue
/// is reported) or fixed, never per-project configurable.
const FOOTPRINT_DEAD_SHARE_THRESHOLD_PCT: u64 = 50;

/// Reclaim hint for dead worktrees: reclaimed automatically the moment
/// [`reclaim_orphan_scratch`] next runs (every `rigger step`), which deregisters each from git
/// as it removes it - so a dead entry an operator sees here is transient, not a leak requiring
/// manual action - naming both the automatic path and the manual fallback for an operator who
/// is not mid-run.
const FOOTPRINT_RECLAIM_HINT_UNIT_SCOPED: &str =
    "reclaimed automatically by the next `rigger step`, or remove the listed dirs directly";

/// Reclaim hint for dead per-unit caches (gap 96): `rigger reset --build-cache` reclaims them
/// now, and the next `rigger step`'s orphan backstop ([`reclaim_orphan_scratch`]) would too -
/// an operator with no run in flight needs the verb, not a step that may never come.
const FOOTPRINT_RECLAIM_HINT_UNIT_CACHES: &str =
    "`rigger reset --build-cache` reclaims it now (the next `rigger step` also does)";

/// Reclaim hint for the registered-scratch-roots category (spec 77 criterion 6): unlike
/// the unit-scoped categories above, a dead spawn leaf is NOT swept by `rigger step`'s
/// per-step orphan backstop (`reclaim_orphan_scratch`'s own doc comment names `agent-scratch`
/// as an area it deliberately never touches) - it is reclaimed by `reclaim_spawn_scratch`,
/// which `cmd_result` calls unconditionally for the spawn id it is given, regardless of
/// outcome (spec 34 criterion 1, spec 77 criterion 3). Naming that real path rather than
/// `rigger step` keeps the advisory accurate for the orphan case this category exists to
/// surface (a hung, never-retried spawn: `adv-u77c2r8-mutation-scratch-orphan-on-never-
/// reported-spawn`) - nothing reclaims it until an explicit `rigger result` names it.
const FOOTPRINT_RECLAIM_HINT_SPAWN_SCOPED: &str =
    "`rigger reset --build-cache` reclaims it now (recording `rigger result` for the owning spawn also does)";

/// Reclaim hint for the "unowned agent scratch" category (spec 77 criterion 6): a top-level
/// dir (or bare file) directly under `agent-scratch` that carries NO run/spawn structure at
/// all - the ad-hoc leak the AGENT SCRATCH IS SPAWN-OWNED Design bullet names by example
/// (`agent-scratch/u77c4-target`, a `CARGO_TARGET_DIR` an agent pointed directly under
/// `agent-scratch` instead of through `rigger scratch <spawn>`'s own `<run>/<spawn>`
/// container). Unlike the two hints above, NEITHER the per-step orphan backstop
/// ([`reclaim_orphan_scratch`], which never descends into `agent-scratch`) NOR the per-spawn
/// reclaim on `rigger result` ([`reclaim_spawn_scratch`], keyed on a spawn id this shape does
/// not carry) will EVER reach it automatically - so this names the operator verb that does.
const FOOTPRINT_RECLAIM_HINT_UNOWNED_AGENT_SCRATCH: &str =
    "no run or spawn owns this, so nothing reclaims it automatically - `rigger reset \
     --build-cache` reclaims it now; point future manual scratch/CARGO_TARGET_DIR at `rigger \
     scratch <spawn>`'s own container instead of a hardcoded agent-scratch/<name> literal";

/// The TOTAL bytes (live and dead together, unconditionally) of the three name-prefix
/// shapes [`scan_residue`] already classifies: `rigger-wt-<slug>` worktrees,
/// `cargo-target-<slug>` per-unit build caches, and the bare `cargo-target`/`target`
/// SHARED build cache. No liveness decision is made here at all - just a name-prefix sum -
/// so [`worktree_belongs_to_live`] is never re-derived a second time; the DEAD half of each
/// category comes from calling [`scan_residue`] itself (see [`scratch_footprint`]), the one
/// place that authority already lives.
///
/// Returns `(worktrees_total, per_unit_caches_total, shared_build_cache_total)`.
fn scratch_totals(scratch_root: &Path) -> (u64, u64, u64) {
    let (mut wt_total, mut cache_total, mut build_cache_total) = (0u64, 0u64, 0u64);
    if let Ok(entries) = std::fs::read_dir(scratch_root) {
        for entry in entries.flatten() {
            let Ok(ft) = entry.file_type() else { continue };
            if !ft.is_dir() {
                continue;
            }
            let name = entry.file_name().to_string_lossy().into_owned();
            let bytes = dir_size_bytes(&entry.path());
            if name.starts_with(rigger::worktree::UNIT_WORKTREE_PREFIX) {
                wt_total += bytes;
            } else if name == "target" || name == "cargo-target" {
                build_cache_total += bytes;
            } else if name.starts_with(rigger::worktree::UNIT_CACHE_PREFIX) {
                cache_total += bytes;
            }
        }
    }
    (wt_total, cache_total, build_cache_total)
}

/// Measure the three categories composed of many individually-classifiable entries
/// directly under `scratch_root`: `rigger-wt-<slug>` worktrees, `cargo-target-<slug>`
/// per-unit build caches, and the bare `cargo-target`/`target` SHARED build cache (Gap 14,
/// spec 77 criterion 5's `reset --build-cache` target). Per `d-p77-c6-prior-art`
/// ("reusing scan_residue's directory-walking rather than building a second parallel
/// scanner"): the DEAD half of worktrees/per-unit-caches comes DIRECTLY from calling
/// [`scan_residue`] - the same authority [`residue_advisories`] reads - and summing its
/// already-classified entries, never a second, independently-derived liveness decision.
/// [`scan_residue`]'s own report only ever holds DEAD entries (by construction: a live
/// one is never pushed), so every byte it returns for a category counts fully toward that
/// category's dead share. The TOTAL half ([`scratch_totals`]) makes no liveness decision at
/// all. The bare shared cache has no owning unit to be live or dead FOR, so its whole size
/// counts as dead: a pure cache is always fully reclaimable (spec 77 Design, criterion 5).
///
/// Returns `(worktrees, per_unit_caches, shared_build_cache)` in that order.
fn scratch_footprint(
    scratch_root: &Path,
    live_slugs: &std::collections::HashSet<String>,
    dead_slugs: &std::collections::HashSet<String>,
) -> (FootprintCategory, FootprintCategory, FootprintCategory) {
    let (wt_total, cache_total, build_cache_total) = scratch_totals(scratch_root);
    let residue = scan_residue(
        scratch_root,
        live_slugs,
        dead_slugs,
        &[],
        &std::collections::HashSet::new(),
    );
    let wt_dead: u64 = residue.worktrees.iter().map(|(_, bytes)| bytes).sum();
    // `residue.caches` conflates the shared cache (bare `cargo-target`/`target`) with
    // per-unit caches (`cargo-target-<slug>`); only the latter belongs to THIS category -
    // the shared cache's dead share is decided unconditionally above, not read from here.
    let dead_caches: Vec<&(String, u64)> = residue
        .caches
        .iter()
        .filter(|(name, _)| name.starts_with(rigger::worktree::UNIT_CACHE_PREFIX))
        .collect();
    (
        FootprintCategory {
            name: "worktrees",
            total_bytes: wt_total,
            dead_bytes: wt_dead,
            reclaim_hint: Some(FOOTPRINT_RECLAIM_HINT_UNIT_SCOPED),
            reclaimable: Vec::new(),
        },
        FootprintCategory {
            name: "per-unit caches",
            total_bytes: cache_total,
            dead_bytes: dead_caches.iter().map(|(_, bytes)| bytes).sum(),
            reclaim_hint: Some(FOOTPRINT_RECLAIM_HINT_UNIT_CACHES),
            reclaimable: dead_caches
                .iter()
                .map(|(name, _)| DeadEntry {
                    path: scratch_root.join(name),
                    root: scratch_root.to_path_buf(),
                })
                .collect(),
        },
        FootprintCategory {
            name: "shared build cache",
            total_bytes: build_cache_total,
            dead_bytes: build_cache_total,
            reclaim_hint: Some("rigger reset --build-cache"),
            reclaimable: Vec::new(),
        },
    )
}

/// The `store` and `backups` categories' total bytes: every regular file directly under
/// `rigger_dir` (the `.rigger` store directory) whose name contains `.bak` is a BACKUP
/// (spec 77 Design, "the store and its backups"; every backup this codebase creates names
/// itself `<store-file>.bak-<suffix>`), and every other file whose name starts with
/// `events.db`, `graph.db`, or `progress.db` (covering each store file's own `-shm`/`-wal`
/// sidecars) is the live STORE. Neither category has a `dead_bytes` split or a reclaim
/// command - spec 77 Global Constraint 4 forbids rigger from ever auto-deleting either, so
/// accounting only reports their size.
fn store_and_backup_bytes(rigger_dir: &Path) -> (u64, u64) {
    let mut store = 0u64;
    let mut backups = 0u64;
    let Ok(entries) = std::fs::read_dir(rigger_dir) else {
        return (0, 0);
    };
    for entry in entries.flatten() {
        let Ok(ft) = entry.file_type() else { continue };
        if !ft.is_file() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
        if name.contains(".bak") {
            backups += size;
        } else if name.starts_with("events.db")
            || name.starts_with("graph.db")
            || name.starts_with("progress.db")
        {
            store += size;
        }
    }
    (store, backups)
}

/// Paths, each with its size in bytes.
type SizedPaths = Vec<(PathBuf, u64)>;

/// `root`'s direct child directories whose name is NOT in `live_leaf_names`, each with its
/// size - the one-level DEAD half of a scratch root whose direct children
/// are themselves spawn leaves (the mutation-scratch root's own shape: every entry
/// directly under `<cache_home>/rigger-mutants` IS a
/// [`crate::liveness::marker_filename`]-encoded spawn leaf,
/// [`crate::driver::replay::mutation_scratch_path`]). A leaf present in `live_leaf_names`
/// is spared (it is a real in-flight spawn's own scratch); everything else - a completed
/// spawn's leaf `reclaim_spawn_scratch` has not yet reclaimed, or a leftover from a run
/// this process no longer tracks - counts fully dead, mirroring how [`scratch_footprint`]
/// already treats an un-owned entry elsewhere in this file. Best-effort like every other
/// scratch walk here: an unreadable `root` reads as empty, never an error.
fn dead_spawn_leaves(
    root: &Path,
    live_leaf_names: &std::collections::HashSet<String>,
) -> SizedPaths {
    let Ok(entries) = std::fs::read_dir(root) else {
        return Vec::new();
    };
    entries
        .flatten()
        .filter(|e| e.file_type().map(|ft| ft.is_dir()).unwrap_or(false))
        .filter(|e| !live_leaf_names.contains(&e.file_name().to_string_lossy().into_owned()))
        .map(|e| (e.path(), dir_size_bytes(&e.path())))
        .collect()
}

/// Whether `dir`'s own direct children are ALL directories - the shape a WELL-FORMED run-id
/// container under `agent-scratch` always has, since [`crate::driver::replay::spawn_scratch_path`]
/// only ever creates spawn-id SUBDIRECTORIES there, never a bare file directly inside a
/// run-id dir. [`classify_agent_scratch`]'s structural test for telling such a container
/// apart from an ad-hoc dir with no run/spawn nesting at all (a leaked `CARGO_TARGET_DIR`
/// pointed directly under `agent-scratch` always holds at least one bare file at its own
/// root - cargo's `CACHEDIR.TAG` / `.rustc_info.json` sit there unconditionally). An
/// unreadable or empty `dir` reads as well-formed (vacuously true: nothing to misclassify).
fn looks_like_run_container(dir: &Path) -> bool {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return true;
    };
    entries
        .flatten()
        .all(|e| e.file_type().map(|ft| ft.is_dir()).unwrap_or(true))
}

/// Classify every entry directly under `agent_scratch_root` (spec 77 criterion 6, closing
/// the AGENT SCRATCH IS SPAWN-OWNED Design bullet's own named gap: "top-level ad-hoc dirs
/// ... that footprint accounting folds into dead-run buckets"): a WELL-FORMED run-id
/// container ([`looks_like_run_container`]) is walked one level deeper for its own per-spawn
/// liveness, exactly as the prior single-purpose walk did; anything else - a bare FILE
/// directly under `agent_scratch_root`, or a directory that fails the run-container test -
/// is an UNOWNED, ad-hoc dir with no run/spawn structure at all, returned separately BY NAME
/// rather than folded into the well-formed dead-bytes tally: no run and no spawn owns it, so
/// classifying it via run/spawn liveness would misrepresent what it even is, not just
/// whether it is dead.
///
/// `live_leaf_names` is scoped to spawns the CURRENT run itself requested
/// ([`current_run_units`]), so it may only be consulted for the ONE run-id subdir that IS
/// the current run (`current_run_leaf`) - every OTHER run-id container counts fully dead
/// unconditionally (an empty live set), regardless of whether its own leaf names happen to
/// COINCIDE with a current-run leaf name. This (run_id, leaf) keying, not leaf name alone,
/// is what closes `sdet-u77c6r2-cross-run-leaf-collision-hides-the-highest-stakes-orphan`:
/// the routine self-hosting pattern (a killed run followed by a fresh run that re-proposes
/// the IDENTICAL unit/spawn id) puts a live current-run spawn and an abandoned prior run's
/// orphan under the SAME leaf name in two DIFFERENT run-id subdirs; matching by leaf name
/// alone would spare both, silently hiding the orphan this category exists to surface. When
/// `current_run_leaf` is `None` (no run has ever started, so nothing can be "this run's own
/// subdir") every run-id subdir counts fully dead, matching the same "no live units" degrade
/// [`current_run_units`] itself already falls back to.
///
/// Returns `(dead_leaves_of_well_formed_containers, ad_hoc_entries)`: a `(path, bytes)` pair
/// per dead spawn leaf, and a `(name, bytes)` pair per unowned top-level entry.
fn classify_agent_scratch(
    agent_scratch_root: &Path,
    current_run_leaf: Option<&str>,
    live_leaf_names: &std::collections::HashSet<String>,
) -> (SizedPaths, Vec<(String, u64)>) {
    let Ok(entries) = std::fs::read_dir(agent_scratch_root) else {
        return (Vec::new(), Vec::new());
    };
    let no_live_leaves = std::collections::HashSet::new();
    let mut dead = Vec::new();
    let mut ad_hoc = Vec::new();
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        let is_dir = entry.file_type().map(|ft| ft.is_dir()).unwrap_or(false);
        if !is_dir {
            // A bare file sitting directly under agent-scratch: unowned by construction -
            // no run/spawn nesting can even begin at a file.
            let bytes = entry.metadata().map(|m| m.len()).unwrap_or(0);
            ad_hoc.push((name, bytes));
            continue;
        }
        let path = entry.path();
        if looks_like_run_container(&path) {
            let is_current_run = current_run_leaf.is_some_and(|leaf| name == leaf);
            let live = if is_current_run {
                live_leaf_names
            } else {
                &no_live_leaves
            };
            dead.extend(dead_spawn_leaves(&path, live));
        } else {
            ad_hoc.push((name, dir_size_bytes(&path)));
        }
    }
    (dead, ad_hoc)
}

/// Rigger's total on-disk footprint by category (spec 77 criterion 6, FOOTPRINT
/// ACCOUNTING): store, backups, shared build cache, per-unit caches, worktrees, and
/// registered scratch roots - the six categories the Design bullet names, in that order -
/// PLUS a seventh, "unowned agent scratch", the Done-when text's own added requirement: a
/// top-level ad-hoc dir directly under `agent-scratch` with no run/spawn owner is reported
/// separately here, never folded into "registered scratch roots"'s dead-run-keyed tally
/// ([`classify_agent_scratch`]). `mutation_root` is `None` in a homeless environment
/// ([`cache_home_from`] found neither `XDG_CACHE_HOME` nor `HOME`), which folds to a zero
/// contribution rather than an error - there is nowhere the mutation-scratch root could
/// exist there either.
///
/// Pure over its filesystem reads (every call is a plain `read_dir`/`metadata` walk with
/// no side effect), so a fixture tree with seeded files/dirs drives this directly in a
/// unit test - matching spec 77 criterion 6's own Done-when text ("on a fixture tree with
/// seeded category sizes").
fn footprint_report(
    rigger_dir: &Path,
    scratch_root: &Path,
    mutation_root: Option<&Path>,
    live_slugs: &std::collections::HashSet<String>,
    dead_slugs: &std::collections::HashSet<String>,
    current_run_scratch_leaf: Option<&str>,
    live_spawn_leaf_names: &std::collections::HashSet<String>,
) -> Vec<FootprintCategory> {
    let (store_bytes, backup_bytes) = store_and_backup_bytes(rigger_dir);
    let (worktrees, unit_caches, build_cache) =
        scratch_footprint(scratch_root, live_slugs, dead_slugs);
    // Registered scratch roots (spec 34 `agent-scratch` + spec 77 criterion 3's mutation-
    // scratch root, `d-p77-needs-c6-after-c3`): a spawn-keyed category, so its dead share
    // is decided by SPAWN liveness (`live_spawn_leaf_names`, `current_run_units`), not the
    // unit liveness `scratch_footprint` reads above - closes
    // `adj-u77c6-verdict-reject-unflaggable-highest-stakes-category`
    // (supersedes `d-u77c6-footprint-design`'s dead_bytes:0/reclaim_hint:None narrowing,
    // which deferred this classification to spec 77 criterion 4; that deferral was empty -
    // criterion 4 only reaps the per-unit cargo-target cache, never this category, per
    // `adv-u77c6-registered-scratch-roots-dead-share-never-flaggable`). `agent-scratch`
    // additionally keys that liveness check by RUN, not just spawn leaf
    // (`current_run_scratch_leaf`, closing
    // `sdet-u77c6r2-cross-run-leaf-collision-hides-the-highest-stakes-orphan`) - the
    // mutation-scratch root has no run-id component to key on
    // ([`crate::driver::replay::mutation_scratch_path`]'s own doc comment: "no run subdir to
    // key on"), so it is unaffected.
    let agent_scratch_root = scratch_root.join("agent-scratch");
    let (agent_scratch_dead, ad_hoc_entries) = classify_agent_scratch(
        &agent_scratch_root,
        current_run_scratch_leaf,
        live_spawn_leaf_names,
    );
    // The ad-hoc entries' bytes are excluded from "registered scratch roots"'s own total -
    // NOT just its dead share - so a byte can never appear in both categories at once
    // (spec 77 criterion 6 Done-when: "never folded into a dead-run bucket").
    let ad_hoc_bytes: u64 = ad_hoc_entries.iter().map(|(_, bytes)| bytes).sum();
    let agent_scratch_well_formed_total =
        dir_size_bytes(&agent_scratch_root).saturating_sub(ad_hoc_bytes);
    let scratch_bytes =
        agent_scratch_well_formed_total + mutation_root.map(dir_size_bytes).unwrap_or(0);
    // Each dead leaf with the root it was enumerated under: agent-scratch leaves sit under the
    // scratch root, mutation-scratch leaves under the mutation-scratch root.
    let dead_leaves: Vec<(PathBuf, u64, PathBuf)> = agent_scratch_dead
        .into_iter()
        .map(|(path, bytes)| (path, bytes, scratch_root.to_path_buf()))
        .chain(mutation_root.into_iter().flat_map(|m| {
            dead_spawn_leaves(m, live_spawn_leaf_names)
                .into_iter()
                .map(move |(path, bytes)| (path, bytes, m.to_path_buf()))
        }))
        .collect();
    vec![
        FootprintCategory {
            name: "store",
            total_bytes: store_bytes,
            dead_bytes: 0,
            reclaim_hint: None,
            reclaimable: Vec::new(),
        },
        FootprintCategory {
            name: "backups",
            total_bytes: backup_bytes,
            dead_bytes: 0,
            reclaim_hint: None,
            reclaimable: Vec::new(),
        },
        build_cache,
        unit_caches,
        worktrees,
        FootprintCategory {
            name: "registered scratch roots",
            total_bytes: scratch_bytes,
            dead_bytes: dead_leaves.iter().map(|(_, bytes, _)| bytes).sum(),
            reclaim_hint: Some(FOOTPRINT_RECLAIM_HINT_SPAWN_SCOPED),
            reclaimable: dead_leaves
                .into_iter()
                .map(|(path, _, root)| DeadEntry { path, root })
                .collect(),
        },
        FootprintCategory {
            name: "unowned agent scratch",
            total_bytes: ad_hoc_bytes,
            // No run and no spawn owns it - the whole point of the classification - so it is
            // always fully dead, mirroring how the bare shared build cache above is always
            // fully dead: there is no "live" reading of a byte nothing owns.
            dead_bytes: ad_hoc_bytes,
            reclaim_hint: Some(FOOTPRINT_RECLAIM_HINT_UNOWNED_AGENT_SCRATCH),
            reclaimable: ad_hoc_entries
                .iter()
                .map(|(name, _)| DeadEntry {
                    path: agent_scratch_root.join(name),
                    root: scratch_root.to_path_buf(),
                })
                .collect(),
        },
    ]
}

/// `rigger validate`'s footprint ADVISORY (spec 77 criterion 6): warn, per category, when
/// its dead share (`dead_bytes / total_bytes`) reaches
/// [`FOOTPRINT_DEAD_SHARE_THRESHOLD_PCT`], naming the reclaiming command. Advisory tone -
/// never changes validate's exit status - and a category with no reclaim command at all
/// (`reclaim_hint: None` - store, backups) is never flagged regardless of its dead share,
/// matching spec 77 Global Constraint 4 (never auto-deleted, reported only). An empty
/// category (`total_bytes == 0`) is silent, not a divide-by-zero: nothing to flag.
fn footprint_advisories(categories: &[FootprintCategory]) -> Vec<String> {
    categories
        .iter()
        .filter_map(|c| {
            if c.total_bytes == 0 {
                return None;
            }
            let hint = c.reclaim_hint?;
            let dead_pct = c.dead_bytes.saturating_mul(100) / c.total_bytes;
            if dead_pct < FOOTPRINT_DEAD_SHARE_THRESHOLD_PCT {
                return None;
            }
            Some(format!(
                "warning: {} is {dead_pct}% dead ({} of {} reclaimable) - {hint}",
                c.name,
                human_size(c.dead_bytes),
                human_size(c.total_bytes)
            ))
        })
        .collect()
}

/// What `rigger reset --build-cache` did to one footprint category's dead entries (gap 96):
/// the bytes and entries it removed, and each entry it left in place because a live process
/// still holds it.
#[derive(Debug, Default, PartialEq)]
struct FootprintReclaim {
    name: &'static str,
    bytes: u64,
    removed: usize,
    held: Vec<PathBuf>,
}

/// Reclaim every dead entry the footprint accounting lists (gap 96, ONE ACCOUNTING, ONE
/// REAPER): the exact entries `rigger validate` counts dead in each category's `reclaimable`,
/// never a second, independently-derived notion of dead. Before any removal it checks for a
/// holder ([`rigger::holders::processes_holding`]: a process whose cwd or open file descriptor
/// is inside) and leaves a held entry where it is; a directory is then removed through
/// [`reap_then_remove_dir`] under the root it was enumerated from. Best-effort per entry: one
/// that still exists afterwards is neither counted nor reported as removed.
fn reclaim_dead_footprint(categories: &[FootprintCategory]) -> Vec<FootprintReclaim> {
    categories
        .iter()
        .filter(|c| !c.reclaimable.is_empty())
        .map(|c| {
            let mut done = FootprintReclaim {
                name: c.name,
                ..FootprintReclaim::default()
            };
            for entry in &c.reclaimable {
                if !rigger::holders::processes_holding(&entry.path).is_empty() {
                    done.held.push(entry.path.clone());
                    continue;
                }
                let bytes = if entry.path.is_dir() {
                    let bytes = dir_size_bytes(&entry.path);
                    reap_then_remove_dir(&entry.path, &entry.root);
                    bytes
                } else {
                    let bytes = entry.path.metadata().map(|m| m.len()).unwrap_or(0);
                    let _ = std::fs::remove_file(&entry.path);
                    bytes
                };
                if !entry.path.exists() {
                    done.bytes += bytes;
                    done.removed += 1;
                }
            }
            done
        })
        .collect()
}

/// The lines `rigger reset --build-cache` reports for [`reclaim_dead_footprint`]'s outcome:
/// per category, what it reclaimed and, separately, every entry a live process kept.
fn footprint_reclaim_lines(reclaims: &[FootprintReclaim]) -> Vec<String> {
    let mut lines = Vec::new();
    for r in reclaims {
        if r.removed > 0 {
            lines.push(format!(
                "--build-cache: reclaimed {} ({} byte(s)) from {} ({} dead entr{})",
                human_size(r.bytes),
                r.bytes,
                r.name,
                r.removed,
                if r.removed == 1 { "y" } else { "ies" }
            ));
        }
        if !r.held.is_empty() {
            let held: Vec<String> = r.held.iter().map(|p| p.display().to_string()).collect();
            lines.push(format!(
                "--build-cache: left {} dead entr{} of {} in place - a live process still holds \
                 each: {}",
                r.held.len(),
                if r.held.len() == 1 { "y" } else { "ies" },
                r.name,
                held.join(", ")
            ));
        }
    }
    lines
}

/// The repo root that owns the CURRENT store scope from `cwd`: the store's OWNING root
/// when a store exists (walking up as the couriers do), else the cwd's git top-level, else
/// the cwd itself. The SINGLE authority [`residue_advisories`] and
/// [`footprint_report_for`] both resolve their scratch root from, so `rigger validate`'s
/// two disk-accounting surfaces can never scan two different scratch roots for what is
/// nominally the same run.
fn owning_repo_root(cwd: &Path) -> String {
    find_store_dir_from(cwd)
        .and_then(|d| d.parent().map(|p| p.to_string_lossy().into_owned()))
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| {
            let top = git_repo_at(cwd);
            if top.is_empty() {
                cwd.to_string_lossy().into_owned()
            } else {
                top
            }
        })
}

/// `entry` under the project's `.rigger/` directory: `<root>/.rigger/<entry>`.
fn rigger_path(root: &Path, entry: &str) -> std::path::PathBuf {
    root.join(RIGGER_DIR).join(entry)
}

/// The [`rigger_path`] entry the per-project JS driver is provisioned into:
/// `<root>/.rigger/shim/`. `rigger setup` writes the embedded runtime files here and
/// installs their npm deps; `rigger workflow` runs `shim.mjs` from here.
const SHIM_DIR: &str = "shim";

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

/// The [`rigger_path`] entry where a repo declares its skill-registry project overlay:
/// `<root>/.rigger/docs-overlay.yml`. Optional - an absent file means every installed skill
/// carries only the shared defaults.
const DOCS_OVERLAY_FILE: &str = "docs-overlay.yml";

/// A per-repo overlay that adds THIS repository's specifics to every INSTALLED registry
/// skill WITHOUT editing the shared discipline source (overlay honored per entry - spec
/// 68, criterion 1). The two drift-prone facts a downstream project may differ on - the
/// base branch a run anchors on and where the repo keeps its specs - are read from
/// [`DOCS_OVERLAY_FILE`] and merged onto the code-derived [`docs_context`] before each
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

/// Read the project's [`DocsOverlay`] from [`DOCS_OVERLAY_FILE`]. An ABSENT file is the
/// common case and yields an empty overlay (no overrides), so a repo that wants only the
/// shared discipline writes no overlay. A PRESENT but malformed overlay is a LOUD error
/// naming the file, never a silent skip that would install a skill missing the repo
/// specifics the author asked for.
fn read_docs_overlay(root: &Path) -> Result<DocsOverlay, Box<dyn std::error::Error>> {
    let path = rigger_path(root, DOCS_OVERLAY_FILE);
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
# The relocated per-unit target (spec 89): a unit's build cache is the sibling
# `cargo-target-<unit>` of its worktree under `<cache home>/rigger/<encoded repo root>`,
# where the repo root is encoded byte by byte - alphanumerics and `-` kept, every other
# byte as `_xx` hex - exactly as the binary encodes it, so this shell derivation and the
# Rust one name the same directory.
encode_repo_path() {
    p="$1"; out=""
    while [ -n "$p" ]; do
        c=${p%"${p#?}"}; p=${p#?}
        case "$c" in
            [A-Za-z0-9-]) out="$out$c" ;;
            *) out="$out$(printf '_%02x' "'$c")" ;;
        esac
    done
    printf '%s' "$out"
}
unit_release=
unit_debug=
relocated_release=
relocated_debug=
shared_debug=
if [ -n "$git_common_dir" ]; then
    if [ -n "$unit" ]; then
        unit_release="$git_common_dir/../.rigger/tmp/cargo-target-$unit/release/rigger"
        unit_debug="$git_common_dir/../.rigger/tmp/cargo-target-$unit/debug/rigger"
        repo_root=$(cd "$git_common_dir/.." 2>/dev/null && pwd -P)
        if [ -n "$repo_root" ]; then
            cache_root="${XDG_CACHE_HOME:-$HOME/.cache}/rigger/$(encode_repo_path "$repo_root")"
            relocated_release="$cache_root/cargo-target-$unit/release/rigger"
            relocated_debug="$cache_root/cargo-target-$unit/debug/rigger"
        fi
    fi
    shared_debug="$git_common_dir/../.rigger/tmp/cargo-target/debug/rigger"
fi
rigger_bin=
for candidate in \
    "${CARGO_TARGET_DIR:+$CARGO_TARGET_DIR/release/rigger}" \
    "${CARGO_TARGET_DIR:+$CARGO_TARGET_DIR/debug/rigger}" \
    "./target/release/rigger" \
    "./target/debug/rigger" \
    "$relocated_release" \
    "$relocated_debug" \
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

/// Write the three embedded shim runtime files into `<root>/.rigger/shim/`,
/// returning that directory. Split out from [`provision_shim`] (which also runs npm
/// install) so the file-provisioning step is testable without invoking npm.
fn write_shim_files(root: &Path) -> Result<std::path::PathBuf, Box<dyn std::error::Error>> {
    let dir = rigger_path(root, SHIM_DIR);
    std::fs::create_dir_all(&dir)?;
    for (name, contents) in SHIM_FILES {
        std::fs::write(dir.join(name), contents)?;
    }
    Ok(dir)
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
        // Spec 92, criterion 4 (IN EVERY SESSION'S HAND): the discipline docs' "Looking
        // things up" section states the graph-first rule for a human reader by
        // interpolating the SAME message `rigger grep-guard` (the installed hook's
        // command) denies with - never a hand-copy that could drift from what the hook
        // actually enforces.
        grep_guard_message: GREP_GUARD_MESSAGE.to_string(),
    }
}

/// Render EVERY [registry skill](rigger::docs::skill_registry) plus every
/// [`HANDBOOK_PAGES`] entry from [`docs_context`], and write them under `root`: each skill
/// at [`skill_source_rel`]`(entry.name)`, each handbook page at its own rel path. Returns
/// the paths written, in registry order followed by [`HANDBOOK_PAGES`] order - a stable
/// order. Rooted at `root` so it is testable against a temp dir without touching the
/// process cwd; parent directories are created so a fresh checkout renders every file
/// (spec 68, criterion 1; spec 66, criterion 2: generalizes over the whole registry AND
/// the whole handbook-page list - adding an entry to either needs no edit here).
fn write_docs(root: &Path) -> Result<Vec<std::path::PathBuf>, Box<dyn std::error::Error>> {
    let ctx = docs_context();
    let mut outputs: Vec<(std::path::PathBuf, String)> = rigger::docs::skill_registry()
        .into_iter()
        .map(|entry| (root.join(skill_source_rel(entry.name)), entry.render(&ctx)))
        .collect();
    for (rel, render) in HANDBOOK_PAGES {
        outputs.push((root.join(rel), render(&ctx)));
    }
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

/// The DISCOVERABILITY reminder (spec 66, criterion 5): the one line every pre-launch
/// "next steps" surface prints when a spec path is in play, naming `rigger validate
/// <spec>` - the mechanical pre-launch spec lint (spec 18/66) - as a next step. Single-
/// sourced so every such surface says the exact same thing rather than each inventing its
/// own wording.
///
/// Callers: `run_cli` (`rigger run <spec>`), `cmd_workflow` (`rigger workflow <spec>`), and
/// `cmd_step` (`rigger step --spec <spec>`) are the surfaces that GENUINELY hold a spec path
/// at the real pre-launch moment in production, so they are the ones that make this
/// criterion's "reaches every consumer" promise true. `cmd_step` is the PRIMARY one: on the
/// native `/rigger <spec>` Claude Code workflow, `cmd_step` IS the driver (there is no
/// separate setup step - `shim/shim.mjs`'s whole mechanism is to courier `rigger step`
/// calls), so it is by far the most-exercised of the three; it prints the reminder on
/// STDERR (unlike the other two, which use stdout) because its stdout carries exactly one
/// line of `{wave,done}` JSON a driver parses, and the reminder must never share that line.
/// `cmd_prime` also calls this when given an explicit spec arg, but its own sole AUTOMATIC
/// caller - the installed Claude Code SessionStart hook (`crates/rigger-driver/src/hooks.rs`) - always invokes it
/// with zero args (the hook fires before any spec is ever chosen), so `cmd_prime`'s spec-arg
/// branch is exercised only by a hand-typed `rigger prime <spec>`, never by that hook. It is
/// kept (harmless, tested) for that manual use, not as this criterion's production path.
fn spec_lint_next_step(spec_path: &str) -> String {
    format!(
        "next: `rigger validate {spec_path}` checks the spec's shape (multi-behavior \
         criteria, missing ownership, draft-smell phrasing, em dashes) before you spend a \
         run on it"
    )
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

fn git_repo() -> String {
    git_repo_at(&cwd())
}

/// The git top-level directory *containing `root`*, resolved with `git -C <root>` so the
/// answer is anchored at `root` rather than the process cwd - empty when `root` is not in
/// a git repo. Running git anchored at an explicit directory is what lets the couriers
/// derive a store's identity from the RESOLVED store root (which git reports as the repo
/// root) instead of the cwd (which, inside a git-linked worktree, git reports as the
/// worktree path) - see [`project_identity_at`].
fn git_repo_at(root: &Path) -> String {
    subprocess::git_in(root)
        .args(["rev-parse", "--show-toplevel"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default()
}

/// The graph-first lookup hook's stated bounce message (spec 92, criterion 4's Design
/// text), naming the escape hatch in the one spelling that survives a sibling PreToolUse
/// hook's rewrite: `--literal` inside a trailing shell comment, which the shell discards
/// before grep runs (lesson-u101c2r3-grep-guard-literal-not-stripped).
const GREP_GUARD_MESSAGE: &str =
    "use rigger_ground / rigger_graph for code lookups; grep is for literal text - end the \
     command with a `# --literal` comment to proceed";

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::assert_driver_guards_a_null_step;
    use crate::test_support::assert_teardown_reaps_what_is_rooted_inside;
    use crate::test_support::git_init_quiet;
    use crate::test_support::git_ok;
    use crate::test_support::js_declaration;
    use crate::test_support::pgid_of;
    use crate::test_support::run_git;
    use crate::test_support::write_file;
    use crate::test_support::CwdGuard;
    use std::process::Command;

    /// Spec 101: the note a read-only surface prints names `rigger setup` exactly when `graph.db`
    /// owes its rebuild - never for a current file, and never by creating an absent one.
    #[test]
    fn the_rebuild_owed_note_speaks_only_for_a_graph_db_that_owes_it_and_creates_none() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("graph.db");
        let graph_db = path.to_str().unwrap();
        assert_eq!(graph_rebuild_owed_note(graph_db, "p"), None);
        assert!(!path.exists(), "an absent graph.db is never created");

        let mut decision = Event::new(
            contextgraph::TYPE_DECISION_MADE,
            br#"{"id":"d","summary":"s","governs":[],"supersedes":""}"#.to_vec(),
        );
        decision.position = 1;
        Projector::open(graph_db, "p")
            .unwrap()
            .apply(&decision)
            .unwrap();
        assert_eq!(
            graph_rebuild_owed_note(graph_db, "p"),
            None,
            "a current file owes nothing"
        );

        rusqlite::Connection::open(&path)
            .unwrap()
            .execute_batch("PRAGMA user_version = 0;")
            .unwrap();
        assert_eq!(
            graph_rebuild_owed_note(graph_db, "p"),
            Some(format!(
                "note: {} - until then the context graph answers as it stands",
                contextgraph::REBUILD_OWED
            ))
        );
    }

    /// A minimal spawn request: the deterministic id derived from `unit` + `role` + `attempt`
    /// (so it cannot drift from the labels), every optional field empty.
    fn test_request(
        unit: &str,
        stage: &str,
        role: &str,
        attempt: u32,
        prompt: &str,
    ) -> spawn::SpawnRequest {
        spawn::SpawnRequest {
            id: spawn::spawn_id(unit, role, attempt),
            unit: unit.to_string(),
            stage: stage.to_string(),
            prompt: prompt.to_string(),
            ..Default::default()
        }
    }

    // --- Spec 66, criterion 5: DISCOVERABILITY - `rigger prime` names the spec lint ---

    /// [`spec_lint_next_step`] is the single-sourced text every pre-launch "next steps"
    /// surface prints when a spec path is in play: it must name the exact command
    /// (`rigger validate <spec>`) against the exact spec path handed in, so an operator or
    /// agent can copy-paste it verbatim.
    #[test]
    fn spec_lint_next_step_names_rigger_validate_and_the_given_spec_path() {
        let line = spec_lint_next_step("specs/42-widgets.md");
        assert!(
            line.contains("rigger validate specs/42-widgets.md"),
            "must name the exact pre-launch lint command against the given spec path; got: \
             {line:?}"
        );
    }

    // --- Spec 66, criterion 4: ONE LINT AUTHORITY - the shared pure formatter ---

    /// [`spec_lint_warning_lines`] is the ONE formatter both the pre-launch `cmd_validate`
    /// and the in-run `load_criteria` call site build their warning lines from - it just
    /// maps `spec::spec_lint_advisories` (criterion 3's untouched classification) through
    /// the `"warning: spec {path}: {advisory}"` convention `cmd_validate` already used.
    /// Pinning this pure function directly (no process capture needed) proves the exact
    /// count and wording either call site would print for a given spec, independent of
    /// which one actually calls it - the CLI tests in tests/cli.rs pin THAT wiring. This is
    /// the runtime pin criterion 4's Done-when names ("a divergence cannot compile or
    /// cannot pass"): both call sites route through this SAME function, so a divergence
    /// would mean editing this function itself, and any change to what it returns is
    /// caught here and by the CLI tests that pin its output verbatim - never a second,
    /// parallel formatter either call site could drift against.
    #[test]
    fn spec_lint_warning_lines_formats_every_advisory_with_the_spec_path() {
        let text = "# W\n\n## Done when\n\n\
                     - [ ] the daemon starts on boot, and it writes a pidfile, and it \
                     rotates the log nightly\n";
        let lines = spec_lint_warning_lines("specs/x.md", text);
        assert_eq!(
            lines.len(),
            spec::spec_lint_advisories(text).len(),
            "must format exactly the advisories spec_lint_advisories returns, no more, no \
             fewer; got: {lines:?}"
        );
        assert!(
            lines
                .iter()
                .all(|l| l.starts_with("warning: spec specs/x.md: ")),
            "every line must be prefixed with the spec path, matching cmd_validate's own \
             convention; got: {lines:?}"
        );
        assert!(
            lines.iter().any(|l| l.contains("multi-behavior")),
            "the multi-behavior advisory must be present verbatim; got: {lines:?}"
        );
    }

    /// A clean spec yields no warning lines at all - the formatter is advisory, never
    /// fabricates a finding.
    #[test]
    fn spec_lint_warning_lines_is_empty_on_a_clean_spec() {
        let text = "# W\n\n## Done when\n\n- [ ] the store passes the contract suite\n";
        assert!(spec_lint_warning_lines("specs/x.md", text).is_empty());
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
    fn precommit_block_finds_the_relocated_unit_target_with_the_binary_s_own_path_encoding() {
        // Spec 89 moved a unit's build cache to `<cache home>/rigger/<encoded repo>/
        // cargo-target-<unit>`; the hook must look there FIRST among the unit-derived
        // candidates (before the pre-relocation `.rigger/tmp` paths), and its shell encoding
        // of the repo root must equal `liveness::marker_filename`'s byte for byte, or the two
        // sides name different directories and the chain silently falls back to PATH.
        let hook = compose_precommit(None);
        let loop_start = hook.find("for candidate in").unwrap();
        let body_start = loop_start + "for candidate in".len();
        let loop_end = body_start + hook[body_start..].find("; do").unwrap();
        let candidates = &hook[body_start..loop_end];
        let local_debug = candidates.find("./target/debug/rigger").unwrap();
        let relocated_release = candidates
            .find("$relocated_release")
            .expect("the relocated per-unit release candidate is tried");
        let relocated_debug = candidates
            .find("$relocated_debug")
            .expect("the relocated per-unit debug candidate is tried");
        let unit_release = candidates.find("$unit_release").unwrap();
        assert!(
            local_debug < relocated_release
                && relocated_release < relocated_debug
                && relocated_debug < unit_release,
            "relocated candidates sit after the local target and before the pre-relocation \
             unit paths; got:\n{candidates}"
        );
        assert!(
            hook.contains("XDG_CACHE_HOME:-$HOME/.cache") && hook.contains("/rigger/"),
            "the relocated root honors XDG_CACHE_HOME and nests under rigger/; got:\n{hook}"
        );

        let fn_start = hook.find("encode_repo_path() {").unwrap();
        let fn_end = fn_start + hook[fn_start..].find("\n}\n").unwrap() + "\n}\n".len();
        let shell_fn = &hook[fn_start..fn_end];
        for path in [
            "/home/byran/Documents/Development/rigger",
            "/srv/build farm/proj.x_y-1",
            "/tmp/a~b@c",
        ] {
            let out = std::process::Command::new("sh")
                .arg("-c")
                .arg(format!("{shell_fn}\nencode_repo_path \"$1\"",))
                .arg("sh")
                .arg(path)
                .output()
                .expect("sh runs the hook's encoder");
            let encoded = String::from_utf8(out.stdout).unwrap();
            assert_eq!(
                encoded,
                rigger::liveness::marker_filename(path).unwrap(),
                "shell and Rust encodings must agree for {path}"
            );
        }
    }

    #[test]
    fn shipped_workflow_driver_tells_a_worker_its_units_build_location() {
        // Spec 77 criterion 1 through the editor's workflow driver: the shipped driver reads
        // the wave's `cargo_target_dir` and makes exporting it a hard rule for every cargo
        // command inside the worktree, so a worker never builds a `target/` in its tree.
        assert!(
            RIGGER_WORKFLOW.contains("req.cargo_target_dir"),
            "the driver reads the build location off the wave item"
        );
        assert!(
            RIGGER_WORKFLOW.contains("BUILD LOCATION (hard rule)")
                && RIGGER_WORKFLOW.contains("export CARGO_TARGET_DIR='${req.cargo_target_dir}'"),
            "the worker prompt names the export as a hard rule"
        );
        let rule = RIGGER_WORKFLOW.find("BUILD LOCATION (hard rule)").unwrap();
        let heartbeat = RIGGER_WORKFLOW
            .find("buildLocation +\n    heartbeat +")
            .unwrap();
        assert!(
            rule < heartbeat,
            "the rule is composed into the prompt ahead of the heartbeat and progress notes"
        );
        // The courier's structured return is the only way a wave reaches the driver, and a
        // key the schema does not REQUIRE is a key the courier can drop while retyping: the
        // schema requires every field the worker's instructions are built from.
        let required = RIGGER_WORKFLOW
            .find("required: ['id', 'unit', 'stage', 'dir', 'max_wall_clock', 'marker_path', 'cargo_target_dir']")
            .expect("the wave-item schema requires the driver-critical fields");
        let items = RIGGER_WORKFLOW.find("wave: {").unwrap();
        assert!(
            items < required,
            "the requirement sits on the wave items, not the top level"
        );
    }

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
        // Spec 92, criterion 4: the discipline docs' lookup-hook message is read from the
        // SAME const `rigger grep-guard` decides against, not a hand copy.
        assert_eq!(ctx.grep_guard_message, GREP_GUARD_MESSAGE);
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
            // Spec 92, criterion 4: the graph-first lookup hook's stated bounce message
            // appears verbatim - the skill's lookup section can never drift from what
            // `rigger grep-guard` actually enforces.
            assert!(
                out.contains(GREP_GUARD_MESSAGE),
                "grep-guard message not verbatim in render"
            );
            assert!(
                out.contains("rigger_ground") && out.contains("rigger_graph"),
                "the operator's own MCP lookup tool names not named in render"
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

    /// `rigger --help` lists `rigger docs`, the verb that renders the committed discipline, on
    /// its own line beside the setup verbs, so an operator can find it without the source.
    #[test]
    fn usage_text_lists_the_docs_verb_on_its_own_line() {
        assert!(
            USAGE_TEXT.contains(
                "\n  rigger docs                 render the code-derived docs (every registry skill and\n"
            ),
            "the usage text must list `rigger docs` with its description; got:\n{USAGE_TEXT}"
        );
        let docs = USAGE_TEXT.find("\n  rigger docs ").unwrap();
        let setup = USAGE_TEXT.find("\n  rigger setup ").unwrap();
        let prime = USAGE_TEXT.find("\n  rigger prime ").unwrap();
        assert!(
            setup < docs && docs < prime,
            "`rigger docs` sits between setup and prime, as in the registry"
        );
    }

    /// Spec 20, unit 1; spec 68, criterion 1; spec 66, criterion 2: `rigger docs` renders
    /// EVERY registry skill plus every [`HANDBOOK_PAGES`] entry and writes them to their
    /// committed paths under the project root. Proven against a temp root so it needs no
    /// process-cwd change: every file lands at its single-source path with the code facts
    /// (and, for a skill, the operator-binary prohibition) in it.
    #[test]
    fn write_docs_writes_every_registry_skill_plus_the_handbook() {
        let dir = tempfile::tempdir().unwrap();
        let written = write_docs(dir.path()).unwrap();
        let mut expected: Vec<std::path::PathBuf> = rigger::docs::skill_registry()
            .iter()
            .map(|e| dir.path().join(skill_source_rel(e.name)))
            .collect();
        for (rel, _) in HANDBOOK_PAGES {
            expected.push(dir.path().join(rel));
        }
        assert_eq!(written, expected);

        let skill_path = dir.path().join(skill_source_rel("using-rigger"));
        let handbook_path = dir.path().join(HANDBOOK_DISCIPLINE_REL);
        let guide_path = dir.path().join(PLANNING_FIELD_GUIDE_REL);
        let skill = std::fs::read_to_string(&skill_path).unwrap();
        let handbook = std::fs::read_to_string(&handbook_path).unwrap();
        let guide = std::fs::read_to_string(&guide_path).unwrap();
        assert!(skill.contains(DEFAULT_BASE_REF) && skill.contains("name: using-rigger"));
        assert!(skill.contains(rigger::docs::OPERATOR_BINARY_PROHIBITION));
        assert!(handbook.contains(DEFAULT_BASE_REF));
        assert!(
            guide.contains("Planning a loop run: the field guide")
                && guide.contains("F1 - Duplicated or ambiguously-owned units")
        );
        // Byte-stable: a second render writes identical bytes (the drift check needs this).
        write_docs(dir.path()).unwrap();
        assert_eq!(std::fs::read_to_string(&skill_path).unwrap(), skill);
        assert_eq!(std::fs::read_to_string(&handbook_path).unwrap(), handbook);
        assert_eq!(std::fs::read_to_string(&guide_path).unwrap(), guide);
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
        for (rel, _) in HANDBOOK_PAGES {
            expected_written.insert(root.join(rel));
        }
        assert_eq!(
            written, expected_written,
            "rigger docs must render EXACTLY the registry's skill paths plus the (non-registry) \
             handbook pages, no more, no less"
        );
    }

    /// Every bare `rigger <cmd>` each `(skill, commands)` case teaches names a REAL entry in
    /// [`SUBCOMMANDS`] and is literally present in that skill's rendered content.
    fn assert_skills_reference_only_real_subcommands(cases: &[(&str, &[&str])]) {
        let ctx = docs_context();
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

    rigger::test_cases! {
        /// Spec 68, criterion 2 (the accuracy pin): every bare `rigger <cmd>` a per-operation
        /// skill teaches names a REAL entry in [`SUBCOMMANDS`] - the one dispatch registry the
        /// runtime and `rigger docs` both read. `rigger::docs` cannot see `SUBCOMMANDS` (it
        /// lives in the binary crate), so this pin lives here: if a command a skill teaches
        /// were ever dropped from dispatch, this test - not just an operator hitting a dead
        /// command - would catch it.
        per_operation_skills_reference_only_real_subcommands:
            assert_skills_reference_only_real_subcommands(&[
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
        ]);
    }

    rigger::test_cases! {
        /// Spec 69, criterion 1 (the accuracy pin, extending the spec-68 sibling
        /// [`per_operation_skills_reference_only_real_subcommands`] to the three watch-discipline
        /// skills): every bare `rigger <cmd>` `rigger-watch-a-run` / `rigger-restore-the-dash` /
        /// `rigger-diagnose-churn` teach names a REAL entry in [`SUBCOMMANDS`], and is literally
        /// present in the rendered output - so a dropped or renamed `rigger dash`, `rigger
        /// status`, `rigger watch`, or `rigger emit` reference in this family fails here, not
        /// just misleads an operator.
        watching_discipline_skills_reference_only_real_subcommands:
            assert_skills_reference_only_real_subcommands(&[
            ("rigger-watch-a-run", &["status", "watch"]),
            ("rigger-restore-the-dash", &["dash", "status", "watch"]),
            ("rigger-diagnose-churn", &["emit", "watch"]),
        ]);
    }

    /// Spec 20, unit 2; spec 68, criterion 1; spec 66, criterion 2 (the CI-lane guard,
    /// generalized over the whole registry AND every [`HANDBOOK_PAGES`] entry): EVERY REAL
    /// committed registry skill plus every handbook page must be byte-identical to a fresh
    /// render of the current code facts, so a changed const/template/registry entry that
    /// was NOT followed by `rigger docs` reddens `cargo test` in CI - not only `rigger
    /// validate` on a live checkout (the validate fixture renders fresh in a temp project,
    /// so it is always in sync THERE and cannot catch real repo drift). Reads the committed
    /// files from the crate manifest dir.
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
        for (rel, render) in HANDBOOK_PAGES {
            checks.push((manifest.join(rel), render(&ctx)));
        }
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
    /// surface: the exact `Vec<String>` `cmd_status` prints (via `console::fold`, spec 93
    /// criterion 4) versus the `line` field the dashboard serializes into its `/api/state`
    /// snapshot (via `dash::build_state`). Byte-identical lines are the structural proof
    /// there is one shared classifier, not two that can drift.
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

        // The `rigger status` production render: the exact lines cmd_status prints, via the
        // console fold (spec 93, criterion 4) - the SAME `blocker::from_state` classifier the
        // dashboard render below also calls.
        let status_lines = console::fold(&events, max_retries).unwrap().blockers;

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

    // ---- `rigger validate` residue report (spec 06:60 / Gap 14d): pure seams --------

    use std::collections::HashSet;

    fn slugs<const N: usize>(xs: [&str; N]) -> HashSet<String> {
        xs.iter().map(|s| s.to_string()).collect()
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
            git_ok(root, args);
        }
        write_file(&root.join(rel), contents.as_bytes());
        for args in [&["add", rel][..], &["commit", "-q", "-m", "seed"]] {
            git_ok(root, args);
        }
    }

    // --- Spec 91 checkin round 4 (op-checkin-round-4-hang-class-mutants-fail-fast-or-justify):
    // a direct, zero-wait contract test for `resolve_main_worktree_or_refuse`'s SUCCESS return
    // value - the whole-diff mutation sweep's own machinery (cargo-mutants --in-diff, spec 91)
    // reported this mutant (line 1807, both String-literal stubs) reachable ONLY through the
    // real-subprocess suite in tests/cli.rs, whose narrowest existing coverage
    // (`serve_from_a_linked_worktree_refuses_naming_both_trees`) exercises only the REFUSAL
    // arm - never the plain, non-linked, single-root SUCCESS arm every other real-subprocess
    // test relies on implicitly. A wholesale body swap there (`Ok("xyzzy".into())` /
    // `Ok(String::new())`) is invisible to every test that merely asserts on a DOWNSTREAM
    // side effect (a store file, a branch, an exit code) reachable via many other paths too;
    // this pins the function's OWN contract directly, in-process, with no subprocess and no
    // wall-clock wait, so a wrong return value fails on the spot rather than only surfacing (if
    // ever) as one of many bounded-wait real-subprocess tests whose CUMULATIVE waits are what
    // exhausted the mutation gate's per-mutant timeout budget (op-checkin-mutation-budget-3x-
    // 95cfdd0) instead of ever reaching a fast, deterministic failure.
    #[test]
    fn resolve_main_worktree_or_refuse_returns_exactly_git_rev_parse_show_toplevel() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        init_committed_repo(root, "README.md", "seed\n");
        let expected = git_repo_at(root);
        assert!(
            !expected.is_empty(),
            "the fixture is a real git repo, so git_repo_at must resolve a real toplevel"
        );
        let got = resolve_main_worktree_or_refuse(root, "test-cmd")
            .expect("a plain, non-linked worktree must never refuse");
        assert_eq!(
            got, expected,
            "resolve_main_worktree_or_refuse must return EXACTLY `git rev-parse --show-toplevel`'s \
             own output for a plain (non-linked) worktree, not a stand-in value"
        );
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

    /// Over a repo whose base commits only `src/main.rs`, each `(setup, criterion, why)` case
    /// proceeds: the missing-paths base refusal never fires.
    fn assert_base_path_check_proceeds(cases: &[(RunBranchSetup, &str, &str)]) {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        init_committed_repo(root, "src/main.rs", "fn main() {}\n");
        let repo = root.to_str().unwrap();
        for (setup, criterion, why) in cases {
            assert!(
                refuse_when_base_lacks_spec_paths(
                    repo,
                    "rigger step",
                    "HEAD",
                    *setup,
                    &[criterion.to_string()],
                )
                .is_ok(),
                "{why}"
            );
        }
    }

    rigger::test_cases! {
        refuse_when_base_lacks_spec_paths_proceeds_when_the_base_contains_them:
            assert_base_path_check_proceeds(&[(
            RunBranchSetup::CreatedFromBase,
            "touches `src/main.rs`",
            "a spec whose referenced path exists in the base must proceed",
        )]);
        /// One present, one absent => partial => warn + proceed, never a refusal.
        refuse_when_base_lacks_spec_paths_partial_match_warns_and_proceeds:
            assert_base_path_check_proceeds(&[(
            RunBranchSetup::CreatedFromBase,
            "touches src/main.rs and adds crates/new/src/lib.rs",
            "a partial match must proceed (some named paths may be to-be-created)",
        )]);
        /// No path-like tokens => nothing to check, even on a fresh-from-base anchor. Only-absent
        /// paths, but a REUSED or HEAD-fallback anchor skips the check: the run already began (or
        /// has no resolvable base), so it must never refuse mid-run.
        refuse_when_base_lacks_spec_paths_skips_without_tokens_or_off_a_fresh_from_base_anchor:
            assert_base_path_check_proceeds(&[
            (
                RunBranchSetup::CreatedFromBase,
                "the store passes its contract suite",
                "a criterion with no path-like tokens has nothing to check",
            ),
            (
                RunBranchSetup::Reused,
                "the file crates/foo/src/bar.rs",
                "a reused run branch must not re-refuse",
            ),
            (
                RunBranchSetup::CreatedFromHead,
                "the file crates/foo/src/bar.rs",
                "a HEAD fallback (no resolvable base) must not refuse",
            ),
        ]);
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
            git_ok(empty_root, args);
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

    #[test]
    fn current_run_units_spares_a_terminal_units_branch_whose_latest_spawn_is_still_in_flight() {
        // Spec 83, criterion 1: THE FENCE. `unit-old` reads TERMINAL by the ledger alone
        // (Integrated), the pre-spec-83 signal `dead_slugs` used exclusively - but a
        // straggler spawn for the SAME unit (a slower review lens still working after the
        // deciding verdict already integrated it) is still unanswered. The fence must keep
        // its branch OUT of `dead_slugs` and IN `live_branches`, so neither
        // `sweep_terminal`'s ancestry sweep nor `reclaim_orphan_scratch`'s backstop
        // (`worktree_belongs_to_live`, keyed off these exact two sets) can remove its
        // worktree out from under the straggler.
        let events = [
            Event::new(
                runscope::TYPE_RUN_STARTED,
                br#"{"run":"r1","criteria":["new"]}"#.to_vec(),
            ),
            Event::new(
                ledger::TYPE_UNIT_STARTED,
                br#"{"id":"unit-old","branch":"rigger/u/unit-old"}"#.to_vec(),
            ),
            Event::new(
                ledger::TYPE_UNIT_INTEGRATED,
                br#"{"id":"unit-old","commit":"abc"}"#.to_vec(),
            ),
            // The straggler: requested AFTER integration, still unanswered.
            test_request("unit-old", "review", "adversary", 1, "p")
                .to_event()
                .unwrap(),
        ];
        let run = current_run_units(&events);
        assert_eq!(
            run.live_branches,
            slugs(["rigger/u/unit-old"]),
            "the fenced unit's branch must be LIVE despite the ledger reading it terminal"
        );
        assert!(
            run.dead_slugs.is_empty(),
            "a fenced unit must not ALSO appear dead - the two sets stay a partition"
        );
    }

    #[test]
    fn current_run_units_still_retires_a_terminal_unit_once_its_latest_spawn_answers() {
        // The counterpart: once that same straggler spawn answers (any result, including a
        // liveness fault), the unit reverts to dead exactly as it always has.
        let events = [
            Event::new(
                runscope::TYPE_RUN_STARTED,
                br#"{"run":"r1","criteria":["new"]}"#.to_vec(),
            ),
            Event::new(
                ledger::TYPE_UNIT_STARTED,
                br#"{"id":"unit-old","branch":"rigger/u/unit-old"}"#.to_vec(),
            ),
            Event::new(
                ledger::TYPE_UNIT_INTEGRATED,
                br#"{"id":"unit-old","commit":"abc"}"#.to_vec(),
            ),
            test_request("unit-old", "review", "adversary", 1, "p")
                .to_event()
                .unwrap(),
            spawn::SpawnResult::ok("unit-old/adversary#1", "approve")
                .to_event()
                .unwrap(),
        ];
        let run = current_run_units(&events);
        assert_eq!(run.dead_slugs, slugs(["unit-old"]));
        assert!(run.live_branches.is_empty());
    }

    #[test]
    fn current_run_units_splits_live_spawns_from_answered_ones_scoped_to_the_current_run() {
        // spec 77 criterion 6, `adj-u77c6-verdict-reject-unflaggable-highest-stakes-
        // category`: the registered-scratch-roots dead-share accounting needs a SPAWN-
        // level liveness set, folded the same conservative way `liveness::sweep` folds its
        // own in-flight set - requested, no result yet - and scoped to the CURRENT run
        // exactly like `live_branches`/`dead_slugs` above.
        let prior_spawn = test_request("unit-prior", "impl", "implementer", 0, "p")
            .to_event()
            .unwrap();
        let in_flight = test_request("unit-6", "impl", "implementer", 0, "p")
            .to_event()
            .unwrap();
        let answered = test_request("unit-6", "impl", "implementer", 1, "p")
            .to_event()
            .unwrap();
        let answered_result = spawn::SpawnResult::ok("unit-6/implementer#1", "done")
            .to_event()
            .unwrap();
        let events = [
            // A PRIOR run's own in-flight spawn - excluded by run-scoping, exactly like
            // `unit-prior`'s branch above.
            Event::new(
                runscope::TYPE_RUN_STARTED,
                br#"{"run":"r0","criteria":["old"]}"#.to_vec(),
            ),
            prior_spawn,
            // The CURRENT run begins here.
            Event::new(
                runscope::TYPE_RUN_STARTED,
                br#"{"run":"r1","criteria":["new"]}"#.to_vec(),
            ),
            in_flight,
            answered,
            answered_result,
        ];
        let run = current_run_units(&events);
        let want_live = rigger::liveness::marker_filename("unit-6/implementer#0").unwrap();
        let want_answered = rigger::liveness::marker_filename("unit-6/implementer#1").unwrap();
        let want_prior = rigger::liveness::marker_filename("unit-prior/implementer#0").unwrap();
        assert_eq!(
            run.live_spawn_leaf_names,
            slugs([want_live.as_str()]),
            "only the current run's UNANSWERED spawn - the answered sibling and the prior \
             run's own in-flight spawn are both excluded"
        );
        assert!(!run.live_spawn_leaf_names.contains(&want_answered));
        assert!(!run.live_spawn_leaf_names.contains(&want_prior));
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

    // --- Spec 77 criterion 4: BOUNDED SHARED CACHE (`reclaim_shared_build_cache`) ---

    #[test]
    fn reclaim_shared_build_cache_deletes_a_populated_cache_and_reports_its_bytes() {
        let dir = tempfile::tempdir().unwrap();
        let cache = dir.path().join("cargo-target");
        write_file(&cache.join("debug").join("a.rlib"), &[0u8; 100]);
        write_file(&cache.join("debug").join("b.rlib"), &[0u8; 250]);

        let reclaimed = match reclaim_shared_build_cache(&cache, dir.path()).expect("no io error") {
            BuildCacheReclaim::Reclaimed(n) => n,
            BuildCacheReclaim::Busy => panic!("an unheld guard must never report busy"),
        };
        assert_eq!(reclaimed, 350, "must report the exact bytes it reclaimed");
        assert!(
            !cache.exists(),
            "reset does not recreate a fresh dir at the original path (spec 77 Notes: cargo \
             creates its target dir on demand): {cache:?} must be gone, not an empty dir"
        );
    }

    #[test]
    fn reclaim_shared_build_cache_is_idempotent_zero_report_when_absent() {
        let dir = tempfile::tempdir().unwrap();
        let cache = dir.path().join("cargo-target");
        // Never created at all (a virgin project) - and calling it a SECOND time after a
        // real reclaim (simulated here by just never creating it) must both read as a
        // clean, honest zero, never an error (spec 77 Notes: "repeated reset --build-cache
        // -> idempotent zero-report").
        for _ in 0..2 {
            let reclaimed =
                match reclaim_shared_build_cache(&cache, dir.path()).expect("no io error") {
                    BuildCacheReclaim::Reclaimed(n) => n,
                    BuildCacheReclaim::Busy => panic!("nothing holds the guard here"),
                };
            assert_eq!(
                reclaimed, 0,
                "a missing cache reclaims 0 bytes, not an error"
            );
        }
    }

    #[test]
    fn reclaim_shared_build_cache_refuses_rather_than_waits_when_a_build_holds_the_guard() {
        // spec 77 Design: the exclusion is EXCLUSIVE and NON-BLOCKING - "never waiting, so
        // no build can queue behind the delete". Simulate a rigger-launched shared-cache
        // build by holding the SAME guard path SHARED (as a build's `flock -s` wrapper does,
        // via `gate::ExecRunner::run`) before calling the reclaim.
        let dir = tempfile::tempdir().unwrap();
        let cache = dir.path().join("cargo-target");
        write_file(&cache.join("debug").join("a.rlib"), &[0u8; 64]);
        let guard_path =
            rigger::worktree::shared_build_cache_guard_path(dir.path().to_str().unwrap());
        let held = std::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(false)
            .open(&guard_path)
            .expect("open guard");
        fs2::FileExt::lock_shared(&held).expect("a build's own shared lock");

        let outcome = reclaim_shared_build_cache(&cache, dir.path()).expect("no io error");
        assert!(
            matches!(outcome, BuildCacheReclaim::Busy),
            "a build holding the guard SHARED must refuse the reclaim, never wait for it: \
             {outcome:?}"
        );
        assert!(
            cache.join("debug").join("a.rlib").exists(),
            "a refused reclaim must leave the cache completely untouched"
        );
    }

    #[test]
    fn reclaim_shared_build_cache_releases_the_guard_promptly_after_a_successful_reclaim() {
        // The lock is released the INSTANT the rename lands, well before the (potentially
        // slow) delete of the tombstone even starts (spec 77 Design) - proven here as: once
        // `reclaim_shared_build_cache` returns successfully, an independent probe can take
        // the guard EXCLUSIVE immediately, with nothing still holding it.
        let dir = tempfile::tempdir().unwrap();
        let cache = dir.path().join("cargo-target");
        write_file(&cache.join("debug").join("a.rlib"), &[0u8; 32]);

        reclaim_shared_build_cache(&cache, dir.path()).expect("no io error");

        let guard_path =
            rigger::worktree::shared_build_cache_guard_path(dir.path().to_str().unwrap());
        let probe = std::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(false)
            .open(&guard_path)
            .expect("open guard for probe");
        assert!(
            fs2::FileExt::try_lock_exclusive(&probe).is_ok(),
            "the guard must be free the instant a successful reclaim returns"
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

    // ---- spec 77 criterion 6: FOOTPRINT ACCOUNTING -----------------------------------

    #[test]
    fn store_and_backup_bytes_splits_live_store_files_from_dot_bak_backups() {
        let dir = tempfile::tempdir().unwrap();
        let rigger_dir = dir.path();
        write_file(&rigger_dir.join("events.db"), &[0u8; 100]);
        write_file(&rigger_dir.join("events.db-wal"), &[0u8; 10]);
        write_file(&rigger_dir.join("graph.db"), &[0u8; 50]);
        write_file(&rigger_dir.join("progress.db"), &[0u8; 5]);
        write_file(&rigger_dir.join("events.db.bak-20260810"), &[0u8; 200]);
        write_file(&rigger_dir.join("events.db.bak-20260810-wal"), &[0u8; 20]);
        write_file(&rigger_dir.join("workflow.yml"), &[0u8; 7]); // neither category

        let (store, backups) = store_and_backup_bytes(rigger_dir);

        assert_eq!(
            store,
            100 + 10 + 50 + 5,
            "every store file + sidecar, no backups"
        );
        assert_eq!(backups, 200 + 20, "every .bak- file, no live store files");
    }

    #[test]
    fn store_and_backup_bytes_is_zero_on_a_missing_directory() {
        let dir = tempfile::tempdir().unwrap();
        let (store, backups) = store_and_backup_bytes(&dir.path().join("does-not-exist"));
        assert_eq!((store, backups), (0, 0));
    }

    #[test]
    fn scratch_footprint_totals_every_entry_and_dead_only_the_non_live_share() {
        let dir = tempfile::tempdir().unwrap();
        let scratch = dir.path();
        write_file(
            &scratch.join("rigger-wt-unit-live").join("a.txt"),
            &[0u8; 10],
        );
        write_file(
            &scratch.join("rigger-wt-unit-dead").join("b.txt"),
            &[0u8; 40],
        );
        write_file(
            &scratch.join("cargo-target-unit-live").join("l.rlib"),
            &[0u8; 3],
        );
        write_file(
            &scratch.join("cargo-target-unit-dead").join("d.rlib"),
            &[0u8; 7],
        );
        write_file(
            &scratch.join("cargo-target").join("shared.rlib"),
            &[0u8; 900],
        );

        let live = slugs(["unit-live"]);
        let dead = slugs(["unit-dead"]);
        let (worktrees, unit_caches, build_cache) = scratch_footprint(scratch, &live, &dead);

        assert_eq!(worktrees.total_bytes, 10 + 40, "live + dead worktree bytes");
        assert_eq!(worktrees.dead_bytes, 40, "only the dead unit's worktree");
        assert_eq!(
            unit_caches.total_bytes,
            3 + 7,
            "live + dead per-unit cache bytes"
        );
        assert_eq!(unit_caches.dead_bytes, 7, "only the dead unit's cache");
        assert_eq!(build_cache.total_bytes, 900);
        assert_eq!(
            build_cache.dead_bytes, 900,
            "a pure shared cache is always fully reclaimable"
        );
        assert_eq!(build_cache.reclaim_hint, Some("rigger reset --build-cache"));
    }

    #[test]
    fn footprint_report_measures_every_category_on_a_seeded_fixture_tree() {
        // Mirrors the Done-when text verbatim: "on a fixture tree with seeded category
        // sizes, [rigger validate] reports each category's total".
        let root = tempfile::tempdir().unwrap();
        let rigger_dir = root.path().join(".rigger");
        write_file(&rigger_dir.join("events.db"), &[0u8; 100]);
        write_file(&rigger_dir.join("events.db.bak-1"), &[0u8; 30]);

        let scratch = root.path().join("scratch");
        write_file(&scratch.join("rigger-wt-unit-dead").join("x"), &[0u8; 40]);
        write_file(&scratch.join("cargo-target").join("x"), &[0u8; 900]);
        // A well-formed <run>/<spawn> container - the real shape `spawn_scratch_path` creates.
        write_file(
            &scratch
                .join("agent-scratch")
                .join("run-1")
                .join("spawn-1")
                .join("x"),
            &[0u8; 12],
        );
        // A top-level AD-HOC dir with no run/spawn nesting - a bare file sits directly
        // inside it, exactly like a leaked `CARGO_TARGET_DIR` pointed straight under
        // `agent-scratch` (spec 77 criterion 6's own added Done-when clause).
        write_file(
            &scratch.join("agent-scratch").join("u-target").join("y"),
            &[0u8; 5],
        );

        let mutation_root = root.path().join("cache-home").join("rigger-mutants");
        write_file(&mutation_root.join("spawn-2").join("x"), &[0u8; 8]);

        let categories = footprint_report(
            &rigger_dir,
            &scratch,
            Some(&mutation_root),
            &slugs([]),
            &slugs(["unit-dead"]),
            None,
            &slugs([]),
        );

        assert_eq!(
            categories.len(),
            7,
            "the six Design-named categories plus 'unowned agent scratch', no more no less"
        );
        let by_name = |n: &str| categories.iter().find(|c| c.name == n).unwrap().clone();
        assert_eq!(by_name("store").total_bytes, 100);
        assert_eq!(by_name("backups").total_bytes, 30);
        assert_eq!(by_name("shared build cache").total_bytes, 900);
        assert_eq!(by_name("worktrees").total_bytes, 40);
        assert_eq!(by_name("worktrees").dead_bytes, 40);
        assert_eq!(by_name("per-unit caches").total_bytes, 0);
        assert_eq!(
            by_name("registered scratch roots").total_bytes,
            12 + 8,
            "the well-formed agent-scratch container + the mutation-scratch root - the \
             ad-hoc dir's bytes are excluded"
        );
        assert_eq!(
            by_name("unowned agent scratch").total_bytes,
            5,
            "the ad-hoc dir's bytes, reported on its own"
        );
        assert_eq!(by_name("unowned agent scratch").dead_bytes, 5);
    }

    #[test]
    fn footprint_report_folds_a_none_mutation_root_to_a_zero_contribution() {
        let root = tempfile::tempdir().unwrap();
        let categories = footprint_report(
            &root.path().join(".rigger"),
            &root.path().join("scratch"),
            None,
            &slugs([]),
            &slugs([]),
            None,
            &slugs([]),
        );
        let scratch_roots = categories
            .iter()
            .find(|c| c.name == "registered scratch roots")
            .unwrap();
        assert_eq!(scratch_roots.total_bytes, 0);
        assert_eq!(scratch_roots.dead_bytes, 0);
    }

    #[test]
    fn footprint_report_flags_registered_scratch_roots_dead_share_and_spares_a_live_spawn() {
        // adj-u77c6-verdict-reject-unflaggable-highest-stakes-category: the ONE category
        // the spec 77 Problem statement names as the worst observed leak must be able to
        // flag a dead-share breach like every other reclaimable category - mirrors
        // `footprint_advisories_flags_a_category_whose_dead_share_reaches_the_threshold`
        // for THIS category, over the real `agent-scratch/<run-id>/<spawn-id>` and
        // `<cache_home>/rigger-mutants/<spawn-id>` nesting
        // ([`crate::driver::replay::spawn_scratch_path`] /
        // [`crate::driver::replay::mutation_scratch_path`]'s own doc comments), not a
        // synthetic flat fixture.
        let root = tempfile::tempdir().unwrap();
        let scratch = root.path().join("scratch");

        let live_id = "u-live/implementer#0";
        let dead_id = "u-dead/implementer#0";
        let live_leaf = rigger::liveness::marker_filename(live_id).unwrap();
        let dead_leaf = rigger::liveness::marker_filename(dead_id).unwrap();
        let run_leaf = rigger::liveness::marker_filename("r1").unwrap();

        // A LIVE spawn's own build/verify scratch, nested under this run's own subdir.
        write_file(
            &scratch
                .join("agent-scratch")
                .join(&run_leaf)
                .join(&live_leaf)
                .join("probe-repo")
                .join("x"),
            &[0u8; 10],
        );
        // A DEAD spawn's leftover scratch - same run, no result recorded for it, but its
        // own id is not in the live set (mirrors a hung, never-retried spawn: nothing has
        // reclaimed it, and nothing else will until a real `rigger result` names it).
        write_file(
            &scratch
                .join("agent-scratch")
                .join(&run_leaf)
                .join(&dead_leaf)
                .join("probe-repo")
                .join("y"),
            &[0u8; 90],
        );

        let mutation_root = root.path().join("cache-home").join("rigger-mutants");
        // The LIVE spawn's own mutation-scratch leaf - spared.
        write_file(&mutation_root.join(&live_leaf).join("z"), &[0u8; 5]);
        // The DEAD spawn's orphaned mutation-scratch tree - the 47G leak class spec 77's
        // own Problem statement names.
        write_file(&mutation_root.join(&dead_leaf).join("w"), &[0u8; 45]);

        let live_leaf_names = slugs([live_leaf.as_str()]);
        let categories = footprint_report(
            &root.path().join(".rigger"),
            &scratch,
            Some(&mutation_root),
            &slugs([]),
            &slugs([]),
            Some(run_leaf.as_str()),
            &live_leaf_names,
        );
        let cat = categories
            .iter()
            .find(|c| c.name == "registered scratch roots")
            .unwrap();
        assert_eq!(
            cat.total_bytes,
            10 + 90 + 5 + 45,
            "every byte, live and dead"
        );
        assert_eq!(
            cat.dead_bytes,
            90 + 45,
            "only the dead spawn's leaves in BOTH roots - the live spawn's are spared"
        );
        assert_eq!(cat.reclaim_hint, Some(FOOTPRINT_RECLAIM_HINT_SPAWN_SCOPED));

        let advisories = footprint_advisories(&categories);
        assert!(
            advisories
                .iter()
                .any(|a| a.contains("registered scratch roots is 90% dead")),
            "advisories: {advisories:?}"
        );
    }

    /// `sdet-u77c6r2-cross-run-leaf-collision-hides-the-highest-stakes-orphan` /
    /// `adj-u77c6r2-verdict-reject-cross-run-leaf-collision`: round 2's fix classified a
    /// spawn leaf live-vs-dead by LEAF NAME ALONE, never by (run_id, leaf) - so the moment a
    /// LATER run re-proposes the SAME unit/spawn id as an EARLIER, abandoned run (the routine
    /// self-hosting pattern: a killed run followed by a fresh run that reuses the identical
    /// unit-title slug), the earlier run's own orphaned `agent-scratch` tree is silently
    /// spared merely because its leaf name coincides with the current run's live spawn - the
    /// exact killed-run-then-rerun shape the round-1 reject exists to prevent. This test
    /// seeds TWO run-id subdirs sharing the IDENTICAL spawn leaf name: one an orphan under an
    /// ABANDONED prior run, the other a genuinely live spawn under the CURRENT run.
    #[test]
    fn footprint_report_keys_agent_scratch_liveness_by_run_id_and_leaf_not_leaf_name_alone() {
        let root = tempfile::tempdir().unwrap();
        let scratch = root.path().join("scratch");

        // Both run-id subdirs' spawns share the IDENTICAL unit/attempt id - the self-hosting
        // re-proposal shape - so they encode to the SAME spawn leaf name.
        let spawn_id = "u77c6/implementer#2";
        let spawn_leaf = rigger::liveness::marker_filename(spawn_id).unwrap();
        let old_run_leaf = rigger::liveness::marker_filename("r-old-abandoned").unwrap();
        let current_run_leaf = rigger::liveness::marker_filename("r-current").unwrap();

        // The OLD, abandoned run's own orphan: never answered before that run was killed.
        write_file(
            &scratch
                .join("agent-scratch")
                .join(&old_run_leaf)
                .join(&spawn_leaf)
                .join("probe-repo")
                .join("orphan"),
            &[0u8; 500],
        );
        // The CURRENT run's own live spawn, reusing the SAME spawn leaf name under its OWN
        // run-id subdir - genuinely in flight, must be spared.
        write_file(
            &scratch
                .join("agent-scratch")
                .join(&current_run_leaf)
                .join(&spawn_leaf)
                .join("probe-repo")
                .join("live"),
            &[0u8; 5],
        );

        let live_leaf_names = slugs([spawn_leaf.as_str()]);
        let categories = footprint_report(
            &root.path().join(".rigger"),
            &scratch,
            None,
            &slugs([]),
            &slugs([]),
            Some(current_run_leaf.as_str()),
            &live_leaf_names,
        );
        let cat = categories
            .iter()
            .find(|c| c.name == "registered scratch roots")
            .unwrap();
        assert_eq!(cat.total_bytes, 500 + 5, "every byte, old and current");
        assert_eq!(
            cat.dead_bytes, 500,
            "the OLD, abandoned run's orphan under a DIFFERENT run-id subdir must count \
             dead even though its spawn leaf name is identical to the current run's live \
             spawn - classification must key off (run_id, leaf), never leaf name alone"
        );
    }

    #[test]
    fn looks_like_run_container_true_when_every_direct_child_is_a_directory() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("spawn-a")).unwrap();
        std::fs::create_dir_all(dir.path().join("spawn-b")).unwrap();
        assert!(looks_like_run_container(dir.path()));
    }

    #[test]
    fn looks_like_run_container_false_when_a_direct_child_is_a_bare_file() {
        // A leaked CARGO_TARGET_DIR always holds at least one bare file at its own root
        // (cargo's CACHEDIR.TAG / .rustc_info.json) - the exact signal that tells it apart
        // from a real run-id container, whose direct children are only ever spawn-id dirs.
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("debug")).unwrap();
        write_file(&dir.path().join("CACHEDIR.TAG"), &[0u8; 3]);
        assert!(!looks_like_run_container(dir.path()));
    }

    #[test]
    fn looks_like_run_container_is_true_on_an_empty_or_missing_dir() {
        let dir = tempfile::tempdir().unwrap();
        assert!(
            looks_like_run_container(&dir.path().join("does-not-exist")),
            "unreadable reads as well-formed - nothing to misclassify"
        );
        std::fs::create_dir_all(dir.path().join("empty")).unwrap();
        assert!(looks_like_run_container(&dir.path().join("empty")));
    }

    #[test]
    fn classify_agent_scratch_separates_a_bare_top_level_file_from_a_well_formed_container() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        // A bare top-level FILE directly under agent-scratch: unowned by construction - no
        // run/spawn nesting can even begin at a file.
        write_file(&root.join("stray.log"), &[0u8; 7]);
        // A well-formed, DEAD (not live) run/spawn container.
        write_file(&root.join("run-1").join("spawn-1").join("x"), &[0u8; 20]);

        let (dead, ad_hoc) = classify_agent_scratch(root, None, &std::collections::HashSet::new());

        assert_eq!(
            dead,
            vec![(root.join("run-1").join("spawn-1"), 20)],
            "only the well-formed container's dead leaf"
        );
        assert_eq!(ad_hoc, vec![("stray.log".to_string(), 7)]);
    }

    #[test]
    fn footprint_report_reports_a_top_level_adhoc_agent_scratch_dir_as_its_own_category_never_folded_into_the_dead_run_bucket(
    ) {
        // spec 77 criterion 6 Done-when: "a top-level ad-hoc dir directly under
        // agent-scratch (no run/spawn owner) is reported as its own recognized-residue
        // category with a reclaim command, never folded into a dead-run bucket" - the exact
        // defect the AGENT SCRATCH IS SPAWN-OWNED Design bullet names by example
        // (`agent-scratch/u77c4-target`, a CARGO_TARGET_DIR an agent pointed directly under
        // `agent-scratch` instead of through `rigger scratch <spawn>`'s own <run>/<spawn>
        // container).
        let root = tempfile::tempdir().unwrap();
        let scratch = root.path().join("scratch");

        let run_leaf = rigger::liveness::marker_filename("r1").unwrap();
        let live_leaf = rigger::liveness::marker_filename("u-live/implementer#0").unwrap();

        // A well-formed, LIVE spawn's own container - must stay spared, and counted ONLY in
        // "registered scratch roots".
        write_file(
            &scratch
                .join("agent-scratch")
                .join(&run_leaf)
                .join(&live_leaf)
                .join("probe-repo")
                .join("x"),
            &[0u8; 10],
        );
        // The ad-hoc, unowned dir: a bare file sits directly inside it - no run/spawn
        // nesting at all.
        write_file(
            &scratch
                .join("agent-scratch")
                .join("u77c4-target")
                .join("CACHEDIR.TAG"),
            &[0u8; 500],
        );

        let live_leaf_names = slugs([live_leaf.as_str()]);
        let categories = footprint_report(
            &root.path().join(".rigger"),
            &scratch,
            None,
            &slugs([]),
            &slugs([]),
            Some(run_leaf.as_str()),
            &live_leaf_names,
        );

        let scratch_roots = categories
            .iter()
            .find(|c| c.name == "registered scratch roots")
            .unwrap();
        assert_eq!(
            scratch_roots.total_bytes, 10,
            "the ad-hoc dir's bytes must NOT appear here"
        );
        assert_eq!(
            scratch_roots.dead_bytes, 0,
            "the live spawn's own container is spared, and the ad-hoc bytes never land in \
             this dead-run bucket either"
        );

        let unowned = categories
            .iter()
            .find(|c| c.name == "unowned agent scratch")
            .unwrap();
        assert_eq!(unowned.total_bytes, 500);
        assert_eq!(
            unowned.dead_bytes, 500,
            "no run or spawn owns it, so it is fully reclaimable"
        );
        assert!(unowned.reclaim_hint.is_some(), "its own reclaim command");

        let advisories = footprint_advisories(&categories);
        assert!(
            advisories
                .iter()
                .any(|a| a.contains("unowned agent scratch is 100% dead")),
            "advisories: {advisories:?}"
        );
    }

    #[test]
    fn footprint_advisories_flags_a_category_whose_dead_share_reaches_the_threshold() {
        let categories = vec![FootprintCategory {
            name: "shared build cache",
            total_bytes: 1000,
            dead_bytes: 1000,
            reclaim_hint: Some("rigger reset --build-cache"),
            reclaimable: Vec::new(),
        }];
        let advisories = footprint_advisories(&categories);
        assert_eq!(advisories.len(), 1);
        assert!(advisories[0].contains("shared build cache is 100% dead"));
        assert!(advisories[0].contains("rigger reset --build-cache"));
    }

    /// Gap 96, ONE ACCOUNTING, ONE REAPER: every dead class `rigger reset --build-cache`
    /// reclaims names that verb in its validate advisory, so the advisory and the reaper can
    /// never disagree about what reclaims the bytes it reports.
    #[test]
    fn footprint_advisories_name_reset_build_cache_for_every_class_it_reclaims() {
        let root = tempfile::tempdir().unwrap();
        let scratch = root.path().join("scratch");
        let mutation_root = root.path().join("cache-home").join("rigger-mutants");
        write_file(&scratch.join("cargo-target-gone").join("a"), &[0u8; 10]);
        let leaf = scratch
            .join("agent-scratch")
            .join("run-gone")
            .join("spawn-gone");
        write_file(&leaf.join("b"), &[0u8; 10]);
        write_file(
            &scratch.join("agent-scratch").join("adhoc").join("c"),
            &[0u8; 10],
        );
        write_file(&mutation_root.join("spawn-gone").join("d"), &[0u8; 10]);
        let none = std::collections::HashSet::new();
        let categories = footprint_report(
            &root.path().join(".rigger"),
            &scratch,
            Some(&mutation_root),
            &none,
            &none,
            None,
            &none,
        );
        let advisories = footprint_advisories(&categories);
        for name in [
            "per-unit caches",
            "registered scratch roots",
            "unowned agent scratch",
        ] {
            let line = advisories
                .iter()
                .find(|a| a.contains(&format!("{name} is 100% dead")))
                .unwrap_or_else(|| panic!("no advisory for {name}: {advisories:?}"));
            assert!(
                line.contains("`rigger reset --build-cache`"),
                "the advisory names the verb that reclaims it: {line}"
            );
        }
    }

    /// One unit-scoped category `name` of `total_bytes`, `dead_bytes` of them dead, draws no
    /// footprint advisory.
    fn assert_hinted_category_is_silent(name: &'static str, total_bytes: u64, dead_bytes: u64) {
        let categories = vec![FootprintCategory {
            name,
            total_bytes,
            dead_bytes,
            reclaim_hint: Some(FOOTPRINT_RECLAIM_HINT_UNIT_SCOPED),
            reclaimable: Vec::new(),
        }];
        assert!(footprint_advisories(&categories).is_empty());
    }

    rigger::test_cases! {
        /// 10% dead, below FOOTPRINT_DEAD_SHARE_THRESHOLD_PCT (50).
        footprint_advisories_is_silent_below_the_threshold: assert_hinted_category_is_silent("per-unit caches", 1000, 100);
    }

    #[test]
    fn footprint_advisories_flags_a_category_exactly_at_the_threshold_boundary() {
        // The Design/doc text says "reaches" the threshold, not "exceeds" it - so the exact
        // boundary value itself (50%, not 49% or 51%) must still warn. Distinguishes `<`
        // from `<=` in the comparison, unlike the 100%-dead fixture above.
        let categories = vec![FootprintCategory {
            name: "per-unit caches",
            total_bytes: 1000,
            dead_bytes: 500, // exactly FOOTPRINT_DEAD_SHARE_THRESHOLD_PCT (50%), not above it
            reclaim_hint: Some(FOOTPRINT_RECLAIM_HINT_UNIT_SCOPED),
            reclaimable: Vec::new(),
        }];
        let advisories = footprint_advisories(&categories);
        assert_eq!(
            advisories.len(),
            1,
            "dead share exactly AT the threshold must still warn, not stay silent"
        );
        assert!(advisories[0].contains("per-unit caches is 50% dead"));
    }

    #[test]
    fn footprint_advisories_never_flags_a_category_with_no_reclaim_command() {
        // store/backups: spec 77 Global Constraint 4, never auto-deleted, reported only -
        // even a 100%-dead-looking category must stay silent when it carries no hint.
        let categories = vec![FootprintCategory {
            name: "backups",
            total_bytes: 1_000_000,
            dead_bytes: 1_000_000,
            reclaim_hint: None,
            reclaimable: Vec::new(),
        }];
        assert!(footprint_advisories(&categories).is_empty());
    }

    rigger::test_cases! {
        footprint_advisories_is_silent_on_an_empty_category: assert_hinted_category_is_silent("worktrees", 0, 0);
    }

    #[test]
    fn owning_repo_root_prefers_the_stores_own_root_over_the_git_toplevel() {
        // The doc comment's stated precedence: the STORE's owning root (found by the
        // `.rigger`-holding walk-up) wins over the git top-level when the two differ - the
        // store lives in `child`, one level below the git root `grandparent`, so a fixture
        // where both resolve to the SAME directory could not tell the two branches apart
        // (and could not distinguish the real function from the `.filter` predicate flipped
        // to keep only the EMPTY case, since git_repo_at's non-empty fallback would then
        // coincidentally match too).
        let dir = tempfile::tempdir().unwrap();
        let grandparent = dir.path();
        git_init_quiet(grandparent);
        let child = grandparent.join("child");
        plant_store(&child);
        let deep = child.join("src").join("deep");
        std::fs::create_dir_all(&deep).unwrap();

        assert_eq!(
            owning_repo_root(&deep),
            child.to_string_lossy().into_owned(),
            "must resolve to the store's own root (child), not the outer git toplevel \
             (grandparent) - proving the store-root branch actually wins over the fallback"
        );
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
            live_spawn_leaf_names: slugs([]),
            current_run_scratch_leaf: None,
        };
        // Empty repo -> the git-aware worktree deregister is skipped and a plain removal runs,
        // which is all the synthetic (non-registered) worktree dirs here need.
        let removed = reclaim_orphan_scratch(
            "",
            scratch.to_str().unwrap(),
            &run_units,
            &std::collections::HashSet::new(),
        );
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
            reclaim_orphan_scratch(
                "",
                scratch.to_str().unwrap(),
                &run_units,
                &std::collections::HashSet::new(),
            ),
            0,
            "the sweep is idempotent - a clean root reclaims nothing"
        );
    }

    #[test]
    fn reclaim_orphan_scratch_spares_a_non_live_worktree_that_is_still_dirty() {
        // Spec 89, criterion 1 (A HALT NEVER DISCARDS A TREE), round 2 fix: this backstop is a
        // SECOND, independent worktree-disposition authority alongside `sweep_terminal` (both
        // run from `cmd_step`, before `conductor::run` ever gets a chance to capture a halted
        // spawn's abandoned edit as its own `wip` commit) - so the SAME "never force-remove a
        // dirty candidate" guard `sweep_terminal_logged` now carries must apply here too, or a
        // unit's tree can still be discarded through this door alone. `reap_then_remove_worktree`
        // itself runs `git worktree remove --force`, which "also tolerates a dirty tree" (its
        // own doc comment) - i.e. force-discards it. Not-yet-live is exactly the "no spawn
        // recorded yet" shape (a store/worktree desync, or this project's very first step): the
        // worktree here is real, dirty, and NOT in `live_branches` at all.
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        init_committed_repo(root, "README.md", "seed\n");
        let repo = root.to_str().unwrap();

        let wt_dir = root.join("rigger-wt-halted-unit");
        Worktree::create(repo, wt_dir.to_str().unwrap(), "rigger/u/halted-unit", "").unwrap();
        write_file(&wt_dir.join("halted-work.txt"), b"abandoned mid-edit\n");

        let run_units = RunUnits {
            live_branches: slugs([]),
            dead_slugs: slugs([]),
            live_spawn_leaf_names: slugs([]),
            current_run_scratch_leaf: None,
        };
        // Declared (round 3 fix): this workflow's own current definition still names
        // "halted-unit" as one of its units - the signal that distinguishes it from
        // `reclaim_orphan_scratch_spares_only_a_declared_dirty_worktree` below's genuinely
        // dead, undeclared one.
        let declared_units = slugs(["rigger/u/halted-unit"]);
        let removed =
            reclaim_orphan_scratch(repo, root.to_str().unwrap(), &run_units, &declared_units);
        assert_eq!(
            removed, 0,
            "a dirty, non-live-owned, but DECLARED worktree is spared, never force-removed"
        );
        assert!(
            wt_dir.join("halted-work.txt").exists(),
            "the abandoned edit must survive the sweep untouched"
        );

        // The paired negative-space case: once the SAME worktree is clean (its work
        // committed - exactly what the halt-recovery wip commit, or an ordinary landed unit,
        // leaves behind) it is reclaimed exactly as before this fix - dirtiness, not mere
        // non-liveness, is what changed.
        run_git(&wt_dir, &["add", "-A"]);
        run_git(&wt_dir, &["commit", "-q", "-m", "resolved"]);
        let removed =
            reclaim_orphan_scratch(repo, root.to_str().unwrap(), &run_units, &declared_units);
        assert_eq!(
            removed, 1,
            "a CLEAN non-live-owned worktree is still reclaimed as before"
        );
        assert!(!wt_dir.exists(), "the clean worktree is gone");
    }

    #[test]
    fn reclaim_orphan_scratch_spares_only_a_declared_dirty_worktree() {
        // Spec 89, criterion 1, round 3 fix: the negative-space twin of the test above.
        // Dirtiness ALONE is not proof of a halted spawn - a genuinely dead, UNDECLARED branch
        // (a prior run's leftover, a hand-made fixture) that happens to also carry untracked
        // content is still reclaimed exactly as it was before this criterion, matching
        // `sweep_terminal`'s own identical `declared_units` gate.
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        init_committed_repo(root, "README.md", "seed\n");
        let repo = root.to_str().unwrap();

        let wt_dir = root.join("rigger-wt-undeclared-orphan");
        Worktree::create(
            repo,
            wt_dir.to_str().unwrap(),
            "rigger/u/undeclared-orphan",
            "",
        )
        .unwrap();
        write_file(&wt_dir.join("stray.txt"), b"unrelated debris\n");

        let run_units = RunUnits {
            live_branches: slugs([]),
            dead_slugs: slugs([]),
            live_spawn_leaf_names: slugs([]),
            current_run_scratch_leaf: None,
        };
        // Declares an UNRELATED unit only - never "undeclared-orphan".
        let declared_units = slugs(["rigger/u/halted-unit"]);
        let removed =
            reclaim_orphan_scratch(repo, root.to_str().unwrap(), &run_units, &declared_units);
        assert_eq!(
            removed, 1,
            "a dirty, non-live-owned, and UNDECLARED worktree is still reclaimed"
        );
        assert!(
            !wt_dir.exists(),
            "the undeclared, unrelated worktree is gone"
        );
    }

    #[test]
    fn reclaim_orphan_scratch_reaps_a_stray_build_cache_tombstone_unconditionally() {
        // spec 77 criterion 5 (BOUNDED SHARED CACHE), Design: "a failed tombstone delete
        // leaves an ENUMERABLE residue shape the orphan sweep reaps unconditionally -
        // correct precisely because nothing can ever want it back". A tombstone
        // (`reclaim_shared_build_cache`'s own rename target) is never referenced by path
        // again once the rename lands, so - unlike every other entry this sweep classifies -
        // it needs NO liveness check at all: it is reaped every time, regardless of any
        // `RunUnits` liveness state.
        let dir = tempfile::tempdir().unwrap();
        let scratch = dir.path();
        let tombstone = scratch.join("cargo-target.tombstone-12345-67890");
        write_file(&tombstone.join("stranded.rlib"), &[0u8; 16]);
        // The LIVE bare cache must still be spared alongside it - the tombstone shape is a
        // distinct, disjoint name (a `.tombstone-` suffix), never confused with the live
        // cache's own bare name.
        write_file(&scratch.join("cargo-target").join("live.rlib"), &[0u8; 8]);

        let run_units = RunUnits::default();
        let removed = reclaim_orphan_scratch(
            "",
            scratch.to_str().unwrap(),
            &run_units,
            &std::collections::HashSet::new(),
        );
        assert_eq!(removed, 1, "exactly the one stray tombstone is reclaimed");
        assert!(!tombstone.exists(), "the stray tombstone must be reaped");
        assert!(
            scratch.join("cargo-target").exists(),
            "the live bare cache must be spared - its name is disjoint from the tombstone shape"
        );
    }

    // ---- store-open hardening: walk up to an existing store, never fabricate one ----

    #[test]
    fn find_store_dir_from_returns_the_dir_that_holds_the_store() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        plant_store(root);
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
        plant_store(root);
        let sub = root.join("src").join("deep");
        std::fs::create_dir_all(&sub).unwrap();
        assert_eq!(find_store_dir_from(&sub), Some(root.join(RIGGER_DIR)));
    }

    /// Plant a rigger store (`.rigger/events.db`) under `dir`.
    fn plant_store(dir: &Path) {
        std::fs::create_dir_all(dir.join(RIGGER_DIR)).unwrap();
        std::fs::File::create(dir.join(RIGGER_DIR).join("events.db")).unwrap();
    }

    /// `git worktree add` a new `branch` worktree of the repo at `root`, at `worktree`.
    fn add_worktree(root: &Path, worktree: &Path, branch: &str) {
        assert!(
            Command::new("git")
                .args(["worktree", "add", "-q"])
                .arg(worktree)
                .args(["-b", branch])
                .current_dir(root)
                .status()
                .unwrap()
                .success(),
            "git worktree add must succeed for the fixture"
        );
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
        plant_store(parent);

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
        plant_store(bare.path());
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
        // this second teardown point (the first is Worktree::remove). `reap_then_remove_dir`
        // requires the swept dir to be strictly under the given `authorized_root` (here, the
        // `.rigger/tmp` its production caller resolves it under), so the fixture mirrors that
        // exact layout - mirroring where `cmd_step` actually points `reap_then_remove_dir` in
        // production.
        let root = tempfile::tempdir().unwrap();
        let root_path = root.path().canonicalize().unwrap();
        git_init_quiet(&root_path);
        let scratch_root = root_path.join(".rigger").join("tmp");
        let swept = scratch_root.join("agent-scratch");
        std::fs::create_dir_all(&swept).unwrap();

        assert_teardown_reaps_what_is_rooted_inside(
            &swept,
            Some(&root_path),
            || reap_then_remove_dir(&swept, &scratch_root),
            "the scratch-area sweep",
        );
        assert!(
            !swept.exists(),
            "the swept scratch dir is removed after its rooted processes are reaped"
        );
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
        plant_store(root);
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
    fn find_store_dir_from_resolves_the_owning_repo_even_when_the_worktree_lives_outside_it() {
        // Spec 89, criterion 2 (SCRATCH IS OUTSIDE THE STORE TREE): a unit's real
        // git-linked worktree no longer nests under `<repo>/.rigger/tmp` - it lives
        // wherever the relocated (cache-home) scratch root resolves, which is now OUTSIDE
        // the repo's own directory tree entirely. A worker's own courier calls (`rigger
        // prompt`/`result`/`emit`/`scratch`/`progress`/`peers`) run from INSIDE that
        // worktree (each is documented as "invoked BY THE WORKER from inside its unit
        // worktree"), so `find_store_dir_from` must still resolve the repo's real store
        // even though a plain filesystem `.parent()` climb from the worktree never
        // physically passes through the repo root any more - the exact regression a
        // naive relocation would otherwise ship silently (every courier call from a real
        // relocated worktree would refuse "no rigger store found").
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        init_committed_repo(root, "README", "x");
        plant_store(root);

        // The worktree lives in a WHOLLY UNRELATED location - a sibling tempdir, never a
        // descendant of `root` - mirroring the relocated cache-home default exactly.
        let elsewhere = tempfile::tempdir().unwrap();
        let worktree = elsewhere.path().join("rigger-wt-x");
        add_worktree(root, &worktree, "rigger/u/x");

        assert_eq!(
            find_store_dir_from(&worktree),
            Some(root.join(RIGGER_DIR)),
            "a courier inside a worktree the relocated scratch root put OUTSIDE the repo \
             must still resolve the repo's real store"
        );
    }

    #[test]
    fn find_store_dir_from_never_climbs_a_relocated_worktrees_own_unrelated_ancestors_into_a_foreign_store(
    ) {
        // The adv9-walkup-cross-project hazard, re-proven for the relocated (non-nested)
        // case: when the worktree lives OUTSIDE the repo, this must NOT fall back to a
        // plain unbounded ancestor climb from the worktree - that would let a courier
        // inside a worktree parked under, say, `<cache-home>/rigger/<project>/rigger-wt-x`
        // bind to a store an ancestor of the CACHE HOME happens to carry (an unrelated
        // project's, or a leftover fixture's), exactly the cross-project escape the
        // original bound was built to close. The sanctioned set for a relocated worktree
        // is exactly {the worktree itself, the resolved repo root} - nothing between.
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        init_committed_repo(root, "README", "x");
        plant_store(root);

        // A FOREIGN store sitting at an ancestor of the relocated worktree - the exact
        // shape a plain unbounded climb would wrongly bind to.
        let elsewhere = tempfile::tempdir().unwrap();
        plant_store(elsewhere.path());
        let worktree = elsewhere.path().join("nested").join("rigger-wt-x");
        std::fs::create_dir_all(worktree.parent().unwrap()).unwrap();
        add_worktree(root, &worktree, "rigger/u/y");

        assert_eq!(
            find_store_dir_from(&worktree),
            Some(root.join(RIGGER_DIR)),
            "must resolve the REAL owning repo's store, never the foreign one sitting at an \
             ancestor of the relocated worktree"
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
        plant_store(root);
        // A nearer SHADOW store in a nested dir under the repo.
        let nested = root.join("sub").join("deep");
        plant_store(&nested);

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
        plant_store(root);
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
        let prev = std::env::current_dir().unwrap();
        let _restore = CwdGuard(prev);
        std::env::remove_var(STORE_FENCE_ENV);

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        plant_store(root);
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
        let req = test_request("u", "impl", "implementer", 0, "do it");
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
        let req = test_request("u", "impl", "implementer", 0, "do it");
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
        let req = test_request("u", "impl", "implementer", 0, "do it");
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

    /// The two checked-in workflows that ship with the repo - the self-hosted
    /// `.rigger/workflow.yml` and `examples/demo` - must each carry a NON-ZERO spawn
    /// budget (FIX 3): a shipped, unattended config must cap its own spawns. A 0
    /// (unlimited) budget here is what let a runaway loop churn for hours.
    #[test]
    // Reads relative paths (`.`, `..`) so it depends on the process CWD. Another test
    // (`cmd_stats_on_a_never_run_project...`) temporarily `set_current_dir`s to a temp
    // dir; if that runs concurrently, `config_store::load(".")` here resolves `.` to that
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
            let cfg = config_store::load(root)
                .unwrap_or_else(|e| panic!("shipped workflow at {root:?} must load: {e}"));
            assert!(
                cfg.workflow.defaults.budget > 0,
                "shipped workflow at {root:?} must cap spawns with a non-zero budget; was {}",
                cfg.workflow.defaults.budget
            );
        }
    }

    #[test]
    fn usage_text_gives_the_jobs_flag_its_own_description_line() {
        // Regression guard (spec 61, ITEM SHARDING AND THE JOBS CAP): a flag tag spliced
        // into the MIDDLE of an unrelated sentence, landing its own description lines away
        // from its tag, is the exact class of usage-text corruption this pins against for
        // both halves of the --jobs/--corpus text.
        let tag = "[--jobs <n>]";
        let tag_pos = USAGE_TEXT
            .find(tag)
            .expect("usage text names the --jobs flag");
        let after_tag = USAGE_TEXT[tag_pos + tag.len()..].trim_start_matches(' ');
        assert!(
            after_tag.starts_with("caps the total concurrent review-panel spawns"),
            "the --jobs tag must introduce its OWN description, not text describing \
             something else: found {:?}",
            &after_tag[..after_tag.len().min(80)]
        );

        // The pre-existing --corpus sentence must stay intact and unsplit by any flag tag.
        let corpus_start = USAGE_TEXT
            .find("(default ./canaries) and score per-tier catch rate")
            .expect("usage text names the --corpus default");
        let corpus_end = USAGE_TEXT
            .find("(read back with `rigger stats --canary`)")
            .expect("usage text names the canary stats readback");
        let corpus_sentence = &USAGE_TEXT[corpus_start..corpus_end];
        assert!(
            !corpus_sentence.contains('['),
            "no flag tag may be spliced into the middle of the --corpus sentence: {corpus_sentence:?}"
        );
    }

    #[test]
    fn usage_text_names_the_build_cache_reset_mode() {
        // spec 77 criterion 5 (BOUNDED SHARED CACHE), Done-when: "`rigger reset
        // --build-cache` ... appears in the usage registry". Named right alongside its
        // `--runs`/`--derived` siblings so an operator discovering one discovers all three.
        assert!(
            USAGE_TEXT.contains("rigger reset --build-cache"),
            "usage text must name the build-cache reset mode"
        );
        let pos = USAGE_TEXT
            .find("rigger reset --build-cache")
            .expect("usage text names --build-cache");
        let after = &USAGE_TEXT[pos..];
        assert!(
            after.contains("shared") && after.contains("build cache"),
            "the --build-cache line must name what it reclaims: {:?}",
            &after[..after.len().min(300)]
        );
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

    /// `parse` refuses every one of the `cases` argument lists.
    fn assert_every_arg_list_refused<T, E>(
        parse: fn(&[String]) -> Result<T, E>,
        cases: &[&[&str]],
    ) {
        for args in cases {
            let args: Vec<String> = args.iter().map(|a| a.to_string()).collect();
            assert!(parse(&args).is_err(), "{args:?} must be refused");
        }
    }

    rigger::test_cases! {
        parse_run_args_rejects_unknown_flags_and_values: assert_every_arg_list_refused(
            parse_run_args,
            &[
                &["--driver", "bogus"],
                &["--eventstore", "bogus"],
                &["--nope"],
                &["a", "b"],
            ],
        );
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
        write_def(base.path(), "dash: on\n", "Do the unit.");
        let dir = base.path().to_str().unwrap();
        let h0 = definition_hash(dir).unwrap();
        // Deterministic: recomputing over the same on-disk definition is byte-identical.
        assert_eq!(
            h0,
            definition_hash(dir).unwrap(),
            "same definition, same hash"
        );
        // Canonicalization: trailing whitespace and CRLF do NOT change the hash.
        write_def(base.path(), "dash: on\r\n", "Do the unit.   ");
        assert_eq!(
            h0,
            definition_hash(dir).unwrap(),
            "trailing-ws / CRLF noise is canonicalized away"
        );
        // A PROMPT edit changes the hash - the mid-campaign edit spec 13 must catch.
        write_def(base.path(), "dash: on\n", "Do the unit differently.");
        assert_ne!(
            h0,
            definition_hash(dir).unwrap(),
            "a prompt edit changes the definition hash"
        );
        // A workflow.yml edit changes the hash too.
        let with_wf = definition_hash(dir).unwrap();
        write_def(base.path(), "dash: off\n", "Do the unit differently.");
        assert_ne!(
            with_wf,
            definition_hash(dir).unwrap(),
            "a workflow edit changes the hash"
        );
        // The operator instruction layer is part of the definition: adding a file changes the
        // hash, editing it changes it again, and trailing-whitespace noise does not.
        let before_instructions = definition_hash(dir).unwrap();
        let ins_dir = base.path().join(".rigger").join("instructions");
        std::fs::create_dir_all(&ins_dir).unwrap();
        std::fs::write(ins_dir.join("10-house.md"), "House rule.\n").unwrap();
        let with_instruction = definition_hash(dir).unwrap();
        assert_ne!(
            before_instructions, with_instruction,
            "adding an operator instruction changes the hash"
        );
        std::fs::write(ins_dir.join("10-house.md"), "House rule.   \r\n").unwrap();
        assert_eq!(
            with_instruction,
            definition_hash(dir).unwrap(),
            "trailing-ws / CRLF noise in an instruction is canonicalized away"
        );
        std::fs::write(ins_dir.join("10-house.md"), "A different house rule.\n").unwrap();
        assert_ne!(
            with_instruction,
            definition_hash(dir).unwrap(),
            "editing an operator instruction changes the hash"
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
        write_config(Some("{}"));
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
        assert_eq!(
            moved.map(|(n, decision)| (n, decision.fold)),
            Some((
                1,
                contextgraph::Fold::NotFolded(
                    "graph: no context graph is wired to this surface".to_string()
                )
            )),
            "one legacy stream renamed; with no graph wired its decision says it was not folded"
        );

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

    /// A pre-mint deployment under its basename identity `oldname`, in `dir`: one event on the
    /// legacy `proj-oldname-run` stream and one decision (`pre-d`, governing `pre.rs`) folded
    /// into the graph tagged `oldname`. Returns the store and the graph's path.
    fn pre_mint_deployment(dir: &Path) -> (Store, String) {
        use rigger::eventstore::ExpectedRevision;
        let store_path = dir.join("events.db");
        let graph_path = dir.join("graph.db").to_str().unwrap().to_string();
        let backend = Store::open(store_path.to_str().unwrap()).unwrap();
        backend
            .append(
                "proj-oldname-run",
                ExpectedRevision::Any,
                &[Event::new("UnitStarted", b"{}".to_vec())],
            )
            .unwrap();
        let legacy_graph = Projector::open(&graph_path, "oldname").unwrap();
        let payload = serde_json::json!({
            "id": "pre-d", "summary": "s", "governs": ["pre.rs"], "supersedes": "",
        });
        let mut e = Event::new(
            contextgraph::TYPE_DECISION_MADE,
            serde_json::to_vec(&payload).unwrap(),
        );
        e.position = 1;
        legacy_graph.apply(&e).unwrap();
        (backend, graph_path)
    }

    /// Run the identity migration `oldname` -> `mint123` over `backend` wired to `graph`, and
    /// assert it renamed the one legacy stream and folded its decision into `graph` (`why` names
    /// the scenario).
    fn migrates_one_stream_folding_into(backend: &Store, graph: &Projector, why: &str) {
        let moved = migrate_project_identity(backend, "mint123", "oldname", Some(graph)).unwrap();
        assert_eq!(
            moved.map(|(n, decision)| (n, decision.fold)),
            Some((1, contextgraph::Fold::Folded)),
            "{why}, and its decision folds into the wired graph"
        );
    }

    #[test]
    fn migrate_project_identity_rekeys_graph_rows_so_pre_mint_history_is_not_orphaned() {
        // Spec 28 GC5 (backward-compat): a single-project deployment behaves EXACTLY as before,
        // even across the spec-09 identity mint. The identity migration renames event STREAMS
        // (`rename_stream_prefix`), but the graph folds incrementally, so the renamed streams are
        // NEVER re-folded - its pre-mint rows keep the legacy scope. Once the read filter
        // (criterion 2) scopes reads to the minted identity, that pre-mint history would be
        // SILENTLY ORPHANED. `migrate_project_identity` must therefore re-key the graph rows the
        // same way it renames the streams, so the minted read still returns the pre-mint history.
        let dir = tempfile::tempdir().unwrap();
        // The deployment runs under its basename identity "oldname": it appends a stream under
        // the legacy namespace and folds a decision into the graph tagged "oldname".
        let (backend, graph_path) = pre_mint_deployment(dir.path());
        let graph_path = graph_path.as_str();

        // It then mints `.rigger/project.id`: the migration opens the graph under the MINTED
        // identity and migrates. Before the re-key fix the graph rows kept the legacy scope, so
        // the minted read returned nothing - the pre-mint history was orphaned.
        let graph = Projector::open(graph_path, "mint123").unwrap();
        migrates_one_stream_folding_into(
            &backend,
            &graph,
            "the one legacy stream is renamed to the minted namespace",
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
        // Spec 28 GC5 (backward-compat), crash-safety RECOVERY. Because the graph re-key runs
        // BEFORE the irreversible stream rename, a crash in the window (graph re-key committed, the
        // rename not yet) leaves the legacy namespace still populated. Recovery therefore decides
        // `Rename` again, REPLAYS the idempotent re-key (which now moves 0 rows, never a duplicate
        // or a collision), and completes the rename - so the pre-mint history stays visible under
        // the minted read filter, exactly as before the mint.
        let dir = tempfile::tempdir().unwrap();
        let (backend, graph_path) = pre_mint_deployment(dir.path());
        let graph_path = graph_path.as_str();

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
        migrates_one_stream_folding_into(
            &backend,
            &graph,
            "recovery completes the stream rename the crash interrupted",
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

    /// `rigger setup` must provision the per-project JS driver: write the three
    /// embedded runtime files into `.rigger/shim/` with the embedded content. (The
    /// npm-install step is asserted separately so this test does not depend on npm.)
    #[test]
    fn setup_provisions_the_shim_runtime_files() {
        let dir = tempfile::tempdir().unwrap();
        let shim = write_shim_files(dir.path()).expect("provisioning writes the shim files");
        assert_eq!(shim, rigger_path(dir.path(), SHIM_DIR));

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

    /// Spec 66, criterion 1 (SKILL IS A REGISTRY ENTRY): `planning-a-spec` is enumerated by
    /// [`rigger::docs::skill_registry`] exactly once, and it travels through the
    /// registry's OWN generic surfaces - [`install_skills`] and [`write_docs`] - the SAME
    /// two calls that install and render every other entry, with no planning-specific
    /// install code anywhere: neither function has a `planning`-named branch, arm, or
    /// helper (each loops `for entry in rigger::docs::skill_registry()`, see their doc
    /// comments), and no `install_planning_a_spec`-shaped function exists in this crate.
    /// This test's claim is narrower than the sibling `using-rigger` coverage above (whose
    /// job is proving the generic install/render contract exists at all - registry
    /// mechanics are spec 68's, not this spec's): here the claim is only that
    /// planning-a-spec RIDES that contract - its installed bytes, its rendered bytes, and
    /// its install/rerun semantics (Installed -> AlreadyCurrent, drift -> Refreshed) are
    /// byte-for-byte and behaviorally identical to going through
    /// [`registry_entry`]`("planning-a-spec").render(&docs_context())`, the exact generic
    /// per-entry render path.
    #[test]
    fn planning_a_spec_installs_and_renders_through_the_registry_with_no_planning_specific_code() {
        let registry = rigger::docs::skill_registry();
        assert_eq!(
            registry
                .iter()
                .filter(|e| e.name == "planning-a-spec")
                .count(),
            1,
            "planning-a-spec must be enumerated by the skill registry exactly once"
        );

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let expected_render = registry_entry("planning-a-spec").render(&docs_context());

        // `rigger setup` (install_skills) installs it at the registry's OWN generic path -
        // the same loop iteration that installs using-rigger and every other entry, with no
        // separate call or branch for planning-a-spec.
        let install_path = skill_install_path(root, "planning-a-spec");
        let outcomes = install_skills(root).expect("installing writes every skill file");
        assert_eq!(
            *outcome_for(&outcomes, "planning-a-spec"),
            InstallOutcome::Installed,
            "the first install reports a fresh install"
        );
        assert_eq!(
            std::fs::read_to_string(&install_path).unwrap(),
            expected_render,
            "the installed planning-a-spec skill must be the registry's own generic render, \
             not a planning-specific rendering path"
        );

        // Non-destructive rerun: the same no-op contract as every other entry - a rerun does
        // not even move the mtime, proving there is no separate planning-specific install
        // path with its own (possibly divergent) rerun behavior.
        let before = std::fs::metadata(&install_path)
            .unwrap()
            .modified()
            .unwrap();
        std::thread::sleep(std::time::Duration::from_millis(20));
        let outcomes = install_skills(root).expect("a no-op rerun must succeed");
        assert_eq!(
            *outcome_for(&outcomes, "planning-a-spec"),
            InstallOutcome::AlreadyCurrent,
            "an up-to-date planning-a-spec skill must be detected as current"
        );
        assert_eq!(
            std::fs::metadata(&install_path)
                .unwrap()
                .modified()
                .unwrap(),
            before,
            "an up-to-date planning-a-spec skill must NOT be rewritten"
        );

        // Drifted -> refreshed to the registry's own render, same as every other entry.
        std::fs::write(&install_path, "stale hand-edit\n").unwrap();
        let outcomes = install_skills(root).expect("re-install must succeed");
        assert_eq!(
            *outcome_for(&outcomes, "planning-a-spec"),
            InstallOutcome::Refreshed,
            "a drifted planning-a-spec skill must be refreshed"
        );
        assert_eq!(
            std::fs::read_to_string(&install_path).unwrap(),
            expected_render,
            "refreshing must overwrite the drift with the registry's own render"
        );

        // `rigger docs` (write_docs) renders it at the registry's OWN committed path -
        // skill_source_rel, the same naming convention every entry shares - byte-identical
        // to what `rigger setup` installed.
        let written = write_docs(root).expect("write_docs must render every registry entry");
        let source_path = root.join(skill_source_rel("planning-a-spec"));
        assert!(
            written.contains(&source_path),
            "write_docs must render planning-a-spec at the registry's own committed path"
        );
        assert_eq!(
            std::fs::read_to_string(&source_path).unwrap(),
            expected_render,
            "rigger docs must render planning-a-spec through the registry's own generic \
             render, identical to what rigger setup installed"
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
            rigger_path(root, DOCS_OVERLAY_FILE),
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
        std::fs::write(
            rigger_path(root, DOCS_OVERLAY_FILE),
            "base_ref: only-base\n",
        )
        .unwrap();

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
        std::fs::write(
            rigger_path(root, DOCS_OVERLAY_FILE),
            "base_ref: [not, a, string]\n",
        )
        .unwrap();
        let err = read_docs_overlay(root).expect_err("a malformed overlay must fail loudly");
        assert!(
            err.to_string().contains("docs-overlay.yml"),
            "the error must name the overlay file; got: {err}"
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
    /// each frontier via `rigger step`, spawns the returned wave natively in parallel with an
    /// `opts.phase` label built from the wave item's ROLE, lets each worker self-report via
    /// `rigger result`, records a dead worker's failure on its behalf via `rigger result
    /// --if-absent --error`, and loops until the step reports `done`. Because `meta` MUST be a pure literal
    /// (statically extracted by the Workflow runtime - no computed values / no interpolation)
    /// and unit ids are only known at runtime, the lifecycle-phase labels live in the runtime
    /// `opts.phase` strings `phaseOf` derives from role, while `meta.phases` keeps the fixed
    /// stage set. This test pins the thin-driver contract so a future edit cannot silently
    /// regress it; it supersedes the fat-workflow `buildUnit`/`PH` structure this workflow
    /// replaced.
    #[test]
    fn workflow_is_a_thin_courier_driver_with_per_unit_phase_labels() {
        let wf = RIGGER_WORKFLOW;
        // Code assertions run against comment-stripped source so the workflow's own prose
        // (which documents the removed fat-workflow constructs) cannot trip them; the meta
        // assertions run against the raw literal object body.
        let code = strip_line_comments(wf);

        // 1. meta.phases keeps the FIXED stage set as a pure up-front literal. (`Integrate`
        //    was retired for `Drive` by spec 67 criterion 5 - pinned in its own dedicated
        //    test below, not re-derived here.)
        let meta = meta_object_body(wf);
        for stage in ["Plan", "Build", "Review", "Drive"] {
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

        // 4. It SPAWNS the wave natively, one agent per wave item - PIPELINED per unit (spec
        //    89, criterion 5): each new item starts its own `runWorker` call and is tracked in
        //    the `inFlight` set rather than every item being awaited together as one
        //    `parallel(wave.map(...))` batch (the pre-criterion-5 shape this superseded), which
        //    is exactly what lets a fast unit's result be couriered while a slow sibling in the
        //    SAME wave is still running.
        assert!(
            code.contains("runWorker(req, fatal)") && code.contains("inFlight.set(req.id,"),
            "the driver must spawn the wave's agents natively, one per item, tracked in the \
             in-flight set"
        );

        // 5. Lifecycle-phase progress groups are produced at runtime from the WAVE ITEM, and
        //    every worker is labelled with one. `phaseOf`'s own role -> {Plan,Build,Review}
        //    mapping (spec 67, criterion 1) is pinned in its own dedicated test below, not
        //    re-derived here.
        assert!(
            code.contains("function phaseOf(req)"),
            "the driver must build each worker's opts.phase label from the wave item"
        );
        assert!(
            code.contains("phase: ph"),
            "each spawned worker must label its progress group with phaseOf's derived phase"
        );
        // No bare global lifecycle phase markers survive at all now (spec 67, criterion 3):
        // Build/Review/Integrate are per-unit (inside the conductor), and Plan's own global
        // `phase('Plan')` call - which used to pin the courier steps under a fixed
        // "orchestration pass" group for the run's whole duration - is retired outright. A
        // global marker for any of these would re-imply a false "all units build, then all
        // review" (or "everything is one Plan pass") order.
        for stage in ["Plan", "Build", "Review", "Integrate"] {
            assert!(
                !code.contains(&format!("phase('{stage}')")),
                "the global phase('{stage}') marker must not exist - {stage} is per-unit or \
                 retired now"
            );
            assert!(
                !code.contains(&format!("phase: '{stage}'")),
                "no agent may use the bare global `phase: '{stage}'` opts - that would collapse \
                 every unit into one global progress group"
            );
        }
        // No global phase(...) marker call of ANY kind survives - the couriers (which have no
        // unit of their own) now group under the dedicated `Drive` orchestration lane instead,
        // via the SAME per-spawn opts.phase literal mechanism every worker already uses.
        assert!(
            !code.contains("phase("),
            "no global phase(...) marker call may survive anywhere in the driver - \
             orchestration groups ride the per-spawn `phase: 'Drive'` opts literal instead"
        );
        assert!(
            code.contains("phase: 'Drive'"),
            "the step courier must group under the dedicated Drive orchestration lane"
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
        // 6f. Spec 88, criterion 3 (ESCALATION RESUMES): the wedge stop names the operator's
        // own remedy - `rigger resume-unit` - not just the bare fact of the wedge, so an
        // unattended run's failure output tells the operator exactly what to run next.
        assert!(
            code.contains("rigger resume-unit"),
            "the escalated-fixpoint stop reason must name `rigger resume-unit` as the \
             operator's remedy for a wedged unit"
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

    /// Spec 67, criterion 1 (THIS unit OWNS phase derivation; courier placement - the Drive
    /// lane call sites - is criterion 3's, a separate function, not this one's). `phaseOf`
    /// must map every wave item to one of the fixed `meta.phases` groups by role, with the
    /// two run-wide meta-stages special-cased ahead of any role read: a `plan`/`plan-critique`
    /// item is `Plan` regardless of its OWN role (the planner spawns as an `implementer` role
    /// and the critique gate spawns as `adversary`/`adjudicator` roles - unmapped, those would
    /// fall into Build/Review and split the two meta-stages across three different groups,
    /// exactly the bug this special-case prevents). Every other item is a per-criterion unit,
    /// and the ROLE half of its deterministic spawn id (`<unit>/<role>#<attempt>`, spec 18)
    /// decides the rest: the three review-tier roles (`lens:*`, `adversary`, `adjudicator`)
    /// map to `Review`; `implementer` and every other role - INCLUDING one this mapping does
    /// not recognize - map to `Build`, the fail-visible default the Design names (an unknown
    /// role groups with ongoing work, never a dropped row).
    #[test]
    fn phase_of_maps_wave_items_to_the_meta_phase_by_role_and_stage() {
        let code = strip_line_comments(RIGGER_WORKFLOW);
        assert!(
            code.contains("function phaseOf(req)"),
            "the driver must define a phaseOf(req) function"
        );
        let decl = js_declaration(&code, "function phaseOf(req) {");
        let body = &decl[decl.find('{').unwrap()..];

        // The two run-wide meta-stages are special-cased on the UNIT, ahead of any role
        // read, and resolve straight to Plan.
        assert!(
            body.contains("req.unit === 'plan'") && body.contains("req.unit === 'plan-critique'"),
            "phaseOf must special-case the plan and plan-critique meta-stages by unit, ahead \
             of role-based mapping: {body}"
        );
        let stage_check = body
            .find("req.unit === 'plan'")
            .expect("plan-stage check must exist");
        let stage_line_end = body[stage_check..]
            .find('\n')
            .map(|n| stage_check + n)
            .unwrap_or(body.len());
        assert!(
            body[stage_check..stage_line_end].contains("'Plan'"),
            "the plan/plan-critique special case must return 'Plan' on the SAME statement as \
             the unit check, ahead of any role-based branch: {body}"
        );

        // The three review-tier roles - lens:* (tier-1), adversary, adjudicator - map to
        // Review, named individually so a mapping that drops one of the three is caught.
        assert!(
            body.contains("adversary") && body.contains("adjudicator") && body.contains("lens"),
            "phaseOf must name all three review-tier roles (lens:*, adversary, adjudicator): \
             {body}"
        );
        assert!(
            body.contains("'Review'"),
            "phaseOf must map the review-tier roles to 'Review': {body}"
        );

        // implementer, and any role this mapping does not recognize, fall to the
        // fail-visible Build default: the function's LAST statement is an unconditional
        // `return 'Build'`, not one more conditional branch that could leave a role
        // unmapped (falling off the end of the function -> undefined, a dropped row).
        let last_stmt = body
            .lines()
            .rev()
            .map(str::trim)
            .find(|l| !l.is_empty() && *l != "}")
            .unwrap_or("");
        assert_eq!(
            last_stmt, "return 'Build'",
            "phaseOf's final, unconditional statement must be `return 'Build'` - the \
             fail-visible default for implementer and any unrecognized role: {body}"
        );
    }

    /// Spec 67, criterion 5 (THIS unit OWNS meta matching reality). `meta.phases` must
    /// declare exactly the four groups a spawn can render under - `Plan`, `Build`, `Review`,
    /// `Drive` - never the retired `Integrate` (conductor-only work: it spawns no agent of
    /// its own, so it was never a group a WORKER rendered under; its detail folds onto
    /// `Review` instead). And no prose anywhere in the template may still teach the
    /// `<unit>:<stage>` per-unit construction criterion 1 retired: every spawn's `opts.phase`
    /// group is now derived from its ROLE by `phaseOf` (Plan/Build/Review, shared across
    /// units), never keyed on a unit+stage pair, so a leftover "per-unit progress group" /
    /// "per-unit distinction" description would actively mislead a reader about how the
    /// grouping actually works today.
    #[test]
    fn meta_matches_reality_drops_integrate_and_the_unit_stage_construction() {
        let wf = RIGGER_WORKFLOW;
        let meta = meta_object_body(wf);

        for stage in ["Plan", "Build", "Review", "Drive"] {
            assert!(
                meta.contains(&format!("title: '{stage}'")),
                "meta.phases must declare the fixed stage '{stage}': {meta}"
            );
        }
        assert!(
            !meta.contains("title: 'Integrate'"),
            "meta.phases must drop the retired 'Integrate' phase - conductor-only work that \
             spawns no agent, so it was never a group a worker rendered under; its detail \
             folds onto 'Review': {meta}"
        );

        // Whitespace-normalize so a phrase wrapped across a `//` comment's line break is one
        // contiguous, checkable string (this is prose scanning, not JS parsing).
        let normalized = wf.split_whitespace().collect::<Vec<_>>().join(" ");
        assert!(
            !normalized.contains("<unit>:<stage>"),
            "no detail line may still describe opts.phase groups as keyed on <unit>:<stage> - \
             phaseOf (spec 67 c1) groups by ROLE-derived lifecycle phase, shared across units, \
             not by a per-unit construction"
        );
        for stale in [
            "own per-unit",
            "per-unit progress group",
            "per-unit distinction",
        ] {
            assert!(
                !normalized.contains(stale),
                "the template must not describe progress groups as per-unit (found {stale:?}) - \
                 phaseOf (spec 67 c1) groups by lifecycle phase (Plan/Build/Review/Drive), \
                 shared across units, never one group per unit"
            );
        }
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

        let decl = js_declaration(&code, "function relayAttention(step) {");
        let body = &decl[decl.find('{').unwrap()..];

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

        // "At the wave it arrived": the relay call must precede the wave-spawn CALL itself
        // (`spawnNewItems(wave)`, spec 89 criterion 5's pipelined replacement for the old
        // `if (wave.length > 0)` conditional), not merely its inner spawn-narration text - a
        // weaker check anchored on the log() line alone stays green even if a future edit
        // nests the call inside that function (review u69c6 round 1, cause genuine-defect:
        // moving the shipped, unconditional call to the block's first line left every
        // periphery test and this test green, because the call still textually preceded the
        // log() line while now running only when something is spawned). Anchoring on the
        // spawn-call line itself catches that exact nesting: a step whose wave is empty - the
        // escalated/halted/stalled-frontier "nothing left to spawn" case an unattended
        // operator most needs the narrator line for - must still get its attention relayed.
        let call_pos = code
            .rfind("relayAttention(step)")
            .expect("relayAttention(step) must be called");
        // `rfind`, not `find`: the FIRST occurrence of "spawnNewItems(wave)" is the function's
        // own declaration (`function spawnNewItems(wave) {`), which sits well before the loop
        // and would wrongly anchor this check on the wrong position; the actual CALL site
        // (inside the loop, after relayAttention) is the last occurrence.
        let wave_conditional_pos = code
            .rfind("spawnNewItems(wave)")
            .expect("the driver must still spawn newly-parked wave items");
        assert!(
            call_pos < wave_conditional_pos,
            "attention must be relayed for the step BEFORE the wave-spawn call \
             (\"at the wave it arrived\"), not nested inside it - a step with an empty wave \
             (escalated/halted/stalled-frontier) must still get its attention relayed"
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
    /// string that opens with `Advance the run one frontier` and runs up to the `{ phase:
    /// 'Drive'` options object that closes the `agent(...)` call (spec 67, criterion 3: the
    /// step courier's own progress group is the dedicated Drive orchestration lane, not the
    /// retired global `Plan` marker). Asserted over comment-stripped source so the phrases are
    /// checked in the actual prompt literal, not the file's documentation prose.
    fn step_courier_prompt(code: &str) -> &str {
        let at = code
            .find("Advance the run one frontier")
            .expect("the driver must still define the step-courier prompt");
        let end = code[at..]
            .find("{ phase: 'Drive'")
            .map(|off| at + off)
            .expect("the step-courier prompt must close with the `{ phase: 'Drive' }` options");
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
        assert_driver_guards_a_null_step(
            &strip_line_comments(RIGGER_WORKFLOW),
            "step.error",
            "the embedded driver",
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

    use rigger::metrics::{GateCounts, SpawnTiming};
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

    /// `m`'s printed canary scorecard, one line per row.
    fn canary_scorecard(m: &metrics::CanaryMetrics) -> String {
        format_canary_stats(m).join("\n")
    }

    /// The findings-volume section renders each `(tier, count)` finding count raised this run.
    fn assert_findings_raised_render(counts: &[(&str, u64)]) {
        let mut m = metrics::CanaryMetrics::default();
        for (tier, count) in counts {
            m.findings_raised.insert(tier.to_string(), *count);
        }
        let out = canary_scorecard(&m);
        assert!(
            out.contains("findings raised by tier"),
            "the findings-volume section must appear:\n{out}"
        );
        for (tier, count) in counts {
            assert!(
                out.contains(&format!("{tier:<16} {count}")),
                "{tier}'s aggregated finding count must render (an honest 0 included):\n{out}"
            );
        }
    }

    rigger::test_cases! {
        /// spec 61, FINDINGS VOLUME criterion: the per-tier finding count `project_canary`
        /// aggregates onto `CanaryMetrics::findings_raised` must reach the printed scorecard,
        /// exactly like every other canary measure `format_canary_stats` renders.
        format_canary_stats_reports_findings_raised_by_tier:
            assert_findings_raised_render(&[("lens", 7), ("adversary", 4)]);
        /// A tier that raised nothing this run still renders its honest `0` (mirrors the
        /// catch-rate section's own `0/N` discipline) rather than a blank or omitted line.
        format_canary_stats_reports_a_zero_findings_count_honestly:
            assert_findings_raised_render(&[("lens", 0), ("adversary", 0)]);
    }

    /// A never-populated scorecard omits every one of `sections` entirely (`why`).
    fn assert_empty_scorecard_omits(sections: &[&str], why: &str) {
        let out = canary_scorecard(&metrics::CanaryMetrics::default());
        for section in sections {
            assert!(!out.contains(section), "{why}:\n{out}");
        }
    }

    rigger::test_cases! {
        /// No findings-volume section at all when the metrics carry none - a caller that never
        /// populated the map (e.g. an older/foreign scorecard) gets the pre-existing render
        /// unchanged rather than an empty header with nothing under it.
        format_canary_stats_omits_the_findings_volume_section_when_empty:
            assert_empty_scorecard_omits(
            &["findings raised by tier"],
            "an empty findings_raised map must not print a bare header",
        );
    }

    /// spec 61, NO FAKE ZEROS criterion: a tier whose catch count is 0 AND the run
    /// recorded at least one correctly-rejected item with no measured attribution must
    /// render `n/a` with a reason, NEVER a fake `0/N (0.0%)` - the exact real-run
    /// misdiagnosis the spec's bug report names. A sibling tier that DID measure a catch
    /// (caught > 0) still renders its real percentage even in the same scorecard.
    #[test]
    fn format_canary_stats_renders_na_for_a_tier_with_unattributed_correct_rejects() {
        let mut m = metrics::CanaryMetrics::default();
        m.tier_catch.insert(
            "lens".to_string(),
            metrics::TierCatch {
                caught: 0,
                planted: 3,
            },
        );
        m.tier_catch.insert(
            "adversary".to_string(),
            metrics::TierCatch {
                caught: 2,
                planted: 3,
            },
        );
        m.unattributed_correct_rejects = 2;
        let out = format_canary_stats(&m).join("\n");
        assert!(
            out.contains(&format!("{:<16} n/a", "lens"))
                && out.to_lowercase().contains("no measured tier attribution"),
            "an unmeasured tier catch rate must render n/a with a reason, not 0/N:\n{out}"
        );
        assert!(
            !out.contains("0/3 (0.0%)"),
            "the fake-zero percentage must never appear for the unmeasured tier:\n{out}"
        );
        assert!(
            out.contains(&format!("{:<16} 2/3 (66.7%)", "adversary")),
            "a tier with a real measured catch still renders its true rate:\n{out}"
        );
    }

    /// A genuine `0/N (0.0%)` - no unattributed correct rejections anywhere in the run -
    /// is a REAL measurement (the tier looked and truly caught nothing) and must keep
    /// rendering the honest percentage, not n/a. This is the regression guard that keeps
    /// the n/a branch from swallowing every zero.
    #[test]
    fn format_canary_stats_renders_the_real_zero_when_attribution_was_fully_measured() {
        let mut m = metrics::CanaryMetrics::default();
        m.tier_catch.insert(
            "lens".to_string(),
            metrics::TierCatch {
                caught: 0,
                planted: 2,
            },
        );
        // unattributed_correct_rejects left at its Default of 0.
        let out = format_canary_stats(&m).join("\n");
        assert!(
            out.contains(&format!("{:<16} 0/2 (0.0%)", "lens")),
            "a genuinely-measured zero catch rate must still print 0/N (0.0%):\n{out}"
        );
        assert!(
            !out.to_lowercase().contains("n/a"),
            "n/a must not appear when every item's attribution was measured:\n{out}"
        );
    }

    /// Over `controls` known-good control items, `false_positives` of them wrongly rejected, the
    /// summary renders its control/false-positive line with the `approved` and `rejected` counts.
    fn assert_control_line(controls: u64, false_positives: u64, approved: &str, rejected: &str) {
        let out = canary_scorecard(&metrics::CanaryMetrics {
            controls,
            control_false_positives: false_positives,
            ..Default::default()
        });
        assert!(
            out.contains("control items") && out.contains("false positive"),
            "the control/false-positive line must appear on the summary:\n{out}"
        );
        assert!(out.contains(approved), "{approved} must render:\n{out}");
        assert!(
            out.contains(rejected),
            "{rejected} must render honestly, never omitted:\n{out}"
        );
    }

    rigger::test_cases! {
        /// spec 61, FALSE POSITIVES criterion: a rejected known-good control must render its own
        /// control/false-positive line on the summary, visible at the same glance as the catch
        /// rate - computed purely from `CanaryMetrics::controls`/`control_false_positives`
        /// (themselves folded from `CanaryOutcome`'s existing `planted`/`verdict_approved`
        /// fields; no new `CanaryOutcome` field). Distinct from the NO FAKE ZEROS criterion's
        /// per-tier n/a branch and the FINDINGS VOLUME criterion's finding-count aggregate.
        format_canary_stats_reports_control_items_and_false_positives:
            assert_control_line(3, 2, "1/3 approved", "2 false positive");
        /// A run with no false positives still renders the control line, with an honest `0` -
        /// mirroring the findings-volume section's own "unmeasured tier reports an honest 0, not
        /// an absent key" discipline. Not the NO FAKE ZEROS n/a case: a control's approve/reject
        /// verdict is always recorded by the adjudicator, never subject to a missing-attribution
        /// failure the way tier catch counts are.
        format_canary_stats_reports_zero_false_positives_honestly:
            assert_control_line(4, 0, "4/4 approved", "0 false positive");
    }

    /// MODEL PINNING criterion (spec 61 c7): the scorecard header names binary build,
    /// corpus hash, and every tier's resolved model id once a run has recorded one -
    /// including a tier the header reported no id for, which must render honestly as
    /// `unmeasured` rather than a blank or a fabricated value.
    #[test]
    fn format_canary_stats_reports_the_model_pinning_header_when_present() {
        let mut resolved_models = BTreeMap::new();
        resolved_models.insert("lens".to_string(), "resolved-lens".to_string());
        resolved_models.insert("adversary".to_string(), String::new());
        resolved_models.insert("adjudicator".to_string(), "resolved-adj".to_string());
        let m = metrics::CanaryMetrics {
            binary_build: "rigger 1.2.3 (build abcdef)".to_string(),
            corpus_hash: "deadbeef".to_string(),
            resolved_models,
            ..Default::default()
        };
        let out = format_canary_stats(&m).join("\n");
        assert!(
            out.contains("binary build       rigger 1.2.3 (build abcdef)"),
            "{out}"
        );
        assert!(out.contains("corpus hash        deadbeef"), "{out}");
        assert!(
            out.contains(&format!("{:<16} resolved-lens", "lens")),
            "{out}"
        );
        assert!(
            out.contains(&format!("{:<16} unmeasured", "adversary")),
            "an unreported tier renders honestly, never a fake id:\n{out}"
        );
        assert!(
            out.contains(&format!("{:<16} resolved-adj", "adjudicator")),
            "{out}"
        );
    }

    rigger::test_cases! {
        /// A run that never recorded a header event (a legacy stream, or `rigger stats
        /// --canary` on a never-run project) must not fabricate one - the section is omitted
        /// entirely, byte-for-byte like the pre-MODEL-PINNING render.
        format_canary_stats_omits_the_model_pinning_header_when_the_run_never_recorded_one:
            assert_empty_scorecard_omits(
            &["binary build", "resolved model by tier"],
            "a legacy/never-run scorecard must not fabricate a header",
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

    /// spec 61 SPAWN TIMING: `rigger stats` renders the per-agent (spawn-role) duration
    /// aggregate - mean/count/total - and separately discloses how many recorded requests
    /// went unanswered (excluded from every aggregate above, reported as its own count so a
    /// dead worker never masquerades as a zero-duration measurement).
    #[test]
    fn format_stats_surfaces_spawn_timing_and_reports_unpaired_separately() {
        let mut spawn_timing = BTreeMap::new();
        spawn_timing.insert(
            "implementer".to_string(),
            SpawnTiming {
                count: 1,
                total: std::time::Duration::from_secs(10),
            },
        );
        spawn_timing.insert(
            "adversary".to_string(),
            SpawnTiming {
                count: 2,
                total: std::time::Duration::from_secs(10),
            },
        );
        let m = Metrics {
            spawn_timing,
            unpaired_spawns: 1,
            ..Default::default()
        };
        let out = format_stats(&m).join("\n");
        assert!(
            out.contains("implementer          10.0s avg / 1 spawns / 10.0s total"),
            "implementer spawn-timing row missing/wrong:\n{out}"
        );
        assert!(
            out.contains("adversary            5.0s avg / 2 spawns / 10.0s total"),
            "adversary spawn-timing row (mean=5.0s) missing/wrong:\n{out}"
        );
        assert!(
            out.contains("1 unpaired spawn request"),
            "the unpaired count must be disclosed on its own:\n{out}"
        );
        let implementer_at = out.find("implementer ").expect("implementer row present");
        let unpaired_at = out.find("unpaired").expect("unpaired disclosure present");
        assert!(
            implementer_at < unpaired_at,
            "aggregates must render before the unpaired disclosure:\n{out}"
        );
    }

    /// A run with no recorded spawn requests at all (the pre-stepwise / cli-driver-only
    /// shape) renders a clear "no recorded spawns" line rather than an empty section or a
    /// spurious unpaired count.
    #[test]
    fn format_stats_spawn_timing_reports_no_spawns_when_none_recorded() {
        let out = format_stats(&Metrics::default()).join("\n");
        assert!(
            out.contains("spawn timing       (no recorded spawns)"),
            "a run with no spawn requests must say so plainly, not print a blank section:\n{out}"
        );
    }

    /// The "no recorded spawns" line is gated on BOTH `spawn_timing` being empty AND
    /// `unpaired_spawns == 0` (an AND, not an OR): a run with a paired aggregate but zero
    /// unpaired requests must still render the aggregate (not the "no recorded spawns"
    /// line), and must NOT render the unpaired disclosure either.
    #[test]
    fn format_stats_spawn_timing_no_spawns_line_requires_both_empty_and_zero_unpaired() {
        let mut spawn_timing = BTreeMap::new();
        spawn_timing.insert(
            "implementer".to_string(),
            SpawnTiming {
                count: 1,
                total: std::time::Duration::from_secs(3),
            },
        );
        let m = Metrics {
            spawn_timing,
            unpaired_spawns: 0,
            ..Default::default()
        };
        let out = format_stats(&m).join("\n");
        assert!(
            !out.contains("(no recorded spawns)"),
            "a run with a real paired aggregate must not say nothing was recorded:\n{out}"
        );
        assert!(
            out.contains("implementer          3.0s avg / 1 spawns / 3.0s total"),
            "the aggregate must still render:\n{out}"
        );
        assert!(
            !out.contains("unpaired spawn request"),
            "zero unpaired requests must not render an unpaired disclosure:\n{out}"
        );
    }

    /// The "no recorded spawns" line is gated on `unpaired_spawns == 0`, not merely on
    /// `spawn_timing` being empty: a run where every recorded request went unanswered
    /// (spawn_timing empty, unpaired_spawns > 0) must NOT claim nothing was recorded - it
    /// must render the (empty) aggregate header plus the unpaired disclosure.
    #[test]
    fn format_stats_spawn_timing_all_unpaired_is_not_reported_as_no_spawns() {
        let m = Metrics {
            spawn_timing: BTreeMap::new(),
            unpaired_spawns: 2,
            ..Default::default()
        };
        let out = format_stats(&m).join("\n");
        assert!(
            !out.contains("(no recorded spawns)"),
            "an all-unpaired run must not claim nothing was recorded:\n{out}"
        );
        assert!(
            out.contains("2 unpaired spawn request"),
            "the 2 unanswered requests must be disclosed:\n{out}"
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

    /// A run whose `lens:sdet` finding survival reads `raised`/`upheld` across `adjudications`
    /// recorded verdicts, `unattributed` upheld findings carrying no actor.
    fn sdet_review(raised: u64, upheld: u64, adjudications: u64, unattributed: u64) -> Metrics {
        let mut finding_survival = BTreeMap::new();
        finding_survival.insert(
            "lens:sdet".to_string(),
            metrics::FindingCounts { raised, upheld },
        );
        Metrics {
            review_quality: metrics::ReviewQuality {
                finding_survival,
                adjudications,
                upheld_unattributed: unattributed,
                ..Default::default()
            },
            ..Default::default()
        }
    }

    /// `m`'s printed stats, one line per row.
    fn stats_text(m: &Metrics) -> String {
        format_stats(m).join("\n")
    }

    /// spec 11 remediation: an in-process (cli) run has findings but records NO adjudicator
    /// verdict (no SpawnResult), so the upheld-based folds are unfed. The render must
    /// DISCLOSE that honestly rather than let a reader misread the 0% survival as the
    /// adjudicator having discarded every finding.
    #[test]
    fn stats_discloses_when_no_verdict_was_recorded_on_this_driver() {
        let out = stats_text(&sdet_review(3, 0, 0, 0));
        assert!(
            out.contains("no adjudicator verdict recorded on this run's driver"),
            "an in-process run with findings but no recorded verdict must disclose the unfed numerator:\n{out}"
        );

        // With a verdict recorded (the courier path), the disclosure is suppressed.
        let out = stats_text(&sdet_review(3, 2, 1, 0));
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
        // A verdict WAS recorded ... but the findings it upheld are unattributed here.
        let mut m = sdet_review(3, 0, 1, 2);
        m.review_reject = 5;
        for (tier, spawns) in [("lens", 2), ("adjudicator", 1)] {
            m.review_quality
                .tier_cost
                .insert(tier.to_string(), metrics::TierCost { spawns, upheld: 0 });
        }
        let out = stats_text(&m);
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
        let out = stats_text(&sdet_review(3, 0, 1, 0));
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

    /// Spec 102 criterion 2: a config that never sets `defaults.max_parallel_units` loads as
    /// UNBOUNDED (`0`), so an existing consumer's behavior is unchanged until it writes the
    /// key - the scaffold's `2` above is a fresh-project opinion, never a forced default onto
    /// an already-authored workflow.yml.
    #[test]
    fn defaults_max_parallel_units_is_unbounded_when_the_key_is_absent() {
        let wf: config::Workflow = serde_yaml::from_str("stages: {}\ngates: {}\n")
            .expect("a workflow with no defaults: block must still parse");
        assert_eq!(
            wf.defaults.max_parallel_units, 0,
            "an absent max_parallel_units key must load as 0 (unbounded), not a forced bound"
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

    /// `rigger step` SERIALIZES: while one step holds the lock, a second concurrent step
    /// REFUSES (with the driver-recognizable busy token) instead of running - so the run
    /// advances one step at a time and two steps never race the shared run state. And the
    /// refusal is not permanent: once the first releases, a later step acquires cleanly.
    #[test]
    #[serial_test::serial(cwd)]
    fn a_second_concurrent_rigger_step_refuses_and_the_lock_frees_on_release() {
        let dir = tempfile::tempdir().unwrap();
        let prev = std::env::current_dir().unwrap();
        let _restore = CwdGuard(prev);
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

    /// `read` (the read behind `what`) against an absent `events.db` returns `None` (it
    /// `reads_as`) and does NOT create the file - opening would create it and mask a never-run
    /// project as an empty one, so the guard must precede the open.
    fn assert_absent_db_reads_none<T: std::fmt::Debug>(
        read: impl FnOnce(&str) -> Result<Option<T>, Box<dyn std::error::Error>>,
        what: &str,
        reads_as: &str,
    ) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("events.db");
        let out = read(path.to_str().unwrap()).expect("absent db is not an error");
        assert!(out.is_none(), "an absent db must read as {reads_as} (None)");
        assert!(
            !path.exists(),
            "{what} must not create events.db when it is absent"
        );
    }

    rigger::test_cases! {
        /// `stats_lines` against an absent `events.db` returns `None` (the "no runs yet"
        /// signal) and - critically - does NOT create the file. Opening would create it
        /// and mask a never-run project as an empty one, so the guard must precede the open.
        stats_lines_absent_db_returns_none_and_creates_no_file: assert_absent_db_reads_none(
            |path| stats_lines(path, "proj-x", false, &StoreSelection::Sqlite),
            "stats_lines",
            "no runs",
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

    /// spec 61 SPAWN TIMING end to end, through the REAL sqlite store's `recorded_at` stamping
    /// (`eventstore::sqlite`: "the store stamps recorded_at on ingest, one clock per batch") -
    /// proving the fold in `metrics::project` and its rendering in `format_stats` are wired
    /// together on the real read path, not just unit-tested apart, for BOTH outcomes the
    /// truthfulness guard must distinguish:
    ///
    ///   - a request and its result landing in the SAME append batch (`seed_run`'s one call)
    ///     get the SAME store-stamped `recorded_at` - a deterministic, non-flaky way to drive
    ///     the real store through the exact SUSPECT same-batch-zero-duration path (finding
    ///     adv-u61c9-same-batch-append-yields-silent-zero-duration) and prove it is excluded,
    ///     not silently folded in as a fabricated zero. This is the gap the shipped test
    ///     previously missed: it seeded exactly this same-batch shape but never asserted
    ///     whether the resulting duration was real or suspect.
    ///   - a request and its result landing in SEPARATE batches get their OWN store-stamped
    ///     `recorded_at`, a genuine (if real-clock-small) elapsed duration that pairs and
    ///     counts normally - proving the guard does not over-exclude an ordinary cross-batch
    ///     pairing, the common case in production.
    ///
    /// A wholly unanswered park (no matching result at all) reaches `rigger stats` as a
    /// disclosed unpaired count in both cases.
    #[test]
    fn stats_lines_pairs_recorded_spawn_request_and_result_into_spawn_timing() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("events.db");
        let path_str = path.to_str().unwrap();

        let same_batch = test_request("u1", "impl", "implementer", 0, "same batch");
        let cross_batch = test_request("u2", "review", "adversary", 0, "cross batch");
        let dead = test_request("u3", "impl", "implementer", 0, "never answered");

        // u1's request+result share ONE append call, so the real store stamps them with the
        // SAME recorded_at - the deterministic same-batch-suspect shape.
        seed_run(
            path_str,
            "proj-me",
            &[
                same_batch.to_event().unwrap(),
                spawn::SpawnResult::ok(same_batch.id.clone(), "done")
                    .to_event()
                    .unwrap(),
            ],
        );
        // u2's request lands in its own batch; its result and u3's dead park land in a LATER,
        // separate batch, so the real store stamps u2's pair with two DIFFERENT recorded_at
        // values - a genuine cross-batch pairing. A short, real sleep guarantees the two
        // batches' store-stamped clocks are measurably distinct even under a loaded/parallel
        // test run (mirrors `spawn_timing_pairs_real_writer_events_through_a_real_store_by_role`
        // in tests/spawn_timing_periphery.rs).
        seed_run(path_str, "proj-me", &[cross_batch.to_event().unwrap()]);
        std::thread::sleep(std::time::Duration::from_millis(20));
        seed_run(
            path_str,
            "proj-me",
            &[
                spawn::SpawnResult::ok(cross_batch.id.clone(), "done")
                    .to_event()
                    .unwrap(),
                dead.to_event().unwrap(),
            ],
        );

        let lines = stats_lines(path_str, "proj-me", false, &StoreSelection::Sqlite)
            .expect("read is not an error")
            .expect("a populated run must render lines, not None");
        let out = lines.join("\n");
        assert!(
            !out.contains("implementer"),
            "the SAME-BATCH pair must NOT fold into an implementer spawn-timing row - a \
             same-batch zero duration is suspect, not a real measurement:\n{out}"
        );
        let adversary = out
            .lines()
            // "avg /" is unique to a spawn-timing aggregate row - "adversary" alone also
            // matches the (unrelated) "adversary precision" review-quality line above it.
            .find(|l| l.contains("adversary") && l.contains("avg /"))
            .unwrap_or_else(|| {
                panic!("the CROSS-BATCH pair must fold into an adversary spawn-timing row:\n{out}")
            });
        assert!(
            adversary.contains("1 spawns"),
            "the cross-batch pairing must be COUNTED, not excluded as suspect - its very \
             presence here (unlike u1's same-batch pair above) proves the production fold saw \
             a positive duration, since a non-positive one can only ever land in \
             unpaired_spawns: {adversary}"
        );
        assert!(
            out.contains("2 unpaired spawn request"),
            "the same-batch suspect pair AND the never-answered u3 park must both surface as \
             unpaired, disclosed together:\n{out}"
        );
    }

    rigger::test_cases! {
        /// `result_of_at` (the read behind `rigger reported`, and the same latest-result read
        /// `spawn_store::record_result_if_absent` consults) treats an absent `events.db` as UNREPORTED
        /// (`None`) and does NOT create the file: a never-run project has no result for any spawn,
        /// and opening would create the db, masking the edge. A `None` here makes `rigger reported`
        /// exit non-zero, correctly reporting the spawn as still unanswered.
        result_of_at_absent_db_reads_as_unreported_and_creates_no_file:
            assert_absent_db_reads_none(
            |path| result_of_at(path, "proj-x", "u/impl#0", &StoreSelection::Sqlite),
            "result_of_at",
            "unreported",
        );
    }

    /// An `events.db` in a fresh dir whose `project` run holds one successful result for `id`.
    /// Returns the dir (to keep it alive) and the db's path.
    fn db_with_one_result(project: &str, id: &str, output: &str) -> (tempfile::TempDir, String) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("events.db").to_str().unwrap().to_string();
        seed_run(
            &path,
            project,
            &[spawn::SpawnResult::ok(id, output).to_event().unwrap()],
        );
        (dir, path)
    }

    /// A spawn with no recorded result reads as UNREPORTED (`None`) even when the db exists and
    /// holds OTHER events (including other spawns' results): `result_of_at` matches on the exact
    /// spawn id, so an unanswered spawn is correctly treated as still-parked.
    #[test]
    fn result_of_at_unrecorded_spawn_reads_as_unreported() {
        // A different spawn HAS a result; the one we ask about does not.
        let (_dir, path) = db_with_one_result("proj-me", "u/other#0", "done");
        let got = result_of_at(&path, "proj-me", "u/impl#0", &StoreSelection::Sqlite)
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
        // proj-other recorded a result for an id that ALSO exists in proj-me's run.
        let (_dir, path) = db_with_one_result("proj-other", "u/impl#0", "theirs");

        // proj-me, reading the same file, sees its OWN (empty) namespace: still unreported.
        let mine = result_of_at(&path, "proj-me", "u/impl#0", &StoreSelection::Sqlite)
            .expect("read is not an error");
        assert!(
            mine.is_none(),
            "another project's result must not leak in - the read must be namespace-scoped"
        );

        // Sanity: the owner DOES see it, so the None above is the namespace boundary, not a miss.
        let theirs = result_of_at(&path, "proj-other", "u/impl#0", &StoreSelection::Sqlite)
            .expect("read is not an error");
        assert!(
            theirs.is_some(),
            "the project that owns the result must see it"
        );
    }

    // --- Spec 44, criterion 3: the always-on dash is SESSION-DETACHED from `rigger step` ---

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
        subprocess::detach_process_group(&mut cmd);
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

    // --- Spec 101, criterion 2: ONE-SHOT COMMANDS READ FROM THE BOUNDARY ---

    /// `rigger status`, `rigger progress` and the dash snapshot (its local and attached arms
    /// alike) fold exactly one read of the run - [`runscope::read::read_current_run`] - and over a log holding
    /// 200,000 derived events and two superseded runs before the boundary it costs exactly the
    /// run's own events plus the typed carry-over, asserted through the counting store double,
    /// and hands back the current run's slice and id.
    #[test]
    fn status_progress_and_the_dash_snapshot_read_the_run_from_its_boundary() {
        use crate::test_support::{seed_one_shot_fixture, ReadCountingStore};

        let inner = Store::open(":memory:").unwrap();
        let fixture = seed_one_shot_fixture(&inner, conductor::STREAM, &[]);
        let store = ReadCountingStore::new(&inner);
        let (run, run_id) = runscope::read::read_current_run(&store, conductor::STREAM).unwrap();
        assert_eq!(store.reads(), fixture.read(conductor::STREAM));
        assert_eq!(store.materialized(), fixture.cost());
        assert_eq!(run_id, "run-c");
        let types: Vec<&str> = run.iter().map(|e| e.type_.as_str()).collect();
        assert_eq!(
            types,
            [
                "RunStarted",
                "RunNote",
                "DecisionMade",
                "ReviewFinding",
                "RunNote"
            ]
        );
        assert_eq!(run[0].revision, fixture.boundary);
    }

    /// `rigger status` and the dash snapshot read the run's progress from its boundary in the
    /// progress store (spec 101): through the project namespace, ONE read of the current run's
    /// own progress stream from its start, materializing exactly that run's reports - never a
    /// superseded run's - asserted through the counting store double.
    #[test]
    fn status_and_the_dash_read_the_runs_progress_from_its_own_stream() {
        use crate::test_support::{seed_one_shot_progress, CountedRead, ReadCountingStore};

        let backend = Store::open(":memory:").unwrap();
        seed_one_shot_progress(&Namespaced::new(&backend, "alpha"), 3);
        let counted = ReadCountingStore::new(&backend);
        let reports = read_run_progress(&counted, "alpha", "run-c");
        let activities: Vec<String> = reports
            .iter()
            .map(|e| {
                serde_json::from_slice::<progress::AgentProgress>(&e.data)
                    .unwrap()
                    .activity
            })
            .collect();
        assert_eq!(activities, ["run-c step 0", "run-c step 1", "run-c step 2"]);
        assert_eq!(
            counted.reads(),
            [CountedRead::Stream {
                stream: format!("{}progress/run-c", Namespaced::prefix_for("alpha")),
                from: 0,
                forward: true,
                materialized: 3,
            }]
        );
    }

    /// `rigger watch`: one poll over the same log reads the run once from its boundary with the
    /// carried-over knowledge by type - no whole-log read for store integrity or anything else -
    /// and a healthy run reports nothing.
    #[test]
    fn a_watch_poll_reads_the_run_from_its_boundary_and_nothing_else() {
        use crate::test_support::{seed_one_shot_fixture, ReadCountingStore};

        let (_dir, loc, _identity) = watch_test_store();
        let inner = Store::open(":memory:").unwrap();
        let fixture = seed_one_shot_fixture(&inner, conductor::STREAM, &[]);
        let store = ReadCountingStore::new(&inner);
        let anomalies = watch_poll_over(&loc, &store).unwrap();
        assert_eq!(store.reads(), fixture.read(conductor::STREAM));
        assert_eq!(store.materialized(), fixture.cost());
        let signals: Vec<watch::Signal> = anomalies.iter().map(|a| a.signal).collect();
        assert_eq!(signals, [], "{anomalies:?}");
    }

    // --- Spec 83, criterion 2: HEARTBEATS ARE VISIBLE AGAIN (write/read agreement) ---

    /// `StoreLocation::repo_root` - the repo [`liveness_ages_for_wave`] resolves the scratch
    /// root from - MUST be the store's resolved OWNING root, never the process's raw cwd. A
    /// courier invoked from a nested unit worktree is the DOCUMENTED, walk-up-supported shape
    /// [`require_store_dir`] exists for (see its own doc comment: "most plausibly a unit
    /// worktree"), and that worktree's OWN git toplevel is a DIFFERENT directory than the main
    /// repo a driver's `rigger step` (which always runs from the repo root) stamped the marker
    /// under - so a cwd-based resolution looks for the marker in a scratch tree nothing ever
    /// wrote to, while the real marker sits fresh under the actual owning root (spec 83's own
    /// Problem statement: "the per-spawn liveness marker the sweep would consult is absent
    /// even while the agent is demonstrably alive").
    ///
    /// This test never touches the process's real cwd (which would race every other parallel
    /// test) - it points a fabricated `StoreLocation` at a tempdir wholly unrelated to wherever
    /// `cargo test` itself runs from, so ANY cwd-based resolution (this crate's own
    /// `git_repo()`) necessarily disagrees with it, exactly reproducing the divergence live
    /// rigger hit - pinned at the real writer (`scratch_root_from_env` + `marker_path`, the
    /// SAME functions `cmd_step` stamps a wave item's marker path with) and the real reader
    /// (`liveness_ages_for_wave` via `StoreLocation::repo_root`), never a mock of either side.
    #[test]
    fn store_location_repo_root_resolves_the_owning_root_not_the_process_cwd_so_a_real_marker_is_found(
    ) {
        let owning_root = tempfile::tempdir().unwrap();
        let rigger_dir = owning_root.path().join(RIGGER_DIR);
        std::fs::create_dir_all(&rigger_dir).unwrap();
        let loc = StoreLocation { dir: rigger_dir };

        // The REAL writer computation - byte-identical to what `cmd_step` stamps onto a wave
        // item's `marker_path` (the absolute path the thin driver frames the worker's `touch`
        // instruction around): the scratch root resolved from the owning root, then the
        // single marker-path authority.
        let run_id = "r-seam";
        let spawn_id = "u-seam/implementer#0";
        let scratch_root =
            rigger::worktree::scratch_root_from_env(owning_root.path().to_str().unwrap(), "");
        let marker = rigger::liveness::marker_path(&scratch_root, run_id, spawn_id).unwrap();
        std::fs::create_dir_all(marker.parent().unwrap()).unwrap();
        std::fs::write(&marker, b"heartbeat").unwrap();
        let touched_at = std::fs::metadata(&marker).unwrap().modified().unwrap();

        let wave = vec![spawn::WaveItem {
            id: spawn_id.to_string(),
            ..Default::default()
        }];
        let now = touched_at + std::time::Duration::from_secs(5);
        let ages = liveness_ages_for_wave(&loc.repo_root(), "", run_id, &wave, now);

        assert_eq!(
            ages.get(spawn_id).copied(),
            Some(5),
            "the reader must resolve the SAME scratch root the writer stamped the marker \
             under (the store's owning root), never a raw process-cwd read: {ages:?}"
        );
    }

    /// Spec 83 criterion 2, ROUND 2 (the reject's own required fix - a half-applied fix left
    /// standing): [`scratch_defaults`] - the shared resolver [`cmd_status`], [`watch_poll`],
    /// and [`reclaim_spawn_scratch`] ALL now delegate to - must resolve `defaults.workdir`
    /// AND `defaults.max_retries` from the store's OWNING root (`loc.dir`'s `workflow.yml`)
    /// and must succeed even when that owning root has NO loadable `.rigger/agents/` fleet
    /// at all (the exact shape `config::load` refuses outright, silently zeroing both fields
    /// for any caller that `.unwrap_or_default()`s past that unrelated failure - the round-2
    /// reject's second, independently-found axis of the same gap). This test never touches
    /// the process's real cwd at all (it only ever hands `scratch_defaults` a fabricated
    /// `StoreLocation`), which is itself part of the proof: the shared resolver has no cwd
    /// input to leak through in the first place.
    #[test]
    fn scratch_defaults_reads_the_owning_roots_config_with_no_agents_fleet_present() {
        let owning_root = tempfile::tempdir().unwrap();
        let rigger_dir = owning_root.path().join(RIGGER_DIR);
        std::fs::create_dir_all(&rigger_dir).unwrap();
        std::fs::write(
            rigger_dir.join("workflow.yml"),
            "defaults:\n  workdir: \"/configured/scratch\"\n  max_retries: 5\n",
        )
        .unwrap();
        // Fixture guard: no `.rigger/agents/` dir exists at all, so `config::load` (the
        // buggy call site's own resolver) fails outright on this exact root - proving this
        // test genuinely discriminates the validate-independent axis, not just field
        // plumbing.
        assert!(
            config_store::load(owning_root.path().to_str().unwrap()).is_err(),
            "fixture bug: config::load must fail on an agents-less root for this test to \
             discriminate the lightweight resolver from the full one"
        );

        let loc = StoreLocation { dir: rigger_dir };
        let (workdir, max_retries) = scratch_defaults(&loc);
        assert_eq!(
            workdir, "/configured/scratch",
            "must read the owning root's configured workdir via the lightweight resolver, \
             never silently defaulting to empty because config::load would have failed"
        );
        assert_eq!(
            max_retries, 5,
            "must read the owning root's configured max_retries via the lightweight \
             resolver, never silently defaulting to 0 because config::load would have failed"
        );
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

    rigger::test_cases! {
        parse_watch_args_rejects_a_non_integer_interval_a_missing_value_and_an_unknown_flag:
            assert_every_arg_list_refused(
            parse_watch_args,
            &[&["--interval", "soon"], &["--interval"], &["--bogus"]],
        );
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
            // An event recorded before the run (revision 0, corrupted below), then the run's
            // boundary in front of every anomaly: the poll reads the run from here (spec 101).
            store
                .append(
                    conductor::STREAM,
                    ExpectedRevision::Any,
                    &[
                        Event::new("E", vec![0]),
                        Event::new(
                            runscope::TYPE_RUN_STARTED,
                            br#"{"run":"watch-run"}"#.to_vec(),
                        ),
                    ],
                )
                .unwrap();
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
        }

        // An out-of-order tail (spec 71's own corruption signature): delete the run stream's
        // revision-0 row - recorded before the run's boundary - and reissue it at the newest
        // position, exactly what a stale (pre-append-guard) writer would do, and exactly the
        // shape `Store::append` itself refuses, so it can only be reproduced by going around it
        // with a raw connection - mirrors
        // `append_refuses_a_stream_whose_position_order_and_revision_order_already_
        // disagree` (src/eventstore/sqlite.rs). The reissued row now sits AFTER the boundary
        // in the log, so the run the poll reads holds it where the log recorded it.
        let scoped_run_stream = format!(
            "{}{}",
            rigger::eventstore::namespace::Namespaced::prefix_for(&identity),
            conductor::STREAM
        );
        {
            let conn = rusqlite::Connection::open(&db).unwrap();
            conn.execute(
                "DELETE FROM events WHERE stream = ?1 AND revision = 0",
                [&scoped_run_stream],
            )
            .unwrap();
            // Stamped now: the reissue is the run's newest event, and a stale timestamp would
            // read as a silent driver rather than the disorder under test.
            let now_nanos = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos() as i64;
            conn.execute(
                "INSERT INTO events (stream, type, id, data, meta, valid_from, recorded_at, \
                 revision) VALUES (?1, 'E', 'reissued', X'00', '{}', ?2, ?2, 0)",
                rusqlite::params![scoped_run_stream, now_nanos],
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
        assert!(si.contains("store integrity"));
        assert_eq!(
            by_signal(watch::Signal::StoreIntegrity).subject,
            conductor::STREAM
        );
        assert_eq!(by_signal(watch::Signal::StoreIntegrity).magnitude, 1);
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

    /// The committed implementer persona (`.rigger/agents/rust-engineer.md`), whitespace-
    /// normalized (newlines and indentation collapsed to single spaces) so a pure reflow of a
    /// wrapped paragraph never false-fails or false-passes a contiguous-phrase check.
    fn implementer_persona_normalized() -> String {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join(RIGGER_DIR)
            .join("agents")
            .join("rust-engineer.md");
        let persona = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("read committed {}: {e}", path.display()));
        persona.split_whitespace().collect::<Vec<_>>().join(" ")
    }

    /// The committed implementer persona carries (`true`) or never carries (`false`) each
    /// `(carried, clause, why)` contiguous clause.
    fn assert_implementer_persona_pins(clauses: &[(bool, &str, &str)]) {
        let normalized = implementer_persona_normalized();
        for (carried, clause, why) in clauses {
            assert_eq!(
                normalized.contains(clause),
                *carried,
                "{why}; got:\n{normalized}"
            );
        }
    }

    rigger::test_cases! {
        /// Spec 91, criterion 3 (NO SWEEP IN THE LOOP). Supersedes
        /// `implementer_persona_pins_the_seeded_mutation_step_contract` (spec 73's persona pin) and
        /// `implementer_persona_pins_the_seeded_mutation_scratch_root_registration_contract` (spec
        /// 77's TMPDIR-registration pin) - both retired here: spec 91 Design decides "the
        /// implementer persona's mutation block is removed together with its unit.diff/TMPDIR
        /// choreography", so there is no more seeded per-round step, gating clause, or TMPDIR
        /// template to pin. The mutant accounting contract those tests protected now
        /// lives in the `checkin` stage's own task text (per `tests/cli.rs`'s
        /// `rigger_workflow_yml_pins_the_checkin_stage_and_mutation_gate_definition_to_spec_91`,
        /// spec 91 criterion 2's own drift guard, naming this as criterion 3's pin) - this
        /// persona's prose for when it is spawned as the `checkin` stage, after every `implement`
        /// unit has already integrated and the `mutation` gate (spec 91 criterion 2) has already
        /// swept the whole spec diff once. This is a DRIFT GUARD, not a feature test: the
        /// implementer persona (`.rigger/agents/rust-engineer.md`) is OPERATOR CONFIGURATION
        /// seeded by the operator, not authored by any unit (spec 73 Design: "the grounder cannot
        /// ground non-code files, so no unit can own a Markdown blast radius").
        implementer_persona_pins_the_checkin_stage_survivor_closing_contract:
            assert_implementer_persona_pins(&[
            // One contiguous-phrase check, not two independently-satisfiable fragments: a
            // decomposed persona that keeps "checkin" and "stage" as bare substrings in unrelated
            // sentences (destroying the "this runs only when you are the checkin stage" gating
            // relation) must fail this test, not pass it.
            (
                true,
                "When you are spawned for the `checkin` stage",
                "the survivor-closing step must be gated on being spawned for the checkin stage, \
                 as one contiguous clause, not two independently-satisfiable fragments",
            ),
            (
                true,
                "read `mutants.out/outcomes.json`",
                "the checkin stage must read the mutation gate's own outcomes file, never \
                 stdout",
            ),
            // A survivor is always a failure (Byran 2026-09-26): one contiguous clause naming
            // the two ways it closes and the instrument narrowing that never closes it, so a
            // persona that re-admits a justification (an equivalence argument recorded as an
            // exclusion) fails this test.
            (
                true,
                "(surviving) mutant is always a failure: it is closed by a test that fails on it \
                 or by rewriting the site so the mutable token disappears, never by an \
                 `exclude_re` or `mutants::skip`",
                "a missed mutant closes only by a failing test or a rewrite, never by an \
                 exclusion, as one contiguous clause",
            ),
            (
                false,
                "is either KILLED by a strengthened test or JUSTIFIED with a concrete \
                 equivalence reason",
                "no justification closes a survivor: the kill-or-justify disjunction is gone",
            ),
            (
                true,
                "a miss still standing means the checkin stage is not done",
                "a missed mutant still standing must leave the checkin stage not done - the \
                 consequence clause itself",
            ),
            // The ACCOUNTING shape (spec 73's deterministic per-mutant DecisionMade format): one
            // contiguous clause each for the id convention, the no-new-event-type + deterministic
            // ordering, the exhaustive status vocabulary (in order), and the empty-diff case - a
            // decomposed persona that keeps these as scattered bare words could satisfy
            // independent substring checks while dropping the actual shape a downstream consumer
            // parses against.
            (
                true,
                "record the accounting as one `<unit>-mutation-accounting`",
                "the accounting must be recorded under the deterministic <unit>-mutation- \
                 accounting id (spec 73's shape)",
            ),
            (
                true,
                "DecisionMade (no new event type), deterministically ordered",
                "the accounting must be one DecisionMade, no new event type, deterministically \
                 ordered",
            ),
            (
                true,
                "caught | missed-caught (naming the catching test) | unviable | timeout",
                "the accounting's per-mutant status vocabulary must be exhaustive and in this \
                 order",
            ),
            (
                true,
                "A diff touching no Rust file records a provably-empty accounting",
                "an empty-diff checkin must still record a provably-empty accounting, never skip \
                 the step",
            ),
            // The scope boundary itself (spec 91 Design: "Nothing mutation-specific enters the
            // conductor... no cargo-mutants path"): the agent must be told the `mutation` gate
            // owns running cargo-mutants, so it never re-invokes the sweep by hand.
            (
                true,
                "the `mutation` gate itself owns running cargo-mutants",
                "the persona must name the mutation gate as the sole cargo-mutants invoker, so \
                 the agent never re-runs it by hand",
            ),
        ]);
        /// Spec 89, criterion 1 (A HALT NEVER DISCARDS A TREE): CHECKPOINT BEFORE LONG WORK.
        /// The persona must carry the checkpoint rule literally, using the design's own
        /// commit-message vocabulary ("mutation sweep", never the banned two-word invocation
        /// phrase "cargo mutants" - see `no_persona_under_rigger_agents_invokes_cargo_mutants`
        /// below, which spec 91 landed first and which this persona edit must not regress).
        implementer_persona_pins_the_checkpoint_before_long_work_contract:
            assert_implementer_persona_pins(&[
            // The trigger and the action as ONE contiguous clause - a decomposed persona
            // that keeps "mutation sweep" and "commit" as unrelated bare words (dropping
            // the "before long work, commit first" relation) must fail this test.
            (
                true,
                "Before a mutation sweep or any full lane suite, commit your current \
                 tree",
                "the checkpoint rule must fire on EITHER a mutation sweep or a full lane \
                 suite, as one contiguous clause",
            ),
            // The exact commit-message template spec 89 Design specifies, verbatim.
            (
                true,
                "`wip(<unit>): checkpoint before <mutation sweep | lane suite>`",
                "the checkpoint commit message template must be pinned verbatim",
            ),
            (
                true,
                "squash that checkpoint into your round's own commit \
                 when you report",
                "the checkpoint must be squashed into the round commit on report, never \
                 left standing as a separate commit",
            ),
            // Never the banned invocation phrase (spec 91): this persona edit must not
            // regress the already-landed no-cargo-mutants-invocation drift guard.
            (
                false,
                "cargo mutants",
                "the checkpoint rule must use the design's own vocabulary (\"mutation \
                 sweep\"), never the literal invocation phrase \"cargo mutants\"",
            ),
        ]);
    }

    /// Spec 91, criterion 3 (NO SWEEP IN THE LOOP). The structural counterpart of
    /// `implementer_persona_pins_the_checkin_stage_survivor_closing_contract` above: no persona
    /// under `.rigger/agents/` - implementer, reviewer, or the SDET author - may INVOKE
    /// `cargo mutants` itself any more. Only the `checkin` stage's `mutation` GATE (spec 91
    /// criterion 2, `.rigger/workflow.yml`) runs that command now; a persona merely reading or
    /// discussing its output (`mutants.out/outcomes.json`, or the noun "cargo-mutants") is
    /// fine, so this checks for the two-word INVOCATION phrase specifically, never the bare
    /// words "cargo" and "mutants" appearing anywhere in unrelated sentences.
    #[test]
    fn no_persona_under_rigger_agents_invokes_cargo_mutants() {
        let agents_dir = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join(RIGGER_DIR)
            .join("agents");
        let mut checked = 0;
        for entry in std::fs::read_dir(&agents_dir)
            .unwrap_or_else(|e| panic!("read committed {}: {e}", agents_dir.display()))
        {
            let path = entry.unwrap().path();
            if path.extension().and_then(|e| e.to_str()) != Some("md") {
                continue;
            }
            let text = std::fs::read_to_string(&path)
                .unwrap_or_else(|e| panic!("read committed {}: {e}", path.display()));
            let normalized = text.split_whitespace().collect::<Vec<_>>().join(" ");
            assert!(
                !normalized.contains("cargo mutants"),
                "{} must never invoke `cargo mutants` itself - only the checkin stage's \
                 `mutation` gate does now (spec 91); got:\n{normalized}",
                path.display()
            );
            checked += 1;
        }
        assert!(
            checked >= 7,
            "expected to check every seeded persona file under {} (adjudicator, adversary, \
             architecture-reviewer, planner, rust-engineer, sdet, sdet-author, plus any \
             others); checked {checked}",
            agents_dir.display()
        );
    }
}
