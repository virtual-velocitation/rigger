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
    pub fn integrate(&self, message: &str) -> Result<String, Error> {
        let committed = self.commit(message)?;
        // Resolve the commit to merge: a fresh commit from this call, otherwise the
        // branch's current HEAD (the pre-committed, already-gated artifact). When the
        // branch never advanced past the base there is nothing to integrate.
        let commit = if committed.is_empty() {
            let head = git(&self.dir, &["rev-parse", "HEAD"])?.trim().to_string();
            let base = git(&self.repo, &["rev-parse", "HEAD"])?.trim().to_string();
            if head == base {
                return Ok(String::new());
            }
            head
        } else {
            committed
        };
        git(&self.repo, &["merge", "--no-edit", &self.branch])?;
        Ok(commit)
    }

    /// Delete the worktree (its branch is left for the caller to clean up).
    pub fn remove(&self) -> Result<(), Error> {
        git(&self.repo, &["worktree", "remove", "--force", &self.dir])?;
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
    fn integrate_lands_work_in_the_repo() {
        let repo = init_repo();
        let repo_path = repo.path().to_str().unwrap().to_string();
        let wt_path = std::env::temp_dir().join(format!("rigger-wt-{}", uuid::Uuid::new_v4()));
        let wt = Worktree::create(&repo_path, wt_path.to_str().unwrap(), "rigger/test").unwrap();

        std::fs::write(wt_path.join("feature.txt"), "work\n").unwrap();
        assert_eq!(wt.changed_files().unwrap(), ["feature.txt"]);

        let commit = wt.integrate("rigger: integrate test").unwrap();
        assert!(!commit.is_empty(), "a commit hash should be returned");
        assert!(
            repo.path().join("feature.txt").exists(),
            "the agent's work must be merged into the repo"
        );
        wt.remove().unwrap();
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

        let merged = wt.integrate("rigger: integrate").unwrap();
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
}
