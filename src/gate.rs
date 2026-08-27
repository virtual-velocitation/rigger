//! Rigger's verification discipline: a gate is a command plus a trust level, it
//! yields compact evidence (never a raw log), and its autonomy moves on a
//! bidirectional ratchet so a graduated gate can never silently auto-pass bad
//! work. `Runner` is the port; `ExecRunner` is the adapter.

use std::process::Command;

use crate::budget::BuildBudget;

/// Kind classifies a gate's authority lifecycle - how far up the autonomy
/// ratchet it is allowed to travel.
///
/// - `Core` gates ratchet normally: a reliable one can be promoted all the way
///   to `Silent`, integrating unattended.
/// - `Elevated` gates carry a higher safety bar: they may earn `AutoNotify` but
///   can **never become silent**. The ceiling is enforced in
///   [`next_autonomy`] (which caps an elevated promotion at `AutoNotify`) and in
///   [`propose_promotion`] (which stops proposing once an elevated gate has
///   reached its `AutoNotify` ceiling), so a graduated elevated gate always
///   surfaces a notification a human can veto - it never auto-passes silently.
/// - `Deferred` gates are held until a phase boundary rather than run inline; see
///   [`Kind::runs_inline`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Core,
    Elevated,
    Deferred,
}

impl Kind {
    pub fn parse(s: &str) -> Kind {
        match s {
            "elevated" => Kind::Elevated,
            "deferred" => Kind::Deferred,
            _ => Kind::Core,
        }
    }

    /// The highest autonomy this kind of gate is allowed to ratchet to. `Core`
    /// and `Deferred` gates may reach `Silent`; an `Elevated` gate tops out at
    /// `AutoNotify` so its verdicts always surface for a human to veto.
    pub fn ceiling(&self) -> Autonomy {
        match self {
            Kind::Elevated => Autonomy::AutoNotify,
            Kind::Core | Kind::Deferred => Autonomy::Silent,
        }
    }

    /// Whether a gate of this kind runs inline with its stage. `Deferred` gates
    /// are held until a phase boundary instead of running in-line.
    pub fn runs_inline(&self) -> bool {
        !matches!(self, Kind::Deferred)
    }
}

/// Autonomy is how much a gate is trusted to run unattended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Autonomy {
    Manual,
    AutoNotify,
    Silent,
}

impl Autonomy {
    /// Parse an autonomy string. An empty / unset value defaults to `AutoNotify`
    /// (§4.3): an unconfigured gate still runs and integrates unattended; only an
    /// explicit `manual` pauses a unit for human review. `manual` is therefore
    /// opt-in, never the silent default.
    pub fn parse(s: &str) -> Autonomy {
        match s {
            "manual" => Autonomy::Manual,
            "silent" => Autonomy::Silent,
            _ => Autonomy::AutoNotify,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Autonomy::Manual => "manual",
            Autonomy::AutoNotify => "auto_notify",
            Autonomy::Silent => "silent",
        }
    }

    /// Position on the ratchet, ascending from least to most autonomous. Used to
    /// compare an autonomy against a [`Kind::ceiling`].
    fn rank(&self) -> u8 {
        match self {
            Autonomy::Manual => 0,
            Autonomy::AutoNotify => 1,
            Autonomy::Silent => 2,
        }
    }
}

/// Consecutive clean passes that propose a promotion.
pub const PROMOTE_THRESHOLD: usize = 3;

/// A gate's verdict with compact evidence.
#[derive(Clone, Debug)]
pub struct GateResult {
    pub pass: bool,
    pub evidence: String,
}

/// One run of a gate, for the ratchet's history.
#[derive(Clone, Debug)]
pub struct HistoryEntry {
    pub pass: bool,
}

/// A verification command and its trust.
#[derive(Clone, Debug)]
pub struct Gate {
    pub id: String,
    pub run: String,
    pub kind: Kind,
    pub autonomy: Autonomy,
    pub history: Vec<HistoryEntry>,
}

/// The conductor's action for a gate, given its autonomy.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    RunSilent,
    RunNotify,
    Pause,
}

/// Runner runs a gate command in a working directory (`dir`, "" = current dir) under an
/// optional `CARGO_TARGET_DIR` override (`target_dir`, "" = inherit the ambient env) and
/// the resolved shared [`BuildEnv`] (spec 65's ONE build-environment authority - the
/// default/empty `BuildEnv` applies nothing, leaving today's behavior unchanged).
///
/// A gate that runs INSIDE a unit's worktree is handed a unit-keyed `target_dir` (Gap 19)
/// so divergent unit trees never share one build cache - a compile error a gate sees is
/// then always that unit's own, never a concurrent neighbour poisoning a shared target. A
/// gate on the single integrated tree (the deferred phase-boundary gate, and the courier's
/// inline `rigger step` gates) is handed "" and keeps inheriting the shared cache.
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
pub trait Runner: Send + Sync {
    #[allow(clippy::too_many_arguments)]
    fn run(
        &self,
        g: &Gate,
        dir: &str,
        target_dir: &str,
        build_cache_dir: &str,
        build_cache_guard: &str,
        store_fence: &str,
        build_env: &BuildEnv,
        budget: &BuildBudget,
    ) -> GateResult;
}

/// Decide maps a gate's autonomy to the conductor's action.
pub fn decide(g: &Gate) -> Action {
    match g.autonomy {
        Autonomy::Silent => Action::RunSilent,
        Autonomy::AutoNotify => Action::RunNotify,
        Autonomy::Manual => Action::Pause,
    }
}

/// ProposePromotion reports whether a gate has earned a promotion: the last
/// PROMOTE_THRESHOLD runs all passed, and it has not already reached its kind's
/// autonomy ceiling. A `Core` gate's ceiling is `Silent`; an `Elevated` gate's
/// ceiling is `AutoNotify`, so a reliable elevated gate stops being proposed for
/// promotion once it reaches `AutoNotify` - it can never be proposed for
/// `Silent`.
pub fn propose_promotion(g: &Gate) -> bool {
    if g.autonomy.rank() >= g.kind.ceiling().rank() || g.history.len() < PROMOTE_THRESHOLD {
        return false;
    }
    g.history[g.history.len() - PROMOTE_THRESHOLD..]
        .iter()
        .all(|h| h.pass)
}

/// NextAutonomy returns the autonomy one notch up the ratchet for a gate, capping
/// at the gate's kind ceiling: `Silent` for a `Core`/`Deferred` gate, but only
/// `AutoNotify` for an `Elevated` gate (which can never become silent).
pub fn next_autonomy(g: &Gate) -> Autonomy {
    let stepped = match g.autonomy {
        Autonomy::Manual => Autonomy::AutoNotify,
        _ => Autonomy::Silent,
    };
    let ceiling = g.kind.ceiling();
    if stepped.rank() > ceiling.rank() {
        ceiling
    } else {
        stepped
    }
}

/// AutoDemote drops a non-manual gate to Manual when it fails, returning the new
/// autonomy and whether a demotion happened.
pub fn auto_demote(g: &Gate, pass: bool) -> (Autonomy, bool) {
    if !pass && g.autonomy != Autonomy::Manual {
        (Autonomy::Manual, true)
    } else {
        (g.autonomy, false)
    }
}

/// BuildEnv is the ONE build-environment authority (spec 65): a single resolver
/// derives the env vars from committed config and applies them everywhere a caller
/// threads it through. Wired so far: inline/deferred gate builds ([`ExecRunner::run`])
/// and the blocking `driver::cli` agent driver's spawned process (via
/// `conductor::SpawnOpts`), so a gate build and that driver's own `cargo test`
/// invocation hit the same compilation cache under the same settings, at the same
/// jobs cap. The turn-key `rigger workflow` Node-shim driver and `driver::replay`'s
/// wire contract do not carry these vars yet - a disclosed, tracked gap for a
/// follow-on unit, not something this resolver claims to reach today. One resolver,
/// two wired injection sites; neither derives its own competing copy.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BuildEnv {
    vars: Vec<(String, String)>,
}

impl BuildEnv {
    /// Resolve the build environment from the workflow's `build.wrapper` /
    /// `build.cache_dir` / `build.jobs` config (§65). An empty or
    /// (case/whitespace-insensitive) `off` wrapper resolves NO wrapper vars at all -
    /// a build this loop runs then inherits the ambient environment untouched,
    /// exactly as before this authority existed; no silent injection of anything
    /// unasked for.
    ///
    /// A configured wrapper (any other value, taken VERBATIM) resolves to exactly
    /// three vars:
    /// - `RUSTC_WRAPPER` - the wrapper binary. Deciding `auto` (probe PATH for a
    ///   known wrapper) or erroring loudly on a named-but-absent binary is spec 65
    ///   unit 2's job (NO SILENT DEGRADE), layered on top of this foundational
    ///   shape; this resolver passes whatever string it is given straight through.
    /// - `<WRAPPER>_DIR` (the wrapper's binary name, uppercased, suffixed `_DIR` -
    ///   the generic cache-directory convention ccache/sccache/buildcache all
    ///   share, so rigger hardcodes no specific tool) - set to `cache_dir`, or the
    ///   default [`default_cache_dir`] when unset, so every project and worktree on
    ///   the machine shares one cache absent explicit configuration.
    /// - `CARGO_INCREMENTAL=0` - incremental output defeats wrapper caching; the
    ///   per-unit warm target dirs (Gap 19 / spec 64) carry the incremental win
    ///   instead.
    ///
    /// `jobs` (spec 65 unit 4 - JOBS CAP) is its OWN facet, independent of the
    /// wrapper: `0` (the config default) means unset and injects nothing, leaving
    /// cargo's own ambient default parallelism untouched; a positive value resolves
    /// to `CARGO_BUILD_JOBS`, added whether or not a wrapper is configured, so
    /// `build.max_concurrent` (unit 3's slot budget) x `build.jobs` can be sized to
    /// the machine regardless of the compilation-cache layer.
    pub fn resolve(wrapper: &str, cache_dir: &str, jobs: u32) -> BuildEnv {
        let wrapper = wrapper.trim();
        let mut vars = Vec::new();
        if !(wrapper.is_empty() || wrapper.eq_ignore_ascii_case("off")) {
            let dir = resolved_cache_dir(cache_dir);
            let dir_var = format!("{}_DIR", wrapper.to_ascii_uppercase());
            vars.push(("RUSTC_WRAPPER".to_string(), wrapper.to_string()));
            vars.push((dir_var, dir));
            vars.push(("CARGO_INCREMENTAL".to_string(), "0".to_string()));
        }
        if jobs > 0 {
            vars.push(("CARGO_BUILD_JOBS".to_string(), jobs.to_string()));
        }
        BuildEnv { vars }
    }

    /// The resolved vars as `(name, value)` pairs, for a caller (the agent driver's
    /// `SpawnOpts`) that carries them onward rather than applying them to a
    /// `Command` directly.
    pub fn vars(&self) -> &[(String, String)] {
        &self.vars
    }

    /// Apply every resolved var to `cmd`, so this environment reaches the process
    /// unchanged whether the caller is a gate's own `Command` ([`ExecRunner::run`])
    /// or an agent driver's.
    pub fn apply(&self, cmd: &mut Command) {
        for (k, v) in &self.vars {
            cmd.env(k, v);
        }
    }
}

/// The default shared build-cache location when `build.cache_dir` is unset:
/// `<state home>/rigger/build-cache`, reusing the registry's own state-home
/// precedence so every project and worktree on the machine shares one cache absent
/// configuration (spec 65). Falls back to a bare relative name in a truly homeless
/// environment (no `XDG_STATE_HOME`/`HOME`) - the wrapper still gets pointed
/// SOMEWHERE consistent rather than left unset.
///
/// Reads `XDG_STATE_HOME`/`HOME` itself, once, right here, and hands them to
/// [`crate::registry::state_home_from`] - the registry's PURE core - rather than
/// calling the registry's own ambient-reading [`crate::registry::state_home`], so
/// this stays the ONE place outside `registry.rs` that touches those two vars
/// instead of hiding the read behind an opaque wrapper a caller cannot reason
/// about.
fn default_cache_dir() -> String {
    crate::registry::state_home_from(std::env::var_os("XDG_STATE_HOME"), std::env::var_os("HOME"))
        .map(|h| h.join("rigger").join("build-cache"))
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|| "rigger-build-cache".to_string())
}

/// The cache-directory value [`BuildEnv::resolve`] (what the `_DIR` env var carries), the
/// build-layer cache-dir probe (spec 65 unit 2, below), AND `rigger validate`'s SURFACES
/// report (spec 65 unit 5 - reads this directly rather than re-deriving the same ternary a
/// third time) all need: the configured `cache_dir` verbatim when set, or
/// [`default_cache_dir`] when it is empty/whitespace-only. `pub` so it is the one place
/// every caller across the crate reads, never a second independent copy that could
/// silently drift from this one.
pub fn resolved_cache_dir(cache_dir: &str) -> String {
    if cache_dir.trim().is_empty() {
        default_cache_dir()
    } else {
        cache_dir.trim().to_string()
    }
}

/// The compilation-cache wrapper binaries [`resolve_wrapper_name`] probes for under
/// `build.wrapper: auto` (spec 65 unit 2, NO SILENT DEGRADE): a small, config-extensible
/// pit-of-success default - `auto` exists so a machine that already has one of these
/// installed benefits without demanding config. A NAMED wrapper (any other `build.wrapper`
/// string) bypasses this list entirely and is checked directly against PATH instead -
/// rigger hardcodes no tool as the only option, just this discovery default.
const KNOWN_WRAPPERS: &[&str] = &["sccache", "ccache"];

/// A CONFIGURED `build.wrapper` binary that is not on PATH (spec 65 unit 2, NO SILENT
/// DEGRADE). The operator named it explicitly, so its absence is a configured-explicit
/// failure - surfaced loudly at run start (via [`crate::config::Config::validate`]) -
/// never silently skipped the way an `auto` probe finding nothing degrades.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("build.wrapper {binary:?} is not on PATH (config key: build.wrapper)")]
pub struct WrapperUnavailable {
    pub binary: String,
}

/// Pure: whether `bin` names an executable regular file inside any directory of `path_var`
/// (a PATH-style, platform-separator-joined directory list from [`std::env::split_paths`]),
/// checked in listed order.
fn path_has_executable(path_var: &std::ffi::OsStr, bin: &str) -> bool {
    std::env::split_paths(path_var).any(|dir| is_executable_file(&dir.join(bin)))
}

#[cfg(unix)]
fn is_executable_file(path: &std::path::Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(path)
        .map(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

#[cfg(not(unix))]
fn is_executable_file(path: &std::path::Path) -> bool {
    path.is_file()
}

/// Resolve `build.wrapper` to the effective binary name the shared-cache layer will use, or
/// `None` when the layer is disabled - the pure core, taking PATH as a value (`path_var`)
/// rather than reading the environment itself, so it is unit-testable against a synthetic
/// PATH without mutating (or racing on) the real process environment; see
/// [`resolve_wrapper_name`] for the ambient-reading edge production callers use. Spec 65
/// unit 2, NO SILENT DEGRADE, layered on top of [`BuildEnv::resolve`]'s own foundational
/// (verbatim, wrapper-agnostic) shape:
/// - empty / (case- and whitespace-insensitive) `off`: unchanged from `BuildEnv::resolve`'s
///   own early return - `Ok(None)`, no injection, PATH never consulted.
/// - `auto`: probes [`KNOWN_WRAPPERS`] against `path_var` in order and returns the first
///   present, or `Ok(None)` when none is - a DISCOVERED-IMPLICIT degrade: the layer is
///   silently SKIPPED, never silently miswired with a wrapper name nothing can run.
/// - any other string (a NAMED wrapper, not restricted to [`KNOWN_WRAPPERS`]): present on
///   `path_var` resolves `Ok(Some(name))`; absent is a CONFIGURED-EXPLICIT failure,
///   `Err(WrapperUnavailable)` naming the binary - the operator asked for it by name, so
///   silence here would fake a cache that never actually runs.
pub fn resolve_wrapper_name_from(
    wrapper: &str,
    path_var: &std::ffi::OsStr,
) -> Result<Option<String>, WrapperUnavailable> {
    let w = wrapper.trim();
    if w.is_empty() || w.eq_ignore_ascii_case("off") {
        return Ok(None);
    }
    if w.eq_ignore_ascii_case("auto") {
        return Ok(KNOWN_WRAPPERS
            .iter()
            .find(|b| path_has_executable(path_var, b))
            .map(|b| (*b).to_string()));
    }
    if path_has_executable(path_var, w) {
        Ok(Some(w.to_string()))
    } else {
        Err(WrapperUnavailable {
            binary: w.to_string(),
        })
    }
}

/// The ambient-PATH-reading edge [`resolve_wrapper_name_from`]'s WRAPPER-BINARY-axis callers
/// use: reads the real `PATH` once, right here (mirrors [`default_cache_dir`]'s own
/// `XDG_STATE_HOME`/`HOME` read pattern), and hands it to the pure core. Folded into
/// [`resolve_build_layer`] below (the production entry point, which also checks the
/// cache-directory axis) rather than called directly by config/conductor/CLI production code -
/// kept `pub` as the wrapper-only building block its own tests exercise and
/// [`resolve_build_layer`] composes.
pub fn resolve_wrapper_name(wrapper: &str) -> Result<Option<String>, WrapperUnavailable> {
    resolve_wrapper_name_from(wrapper, &std::env::var_os("PATH").unwrap_or_default())
}

/// A `build.cache_dir` (or the DEFAULT [`default_cache_dir`]/[`resolved_cache_dir`] when
/// unset) that cannot be created, OR that already exists but is not WRITABLE (see
/// [`usable_with_cache_dir`]'s probe), for a CONFIGURED (non-`auto`, non-`off`)
/// `build.wrapper` (spec 65 unit 2, NO SILENT DEGRADE): the SAME Design sentence
/// (specs/65:26-28) that decides a named-but-absent wrapper BINARY is a configured-explicit
/// failure decides this failure direction too - the operator asked for the wrapper (and, if
/// set, this exact dir) by name, so proceeding would silently fake a cache that never
/// actually writes anything. Mirrors [`WrapperUnavailable`]'s shape - a plain `(field,
/// field)` struct naming what failed and the config key, not a raw `io::Error` (neither
/// `Clone` nor `PartialEq`, so a caller could not compare/propagate it alongside
/// `WrapperUnavailable` uniformly).
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("build.cache_dir {dir:?} is not usable: {reason} (config key: build.cache_dir)")]
pub struct CacheDirUnusable {
    pub dir: String,
    pub reason: String,
}

/// Creates `dir` if needed and proves it is actually WRITABLE, not merely creatable (closing
/// the "creatable but not writable" gap): for an ALREADY-EXISTING directory - the realistic
/// steady state, since [`default_cache_dir`] is a machine-wide dir every project reuses
/// after the first one creates it - `std::fs::create_dir_all` alone is a no-op `Ok(())`
/// regardless of write permission, so it is not proof the cache is actually live. Writes,
/// then best-effort removes, a small probe file inside `dir` (name salted with the PID so
/// concurrent callers sharing one cache dir never collide on it); any failure -
/// `create_dir_all` OR the probe write - is reported identically to the caller, which is
/// exactly what [`usable_with_cache_dir`] needs: it does not care WHICH step failed, only
/// that the dir turned out unusable.
fn ensure_cache_dir_writable(dir: &str) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    let probe =
        std::path::Path::new(dir).join(format!(".rigger-cache-probe-{}", std::process::id()));
    std::fs::write(&probe, b"")?;
    // Best-effort cleanup: a probe file left behind by a failed remove (e.g. some exotic
    // filesystem that permits writes but not deletes) is not itself proof the dir is
    // unusable for the cache the wrapper actually needs to WRITE into.
    let _ = std::fs::remove_file(&probe);
    Ok(())
}

/// Either no-silent-degrade axis of the build-environment layer (spec 65 unit 2): a resolved
/// wrapper NAME is only genuinely usable when BOTH its binary is on PATH
/// ([`WrapperUnavailable`]) and its cache directory is actually writable ([`CacheDirUnusable`]) -
/// the SAME Design sentence decides both failure directions, so [`resolve_build_layer`] /
/// [`resolve_build_layer_from`] fold them into this ONE error every caller matches once
/// rather than two independently-shaped results.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum BuildLayerUnavailable {
    #[error(transparent)]
    Wrapper(#[from] WrapperUnavailable),
    #[error(transparent)]
    CacheDir(#[from] CacheDirUnusable),
}

/// The wrapper name resolved by either axis is only USABLE once its cache directory is too
/// (spec 65 unit 2): the cache-dir half of [`resolve_build_layer`] /
/// [`resolve_build_layer_from`], shared so neither duplicates this branch. `named` is
/// whether `build.wrapper` was a CONFIGURED string (not `auto`) - the same
/// configured-explicit-vs-discovered-implicit distinction [`resolve_wrapper_name_from`]
/// already draws for the PATH axis, mirrored here for the cache-dir axis: a named wrapper's
/// unusable dir errors loudly; an auto-discovered wrapper's unusable dir silently skips the
/// whole layer (`Ok(None)`), exactly like auto finding no wrapper binary at all. "Unusable"
/// is [`ensure_cache_dir_writable`]'s call, not a bare `create_dir_all`: an already-EXISTING
/// dir that cannot be WRITTEN into - the realistic steady state for a persisted, shared
/// cache dir - must fail this exactly like one that cannot be created at all, never report
/// the layer usable.
fn usable_with_cache_dir(
    name: Option<String>,
    named: bool,
    cache_dir: &str,
) -> Result<Option<String>, BuildLayerUnavailable> {
    let Some(name) = name else {
        return Ok(None);
    };
    let dir = resolved_cache_dir(cache_dir);
    match ensure_cache_dir_writable(&dir) {
        Ok(()) => Ok(Some(name)),
        Err(_) if !named => Ok(None),
        Err(e) => Err(CacheDirUnusable {
            dir,
            reason: e.to_string(),
        }
        .into()),
    }
}

/// Whether `wrapper` is a CONFIGURED name rather than the `auto` discovery keyword -
/// `off`/empty never reach here (both axes resolve to `Ok(None)` before this matters), so
/// this only ever discriminates a NAMED wrapper from `auto`.
fn is_named_wrapper(wrapper: &str) -> bool {
    !wrapper.trim().eq_ignore_ascii_case("auto")
}

/// The build-environment layer's resolved, USABLE wrapper name (spec 65 unit 2): the ONE
/// production entry point that folds BOTH no-silent-degrade axes - the wrapper binary
/// ([`resolve_wrapper_name_from`]) and the cache directory ([`usable_with_cache_dir`]) -
/// into the single decision every caller needs, since the Design decides both in the SAME
/// sentence (specs/65:26-28). [`crate::config::Config::validate`] (the run-start loud-failure
/// check), the conductor's build-environment authority, and `rigger validate`'s reporting
/// surface all call this - never re-deriving the wrapper-vs-cache-dir, named-vs-auto
/// distinction independently. Pure core; see [`resolve_build_layer`] for the ambient-PATH
/// edge production callers use.
pub fn resolve_build_layer_from(
    wrapper: &str,
    cache_dir: &str,
    path_var: &std::ffi::OsStr,
) -> Result<Option<String>, BuildLayerUnavailable> {
    let name = resolve_wrapper_name_from(wrapper, path_var)?;
    usable_with_cache_dir(name, is_named_wrapper(wrapper), cache_dir)
}

/// The ambient-PATH-reading edge [`resolve_build_layer_from`]'s production callers use -
/// mirrors [`resolve_wrapper_name`]'s own ambient-PATH read, composed with the cache-dir
/// axis. See [`resolve_build_layer_from`] for the full contract.
pub fn resolve_build_layer(
    wrapper: &str,
    cache_dir: &str,
) -> Result<Option<String>, BuildLayerUnavailable> {
    resolve_build_layer_from(
        wrapper,
        cache_dir,
        &std::env::var_os("PATH").unwrap_or_default(),
    )
}

/// The fixed binary [`resolve_mutation_layer_from`] probes PATH for when `build.mutation` is
/// `on` (spec 73). Unlike `build.wrapper`, this name is not configurable - the mutation
/// efficacy step always shells out to `cargo mutants`, so there is exactly one binary to
/// resolve, never a list or an operator-named override.
const MUTATION_BINARY: &str = "cargo-mutants";

/// A `build.mutation: on` whose required [`MUTATION_BINARY`] is not resolvable on PATH
/// (spec 73, ENABLED-BUT-ABSENT FAILS AT RUN START): mirrors [`WrapperUnavailable`]'s
/// configured-explicit-failure shape (spec 65 unit 2) - the operator turned the mutation
/// efficacy step on explicitly, so proceeding would silently skip a check they asked for.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("build.mutation is on but {binary:?} is not on PATH (config key: build.mutation)")]
pub struct MutationBinaryUnavailable {
    pub binary: String,
}

/// Resolve `build.mutation` (spec 73) against `path_var` (pure core, PATH as a value -
/// mirrors [`resolve_wrapper_name_from`]'s own testability shape): a strict two-value gate,
/// unlike `build.wrapper`'s three-way (`off`/named/`auto`) - there is no discovery state
/// here, so this never silently degrades.
/// - empty / (case- and whitespace-insensitive) `off` / anything other than `on`: resolves
///   `Ok(false)` WITHOUT ever consulting `path_var` - the disabled step costs nothing and
///   probes nothing.
/// - `on` (case- and whitespace-insensitive): probes for [`MUTATION_BINARY`] on `path_var`;
///   present resolves `Ok(true)`, absent is a CONFIGURED-EXPLICIT failure -
///   `Err(MutationBinaryUnavailable)` naming the binary - the operator asked for the step by
///   turning it on, so silence here would fake a check that never actually runs.
pub fn resolve_mutation_layer_from(
    mutation: &str,
    path_var: &std::ffi::OsStr,
) -> Result<bool, MutationBinaryUnavailable> {
    if !mutation.trim().eq_ignore_ascii_case("on") {
        return Ok(false);
    }
    if path_has_executable(path_var, MUTATION_BINARY) {
        Ok(true)
    } else {
        Err(MutationBinaryUnavailable {
            binary: MUTATION_BINARY.to_string(),
        })
    }
}

/// The ambient-PATH-reading edge [`resolve_mutation_layer_from`]'s production callers use -
/// mirrors [`resolve_build_layer`]'s own ambient-PATH read. [`crate::config::Config::validate`]
/// (the run-start loud-failure check) and `rigger validate`'s reporting surface both call
/// this - never re-deriving the on/off, binary-probe distinction independently.
pub fn resolve_mutation_layer(mutation: &str) -> Result<bool, MutationBinaryUnavailable> {
    resolve_mutation_layer_from(mutation, &std::env::var_os("PATH").unwrap_or_default())
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

/// The scratch-dir naming suffix [`ExecRunner::run`] appends to `target_dir` to derive the
/// store fence's own sibling scratch dir (`{target_dir}{STORE_FENCE_SUFFIX}`). Shared with
/// `worktree::reclaim_cache_sibling` (the ONE reclaim authority for a unit's
/// `cargo-target-<slug>` cache sibling) so it reclaims the fence's sibling by the exact
/// same name it was created under, rather than a second, independently-spelled copy that
/// could drift from this one.
pub const STORE_FENCE_SUFFIX: &str = "-store-fence";

/// ExecRunner runs a gate as a shell command, reducing output to compact evidence.
pub struct ExecRunner;

impl Runner for ExecRunner {
    fn run(
        &self,
        g: &Gate,
        dir: &str,
        target_dir: &str,
        build_cache_dir: &str,
        build_cache_guard: &str,
        store_fence: &str,
        build_env: &BuildEnv,
        budget: &BuildBudget,
    ) -> GateResult {
        // The shared gate build cache's exclusion protocol, PRODUCER half (spec 77
        // criterion 5, BOUNDED SHARED CACHE), FIXED round 2 for
        // adv-u77c5bsc-guard-lifetime-bound-to-orchestrator-not-the-cargo-child: a
        // non-empty `build_cache_guard` now wraps the WHOLE gate command through an
        // EXEC-REPLACING `flock -s -F -- <guard> sh -c '<g.run>'`, rather than opening
        // and `fs2`-locking the guard fd IN THIS PROCESS and merely holding it across a
        // separately spawned child (`Command::output()`'s own fork+exec). That older
        // shape tied the lock's lifetime to THIS orchestrator process: std's `File`
        // always opens close-on-exec, so the spawned `sh -c` child never inherited the
        // locked fd, and the lock was released the instant this process died - even
        // while a live, orphaned cargo/rustc tree that child had spawned kept writing
        // into the very cache the lock exists to protect. `flock(1)`'s `-F`/`--no-fork`
        // takes the shared lock ITSELF, in the CHILD process `Command::output()` forks,
        // then execve()s directly into the wrapped `sh -c` - preserving the locked fd
        // across that exec (flock deliberately does not set close-on-exec on it) - so
        // the exec'd shell, and every cargo/rustc descendant it forks in turn, inherits
        // the SAME open file description the lock is held against and keeps it held for
        // as long as ANY of them is alive. If this orchestrator alone is killed
        // mid-build (the exact "killed runs never clean up" crash shape spec 77's own
        // Problem statement targets, and the class the driver's own liveness heartbeat
        // exists around), the already-forked child is a genuinely separate OS process
        // and is wholly unaffected - it keeps the fd it always held, never re-acquiring
        // anything. This mirrors `src/budget.rs`'s own proven pattern for the identical
        // "supervisor may die independently of the work it launched" problem class (its
        // own test fixture already relies on this exact flock(1)-preserves-the-fd-
        // across-exec guarantee to simulate an abnormally-killed holder).
        //
        // `build_cache_guard_is_usable` is a best-effort, released-before-use PROBE
        // (never held across the child) mirroring `BuildBudget::acquire`'s own
        // degrade-to-unlimited convention: filesystem trouble unrelated to the build
        // itself (an unwritable guard path, a missing parent dir) degrades to running
        // unguarded rather than failing a gate over a lock this build does not
        // otherwise need.
        let guarded =
            !build_cache_guard.is_empty() && build_cache_guard_is_usable(build_cache_guard);
        let mut cmd = if guarded {
            let mut c = Command::new("flock");
            c.arg("-s")
                .arg("-F")
                .arg("--")
                .arg(build_cache_guard)
                .arg("sh")
                .arg("-c")
                .arg(&g.run);
            c
        } else {
            let mut c = Command::new("sh");
            c.arg("-c").arg(&g.run);
            c
        };
        if !dir.is_empty() {
            cmd.current_dir(dir);
        }
        // Per-unit build cache (Gap 19): a non-empty target_dir points cargo at a
        // unit-keyed CARGO_TARGET_DIR so this gate's incremental state is never shared
        // with a concurrent unit's divergent tree. The empty/ambient case (below) is the
        // shared-cache one `build_cache_dir` now pins explicitly (spec 77 criterion 5).
        if !target_dir.is_empty() {
            cmd.env("CARGO_TARGET_DIR", target_dir);
            // The gate store fence (spec 70 criterion 3), on the SAME signal: a non-empty
            // target_dir means this gate runs inside a unit worktree, so a store-opening
            // courier the gate's spawned process runs must never walk up into the repo's
            // LIVE run stream. Pin a sibling scratch dir (never inside target_dir itself,
            // which cargo owns and may wipe) so main.rs's store-resolution authority
            // resolves there instead of walking up. This branch always wins over
            // `store_fence` below - a unit worktree derives its own fence from target_dir.
            cmd.env(STORE_FENCE_ENV, format!("{target_dir}{STORE_FENCE_SUFFIX}"));
        } else {
            // The shared gate build cache's CONSUMER half (spec 77 criterion 5, BOUNDED
            // SHARED CACHE - the round 8/9 lesson from this same criterion's own prior
            // review history): a non-empty `build_cache_dir` FORCES this ambient-cache
            // gate build onto the exact directory `build_cache_guard` protects, rather
            // than trusting whatever CARGO_TARGET_DIR the ambient/inherited process env
            // happens to carry. Without this, the guard and the real cargo invocation
            // could name two different directories the moment an external launcher's
            // hardcoded courier env diverges from the RIGGER_TMPDIR/defaults.workdir-
            // aware path the guard is derived from - `rigger reset --build-cache` would
            // then reclaim an empty decoy while the real, growing cache goes both
            // unprotected and unreclaimed. Both `build_cache_dir` and `build_cache_guard`
            // are derived from the identical scratch-root authority by the caller
            // (`conductor::shared_build_cache_paths`), so this can never disagree with
            // the lock it is paired with.
            if !build_cache_dir.is_empty() {
                cmd.env("CARGO_TARGET_DIR", build_cache_dir);
            }
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
        // The ONE build-environment authority's first injection site (spec 65): the
        // resolved wrapper/cache-dir/incremental-off vars (empty when no wrapper is
        // configured, applying nothing) so this gate build and an agent's own
        // `cargo test` hit the same compilation cache under the same settings.
        build_env.apply(&mut cmd);
        // The machine-wide build budget's ONE gating point (spec 65): held for exactly
        // this command's duration, so a concurrent gate build elsewhere on the machine
        // waits for a free slot rather than stacking another compiler fleet into
        // memory. Never held across anything but this call - a hung command still
        // frees its slot the instant it is killed, and no other code path in the crate
        // acquires a slot, so non-build work is never gated.
        let _slot = budget.acquire();
        match cmd.output() {
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
        }
    }
}

/// Best-effort capability PROBE for `path` as a shared-cache guard (spec 77 criterion 5,
/// BOUNDED SHARED CACHE): briefly creates it, takes a shared `fs2` lock, and immediately
/// releases it - never held across anything - purely to confirm the path is writable and
/// this filesystem supports advisory locks, mirroring `BuildBudget::acquire`'s own
/// degrade-to-unlimited convention for the identical class of trouble (an unwritable guard
/// path, a missing parent dir). Empty disables this entirely (a per-unit build's own
/// `cargo-target-<slug>` cache is never at risk from `rigger reset --build-cache`, matching
/// `target_dir`'s own "empty means off" convention).
///
/// The REAL, invocation-lifetime lock is taken by the external `flock(1)` process
/// `ExecRunner::run` execs into when this returns `true` - never by this probe. Locking
/// in-process here, even briefly, is safe only because it is released before the gate
/// command ever spawns; holding it across that spawn is precisely the bug this function's
/// caller exists to fix (see `ExecRunner::run`'s own doc comment).
fn build_cache_guard_is_usable(path: &str) -> bool {
    let Ok(f) = std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(false)
        .open(path)
    else {
        return false;
    };
    // Fully-qualified (not `f.lock_shared()`): this crate deliberately uses `fs2` for
    // every advisory lock ([`crate::budget`], `main.rs`'s step lock) rather than std's
    // more recently stabilized `File::lock_shared`, which shares this exact method name -
    // a plain method call would silently resolve to the std inherent method (inherent
    // methods always win over a trait's) and leave the `fs2` import genuinely unused.
    if fs2::FileExt::lock_shared(&f).is_err() {
        return false;
    }
    let _ = fs2::FileExt::unlock(&f);
    true
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
                    &BuildEnv::default(),
                    &BuildBudget::default()
                )
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
                    &BuildEnv::default(),
                    &BuildBudget::default()
                )
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
        let res = ExecRunner.run(
            &gate_cmd(&cmd),
            "",
            "",
            "",
            "",
            "",
            &BuildEnv::default(),
            &BuildBudget::default(),
        );
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

    #[test]
    fn exec_runner_exports_cargo_target_dir_only_when_given() {
        // Gap 19: a non-empty target_dir is exported to the gate command as
        // CARGO_TARGET_DIR (the unit-keyed build cache); an empty one must NOT force an
        // override, leaving the ambient env in place. The command asserts the value it
        // sees and passes iff it matches.
        let with = ExecRunner.run(
            &gate_cmd("test \"$CARGO_TARGET_DIR\" = /tmp/rigger-gap19-probe"),
            "",
            "/tmp/rigger-gap19-probe",
            "",
            "",
            "",
            &BuildEnv::default(),
            &BuildBudget::default(),
        );
        assert!(
            with.pass,
            "a non-empty target_dir must reach the gate as CARGO_TARGET_DIR: {with:?}"
        );

        let without = ExecRunner.run(
            &gate_cmd("test \"$CARGO_TARGET_DIR\" != /tmp/rigger-gap19-probe"),
            "",
            "",
            "",
            "",
            "",
            &BuildEnv::default(),
            &BuildBudget::default(),
        );
        assert!(
            without.pass,
            "an empty target_dir must not force a CARGO_TARGET_DIR override: {without:?}"
        );
    }

    #[test]
    fn exec_runner_forces_cargo_target_dir_onto_build_cache_dir_when_target_dir_is_empty() {
        // spec 77 criterion 5 (BOUNDED SHARED CACHE), the round 8/9 lesson from this same
        // criterion's own prior review history: when target_dir is empty (the ambient/
        // shared-cache case), a non-empty build_cache_dir must FORCE the real cargo
        // invocation's CARGO_TARGET_DIR onto it - never leaving it to whatever the
        // ambient/inherited process env happens to carry - so the directory a real gate
        // build actually writes into can never silently diverge from the directory
        // `build_cache_guard` (and `rigger reset --build-cache`) protects. No ambient env
        // mutation here (this test binary runs its suite with multiple tests concurrently,
        // and `std::env::set_var` is process-wide) - a direct positive assertion that the
        // spawned command's own CARGO_TARGET_DIR equals build_cache_dir is exactly as
        // conclusive and carries no cross-test race.
        let res = ExecRunner.run(
            &gate_cmd("test \"$CARGO_TARGET_DIR\" = /tmp/rigger-shared-cache-probe"),
            "",
            "",
            "/tmp/rigger-shared-cache-probe",
            "",
            "",
            &BuildEnv::default(),
            &BuildBudget::default(),
        );
        assert!(
            res.pass,
            "an empty target_dir with a non-empty build_cache_dir must force CARGO_TARGET_DIR \
             onto build_cache_dir: {res:?}"
        );
    }

    #[test]
    fn exec_runner_env_vars_reach_the_gate_command_through_the_flock_guard_wrapper() {
        // The guard wrapper (`flock -s -F -- <guard> sh -c '<g.run>'`, spec 77 criterion 5
        // round 2 fix) must never swallow the env vars this function sets on the `Command`
        // BEFORE exec - CARGO_TARGET_DIR here, standing in for every var this module
        // injects (BuildEnv's own wrapper vars, STORE_FENCE_ENV, ...). Proven with a REAL
        // guard (both non-empty AND usable, so `guarded` is actually true and the command
        // really is wrapped) - the exact combination none of this file's other
        // build_cache_dir/env tests exercise together, since they each leave one of the two
        // empty.
        let dir = tempfile::tempdir().expect("tempdir");
        let guard_path = dir.path().join("cargo-target.lock");
        let res = ExecRunner.run(
            &gate_cmd("test \"$CARGO_TARGET_DIR\" = /tmp/rigger-flock-env-probe"),
            "",
            "",
            "/tmp/rigger-flock-env-probe",
            guard_path.to_str().unwrap(),
            "",
            &BuildEnv::default(),
            &BuildBudget::default(),
        );
        assert!(
            res.pass,
            "CARGO_TARGET_DIR must reach the gate command even when wrapped through flock: \
             {res:?}"
        );
    }

    #[test]
    fn exec_runner_target_dir_wins_over_build_cache_dir_when_both_are_given() {
        // The two are mutually exclusive by every real caller's own construction
        // (`conductor::run_gates` only ever resolves a non-empty `target` XOR a non-empty
        // shared-cache pair - see `shared_build_cache_paths`), but ExecRunner's own
        // precedence must not silently rely on that external discipline: a non-empty
        // target_dir must always win, so a per-unit build can never be redirected onto the
        // shared cache even if a caller were to pass both.
        let with_both = ExecRunner.run(
            &gate_cmd("test \"$CARGO_TARGET_DIR\" = /tmp/rigger-gap19-probe"),
            "",
            "/tmp/rigger-gap19-probe",
            "/tmp/rigger-shared-cache-probe",
            "",
            "",
            &BuildEnv::default(),
            &BuildBudget::default(),
        );
        assert!(
            with_both.pass,
            "a non-empty target_dir must win over a non-empty build_cache_dir: {with_both:?}"
        );
    }

    /// Poll `pred` until it holds or a generous timeout elapses; returns whether it held.
    /// Mirrors `budget::tests::wait_until` (the SAME flock-adjacent polling idiom, needed
    /// here for the identical reason: a real kernel flock's timing is not something a
    /// fixed sleep can pin reliably).
    fn wait_until(mut pred: impl FnMut() -> bool) -> bool {
        for _ in 0..400 {
            if pred() {
                return true;
            }
            std::thread::sleep(std::time::Duration::from_millis(25));
        }
        false
    }

    #[test]
    fn exec_runner_holds_the_shared_build_cache_guard_lock_for_the_whole_cargo_invocation() {
        // spec 77 criterion 5 (BOUNDED SHARED CACHE): a non-empty `build_cache_guard`
        // names the sibling lock file `rigger reset --build-cache` takes EXCLUSIVE and
        // non-blocking; a gate build must hold it SHARED for its whole invocation, so a
        // reset attempted while this gate's command is still running is refused rather
        // than corrupting the cache out from under it. Proven from the OTHER side: while
        // the gate command is still running, an independent probe's try_lock_exclusive on
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
                &guard_str,
                "",
                &BuildEnv::default(),
                &BuildBudget::default(),
            )
        });
        assert!(
            wait_until(|| started.exists()),
            "the gate command must start and touch its marker"
        );
        {
            use fs2::FileExt;
            let probe = std::fs::OpenOptions::new()
                .create(true)
                .write(true)
                .truncate(false)
                .open(&guard_path)
                .expect("open guard for probe");
            assert!(
                probe.try_lock_exclusive().is_err(),
                "a running shared-cache gate build must hold the guard SHARED, blocking an \
                 exclusive probe"
            );
        }
        let res = handle.join().expect("gate thread must not panic");
        assert!(res.pass, "the gate command must pass: {res:?}");
        {
            use fs2::FileExt;
            let probe = std::fs::OpenOptions::new()
                .create(true)
                .write(true)
                .truncate(false)
                .open(&guard_path)
                .expect("open guard for probe");
            assert!(
                probe.try_lock_exclusive().is_ok(),
                "once the gate build exits its shared hold must be released"
            );
        }
    }

    #[test]
    fn exec_runner_never_touches_the_guard_when_build_cache_guard_is_empty() {
        // Empty means off, mirroring `target_dir`'s own convention: a per-unit build's
        // own `cargo-target-<slug>` cache is never at risk from `reset --build-cache`, so
        // it takes no lock at all and never even creates a guard file.
        let dir = tempfile::tempdir().expect("tempdir");
        let guard_path = dir.path().join("cargo-target.lock");
        let res = ExecRunner.run(
            &gate_cmd("true"),
            "",
            "",
            "",
            "",
            "",
            &BuildEnv::default(),
            &BuildBudget::default(),
        );
        assert!(res.pass);
        assert!(
            !guard_path.exists(),
            "an empty build_cache_guard must never create a guard file: {guard_path:?}"
        );
    }

    #[test]
    fn exec_runner_degrades_to_unguarded_when_the_guard_path_cannot_be_opened() {
        // Best-effort degrade (mirroring `BuildBudget::acquire`'s own convention, spec 77
        // criterion 5 round 2 fix): a `build_cache_guard` whose parent directory does not
        // exist can never be opened, so the gate command must still run (unguarded) rather
        // than fail over guard trouble unrelated to the build itself - and, since this
        // degrades BEFORE ever invoking `flock(1)`, no guard file is created either.
        let dir = tempfile::tempdir().expect("tempdir");
        let guard_path = dir.path().join("missing-parent").join("cargo-target.lock");
        let res = ExecRunner.run(
            &gate_cmd("true"),
            "",
            "",
            "",
            guard_path.to_str().unwrap(),
            "",
            &BuildEnv::default(),
            &BuildBudget::default(),
        );
        assert!(
            res.pass,
            "an unusable guard path must degrade to running unguarded, never fail the gate: \
             {res:?}"
        );
        assert!(
            !guard_path.exists(),
            "a degraded (unguarded) run must never create the guard file: {guard_path:?}"
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
        let with = ExecRunner.run(
            &gate_cmd(&format!(
                "test \"${STORE_FENCE_ENV}\" = /tmp/rigger-gap19-probe-store-fence"
            )),
            "",
            "/tmp/rigger-gap19-probe",
            "",
            "",
            "",
            &BuildEnv::default(),
            &BuildBudget::default(),
        );
        assert!(
            with.pass,
            "a non-empty target_dir must fence the gate's store resolution via {STORE_FENCE_ENV}: {with:?}"
        );

        let without = ExecRunner.run(
            &gate_cmd(&format!("test -z \"${STORE_FENCE_ENV}\"")),
            "",
            "",
            "",
            "",
            "",
            &BuildEnv::default(),
            &BuildBudget::default(),
        );
        assert!(
            without.pass,
            "an empty target_dir (the integrated-tree/deferred gate) must not force a store fence: {without:?}"
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

        let with = ExecRunner.run(
            &gate_cmd(&format!("test \"${STORE_FENCE_ENV}\" = {injected_fence}")),
            review_dir,
            "",
            "",
            "",
            &injected_fence,
            &BuildEnv::default(),
            &BuildBudget::default(),
        );
        assert!(
            with.pass,
            "an injected store_fence must reach the gate via {STORE_FENCE_ENV} even with an \
             empty target_dir: {with:?}"
        );

        let no_cargo_override = ExecRunner.run(
            &gate_cmd(&format!("test \"$CARGO_TARGET_DIR\" != {review_dir}")),
            review_dir,
            "",
            "",
            "",
            &injected_fence,
            &BuildEnv::default(),
            &BuildBudget::default(),
        );
        assert!(
            no_cargo_override.pass,
            "an injected store_fence must never force a CARGO_TARGET_DIR override too: \
             {no_cargo_override:?}"
        );

        let unfenced = ExecRunner.run(
            &gate_cmd(&format!("test -z \"${STORE_FENCE_ENV}\"")),
            "",
            "",
            "",
            "",
            "",
            &BuildEnv::default(),
            &BuildBudget::default(),
        );
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

    #[test]
    fn build_env_resolves_no_vars_when_wrapper_is_off_or_empty() {
        // spec 65: no wrapper configured (empty, the default) or an explicit `off` must
        // resolve to NO vars at all - a build the loop runs then inherits the ambient
        // environment untouched, exactly as before this authority existed. No silent
        // degrade in the other direction either: nothing is injected when nothing was
        // asked for.
        assert!(BuildEnv::resolve("", "", 0).vars().is_empty());
        assert!(BuildEnv::resolve("off", "/some/cache", 0).vars().is_empty());
        // Matched case- and whitespace-insensitively, like the workflow's other on/off
        // scalars (`dash`, `autonomy`).
        assert!(BuildEnv::resolve("  OFF  ", "", 0).vars().is_empty());
    }

    #[test]
    fn build_env_resolves_wrapper_cache_dir_and_incremental_off_when_configured() {
        // With a wrapper configured, ONE resolver derives exactly three vars: the
        // wrapper itself (verbatim - `auto`/absent-on-PATH resolution is spec 65 unit
        // 2's job, layered on top of this foundational shape), that wrapper's own
        // cache-directory var (the generic `<WRAPPER>_DIR` convention - rigger hardcodes
        // no specific tool), and CARGO_INCREMENTAL=0 (incremental output defeats
        // wrapper caching).
        let env = BuildEnv::resolve("sccache", "/shared/build-cache", 0);
        let vars: std::collections::HashMap<_, _> = env.vars().iter().cloned().collect();
        assert_eq!(
            vars.get("RUSTC_WRAPPER").map(String::as_str),
            Some("sccache")
        );
        assert_eq!(
            vars.get("SCCACHE_DIR").map(String::as_str),
            Some("/shared/build-cache")
        );
        assert_eq!(vars.get("CARGO_INCREMENTAL").map(String::as_str), Some("0"));
        assert_eq!(env.vars().len(), 3, "exactly these three vars: {env:?}");
    }

    #[test]
    fn build_env_derives_the_wrapper_specific_cache_dir_var_name() {
        // The <WRAPPER>_DIR convention is generic, not hardcoded to one tool: a
        // differently-named wrapper gets its OWN uppercased var.
        let env = BuildEnv::resolve("ccache", "/x", 0);
        let vars: std::collections::HashMap<_, _> = env.vars().iter().cloned().collect();
        assert_eq!(vars.get("CCACHE_DIR").map(String::as_str), Some("/x"));
        assert!(!vars.contains_key("SCCACHE_DIR"));
    }

    #[test]
    fn build_env_defaults_the_cache_dir_when_unset() {
        // An empty cache_dir with a configured wrapper still resolves to a real,
        // non-empty shared location (`<state home>/rigger/build-cache`) - never an
        // empty/unset cache var, which would leave the wrapper's OWN scattered
        // per-invocation default in play instead of one shared machine-wide cache.
        let env = BuildEnv::resolve("sccache", "", 0);
        let vars: std::collections::HashMap<_, _> = env.vars().iter().cloned().collect();
        let dir = vars.get("SCCACHE_DIR").expect("a default cache dir var");
        assert!(!dir.is_empty());
        assert!(
            dir.ends_with(&format!("rigger{}build-cache", std::path::MAIN_SEPARATOR)),
            "default cache dir must be <state home>/rigger/build-cache, got {dir:?}"
        );
    }

    #[test]
    fn exec_runner_applies_the_build_env_it_is_given() {
        // The ONE build-environment authority's first injection site: a gate build
        // carries whatever BuildEnv it is handed (spec 65). With a wrapper configured,
        // the gate command sees RUSTC_WRAPPER/<WRAPPER>_DIR/CARGO_INCREMENTAL=0; with
        // the default (no wrapper), it sees none of them - the ambient env is
        // untouched, exactly like the existing empty-target_dir behavior.
        let env = BuildEnv::resolve("sccache", "/shared/build-cache", 0);
        let with = ExecRunner.run(
            &gate_cmd(
                "test \"$RUSTC_WRAPPER\" = sccache && test \"$SCCACHE_DIR\" = /shared/build-cache \
                 && test \"$CARGO_INCREMENTAL\" = 0",
            ),
            "",
            "",
            "",
            "",
            "",
            &env,
            &BuildBudget::default(),
        );
        assert!(
            with.pass,
            "a configured BuildEnv must reach the gate: {with:?}"
        );

        let without = ExecRunner.run(
            &gate_cmd("test -z \"$RUSTC_WRAPPER\" && test -z \"$SCCACHE_DIR\""),
            "",
            "",
            "",
            "",
            "",
            &BuildEnv::default(),
            &BuildBudget::default(),
        );
        assert!(
            without.pass,
            "the default (empty) BuildEnv must not force any wrapper var: {without:?}"
        );
    }

    #[test]
    fn build_env_apply_sets_every_resolved_var_on_a_command() {
        let env = BuildEnv::resolve("sccache", "/shared/build-cache", 0);
        let mut cmd = Command::new("sh");
        env.apply(&mut cmd);
        cmd.arg("-c").arg(
            "test \"$RUSTC_WRAPPER\" = sccache && test \"$SCCACHE_DIR\" = /shared/build-cache \
             && test \"$CARGO_INCREMENTAL\" = 0",
        );
        let status = cmd.status().unwrap();
        assert!(status.success(), "apply must set every resolved var");
    }

    #[test]
    fn build_env_jobs_cap_reaches_the_build_when_set() {
        // spec 65, JOBS CAP (unit 4): a configured `build.jobs` must resolve to
        // CARGO_BUILD_JOBS, threaded through the SAME BuildEnv the wrapper vars ride -
        // no second, competing env-derivation path.
        let env = BuildEnv::resolve("", "", 4);
        let vars: std::collections::HashMap<_, _> = env.vars().iter().cloned().collect();
        assert_eq!(vars.get("CARGO_BUILD_JOBS").map(String::as_str), Some("4"));
    }

    #[test]
    fn build_env_jobs_cap_is_independent_of_the_wrapper() {
        // The jobs cap is its own facet of the build environment: it must reach the
        // build whether or not a compilation-cache wrapper is configured, and a
        // configured wrapper must not suppress it or vice versa.
        let env = BuildEnv::resolve("sccache", "/shared/build-cache", 8);
        let vars: std::collections::HashMap<_, _> = env.vars().iter().cloned().collect();
        assert_eq!(vars.get("CARGO_BUILD_JOBS").map(String::as_str), Some("8"));
        assert_eq!(
            vars.get("RUSTC_WRAPPER").map(String::as_str),
            Some("sccache")
        );
        assert_eq!(env.vars().len(), 4, "wrapper's 3 vars plus jobs: {env:?}");
    }

    #[test]
    fn build_env_jobs_cap_unset_leaves_the_ambient_default_untouched() {
        // 0 (the config default, matching the budget/max_retries/speculation_width
        // zero-as-unset convention) means unset - CARGO_BUILD_JOBS must NOT be
        // injected, so an un-set workflow leaves cargo's own ambient default jobs
        // count untouched, exactly as before this criterion existed.
        assert!(!BuildEnv::resolve("", "", 0)
            .vars()
            .iter()
            .any(|(k, _)| k == "CARGO_BUILD_JOBS"));
        assert!(!BuildEnv::resolve("sccache", "/x", 0)
            .vars()
            .iter()
            .any(|(k, _)| k == "CARGO_BUILD_JOBS"));
    }

    #[test]
    fn exec_runner_applies_the_jobs_cap_it_is_given() {
        // The jobs cap reaches a real gate subprocess through the SAME injection site
        // (ExecRunner::run) the wrapper vars already use - no second call needed.
        let env = BuildEnv::resolve("", "", 3);
        let res = ExecRunner.run(
            &gate_cmd("test \"$CARGO_BUILD_JOBS\" = 3"),
            "",
            "",
            "",
            "",
            "",
            &env,
            &BuildBudget::default(),
        );
        assert!(
            res.pass,
            "a configured jobs cap must reach the gate: {res:?}"
        );
    }

    // --- resolve_wrapper_name (spec 65 unit 2, NO SILENT DEGRADE) -----------------------
    //
    // All of these drive the pure, injectable core (`resolve_wrapper_name_from`) against a
    // synthetic PATH built from temp dirs, never the real ambient environment - so none of
    // them touch (or race on) the process-global `PATH` var. `resolve_wrapper_name` itself
    // (the ambient-reading edge) is exercised at the CLI level in tests/cli.rs, where a
    // synthetic PATH is safely scoped to a child process instead.

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

    fn path_var(dirs: &[&std::path::Path]) -> std::ffi::OsString {
        std::env::join_paths(dirs.iter().copied()).expect("join synthetic PATH")
    }

    #[test]
    fn resolve_wrapper_name_off_and_empty_resolve_to_none_without_touching_path() {
        // An empty PATH proves these branches never even reach the probe: they must
        // resolve `None` regardless of what (or how little) PATH contains.
        let empty_path = path_var(&[]);
        assert_eq!(resolve_wrapper_name_from("", &empty_path), Ok(None));
        assert_eq!(resolve_wrapper_name_from("off", &empty_path), Ok(None));
        // Matched case- and whitespace-insensitively, like BuildEnv::resolve's own off.
        assert_eq!(resolve_wrapper_name_from("  OFF  ", &empty_path), Ok(None));
    }

    #[test]
    fn resolve_wrapper_name_auto_probes_known_wrappers_and_finds_one_present() {
        let dir = tempfile::tempdir().expect("tempdir");
        write_executable(dir.path(), "ccache");
        let path = path_var(&[dir.path()]);
        assert_eq!(
            resolve_wrapper_name_from("auto", &path),
            Ok(Some("ccache".to_string())),
            "auto must find the known wrapper present on PATH"
        );
    }

    #[test]
    fn resolve_wrapper_name_auto_with_nothing_on_path_resolves_to_none() {
        let dir = tempfile::tempdir().expect("tempdir");
        // A directory that exists but holds no known wrapper binary at all.
        let path = path_var(&[dir.path()]);
        assert_eq!(
            resolve_wrapper_name_from("auto", &path),
            Ok(None),
            "auto finding nothing must DEGRADE (inject nothing), never error"
        );
    }

    #[test]
    fn resolve_wrapper_name_named_wrapper_present_on_path_resolves_to_itself() {
        let dir = tempfile::tempdir().expect("tempdir");
        write_executable(dir.path(), "my-custom-wrapper");
        let path = path_var(&[dir.path()]);
        assert_eq!(
            resolve_wrapper_name_from("my-custom-wrapper", &path),
            Ok(Some("my-custom-wrapper".to_string())),
            "a NAMED wrapper is not restricted to the known-wrapper list - any binary name \
             on PATH resolves"
        );
    }

    #[test]
    fn resolve_wrapper_name_named_wrapper_absent_from_path_errors_naming_the_binary() {
        let empty_path = path_var(&[]);
        let err = resolve_wrapper_name_from("ghost-wrapper-xyz", &empty_path)
            .expect_err("a configured-explicit wrapper absent from PATH must error, not degrade");
        let msg = err.to_string();
        assert!(
            msg.contains("ghost-wrapper-xyz"),
            "the error must name the missing binary: {msg:?}"
        );
        assert!(
            msg.contains("build.wrapper"),
            "the error must name the config key: {msg:?}"
        );
    }

    #[test]
    fn resolve_wrapper_name_ignores_a_same_named_non_executable_file_on_path() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(dir.path().join("sccache"), "not a binary").expect("write plain file");
        let path = path_var(&[dir.path()]);
        // Present as a FILE but not executable: must not count as "found" in either the
        // auto-probe or the named-wrapper check - a stray non-executable file of the same
        // name must never masquerade as a real wrapper.
        assert_eq!(resolve_wrapper_name_from("auto", &path), Ok(None));
        assert!(resolve_wrapper_name_from("sccache", &path).is_err());
    }

    // --- resolve_mutation_layer (spec 73, ENABLED-BUT-ABSENT FAILS LOUD) ----------------
    //
    // Same injectable-PATH shape as `resolve_wrapper_name_from`'s own tests above: the pure
    // core takes PATH as a value so these never touch (or race on) the process-global PATH.
    // Unlike `build.wrapper`, `build.mutation` is a strict two-value gate (`on`/`off`), not a
    // three-way with an `auto` discovery state - so there is no discovered-implicit degrade
    // direction here, only the configured-explicit one.

    #[test]
    fn resolve_mutation_layer_off_and_empty_resolve_to_false_without_touching_path() {
        // An empty PATH proves these branches never even reach the probe: they must
        // resolve `Ok(false)` regardless of what (or how little) PATH contains.
        let empty_path = path_var(&[]);
        assert_eq!(resolve_mutation_layer_from("", &empty_path), Ok(false));
        assert_eq!(resolve_mutation_layer_from("off", &empty_path), Ok(false));
        // Matched case- and whitespace-insensitively, like the wrapper's own off/auto.
        assert_eq!(
            resolve_mutation_layer_from("  OFF  ", &empty_path),
            Ok(false)
        );
    }

    #[test]
    fn resolve_mutation_layer_off_ignores_a_path_that_actually_has_the_binary() {
        // `off` must never activate even when cargo-mutants IS reachable - the config key
        // is the sole authority, not PATH content.
        let dir = tempfile::tempdir().expect("tempdir");
        write_executable(dir.path(), "cargo-mutants");
        let path = path_var(&[dir.path()]);
        assert_eq!(resolve_mutation_layer_from("off", &path), Ok(false));
        assert_eq!(resolve_mutation_layer_from("", &path), Ok(false));
    }

    #[test]
    fn resolve_mutation_layer_on_with_the_binary_present_resolves_true() {
        let dir = tempfile::tempdir().expect("tempdir");
        write_executable(dir.path(), "cargo-mutants");
        let path = path_var(&[dir.path()]);
        assert_eq!(resolve_mutation_layer_from("on", &path), Ok(true));
        // Matched case- and whitespace-insensitively, like the wrapper's own off/auto.
        assert_eq!(resolve_mutation_layer_from("  ON  ", &path), Ok(true));
    }

    #[test]
    fn resolve_mutation_layer_on_with_the_binary_absent_errors_naming_the_binary_and_key() {
        let empty_path = path_var(&[]);
        let err = resolve_mutation_layer_from("on", &empty_path).expect_err(
            "a configured-explicit build.mutation: on with no cargo-mutants on PATH must \
             error, not degrade",
        );
        let msg = err.to_string();
        assert!(
            msg.contains("cargo-mutants"),
            "the error must name the missing binary: {msg:?}"
        );
        assert!(
            msg.contains("build.mutation"),
            "the error must name the config key: {msg:?}"
        );
    }

    #[test]
    fn resolve_mutation_layer_ignores_a_same_named_non_executable_file_on_path() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(dir.path().join("cargo-mutants"), "not a binary").expect("write plain file");
        let path = path_var(&[dir.path()]);
        // Present as a FILE but not executable: must not count as "found" - a stray
        // non-executable file of the same name must never masquerade as the real tool.
        assert!(resolve_mutation_layer_from("on", &path).is_err());
    }

    // --- resolve_build_layer (spec 65 unit 2, NO SILENT DEGRADE, cache-dir axis) --------
    //
    // The SAME Design sentence (specs/65:26-28) that decides a named-but-absent wrapper
    // BINARY errors loudly also decides an uncreatable cache DIR errors loudly for a named
    // wrapper, and silently degrades (skips the whole layer) under `auto`. These drive the
    // real filesystem (never a mock): a regular FILE placed where a directory component
    // must go makes `create_dir_all` fail deterministically, on any machine, without
    // needing root or permission tricks that a CI/root user could bypass.

    /// A cache-dir path guaranteed to be uncreatable: `<tmpdir>/blocker` is a plain FILE, so
    /// `create_dir_all("<tmpdir>/blocker/nested/cache")` fails because a path COMPONENT
    /// already exists as a non-directory - deterministic on every OS/user, unlike a
    /// permission-bit trick a root-run test would silently bypass.
    fn uncreatable_dir(root: &std::path::Path) -> String {
        let blocker = root.join("blocker");
        std::fs::write(&blocker, "not a directory").expect("write blocker file");
        blocker
            .join("nested")
            .join("cache")
            .to_string_lossy()
            .into_owned()
    }

    #[test]
    fn resolve_build_layer_off_and_empty_never_touch_the_cache_dir() {
        let empty_path = path_var(&[]);
        let tmp = tempfile::tempdir().expect("tempdir");
        let blocked = uncreatable_dir(tmp.path());
        // An uncreatable cache_dir would error/degrade if ever probed; off/empty must
        // resolve None WITHOUT reaching the cache-dir axis at all.
        assert_eq!(
            resolve_build_layer_from("off", &blocked, &empty_path),
            Ok(None)
        );
        assert_eq!(
            resolve_build_layer_from("", &blocked, &empty_path),
            Ok(None)
        );
    }

    #[test]
    fn resolve_build_layer_named_wrapper_with_a_creatable_dir_resolves_and_creates_it() {
        let dir = tempfile::tempdir().expect("tempdir");
        write_executable(dir.path(), "my-custom-wrapper");
        let path = path_var(&[dir.path()]);
        let cache_dir = dir.path().join("cache-goes-here");
        assert!(!cache_dir.exists(), "precondition: not created yet");
        assert_eq!(
            resolve_build_layer_from("my-custom-wrapper", &cache_dir.to_string_lossy(), &path),
            Ok(Some("my-custom-wrapper".to_string()))
        );
        assert!(
            cache_dir.is_dir(),
            "a resolved layer must actually create its cache dir"
        );
    }

    #[test]
    fn resolve_build_layer_named_wrapper_with_an_uncreatable_dir_errors_naming_dir_and_key() {
        let dir = tempfile::tempdir().expect("tempdir");
        write_executable(dir.path(), "my-custom-wrapper");
        let path = path_var(&[dir.path()]);
        let blocked = uncreatable_dir(dir.path());
        let err = resolve_build_layer_from("my-custom-wrapper", &blocked, &path)
            .expect_err("a NAMED wrapper's uncreatable cache dir must error, not degrade");
        let msg = err.to_string();
        assert!(
            msg.contains(&blocked),
            "the error must name the cache dir: {msg:?}"
        );
        assert!(
            msg.contains("build.cache_dir"),
            "the error must name the config key: {msg:?}"
        );
    }

    #[test]
    fn resolve_build_layer_auto_with_an_uncreatable_dir_skips_the_whole_layer() {
        let dir = tempfile::tempdir().expect("tempdir");
        // A KNOWN wrapper IS present on PATH, so the wrapper-binary axis alone would
        // resolve `Some` - but its cache dir cannot be created, so under `auto` the whole
        // layer must degrade to `None` (mirroring auto finding no wrapper at all), never
        // error.
        write_executable(dir.path(), "ccache");
        let path = path_var(&[dir.path()]);
        let blocked = uncreatable_dir(dir.path());
        assert_eq!(
            resolve_build_layer_from("auto", &blocked, &path),
            Ok(None),
            "auto must silently skip the whole layer when the cache dir is unusable"
        );
    }

    #[test]
    fn resolve_build_layer_auto_with_nothing_on_path_never_touches_the_cache_dir() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = path_var(&[dir.path()]);
        let blocked = uncreatable_dir(dir.path());
        // Nothing on PATH: the wrapper-binary axis alone already resolves None, so the
        // (also-broken) cache dir must never even be probed - a caller that inspects
        // filesystem state after this call sees nothing created and no directory-creation
        // side effect from a layer that was never going to be active.
        assert_eq!(resolve_build_layer_from("auto", &blocked, &path), Ok(None));
    }

    #[test]
    fn resolve_build_layer_named_wrapper_absent_from_path_still_errors_before_the_cache_dir() {
        let empty_path = path_var(&[]);
        // Both axes are broken (absent binary AND an uncreatable dir); the wrapper-binary
        // axis must win (its error is the one surfaced), matching resolve_wrapper_name's
        // own established precedence.
        let tmp = tempfile::tempdir().expect("tempdir");
        let blocked = uncreatable_dir(tmp.path());
        let err = resolve_build_layer_from("ghost-wrapper-xyz", &blocked, &empty_path)
            .expect_err("an absent named wrapper must still error");
        assert!(matches!(err, BuildLayerUnavailable::Wrapper(_)));
    }

    // --- pre-existing-but-unwritable cache dir (spec 65 unit 2 round 2) ----------------
    //
    // The realistic steady state: the shared default_cache_dir every project reuses after
    // first creation ALREADY EXISTS, so `create_dir_all` alone is a no-op success regardless
    // of write permission - it is not proof the cache is actually live. These chmod an
    // ALREADY-CREATED directory read+execute-only (0o555) AFTER creation - unlike
    // `uncreatable_dir` above (a blocked path component), this is the only way to make a
    // directory that exists yet cannot be written into.

    /// A cache-dir path that exists but cannot be WRITTEN into: created first, then chmod'd
    /// read+execute-only (0o555) - `create_dir_all` against it succeeds (no-op, already
    /// exists), but writing a file inside it fails with `PermissionDenied`.
    #[cfg(unix)]
    fn preexisting_unwritable_dir(root: &std::path::Path) -> String {
        use std::os::unix::fs::PermissionsExt;
        let dir = root.join("preexisting-cache");
        std::fs::create_dir_all(&dir).expect("pre-create cache dir");
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o555))
            .expect("chmod cache dir read+execute-only");
        dir.to_string_lossy().into_owned()
    }

    #[cfg(unix)]
    #[test]
    fn resolve_build_layer_named_wrapper_with_a_preexisting_unwritable_dir_errors_naming_dir_and_key(
    ) {
        let dir = tempfile::tempdir().expect("tempdir");
        write_executable(dir.path(), "my-custom-wrapper");
        let path = path_var(&[dir.path()]);
        let unwritable = preexisting_unwritable_dir(dir.path());
        let err = resolve_build_layer_from("my-custom-wrapper", &unwritable, &path).expect_err(
            "a NAMED wrapper's pre-existing-but-unwritable cache dir must error, not silently \
             report the layer usable",
        );
        let msg = err.to_string();
        assert!(
            msg.contains(&unwritable),
            "the error must name the cache dir: {msg:?}"
        );
        assert!(
            msg.contains("build.cache_dir"),
            "the error must name the config key: {msg:?}"
        );
    }

    #[cfg(unix)]
    #[test]
    fn resolve_build_layer_auto_with_a_preexisting_unwritable_dir_skips_the_whole_layer() {
        let dir = tempfile::tempdir().expect("tempdir");
        // A KNOWN wrapper IS present on PATH, so the wrapper-binary axis alone would
        // resolve `Some` - but its cache dir, though it already EXISTS, cannot be written
        // into, so under `auto` the whole layer must degrade to `None`, never error and
        // never report the layer live.
        write_executable(dir.path(), "ccache");
        let path = path_var(&[dir.path()]);
        let unwritable = preexisting_unwritable_dir(dir.path());
        assert_eq!(
            resolve_build_layer_from("auto", &unwritable, &path),
            Ok(None),
            "auto must silently skip the whole layer when the (pre-existing) cache dir is \
             not writable"
        );
    }
}
