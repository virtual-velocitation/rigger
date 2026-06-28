//! Rigger's verification discipline: a gate is a command plus a trust level, it
//! yields compact evidence (never a raw log), and its autonomy moves on a
//! bidirectional ratchet so a graduated gate can never silently auto-pass bad
//! work. `Runner` is the port; `ExecRunner` is the adapter.

use std::process::Command;

/// Kind classifies a gate's authority lifecycle.
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

/// Runner runs a gate command in a working directory ("" = current dir).
pub trait Runner: Send + Sync {
    fn run(&self, g: &Gate, dir: &str) -> GateResult;
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
/// PROMOTE_THRESHOLD runs all passed, and it is not already Silent.
pub fn propose_promotion(g: &Gate) -> bool {
    if g.autonomy == Autonomy::Silent || g.history.len() < PROMOTE_THRESHOLD {
        return false;
    }
    g.history[g.history.len() - PROMOTE_THRESHOLD..]
        .iter()
        .all(|h| h.pass)
}

/// NextAutonomy returns the autonomy one notch up the ratchet, capping at Silent.
pub fn next_autonomy(a: Autonomy) -> Autonomy {
    match a {
        Autonomy::Manual => Autonomy::AutoNotify,
        _ => Autonomy::Silent,
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
    fn run(&self, g: &Gate, dir: &str) -> GateResult {
        let mut cmd = Command::new("sh");
        cmd.arg("-c").arg(&g.run);
        if !dir.is_empty() {
            cmd.current_dir(dir);
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
        Gate {
            id: "g".into(),
            run: String::new(),
            kind: Kind::Core,
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
        assert!(ExecRunner.run(&gate_cmd("true"), "").pass);
        assert!(!ExecRunner.run(&gate_cmd("false"), "").pass);
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
        let res = ExecRunner.run(&gate_cmd(&cmd), "");
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
