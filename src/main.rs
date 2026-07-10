//! The harness CLI and the single composition root: it constructs the concrete
//! adapters (event store, agent driver, grounder, projector) and injects them into
//! the conductor, which depends only on ports. `rigger run` executes the configured
//! workflow - the agent driver (`--driver cli|workflow`) and the event store
//! (`--eventstore sqlite|kurrentdb`) are selected by flag; `rigger graph` inspects
//! the context graph; `rigger init`/`setup` scaffold a project.

use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::process::Command;

use rigger::canary;
use rigger::conductor::{self, Deps};
use rigger::config;
use rigger::contextgraph::{self, sqlite::Projector, Projection};
use rigger::dash;
use rigger::driver::cli;
use rigger::driver::replay::ReplayDriver;
use rigger::eventstore::namespace::Namespaced;
use rigger::eventstore::{sqlite::Store, Direction, Event, EventStore, ExpectedRevision, Filter};
use rigger::gate::{ExecRunner, Gate, GateResult, Runner};
use rigger::grounder::Grounder;
use rigger::ledger::{self, RunState};
use rigger::metrics::{self, Metrics};
use rigger::run as runscope;
use rigger::sidecar::{PeerDecision, Sidecar};
use rigger::worktree::{RunBranchSetup, Worktree};
use rigger::{hooks, mcpserver, playbooks, progress, spawn, spec};

const RIGGER_DIR: &str = ".rigger";

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

type Res = Result<(), Box<dyn std::error::Error>>;

/// Which agent driver a `run` uses (§10): `cli` is the standalone `claude`
/// subprocess path; `workflow` is the in-Claude-Code MCP-server path.
#[derive(Clone, Copy, PartialEq, Eq)]
enum DriverKind {
    Cli,
    Workflow,
}

/// Which event-store backend a run uses (§10): `sqlite` is the embedded default;
/// `kurrentdb` is the server backend (built only behind the `kurrentdb` feature).
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
    store: StoreKind,
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
}

/// Parse `rigger run`'s flags: `--driver <cli|workflow>`, `--eventstore
/// <sqlite|kurrentdb>`, `--conn <url>`, and a single positional spec path. Unknown
/// flags and a second positional are rejected (§10).
fn parse_run_args(args: &[String]) -> Result<RunArgs, Box<dyn std::error::Error>> {
    let mut driver = DriverKind::Cli;
    let mut store = StoreKind::Sqlite;
    let mut conn = None;
    let mut spec = None;
    let mut fresh = false;
    let mut rebase_definition = false;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--fresh" => fresh = true,
            "--rebase-definition" => rebase_definition = true,
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
                    Some("sqlite") => StoreKind::Sqlite,
                    Some("kurrentdb") => StoreKind::KurrentDb,
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
    })
}

/// Construct the selected event-store backend as a boxed port (§10). `sqlite` (the
/// default) opens the embedded file under `.rigger/`; `kurrentdb` reads its
/// connection string from `--conn` or `KURRENTDB_CONN` and is built only behind the
/// `kurrentdb` feature - requesting it without the feature is a clear error, so the
/// default build stays green.
fn open_store(
    kind: StoreKind,
    conn: Option<&str>,
) -> Result<Box<dyn EventStore>, Box<dyn std::error::Error>> {
    match kind {
        StoreKind::Sqlite => Ok(Box::new(Store::open(&db_path("events.db"))?)),
        StoreKind::KurrentDb => open_kurrentdb(conn),
    }
}

#[cfg(feature = "kurrentdb")]
fn open_kurrentdb(conn: Option<&str>) -> Result<Box<dyn EventStore>, Box<dyn std::error::Error>> {
    let conn = conn
        .map(str::to_string)
        .or_else(|| std::env::var("KURRENTDB_CONN").ok())
        .ok_or(
            "run: --eventstore kurrentdb needs a connection string via --conn <url> or KURRENTDB_CONN",
        )?;
    Ok(Box::new(rigger::eventstore::kurrentdb::Store::open(&conn)?))
}

#[cfg(not(feature = "kurrentdb"))]
fn open_kurrentdb(_conn: Option<&str>) -> Result<Box<dyn EventStore>, Box<dyn std::error::Error>> {
    Err(
        "run: --eventstore kurrentdb requires the `kurrentdb` cargo feature (build with -F kurrentdb)"
            .into(),
    )
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
fn enforce_definition_pin(
    store: &dyn EventStore,
    criteria: &[String],
    definition: &str,
    rebase: bool,
) -> Res {
    match runscope::ensure_started_pinned(store, criteria, definition, rebase)? {
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
    graph: Option<&dyn Projection>,
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
            mcpserver::emit_event(&store, conductor::STREAM, graph, &args)
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
    let store_path = db_path("events.db");
    if !Path::new(&store_path).is_file() {
        return Ok(()); // a fresh project: no history to migrate
    }
    let cwd = std::env::current_dir()?;
    let minted = project_identity_at(&cwd);
    let legacy = legacy_identity_at(&cwd);
    if minted == legacy {
        return Ok(()); // no minted identity distinct from the basename
    }
    let backend = Store::open(&store_path)?;
    let graph = Projector::open(&db_path("graph.db"))?;
    if let Some(n) = migrate_project_identity(&backend, &minted, &legacy, Some(&graph))? {
        eprintln!(
            "rigger: migrated project identity - renamed {n} stream(s) from the legacy \
             namespace {legacy:?} to the minted identity {minted:?} (.rigger/{PROJECT_ID_FILE})"
        );
    }
    Ok(())
}

fn main() {
    // Point `ort` at a CUDA-enabled ONNX Runtime `.so` to `dlopen` (it is built with
    // `load-dynamic`) BEFORE anything constructs a grounder, so the turbovec grounder
    // embeds on the GPU with no user-set env - for both the standalone binary and a
    // `cargo install`ed one. A no-op when the runtime is not found or the feature is
    // off; see `rigger::ort_runtime` for the discovery order.
    //
    // SAFETY: this is the first statement in `main`, before any thread is spawned, so
    // mutating the process environment here is sound (no concurrent env reader).
    #[cfg(feature = "turbovec")]
    unsafe {
        rigger::ort_runtime::ensure_dylib_path();
    }

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
        "ground" => cmd_ground(&args[2..]),
        "reindex" => cmd_reindex(&args[2..]),
        "symbols-index" => cmd_symbols_index(&args[2..]),
        "emit" => cmd_emit(&args[2..]),
        "progress" => cmd_progress(&args[2..]),
        "result" => cmd_result(&args[2..]),
        "peers" => cmd_peers(&args[2..]),
        "validate" => cmd_validate(),
        "init" => cmd_init(),
        "setup" => cmd_setup(&args[2..]),
        "prime" => cmd_prime(),
        "help" | "-h" | "--help" => {
            usage();
            Ok(())
        }
        other => {
            eprintln!("rigger: unknown command {other:?}");
            usage();
            std::process::exit(2);
        }
    };

    // Choose the exit code, but do NOT exit yet: whether the command succeeded or failed,
    // we must first release the ONNX Runtime / CUDA runtime deterministically (below), so
    // both paths converge on a single controlled teardown.
    let code = match result {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("rigger: {e}");
            1
        }
    };

    // Tear the ONNX Runtime / CUDA runtime down EXPLICITLY, on this (main) thread, before
    // the process exits, to close the intermittent teardown heap corruption
    // (upstream pykeio/ort#564: a use-after-free in ORT's CUDA-provider teardown racing
    // the C `atexit` destructors, `malloc(): ... double linked list corrupted`, SIGABRT).
    // Releasing the environment here, while the process is healthy and single-threaded,
    // makes ORT/CUDA teardown run in the upstream-proven `ReleaseSession` -> `ReleaseEnv`
    // order so the later atexit destructors find already-released state - see `ort_teardown`
    // for the full rationale and the upstream evidence that a version bump does not fix it
    // and that explicit release does.
    //
    // WHAT THIS COVERS: it runs on BOTH the success and the error path (unlike the old
    // `libc::_exit(0)` dodge, which covered only success and skipped ALL destructors), it
    // runs every other destructor normally rather than skipping them, and it is a clean
    // no-op on any run that never built a GPU/CPU session. After it, the ordinary
    // `process::exit` runs the remaining Rust/atexit teardown normally.
    //
    // WHAT THIS DOES *NOT* CLAIM: it does not remove the buggy upstream code path itself -
    // pykeio/ort#564 remains open (closed not-planned upstream), so the corrupting CUDA-vs-
    // atexit teardown code still ships inside ORT. What we do is deprive it of the race by
    // releasing the environment first, deterministically and single-threaded. The guarantee
    // is therefore CONDITIONAL, not absolute, and rests on two invariants documented in
    // `ort_teardown`: (1) the grounder - and thus the `TextEmbedding`/`Session` - is dropped
    // BEFORE this call, so `ReleaseSession` has already run and only the env remains to
    // release; and (2) ORT keeps its environment in a leaked `G_ENV` `static` whose `Arc` is
    // never dropped, so our single `ReleaseEnv` here is the only one and cannot double-free.
    // If a future ORT release changed either invariant (e.g. began dropping `G_ENV` at exit,
    // or reordered provider teardown), this mitigation could stop holding and would need
    // revisiting. It is a robust, scoped mitigation of a live upstream bug - NOT a claim that
    // the buggy path has been removed entirely.
    #[cfg(feature = "turbovec")]
    rigger::ort_teardown::release_ort_runtime();

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
rigger ground <query> [k]   print up to k (default 8) repo references the project's\n                              \
configured grounder finds for <query>, as `file:line: text`\n  \
rigger reindex <file>...    incrementally re-embed the named files in the project's\n                              \
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
rigger validate             load and validate the workflow + agents\n  \
rigger init                 set up a project: scaffold .rigger/ (workflow.yml +\n                              \
an agents/ folder) and install the Claude Code\n                              \
SessionStart hook (it runs `rigger prime`)\n  \
rigger setup                full setup: everything `init` does, PLUS install the\n                              \
native /rigger Claude Code workflow (.claude/workflows/\n                              \
rigger.js) and provision the JS driver (.rigger/shim/ +\n                              \
npm install). After it: run `/rigger <spec>` in Claude\n                              \
Code (primary), or `rigger workflow` as a fallback\n  \
rigger prime                print recent decisions (what the hook runs)\n\n\
run/serve options:\n  \
--driver <cli|workflow>          cli (default): standalone claude subprocess;\n                                   \
workflow: in-Claude-Code MCP server\n  \
--eventstore <sqlite|kurrentdb>  sqlite (default): embedded file in .rigger/;\n                                   \
kurrentdb: server (needs the kurrentdb feature)\n  \
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
fn require_store_dir() -> Result<StoreLocation, Box<dyn std::error::Error>> {
    let cwd = std::env::current_dir()?;
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
    Ok(StoreLocation { dir })
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
/// steps (and thus two cross-process ORT/CUDA gate builds) at once. See the call site for
/// why concurrent steps deadlock the GPU.
fn acquire_step_lock() -> Result<std::fs::File, Box<dyn std::error::Error>> {
    use fs2::FileExt;
    let path = Path::new(RIGGER_DIR).join("step.lock");
    let f = std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(false)
        .open(&path)?;
    f.try_lock_exclusive()
        .map_err(|_| -> Box<dyn std::error::Error> {
            format!(
                "rigger step: {STEP_BUSY_TOKEN} in this repo (lock {}). Refusing to run \
             concurrently: two steps run two `cargo test` gates whose grounder subprocesses \
             build ORT/CUDA sessions concurrently across processes, which deadlocks the GPU. \
             Wait for the running step to finish (or kill it) and retry.",
                path.display()
            )
            .into()
        })?;
    Ok(f)
}

fn cmd_step(args: &[String]) -> Res {
    let args = parse_step_args(args)?;
    let cfg = config::load(".")?;
    let criteria = load_criteria(args.spec.as_deref())?;
    std::fs::create_dir_all(RIGGER_DIR)?;

    // Serialize concurrent `rigger step` invocations (root-cause fix for the ORT/CUDA
    // GPU deadlock). Two steps at once run two `cargo test` gates whose grounder
    // subprocesses build ORT/CUDA sessions CONCURRENTLY ACROSS PROCESSES - the documented
    // heap-corruption/deadlock hazard (Cargo.toml turbovec test-serial note). A single
    // step's own gate is already serialized internally (the grounder's CONSTRUCT_MU +
    // the tests' `file_serial`), which is why one gate runs clean; the ONLY source of
    // cross-process concurrency is OVERLAPPING steps - e.g. a driver re-couriering a step
    // while the first's minutes-long gate still runs. Held for the whole step and released
    // when this process exits (even on crash/kill), so a dead step never wedges the run.
    // The guard binds a name so it is not dropped early.
    let _step_lock = acquire_step_lock()?;

    // Anchor + check out the run branch before the conductor branches any unit worktree
    // off HEAD. Guarded on a real repo so the repo-less unit-test path is untouched. A
    // failure here aborts the step (with a clear, actionable error) rather than driving
    // the conductor on the wrong branch - isolation is a precondition, not best-effort.
    let repo = git_repo();
    if !repo.is_empty() {
        let setup = Worktree::ensure_run_branch(&repo, RUN_BRANCH, &args.base).map_err(|e| {
            format!(
                "rigger step: could not prepare the run branch {RUN_BRANCH:?} (base {:?}): {e}. \
                 The step did not run; resolve the git state (e.g. commit or stash a dirty tree) and retry.",
                args.base
            )
        })?;
        warn_on_run_branch_divergence(setup, &args);
        // The maintenance half of Gap 14: every step starts by sweeping the scratch
        // root's terminal worktrees (integrated units, review scaffolding), so leaks
        // from crashed or superseded step processes are reclaimed by the loop itself
        // instead of accumulating until a human notices a full disk.
        let root = rigger::worktree::scratch_root_from_env(&repo, &cfg.workflow.defaults.workdir);
        match rigger::worktree::sweep_terminal(&repo, &root, RUN_BRANCH) {
            Ok(0) => {}
            Ok(n) => eprintln!("rigger step: swept {n} terminal worktree(s) from {root}"),
            Err(e) => eprintln!("rigger step: scratch sweep skipped: {e}"),
        }
    }

    // Migrate a pre-spec-09 store's legacy-namespace history to the minted identity once,
    // before opening the run backend (spec 09, Gap 20). A no-op unless `.rigger/project.id`
    // was minted with an id distinct from the basename and the legacy namespace still holds
    // the history; refuses loudly if both namespaces are populated.
    migrate_local_identity()?;

    let backend = Store::open(&db_path("events.db"))?;
    let store = Namespaced::new(&backend, &project_identity());

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
        let run = runscope::start_fresh(&store, &criteria, &definition)?;
        eprintln!("rigger step: --fresh: began a new run {run} (the prior run stays in the log)");
    }

    // Definition pinning (spec 13, unit 1): pin this run's definition (a fresh run) or enforce
    // it (a live run). A drifted live-run definition WITHOUT `--rebase-definition` HALTS here,
    // loudly and before any worktree work, so a mid-campaign prompt edit can never silently
    // change replay semantics; `--rebase-definition` records the supersession and continues.
    enforce_definition_pin(&store, &criteria, &definition, args.rebase_definition)?;

    // Captured before `repo` moves into Deps: the fixpoint sweep below needs it.
    let scratch_root = if repo.is_empty() {
        None
    } else {
        Some(rigger::worktree::scratch_root_from_env(
            &repo,
            &cfg.workflow.defaults.workdir,
        ))
    };

    let graph = Projector::open(&db_path("graph.db"))?;
    let grounder = select_grounder(&cfg.workflow.defaults.grounder)?;
    // Liveness sweep (spec 10, unit 3): BEFORE the conductor replays the frontier, classify
    // any IN-FLIGHT spawn whose per-spawn heartbeat marker went stale beyond its
    // `max_wall_clock` as an infrastructure fault (a HUNG agent) and record it on the
    // spawn's id. The conductor then re-parks that fault (charging no remediation attempt -
    // the unit's code is not at fault), and it surfaces as a halt below. Best-effort and
    // scoped to the current run; a sweep failure never blocks the step.
    if let Some(root) = &scratch_root {
        match cfg.workflow.failure_taxonomy() {
            Ok(taxonomy) => {
                let pre = store.read_stream(conductor::STREAM, 0, Direction::Forward)?;
                // The current run id scopes the marker path (spec 10, unit 3): the sweep reads
                // markers under this run's subdir, so a slug-colliding re-run never reads a
                // prior run's leftover mtime. Empty before the first RunStarted (the first
                // step, where nothing is in-flight to sweep anyway).
                let run_id = runscope::current_run_id(&pre).unwrap_or_default();
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
    step.halted = rs.budget_halt;
    // Surface hung agents (spec 10, unit 3): any spawn whose LATEST result is a liveness
    // fault is a hung, unrecovered agent. Halt LOUDLY - like the budget breaker - so the
    // driver stops on a named reason instead of reading a stalled wave as a clean fixpoint;
    // a hung agent can no longer stall the wave invisibly. A budget halt already on the
    // channel takes precedence (it is the harder global rail). Recovery: record a real
    // result on the named spawn (last-write-wins supersedes the fault), then re-drive.
    if step.halted.is_none() {
        let hung = rigger::liveness::hung_spawns(runscope::current_run(&events))
            .map_err(|e| e.to_string())?;
        if !hung.is_empty() {
            step.halted = Some(rigger::liveness::halt_reason(&hung));
        }
    }
    // At a CLEAN fixpoint, reclaim the run's shared scratch areas: `agent-scratch` (probe
    // repos, verification builds workers park under <scratch-root>/agent-scratch per the
    // driver's scratch policy) and `agent-live` (the per-spawn liveness markers, spec 10
    // unit 3) - both exist only to serve in-flight spawns, and leaving them is how a run
    // leaks gigabytes of build debris (Gap 14) or unbounded stale markers. Gated on
    // `halted.is_none()`: a liveness halt sets `done` while a spawn is still hung (a
    // liveness-fault result counts as answered), and an abandoned-but-possibly-alive worker
    // may still be writing under agent-scratch - so a halted step must NOT wipe the areas out
    // from under it; only a genuine, unhalted convergence reclaims. Best-effort - never fails
    // the step.
    if step.done && step.halted.is_none() {
        if let Some(root) = &scratch_root {
            let _ = std::fs::remove_dir_all(std::path::Path::new(root).join("agent-scratch"));
            let _ = std::fs::remove_dir_all(
                std::path::Path::new(root).join(rigger::liveness::MARKER_SUBDIR),
            );
        }
    }
    println!("{}", serde_json::to_string(&step)?);
    Ok(())
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
        Ok(loc) => result_of_at(&loc.file("events.db"), &loc.identity(), id)?,
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
    let loc = require_store_dir()?;
    let backend = Store::open(&loc.file("events.db"))?;
    let store = Namespaced::new(&backend, &loc.identity());
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
) -> Result<Option<spawn::SpawnResult>, Box<dyn std::error::Error>> {
    if !Path::new(path).exists() {
        return Ok(None);
    }
    let backend = Store::open(path)?;
    let store = Namespaced::new(&backend, project);
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
/// no-op'd an unresolvable base and silently ignored `--base` on every step after the
/// first). The `{wave,done}` JSON still goes to stdout untouched; these are stderr
/// advisories, not errors - isolation is intact in every case, only the anchor differs.
fn warn_on_run_branch_divergence(setup: RunBranchSetup, args: &StepArgs) {
    match setup {
        RunBranchSetup::CreatedFromHead => eprintln!(
            "rigger step: base {:?} did not resolve, so the run branch {RUN_BRANCH:?} was anchored \
             on the current HEAD instead (unit isolation is intact, but not anchored on {:?}). \
             Fetch the base or pass an existing ref as --base to anchor there.",
            args.base, args.base
        ),
        // The run branch already exists and was reused. Reusing the default base every
        // step is the expected steady state and stays silent; only an EXPLICIT --base
        // that got ignored (because re-anchoring would orphan integrated work) is worth a
        // word, so the operator is not left thinking their re-anchor took effect.
        RunBranchSetup::Reused if args.base_explicit => eprintln!(
            "rigger step: the run branch {RUN_BRANCH:?} already exists and was reused (its \
             integrated work is preserved); --base {:?} was NOT applied. Re-anchoring an existing \
             run branch would discard integrated units; to anchor a run on {:?}, start it on a \
             repo without {RUN_BRANCH:?} (or delete that branch first).",
            args.base, args.base
        ),
        RunBranchSetup::Reused | RunBranchSetup::CreatedFromBase => {}
    }
}

/// The standalone CLI path: ground, spawn agents as `claude` subprocesses, drive
/// the DAG to integration. The store is selected by flag and wrapped in the
/// per-project namespace decorator before it is injected (§5.1.1, R9).
fn run_cli(parsed: &RunArgs) -> Res {
    let cfg = config::load(".")?;
    let criteria = load_criteria(parsed.spec.as_deref())?;
    std::fs::create_dir_all(RIGGER_DIR)?;
    // The boxed backend and its namespaced wrapper both live here, in this stack
    // frame, for the whole run: the decorator borrows the concrete store, and both
    // outlive the `conductor::run` call below.
    // Migrate a pre-spec-09 store's legacy-namespace history to the minted identity once,
    // before opening the run backend (spec 09). Local-sqlite only - the migration renames
    // streams in the local `.rigger/events.db`; a shared KurrentDB backend is out of scope.
    if parsed.store == StoreKind::Sqlite {
        migrate_local_identity()?;
    }
    let backend = open_store(parsed.store, parsed.conn.as_deref())?;
    let store = Namespaced::new(backend.as_ref(), &project_identity());
    // `--fresh`: begin a NEW run before driving, so the conductor's own `ensure_started`
    // adopts this just-minted boundary instead of the (possibly wedged) latest run. See
    // `runscope::start_fresh` - the evented restart for a terminal escalation on an
    // unchanged spec.
    fresh_run_if_requested(parsed, &store, &criteria)?;
    let graph = Projector::open(&db_path("graph.db"))?;
    let driver = cli::Driver::default();
    let grounder = select_grounder(&cfg.workflow.defaults.grounder)?;
    let deps = Deps {
        store: &store,
        driver: &driver,
        gates: &ExecRunner,
        repo: git_repo(),
        grounder: Some(grounder.as_ref()),
        graph: Some(&graph),
        criteria,
    };
    let rs = conductor::run(&cfg, &deps)?;
    print_run_state(&rs);
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
    if parsed.fresh {
        let run = runscope::start_fresh(store, criteria, &definition)?;
        println!("rigger: --fresh: began a new run {run} (the prior run stays in the log)");
    }
    enforce_definition_pin(store, criteria, &definition, parsed.rebase_definition)?;
    Ok(())
}

/// The in-Claude-Code MCP-server path (`rigger serve` / `rigger run --driver
/// workflow`): the conductor orchestrates on a background thread and this thread
/// serves the MCP bridge over stdio. The store is selected by flag and wrapped in
/// the per-project namespace decorator before it is injected into BOTH the
/// conductor and the side-car (§5.1.1, R9).
fn run_workflow(parsed: &RunArgs) -> Res {
    let cfg = config::load(".")?;
    let criteria = load_criteria(parsed.spec.as_deref())?;
    std::fs::create_dir_all(RIGGER_DIR)?;
    // One-time spec-09 identity migration before opening the run backend (local-sqlite only).
    if parsed.store == StoreKind::Sqlite {
        migrate_local_identity()?;
    }
    let backend = open_store(parsed.store, parsed.conn.as_deref())?;
    let store = Namespaced::new(backend.as_ref(), &project_identity());
    // `--fresh`: begin a NEW run before the conductor thread starts, so its `ensure_started`
    // adopts this boundary rather than the latest (possibly wedged) run.
    fresh_run_if_requested(parsed, &store, &criteria)?;
    let graph = Projector::open(&db_path("graph.db"))?;
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

/// `rigger workflow [spec]` is the turn-key one-command activation of the workflow
/// driver: it execs the Node shim (`shim/shim.mjs`), which spawns `rigger serve`
/// (this same binary, via `RIGGER_BIN`), connects an MCP client to it, and drives
/// the agent loop via the Claude Agent SDK. The user runs ONE command instead of
/// hand-wiring `rigger serve` into an MCP host.
fn cmd_workflow(args: &[String]) -> Res {
    // The shim takes an optional spec path; reject extra positionals so a typo is a
    // clear error, not a silently-ignored argument.
    if args.len() > 1 {
        return Err(format!(
            "workflow: expected at most one spec path, got {} arguments",
            args.len()
        )
        .into());
    }
    let shim = locate_shim(Path::new("."))?;
    // The shim spawns `rigger serve` itself; point it at THIS binary so the driver
    // and the served conductor are always the same build (no PATH ambiguity).
    let rigger_bin = std::env::current_exe()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|_| "rigger".to_string());

    let node = std::env::var("RIGGER_NODE").unwrap_or_else(|_| "node".to_string());
    let mut cmd = Command::new(&node);
    cmd.arg(&shim);
    if let Some(spec) = args.first() {
        cmd.arg(spec);
    }
    cmd.env("RIGGER_BIN", &rigger_bin);

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
    let mut around = String::new();
    let mut depth: i64 = 2;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--around" => {
                i += 1;
                around = args.get(i).cloned().unwrap_or_default();
            }
            "--depth" => {
                i += 1;
                depth = args.get(i).and_then(|d| d.parse().ok()).unwrap_or(2);
            }
            _ => {}
        }
        i += 1;
    }
    if around.is_empty() {
        return Err("graph: --around <id> is required".into());
    }
    let gp = Projector::open(&db_path("graph.db"))?;
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
    match stats_lines(&db_path("events.db"), &project_identity(), all)? {
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
) -> Result<Option<Vec<String>>, Box<dyn std::error::Error>> {
    if !Path::new(path).exists() {
        return Ok(None);
    }

    let backend = Store::open(path)?;
    let store = Namespaced::new(&backend, project);
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
    if !Path::new(path).exists() {
        return Ok(None);
    }
    let backend = Store::open(path)?;
    let store = Namespaced::new(&backend, project);
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
    if !Path::new(path).exists() {
        return Ok(metrics::ModelDrift::default());
    }
    let backend = Store::open(path)?;
    let store = Namespaced::new(&backend, project);
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
    migrate_local_identity()?;
    let backend = Store::open(&db_path("events.db"))?;
    let store = Namespaced::new(&backend, &project_identity());
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
    migrate_local_identity()?;
    let db = db_path("events.db");
    let events = if Path::new(&db).exists() {
        let backend = Store::open(&db)?;
        let store = Namespaced::new(&backend, &project_identity());
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
    if !Path::new(&db).exists() {
        return Err(format!(
            "rigger replay: no runs recorded yet for this project (no {db}); run `rigger run` first"
        )
        .into());
    }
    let backend = Store::open(&db)?;
    let real = Namespaced::new(&backend, &project_identity());
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
        let iso_backend = Store::open(replay_db.to_str().unwrap_or_default())?;
        let iso = Namespaced::new(&iso_backend, "rigger-replay");
        runscope::start_fresh(&iso, &criteria, &candidate_definition)?;
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
    fn run(&self, g: &Gate, _dir: &str, _target_dir: &str) -> GateResult {
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
fn cmd_dash(args: &[String]) -> Res {
    // `--export <path>` and/or `--port <n>`; loopback only (no host flag by design).
    let mut export: Option<String> = None;
    let mut port: u16 = dash::DEFAULT_PORT;
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
    let workdir = config::load(".")
        .map(|c| c.workflow.defaults.workdir)
        .unwrap_or_default();
    let scratch_root = {
        let repo = git_repo();
        if repo.is_empty() {
            String::new()
        } else {
            rigger::worktree::scratch_root_from_env(&repo, &workdir)
        }
    };

    // Fresh projection inputs on every request. Reading (not holding an open handle) is
    // what lets the dash start before the store exists and pick the run up once it does.
    let provider = move || -> Result<dash::DashInputs, String> {
        let events = dash_read_run(&events_db, &identity).map_err(|e| e.to_string())?;
        let graph = dash_read_graph(&graph_db, &events);
        let run_id = runscope::current_run_id(&events).unwrap_or_default();
        let progress = dash_read_progress(&progress_db, &identity, &run_id);
        let liveness = dash_read_liveness(&events, &scratch_root, &run_id);
        Ok((events, graph, progress, liveness))
    };

    match export {
        Some(path) => {
            let (events, graph, progress, liveness) =
                provider().map_err(|e| -> Box<dyn std::error::Error> { e.into() })?;
            let html = dash::render_export(&events, &graph, &progress, &liveness)?;
            std::fs::write(&path, html)?;
            println!("wrote dash snapshot to {path}");
            Ok(())
        }
        None => {
            let addr = SocketAddr::from(([127, 0, 0, 1], port));
            dash::serve(addr, provider)?;
            Ok(())
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
    if !Path::new(events_db).exists() {
        return Ok(Vec::new());
    }
    let backend = Store::open(events_db)?;
    let store = Namespaced::new(&backend, identity);
    let all = store.read_stream(conductor::STREAM, 0, Direction::Forward)?;
    Ok(runscope::current_run(&all).to_vec())
}

/// Build the context subgraph around the run's own units/decisions/findings from
/// `graph_db` (seeds via [`dash::graph_seeds`]). Best-effort: an absent graph (a grep-only
/// run never builds one) or any query error yields an empty graph, so the rest of the dash
/// still serves.
fn dash_read_graph(graph_db: &str, events: &[Event]) -> contextgraph::Graph {
    if !Path::new(graph_db).exists() {
        return contextgraph::Graph::default();
    }
    let seeds = dash::graph_seeds(events);
    if seeds.is_empty() {
        return contextgraph::Graph::default();
    }
    match Projector::open(graph_db) {
        Ok(p) => p.subgraph(&seeds, 2).unwrap_or_default(),
        Err(_) => contextgraph::Graph::default(),
    }
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
    // (the empty name -> grep, the scaffold default), so an agent can ground before a
    // workflow is authored rather than hitting a config error.
    let name = config::load(".")
        .map(|cfg| cfg.workflow.defaults.grounder)
        .unwrap_or_default();
    let grounder = select_grounder(&name)?;
    for r in grounder.ground(query, k) {
        println!("{}:{}: {}", r.file, r.line, r.text);
    }
    Ok(())
}

/// `rigger reindex <file>...` - incrementally re-embed the named files in the
/// project's persisted grounding index. It resolves the grounder from
/// `defaults.grounder` via [`select_reindex_grounder`] (rooted at `.`) - which, unlike
/// [`select_grounder`], loads the turbovec store WITHOUT freshening the whole tree, so
/// the named files are re-embedded exactly ONCE here rather than once by a load-time
/// freshen and again by the reindex. It then calls [`Grounder::reindex`] on the changed
/// files, so the turbovec grounder drops each file's old chunks, re-embeds its current
/// content, and persists the delta to `.rigger/grounding/` - a later `rigger ground`
/// (and the review tier the workflow runs after a unit lands) then reflects the
/// just-integrated code WITHOUT re-embedding the whole repo. For the grep / nop
/// grounders `reindex` is a no-op (they re-read the tree each call), so this command is
/// harmless there. Files are repo-relative, matching how the grounder records and
/// grounds them. At least one file is required.
fn cmd_reindex(args: &[String]) -> Res {
    if args.is_empty() {
        return Err("reindex: expected at least one file: rigger reindex <file>...".into());
    }
    // Same selection path as `cmd_ground`: honor `defaults.grounder` when a config
    // is present, else the unset default (turbovec). The grounder is rooted at `.`,
    // so the persisted store it loads/updates is this project's `.rigger/grounding/`.
    let name = config::load(".")
        .map(|cfg| cfg.workflow.defaults.grounder)
        .unwrap_or_default();
    // Use the reindex-specific constructor: it loads the persisted store WITHOUT a
    // whole-tree freshen, so `reindex` re-embeds ONLY the named files - never those
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
    let typ = args
        .first()
        .ok_or("emit: expected a type: rigger emit <type> '<json-object>'")?;
    let json_arg = args
        .get(1)
        .ok_or("emit: expected a JSON object: rigger emit <type> '<json-object>'")?;
    if args.len() > 2 {
        return Err(format!(
            "emit: expected a type and a single JSON object, got {} arguments",
            args.len()
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
    let loc = require_store_dir()?;
    let backend = Store::open(&loc.file("events.db"))?;
    let store = Namespaced::new(&backend, &loc.identity());
    let graph = Projector::open(&loc.file("graph.db"))?;

    // Same args shape the MCP tool receives, so emit_event - the shared core both
    // surfaces call - behaves identically here and over MCP.
    let tool_args = serde_json::json!({ "type": typ, "data": data });
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

    let loc = require_store_dir()?;
    // Resolve the current run READ-ONLY from the run store, only to scope the report.
    let run_backend = Store::open(&loc.file("events.db"))?;
    let run_store = Namespaced::new(&run_backend, &loc.identity());
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
    let loc = require_store_dir()?;
    let now = std::time::SystemTime::now();

    // The current run's slice of the run stream, and its id.
    let run_backend = Store::open(&loc.file("events.db"))?;
    let run_store = Namespaced::new(&run_backend, &loc.identity());
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
    // the JS driver's haiku probe was reconstructing by proxy - unit 3 retires it).
    let workdir = config::load(".")
        .map(|c| c.workflow.defaults.workdir)
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
    if json {
        println!("{}", serde_json::to_string(&view)?);
        return Ok(());
    }

    // Readable table. The blackout is visible as `last store event` age >> activity age.
    let short = |s: &str| s.chars().take(12).collect::<String>();
    if view.is_empty() {
        println!("run {}: no agents in flight", short(&run_id));
        return Ok(());
    }
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
    Ok(())
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

    let loc = require_store_dir()?;
    let backend = Store::open(&loc.file("events.db"))?;
    let store = Namespaced::new(&backend, &loc.identity());

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
        let id = d["id"].as_str().unwrap_or_default();
        let summary = d["summary"].as_str().unwrap_or_default();
        let governs = json_str_array(&d["governs"]);
        println!("decision {id} | {summary} | governs: {governs}");
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
    let loc = require_store_dir()?;
    let backend = Store::open(&loc.file("events.db"))?;
    let store = Namespaced::new(&backend, &loc.identity());

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
    if parsed.if_absent {
        // Conditional atomic record: write only if the spawn is still unanswered, never
        // overwriting an existing result. A no-op (a result already stood) is a success,
        // so the courier's `|| ...`-free single command always exits 0.
        match spawn::record_result_if_absent(&store, &res)? {
            Some(pos) => println!("recorded {kind} for {} (position {pos})", res.id),
            None => println!(
                "{} already has a result; --if-absent left it untouched",
                res.id
            ),
        }
    } else {
        let pos = spawn::record_result(&store, &res)?;
        println!("recorded {kind} for {} (position {pos})", res.id);
    }
    Ok(())
}

fn cmd_validate() -> Res {
    let root = Path::new(".");
    let cfg = config::load(".")?;
    println!(
        "config valid: {} agents, {} stages, {} gates",
        cfg.agents.len(),
        cfg.workflow.stages.len(),
        cfg.workflow.gates.len()
    );
    // Non-fatal advisories (spec 05:55): surface config/install drift so it is seen,
    // not discovered by accident. Each is a stderr warning that never changes the exit
    // status - `rigger validate` still succeeds so long as the config itself is valid.
    for advisory in validate_advisories(root) {
        eprintln!("{advisory}");
    }
    // Residue surfacing (spec 06, unit 6 / Gap 14d): report leftover scratch worktrees,
    // orphaned build caches, shadow stores, and dead `rigger/u/*` branches - with sizes -
    // so residue is seen before a disk fills. Warnings only; validate NEVER fails or
    // deletes anything (cleanup stays with the step-start sweep).
    for advisory in residue_advisories(root, &cfg) {
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
    Ok(())
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
    if installed_workflow_drifted(root) {
        advisories.push(format!(
            "warning: the installed /rigger workflow ({}) has drifted from this rigger \
             binary's embedded copy. Run `rigger setup` to refresh it so the workflow and \
             the binary that drives it stay the same build.",
            workflow_path(root).display()
        ));
    }
    if let Some(dirty) = uncommitted_rigger_advisory(root) {
        advisories.push(dirty);
    }
    advisories
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
fn residue_advisories(root: &Path, cfg: &config::Config) -> Vec<String> {
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
    let run_units = read_run_units(&cwd);
    let slugs = live_slugs(&run_units.live_branches);
    let local_branches = local_unit_branches(&cwd);
    let report = scan_residue(
        &scratch,
        &slugs,
        &run_units.dead_slugs,
        &local_branches,
        &run_units.live_branches,
    );
    format_residue(&report)
}

/// The CURRENT run's unit liveness, read from the run store the SAME way the couriers do
/// (walk UP to the owning store, scope by its identity). No store (a project that never
/// ran) means no live units, so every scratch worktree and `rigger/u/*` branch reads as
/// residue.
fn read_run_units(cwd: &Path) -> RunUnits {
    let Some(dir) = find_store_dir_from(cwd) else {
        return RunUnits::default();
    };
    let loc = StoreLocation { dir };
    let Ok(backend) = Store::open(&loc.file("events.db")) else {
        return RunUnits::default();
    };
    let store = Namespaced::new(&backend, &loc.identity());
    match store.read_stream(conductor::STREAM, 0, Direction::Forward) {
        Ok(events) => current_run_units(&events),
        Err(_) => RunUnits::default(),
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

    // 4. Write .gitignore entries for machine-local installs (.claude/ and .rigger/shim/)
    // when they are not already ignored or tracked. Record WHICH patterns were appended
    // so the summary reports the real gitignore change and nothing it did not do.
    let mut gitignore_added = Vec::new();
    for pattern in [".claude/", ".rigger/shim/"] {
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

/// Write a .gitignore entry for the given pattern if it is not already ignored or
/// tracked, returning whether it APPENDED an entry (`true`) or left `.gitignore`
/// untouched (`false`). Idempotent: a rerun finds the entry already present and is a
/// no-op, so setup never pollutes `.gitignore` with duplicates.
fn write_gitignore_entries(root: &Path, pattern: &str) -> Result<bool, Box<dyn std::error::Error>> {
    let gitignore_path = root.join(".gitignore");
    let normalized_pattern = pattern.trim_end_matches('/');

    // Check if already in .gitignore
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

/// What [`install_workflow`] did to `.claude/workflows/rigger.js`, so `rigger setup`
/// can REPORT a refresh but stay a silent no-op when nothing drifted (spec 05,
/// criterion 4: setup is re-runnable - it detects and refreshes a drifted installed
/// workflow, reports the refresh, and changes nothing when the workflow is current).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WorkflowInstall {
    /// No workflow was installed before; the embedded copy was written fresh.
    Installed,
    /// An installed workflow had DRIFTED from the embedded copy (e.g. an older
    /// `rigger` build wrote it) and was refreshed to match this binary.
    Refreshed,
    /// The installed workflow already matched the embedded copy byte-for-byte, so
    /// nothing was written - a rerun changes nothing (not even the file's mtime,
    /// which the grounder keys off).
    AlreadyCurrent,
}

/// Install (or refresh) the native `/rigger` Claude Code workflow at
/// `<root>/.claude/workflows/rigger.js`, returning [what it did](WorkflowInstall).
///
/// It COMPARES the installed file against the embedded [`RIGGER_WORKFLOW`] and writes
/// ONLY when the file is absent (a fresh install) or has drifted (a stale copy from an
/// older `rigger` build): an up-to-date workflow is left untouched so a `rigger setup`
/// rerun is a true no-op. A drifted file is overwritten so an upgrade refreshes the
/// workflow to match the binary - the workflow and the conductor / CLI it drives stay
/// the same build. Claude Code auto-discovers `.js` here, so the user can run `/rigger
/// <spec>` immediately, with no registration. Rooted at `root` so it is testable
/// against a temp dir. The installed path is [`workflow_path`]`(root)`.
fn install_workflow(root: &Path) -> Result<WorkflowInstall, Box<dyn std::error::Error>> {
    let path = workflow_path(root);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    // Compare bytes so a byte-identical installed workflow is left in place - rewriting
    // identical content would still bump the file's mtime, which is an observable side
    // effect (the grounder's staleness gate keys off mtime), not a true no-op.
    let existed = path.exists();
    if existed && std::fs::read(&path)? == RIGGER_WORKFLOW.as_bytes() {
        return Ok(WorkflowInstall::AlreadyCurrent);
    }
    std::fs::write(&path, RIGGER_WORKFLOW)?;
    Ok(if existed {
        WorkflowInstall::Refreshed
    } else {
        WorkflowInstall::Installed
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

    let workflow_changed = workflow != WorkflowInstall::AlreadyCurrent;
    if !scaffold.changed() && !workflow_changed && !provisioned && !imported {
        // A silent no-op: nothing drifted, so there is nothing to report.
        return Ok(());
    }

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
        WorkflowInstall::Installed => println!(
            "installed the /rigger workflow (.claude/workflows/rigger.js) - run it with: /rigger \
             <spec-path>"
        ),
        WorkflowInstall::Refreshed => println!(
            "refreshed the drifted /rigger workflow (.claude/workflows/rigger.js) to match this \
             rigger build"
        ),
        WorkflowInstall::AlreadyCurrent => {}
    }
    // The starter-fleet pointer fires exactly when default agents were NEWLY
    // scaffolded (spec 05 line 57 clause 2): the per-artifact report's `new_agents`
    // is the scaffolded-new signal.
    if !scaffold.new_agents.is_empty() {
        print_scaffold_pointer();
    }
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

fn cmd_prime() -> Res {
    let path = db_path("events.db");
    if !Path::new(&path).exists() {
        println!("# Rigger: no decisions recorded yet (run `rigger run` to start).");
        return Ok(());
    }
    let store = Store::open(&path)?;
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

/// Build the grounder named by `defaults.grounder` (§3.2, §5.4, R4). Turbovec is the
/// DEFAULT grounder: the turbovec names (`vector`/`turbovec`) AND an UNSET / empty
/// `defaults.grounder` resolve to the real semantic engine. `grep` and `nop` resolve
/// via `grounder::grounder_for` and are reachable ONLY when named explicitly.
///
/// When the binary is built WITHOUT the `turbovec` feature, resolving to turbovec is
/// a LOUD error (a clear message + non-zero exit), never a silent degrade to grep -
/// that silent degrade is exactly what hid turbovec being absent for a whole session.
/// Grep runs ONLY when the user writes `grounder: grep`.
#[cfg(feature = "turbovec")]
fn select_grounder(name: &str) -> Result<Box<dyn Grounder>, Box<dyn std::error::Error>> {
    if rigger::grounder::resolves_to_turbovec(name) {
        // Building the index can fail for a real, distinct reason (e.g. the embedding
        // model cannot be loaded); that is its OWN loud error, not a grep fallback.
        // `new` freshens any tree drift on load, which is what the grounding-read paths
        // (`ground`/`run`/`serve`) want.
        let tv = rigger::grounder::turbovec::Turbovec::new(".")
            .map_err(|e| format!("turbovec grounder unavailable: {e}"))?;
        return Ok(Box::new(tv));
    }
    // `symbols` resolves to the real structural grounder when the feature is built (it opens or
    // builds+persists the index over the repo root); a build WITHOUT the feature falls through to
    // `grounder_for`, whose `symbols` arm is the loud no-silent-degrade error.
    #[cfg(feature = "symbols")]
    if name.trim().eq_ignore_ascii_case("symbols") {
        return Ok(Box::new(
            rigger::grounder::symbols::grounder::Symbols::open(".", None),
        ));
    }
    Ok(rigger::grounder::grounder_for(name, ".")?)
}

#[cfg(not(feature = "turbovec"))]
fn select_grounder(name: &str) -> Result<Box<dyn Grounder>, Box<dyn std::error::Error>> {
    // No turbovec feature compiled in: `grounder_for` returns the loud
    // "built without the turbovec feature" error for the default / turbovec names,
    // and resolves grep / nop normally. We never silently degrade to grep.
    // `symbols` still resolves to the structural grounder when THAT feature is built (it is
    // independent of turbovec); without it, `grounder_for` returns the loud symbols error.
    #[cfg(feature = "symbols")]
    if name.trim().eq_ignore_ascii_case("symbols") {
        return Ok(Box::new(
            rigger::grounder::symbols::grounder::Symbols::open(".", None),
        ));
    }
    Ok(rigger::grounder::grounder_for(name, ".")?)
}

/// The grounder for `rigger reindex`, which differs from [`select_grounder`] ONLY for
/// turbovec: it constructs via `Turbovec::new_for_reindex`, which loads the persisted
/// store WITHOUT freshening tree drift. `reindex` then re-embeds exactly the named
/// files; using the freshening `new` here would re-embed every drifted file on load and
/// then the named files AGAIN - a double-embed.
///
/// EVERY OTHER name resolves IDENTICALLY to [`select_grounder`], and MUST: the two are one
/// grounder-selection concern, not two authorities to keep in sync by hand. `symbols` resolves
/// to the SAME real `Symbols::open` here as in `select_grounder` - `Symbols::open` only LOADS
/// the persisted index (it does not freshen the whole tree the way turbovec's `new` does), so
/// opening it for a reindex re-parses ONLY the named files and there is no double-work to avoid;
/// omitting this arm is exactly the parallel-selector drift that made `rigger reindex` under
/// `defaults.grounder: symbols` return the false `symbols_feature_missing_error` while the
/// feature was built. grep / nop have no index, so their `reindex` is a no-op.
#[cfg(feature = "turbovec")]
fn select_reindex_grounder(name: &str) -> Result<Box<dyn Grounder>, Box<dyn std::error::Error>> {
    if rigger::grounder::resolves_to_turbovec(name) {
        let tv = rigger::grounder::turbovec::Turbovec::new_for_reindex(".")
            .map_err(|e| format!("turbovec grounder unavailable: {e}"))?;
        return Ok(Box::new(tv));
    }
    // `symbols` resolves to the SAME structural grounder `select_grounder` builds (open only loads
    // the persisted index, so there is no freshen-on-open double-work); a build WITHOUT the feature
    // falls through to `grounder_for`, whose `symbols` arm is the loud no-silent-degrade error.
    #[cfg(feature = "symbols")]
    if name.trim().eq_ignore_ascii_case("symbols") {
        return Ok(Box::new(
            rigger::grounder::symbols::grounder::Symbols::open(".", None),
        ));
    }
    Ok(rigger::grounder::grounder_for(name, ".")?)
}

#[cfg(not(feature = "turbovec"))]
fn select_reindex_grounder(name: &str) -> Result<Box<dyn Grounder>, Box<dyn std::error::Error>> {
    // `symbols` resolves identically to `select_grounder` (open only loads the persisted index, so
    // no freshen-on-open double-work); without the feature, `grounder_for` returns the loud error.
    #[cfg(feature = "symbols")]
    if name.trim().eq_ignore_ascii_case("symbols") {
        return Ok(Box::new(
            rigger::grounder::symbols::grounder::Symbols::open(".", None),
        ));
    }
    Ok(rigger::grounder::grounder_for(name, ".")?)
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

fn print_run_state(rs: &RunState) {
    println!("run state:");
    for (name, u) in &rs.units {
        println!("  {:<20} {}", name, u.status.as_str());
    }
    if rs.done() {
        println!("done: every unit integrated");
    } else {
        println!("incomplete: not every unit integrated");
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
grounder: turbovec      # turbovec (default; the real semantic grounder) | grep | nop\n  \
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
        // The scaffold sets turbovec EXPLICITLY (visible, not implicit) - it is the
        // default grounder and the default cargo feature.
        assert_eq!(cfg.workflow.defaults.grounder, "turbovec");
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
    fn parse_run_args_defaults_to_cli_sqlite() {
        let a = parse_run_args(&[]).unwrap();
        assert!(a.driver == DriverKind::Cli);
        assert!(a.store == StoreKind::Sqlite);
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
        assert!(a.store == StoreKind::KurrentDb);
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

    /// With the default build (no `kurrentdb` feature), requesting the server store
    /// is a clear error, never a silent fallback - the default build stays green.
    #[cfg(not(feature = "kurrentdb"))]
    #[test]
    fn kurrentdb_without_the_feature_is_a_clear_error() {
        match open_store(StoreKind::KurrentDb, Some("kurrentdb://x")) {
            Ok(_) => panic!("kurrentdb must not open without the feature"),
            Err(e) => assert!(
                e.to_string().contains("kurrentdb"),
                "the error must name the missing feature; got: {e}"
            ),
        }
    }

    /// With the turbovec feature compiled OUT, selecting the DEFAULT grounder (an
    /// unset name) or an explicit turbovec/vector name FAILS LOUDLY - a clear error
    /// naming turbovec, the missing feature, and the explicit grep opt-out - and never
    /// silently degrades to grep. This is the regression guard for the silent degrade
    /// that hid turbovec being absent for a whole session.
    #[cfg(not(feature = "turbovec"))]
    #[test]
    fn select_grounder_fails_loudly_without_the_turbovec_feature() {
        for name in ["", "turbovec", "vector"] {
            let err = select_grounder(name)
                .err()
                .unwrap_or_else(|| panic!("{name:?} must fail loudly without the feature"));
            let msg = err.to_string();
            assert!(
                msg.contains("turbovec") && msg.contains("feature") && msg.contains("grep"),
                "the loud error must name turbovec, the feature, and the grep opt-out; got: {msg}"
            );
        }
        // grep and nop are the explicit-only opt-outs and still resolve fine.
        assert!(select_grounder("grep").is_ok());
        assert!(select_grounder("nop").is_ok());
        // An unknown name is a hard error too, not a silent grep fallback.
        assert!(select_grounder("bogus").is_err());
    }

    /// With the turbovec feature compiled IN, grep is still reachable when named
    /// EXPLICITLY (the deliberate literal-grounder opt-out), and an unknown name is a
    /// hard error rather than a silent grep fallback. (The turbovec / default path is
    /// exercised by the grounder's own model-loading test, which downloads weights.)
    #[cfg(feature = "turbovec")]
    #[test]
    fn select_grounder_with_feature_resolves_grep_explicitly_and_rejects_unknown() {
        assert!(
            select_grounder("grep").is_ok(),
            "explicit grep must resolve even with the turbovec feature on"
        );
        assert!(select_grounder("nop").is_ok());
        assert!(
            select_grounder("bogus-grounder").is_err(),
            "an unknown grounder name must be a hard error, not a silent grep fallback"
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
            WorkflowInstall::Installed,
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
            WorkflowInstall::AlreadyCurrent,
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
            WorkflowInstall::Refreshed,
            "a drifted workflow must be refreshed"
        );
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            RIGGER_WORKFLOW,
            "refreshing must overwrite the drifted workflow with the embedded content"
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
    /// REFUSES (with the driver-recognizable busy token) instead of running - the root-cause
    /// fix for the cross-process ORT/CUDA deadlock two overlapping gate builds cause. And the
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
        let held = acquire_step_lock().expect("the first step must acquire the lock");
        // A second concurrent step must REFUSE fast (not block, not double-run) and carry the
        // token the driver keys on to back off rather than tear the run down.
        let err = acquire_step_lock().expect_err("a second concurrent step must refuse");
        assert!(
            err.to_string().contains(STEP_BUSY_TOKEN),
            "the refusal must carry the busy token for the driver: {err}"
        );
        // Releasing the first frees the lock so a later step proceeds - the refusal is
        // transient, not a wedge.
        drop(held);
        let _reacquired =
            acquire_step_lock().expect("after the first releases, a later step acquires cleanly");
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

        let out = stats_lines(path_str, "proj-x", false).expect("absent db is not an error");
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

        let out =
            stats_lines(path_str, "proj-me", false).expect("empty run stream is not an error");
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
        let mine = stats_lines(path_str, "proj-me", false).expect("read is not an error");
        assert!(
            mine.is_none(),
            "stats must be namespace-scoped: another project's run must not leak in"
        );

        // Sanity: the other project's run IS visible to it, so the data really is there
        // and the None above is the namespace boundary, not a read failure.
        let theirs = stats_lines(path_str, "proj-other", false).expect("read is not an error");
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

        let lines = stats_lines(path_str, "proj-me", false)
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

        let got = result_of_at(path_str, "proj-x", "u/impl#0").expect("absent db is not an error");
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

        let got = result_of_at(path_str, "proj-me", "u/impl#0").expect("read is not an error");
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

        let got = result_of_at(path_str, "proj-me", "u/impl#0")
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
        let mine = result_of_at(path_str, "proj-me", "u/impl#0").expect("read is not an error");
        assert!(
            mine.is_none(),
            "another project's result must not leak in - the read must be namespace-scoped"
        );

        // Sanity: the owner DOES see it, so the None above is the namespace boundary, not a miss.
        let theirs =
            result_of_at(path_str, "proj-other", "u/impl#0").expect("read is not an error");
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
}
