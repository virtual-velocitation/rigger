//! The gate model: a gate is a command plus a trust level, and its autonomy moves on a
//! bidirectional ratchet so a graduated gate can never silently auto-pass bad work. Also the
//! gate vocabulary the adapters share: names both the gate runner and the worktree reclaimer
//! resolve through, so the two can never spell them differently. The runner itself lives in
//! the `rigger-gates-shell` adapter.

/// The scratch-dir naming suffix [`ExecRunner::run`] appends to `target_dir` to derive the
/// store fence's own sibling scratch dir (`{target_dir}{STORE_FENCE_SUFFIX}`). Shared with
/// `worktree::reclaim_cache_sibling` (the ONE reclaim authority for a unit's
/// `cargo-target-<slug>` cache sibling) so it reclaims the fence's sibling by the exact
/// same name it was created under, rather than a second, independently-spelled copy that
/// could drift from this one.
pub const STORE_FENCE_SUFFIX: &str = "-store-fence";

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

/// A config keyword enum: each spelled value maps to one variant, and anything else (an
/// empty / unset value included) falls back to [`Keyword::DEFAULT`]. [`parse`] reads one.
pub trait Keyword: Copy + 'static {
    /// The spelled keywords and the variant each names.
    const KEYWORDS: &'static [(&'static str, Self)];
    /// The variant every other spelling parses to.
    const DEFAULT: Self;
}

/// Parse a config keyword: the variant `s` names in [`Keyword::KEYWORDS`], else
/// [`Keyword::DEFAULT`].
pub fn parse<T: Keyword>(s: &str) -> T {
    T::KEYWORDS
        .iter()
        .find(|(word, _)| *word == s)
        .map_or(T::DEFAULT, |(_, v)| *v)
}

/// A gate kind parses from `elevated` / `deferred`; anything else is `Core`.
impl Keyword for Kind {
    const KEYWORDS: &'static [(&'static str, Self)] =
        &[("elevated", Kind::Elevated), ("deferred", Kind::Deferred)];
    const DEFAULT: Self = Kind::Core;
}

impl Kind {
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

/// An autonomy string parses from `manual` / `silent`. An empty / unset value defaults to
/// `AutoNotify` (§4.3): an unconfigured gate still runs and integrates unattended; only an
/// explicit `manual` pauses a unit for human review. `manual` is therefore opt-in, never the
/// silent default.
impl Keyword for Autonomy {
    const KEYWORDS: &'static [(&'static str, Self)] =
        &[("manual", Autonomy::Manual), ("silent", Autonomy::Silent)];
    const DEFAULT: Self = Autonomy::AutoNotify;
}

impl Autonomy {
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
