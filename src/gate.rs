//! Rigger's verification discipline: a gate is a command plus a trust level, it
//! yields compact evidence (never a raw log), and its autonomy moves on a
//! bidirectional ratchet so a graduated gate can never silently auto-pass bad
//! work. `Runner` is the port; `ExecRunner` is the adapter.

use std::process::Command;

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
/// optional `CARGO_TARGET_DIR` override (`target_dir`, "" = inherit the ambient env).
///
/// A gate that runs INSIDE a unit's worktree is handed a unit-keyed `target_dir` (Gap 19)
/// so divergent unit trees never share one build cache - a compile error a gate sees is
/// then always that unit's own, never a concurrent neighbour poisoning a shared target. A
/// gate on the single integrated tree (the deferred phase-boundary gate, and the courier's
/// inline `rigger step` gates) is handed "" and keeps inheriting the shared cache.
pub trait Runner: Send + Sync {
    fn run(&self, g: &Gate, dir: &str, target_dir: &str) -> GateResult;
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

/// ExecRunner runs a gate as a shell command, reducing output to compact evidence.
pub struct ExecRunner;

impl Runner for ExecRunner {
    fn run(&self, g: &Gate, dir: &str, target_dir: &str) -> GateResult {
        let mut cmd = Command::new("sh");
        cmd.arg("-c").arg(&g.run);
        if !dir.is_empty() {
            cmd.current_dir(dir);
        }
        // Per-unit build cache (Gap 19): a non-empty target_dir points cargo at a
        // unit-keyed CARGO_TARGET_DIR so this gate's incremental state is never shared
        // with a concurrent unit's divergent tree. Empty leaves the ambient env
        // untouched (the integrated-tree/deferred gate keeps the shared cache).
        if !target_dir.is_empty() {
            cmd.env("CARGO_TARGET_DIR", target_dir);
        }
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

/// Cap on the length of any single evidence line (§3.3).
const LINE_CAP: usize = 200;
/// Cap on the number of signal lines carried in the evidence (§3.3).
const MAX_LINES: usize = 5;

/// Reduce a gate's raw output to a compact summary (§3.3): the verdict
/// (`PASS`/`FAIL`) followed by up to five lines that signal failure - lines
/// containing `error`, `fail`, `panic`, or `assert` (case-insensitive), or the
/// last five non-empty lines if none match. Each line is length-capped; the raw
/// log is never carried.
fn compact(pass: bool, s: &str) -> String {
    let verdict = if pass { "PASS" } else { "FAIL" };
    let lines: Vec<&str> = s.lines().map(str::trim).filter(|l| !l.is_empty()).collect();

    let signal: Vec<&str> = lines
        .iter()
        .filter(|l| is_signal(l))
        .take(MAX_LINES)
        .copied()
        .collect();
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

/// Whether a line signals a failure (matched case-insensitively).
fn is_signal(line: &str) -> bool {
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
        assert!(ExecRunner.run(&gate_cmd("true"), "", "").pass);
        assert!(!ExecRunner.run(&gate_cmd("false"), "", "").pass);
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
        let res = ExecRunner.run(&gate_cmd(&cmd), "", "");
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
        );
        assert!(
            with.pass,
            "a non-empty target_dir must reach the gate as CARGO_TARGET_DIR: {with:?}"
        );

        let without = ExecRunner.run(
            &gate_cmd("test \"$CARGO_TARGET_DIR\" != /tmp/rigger-gap19-probe"),
            "",
            "",
        );
        assert!(
            without.pass,
            "an empty target_dir must not force a CARGO_TARGET_DIR override: {without:?}"
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
}
