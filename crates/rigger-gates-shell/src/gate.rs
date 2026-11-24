//! Rigger's verification discipline: a gate is a command plus a trust level, it
//! yields compact evidence (never a raw log), and its autonomy moves on a
//! bidirectional ratchet so a graduated gate can never silently auto-pass bad
//! work. `Runner` is the interface; `ExecRunner` is the adapter.

use std::collections::BTreeMap;
use std::process::Command;

use crate::budget::BuildBudget;
use crate::subprocess::{find_executable, inherit_on_exec, session_dir, shell_command, SearchPath};

pub use rigger_domain::gate::*;

/// Runner runs a gate command in a working directory (`dir`, "" = current dir) with the
/// declared [`BuildEnv`] resolved at the gate's site build cache ([`gate_site_cache`]): its
/// unit's `target_dir` when set, else the shared gate cache `build_cache_dir`, "" for none.
/// The default/empty `BuildEnv` sets nothing, so the gate inherits the runner's environment.
///
/// A gate that runs INSIDE a unit's worktree is handed a unit-keyed `target_dir` (Gap 19)
/// so a declaration over `{build_cache}` never has divergent unit trees share one build
/// cache: a compile error a gate sees is then always that unit's own, never a concurrent
/// neighbour poisoning a shared target. A gate on the single integrated tree (the deferred
/// phase-boundary gate) is handed "" and builds over the shared gate cache.
///
/// The SAME non-empty `target_dir` signal also fences the gate's own store resolution
/// (spec 70 criterion 3, "the gate store fence"): [`ExecRunner::run`] pins
/// [`STORE_FENCE_ENV`] to a scratch location derived from `target_dir`, so a store-opening
/// courier the gate's spawned process runs (a test invoking `rigger emit`/`result`/`peers`/
/// `reported`, say) can never walk up past the unit worktree into the repo's LIVE run
/// stream - `main.rs::require_store_dir` honors that env var. A gate on the integrated
/// tree ("" target_dir) is never fenced by that path, exactly as it is never given a
/// target_dir. The fenced-sibling scratch dir this creates is reclaimed by the SAME
/// authority that reclaims `target_dir`'s own `cargo-target-<slug>` sibling (`worktree::
/// reclaim_cache_sibling`, sharing [`STORE_FENCE_SUFFIX`]'s naming), so a fenced gate
/// leaves no more orphaned state on disk than the pre-existing cache sibling already did.
///
/// `store_fence` is a SECOND, caller-injected fence override for a gate that has no
/// `target_dir` to derive one from yet still needs fencing: a standalone review worktree
/// (`rigger-review-*`) owns no per-unit build cache (so `target_dir` is always ""), but its
/// EXHAUSTIVE gate pass still runs real store-opening couriers inside a scratch-root
/// worktree. `store_fence` is used ONLY when `target_dir` is empty (the `target_dir` branch
/// always wins when both could apply); "" means no override, leaving the gate unfenced
/// exactly as before this parameter existed. Deliberately a plain value, not derived here:
/// the CALLER (`conductor::run_gates`) computes it via `worktree::review_fence_sibling(dir)`
/// and threads it in, so this module never depends on `worktree` - the one-directional
/// `gate` -> nothing / `conductor` -> `{gate, worktree}` dependency shape stays intact
/// rather than growing a cycle back into `worktree`.
///
/// This is a process-tree-wide fence, not a per-courier one: EVERY descendant of the
/// gate's spawned process inherits [`STORE_FENCE_ENV`] via normal env-var inheritance,
/// including a periphery test that spawns the product binary to prove its OWN
/// store-resolution/precedence logic against a fixture of its own choosing - a caller the
/// fence was never meant to redirect. Such a test must defensively clear the var before it
/// spawns its own courier (`tests/common::rigger_courier` is the one shared authority every
/// periphery suite already spawns the product through, so this is handled once there
/// rather than by each test file remembering it).
///
/// `gate_scratch` (spec 113, THE GATE SCRATCH ROOT IS HANDED GENERICALLY) is the unit's gate
/// scratch root, handed to the gate as [`GATE_SCRATCH_ENV`]: the `rigger-gate-<slug>` sibling
/// of the unit worktree, a directory the unit's gates share and no other unit's gate sees. The
/// CALLER (`conductor::run_gates`) computes it via
/// `worktree::unit_sibling(dir, UNIT_GATE_SCRATCH_PREFIX)` and threads it in, exactly like
/// `target_dir`, so this module still never depends on `worktree`. Empty for a gate that runs
/// for no unit (a standalone review worktree, a run with no worktree, the deferred gate), and
/// then [`ExecRunner::run`] REMOVES the variable from the gate's environment, so no gate ever
/// inherits a root from the process that runs it. rigger never creates the root; a gate that
/// uses it does, and touches only the names it created there.
///
/// A non-empty `build_cache_guard` is locked shared through [`hold_shared_lock`] before the gate
/// command runs (spec 121 criterion 19, spec 125 criterion 10): a guard that cannot be locked is
/// the [`ProbeFault`] `Err`, never a [`GateResult`], and the gate command never runs, so nothing
/// is built anywhere.
pub trait Runner: Send + Sync {
    #[allow(clippy::too_many_arguments)]
    fn run(
        &self,
        g: &Gate,
        dir: &str,
        target_dir: &str,
        gate_scratch: &str,
        build_cache_dir: &str,
        build_cache_guard: &str,
        store_fence: &str,
        build_env: &BuildEnv,
        budget: &BuildBudget,
    ) -> Result<GateResult, ProbeFault>;
}

/// A requested shared-cache guard that [`hold_shared_lock`] could not lock (spec 121 criterion
/// 19): its parent directory could not be created, the guard could not be opened, or its
/// filesystem refused an advisory lock. An infrastructure fault, not a gate failure: no wait
/// and no rerun changes a local filesystem's answer, only the operator's fix, so its text names
/// the guard path, the OS error and that fix.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error(
    "build cache guard {guard:?} failed its probe: {error}; fix the path or set the project's \
     scratch root to a filesystem with advisory locks, then relaunch"
)]
pub struct ProbeFault {
    pub guard: String,
    pub error: String,
}

/// BuildEnv is the ONE build-environment authority: the workflow's declared `build.env`,
/// checked once by [`declared`](Self::declared), plus the pairs a caller layers on with
/// [`with_var`](Self::with_var). [`at`](Self::at) resolves it at one site's build cache into the
/// [`SiteEnv`] that site's process carries: a gate build ([`ExecRunner::run`], over
/// [`gate_site_cache`]) and an agent spawn (the conductor's `spawn_env`, over the spawn's unit
/// sibling), so a gate build and an agent's own build see the same declared variables, each
/// over its own cache. rigger exports nothing else of a build environment: a variable no
/// declaration names reaches a gate or a spawn as rigger's own process holds it.
/// `driver::replay`'s wire contract does not carry these vars.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BuildEnv {
    /// The declared variables in name order, each its template's pieces.
    declared: Vec<(String, Vec<Piece>)>,
    /// The `build.jobs` a `{jobs}` placeholder resolves to; 0 resolves it nowhere.
    jobs: u32,
    /// The [`with_var`](Self::with_var) pairs, in the order they were layered on.
    with: Vec<(String, String)>,
}

/// One piece of a declared variable's template: literal text, or a placeholder resolved at a
/// site.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Piece {
    Text(String),
    BuildCache,
    Jobs,
}

/// The names rigger sets itself on a gate or a spawn, which a `build.env` declaration may not
/// take in any letter case: the gate store fence, the gate scratch root, the run base, the spawn
/// marker, the gate socket, the event store connection and the two agent credential names. A
/// host whose variable names ignore case (Windows) reads any spelling as the variable rigger
/// sets, and one workflow means the same on every host.
const RESERVED_ENV: [&str; 8] = [
    STORE_FENCE_ENV,
    GATE_SCRATCH_ENV,
    rigger_domain::spawn::RUN_BASE_ENV,
    rigger_domain::spawn::SPAWN_ENV,
    rigger_domain::gate_channel::GATE_SOCKET_ENV,
    rigger_domain::spawn::CONN_ENV,
    rigger_domain::spawn::AGENT_TOKEN_VAR,
    rigger_domain::spawn::RIGGER_AGENT_TOKEN_VAR,
];

/// The build environment one site's process carries: the declared variables that resolved
/// there and the [`BuildEnv::with_var`] pairs after them, and the site's build cache when an
/// exported variable uses `{build_cache}`, else empty.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SiteEnv {
    pub vars: Vec<(String, String)>,
    pub cache: String,
}

impl SiteEnv {
    /// Set every pair on `cmd`, whether it is a gate's own `Command` ([`ExecRunner::run`]) or
    /// an agent driver's.
    pub fn apply(&self, cmd: &mut Command) {
        for (k, v) in &self.vars {
            cmd.env(k, v);
        }
    }
}

impl BuildEnv {
    /// The one check of the workflow's `build.env`, holding the declaration it accepts with
    /// `build.jobs`. Each variable in name order: a name that is not a letter or underscore
    /// followed by letters, digits and underscores, a name rigger sets itself in any letter
    /// case, a placeholder
    /// other than `{build_cache}` and `{jobs}`, and a brace that is neither doubled nor part of
    /// a placeholder are each refused with a message naming the variable.
    pub fn declared(build: &rigger_domain::config::BuildConfig) -> Result<BuildEnv, String> {
        let declared = build
            .env
            .iter()
            .map(|(name, value)| {
                if !is_variable_name(name) {
                    return Err(format!(
                        "build.env: {name:?} is not a variable name; a name is a letter or \
                         underscore followed by letters, digits and underscores"
                    ));
                }
                if RESERVED_ENV.iter().any(|r| r.eq_ignore_ascii_case(name)) {
                    return Err(format!(
                        "build.env: {name} is set by rigger itself; declare another name"
                    ));
                }
                Ok((name.clone(), template_pieces(name, value)?))
            })
            .collect::<Result<_, String>>()?;
        Ok(BuildEnv {
            declared,
            jobs: build.jobs,
            with: Vec::new(),
        })
    }

    /// Layer one more `(name, value)` pair onto this environment, exported after the declared
    /// variables at every site - for a caller that must add a value the workflow's `build`
    /// block cannot carry (`$RIGGER_RUN_BASE`, the run's own recorded state, which the check-in
    /// stage's gates diff the whole spec's work against). A no-op for an empty `value` -
    /// "empty means off", the same convention every other optional var in this environment
    /// already follows. No new [`Runner::run`] parameter and no new call site to update: every
    /// existing caller already threads `build_env` through unconditionally, so a value layered
    /// on here reaches every gate command this environment is passed to and every agent spawn
    /// whose environment is built from it.
    pub fn with_var(mut self, name: &str, value: &str) -> BuildEnv {
        if !value.is_empty() {
            self.with.push((name.to_string(), value.to_string()));
        }
        self
    }

    /// This environment at the site whose build cache is `cache` (empty for a site with none):
    /// each declared variable, in name order, whose placeholders all resolve non-empty there -
    /// `{build_cache}` to `cache`, `{jobs}` to `build.jobs` when above 0 - so a template with no
    /// placeholder is exported as written at every site; then the
    /// [`with_var`](Self::with_var) pairs. The site's cache is `cache` exactly when an exported
    /// variable uses `{build_cache}`.
    pub fn at(&self, cache: &str) -> SiteEnv {
        let mut site = SiteEnv {
            vars: Vec::new(),
            cache: String::new(),
        };
        for (name, pieces) in &self.declared {
            let Some(value) = self.resolved(pieces, cache) else {
                continue;
            };
            if pieces.contains(&Piece::BuildCache) {
                site.cache = cache.to_string();
            }
            site.vars.push((name.clone(), value));
        }
        site.vars.extend(self.with.iter().cloned());
        site
    }

    /// The value `pieces` resolve to at the site whose build cache is `cache`, or `None` when
    /// a placeholder resolves empty there.
    fn resolved(&self, pieces: &[Piece], cache: &str) -> Option<String> {
        let mut value = String::new();
        for piece in pieces {
            match piece {
                Piece::Text(text) => value.push_str(text),
                Piece::BuildCache if !cache.is_empty() => value.push_str(cache),
                Piece::Jobs if self.jobs > 0 => value.push_str(&self.jobs.to_string()),
                Piece::BuildCache | Piece::Jobs => return None,
            }
        }
        Some(value)
    }
}

/// Whether `name` is a variable name: a letter or underscore followed by letters, digits and
/// underscores, ASCII only.
fn is_variable_name(name: &str) -> bool {
    let mut chars = name.chars();
    chars
        .next()
        .is_some_and(|first| first.is_ascii_alphabetic() || first == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// The pieces of the template `value` declared for the variable `name`: `{{` and `}}` are
/// literal braces, `{build_cache}` and `{jobs}` placeholders, and anything else between braces
/// an unknown placeholder; a `{` that closes on no `}` before another `{` or the end, and a `}`
/// that closes nothing, is a stray brace. Either refusal names the variable and the value.
fn template_pieces(name: &str, value: &str) -> Result<Vec<Piece>, String> {
    let stray = || {
        format!(
            "build.env: {name}: a stray brace in {value:?}; write {{{{ or }}}} for a literal brace"
        )
    };
    let mut pieces = Vec::new();
    let mut text = String::new();
    let mut chars = value.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '{' | '}' if chars.peek() == Some(&c) => {
                chars.next();
                text.push(c);
            }
            '}' => return Err(stray()),
            '{' => {
                let mut inner = String::new();
                loop {
                    match chars.next() {
                        Some('}') => break,
                        Some('{') | None => return Err(stray()),
                        Some(ch) => inner.push(ch),
                    }
                }
                let placeholder = match inner.as_str() {
                    "build_cache" => Piece::BuildCache,
                    "jobs" => Piece::Jobs,
                    _ => {
                        return Err(format!(
                            "build.env: {name}: unknown placeholder {{{inner}}} in {value:?}; a \
                             value knows {{build_cache}} and {{jobs}}, and {{{{ and }}}} for a \
                             literal brace"
                        ))
                    }
                };
                if !text.is_empty() {
                    pieces.push(Piece::Text(std::mem::take(&mut text)));
                }
                pieces.push(placeholder);
            }
            c => text.push(c),
        }
    }
    if !text.is_empty() {
        pieces.push(Piece::Text(text));
    }
    Ok(pieces)
}

/// The build cache a gate's site resolves `{build_cache}` to: the unit's `target_dir` when the
/// gate runs for a unit, else the shared gate cache `build_cache_dir`, empty when neither is
/// set.
pub fn gate_site_cache(target_dir: &str, build_cache_dir: &str) -> String {
    if target_dir.is_empty() {
        build_cache_dir.to_string()
    } else {
        target_dir.to_string()
    }
}

/// One requirement a gate declares, resolved: the `requires` entry as written (`name`) and the
/// absolute path of the executable it resolved to (`at`) - spec 113, A GATE DECLARES WHAT IT
/// REQUIRES.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResolvedRequirement {
    pub name: String,
    pub at: std::path::PathBuf,
}

/// One declared gate's resolved requirements, in the order its `requires` lists them. A gate
/// requiring nothing holds an empty list; an unresolved entry is never held, because the
/// resolver answers it as [`RequirementUnavailable`] instead.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GateRequirements {
    pub gate: String,
    pub requires: Vec<ResolvedRequirement>,
}

/// A declared gate whose `requires` entry is not an executable on PATH: the operator wired the
/// gate, so its missing tool refuses the load rather than failing the gate mid-run.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error(
    "gate {gate:?} requires {requirement:?}, which is not an executable on PATH (config key: \
     gates.{gate}.requires)"
)]
pub struct RequirementUnavailable {
    pub gate: String,
    pub requirement: String,
}

/// The one requirement resolver (the search path as a value): every entry of every declared
/// gate, in gate-id order and then list order, as written - a name listed twice resolves twice -
/// each looked up over `search` through [`find_executable`]. The first entry that resolves to
/// nothing is the `Err`, so the success value holds resolved entries only.
pub fn resolve_requirements(
    gates: &BTreeMap<String, rigger_domain::config::Gate>,
    search: &SearchPath,
) -> Result<Vec<GateRequirements>, RequirementUnavailable> {
    gates
        .iter()
        .map(|(gate, def)| {
            let requires = def
                .requires
                .iter()
                .map(|name| {
                    find_executable(search, name)
                        .map(|at| ResolvedRequirement {
                            name: name.clone(),
                            at,
                        })
                        .ok_or_else(|| RequirementUnavailable {
                            gate: gate.clone(),
                            requirement: name.clone(),
                        })
                })
                .collect::<Result<_, _>>()?;
            Ok(GateRequirements {
                gate: gate.clone(),
                requires,
            })
        })
        .collect()
}

/// The ambient edge of [`resolve_requirements`]: reads the real search path once, here,
/// through [`SearchPath::ambient`]. `Config::validate` calls it last, once per validating load.
pub fn resolve_requirements_on_path(
    gates: &BTreeMap<String, rigger_domain::config::Gate>,
) -> Result<Vec<GateRequirements>, RequirementUnavailable> {
    resolve_requirements(gates, &SearchPath::ambient())
}

/// The env var [`ExecRunner::run`] pins to fence a gate's store resolution (spec 70
/// criterion 3) - a unit-worktree gate's `target_dir` signal, or a caller-injected
/// `store_fence` override for a worktree (a standalone review worktree) with no
/// `target_dir` to derive one from. `main.rs`'s store-resolution authority
/// (`require_store_dir`/`walk_stores_from`) honors this when set, PINNING resolution to
/// the named scratch directory instead of walking up - so a gate-spawned process (a test
/// invoking a store-opening courier) can never reach the repo's live store. Additive and
/// defaulted off: unset, resolution is byte-identical to before this existed. Set ONLY by
/// the gate runner around a gate's spawned process, never by a test itself - "the fence is
/// the gate runner's job, not each test's" (spec 70 Design).
///
/// SCOPE: this is a process-tree-wide override - every descendant of the gate's spawned
/// process inherits it, not just an incidental courier the gate's own test suite spawns as
/// a side effect. A periphery test whose entire purpose is driving the product binary
/// through its OWN store-resolution/precedence logic end to end (never the gate runner's
/// intended protection target) must therefore defensively clear this var before spawning
/// its own courier, or it would silently observe the fenced scratch location instead of
/// the fixture it built - `tests/common::rigger_courier` is the one shared authority every
/// such periphery suite spawns the product through, so it clears this ambient inheritance
/// once, for every current and future courier-spawning call site.
pub const STORE_FENCE_ENV: &str = "RIGGER_STORE_FENCE_DIR";

/// The env var naming a unit's gate scratch root (spec 113): set on a gate's spawned process
/// from a non-empty `gate_scratch` and removed from it on an empty one (see [`Runner`]).
pub const GATE_SCRATCH_ENV: &str = "RIGGER_GATE_SCRATCH";

/// ExecRunner runs a gate as a shell command, reducing output to compact evidence.
pub struct ExecRunner;

impl Runner for ExecRunner {
    fn run(
        &self,
        g: &Gate,
        dir: &str,
        target_dir: &str,
        gate_scratch: &str,
        build_cache_dir: &str,
        build_cache_guard: &str,
        store_fence: &str,
        build_env: &BuildEnv,
        budget: &BuildBudget,
    ) -> Result<GateResult, ProbeFault> {
        // The gate's command line runs whole in the host's shell (spec 125 criterion 10): `sh
        // -c` on Linux and macOS, `cmd` on Windows, in `dir` or this process's own directory.
        let mut cmd = shell_command(&g.run, session_dir(dir));
        if !target_dir.is_empty() {
            // The gate store fence (spec 70 criterion 3): a non-empty
            // target_dir means this gate runs inside a unit worktree, so a store-opening
            // courier the gate's spawned process runs must never walk up into the repo's
            // LIVE run stream. Pin a sibling scratch dir (never inside target_dir itself,
            // which cargo owns and may wipe) so main.rs's store-resolution authority
            // resolves there instead of walking up. This branch always wins over
            // `store_fence` below - a unit worktree derives its own fence from target_dir.
            cmd.env(STORE_FENCE_ENV, format!("{target_dir}{STORE_FENCE_SUFFIX}"));
        } else {
            // Widened (spec 70, u4 round 2 fix for
            // adv-u3c70-store-fence-half-wired-review-worktree-call-site-unfenced, then
            // u4 round 3 fix for arch-u4c70r2-fence-signal-not-injected-into-runner-review-case):
            // a standalone review stage's EXHAUSTIVE gate pass (`run_fan_out_stage` ->
            // `run_gates`) runs with `dir` set to the throwaway review worktree and an
            // EMPTY target_dir (a review worktree owns no per-unit build cache, so the
            // branch above never fires), yet it is a real, unconditional gate run inside a
            // scratch-root worktree whose spawned process must be fenced the same way.
            // `store_fence` is a plain, caller-injected value here rather than derived
            // by THIS module from `dir` - the caller (`conductor::run_gates`) computes it
            // via `worktree::review_fence_sibling(dir)`, so `gate.rs` never depends on
            // `worktree` and the one-directional dependency (`conductor` -> `{gate,
            // worktree}`) stays intact.
            if !store_fence.is_empty() {
                cmd.env(STORE_FENCE_ENV, store_fence);
            }
        }
        // The unit's gate scratch root (spec 113): its own check, independent of
        // `target_dir`'s branch above. Set from a non-empty `gate_scratch`; on an empty one
        // REMOVED, never left to inheritance - this repository's own `test` gate runs the
        // suite with the variable naming the outer unit's root, and an inner gate on a review
        // worktree or with no worktree must still run without it.
        if gate_scratch.is_empty() {
            cmd.env_remove(GATE_SCRATCH_ENV);
        } else {
            cmd.env(GATE_SCRATCH_ENV, gate_scratch);
        }
        // The ONE build-environment authority's gate site: the declared variables resolved
        // over this gate's build cache - the unit's `target_dir`, else the shared
        // `build_cache_dir` the guard protects (spec 77 criterion 5) - so a declaration
        // pointing a build at `{build_cache}` builds into the directory the guard names. With
        // nothing declared it sets nothing, and a variable rigger's environment holds reaches
        // the gate unchanged; a guarded gate holds the shared lock whatever it builds into,
        // since an inherited variable may point its build at the shared cache too.
        build_env
            .at(&gate_site_cache(target_dir, build_cache_dir))
            .apply(&mut cmd);
        // The machine-wide build budget's ONE gating point (spec 65): held for exactly
        // this command's duration, so a concurrent gate build elsewhere on the machine
        // waits for a free slot rather than stacking another compiler fleet into
        // memory. Never held across anything but this call - a hung command still
        // frees its slot the instant it is killed, and no other code path in the crate
        // acquires a slot, so non-build work is never gated. A slot directory where no slot
        // can ever be locked fails the gate as a command that could not start does.
        let _slot = match budget.acquire() {
            Ok(slot) => slot,
            Err(e) => {
                return Ok(GateResult {
                    pass: false,
                    evidence: format!("FAIL\ngate {}: {e}", g.id),
                })
            }
        };
        // The shared gate build cache's exclusion protocol, PRODUCER half (spec 77 criterion 5,
        // BOUNDED SHARED CACHE; spec 125 criterion 10): a non-empty `build_cache_guard` names the
        // lock file `rigger reset --build-cache` takes EXCLUSIVE before it reclaims the shared
        // cache, and a guarded gate holds it SHARED for as long as what it started may still
        // build there. `hold_shared_lock` takes the lock in this process, immediately before the
        // command spawns, and `inherit_on_exec` hands its open guard across the command's exec;
        // this process closes its own handle once the command returns, never unlocking. On unix
        // the lock belongs to the open guard every process the gate started shares while it
        // keeps the inherited descriptor, so it lives until the last of them exits: a build the
        // command leaves running in the background, or one this process leaves behind when it
        // dies mid-build, keeps the cache excluded. On Windows it lives as long as this process's
        // handle, the gate's run: a child the gate leaves running is outside the exclusion, and
        // no holder check stands in for it, since the reclaim renames the shared cache whole
        // once it takes the guard. A guard that cannot be locked returns
        // its [`ProbeFault`] before the command runs, releasing the slot (spec 121 criterion
        // 19), so a gate never builds unguarded into the directory the guard protects nor
        // anywhere else. An empty guard takes no lock: a unit's own build cache is never one
        // `rigger reset --build-cache` reclaims.
        let _lock = if build_cache_guard.is_empty() {
            None
        } else {
            let lock = hold_shared_lock(build_cache_guard)?;
            inherit_on_exec(&mut cmd, &lock);
            Some(lock)
        };
        Ok(match cmd.output() {
            Ok(out) => {
                let mut evidence = String::from_utf8_lossy(&out.stdout).into_owned();
                evidence.push_str(&String::from_utf8_lossy(&out.stderr));
                let pass = out.status.success();
                GateResult {
                    pass,
                    evidence: compact(pass, &evidence),
                }
            }
            Err(e) => GateResult {
                pass: false,
                evidence: format!("FAIL\ngate {}: {e}", g.id),
            },
        })
    }
}

/// Opens the shared-cache guard file at `path`, creating its parent directory and the file
/// when absent and never truncating it: the one open both a gate's shared lock
/// ([`hold_shared_lock`]) and `rigger reset --build-cache`'s reclaim take on the guard.
pub fn open_build_cache_guard(path: &std::path::Path) -> std::io::Result<std::fs::File> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(false)
        .open(path)
}

/// Takes the in-process shared lock on the shared-cache guard at `guard` (spec 77 criterion 5,
/// BOUNDED SHARED CACHE; spec 121 criterion 19; spec 125 criterion 10) and answers the open
/// guard holding it. The guard is opened through [`open_build_cache_guard`], so a missing
/// scratch root is created rather than faulted, and locked shared through std's
/// [`File::lock_shared`](std::fs::File::lock_shared) (`flock(2)` on unix, `LockFileEx` on
/// Windows), waiting while `rigger reset --build-cache` holds it exclusive. Nothing here unlocks
/// it: the lock lasts while the answered file stays open, and on unix while any process that
/// inherited its descriptor ([`inherit_on_exec`]) keeps it. A refused create, open or lock is
/// the [`ProbeFault`] naming `guard` and the OS error.
pub fn hold_shared_lock(guard: &str) -> Result<std::fs::File, ProbeFault> {
    lock_guard(guard, std::fs::File::lock_shared)
}

/// Opens the guard at `guard` through [`open_build_cache_guard`], takes `lock` on it and
/// answers the open guard; a refused create, open or lock is the [`ProbeFault`] naming `guard`
/// and the OS error. [`hold_shared_lock`] passes the shared lock; a test passes a lock that
/// refuses, the answer of a filesystem without advisory locks, which no test can mount.
fn lock_guard(
    guard: &str,
    lock: impl FnOnce(&std::fs::File) -> std::io::Result<()>,
) -> Result<std::fs::File, ProbeFault> {
    let fault = |e: std::io::Error| ProbeFault {
        guard: guard.to_string(),
        error: e.to_string(),
    };
    let file = open_build_cache_guard(std::path::Path::new(guard)).map_err(fault)?;
    lock(&file).map_err(fault)?;
    Ok(file)
}

/// Cap on the length of any single evidence line (§3.3).
const LINE_CAP: usize = 200;
/// Cap on the number of signal lines carried in the evidence (§3.3).
const MAX_LINES: usize = 5;

/// Reduce a gate's raw output to a compact summary (§3.3; ranking per spec
/// 70 criterion 2): the verdict (`PASS`/`FAIL`) followed by up to five lines
/// that signal failure. Genuine failure syntax (a `FAILED` test line, a
/// rustc `error[...]` code, a `panicked at` location, or the `failures:` /
/// `test result: FAILED` summary) fills the budget first; other lines merely
/// containing `error`, `fail`, `panic`, or `assert` (case-insensitive) fill
/// any remaining slots. A passing `test <name> ... ok` line never occupies a
/// slot, even when the test's own name contains one of those keywords. If
/// nothing signals, the last five non-empty lines are used instead. Each
/// line is length-capped; the raw log is never carried.
fn compact(pass: bool, s: &str) -> String {
    let verdict = if pass { "PASS" } else { "FAIL" };
    let lines: Vec<&str> = s.lines().map(str::trim).filter(|l| !l.is_empty()).collect();

    // Real failure markers rank above mere keyword hits, so a keyword hit
    // can only consume a slot once every marker has been carried.
    let markers = lines.iter().filter(|l| is_failure_marker(l)).copied();
    let keyword_hits = lines
        .iter()
        .filter(|l| is_signal(l) && !is_failure_marker(l))
        .copied();
    let signal: Vec<&str> = markers.chain(keyword_hits).take(MAX_LINES).collect();

    let chosen: Vec<&str> = if signal.is_empty() {
        // No failure-signalling line matched: fall back to the last few lines.
        let start = lines.len().saturating_sub(MAX_LINES);
        lines[start..].to_vec()
    } else {
        signal
    };

    let mut out = String::from(verdict);
    for line in chosen {
        out.push('\n');
        out.push_str(&cap_line(line));
    }
    out
}

/// Whether a line reports a passing test result (`test <name> ... ok`). Such
/// a line must never consume an evidence slot, even when the test's own name
/// contains a failure keyword like `assert` or `panic` (spec 70 criterion 2).
fn is_passing_test_line(line: &str) -> bool {
    line.starts_with("test ") && line.ends_with("... ok")
}

/// Whether a line is genuine cargo/rustc failure syntax rather than a mere
/// keyword hit: a `FAILED` test result, a compiler error code, a panic
/// location, or the `failures:` / `test result: FAILED` summary lines. These
/// rank above other keyword-matching lines (spec 70 criterion 2).
fn is_failure_marker(line: &str) -> bool {
    (line.starts_with("test ") && line.ends_with("... FAILED"))
        || line.starts_with("error[")
        || line.contains("panicked at")
        || line == "failures:"
        || line.starts_with("test result: FAILED")
}

/// Whether a line signals a failure (matched case-insensitively): `error`,
/// `fail`, `panic`, or `assert` anywhere in the line - except a passing
/// `... ok` test-result line, which never signals even when its test name
/// happens to contain one of those keywords.
fn is_signal(line: &str) -> bool {
    if is_passing_test_line(line) {
        return false;
    }
    let lower = line.to_lowercase();
    ["error", "fail", "panic", "assert"]
        .iter()
        .any(|kw| lower.contains(kw))
}

/// Truncate a line to [`LINE_CAP`] characters, on a char boundary.
fn cap_line(line: &str) -> String {
    if line.chars().count() <= LINE_CAP {
        return line.to_string();
    }
    let truncated: String = line.chars().take(LINE_CAP).collect();
    format!("{truncated}...")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::subprocess::is_executable_file;
    use crate::test_support::{
        build_declaring, contended, try_exclusive_lock, wait_until_for, DECLARED_BJL,
        REFUSED_DECLARATIONS,
    };

    fn gate(autonomy: Autonomy, passes: usize) -> Gate {
        gate_with_kind(Kind::Core, autonomy, passes)
    }

    fn gate_with_kind(kind: Kind, autonomy: Autonomy, passes: usize) -> Gate {
        Gate {
            id: "g".into(),
            run: String::new(),
            kind,
            autonomy,
            history: (0..passes).map(|_| HistoryEntry { pass: true }).collect(),
        }
    }

    #[test]
    fn promotes_after_clean_passes() {
        assert!(propose_promotion(&gate(Autonomy::Manual, 3)));
        assert!(!propose_promotion(&gate(Autonomy::Manual, 2)));
        assert!(!propose_promotion(&gate(Autonomy::Silent, 3)));
    }

    #[test]
    fn elevated_gate_can_never_become_silent() {
        // A Core gate at AutoNotify ratchets up to Silent.
        let core = gate_with_kind(Kind::Core, Autonomy::AutoNotify, 3);
        assert!(
            propose_promotion(&core),
            "a reliable Core gate at auto_notify is still proposable"
        );
        assert_eq!(next_autonomy(&core), Autonomy::Silent);

        // An Elevated gate at AutoNotify has reached its ceiling: it is NOT
        // proposed for promotion, and even if one were forced the next step caps
        // at AutoNotify, never Silent.
        let elevated_top = gate_with_kind(Kind::Elevated, Autonomy::AutoNotify, 3);
        assert!(
            !propose_promotion(&elevated_top),
            "an elevated gate at its auto_notify ceiling must not be proposed for silent"
        );
        assert_eq!(
            next_autonomy(&elevated_top),
            Autonomy::AutoNotify,
            "an elevated promotion can never step to silent"
        );

        // From Manual, a reliable Elevated gate still earns AutoNotify - the
        // ceiling stops it at notify, it does not freeze it at manual.
        let elevated_from_manual = gate_with_kind(Kind::Elevated, Autonomy::Manual, 3);
        assert!(propose_promotion(&elevated_from_manual));
        assert_eq!(next_autonomy(&elevated_from_manual), Autonomy::AutoNotify);
    }

    #[test]
    fn demotes_on_failure_only_when_graduated() {
        assert_eq!(
            auto_demote(&gate(Autonomy::Silent, 0), false),
            (Autonomy::Manual, true)
        );
        assert!(!auto_demote(&gate(Autonomy::Manual, 0), false).1);
        assert!(!auto_demote(&gate(Autonomy::Silent, 0), true).1);
    }

    #[test]
    fn exec_runner_reports_pass_fail() {
        assert!(
            ExecRunner
                .run(
                    &gate_cmd("true"),
                    "",
                    "",
                    "",
                    "",
                    "",
                    "",
                    &BuildEnv::default(),
                    &BuildBudget::default()
                )
                .unwrap()
                .pass
        );
        assert!(
            !ExecRunner
                .run(
                    &gate_cmd("false"),
                    "",
                    "",
                    "",
                    "",
                    "",
                    "",
                    &BuildEnv::default(),
                    &BuildBudget::default()
                )
                .unwrap()
                .pass
        );
    }

    #[test]
    fn evidence_names_failure_not_crowded_out_by_passing_keyword_named_tests() {
        // spec 70 criterion 2: passing tests whose own NAMES contain a
        // failure keyword (assert/panic/error/fail) must never crowd the
        // real FAILED line and failures summary out of the five-line budget.
        let output = "\
running 7 tests
test test_assert_boundary ... ok
test test_panic_recovery ... ok
test test_error_handling_ok ... ok
test test_failover_logic ... ok
test test_assertion_helper ... ok
test really_failing_test ... FAILED

failures:

---- really_failing_test stdout ----
thread 'really_failing_test' panicked at src/lib.rs:10:5:
assertion failed: false

failures:
    really_failing_test

test result: FAILED. 6 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
";
        let evidence = compact(false, output);
        assert!(
            evidence.contains("test really_failing_test ... FAILED"),
            "evidence must name the actual failing test: {evidence:?}"
        );
        assert!(
            evidence.contains("test result: FAILED"),
            "evidence must carry the failures summary: {evidence:?}"
        );
        assert!(
            !evidence.lines().any(|l| l.ends_with("... ok")),
            "no passing '... ok' line may occupy an evidence slot: {evidence:?}"
        );
    }

    #[test]
    fn compact_summary_is_verdict_plus_failing_lines() {
        // A gate that prints 20 lines including a failure signal, then fails.
        let mut cmd = String::new();
        for i in 1..=20 {
            if i == 7 {
                cmd.push_str("echo 'error: boom'; ");
            } else {
                cmd.push_str(&format!("echo 'line {i}'; "));
            }
        }
        cmd.push_str("false");
        let res = ExecRunner
            .run(
                &gate_cmd(&cmd),
                "",
                "",
                "",
                "",
                "",
                "",
                &BuildEnv::default(),
                &BuildBudget::default(),
            )
            .unwrap();
        assert!(!res.pass);

        let lines: Vec<&str> = res.evidence.lines().collect();
        // Verdict line plus at most MAX_LINES signal lines.
        assert!(lines.len() <= MAX_LINES + 1, "evidence: {:?}", res.evidence);
        assert_eq!(lines[0], "FAIL", "verdict names the failure");
        assert!(
            res.evidence.contains("error: boom"),
            "evidence keeps the failure line, not just the trailing bytes: {:?}",
            res.evidence
        );
    }

    /// A shell `probe` run as a gate through [`ExecRunner`] with the given `target_dir`,
    /// `build_cache_dir`, `guard` and `env` (every other input empty/default) passes - the
    /// probe asserts the environment the gate command actually sees.
    fn assert_probe_passes(
        probe: &str,
        (target_dir, build_cache_dir, guard): (&str, &str, &str),
        env: &BuildEnv,
        why: &str,
    ) {
        let res = ExecRunner
            .run(
                &gate_cmd(probe),
                "",
                target_dir,
                "",
                build_cache_dir,
                guard,
                "",
                env,
                &BuildBudget::default(),
            )
            .unwrap();
        assert!(res.pass, "{why}: {res:?}");
    }

    /// A shell probe that passes exactly when each `(name, value)` pair holds in the environment
    /// it runs in: `Some(value)` the variable set to `value`, `None` the variable unset.
    fn probe_sees(pairs: &[(&str, Option<&str>)]) -> String {
        pairs
            .iter()
            .map(|(name, value)| {
                format!(
                    "test \"${{{name}-<unset>}}\" = '{}'",
                    value.unwrap_or("<unset>")
                )
            })
            .collect::<Vec<_>>()
            .join(" && ")
    }

    /// The value `name` holds in this test process's own environment, which a gate run
    /// inherits for every variable rigger neither sets nor clears.
    fn ambient(name: &str) -> Option<String> {
        std::env::var(name).ok()
    }

    /// The gate site (spec 125 criterion 4): a gate run exports the declared environment over
    /// its site's build cache - the unit's `target_dir` when it has one, else the shared
    /// `build_cache_dir` - under the guard's shared lock too, and with nothing declared sets no
    /// build-cache variable, so `CARGO_TARGET_DIR` reaches the gate exactly as this process
    /// holds it. A declared variable the site cannot resolve is neither set nor cleared.
    #[test]
    fn exec_runner_exports_the_declared_environment_over_its_site_cache() {
        struct Case {
            why: &'static str,
            target_dir: &'static str,
            build_cache_dir: &'static str,
            guarded: bool,
            declared: &'static [(&'static str, &'static str)],
            sees: Vec<(&'static str, Option<String>)>,
        }
        let bjl = |b: Option<&str>| {
            vec![
                ("B", b.map(str::to_string)),
                ("J", Some("3".to_string())),
                ("L", Some("lit".to_string())),
            ]
        };
        let cases = [
            Case {
                why: "a unit's target_dir is the site cache",
                target_dir: "/tmp/rigger-site-t",
                build_cache_dir: "",
                guarded: false,
                declared: &DECLARED_BJL,
                sees: bjl(Some("/tmp/rigger-site-t/x")),
            },
            Case {
                why: "with no target_dir the shared build_cache_dir is the site cache",
                target_dir: "",
                build_cache_dir: "/tmp/rigger-site-s",
                guarded: false,
                declared: &DECLARED_BJL,
                sees: bjl(Some("/tmp/rigger-site-s/x")),
            },
            Case {
                why: "a target_dir wins over a build_cache_dir",
                target_dir: "/tmp/rigger-site-t",
                build_cache_dir: "/tmp/rigger-site-s",
                guarded: false,
                declared: &DECLARED_BJL,
                sees: bjl(Some("/tmp/rigger-site-t/x")),
            },
            Case {
                why: "the declared variables reach the gate under the guard's shared lock",
                target_dir: "",
                build_cache_dir: "/tmp/rigger-site-s",
                guarded: true,
                declared: &DECLARED_BJL,
                sees: bjl(Some("/tmp/rigger-site-s/x")),
            },
            Case {
                why: "a site with no cache exports J and L and leaves B as found",
                target_dir: "",
                build_cache_dir: "",
                guarded: false,
                declared: &DECLARED_BJL,
                sees: bjl(ambient("B").as_deref()),
            },
            Case {
                why: "with nothing declared a unit gate's CARGO_TARGET_DIR is as found",
                target_dir: "/tmp/rigger-site-t",
                build_cache_dir: "",
                guarded: false,
                declared: &[],
                sees: vec![("CARGO_TARGET_DIR", ambient("CARGO_TARGET_DIR"))],
            },
            Case {
                why: "with nothing declared a shared-cache gate's CARGO_TARGET_DIR is as found",
                target_dir: "",
                build_cache_dir: "/tmp/rigger-site-s",
                guarded: false,
                declared: &[],
                sees: vec![("CARGO_TARGET_DIR", ambient("CARGO_TARGET_DIR"))],
            },
        ];
        for case in cases {
            let dir = tempfile::tempdir().expect("tempdir");
            let guard_path = dir.path().join("cargo-target.lock");
            let guard = if case.guarded {
                guard_path.to_str().unwrap()
            } else {
                ""
            };
            let sees: Vec<(&str, Option<&str>)> = case
                .sees
                .iter()
                .map(|(name, value)| (*name, value.as_deref()))
                .collect();
            assert_probe_passes(
                &probe_sees(&sees),
                (case.target_dir, case.build_cache_dir, guard),
                &BuildEnv::declared(&build_declaring(case.declared, 3)).unwrap(),
                case.why,
            );
        }
    }

    /// The build environment the `build` block declaring `pairs` with `build.jobs` = `jobs`
    /// resolves to, at the build cache `cache`.
    fn site_of(pairs: &[(&str, &str)], jobs: u32, cache: &str) -> SiteEnv {
        BuildEnv::declared(&build_declaring(pairs, jobs))
            .expect("an accepted declaration")
            .at(cache)
    }

    fn owned(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
        pairs
            .iter()
            .map(|(name, value)| (name.to_string(), value.to_string()))
            .collect()
    }

    /// Every variable name of a letter or underscore followed by letters, digits and
    /// underscores is accepted, `CLAUDE_CODE_PRINT_BG_WAIT_CEILING_MS` included: rigger sets it
    /// only as a default a declaration overrides.
    #[test]
    fn build_env_declared_accepts_every_variable_name() {
        for name in ["_", "a", "Z9_", "CLAUDE_CODE_PRINT_BG_WAIT_CEILING_MS"] {
            assert_eq!(
                site_of(&[(name, "lit")], 0, "").vars,
                owned(&[(name, "lit")]),
                "{name}"
            );
        }
    }

    /// A site exports a declared variable exactly when every placeholder of its template
    /// resolves non-empty (`{build_cache}` to the site's cache, `{jobs}` to a positive jobs
    /// cap), and a placeholder-free template, an empty one included, everywhere, in name order.
    /// The site's cache is the given one exactly when an exported variable uses `{build_cache}`.
    #[test]
    fn build_env_at_exports_each_variable_whose_placeholders_resolve() {
        struct Case {
            why: &'static str,
            declared: &'static [(&'static str, &'static str)],
            jobs: u32,
            cache: &'static str,
            vars: &'static [(&'static str, &'static str)],
            site_cache: &'static str,
        }
        let cases = [
            Case {
                why: "the criterion's declaration at a cache with jobs 3",
                declared: &DECLARED_BJL,
                jobs: 3,
                cache: "/c",
                vars: &[("B", "/c/x"), ("J", "3"), ("L", "lit")],
                site_cache: "/c",
            },
            Case {
                why: "no cache exports no B",
                declared: &DECLARED_BJL,
                jobs: 3,
                cache: "",
                vars: &[("J", "3"), ("L", "lit")],
                site_cache: "",
            },
            Case {
                why: "jobs 0 exports no J",
                declared: &DECLARED_BJL,
                jobs: 0,
                cache: "/c",
                vars: &[("B", "/c/x"), ("L", "lit")],
                site_cache: "/c",
            },
            Case {
                why: "jobs 1 exports J",
                declared: &[("J", "{jobs}")],
                jobs: 1,
                cache: "",
                vars: &[("J", "1")],
                site_cache: "",
            },
            Case {
                why: "neither a cache nor jobs exports only the literal",
                declared: &DECLARED_BJL,
                jobs: 0,
                cache: "",
                vars: &[("L", "lit")],
                site_cache: "",
            },
            Case {
                why: "an empty template is exported",
                declared: &[("E", "")],
                jobs: 0,
                cache: "/c",
                vars: &[("E", "")],
                site_cache: "",
            },
            Case {
                why: "one unresolved placeholder withholds the whole variable",
                declared: &[("M", "{build_cache}/{jobs}")],
                jobs: 0,
                cache: "/c",
                vars: &[],
                site_cache: "",
            },
            Case {
                why: "both placeholders resolved",
                declared: &[("M", "{build_cache}/{jobs}")],
                jobs: 2,
                cache: "/c",
                vars: &[("M", "/c/2")],
                site_cache: "/c",
            },
            Case {
                why: "the cache unresolved",
                declared: &[("M", "{build_cache}/{jobs}")],
                jobs: 2,
                cache: "",
                vars: &[],
                site_cache: "",
            },
            Case {
                why: "a placeholder twice",
                declared: &[("R", "{build_cache}:{build_cache}")],
                jobs: 0,
                cache: "/c",
                vars: &[("R", "/c:/c")],
                site_cache: "/c",
            },
            Case {
                why: "doubled braces are literal braces, never a placeholder",
                declared: &[("T", "{{build_cache}}")],
                jobs: 0,
                cache: "/c",
                vars: &[("T", "{build_cache}")],
                site_cache: "",
            },
            Case {
                why: "doubled braces anywhere",
                declared: &[("T", "a}}b{{c")],
                jobs: 0,
                cache: "",
                vars: &[("T", "a}b{c")],
                site_cache: "",
            },
            Case {
                why: "variables in name order",
                declared: &[("Z", "1"), ("A", "{build_cache}")],
                jobs: 0,
                cache: "/c",
                vars: &[("A", "/c"), ("Z", "1")],
                site_cache: "/c",
            },
            Case {
                why: "nothing declared",
                declared: &[],
                jobs: 3,
                cache: "/c",
                vars: &[],
                site_cache: "",
            },
        ];
        for case in cases {
            assert_eq!(
                site_of(case.declared, case.jobs, case.cache),
                SiteEnv {
                    vars: owned(case.vars),
                    cache: case.site_cache.to_string(),
                },
                "{}",
                case.why
            );
        }
    }

    /// A `with_var` pair follows the declared variables at every site, an empty value adds
    /// nothing, and neither makes the site's cache.
    #[test]
    fn build_env_at_appends_the_with_var_pairs_after_the_declared_ones() {
        let env = BuildEnv::declared(&build_declaring(&DECLARED_BJL, 3))
            .unwrap()
            .with_var("RIGGER_RUN_BASE", "abc")
            .with_var("X", "");
        assert_eq!(
            env.at("/c"),
            SiteEnv {
                vars: owned(&[
                    ("B", "/c/x"),
                    ("J", "3"),
                    ("L", "lit"),
                    ("RIGGER_RUN_BASE", "abc"),
                ]),
                cache: "/c".to_string(),
            }
        );
        assert_eq!(
            BuildEnv::default().with_var("R", "v").at("/c"),
            SiteEnv {
                vars: owned(&[("R", "v")]),
                cache: String::new(),
            }
        );
    }

    /// Every refused `build.env` entry is refused naming the variable: a bad name, each name
    /// rigger sets itself, an unknown placeholder and a stray brace.
    #[test]
    fn build_env_declared_refuses_each_refused_declaration_naming_the_variable() {
        for row in REFUSED_DECLARATIONS {
            assert_eq!(
                BuildEnv::declared(&build_declaring(&[(row.name, row.value)], 3)),
                Err(row.message.to_string()),
                "{:?} = {:?}",
                row.name,
                row.value
            );
        }
    }

    /// A SiteEnv applied to a command sets each of its pairs on it.
    #[test]
    fn site_env_apply_sets_every_pair_on_a_command() {
        let site = site_of(&DECLARED_BJL, 3, "/c");
        let mut cmd = Command::new("sh");
        site.apply(&mut cmd);
        cmd.arg("-c").arg(probe_sees(&[
            ("B", Some("/c/x")),
            ("J", Some("3")),
            ("L", Some("lit")),
        ]));
        assert!(cmd.status().unwrap().success(), "apply sets every pair");
    }

    #[test]
    fn exec_runner_holds_the_shared_build_cache_guard_lock_for_the_whole_cargo_invocation() {
        // spec 77 criterion 5 (BOUNDED SHARED CACHE): a non-empty `build_cache_guard`
        // names the sibling lock file `rigger reset --build-cache` takes EXCLUSIVE and
        // non-blocking; a gate build must hold it SHARED for its whole invocation, so a
        // reset attempted while this gate's command is still running is refused rather
        // than corrupting the cache out from under it. Proven from the OTHER side: while
        // the gate command is still running, an independent probe's exclusive try-lock on
        // the SAME guard path must fail (contended); once the gate command exits, the
        // same probe must succeed (the shared hold was released).
        let dir = tempfile::tempdir().expect("tempdir");
        let guard_path = dir.path().join("cargo-target.lock");
        let started = dir.path().join("started");
        let guard_str = guard_path.to_str().unwrap().to_string();
        let started_str = started.to_str().unwrap().to_string();
        let handle = std::thread::spawn(move || {
            ExecRunner.run(
                &gate_cmd(&format!("touch {started_str}; sleep 1")),
                "",
                "",
                "",
                "",
                &guard_str,
                "",
                &BuildEnv::default(),
                &BuildBudget::default(),
            )
        });
        assert!(
            wait_until_for(400, || started.exists()),
            "the gate command must start and touch its marker"
        );
        assert_eq!(
            try_exclusive_lock(&guard_path),
            contended(),
            "a running shared-cache gate build must hold the guard SHARED, blocking an \
             exclusive probe"
        );
        let res = handle.join().expect("gate thread must not panic").unwrap();
        assert!(res.pass, "the gate command must pass: {res:?}");
        // The gate releases its shared hold by closing the guard, by design, since its command
        // inherits it; a process another test thread forked during the run holds a copy of the
        // guard's descriptor until that process execs, so the release is waited for, bounded.
        assert!(
            wait_until_for(400, || try_exclusive_lock(&guard_path) == Ok(())),
            "once the gate build exits its shared hold must be released"
        );
    }

    /// Spec 125 criterion 10 (THE SHELL AND THE LOCK ARE PORTABLE): the shared lock a guarded
    /// gate takes belongs to every process its command starts, not to the gate's run alone. A
    /// command that backgrounds a child with its stdout and stderr sent away from the gate's
    /// pipes returns at once, and the child still holds the guard: an exclusive try-lock is
    /// refused after the run returns while the child lives, and taken once the child exits.
    /// The child waits on a marker the test removes, bounded so it never outlives the test.
    #[cfg(unix)]
    #[test]
    fn a_guarded_gates_background_child_holds_the_shared_lock_until_it_exits() {
        let dir = tempfile::tempdir().expect("tempdir");
        let guard = dir.path().join("cargo-target.lock");
        let hold = dir.path().join("hold");
        std::fs::write(&hold, b"").expect("write the hold marker");
        let res = ExecRunner
            .run(
                &gate_cmd(&crate::test_support::background_hold_command(&hold)),
                "",
                "",
                "",
                "",
                guard.to_str().unwrap(),
                "",
                &BuildEnv::default(),
                &BuildBudget::default(),
            )
            .expect("a usable guard runs the gate");
        assert_eq!(
            (res.pass, try_exclusive_lock(&guard)),
            (true, contended()),
            "the gate's run has returned and its background child still holds the guard"
        );
        std::fs::remove_file(&hold).expect("release the background child");
        assert!(
            wait_until_for(400, || try_exclusive_lock(&guard) == Ok(())),
            "once the background child exits the guard is free"
        );
    }

    /// A build budget whose every slot fails to open, so no holder will ever free one, fails the
    /// gate with the slot's error and never runs its command.
    #[test]
    fn a_budget_no_slot_can_satisfy_fails_the_gate_without_running_it() {
        let dir = tempfile::tempdir().expect("tempdir");
        let slot = dir.path().join("slots/slot-0.lock");
        std::fs::create_dir_all(&slot).expect("a directory where the slot file would be");
        let ran = dir.path().join("ran");
        let res = ExecRunner
            .run(
                &gate_cmd(&format!("touch {}", ran.display())),
                "",
                "",
                "",
                "",
                "",
                "",
                &BuildEnv::default(),
                &BuildBudget::new(1, dir.path().join("slots")),
            )
            .expect("no guard is requested, so no probe faults");
        assert_eq!(
            (res.pass, res.evidence.contains("slot-0.lock"), ran.exists()),
            (false, true, false),
            "the gate fails naming the slot and its command never runs: {}",
            res.evidence
        );
    }

    /// [`hold_shared_lock`] takes the guard SHARED: while it holds the guard, a second open of
    /// it takes a shared try-lock and is refused an exclusive one, so two guarded gates on one
    /// cache run side by side and only `rigger reset --build-cache` waits for them.
    #[test]
    fn the_held_guard_admits_a_second_shared_lock_and_refuses_an_exclusive_one() {
        let dir = tempfile::tempdir().expect("tempdir");
        let guard = dir.path().join("cargo-target.lock");
        let _held = hold_shared_lock(guard.to_str().unwrap()).expect("a usable guard locks");
        let second = open_build_cache_guard(&guard).expect("a second open of the guard");
        assert_eq!(
            (
                second
                    .try_lock_shared()
                    .map_err(|e| std::io::Error::from(e).kind()),
                try_exclusive_lock(&guard),
            ),
            (Ok(()), contended()),
            "a shared hold shares the guard and refuses an exclusive lock"
        );
    }

    #[test]
    fn exec_runner_never_touches_the_guard_when_build_cache_guard_is_empty() {
        // Empty means off, mirroring `target_dir`'s own convention: a per-unit build's
        // own `cargo-target-<slug>` cache is never at risk from `reset --build-cache`, so
        // it takes no lock at all and never even creates a guard file.
        let dir = tempfile::tempdir().expect("tempdir");
        let guard_path = dir.path().join("cargo-target.lock");
        assert_probe_passes("true", ("", "", ""), &BuildEnv::default(), "true must pass");
        assert!(
            !guard_path.exists(),
            "an empty build_cache_guard must never create a guard file: {guard_path:?}"
        );
    }

    /// One shared-cache gate run through [`ExecRunner`] under a tempdir `root`: the guard is
    /// the path `lay_out` prepares under `root`, the shared cache is `root/cache`, the
    /// workflow declares `CARGO_TARGET_DIR` over the build cache, and the command marks
    /// `root/ran` and builds into `$CARGO_TARGET_DIR`. `os_error` reads, before
    /// the run, the OS error the guard's probe must meet (`None` for a usable guard). A usable
    /// guard runs the command guarded, creating the guard file and building into the shared
    /// cache; an unusable one returns [`ProbeFault`] naming the guard and that error, and runs
    /// and builds nothing, and [`hold_shared_lock`] on it fails with that fault's message.
    fn assert_guard_probe(
        lay_out: fn(&std::path::Path) -> std::path::PathBuf,
        os_error: Option<fn(&std::path::Path) -> std::io::Error>,
    ) {
        let root = tempfile::tempdir().expect("tempdir");
        let guard = lay_out(root.path());
        let guard_str = guard.to_str().unwrap();
        let ran = root.path().join("ran");
        let cache = root.path().join("cache");
        let expected_fault = os_error.map(|error| ProbeFault {
            guard: guard_str.to_string(),
            error: error(&guard).to_string(),
        });
        let got = ExecRunner.run(
            &gate_cmd(&format!(
                "touch {} && mkdir -p \"$CARGO_TARGET_DIR\"",
                ran.display()
            )),
            "",
            "",
            "",
            cache.to_str().unwrap(),
            guard_str,
            "",
            &BuildEnv::declared(&build_declaring(
                &[("CARGO_TARGET_DIR", "{build_cache}")],
                0,
            ))
            .unwrap(),
            &BuildBudget::default(),
        );
        let ran_and_built = (ran.exists(), cache.is_dir());
        match expected_fault {
            Some(fault) => {
                assert_eq!(
                    got.map(|res| res.pass),
                    Err(fault),
                    "an unusable guard faults"
                );
                assert_eq!(
                    ran_and_built,
                    (false, false),
                    "a faulted probe runs no command and leaves no build directory"
                );
                assert_eq!(
                    hold_shared_lock(guard_str)
                        .map(drop)
                        .map_err(|fault| fault.to_string()),
                    Err(format!(
                        "build cache guard {guard_str:?} failed its probe: {}; fix the path or \
                         set the project's scratch root to a filesystem with advisory locks, \
                         then relaunch",
                        os_error.expect("an unusable guard's OS error")(&guard)
                    )),
                    "a guard that cannot be locked fails with the probe fault's message"
                );
            }
            None => {
                assert_eq!(
                    got.map(|res| res.pass),
                    Ok(true),
                    "a usable guard runs the gate"
                );
                assert_eq!(
                    (ran_and_built, guard.is_file()),
                    ((true, true), true),
                    "a usable guard is created and the command builds into the shared cache"
                );
                assert_eq!(
                    hold_shared_lock(guard_str)
                        .map(drop)
                        .map_err(|fault| fault.to_string()),
                    Ok(()),
                    "a usable guard is locked"
                );
            }
        }
    }

    crate::test_cases! {
        /// Spec 121 criterion 19: the probe creates the guard's missing parent first, so a
        /// scratch root that does not exist yet is never a fault.
        a_guard_over_a_missing_parent_is_created_and_the_gate_runs_guarded:
            assert_guard_probe(|root| root.join("missing-parent").join("cargo-target.lock"), None);
        /// Spec 121 criterion 19: a guard whose parent cannot be created (it lies under a
        /// regular file) is a probe fault naming the create's OS error.
        a_guard_whose_parent_cannot_be_created_is_a_probe_fault_and_runs_nothing:
            assert_guard_probe(
                |root| {
                    std::fs::write(root.join("file"), b"").expect("write the regular file");
                    root.join("file").join("sub").join("cargo-target.lock")
                },
                Some(|guard| std::fs::create_dir_all(guard.parent().unwrap()).unwrap_err()),
            );
        /// Spec 121 criterion 19: a guard the OS refuses to open (it names a directory) is a
        /// probe fault naming the open's OS error.
        a_guard_that_cannot_be_opened_is_a_probe_fault_and_runs_nothing:
            assert_guard_probe(
                |root| {
                    let guard = root.join("cargo-target.lock");
                    std::fs::create_dir(&guard).expect("create the directory guard");
                    guard
                },
                Some(|guard| {
                    std::fs::OpenOptions::new()
                        .create(true)
                        .write(true)
                        .truncate(false)
                        .open(guard)
                        .unwrap_err()
                }),
            );
    }

    /// Spec 125 criterion 10: a guard that opens but whose lock is refused, as a filesystem
    /// without advisory locks refuses it, is the probe fault naming the guard and the lock's
    /// error, taken after the guard is opened, so a gate never builds unguarded.
    #[test]
    fn a_guard_whose_lock_is_refused_is_a_probe_fault() {
        let root = tempfile::tempdir().expect("tempdir");
        let guard = root.path().join("cargo-target.lock");
        let guard_str = guard.to_str().unwrap();
        let got = lock_guard(guard_str, |_| {
            Err(std::io::Error::other("no locks available"))
        });
        assert_eq!(
            (got.map(drop), guard.is_file()),
            (
                Err(ProbeFault {
                    guard: guard_str.to_string(),
                    error: "no locks available".to_string(),
                }),
                true
            ),
            "a refused lock faults, naming the guard it opened and the lock's error"
        );
    }

    #[test]
    fn exec_runner_fences_gate_store_resolution_when_target_dir_is_given() {
        // Spec 70 criterion 3 (the gate store fence): a gate that runs INSIDE a unit
        // worktree is signaled by the SAME non-empty target_dir Gap 19 already carries
        // (a gate on the single integrated tree gets "" and is never fenced). ExecRunner
        // pins STORE_FENCE_ENV to an isolated scratch location derived from target_dir, so
        // any store-opening courier the gate's spawned process runs (a test invoking
        // `rigger emit`/`result`/`peers`/`reported`, say) resolves to that fenced, empty
        // location instead of walking up into the repo's LIVE run stream - main.rs's
        // require_store_dir honors this env var (see the main.rs-side fence test). The
        // fence is the gate runner's job here, not each test's.
        //
        // Defensively clear any AMBIENT STORE_FENCE_ENV this test binary's own process may
        // have inherited before asserting the unfenced branch below: this exact suite is
        // itself liable to run AS a `cargo test` gate inside a unit worktree (a real,
        // non-empty target_dir), which per the branch just above always pins
        // STORE_FENCE_ENV onto the cargo test process itself - and that would leak onto
        // every subprocess THIS test spawns, silently fencing the one assertion whose whole
        // point is proving the no-override path. Mirrors the identical precedent in
        // tests/gate_store_fence_periphery.rs::an_unfenced_integrated_tree_gate_still_walks_up_to_the_live_store
        // and src/main.rs's own fence tests. Never a race with those: ExecRunner scopes
        // every fence injection to the spawned Command only (never std::env::set_var), so
        // clearing here cannot clobber a concurrent test's in-flight state.
        std::env::remove_var(STORE_FENCE_ENV);
        let with = ExecRunner
            .run(
                &gate_cmd(&format!(
                    "test \"${STORE_FENCE_ENV}\" = /tmp/rigger-gap19-probe-store-fence"
                )),
                "",
                "/tmp/rigger-gap19-probe",
                "",
                "",
                "",
                "",
                &BuildEnv::default(),
                &BuildBudget::default(),
            )
            .unwrap();
        assert!(
            with.pass,
            "a non-empty target_dir must fence the gate's store resolution via {STORE_FENCE_ENV}: {with:?}"
        );

        let without = ExecRunner
            .run(
                &gate_cmd(&format!("test -z \"${STORE_FENCE_ENV}\"")),
                "",
                "",
                "",
                "",
                "",
                "",
                &BuildEnv::default(),
                &BuildBudget::default(),
            )
            .unwrap();
        assert!(
            without.pass,
            "an empty target_dir (the integrated-tree/deferred gate) must not force a store fence: {without:?}"
        );
    }

    #[test]
    fn exec_runner_removes_an_inherited_gate_scratch_root_when_handed_none() {
        // Spec 113, THE GATE SCRATCH ROOT IS HANDED GENERICALLY: the gate scratch root follows
        // the `gate_scratch` the caller hands, never the environment of the process that runs
        // the gate. This repository's own `test` gate runs this very suite with
        // `RIGGER_GATE_SCRATCH` set to the outer unit's root, so the variable is set in THIS
        // process first: handed an empty `gate_scratch`, the gate must find it unset
        // (`${RIGGER_GATE_SCRATCH+set}` expands empty), and handed a path, exactly that path.
        // The prior value is restored afterwards so the rest of the suite sees its own.
        struct Restore(Option<std::ffi::OsString>);
        impl Drop for Restore {
            fn drop(&mut self) {
                match self.0.take() {
                    Some(v) => std::env::set_var(GATE_SCRATCH_ENV, v),
                    None => std::env::remove_var(GATE_SCRATCH_ENV),
                }
            }
        }
        let _restore = Restore(std::env::var_os(GATE_SCRATCH_ENV));
        std::env::set_var(GATE_SCRATCH_ENV, "/inherited/rigger-gate-outer");
        let run = |gate_scratch: &str, check: &str| {
            ExecRunner
                .run(
                    &gate_cmd(check),
                    "",
                    "",
                    gate_scratch,
                    "",
                    "",
                    "",
                    &BuildEnv::default(),
                    &BuildBudget::default(),
                )
                .unwrap()
        };

        let none = run("", &format!("test -z \"${{{GATE_SCRATCH_ENV}+set}}\""));
        assert!(
            none.pass,
            "an empty gate_scratch must remove an inherited {GATE_SCRATCH_ENV}: {none:?}"
        );

        let handed = "/scratch/rigger-gate-unit-7";
        let set = run(handed, &format!("test \"${GATE_SCRATCH_ENV}\" = {handed}"));
        assert!(
            set.pass,
            "a non-empty gate_scratch must reach the gate as {GATE_SCRATCH_ENV}, over the \
             inherited value: {set:?}"
        );
    }

    /// A gate never sees the agents' login token, even when this process holds it: the gate's
    /// command is project code. The gate echoes the variable and fails, so its evidence carries
    /// the line; the assertions never print the evidence, which holds the value when inherited.
    #[test]
    fn exec_runner_never_hands_a_gate_the_agents_login_token() {
        const TOKEN: &str = rigger_domain::spawn::RIGGER_AGENT_TOKEN_VAR;
        struct Restore(Option<std::ffi::OsString>);
        impl Drop for Restore {
            fn drop(&mut self) {
                match self.0.take() {
                    Some(v) => std::env::set_var(TOKEN, v),
                    None => std::env::remove_var(TOKEN),
                }
            }
        }
        let _restore = Restore(std::env::var_os(TOKEN));
        let dummy = "dummy-token-for-test";
        std::env::set_var(TOKEN, dummy);
        let result = ExecRunner
            .run(
                &gate_cmd(&format!("echo \"error: token=[${{{TOKEN}}}]\"; false")),
                "",
                "",
                "",
                "",
                "",
                "",
                &BuildEnv::default(),
                &BuildBudget::default(),
            )
            .unwrap();
        assert!(!result.pass, "the gate exits 1");
        assert!(
            result.evidence.contains("token=[]"),
            "the gate's echo of an unset token reaches the evidence"
        );
        assert!(
            !result.evidence.contains(dummy),
            "a gate must not be handed {TOKEN}"
        );
    }

    #[test]
    fn exec_runner_honors_an_injected_store_fence_override_when_target_dir_is_empty() {
        // Spec 70 criterion 3, u4 round 3 fix for
        // arch-u4c70r2-fence-signal-not-injected-into-runner-review-case /
        // arch-u4c70r2-sharpens-round1-no-cycle-invariant-now-broken: a standalone review
        // stage's EXHAUSTIVE gate pass (conductor.rs's `run_fan_out_stage` -> `run_gates`)
        // runs gates with `dir` set to the throwaway `rigger-review-<stage>-<attempt>`
        // worktree and an EMPTY target_dir (a review worktree owns no per-unit build cache),
        // so the target_dir-driven branch above never fires for this call site even though
        // it is a real, unconditional gate run inside a scratch-root worktree. Before this
        // fix ExecRunner derived that fence ITSELF by calling `worktree::review_fence_sibling
        // (dir)` - a dependency from `gate.rs` back into `worktree.rs` that broke the
        // one-directional dependency shape round 1 certified. Now the CALLER
        // (`conductor::run_gates`) derives it and injects it as a plain `store_fence`
        // parameter; this test proves ExecRunner just forwards whatever it is handed,
        // verbatim, when target_dir is empty - never deriving it, never forcing a
        // CARGO_TARGET_DIR override alongside it (a review worktree has no build cache to
        // key one off). `dir` must actually exist on disk (unlike the target_dir-only tests
        // above) - ExecRunner chdirs the spawned `sh` into it - so this test roots a real
        // dir under a tempdir rather than a literal `/tmp/...` path.
        // Same defensive clear as the sibling test above, for the same reason: this test's
        // final assertion (an empty store_fence must not force a fence) spawns a child with
        // NO override at all, so it would otherwise silently inherit an ambient
        // STORE_FENCE_ENV this test binary's own process carries whenever this exact suite
        // itself runs as a fenced `cargo test` gate.
        std::env::remove_var(STORE_FENCE_ENV);
        let parent = tempfile::tempdir().expect("tempdir");
        let review_dir = parent.path().join("rigger-review-fanout-probe-0");
        std::fs::create_dir_all(&review_dir).expect("create review dir");
        let review_dir = review_dir.to_str().unwrap();
        let injected_fence = format!("{review_dir}-store-fence");

        let with = ExecRunner
            .run(
                &gate_cmd(&format!("test \"${STORE_FENCE_ENV}\" = {injected_fence}")),
                review_dir,
                "",
                "",
                "",
                "",
                &injected_fence,
                &BuildEnv::default(),
                &BuildBudget::default(),
            )
            .unwrap();
        assert!(
            with.pass,
            "an injected store_fence must reach the gate via {STORE_FENCE_ENV} even with an \
             empty target_dir: {with:?}"
        );

        let no_cargo_override = ExecRunner
            .run(
                &gate_cmd(&format!("test \"$CARGO_TARGET_DIR\" != {review_dir}")),
                review_dir,
                "",
                "",
                "",
                "",
                &injected_fence,
                &BuildEnv::default(),
                &BuildBudget::default(),
            )
            .unwrap();
        assert!(
            no_cargo_override.pass,
            "an injected store_fence must never force a CARGO_TARGET_DIR override too: \
             {no_cargo_override:?}"
        );

        let unfenced = ExecRunner
            .run(
                &gate_cmd(&format!("test -z \"${STORE_FENCE_ENV}\"")),
                "",
                "",
                "",
                "",
                "",
                "",
                &BuildEnv::default(),
                &BuildBudget::default(),
            )
            .unwrap();
        assert!(
            unfenced.pass,
            "an empty store_fence (the un-widened default) must not force a fence: {unfenced:?}"
        );
    }

    fn gate_cmd(run: &str) -> Gate {
        Gate {
            id: "g".into(),
            run: run.into(),
            kind: Kind::Core,
            autonomy: Autonomy::Manual,
            history: vec![],
        }
    }

    // --- resolve_requirements (spec 113, A GATE DECLARES WHAT IT REQUIRES) ------------------
    //
    // The pure core takes PATH as a value, so these drive it against a synthetic PATH built
    // from temp dirs and never touch (or race on) the process-global PATH.
    // The two PATH-component cases stage their stub in this process's working directory,
    // because an empty component names that directory and a relative one resolves against
    // it; `tempfile` removes each stub when its test ends.

    fn write_executable(dir: &std::path::Path, name: &str) -> std::path::PathBuf {
        let path = dir.join(name);
        std::fs::write(&path, "#!/bin/sh\nexit 0\n").expect("write fixture binary");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755))
                .expect("chmod fixture binary");
        }
        path
    }

    /// A search path over `dirs`, in order, with no `PATHEXT`.
    fn search_over(dirs: &[&std::path::Path]) -> SearchPath {
        SearchPath {
            dirs: std::env::join_paths(dirs.iter().copied()).expect("join synthetic PATH"),
            ..SearchPath::default()
        }
    }

    /// A gates map of `(gate id, requires)` pairs, every gate's command `true`.
    fn gates_requiring(
        gates: &[(&str, &[&str])],
    ) -> std::collections::BTreeMap<String, rigger_domain::config::Gate> {
        gates
            .iter()
            .map(|(id, requires)| {
                let gate = rigger_domain::config::Gate {
                    run: "true".into(),
                    requires: requires.iter().map(|r| (*r).to_string()).collect(),
                    ..Default::default()
                };
                ((*id).to_string(), gate)
            })
            .collect()
    }

    fn resolved(name: &str, at: &std::path::Path) -> ResolvedRequirement {
        ResolvedRequirement {
            name: name.into(),
            at: at.to_path_buf(),
        }
    }

    fn unavailable(gate: &str, requirement: &str) -> RequirementUnavailable {
        RequirementUnavailable {
            gate: gate.into(),
            requirement: requirement.into(),
        }
    }

    #[test]
    fn resolve_requirements_resolves_every_entry_in_gate_id_then_list_order_as_written() {
        let dir = tempfile::tempdir().expect("tempdir");
        let tool_a = write_executable(dir.path(), "tool-a");
        let tool_b = write_executable(dir.path(), "tool-b");
        let gates = gates_requiring(&[
            ("sweep", &["tool-b", "tool-a", "tool-b"]),
            ("build", &[]),
            ("lint", &["tool-a"]),
        ]);
        assert_eq!(
            resolve_requirements(&gates, &search_over(&[dir.path()])),
            Ok(vec![
                GateRequirements {
                    gate: "build".into(),
                    requires: vec![],
                },
                GateRequirements {
                    gate: "lint".into(),
                    requires: vec![resolved("tool-a", &tool_a)],
                },
                GateRequirements {
                    gate: "sweep".into(),
                    requires: vec![
                        resolved("tool-b", &tool_b),
                        resolved("tool-a", &tool_a),
                        resolved("tool-b", &tool_b),
                    ],
                },
            ]),
            "every gate in gate-id order, each entry in list order, a repeated name twice"
        );
    }

    #[test]
    fn resolve_requirements_of_no_gates_is_empty() {
        assert_eq!(
            resolve_requirements(&gates_requiring(&[]), &search_over(&[])),
            Ok(vec![])
        );
    }

    #[test]
    fn resolve_requirements_resolves_each_entry_at_the_first_path_directory_holding_it() {
        let first = tempfile::tempdir().expect("tempdir");
        let second = tempfile::tempdir().expect("tempdir");
        let in_first = write_executable(first.path(), "tool");
        write_executable(second.path(), "tool");
        let only_in_second = write_executable(second.path(), "late-tool");
        let gates = gates_requiring(&[("sweep", &["tool", "late-tool"])]);
        assert_eq!(
            resolve_requirements(&gates, &search_over(&[first.path(), second.path()])),
            Ok(vec![GateRequirements {
                gate: "sweep".into(),
                requires: vec![
                    resolved("tool", &in_first),
                    resolved("late-tool", &only_in_second),
                ],
            }])
        );
    }

    #[test]
    fn resolve_requirements_answers_the_first_missing_entry_in_gate_id_then_list_order() {
        let dir = tempfile::tempdir().expect("tempdir");
        write_executable(dir.path(), "present");
        let gates = gates_requiring(&[
            ("gamma", &["missing-g"]),
            ("alpha", &["present"]),
            ("beta", &["present", "missing-b1", "missing-b2"]),
        ]);
        let err = resolve_requirements(&gates, &search_over(&[dir.path()]))
            .expect_err("a missing requirement refuses");
        assert_eq!(err, unavailable("beta", "missing-b1"));
        assert_eq!(
            err.to_string(),
            "gate \"beta\" requires \"missing-b1\", which is not an executable on PATH \
             (config key: gates.beta.requires)"
        );
    }

    /// The shared case body: `name` resolves missing on a `PATH` of `dir` alone.
    fn assert_requirement_missing(dir: &std::path::Path, name: &str, why: &str) {
        let gates = gates_requiring(&[("sweep", &[name])]);
        assert_eq!(
            resolve_requirements(&gates, &search_over(&[dir])),
            Err(unavailable("sweep", name)),
            "{why}"
        );
    }

    /// The shared case body: `name` resolves missing on a `PATH` of one empty directory.
    fn assert_requirement_missing_on_an_empty_dir(name: &str, why: &str) {
        let dir = tempfile::tempdir().expect("tempdir");
        assert_requirement_missing(dir.path(), name, why);
    }

    crate::test_cases! {
        resolve_requirements_refuses_a_name_on_no_path_directory:
            assert_requirement_missing_on_an_empty_dir("absent-tool", "nothing on PATH holds it");
        resolve_requirements_refuses_an_empty_name:
            assert_requirement_missing_on_an_empty_dir("", "an empty name names no file");
    }

    #[test]
    fn resolve_requirements_ignores_a_same_named_non_executable_file_on_path() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(dir.path().join("tool"), "not a binary").expect("write plain file");
        assert_requirement_missing(
            dir.path(),
            "tool",
            "a stray non-executable file of the same name is never the tool",
        );
    }

    #[test]
    fn resolve_requirements_refuses_a_name_holding_a_slash() {
        let dir = tempfile::tempdir().expect("tempdir");
        let nested = dir.path().join("sub");
        std::fs::create_dir_all(&nested).expect("mkdir sub");
        let stub = write_executable(&nested, "tool");
        assert_requirement_missing(
            dir.path(),
            "sub/tool",
            "a relative name holding `/` names no file in a PATH directory",
        );
        assert_requirement_missing(
            dir.path(),
            stub.to_str().expect("utf-8 stub path"),
            "an absolute name names no file in a PATH directory",
        );
    }

    #[cfg(unix)]
    #[test]
    fn resolve_requirements_resolves_a_symlink_to_an_executable_at_the_symlinks_own_path() {
        let target_dir = tempfile::tempdir().expect("tempdir");
        let link_dir = tempfile::tempdir().expect("tempdir");
        let target = write_executable(target_dir.path(), "real-tool");
        let link = link_dir.path().join("tool");
        std::os::unix::fs::symlink(&target, &link).expect("symlink");
        let gates = gates_requiring(&[("sweep", &["tool"])]);
        assert_eq!(
            resolve_requirements(&gates, &search_over(&[link_dir.path()])),
            Ok(vec![GateRequirements {
                gate: "sweep".into(),
                requires: vec![resolved("tool", &link)],
            }])
        );
    }

    #[cfg(unix)]
    #[test]
    fn resolve_requirements_refuses_a_dangling_symlink() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::os::unix::fs::symlink(dir.path().join("nowhere"), dir.path().join("tool"))
            .expect("symlink");
        assert_requirement_missing(dir.path(), "tool", "a dangling symlink resolves missing");
    }

    /// An executable stub staged in this process's working directory, removed when dropped.
    fn executable_in_cwd() -> tempfile::NamedTempFile {
        let stub = tempfile::Builder::new()
            .prefix("rigger-requires-stub-")
            .tempfile_in(std::env::current_dir().expect("cwd"))
            .expect("stub in cwd");
        std::fs::write(stub.path(), "#!/bin/sh\nexit 0\n").expect("write stub");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(stub.path(), std::fs::Permissions::from_mode(0o755))
                .expect("chmod stub");
        }
        stub
    }

    fn file_name(path: &std::path::Path) -> String {
        path.file_name()
            .and_then(|n| n.to_str())
            .expect("utf-8 file name")
            .to_string()
    }

    #[test]
    fn resolve_requirements_skips_an_empty_path_component() {
        let stub = executable_in_cwd();
        let name = file_name(stub.path());
        assert!(
            is_executable_file(&std::path::Path::new("").join(&name)),
            "precondition: the stub is reachable through an empty component"
        );
        let absolute = tempfile::tempdir().expect("tempdir");
        let path = SearchPath {
            dirs: format!("{}:", absolute.path().display()).into(),
            ..SearchPath::default()
        };
        let gates = gates_requiring(&[("sweep", &[name.as_str()])]);
        assert_eq!(
            resolve_requirements(&gates, &path),
            Err(unavailable("sweep", &name))
        );
    }

    /// A directory staged in this process's working directory holding an executable stub,
    /// with the directory's relative name, both removed when the directory is dropped.
    fn relative_dir_holding(name: &str) -> (tempfile::TempDir, String) {
        let dir = tempfile::Builder::new()
            .prefix("rigger-requires-dir-")
            .tempdir_in(std::env::current_dir().expect("cwd"))
            .expect("dir in cwd");
        write_executable(dir.path(), name);
        let relative = file_name(dir.path());
        assert!(
            is_executable_file(&std::path::Path::new(&relative).join(name)),
            "precondition: the stub is reachable through the relative component"
        );
        (dir, relative)
    }

    #[test]
    fn resolve_requirements_skips_a_relative_path_component() {
        let (_dir, relative) = relative_dir_holding("tool");
        let gates = gates_requiring(&[("sweep", &["tool"])]);
        assert_eq!(
            resolve_requirements(
                &gates,
                &SearchPath {
                    dirs: relative.into(),
                    ..SearchPath::default()
                }
            ),
            Err(unavailable("sweep", "tool"))
        );
    }

    #[test]
    fn resolve_requirements_on_path_resolves_against_the_ambient_path() {
        let ambient = SearchPath::ambient();
        let nothing = gates_requiring(&[("sweep", &[])]);
        assert_eq!(
            resolve_requirements_on_path(&nothing),
            Ok(vec![GateRequirements {
                gate: "sweep".into(),
                requires: vec![],
            }])
        );
        let missing = gates_requiring(&[("sweep", &["rigger-no-such-requirement-on-any-path"])]);
        assert_eq!(
            resolve_requirements_on_path(&missing),
            Err(unavailable(
                "sweep",
                "rigger-no-such-requirement-on-any-path"
            ))
        );
        let present = gates_requiring(&[("sweep", &["sh"])]);
        assert_eq!(
            resolve_requirements_on_path(&present),
            resolve_requirements(&present, &ambient)
        );
    }
}
