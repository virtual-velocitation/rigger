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
    /// Add a worktree at dir (which must not already exist), on a new branch off
    /// the repo's current HEAD.
    pub fn create(repo: &str, dir: &str, branch: &str) -> Result<Self, Error> {
        git(repo, &["worktree", "add", "-b", branch, dir, "HEAD"])?;
        Ok(Worktree {
            dir: dir.to_string(),
            branch: branch.to_string(),
            repo: repo.to_string(),
        })
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

    /// Commit the agent's changes and merge the branch into the base, returning
    /// the commit hash. A read-only stage (no changes) commits nothing and
    /// returns "".
    pub fn integrate(&self, message: &str) -> Result<String, Error> {
        git(&self.dir, &["add", "-A"])?;
        match run_git(&self.dir, &["commit", "-m", message]) {
            Ok(_) => {}
            Err(out) if out.contains("nothing to commit") => return Ok(String::new()),
            Err(out) => return Err(Error(format!("commit: {out}"))),
        }
        let commit = git(&self.dir, &["rev-parse", "HEAD"])?.trim().to_string();
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
