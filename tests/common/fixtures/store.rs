//! Fixtures over the sqlite-backed event log.

use rigger::eventstore::Event;

/// Every event on `store`'s run stream, oldest first.
pub fn run_log(store: &rigger::eventstore::sqlite::Store) -> Vec<Event> {
    use rigger::eventstore::EventStore;
    store
        .read_stream(
            rigger::conductor::STREAM,
            0,
            rigger::eventstore::Direction::Forward,
        )
        .unwrap()
}

/// Race two `Worktree::create` calls against the ONE repository at `repo_path` for `rounds`
/// rounds - disjoint branches, and dirs named `{dir_prefix}race-a-{round}` /
/// `{dir_prefix}race-b-{round}` - asserting every create on both threads succeeds: a heal scan on
/// one thread must never delete a sibling's in-flight `git worktree add` admin entry. `context`
/// is appended to the race named in the failure message.
pub fn assert_concurrent_creates_succeed(
    repo_path: &str,
    dir_prefix: &str,
    rounds: u32,
    context: &str,
) {
    use rigger::worktree::Worktree;
    for round in 0..rounds {
        let dir_a = format!("{dir_prefix}race-a-{round}");
        let dir_b = format!("{dir_prefix}race-b-{round}");
        let branch_a = format!("rigger/u/race-a-{round}");
        let branch_b = format!("rigger/u/race-b-{round}");

        let (ra, rb) = std::thread::scope(|s| {
            let ha = s.spawn(|| Worktree::create(repo_path, &dir_a, &branch_a, ""));
            let hb = s.spawn(|| Worktree::create(repo_path, &dir_b, &branch_b, ""));
            (ha.join().unwrap(), hb.join().unwrap())
        });

        assert!(
            ra.is_ok(),
            "round {round}: thread A's create must never lose the admin-directory race{context}: \
             {:?}",
            ra.err()
        );
        assert!(
            rb.is_ok(),
            "round {round}: thread B's create must never lose the admin-directory race{context}: \
             {:?}",
            rb.err()
        );
    }
}

/// Every registry entry under `state_home` with its path, decoded through `Instance`'s own
/// (de)serialization - a raw directory read, so a test sees exactly what a courier wrote without
/// depending on `read_live`'s pruning (which mutates the directory as a side effect of reading).
pub fn registry_entries(
    state_home: &std::path::Path,
) -> Vec<(std::path::PathBuf, rigger::registry::Instance)> {
    let dir = rigger::registry::instances_dir(state_home);
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return out;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        if let Ok(body) = std::fs::read(&path) {
            if let Ok(inst) = serde_json::from_slice::<rigger::registry::Instance>(&body) {
                out.push((path, inst));
            }
        }
    }
    out
}

/// A git repo with one commit and an installed, always-refusing `pre-commit` hook, returned with
/// its path - after proving the hook really refuses an ordinary commit in a worktree of the same
/// repo, so a green run over it is proof of a bypass, never proof the hook was toothless.
pub fn repo_with_refusing_hook() -> (tempfile::TempDir, String) {
    let repo = super::temp_git_project_with_commit();
    let repo_path = repo.path().to_str().unwrap().to_string();
    super::install_refusing_hook(&repo_path);

    let wt_path = std::env::temp_dir().join(format!("hook-sanity-{}", uuid::Uuid::new_v4()));
    let wt = rigger::worktree::Worktree::create(
        &repo_path,
        wt_path.to_str().unwrap(),
        "rigger/hook-sanity",
        "",
    )
    .unwrap();
    std::fs::write(wt_path.join("probe.txt"), "x\n").unwrap();
    let err = wt
        .commit("rigger: probe")
        .expect_err("the installed hook must refuse an ordinary commit in a sibling worktree");
    assert!(err.to_string().contains("hook: refusing"), "{err}");
    drop(wt);
    let _ = std::fs::remove_dir_all(&wt_path);
    (repo, repo_path)
}
