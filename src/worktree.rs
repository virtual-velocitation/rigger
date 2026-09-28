//! The git worktree adapter's unit tests. The adapter itself is
//! [`rigger_worktree_git::worktree`], re-exported here under its historical path.

pub use rigger_worktree_git::worktree::*;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eventstore::Event;
    use crate::liveness::marker_filename;
    use crate::spawn::SpawnEvent;
    use crate::test_support::assert_concurrent_creates_succeed;
    use crate::test_support::assert_teardown_reaps_what_is_rooted_inside;
    use crate::test_support::commit_at_fixed_date;
    use crate::test_support::run_log;

    /// Test-only recomposition of [`Worktree::merge_into_worktree`] + [`Worktree::land`] into
    /// the single combined call this file's OWN pre-round-4 tests were written against (spec
    /// 88, criterion 1 round 4): a plain merge-then-land, matching the production shape
    /// `integrate_and_emit` used before it split the two so it could bracket each with its own
    /// durable row-level record. Kept HERE, test-scoped, rather than in production - production
    /// has no caller for the combined form any more (only this module's tests did, which the
    /// dead-code audit would otherwise flag as a real production surface with zero real
    /// callers) - so the tests that genuinely want to exercise the combined merge+land behavior
    /// end to end (crash-resume idempotency, conflict-leaves-markers-in-place, a non-content
    /// merge failure surfacing) keep doing so through one call, unchanged.
    enum IntegrateOutcome {
        Merged(String),
        Conflict(Vec<String>),
    }

    impl IntegrateOutcome {
        fn expect_merged(self) -> String {
            match self {
                IntegrateOutcome::Merged(sha) => sha,
                IntegrateOutcome::Conflict(paths) => {
                    panic!(
                        "expected a clean merge, got a conflict in: {}",
                        paths.join(", ")
                    )
                }
            }
        }
    }

    trait IntegrateForTest {
        fn integrate(&self, message: &str) -> Result<IntegrateOutcome, Error>;
    }

    impl IntegrateForTest for Worktree {
        fn integrate(&self, message: &str) -> Result<IntegrateOutcome, Error> {
            match self.merge_into_worktree(message)? {
                MergeOutcome::Conflict(paths) => Ok(IntegrateOutcome::Conflict(paths)),
                MergeOutcome::Ready(commit) => {
                    if !commit.is_empty() && self.land()? == LandOutcome::TipMoved {
                        return Err(Error("run tip moved under the test".into()));
                    }
                    Ok(IntegrateOutcome::Merged(commit))
                }
            }
        }
    }

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

    /// Set `path`'s mtime to `secs_ago` seconds in the past (spec 103 criterion 4), so a
    /// heal grace-period check reads it as old without the test actually sleeping. Works on
    /// a directory too: `File::open` succeeds read-only on a directory on Unix, and
    /// `set_modified` only needs an open handle, never write access to its contents.
    fn backdate(path: &std::path::Path, secs_ago: u64) {
        let target = std::time::SystemTime::now() - std::time::Duration::from_secs(secs_ago);
        std::fs::File::open(path)
            .unwrap()
            .set_modified(target)
            .unwrap();
    }

    /// A worktree on `branch` in a fresh temp dir outside any scratch root.
    fn temp_wt(repo_path: &str, branch: &str) -> (std::path::PathBuf, Worktree) {
        let wt_path = std::env::temp_dir().join(format!("rigger-wt-{}", uuid::Uuid::new_v4()));
        let wt = Worktree::create(repo_path, wt_path.to_str().unwrap(), branch, "").unwrap();
        (wt_path, wt)
    }

    /// A fresh repo, its path and its scratch root; `on_run_branch` first checks out the
    /// `rigger-run` branch every `sweep_terminal` test sweeps against.
    fn scratch_repo(on_run_branch: bool) -> (tempfile::TempDir, String, String) {
        let repo = init_repo();
        let repo_path = repo.path().to_str().unwrap().to_string();
        if on_run_branch {
            run_git(&repo_path, &["checkout", "-b", "rigger-run"]).unwrap();
        }
        let root = scratch_root(&repo_path, "", None);
        (repo, repo_path, root)
    }

    /// Create the worktree `{root}/{name}` on `branch`, returning its dir.
    fn wt_at(repo_path: &str, root: &str, name: &str, branch: &str) -> (String, Worktree) {
        let dir = format!("{root}/{name}");
        let wt = Worktree::create(repo_path, &dir, branch, "").unwrap();
        (dir, wt)
    }

    /// Create the unit worktree `{root}/rigger-wt-{slug}` on `rigger/u/{slug}`.
    fn unit_wt(repo_path: &str, root: &str, slug: &str) -> (String, Worktree) {
        wt_at(
            repo_path,
            root,
            &format!("{UNIT_WORKTREE_PREFIX}{slug}"),
            &format!("rigger/u/{slug}"),
        )
    }

    /// Create `dir` (with its parents) holding an `x` file for each of `files`.
    fn populate(dir: &str, files: &[&str]) {
        std::fs::create_dir_all(dir).unwrap();
        for f in files {
            std::fs::write(std::path::Path::new(dir).join(f), "x").unwrap();
        }
    }

    fn exists(path: &str) -> bool {
        std::path::Path::new(path).exists()
    }

    /// [`sweep_terminal`] over `root` against `rigger-run`, with no live branch or declared unit.
    fn sweep(repo_path: &str, root: &str, events: &[Event]) -> usize {
        sweep_terminal(
            repo_path,
            root,
            "rigger-run",
            &std::collections::HashSet::new(),
            &std::collections::HashSet::new(),
            events,
        )
        .unwrap()
    }

    /// [`sweep`] through `sweep_terminal_logged`, also returning the evidence lines it printed.
    fn sweep_logged(repo_path: &str, root: &str, events: &[Event]) -> (usize, Vec<String>) {
        let mut lines = Vec::new();
        let removed = sweep_terminal_logged(
            repo_path,
            root,
            "rigger-run",
            &std::collections::HashSet::new(),
            &std::collections::HashSet::new(),
            events,
            &mut |l| lines.push(l.to_string()),
        )
        .unwrap();
        (removed, lines)
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
        let wt =
            Worktree::create(&repo_path, wt_path.to_str().unwrap(), "rigger/test", "").unwrap();

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
    fn integrate_conflict_merges_the_run_branch_into_the_worktree_leaving_the_unit_branch_untouched(
    ) {
        // Spec 88, criterion 1 (INTEGRATE-CONFLICT MERGES): the unpredicted-overlap case the
        // spec-13 dogfood hit - two units the partitioner placed in ONE batch both add the SAME
        // file with DIFFERENT content off the same base. The first merges; the second's merge
        // CONFLICTS. integrate() must merge the RUN BRANCH'S TIP INTO B's OWN worktree, leave
        // conflict markers there (never abort), and leave B's BRANCH REF untouched (no reset, no
        // lost commit) - so the conductor can re-park B's implementer to resolve on the SAME
        // branch instead of discarding its work or wedging the run.
        let repo = init_repo();
        let repo_path = repo.path().to_str().unwrap().to_string();

        // Both worktrees branch off the SAME base, as concurrent batch-mates do.
        let wa = std::env::temp_dir().join(format!("rigger-wt-{}", uuid::Uuid::new_v4()));
        let wb = std::env::temp_dir().join(format!("rigger-wt-{}", uuid::Uuid::new_v4()));
        let a = Worktree::create(&repo_path, wa.to_str().unwrap(), "rigger/u/a", "").unwrap();
        let b = Worktree::create(&repo_path, wb.to_str().unwrap(), "rigger/u/b", "").unwrap();

        // A adds shared.txt and integrates cleanly.
        std::fs::write(wa.join("shared.txt"), "A version\n").unwrap();
        a.integrate("rigger: integrate a").unwrap().expect_merged();
        let head_after_a = run_git(&repo_path, &["rev-parse", "HEAD"])
            .unwrap()
            .trim()
            .to_string();

        // B adds the SAME file with DIFFERENT content off the same base: an add/add conflict.
        // Committed here (like the conductor's OWN pre-gate commit, which always runs before
        // `integrate` in production) so the branch-untouched assertion below measures the
        // conflict handling alone, not B's own ordinary work commit.
        std::fs::write(wb.join("shared.txt"), "B version\n").unwrap();
        b.commit("rigger: b's own work").unwrap();
        let b_branch_before = run_git(&repo_path, &["rev-parse", "rigger/u/b"])
            .unwrap()
            .trim()
            .to_string();
        match b.integrate("rigger: integrate b").unwrap() {
            IntegrateOutcome::Conflict(paths) => {
                assert_eq!(
                    paths,
                    ["shared.txt"],
                    "the conflict names exactly the conflicting path"
                );
            }
            IntegrateOutcome::Merged(_) => {
                panic!("a divergent add/add merge must conflict, not merge")
            }
        }

        // The RUN BRANCH is untouched: A's content stands, HEAD is unchanged - a conflict never
        // touches the shared repo checkout at all.
        assert_eq!(
            std::fs::read_to_string(repo.path().join("shared.txt")).unwrap(),
            "A version\n",
            "a conflict must not alter the run branch"
        );
        let head_now = run_git(&repo_path, &["rev-parse", "HEAD"])
            .unwrap()
            .trim()
            .to_string();
        assert_eq!(head_now, head_after_a, "HEAD is unchanged after a conflict");

        // B's BRANCH REF is untouched: still the exact commit it carried before this call - no
        // reset, so every prior commit stays exactly as it was.
        let b_branch_after = run_git(&repo_path, &["rev-parse", "rigger/u/b"])
            .unwrap()
            .trim()
            .to_string();
        assert_eq!(
            b_branch_after, b_branch_before,
            "the unit branch ref must not move on a conflict (--no-commit never advances it)"
        );

        // The run branch's tip WAS merged INTO B's own worktree: a merge is in progress there,
        // and B's version of shared.txt now carries real conflict markers naming both sides.
        assert!(
            b.merge_in_progress(),
            "the run branch's tip must be merged into the unit's OWN worktree, not aborted"
        );
        let conflicted = std::fs::read_to_string(wb.join("shared.txt")).unwrap();
        assert!(
            conflicted.contains("<<<<<<<") && conflicted.contains("A version"),
            "conflict markers naming both sides are left in place in the worktree: {conflicted}"
        );
        assert_eq!(
            b.conflicting_paths().unwrap(),
            ["shared.txt"],
            "conflicting_paths reads the same list back from worktree state"
        );
    }

    #[test]
    fn commit_refuses_a_worktree_with_a_merge_left_in_progress() {
        // Spec 89, criterion 1 (A CHECKPOINT NEVER COMMITS A HALF-MERGE): the
        // 2026-09-12 incident this guards against - an ordinary checkpoint's `git add
        // -A && git commit` ran over a worktree where a conflicted `integrate()` call
        // (exactly like the one in the test just above) had left a real merge in
        // progress, silently staging the literal conflict-marker text as "resolved"
        // and landing a merge commit that still carried 2720 markers. `commit` itself
        // must refuse instead: no `git add`, no commit, the merge and its markers
        // left exactly as they were.
        let repo = init_repo();
        let repo_path = repo.path().to_str().unwrap().to_string();
        let wa = std::env::temp_dir().join(format!("rigger-wt-{}", uuid::Uuid::new_v4()));
        let wb = std::env::temp_dir().join(format!("rigger-wt-{}", uuid::Uuid::new_v4()));
        let a = Worktree::create(
            &repo_path,
            wa.to_str().unwrap(),
            "rigger/u/a-commit-guard",
            "",
        )
        .unwrap();
        let b = Worktree::create(
            &repo_path,
            wb.to_str().unwrap(),
            "rigger/u/b-commit-guard",
            "",
        )
        .unwrap();
        std::fs::write(wa.join("shared.txt"), "A version\n").unwrap();
        a.integrate("rigger: integrate a").unwrap().expect_merged();
        std::fs::write(wb.join("shared.txt"), "B version\n").unwrap();
        match b.integrate("rigger: integrate b").unwrap() {
            IntegrateOutcome::Conflict(_) => {}
            IntegrateOutcome::Merged(_) => {
                panic!("a divergent add/add merge must conflict, not merge")
            }
        }
        assert!(
            b.merge_in_progress(),
            "setup must leave a genuine merge in progress"
        );
        let head_before = run_git(wb.to_str().unwrap(), &["rev-parse", "HEAD"]).unwrap();

        let err = b
            .commit("rigger: b's own work, unaware of the stuck merge")
            .expect_err("commit must refuse a worktree with a merge left in progress");
        assert!(
            err.to_string().contains("conflict"),
            "names the conflict-marker state: {err}"
        );
        assert!(
            err.to_string().contains(wb.to_str().unwrap()),
            "names the worktree: {err}"
        );

        // Nothing was staged or committed: HEAD unchanged, the merge still in
        // progress, and the markers still literally in the file - never staged as
        // "resolved".
        let head_after = run_git(wb.to_str().unwrap(), &["rev-parse", "HEAD"]).unwrap();
        assert_eq!(head_before, head_after, "no commit was made");
        assert!(
            b.merge_in_progress(),
            "the merge is still in progress, never finalized"
        );
        let content = std::fs::read_to_string(wb.join("shared.txt")).unwrap();
        assert!(
            content.contains("<<<<<<<"),
            "conflict markers are untouched: {content}"
        );
    }

    #[test]
    fn commit_refuses_when_tracked_content_carries_conflict_marker_text() {
        // Spec 89, criterion 1: the AFTER-THE-FACT half of the guard. `git add -A`
        // clears a path's UNMERGED index state the instant it is staged, even when
        // the staged CONTENT is still literal marker text - exactly what let the
        // 2026-09-12 incident's checkpoint stage a conflicted file as "resolved".
        // Proven directly at the content layer, independent of which git operation
        // left the markers behind: `merge_in_progress`/`conflicting_paths` are both
        // clean here - only the tracked file's own content carries the markers.
        let repo = init_repo();
        let repo_path = repo.path().to_str().unwrap().to_string();
        let wt_path = std::env::temp_dir().join(format!("rigger-wt-{}", uuid::Uuid::new_v4()));
        let wt = Worktree::create(
            &repo_path,
            wt_path.to_str().unwrap(),
            "rigger/marker-guard",
            "",
        )
        .unwrap();

        std::fs::write(wt_path.join("shared.txt"), "clean\n").unwrap();
        wt.commit("rigger: seed shared.txt").unwrap();
        std::fs::write(
            wt_path.join("shared.txt"),
            "<<<<<<< HEAD\nours\n=======\ntheirs\n>>>>>>> other\n",
        )
        .unwrap();
        assert!(!wt.merge_in_progress());
        assert!(wt.conflicting_paths().unwrap().is_empty());

        let head_before = run_git(wt_path.to_str().unwrap(), &["rev-parse", "HEAD"]).unwrap();
        let err = wt
            .commit("rigger: sweep it in")
            .expect_err("commit must refuse tracked content that still carries conflict markers");
        assert!(
            err.to_string().contains("conflict"),
            "names the conflict-marker state: {err}"
        );
        let head_after = run_git(wt_path.to_str().unwrap(), &["rev-parse", "HEAD"]).unwrap();
        assert_eq!(head_before, head_after, "no commit was made");
        let status = run_git(wt_path.to_str().unwrap(), &["status", "--porcelain"]).unwrap();
        assert!(
            status.contains(" M shared.txt"),
            "shared.txt must remain an UNSTAGED modification, never staged by a \
             refused commit: {status:?}"
        );
    }

    #[test]
    fn commit_checkpoint_commits_through_a_refusing_hook_while_commit_is_refused() {
        // A checkpoint preserves a halted spawn's tree; a content hook (the docs-drift
        // pre-commit hook, in the incident) must not be able to turn that into a lost tree
        // and a dead step. The agent's own commit path keeps running hooks.
        use std::os::unix::fs::PermissionsExt;
        let repo = init_repo();
        let repo_path = repo.path().to_str().unwrap().to_string();
        let hooks = repo.path().join(".git").join("hooks");
        std::fs::create_dir_all(&hooks).unwrap();
        let hook = hooks.join("pre-commit");
        std::fs::write(&hook, "#!/bin/sh\necho 'hook: refusing' >&2\nexit 1\n").unwrap();
        std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o755)).unwrap();
        let wt_path = std::env::temp_dir().join(format!("rigger-wt-{}", uuid::Uuid::new_v4()));
        let wt = Worktree::create(
            &repo_path,
            wt_path.to_str().unwrap(),
            "rigger/checkpoint-hook",
            "",
        )
        .unwrap();
        let head_before = run_git(wt_path.to_str().unwrap(), &["rev-parse", "HEAD"]).unwrap();

        std::fs::write(wt_path.join("work.txt"), "half-done\n").unwrap();
        let err = wt
            .commit("rigger: an agent's own commit")
            .expect_err("the hook refuses an ordinary commit");
        assert!(
            err.to_string().contains("hook: refusing"),
            "the refusal is the hook's, surfaced verbatim: {err}"
        );
        let head_mid = run_git(wt_path.to_str().unwrap(), &["rev-parse", "HEAD"]).unwrap();
        assert_eq!(head_before, head_mid, "the ordinary commit made nothing");

        let sha = wt
            .commit_checkpoint("wip(unit): tree of halted spawn unit/implementer#1")
            .expect("a checkpoint commits through the refusing hook");
        let head_after = run_git(wt_path.to_str().unwrap(), &["rev-parse", "HEAD"]).unwrap();
        assert_eq!(sha, head_after.trim(), "the checkpoint sha is HEAD");
        assert_ne!(
            head_before, head_after,
            "the tree was preserved in a commit"
        );
        let shown = run_git(
            wt_path.to_str().unwrap(),
            &["show", "--stat", "--oneline", "HEAD"],
        )
        .unwrap();
        assert!(
            shown.contains("work.txt"),
            "the checkpoint carries the tree: {shown}"
        );
    }

    #[test]
    fn land_is_fast_forward_only_and_reports_a_moved_tip_without_dirtying_the_repo() {
        // The worktree merge has just brought the run tip into the unit branch, so a landing
        // is a fast-forward; if the run branch moved meanwhile, `land` must say so and leave
        // the repo untouched (no MERGE_HEAD, no conflicted index) - the conductor then merges
        // the new tip into the worktree and lands again. A real merge here once left the main
        // checkout mid-merge and failed every later step.
        let repo = init_repo();
        let repo_path = repo.path().to_str().unwrap().to_string();
        let wt_path = std::env::temp_dir().join(format!("rigger-wt-{}", uuid::Uuid::new_v4()));
        let wt =
            Worktree::create(&repo_path, wt_path.to_str().unwrap(), "rigger/land-ff", "").unwrap();
        std::fs::write(wt_path.join("unit.txt"), "unit work\n").unwrap();
        wt.commit("rigger: unit work").unwrap();

        // The run branch moves under the unit (an operator commit on the run branch).
        std::fs::write(repo.path().join("operator.txt"), "operator work\n").unwrap();
        git(&repo_path, &["add", "operator.txt"]).unwrap();
        git(
            &repo_path,
            &["commit", "-q", "-m", "operator: moved the tip"],
        )
        .unwrap();
        let tip_before = git(&repo_path, &["rev-parse", "HEAD"]).unwrap();

        assert_eq!(
            wt.land().unwrap(),
            LandOutcome::TipMoved,
            "no fast-forward is possible"
        );
        assert!(
            !repo.path().join(".git").join("MERGE_HEAD").exists(),
            "a refused landing never leaves the repo mid-merge"
        );
        assert_eq!(
            git(&repo_path, &["rev-parse", "HEAD"]).unwrap(),
            tip_before,
            "the run branch is untouched"
        );
        assert_eq!(
            git(&repo_path, &["status", "--porcelain"]).unwrap().trim(),
            "",
            "the main checkout stays clean"
        );

        // Merging the new tip into the worktree makes the next landing a fast-forward.
        match wt.merge_into_worktree("rigger: integrate land-ff").unwrap() {
            MergeOutcome::Ready(c) => assert!(!c.is_empty(), "the merge commits"),
            MergeOutcome::Conflict(paths) => {
                panic!("expected a clean merge, got a conflict on {paths:?}")
            }
        }
        assert_eq!(wt.land().unwrap(), LandOutcome::Landed);
        assert_eq!(
            git(&repo_path, &["rev-parse", "HEAD"]).unwrap(),
            git(wt_path.to_str().unwrap(), &["rev-parse", "HEAD"]).unwrap(),
            "the run branch fast-forwarded to the unit branch"
        );
    }

    /// Land a worktree on `branch` whose one commit (`message`) writes `file` as `unit_content`,
    /// while the repo checkout holds `local_content` at that same path, and return the paths
    /// of the `LandOutcome::Blocked` refusal.
    fn land_over_local_content(
        repo_path: &str,
        branch: &str,
        file: &str,
        unit_content: &str,
        local_content: &str,
        message: &str,
    ) -> Vec<String> {
        let (wt_path, wt) = temp_wt(repo_path, branch);
        std::fs::write(wt_path.join(file), unit_content).unwrap();
        wt.commit(message).unwrap();
        std::fs::write(std::path::Path::new(repo_path).join(file), local_content).unwrap();
        match wt.land().unwrap() {
            LandOutcome::Blocked(paths) => paths,
            other => panic!("expected Blocked(_), got {other:?}"),
        }
    }

    #[test]
    fn land_reports_untracked_blocking_paths_and_leaves_the_repo_untouched() {
        // Spec 103 criterion 8 (A REFUSED LANDING NAMES ITS PATHS): a `git merge --ff-only`
        // refusal because untracked local content in the repo checkout would be overwritten
        // is a DISTINCT, recognized outcome - not the generic `Err` every other non-content
        // git failure falls through to - so the caller (the conductor) can name the exact
        // paths in its lesson instead of just relaying git's raw text.
        let repo = init_repo();
        let repo_path = repo.path().to_str().unwrap().to_string();
        let tip_before = git(&repo_path, &["rev-parse", "HEAD"]).unwrap();

        // Untracked local content sits in the repo checkout at the exact path the unit's
        // branch newly introduces - never committed, so `git status` in the repo never even
        // names it as a change to reconcile.
        let paths = land_over_local_content(
            &repo_path,
            "rigger/land-blocked",
            "new.txt",
            "unit work\n",
            "stray local content\n",
            "rigger: unit work",
        );
        assert_eq!(paths, vec!["new.txt".to_string()]);
        assert!(
            !repo.path().join(".git").join("MERGE_HEAD").exists(),
            "a refused landing never leaves the repo mid-merge"
        );
        assert_eq!(
            git(&repo_path, &["rev-parse", "HEAD"]).unwrap(),
            tip_before,
            "the run branch is untouched"
        );
        assert_eq!(
            std::fs::read_to_string(repo.path().join("new.txt")).unwrap(),
            "stray local content\n",
            "the blocking local content itself is untouched"
        );
    }

    #[test]
    fn land_reports_locally_modified_tracked_blocking_paths() {
        // The sibling shape of the untracked case above: a TRACKED file the repo checkout has
        // dirtied (never committed) blocks the identical fast-forward with git's OTHER local-
        // changes wording ("Your local changes to the following files..."). Both must resolve
        // to the same `LandOutcome::Blocked` - the caller does not care which git wording fired.
        let repo = init_repo();
        let repo_path = repo.path().to_str().unwrap().to_string();
        std::fs::write(repo.path().join("tracked.txt"), "base\n").unwrap();
        git(&repo_path, &["add", "tracked.txt"]).unwrap();
        git(&repo_path, &["commit", "-q", "-m", "add tracked.txt"]).unwrap();

        let paths = land_over_local_content(
            &repo_path,
            "rigger/land-blocked-tracked",
            "tracked.txt",
            "feature version\n",
            "dirty local edit\n",
            "rigger: modify tracked.txt",
        );
        assert_eq!(paths, vec!["tracked.txt".to_string()]);
        assert_eq!(
            std::fs::read_to_string(repo.path().join("tracked.txt")).unwrap(),
            "dirty local edit\n",
            "the blocking local edit itself is untouched"
        );
    }

    #[test]
    fn land_reports_a_generic_error_for_a_refusal_that_is_neither_tip_moved_nor_blocked() {
        // The two named outcomes above (`TipMoved`, `Blocked`) each require their own git
        // wording; every OTHER `git merge --ff-only` failure - this test forces one by
        // pointing the worktree at a branch name that was never created - must fall through
        // to the generic `Err`, never be misread as a content-overwrite `Blocked`.
        let repo = init_repo();
        let repo_path = repo.path().to_str().unwrap().to_string();
        let wt_path = std::env::temp_dir().join(format!("rigger-wt-{}", uuid::Uuid::new_v4()));
        let mut wt = Worktree::create(
            &repo_path,
            wt_path.to_str().unwrap(),
            "rigger/land-generic-error",
            "",
        )
        .unwrap();
        wt.branch = "rigger/no-such-branch".to_string();

        let err = match wt.land() {
            Err(e) => e,
            Ok(outcome) => panic!("expected a generic Err, got Ok({outcome:?})"),
        };
        assert!(
            !err.to_string()
                .to_ascii_lowercase()
                .contains("would be overwritten by merge"),
            "a missing-branch failure is not a content-overwrite refusal: {err}"
        );
    }

    #[test]
    fn parse_blocking_paths_reads_every_tab_indented_line_sorted_and_deduped() {
        let untracked = "error: The following untracked working tree files would be overwritten by merge:\n\tb.txt\n\ta.txt\nPlease move or remove them before you merge.\nAborting\n";
        assert_eq!(
            parse_blocking_paths(untracked),
            vec!["a.txt".to_string(), "b.txt".to_string()]
        );

        let local_changes = "error: Your local changes to the following files would be overwritten by merge:\n\tc.txt\nPlease commit your changes or stash them before you merge.\nAborting\n";
        assert_eq!(
            parse_blocking_paths(local_changes),
            vec!["c.txt".to_string()]
        );

        assert_eq!(
            parse_blocking_paths("fatal: not a git repository\n"),
            Vec::<String>::new(),
            "text with no blocking-path header names nothing"
        );
    }

    #[test]
    fn blob_at_reads_committed_content_and_none_when_absent_or_unresolvable() {
        let repo = init_repo();
        let p = repo.path().to_str().unwrap();
        std::fs::write(repo.path().join("f.txt"), "hello\n").unwrap();
        run_git(p, &["add", "f.txt"]).unwrap();
        run_git(p, &["commit", "-q", "-m", "add f.txt"]).unwrap();

        assert_eq!(blob_at(p, "HEAD", "f.txt"), Some(b"hello\n".to_vec()));
        assert_eq!(blob_at(p, "HEAD", "missing.txt"), None);
        assert_eq!(blob_at(p, "no-such-ref", "f.txt"), None);
    }

    #[test]
    fn unit_branches_lists_only_rigger_u_branches_sorted() {
        let repo = init_repo();
        let p = repo.path().to_str().unwrap();
        for name in ["rigger/u/zeta", "rigger/u/alpha", "rigger/review/panel-0"] {
            run_git(p, &["branch", name]).unwrap();
        }
        assert_eq!(
            unit_branches(p),
            vec!["rigger/u/alpha".to_string(), "rigger/u/zeta".to_string()],
            "sorted, and never a non-unit branch like rigger/review/*"
        );
    }

    #[test]
    fn creating_a_unit_worktree_writes_the_scratch_roots_shared_build_location_once() {
        // Spec 77 criterion 1's mechanical half: the first unit worktree under a scratch root
        // leaves `<root>/.cargo/config.toml` pointing unpinned cargo runs at the root's shared
        // cache; a second worktree leaves an existing file alone; a non-unit worktree writes
        // nothing.
        let repo = init_repo();
        let repo_path = repo.path().to_str().unwrap().to_string();
        let root = std::env::temp_dir().join(format!("rigger-scratch-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let wt_a = root.join("rigger-wt-unit-a");
        Worktree::create(&repo_path, wt_a.to_str().unwrap(), "rigger/u/unit-a", "").unwrap();
        let cfg = root.join(".cargo").join("config.toml");
        let written = std::fs::read_to_string(&cfg).expect("the root carries a cargo config");
        assert!(
            written.contains("[build]") && written.contains("target-dir = \"cargo-target-shared\""),
            "unpinned cargo inside a unit worktree builds into the root's shared cache: {written}"
        );
        std::fs::write(&cfg, "[build]\ntarget-dir = \"operator-tuned\"\n").unwrap();
        let wt_b = root.join("rigger-wt-unit-b");
        Worktree::create(&repo_path, wt_b.to_str().unwrap(), "rigger/u/unit-b", "").unwrap();
        assert!(
            std::fs::read_to_string(&cfg)
                .unwrap()
                .contains("operator-tuned"),
            "an existing file is never rewritten"
        );
        let other = std::env::temp_dir().join(format!("rigger-other-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&other).unwrap();
        let review = other.join("rigger-review-panel-0");
        Worktree::create(&repo_path, review.to_str().unwrap(), "rigger/review-0", "").unwrap();
        assert!(
            !other.join(".cargo").exists(),
            "a review worktree is no unit and writes no build location"
        );
    }

    #[test]
    fn sweep_orphan_scratch_roots_reclaims_only_roots_whose_decoded_repo_is_gone() {
        let cache = tempfile::tempdir().unwrap();
        let live_repo = tempfile::tempdir().unwrap();
        let live_path = live_repo.path().to_str().unwrap().to_string();
        let enc = |p: &str| crate::liveness::marker_filename(p).unwrap();
        let live = cache.path().join(enc(&live_path));
        let gone = cache
            .path()
            .join(enc(&format!("{live_path}/vanished-checkout")));
        let plain = cache.path().join("test-tmp");
        let relative = cache.path().join(enc("relative/repo"));
        for dir in [&live, &gone, &plain, &relative] {
            std::fs::create_dir_all(dir.join("cargo-target-x")).unwrap();
            std::fs::write(dir.join("cargo-target-x").join("f"), "x").unwrap();
        }
        let file = cache.path().join(enc("/some/where/absent"));
        std::fs::write(&file, "not a root").unwrap();
        assert_eq!(sweep_orphan_scratch_roots(cache.path()), 1);
        assert!(!gone.exists(), "a root whose repo is gone is reclaimed");
        assert!(live.exists(), "a root whose repo exists is kept");
        assert!(plain.exists(), "a name that is not an encoded path is kept");
        assert!(
            relative.exists(),
            "a name that decodes to a relative path is kept"
        );
        assert!(file.exists(), "a non-directory is kept");
        assert_eq!(sweep_orphan_scratch_roots(cache.path()), 0, "idempotent");
        assert_eq!(sweep_orphan_scratch_roots(&cache.path().join("absent")), 0);
    }

    #[test]
    fn creating_the_cache_default_scratch_root_reclaims_orphaned_siblings_but_a_configured_root_never_sweeps(
    ) {
        let xdg = tempfile::tempdir().unwrap();
        let repo = init_repo();
        let repo_path = repo.path().to_str().unwrap().to_string();
        let rigger_dir = xdg.path().join("rigger");
        let orphan = rigger_dir
            .join(crate::liveness::marker_filename(&format!("{repo_path}-gone")).unwrap());
        std::fs::create_dir_all(orphan.join("rigger-wt-old")).unwrap();
        let xdg_os = Some(xdg.path().as_os_str().to_os_string());
        let root = scratch_root_with(&repo_path, "", None, xdg_os.clone(), None);
        assert_eq!(
            std::path::Path::new(&root),
            rigger_dir.join(crate::liveness::marker_filename(&repo_path).unwrap())
        );
        assert!(std::path::Path::new(&root).is_dir(), "the root is created");
        assert!(
            !orphan.exists(),
            "the default rung reclaims a sibling whose repo is gone"
        );
        std::fs::create_dir_all(orphan.join("rigger-wt-old")).unwrap();
        let configured = xdg.path().join("operator-chosen");
        let root2 = scratch_root_with(&repo_path, configured.to_str().unwrap(), None, xdg_os, None);
        assert_eq!(std::path::Path::new(&root2), configured);
        assert!(
            orphan.exists(),
            "an operator's configured root sweeps nothing"
        );
    }

    #[test]
    fn commit_ignores_conflict_marker_lookalike_text_in_an_untouched_tracked_file() {
        // Spec 89, criterion 1, round 2 fix
        // (adv-u89c1-conflict-marker-scan-is-repo-wide-content-not-diff-scoped-false-positive):
        // the guard is scoped to THIS commit's own touched/unmerged paths, never an
        // unconditional whole-tracked-tree scan. A pre-existing, ALREADY-committed file this
        // commit never touches - here a benign Markdown Setext heading underline, seven `=`
        // characters, which happens to match the same `^=======$` pattern a real conflict
        // marker line does - must never block an unrelated commit anywhere else in the repo.
        // `commit_refuses_when_tracked_content_carries_conflict_marker_text` above pins the
        // opposite case (the guard MUST still catch marker text in a file THIS commit does
        // touch); this test is its negative-space twin.
        let repo = init_repo();
        let repo_path = repo.path().to_str().unwrap().to_string();
        let wt_path = std::env::temp_dir().join(format!("rigger-wt-{}", uuid::Uuid::new_v4()));
        let wt = Worktree::create(
            &repo_path,
            wt_path.to_str().unwrap(),
            "rigger/marker-lookalike",
            "",
        )
        .unwrap();

        // Seed the lookalike file via raw git, bypassing `Worktree::commit` entirely, so it
        // lands as ALREADY-committed history this test's own commit call below never touches -
        // exactly like any other pre-existing file elsewhere in a real repo.
        std::fs::write(wt_path.join("notes.md"), "Title\n=======\nbody text\n").unwrap();
        git(wt_path.to_str().unwrap(), &["add", "notes.md"]).unwrap();
        git(
            wt_path.to_str().unwrap(),
            &["commit", "-m", "seed a benign Setext heading"],
        )
        .unwrap();

        // A real edit to a DIFFERENT, unrelated file - the only thing this commit touches.
        std::fs::write(wt_path.join("other.txt"), "hello\n").unwrap();
        let sha = wt
            .commit("rigger: touch an unrelated file")
            .expect("a lookalike elsewhere in the repo must never block this commit");
        assert!(!sha.is_empty(), "the commit must actually land");
        assert_eq!(
            std::fs::read_to_string(wt_path.join("notes.md")).unwrap(),
            "Title\n=======\nbody text\n",
            "the untouched lookalike file is unchanged"
        );
    }

    #[test]
    fn integrate_conflict_is_idempotent_on_a_crash_resumed_worktree() {
        // Spec 88, criterion 1, CONSTRAINTS WALK: "the merge is worktree state, not log state" -
        // a re-park that re-enters integrate() on a worktree ALREADY carrying an in-progress
        // merge (a crash between the first conflict and the resolving commit) must never
        // re-invoke `git merge` (which git refuses on a tree with unmerged paths) and must read
        // the SAME conflict list back from the worktree, not error.
        let repo = init_repo();
        let repo_path = repo.path().to_str().unwrap().to_string();
        let wa = std::env::temp_dir().join(format!("rigger-wt-{}", uuid::Uuid::new_v4()));
        let wb = std::env::temp_dir().join(format!("rigger-wt-{}", uuid::Uuid::new_v4()));
        let a = Worktree::create(&repo_path, wa.to_str().unwrap(), "rigger/u/a", "").unwrap();
        let b = Worktree::create(&repo_path, wb.to_str().unwrap(), "rigger/u/b", "").unwrap();
        std::fs::write(wa.join("shared.txt"), "A version\n").unwrap();
        a.integrate("rigger: integrate a").unwrap().expect_merged();
        std::fs::write(wb.join("shared.txt"), "B version\n").unwrap();

        let first = b.integrate("rigger: integrate b").unwrap();
        let IntegrateOutcome::Conflict(first_paths) = first else {
            panic!("expected a conflict");
        };

        // Re-enter exactly as a resumed step would, with nothing resolved yet.
        let second = b.integrate("rigger: integrate b (resumed)").unwrap();
        let IntegrateOutcome::Conflict(second_paths) = second else {
            panic!("a re-entered integrate on an unresolved conflict must still report Conflict");
        };
        assert_eq!(
            first_paths, second_paths,
            "the re-entered call reads the identical conflict list from worktree state"
        );
        assert!(
            b.merge_in_progress(),
            "the in-progress merge survives the idempotent re-entry untouched"
        );
    }

    #[test]
    #[cfg(unix)]
    fn integrate_reports_a_non_content_merge_failure_instead_of_silently_landing_a_stale_branch() {
        // Spec 88, criterion 1, operator ruling op-u88c1-round-1-conflict-resolution-is-a-
        // parked-spawn-not-an-inline-loop, point (e): "Every worktree git result is checked: a
        // non-content failure of merge --no-commit ... is an integration ERROR ..., never a
        // silent fall-through that lands an unvalidated branch." (sdet-u88c1-worktree-merge-
        // result-discarded). `git merge --no-commit --no-ff` can fail for a reason that leaves
        // BOTH `conflicting_paths()` and `merge_in_progress()` at their ordinary "nothing to
        // do" defaults - a stray untracked, non-regular file (a build tool's leftover FIFO or
        // socket, say) at a path the run branch's tip newly tracks makes git refuse outright
        // ("untracked working tree files would be overwritten"), with no MERGE_HEAD and no
        // unmerged path ever created. `Worktree::commit`'s own `git add -A` (always run first)
        // cannot sweep it into the unit's own commit first (unlike a plain regular file) - `git
        // add` has no blob to record for a FIFO, so `git status`/`add -A` never even see it -
        // which is exactly what makes this reachable via the ordinary call sequence, not a
        // fabricated repository state. Reading only worktree state (as every other outcome in
        // this function correctly does) cannot distinguish that from "nothing changed", so
        // discarding this command's own Result silently falls through to landing the unit's
        // branch UNCHANGED - never actually merging the run branch's new content in at all.
        let repo = init_repo();
        let repo_path = repo.path().to_str().unwrap().to_string();
        let wb = std::env::temp_dir().join(format!("rigger-wt-{}", uuid::Uuid::new_v4()));
        // b branches off the CURRENT base first, so its own history never learns about
        // "clash.rs" - only the run branch's tip (landed directly below) tracks it.
        let b = Worktree::create(&repo_path, wb.to_str().unwrap(), "rigger/u/b", "").unwrap();

        // Land "clash.rs" on the run branch directly (simulating a sibling unit's own
        // already-integrated work).
        std::fs::write(repo.path().join("clash.rs"), "FROM_A\n").unwrap();
        run_git(&repo_path, &["add", "--", "clash.rs"]).unwrap();
        run_git(&repo_path, &["commit", "-q", "-m", "a lands clash.rs"]).unwrap();

        std::fs::write(wb.join("b.rs"), "B_WORK\n").unwrap();
        b.commit("rigger: b's own work").unwrap();
        // A stray untracked FIFO at the exact path the run branch's tip now carries - `git
        // add -A` cannot stage a non-regular file, so it stays genuinely untracked (invisible
        // to `git status`, even) all the way to the merge attempt below; git itself (not this
        // crate) then refuses to clobber it.
        assert!(
            std::process::Command::new("mkfifo")
                .arg(wb.join("clash.rs"))
                .status()
                .unwrap()
                .success(),
            "test setup: mkfifo must succeed"
        );

        let err = match b.integrate("rigger: integrate b") {
            Err(e) => e,
            Ok(IntegrateOutcome::Merged(c)) => panic!(
                "a non-content git failure must surface as an Err, never a silent Merged({c:?})"
            ),
            Ok(IntegrateOutcome::Conflict(paths)) => panic!(
                "a non-content git failure is not a real content conflict, got Conflict({paths:?})"
            ),
        };
        assert!(
            err.0.contains("clash.rs") || err.0.to_lowercase().contains("untracked"),
            "the real git failure must propagate, not a fabricated message: {}",
            err.0
        );
        // The stray FIFO is exactly what git itself refused to touch - proof this is the real
        // git refusal, not some other failure.
        use std::os::unix::fs::FileTypeExt;
        assert!(
            std::fs::symlink_metadata(wb.join("clash.rs"))
                .unwrap()
                .file_type()
                .is_fifo(),
            "git's own refusal leaves the stray file untouched"
        );
        assert!(
            !b.merge_in_progress(),
            "git refused before ever starting the merge - no MERGE_HEAD to speak of"
        );
    }

    #[test]
    fn integrate_propagates_a_genuine_commit_failure_finalizing_a_resolved_merge_instead_of_treating_it_as_a_no_op(
    ) {
        // worktree.rs:635 treats ONLY a "nothing to commit" failure from the finalizing
        // `git commit --no-edit` as a benign no-op (an already-empty resolution, tolerated
        // for crash-resume idempotency). Any OTHER failure - a signing failure, disk full -
        // must propagate as a genuine `Err`, never be silently swallowed as if the merge had
        // finished; swallowing it would let `integrate` fall through to `git merge --no-edit`
        // on the run branch believing a merge commit exists that was never actually made.
        // Injected via a permission-denied object write, not a refusing hook: this finalizing
        // commit is rigger's own merge-conclusion bookkeeping, so it now runs `--no-verify`
        // (d-checkin-rigger-own-commits-bypass-hooks) and a hook is no longer an available
        // failure instrument here. A forced signing failure is not available either - per
        // spec 90, the whole suite runs under the hermetic test-git runner's own commit-signing
        // suppression (see `tests/hermetic_test_git_audit.rs`), the SOLE authority for that
        // override, so a repo-local config can never re-enable signing here. A read-only object
        // database is an OS-level failure that override does not touch, so it still proves a
        // genuine, unrelated failure propagates.
        let repo = init_repo();
        let repo_path = repo.path().to_str().unwrap().to_string();
        let wa = std::env::temp_dir().join(format!("rigger-wt-{}", uuid::Uuid::new_v4()));
        let wb = std::env::temp_dir().join(format!("rigger-wt-{}", uuid::Uuid::new_v4()));
        let a = Worktree::create(&repo_path, wa.to_str().unwrap(), "rigger/u/a", "").unwrap();
        let b = Worktree::create(&repo_path, wb.to_str().unwrap(), "rigger/u/b", "").unwrap();
        std::fs::write(wa.join("shared.txt"), "A version\n").unwrap();
        a.integrate("rigger: integrate a").unwrap().expect_merged();
        std::fs::write(wb.join("shared.txt"), "B version\n").unwrap();
        b.commit("rigger: b's own work").unwrap();
        match b.integrate("rigger: integrate b").unwrap() {
            IntegrateOutcome::Conflict(_) => {}
            IntegrateOutcome::Merged(_) => panic!("a divergent add/add merge must conflict"),
        }
        assert!(b.merge_in_progress());

        // Resolve the conflict for real, to content that differs from both sides so the
        // finalizing commit is never itself a no-op.
        std::fs::write(wb.join("shared.txt"), "RESOLVED\n").unwrap();
        run_git(wb.to_str().unwrap(), &["add", "--", "shared.txt"]).unwrap();

        // A worktree's object database is the MAIN repo's (git worktree add shares one
        // `.git/objects`) - strip write permission from it so the finalizing commit cannot
        // write its new tree/commit objects: a genuine, unrelated, OS-level failure.
        let objects_dir = repo.path().join(".git").join("objects");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&objects_dir).unwrap().permissions();
            perms.set_mode(0o555);
            std::fs::set_permissions(&objects_dir, perms).unwrap();
        }

        let result = b.integrate("rigger: integrate b (finalize)");

        // Restore write permission before any assertion can panic and before `repo` drops -
        // otherwise the read-only directory would make its own teardown fail.
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&objects_dir).unwrap().permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(&objects_dir, perms).unwrap();
        }

        let err = match result {
            Err(e) => e,
            Ok(_) => panic!("a genuine commit failure must surface as an Err, not a silent no-op"),
        };
        assert!(
            err.0.contains("insufficient permission"),
            "the real failure must propagate verbatim: {}",
            err.0
        );
        assert!(
            b.merge_in_progress(),
            "a failed finalize must leave the merge in progress, not silently drop it"
        );
    }

    #[test]
    fn integrate_finalizes_a_divergent_merge_that_nets_to_an_empty_commit_when_both_sides_converge_on_identical_content(
    ) {
        // The MIRROR of the genuine-failure test above: worktree.rs:635's "nothing to
        // commit" guard exists for a real, reachable case - two worktrees branch off the
        // SAME base and independently add the SAME file with IDENTICAL content (an honest
        // duplicate fix, not a conflict). Git's 3-way merge resolves "both sides added the
        // same content" cleanly (no markers), but because the histories diverged, `--no-ff`
        // still requires a merge commit for lineage - and because the resulting tree is
        // byte-identical to the unit's own current HEAD, `git commit --no-edit` reports
        // "nothing to commit, working tree clean" even though a real merge (MERGE_HEAD) is
        // in progress. That must be tolerated as a benign no-op and still finalize as a
        // successful [`IntegrateOutcome::Merged`] - never surfaced as a conflict, and never
        // silently dropped without ever finalizing the merge commit either.
        let repo = init_repo();
        let repo_path = repo.path().to_str().unwrap().to_string();
        let wa = std::env::temp_dir().join(format!("rigger-wt-{}", uuid::Uuid::new_v4()));
        let wb = std::env::temp_dir().join(format!("rigger-wt-{}", uuid::Uuid::new_v4()));
        let a = Worktree::create(&repo_path, wa.to_str().unwrap(), "rigger/u/a", "").unwrap();
        let b = Worktree::create(&repo_path, wb.to_str().unwrap(), "rigger/u/b", "").unwrap();

        std::fs::write(wa.join("shared.txt"), "identical\n").unwrap();
        std::fs::write(wb.join("shared.txt"), "identical\n").unwrap();
        a.integrate("rigger: integrate a").unwrap().expect_merged();
        b.commit("rigger: b's own work").unwrap();

        let commit = match b.integrate("rigger: integrate b").unwrap() {
            IntegrateOutcome::Merged(c) => c,
            IntegrateOutcome::Conflict(paths) => panic!(
                "both sides adding IDENTICAL content must merge cleanly, not conflict: {paths:?}"
            ),
        };
        assert!(
            !commit.is_empty(),
            "a real merge commit hash is still returned, even though its content is empty"
        );
        assert_eq!(
            std::fs::read_to_string(repo.path().join("shared.txt")).unwrap(),
            "identical\n",
            "the run branch carries the converged content either way"
        );
        assert!(
            !b.merge_in_progress(),
            "the merge must be finalized (MERGE_HEAD cleared), not left dangling"
        );
    }

    #[test]
    fn commits_since_base_lists_this_branchs_own_commits_oldest_first() {
        let repo = init_repo();
        let repo_path = repo.path().to_str().unwrap().to_string();
        let wt_path = std::env::temp_dir().join(format!("rigger-wt-{}", uuid::Uuid::new_v4()));
        let wt =
            Worktree::create(&repo_path, wt_path.to_str().unwrap(), "rigger/u/plan", "").unwrap();

        // No commits beyond base yet.
        assert_eq!(wt.commits_since_base().unwrap(), Vec::<String>::new());

        std::fs::create_dir_all(wt_path.join("specs")).unwrap();
        std::fs::write(wt_path.join("specs").join("90-foo.md"), "amend one\n").unwrap();
        run_git(wt_path.to_str().unwrap(), &["add", "-A"]).unwrap();
        run_git(
            wt_path.to_str().unwrap(),
            &["commit", "-q", "-m", "amend one"],
        )
        .unwrap();
        let first = git(wt_path.to_str().unwrap(), &["rev-parse", "HEAD"])
            .unwrap()
            .trim()
            .to_string();

        std::fs::write(wt_path.join("specs").join("90-foo.md"), "amend two\n").unwrap();
        run_git(wt_path.to_str().unwrap(), &["add", "-A"]).unwrap();
        run_git(
            wt_path.to_str().unwrap(),
            &["commit", "-q", "-m", "amend two"],
        )
        .unwrap();
        let second = git(wt_path.to_str().unwrap(), &["rev-parse", "HEAD"])
            .unwrap()
            .trim()
            .to_string();

        assert_eq!(
            wt.commits_since_base().unwrap(),
            vec![first, second],
            "oldest-first, exactly the two commits beyond the run branch's HEAD"
        );
        wt.remove().unwrap();
    }

    #[test]
    fn cherry_pick_onto_run_branch_lands_commits_preserving_the_original_shas() {
        let repo = init_repo();
        let repo_path = repo.path().to_str().unwrap().to_string();
        let wt_path = std::env::temp_dir().join(format!("rigger-wt-{}", uuid::Uuid::new_v4()));
        let wt =
            Worktree::create(&repo_path, wt_path.to_str().unwrap(), "rigger/u/plan", "").unwrap();

        std::fs::create_dir_all(wt_path.join("specs")).unwrap();
        std::fs::write(wt_path.join("specs").join("90-foo.md"), "amend\n").unwrap();
        run_git(wt_path.to_str().unwrap(), &["add", "-A"]).unwrap();
        run_git(wt_path.to_str().unwrap(), &["commit", "-q", "-m", "amend"]).unwrap();
        let shas = wt.commits_since_base().unwrap();
        assert_eq!(shas.len(), 1);

        let landed = match wt.cherry_pick_onto_run_branch(&shas).unwrap() {
            CherryPickOutcome::Picked(landed) => landed,
            CherryPickOutcome::Conflict(detail) => {
                panic!("a clean specs/-only cherry-pick must not conflict: {detail}")
            }
        };
        assert_eq!(landed.len(), 1, "one commit in, one commit landed");

        // The content landed on the run branch...
        assert_eq!(
            std::fs::read_to_string(repo.path().join("specs").join("90-foo.md")).unwrap(),
            "amend\n",
            "the cherry-picked content must be present on the run branch"
        );
        // ...and the landed sha is the run branch's new HEAD: a real, reachable,
        // revertible commit on the run branch, whatever its relationship to the
        // pre-landing sha (see `CherryPickOutcome::Picked`'s doc comment).
        let run_head = git(&repo_path, &["rev-parse", "HEAD"])
            .unwrap()
            .trim()
            .to_string();
        assert_eq!(
            landed[0], run_head,
            "the landed sha must be the run branch's new HEAD"
        );
        wt.remove().unwrap();
    }

    #[test]
    fn cherry_pick_onto_run_branch_reports_a_conflict_and_leaves_the_run_branch_untouched() {
        let repo = init_repo();
        let repo_path = repo.path().to_str().unwrap().to_string();

        // The run branch independently gains a conflicting edit to the same spec path.
        std::fs::create_dir_all(repo.path().join("specs")).unwrap();
        std::fs::write(repo.path().join("specs").join("90-foo.md"), "operator\n").unwrap();
        run_git(&repo_path, &["add", "-A"]).unwrap();
        run_git(&repo_path, &["commit", "-q", "-m", "operator edit"]).unwrap();
        let head_before = run_git(&repo_path, &["rev-parse", "HEAD"])
            .unwrap()
            .trim()
            .to_string();

        // A plan worktree, branched from the PRE-operator-edit base, commits a
        // DIFFERENT amendment to the same path - a genuine conflict on cherry-pick.
        let wt_path = std::env::temp_dir().join(format!("rigger-wt-{}", uuid::Uuid::new_v4()));
        let wt =
            Worktree::create(&repo_path, wt_path.to_str().unwrap(), "rigger/u/plan", "").unwrap();
        // Rewind the worktree branch to before the operator edit landed, so its own
        // commit is a genuine divergent edit rather than a fast-forward.
        run_git(wt_path.to_str().unwrap(), &["reset", "--hard", "HEAD~1"]).unwrap();
        std::fs::create_dir_all(wt_path.join("specs")).unwrap();
        std::fs::write(wt_path.join("specs").join("90-foo.md"), "planner\n").unwrap();
        run_git(wt_path.to_str().unwrap(), &["add", "-A"]).unwrap();
        run_git(
            wt_path.to_str().unwrap(),
            &["commit", "-q", "-m", "planner amend"],
        )
        .unwrap();
        let sha = git(wt_path.to_str().unwrap(), &["rev-parse", "HEAD"])
            .unwrap()
            .trim()
            .to_string();

        match wt.cherry_pick_onto_run_branch(&[sha]).unwrap() {
            CherryPickOutcome::Conflict(detail) => assert!(
                detail.to_lowercase().contains("conflict"),
                "the conflict detail names the conflict; got: {detail}"
            ),
            CherryPickOutcome::Picked(_) => {
                panic!("a divergent edit to the same path must conflict, not land")
            }
        }

        // The run branch is UNTOUCHED and no cherry-pick is left in progress.
        assert_eq!(
            std::fs::read_to_string(repo.path().join("specs").join("90-foo.md")).unwrap(),
            "operator\n",
            "the aborted cherry-pick must not alter the run branch"
        );
        let head_now = run_git(&repo_path, &["rev-parse", "HEAD"])
            .unwrap()
            .trim()
            .to_string();
        assert_eq!(head_now, head_before, "HEAD is unchanged after the abort");
        assert!(
            !repo.path().join(".git").join("CHERRY_PICK_HEAD").exists(),
            "no cherry-pick is left in progress after the abort"
        );
        wt.remove().unwrap();
    }

    #[test]
    fn cherry_pick_onto_run_branch_is_idempotent_on_a_resumed_already_landed_sequence() {
        // sdet-u88c4-cherry-pick-resume-not-idempotent / adv-u88c4-crash-resume-halts-
        // the-whole-run-not-just-the-stage: a crash between THIS call's real git
        // success and the caller recording it can mean the SAME shas get cherry-picked
        // again on a resume - the caller recomputes `commits_since_base`, which is
        // IDENTITY-based, and a cherry-pick mints a NEW commit object, so the ORIGINAL
        // sha stays unreachable-by-identity from the run branch even once its content
        // already landed. A second call with the SAME (pre-landing) shas must resolve
        // to a benign no-op (`Picked(vec![])`), never a hard Err that would propagate
        // through the caller's `?` and halt the WHOLE step/wave, not just this stage.
        let repo = init_repo();
        let repo_path = repo.path().to_str().unwrap().to_string();
        let wt_path = std::env::temp_dir().join(format!("rigger-wt-{}", uuid::Uuid::new_v4()));
        let wt =
            Worktree::create(&repo_path, wt_path.to_str().unwrap(), "rigger/u/plan", "").unwrap();

        std::fs::create_dir_all(wt_path.join("specs")).unwrap();
        std::fs::write(wt_path.join("specs").join("90-foo.md"), "amend\n").unwrap();
        run_git(wt_path.to_str().unwrap(), &["add", "-A"]).unwrap();
        run_git(wt_path.to_str().unwrap(), &["commit", "-q", "-m", "amend"]).unwrap();
        let shas = wt.commits_since_base().unwrap();
        assert_eq!(shas.len(), 1);

        // FIRST call: a real, fresh landing (the pre-crash attempt).
        match wt.cherry_pick_onto_run_branch(&shas).unwrap() {
            CherryPickOutcome::Picked(landed) => assert_eq!(landed.len(), 1),
            CherryPickOutcome::Conflict(detail) => {
                panic!("a clean specs/-only cherry-pick must not conflict: {detail}")
            }
        }
        let head_after_first = git(&repo_path, &["rev-parse", "HEAD"])
            .unwrap()
            .trim()
            .to_string();

        // SECOND call with the SAME (original, pre-landing) `shas` - the resumed-
        // process shape: `commits_since_base` would recompute this identical list,
        // since the landed content sits on the run branch under a DIFFERENT
        // (cherry-pick-minted) commit object.
        let second = wt.cherry_pick_onto_run_branch(&shas);
        assert!(
            second.is_ok(),
            "a resumed already-landed cherry-pick must resolve, never hard-error: {:?}",
            second.err().map(|e| e.0)
        );
        match second.unwrap() {
            CherryPickOutcome::Picked(landed) => assert!(
                landed.is_empty(),
                "nothing NEW lands the second time - it was already there; got {landed:?}"
            ),
            CherryPickOutcome::Conflict(detail) => {
                panic!("an already-landed resume must never be treated as a conflict: {detail}")
            }
        }
        // The run branch is UNCHANGED by the idempotent second call, and no
        // cherry-pick is left in progress.
        assert_eq!(
            git(&repo_path, &["rev-parse", "HEAD"]).unwrap().trim(),
            head_after_first,
            "the second, already-applied call must not move the run branch's HEAD"
        );
        assert!(
            !repo.path().join(".git").join("CHERRY_PICK_HEAD").exists(),
            "no cherry-pick is left in progress after the idempotent no-op"
        );
        wt.remove().unwrap();
    }

    /// Commit three specs, each adding its OWN new path (no real conflicts among them), on a
    /// temp unit worktree; pre-land the commits at `pre_landed` directly on the run branch,
    /// independent of the interrupted sequence - modeling content that already reached the
    /// run branch by some earlier means, so replaying it becomes an EMPTY re-pick git pauses
    /// on. Then simulate the crash: run the RAW multi-sha cherry-pick directly (bypassing this
    /// crate's own skip-loop entirely) so it naturally pauses on the first now-empty commit
    /// (`paused_on`) - exactly the state a process death mid skip-loop leaves, never a
    /// synthetic one. A FRESH call with the ORIGINAL (identity) shas, exactly as a resumed
    /// process recomputing `commits_since_base` would, must self-heal the leftover marker
    /// through every chained empty commit and complete, never hard-error on git's own
    /// "cherry-pick is already in progress".
    fn assert_self_heals_a_leftover_cherry_pick_marker(pre_landed: &[usize], paused_on: &str) {
        let repo = init_repo();
        let repo_path = repo.path().to_str().unwrap().to_string();
        let (wt_path, wt) = temp_wt(&repo_path, "rigger/u/plan");

        let names = ["90-a.md", "90-b.md", "90-c.md"];
        let mut shas = Vec::new();
        for (name, content) in names.iter().zip(["amend a\n", "amend b\n", "amend c\n"]) {
            std::fs::create_dir_all(wt_path.join("specs")).unwrap();
            std::fs::write(wt_path.join("specs").join(name), content).unwrap();
            run_git(wt_path.to_str().unwrap(), &["add", "-A"]).unwrap();
            run_git(
                wt_path.to_str().unwrap(),
                &["commit", "-q", "-m", &format!("amend {name}")],
            )
            .unwrap();
            shas.push(
                git(wt_path.to_str().unwrap(), &["rev-parse", "HEAD"])
                    .unwrap()
                    .trim()
                    .to_string(),
            );
        }
        assert_eq!(shas.len(), 3);

        let mut pre_land = vec!["cherry-pick"];
        pre_land.extend(pre_landed.iter().map(|&i| shas[i].as_str()));
        run_git(&repo_path, &pre_land).unwrap();
        for &i in pre_landed {
            assert!(
                repo.path().join("specs").join(names[i]).exists(),
                "precondition: {}'s content is already present before the interrupted \
                 sequence starts",
                names[i]
            );
        }

        let raw = run_git(&repo_path, &["cherry-pick", &shas[0], &shas[1], &shas[2]]);
        assert!(
            raw.is_err(),
            "the raw sequence must pause on the empty {paused_on} commit, not succeed outright"
        );
        assert!(
            repo.path().join(".git").join("CHERRY_PICK_HEAD").exists(),
            "precondition: a cherry-pick sequencer marker is left in progress"
        );
        assert!(
            run_git(&repo_path, &["ls-files", "--unmerged"])
                .unwrap()
                .trim()
                .is_empty(),
            "precondition: the pause carries ZERO unmerged files - it is not a conflict"
        );

        let resumed = wt.cherry_pick_onto_run_branch(&shas);
        assert!(
            resumed.is_ok(),
            "a leftover in-progress marker must be self-healed, never surfaced as a hard \
             error: {:?}",
            resumed.err().map(|e| e.0)
        );
        match resumed.unwrap() {
            CherryPickOutcome::Conflict(detail) => {
                panic!(
                    "a self-healed, non-conflicting sequence must not read as a conflict: {detail}"
                )
            }
            CherryPickOutcome::Picked(_) => {}
        }
        assert!(
            !repo.path().join(".git").join("CHERRY_PICK_HEAD").exists(),
            "no cherry-pick is left in progress after the self-healed retry"
        );
        for name in names {
            assert!(
                repo.path().join("specs").join(name).exists(),
                "every commit's content must be present on the run branch after the \
                 self-healed retry completes the interrupted sequence: missing {name}"
            );
        }
        wt.remove().unwrap();
    }

    crate::test_cases! {
        /// adv-u88c4-r4-cherry-pick-in-progress-marker-survives-a-crash-mid-skip-loop: a crash
        /// WHILE the skip-loop is running (not merely after the whole sequence finishes, the
        /// case the sibling idempotency test above covers) leaves a real git CHERRY_PICK_HEAD
        /// sequencer marker on disk - zero unmerged files, since the pause is on an empty
        /// re-pick, never a conflict - that a fresh call must not choke on.
        cherry_pick_onto_run_branch_self_heals_a_leftover_marker_from_a_crash_mid_skip_loop:
            assert_self_heals_a_leftover_cherry_pick_marker(&[1], "second");
        /// arch-u88c4-r7-classification-skip-is-single-shot-not-a-loop /
        /// sdet-u88c4-r7-classification-skip-confirmed-live-and-untested-for-2plus-chained-
        /// empties: an ordinary multi-commit plan amendment can leave TWO OR MORE chained
        /// empty commits ahead of the leftover marker - each `--skip` only ever advances the
        /// sequencer by ONE, so a single attempt still finds CHERRY_PICK_HEAD set on the
        /// SECOND empty commit. Pre-landing the first AND second commits makes the replay
        /// pause on the first (empty) and skipping ONCE land on the second, ALSO empty - the
        /// state a process death right after the pause (before even ONE skip ran) leaves.
        cherry_pick_onto_run_branch_self_heals_a_leftover_marker_ahead_of_two_chained_empty_commits:
            assert_self_heals_a_leftover_cherry_pick_marker(&[0, 1], "first");
    }

    #[test]
    fn cherry_pick_onto_run_branch_self_heals_a_leftover_marker_with_no_sequencer_todo_file() {
        // adj-u88c4-r8-verdict-reject / sdet-u88c4-r8-single-commit-leftover-marker-
        // hard-errors-on-missing-sequencer-todo: git NEVER materializes
        // `.git/sequencer/todo` for a cherry-pick whose remaining set is exactly ONE
        // sha - a plain `git cherry-pick <sha>` never engages the sequencer
        // machinery at all, so `sequencer_todo_remaining`'s
        // `std::fs::read_to_string` sees a genuine `NotFound`, not an empty file.
        // This is the ORDINARY shape for a single-commit plan-stage amendment
        // resumed after a crash, not a corner case - the leftover-marker
        // classification above must resolve it with one `--skip`, never hard-error.
        let repo = init_repo();
        let repo_path = repo.path().to_str().unwrap().to_string();
        let wt_path = std::env::temp_dir().join(format!("rigger-wt-{}", uuid::Uuid::new_v4()));
        let wt =
            Worktree::create(&repo_path, wt_path.to_str().unwrap(), "rigger/u/plan", "").unwrap();

        std::fs::create_dir_all(wt_path.join("specs")).unwrap();
        std::fs::write(wt_path.join("specs").join("90-a.md"), "amend a\n").unwrap();
        run_git(wt_path.to_str().unwrap(), &["add", "-A"]).unwrap();
        run_git(
            wt_path.to_str().unwrap(),
            &["commit", "-q", "-m", "amend 90-a.md"],
        )
        .unwrap();
        let sha = git(wt_path.to_str().unwrap(), &["rev-parse", "HEAD"])
            .unwrap()
            .trim()
            .to_string();
        let shas = vec![sha.clone()];

        // Pre-land the ONLY commit's content directly, independent of the
        // interrupted attempt below, so replaying it becomes an EMPTY re-pick.
        run_git(&repo_path, &["cherry-pick", &sha]).unwrap();
        assert!(
            repo.path().join("specs").join("90-a.md").exists(),
            "precondition: the sole commit's content is already present"
        );

        // Simulate the crash: a RAW single-sha cherry-pick (bypassing this crate's
        // own skip-loop entirely), the exact same invocation shape a call with
        // `shas.len() == 1` makes - so it naturally pauses empty with NO sequencer
        // directory ever created.
        let raw = run_git(&repo_path, &["cherry-pick", &sha]);
        assert!(
            raw.is_err(),
            "the raw single-sha pick must pause on the empty commit, not succeed outright"
        );
        assert!(
            repo.path().join(".git").join("CHERRY_PICK_HEAD").exists(),
            "precondition: a cherry-pick sequencer marker is left in progress"
        );
        assert!(
            run_git(&repo_path, &["ls-files", "--unmerged"])
                .unwrap()
                .trim()
                .is_empty(),
            "precondition: the pause carries ZERO unmerged files - it is not a conflict"
        );
        let todo_path = git(&repo_path, &["rev-parse", "--git-path", "sequencer/todo"])
            .unwrap()
            .trim()
            .to_string();
        let todo_path = std::path::Path::new(&todo_path);
        let todo_path = if todo_path.is_absolute() {
            todo_path.to_path_buf()
        } else {
            std::path::Path::new(&repo_path).join(todo_path)
        };
        assert!(
            !todo_path.exists(),
            "precondition: git never materializes sequencer/todo for a genuinely \
             single-sha cherry-pick - {} must be ABSENT",
            todo_path.display()
        );

        // A FRESH call with the ORIGINAL (identity) single-element shas, exactly as
        // a resumed process recomputing a shrunk-to-one `still_pending` would - must
        // self-heal the leftover marker despite the missing sequencer/todo file,
        // never hard-error on it.
        let resumed = wt.cherry_pick_onto_run_branch(&shas);
        assert!(
            resumed.is_ok(),
            "a leftover in-progress marker with no sequencer/todo file must be \
             self-healed, never surfaced as a hard error: {:?}",
            resumed.err().map(|e| e.0)
        );
        match resumed.unwrap() {
            CherryPickOutcome::Conflict(detail) => {
                panic!("a self-healed, non-conflicting pause must not read as a conflict: {detail}")
            }
            CherryPickOutcome::Picked(_) => {}
        }
        assert!(
            !repo.path().join(".git").join("CHERRY_PICK_HEAD").exists(),
            "no cherry-pick is left in progress after the self-healed retry"
        );
        assert!(repo.path().join("specs").join("90-a.md").exists());
        wt.remove().unwrap();
    }

    #[test]
    fn cherry_pick_onto_run_branch_self_heals_when_a_multi_commit_amendment_shrinks_to_one_still_pending(
    ) {
        // adv-u88c4-r8-single-commit-trigger-is-any-still-pending-len-1-not-just-
        // single-commit-units: the sibling test above proves the missing-
        // sequencer/todo trigger with a literal one-commit-total unit; this proves
        // the trigger is reachable from the conductor's REAL, routine steady state
        // too - an ordinary multi-commit plan amendment whose `still_pending` set
        // has shrunk to exactly one sha across two separate calls (all-but-one of
        // its commits already confirmed landed), never merely a synthetic single-
        // commit unit.
        let repo = init_repo();
        let repo_path = repo.path().to_str().unwrap().to_string();
        let wt_path = std::env::temp_dir().join(format!("rigger-wt-{}", uuid::Uuid::new_v4()));
        let wt =
            Worktree::create(&repo_path, wt_path.to_str().unwrap(), "rigger/u/plan", "").unwrap();

        let mut shas = Vec::new();
        for (name, content) in [
            ("90-a.md", "amend a\n"),
            ("90-b.md", "amend b\n"),
            ("90-c.md", "amend c\n"),
        ] {
            std::fs::create_dir_all(wt_path.join("specs")).unwrap();
            std::fs::write(wt_path.join("specs").join(name), content).unwrap();
            run_git(wt_path.to_str().unwrap(), &["add", "-A"]).unwrap();
            run_git(
                wt_path.to_str().unwrap(),
                &["commit", "-q", "-m", &format!("amend {name}")],
            )
            .unwrap();
            shas.push(
                git(wt_path.to_str().unwrap(), &["rev-parse", "HEAD"])
                    .unwrap()
                    .trim()
                    .to_string(),
            );
        }
        assert_eq!(shas.len(), 3);

        // CALL 1: a real, ordinary landing of the first two commits - mirroring the
        // conductor calling `cherry_pick_onto_run_branch(&still_pending)` while two
        // of the amendment's three commits are still pending.
        let first_call = shas[0..2].to_vec();
        let first = wt.cherry_pick_onto_run_branch(&first_call).unwrap();
        match first {
            CherryPickOutcome::Picked(landed) => assert_eq!(landed.len(), 2),
            CherryPickOutcome::Conflict(detail) => {
                panic!("the first two commits must land cleanly: {detail}")
            }
        }
        for name in ["90-a.md", "90-b.md"] {
            assert!(repo.path().join("specs").join(name).exists());
        }

        // The conductor now recomputes `still_pending` down to the ONE commit its
        // patch-id check has not yet confirmed: `shas[2]` alone.
        let still_pending = vec![shas[2].clone()];

        // That sole remaining commit's content is separately, coincidentally
        // already on the run branch (an out-of-band landing, or a duplicate
        // amendment) - so a call with this single-element list pauses empty too.
        run_git(&repo_path, &["cherry-pick", &shas[2]]).unwrap();
        assert!(repo.path().join("specs").join("90-c.md").exists());

        // Simulate a crash mid this SECOND, single-remaining-sha call: the raw
        // single-sha invocation the crate's own fresh path would have made.
        let raw = run_git(&repo_path, &["cherry-pick", &shas[2]]);
        assert!(
            raw.is_err(),
            "the raw single-sha pick must pause on the empty commit, not succeed outright"
        );
        assert!(repo.path().join(".git").join("CHERRY_PICK_HEAD").exists());
        assert!(run_git(&repo_path, &["ls-files", "--unmerged"])
            .unwrap()
            .trim()
            .is_empty());
        let todo_path = git(&repo_path, &["rev-parse", "--git-path", "sequencer/todo"])
            .unwrap()
            .trim()
            .to_string();
        let todo_path = std::path::Path::new(&todo_path);
        let todo_path = if todo_path.is_absolute() {
            todo_path.to_path_buf()
        } else {
            std::path::Path::new(&repo_path).join(todo_path)
        };
        assert!(
            !todo_path.exists(),
            "precondition: a single-element still_pending call never materializes \
             sequencer/todo either - {} must be ABSENT",
            todo_path.display()
        );

        // A FRESH call with the SAME shrunk-to-one still_pending list, exactly as a
        // resumed conductor recomputing it would - must self-heal, never hard-error.
        let resumed = wt.cherry_pick_onto_run_branch(&still_pending);
        assert!(
            resumed.is_ok(),
            "a shrunk-to-one still_pending leftover marker with no sequencer/todo \
             file must be self-healed, never surfaced as a hard error: {:?}",
            resumed.err().map(|e| e.0)
        );
        match resumed.unwrap() {
            CherryPickOutcome::Conflict(detail) => {
                panic!("a self-healed, non-conflicting pause must not read as a conflict: {detail}")
            }
            CherryPickOutcome::Picked(_) => {}
        }
        assert!(!repo.path().join(".git").join("CHERRY_PICK_HEAD").exists());
        for name in ["90-a.md", "90-b.md", "90-c.md"] {
            assert!(repo.path().join("specs").join(name).exists());
        }
        wt.remove().unwrap();
    }

    #[test]
    fn patch_id_is_stable_across_a_cherry_pick_but_differs_for_different_content() {
        // op-u88c4-next-round-plan-commit-landing-is-log-carried-and-idempotent, item
        // 2: "an equivalent commit is reachable from the run branch by patch-id" -
        // this is the git-native, CONTENT-based (never tree-POSITION-based) identity
        // `find_landed_by_patch_id` is built on. A cherry-pick mints a brand new
        // commit object (different parent, timestamp, sha) but must carry the SAME
        // patch-id as its original, since `git patch-id` hashes only the diff.
        let repo = init_repo();
        let repo_path = repo.path().to_str().unwrap().to_string();
        let wt_path = std::env::temp_dir().join(format!("rigger-wt-{}", uuid::Uuid::new_v4()));
        let wt =
            Worktree::create(&repo_path, wt_path.to_str().unwrap(), "rigger/u/plan", "").unwrap();

        std::fs::create_dir_all(wt_path.join("specs")).unwrap();
        std::fs::write(wt_path.join("specs").join("90-a.md"), "amend a\n").unwrap();
        run_git(wt_path.to_str().unwrap(), &["add", "-A"]).unwrap();
        // A FIXED, deliberately old author/committer date - never the wall-clock "now"
        // a bare `git commit` would use - so the cherry-pick below (which stamps its
        // OWN committer time as real "now") cannot coincidentally reproduce a
        // byte-identical commit object in the rare same-committer-second case (see the
        // sibling idempotency tests' identical guard) - which would defeat this very
        // test's own `assert_ne!` below.
        commit_at_fixed_date(wt_path.to_str().unwrap(), "amend a");
        let original = git(wt_path.to_str().unwrap(), &["rev-parse", "HEAD"])
            .unwrap()
            .trim()
            .to_string();

        let landed = match wt
            .cherry_pick_onto_run_branch(std::slice::from_ref(&original))
            .unwrap()
        {
            CherryPickOutcome::Picked(landed) => landed,
            CherryPickOutcome::Conflict(detail) => {
                panic!("a clean specs/-only cherry-pick must not conflict: {detail}")
            }
        };
        assert_eq!(landed.len(), 1);
        assert_ne!(
            landed[0], original,
            "a cherry-pick mints a genuinely different commit object"
        );

        assert_eq!(
            wt.patch_id(&original).unwrap(),
            wt.patch_id(&landed[0]).unwrap(),
            "the same content re-committed by a cherry-pick must carry the SAME patch-id"
        );

        // A DIFFERENT commit (different content) must carry a DIFFERENT patch-id -
        // proving this is a real content hash, not a constant.
        std::fs::write(wt_path.join("specs").join("90-b.md"), "amend b\n").unwrap();
        run_git(wt_path.to_str().unwrap(), &["add", "-A"]).unwrap();
        run_git(
            wt_path.to_str().unwrap(),
            &["commit", "-q", "-m", "amend b"],
        )
        .unwrap();
        let other = git(wt_path.to_str().unwrap(), &["rev-parse", "HEAD"])
            .unwrap()
            .trim()
            .to_string();
        assert_ne!(
            wt.patch_id(&original).unwrap(),
            wt.patch_id(&other).unwrap(),
            "different content must carry a different patch-id"
        );

        wt.remove().unwrap();
    }

    #[test]
    fn find_landed_by_patch_id_recovers_by_content_never_by_position() {
        // op-u88c4-next-round-plan-commit-landing-is-log-carried-and-idempotent, item
        // 2, superseding the removed `already_landed_commits` (a tree-POSITION
        // heuristic UPHELD REJECT three times: arch-u88c4-r6-operator-ruling-
        // unimplemented-still-a-heuristic et al.): the recovery must find an
        // already-landed commit by its CONTENT identity, regardless of what else has
        // landed on the run branch in between - the exact case the old positional
        // walk could not handle (an intervening, unrelated commit shifts every
        // position).
        let repo = init_repo();
        let repo_path = repo.path().to_str().unwrap().to_string();
        let wt_path = std::env::temp_dir().join(format!("rigger-wt-{}", uuid::Uuid::new_v4()));
        let wt =
            Worktree::create(&repo_path, wt_path.to_str().unwrap(), "rigger/u/plan", "").unwrap();

        std::fs::create_dir_all(wt_path.join("specs")).unwrap();
        std::fs::write(wt_path.join("specs").join("98-diverged.md"), "amend\n").unwrap();
        run_git(wt_path.to_str().unwrap(), &["add", "-A"]).unwrap();
        run_git(wt_path.to_str().unwrap(), &["commit", "-q", "-m", "amend"]).unwrap();
        let original = git(wt_path.to_str().unwrap(), &["rev-parse", "HEAD"])
            .unwrap()
            .trim()
            .to_string();

        let landed = match wt
            .cherry_pick_onto_run_branch(std::slice::from_ref(&original))
            .unwrap()
        {
            CherryPickOutcome::Picked(landed) => landed,
            CherryPickOutcome::Conflict(detail) => {
                panic!("a clean specs/-only cherry-pick must not conflict: {detail}")
            }
        };
        assert_eq!(landed.len(), 1);

        // MEANWHILE: an unrelated commit lands on the run branch - the shape that
        // broke the old position-based walk.
        std::fs::write(repo.path().join("specs").join("99-unrelated.md"), "x\n").unwrap();
        run_git(&repo_path, &["add", "-A"]).unwrap();
        run_git(&repo_path, &["commit", "-q", "-m", "unrelated meanwhile"]).unwrap();

        let recovered = wt
            .find_landed_by_patch_id(&original, 50)
            .unwrap()
            .expect("content-based recovery must succeed despite the intervening commit");
        assert_eq!(
            recovered, landed[0],
            "the recovered sha must be the real, reachable run-branch commit"
        );

        // Content that was never landed at all must never be confirmed.
        std::fs::write(wt_path.join("specs").join("never-landed.md"), "nope\n").unwrap();
        run_git(wt_path.to_str().unwrap(), &["add", "-A"]).unwrap();
        run_git(
            wt_path.to_str().unwrap(),
            &["commit", "-q", "-m", "never landed"],
        )
        .unwrap();
        let never_landed = git(wt_path.to_str().unwrap(), &["rev-parse", "HEAD"])
            .unwrap()
            .trim()
            .to_string();
        assert_eq!(
            wt.find_landed_by_patch_id(&never_landed, 50).unwrap(),
            None,
            "content that was never landed must never be confirmed - never a guess"
        );

        // A window too narrow to reach the real match must also refuse to confirm -
        // never a guess beyond what was actually searched.
        std::fs::write(repo.path().join("specs").join("100-filler.md"), "y\n").unwrap();
        run_git(&repo_path, &["add", "-A"]).unwrap();
        run_git(&repo_path, &["commit", "-q", "-m", "filler 1"]).unwrap();
        std::fs::write(repo.path().join("specs").join("101-filler.md"), "z\n").unwrap();
        run_git(&repo_path, &["add", "-A"]).unwrap();
        run_git(&repo_path, &["commit", "-q", "-m", "filler 2"]).unwrap();
        assert_eq!(
            wt.find_landed_by_patch_id(&original, 2).unwrap(),
            None,
            "a window that does not reach the real match must not confirm it"
        );

        wt.remove().unwrap();
    }

    #[test]
    fn cherry_pick_onto_run_branch_self_heals_a_leftover_conflict_marker_as_conflict() {
        // op-u88c4-next-round-plan-commit-landing-is-log-carried-and-idempotent, item
        // 3 (GIT IN-PROGRESS STATE IS CLASSIFIED EXPLICITLY): a leftover
        // CHERRY_PICK_HEAD from an earlier, crashed call that DOES carry unmerged
        // files is a real conflict from that earlier call, not an empty-commit pause
        // - it must be reported as `Conflict`, never blindly discarded and retried
        // as if nothing had happened.
        let repo = init_repo();
        let repo_path = repo.path().to_str().unwrap().to_string();
        std::fs::create_dir_all(repo.path().join("specs")).unwrap();
        std::fs::write(repo.path().join("specs").join("90-a.md"), "base\n").unwrap();
        run_git(&repo_path, &["add", "-A"]).unwrap();
        run_git(&repo_path, &["commit", "-q", "-m", "seed 90-a.md"]).unwrap();

        let wt_path = std::env::temp_dir().join(format!("rigger-wt-{}", uuid::Uuid::new_v4()));
        let wt =
            Worktree::create(&repo_path, wt_path.to_str().unwrap(), "rigger/u/plan", "").unwrap();
        std::fs::write(wt_path.join("specs").join("90-a.md"), "amend on worktree\n").unwrap();
        run_git(wt_path.to_str().unwrap(), &["add", "-A"]).unwrap();
        run_git(
            wt_path.to_str().unwrap(),
            &["commit", "-q", "-m", "amend 90-a.md"],
        )
        .unwrap();
        let sha = git(wt_path.to_str().unwrap(), &["rev-parse", "HEAD"])
            .unwrap()
            .trim()
            .to_string();

        // Meanwhile the run branch diverges on the SAME path, so a raw cherry-pick
        // genuinely conflicts.
        std::fs::write(repo.path().join("specs").join("90-a.md"), "diverged\n").unwrap();
        run_git(&repo_path, &["add", "-A"]).unwrap();
        run_git(&repo_path, &["commit", "-q", "-m", "diverge 90-a.md"]).unwrap();

        // Simulate the crash: a raw cherry-pick left mid-conflict, bypassing this
        // crate's own conflict handling entirely.
        let raw = run_git(&repo_path, &["cherry-pick", &sha]);
        assert!(raw.is_err(), "the raw cherry-pick must conflict");
        assert!(
            repo.path().join(".git").join("CHERRY_PICK_HEAD").exists(),
            "precondition: a cherry-pick sequencer marker is left in progress"
        );
        assert!(
            !run_git(&repo_path, &["ls-files", "--unmerged"])
                .unwrap()
                .trim()
                .is_empty(),
            "precondition: the leftover marker DOES carry unmerged files - a real conflict"
        );

        let resumed = wt.cherry_pick_onto_run_branch(&[sha]);
        assert!(
            resumed.is_ok(),
            "a leftover conflict marker must resolve, never hard-error: {:?}",
            resumed.err().map(|e| e.0)
        );
        match resumed.unwrap() {
            CherryPickOutcome::Conflict(_) => {}
            CherryPickOutcome::Picked(landed) => panic!(
                "a leftover marker WITH unmerged files is a real conflict, not a \
                 resolvable pause: got Picked({landed:?})"
            ),
        }
        assert!(
            !repo.path().join(".git").join("CHERRY_PICK_HEAD").exists(),
            "no cherry-pick is left in progress after the classified conflict"
        );
        assert_eq!(
            std::fs::read_to_string(repo.path().join("specs").join("90-a.md")).unwrap(),
            "diverged\n",
            "the run branch is left untouched by a leftover conflict marker"
        );
        wt.remove().unwrap();
    }

    #[test]
    fn cherry_pick_onto_run_branch_reports_the_real_git_failure_not_a_wasted_skip_retry() {
        // The retry loop's guard (`skips_left == 0 || is_conflicted(&out) ||
        // !out.contains("previous cherry-pick is now empty")`) must break on the
        // FIRST error for a failure that is neither a real conflict NOR the
        // already-applied-empty marker this loop exists to skip past - a bad
        // (nonexistent) sha is exactly that shape (git fails with "fatal: bad
        // object", before any sequencer state even starts). Breaking immediately
        // means `shas`' own fatal reason reaches the caller; mis-classifying it as
        // skippable would instead waste a `git cherry-pick --skip` call (which
        // itself fails with the unrelated "no cherry-pick in progress", since
        // nothing was ever in progress) and surface THAT confusing message
        // instead of the real one.
        let repo = init_repo();
        let repo_path = repo.path().to_str().unwrap().to_string();
        let wt_path = std::env::temp_dir().join(format!("rigger-wt-{}", uuid::Uuid::new_v4()));
        let wt =
            Worktree::create(&repo_path, wt_path.to_str().unwrap(), "rigger/u/plan", "").unwrap();

        let bogus_sha = "deadbeefdeadbeefdeadbeefdeadbeefdeadbeef".to_string();
        let err = match wt.cherry_pick_onto_run_branch(&[bogus_sha]) {
            Ok(CherryPickOutcome::Picked(landed)) => {
                panic!("a nonexistent sha must never land; got Picked({landed:?})")
            }
            Ok(CherryPickOutcome::Conflict(detail)) => {
                panic!("a nonexistent sha is not a conflict; got Conflict({detail})")
            }
            Err(e) => e.0,
        };
        assert!(
            err.contains("bad object"),
            "the real git failure for a nonexistent sha must reach the caller; got: {err}"
        );
        assert!(
            !err.contains("no cherry-pick in progress"),
            "a fatal, non-conflict, non-empty failure must break immediately rather than \
             waste a --skip retry that masks it with an unrelated message; got: {err}"
        );
        wt.remove().unwrap();
    }

    #[test]
    fn revert_on_base_rolls_back_an_integrated_commit_with_a_provenance_message() {
        // spec 12, unit 4: revert_on_base reverses an integrated commit's diff on the run
        // branch as a NEW, message-carrying commit (an evented rollback, not a rewrite), so a
        // compensated unit's change is undone with auditable provenance.
        let repo = init_repo();
        let repo_path = repo.path().to_str().unwrap().to_string();
        let wt_path = std::env::temp_dir().join(format!("rigger-wt-{}", uuid::Uuid::new_v4()));
        let wt =
            Worktree::create(&repo_path, wt_path.to_str().unwrap(), "rigger/revert", "").unwrap();
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
            Worktree::create(&repo_path, wt_path.to_str().unwrap(), "rigger/conflict", "").unwrap();
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
        let wt =
            Worktree::create(&repo_path, wt_path.to_str().unwrap(), "rigger/idem", "").unwrap();
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
        let wt =
            Worktree::create(&repo_path, wt_path.to_str().unwrap(), "rigger/commit", "").unwrap();

        std::fs::write(wt_path.join("feature.txt"), "work\n").unwrap();
        assert!(
            path_is_dirty(&wt.dir).unwrap(),
            "an uncommitted file leaves a dirty tree"
        );

        let commit = wt.commit("rigger: commit before gating").unwrap();
        assert!(
            !commit.is_empty(),
            "committing must return the new commit hash"
        );
        assert!(
            !path_is_dirty(&wt.dir).unwrap(),
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
        // spec 12, unit 1: the HEAD_TREE sha is the content address of the committed tree. Two
        // DISTINCT commits (different message / parent / time, so a different COMMIT sha)
        // that carry byte-identical trees must yield the SAME tree sha - so a gate re-run
        // over an unchanged input is a cache hit - while a real content change must yield a
        // DIFFERENT sha - so a changed input misses.
        let repo = init_repo();
        let p = repo.path().to_str().unwrap().to_string();

        std::fs::write(repo.path().join("a.txt"), "one\n").unwrap();
        run_git(&p, &["add", "-A"]).unwrap();
        run_git(&p, &["commit", "-q", "-m", "first"]).unwrap();
        let t1 = rev_sha_of(&p, HEAD_TREE);
        assert_eq!(t1.len(), 40, "a git tree sha is 40 hex chars: {t1:?}");
        assert!(t1.chars().all(|c| c.is_ascii_hexdigit()));

        // A fresh EMPTY commit advances the COMMIT sha but leaves the tree bytes unchanged,
        // so the TREE sha is stable - the exact property head_sha_of does NOT have.
        let head1 = head_sha_of(&p);
        run_git(&p, &["commit", "--allow-empty", "-q", "-m", "empty"]).unwrap();
        assert_ne!(head_sha_of(&p), head1, "the commit sha advances");
        assert_eq!(
            rev_sha_of(&p, HEAD_TREE),
            t1,
            "an empty commit leaves the tree bytes unchanged, so the tree sha is stable"
        );

        // A real content change must move the tree sha.
        std::fs::write(repo.path().join("a.txt"), "two\n").unwrap();
        run_git(&p, &["add", "-A"]).unwrap();
        run_git(&p, &["commit", "-q", "-m", "second"]).unwrap();
        assert_ne!(
            rev_sha_of(&p, HEAD_TREE),
            t1,
            "changed content must change the tree sha"
        );

        // A worktree-less (empty) dir yields no address, so the caller skips addressing.
        assert!(rev_sha_of("", HEAD_TREE).is_empty());
    }

    #[test]
    fn integrate_lands_a_pre_committed_artifact_unchanged() {
        // After the conductor commits before gating, integrate must merge that EXACT
        // committed artifact - not re-commit, not drop it. The merged commit equals
        // the one `commit` produced, so gate-green and merged are the same commit.
        let repo = init_repo();
        let repo_path = repo.path().to_str().unwrap().to_string();
        let wt_path = std::env::temp_dir().join(format!("rigger-wt-{}", uuid::Uuid::new_v4()));
        let wt = Worktree::create(&repo_path, wt_path.to_str().unwrap(), "rigger/pre", "").unwrap();

        std::fs::write(wt_path.join("feature.txt"), "work\n").unwrap();
        let committed = wt.commit("rigger: pre-commit").unwrap();
        assert!(!path_is_dirty(&wt.dir).unwrap());

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
    fn restore_reviewed_sha_discards_both_tracked_and_untracked_residue() {
        // Spec 103, criterion 6: `reset_branch_to` alone only rewinds TRACKED content - an
        // untracked file (e.g. a reviewer's own scratch droppings, against protocol) would
        // survive a plain `git reset --hard` and keep the tree dirty. This proves
        // `restore_reviewed_sha` clears both: a committed change past `sha` AND an
        // untracked file are both gone, and the worktree is exactly `sha` again.
        let repo = init_repo();
        let repo_path = repo.path().to_str().unwrap().to_string();
        let wt_path = std::env::temp_dir().join(format!("rigger-wt-{}", uuid::Uuid::new_v4()));
        let wt =
            Worktree::create(&repo_path, wt_path.to_str().unwrap(), "rigger/residue", "").unwrap();

        std::fs::write(wt_path.join("reviewed.txt"), "the reviewed work\n").unwrap();
        let reviewed_sha = wt.commit("rigger: reviewed work").unwrap();

        // Residue: a committed change AND an untracked file, both past `reviewed_sha`.
        std::fs::write(wt_path.join("reviewed.txt"), "tampered\n").unwrap();
        wt.commit("wip: residue commit").unwrap();
        std::fs::write(wt_path.join("untracked-residue.txt"), "leftover\n").unwrap();
        assert!(
            path_is_dirty(&wt.dir).unwrap(),
            "premise: the tree must be dirty before restore"
        );

        wt.restore_reviewed_sha(&reviewed_sha).unwrap();

        assert_eq!(
            head_sha_of(wt_path.to_str().unwrap()),
            reviewed_sha,
            "the branch tip must be back at exactly the reviewed sha"
        );
        assert!(
            !path_is_dirty(&wt.dir).unwrap(),
            "the worktree must be clean - both the tracked residue commit and the \
             untracked file must be gone"
        );
        assert!(
            !wt_path.join("untracked-residue.txt").exists(),
            "an untracked file left by the residue must not survive the restore"
        );
        assert_eq!(
            std::fs::read_to_string(wt_path.join("reviewed.txt")).unwrap(),
            "the reviewed work\n",
            "the tracked file must be back at its reviewed content"
        );
        wt.remove().unwrap();
    }

    /// `changed_files` on a temp worktree on `branch`, after `work` edits its checkout.
    fn assert_changed_files(branch: &str, work: impl FnOnce(&std::path::Path), expected: &[&str]) {
        let repo = init_repo();
        let repo_path = repo.path().to_str().unwrap().to_string();
        let (wt_path, wt) = temp_wt(&repo_path, branch);
        work(&wt_path);
        assert_eq!(wt.changed_files().unwrap(), expected);
        wt.remove().unwrap();
    }

    crate::test_cases! {
        /// A committed file renamed with `git mv` (so git reports it as `R` rather than an
        /// add+delete pair) yields the destination path only - never the bogus
        /// `orig.txt -> renamed.txt` string the plain --porcelain form would have yielded, and
        /// never the original `orig.txt`.
        changed_files_reports_only_the_rename_destination: assert_changed_files(
            "rigger/rename",
            |wt_path| {
                let wt = wt_path.to_str().unwrap();
                std::fs::write(wt_path.join("orig.txt"), "content\n").unwrap();
                run_git(wt, &["add", "-A"]).unwrap();
                run_git(wt, &["commit", "-q", "-m", "add orig"]).unwrap();
                run_git(wt, &["mv", "orig.txt", "renamed.txt"]).unwrap();
            },
            &["renamed.txt"],
        );
        /// The plain --porcelain form C-quotes this to `"a file.txt"`; the -z form must hand
        /// back the real, unquoted path.
        changed_files_unquotes_paths_with_spaces: assert_changed_files(
            "rigger/spaces",
            |wt_path| std::fs::write(wt_path.join("a file.txt"), "work\n").unwrap(),
            &["a file.txt"],
        );
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
        let wt1 = Worktree::create(&repo_path, dir1.to_str().unwrap(), branch, "").unwrap();
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
        let wt2 = Worktree::create(&repo_path, dir2.to_str().unwrap(), branch, "").unwrap();
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
        // > the cache-home default (spec 89, criterion 2: SCRATCH IS OUTSIDE THE STORE
        // TREE). The default no longer nests inside the repo's own `.rigger` - a spawn's
        // own TMPDIR/CARGO_TARGET_DIR (via `rigger scratch`) used to resolve under
        // `<repo>/.rigger/tmp`, so a `tempfile::tempdir()` created under it walked up into
        // the REAL repo's `.rigger/events.db` and either bound a store it should not have,
        // or (spec 89 Problem 4) had its live worktrees swept as a stray fixture's.
        //
        // This assertion reads (never mutates) the real `XDG_CACHE_HOME`/`HOME` so it never
        // races a concurrently-running test over process-global environment state; on a
        // genuinely homeless host (neither set - a bare CI container) [`scratch_root_path`]
        // has nothing to key a cache path on and keeps the pre-relocation repo-nested
        // degrade, which the `else` arm below proves instead.
        let repo = init_repo();
        let repo_path = repo.path().to_str().unwrap().to_string();

        let dflt = scratch_root(&repo_path, "", None);
        let homeful = std::env::var_os("XDG_CACHE_HOME")
            .filter(|v| !v.is_empty())
            .or_else(|| std::env::var_os("HOME").filter(|v| !v.is_empty()))
            .is_some();
        if homeful {
            assert_ne!(
                dflt,
                format!("{repo_path}/.rigger/tmp"),
                "the default must no longer nest inside the repo's own .rigger: {dflt:?}"
            );
            assert!(
                !dflt.contains("/.rigger/") && !dflt.ends_with("/.rigger"),
                "the default must never live under any .rigger: {dflt:?}"
            );
            assert!(
                !std::path::Path::new(&dflt).starts_with(&repo_path),
                "the default must live outside the repo entirely, on the cache-home mount: \
                 {dflt:?}"
            );
            let expected = cache_scratch_root_from(
                &repo_path,
                std::env::var_os("XDG_CACHE_HOME"),
                std::env::var_os("HOME"),
            )
            .expect("a non-empty repo with a real HOME/XDG_CACHE_HOME always resolves");
            assert_eq!(
                std::path::PathBuf::from(&dflt),
                expected,
                "must equal the SAME pure resolver `rigger scratch`/`validate`/the reaper share"
            );
        } else {
            assert_eq!(dflt, format!("{repo_path}/.rigger/tmp"));
        }
        assert!(std::path::Path::new(&dflt).is_dir(), "the root is created");
        // Unlike the pre-relocation default, `dflt` may now live outside `repo`'s own
        // TempDir (on the cache-home mount) and so is NOT auto-cleaned by `repo`'s
        // `Drop` - mirror the tilde-case cleanup below so this test never leaks a real
        // directory onto the operator's `~/.cache/rigger` on every run (round 3 review:
        // sdet-u89c2r3-default-scratch-test-leaks-outside-fixture-tempdir).
        let _ = std::fs::remove_dir_all(&dflt);

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

    // ---- cache_scratch_root_from: PURE, so every case is driven with explicit params,
    // never the real process environment (spec 89, criterion 2) ----

    #[test]
    fn cache_scratch_root_from_prefers_xdg_over_home_and_nests_under_rigger() {
        let repo = "/home/dev/acme";
        let got = cache_scratch_root_from(
            repo,
            Some(std::ffi::OsString::from("/xdg-cache")),
            Some(std::ffi::OsString::from("/home/dev")),
        )
        .unwrap();
        assert_eq!(
            got,
            std::path::PathBuf::from("/xdg-cache/rigger").join(marker_filename(repo).unwrap())
        );
    }

    #[test]
    fn cache_scratch_root_from_falls_back_to_home_dot_cache_absent_xdg() {
        let repo = "/home/dev/acme";
        let got = cache_scratch_root_from(repo, None, Some(std::ffi::OsString::from("/home/dev")))
            .unwrap();
        assert_eq!(
            got,
            std::path::PathBuf::from("/home/dev/.cache/rigger")
                .join(marker_filename(repo).unwrap())
        );
    }

    #[test]
    fn cache_scratch_root_from_none_when_repo_is_empty() {
        // Nothing to key the cache path on; the caller degrades to the pre-relocation
        // repo-nested default instead (see `scratch_root_path`).
        assert_eq!(
            cache_scratch_root_from(
                "",
                Some(std::ffi::OsString::from("/xdg-cache")),
                Some(std::ffi::OsString::from("/home/dev")),
            ),
            None
        );
    }

    #[test]
    fn cache_scratch_root_from_none_when_homeless() {
        assert_eq!(cache_scratch_root_from("/home/dev/acme", None, None), None);
    }

    #[test]
    fn cache_scratch_root_from_gives_distinct_repos_distinct_directories() {
        let a = cache_scratch_root_from(
            "/home/dev/proj-a",
            Some(std::ffi::OsString::from("/xdg-cache")),
            None,
        )
        .unwrap();
        let b = cache_scratch_root_from(
            "/home/dev/proj-b",
            Some(std::ffi::OsString::from("/xdg-cache")),
            None,
        )
        .unwrap();
        assert_ne!(
            a, b,
            "two different repos must never alias onto one cache directory"
        );
    }

    /// A never-advanced (terminal) unit worktree `done` and an in-flight one `live` whose
    /// branch carries a commit the run branch does not have, under `root`; returns their dirs.
    fn done_and_live_units(repo_path: &str, root: &str) -> (String, String) {
        let (done_dir, _) = unit_wt(repo_path, root, "done");
        let (live_dir, live) = unit_wt(repo_path, root, "live");
        std::fs::write(std::path::Path::new(&live_dir).join("wip.txt"), "wip\n").unwrap();
        live.commit("rigger: in-flight").unwrap();
        (done_dir, live_dir)
    }

    #[test]
    fn sweep_terminal_removes_merged_worktrees_and_keeps_inflight_ones() {
        // Gap 14 maintenance: a worktree whose branch is already an ancestor of the
        // run branch serves no in-flight unit and is swept; an unmerged branch is a
        // live checkpoint and must be left alone. Only dirs under the scratch root
        // are considered.
        let (_repo, repo_path, root) = scratch_repo(true);
        let (done_dir, live_dir) = done_and_live_units(&repo_path, &root);

        assert_eq!(
            sweep(&repo_path, &root, &[]),
            1,
            "exactly the terminal worktree is swept"
        );
        assert!(
            !exists(&done_dir),
            "the merged/never-advanced worktree is gone"
        );
        assert!(
            std::path::Path::new(&live_dir).join("wip.txt").exists(),
            "the in-flight worktree is untouched"
        );
    }

    #[test]
    fn sweep_terminal_spares_a_live_units_worktree_even_at_the_empty_diff_run_tip() {
        // Spec 64 criterion 4: the merged-only ancestry rule alone is NOT sufficient. A
        // PARKED unit whose attempt produced an EMPTY diff has a branch tip that IS an
        // ancestor of the run branch (trivially - it never advanced past it) while the
        // unit is still LIVE in review. Liveness - read from the current run's event-log
        // slice, passed in as `live_branches` - must spare it despite it passing the
        // ancestry test; a dead unit in the identical empty-diff shape is still swept.
        let repo = init_repo();
        let repo_path = repo.path().to_str().unwrap().to_string();
        run_git(&repo_path, &["checkout", "-b", "rigger-run"]).unwrap();
        let root = scratch_root(&repo_path, "", None);

        // Live, empty-diff: branch created off the run branch, never advanced (so it IS
        // an ancestor of run_branch, exactly like a terminal unit) but its unit is still
        // in-flight per the current run's log.
        let live_dir = format!("{root}/rigger-wt-live-empty-diff");
        Worktree::create(&repo_path, &live_dir, "rigger/u/live-empty-diff", "").unwrap();

        // Dead, empty-diff: the identical shape, but no live unit claims its branch - this
        // is the case the pre-existing ancestry rule already swept and must keep sweeping.
        let dead_dir = format!("{root}/rigger-wt-dead-empty-diff");
        Worktree::create(&repo_path, &dead_dir, "rigger/u/dead-empty-diff", "").unwrap();

        let mut live_branches = std::collections::HashSet::new();
        live_branches.insert("rigger/u/live-empty-diff".to_string());

        let removed = sweep_terminal(
            &repo_path,
            &root,
            "rigger-run",
            &live_branches,
            &std::collections::HashSet::new(),
            &[],
        )
        .unwrap();
        assert_eq!(removed, 1, "only the dead empty-diff worktree is swept");
        assert!(
            std::path::Path::new(&live_dir).exists(),
            "the live unit's worktree survives despite its branch tip equalling the run tip"
        );
        assert!(
            !std::path::Path::new(&dead_dir).exists(),
            "a dead unit in the identical empty-diff shape is still reclaimed"
        );
    }

    // --- Spec 83, criterion 1: THE FENCE (direct `spawn_fence` unit tests) ---

    use crate::conductor::STREAM;
    use crate::eventstore::sqlite::Store;
    use crate::eventstore::{EventStore, ExpectedRevision};
    use crate::spawn::SpawnResult;

    #[test]
    fn spawn_fence_is_no_spawn_when_the_unit_has_never_requested_one() {
        assert_eq!(spawn_fence(&[], "ghost-unit"), SpawnFence::NoSpawn);
        assert!(SpawnFence::NoSpawn.permits_reclaim());
    }

    #[test]
    fn spawn_fence_is_in_flight_when_the_latest_spawn_has_no_recorded_result() {
        let store = Store::open(":memory:").unwrap();
        let req = crate::spawn::test_request("u1", "u1", "implementer", 0, "task");
        store
            .append(STREAM, ExpectedRevision::Any, &[req.to_event().unwrap()])
            .unwrap();
        let events = run_log(&store);

        let fence = spawn_fence(&events, "u1");
        assert_eq!(
            fence,
            SpawnFence::InFlight {
                spawn: req.id.clone()
            }
        );
        assert!(
            !fence.permits_reclaim(),
            "an in-flight latest spawn must NOT permit a reclaim"
        );
        assert!(fence.evidence("u1").contains(&req.id));
    }

    #[test]
    fn spawn_fence_is_terminal_at_the_results_position_once_a_real_result_lands() {
        let store = Store::open(":memory:").unwrap();
        let req = crate::spawn::test_request("u2", "u2", "implementer", 0, "task");
        store
            .append(STREAM, ExpectedRevision::Any, &[req.to_event().unwrap()])
            .unwrap();
        let res = SpawnResult::ok(&req.id, "done");
        let pos = store
            .append(STREAM, ExpectedRevision::Any, &[res.to_event().unwrap()])
            .unwrap()
            .one("result")
            .unwrap();
        let events = run_log(&store);

        let fence = spawn_fence(&events, "u2");
        assert_eq!(
            fence,
            SpawnFence::Terminal {
                spawn: req.id.clone(),
                at: pos,
                hung: false,
            }
        );
        assert!(fence.permits_reclaim());
        let ev = fence.evidence("u2");
        assert!(ev.contains(&req.id) && ev.contains(&pos.to_string()));
    }

    #[test]
    fn spawn_fence_finds_the_true_results_position_past_a_later_event_sharing_its_id() {
        // A decoy event AFTER the real result reuses the SAME id in a DIFFERENT event type
        // (a re-parked `SpawnRequested`, unrealistic in production but constructible directly
        // on the log) - its JSON body still decodes successfully as a `SpawnResult` (both
        // share the `id` field, and `SpawnResult`'s other fields all default), so the
        // position lookup's `find` predicate must match on TYPE *and* id: a `||` in place of
        // the `&&`, or a flipped `==`, would let this wrong-typed decoy's LATER position (or
        // no position at all) leak into the evidence instead of the real result's.
        let store = Store::open(":memory:").unwrap();
        let req = crate::spawn::test_request("u5", "u5", "implementer", 0, "task");
        store
            .append(STREAM, ExpectedRevision::Any, &[req.to_event().unwrap()])
            .unwrap();
        let res = SpawnResult::ok(&req.id, "done");
        let real_pos = store
            .append(STREAM, ExpectedRevision::Any, &[res.to_event().unwrap()])
            .unwrap()
            .one("result")
            .unwrap();
        // The decoy: a SECOND `SpawnRequested` reusing the identical id, appended AFTER the
        // real result so it sits at a LATER position - reverse iteration reaches it FIRST.
        store
            .append(STREAM, ExpectedRevision::Any, &[req.to_event().unwrap()])
            .unwrap();
        let events = run_log(&store);

        let fence = spawn_fence(&events, "u5");
        assert_eq!(
            fence,
            SpawnFence::Terminal {
                spawn: req.id.clone(),
                at: real_pos,
                hung: false,
            },
            "the position must be the REAL result's, never the later decoy's matching id"
        );
    }

    #[test]
    fn spawn_fence_names_a_liveness_fault_result_as_hung() {
        let store = Store::open(":memory:").unwrap();
        let mut req = crate::spawn::test_request("u3", "u3", "implementer", 0, "task");
        req.max_wall_clock = Some(60);
        store
            .append(STREAM, ExpectedRevision::Any, &[req.to_event().unwrap()])
            .unwrap();
        let fault = SpawnResult::liveness_fault(&req.id, "stale marker", "infra");
        let pos = store
            .append(STREAM, ExpectedRevision::Any, &[fault.to_event().unwrap()])
            .unwrap()
            .one("result")
            .unwrap();
        let events = run_log(&store);

        let fence = spawn_fence(&events, "u3");
        assert_eq!(
            fence,
            SpawnFence::Terminal {
                spawn: req.id.clone(),
                at: pos,
                hung: true,
            }
        );
        assert!(
            fence.permits_reclaim(),
            "a hung latest spawn IS reclaimable"
        );
        assert!(fence.evidence("u3").contains("hung"));
    }

    #[test]
    fn spawn_fence_tracks_only_the_units_latest_spawn_across_roles_and_attempts() {
        // Two roles for the SAME unit: the implementer already answered (attempt 0), but the
        // review-tier spawn requested AFTER it (attempt 1, a distinct role) has not - the
        // fence must follow the LATEST request, not the first one, keeping the worktree live.
        let store = Store::open(":memory:").unwrap();
        let impl_req = crate::spawn::test_request("u4", "u4", "implementer", 0, "task");
        store
            .append(
                STREAM,
                ExpectedRevision::Any,
                &[impl_req.to_event().unwrap()],
            )
            .unwrap();
        store
            .append(
                STREAM,
                ExpectedRevision::Any,
                &[SpawnResult::ok(&impl_req.id, "done").to_event().unwrap()],
            )
            .unwrap();

        let review_req = crate::spawn::test_request("u4", "u4", "adversary", 1, "review");
        store
            .append(
                STREAM,
                ExpectedRevision::Any,
                &[review_req.to_event().unwrap()],
            )
            .unwrap();
        let events = run_log(&store);

        let fence = spawn_fence(&events, "u4");
        assert_eq!(
            fence,
            SpawnFence::InFlight {
                spawn: review_req.id.clone()
            },
            "the LATEST spawn (the still-unanswered review) governs, not the answered implementer"
        );
    }

    #[test]
    fn spawn_fence_scoped_out_of_a_prior_run_never_sees_its_resolved_spawn() {
        // A prior run's unit shared the same slug and its spawn is long resolved; an
        // UNSCOPED read would wrongly see it as terminal. Scoping to the current run (as
        // `sweep_terminal`'s caller does) must show `NoSpawn` instead - the current run
        // never requested anything for this unit.
        let store = Store::open(":memory:").unwrap();
        let prior =
            crate::spawn::test_request("reused-slug", "reused-slug", "implementer", 0, "task");
        store
            .append(STREAM, ExpectedRevision::Any, &[prior.to_event().unwrap()])
            .unwrap();
        store
            .append(
                STREAM,
                ExpectedRevision::Any,
                &[SpawnResult::ok(&prior.id, "done").to_event().unwrap()],
            )
            .unwrap();
        store
            .append(
                STREAM,
                ExpectedRevision::Any,
                &[Event::new(
                    crate::run::TYPE_RUN_STARTED,
                    br#"{"run":"r2","criteria":["c"]}"#.to_vec(),
                )],
            )
            .unwrap();
        let events = run_log(&store);
        let scoped = crate::run::current_run(&events);

        assert_eq!(spawn_fence(scoped, "reused-slug"), SpawnFence::NoSpawn);
    }

    /// A `SpawnRequested` event for `unit`, unanswered - the shape `spawn_fence` reads as
    /// "in flight".
    fn requested(unit: &str) -> Event {
        let req = crate::spawn::test_request(unit, unit, "implementer", 0, "task");
        req.to_event().unwrap()
    }

    /// A `SpawnRequested` for `unit` followed by the `SpawnResult` `result` builds for its id:
    /// `SpawnResult::ok` is the shape `spawn_fence` reads as "terminal", a liveness fault the
    /// shape it reads as "hung".
    fn requested_with(unit: &str, result: impl FnOnce(&str) -> SpawnResult) -> Vec<Event> {
        let req = crate::spawn::test_request(unit, unit, "implementer", 0, "task");
        let res = result(&req.id);
        vec![req.to_event().unwrap(), res.to_event().unwrap()]
    }

    /// Sweep a merged unit worktree `slug` that reads terminal by BOTH pre-spec-83 signals
    /// (merged into the run branch, absent from `live_branches`) with `events` recorded: it is
    /// reclaimed exactly when `reclaimed`, and otherwise survives on disk.
    fn assert_sweep_of_a_merged_unit(slug: &str, events: &[Event], reclaimed: bool, why: &str) {
        let (_repo, repo_path, root) = scratch_repo(true);
        let (dir, _) = unit_wt(&repo_path, &root, slug);
        assert_eq!(
            sweep(&repo_path, &root, events),
            usize::from(reclaimed),
            "{why}"
        );
        assert_eq!(exists(&dir), !reclaimed, "{why}");
    }

    crate::test_cases! {
        /// Spec 83, criterion 1: THE FENCE - exactly the shape that raced ahead of a straggler
        /// spawn in the observed bug (a reviewer's verdict integrates the unit while an
        /// adversary/sdet lens for the SAME unit is still working the identical worktree). No
        /// liveness MARKER exists at all - the Design's explicit "absence is never reapable
        /// evidence on its own" case - so the worktree survives despite reading terminal by
        /// every pre-spec-83 signal.
        sweep_terminal_spares_a_merged_branch_whose_units_latest_spawn_is_still_in_flight:
            assert_sweep_of_a_merged_unit(
                "fenced",
                &[requested("fenced")],
                false,
                "an in-flight latest spawn must fence off the reclaim entirely",
            );
        /// The counterpart to the fence case: once the SAME shape's latest spawn has actually
        /// answered, the fence must not block the pre-existing removal.
        sweep_terminal_reclaims_a_merged_branch_once_its_latest_spawn_has_a_real_result:
            assert_sweep_of_a_merged_unit(
                "answered",
                &requested_with("answered", |id| SpawnResult::ok(id, "done")),
                true,
                "a terminal latest spawn does not block the reclaim",
            );
        /// A latest spawn the liveness sweep already classified hung (a recorded
        /// liveness-fault SpawnResult, spec 10 unit 3) is ALSO terminal for fencing purposes:
        /// "hung past max_wall_clock" is the fence's other reclaim-eligible arm.
        sweep_terminal_reclaims_a_merged_branch_whose_latest_spawn_is_hung:
            assert_sweep_of_a_merged_unit(
                "hung",
                &requested_with("hung", |id| {
                    SpawnResult::liveness_fault(id, "stale marker", "infra")
                }),
                true,
                "a hung latest spawn does not block the reclaim",
            );
        /// Back-compat: a branch whose unit never recorded ANY spawn (`SpawnFence::NoSpawn`)
        /// must sweep exactly as it did before spec 83 - the fence has nothing to add and must
        /// never itself become a NEW reason to keep dead residue around forever.
        sweep_terminal_reclaims_a_merged_branch_with_no_spawn_recorded_at_all_unchanged:
            assert_sweep_of_a_merged_unit("nospawn", &[], true, "no spawn recorded at all");
    }

    #[test]
    fn sweep_terminal_spares_a_dirty_no_spawn_worktree_pending_halt_recovery() {
        // Spec 89, criterion 1 (A HALT NEVER DISCARDS A TREE), round 2 fix
        // (sdet-u89c1-sweep-terminal-discards-halted-tree): a worktree can exist, dirty, at
        // this exact "branch tip is an ancestor of run_branch, no spawn ever recorded" shape
        // for a REAL reason, not just the back-compat test above's clean one - a store
        // restored from an older snapshot (or this project's very first `rigger step`) whose
        // event log has not yet caught up to a worktree already sitting on disk. The
        // halted-commit recovery that would turn this dirt into a durable `wip` commit lives
        // in `run_single_stage` (src/conductor.rs), which `cmd_step` calls strictly AFTER
        // this sweep (main.rs) - so sweeping a dirty candidate here, before that recovery
        // ever runs, discards the tree outright rather than merely deferring its capture.
        // `SpawnFence::NoSpawn` alone (the back-compat test just above) must keep sweeping a
        // CLEAN worktree in this shape exactly as before; only DIRTY content changes the
        // outcome, regardless of the fence - PROVIDED the branch is one `declared_units`
        // names (round 3 fix,
        // `step_start_sweep_spares_a_live_units_empty_diff_worktree_but_reclaims_a_dead_
        // ancestor_leftover`): a dirty branch this run's own workflow does NOT declare is
        // still genuinely dead residue, not a halted spawn, and is swept exactly as before -
        // pinned by the second half of this test below.
        let repo = init_repo();
        let repo_path = repo.path().to_str().unwrap().to_string();
        run_git(&repo_path, &["checkout", "-b", "rigger-run"]).unwrap();
        let root = scratch_root(&repo_path, "", None);

        let dir = format!("{root}/rigger-wt-halted");
        Worktree::create(&repo_path, &dir, "rigger/u/halted", "").unwrap();
        std::fs::write(
            std::path::Path::new(&dir).join("halted-work.txt"),
            "abandoned mid-edit\n",
        )
        .unwrap();

        let mut declared_units = std::collections::HashSet::new();
        declared_units.insert("rigger/u/halted".to_string());
        let removed = sweep_terminal(
            &repo_path,
            &root,
            "rigger-run",
            &std::collections::HashSet::new(),
            &declared_units,
            &[],
        )
        .unwrap();
        assert_eq!(
            removed, 0,
            "a dirty candidate this workflow declares is spared, never force-removed"
        );
        assert!(
            std::path::Path::new(&dir).join("halted-work.txt").exists(),
            "the abandoned edit must survive the sweep untouched"
        );

        // The negative-space twin: the IDENTICAL dirty, no-spawn, empty-diff shape, but for a
        // branch this workflow does NOT declare - genuinely dead, unrelated residue (a prior
        // run's leftover registration, a hand-made fixture), not a halted spawn worth
        // protecting - is still reclaimed exactly as it was before this criterion.
        let orphan_dir = format!("{root}/rigger-wt-undeclared-orphan");
        Worktree::create(&repo_path, &orphan_dir, "rigger/u/undeclared-orphan", "").unwrap();
        std::fs::write(
            std::path::Path::new(&orphan_dir).join("stray.txt"),
            "unrelated debris\n",
        )
        .unwrap();
        let removed = sweep_terminal(
            &repo_path,
            &root,
            "rigger-run",
            &std::collections::HashSet::new(),
            &declared_units,
            &[],
        )
        .unwrap();
        assert_eq!(
            removed, 1,
            "a dirty candidate this workflow never declared is still reclaimed"
        );
        assert!(
            !std::path::Path::new(&orphan_dir).exists(),
            "the undeclared, unrelated worktree is gone"
        );
    }

    #[test]
    fn sweep_terminal_prints_evidence_for_a_kept_decision_but_not_for_a_removed_no_spawn_one() {
        // Spec 83, criterion 1: "each sweep decision is attributable from the log with its
        // evidence". Drives `sweep_terminal_logged` directly (the DI seam) so the printed
        // evidence text itself is an assertable fact: a FENCED (kept) worktree names WHY in
        // the log, while the ordinary NO-SPAWN removal (the ubiquitous common case, unrelated
        // to this fence) stays exactly as silent as it was before spec 83 - no new noise for
        // every routine integration.
        let (_repo, repo_path, root) = scratch_repo(true);
        unit_wt(&repo_path, &root, "fenced");
        unit_wt(&repo_path, &root, "nospawn");

        let (removed, lines) = sweep_logged(&repo_path, &root, &[requested("fenced")]);
        assert_eq!(removed, 1, "only the no-spawn worktree is reclaimed");
        assert!(
            lines
                .iter()
                .any(|l| l.contains("kept") && l.contains("fenced") && l.contains("in flight")),
            "the fenced (kept) decision must be attributable from the log: {lines:?}"
        );
        assert!(
            !lines.iter().any(|l| l.contains("nospawn")),
            "the ordinary no-spawn removal stays silent, exactly as before spec 83: {lines:?}"
        );
    }

    #[test]
    fn sweep_terminal_prints_evidence_for_a_removed_terminal_spawn_decision() {
        // The counterpart: a MERGED branch whose latest spawn genuinely answered is REMOVED,
        // and that removal is ALSO attributable - the evidence line must fire on the
        // REMOVING arm, not just the KEPT one (this is what
        // `sweep_terminal_prints_evidence_for_a_kept_decision...` cannot pin alone: a
        // flipped condition that prints on the WRONG arm still passes that test's `nospawn`
        // exclusion since "answered" isn't "nospawn").
        let (_repo, repo_path, root) = scratch_repo(true);
        unit_wt(&repo_path, &root, "answered");

        let events = requested_with("answered", |id| SpawnResult::ok(id, "done"));
        let (removed, lines) = sweep_logged(&repo_path, &root, &events);
        assert_eq!(removed, 1);
        assert!(
            lines.iter().any(|l| l.contains("removing")
                && l.contains("answered")
                && l.contains("terminal")),
            "the removed decision must be attributable from the log: {lines:?}"
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
        let (_repo, repo_path, root) = scratch_repo(true);
        done_and_live_units(&repo_path, &root);
        let done_cache = format!("{root}/{UNIT_CACHE_PREFIX}done");
        populate(&done_cache, &["incremental"]);
        let live_cache = format!("{root}/{UNIT_CACHE_PREFIX}live");
        populate(&live_cache, &[]);

        assert_eq!(
            sweep(&repo_path, &root, &[]),
            1,
            "exactly the terminal unit worktree is swept"
        );
        assert!(
            !exists(&done_cache),
            "the swept unit's per-unit build cache must be removed alongside its worktree"
        );
        assert!(
            exists(&live_cache),
            "an in-flight unit's build cache must be left untouched"
        );
    }

    #[test]
    fn sweep_terminal_reaps_a_process_rooted_in_a_terminal_worktree_before_removing_it() {
        // spec 79 inventory item 1: `sweep_terminal` (crash recovery) removed a terminal
        // worktree via a bare `git worktree remove --force` with no reap of its own - a build
        // or tool a killed step process left running inside it outlived the removed dir. Mirrors
        // `remove_reaps_a_process_rooted_inside_the_worktree_and_spares_one_outside`'s fixture
        // shape (SIGTERM-ignoring, so only the SIGKILL escalation ends it) but drives it through
        // `sweep_terminal` instead of `Worktree::remove`.
        let repo = init_repo();
        let repo_path = repo.path().to_str().unwrap().to_string();
        run_git(&repo_path, &["checkout", "-b", "rigger-run"]).unwrap();
        let root = scratch_root(&repo_path, "", None);

        let done_dir = format!("{root}/{UNIT_WORKTREE_PREFIX}sweepreap");
        Worktree::create(&repo_path, &done_dir, "rigger/u/sweepreap", "").unwrap();
        let done_path = std::path::Path::new(&done_dir).to_path_buf();

        assert_teardown_reaps_what_is_rooted_inside(
            &done_path,
            None,
            || {
                let removed = sweep_terminal(
                    &repo_path,
                    &root,
                    "rigger-run",
                    &std::collections::HashSet::new(),
                    &std::collections::HashSet::new(),
                    &[],
                )
                .unwrap();
                assert_eq!(removed, 1, "the terminal worktree is swept");
            },
            "sweep_terminal",
        );
        assert!(
            !done_path.exists(),
            "the terminal worktree is still removed once its rooted process is reaped"
        );
    }

    /// `Worktree::remove` on the unit worktree `slug` must reclaim its populated
    /// `{prefix}{slug}` sibling WITH the worktree, while removing the review worktree
    /// `rigger-review-{panel}` - which owns no such sibling - must leave an unrelated
    /// `{prefix}unrelated` dir under the same scratch root alone.
    fn assert_remove_reclaims_the_unit_sibling(prefix: &str, slug: &str, file: &str, panel: &str) {
        let (_repo, repo_path, root) = scratch_repo(false);
        let (unit_dir, unit) = unit_wt(&repo_path, &root, slug);
        let sibling = format!("{root}/{prefix}{slug}");
        populate(&sibling, &[file]);
        let (_, review) = wt_at(
            &repo_path,
            &root,
            &format!("rigger-review-{panel}"),
            &format!("rigger/rev/{panel}"),
        );
        let bystander = format!("{root}/{prefix}unrelated");
        populate(&bystander, &[]);

        unit.remove().unwrap();
        assert!(
            !exists(&unit_dir),
            "the unit worktree is gone after remove()"
        );
        assert!(
            !exists(&sibling),
            "removing the unit worktree must reclaim its sibling {prefix}{slug}, leaked at {sibling}"
        );

        review.remove().unwrap();
        assert!(
            exists(&bystander),
            "removing a review worktree (which owns no {prefix} sibling) must not touch an \
             unrelated {prefix} dir"
        );
    }

    /// `Worktree::remove` on the worktree `name` (on `branch`) must reclaim the store-fence
    /// sibling a fenced courier left populated - a live sqlite store with its WAL sibling -
    /// derived one suffix (`gate::STORE_FENCE_SUFFIX`) past the unit's `cargo-target-<slug>`
    /// cache for a unit worktree (`cache_slug`), or past the worktree dir itself otherwise.
    fn assert_remove_reclaims_the_store_fence(name: &str, branch: &str, cache_slug: Option<&str>) {
        let (_repo, repo_path, root) = scratch_repo(false);
        let (dir, wt) = wt_at(&repo_path, &root, name, branch);
        let base = match cache_slug {
            Some(slug) => {
                let cache = format!("{root}/{UNIT_CACHE_PREFIX}{slug}");
                populate(&cache, &[]);
                cache
            }
            None => dir,
        };
        let fence_dir = format!("{base}{}", crate::gate::STORE_FENCE_SUFFIX);
        populate(&fence_dir, &["events.db", "events.db-wal"]);

        wt.remove().unwrap();

        assert!(
            !exists(&fence_dir),
            "removing the worktree must reclaim its store-fence sibling too, leaked at {fence_dir}"
        );
    }

    crate::test_cases! {
        /// Gap 19 DOMINANT graceful path: `Worktree::remove` is what the conductor's
        /// `run_stage` calls to tear a unit's worktree down at stage-end (on integrate / park
        /// / err). It must reclaim the unit's sibling per-unit build cache
        /// (`cargo-target-<slug>`, a plain dir git never tracks) WITH the worktree, or every
        /// gracefully-terminated unit leaks a multi-gigabyte cache.
        worktree_remove_reclaims_the_sibling_per_unit_cache:
            assert_remove_reclaims_the_unit_sibling(
                UNIT_CACHE_PREFIX,
                "graceful",
                "built.rlib",
                "panel-0",
            );
        /// Spec 91, THE GATE ENVIRONMENT: the `checkin` stage's `mutation` gate populates a
        /// THIRD per-unit scratch sibling - `cargo-mutants-<slug>` - alongside the build
        /// cache. It must be reclaimed on the SAME dominant graceful path, or every
        /// gracefully-terminated unit leaks its cargo-mutants build debris exactly as an
        /// un-reclaimed cache would.
        worktree_remove_also_reclaims_the_sibling_mutants_root:
            assert_remove_reclaims_the_unit_sibling(
                UNIT_MUTANTS_PREFIX,
                "mutated",
                "outcomes.json",
                "panel-1",
            );
        /// Ground (b) of the u3 reject (adv-u3-fence-dir-leaks-forever-uncleaned): the gate
        /// store fence (spec 70 criterion 3) creates a SECOND per-unit scratch sibling next to
        /// the `cargo-target-<slug>` cache - `cargo-target-<slug>-store-fence`, a live sqlite
        /// events.db a fenced courier subprocess opened during this unit's own test gate
        /// (gate::ExecRunner::run derives its name from target_dir, main.rs's
        /// require_store_dir creates it). It must be reclaimed by the SAME authority, on the
        /// SAME dominant graceful path `Worktree::remove` already reclaims the cache sibling on.
        worktree_remove_also_reclaims_the_store_fence_sibling:
            assert_remove_reclaims_the_store_fence(
                &format!("{UNIT_WORKTREE_PREFIX}fenced"),
                "rigger/u/fenced",
                Some("fenced"),
            );
        /// Spec 70 criterion 3, widened (u4 round 2 fix for
        /// adv-u3c70-reclaim-shares-the-same-exclusion-fix-fence-alone-leaks): every
        /// standalone review stage's EXHAUSTIVE gate pass leaves a live sqlite events.db (plus
        /// WAL/SHM) sibling of the review worktree, so `Worktree::remove` - which runs for a
        /// review worktree too - must reclaim this kind's fence sibling as well.
        worktree_remove_also_reclaims_a_review_worktrees_store_fence_sibling:
            assert_remove_reclaims_the_store_fence(
                "rigger-review-fanout-stage-0",
                "rigger/review/fanout-0",
                None,
            );
    }

    /// Each `(dir, expected)` of `cases` maps through the sibling derivation `derive`.
    fn assert_sibling_derivation(
        derive: fn(&str) -> Option<String>,
        cases: &[(&str, Option<&str>)],
    ) {
        for (dir, expected) in cases {
            assert_eq!(derive(dir), expected.map(str::to_string), "{dir}");
        }
    }

    crate::test_cases! {
        /// Spec 70 criterion 3, widened (u4 round 2 fix for
        /// adv-u3c70-store-fence-half-wired-review-worktree-call-site-unfenced): the dir-driven
        /// derivation authority for a review worktree's fence sibling, parallel to
        /// `unit_cache_sibling`'s cache derivation for a unit worktree. A `rigger-wt-*` unit
        /// worktree - already fenced via its non-empty target_dir - and the empty
        /// worktree-less path own no fence sibling HERE (they map to None), so nothing
        /// double-fences or tries to reclaim a sibling this function never derived.
        review_fence_sibling_maps_a_review_worktree_to_its_fence_sibling_and_ignores_the_rest:
            assert_sibling_derivation(
                review_fence_sibling,
                &[
                    (
                        "/scratch/rigger-review-panel-0",
                        Some("/scratch/rigger-review-panel-0-store-fence"),
                    ),
                    ("/scratch/rigger-wt-unit-7", None),
                    ("", None),
                ],
            );
        /// The single derivation authority (Gap 19): a `rigger-wt-<slug>` unit worktree maps to
        /// its `cargo-target-<slug>` sibling under the SAME parent; anything that is not a unit
        /// worktree - a `rigger-review-*` review worktree, the shared `cargo-target` dir, or the
        /// empty worktree-less path - owns no per-unit cache and maps to None (so its gate
        /// inherits the shared target and nothing tries to reclaim a cache it never had).
        unit_cache_sibling_maps_a_unit_worktree_to_its_cache_and_ignores_the_rest:
            assert_sibling_derivation(
                unit_cache_sibling,
                &[
                    ("/scratch/rigger-wt-unit-7", Some("/scratch/cargo-target-unit-7")),
                    ("/scratch/rigger-review-panel-0", None),
                    ("/scratch/cargo-target", None),
                    ("", None),
                ],
            );
    }

    // Periphery layer (SDET), spec 38 criterion 1: direct API/contract tests for the ONE
    // new public function this unit adds, `reclaim_worktree_on_branch` (the branch-keyed
    // half of the resume-path teardown `gc_integrated_branches` drives). The run()-level
    // integration test `branch_gc_removes_a_lingering_worktree_before_reclaiming_the_branch_on_resume`
    // drives it only TRANSITIVELY and seeds NO cargo-target sibling, so it cannot pin the
    // cache-sibling reclaim nor the no-op / stale-registration boundaries the API promises.
    // These three tests exercise the function AT ITS OWN EDGES.

    /// A unit worktree `rigger-wt-{slug}` on `rigger/u/{slug}` in its own temp parent, holding
    /// one committed file of prior window work; returns the parent, the dir and the branch.
    fn committed_unit_wt(repo_path: &str, slug: &str) -> (tempfile::TempDir, String, String) {
        let parent = tempfile::tempdir().unwrap();
        let branch = format!("rigger/u/{slug}");
        let (wt_dir, wt) = wt_at(
            repo_path,
            parent.path().to_str().unwrap(),
            &format!("rigger-wt-{slug}"),
            &branch,
        );
        std::fs::write(
            std::path::Path::new(&wt_dir).join("work.rs"),
            "fn work() {}\n",
        )
        .unwrap();
        wt.commit("rigger: prior window work").unwrap();
        (parent, wt_dir, branch)
    }

    /// Precondition: `wt_dir` is still registered on `branch` and holds it, so a bare
    /// `branch -D` refuses - the exact arm a reclaim must clear first.
    fn assert_branch_held_by(repo_path: &str, branch: &str, wt_dir: &str) {
        assert_eq!(
            registered_worktree_for(repo_path, branch).as_deref(),
            Some(wt_dir),
            "precondition: the worktree registration lingers on the branch"
        );
        assert!(
            Worktree::delete_branch(repo_path, branch).is_err(),
            "precondition: git refuses to delete a branch a worktree registration holds"
        );
    }

    /// `reclaim_worktree_on_branch` deregisters whatever held `branch`, leaving it deletable.
    fn assert_reclaim_frees_the_branch(repo_path: &str, branch: &str) {
        reclaim_worktree_on_branch(repo_path, branch, "").unwrap();
        assert_eq!(
            registered_worktree_for(repo_path, branch),
            None,
            "the lingering registration is gone so it no longer holds the branch"
        );
        assert!(
            Worktree::delete_branch(repo_path, branch).is_ok(),
            "with the registration gone the branch is finally deletable - the point of the \
             ordered teardown"
        );
    }

    #[test]
    fn reclaim_worktree_on_branch_deregisters_the_lingering_worktree_reclaims_its_cache_and_frees_the_branch(
    ) {
        // Happy path: a step process killed between its UnitIntegrated emit and
        // Worktree::remove leaves a worktree STILL registered on the integrated unit's
        // branch WITH its multi-gigabyte `cargo-target-<slug>` sibling on disk. The reclaim
        // must (a) deregister the worktree, (b) tear the dir down, (c) reclaim the sibling
        // cache, and (d) leave the branch DELETABLE - git refuses `branch -D` while a
        // worktree holds the branch, so a reclaim that skipped the teardown would strand it.
        let repo = init_repo();
        let repo_path = repo.path().to_str().unwrap().to_string();
        let (_parent, wt_dir, branch) = committed_unit_wt(&repo_path, "lingered");
        let cache = unit_cache_sibling(&wt_dir).expect("a unit worktree owns a cache sibling");
        populate(&cache, &["built.rlib"]);
        assert_branch_held_by(&repo_path, &branch, &wt_dir);

        assert_reclaim_frees_the_branch(&repo_path, &branch);
        assert!(
            !exists(&wt_dir),
            "the lingering worktree dir is torn down off disk"
        );
        assert!(
            !exists(&cache),
            "the sibling per-unit build cache is reclaimed alongside the worktree, leaked at {cache}"
        );
    }

    #[test]
    fn reclaim_worktree_on_branch_reaps_a_process_rooted_in_the_lingering_worktree_before_removing_it(
    ) {
        // spec 79 inventory item: `reclaim_worktree_on_branch`'s own doc comment claimed "the
        // owning process is already dead on this path, so no process reap is needed" - the spec
        // Goal names this claim WRONG. It tears down the lingering worktree through
        // `clear_worktree_dir`, which (like every other inventoried removal site) must reap
        // whatever is rooted inside first: the step process that abandoned this worktree may
        // have LEFT a build or tool still running behind it, so "the owning process is dead"
        // does not mean nothing is rooted in the dir. Fixing `clear_worktree_dir` transitively
        // covers this call site.
        let repo = init_repo();
        let repo_path = repo.path().to_str().unwrap().to_string();
        let branch = "rigger/u/lingered-reap";

        let parent = tempfile::tempdir().unwrap();
        let parent_path = parent.path().canonicalize().unwrap();
        let parent_str = parent_path.to_str().unwrap().to_string();
        let wt_dir = parent_path
            .join("rigger-wt-lingered-reap")
            .to_str()
            .unwrap()
            .to_string();
        let wt = Worktree::create(&repo_path, &wt_dir, branch, &parent_str).unwrap();
        std::fs::write(
            std::path::Path::new(&wt_dir).join("work.rs"),
            "fn work() {}\n",
        )
        .unwrap();
        wt.commit("rigger: prior window work").unwrap();
        let wt_path = std::path::Path::new(&wt_dir).to_path_buf();

        assert_teardown_reaps_what_is_rooted_inside(
            &wt_path,
            None,
            || reclaim_worktree_on_branch(&repo_path, branch, &parent_str).unwrap(),
            "reclaim_worktree_on_branch",
        );
        assert!(
            !wt_path.exists(),
            "the lingering worktree is still torn down once its rooted process is reaped"
        );
    }

    #[test]
    fn reclaim_worktree_on_branch_is_a_no_op_that_spares_an_unrelated_in_flight_worktree() {
        // The graceful no-op path AND branch-keyed matching: the reclaim is called once per
        // integrated unit, so it must (a) do NOTHING but succeed when the target branch has
        // no lingering worktree (the dominant path, where Worktree::remove already ran), and
        // (b) NEVER tear down an UNRELATED in-flight unit's still-registered worktree or its
        // cache. A reclaim keyed on anything but the branch would strand a live unit.
        let repo = init_repo();
        let repo_path = repo.path().to_str().unwrap().to_string();

        // The target: an integrated unit's branch carrying committed work whose worktree was
        // already removed gracefully - only the branch remains, no worktree lingers.
        let done_branch = "rigger/u/done";
        {
            let seed = tempfile::tempdir().unwrap();
            let d = seed
                .path()
                .join("rigger-wt-done")
                .to_str()
                .unwrap()
                .to_string();
            let wt = Worktree::create(&repo_path, &d, done_branch, "").unwrap();
            std::fs::write(std::path::Path::new(&d).join("done.rs"), "fn done() {}\n").unwrap();
            wt.commit("rigger: prior window work").unwrap();
            wt.remove().unwrap();
        }
        assert!(
            branch_exists(&repo_path, done_branch),
            "precondition: the target branch exists with no worktree on it"
        );
        assert_eq!(
            registered_worktree_for(&repo_path, done_branch),
            None,
            "precondition: no worktree lingers on the target branch"
        );

        // An UNRELATED in-flight unit: its worktree is still registered on ITS OWN branch,
        // with a populated per-unit cache, exactly as a live concurrent unit leaves it.
        let inflight = tempfile::tempdir().unwrap();
        let other_dir = inflight
            .path()
            .join("rigger-wt-inflight")
            .to_str()
            .unwrap()
            .to_string();
        let other = Worktree::create(&repo_path, &other_dir, "rigger/u/inflight", "").unwrap();
        std::fs::write(
            std::path::Path::new(&other_dir).join("wip.rs"),
            "fn wip() {}\n",
        )
        .unwrap();
        other.commit("rigger: in-flight").unwrap();
        let other_cache = unit_cache_sibling(&other_dir).unwrap();
        std::fs::create_dir_all(&other_cache).unwrap();

        // Reclaiming the target (no worktree on it) is a graceful no-op, not an error.
        reclaim_worktree_on_branch(&repo_path, done_branch, "").unwrap();

        assert!(
            branch_exists(&repo_path, done_branch),
            "the no-op reclaim leaves the worktree-less target branch untouched"
        );
        // The unrelated in-flight unit's worktree, registration, and cache are all intact.
        assert!(
            std::path::Path::new(&other_dir).exists(),
            "an unrelated in-flight worktree dir must not be torn down"
        );
        assert_eq!(
            registered_worktree_for(&repo_path, "rigger/u/inflight").as_deref(),
            Some(other_dir.as_str()),
            "an unrelated unit's worktree registration must be left in place"
        );
        assert!(
            std::path::Path::new(&other_cache).exists(),
            "an unrelated in-flight unit's per-unit cache must be left intact"
        );
    }

    #[test]
    fn reclaim_worktree_on_branch_prunes_a_stale_registration_whose_dir_was_deleted_and_frees_the_branch(
    ) {
        // The residue-that-no-longer-occupies-disk edge the doc-comment calls out: a temp
        // cleaner (or a crash) deletes the worktree DIR but git's registration for it
        // lingers, so git STILL treats the branch as checked out and refuses `branch -D`.
        // The reclaim must prune that dangling registration (clear_worktree_dir's
        // `git worktree prune`) so the branch stops being held - a reclaim that only removed
        // the dir off disk, without pruning, would leave the branch permanently un-deletable.
        let repo = init_repo();
        let repo_path = repo.path().to_str().unwrap().to_string();
        let (_parent, wt_dir, branch) = committed_unit_wt(&repo_path, "vanished");

        // The dir vanishes WITHOUT deregistration; the registration dangles on.
        std::fs::remove_dir_all(&wt_dir).unwrap();
        assert_branch_held_by(&repo_path, &branch, &wt_dir);

        assert_reclaim_frees_the_branch(&repo_path, &branch);
    }

    #[test]
    fn unit_mutants_sibling_maps_a_unit_worktree_to_its_mutants_root_and_ignores_the_rest() {
        // Spec 91, THE GATE ENVIRONMENT: the identical derivation shape as
        // `unit_cache_sibling` above, just a different sibling name - a `rigger-wt-<slug>`
        // unit worktree maps to its `cargo-mutants-<slug>` sibling under the SAME parent;
        // anything that is not a unit worktree owns no such root and maps to None.
        assert_eq!(
            unit_sibling("/scratch/rigger-wt-unit-7", UNIT_MUTANTS_PREFIX),
            Some("/scratch/cargo-mutants-unit-7".to_string())
        );
        assert_eq!(
            unit_sibling("/scratch/rigger-review-panel-0", UNIT_MUTANTS_PREFIX),
            None
        );
        assert_eq!(
            unit_sibling("/scratch/cargo-mutants", UNIT_MUTANTS_PREFIX),
            None
        );
        assert_eq!(unit_sibling("", UNIT_MUTANTS_PREFIX), None);
    }

    #[test]
    fn shared_build_cache_guard_path_is_a_sibling_lock_file_of_the_cache_dir() {
        // spec 77 criterion 5 (BOUNDED SHARED CACHE): the guard lives BESIDE the cache
        // (never inside it), named from the SAME `SHARED_BUILD_CACHE_NAME` constant every
        // reader of this cache uses - so `rigger reset --build-cache`'s exclusive attempt
        // and every gate build's shared hold can never disagree about which file guards
        // which cache, and the rename this reclaim performs on the cache itself can never
        // touch (or invalidate) the guard.
        assert_eq!(
            shared_build_cache_guard_path("/scratch"),
            "/scratch/cargo-target.lock"
        );
        assert!(shared_build_cache_guard_path("/scratch")
            .ends_with(&format!("{SHARED_BUILD_CACHE_NAME}.lock")));
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
        let wt1 = Worktree::create(&repo_path, dir1.to_str().unwrap(), branch, "").unwrap();
        std::fs::write(dir1.join("inflight.txt"), "wave-1 work\n").unwrap();
        wt1.commit("rigger: in-flight work").unwrap();

        // Process 2: same branch, different dir. Must ADOPT dir1, not fail.
        let dir2 = std::env::temp_dir().join(format!("rigger-wt-{}", uuid::Uuid::new_v4()));
        let wt2 = Worktree::create(&repo_path, dir2.to_str().unwrap(), branch, "").unwrap();
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
        let wt3 = Worktree::create(&repo_path, dir3.to_str().unwrap(), branch, "").unwrap();
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
        let wt1 = Worktree::create(&repo_path, &dir, branch, "").unwrap();
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
        let wt2 = Worktree::create(&repo_path, &dir, branch, "")
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
        let wt1 = Worktree::create(&repo_path, &dir, branch, "").unwrap();
        std::fs::write(std::path::Path::new(&dir).join("work.txt"), "det\n").unwrap();
        wt1.commit("rigger: process 1 work").unwrap();

        // Process 2: SAME deterministic dir + branch. It must adopt the existing dir (a
        // path lookup), returning that exact dir with the committed work present.
        let wt2 = Worktree::create(&repo_path, &dir, branch, "").unwrap();
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
    fn create_branch_at_points_a_new_ref_at_a_prior_branchs_tip_without_touching_it() {
        // Spec 88, ADOPTION KEYS ON THE CRITERION: the conductor seeds a FRESH unit's own
        // branch as a NEW ref at a prior (differently-named) unit's tip - never a rename -
        // so the prior branch name stays resolvable, and `Worktree::create`'s existing
        // adopt-by-path-lookup machinery then reuses the new ref exactly like any other
        // unit branch that already carries committed work.
        let repo = init_repo();
        let repo_path = repo.path().to_str().unwrap().to_string();
        let prior_branch = "rigger/u/prior-unit";
        let dir = std::env::temp_dir().join(format!("rigger-wt-{}", uuid::Uuid::new_v4()));
        let prior_wt =
            Worktree::create(&repo_path, dir.to_str().unwrap(), prior_branch, "").unwrap();
        std::fs::write(dir.join("checkpoint.txt"), "prior work\n").unwrap();
        prior_wt.commit("rigger: prior unit checkpoint").unwrap();
        let prior_tip = run_git(&repo_path, &["rev-parse", prior_branch])
            .unwrap()
            .trim()
            .to_string();
        prior_wt.remove().unwrap();

        let new_branch = "rigger/u/new-unit";
        assert!(
            !branch_exists(&repo_path, new_branch),
            "precondition: the new unit's branch does not exist yet"
        );

        let tip = Worktree::create_branch_at(&repo_path, new_branch, prior_branch).unwrap();
        assert_eq!(
            tip, prior_tip,
            "the new ref's tip is the prior branch's tip at the moment of creation"
        );
        assert!(
            branch_exists(&repo_path, new_branch),
            "the new branch ref now exists"
        );
        assert!(
            branch_exists(&repo_path, prior_branch),
            "the prior branch name stays resolvable - a new ref, never a rename"
        );

        // `Worktree::create` then adopts the new ref exactly as any branch with prior work.
        let dir2 = std::env::temp_dir().join(format!("rigger-wt-{}", uuid::Uuid::new_v4()));
        let adopted = Worktree::create(&repo_path, dir2.to_str().unwrap(), new_branch, "").unwrap();
        assert!(
            dir2.join("checkpoint.txt").exists(),
            "the adopted worktree checks out the prior unit's committed work"
        );
        adopted.remove().unwrap();
    }

    #[test]
    fn create_branch_at_refuses_to_clobber_an_already_existing_branch() {
        // The caller (the conductor's adoption check) is responsible for guarding this
        // with `branch_exists` first - this test pins that `create_branch_at` itself never
        // silently re-points an existing ref (which would discard whatever that branch
        // already carries as a durable checkpoint).
        let repo = init_repo();
        let repo_path = repo.path().to_str().unwrap().to_string();
        run_git(&repo_path, &["branch", "rigger/u/prior"]).unwrap();
        run_git(&repo_path, &["branch", "rigger/u/existing"]).unwrap();

        let err = Worktree::create_branch_at(&repo_path, "rigger/u/existing", "rigger/u/prior")
            .expect_err("create_branch_at must fail rather than clobber an existing branch");
        assert!(
            !err.to_string().is_empty(),
            "the failure surfaces git's own refusal"
        );
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
        let wt = Worktree::create(&repo_path, &dir, branch, "").unwrap();
        assert!(
            worktree_on_branch(&dir, branch),
            "the dir that IS this branch's worktree matches by its own HEAD"
        );

        // A worktree checked out on a DIFFERENT branch is not matched for `branch`.
        let other_dir = format!("{root}/rigger-wt-other");
        let other = Worktree::create(&repo_path, &other_dir, "rigger/u/other", "").unwrap();
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
        let stale = Worktree::create(&repo_path, &dir, branch, "").unwrap();
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
        Worktree::discard(&repo_path, &dir, branch, "").unwrap();
        assert!(
            !branch_exists(&repo_path, branch),
            "discard deletes the throwaway review branch"
        );
        let fresh = Worktree::create(&repo_path, &dir, branch, "").unwrap();
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
    fn discard_also_reclaims_the_review_worktrees_store_fence_sibling() {
        // adv-u4c70r2-discard-path-leaks-review-fence-sibling (u4 round 2 fix): `discard`
        // is the FOURTH teardown path a review worktree goes through -
        // `review_only_worktree` calls it unconditionally before `create()` on every
        // standalone-review-stage attempt, the crash-resume path this function's own doc
        // comment describes ("a resumed review step recomputes the same path and reclaims
        // it instead of leaking a fresh worktree each process"). `remove`, `sweep_terminal`,
        // and `reclaim_worktree_on_branch` already reclaim a fence sibling via
        // `reclaim_cache_sibling`; `discard` did not, so a process that crashed after a
        // fenced gate wrote a real events.db into `<dir>-store-fence` left it orphaned with
        // no teardown path guaranteed to ever reclaim it - populated here exactly as a real
        // fenced courier would leave it (a live sqlite store with a WAL sibling).
        let repo = init_repo();
        let repo_path = repo.path().to_str().unwrap().to_string();
        let root = scratch_root(&repo_path, "", None);
        let branch = "rigger/review/discard-fence-0";
        let dir = format!("{root}/rigger-review-discard-fence-0");

        let stale = Worktree::create(&repo_path, &dir, branch, "").unwrap();
        drop(stale); // the Rust struct is gone but the worktree registration + dir survive.

        let fence_dir = format!("{dir}{}", crate::gate::STORE_FENCE_SUFFIX);
        std::fs::create_dir_all(&fence_dir).unwrap();
        std::fs::write(std::path::Path::new(&fence_dir).join("events.db"), "x").unwrap();
        std::fs::write(std::path::Path::new(&fence_dir).join("events.db-wal"), "x").unwrap();

        Worktree::discard(&repo_path, &dir, branch, "").unwrap();

        assert!(
            !std::path::Path::new(&fence_dir).exists(),
            "discard must reclaim the review worktree's store-fence sibling too, leaked at {fence_dir}"
        );
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
        // The pure git-adapter classification for a repo whose base ref (e.g. the default
        // origin/main) does NOT resolve - no remote, master-default, or pre-fetch - but whose
        // HEAD IS a real commit: it must NOT no-op (which would leave HEAD on the operator's
        // branch) but create the run branch off the current HEAD, check it out, and report
        // CreatedFromHead. Because HEAD is a real commit, the run branch descends from a
        // reachable base (the operator's own branch) a PR still applies to, so the run-entry
        // POLICY (the spec 38 loop-readiness gate `refuse_when_base_unreachable` in main.rs)
        // lets this proceed and only advises the divergence. That gate refuses ONLY the
        // genuinely baseless case (this same fallback but with an UNBORN HEAD - nothing to
        // branch from), which is a separate test on the CLI path.
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
        let base_tip = run_git(&p, &["rev-parse", &base])
            .unwrap()
            .trim()
            .to_string();

        // Anchor the run branch on the release target.
        let setup = Worktree::ensure_run_branch(&p, "rigger-run", &base).unwrap();
        assert_eq!(setup, RunBranchSetup::CreatedFromBase);

        // Two units integrate onto the run branch (empty commits stand in for merged work).
        run_git(
            &p,
            &["commit", "--allow-empty", "-q", "-m", "integrate unit A"],
        )
        .unwrap();
        let a = run_git(&p, &["rev-parse", "HEAD"])
            .unwrap()
            .trim()
            .to_string();
        run_git(
            &p,
            &["commit", "--allow-empty", "-q", "-m", "integrate unit B"],
        )
        .unwrap();
        let b = run_git(&p, &["rev-parse", "HEAD"])
            .unwrap()
            .trim()
            .to_string();

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
            run_git(
                &p,
                &["merge-base", "--is-ancestor", &base_tip, "rigger-run"]
            )
            .is_ok(),
            "the release target must be an ancestor of the run branch (an applicable PR diff)"
        );
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
        let wt = Worktree::create(
            &repo_path,
            wt_dir.to_str().unwrap(),
            "rigger/u/reaptest",
            "",
        )
        .unwrap();

        assert_teardown_reaps_what_is_rooted_inside(
            &wt_dir,
            Some(repo.path()),
            || wt.remove().unwrap(),
            "Worktree::remove",
        );
        assert!(
            !wt_dir.exists(),
            "the worktree dir is removed after its rooted processes are reaped"
        );
    }

    /// Register the unit worktree `slug` under `root` and return its git admin entry
    /// (`<git-common-dir>/worktrees/rigger-wt-<slug>`).
    fn admin_entry(repo_path: &str, root: &str, slug: &str) -> std::path::PathBuf {
        unit_wt(repo_path, root, slug);
        std::path::Path::new(repo_path)
            .join(".git")
            .join("worktrees")
            .join(format!("{UNIT_WORKTREE_PREFIX}{slug}"))
    }

    /// A repo holding a HEALTHY registered unit worktree whose admin entry the healing must
    /// leave completely alone, next to a `doomed` one each test corrupts.
    struct HealFixture {
        _repo: tempfile::TempDir,
        repo_path: String,
        root: String,
        healthy_dir: String,
        healthy_admin: std::path::PathBuf,
        doomed_admin: std::path::PathBuf,
    }

    impl HealFixture {
        fn new() -> Self {
            let (repo, repo_path, root) = scratch_repo(false);
            let healthy_admin = admin_entry(&repo_path, &root, "healthy");
            let doomed_admin = admin_entry(&repo_path, &root, "doomed");
            HealFixture {
                _repo: repo,
                healthy_dir: format!("{root}/{UNIT_WORKTREE_PREFIX}healthy"),
                repo_path,
                root,
                healthy_admin,
                doomed_admin,
            }
        }

        /// Backdate the corrupted doomed entry past the heal grace period (spec 103 criterion
        /// 4: a freshly-corrupted entry this young survives the heal on purpose, since it
        /// could be a live in-flight add), then `create` on a fresh branch must prune ONLY
        /// that entry and SUCCEED, leaving the healthy worktree registered and untouched.
        fn assert_create_heals(&self, corruption: &str) {
            backdate(&self.doomed_admin, 120);
            let new_dir = format!("{}/{UNIT_WORKTREE_PREFIX}fresh", self.root);
            let created = Worktree::create(&self.repo_path, &new_dir, "rigger/u/fresh", "");
            assert!(
                created.is_ok(),
                "create must self-heal {corruption} before adding: {:?}",
                created.err()
            );
            assert!(
                std::path::Path::new(&new_dir).join(".git").exists(),
                "the freshly added worktree is a real checkout"
            );
            assert!(
                !self.doomed_admin.exists(),
                "the admin entry with {corruption} is pruned by the healing"
            );
            assert!(
                self.healthy_admin.is_dir(),
                "a healthy registered worktree is NEVER pruned by the healing"
            );
            let list = run_git(&self.repo_path, &["worktree", "list", "--porcelain"]).unwrap();
            assert!(
                list.contains(&self.healthy_dir),
                "the healthy worktree stays registered after healing"
            );
        }
    }

    #[test]
    fn create_self_heals_a_corrupt_worktree_admin_entry_and_spares_healthy_ones() {
        // Spec 51 criterion 4: a lifecycle killed mid-`git worktree remove` can leave a
        // half-removed admin entry under `<git-common-dir>/worktrees/<name>/` whose
        // `commondir` marker is truncated to ZERO length. git reads EVERY admin entry
        // up-front on any worktree command, so one such entry makes EVERY later
        // `git worktree add` hard-fail (`failed to read .git/worktrees/<name>/commondir`,
        // exit 128) - and `git worktree prune` does NOT clear it (it hits the same read) -
        // permanently wedging the run until an operator deletes the entry by hand. `create`
        // must detect and prune ONLY the provably-corrupt entry before adding, so the next
        // add succeeds, while leaving a HEALTHY registered worktree completely untouched.
        let fx = HealFixture::new();
        assert!(
            fx.healthy_admin.is_dir(),
            "precondition: the healthy worktree has a registered admin entry"
        );

        // A CORRUPT admin entry: truncate its `commondir` to zero length - the exact residue
        // a SIGKILL mid `git worktree remove` leaves.
        std::fs::write(fx.doomed_admin.join("commondir"), b"").unwrap();
        assert_eq!(
            std::fs::metadata(fx.doomed_admin.join("commondir"))
                .unwrap()
                .len(),
            0,
            "precondition: the doomed entry's commondir is zero-length"
        );
        // The corruption blocks git entirely: even enumerating worktrees fails now, which
        // is why git's own prune cannot recover and self-healing on disk is required.
        assert!(
            run_git(
                &fx.repo_path,
                &[
                    "worktree",
                    "add",
                    &format!("{}/probe", fx.root),
                    "-b",
                    "probe"
                ]
            )
            .is_err(),
            "precondition: the corrupt entry makes a bare `git worktree add` hard-fail"
        );

        fx.assert_create_heals("a zero-length commondir marker");
    }

    #[test]
    fn create_heals_a_zero_length_gitdir_marker_the_commondir_case_leaves_untested() {
        // PERIPHERY contract test for the PUBLIC `Worktree::create` self-heal boundary
        // (spec 51 criterion 4). The implementer's unit test proves the healing for ONE
        // operand of `worktree_admin_is_corrupt` - a zero-length `commondir`. That helper
        // deems an entry corrupt when EITHER marker (`commondir` OR `gitdir`) is missing or
        // zero-length, so the `gitdir` operand is a DISTINCT arm of `create`'s documented
        // contract that no unit test reaches: had the healing checked `commondir` alone, a
        // `gitdir`-truncated entry would slip through. A `git worktree remove` killed a step
        // earlier can truncate `gitdir` just as readily as `commondir`. Unlike a zero-length
        // `commondir` (which wedges git outright), a zero-length `gitdir` does NOT wedge a
        // bare `git worktree add`, and a bare add never prunes the stale entry - so the entry
        // surviving-vs-pruned is the observable that pins the `gitdir` arm, and only
        // `create`'s explicit healing prunes it. Proven end-to-end through the public
        // `create`, never by calling the private helper.
        let fx = HealFixture::new();
        std::fs::write(fx.doomed_admin.join("gitdir"), b"").unwrap();
        assert_eq!(
            std::fs::metadata(fx.doomed_admin.join("gitdir"))
                .unwrap()
                .len(),
            0,
            "precondition: the doomed entry's gitdir marker is zero-length"
        );
        assert!(
            fx.doomed_admin.is_dir(),
            "precondition: the doomed admin entry is present before the heal"
        );

        fx.assert_create_heals("a zero-length gitdir marker");
    }

    #[test]
    fn create_heals_a_fully_missing_marker_not_just_a_truncated_one() {
        // PERIPHERY contract test for the PUBLIC `Worktree::create` self-heal boundary
        // (spec 51 criterion 4). The implementer's unit test corrupts a marker by
        // TRUNCATING it to zero length; `worktree_admin_is_corrupt` also treats a marker
        // whose metadata read ERRORS - a fully ABSENT file - as corrupt (the
        // `map_or(true, ..)` arm). A `git worktree remove` killed after it has already
        // unlinked a marker leaves exactly this residue, so the missing-file branch is a
        // DISTINCT arm of `create`'s contract that the truncation case leaves untested: had
        // the healing keyed on `len() == 0` of a readable file alone, a missing marker would
        // slip through. A bare `git worktree add` tolerates a missing `commondir` and never
        // prunes the stale entry, so the entry surviving-vs-pruned pins the missing-file arm
        // and only `create`'s explicit healing removes it.
        let fx = HealFixture::new();
        std::fs::remove_file(fx.doomed_admin.join("commondir")).unwrap();
        assert!(
            std::fs::metadata(fx.doomed_admin.join("commondir")).is_err(),
            "precondition: the doomed entry's commondir marker is fully absent"
        );
        assert!(
            fx.doomed_admin.is_dir(),
            "precondition: the doomed admin entry is present before the heal"
        );

        fx.assert_create_heals("a MISSING marker");
    }

    #[test]
    fn heal_never_prunes_a_locked_admin_entry() {
        // Spec 103 criterion 4: git itself writes an admin entry as mkdir, `locked`,
        // `gitdir`, `HEAD`, `commondir` (NOT atomically), so a scan landing mid-write can
        // observe `locked` present with `commondir` still missing - indistinguishable from
        // the OLD provably-corrupt-and-abandoned shape unless the heal honors the SAME
        // marker git's own `worktree prune` already refuses to touch. Without this guard a
        // batch-mate's in-flight `git worktree add` gets deleted out from under it mid-write
        // (the production signature: `fatal: failed to read .git/worktrees/<name>/
        // commondir`, gap 57, `checkin94-gap57-root-fix-moves-to-spec-103`).
        let (_repo, repo_path, root) = scratch_repo(false);
        let doomed_admin = admin_entry(&repo_path, &root, "inflight");
        // Simulate the mid-write window: `locked` present, `commondir` gone - exactly what
        // a real in-flight `git worktree add` looks like before its own last write, and
        // backdated well past the grace period so ONLY the lock, not the age, saves it.
        std::fs::write(doomed_admin.join("locked"), b"").unwrap();
        std::fs::remove_file(doomed_admin.join("commondir")).unwrap();
        backdate(&doomed_admin, 120);
        assert!(
            doomed_admin.join("locked").exists(),
            "precondition: the entry carries git's own locked marker"
        );

        heal_corrupt_worktree_admin(&repo_path);

        assert!(
            doomed_admin.is_dir(),
            "a locked admin entry must survive the heal even though its commondir marker \
             is missing and it is well past the grace period - git's own worktree prune \
             honors the same marker"
        );
    }

    #[test]
    fn heal_never_prunes_an_admin_entry_younger_than_the_grace_period() {
        // Spec 103 criterion 4: an admin entry with no `locked` marker can still be a live
        // add observed between two of git's non-atomic writes (an unlucky read right after
        // `locked` is removed but before `commondir` lands), so a freshly-touched entry
        // survives even with a missing marker - only an entry that has sat corrupt for a
        // while is provably abandoned.
        let (_repo, repo_path, root) = scratch_repo(false);
        let doomed_admin = admin_entry(&repo_path, &root, "toosoon");
        std::fs::remove_file(doomed_admin.join("commondir")).unwrap();
        // Left exactly as `Worktree::create` just touched it - fresh, well inside the
        // grace period. No `backdate` call: that is the whole point of this scenario.

        heal_corrupt_worktree_admin(&repo_path);

        assert!(
            doomed_admin.is_dir(),
            "an admin entry younger than the grace period must survive the heal even \
             though its commondir marker is missing"
        );
    }

    #[test]
    fn concurrent_worktree_creates_in_one_repository_all_succeed_across_50_rounds() {
        // Spec 103 criterion 4: the ORIGINAL race this whole mechanism exists to close.
        // `run_batch` spawns one real OS thread per concurrent unit in a wave and each
        // calls `Worktree::create` independently against the SAME shared repository - a
        // heal scan on one thread could delete a sibling's in-flight `git worktree add`
        // admin entry mid-write. Drive that EXACT shape directly: two threads, 50 rounds,
        // disjoint branches/dirs, one shared repo, and require every round to succeed on
        // both threads with none tripping the corrupt-admin-read failure.
        let repo = init_repo();
        let repo_path = repo.path().to_str().unwrap().to_string();
        let root = scratch_root(&repo_path, "", None);

        assert_concurrent_creates_succeed(
            &repo_path,
            &format!("{root}/{UNIT_WORKTREE_PREFIX}"),
            50,
            "",
        );
    }

    #[test]
    fn concurrent_ensure_present_on_a_deleted_worktree_never_races_create() {
        // Spec 64 criterion 3, adjudication round 4
        // (adv-u3c3r4-concurrent-lens-ensure-present-races-worktree-create,
        // sdet-u3c3r4-concurrent-lenses-race-ensure-present-on-the-same-worktree, UPHELD):
        // the review tier's lens fan-out (`run_review_agents_concurrently`) runs REAL
        // concurrent OS threads that all share ONE `&Worktree` reference and each calls
        // `ensure_present` independently before its own spawn. `Worktree::create`'s own doc
        // comment above states its mutation path does not support concurrent callers ("two
        // processes that both see the branch absent still race the underlying `git worktree
        // add -b`"), and there was no lock anywhere enforcing that. This drives that EXACT
        // shape directly against the mechanism: N real threads sharing one `Worktree` whose
        // dir was deleted out from under git, all calling `ensure_present` at once. Every
        // call must succeed - none may observe the underlying `git worktree add`/adopt race
        // (a torn admin-dir read, an `already exists`, or any other transient git failure).
        let repo = init_repo();
        let repo_path = repo.path().to_str().unwrap().to_string();
        let root = scratch_root(&repo_path, "", None);
        let dir = format!("{root}/{UNIT_WORKTREE_PREFIX}racer");
        let wt = Worktree::create(&repo_path, &dir, "rigger/u/racer", "").unwrap();

        // Out-of-band deletion: the exact scenario `ensure_present` exists to self-heal -
        // the dir is gone but the branch (the durable checkpoint) still exists.
        std::fs::remove_dir_all(&dir).unwrap();
        assert!(
            !std::path::Path::new(&dir).exists(),
            "premise: the out-of-band deletion must actually remove it, or this test proves \
             nothing"
        );

        // N concurrent callers sharing the SAME `&Worktree`, matching the lens fan-out's own
        // sharing of one `wt: Option<&Worktree>` reference across threads (MAX_CONCURRENCY =
        // 4 in production; over-subscribe here to widen the race window).
        let results: Vec<Result<(), Error>> = std::thread::scope(|s| {
            let handles: Vec<_> = (0..8).map(|_| s.spawn(|| wt.ensure_present())).collect();
            handles.into_iter().map(|h| h.join().unwrap()).collect()
        });

        for (i, r) in results.iter().enumerate() {
            assert!(
                r.is_ok(),
                "every concurrent ensure_present call must succeed - call {i} raced the \
                 underlying git mutation: {r:?}"
            );
        }
        assert!(
            std::path::Path::new(&dir).is_dir(),
            "the worktree must exist after the concurrent re-assert: {dir}"
        );
        assert!(
            worktree_on_branch(&dir, "rigger/u/racer"),
            "the restored worktree must be checked out on its own branch, not left in a \
             half-recreated state"
        );
    }
}
