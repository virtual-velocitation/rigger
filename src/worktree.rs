//! Isolate a unit of work in a throwaway git worktree branched from HEAD, so
//! parallel units cannot conflict on the filesystem while the event stream stays
//! the shared decision channel. Integrate commits the agent's changes and merges
//! the branch into the base; the work lands.

use std::process::Command;

#[derive(Debug, thiserror::Error)]
#[error("worktree: {0}")]
pub struct Error(pub String);

/// An isolated git worktree for one unit of work.
pub struct Worktree {
    pub dir: String,
    pub branch: String,
    repo: String,
}

/// What [`Worktree::ensure_run_branch`] did, so the caller can tell the operator when
/// the run branch was anchored somewhere OTHER than the base they asked for (a silent
/// divergence otherwise).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunBranchSetup {
    /// The run branch already existed; it was reused (checked out if it was not the
    /// current branch) and NEVER reset, so the units prior steps integrated onto it are
    /// preserved. `base` was NOT consulted - once the run branch exists, its own history
    /// is the run's anchor, and re-anchoring it would discard integrated work.
    Reused,
    /// The run branch did not exist and was created anchored on the requested base ref,
    /// then checked out.
    CreatedFromBase,
    /// The run branch did not exist AND the requested base did not resolve, so it was
    /// created off the current HEAD instead, then checked out. Isolation is still
    /// established (units branch off the run branch, not the operator's branch), but the
    /// anchor is HEAD, not the base the caller asked for.
    CreatedFromHead,
}

/// The outcome of merging a unit's branch into the run branch ([`Worktree::integrate`]).
pub enum IntegrateOutcome {
    /// The branch merged cleanly; carries the integrated commit sha (empty for a read-only
    /// stage that merged nothing).
    Merged(String),
    /// The merge CONFLICTED and was ABORTED, so the run branch is left EXACTLY as it was.
    /// Carries the conflict detail for the unit's remediation feedback. The conductor treats
    /// this like a failed gate - the unit re-enters remediation off the current integrated
    /// tree so its next merge rebases cleanly - instead of the run wedging on a broken branch.
    /// This is the textual-conflict sibling of spec-12-unit-5's semantic-break rollback: that
    /// one re-gates a SUCCESSFUL merge; this one never gets a clean merge to gate.
    Conflict(String),
}

impl IntegrateOutcome {
    /// The merged commit sha; panics on a conflict. A convenience for a caller that has
    /// already established a clean merge is expected (the happy-path tests).
    pub fn expect_merged(self) -> String {
        match self {
            IntegrateOutcome::Merged(sha) => sha,
            IntegrateOutcome::Conflict(detail) => {
                panic!("expected a clean merge, got a conflict: {detail}")
            }
        }
    }
}

impl Worktree {
    /// Add a worktree at dir (which must not already exist), on `branch`.
    ///
    /// The branch is a unit's DURABLE checkpoint (resume-continuity): it survives
    /// process death and worktree removal, so the same deterministic branch name is
    /// reused across runs and the unit's committed work persists. This handles BOTH
    /// cases:
    /// - the branch does NOT exist yet: create it off the repo's current HEAD (a
    ///   fresh unit, the historical behavior);
    /// - the branch ALREADY exists with prior commits: check it out into the fresh
    ///   `dir`, REUSING the work a prior window committed - never throwing it away.
    ///
    /// The worktree DIR is transient (it can live in a temp dir and be recreated);
    /// the BRANCH is the checkpoint. A branch that already exists cannot be
    /// `worktree add -b`'d (git refuses to clobber a ref), so we detect it and check
    /// it out instead.
    pub fn create(repo: &str, dir: &str, branch: &str) -> Result<Self, Error> {
        if branch_exists(repo, branch) {
            // FAST PATH - adoption by PATH LOOKUP (Gap 12, spec 06). The dir is now
            // DETERMINISTIC (derived from the unit id / stage+attempt, no per-process
            // uuid), so a resume - or a step that SUPERSEDES a prior one that died -
            // derives the SAME `dir` for this branch. If that dir already IS this branch's
            // worktree, adopt it directly - a check on the dir's own HEAD, with no
            // `git worktree list` porcelain parse and no re-`add` (which git refuses for a
            // branch already checked out). (This handles sequential resume/supersede, not
            // a true create-race: two processes that both see the branch absent still race
            // the underlying `git worktree add -b`; rigger drives unit-worktree creation
            // single-threaded within one `rigger step`, so that is not a first-class case.)
            if worktree_on_branch(dir, branch) {
                return Ok(Worktree {
                    dir: dir.to_string(),
                    branch: branch.to_string(),
                    repo: repo.to_string(),
                });
            }
            // FALLBACK - adopt-or-prune, for a dir DELETED out from under git (the branch
            // is still checked out in a PRIOR process's registration - a killed or
            // superseded `rigger step` - whose working dir may be at a different/old path
            // or gone entirely). ADOPT the surviving registration when its dir survives,
            // and prune-then-recreate when it does not; never fail on it.
            if let Some(existing) = registered_worktree_for(repo, branch) {
                if std::path::Path::new(&existing).is_dir() {
                    return Ok(Worktree {
                        dir: existing,
                        branch: branch.to_string(),
                        repo: repo.to_string(),
                    });
                }
                git(repo, &["worktree", "prune"])?;
            }
            // DEFEND THE DETERMINISTIC DIR before re-adding. Because the path no longer
            // carries a per-process uuid, a SIGKILL mid `git worktree add` (dir populated,
            // registration not finalized) - or any crash that leaves a populated dir at
            // this fixed path that is NOT a registered worktree on the branch - would make
            // the `add` below hard-fail (`fatal: <dir> already exists`, exit 128), and
            // every subsequent resume re-derives the SAME path and re-hits the SAME failure:
            // a NON-SELF-HEALING PERMANENT WEDGE on the very resume path this unit hardens.
            // Clear the unregistered leftover (deregister it if git still tracks it, else
            // remove the bare dir) so the branch's committed checkpoint is checked out
            // afresh (adv-u4det-leftover-hardfail-confirmed-nonselfhealing). Only the dir is
            // cleared, never the durable branch - the branch's work is exactly what we reuse.
            if std::path::Path::new(dir).exists() {
                clear_worktree_dir(repo, dir)?;
            }
            // Reuse the existing branch's committed work: check it out into the fresh
            // worktree dir, no `-b` (which would refuse, the ref already exists).
            git(repo, &["worktree", "add", dir, branch])?;
        } else {
            git(repo, &["worktree", "add", "-b", branch, dir, "HEAD"])?;
        }
        Ok(Worktree {
            dir: dir.to_string(),
            branch: branch.to_string(),
            repo: repo.to_string(),
        })
    }

    /// Whether the unit's branch has at least one commit beyond the base the run is
    /// integrating into - i.e. the branch carries committed work to REUSE on resume.
    /// A branch that exists but never advanced past the base (`git worktree add -b`
    /// then nothing committed) carries nothing and is treated as no prior work.
    pub fn branch_has_work(repo: &str, branch: &str) -> bool {
        if !branch_exists(repo, branch) {
            return false;
        }
        let base = match run_git(repo, &["rev-parse", "HEAD"]) {
            Ok(b) => b.trim().to_string(),
            Err(_) => return false,
        };
        let tip = match run_git(repo, &["rev-parse", &format!("refs/heads/{branch}")]) {
            Ok(t) => t.trim().to_string(),
            Err(_) => return false,
        };
        if tip == base {
            return false;
        }
        // The branch carries work iff it has commits the base does not: a non-empty
        // `base..branch` range.
        match run_git(repo, &["rev-list", "--count", &format!("{base}..{branch}")]) {
            Ok(n) => n.trim() != "0" && !n.trim().is_empty(),
            Err(_) => false,
        }
    }

    /// Delete the unit's branch ref. Called ONLY after a successful integrate has
    /// merged the branch into the base - the checkpoint has served its purpose and
    /// the merged work lives in the base. An INTERRUPTED unit's branch is NEVER
    /// deleted (that is the whole point of the durable checkpoint), so this is not
    /// part of `remove`, which only tears down the transient dir.
    pub fn delete_branch(repo: &str, branch: &str) -> Result<(), Error> {
        if branch_exists(repo, branch) {
            git(repo, &["branch", "-D", branch])?;
        }
        Ok(())
    }

    /// REVERT `commit` on the run branch checked out in `repo` (spec 12, unit 4): apply the
    /// inverse of the commit's diff and record it as a NEW commit carrying `message` (the
    /// compensation provenance) - never a history rewrite, so the reverse gear is evented and
    /// auditable exactly like the forward [`Self::integrate`] merge. Returns the revert
    /// commit's sha.
    ///
    /// A `--no-commit` revert then an explicit commit lets `message` name the compensation
    /// (git's own revert subject would only echo the reverted commit's subject). A revert
    /// that CONFLICTS is aborted so the run branch is left unchanged and the error surfaces -
    /// the compensation then fails loudly rather than landing a half-reverted tree. A revert
    /// that yields NO change (the commit's effect is already gone) commits nothing and
    /// returns the current HEAD, so it is safely idempotent at the git layer too.
    pub fn revert_on_base(repo: &str, commit: &str, message: &str) -> Result<String, Error> {
        // Reverse-apply the commit's diff to the index/worktree WITHOUT committing, so the
        // compensation message records the rollback instead of git's default "Revert ...".
        if let Err(out) = run_git(repo, &["revert", "--no-commit", commit]) {
            // A conflicting revert leaves partial changes staged; abort so the run branch is
            // untouched and the failure is not silently half-applied.
            let _ = run_git(repo, &["revert", "--abort"]);
            return Err(Error(format!("revert {commit}: {out}")));
        }
        match run_git(repo, &["commit", "--no-edit", "-m", message]) {
            Ok(_) => {}
            // The commit's effect was already absent, so there is nothing to revert: leave
            // HEAD where it is (idempotent), never an error.
            Err(out) if out.contains("nothing to commit") => {}
            Err(out) => return Err(Error(format!("commit revert of {commit}: {out}"))),
        }
        Ok(git(repo, &["rev-parse", "HEAD"])?.trim().to_string())
    }

    /// Reset the run branch checked out in `repo` HARD back to `sha` (spec 12, unit 5): used
    /// to UNDO a merge whose POST-MERGE re-gate went RED, so the broken merged tree never
    /// lands. Unlike [`Self::revert_on_base`] (which reverses an ALREADY-integrated commit as
    /// a new, evented commit - unit 4), this removes a merge that was NEVER recorded with an
    /// `UnitIntegrated`: nothing in the log ever claimed it landed, so discarding it is not a
    /// history rewrite of recorded work, it is aborting a failed integration attempt. The
    /// caller holds the integrate lock, so no concurrent integration observes the reset, and a
    /// following remediation re-attempt re-merges against this same restored tip. An empty
    /// `sha` (no resolvable pre-merge tip) is a no-op rather than an error.
    pub fn reset_to(repo: &str, sha: &str) -> Result<(), Error> {
        if sha.is_empty() {
            return Ok(());
        }
        git(repo, &["reset", "--hard", sha])?;
        Ok(())
    }

    /// Reset THIS worktree's branch HARD to `sha` (the run-branch tip), discarding the unit's
    /// current commits so its NEXT remediation attempt re-implements off that tree. Used when
    /// a unit's integration merge CONFLICTED (an unpredicted overlap): its work was based on a
    /// tree a batch-mate has since changed, so re-doing it off the integrated tree lets its
    /// next merge rebase cleanly - the recovery from the conflict, not a wedge. Resetting a
    /// branch checked out in its OWN worktree is allowed (unlike deleting it).
    pub fn reset_branch_to(&self, sha: &str) -> Result<(), Error> {
        if sha.is_empty() {
            return Ok(());
        }
        git(&self.dir, &["reset", "--hard", sha])?;
        Ok(())
    }

    /// Discard any leftover worktree at `dir` AND any existing `branch`, so a following
    /// [`Self::create`] checks out a FRESH worktree off the repo's CURRENT HEAD.
    ///
    /// For THROWAWAY review scaffolding whose deterministic branch/dir must never ADOPT a
    /// stale checkpoint: a review stage carries no durable work, so its branch is created
    /// off the base HEAD and torn down each step. If a step CRASHES after the review
    /// worktree is created but before cleanup, the deterministic review branch+dir survive
    /// pinned at the OLD base HEAD; on resume [`Self::create`] would ADOPT that surviving
    /// worktree (the fast path / registration adopt), and if sibling stages integrated onto
    /// the base meanwhile the reviewers would review STALE code
    /// (adv-u4det-review-adopt-staleness). Because the review worktree holds nothing worth
    /// keeping, the safe resume is always prune-then-recreate: this clears the dir and the
    /// branch so the subsequent `create` mints a fresh checkout of the current HEAD. NEVER
    /// call this on a unit's durable `rigger/u/*` branch - that would throw away a
    /// checkpoint; it is only for the non-durable `rigger/review/*` branch.
    pub fn discard(repo: &str, dir: &str, branch: &str) -> Result<(), Error> {
        if std::path::Path::new(dir).exists() {
            clear_worktree_dir(repo, dir)?;
        } else {
            // No dir to clear, but a killed process may still leave a dangling admin entry.
            git(repo, &["worktree", "prune"])?;
        }
        Self::delete_branch(repo, branch)
    }

    /// Ensure the run branch `branch` is present in `repo` and CHECKED OUT - the branch
    /// every unit worktree is created from (the conductor branches units off HEAD) and
    /// every [`Self::integrate`] merges into (it merges into the repo's current branch).
    /// Checking it out is therefore mandatory, not incidental: it is what makes the run
    /// branch - not the operator's own branch - the isolation boundary the whole run
    /// depends on. Idempotent, so it is safe to call at the top of every `rigger step`.
    ///
    /// Three cases, returning [`RunBranchSetup`] so the caller can report a divergence:
    ///
    /// - `branch` already exists: REUSE it - check it out if it is not the current
    ///   branch, and NEVER reset it, so the units a prior step integrated onto it are
    ///   preserved. `base` is NOT consulted here: once the run branch exists it is the
    ///   run's durable anchor, and reusing it is exactly how a later step (or a fresh
    ///   `rigger step` after an interruption) CONTINUES the accumulated run. Re-anchoring
    ///   an existing run branch to a different base would orphan every integrated unit,
    ///   so this method deliberately refuses to (`base` re-anchoring only happens on a
    ///   run branch that does not exist yet). Returns [`RunBranchSetup::Reused`].
    /// - `branch` absent and `base` resolves to a commit: create `branch` off `base` and
    ///   check it out. Returns [`RunBranchSetup::CreatedFromBase`].
    /// - `branch` absent and `base` does NOT resolve (e.g. the default `origin/main` on a
    ///   repo with no remote, a `master`-default repo, or a pre-fetch clone): create
    ///   `branch` off the current HEAD instead and check it out. This is NOT a no-op: on
    ///   the native `rigger step` path there is no separate setup step (`cmd_step` IS the
    ///   driver), so if this did nothing HEAD would stay on the operator's branch and the
    ///   conductor would branch and merge machine-generated units directly onto it - the
    ///   exact opposite of the isolation the run branch exists for. Creating off HEAD
    ///   preserves isolation (it mirrors the JS driver's `|| git checkout -B <run>`
    ///   fallback); the caller learns the base was unresolvable via
    ///   [`RunBranchSetup::CreatedFromHead`] and can warn. (`checkout -B` with no
    ///   start-point anchors on the current HEAD and also succeeds on an unborn HEAD.)
    pub fn ensure_run_branch(
        repo: &str,
        branch: &str,
        base: &str,
    ) -> Result<RunBranchSetup, Error> {
        // Classify once (the single authority), then apply only the matching checkout.
        match Self::planned_run_branch_setup(repo, branch, base) {
            RunBranchSetup::Reused => {
                if current_branch(repo).as_deref() != Some(branch) {
                    git(repo, &["checkout", branch])?;
                }
                Ok(RunBranchSetup::Reused)
            }
            RunBranchSetup::CreatedFromBase => {
                git(repo, &["checkout", "-B", branch, base])?;
                Ok(RunBranchSetup::CreatedFromBase)
            }
            RunBranchSetup::CreatedFromHead => {
                git(repo, &["checkout", "-B", branch])?;
                Ok(RunBranchSetup::CreatedFromHead)
            }
        }
    }

    /// What [`Self::ensure_run_branch`] WOULD establish for `branch`/`base` in `repo`,
    /// computed WITHOUT any side effect (no checkout, no branch creation). The SINGLE
    /// authority for the three-way run-branch classification: `ensure_run_branch` dispatches
    /// on this and adds only the matching checkout, so the peek and the act can never diverge.
    ///
    /// A run entry uses this to run the missing-files base check (spec 18, criterion 7) BEFORE
    /// the run branch is anchored: an obviously-wrong base is then refused without ever creating
    /// a run branch that would have to be rolled back, and the corrected `--base` retry re-anchors
    /// fresh because the refused first attempt left no branch. The classification mirrors
    /// `ensure_run_branch` exactly - an existing branch is [`RunBranchSetup::Reused`], an absent
    /// branch with a resolvable `base` is [`RunBranchSetup::CreatedFromBase`], and an absent branch
    /// with an unresolvable `base` is [`RunBranchSetup::CreatedFromHead`].
    pub fn planned_run_branch_setup(repo: &str, branch: &str, base: &str) -> RunBranchSetup {
        if branch_exists(repo, branch) {
            RunBranchSetup::Reused
        } else if ref_resolves(repo, base) {
            RunBranchSetup::CreatedFromBase
        } else {
            RunBranchSetup::CreatedFromHead
        }
    }

    /// The paths an agent created or modified in the worktree.
    ///
    /// Uses `git status --porcelain -z`: NUL-delimited records, which suppresses
    /// the C-quoting that the plain `--porcelain` form applies to paths with
    /// spaces or other special characters. Each record is `XY <path>` where `XY`
    /// is the two-character status and a single space precedes the path. For a
    /// rename or copy (an `R` or `C` in either status column) the `-z` format
    /// splits the entry across two NUL-separated fields - the NEW path first,
    /// then the original - so we keep the new path and skip the original field.
    pub fn changed_files(&self) -> Result<Vec<String>, Error> {
        let out = git(&self.dir, &["status", "--porcelain", "-z"])?;
        Ok(parse_status_z(&out))
    }

    /// Stage and commit the agent's changes on the worktree's branch, returning
    /// the new commit hash - or "" when there was nothing to commit (a read-only
    /// stage, or a stage whose changes are already committed).
    ///
    /// This is the seam that makes a gate measure the COMMITTED artifact, not the
    /// dirty worktree (§3.2): the conductor commits BEFORE running a unit's gates,
    /// so `cargo test` (and every other gate) runs against exactly the tree the
    /// subsequent [`Self::integrate`] merges. Without it a gate could pass on
    /// uncommitted files that never reach the base - a false green.
    pub fn commit(&self, message: &str) -> Result<String, Error> {
        git(&self.dir, &["add", "-A"])?;
        match run_git(&self.dir, &["commit", "-m", message]) {
            Ok(_) => {}
            Err(out) if out.contains("nothing to commit") => return Ok(String::new()),
            Err(out) => return Err(Error(format!("commit: {out}"))),
        }
        Ok(git(&self.dir, &["rev-parse", "HEAD"])?.trim().to_string())
    }

    /// Whether the worktree has uncommitted changes (a dirty tree). Used to assert
    /// the gate runs against a CLEAN, committed tree.
    pub fn is_dirty(&self) -> Result<bool, Error> {
        Ok(!git(&self.dir, &["status", "--porcelain", "-z"])?.is_empty())
    }

    /// Every path this unit changed relative to the base the worktree branched
    /// from - the COMMITTED diff (`git diff --name-only <base>..HEAD`) UNIONED with
    /// any still-uncommitted changes (`git status`).
    ///
    /// [`Self::changed_files`] alone reports only the dirty worktree, which goes
    /// EMPTY once the conductor commits before gating (§3.2); this method spans the
    /// commit, so the FILE_TOUCHED / GATED_BY edges and the grounder reindex still
    /// see the unit's real artifact set whether or not it was committed first. Paths
    /// are sorted and de-duplicated.
    pub fn changed_since_base(&self) -> Result<Vec<String>, Error> {
        // Anchor on the branch's merge-base with the repo HEAD, not the repo HEAD
        // itself: other units may have merged into base since this worktree branched,
        // and a three-dot diff from the merge-base reports only THIS branch's own
        // changes, never the unrelated commits that landed meanwhile.
        let base = git(&self.repo, &["rev-parse", "HEAD"])?.trim().to_string();
        let committed = git(
            &self.dir,
            &["diff", "--name-only", &format!("{base}...HEAD")],
        )?;
        let mut paths: Vec<String> = committed
            .lines()
            .map(|l| l.trim())
            .filter(|l| !l.is_empty())
            .map(|l| l.to_string())
            .collect();
        paths.extend(self.changed_files()?);
        paths.sort();
        paths.dedup();
        Ok(paths)
    }

    /// Commit any remaining changes and merge the branch into the base, returning
    /// the commit hash that landed. A read-only stage (no changes, nothing ever
    /// committed) merges nothing and returns "".
    ///
    /// Idempotent with respect to [`Self::commit`]: when the unit's changes were
    /// already committed (the conductor commits before gating), `commit` here finds
    /// nothing new, so we resolve the branch's existing HEAD and merge that exact
    /// commit. The gate-green artifact and the merged artifact are therefore the
    /// same commit, by construction.
    pub fn integrate(&self, message: &str) -> Result<IntegrateOutcome, Error> {
        let committed = self.commit(message)?;
        // Resolve the commit to merge: a fresh commit from this call, otherwise the
        // branch's current HEAD (the pre-committed, already-gated artifact). When the
        // branch never advanced past the base there is nothing to integrate.
        let commit = if committed.is_empty() {
            let head = git(&self.dir, &["rev-parse", "HEAD"])?.trim().to_string();
            let base = git(&self.repo, &["rev-parse", "HEAD"])?.trim().to_string();
            if head == base {
                return Ok(IntegrateOutcome::Merged(String::new()));
            }
            head
        } else {
            committed
        };
        // A merge that fails leaves the run branch mid-merge with unmerged files. A CONFLICT
        // (an unpredicted overlap the partitioner did not serialize into separate batches) is
        // a RECOVERABLE outcome: ABORT the merge so the run branch is left untouched, and
        // report it so the conductor re-mediates the unit rather than wedging the whole run on
        // a broken branch. Any OTHER merge failure is a genuine error and still surfaces.
        match run_git(&self.repo, &["merge", "--no-edit", &self.branch]) {
            Ok(_) => Ok(IntegrateOutcome::Merged(commit)),
            Err(out) => {
                // Unmerged files are the definitive conflict signal (git-version-independent);
                // the phrasing checks are a belt-and-braces backup. Read BEFORE the abort.
                let conflicted = run_git(&self.repo, &["ls-files", "--unmerged"])
                    .map(|u| !u.trim().is_empty())
                    .unwrap_or(false)
                    || out.contains("CONFLICT")
                    || out.contains("Automatic merge failed")
                    || out.contains("unmerged files");
                let _ = run_git(&self.repo, &["merge", "--abort"]);
                if conflicted {
                    Ok(IntegrateOutcome::Conflict(out))
                } else {
                    Err(Error(format!("git merge --no-edit {}: {out}", self.branch)))
                }
            }
        }
    }

    /// Delete the worktree (its branch is left for the caller to clean up), and reclaim its
    /// sibling per-unit build cache (`cargo-target-<slug>`, Gap 19). This is the DOMINANT
    /// graceful path a unit's worktree is torn down (the conductor's `run_stage` calls it at
    /// stage-end on integrate / park / err), and the cache is a plain dir git never tracks, so
    /// removing the worktree alone would leak a multi-gigabyte cache on the operator's small
    /// partition. Reclamation is best-effort - a review worktree or an un-built unit has no
    /// such sibling and it is a no-op there - and never changes the removal's result.
    pub fn remove(&self) -> Result<(), Error> {
        // Reap any process still rooted inside this worktree BEFORE git removes the dir (spec
        // 23): otherwise a build or tool an agent left running holds a now-deleted cwd and
        // outlives its worktree, leaking memory. Scoped to this EXACT dir, so a process rooted
        // at the repo root or outside rigger's scratch is never touched. Best-effort and a
        // graceful no-op off Linux; it never changes the removal's result.
        crate::reap::reap_processes_rooted_under(std::path::Path::new(&self.dir));
        git(&self.repo, &["worktree", "remove", "--force", &self.dir])?;
        reclaim_cache_sibling(&self.dir);
        Ok(())
    }
}

/// Parse the output of `git status --porcelain -z` into the list of changed
/// destination paths. Records are NUL-terminated; a rename/copy record is
/// followed by an extra NUL-terminated field holding the original path, which we
/// consume and discard (we want the new path only).
fn parse_status_z(out: &str) -> Vec<String> {
    let mut fields = out.split('\0').filter(|f| !f.is_empty());
    let mut paths = Vec::new();
    while let Some(record) = fields.next() {
        // Each record is `XY <path>`: a two-char status, a space, then the path.
        if record.len() < 4 {
            continue;
        }
        let status = &record[..2];
        let path = &record[3..];
        // A rename (`R`) or copy (`C`) in either column carries the original path
        // in the next NUL-separated field; skip it so it is not reported.
        if status.starts_with('R')
            || status.starts_with('C')
            || status[1..].starts_with('R')
            || status[1..].starts_with('C')
        {
            fields.next();
        }
        paths.push(path.to_string());
    }
    paths
}

/// Whether a local branch ref exists in the repo. Used by [`Worktree::create`] to
/// decide between creating the unit's deterministic branch and checking out the
/// existing one (reusing a prior window's committed work).
fn branch_exists(repo: &str, branch: &str) -> bool {
    run_git(
        repo,
        &[
            "rev-parse",
            "--verify",
            "--quiet",
            &format!("refs/heads/{branch}"),
        ],
    )
    .is_ok()
}

/// Whether `r` resolves to a commit in `repo` (a branch, tag, remote-tracking ref,
/// or sha). Used by [`Worktree::ensure_run_branch`] to distinguish a base ref it can
/// anchor the run branch to from a not-yet-present default (e.g. `origin/main` on a
/// repo with no remote), which triggers the create-off-HEAD fallback rather than an
/// error. Public so a run entry can guard the missing-files base check on a base that
/// actually resolves (an unresolvable base has no tree to look paths up in - checking
/// against it would read as "every path absent" and refuse spuriously).
pub fn ref_resolves(repo: &str, r: &str) -> bool {
    run_git(
        repo,
        &[
            "rev-parse",
            "--verify",
            "--quiet",
            &format!("{r}^{{commit}}"),
        ],
    )
    .is_ok()
}

/// Whether the repo-relative `path` exists in the tree of the commit `base_ref` names -
/// as either a blob (file) or a sub-tree (directory). Implemented with
/// `git cat-file -e <base_ref>:<path>`, which exits 0 when the object is present and
/// non-zero (with a captured, non-leaking diagnostic) when it is absent or `base_ref`
/// does not resolve. A run entry uses this to check the path-like tokens a spec's criteria
/// reference against the base the run is anchored on, so an obviously-wrong base (none of
/// the spec's paths present) is refused before the run parks its first unit (spec 18).
/// Callers must have already confirmed `base_ref` resolves (see [`ref_resolves`]); against
/// an unresolvable ref every path reads as absent.
pub fn path_in_ref(repo: &str, base_ref: &str, path: &str) -> bool {
    run_git(repo, &["cat-file", "-e", &format!("{base_ref}:{path}")]).is_ok()
}

/// The name of the branch currently checked out in `repo`, or None on a detached
/// HEAD. An unborn HEAD (a fresh repo with no commit) still reports its default
/// branch name, so this only returns None for a genuinely detached HEAD.
fn current_branch(repo: &str) -> Option<String> {
    run_git(repo, &["symbolic-ref", "--short", "-q", "HEAD"])
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// Resolve the run's scratch root - where transient worktrees live. Precedence:
/// `env_override` (the `RIGGER_TMPDIR` environment variable, machine-local placement) >
/// `configured` (`defaults.workdir` from workflow.yml, versioned placement) > the
/// default `<repo>/.rigger/tmp`. A leading `~/` expands to $HOME. NEVER the OS temp
/// dir: worktrees carry multi-gigabyte build dirs, and on the common
/// small-root/large-home partition layout the OS disk is the one that cannot absorb
/// them (design-intent Gap 14). The resolved dir is created if absent.
pub fn scratch_root(repo: &str, configured: &str, env_override: Option<&str>) -> String {
    let expanded = scratch_root_path(repo, configured, env_override);
    let _ = std::fs::create_dir_all(&expanded);
    expanded
}

/// Resolve the scratch root PATH by the SAME precedence as [`scratch_root`] but WITHOUT
/// the create-if-absent side effect - the read-only half. `rigger validate`'s residue
/// scan (spec 06) needs the path to READ leftover worktrees/caches under it and must stay
/// read-only, so it resolves here and never conjures a `.rigger/tmp` on a project that
/// never ran. [`scratch_root`] is this plus a `create_dir_all`, keeping ONE resolver.
pub fn scratch_root_path(repo: &str, configured: &str, env_override: Option<&str>) -> String {
    let chosen = match env_override {
        Some(v) if !v.trim().is_empty() => v.trim().to_string(),
        _ if !configured.trim().is_empty() => configured.trim().to_string(),
        _ => format!("{}/.rigger/tmp", if repo.is_empty() { "." } else { repo }),
    };
    match (chosen.strip_prefix("~/"), std::env::var("HOME")) {
        (Some(rest), Ok(home)) => format!("{home}/{rest}"),
        _ => chosen,
    }
}

/// [`scratch_root`] with the `RIGGER_TMPDIR` environment variable as the override.
pub fn scratch_root_from_env(repo: &str, configured: &str) -> String {
    let env = std::env::var("RIGGER_TMPDIR").ok();
    scratch_root(repo, configured, env.as_deref())
}

/// [`scratch_root_path`] with the `RIGGER_TMPDIR` environment variable as the override -
/// the read-only resolver `rigger validate` uses to locate (never create) the scratch root.
pub fn scratch_root_path_from_env(repo: &str, configured: &str) -> String {
    let env = std::env::var("RIGGER_TMPDIR").ok();
    scratch_root_path(repo, configured, env.as_deref())
}

/// Filesystem prefix of a unit's DETERMINISTIC worktree dir under the scratch root
/// (`rigger-wt-<slug>`); the conductor's `unit_worktree_dir` is the single authority that
/// builds it, and [`sweep_terminal`] / [`unit_cache_sibling`] read it back.
pub const UNIT_WORKTREE_PREFIX: &str = "rigger-wt-";

/// Filesystem prefix of a unit's per-unit build cache dir (`cargo-target-<slug>`), a
/// SIBLING of its worktree under the scratch root (Gap 19). Gate commands the conductor
/// runs inside a unit worktree build into this unit-keyed `CARGO_TARGET_DIR` so divergent
/// unit trees never share incremental state. [`unit_cache_sibling`] is the single authority
/// that derives the path (from the worktree dir the gate runs in); it is a plain dir, NOT a
/// registered git worktree, so nothing git-side reclaims it. [`reclaim_cache_sibling`] does:
/// [`Worktree::remove`] reclaims it on the DOMINANT graceful path (a unit's worktree is torn
/// down at the end of `run_stage`), and [`sweep_terminal`] reclaims it on the crash-recovery
/// path (a killed step process leaves the worktree still registered).
pub const UNIT_CACHE_PREFIX: &str = "cargo-target-";

/// The per-unit build cache dir that is a SIBLING of the unit worktree at `worktree_dir`
/// (Gap 19): `<root>/rigger-wt-<slug>` -> `<root>/cargo-target-<slug>`. Returns None for any
/// dir that is not a unit worktree (e.g. a `rigger-review-*` review worktree, or the empty
/// worktree-less path), which owns no such cache. Because both the worktree dir and the cache
/// dir derive from the same scratch root and the same slug, swapping the prefix reconstructs
/// the exact cache path. This is the SINGLE source of the derivation: the conductor's
/// `run_gates` uses it to point a gate's `CARGO_TARGET_DIR` at the cache, and
/// [`reclaim_cache_sibling`] uses it to reclaim that same cache when the worktree is removed.
pub fn unit_cache_sibling(worktree_dir: &str) -> Option<String> {
    let path = std::path::Path::new(worktree_dir);
    let slug = path
        .file_name()?
        .to_str()?
        .strip_prefix(UNIT_WORKTREE_PREFIX)?;
    let parent = path.parent()?.to_str()?;
    Some(format!("{parent}/{UNIT_CACHE_PREFIX}{slug}"))
}

/// Reclaim the per-unit build cache that is a SIBLING of the unit worktree at `worktree_dir`
/// (Gap 19) - the ONE mutation authority for cache reclamation, called from both worktree
/// removal paths: [`Worktree::remove`] (the dominant graceful teardown) and [`sweep_terminal`]
/// (crash recovery). A no-op for any dir that owns no such cache (a review worktree, or a unit
/// whose gates never ran cargo, has none). Best-effort: a failed reclaim of a throwaway cache
/// must never fail worktree teardown or abort the sweep.
fn reclaim_cache_sibling(worktree_dir: &str) {
    if let Some(cache) = unit_cache_sibling(worktree_dir) {
        let _ = std::fs::remove_dir_all(cache);
    }
}

/// Sweep the scratch root's TERMINAL worktrees: prune stale registrations, then remove
/// every registered worktree under `root` whose branch tip is already an ancestor of
/// `run_branch` - integrated (or never-advanced review scaffolding), so the worktree
/// serves no in-flight unit. Unmerged branches are in-flight checkpoints and are left
/// alone. Returns how many worktrees were removed. This is the "the loop cleans up
/// after itself" half of Gap 14: crashed or superseded step processes leak worktrees,
/// and integrate-time removal alone never reclaims them.
///
/// Removing a UNIT worktree also reclaims its sibling per-unit build cache
/// (`cargo-target-<slug>`, Gap 19) via [`reclaim_cache_sibling`]. This is the CRASH-recovery
/// half: a step process killed before it reached [`Worktree::remove`] leaves its worktree
/// still registered, so the graceful reclamation never ran and the sweep must reclaim the
/// cache here. On the dominant graceful path [`Worktree::remove`] already reclaimed it, so
/// this sweep never sees that worktree at all. Reclamation is best-effort and never aborts
/// the sweep.
pub fn sweep_terminal(repo: &str, root: &str, run_branch: &str) -> Result<usize, Error> {
    git(repo, &["worktree", "prune"])?;
    let out = run_git(repo, &["worktree", "list", "--porcelain"]).map_err(Error)?;
    let mut removed = 0;
    let mut dir: Option<String> = None;
    for line in out.lines() {
        if let Some(d) = line.strip_prefix("worktree ") {
            dir = Some(d.to_string());
        } else if let Some(branch) = line.strip_prefix("branch refs/heads/") {
            let Some(d) = dir.take() else { continue };
            if !d.starts_with(root) || branch == run_branch {
                continue;
            }
            let merged =
                run_git(repo, &["merge-base", "--is-ancestor", branch, run_branch]).is_ok();
            if merged {
                git(repo, &["worktree", "remove", "--force", &d])?;
                reclaim_cache_sibling(&d);
                removed += 1;
            }
        }
    }
    Ok(removed)
}

/// Whether `dir` already exists on disk AS the worktree that has `branch` checked out -
/// a direct PATH LOOKUP (the dir's own HEAD via `symbolic-ref`), NOT a parse of the
/// repo-wide `git worktree list`. Because unit and review worktree dirs are now
/// DETERMINISTIC (derived from the id / stage+attempt, no per-process uuid, Gap 12), a
/// resume or concurrent process derives the same `dir`, and [`Worktree::create`] uses
/// this to ADOPT it without the porcelain adopt-or-prune scan. A dir that is absent, is
/// not a git worktree, or is checked out to a different branch yields false, so the
/// caller falls back to the porcelain adopt-or-prune path.
fn worktree_on_branch(dir: &str, branch: &str) -> bool {
    std::path::Path::new(dir).is_dir() && current_branch(dir).as_deref() == Some(branch)
}

/// The dir of the worktree that already has `branch` checked out, if any - parsed
/// from `git worktree list --porcelain` (a `worktree <dir>` line followed by its
/// `branch refs/heads/<name>` line). Registrations whose dirs were deleted out from
/// under git still appear here; the caller decides adopt-vs-prune by checking the dir.
fn registered_worktree_for(repo: &str, branch: &str) -> Option<String> {
    let out = run_git(repo, &["worktree", "list", "--porcelain"]).ok()?;
    let want = format!("branch refs/heads/{branch}");
    let mut dir: Option<&str> = None;
    for line in out.lines() {
        if let Some(d) = line.strip_prefix("worktree ") {
            dir = Some(d);
        } else if line.trim() == want {
            return dir.map(|d| d.to_string());
        }
    }
    None
}

/// Remove whatever occupies `dir` so a subsequent `git worktree add <dir>` cannot
/// hard-fail on a pre-existing path, then prune dangling worktree admin entries. Handles
/// BOTH a worktree git still tracks (deregistered cleanly via `git worktree remove
/// --force`, which also tolerates a dirty tree) AND a bare leftover directory a killed
/// process left behind (`git worktree remove` refuses it - "not a working tree" - so we
/// delete it off disk). Used to defend the now-DETERMINISTIC unit dir in
/// [`Worktree::create`] and to reset a throwaway review worktree in [`Worktree::discard`].
fn clear_worktree_dir(repo: &str, dir: &str) -> Result<(), Error> {
    if run_git(repo, &["worktree", "remove", "--force", dir]).is_err()
        && std::path::Path::new(dir).exists()
    {
        std::fs::remove_dir_all(dir)
            .map_err(|e| Error(format!("remove leftover worktree dir {dir}: {e}")))?;
    }
    git(repo, &["worktree", "prune"])?;
    Ok(())
}

/// The current HEAD sha of the git checkout at `dir`, or `""` when `dir` is empty
/// (a repo-less run, which has no worktree to stamp) or git cannot resolve it.
///
/// This is the seam spec-11 unit-1 uses to stamp the reviewed sha as metadata on the
/// review-boundary events (`verified`, the review-reject `UnitFailed`, `reviewed`),
/// mirroring the commit `UnitIntegrated` already carries: two review verdicts on the
/// SAME sha are reviewer noise (the flip-flop metric), so the fold needs the sha the
/// tiers actually judged. It is deliberately non-failing - an unresolvable HEAD yields
/// an empty stamp that the emit path then omits, never an error that fails the run.
pub fn head_sha_of(dir: &str) -> String {
    if dir.is_empty() {
        return String::new();
    }
    run_git(dir, &["rev-parse", "HEAD"])
        .map(|s| s.trim().to_string())
        .unwrap_or_default()
}

/// The git TREE-SHA of the committed HEAD tree in `dir` - the content address of the
/// whole worktree (spec 12, unit 1: content-addressed gate verdicts). Unlike
/// [`head_sha_of`] (the COMMIT sha, which changes on every commit even when the tree is
/// byte-identical), this is the TREE object sha, so two commits carrying the same file
/// content hash EQUAL: it is a pure function of the tree's bytes, which is exactly the
/// property the gate cache needs (a gate re-run over an unchanged tree is a hit; a
/// changed tree misses). It is the whole-tree default; unit 3 narrows the addressed
/// inputs to a gate's `inputs:` paths.
///
/// Deliberately non-failing, mirroring [`head_sha_of`]: an empty `dir` (a repo-less /
/// worktree-less gate run) or an unresolvable HEAD yields an empty string, which the
/// caller reads as "no tree to address" and simply skips content-addressing - never an
/// error that fails the run.
pub fn tree_sha_of(dir: &str) -> String {
    if dir.is_empty() {
        return String::new();
    }
    run_git(dir, &["rev-parse", "HEAD^{tree}"])
        .map(|s| s.trim().to_string())
        .unwrap_or_default()
}

fn git(dir: &str, args: &[&str]) -> Result<String, Error> {
    run_git(dir, args).map_err(|out| Error(format!("git {}: {out}", args.join(" "))))
}

fn run_git(dir: &str, args: &[&str]) -> Result<String, String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .map_err(|e| e.to_string())?;
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    if out.status.success() {
        Ok(combined)
    } else {
        Err(combined)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn init_repo() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().to_str().unwrap();
        for args in [
            &["init", "-q"][..],
            &["config", "user.email", "t@example.com"],
            &["config", "user.name", "t"],
            &["commit", "--allow-empty", "-q", "-m", "init"],
        ] {
            run_git(p, args).unwrap();
        }
        dir
    }

    #[test]
    fn path_in_ref_sees_committed_files_and_directories_only() {
        let repo = init_repo();
        let p = repo.path().to_str().unwrap();
        std::fs::create_dir_all(repo.path().join("src")).unwrap();
        std::fs::write(repo.path().join("src").join("main.rs"), "fn main() {}\n").unwrap();
        run_git(p, &["add", "src/main.rs"]).unwrap();
        run_git(p, &["commit", "-q", "-m", "add main"]).unwrap();

        // A committed file and its containing directory both resolve in HEAD's tree.
        assert!(path_in_ref(p, "HEAD", "src/main.rs"));
        assert!(path_in_ref(p, "HEAD", "src"));
        // A path never committed does not, and neither does one against an unresolvable ref.
        assert!(!path_in_ref(p, "HEAD", "src/does_not_exist.rs"));
        assert!(!path_in_ref(p, "HEAD", "crates/foo/src/bar.rs"));
        assert!(!path_in_ref(p, "no-such-ref", "src/main.rs"));
    }

    #[test]
    fn integrate_lands_work_in_the_repo() {
        let repo = init_repo();
        let repo_path = repo.path().to_str().unwrap().to_string();
        let wt_path = std::env::temp_dir().join(format!("rigger-wt-{}", uuid::Uuid::new_v4()));
        let wt = Worktree::create(&repo_path, wt_path.to_str().unwrap(), "rigger/test").unwrap();

        std::fs::write(wt_path.join("feature.txt"), "work\n").unwrap();
        assert_eq!(wt.changed_files().unwrap(), ["feature.txt"]);

        let commit = wt
            .integrate("rigger: integrate test")
            .unwrap()
            .expect_merged();
        assert!(!commit.is_empty(), "a commit hash should be returned");
        assert!(
            repo.path().join("feature.txt").exists(),
            "the agent's work must be merged into the repo"
        );
        wt.remove().unwrap();
    }

    #[test]
    fn integrate_reports_a_merge_conflict_and_leaves_the_run_branch_untouched() {
        // The unpredicted-overlap case the spec-13 dogfood hit: two units the partitioner
        // placed in ONE batch both add the SAME file with DIFFERENT content off the same base.
        // The first merges; the second's merge CONFLICTS. integrate() must ABORT the merge,
        // leave the run branch EXACTLY as the first unit left it, and report Conflict - so the
        // conductor re-mediates the second unit instead of the run wedging on a broken branch.
        let repo = init_repo();
        let repo_path = repo.path().to_str().unwrap().to_string();

        // Both worktrees branch off the SAME base, as concurrent batch-mates do.
        let wa = std::env::temp_dir().join(format!("rigger-wt-{}", uuid::Uuid::new_v4()));
        let wb = std::env::temp_dir().join(format!("rigger-wt-{}", uuid::Uuid::new_v4()));
        let a = Worktree::create(&repo_path, wa.to_str().unwrap(), "rigger/u/a").unwrap();
        let b = Worktree::create(&repo_path, wb.to_str().unwrap(), "rigger/u/b").unwrap();

        // A adds shared.txt and integrates cleanly.
        std::fs::write(wa.join("shared.txt"), "A version\n").unwrap();
        a.integrate("rigger: integrate a").unwrap().expect_merged();
        let head_after_a = run_git(&repo_path, &["rev-parse", "HEAD"])
            .unwrap()
            .trim()
            .to_string();

        // B adds the SAME file with DIFFERENT content off the same base: an add/add conflict.
        std::fs::write(wb.join("shared.txt"), "B version\n").unwrap();
        match b.integrate("rigger: integrate b").unwrap() {
            IntegrateOutcome::Conflict(detail) => assert!(
                detail.to_lowercase().contains("conflict"),
                "the conflict detail names the conflict; got: {detail}"
            ),
            IntegrateOutcome::Merged(_) => {
                panic!("a divergent add/add merge must conflict, not merge")
            }
        }

        // The run branch is UNTOUCHED: A's content stands, HEAD is unchanged, and no merge is
        // left in progress (the abort cleaned it) - so the next `rigger step` runs clean.
        assert_eq!(
            std::fs::read_to_string(repo.path().join("shared.txt")).unwrap(),
            "A version\n",
            "the aborted conflict must not alter the run branch"
        );
        let head_now = run_git(&repo_path, &["rev-parse", "HEAD"])
            .unwrap()
            .trim()
            .to_string();
        assert_eq!(
            head_now, head_after_a,
            "HEAD is unchanged after the aborted merge"
        );
        assert!(
            !repo.path().join(".git").join("MERGE_HEAD").exists(),
            "no merge is left in progress after the abort"
        );
    }

    #[test]
    fn revert_on_base_rolls_back_an_integrated_commit_with_a_provenance_message() {
        // spec 12, unit 4: revert_on_base reverses an integrated commit's diff on the run
        // branch as a NEW, message-carrying commit (an evented rollback, not a rewrite), so a
        // compensated unit's change is undone with auditable provenance.
        let repo = init_repo();
        let repo_path = repo.path().to_str().unwrap().to_string();
        let wt_path = std::env::temp_dir().join(format!("rigger-wt-{}", uuid::Uuid::new_v4()));
        let wt = Worktree::create(&repo_path, wt_path.to_str().unwrap(), "rigger/revert").unwrap();
        std::fs::write(wt_path.join("wrong.txt"), "buggy\n").unwrap();
        let commit = wt
            .integrate("rigger: integrate wrong")
            .unwrap()
            .expect_merged();
        wt.remove().unwrap();
        assert!(
            repo.path().join("wrong.txt").exists(),
            "precondition: the integrated file lands in the repo"
        );

        let revert = Worktree::revert_on_base(
            &repo_path,
            &commit,
            "rigger: compensate unit-a (revert the buggy change)",
        )
        .unwrap();
        assert_ne!(
            revert, commit,
            "the revert is a new commit, not the original"
        );
        assert!(
            !repo.path().join("wrong.txt").exists(),
            "reverting the integrating commit removes its change from the run branch"
        );
        let subjects = run_git(&repo_path, &["log", "--pretty=%s"]).unwrap();
        assert!(
            subjects.lines().any(|l| l.contains("compensate unit-a")),
            "the rollback is evented with the compensation provenance message; log:\n{subjects}"
        );
        // The original integrating commit is still in history (an evented revert never
        // rewrites the past), so the rollback is fully auditable.
        assert!(
            run_git(&repo_path, &["cat-file", "-t", &commit]).is_ok(),
            "the reverted commit remains reachable in history"
        );
    }

    #[test]
    fn revert_on_base_aborts_and_errors_on_a_conflicting_revert() {
        // spec 12, unit 4 (the reverse gear's FAILURE path, which the happy-path test never
        // drives): a compensation whose revert CONFLICTS - a later commit rewrote the same
        // region the condemned commit introduced, the REALISTIC case since a unit that proves
        // a prior unit wrong usually built ON it - must ABORT and surface an error, leaving the
        // run branch UNCHANGED rather than a half-reverted tree. drain_compensations propagates
        // this Err and the run aborts loudly instead of landing a partial rollback.
        let repo = init_repo();
        let repo_path = repo.path().to_str().unwrap().to_string();

        // C1 introduces `shared.txt`; a later commit rewrites the SAME line, so reverting C1
        // (which wants to delete the line C1 added) conflicts with the later modification.
        let wt_path = std::env::temp_dir().join(format!("rigger-wt-{}", uuid::Uuid::new_v4()));
        let wt =
            Worktree::create(&repo_path, wt_path.to_str().unwrap(), "rigger/conflict").unwrap();
        std::fs::write(wt_path.join("shared.txt"), "original\n").unwrap();
        let c1 = wt
            .integrate("rigger: integrate original")
            .unwrap()
            .expect_merged();
        wt.remove().unwrap();
        std::fs::write(repo.path().join("shared.txt"), "changed later\n").unwrap();
        run_git(&repo_path, &["add", "shared.txt"]).unwrap();
        run_git(
            &repo_path,
            &["commit", "-m", "later change to the same line"],
        )
        .unwrap();
        let head_before = run_git(&repo_path, &["rev-parse", "HEAD"]).unwrap();
        let head_before = head_before.trim();

        let result =
            Worktree::revert_on_base(&repo_path, &c1, "rigger: compensate unit-a (revert c1)");
        assert!(
            result.is_err(),
            "a conflicting revert surfaces as an error, never a silent half-apply"
        );
        // The run branch is UNCHANGED: same HEAD, the later content stands, and the abort
        // cleaned the sequencer so no revert is left in progress for the next operation.
        let head_after = run_git(&repo_path, &["rev-parse", "HEAD"]).unwrap();
        assert_eq!(
            head_after.trim(),
            head_before,
            "an aborted revert leaves the run branch HEAD untouched"
        );
        assert_eq!(
            std::fs::read_to_string(repo.path().join("shared.txt")).unwrap(),
            "changed later\n",
            "the conflicting file keeps the branch content, not a half-reverted tree"
        );
        assert!(
            !repo.path().join(".git/REVERT_HEAD").exists(),
            "the abort clears the in-progress revert so the branch is clean for the next op"
        );
    }

    #[test]
    fn revert_on_base_is_idempotent_when_the_effect_is_already_gone() {
        // spec 12, unit 4 (the reverse gear's git-layer idempotency, the last-line defense
        // behind the `compensated_commits` replay guard): reverting a commit whose effect is
        // ALREADY absent from the run branch commits NOTHING and returns the current HEAD -
        // never an error, never a spurious empty commit - so a re-reached rollback is safe.
        let repo = init_repo();
        let repo_path = repo.path().to_str().unwrap().to_string();

        let wt_path = std::env::temp_dir().join(format!("rigger-wt-{}", uuid::Uuid::new_v4()));
        let wt = Worktree::create(&repo_path, wt_path.to_str().unwrap(), "rigger/idem").unwrap();
        std::fs::write(wt_path.join("gone.txt"), "effect\n").unwrap();
        let c1 = wt
            .integrate("rigger: integrate effect")
            .unwrap()
            .expect_merged();
        wt.remove().unwrap();

        // First revert removes the effect and lands a real compensation commit.
        let r1 =
            Worktree::revert_on_base(&repo_path, &c1, "rigger: compensate (revert c1)").unwrap();
        assert!(
            !repo.path().join("gone.txt").exists(),
            "the first revert removes the effect from the run branch"
        );
        let count_after_r1 = run_git(&repo_path, &["rev-list", "--count", "HEAD"]).unwrap();

        // A SECOND revert of the SAME commit - its effect already gone - is a no-op: the
        // `nothing to commit` branch returns the unchanged HEAD without adding an empty commit.
        let r2 = Worktree::revert_on_base(&repo_path, &c1, "rigger: compensate again (revert c1)")
            .unwrap();
        assert_eq!(
            r2, r1,
            "the idempotent second revert returns the unchanged HEAD"
        );
        let count_after_r2 = run_git(&repo_path, &["rev-list", "--count", "HEAD"]).unwrap();
        assert_eq!(
            count_after_r2.trim(),
            count_after_r1.trim(),
            "the idempotent revert adds no spurious empty commit"
        );
    }

    #[test]
    fn commit_cleans_the_tree_so_a_gate_sees_the_committed_artifact() {
        // FIX 2: the conductor commits the worktree BEFORE gating, so a gate runs
        // against the committed state, not the dirty worktree. After `commit` the
        // tree must be clean (no uncommitted false-green source) and the work must
        // be a real commit on the branch.
        let repo = init_repo();
        let repo_path = repo.path().to_str().unwrap().to_string();
        let wt_path = std::env::temp_dir().join(format!("rigger-wt-{}", uuid::Uuid::new_v4()));
        let wt = Worktree::create(&repo_path, wt_path.to_str().unwrap(), "rigger/commit").unwrap();

        std::fs::write(wt_path.join("feature.txt"), "work\n").unwrap();
        assert!(
            wt.is_dirty().unwrap(),
            "an uncommitted file leaves a dirty tree"
        );

        let commit = wt.commit("rigger: commit before gating").unwrap();
        assert!(
            !commit.is_empty(),
            "committing must return the new commit hash"
        );
        assert!(
            !wt.is_dirty().unwrap(),
            "after commit the worktree must be clean - the gate sees the committed artifact"
        );
        // The committed file is the one the unit changed relative to base, surviving
        // the now-clean `git status`.
        assert_eq!(wt.changed_since_base().unwrap(), ["feature.txt"]);

        // A second commit with nothing new returns "" (idempotent).
        assert!(wt.commit("rigger: noop").unwrap().is_empty());
        wt.remove().unwrap();
    }

    #[test]
    fn tree_sha_of_addresses_tree_content_not_the_commit() {
        // spec 12, unit 1: tree_sha_of is the content address of the committed tree. Two
        // DISTINCT commits (different message / parent / time, so a different COMMIT sha)
        // that carry byte-identical trees must yield the SAME tree sha - so a gate re-run
        // over an unchanged input is a cache hit - while a real content change must yield a
        // DIFFERENT sha - so a changed input misses.
        let repo = init_repo();
        let p = repo.path().to_str().unwrap().to_string();

        std::fs::write(repo.path().join("a.txt"), "one\n").unwrap();
        run_git(&p, &["add", "-A"]).unwrap();
        run_git(&p, &["commit", "-q", "-m", "first"]).unwrap();
        let t1 = tree_sha_of(&p);
        assert_eq!(t1.len(), 40, "a git tree sha is 40 hex chars: {t1:?}");
        assert!(t1.chars().all(|c| c.is_ascii_hexdigit()));

        // A fresh EMPTY commit advances the COMMIT sha but leaves the tree bytes unchanged,
        // so the TREE sha is stable - the exact property head_sha_of does NOT have.
        let head1 = head_sha_of(&p);
        run_git(&p, &["commit", "--allow-empty", "-q", "-m", "empty"]).unwrap();
        assert_ne!(head_sha_of(&p), head1, "the commit sha advances");
        assert_eq!(
            tree_sha_of(&p),
            t1,
            "an empty commit leaves the tree bytes unchanged, so the tree sha is stable"
        );

        // A real content change must move the tree sha.
        std::fs::write(repo.path().join("a.txt"), "two\n").unwrap();
        run_git(&p, &["add", "-A"]).unwrap();
        run_git(&p, &["commit", "-q", "-m", "second"]).unwrap();
        assert_ne!(
            tree_sha_of(&p),
            t1,
            "changed content must change the tree sha"
        );

        // A worktree-less (empty) dir yields no address, so the caller skips addressing.
        assert!(tree_sha_of("").is_empty());
    }

    #[test]
    fn integrate_lands_a_pre_committed_artifact_unchanged() {
        // After the conductor commits before gating, integrate must merge that EXACT
        // committed artifact - not re-commit, not drop it. The merged commit equals
        // the one `commit` produced, so gate-green and merged are the same commit.
        let repo = init_repo();
        let repo_path = repo.path().to_str().unwrap().to_string();
        let wt_path = std::env::temp_dir().join(format!("rigger-wt-{}", uuid::Uuid::new_v4()));
        let wt = Worktree::create(&repo_path, wt_path.to_str().unwrap(), "rigger/pre").unwrap();

        std::fs::write(wt_path.join("feature.txt"), "work\n").unwrap();
        let committed = wt.commit("rigger: pre-commit").unwrap();
        assert!(!wt.is_dirty().unwrap());

        let merged = wt.integrate("rigger: integrate").unwrap().expect_merged();
        assert_eq!(
            merged, committed,
            "integrate must merge the same commit that was gated, not a new one"
        );
        assert!(
            repo.path().join("feature.txt").exists(),
            "the pre-committed work must land in the repo"
        );
        wt.remove().unwrap();
    }

    #[test]
    fn changed_files_reports_only_the_rename_destination() {
        let repo = init_repo();
        let repo_path = repo.path().to_str().unwrap().to_string();
        let wt_path = std::env::temp_dir().join(format!("rigger-wt-{}", uuid::Uuid::new_v4()));
        let wt = Worktree::create(&repo_path, wt_path.to_str().unwrap(), "rigger/rename").unwrap();

        // Commit an original file, then rename it (git stages the rename via `mv`)
        // so git reports it as `R` rather than an add+delete pair.
        std::fs::write(wt_path.join("orig.txt"), "content\n").unwrap();
        run_git(wt_path.to_str().unwrap(), &["add", "-A"]).unwrap();
        run_git(
            wt_path.to_str().unwrap(),
            &["commit", "-q", "-m", "add orig"],
        )
        .unwrap();
        run_git(
            wt_path.to_str().unwrap(),
            &["mv", "orig.txt", "renamed.txt"],
        )
        .unwrap();

        // The destination path only - never the bogus `orig.txt -> renamed.txt`
        // string the plain --porcelain form would have yielded, and never the
        // original `orig.txt`.
        assert_eq!(wt.changed_files().unwrap(), ["renamed.txt"]);
        wt.remove().unwrap();
    }

    #[test]
    fn create_reuses_an_existing_branchs_head() {
        // Resume-continuity: a unit's deterministic branch is the durable checkpoint.
        // After its worktree dir is removed, `create` on the SAME branch must check
        // out the existing branch (not fail trying to re-create the ref, and not
        // branch fresh off HEAD), so a file the prior window committed is present in
        // the recreated worktree - the work is reused, never thrown away.
        let repo = init_repo();
        let repo_path = repo.path().to_str().unwrap().to_string();
        let branch = "rigger/u/unit-1";

        // Window 1: create the branch, commit work, remove the transient dir. The
        // branch ref survives.
        let dir1 = std::env::temp_dir().join(format!("rigger-wt-{}", uuid::Uuid::new_v4()));
        let wt1 = Worktree::create(&repo_path, dir1.to_str().unwrap(), branch).unwrap();
        std::fs::write(dir1.join("carried.txt"), "prior-window work\n").unwrap();
        let committed = wt1.commit("rigger: window 1").unwrap();
        assert!(!committed.is_empty(), "window 1 must commit work");
        assert!(
            Worktree::branch_has_work(&repo_path, branch),
            "the branch must carry committed work for resume to reuse"
        );
        wt1.remove().unwrap(); // tear down the transient dir; branch survives.

        // Window 2: a FRESH dir, same branch. `create` must check out the existing
        // branch, so the committed file is present without re-implementing.
        let dir2 = std::env::temp_dir().join(format!("rigger-wt-{}", uuid::Uuid::new_v4()));
        let wt2 = Worktree::create(&repo_path, dir2.to_str().unwrap(), branch).unwrap();
        assert!(
            dir2.join("carried.txt").exists(),
            "the recreated worktree must contain the file committed on the branch in the prior window"
        );
        assert_eq!(
            std::fs::read_to_string(dir2.join("carried.txt")).unwrap(),
            "prior-window work\n",
            "the reused branch's committed content must be intact"
        );
        // The reused worktree's HEAD is the prior window's commit, not the base.
        let head = git(dir2.to_str().unwrap(), &["rev-parse", "HEAD"])
            .unwrap()
            .trim()
            .to_string();
        assert_eq!(
            head, committed,
            "the reused worktree's HEAD is the branch tip"
        );
        wt2.remove().unwrap();

        // After integrate the branch is cleaned up; an interrupted branch is not.
        Worktree::delete_branch(&repo_path, branch).unwrap();
        assert!(
            !Worktree::branch_has_work(&repo_path, branch),
            "delete_branch removes the checkpoint after it has served its purpose"
        );
    }

    #[test]
    fn scratch_root_resolves_env_then_config_then_repo_default() {
        // Precedence: RIGGER_TMPDIR (passed as the override param) > defaults.workdir
        // > <repo>/.rigger/tmp. The default lives on the REPO's partition, never the
        // OS temp dir (Gap 14).
        let repo = init_repo();
        let repo_path = repo.path().to_str().unwrap().to_string();

        let dflt = scratch_root(&repo_path, "", None);
        assert_eq!(dflt, format!("{repo_path}/.rigger/tmp"));
        assert!(std::path::Path::new(&dflt).is_dir(), "the root is created");

        let cfg_dir = repo.path().join("elsewhere");
        let configured = scratch_root(&repo_path, cfg_dir.to_str().unwrap(), None);
        assert_eq!(configured, cfg_dir.to_str().unwrap());

        let env_dir = repo.path().join("env-wins");
        let env = scratch_root(
            &repo_path,
            cfg_dir.to_str().unwrap(),
            Some(env_dir.to_str().unwrap()),
        );
        assert_eq!(env, env_dir.to_str().unwrap(), "env override beats config");

        // A leading ~/ expands to $HOME (workflow.yml can say ~/.rigger/tmp).
        if let Ok(home) = std::env::var("HOME") {
            let tilde = scratch_root(&repo_path, "~/.rigger-scratch-test", None);
            assert_eq!(tilde, format!("{home}/.rigger-scratch-test"));
            let _ = std::fs::remove_dir_all(tilde);
        }
    }

    #[test]
    fn sweep_terminal_removes_merged_worktrees_and_keeps_inflight_ones() {
        // Gap 14 maintenance: a worktree whose branch is already an ancestor of the
        // run branch serves no in-flight unit and is swept; an unmerged branch is a
        // live checkpoint and must be left alone. Only dirs under the scratch root
        // are considered.
        let repo = init_repo();
        let repo_path = repo.path().to_str().unwrap().to_string();
        run_git(&repo_path, &["checkout", "-b", "rigger-run"]).unwrap();
        let root = scratch_root(&repo_path, "", None);

        // Terminal: branch created off the run branch, never advanced (ancestor).
        let done_dir = format!("{root}/rigger-wt-done");
        Worktree::create(&repo_path, &done_dir, "rigger/u/done").unwrap();

        // In-flight: branch carries a commit the run branch does not have.
        let live_dir = format!("{root}/rigger-wt-live");
        let live = Worktree::create(&repo_path, &live_dir, "rigger/u/live").unwrap();
        std::fs::write(std::path::Path::new(&live_dir).join("wip.txt"), "wip\n").unwrap();
        live.commit("rigger: in-flight").unwrap();

        let removed = sweep_terminal(&repo_path, &root, "rigger-run").unwrap();
        assert_eq!(removed, 1, "exactly the terminal worktree is swept");
        assert!(
            !std::path::Path::new(&done_dir).exists(),
            "the merged/never-advanced worktree is gone"
        );
        assert!(
            std::path::Path::new(&live_dir).join("wip.txt").exists(),
            "the in-flight worktree is untouched"
        );
    }

    #[test]
    fn sweep_terminal_reclaims_a_crash_left_terminal_units_per_unit_build_cache() {
        // Gap 19 CRASH-recovery path: a step process killed before it reached
        // `Worktree::remove` leaves its unit worktree STILL REGISTERED, so the graceful
        // reclamation never ran and its sibling per-unit build cache (`cargo-target-<slug>`)
        // is dead weight on disk. When the next step's sweep removes that still-registered
        // TERMINAL worktree it must also reclaim the sibling cache; an IN-FLIGHT unit's cache
        // (its worktree is kept) must be left untouched. (The DOMINANT graceful path, where
        // `Worktree::remove` reclaims the cache directly, is pinned by
        // `worktree_remove_reclaims_the_sibling_per_unit_cache`.)
        let repo = init_repo();
        let repo_path = repo.path().to_str().unwrap().to_string();
        run_git(&repo_path, &["checkout", "-b", "rigger-run"]).unwrap();
        let root = scratch_root(&repo_path, "", None);

        // Terminal unit: `rigger-wt-done` with its sibling `cargo-target-done` cache.
        let done_dir = format!("{root}/{UNIT_WORKTREE_PREFIX}done");
        Worktree::create(&repo_path, &done_dir, "rigger/u/done").unwrap();
        let done_cache = format!("{root}/{UNIT_CACHE_PREFIX}done");
        std::fs::create_dir_all(&done_cache).unwrap();
        std::fs::write(std::path::Path::new(&done_cache).join("incremental"), "x").unwrap();

        // In-flight unit: its worktree carries an unmerged commit, so both the worktree
        // AND its sibling cache must survive the sweep.
        let live_dir = format!("{root}/{UNIT_WORKTREE_PREFIX}live");
        let live = Worktree::create(&repo_path, &live_dir, "rigger/u/live").unwrap();
        std::fs::write(std::path::Path::new(&live_dir).join("wip.txt"), "wip\n").unwrap();
        live.commit("rigger: in-flight").unwrap();
        let live_cache = format!("{root}/{UNIT_CACHE_PREFIX}live");
        std::fs::create_dir_all(&live_cache).unwrap();

        let removed = sweep_terminal(&repo_path, &root, "rigger-run").unwrap();
        assert_eq!(removed, 1, "exactly the terminal unit worktree is swept");
        assert!(
            !std::path::Path::new(&done_cache).exists(),
            "the swept unit's per-unit build cache must be removed alongside its worktree"
        );
        assert!(
            std::path::Path::new(&live_cache).exists(),
            "an in-flight unit's build cache must be left untouched"
        );
    }

    #[test]
    fn worktree_remove_reclaims_the_sibling_per_unit_cache() {
        // Gap 19 DOMINANT graceful path: `Worktree::remove` is what the conductor's
        // `run_stage` calls to tear a unit's worktree down at stage-end (on integrate / park
        // / err). It must reclaim the unit's sibling per-unit build cache
        // (`cargo-target-<slug>`, a plain dir git never tracks) WITH the worktree, or every
        // gracefully-terminated unit leaks a multi-gigabyte cache. A review worktree
        // (`rigger-review-*`) owns no such sibling, so removing it must NOT disturb an
        // unrelated sibling dir.
        let repo = init_repo();
        let repo_path = repo.path().to_str().unwrap().to_string();
        let root = scratch_root(&repo_path, "", None);

        // A unit worktree with its sibling per-unit cache populated (as a real gate build).
        let unit_dir = format!("{root}/{UNIT_WORKTREE_PREFIX}graceful");
        let unit = Worktree::create(&repo_path, &unit_dir, "rigger/u/graceful").unwrap();
        let unit_cache = format!("{root}/{UNIT_CACHE_PREFIX}graceful");
        std::fs::create_dir_all(&unit_cache).unwrap();
        std::fs::write(std::path::Path::new(&unit_cache).join("built.rlib"), "x").unwrap();

        // A review worktree owns no `cargo-target-*` sibling; removing it must NOT touch an
        // unrelated cache dir that happens to sit under the same scratch root.
        let review_dir = format!("{root}/rigger-review-panel-0");
        let review = Worktree::create(&repo_path, &review_dir, "rigger/rev/panel-0").unwrap();
        let bystander = format!("{root}/{UNIT_CACHE_PREFIX}unrelated");
        std::fs::create_dir_all(&bystander).unwrap();

        unit.remove().unwrap();
        assert!(
            !std::path::Path::new(&unit_dir).exists(),
            "the unit worktree is gone after remove()"
        );
        assert!(
            !std::path::Path::new(&unit_cache).exists(),
            "removing the unit worktree must reclaim its sibling per-unit cache, leaked at {unit_cache}"
        );

        review.remove().unwrap();
        assert!(
            std::path::Path::new(&bystander).exists(),
            "removing a review worktree (which owns no per-unit cache) must not touch an unrelated cache dir"
        );
    }

    #[test]
    fn unit_cache_sibling_maps_a_unit_worktree_to_its_cache_and_ignores_the_rest() {
        // The single derivation authority (Gap 19): a `rigger-wt-<slug>` unit worktree maps to
        // its `cargo-target-<slug>` sibling under the SAME parent; anything that is not a unit
        // worktree - a `rigger-review-*` review worktree, the shared `cargo-target` dir, or the
        // empty worktree-less path - owns no per-unit cache and maps to None (so its gate
        // inherits the shared target and nothing tries to reclaim a cache it never had).
        assert_eq!(
            unit_cache_sibling("/scratch/rigger-wt-unit-7"),
            Some("/scratch/cargo-target-unit-7".to_string())
        );
        assert_eq!(unit_cache_sibling("/scratch/rigger-review-panel-0"), None);
        assert_eq!(unit_cache_sibling("/scratch/cargo-target"), None);
        assert_eq!(unit_cache_sibling(""), None);
    }

    #[test]
    fn create_adopts_a_branch_still_checked_out_in_a_prior_processes_worktree() {
        // Step-process disposability (Gap 12): a killed `rigger step` leaves its
        // worktree REGISTERED with the branch checked out. A later process derives a
        // DIFFERENT dir for the same branch; git refuses a second checkout, so
        // `create` must ADOPT the surviving registration (returning ITS dir with the
        // committed work present) instead of failing - and when the registered dir
        // was deleted out from under git, it must prune and re-create.
        let repo = init_repo();
        let repo_path = repo.path().to_str().unwrap().to_string();
        let branch = "rigger/u/unit-adopt";

        // Process 1: create, commit, and do NOT remove - the process "died".
        let dir1 = std::env::temp_dir().join(format!("rigger-wt-{}", uuid::Uuid::new_v4()));
        let wt1 = Worktree::create(&repo_path, dir1.to_str().unwrap(), branch).unwrap();
        std::fs::write(dir1.join("inflight.txt"), "wave-1 work\n").unwrap();
        wt1.commit("rigger: in-flight work").unwrap();

        // Process 2: same branch, different dir. Must ADOPT dir1, not fail.
        let dir2 = std::env::temp_dir().join(format!("rigger-wt-{}", uuid::Uuid::new_v4()));
        let wt2 = Worktree::create(&repo_path, dir2.to_str().unwrap(), branch).unwrap();
        assert_eq!(
            wt2.dir,
            dir1.to_str().unwrap(),
            "create adopts the surviving registration's dir rather than colliding"
        );
        assert!(
            std::path::Path::new(&wt2.dir).join("inflight.txt").exists(),
            "the adopted worktree carries the in-flight committed work"
        );

        // Process 3: the registered dir vanishes without deregistration (a temp
        // cleaner). create must prune the stale registration and re-create at the
        // requested dir, with the branch's committed work checked out.
        std::fs::remove_dir_all(&dir1).unwrap();
        let dir3 = std::env::temp_dir().join(format!("rigger-wt-{}", uuid::Uuid::new_v4()));
        let wt3 = Worktree::create(&repo_path, dir3.to_str().unwrap(), branch).unwrap();
        assert_eq!(
            wt3.dir,
            dir3.to_str().unwrap(),
            "a stale registration is pruned and the requested dir is used"
        );
        assert!(
            dir3.join("inflight.txt").exists(),
            "the re-created worktree checks out the branch's committed work"
        );
        wt3.remove().unwrap();
    }

    #[test]
    fn create_heals_a_leftover_dir_at_the_deterministic_path() {
        // Resume self-heal (Gap 12, spec 06:48): with a DETERMINISTIC dir, a SIGKILL mid
        // `git worktree add` can leave a POPULATED dir at the fixed path that is NOT a
        // registered worktree, while the unit's durable BRANCH survives as a checkpoint.
        // The old per-process-uuid design made this collision IMPOSSIBLE; determinism must
        // not trade self-healing for a permanent wedge. `create` must REMOVE the
        // unregistered leftover and check the branch out afresh - never hard-fail
        // `git worktree add` (exit 128) on every subsequent resume that re-derives the same
        // path (adv-u4det-leftover-hardfail-confirmed-nonselfhealing).
        let repo = init_repo();
        let repo_path = repo.path().to_str().unwrap().to_string();
        run_git(&repo_path, &["checkout", "-b", "rigger-run"]).unwrap();
        let root = scratch_root(&repo_path, "", None);
        let branch = "rigger/u/unit-leftover";
        let dir = format!("{root}/rigger-wt-unit-leftover");

        // Establish the durable branch checkpoint with committed work, then remove the
        // worktree dir (registration gone) - the branch ref survives.
        let wt1 = Worktree::create(&repo_path, &dir, branch).unwrap();
        std::fs::write(
            std::path::Path::new(&dir).join("carried.txt"),
            "checkpoint\n",
        )
        .unwrap();
        wt1.commit("rigger: checkpoint work").unwrap();
        wt1.remove().unwrap();
        assert!(
            !std::path::Path::new(&dir).exists(),
            "precondition: the deterministic dir is gone after remove"
        );
        assert!(
            Worktree::branch_has_work(&repo_path, branch),
            "precondition: the durable branch still carries the checkpoint work"
        );

        // Plant a POPULATED leftover at the deterministic path that is NOT a registered
        // worktree - exactly the residue a SIGKILL mid `worktree add` leaves behind.
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(std::path::Path::new(&dir).join("leftover.txt"), "torn\n").unwrap();
        assert!(
            !worktree_on_branch(&dir, branch),
            "precondition: the leftover is not this branch's registered worktree (fast path can't adopt)"
        );
        assert!(
            registered_worktree_for(&repo_path, branch).is_none(),
            "precondition: no worktree is registered for the branch (fallback can't adopt)"
        );

        // `create` must HEAL rather than hard-fail exit 128.
        let wt2 = Worktree::create(&repo_path, &dir, branch)
            .expect("create must self-heal a leftover dir, not wedge on `git worktree add`");
        assert_eq!(
            wt2.dir, dir,
            "the healed worktree uses the requested deterministic dir"
        );
        assert!(
            std::path::Path::new(&dir).join("carried.txt").exists(),
            "the healed worktree checks out the branch's committed checkpoint work"
        );
        assert!(
            !std::path::Path::new(&dir).join("leftover.txt").exists(),
            "the unregistered leftover residue is removed, not merged into the fresh checkout"
        );
        wt2.remove().unwrap();
    }

    #[test]
    fn create_adopts_the_deterministic_dir_via_a_path_lookup() {
        // Gap 12 (spec 06:48): with a DETERMINISTIC dir, a second process computes the
        // SAME path for the branch. `create` must adopt that existing worktree by a
        // direct PATH LOOKUP on the requested dir (it is already this branch's worktree)
        // - never failing on the double-checkout, never needing to parse the porcelain
        // worktree list to discover where the branch lives. The adopted worktree carries
        // the prior process's committed work.
        let repo = init_repo();
        let repo_path = repo.path().to_str().unwrap().to_string();
        run_git(&repo_path, &["checkout", "-b", "rigger-run"]).unwrap();
        let root = scratch_root(&repo_path, "", None);
        let branch = "rigger/u/unit-det";
        // Deterministic dir - the same string both processes derive, no uuid.
        let dir = format!("{root}/rigger-wt-unit-det");

        // Process 1: create the deterministic worktree and commit work.
        let wt1 = Worktree::create(&repo_path, &dir, branch).unwrap();
        std::fs::write(std::path::Path::new(&dir).join("work.txt"), "det\n").unwrap();
        wt1.commit("rigger: process 1 work").unwrap();

        // Process 2: SAME deterministic dir + branch. It must adopt the existing dir (a
        // path lookup), returning that exact dir with the committed work present.
        let wt2 = Worktree::create(&repo_path, &dir, branch).unwrap();
        assert_eq!(
            wt2.dir, dir,
            "create adopts the requested deterministic dir directly"
        );
        assert!(
            std::path::Path::new(&wt2.dir).join("work.txt").exists(),
            "the adopted deterministic worktree carries the committed work"
        );
        wt2.remove().unwrap();
    }

    #[test]
    fn worktree_on_branch_matches_only_this_branchs_own_checkout() {
        // The fast-path adoption arm (Gap 12) is a PATH LOOKUP on the dir's OWN HEAD, not a
        // `git worktree list` porcelain parse. Pin the predicate directly so a mutation of
        // the fast path is caught (the flagship adopt test alone stays green with the fast
        // path deleted, because the porcelain fallback adopts the same registered dir -
        // adv-u4det-adopt-test-nondiscriminating). It must be TRUE only for a dir that IS
        // this branch's worktree, and FALSE for an absent dir, a bare non-worktree dir, and
        // a worktree on a DIFFERENT branch.
        let repo = init_repo();
        let repo_path = repo.path().to_str().unwrap().to_string();
        run_git(&repo_path, &["checkout", "-b", "rigger-run"]).unwrap();
        let root = scratch_root(&repo_path, "", None);
        let branch = "rigger/u/unit-fastpath";
        let dir = format!("{root}/rigger-wt-unit-fastpath");

        // Absent dir: no worktree to adopt.
        assert!(
            !worktree_on_branch(&dir, branch),
            "an absent dir is not a worktree on the branch"
        );

        // A bare, populated NON-worktree dir under the repo: its HEAD walks UP to the parent
        // repo's branch (rigger-run), not `branch`, so the fast path must NOT adopt it - this
        // is exactly the leftover the fallback must defend against.
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(std::path::Path::new(&dir).join("x.txt"), "y\n").unwrap();
        assert!(
            !worktree_on_branch(&dir, branch),
            "a bare leftover dir (HEAD resolves to the parent repo) is not this branch's worktree"
        );
        std::fs::remove_dir_all(&dir).unwrap();

        // The real worktree on the branch: matched by path lookup.
        let wt = Worktree::create(&repo_path, &dir, branch).unwrap();
        assert!(
            worktree_on_branch(&dir, branch),
            "the dir that IS this branch's worktree matches by its own HEAD"
        );

        // A worktree checked out on a DIFFERENT branch is not matched for `branch`.
        let other_dir = format!("{root}/rigger-wt-other");
        let other = Worktree::create(&repo_path, &other_dir, "rigger/u/other").unwrap();
        assert!(
            !worktree_on_branch(&other_dir, branch),
            "a worktree on another branch does not match this branch's path lookup"
        );
        wt.remove().unwrap();
        other.remove().unwrap();
    }

    #[test]
    fn discard_resets_a_throwaway_review_worktree_to_the_current_head() {
        // adv-u4det-review-adopt-staleness: a review worktree's deterministic branch/dir
        // must never ADOPT a stale checkpoint. A review step that crashed after creating the
        // throwaway worktree leaves the branch pinned at the OLD base HEAD; a naive `create`
        // would adopt it and review STALE code once the base advanced. `discard` + `create`
        // must instead tear down the leftover and recreate off the CURRENT HEAD.
        let repo = init_repo();
        let repo_path = repo.path().to_str().unwrap().to_string();
        run_git(&repo_path, &["checkout", "-b", "rigger-run"]).unwrap();
        let root = scratch_root(&repo_path, "", None);
        let branch = "rigger/review/stage-0";
        let dir = format!("{root}/rigger-review-stage-0");

        // A prior review step created the throwaway worktree off the base HEAD, then CRASHED
        // (no cleanup): the branch + dir survive, pinned at the OLD head.
        let stale = Worktree::create(&repo_path, &dir, branch).unwrap();
        let old_head = git(&dir, &["rev-parse", "HEAD"])
            .unwrap()
            .trim()
            .to_string();
        drop(stale); // the Rust struct is gone but the worktree registration + dir survive.

        // The base advances (a sibling unit integrates onto the run branch).
        run_git(
            &repo_path,
            &["commit", "--allow-empty", "-q", "-m", "sibling integrated"],
        )
        .unwrap();
        let new_head = run_git(&repo_path, &["rev-parse", "HEAD"])
            .unwrap()
            .trim()
            .to_string();
        assert_ne!(
            old_head, new_head,
            "precondition: the base advanced past the stale review worktree"
        );

        // The resumed review step discards the stale scaffolding and recreates off HEAD.
        Worktree::discard(&repo_path, &dir, branch).unwrap();
        assert!(
            !branch_exists(&repo_path, branch),
            "discard deletes the throwaway review branch"
        );
        let fresh = Worktree::create(&repo_path, &dir, branch).unwrap();
        let fresh_head = git(&fresh.dir, &["rev-parse", "HEAD"])
            .unwrap()
            .trim()
            .to_string();
        assert_eq!(
            fresh_head, new_head,
            "the recreated review worktree reflects the CURRENT base HEAD, not the stale one"
        );
        fresh.remove().unwrap();
        Worktree::delete_branch(&repo_path, branch).unwrap();
    }

    #[test]
    fn ensure_run_branch_creates_off_base_and_checks_it_out() {
        // Absent run branch + a base that resolves: create the run branch off the base,
        // check it out, and report CreatedFromBase.
        let repo = init_repo();
        let p = repo.path().to_str().unwrap().to_string();
        let default = current_branch(&p).expect("init_repo leaves a named branch checked out");

        let setup = Worktree::ensure_run_branch(&p, "rigger-run", &default).unwrap();
        assert_eq!(setup, RunBranchSetup::CreatedFromBase);
        assert_eq!(
            current_branch(&p).as_deref(),
            Some("rigger-run"),
            "ensure_run_branch must check out the run branch it creates"
        );
        assert!(branch_exists(&p, "rigger-run"));
        assert_eq!(
            run_git(&p, &["rev-parse", "rigger-run"]).unwrap().trim(),
            run_git(&p, &["rev-parse", &default]).unwrap().trim(),
            "a freshly-created run branch starts at the base commit"
        );
    }

    #[test]
    fn ensure_run_branch_reuses_and_never_resets_an_existing_run_branch() {
        // An existing run branch is the run's durable anchor: a re-ensure REUSES it (and
        // checks it back out if the operator switched away), NEVER resets it, so a prior
        // step's integrated work survives and the run CONTINUES from it. This is the
        // in-place mechanism by which a later step builds on the accumulated run - not a
        // re-anchor to a new base (which would orphan the integrated units).
        let repo = init_repo();
        let p = repo.path().to_str().unwrap().to_string();
        let default = current_branch(&p).expect("init_repo leaves a named branch checked out");
        Worktree::ensure_run_branch(&p, "rigger-run", &default).unwrap();

        // A prior step integrates a unit onto the run branch.
        run_git(
            &p,
            &["commit", "--allow-empty", "-q", "-m", "integrated unit"],
        )
        .unwrap();
        let integrated_tip = run_git(&p, &["rev-parse", "rigger-run"])
            .unwrap()
            .trim()
            .to_string();

        // Re-ensure from another branch, even pointing base ELSEWHERE: it must reuse the
        // existing run branch (report Reused), check it back out, and preserve the tip -
        // base is deliberately ignored once the run branch exists.
        run_git(&p, &["checkout", "-q", &default]).unwrap();
        let setup = Worktree::ensure_run_branch(&p, "rigger-run", &default).unwrap();
        assert_eq!(setup, RunBranchSetup::Reused);
        assert_eq!(
            current_branch(&p).as_deref(),
            Some("rigger-run"),
            "a re-ensure checks the existing run branch back out"
        );
        assert_eq!(
            run_git(&p, &["rev-parse", "rigger-run"]).unwrap().trim(),
            integrated_tip,
            "reuse must NOT reset the run branch - a prior step's integration is preserved"
        );
    }

    #[test]
    fn ensure_run_branch_creates_off_head_when_base_unresolvable() {
        // BLOCKER regression: a repo whose base ref (e.g. the default origin/main) does
        // NOT resolve - no remote, master-default, or pre-fetch. ensure_run_branch must
        // NOT no-op (which would leave HEAD on the operator's branch and let the conductor
        // branch/merge units directly onto it). It must create the run branch off the
        // current HEAD, check it out, and report CreatedFromHead so isolation is always
        // established on the native path.
        let repo = init_repo();
        let p = repo.path().to_str().unwrap().to_string();
        let head_before = run_git(&p, &["rev-parse", "HEAD"])
            .unwrap()
            .trim()
            .to_string();

        let setup = Worktree::ensure_run_branch(&p, "rigger-run", "origin/does-not-exist").unwrap();

        assert_eq!(setup, RunBranchSetup::CreatedFromHead);
        assert!(
            branch_exists(&p, "rigger-run"),
            "an unresolvable base must still create the run branch (off HEAD), not no-op"
        );
        assert_eq!(
            current_branch(&p).as_deref(),
            Some("rigger-run"),
            "the HEAD-anchored run branch must be checked out so units branch off it"
        );
        assert_eq!(
            run_git(&p, &["rev-parse", "rigger-run"]).unwrap().trim(),
            head_before,
            "the fallback run branch is anchored on the HEAD it was created from"
        );
    }

    #[test]
    fn run_branch_based_on_release_target_contains_exactly_the_runs_work() {
        // Spec 38, criterion 2 (run-branch basing): a run branch created off the release
        // target (base) yields a clean, APPLICABLE PR diff - base..run-branch is EXACTLY the
        // run's integrated commits and base is an ANCESTOR of the run branch, never the
        // history disjoint from the base that a PR refuses to apply.
        let repo = init_repo();
        let p = repo.path().to_str().unwrap().to_string();
        let base = current_branch(&p).expect("init_repo leaves a named branch checked out");
        let base_tip = run_git(&p, &["rev-parse", &base]).unwrap().trim().to_string();

        // Anchor the run branch on the release target.
        let setup = Worktree::ensure_run_branch(&p, "rigger-run", &base).unwrap();
        assert_eq!(setup, RunBranchSetup::CreatedFromBase);

        // Two units integrate onto the run branch (empty commits stand in for merged work).
        run_git(
            &p,
            &["commit", "--allow-empty", "-q", "-m", "integrate unit A"],
        )
        .unwrap();
        let a = run_git(&p, &["rev-parse", "HEAD"]).unwrap().trim().to_string();
        run_git(
            &p,
            &["commit", "--allow-empty", "-q", "-m", "integrate unit B"],
        )
        .unwrap();
        let b = run_git(&p, &["rev-parse", "HEAD"]).unwrap().trim().to_string();

        // base..run-branch is EXACTLY the two integrated commits (newest first) - none of the
        // base's own history leaks into the run's PR range.
        let range = run_git(&p, &["rev-list", &format!("{base}..rigger-run")]).unwrap();
        let commits: Vec<&str> = range
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .collect();
        assert_eq!(
            commits,
            vec![b.as_str(), a.as_str()],
            "base..run-branch must be exactly the run's integrated commits"
        );

        // The release target is an ANCESTOR of the run branch, so a PR from the run branch to
        // the base applies cleanly (the disjoint-history failure this criterion prevents).
        assert!(
            run_git(&p, &["merge-base", "--is-ancestor", &base_tip, "rigger-run"]).is_ok(),
            "the release target must be an ancestor of the run branch (an applicable PR diff)"
        );
    }

    #[test]
    fn changed_files_unquotes_paths_with_spaces() {
        let repo = init_repo();
        let repo_path = repo.path().to_str().unwrap().to_string();
        let wt_path = std::env::temp_dir().join(format!("rigger-wt-{}", uuid::Uuid::new_v4()));
        let wt = Worktree::create(&repo_path, wt_path.to_str().unwrap(), "rigger/spaces").unwrap();

        // The plain --porcelain form C-quotes this to `"a file.txt"`; the -z form
        // must hand back the real, unquoted path.
        std::fs::write(wt_path.join("a file.txt"), "work\n").unwrap();
        assert_eq!(wt.changed_files().unwrap(), ["a file.txt"]);
        wt.remove().unwrap();
    }

    #[test]
    fn remove_reaps_a_process_rooted_inside_the_worktree_and_spares_one_outside() {
        // spec 23 done-when: tearing a worktree down first REAPS every process whose cwd is
        // inside it (SIGTERM then SIGKILL after a grace), so nothing outlives the removed dir -
        // proven with a child that IGNORES SIGTERM (only the SIGKILL escalation can end it). A
        // second child rooted OUTSIDE the worktree, at the repo root, is proven STILL alive:
        // the reap is scoped strictly to the dir being removed and never reaches the repo root.
        let repo = init_repo();
        let repo_path = repo.path().to_str().unwrap().to_string();
        // Mirror production: the worktree lives under `<repo>/.rigger/tmp/`.
        let scratch = repo.path().join(".rigger").join("tmp");
        std::fs::create_dir_all(&scratch).unwrap();
        let wt_dir = scratch.join("rigger-wt-reaptest");
        let wt =
            Worktree::create(&repo_path, wt_dir.to_str().unwrap(), "rigger/u/reaptest").unwrap();

        // Inside: a process rooted in the worktree that ignores SIGTERM, so only the SIGKILL
        // escalation reaps it. Outside: a plain sleeper rooted at the repo root.
        let mut inside = Command::new("sh")
            .arg("-c")
            .arg("trap '' TERM; while :; do sleep 1; done")
            .current_dir(&wt_dir)
            .spawn()
            .expect("spawn inside child");
        let mut outside = Command::new("sleep")
            .arg("300")
            .current_dir(repo.path())
            .spawn()
            .expect("spawn outside child");

        // Wait until the inside child is actually rooted in the worktree before tearing down.
        let detected = (0..200).any(|_| {
            if crate::reap::processes_rooted_under(&wt_dir)
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
            "precondition: the inside child is rooted in the worktree"
        );

        wt.remove().unwrap();

        // The inside child is no longer alive (reaped before the dir was removed).
        let inside_died = (0..200).any(|_| {
            if matches!(inside.try_wait(), Ok(Some(_))) {
                return true;
            }
            std::thread::sleep(std::time::Duration::from_millis(25));
            false
        });
        // The outside child is still alive - the safety boundary held.
        let outside_alive = matches!(outside.try_wait(), Ok(None));

        // Clean up the fixtures before asserting so a failure never leaks processes.
        let _ = outside.kill();
        let _ = outside.wait();
        if !inside_died {
            let _ = inside.kill();
            let _ = inside.wait();
        }

        assert!(
            inside_died,
            "a process rooted inside the worktree must be reaped by remove() (SIGTERM then SIGKILL)"
        );
        assert!(
            outside_alive,
            "a process rooted at the repo root (OUTSIDE the worktree) must survive - the safety boundary"
        );
        assert!(
            !wt_dir.exists(),
            "the worktree dir is removed after its rooted processes are reaped"
        );
    }
}
