//! Conductor-port fixtures.

use std::path::Path;

use rigger::conductor::{AgentDriver, AgentResult, Deps, Error, SpawnOpts};
use rigger::config::AgentDef;
use rigger::eventstore::EventStore;
use rigger::gate;

/// A no-op emit sink for an `AgentDriver::spawn` whose emits the test does not observe.
pub fn no_emit(_: &str, _: serde_json::Value) -> Result<(), Error> {
    Ok(())
}

/// A reviewer double: the adjudicator approves, every other reviewer returns a plain note.
pub fn review_or_adjudicate(opts: &SpawnOpts) -> AgentResult {
    if opts.id.contains("/adjudicator#") {
        return AgentResult {
            output: r#"{"verdict":"approve"}"#.into(),
            resolved_model: String::new(),
        };
    }
    AgentResult {
        output: "reviewed the diff".into(),
        resolved_model: String::new(),
    }
}

/// The conductor's ports for one run over `repo`: no grounder, no graph and no criteria.
pub fn bare_deps<'a>(
    store: &'a dyn EventStore,
    driver: &'a dyn AgentDriver,
    gates: &'a dyn gate::Runner,
    repo: &str,
) -> Deps<'a> {
    Deps {
        store,
        driver,
        gates,
        repo: repo.to_string(),
        grounder: None,
        graph: None,
        criteria: Vec::new(),
    }
}

/// The base `m.rs` content the merge-break fixture starts from: six MARK-free lines so
/// `unit-a`'s top insert and `unit-b`'s bottom append land in NON-overlapping hunks that git
/// auto-merges cleanly (no conflict) into a tree carrying BOTH marks.
pub const MERGE_BREAK_BASE: &str = "l1\nl2\nl3\nl4\nl5\nl6\n";

/// A driver for the post-merge re-gate fixture (spec 12, unit 5): `unit-a` and `unit-b` are two
/// batch-mates with NO dependency between them, each editing the SAME file `m.rs`. `unit-a`
/// PREPENDS one `MARK` line, `unit-b` APPENDS one - so each unit's OWN tree carries exactly one
/// MARK (its gate passes in isolation), but the textual auto-merge combines both into a two-MARK
/// tree the gate REJECTS. Both writes are recomputed from the fixed base each attempt
/// (idempotent), so a remediation re-attempt never doubles a unit's own mark. A barrier makes
/// both worktrees branch from the SAME base commit (neither integrates before the other's branch
/// exists), which is what creates the unpredicted overlap. Reviewers answer through
/// [`review_or_adjudicate`].
pub struct MergeBreakDriver {
    pub repo: String,
}

impl AgentDriver for MergeBreakDriver {
    fn spawn(
        &self,
        _agent: &AgentDef,
        _prompt: &str,
        opts: &SpawnOpts,
        _emit: &dyn Fn(&str, serde_json::Value) -> Result<(), Error>,
    ) -> Result<AgentResult, Error> {
        let unit = opts.id.split('/').next().unwrap_or_default();
        if !opts.id.contains("/implementer#") {
            return Ok(review_or_adjudicate(opts));
        }
        if !opts.dir.is_empty() {
            // Barrier: block until BOTH unit branches exist, so both worktrees were cut from the
            // SAME base commit (neither has integrated yet - an implementer runs strictly before
            // its unit's integrate).
            for _ in 0..400 {
                let n = std::process::Command::new("git")
                    .arg("-C")
                    .arg(&self.repo)
                    .args(["branch", "--list", "rigger/u/*"])
                    .output()
                    .map(|o| String::from_utf8_lossy(&o.stdout).lines().count())
                    .unwrap_or(0);
                if n >= 2 {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(25));
            }
            // Idempotent per attempt: always the fixed base plus this unit's ONE mark.
            let content = if unit == "unit-a" {
                format!("MARK\n{MERGE_BREAK_BASE}")
            } else {
                format!("{MERGE_BREAK_BASE}MARK\n")
            };
            std::fs::write(Path::new(&opts.dir).join("m.rs"), content).unwrap();
        }
        Ok(AgentResult::default())
    }
}
