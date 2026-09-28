//! Isolate a unit of work in a throwaway git worktree branched from HEAD, so
//! parallel units cannot conflict on the filesystem while the event stream stays
//! the shared decision channel. Integrate commits the agent's changes and merges
//! the branch into the base; the work lands.

use crate::eventstore::Event;
use crate::spawn::SpawnEvent;

pub use rigger_domain::worktree::Error;

/// Which `git diff` range [`Worktree::diff_names`] compares `from` against `HEAD` with.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiffMode {
    /// The committed three-dot diff (`from...HEAD`), anchored on the merge-base of `from`
    /// and `HEAD` - the shared primitive [`Worktree::changed_since_base`] calls with `from` =
    /// the run branch's CURRENT tip, and the one a resumed
    /// `RunCtx::integrate_and_emit` uses to recompute the SAME fact against an OLDER `from`
    /// (round 4, spec 88 criterion 1) - the tip a durably-recorded landing-intent named -
    /// when `changed_since_base` itself would see nothing: by the time that resume runs, the
    /// run branch has ALREADY fast-forward-absorbed everything this worktree has, so a fresh
    /// diff against its CURRENT tip is empty even though real, unrecorded work landed.
    ///
    /// Three-dot is the RIGHT choice there: `from` is a branch that may have DIVERGED (other
    /// units merged into it meanwhile), so anchoring on its merge-base with `HEAD` reports
    /// only changes new to THIS branch, never unrelated commits that landed on `from` in the
    /// meantime. Do not use it for a same-branch residue check - see [`DiffMode::Direct`].
    MergeBase,
    /// The direct two-dot diff (`from..HEAD`, NOT merge-base-anchored) - a straight
    /// tree-to-tree comparison of the two shas, regardless of whether either is an ancestor
    /// of the other. This is for naming residue on `from`'s OWN branch
    /// (`guard_review_round_tree`, spec 103 criterion 6): `from` there is `round_start_sha`, a
    /// sha this SAME worktree's tip already passed through, so the files that actually
    /// differ are whatever the current tree adds on top of it - even in the non-ancestor
    /// shape (a reviewer's own tooling force-pushing or amending the branch is exactly the
    /// protocol break this guard exists to catch), where a three-dot diff would instead
    /// anchor on their merge-base and pull in unrelated files that already differed at
    /// `from`, over-reporting the round's own residue
    /// (sdet-u103c6-committed-diff-names-triple-dot-non-ancestor).
    Direct,
}

/// An isolated git worktree for one unit of work.
pub struct Worktree {
    pub dir: String,
    pub branch: String,
    repo: String,
    /// The scratch root [`Self::create`]'s caller independently resolved `dir` under
    /// (spec 79 round-2 fix, `arch-u79c1-reap-dir-before-removal-self-authorizes` /
    /// `sdet-u79c1-authorized-root-tautology`, both UPHELD): carried on the instance so
    /// [`Self::ensure_present`]'s and [`Self::remove`]'s later reap-before-removal calls
    /// reuse the SAME caller-supplied authority `create` was given, rather than
    /// re-deriving one from `dir`'s own filesystem position (`dir.parent()` trivially
    /// contains `dir` after canonicalization, which made the prior shape's containment
    /// check an unconditional pass - see [`reap_dir_before_removal`]'s doc comment).
    /// Empty for a caller with no such root to supply (e.g. a test scaffold that never
    /// exercises the reap boundary), in which case every reap this instance drives is a
    /// no-op, exactly as if no root had ever authorized it.
    authorized_root: String,
    /// Serializes [`Self::ensure_present`]'s call into [`Self::create`]'s mutation path
    /// (spec 64 criterion 3, round 5: adv-u3c3r4-concurrent-lens-ensure-present-races-
    /// worktree-create, sdet-u3c3r4-concurrent-lenses-race-ensure-present-on-the-same-
    /// worktree, both UPHELD). The review tier's lens fan-out shares ONE `&Worktree`
    /// across N real OS threads (`run_review_agents_concurrently`), and each calls
    /// `ensure_present` independently before its own spawn - `create`'s own doc comment
    /// above states its `git worktree add`/adopt path does not support concurrent
    /// callers. This lock is per-WORKTREE (not per-run), so it serializes only concurrent
    /// re-asserts of THIS SAME instance - it never adds contention across different units
    /// racing in `run_batch`; that WIDER admin-directory race is `repo_admin_lock`'s
    /// (per-repository, spec 103 criterion 4), a separate lock this instance-scoped one
    /// composes with rather than duplicates. `()` payload: only mutual exclusion is needed.
    reassert_mu: std::sync::Mutex<()>,
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

/// What [`Worktree::land`] did with the run branch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LandOutcome {
    /// The run branch fast-forwarded to the unit's branch: the unit is on the run branch.
    Landed,
    /// The run branch moved after the worktree merge, so a fast-forward was impossible; the
    /// repo is untouched. Merge the new tip into the worktree and land again.
    TipMoved,
    /// The fast-forward was refused because LOCAL content in the repo checkout - untracked,
    /// or a tracked file dirtied but never committed - sits at one of these paths and would
    /// be clobbered (spec 103, criterion 8: A REFUSED LANDING NAMES ITS PATHS). The repo is
    /// untouched, exactly like [`Self::TipMoved`]; unlike a genuine content conflict this
    /// never touches the unit's own branch either - the paths are sorted and deduplicated.
    /// The caller decides what the blocked local content means (operator debris to clear,
    /// or content some unit branch already carries, so nothing is actually lost).
    Blocked(Vec<String>),
}

/// The outcome of [`Worktree::merge_into_worktree`] (spec 88, criterion 1 round 4, TABLE row
/// 1: "conflict detection"). This is the FRONT HALF of what a single pre-round-4 `integrate`
/// method used to do in one call - merging the run branch's tip into the unit's own worktree
/// and either leaving conflict markers or finishing with a committed, ready-to-land branch -
/// split out so the
/// caller (`integrate_and_emit`) can bracket the durable row-1 record around exactly this
/// mutation and the row-4 record around the separate [`Worktree::land`] call, instead of both
/// rows sharing one opaque function call with no seam in between.
pub enum MergeOutcome {
    /// The merge (or an already-resolved worktree) is fully committed on the unit's OWN
    /// branch and ready to land via [`Worktree::land`]. Empty for a true no-op stage (nothing
    /// to merge or land at all) - the caller must not call `land` in that case.
    Ready(String),
    /// The merge CONFLICTED: the sorted, deduplicated list of conflicting paths, read
    /// directly from the worktree (never the event log) so a crash-resumed re-check of an
    /// already-in-progress merge answers identically without re-invoking `git merge` (which
    /// git would refuse).
    Conflict(Vec<String>),
}

/// The outcome of [`Worktree::cherry_pick_onto_run_branch`] (spec 88, criterion 4 - PLAN
/// AMENDMENTS LAND): landing a `produces` stage's own `specs/`-only commits onto the run
/// branch, so the next plan-critique worktree (branched from the run branch) sees them.
pub enum CherryPickOutcome {
    /// Every named commit applied cleanly, in order: the shas AS THEY LANDED on the run
    /// branch. Usually distinct new commit objects (cherry-pick mints a fresh committer
    /// timestamp), but NOT always - a cherry-pick onto its own original parent, applied
    /// within the same committer-timestamp second, reproduces the byte-identical commit
    /// object (same tree, parent, author, and now-matching committer), so the landed sha
    /// can equal the original. Either way this is what is actually reachable on the run
    /// branch right now - the caller records THIS, never the pre-landing sha it read from
    /// [`Self::commits_since_base`].
    Picked(Vec<String>),
    /// The cherry-pick CONFLICTED partway through and the WHOLE sequence was ABORTED
    /// (git's cherry-pick sequencer unwinds every commit it had already applied this
    /// call), so the run branch is left EXACTLY as it was - the textual-conflict sibling
    /// of [`MergeOutcome::Conflict`]. Carries the conflict detail for the caller's
    /// remediation feedback.
    Conflict(String),
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
    ///
    /// `authorized_root` is the scratch root the CALLER independently resolved `dir`
    /// under (the same value [`crate::worktree::scratch_root_from_env`] or an
    /// equivalent caller-side authority already produced to build `dir` itself) - spec
    /// 79 round-2 fix. It gates the self-heal reap below via [`reap_dir_before_removal`]
    /// and is carried on the returned instance for [`Self::ensure_present`] and
    /// [`Self::remove`] to reuse later; it is NEVER re-derived here from `dir`'s own
    /// filesystem position (`dir.parent()` trivially contains `dir`, which is exactly
    /// the tautological self-authorization this fix removes). Pass `""` when the
    /// caller has no such root (the reap becomes a no-op, matching the
    /// best-effort/never-fails contract every other reap call site already has).
    pub fn create(
        repo: &str,
        dir: &str,
        branch: &str,
        authorized_root: &str,
    ) -> Result<Self, Error> {
        // Serialize this WHOLE call - heal scan through the `git worktree add` below -
        // against every other in-process admin-directory mutation this process makes for
        // the SAME repository (spec 103 criterion 4, widened at the checkin seam: see
        // `repo_admin_lock`'s doc comment for the race this closes and what it does not).
        let repo_lock = repo_admin_lock(repo);
        let _repo_guard = repo_lock.lock().unwrap();
        // SELF-HEAL before any `git worktree add` (spec 51): a lifecycle killed mid
        // `git worktree remove` can leave a corrupt admin entry (a zero-length `commondir`)
        // that makes EVERY add below hard-fail; prune the provably-corrupt entry first so
        // one crashed lifecycle can never permanently wedge the run. A healthy metadata dir
        // is a no-op, and a healthy registered worktree is never touched.
        heal_corrupt_worktree_admin(repo);
        ensure_scratch_root_cargo_config(dir);
        if branch_exists(repo, branch) {
            // FAST PATH - adoption by PATH LOOKUP (Gap 12, spec 06). The dir is now
            // DETERMINISTIC (derived from the unit id / stage+attempt, no per-process
            // uuid), so a resume - or a step that SUPERSEDES a prior one that died -
            // derives the SAME `dir` for this branch. If that dir already IS this branch's
            // worktree, adopt it directly - a check on the dir's own HEAD, with no
            // `git worktree list` porcelain parse and no re-`add` (which git refuses for a
            // branch already checked out). (This handles sequential resume/supersede, not
            // a true create-race for the SAME branch name: two INDEPENDENT `Worktree::create`
            // calls that both see one particular branch absent still race the underlying
            // `git worktree add -b` for THAT branch - rigger never asks two units to create
            // the same branch concurrently, so that shape is not a first-class case. The
            // WIDER admin-directory race - two units' own DIFFERENT worktrees within one
            // `run_batch`, whose heal scans and adds could interleave and corrupt each
            // other's admin entries - is now closed in-process by `repo_admin_lock` above
            // (spec 103 criterion 4); it does not cover a second SEPARATE process adding
            // worktrees against this same repository, which only the `locked`/grace-period
            // guards in `worktree_admin_is_corrupt` defend against.
            // [`Self::ensure_present`]'s OWN repeat calls on the SAME instance are a
            // different shape - N threads that already share one `&Worktree` - and that one
            // IS serialized, by `reassert_mu`.)
            if worktree_on_branch(dir, branch) {
                return Ok(Worktree {
                    dir: dir.to_string(),
                    branch: branch.to_string(),
                    repo: repo.to_string(),
                    authorized_root: authorized_root.to_string(),
                    reassert_mu: std::sync::Mutex::new(()),
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
                        authorized_root: authorized_root.to_string(),
                        reassert_mu: std::sync::Mutex::new(()),
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
                clear_worktree_dir(repo, dir, authorized_root)?;
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
            authorized_root: authorized_root.to_string(),
            reassert_mu: std::sync::Mutex::new(()),
        })
    }

    /// Re-assert THIS worktree exists on its branch, at the tip [`Self::create`] would
    /// hand out, right now (ensure-on-park, spec 64 criterion 3: defense in depth).
    ///
    /// `stage_worktree` (the conductor's caller) already guarantees the worktree exists
    /// exactly ONCE, at the top of a unit's `run_stage` call - but that single call can
    /// go on to reach a LATER spawn point (the review tier, after the gates run - real
    /// wall-clock time) in the SAME process. An out-of-band actor that deletes the
    /// worktree in that window - the historical fault this whole spec closes: an agent
    /// finding its assigned worktree gone at spawn - would otherwise hand the next spawn
    /// a `dir` string whose directory no longer exists. Calling this again immediately
    /// before every such LATER spawn closes that window with the SAME deterministic
    /// adopt-or-create machinery `stage_worktree`'s first call already uses, so it never
    /// deviates behavior for the common case: a worktree that is still exactly where it
    /// was left is the FAST `worktree_on_branch` path-lookup inside [`Self::create`], a
    /// cheap no-op.
    ///
    /// Never mutates the branch tip or discards commits - `Self::create`'s adopt path
    /// checks out the branch's CURRENT head exactly as it is; this only guarantees the
    /// DIR is present and checked out.
    ///
    /// Concurrent-caller safe (spec 64 criterion 3, round 5), UNLIKE a bare `Self::create`
    /// call: the review tier's lens fan-out shares ONE `&Worktree` across N real OS
    /// threads (`run_review_agents_concurrently`), each calling this independently right
    /// before its own spawn - so two threads can both find the dir gone at once. `reassert_
    /// mu` serializes this instance's calls into `Self::create`'s mutation path, so at most
    /// one thread actually runs `git worktree add`/adopt at a time; the rest either take
    /// the cheap no-op fast path once the winner has restored it, or (rare: the winner's
    /// OWN restore was itself raced out from under it) retry. Per-INSTANCE, not global - it
    /// never adds contention across a DIFFERENT unit's worktree.
    pub fn ensure_present(&self) -> Result<(), Error> {
        let _lock = self.reassert_mu.lock().unwrap();
        Worktree::create(&self.repo, &self.dir, &self.branch, &self.authorized_root)?;
        Ok(())
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

    /// Create a NEW branch ref `new_branch` pointing at `at_branch` (spec 88, ADOPTION
    /// KEYS ON THE CRITERION): a plain `git branch <new_branch> <at_branch>`, so
    /// `at_branch` itself is left completely untouched - a new ref, never a rename, so
    /// the prior unit's own branch name stays resolvable. `at_branch` is any git
    /// revision, not necessarily a branch name: since round 4 the conductor passes the
    /// exact sha it already read via [`branch_tip`] and recorded as durable provenance
    /// (rather than the moving branch name a second time), so the new ref lands on
    /// EXACTLY the commit the provenance record names even if the source branch moved
    /// in between. This is how the conductor seeds a FRESH unit's durable branch from a
    /// prior (differently-named) run's still un-integrated unit that served the same
    /// criterion: once this ref exists, [`Self::create`]'s ordinary adopt-by-path-lookup
    /// machinery reuses it exactly as it reuses this unit's own prior work on any other
    /// resume.
    ///
    /// Returns the new branch's tip sha (== `at_branch` resolved at the moment of
    /// creation - identical to the input when the caller already passed a sha). The
    /// caller is responsible for confirming `new_branch` does not already exist
    /// ([`branch_exists`]) - `git branch` refuses to clobber an existing ref, so a
    /// caller that races this against an already-started unit fails loudly rather than
    /// silently re-pointing a durable checkpoint.
    pub fn create_branch_at(
        repo: &str,
        new_branch: &str,
        at_branch: &str,
    ) -> Result<String, Error> {
        git(repo, &["branch", new_branch, at_branch])?;
        Ok(git(repo, &["rev-parse", new_branch])?.trim().to_string())
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
        // Rigger's own compensation-revert commit (d-checkin-rigger-own-commits-bypass-
        // hooks): the same class as the merge-into-worktree bookkeeping commits above -
        // machine provenance of a rollback, not a commit an agent or a person means to
        // make, so it bypasses hooks too.
        match run_git(repo, &["commit", "--no-edit", "--no-verify", "-m", message]) {
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

    /// Reset THIS worktree's branch HARD to `sha`, discarding only what [`Self::integrate`]
    /// itself added since `sha` - never a unit's genuinely reviewed prior work (spec 88,
    /// criterion 1: a real merge CONFLICT is resolved in place and never reaches this call at
    /// all; the unit's approved rounds stay exactly as they are). The caller passes the
    /// worktree's OWN tip from immediately before its `integrate` call, so this undoes exactly
    /// that call's abandoned merge attempt: a POST-MERGE re-gate going RED (spec 12, unit 5 -
    /// the merge was clean but semantically broken) or the implementer-respawn bound being
    /// exhausted with a real conflict still unresolved (spec 88, criterion 1's ONE
    /// attempt-charging fallback). Leaves the unit's branch clean (any in-progress merge is
    /// also aborted by the hard reset) for its next attempt. Resetting a branch checked out in
    /// its OWN worktree is allowed (unlike deleting it).
    pub fn reset_branch_to(&self, sha: &str) -> Result<(), Error> {
        if sha.is_empty() {
            return Ok(());
        }
        git(&self.dir, &["reset", "--hard", sha])?;
        Ok(())
    }

    /// Restores THIS worktree's TRACKED and UNTRACKED state to `sha` (spec 103, criterion
    /// 6): a review round's own tiers must never leave residue in the unit worktree - the
    /// review protocol tells every lens, adversary and adjudicator to reproduce a suspected
    /// failure in its OWN scratch worktree, never this one - so a worktree a round leaves
    /// dirty, or whose tip has moved off the sha it actually judged, is residue, never
    /// legitimate work. [`Self::reset_branch_to`] alone rewinds only tracked content; an
    /// untracked file dropped against protocol would otherwise survive the hard reset and
    /// keep the tree dirty for the caller's very next check, so this also runs `git clean`
    /// (respecting `.gitignore`, never `-x`) to clear it. The caller PROVES the worktree is
    /// dirty or its tip has moved, and records a lesson naming what, before calling this -
    /// it does not check either itself.
    pub fn restore_reviewed_sha(&self, sha: &str) -> Result<(), Error> {
        self.reset_branch_to(sha)?;
        git(&self.dir, &["clean", "-fd"])?;
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
    ///
    /// Also reclaims the dir's store-fence sibling (spec 70 criterion 3, u4 round 2 fix for
    /// `adv-u4c70r2-discard-path-leaks-review-fence-sibling`), via the SAME
    /// [`reclaim_cache_sibling`] authority [`Self::remove`]/[`sweep_terminal`]/
    /// [`reclaim_worktree_on_branch`] already call - mirroring the exact clear-then-reclaim
    /// sequence [`reclaim_worktree_on_branch`] uses. `discard` is the FOURTH teardown path
    /// (this doc comment's own crash-resume case, driven by `review_only_worktree` on every
    /// standalone-review-stage attempt): before this fix a fenced review worktree's
    /// `-store-fence` sibling - a live sqlite `events.db` a gate-spawned courier opened -
    /// survived every discard-then-recreate cycle, leaked forever on the operator's small
    /// scratch partition. A unit's durable worktree owns no fence sibling here (`discard` is
    /// never called on one), so this is a no-op on that path.
    ///
    /// `authorized_root` is the SAME caller-resolved scratch root [`Self::create`] takes
    /// (spec 79 round-2 fix) - it gates the reap-before-removal of both `dir` and its
    /// reclaimed siblings, never re-derived from `dir`'s own position. Pass `""` when the
    /// caller has no such root; the reap is then a no-op.
    pub fn discard(
        repo: &str,
        dir: &str,
        branch: &str,
        authorized_root: &str,
    ) -> Result<(), Error> {
        // Serialize this WHOLE call - both the dir-present `clear_worktree_dir` path and the
        // dir-absent `git worktree prune` fallback below - against every other in-process
        // admin-directory mutation this process makes for the SAME repository (spec 103
        // checkin round 4: a sibling unit's `Worktree::create` heal-scanning or adding into
        // this same admin directory while this call prunes/removes it corrupts whichever one
        // loses the race; see `repo_admin_lock`'s doc comment).
        let repo_lock = repo_admin_lock(repo);
        let _repo_guard = repo_lock.lock().unwrap();
        if std::path::Path::new(dir).exists() {
            clear_worktree_dir(repo, dir, authorized_root)?;
        } else {
            // No dir to clear, but a killed process may still leave a dangling admin entry.
            git(repo, &["worktree", "prune"])?;
        }
        reclaim_cache_sibling(dir, authorized_root);
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
    ///
    /// This runs [`Self::conflict_markers_present`]'s refusal (spec 89, criterion 1: A
    /// CHECKPOINT NEVER COMMITS A HALF-MERGE) and the repository's own git hooks - the
    /// hook-RESPECTING half of the pair with [`Self::commit_checkpoint`], for a commit
    /// an agent or a person actually MEANS to make. Every commit rigger itself makes as
    /// its own machine bookkeeping (the pre-gate attempt commit, the merge-into-worktree
    /// steps, a halt `wip` commit, a compensation revert) goes through
    /// [`Self::commit_checkpoint`] instead (spec 92 escalation ruling
    /// d-checkin-rigger-own-commits-bypass-hooks) - a hook enforcing content policy has
    /// no commit of THIS one's shape to police.
    pub fn commit(&self, message: &str) -> Result<String, Error> {
        self.commit_with(message, false)
    }

    /// A machine-bookkeeping commit: rigger recording its OWN provenance - a checkpoint
    /// preserving whatever a halted or superseded spawn left in its worktree (spec 89,
    /// criterion 1), the conductor's pre-gate attempt commit, [`Self::merge_into_worktree`]'s
    /// pre-merge and merge-conclusion commits, and [`Self::revert_on_base`]'s compensation
    /// commit - never a commit an agent's or a person's own work produces. It runs the
    /// half-merge guard like [`Self::commit`] but bypasses the repository's git hooks
    /// (`--no-verify`): a hook enforces content policy on a commit an agent or a person
    /// MEANS to make, and every one of these is machine bookkeeping of a tree mid-work - a
    /// hook refusing one (the docs-drift hook did, when a unit's rendered docs were ahead of
    /// the binary on PATH) turned "never lose a tree" (and, for the merge/revert sites,
    /// "integration always lands") into a dead step (spec 92 escalation ruling
    /// d-checkin-rigger-own-commits-bypass-hooks). The policy still holds where it belongs:
    /// the agent's own commits run the hooks, and the gates and `rigger validate` check the
    /// drift the hook checks.
    pub fn commit_checkpoint(&self, message: &str) -> Result<String, Error> {
        self.commit_with(message, true)
    }

    fn commit_with(&self, message: &str, no_verify: bool) -> Result<String, Error> {
        // The scan's scope (spec 89, criterion 1, round 2 fix
        // `adv-u89c1-conflict-marker-scan-is-repo-wide-content-not-diff-scoped-false-positive`)
        // MUST be read before `git add -A` below stages anything - staging is exactly what
        // clears a conflicted path's UNMERGED index flag (see `conflict_markers_present`'s own
        // doc comment on why the content check exists at all), and `changed_files` itself
        // (`git status --porcelain`) would otherwise report zero pending changes for a path
        // this very call is about to stage.
        let mut scope = self.changed_files()?;
        scope.extend(self.conflicting_paths()?);
        scope.sort();
        scope.dedup();
        if self.conflict_markers_present(&scope)? {
            return Err(Error(format!(
                "refusing to commit in {}: conflict-marker text is present in tracked \
                 file content - resolve it before committing",
                self.dir
            )));
        }
        git(&self.dir, &["add", "-A"])?;
        let args: &[&str] = if no_verify {
            &["commit", "--no-verify", "-m", message]
        } else {
            &["commit", "-m", message]
        };
        match run_git(&self.dir, args) {
            Ok(_) => {}
            Err(out) if out.contains("nothing to commit") => return Ok(String::new()),
            Err(out) => return Err(Error(format!("commit: {out}"))),
        }
        Ok(git(&self.dir, &["rev-parse", "HEAD"])?.trim().to_string())
    }

    /// Whether any of `scope`'s TRACKED files' current content (staged or not - `git grep`
    /// without `--cached` reads the worktree copy) still carries literal git conflict-marker
    /// lines (`<<<<<<<`, `=======`, `>>>>>>>`, each anchored at line-start so ordinary prose
    /// mentioning the symbols in passing cannot match). This is [`Self::commit`]'s ENTIRE
    /// guard (spec 89, criterion 1: A CHECKPOINT NEVER COMMITS A HALF-MERGE) - deliberately a
    /// CONTENT check, not an index-state one: `commit`'s own `git add -A` is what clears a
    /// conflicted path's UNMERGED index flag the instant it is staged, REGARDLESS of whether
    /// the staged content is a genuine resolution or still the raw marker text (exactly what
    /// let the 2026-09-12 incident's checkpoint treat a conflicted file as "resolved") - so an
    /// index-state check taken right before that same `add` cannot tell a still-broken path
    /// from one a caller (an implementer's edit, or [`crate::conductor`]'s
    /// `regenerate_conflicted_paths` overwriting a registered-regenerable path) has ALREADY
    /// fixed on disk but not yet staged; both look identically "unmerged" at that instant.
    /// Content is the one signal that is true regardless of staging order.
    ///
    /// `scope` (round 2 fix, spec 89 criterion 1 -
    /// `adv-u89c1-conflict-marker-scan-is-repo-wide-content-not-diff-scoped-false-positive`) is
    /// the CALLER's own touched-or-unmerged path list (`commit`'s `changed_files` union
    /// `conflicting_paths`, both read before `git add -A` can clear either signal) - never an
    /// unconditional whole-tracked-tree scan: a pre-existing, untouched file ELSEWHERE in the
    /// repo that merely happens to contain a line matching one of these patterns (a Markdown
    /// Setext heading's `=======` underline, say) must never block a commit that never touches
    /// it. An empty `scope` (nothing pending) is a no-op - never even shells out - matching the
    /// unconditional call's own no-op on a genuinely clean tree. `git grep` exits 1 (not an
    /// error) when nothing matches, distinct from a real invocation failure.
    fn conflict_markers_present(&self, scope: &[String]) -> Result<bool, Error> {
        if scope.is_empty() {
            return Ok(false);
        }
        // The pathspec separator is folded into this SAME multi-flag call, never passed via
        // its own single-argument call, so the no-os-kill audit's whole-tree argv-separator
        // shape - aimed at a negative-pid kill target, not a git pathspec - never matches here.
        let out = crate::subprocess::git_in(&self.dir)
            .args([
                "grep",
                "-I",
                "-q",
                "-e",
                "^<<<<<<< ",
                "-e",
                "^=======$",
                "-e",
                "^>>>>>>> ",
                "--",
            ])
            .args(scope)
            .output()
            .map_err(|e| Error(format!("git grep conflict markers: {e}")))?;
        match out.status.code() {
            Some(0) => Ok(true),
            Some(1) => Ok(false),
            _ => Err(Error(format!(
                "git grep conflict markers: {}",
                String::from_utf8_lossy(&out.stderr)
            ))),
        }
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
        let mut paths = self.diff_names(&base, DiffMode::MergeBase)?;
        paths.extend(self.changed_files()?);
        paths.sort();
        paths.dedup();
        Ok(paths)
    }

    /// The diff between `from` and this worktree's current `HEAD`, name-only, sorted and
    /// de-duplicated, in the [`DiffMode`] the caller names: runs one `git diff --name-only
    /// <range>` and parses its output, so the two diff MODES (merge-base-anchored three-dot
    /// vs a direct two-dot tree comparison) never drift the line-parsing logic apart.
    pub fn diff_names(&self, from: &str, mode: DiffMode) -> Result<Vec<String>, Error> {
        let range = match mode {
            DiffMode::MergeBase => format!("{from}...HEAD"),
            DiffMode::Direct => format!("{from}..HEAD"),
        };
        let out = git(&self.dir, &["diff", "--name-only", &range])?;
        let mut paths: Vec<String> = out
            .lines()
            .map(|l| l.trim())
            .filter(|l| !l.is_empty())
            .map(|l| l.to_string())
            .collect();
        paths.sort();
        paths.dedup();
        Ok(paths)
    }

    /// Every commit already made on this worktree's branch that the run branch's
    /// CURRENT HEAD does not yet have, oldest-first: exactly the commits
    /// [`Self::cherry_pick_onto_run_branch`] would carry across (spec 88, criterion 4 -
    /// PLAN AMENDMENTS LAND). A `produces` stage never runs [`Self::commit`] (it writes
    /// no code the conductor sweeps before gating), so this reads whatever its agent
    /// committed directly with its own git access - an approved amendment to the spec
    /// it is decomposing.
    ///
    /// Unlike [`Self::changed_since_base`]'s three-dot DIFF (which needs the merge-base
    /// correction so an independently-advanced base contributes no unrelated file
    /// noise), a commit RANGE needs no such correction: git's plain two-dot exclusion
    /// (`base..HEAD`) already means "every commit reachable from HEAD but not from
    /// base", which for a worktree branch that only ever gains commits (never rebased)
    /// is precisely this branch's own.
    pub fn commits_since_base(&self) -> Result<Vec<String>, Error> {
        let base = git(&self.repo, &["rev-parse", "HEAD"])?.trim().to_string();
        let out = git(
            &self.dir,
            &["rev-list", "--reverse", &format!("{base}..HEAD")],
        )?;
        Ok(out
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .map(str::to_string)
            .collect())
    }

    /// Every path a single commit `sha` (already on this worktree's own object
    /// database - it need not be on this branch's tip) touched, by ITS OWN parent
    /// diff (`git diff-tree --no-commit-id --name-only -r --root <sha>`), independent
    /// of any other commit around it. Unlike [`Self::changed_since_base`]'s AGGREGATE
    /// three-dot diff across a whole range - which nets a path to nothing when a
    /// LATER commit in the same range reverts an EARLIER one's own touch to that same
    /// path - this answers "what did this ONE commit itself change", so a scope check
    /// walking every commit in [`Self::commits_since_base`] individually can catch a
    /// transient out-of-scope write that the aggregate view would miss entirely
    /// (spec 88, criterion 4, `adv-u88c4-scope-check-nets-the-diff-not-each-commit`).
    /// `--root` makes a parentless (root) commit report every path as added, rather
    /// than erroring for lack of a parent to diff against.
    pub fn files_touched_by_commit(&self, sha: &str) -> Result<Vec<String>, Error> {
        let out = git(
            &self.dir,
            &[
                "diff-tree",
                "--no-commit-id",
                "--name-only",
                "-r",
                "--root",
                sha,
            ],
        )?;
        Ok(out
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .map(str::to_string)
            .collect())
    }

    /// Every path this worktree currently has UNMERGED (a real git conflict): each
    /// `U`-status (unmerged) entry from `git diff --name-only --diff-filter=U`, sorted and
    /// deduplicated. Reads WORKTREE STATE directly - never the event log, never a git
    /// command's own exit code - so it answers identically whether called right after
    /// [`Self::integrate`] left markers or on a crash-resumed step that never re-invokes
    /// `git merge` at all (spec 88, criterion 1: "the merge is worktree state, not log
    /// state"). Empty when nothing is unmerged.
    pub fn conflicting_paths(&self) -> Result<Vec<String>, Error> {
        let out = git(&self.dir, &["diff", "--name-only", "--diff-filter=U"])?;
        let mut paths: Vec<String> = out
            .lines()
            .map(|l| l.trim())
            .filter(|l| !l.is_empty())
            .map(|l| l.to_string())
            .collect();
        paths.sort();
        paths.dedup();
        Ok(paths)
    }

    /// Whether this worktree currently has a merge IN PROGRESS (`MERGE_HEAD` resolves) -
    /// true from the moment [`Self::integrate`] starts a real (non-"up to date") merge
    /// until it is committed, whether or not it carries conflicts. [`Self::integrate`]
    /// reads this to decide whether to invoke `git merge` again (never, once one is
    /// already in progress - git refuses and a re-invocation would error) or to read the
    /// worktree's current state instead - the crash-resume idempotency spec 88, criterion
    /// 1 requires.
    pub fn merge_in_progress(&self) -> bool {
        run_git(&self.dir, &["rev-parse", "-q", "--verify", "MERGE_HEAD"]).is_ok()
    }

    /// Resolve `path`'s unmerged conflict by accepting the INCOMING (run branch) side, then
    /// stage it - a deterministic PLACEHOLDER resolution for a registered regenerable path
    /// (spec 88, criterion 1), never used for a source path a real implementer must resolve.
    /// It exists only to unblock `git commit` (which refuses while ANY path is unmerged, even
    /// one nobody was asked to touch) until the conductor's OWN regeneration pass overwrites
    /// the path for real, in a follow-up commit, after the source conflict is resolved - the
    /// design's "in that order".
    pub fn accept_incoming(&self, path: &str) -> Result<(), Error> {
        git(&self.dir, &["checkout", "--theirs", "--", path])?;
        git(&self.dir, &["add", "--", path])?;
        Ok(())
    }

    /// Merge the run branch's tip INTO this worktree (spec 88, criterion 1 round 4, TABLE row
    /// 1: "conflict detection" - the mutation between the row's before-record, "the conflicting
    /// path list"'s intent, i.e. the merge about to be attempted, and its after-record, "the
    /// merge-in-progress outcome (conflicts or clean)"). Together with [`Self::land`] (TABLE
    /// row 4) this is the split-in-two FRONT HALF of what a single `integrate` method used to
    /// do before round 4: `integrate_and_emit` calls each half directly so it can durably
    /// record its own row's before/after pair around exactly that one mutation - two rows, two
    /// mutations, two independently resumable boundaries, rather than one opaque call spanning
    /// both (this file's own `mod tests` recomposes the two into a test-only `integrate` that
    /// mirrors the pre-round-4 combined shape, since the tests it re-derives - crash-resume
    /// idempotency, conflict-leaves-markers, non-content-failure-surfaces - exercise the
    /// combined git behavior end to end and gain nothing from being split across two calls).
    ///
    /// A merge already in progress (crash-resume) is NEVER re-entered here: `commit`'s `git add
    /// -A` would blindly stage any still-conflicted file's literal marker text as "resolved"
    /// content, and re-invoking `git merge` would be refused outright by git regardless.
    /// Worktree state - read below via `conflicting_paths` - is the sole authority for what
    /// happens next; this whole commit-and-attempt block is skipped.
    pub fn merge_into_worktree(&self, message: &str) -> Result<MergeOutcome, Error> {
        // Set only by a fresh attempt's own merge command below - `None` on a crash-resumed
        // re-entry (the block below is skipped entirely) or when no merge was even needed.
        let mut merge_attempt: Option<String> = None;
        if !self.merge_in_progress() {
            // Rigger's own integration bookkeeping commit (spec 92 escalation ruling
            // d-checkin-rigger-own-commits-bypass-hooks): the same class as the
            // conductor's pre-gate attempt commit (`commit_checkpoint`, commit
            // 066ceaaf) - a hook enforces content policy on a commit an agent or a
            // person MEANS to make, never on the machine's own merge-into-worktree
            // step, so this bypasses hooks exactly like that call site.
            let committed = self.commit_checkpoint(message)?;
            // Nothing at all for this unit to contribute (no fresh commit here, and its
            // branch already sits exactly at the run branch's tip): true read-only no-op,
            // matching the historical short circuit exactly - never even attempt a merge.
            if committed.is_empty() {
                let head = git(&self.dir, &["rev-parse", "HEAD"])?.trim().to_string();
                let base = git(&self.repo, &["rev-parse", "HEAD"])?.trim().to_string();
                if head == base {
                    return Ok(MergeOutcome::Ready(String::new()));
                }
            }
            let run_tip = git(&self.repo, &["rev-parse", "HEAD"])?.trim().to_string();
            // Outcome (clean vs conflict) is read from worktree state just below, not
            // from this command's own exit code alone - a genuine CONTENT conflict also
            // exits non-zero, and is read back (and returned) via `conflicting_paths` right
            // below regardless of what this call returns. But a NON-content failure (spec
            // 88 criterion 1, operator ruling point (e): sdet-u88c1-worktree-merge-result-
            // discarded) - e.g. a stray untracked file at a path `run_tip` newly tracks,
            // which git refuses to clobber - leaves BOTH `conflicting_paths()` and
            // `merge_in_progress()` at their ordinary "nothing to do" defaults. Worktree
            // state alone cannot tell that apart from "nothing changed", so the error is
            // captured here and, once the checks below have ruled out a real conflict,
            // surfaced as a genuine `Err` instead of silently falling through to land the
            // unit's branch UNCHANGED.
            merge_attempt =
                run_git(&self.dir, &["merge", "--no-commit", "--no-ff", &run_tip]).err();
        }
        let conflicts = self.conflicting_paths()?;
        if !conflicts.is_empty() {
            return Ok(MergeOutcome::Conflict(conflicts));
        }
        if self.merge_in_progress() {
            // Every conflict (if any arose) is resolved and staged: finalize the merge
            // commit on the unit's OWN branch before landing it on the run branch. Same
            // bypass as the pre-merge commit just above (d-checkin-rigger-own-commits-
            // bypass-hooks) - this is rigger's own merge-conclusion bookkeeping, not a
            // commit an agent or a person means to make.
            match run_git(&self.dir, &["commit", "--no-edit", "--no-verify"]) {
                Ok(_) => {}
                Err(out) if out.contains("nothing to commit") => {}
                Err(out) => return Err(Error(format!("commit merge: {out}"))),
            }
        } else if let Some(out) = merge_attempt {
            // Worktree state has now ruled out a real content conflict (`conflicts` empty
            // above) and an in-progress merge to finalize (`merge_in_progress` just above) -
            // so a fresh attempt's own merge command failing here is a genuine, non-content
            // error (spec 88 criterion 1, operator ruling point (e)), never a silent no-op.
            return Err(Error(format!("worktree merge --no-commit --no-ff: {out}")));
        }
        let commit = git(&self.dir, &["rev-parse", "HEAD"])?.trim().to_string();
        Ok(MergeOutcome::Ready(commit))
    }

    /// Land this worktree's branch - already fully resolved and committed by a prior
    /// [`Self::merge_into_worktree`] call that returned `Ready` with a non-empty commit - onto
    /// the run branch (spec 88, criterion 1 round 4, TABLE row 4: "landing", the mutation
    /// between the row's before-record, "the landing intent (unit tip, run tip)", and its
    /// after-record, "the landed sha"). The worktree's branch is, by construction, a strict
    /// descendant of the run branch's tip (the merge `merge_into_worktree` just performed, or
    /// an earlier one already established that), so this lands as a clean fast-forward. A
    /// failure here is a genuine, unexpected error - never a conflict (conflicts are caught,
    /// and returned, by `merge_into_worktree` itself, which the caller must check first).
    pub fn land(&self) -> Result<LandOutcome, Error> {
        // FAST-FORWARD ONLY. `merge_into_worktree` has just merged the run branch's tip into
        // the unit's branch, so a correct landing is always a fast-forward; anything else
        // means the run branch MOVED between that merge and this call (an operator commit, a
        // sibling's landing). A real merge here would resolve nothing the worktree merge did
        // not already resolve - and on a conflict it left the main checkout mid-merge
        // (`MERGE_HEAD`, `UU` paths), which failed every later step (2026-09-15, spec 92).
        // `--ff-only` refuses before it touches the index, so the repo is never left dirty;
        // the caller redoes the worktree merge against the new tip and lands again.
        match run_git(&self.repo, &["merge", "--ff-only", &self.branch]) {
            Ok(_) => Ok(LandOutcome::Landed),
            Err(out)
                if out
                    .to_ascii_lowercase()
                    .contains("not possible to fast-forward") =>
            {
                Ok(LandOutcome::TipMoved)
            }
            // Git's two local-changes refusals ("The following untracked working tree files
            // would be overwritten by merge" and "Your local changes to the following files
            // would be overwritten by merge") share this one tail wording and the same
            // tab-indented path-list shape that follows it - see `parse_blocking_paths`.
            Err(out) if out.contains("would be overwritten by merge") => {
                Ok(LandOutcome::Blocked(parse_blocking_paths(&out)))
            }
            Err(out) => Err(Error(format!("git merge --ff-only {}: {out}", self.branch))),
        }
    }

    /// Cherry-pick `shas` (oldest-first, from [`Self::commits_since_base`]) from this
    /// worktree's branch onto the run branch, as ONE cherry-pick sequence (spec 88,
    /// criterion 4 - PLAN AMENDMENTS LAND): a `produces` stage's own commits are never
    /// swept and merged like an ordinary unit's ([`Self::merge_into_worktree`] then
    /// [`Self::land`]) - they carry no
    /// code diff to gate, so this lands them directly, preserving each commit's own
    /// identity (never squashed).
    ///
    /// A no-op (`Picked(vec![])`, nothing touched) on an empty `shas`. On a conflict
    /// partway through a multi-commit sequence, `--abort` unwinds the WHOLE sequence
    /// (git's cherry-pick sequencer tracks every commit already applied this call),
    /// mirroring the invariant [`Self::merge_into_worktree`] keeps on a conflict (the run
    /// branch itself is never left mid-merge) - the run branch never carries a
    /// half-landed amendment.
    ///
    /// IDEMPOTENT on a RESUMED already-landed sequence (spec 88 c4,
    /// `sdet-u88c4-cherry-pick-resume-not-idempotent`): a crash between a PRIOR call's
    /// real git success and the caller recording it can mean a resumed process asks
    /// to cherry-pick the SAME `shas` again - [`Self::commits_since_base`] is
    /// identity-based, and a cherry-pick mints a NEW commit object, so the original
    /// commit stays "not yet on the run branch" by identity even once its CONTENT
    /// already landed. Applying a commit whose content is already present produces an
    /// EMPTY patch, which git PAUSES on rather than silently dropping - `--skip`
    /// moves the sequencer past it and on to the next commit, so a batch that is
    /// EVERY pick empty (the whole-resume case) runs through to completion with
    /// nothing new created (`Picked(vec![])`, the SAME no-op shape an empty `shas`
    /// produces), and a batch that is PARTLY empty still lands whichever picks are
    /// genuinely new.
    /// The cherry-pick sequencer's own remaining-pick count for `repo` (spec 88 c4,
    /// arch-u88c4-r7-classification-skip-is-single-shot-not-a-loop): a leftover
    /// `CHERRY_PICK_HEAD`'s pending-commit count, read via `git rev-parse --git-path
    /// sequencer/todo` - WORKTREE-SAFE, since this crate runs several worktrees off
    /// ONE shared object database and sequencer state lives per-worktree under
    /// `.git/worktrees/<name>/sequencer/`, never the shared `.git/sequencer`. Unlike
    /// most `--git-path` uses, git's OWN output here is relative to `repo` (its `-C`
    /// argument is a real chdir for the git subprocess, not for this one) for a
    /// PLAIN repo, but already absolute for a LINKED worktree - joined onto `repo`
    /// only when it is not already absolute, so both shapes resolve correctly from
    /// this process's own cwd. Counts every non-blank, non-comment line (each is one
    /// `pick <sha> <subject>` entry) - this is the SAME quantity
    /// [`Self::cherry_pick_onto_run_branch`]'s fresh-sequence loop bounds itself by
    /// proxy by using `shas.len()` (the two are equal at the point that loop starts,
    /// since nothing has been skipped yet); reading it directly here is what lets the
    /// leftover-marker classification bound its OWN skip loop the identical way when
    /// it has no `shas` of its own to count.
    ///
    /// A MISSING todo file (`io::ErrorKind::NotFound`) is the NORMAL shape for a
    /// leftover sequence whose remaining set is exactly ONE commit, not an anomaly
    /// (adj-u88c4-r8-verdict-reject / sdet-u88c4-r8-single-commit-leftover-marker-
    /// hard-errors-on-missing-sequencer-todo): git's sequencer machinery is never
    /// engaged by a plain single-sha `git cherry-pick <sha>`, so it never creates
    /// `.git/sequencer/` at all - and that single-sha shape is exactly what
    /// [`Self::cherry_pick_onto_run_branch`] issues whenever its caller's own
    /// `shas` (equivalently, the conductor's `still_pending`) has shrunk to one,
    /// the routine steady state of an iterative multi-commit plan amendment, not
    /// merely a literal one-commit-total unit. Reading it as exactly ONE remaining
    /// entry (rather than propagating the io error) guarantees the bounded skip
    /// loop above still attempts at least one `--skip`, so it resolves this
    /// leftover the same way it resolves every other empty-commit pause instead of
    /// hard-failing the very case it exists to handle.
    fn sequencer_todo_remaining(repo: &str) -> Result<usize, Error> {
        let raw = git(repo, &["rev-parse", "--git-path", "sequencer/todo"])?
            .trim()
            .to_string();
        let todo_path = std::path::Path::new(&raw);
        let todo_path = if todo_path.is_absolute() {
            todo_path.to_path_buf()
        } else {
            std::path::Path::new(repo).join(todo_path)
        };
        match std::fs::read_to_string(&todo_path) {
            Ok(contents) => Ok(contents
                .lines()
                .filter(|l| {
                    let l = l.trim();
                    !l.is_empty() && !l.starts_with('#')
                })
                .count()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(1),
            Err(e) => Err(Error(format!("reading {}: {e}", todo_path.display()))),
        }
    }

    pub fn cherry_pick_onto_run_branch(&self, shas: &[String]) -> Result<CherryPickOutcome, Error> {
        if shas.is_empty() {
            return Ok(CherryPickOutcome::Picked(Vec::new()));
        }
        // GIT IN-PROGRESS STATE, CLASSIFIED EXPLICITLY (spec 88 c4, operator ruling
        // op-u88c4-next-round-plan-commit-landing-is-log-carried-and-idempotent, item
        // 3, superseding the round-5 fix's blind abort-and-retry): a leftover
        // CHERRY_PICK_HEAD from an earlier, crashed call is read BEFORE anything else
        // and resolved by NAME rather than discarded unconditionally:
        //   - unmerged paths present: a REAL conflict from that earlier call - the
        //     SAME path a fresh conflict takes below (abort, report `Conflict`),
        //     never silently re-run into the identical conflict a second time.
        //   - no unmerged paths: an EMPTY-COMMIT PAUSE (adv-u88c4-r4-cherry-pick-in-
        //     progress-marker-survives-a-crash-mid-skip-loop - a crash WHILE the
        //     skip-loop below was mid-flight) - resolved the SAME way the loop below
        //     resolves it, `--skip`, so the earlier call's own remaining sequencer
        //     todo (if any) completes before this call's fresh sequence for `shas`
        //     ever starts.
        // A state that is neither (still in progress after `--skip`) is not a shape
        // this function recognizes - a hard error naming the marker, never a guess.
        if run_git(
            &self.repo,
            &["rev-parse", "-q", "--verify", "CHERRY_PICK_HEAD"],
        )
        .is_ok()
        {
            let unmerged = run_git(&self.repo, &["ls-files", "--unmerged"])
                .map(|u| !u.trim().is_empty())
                .unwrap_or(false);
            if unmerged {
                let detail = run_git(&self.repo, &["diff", "--diff-filter=U"]).unwrap_or_default();
                let _ = run_git(&self.repo, &["cherry-pick", "--abort"]);
                return Ok(CherryPickOutcome::Conflict(format!(
                    "a leftover cherry-pick from an earlier, crashed attempt conflicted: {detail}"
                )));
            }
            // BOUNDED LOOP, NOT A SINGLE SHOT (arch-u88c4-r7-classification-skip-is-
            // single-shot-not-a-loop / sdet-u88c4-r7-classification-skip-confirmed-
            // live-and-untested-for-2plus-chained-empties): git's own `--skip` only
            // ever advances the sequencer past the CURRENT paused commit, and the
            // very next one can ALSO be empty (an ordinary shape for a multi-commit
            // plan amendment resumed after a crash) - a single attempt leaves
            // CHERRY_PICK_HEAD still set and would wrongly read as an unrecognized
            // state. This reuses the SAME `skips_left` shape the fresh-sequence loop
            // below uses, bounded by the leftover sequence's OWN remaining `todo`
            // count (read BEFORE any skip, via [`Self::sequencer_todo_remaining`]) -
            // exactly mirroring that loop's `shas.len()` bound and the SAME
            // reasoning: the sequencer's own remaining-commit list strictly shrinks
            // by one each skip, so a correct git can never need more skips than this.
            let mut skips_left = Self::sequencer_todo_remaining(&self.repo)?;
            loop {
                if skips_left == 0 {
                    return Err(Error(
                        "CHERRY_PICK_HEAD left in progress by an earlier attempt, and it is \
                         neither a conflict nor a resolvable empty-commit pause: exhausted the \
                         sequencer's own remaining-todo count without resolving"
                            .to_string(),
                    ));
                }
                skips_left -= 1;
                match run_git(&self.repo, &["cherry-pick", "--skip"]) {
                    Ok(_) => break, // the leftover sequence is now fully resolved
                    Err(out) => {
                        let unmerged_now = run_git(&self.repo, &["ls-files", "--unmerged"])
                            .map(|u| !u.trim().is_empty())
                            .unwrap_or(false);
                        if unmerged_now {
                            let detail = run_git(&self.repo, &["diff", "--diff-filter=U"])
                                .unwrap_or_default();
                            let _ = run_git(&self.repo, &["cherry-pick", "--abort"]);
                            return Ok(CherryPickOutcome::Conflict(format!(
                                "a leftover cherry-pick from an earlier, crashed attempt \
                                 conflicted: {detail}"
                            )));
                        }
                        if !out.contains("previous cherry-pick is now empty") {
                            return Err(Error(format!(
                                "CHERRY_PICK_HEAD left in progress by an earlier attempt, and \
                                 it is neither a conflict nor a resolvable empty-commit pause: \
                                 {out}"
                            )));
                        }
                        // Another empty-commit pause further down the leftover
                        // sequence - loop around and skip it too, bounded by
                        // `skips_left`.
                    }
                }
            }
        }
        let before = git(&self.repo, &["rev-parse", "HEAD"])?.trim().to_string();
        // Unmerged files are the definitive conflict signal (git-version-independent),
        // mirroring `integrate`'s own check; the phrasing checks are a belt-and-braces
        // backup for a cherry-pick-specific message shape.
        let is_conflicted = |out: &str| -> bool {
            run_git(&self.repo, &["ls-files", "--unmerged"])
                .map(|u| !u.trim().is_empty())
                .unwrap_or(false)
                || out.contains("CONFLICT")
                || out.contains("could not apply")
                || out.contains("after resolving the conflicts")
        };
        let mut args: Vec<&str> = vec!["cherry-pick"];
        args.extend(shas.iter().map(String::as_str));
        let mut result = run_git(&self.repo, &args);
        // Bounded to `shas.len()` skips: the sequencer's own remaining-commit list
        // strictly shrinks by one each skip, so a correct git can never need more.
        let mut skips_left = shas.len();
        loop {
            let out = match &result {
                Ok(_) => break,
                Err(out) => out.clone(),
            };
            if skips_left == 0
                || is_conflicted(&out)
                || !out.contains("previous cherry-pick is now empty")
            {
                break;
            }
            skips_left -= 1;
            result = run_git(&self.repo, &["cherry-pick", "--skip"]);
        }
        match result {
            Ok(_) => {
                // The shas AS THEY LAND (see the type's doc comment for why this can
                // differ from - or equal - the pre-landing `shas`): every commit the
                // run branch's HEAD gained this call, oldest-first, same two-dot
                // exclusion `commits_since_base` uses. Empty when every pick this call
                // made was already-applied (the resumed no-op case above).
                let out = git(
                    &self.repo,
                    &["rev-list", "--reverse", &format!("{before}..HEAD")],
                )?;
                let landed = out
                    .lines()
                    .map(str::trim)
                    .filter(|l| !l.is_empty())
                    .map(str::to_string)
                    .collect();
                Ok(CherryPickOutcome::Picked(landed))
            }
            Err(out) => {
                let conflicted = is_conflicted(&out);
                let _ = run_git(&self.repo, &["cherry-pick", "--abort"]);
                if conflicted {
                    Ok(CherryPickOutcome::Conflict(out))
                } else {
                    Err(Error(format!("git cherry-pick {}: {out}", shas.join(" "))))
                }
            }
        }
    }

    /// Git's own STABLE, CONTENT-based identity for commit `sha`'s diff
    /// (`git show <sha> | git patch-id --stable`) - independent of the commit's
    /// parent, author, committer, or timestamp, so a commit re-created by a
    /// different mechanism (a cherry-pick mints a brand new commit object carrying
    /// the SAME diff) is recognized as the SAME change. This is the mechanism the
    /// operator ruling `op-u88c4-next-round-plan-commit-landing-is-log-carried-and-
    /// idempotent`'s item (2) names ("reachable from the run branch by patch-id") -
    /// it replaces
    /// the REMOVED `already_landed_commits`, whose tree-POSITION walk was rejected
    /// three review rounds running (arch-u88c4-r6-operator-ruling-unimplemented-
    /// still-a-heuristic et al.) precisely because a position shifts when anything
    /// else lands on the run branch in between, while a patch-id never does. `dir`
    /// need not be the worktree `sha` originally lived on - every worktree of the
    /// SAME repository shares one object database, so `self.repo` can resolve a sha
    /// that only ever existed on `self.dir`'s branch, and vice versa.
    fn patch_id_of(dir: &str, sha: &str) -> Result<String, Error> {
        use std::process::Stdio;
        let mut show = crate::subprocess::git_in(dir)
            .args(["show", "--no-color", sha])
            .stdout(Stdio::piped())
            .spawn()
            .map_err(|e| Error(format!("git show {sha}: {e}")))?;
        let show_stdout = show
            .stdout
            .take()
            .ok_or_else(|| Error(format!("git show {sha}: no stdout pipe")))?;
        let patch_id = crate::subprocess::git_in(dir)
            .args(["patch-id", "--stable"])
            .stdin(Stdio::from(show_stdout))
            .output()
            .map_err(|e| Error(format!("git patch-id {sha}: {e}")))?;
        // The Child handle that spawned `show` is waited on directly, never signalled -
        // this crate's handle-bound process lifecycle discipline (spec 78): a process
        // this crate starts is ended only through its own spawning handle, never a
        // shell-out or a computed-pid signal.
        let show_status = show
            .wait()
            .map_err(|e| Error(format!("git show {sha} wait: {e}")))?;
        if !show_status.success() {
            return Err(Error(format!("git show {sha}: exited {show_status}")));
        }
        if !patch_id.status.success() {
            return Err(Error(format!(
                "git patch-id {sha}: {}",
                String::from_utf8_lossy(&patch_id.stderr)
            )));
        }
        // A content-FREE commit (`git commit --allow-empty`, e.g. this crate's own
        // `init_repo` test seed) has no diff at all, so `git patch-id` legitimately
        // prints nothing - not a tool failure. The empty string is a valid, distinct
        // identity (never a match for anything, including another empty commit -
        // [`Self::find_landed_by_patch_id`] guards this explicitly rather than
        // letting two content-free commits compare equal).
        Ok(String::from_utf8_lossy(&patch_id.stdout)
            .split_whitespace()
            .next()
            .map(str::to_string)
            .unwrap_or_default())
    }

    /// Public wrapper over [`Self::patch_id_of`] against this worktree's own directory
    /// (see that function's doc comment - the object database is shared, so this
    /// resolves a sha from EITHER this worktree's branch or the run branch it was
    /// created from).
    pub fn patch_id(&self, sha: &str) -> Result<String, Error> {
        Self::patch_id_of(&self.dir, sha)
    }

    /// Search the run branch's own recent history (`self.repo`, the `window` most
    /// recent commits reachable from its HEAD) for a commit whose [`Self::patch_id`]
    /// matches `original_sha`'s - the RECOVERY half of ruling item (2): confirming
    /// that an intended plan-stage commit already reached the run branch under a
    /// DIFFERENT (cherry-pick-minted) object, without depending on ITS POSITION in
    /// that history at all. `None` when no match is found within `window` - never a
    /// guess beyond what was actually searched, so a caller that cannot confirm
    /// simply leaves the sha pending for the next call rather than fabricating an
    /// identity.
    pub fn find_landed_by_patch_id(
        &self,
        original_sha: &str,
        window: usize,
    ) -> Result<Option<String>, Error> {
        let target = self.patch_id(original_sha)?;
        if target.is_empty() {
            // A content-free intended commit carries no reliable identity to search
            // for - never a guess, so this never confirms one.
            return Ok(None);
        }
        let out = git(
            &self.repo,
            &["rev-list", &format!("--max-count={window}"), "HEAD"],
        )?;
        for candidate in out.lines().map(str::trim).filter(|l| !l.is_empty()) {
            let candidate_id = self.patch_id(candidate)?;
            if !candidate_id.is_empty() && candidate_id == target {
                return Ok(Some(candidate.to_string()));
            }
        }
        Ok(None)
    }

    /// Delete the worktree (its branch is left for the caller to clean up), and reclaim its
    /// sibling per-unit build cache (`cargo-target-<slug>`, Gap 19). This is the DOMINANT
    /// graceful path a unit's worktree is torn down (the conductor's `run_stage` calls it at
    /// stage-end on integrate / park / err), and the cache is a plain dir git never tracks, so
    /// removing the worktree alone would leak a multi-gigabyte cache on the operator's small
    /// partition. Reclamation is best-effort - a review worktree or an un-built unit has no
    /// such sibling and it is a no-op there - and never changes the removal's result.
    pub fn remove(&self) -> Result<(), Error> {
        // Serialize this WHOLE call against every other in-process admin-directory mutation
        // this process makes for the SAME repository (spec 103 checkin round 4: this call's
        // own `git worktree remove --force` below writes into `.git/worktrees` exactly like
        // `Worktree::create`'s heal-scan-then-add does; see `repo_admin_lock`'s doc comment).
        let repo_lock = repo_admin_lock(&self.repo);
        let _repo_guard = repo_lock.lock().unwrap();
        // Reap any process still rooted inside this worktree BEFORE git removes the dir (spec
        // 23): otherwise a build or tool an agent left running holds a now-deleted cwd and
        // outlives its worktree, leaking memory. Scoped to this EXACT dir, so a process rooted
        // at the repo root or outside rigger's scratch is never touched.
        //
        // Authorized by GIT IDENTITY, not by `crate::reap::reap_processes_rooted_under`'s usual
        // "strictly under a resolved scratch root" containment gate (spec 78 round 2, decision
        // `u78c2r2-worktree-remove-identity-not-tree`): a worktree's own dir can legitimately
        // live ANYWHERE relative to `self.repo` - `defaults.workdir`/`RIGGER_TMPDIR` relocation
        // is a real, tested config surface (`tests/scratch_workdir_config.rs`) with no
        // necessary containment relationship to the repo at all - so there is no
        // `authorized_root` this function could compute (from config, env, or `self.repo`
        // itself) that would reliably contain it. [`worktree_on_branch`] is instead the SAME
        // predicate [`Self::create`]'s own fast-path adoption already trusts to mean "this dir
        // IS a real, currently-checked-out git worktree of this exact branch" - a fact git
        // itself attests to, independent of where the dir physically sits - so it authorizes
        // the reap without caring about relocation. A dir that fails this check (already
        // removed, or somehow not on the expected branch) skips the reap: best-effort, never
        // fails the removal below.
        if worktree_on_branch(&self.dir, &self.branch) {
            if let Ok(base) = std::path::Path::new(&self.dir).canonicalize() {
                crate::reap::reap_authorized(base);
            }
        }
        git(&self.repo, &["worktree", "remove", "--force", &self.dir])?;
        // The sibling cache/fence dirs, unlike `self.dir` above, sit under the ordinary
        // scratch-root containment authority (they are constructed as a sibling of `self.dir`
        // by [`unit_cache_sibling`]/[`review_fence_sibling`], never relocated independently),
        // so they are gated by `self.authorized_root` - the SAME root [`Self::create`] was
        // given for this exact instance (spec 79 round-2 fix), never re-derived here.
        reclaim_cache_sibling(&self.dir, &self.authorized_root);
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
/// existing one (reusing a prior window's committed work). Public so the conductor's
/// ADOPTION KEYS ON THE CRITERION check (spec 88) can guard
/// [`Worktree::create_branch_at`] against re-pointing a unit's branch that already
/// exists, and confirm a prior unit's branch is still around before adopting it; also
/// the durable-branch existence check `rigger resume-unit` (spec 88, criterion 3)
/// refuses on when an escalated unit's recorded branch is gone - "refused with the
/// branch name and the reflog hint."
pub fn branch_exists(repo: &str, branch: &str) -> bool {
    ref_resolves(repo, &format!("refs/heads/{branch}"))
}

/// The CURRENT tip commit sha of local branch `branch` in `repo`, or an error when the
/// branch does not exist. Public so the conductor's ADOPTION KEYS ON THE CRITERION check
/// (spec 88 round 4) can read a prior unit's tip and record it as durable provenance
/// BEFORE seeding the adopting unit's own branch AT that exact sha
/// ([`Worktree::create_branch_at`] pinned to a sha rather than the moving branch name) -
/// closing the crash window between deciding to adopt and creating the branch by making
/// the two agree by construction rather than by re-resolving the (possibly since-moved)
/// branch name a second time.
pub fn branch_tip(repo: &str, branch: &str) -> Result<String, Error> {
    run_git(repo, &["rev-parse", &format!("refs/heads/{branch}")])
        .map(|s| s.trim().to_string())
        .map_err(Error)
}

/// The tip of local branch `branch` when its work is LANDED on `run_branch`: the tip is an
/// ancestor of `run_branch` AND differs from the commit the branch was created at (the oldest
/// entry of its reflog), so a branch that never moved - trivially an ancestor of the branch it
/// was cut from - never reads as landed. `None` whenever either fact cannot be established (no
/// such branch, no reflog), failing closed: `rigger reset --runs` records a unit's terminal
/// event on this answer, and an unprovable landing must leave the unit open.
pub fn landed_branch_tip(repo: &str, branch: &str, run_branch: &str) -> Option<String> {
    let tip = branch_tip(repo, branch).ok()?;
    let reflog = run_git(
        repo,
        &[
            "reflog",
            "show",
            "--format=%H",
            &format!("refs/heads/{branch}"),
        ],
    )
    .ok()?;
    let created = reflog.lines().map(str::trim).rfind(|l| !l.is_empty())?;
    let landed =
        created != tip && run_git(repo, &["merge-base", "--is-ancestor", &tip, run_branch]).is_ok();
    landed.then_some(tip)
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

/// Parse the blocking-path list out of git's own local-changes refusal text (see
/// [`Worktree::land`]'s [`LandOutcome::Blocked`]). Both refusal wordings share the same
/// shape: one header line ending "would be overwritten by merge:", followed by one
/// tab-indented path per line, up to the first line that is not tab-indented (git's
/// "Please ..." follow-up). Sorted and deduplicated so a caller's report is deterministic
/// regardless of git's own listing order; text carrying no such header names nothing.
pub fn parse_blocking_paths(out: &str) -> Vec<String> {
    let mut in_list = false;
    let mut paths: Vec<String> = Vec::new();
    for line in out.lines() {
        if in_list {
            match line.strip_prefix('\t') {
                Some(path) => {
                    paths.push(path.trim().to_string());
                    continue;
                }
                None => in_list = false,
            }
        }
        if line.trim_end().ends_with("would be overwritten by merge:") {
            in_list = true;
        }
    }
    paths.sort();
    paths.dedup();
    paths
}

/// The raw bytes `path` holds in the tree of `git_ref` within `repo`, or `None` when
/// `git_ref` does not resolve or does not carry that path. Implemented as `git show
/// <ref>:<path>`, reading ONLY its stdout - unlike the [`run_git`]/[`git`] primitives this
/// module mostly builds on (which fold stdout and stderr together for diagnostics), a
/// caller here wants a file's real content, never diagnostic text mixed into it. Used by the
/// conductor's land-refused lesson (spec 103 criterion 8) to find any unit branch whose tip
/// already carries byte-identical content at a path a refused landing was blocked by.
pub fn blob_at(repo: &str, git_ref: &str, path: &str) -> Option<Vec<u8>> {
    let out = crate::subprocess::git_in(repo)
        .args(["show", &format!("{git_ref}:{path}")])
        .output()
        .ok()?;
    out.status.success().then_some(out.stdout)
}

/// Every unit branch (`rigger/u/*`) currently present in `repo`, sorted for determinism, via
/// `git for-each-ref`. Empty when git is unavailable or `repo` is not a repository. Used by the
/// conductor's land-refused lesson (spec 103 criterion 8) to search every unit's branch for one
/// whose tip already carries the content a refused landing was blocked by, and by `rigger
/// validate`'s residue scan to flag unit branches no live unit owns.
pub fn unit_branches(repo: impl AsRef<std::ffi::OsStr>) -> Vec<String> {
    let out = crate::subprocess::git_in(repo)
        .args([
            "for-each-ref",
            "--format=%(refname:short)",
            "refs/heads/rigger/u/",
        ])
        .output();
    let mut branches: Vec<String> = match out {
        Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout)
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .map(str::to_string)
            .collect(),
        _ => Vec::new(),
    };
    branches.sort();
    branches
}

/// The name of the branch currently checked out in `repo`, or None on a detached
/// HEAD. An unborn HEAD (a fresh repo with no commit) still reports its default
/// branch name, so this only returns None for a genuinely detached HEAD.
pub fn current_branch(repo: &str) -> Option<String> {
    run_git(repo, &["symbolic-ref", "--short", "-q", "HEAD"])
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

// UNIT_WORKTREE_PREFIX, UNIT_CACHE_PREFIX, unit_cache_sibling, UNIT_MUTANTS_PREFIX and
// unit_sibling are defined in `crate::spawn` (spec 93, criterion 1) rather than
// here: `spawn::WaveItem::from` (a PURE fold, part of the `core` lane) needs
// `unit_cache_sibling`, and this module is `store`-gated (real git/filesystem
// operations) and excluded from `core`. Re-exported so this module's own ~30 call
// sites are unaffected.
pub use crate::spawn::{
    unit_cache_sibling, unit_sibling, UNIT_CACHE_PREFIX, UNIT_MUTANTS_PREFIX, UNIT_WORKTREE_PREFIX,
};

/// The shared gate build cache's directory NAME directly under the scratch root (spec 77
/// Problem statement: the driver's own `CARGO_TARGET_DIR`, observed at up to 39G) - the
/// ambient/inherited target any gate build with no per-unit `target_dir` override
/// ([`unit_cache_sibling`]'s `None` case) builds into. Named ONCE here so `rigger reset
/// --build-cache` (spec 77 criterion 5), the run-teardown reap
/// ([`crate::worktree::shared_build_cache_guard_path`]'s sibling authority) and every
/// shared-lock-holding gate build resolve the identical spelling - never a second,
/// independently-typed literal that could drift.
pub const SHARED_BUILD_CACHE_NAME: &str = "cargo-target";

/// The guard file's path (spec 77 criterion 5, BOUNDED SHARED CACHE): a SIBLING of the
/// shared build cache dir under `scratch_root` - BESIDE it, never inside it, so the guard
/// survives the very rename `rigger reset --build-cache`'s reclaim performs on the cache
/// itself (three rounds of a prior, now-superseded design proved an in-cache lock cannot
/// close this class of race: flock is advisory to lock-takers and never gates unlink, so a
/// lock file that lives inside the directory being renamed/deleted is no protection at
/// all). This is the ONE naming authority both halves of the exclusion protocol resolve
/// through: the exclusive, non-blocking attempt `rigger reset --build-cache` makes, and the
/// shared hold every rigger-launched shared-cache build takes for its whole cargo
/// invocation - so they can never disagree about which file guards which cache.
pub fn shared_build_cache_guard_path(scratch_root: &str) -> String {
    format!("{scratch_root}/{SHARED_BUILD_CACHE_NAME}.lock")
}

/// The scratch root's ONE build location for anything a driver cannot pin (spec 77,
/// criterion 1, the mechanical half): every unit worktree lives directly under the scratch
/// root, and cargo reads `.cargo/config.toml` from every parent directory of its cwd, so a
/// `[build] target-dir` written once at the root catches every cargo run inside a unit
/// worktree that carries no `CARGO_TARGET_DIR` - a worker whose driver could not set the
/// variable, an operator's hand-run test - and sends it to `<root>/cargo-target-shared`
/// instead of `<worktree>/target` (three such 50 GB trees filled the disk on 2026-09-15).
/// The environment variable still wins, so the gates and compliant workers keep their
/// per-unit `cargo-target-<unit>` siblings. Written only when absent, never rewritten: the
/// root is rigger's, but an operator may tune the file. Best-effort by design - a scratch
/// root that cannot take the file (read-only, or a unit dir with no parent) changes nothing
/// about worktree creation, which must go on.
pub const SCRATCH_CARGO_CONFIG: &str = "\
# Written by rigger at scratch-root creation (spec 77, criterion 1): every cargo run inside a
# unit worktree under this root that carries no CARGO_TARGET_DIR builds here, never into
# `<worktree>/target`. The per-unit caches the gates use still win through the environment.
[build]
target-dir = \"cargo-target-shared\"
";

pub fn ensure_scratch_root_cargo_config(worktree_dir: &str) {
    if unit_cache_sibling(worktree_dir).is_none() {
        return;
    }
    let Some(root) = std::path::Path::new(worktree_dir).parent() else {
        return;
    };
    let dir = root.join(".cargo");
    let file = dir.join("config.toml");
    if file.exists() {
        return;
    }
    if std::fs::create_dir_all(&dir).is_ok() {
        let _ = std::fs::write(&file, SCRATCH_CARGO_CONFIG);
    }
}

/// The gate store fence's scratch sibling for a STANDALONE REVIEW worktree at
/// `worktree_dir` (spec 70 criterion 3, widened - u4 round 2 fix for
/// `adv-u3c70-store-fence-half-wired-review-worktree-call-site-unfenced`): a review
/// worktree (`rigger-review-<stage>-<attempt>`) owns no per-unit build cache to key off
/// (unlike [`unit_cache_sibling`]'s unit-worktree case, which `gate::ExecRunner::run`
/// already fences via its non-empty `target_dir`), yet `run_fan_out_stage`'s EXHAUSTIVE
/// gate pass (conductor.rs) still runs real store-opening couriers inside one. This is a
/// direct sibling of the worktree itself - `{worktree_dir}{gate::STORE_FENCE_SUFFIX}` -
/// the same naming shape as the unit-worktree fence sibling, just not routed through a
/// build cache that does not exist for this kind. Returns None for anything that is not a
/// review worktree (a unit worktree - already fenced above - or the empty worktree-less
/// path), which owns no fence sibling here.
pub fn review_fence_sibling(worktree_dir: &str) -> Option<String> {
    let path = std::path::Path::new(worktree_dir);
    let name = path.file_name()?.to_str()?;
    if !name.starts_with("rigger-review-") {
        return None;
    }
    Some(format!("{worktree_dir}{}", crate::gate::STORE_FENCE_SUFFIX))
}

/// Reclaim the per-unit build cache that is a SIBLING of the unit worktree at `worktree_dir`
/// (Gap 19) - the ONE mutation authority for cache reclamation, called from every worktree
/// removal path: [`Worktree::remove`] (the dominant graceful teardown), [`sweep_terminal`]
/// (crash recovery), and [`reclaim_worktree_on_branch`] (the resume-path branch GC). A no-op
/// for any dir that owns no such cache (a review worktree, or a unit whose gates never ran
/// cargo, has none). Best-effort: a failed reclaim of a throwaway cache must never fail
/// worktree teardown or abort the sweep.
///
/// Also reclaims the gate store fence's own sibling scratch dir (spec 70 criterion 3,
/// `cargo-target-<slug>{gate::STORE_FENCE_SUFFIX}`) at the SAME coordinate: `gate::
/// ExecRunner::run` derives it as a further-suffixed sibling of this same cache path
/// whenever a unit-worktree gate runs with a non-empty target_dir (the everyday case), and
/// nothing else on any path ever removes it - left alone, it is a live sqlite events.db
/// (plus WAL/SHM) orphaned forever on every such gate run. Reclaiming it HERE, in the one
/// authority already reclaiming its `cargo-target-<slug>` sibling, means every current and
/// future call site inherits the fix uniformly rather than needing its own copy.
///
/// Widened (spec 70, u4 round 2 fix for
/// `adv-u3c70-reclaim-shares-the-same-exclusion-fix-fence-alone-leaks`): a standalone
/// review worktree owns no `cargo-target-<slug>` cache above, but now that
/// `gate::ExecRunner::run` fences its store resolution too (via [`review_fence_sibling`]),
/// it owns THAT fence sibling and must be reclaimed here in lockstep - `Worktree::remove`
/// runs for both worktree kinds (its own doc comment), so fixing only the fence half
/// without widening this reclaim half in the SAME change would leave a newly-created,
/// previously-nonexistent leak on every review-worktree gate run.
///
/// Reaps each sibling before removing it (spec 79, criterion 1 - "even the exemplar leaks
/// here": [`Worktree::remove`] already reaps the worktree dir itself, but a real gate build
/// pointed at the cache dir, or a fenced courier that opened the fence dir's sqlite store,
/// can still be alive when the worktree it is a sibling of is torn down; a bare removal here
/// would outlive that process's now-deleted cwd exactly like the worktree dir itself would).
///
/// `authorized_root` is the caller's independently-resolved scratch root (spec 79 round-2
/// fix, `arch-u79c1-reap-dir-before-removal-self-authorizes` / `sdet-u79c1-authorized-root-
/// tautology`, both UPHELD): passed straight through to [`reap_dir_before_removal`] for
/// every sibling, NEVER re-derived from `worktree_dir`'s own position. Every call site
/// already has this value to hand ([`Worktree::remove`]/[`Worktree::discard`] carry or take
/// it, [`sweep_terminal`] already resolves it to confirm `d.starts_with(root)`,
/// [`reclaim_worktree_on_branch`]'s caller resolves it the same way `Worktree::create`'s
/// callers do).
fn reclaim_cache_sibling(worktree_dir: &str, authorized_root: &str) {
    if let Some(cache) = unit_cache_sibling(worktree_dir) {
        let fence = format!("{cache}{}", crate::gate::STORE_FENCE_SUFFIX);
        reap_dir_before_removal(&fence, authorized_root);
        let _ = std::fs::remove_dir_all(&fence);
        reap_dir_before_removal(&cache, authorized_root);
        let _ = std::fs::remove_dir_all(&cache);
    }
    if let Some(fence) = review_fence_sibling(worktree_dir) {
        reap_dir_before_removal(&fence, authorized_root);
        let _ = std::fs::remove_dir_all(&fence);
    }
    // The unit-keyed mutants root (spec 91, THE GATE ENVIRONMENT): a THIRD sibling of the
    // unit worktree, on the identical coordinate the cache sibling above already reclaims -
    // widened here, in the ONE reclaim authority, so every current call site (`Worktree::
    // remove`'s dominant graceful path, `sweep_terminal`'s crash recovery, and
    // `reclaim_worktree_on_branch`'s resume-path branch GC) inherits the fix uniformly
    // rather than each needing its own copy. A no-op for anything that owns no such root
    // (mirrors `unit_cache_sibling`'s own `None` cases exactly, since both derive from the
    // same worktree-dir shape).
    if let Some(mutants) = unit_sibling(worktree_dir, UNIT_MUTANTS_PREFIX) {
        reap_dir_before_removal(&mutants, authorized_root);
        let _ = std::fs::remove_dir_all(&mutants);
    }
}

/// The spec-83 (criterion 1) worktree-fence verdict for one unit's LATEST requested spawn:
/// whether the unit's worktree may be reclaimed by a sweep this step runs, and the evidence
/// a log line can name so a vanished (or spared) worktree is attributable from the log
/// afterward.
///
/// A FENCE, not a replacement: a caller consults this only for a unit its OWN liveness
/// signal (the ledger's terminal read, or [`sweep_terminal`]'s own ancestry-merge test)
/// already reads as terminal - this closes the gap where that signal races ahead of a
/// straggler spawn still working the SAME unit (a slower confirmatory review lens after the
/// deciding verdict already integrated the unit - spec 83's `u81c1` observation), not a
/// parallel or overriding notion of "unit in flight".
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SpawnFence {
    /// No spawn has EVER been requested for this unit - the fence has nothing to add; the
    /// caller's own liveness signal decides alone.
    NoSpawn,
    /// The unit's LATEST requested spawn has no recorded result - keep the worktree live
    /// regardless of the caller's own terminal read. A MISSING liveness marker is never
    /// reapable evidence on its own (spec 83 Design): both "no marker yet" and "a fresh
    /// marker" land here, since only a RECORDED RESULT (a real one, or the liveness sweep's
    /// own stale-marker classification) ever moves a spawn out of this arm.
    InFlight { spawn: String },
    /// The unit's LATEST requested spawn's result is recorded at event `position` `at` -
    /// eligible for reclaim. `hung` names whether that result is the liveness sweep's own
    /// stale-marker classification ([`crate::spawn::SpawnResult::is_liveness_fault`]) rather
    /// than a worker- or courier-reported outcome, for a more specific evidence line.
    Terminal {
        spawn: String,
        at: crate::eventstore::Position,
        hung: bool,
    },
}

impl SpawnFence {
    /// Whether this verdict permits a worktree to be reclaimed this sweep: every arm does
    /// except [`SpawnFence::InFlight`] - `NoSpawn` has nothing to fence on, so the caller's
    /// own (pre-spec-83) signal governs alone, exactly as it did before this fence existed.
    pub fn permits_reclaim(&self) -> bool {
        !matches!(self, SpawnFence::InFlight { .. })
    }

    /// The human-readable evidence line named in a sweep's log output for `unit`, so a
    /// worktree's vanish (or its being spared) is attributable from the log after the fact.
    pub fn evidence(&self, unit: &str) -> String {
        match self {
            SpawnFence::NoSpawn => format!("unit {unit:?}: no spawn ever recorded for it"),
            SpawnFence::InFlight { spawn } => {
                format!(
                    "unit {unit:?}: latest spawn {spawn:?} is in flight (no recorded result yet)"
                )
            }
            SpawnFence::Terminal {
                spawn,
                at,
                hung: false,
            } => {
                format!(
                    "unit {unit:?}: latest spawn {spawn:?} terminal (result recorded at position {at})"
                )
            }
            SpawnFence::Terminal {
                spawn,
                at,
                hung: true,
            } => {
                format!(
                    "unit {unit:?}: latest spawn {spawn:?} hung past its max_wall_clock \
                     (the liveness sweep classified it at position {at})"
                )
            }
        }
    }
}

/// Classify `unit`'s LATEST requested spawn (spec 83, criterion 1): the one whose liveness
/// governs whether a worktree candidate the caller already reads as terminal may actually be
/// reclaimed. "Latest" is by REQUEST ORDER in `events` (the last
/// [`crate::spawn::TYPE_SPAWN_REQUESTED`] whose [`crate::spawn::SpawnRequest::unit`] matches) -
/// a unit accumulates one spawn per role per attempt (implementer, reviewer, adversary, ...),
/// and it is the most recently dispatched one that can still be working while an earlier one
/// already answered.
///
/// `events` should already be scoped to the run the caller cares about (e.g.
/// [`crate::run::current_run`]) - an unscoped slice risks matching a PRIOR run's
/// identically-named unit's already-resolved spawn as "the latest", which would wrongly
/// permit a reclaim this fence exists to prevent. Reuses [`crate::spawn::recorded`] /
/// [`crate::spawn::result_of`] (the SAME spawn-request/result authority every other liveness
/// reader folds) rather than re-deriving a second notion of "answered".
pub fn spawn_fence(events: &[Event], unit: &str) -> SpawnFence {
    let mut latest: Option<crate::spawn::SpawnRequest> = None;
    for e in events {
        if e.type_ == crate::spawn::TYPE_SPAWN_REQUESTED {
            if let Ok(req) = crate::spawn::SpawnRequest::from_event(e) {
                if req.unit == unit {
                    latest = Some(req);
                }
            }
        }
    }
    let Some(req) = latest else {
        return SpawnFence::NoSpawn;
    };
    match crate::spawn::result_of(events, &req.id) {
        Ok(Some(res)) => {
            // The position of the LATEST result event for this id - mirrors `result_of`'s
            // own "later results win" fold (last-write-wins), just walked in reverse to stop
            // at the first (i.e. latest) match instead of folding every candidate.
            let at = events
                .iter()
                .rev()
                .find(|e| {
                    e.type_ == crate::spawn::TYPE_SPAWN_RESULT
                        && crate::spawn::SpawnResult::from_event(e).is_ok_and(|r| r.id == req.id)
                })
                .map(|e| e.position)
                .unwrap_or_default();
            SpawnFence::Terminal {
                spawn: req.id,
                at,
                hung: res.is_liveness_fault(),
            }
        }
        // A malformed result body degrades identically to "no result yet" - the same
        // conservative direction `result_of`'s own callers already take on decode failure.
        _ => SpawnFence::InFlight { spawn: req.id },
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
/// `live_branches` is the `rigger/u/<slug>` set of the CURRENT run's non-terminal units (the
/// same run-scoped fold the conductor already reads to decide liveness elsewhere - see
/// `current_run_units` in `main.rs`), never a process-memory list. The merged-only ancestry
/// rule alone is not sufficient: a PARKED unit whose attempt produced an EMPTY diff has a
/// branch tip that IS an ancestor of `run_branch` (trivially - it never advanced past it)
/// while the unit is still live in review, so `live_branches` is checked BEFORE the ancestry
/// test and spares such a worktree outright; a merged-or-dead, not-live worktree is still
/// reclaimed exactly as before.
///
/// `events` is the SAME current-run-scoped slice `live_branches` was folded from (spec 83,
/// criterion 1: THE FENCE). Both pre-existing signals above can still read a branch as
/// terminal while a STRAGGLER spawn for the identical unit keeps working the very worktree
/// this loop is about to remove (the deciding verdict integrates the unit while a slower
/// confirmatory review lens is still running - the observed `u81c1` bug); [`spawn_fence`]
/// closes that gap by consulting the unit's LATEST requested spawn directly. A branch with NO
/// recorded spawn at all ([`SpawnFence::NoSpawn`]) sweeps exactly as before -
/// the fence has nothing to add and must never itself become a reason dead residue lingers.
/// Every fence-relevant decision (kept in flight, or removed with its terminal/hung evidence)
/// is printed, so a worktree's vanish - or its being spared - is attributable from the step's
/// own log output after the fact.
///
/// `declared_units` (spec 89, criterion 1, round 2 fix) is the `rigger/u/<slug>` set of the
/// CURRENTLY LOADED workflow's own stages - config, never the event log - so it stays
/// populated even at this project's very first step, before a single event has ever been
/// recorded. It narrows ONLY the dirty-spare exception below to a unit THIS workflow actually
/// declares: an unrelated, genuinely dead branch (a prior run's leftover, a hand-made test
/// fixture) that happens to also be dirty is still reclaimed exactly as before this fix -
/// dirtiness alone is not evidence of a halted spawn worth protecting; dirtiness on a branch
/// this run's own definition still claims is.
pub fn sweep_terminal(
    repo: &str,
    root: &str,
    run_branch: &str,
    live_branches: &std::collections::HashSet<String>,
    declared_units: &std::collections::HashSet<String>,
    events: &[Event],
) -> Result<usize, Error> {
    sweep_terminal_logged(
        repo,
        root,
        run_branch,
        live_branches,
        declared_units,
        events,
        &mut |line| eprintln!("{line}"),
    )
}

/// [`sweep_terminal`]'s real body, with its evidence lines routed through an injected `log`
/// sink instead of a hardcoded `eprintln!` (strict DI, per this crate's own discipline: no
/// hardcoded I/O a test cannot observe) - production wires stderr; the fence's own test
/// module wires a `Vec<String>` collector so a KEPT vs. REMOVED decision's evidence text is
/// itself an assertable fact, not merely a side effect no test can see.
pub fn sweep_terminal_logged(
    repo: &str,
    root: &str,
    run_branch: &str,
    live_branches: &std::collections::HashSet<String>,
    declared_units: &std::collections::HashSet<String>,
    events: &[Event],
    log: &mut dyn FnMut(&str),
) -> Result<usize, Error> {
    // Serialize this WHOLE sweep - the prune below and every candidate's own `git worktree
    // remove --force` in the loop - against every other in-process admin-directory mutation
    // this process makes for the SAME repository (spec 103 checkin round 4; see
    // `repo_admin_lock`'s doc comment).
    let repo_lock = repo_admin_lock(repo);
    let _repo_guard = repo_lock.lock().unwrap();
    git(repo, &["worktree", "prune"])?;
    let out = run_git(repo, &["worktree", "list", "--porcelain"]).map_err(Error)?;
    let mut removed = 0;
    let mut dir: Option<String> = None;
    for line in out.lines() {
        if let Some(d) = line.strip_prefix("worktree ") {
            dir = Some(d.to_string());
        } else if let Some(branch) = line.strip_prefix("branch refs/heads/") {
            let Some(d) = dir.take() else { continue };
            if !d.starts_with(root) || branch == run_branch || live_branches.contains(branch) {
                continue;
            }
            let merged =
                run_git(repo, &["merge-base", "--is-ancestor", branch, run_branch]).is_ok();
            if merged {
                // A HALT NEVER DISCARDS A TREE (spec 89, criterion 1), the ordering contract
                // between this sweep and `run_single_stage`'s halted-commit recovery
                // (src/conductor.rs): that recovery captures a prior incarnation's abandoned
                // edit as its own `wip` commit the instant a unit's worktree is adopted, but
                // `cmd_step` (main.rs) runs THIS sweep strictly BEFORE it ever gets that
                // chance. A candidate that still carries uncommitted changes - an in-progress
                // merge or a real content conflict included, since either always leaves the
                // tree dirty - has not yet had its edit captured, so removing it here would
                // discard it outright rather than merely defer the capture. This spares the
                // candidate regardless of the fence below (even `NoSpawn`, which normally
                // defers entirely to the ancestry signal): a store desynced from the worktree
                // on disk - a restored snapshot, or this project's very first step, adopting a
                // worktree that already exists - never gets a chance to record a spawn before
                // this sweep runs, so the fence alone cannot protect it. An unreadable status
                // (`run_git` errors) is treated as dirty too - liveness here can only be
                // under-, never over-determined, exactly like `live_branches_for_sweep`'s own
                // fail-closed read one call site up. A clean worktree in this same shape
                // (`sweep_terminal_reclaims_a_merged_branch_with_no_spawn_recorded_at_all_
                // unchanged`) is unaffected: it sweeps exactly as it did before this fix.
                //
                // Gated on `declared_units` too (round 2 fix,
                // `step_start_sweep_spares_a_live_units_empty_diff_worktree_but_reclaims_a_dead_
                // ancestor_leftover`): dirtiness ALONE is not proof of a halted spawn worth
                // protecting - a genuinely dead, unrelated branch (a prior run's leftover
                // registration, a hand-made fixture) this workflow never declared is just as
                // dirty-looking and must still be reclaimed exactly as before this criterion; a
                // branch this run's OWN definition still claims as one of its units is the one
                // worth deferring for.
                //
                // The status read itself goes through [`path_is_dirty`] (round 3 fix,
                // `arch-u89c1r2-dirty-check-duplicated-and-diverges-fail-direction`) - the same
                // shared primitive `reclaim_orphan_scratch` (main.rs) now calls too, rather than
                // each growing its own inline `git status` call that can silently pick a
                // different failure direction. `unwrap_or(true)`: an unreadable status fails
                // CLOSED (dirty), never open - see that function's own doc comment for why.
                let dirty = declared_units.contains(branch) && path_is_dirty(&d).unwrap_or(true);
                if dirty {
                    log(&format!(
                        "rigger step: worktree sweep: kept {d:?} (branch {branch:?}) - \
                         uncommitted changes are pending the halt-recovery commit"
                    ));
                    continue;
                }
                // THE FENCE (spec 83, criterion 1): the unit id doubles as the branch's
                // `rigger/u/<slug>` tail - the same assumption `current_run_units`'
                // dead/live-slug split already makes for a branch in this exact shape.
                let unit = branch.strip_prefix("rigger/u/").unwrap_or(branch);
                let fence = spawn_fence(events, unit);
                if !fence.permits_reclaim() {
                    log(&format!(
                        "rigger step: worktree sweep: kept {d:?} (branch {branch:?}) - {}",
                        fence.evidence(unit)
                    ));
                    continue;
                }
                if !matches!(fence, SpawnFence::NoSpawn) {
                    log(&format!(
                        "rigger step: worktree sweep: removing {d:?} (branch {branch:?}) - {}",
                        fence.evidence(unit)
                    ));
                }
                // Reap any process rooted inside this terminal worktree BEFORE removing it
                // (spec 79, criterion 1): a crashed step process can leave a build or tool
                // still running here, and this is the CRASH-recovery path, not the graceful
                // `Worktree::remove` one - nothing else reaps it. `root` is the SAME resolved
                // scratch root already used to confirm `d.starts_with(root)` above.
                crate::reap::reap_processes_rooted_under(
                    std::path::Path::new(&d),
                    std::path::Path::new(root),
                );
                git(repo, &["worktree", "remove", "--force", &d])?;
                reclaim_cache_sibling(&d, root);
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
pub fn worktree_on_branch(dir: &str, branch: &str) -> bool {
    std::path::Path::new(dir).is_dir() && current_branch(dir).as_deref() == Some(branch)
}

/// The dir of the worktree that already has `branch` checked out, if any - parsed
/// from `git worktree list --porcelain` (a `worktree <dir>` line followed by its
/// `branch refs/heads/<name>` line). Registrations whose dirs were deleted out from
/// under git still appear here; the caller decides adopt-vs-prune by checking the dir.
///
/// `pub` (spec 83 round 3): `conductor.rs::gc_integrated_branches_logged` uses this
/// as its own presence check before printing "removing" evidence, mirroring `sweep_
/// terminal_logged`'s identical `git worktree list --porcelain`-driven candidate set -
/// never a second, parallel notion of "is this worktree still here".
pub fn registered_worktree_for(repo: &str, branch: &str) -> Option<String> {
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
/// [`Worktree::create`] and to reset a throwaway review worktree in [`Worktree::discard`],
/// and (via [`reclaim_worktree_on_branch`]) to tear down a lingering worktree on resume.
///
/// Reaps whatever is rooted inside `dir` FIRST (spec 79, criterion 1): whichever teardown
/// path the caller ends up on - `Worktree::create`'s self-heal, `Worktree::discard`'s reset,
/// or [`reclaim_worktree_on_branch`]'s resume-path reclaim - a build, tool, or courier a
/// prior process left running inside `dir` must not outlive the dir holding a now-deleted
/// cwd. `authorized_root` is threaded straight through to [`reap_dir_before_removal`] - see
/// that function's doc comment for why it must be the CALLER's independently-resolved root,
/// never derived from `dir` itself.
///
/// CALLER-LOCKED (spec 103 checkin round 4): this mutates the repository's worktree admin
/// directory (`git worktree remove --force` / `git worktree prune`), so every call site -
/// `Worktree::create`, `Worktree::discard`, [`reclaim_worktree_on_branch`] - already holds
/// `repo_admin_lock(repo)` across its own whole call before reaching here. This function
/// itself must NEVER take that lock: `std::sync::Mutex` is not reentrant, and doing so would
/// deadlock every one of its callers against itself.
fn clear_worktree_dir(repo: &str, dir: &str, authorized_root: &str) -> Result<(), Error> {
    reap_dir_before_removal(dir, authorized_root);
    if run_git(repo, &["worktree", "remove", "--force", dir]).is_err()
        && std::path::Path::new(dir).exists()
    {
        std::fs::remove_dir_all(dir)
            .map_err(|e| Error(format!("remove leftover worktree dir {dir}: {e}")))?;
    }
    git(repo, &["worktree", "prune"])?;
    Ok(())
}

/// Reap every process rooted inside `dir` (spec 79, criterion 1) before a caller in this
/// module removes it, gated by [`crate::reap::reap_processes_rooted_under`]'s usual
/// strictly-under-`authorized_root` containment check.
///
/// `authorized_root` MUST be a value the CALLER independently resolved via the SAME
/// authority it already used to build `dir` itself (`scratch_root_from_env`/
/// `scratch_root_path_from_env`, or an equivalent caller-side resolution) - spec 79
/// round-2 fix, decision `u79c1r2-reap-dir-before-removal-authorized-root-param`
/// (supersedes the round-1 shape). The prior round-1 version of this function computed
/// `dir.parent()` and used THAT as the authorized root: a canonicalized path always
/// `starts_with` its own canonicalized parent, so that containment check was a
/// TAUTOLOGY - it could never refuse, for any `dir` with a parent, regardless of
/// whether `dir` was actually placed under the run's real, independently-resolved
/// scratch root (reviewed and rejected: `arch-u79c1-reap-dir-before-removal-self-
/// authorizes` / `sdet-u79c1-authorized-root-tautology`, both UPHELD - mirrors spec
/// 78 round 2's `u78c2r2-authorized-root-caller-supplied`, which this function now
/// finally also honors: "a root the CALLER resolves and supplies... never re-derived
/// here from base's own git/filesystem position"). An empty `authorized_root` (no such
/// root to hand, e.g. a test scaffold that never exercises this boundary) is a no-op:
/// [`crate::reap::processes_rooted_under`]'s canonicalize of `""` fails to resolve,
/// which the containment gate already reads as "nothing to authorize", so it refuses
/// safely rather than reaping unconditionally.
fn reap_dir_before_removal(dir: &str, authorized_root: &str) {
    if authorized_root.is_empty() {
        return;
    }
    crate::reap::reap_processes_rooted_under(
        std::path::Path::new(dir),
        std::path::Path::new(authorized_root),
    );
}

/// Prune PROVABLY-CORRUPT worktree admin entries before a `git worktree add`, so one
/// crashed worktree lifecycle can never permanently block every later add (spec 51).
///
/// A `git worktree remove` (or a bare-directory sweep) killed mid-flight can leave an admin
/// entry under `<git-common-dir>/worktrees/<name>/` whose `commondir` (or `gitdir`) marker
/// is truncated to ZERO length. git reads EVERY admin entry up-front on any worktree
/// command, so a single such entry makes every subsequent `git worktree add` hard-fail
/// (`fatal: failed to read .git/worktrees/<name>/commondir`, exit 128) - and even
/// `git worktree list` / `git worktree prune` fail the same way, so git's own prune cannot
/// recover it and the corrupt entry must be removed off disk directly.
///
/// The metadata dir is located via `git rev-parse --git-common-dir`, which does NOT read
/// the per-worktree admin entries and so still succeeds under the corruption. The healing is
/// NARROW: only a PROVABLY-corrupt entry is removed - one whose `commondir` OR `gitdir`
/// marker is missing or zero-length; a healthy registered worktree (both markers present and
/// non-empty) is never touched. Best-effort and non-failing, mirroring the sweep helpers: a
/// repo with no linked worktrees (no metadata dir) is a no-op.
///
/// reap-exempt (spec 79, criterion 2): the dir this removes is one worktree's admin
/// METADATA entry under `<git-common-dir>/worktrees/<name>/` (a few marker files git itself
/// reads), never a process's working directory - no process ever has its cwd inside a git
/// admin dir - and it is removed only when [`worktree_admin_is_corrupt`] has already proven
/// the entry provably corrupt (a marker file missing or zero-length). Nothing hostable, so no
/// reap is needed here.
///
/// CALLER-LOCKED (spec 103 checkin round 4): its only production call site
/// (`Worktree::create`) already holds `repo_admin_lock(repo)` across its whole call before
/// reaching here. This function itself must NEVER take that lock: `std::sync::Mutex` is not
/// reentrant, and doing so would deadlock `Worktree::create` against itself. (Its unit tests
/// below call it directly, bypassing `Worktree::create` and the lock entirely - that is fine,
/// they exercise the heal predicate in isolation, not the concurrency guard.)
pub fn heal_corrupt_worktree_admin(repo: &str) {
    let Ok(common) = run_git(repo, &["rev-parse", "--git-common-dir"]) else {
        return;
    };
    let common = common.trim();
    let common_path = {
        let p = std::path::Path::new(common);
        if p.is_absolute() {
            p.to_path_buf()
        } else {
            std::path::Path::new(repo).join(p)
        }
    };
    let Ok(entries) = std::fs::read_dir(common_path.join("worktrees")) else {
        return;
    };
    for entry in entries.flatten() {
        let admin = entry.path();
        if admin.is_dir() && worktree_admin_is_corrupt(&admin) {
            let _ = std::fs::remove_dir_all(&admin);
        }
    }
}

/// Per-repository in-process mutual exclusion across EVERY in-process mutation of a
/// repository's worktree admin directory (`.git/worktrees/<name>/`) - originally scoped to
/// just [`heal_corrupt_worktree_admin`] and the `git worktree add` it guards (spec 103
/// criterion 4), widened at the whole-spec checkin seam (round 4) once a second admin-
/// directory writer, [`Worktree::discard`], was found racing a sibling's [`Worktree::create`]
/// in the same `run_batch` wave (`adv-checkin-r3-discard-vs-create-race-flakes-the-new-soak-
/// test`): `git worktree prune`/`git worktree remove --force`/`git worktree add` all read
/// and rewrite the SAME admin directory, so every one of them - not just `add` - must be
/// serialized against every other. [`Worktree::create`] is a plain associated function with
/// no owning instance - `run_batch` spawns one real OS thread per concurrent unit in a wave
/// and each calls `create` independently against the SAME shared repository, so nothing
/// before criterion 4 serialized one thread's heal scan against a sibling thread's in-flight
/// `git worktree add` writing into that same admin directory (the exact shape the Goal
/// names: "a batch-mate's add on a concurrent thread... is deleted mid-write"); nothing
/// before this round serialized that same heal scan / `add` against a DIFFERENT thread's
/// `discard`-driven `git worktree prune` or `remove --force` doing the same thing from the
/// other direction. [`Worktree::ensure_present`]'s own `reassert_mu` is a DIFFERENT,
/// narrower lock - per-worktree instance, serializing only concurrent re-asserts of ONE
/// already-created `Worktree`; this one is per-REPOSITORY, serializing every admin-directory
/// mutation this process makes against that repo, whichever unit, instance, or call site it
/// is for.
///
/// Every public entry point that mutates the admin directory takes this ONCE, for its whole
/// call: [`Worktree::create`], [`Worktree::discard`], [`Worktree::remove`],
/// [`sweep_terminal_logged`], and [`reclaim_worktree_on_branch`]. The helpers those
/// entry points call - [`clear_worktree_dir`] and [`heal_corrupt_worktree_admin`] - are
/// deliberately left LOCK-FREE: the underlying `std::sync::Mutex` is not reentrant, and
/// every caller of either helper already holds this lock across the helper's call, so a
/// helper that also locked would deadlock its own caller. Never add a new admin-directory
/// mutation site without taking this lock at ITS public entry point first.
///
/// This is defense IN ADDITION TO the heal predicate's own `locked`/grace-period guards
/// above, never a replacement for them: a lock held by THIS process cannot serialize
/// against a `git worktree` command some OTHER process runs against the same repository (a
/// second `rigger step`, or an operator's own `git` invocation) - only the marker git
/// itself writes into the entry is authoritative across process boundaries.
///
/// Keyed by the repo path exactly as the caller spells it (never canonicalized): every
/// caller in this codebase already resolves and passes one consistent spelling for a given
/// repository across a process's lifetime, so two different spellings of the same physical
/// path never legitimately arise here; two DIFFERENT repositories simply get two different
/// map entries and never contend on each other's lock. Mirrors the existing
/// `static TMP_NONCE: AtomicU64` synchronization primitive in `src/registry.rs` - an
/// internal concurrency detail, not an injected dependency.
fn repo_admin_lock(repo: &str) -> std::sync::Arc<std::sync::Mutex<()>> {
    static LOCKS: std::sync::OnceLock<
        std::sync::Mutex<std::collections::HashMap<String, std::sync::Arc<std::sync::Mutex<()>>>>,
    > = std::sync::OnceLock::new();
    let registry = LOCKS.get_or_init(|| std::sync::Mutex::new(std::collections::HashMap::new()));
    registry
        .lock()
        .unwrap()
        .entry(repo.to_string())
        .or_insert_with(|| std::sync::Arc::new(std::sync::Mutex::new(())))
        .clone()
}

/// An admin entry must sit unwritten-to for this long before the heal will touch it (spec
/// 103 criterion 4). git's own five-file write (mkdir, `locked`, `gitdir`, `HEAD`,
/// `commondir`) is not atomic, so a scan landing mid-write sees the SAME missing-marker
/// shape as a genuinely abandoned entry; only age tells the two apart.
const HEAL_GRACE_PERIOD: std::time::Duration = std::time::Duration::from_secs(60);

/// Whether a worktree admin-entry directory is PROVABLY corrupt AND SAFE TO HEAL RIGHT NOW:
/// its `commondir` or `gitdir` marker file is MISSING or ZERO-LENGTH - the exact residue a
/// killed `git worktree remove` leaves, and precisely what makes git's up-front admin-entry
/// read fail - AND it carries neither of the two signs of a live, in-flight `git worktree
/// add` (spec 103 criterion 4, gap 57 third hit: `checkin94-gap57-root-fix-moves-to-
/// spec-103`):
///
/// - a `locked` marker: git writes this into an entry mid-add and its OWN `worktree prune`
///   already refuses to touch a locked entry for exactly this reason - an entry carrying it
///   is honored the same way here, never healed regardless of its other markers' state.
/// - younger than [`HEAL_GRACE_PERIOD`]: git's five-file write is not atomic, so a scan
///   landing between two of those writes sees a legitimately in-flight add as
///   indistinguishable from an abandoned one; the entry directory's own mtime (bumped by
///   every file git writes into it) is the recency signal, and metadata this fresh survives
///   even with a marker missing. A metadata read that fails outright (the entry vanished
///   mid-scan, or a permissions race) is treated the same as "too young to prove" - never
///   healed on an unprovable age.
///
/// A healthy entry always has both markers present and non-empty, so this never flags a
/// live worktree regardless of age or lock state.
fn worktree_admin_is_corrupt(admin: &std::path::Path) -> bool {
    if admin.join("locked").exists() {
        return false;
    }
    let old_enough = std::fs::metadata(admin)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|mtime| std::time::SystemTime::now().duration_since(mtime).ok())
        .is_some_and(|age| age >= HEAL_GRACE_PERIOD);
    if !old_enough {
        return false;
    }
    ["commondir", "gitdir"]
        .iter()
        .any(|marker| std::fs::metadata(admin.join(marker)).map_or(true, |m| m.len() == 0))
}

/// Tear down any scratch worktree still CHECKED OUT on `branch`, then reclaim its sibling
/// per-unit build cache - the branch-keyed half of the ordered teardown that both
/// [`Worktree::remove`] (the graceful `run_stage` path) and [`sweep_terminal`] (crash
/// recovery) already perform. git REFUSES to delete a branch that is still checked out in
/// a worktree, so branch-GC on resume must remove the lingering worktree FIRST or the
/// `git branch -D` fails and BOTH the branch and the worktree survive as per-unit debris.
///
/// The residue this reclaims: a step process killed between its `UnitIntegrated` emit and
/// [`Worktree::remove`] leaves a worktree still registered on the unit's now-integrated
/// branch. The reaper ([`sweep_terminal`]) runs only on the `rigger step` path, so on the
/// `rigger run` resume path the branch-GC must reclaim it here - the spec's "and remove
/// its worktree if the reaper has not".
///
/// This composes the EXISTING single-authorities, minting no parallel teardown:
/// [`registered_worktree_for`] finds the dir, [`clear_worktree_dir`] deregisters it
/// (tolerating both a still-tracked worktree and a bare leftover dir, and pruning a stale
/// registration whose dir was deleted out from under git - so even a residue that no
/// longer occupies disk stops holding the branch), and [`reclaim_cache_sibling`] reclaims
/// the multi-gigabyte `cargo-target-<slug>` cache, exactly as the two teardowns above do.
/// A branch with no lingering worktree is a graceful no-op.
///
/// The owning STEP process is dead on this path, but that is not the same as "nothing is
/// rooted in the dir" (spec 79, criterion 1 - the prior wording here claimed exactly that,
/// and the spec's Goal names it wrong): a build, test binary, or dash the dead step's own
/// gate spawned can independently outlive it, so [`clear_worktree_dir`] and
/// [`reclaim_cache_sibling`] both reap whatever they find rooted inside before removing
/// anything, exactly as [`Worktree::remove`]'s live in-window teardown does.
///
/// `authorized_root` is the caller's independently-resolved scratch root (spec 79 round-2
/// fix), threaded straight through to both [`clear_worktree_dir`] and
/// [`reclaim_cache_sibling`] - resolved by the caller via the SAME authority
/// [`Worktree::create`]'s own callers already use, never re-derived from `dir`.
pub fn reclaim_worktree_on_branch(
    repo: &str,
    branch: &str,
    authorized_root: &str,
) -> Result<(), Error> {
    // Serialize this WHOLE call against every other in-process admin-directory mutation
    // this process makes for the SAME repository (spec 103 checkin round 4: this call's own
    // `clear_worktree_dir` below writes into `.git/worktrees` exactly like `Worktree::
    // create`'s heal-scan-then-add does; see `repo_admin_lock`'s doc comment).
    let repo_lock = repo_admin_lock(repo);
    let _repo_guard = repo_lock.lock().unwrap();
    if let Some(dir) = registered_worktree_for(repo, branch) {
        clear_worktree_dir(repo, &dir, authorized_root)?;
        reclaim_cache_sibling(&dir, authorized_root);
    }
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
    rev_sha_of(dir, "HEAD")
}

/// The sha `rev` resolves to in `dir`, deliberately non-failing: an empty `dir` (a repo-less /
/// worktree-less run) or an unresolvable `rev` yields an empty string, never an error. The one
/// resolver behind [`head_sha_of`] (the COMMIT sha) and the [`HEAD_TREE`] tree address.
pub fn rev_sha_of(dir: &str, rev: &str) -> String {
    if dir.is_empty() {
        return String::new();
    }
    run_git(dir, &["rev-parse", rev])
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
/// Deliberately non-failing, mirroring [`head_sha_of`] via [`rev_sha_of`]: an empty `dir` (a repo-less /
/// worktree-less gate run) or an unresolvable HEAD yields an empty string, which the
/// caller reads as "no tree to address" and simply skips content-addressing - never an
/// error that fails the run.
pub const HEAD_TREE: &str = "HEAD^{tree}";

pub fn git(dir: &str, args: &[&str]) -> Result<String, Error> {
    run_git(dir, args).map_err(|out| Error(format!("git {}: {out}", args.join(" "))))
}

/// Whether the git worktree rooted at `dir` has uncommitted changes (a dirty tree) - the
/// single `git status --porcelain -z` primitive [`sweep_terminal_logged`] and `main.rs`'s
/// `reclaim_orphan_scratch` share (spec 89 round 3,
/// `arch-u89c1r2-dirty-check-duplicated-and-diverges-fail-direction`). Round 2 had grown TWO
/// separate inline `git status` calls at those last two sites instead of reusing the one
/// abstraction already sitting right here, private to this module - and the pair silently
/// diverged on which way to fail when the status read itself fails: `sweep_terminal_logged`'s
/// treated an unreadable status as dirty (spare the tree), `reclaim_orphan_scratch`'s treated
/// the IDENTICAL failure as clean (discard it), despite a doc comment on the latter claiming to
/// mirror the former. `pub`, not `pub(crate)`, because `main.rs` is a separate binary crate
/// that can only reach this module through `rigger::worktree::*` (see [`branch_tip`],
/// [`ref_resolves`], [`path_in_ref`] for the same cross-crate shape).
///
/// Returns `Err` exactly like the `git`/`run_git` primitives this is built on, so a caller
/// picks its own fail direction explicitly rather than this function silently picking one for
/// everybody. Both call sites this round fixes now pick the SAME direction on purpose -
/// `unwrap_or(true)`, fail CLOSED, an unreadable status counts as dirty - because "A HALT NEVER
/// DISCARDS A TREE" (spec 89, criterion 1) means an unreadable tree must default to "protect
/// it", never "safe to remove".
pub fn path_is_dirty(dir: &str) -> Result<bool, Error> {
    Ok(!git(dir, &["status", "--porcelain", "-z"])?.is_empty())
}

pub fn run_git(dir: &str, args: &[&str]) -> Result<String, String> {
    let out = crate::subprocess::git_in(dir)
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
