//! Integration tests for the `rigger ground` / `rigger emit` / `rigger peers` CLI
//! subcommands - the surface native-workflow agents (which have Bash, not the MCP
//! tools) use to reach rigger's grounder, event store, and context graph. They run
//! the COMPILED `rigger` binary against a throwaway project so they exercise the
//! same composition path (`Store::open(.rigger/events.db)` namespaced, the
//! `graph.db` projector, `conductor::STREAM`) the `serve` path uses.

use std::path::Path;
use std::process::Command;

/// The compiled `rigger` binary under test (Cargo sets this for integration tests).
fn rigger_bin() -> &'static str {
    env!("CARGO_BIN_EXE_rigger")
}

/// A throwaway project dir that is its own git repo, so `project_identity()` (which
/// scopes the namespaced streams) is stable across the emit and the peers reads.
fn temp_project() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    // `git init` makes project_identity() resolve to the dir's basename
    // deterministically; a non-repo dir would fall back to the current-dir name,
    // which is also fine, but a real repo mirrors how rigger is actually used.
    let _ = Command::new("git")
        .args(["init", "-q"])
        .current_dir(dir.path())
        .status();
    dir
}

/// Seed an initialized `.rigger/events.db` under `root`, standing in for the store a
/// prior `rigger run`/`step` would have created. The store-opening couriers
/// (`emit`/`result`/`peers`) now REFUSE to fabricate a fresh store from the wrong cwd
/// (spec 05), so a round-trip test must first establish one, exactly as a real run does
/// before any courier appends to it. An empty file is a valid empty SQLite database;
/// `Store::open` adds the schema on first open - so this models "the run created the
/// store" without needing a full workflow.
fn seed_store(root: &Path) {
    let rigger = root.join(".rigger");
    std::fs::create_dir_all(&rigger).unwrap();
    std::fs::File::create(rigger.join("events.db")).unwrap();
}

/// A throwaway git project with a real commit, so a base ref like `HEAD` resolves.
/// `temp_project` only `git init`s (unborn HEAD), which is enough for the offline
/// step tests but not for the run-branch-anchoring path that needs a base commit.
fn temp_git_project_with_commit() -> tempfile::TempDir {
    let dir = temp_project();
    let root = dir.path();
    for args in [
        &["config", "user.email", "t@example.com"][..],
        &["config", "user.name", "t"],
        &["commit", "--allow-empty", "-q", "-m", "init"],
    ] {
        let ok = Command::new("git")
            .args(args)
            .current_dir(root)
            .status()
            .expect("git must be runnable")
            .success();
        assert!(ok, "git {args:?} must succeed while seeding the repo");
    }
    dir
}

/// Run a read-only `git <args...>` in `cwd`, returning its trimmed stdout on success
/// (used to assert branch state after a `rigger step --base`), or None on failure.
fn git_out(cwd: &Path, args: &[&str]) -> Option<String> {
    let out = Command::new("git")
        .args(args)
        .current_dir(cwd)
        .output()
        .expect("git must be runnable");
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
        .filter(|s| !s.is_empty())
}

/// Run `rigger <args...>` in `cwd` and return (stdout, stderr, success).
fn run_rigger(cwd: &Path, args: &[&str]) -> (String, String, bool) {
    run_rigger_envs(cwd, args, &[])
}

/// Run `rigger <args...>` in `cwd` with extra environment `envs` and return
/// (stdout, stderr, success). Used by the `rigger validate` advisory tests to stub
/// `RIGGER_NPM` (so `rigger setup` installs the workflow without a real npm).
fn run_rigger_envs(cwd: &Path, args: &[&str], envs: &[(&str, &str)]) -> (String, String, bool) {
    let mut cmd = Command::new(rigger_bin());
    cmd.args(args).current_dir(cwd);
    for (k, v) in envs {
        cmd.env(k, v);
    }
    let out = cmd.output().expect("failed to spawn the rigger binary");
    (
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
        out.status.success(),
    )
}

/// Extract a JSON string field's value from a one-line JSON object `line` - a tiny reader
/// for asserting on `rigger step`'s printed wave without a JSON dependency in the test crate.
/// Finds `"key":"` and returns everything up to the next `"`. Sufficient for the values these
/// tests read (deterministic ids and filesystem paths, which carry no embedded quote/backslash
/// that would need JSON unescaping); returns `None` when the key is absent.
fn json_string_field(line: &str, key: &str) -> Option<String> {
    let needle = format!("\"{key}\":\"");
    let start = line.find(&needle)? + needle.len();
    let rest = &line[start..];
    let end = rest.find('"')?;
    Some(rest[..end].to_string())
}

/// Run `git <args...>` in `cwd` and assert it succeeds (for seeding a repo state in a
/// test - staging and committing scaffolded files so `.rigger/` is tracked+clean).
fn git_ok(cwd: &Path, args: &[&str]) {
    let ok = Command::new("git")
        .args(args)
        .current_dir(cwd)
        .status()
        .expect("git must be runnable")
        .success();
    assert!(ok, "git {args:?} must succeed");
}

/// The full exit disposition of a spawned `rigger`, richer than `run_rigger`'s bare
/// `success` bool: the raw exit code (if the process exited normally) and the signal
/// (if it was killed) - both needed to distinguish a CLEAN non-zero exit (a graceful
/// `Err` surfaced by `main`, code 1) from an ABORT (SIGABRT kills the process with a
/// signal and no exit code; a Rust panic that escapes `main` exits 101).
///
/// Gated to the `turbovec` feature: its ONLY consumer is the ORT-dylib degrade test,
/// which is itself `#[cfg(feature = "turbovec")]`, so without the gate this is dead
/// code in the `--no-default-features` lane (and `-D warnings` would reject it).
#[cfg(feature = "turbovec")]
struct RiggerOutcome {
    stderr: String,
    /// The process's exit code, or `None` if it was terminated by a signal.
    code: Option<i32>,
    /// The signal that killed the process, or `None` if it exited normally.
    signal: Option<i32>,
}

/// Run `rigger <args...>` in `cwd` with the extra environment `envs` applied to the
/// CHILD, capturing its full exit disposition. Used by the ORT-dylib degrade test,
/// which must set `ORT_DYLIB_PATH` for the child ONLY (a fresh process, so `ort`'s
/// process-global runtime `OnceLock` is uncached) and then inspect whether it exited
/// cleanly or was aborted by a signal. Gated to `turbovec` for the same reason as
/// [`RiggerOutcome`]: its only caller is the feature-gated degrade test.
#[cfg(feature = "turbovec")]
fn run_rigger_env(cwd: &Path, args: &[&str], envs: &[(&str, &str)]) -> RiggerOutcome {
    use std::os::unix::process::ExitStatusExt;
    let mut cmd = Command::new(rigger_bin());
    cmd.args(args).current_dir(cwd);
    for (k, v) in envs {
        cmd.env(k, v);
    }
    let out = cmd.output().expect("failed to spawn the rigger binary");
    RiggerOutcome {
        stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
        code: out.status.code(),
        signal: out.status.signal(),
    }
}

/// `rigger emit` appends + folds, and `rigger peers <file>` then shows the decision
/// scoped to the file it governs - the round-trip a workflow agent makes to record a
/// decision and have a peer read it back through the context graph.
#[test]
fn emit_appends_and_folds_then_peers_shows_it() {
    let dir = temp_project();
    let root = dir.path();
    // A run already created the store; the courier appends to it (it never fabricates).
    seed_store(root);

    // Emit a DecisionMade governing src/foo.rs.
    let (out, err, ok) = run_rigger(
        root,
        &[
            "emit",
            "DecisionMade",
            r#"{"id":"d1","summary":"x","governs":["src/foo.rs"]}"#,
        ],
    );
    assert!(ok, "emit must succeed; stderr: {err}");
    assert!(
        out.contains("emitted DecisionMade"),
        "emit prints a one-line confirmation; got: {out:?}"
    );

    // The seeded event store still holds the append, and emit created the graph db
    // beside it (the projector db is derived state, so emit builds it on demand).
    assert!(
        root.join(".rigger").join("events.db").exists(),
        "emit must append to the seeded event store"
    );
    assert!(
        root.join(".rigger").join("graph.db").exists(),
        "emit must create the context-graph projector db"
    );

    // peers scoped to the file the decision governs shows it back.
    let (out, err, ok) = run_rigger(root, &["peers", "src/foo.rs"]);
    assert!(ok, "peers must succeed; stderr: {err}");
    assert!(
        out.contains("decision d1") && out.contains("governs: src/foo.rs"),
        "peers src/foo.rs must show the d1 decision governing it; got: {out:?}"
    );

    // peers scoped to an UNRELATED file does not surface d1 (the blast-radius scope).
    let (out, _err, ok) = run_rigger(root, &["peers", "src/other.rs"]);
    assert!(ok, "peers must succeed for an unrelated file");
    assert!(
        !out.contains("decision d1"),
        "peers scoped to an unrelated file must not show d1; got: {out:?}"
    );

    // peers with no files returns every decision (the unscoped view).
    let (out, _err, ok) = run_rigger(root, &["peers"]);
    assert!(ok, "unscoped peers must succeed");
    assert!(
        out.contains("decision d1"),
        "unscoped peers must show d1; got: {out:?}"
    );
}

/// `rigger playbooks --rebuild` (spec 13b, unit 2) distills the recorded `LessonLearned`
/// stream into a deduplicated, trigger-scoped playbook pool under `.rigger/playbooks/`,
/// reconstructing it from the log. Two lessons carrying the SAME text collapse into ONE
/// playbook whose trigger scope unions their `about` files; a distinct lesson is its own
/// playbook; and the pool is a projection - re-running the rebuild is idempotent.
#[test]
fn playbooks_rebuild_distills_the_lesson_log_into_a_deduped_pool() {
    let dir = temp_project();
    let root = dir.path();
    // A prior run created the store; the distiller only READS it.
    seed_store(root);

    // Two lessons with the SAME text about DIFFERENT files (they must dedup + union), plus
    // one distinct lesson (its own playbook).
    let lessons = [
        (r#"{"id":"la","summary":"guard the checked add","about":["a.rs"]}"#),
        (r#"{"id":"lb","summary":"guard the checked add","about":["b.rs"]}"#),
        (r#"{"id":"lc","summary":"close the scratch file","about":["c.rs"]}"#),
    ];
    for l in lessons {
        let (_out, err, ok) = run_rigger(root, &["emit", "LessonLearned", l]);
        assert!(ok, "emit LessonLearned must succeed; stderr: {err}");
    }

    // Rebuild the pool from the log.
    let (out, err, ok) = run_rigger(root, &["playbooks", "--rebuild"]);
    assert!(ok, "playbooks --rebuild must succeed; stderr: {err}");
    assert!(
        out.contains("rebuilt 2 playbook(s)"),
        "two distinct lessons distill to 2 playbooks; got: {out:?}"
    );

    // The pool is on disk as native agent-files.
    let pool = root.join(".rigger").join("playbooks");
    let read_pool = || -> (usize, String) {
        let mut files = 0;
        let mut bodies = String::new();
        for entry in std::fs::read_dir(&pool).unwrap() {
            let p = entry.unwrap().path();
            if p.extension().and_then(|x| x.to_str()) == Some("md") {
                files += 1;
                bodies.push_str(&std::fs::read_to_string(&p).unwrap());
            }
        }
        (files, bodies)
    };
    let (files, bodies) = read_pool();
    assert_eq!(
        files, 2,
        "the deduped pool holds one file per distinct lesson"
    );
    // The deduped playbook unions both lessons' about files as its trigger predicate and
    // records the fold count in its frontmatter.
    assert!(
        bodies.contains("guard the checked add") && bodies.contains("close the scratch file"),
        "both distinct lesson bodies must be present; got:\n{bodies}"
    );
    assert!(
        bodies.contains("- a.rs") && bodies.contains("- b.rs"),
        "the deduped playbook's trigger scope must union both lessons' about files;\n{bodies}"
    );
    assert!(
        bodies.contains("lessons: 2"),
        "the deduped playbook must record it folded 2 lessons;\n{bodies}"
    );

    // The pool is a rebuildable PROJECTION: re-running over the same log is idempotent.
    let (out2, _e, ok2) = run_rigger(root, &["playbooks", "--rebuild"]);
    assert!(ok2, "a second rebuild must succeed");
    assert!(out2.contains("rebuilt 2 playbook(s)"));
    let (files2, _b2) = read_pool();
    assert_eq!(
        files2, 2,
        "re-running the projection leaves no duplicate/leftover files"
    );
}

/// `rigger emit` of a ReviewFinding shows back through `rigger peers` as a finding
/// line (id, by, summary, about) - the same channel concurrent reviewers use.
#[test]
fn emit_review_finding_shows_in_peers() {
    let dir = temp_project();
    let root = dir.path();
    seed_store(root);

    let (_out, err, ok) = run_rigger(
        root,
        &[
            "emit",
            "ReviewFinding",
            r#"{"id":"f1","by":"tech-lens","summary":"skips the buffer","about":["combat.rs"]}"#,
        ],
    );
    assert!(ok, "emit ReviewFinding must succeed; stderr: {err}");

    let (out, err, ok) = run_rigger(root, &["peers", "combat.rs"]);
    assert!(ok, "peers must succeed; stderr: {err}");
    assert!(
        out.contains("finding f1")
            && out.contains("by tech-lens")
            && out.contains("about: combat.rs"),
        "peers must render the finding's id/by/about; got: {out:?}"
    );
}

/// `rigger emit` from a directory with NO existing `.rigger/events.db` (and no ancestor
/// that has one) REFUSES rather than fabricating a fresh empty store there (spec 05). The
/// payload is valid JSON, so this reaches the store-open seam rather than failing at parse.
#[test]
fn emit_refuses_to_fabricate_a_store_when_none_exists() {
    let dir = temp_project();
    let root = dir.path();

    let (_out, err, ok) = run_rigger(
        root,
        &[
            "emit",
            "DecisionMade",
            r#"{"id":"d1","summary":"x","governs":["src/foo.rs"]}"#,
        ],
    );
    assert!(
        !ok,
        "emit must refuse when there is no existing store; stderr: {err}"
    );
    assert!(
        err.contains("no rigger store found") && err.contains("refusing to fabricate"),
        "emit must explain the refusal; got: {err:?}"
    );
    assert!(
        !root.join(".rigger").join("events.db").exists(),
        "emit must NOT fabricate a store when it refuses"
    );
}

/// `rigger prompt` is a WORKER-INVOKED store-opening courier (a unit fetches its own slim
/// spawn manifest from the log), so run from a storeless cwd it must REFUSE like `emit`/
/// `result`/`reported`, never fabricate a fresh empty `.rigger/events.db` and then report
/// "no spawn request recorded" for every id, stranding the worker. Guards the routing of
/// `cmd_prompt` through [`require_store_dir`] against regressing to a cwd-relative
/// `Store::open`.
#[test]
fn prompt_refuses_to_fabricate_a_store_when_none_exists() {
    let dir = temp_project();
    let root = dir.path();

    let (_out, err, ok) = run_rigger(root, &["prompt", "u/implementer#0"]);
    assert!(
        !ok,
        "prompt must refuse when there is no existing store; stderr: {err}"
    );
    assert!(
        err.contains("no rigger store found") && err.contains("refusing to fabricate"),
        "prompt must explain the refusal; got: {err:?}"
    );
    assert!(
        !root.join(".rigger").join("events.db").exists(),
        "prompt must NOT fabricate a store when it refuses"
    );
}

/// The paradigm defect (adv-result-wrong-cwd-fabricates-store): `rigger result` run from
/// a unit-worktree-shaped cwd - a tracked `.rigger/workflow.yml` but NO machine-local
/// `.rigger/events.db` - must REFUSE instead of fabricating a fresh dead store and printing
/// success while the real spawn stays parked. Without the guard, result would create
/// `.rigger/events.db` here and exit 0.
#[test]
fn result_refuses_to_fabricate_a_store_from_a_worktree_shaped_cwd() {
    let dir = temp_project();
    let root = dir.path();
    // The tracked half of a checkout: `.rigger/` with workflow.yml, but no events.db.
    std::fs::create_dir_all(root.join(".rigger")).unwrap();
    std::fs::write(root.join(".rigger").join("workflow.yml"), "stages: []\n").unwrap();

    let (out, err, ok) = run_rigger(root, &["result", "u/implementer#0", "did the work"]);
    assert!(
        !ok,
        "result must refuse from a storeless worktree; stdout: {out:?} stderr: {err}"
    );
    assert!(
        err.contains("no rigger store found"),
        "result must explain the refusal; got: {err:?}"
    );
    assert!(
        !root.join(".rigger").join("events.db").exists(),
        "result must NOT fabricate a store when it refuses"
    );
}

/// A courier run from a SUBDIRECTORY of the project root walks up to the root's existing
/// store and records THERE - it does not create a second store in the subdir. Proven by
/// `rigger reported` from the root finding the result the subdir invocation wrote.
#[test]
fn result_walks_up_to_a_parent_store_from_a_subdirectory() {
    let dir = temp_project();
    let root = dir.path();
    seed_store(root);
    let sub = root.join("crate").join("src");
    std::fs::create_dir_all(&sub).unwrap();

    let (_out, err, ok) = run_rigger(&sub, &["result", "u/implementer#0", "did the work"]);
    assert!(
        ok,
        "result from a subdir must record into the parent store; stderr: {err}"
    );
    assert!(
        !sub.join(".rigger").exists(),
        "result must not fabricate a store in the subdir; it walks up"
    );

    // The result landed in the ROOT store (not a fabricated subdir one): `reported`,
    // which resolves the store the same walk-up way, finds it from the root.
    let (out, err, ok) = run_rigger(root, &["reported", "u/implementer#0"]);
    assert!(
        ok,
        "the walked-up result must be readable from the root store; stderr: {err}"
    );
    assert!(
        out.contains("u/implementer#0") && out.contains("ok"),
        "reported must confirm the recorded result; got: {out:?}"
    );
}

/// The PRIMARY named threat (adv-u9-walkup-namespace-misfile-default-layout): a courier run
/// from a REAL git-linked worktree nested INSIDE the repo - the Gap-14 default scratch root
/// `<repo>/.rigger/tmp/...`, where the conductor actually spawns units - must record into the
/// SAME namespaced stream the conductor reads, not misfile it under `proj-<worktree>-run`
/// while the spawn stays parked. Walking up alone is not enough: the walked-up write lands in
/// the real store FILE, but the stream is chosen by the identity, and `git rev-parse
/// --show-toplevel` from inside a linked worktree returns the WORKTREE path (basename
/// `rigger-wt-x`), so a cwd-anchored identity misfiles the append. A plain subdir shares the
/// git top-level and hides this; only a real linked worktree exposes the divergence. Proven
/// end-to-end: `rigger result` from inside the worktree, then `rigger reported` FROM THE REPO
/// ROOT must see the recorded result (it reads `proj-<repo>-run`, the conductor's stream).
#[test]
fn result_from_a_nested_git_worktree_records_into_the_repo_stream() {
    let dir = temp_git_project_with_commit();
    let root = dir.path();
    // A prior run created the store the conductor reads (identity = the repo basename).
    seed_store(root);

    // A REAL git-linked worktree nested under the repo, exactly like the conductor's
    // Gap-14 scratch root. `git worktree add` needs a committed HEAD, which
    // `temp_git_project_with_commit` provides.
    let wt = root.join(".rigger").join("tmp").join("rigger-wt-x");
    std::fs::create_dir_all(wt.parent().unwrap()).unwrap();
    let ok = Command::new("git")
        .args(["worktree", "add", "-q"])
        .arg(&wt)
        .current_dir(root)
        .status()
        .expect("git must be runnable")
        .success();
    assert!(
        ok,
        "git worktree add must succeed for the nested-worktree test"
    );

    // Record a result from INSIDE the nested worktree.
    let (_out, err, ok) = run_rigger(&wt, &["result", "u/implementer#0", "did the work"]);
    assert!(
        ok,
        "result from inside a nested git worktree must succeed; stderr: {err}"
    );
    // It walked up to the repo store - it did NOT fabricate a store inside the worktree.
    assert!(
        !wt.join(".rigger").join("events.db").exists(),
        "result must NOT fabricate a store inside the worktree; it walks up to the repo"
    );

    // The write landed in the stream the CONDUCTOR reads (identity = repo root, not the
    // worktree), so `reported` FROM THE REPO ROOT sees it. Before the identity fix, the
    // append misfiled under `proj-rigger-wt-x-run` and this read returned exit-non-zero
    // "no recorded result yet" while the spawn stayed parked - the exact charter defect.
    let (out, err, ok) = run_rigger(root, &["reported", "u/implementer#0"]);
    assert!(
        ok,
        "the worktree's result must be readable from the repo root (the conductor's \
         stream); stderr: {err}, stdout: {out}"
    );
    assert!(
        out.contains("u/implementer#0") && out.contains("ok"),
        "reported from the repo root must confirm the worktree's recorded result; got: {out:?}"
    );
}

/// Spec 08 item 6: within the bounded walk scope the OUTERMOST store wins. A courier run
/// from a subdir that carries its OWN shadow `.rigger/events.db` must record into the repo
/// ROOT's store (the real run stream), never the nearer shadow - and it WARNS on stderr,
/// naming BOTH paths, so a shadow can never silently eclipse the run. Proven end-to-end:
/// `rigger result` from the shadowed subdir, `rigger reported` FROM THE ROOT sees it, and
/// the bypassed shadow `events.db` stays a byte-empty file (nothing was ever written into it).
#[test]
fn result_binds_the_outermost_store_and_warns_about_a_bypassed_shadow() {
    let dir = temp_project();
    let root = dir.path();
    seed_store(root); // the repo root's real store (the outermost in scope)

    // A nested subdir of the SAME repo carrying its own shadow store.
    let shadowed = root.join("crate").join("nested");
    std::fs::create_dir_all(&shadowed).unwrap();
    seed_store(&shadowed);
    let shadow_db = shadowed.join(".rigger").join("events.db");

    let (out, err, ok) = run_rigger(&shadowed, &["result", "u/implementer#0", "did the work"]);
    assert!(
        ok,
        "result from a shadowed subdir must record into the outermost store; stderr: {err}"
    );
    assert!(
        out.contains("recorded result for u/implementer#0"),
        "the result must still be recorded; got: {out:?}"
    );
    // The warning names BOTH the bypassed nearer shadow and the chosen outermost store.
    assert!(
        err.contains("shadow store")
            && err.contains(&shadow_db.parent().unwrap().display().to_string())
            && err.contains(&root.join(".rigger").display().to_string()),
        "result must warn, naming both the bypassed shadow and the outermost store; got: {err:?}"
    );
    // The bypassed shadow store was NEVER opened: its seeded events.db stays byte-empty
    // (a real write would have Store::open-initialized the schema, growing it past 0 bytes).
    assert_eq!(
        std::fs::metadata(&shadow_db).unwrap().len(),
        0,
        "the bypassed shadow store must stay untouched (byte-empty)"
    );

    // The write landed in the OUTERMOST (repo root) store: `reported` from the root - which
    // resolves that same store - confirms the spawn is answered.
    let (rout, rerr, ok) = run_rigger(root, &["reported", "u/implementer#0"]);
    assert!(
        ok,
        "the result must be readable from the outermost store; stderr: {rerr}"
    );
    assert!(
        rout.contains("u/implementer#0") && rout.contains("ok"),
        "reported from the root must confirm the outermost-store record; got: {rout:?}"
    );
}

/// Spec 08 item 5: under `--if-absent` the orphan advisory states the CONDITIONAL - it must
/// never claim it is "recording an orphan result", because the CAS records only if the spawn
/// is still unanswered (an already-answered spawn is left untouched). The plain path keeps
/// its "recording an orphan result" wording (pinned by
/// `result_prints_an_orphan_advisory_for_an_unrecorded_id`).
#[test]
fn result_if_absent_orphan_advisory_states_the_conditional_not_a_recording() {
    let dir = temp_project();
    let root = dir.path();
    seed_store(root);

    let (_out, err, ok) = run_rigger(
        root,
        &["result", "ghost/implementer#0", "--if-absent", "output"],
    );
    assert!(
        ok,
        "an --if-absent orphan record must still succeed; stderr: {err}"
    );
    assert!(
        err.contains("no spawn request is recorded")
            && err.contains("ghost/implementer#0")
            && err.contains("--if-absent records only if the spawn is unanswered"),
        "the --if-absent orphan advisory must state the conditional; got: {err:?}"
    );
    assert!(
        !err.contains("recording an orphan result"),
        "the --if-absent advisory must NOT claim a recording it may not make; got: {err:?}"
    );
}

/// `rigger result` for an id with no recorded spawn request prints an ORPHAN advisory to
/// stderr - and still records (advisory only; pre-recording is legitimate).
#[test]
fn result_prints_an_orphan_advisory_for_an_unrecorded_id() {
    let dir = temp_project();
    let root = dir.path();
    seed_store(root);

    let (out, err, ok) = run_rigger(root, &["result", "ghost/implementer#0", "output"]);
    assert!(
        ok,
        "an orphan result still records (advisory only); stderr: {err}"
    );
    assert!(
        err.contains("no spawn request is recorded") && err.contains("ghost/implementer#0"),
        "result must advise about the orphan id on stderr; got: {err:?}"
    );
    assert!(
        out.contains("recorded result for ghost/implementer#0"),
        "the orphan result must still be recorded; got: {out:?}"
    );
}

/// Re-recording a result for the same id prints a SUPERSEDE advisory (naming the prior
/// result's log position) - the record still lands (results are last-write-wins).
#[test]
fn result_prints_a_supersede_advisory_when_a_result_already_exists() {
    let dir = temp_project();
    let root = dir.path();
    seed_store(root);

    let (_out, _err, ok) = run_rigger(root, &["result", "u/implementer#0", "first"]);
    assert!(ok, "the first record must succeed");

    let (out, err, ok) = run_rigger(root, &["result", "u/implementer#0", "second"]);
    assert!(
        ok,
        "the superseding record must succeed (advisory only); stderr: {err}"
    );
    assert!(
        err.contains("already has a recorded result at position") && err.contains("supersedes"),
        "result must advise that it supersedes the prior result; got: {err:?}"
    );
    assert!(
        out.contains("recorded result for u/implementer#0"),
        "the superseding result must still be recorded; got: {out:?}"
    );
}

/// Write a minimal `.rigger/workflow.yml` into `root` pinning `defaults.grounder` to
/// the given name. Tests that exercise the LITERAL grep grounder pin `grep`
/// explicitly: turbovec is the default grounder now, so an unconfigured project would
/// resolve to the semantic engine (which embeds via a downloaded model and does not
/// have grep's exact-line / no-match-empty / k-cap contract). Pinning grep keeps the
/// test deterministic, offline, and focused on the literal grounder's behavior.
fn write_grounder_workflow(root: &Path, grounder: &str) {
    let rigger = root.join(".rigger");
    // The agents/ dir must exist for `config::load` to succeed; without it the load
    // fails and `cmd_ground` falls back to the UNSET grounder (which resolves to
    // turbovec), so the pinned `grounder` would never take effect.
    std::fs::create_dir_all(rigger.join("agents")).unwrap();
    std::fs::write(
        rigger.join("workflow.yml"),
        format!("name: t\ndefaults:\n  grounder: {grounder}\n"),
    )
    .unwrap();
}

/// `rigger ground "<query>"` returns repo references (`file:line: <text>`) from the
/// project's configured grounder over a small temp repo. This pins the LITERAL grep
/// grounder (its exact-line / empty-on-no-match / k-cap contract); turbovec, the
/// default grounder, is exercised by its own unit test (which downloads the model).
#[test]
fn ground_returns_references_from_the_repo() {
    let dir = temp_project();
    let root = dir.path();
    write_grounder_workflow(root, "grep");
    std::fs::write(
        root.join("combat.rs"),
        "fn apply_damage() {}\nfn render() {}\n",
    )
    .unwrap();

    // The configured grounder is grep; a query that matches a line returns it.
    let (out, err, ok) = run_rigger(root, &["ground", "apply_damage"]);
    assert!(ok, "ground must succeed; stderr: {err}");
    assert!(
        out.lines()
            .any(|l| l.starts_with("combat.rs:1:") && l.contains("apply_damage")),
        "ground must return combat.rs:1: with the matching text; got: {out:?}"
    );

    // A query that matches nothing yields empty output, not an error.
    let (out, _err, ok) = run_rigger(root, &["ground", "no_such_symbol_anywhere"]);
    assert!(ok, "ground must succeed even with no matches");
    assert!(
        out.trim().is_empty(),
        "ground with no matches prints nothing; got: {out:?}"
    );

    // The explicit k argument caps the number of references.
    std::fs::write(
        root.join("many.rs"),
        "needle\nneedle\nneedle\nneedle\nneedle\n",
    )
    .unwrap();
    let (out, _err, ok) = run_rigger(root, &["ground", "needle", "2"]);
    assert!(ok, "ground with an explicit k must succeed");
    assert_eq!(
        out.lines().filter(|l| !l.is_empty()).count(),
        2,
        "ground <query> 2 must return at most two references; got: {out:?}"
    );
}

/// `rigger reindex <file>` requires at least one file and is a clear error otherwise:
/// a workflow agent calling it with no files must get a non-zero exit, not a silent
/// no-op. (This holds for every grounder, so it needs no model and runs in both lanes.)
#[test]
fn reindex_requires_at_least_one_file() {
    let dir = temp_project();
    let root = dir.path();
    // The grep grounder's reindex is a no-op, but the CLI still enforces the arg
    // contract before dispatching, so this is deterministic and offline.
    write_grounder_workflow(root, "grep");

    let (_out, err, ok) = run_rigger(root, &["reindex"]);
    assert!(!ok, "reindex with no files must be a non-zero exit");
    assert!(
        err.contains("expected at least one file"),
        "the error must explain that a file is required; got: {err:?}"
    );
}

/// Criterion 3 (spec 15): the persisted symbol index is BYTE-IDENTICAL when built in two
/// SEPARATE processes over the same tree. This is the guard the in-process lib test
/// structurally CANNOT make: Rust `HashMap`/`HashSet` seed randomization differs only ACROSS
/// processes, so a `HashMap` that leaked onto the serialized path would pass every in-process
/// test yet diverge here. The `rigger symbols-index` harness builds + persists unit 3's index
/// directly (independent of grounder selection - so this test needs nothing from unit 4), and
/// each `run_rigger` is a genuinely fresh process with its own hash seed; a stable diff proves
/// the determinism-by-construction (`BTreeMap`, never `HashMap`) the persistence relies on.
#[cfg(feature = "symbols")]
#[test]
fn symbol_index_is_byte_identical_across_processes() {
    let dir = temp_project();
    let root = dir.path();
    // TWO source files with distinct symbols, NOT one: a single-key map serializes identically
    // whether it is a `BTreeMap` or a `HashMap`, so the cross-process guard only bites with >= 2
    // keys whose rel-path ordering a `HashMap` seed would scramble. Their names also let us assert
    // the index is NON-EMPTY, so a total extraction failure (an empty index in BOTH processes,
    // which is vacuously byte-identical) cannot pass this test green.
    std::fs::write(root.join("m.rs"), "fn alpha(){} fn beta(){}\n").unwrap();
    std::fs::write(root.join("z.rs"), "fn gamma(){} fn delta(){}\n").unwrap();
    let index = root.join(".rigger").join("symbols").join("index.json");

    // Process 1 builds + persists the index.
    let (out1, err1, ok1) = run_rigger(root, &["symbols-index"]);
    assert!(ok1, "first symbols-index must succeed; stderr: {err1}");
    assert!(
        out1.contains("2 file(s)"),
        "the harness must report both indexed files, not a vacuous empty index; stdout: {out1}"
    );
    let first = std::fs::read(&index).expect("the first process must persist the index");
    let first_text = String::from_utf8(first.clone()).expect("index.json is UTF-8");
    // The index must actually reflect the tree, so byte-identity is over MEANINGFUL content.
    for name in ["alpha", "beta", "gamma", "delta"] {
        assert!(
            first_text.contains(name),
            "the persisted index must contain the extracted symbol {name:?}; got: {first_text}"
        );
    }

    // Remove it, then a SECOND, independent process rebuilds it over the same tree.
    std::fs::remove_file(&index).unwrap();
    let (_out2, err2, ok2) = run_rigger(root, &["symbols-index"]);
    assert!(ok2, "second symbols-index must succeed; stderr: {err2}");
    let second = std::fs::read(&index).expect("the second process must persist the index");

    assert_eq!(
        first, second,
        "the persisted multi-file index must be byte-identical across processes"
    );
}

/// End-to-end selection wiring (spec 15, unit 4): with `defaults.grounder: symbols`, `rigger
/// ground` resolves the real `Symbols` grounder through `select_grounder` - building + persisting
/// the structural index over the project - and ranks a DEFINITION above an incidental prose
/// mention. This drives the whole feature-on path (config -> select_grounder -> Symbols::open ->
/// build_index -> ground) that a lib test rooted at `.` cannot exercise over a controlled tree.
#[cfg(feature = "symbols")]
#[test]
fn ground_via_symbols_grounder_ranks_a_definition_first() {
    let dir = temp_project();
    let root = dir.path();
    // Pin `defaults.grounder: symbols` (the helper also creates the `agents/` dir `config::load`
    // needs, so the pinned grounder actually takes effect rather than falling back to the default).
    write_grounder_workflow(root, "symbols");
    // combat.rs DEFINES apply_damage; notes.rs only mentions it in a comment (prose, not a symbol).
    std::fs::write(
        root.join("combat.rs"),
        "fn apply_damage(x: u8) -> u8 { x }\n",
    )
    .unwrap();
    std::fs::write(
        root.join("notes.rs"),
        "// TODO: revisit apply_damage later\nfn unrelated() {}\n",
    )
    .unwrap();

    let (out, err, ok) = run_rigger(root, &["ground", "apply_damage", "5"]);
    assert!(
        ok,
        "ground via the symbols grounder must succeed; stderr: {err}"
    );
    let first_line = out.lines().next().unwrap_or_default();
    assert!(
        first_line.starts_with("combat.rs:"),
        "the definition site must be grounded first; stdout: {out}"
    );
    assert!(
        !out.contains("notes.rs"),
        "an incidental prose mention must not be grounded as a symbol; stdout: {out}"
    );
}

/// End-to-end reindex wiring (spec 15, unit 4): with `defaults.grounder: symbols`, the shipped
/// `rigger reindex <file>` CLI must resolve the SAME real `Symbols` grounder through
/// `select_reindex_grounder` that `rigger ground` resolves through `select_grounder` - NOT die
/// with the false `symbols_feature_missing_error` while the feature is built. It must exit 0 AND
/// actually freshen: a symbol written into a file AFTER the index is first built becomes findable
/// via `rigger ground` once that file is reindexed. This is the symmetric guard to
/// `ground_via_symbols_grounder_ranks_a_definition_first`; without the symbols arm in
/// `select_reindex_grounder` (both cfg lanes) it turns RED at the very first `reindex` exit code.
#[cfg(feature = "symbols")]
#[test]
fn reindex_via_symbols_grounder_updates_the_persisted_index() {
    let dir = temp_project();
    let root = dir.path();
    // Pin `defaults.grounder: symbols` (the helper also creates the `agents/` dir `config::load`
    // needs, so the pinned grounder actually takes effect rather than falling back to the default).
    write_grounder_workflow(root, "symbols");
    std::fs::write(root.join("combat.rs"), "fn apply_damage() {}\n").unwrap();

    // First ground builds + persists the structural index (cold path) under .rigger/symbols/.
    let (_out, err, ok) = run_rigger(root, &["ground", "apply_damage", "1"]);
    assert!(ok, "the initial ground must build the index; stderr: {err}");
    assert!(
        root.join(".rigger")
            .join("symbols")
            .join("index.json")
            .exists(),
        "grounding via symbols must persist the structural index to .rigger/symbols/"
    );

    // The change lands: combat.rs gains teleport_player (a symbol absent from the built index).
    std::fs::write(
        root.join("combat.rs"),
        "fn apply_damage() {}\nfn teleport_player() {}\n",
    )
    .unwrap();

    // Reindex ONLY that file via the shipped CLI. Under `defaults.grounder: symbols` with the
    // feature BUILT this MUST exit 0 (the selector-drift regression made it exit 1 with a false
    // feature-missing error), and it must name the reindexed file.
    let (out, err, ok) = run_rigger(root, &["reindex", "combat.rs"]);
    assert!(
        ok,
        "reindex under defaults.grounder: symbols must succeed, not falsely report a missing \
         feature; stderr: {err}"
    );
    assert!(
        out.contains("combat.rs"),
        "reindex prints a confirmation naming the file; got: {out:?}"
    );

    // The just-landed symbol is now findable through the SAME persisted index a later ground uses -
    // the reindex freshened the on-disk store, not just an in-process copy.
    let (out, err, ok) = run_rigger(root, &["ground", "teleport_player", "1"]);
    assert!(ok, "ground after reindex must succeed; stderr: {err}");
    assert!(
        out.lines()
            .next()
            .map(|l| l.starts_with("combat.rs:"))
            .unwrap_or(false),
        "after the reindex CLI freshens the symbols index, the new symbol must ground to \
         combat.rs; got: {out:?}"
    );
}

/// End-to-end selection wiring for the `hybrid` grounder (spec 15, unit 5): with
/// `defaults.grounder: hybrid` and BOTH features built, `rigger ground` must resolve the real
/// `Hybrid` through `select_grounder` and `rigger reindex` must resolve it through
/// `select_reindex_grounder` - the SAME symmetric CLI surface unit 4 was rejected for omitting for
/// `symbols`. It pins the composition end-to-end: the structural definition (`combat.rs`) ranks
/// FIRST and the semantic pass still fills a file the name match misses (`enemy.rs`, which defines
/// no matching symbol), and the shipped `rigger reindex <file>` exits 0 (NOT the false
/// unknown/feature error) and freshens both axes so a just-landed symbol becomes findable.
/// Gated to the both-features lane and `file_serial(turbovec_model)` because it spawns a `rigger`
/// subprocess that builds an ort/model session, which must never race another model construction.
#[cfg(all(feature = "symbols", feature = "turbovec"))]
#[test]
#[serial_test::file_serial(turbovec_model)]
fn hybrid_grounds_and_reindexes_via_the_shipped_cli() {
    let dir = temp_project();
    let root = dir.path();
    // Pin `defaults.grounder: hybrid` (the helper also creates the `agents/` dir `config::load`
    // needs, so the pinned grounder actually takes effect rather than falling back to the default).
    write_grounder_workflow(root, "hybrid");
    // combat.rs DEFINES apply_damage (a structural hit); enemy.rs is semantically about dealing
    // damage but defines NO such symbol (a semantic-only hit the structural axis alone misses).
    std::fs::write(
        root.join("combat.rs"),
        "fn apply_damage(target: &mut Entity, amount: f32) {\n    target.health -= amount;\n}\n",
    )
    .unwrap();
    std::fs::write(
        root.join("enemy.rs"),
        "fn reduce_hitpoints(enemy: &mut Enemy, blow: f32) {\n    enemy.hp -= blow;\n}\n",
    )
    .unwrap();

    // `rigger ground` resolves `hybrid` via select_grounder: structural definition first, then the
    // semantic pass fills the recall the name match missed.
    let (out, err, ok) = run_rigger(root, &["ground", "apply_damage", "5"]);
    assert!(
        ok,
        "ground via the hybrid grounder must succeed; stderr: {err}"
    );
    assert!(
        out.lines()
            .next()
            .map(|l| l.starts_with("combat.rs:"))
            .unwrap_or(false),
        "the structural definition must be grounded FIRST; stdout: {out}"
    );
    assert!(
        out.contains("enemy.rs"),
        "the semantic pass must fill the recall the name match misses (enemy.rs); stdout: {out}"
    );

    // The change lands: combat.rs gains teleport_player (a symbol absent from the built index).
    std::fs::write(
        root.join("combat.rs"),
        "fn apply_damage(target: &mut Entity, amount: f32) {\n    target.health -= amount;\n}\n\
         fn teleport_player(player: &mut Player, dest: Tile) {\n    player.position = dest;\n}\n",
    )
    .unwrap();

    // `rigger reindex <file>` resolves `hybrid` via select_reindex_grounder and MUST exit 0 (the
    // omitted-arm regression made the symbols surface exit 1 with a false feature-missing error).
    let (out, err, ok) = run_rigger(root, &["reindex", "combat.rs"]);
    assert!(
        ok,
        "reindex under defaults.grounder: hybrid must succeed, not falsely report an unknown or \
         missing grounder; stderr: {err}"
    );
    assert!(
        out.contains("combat.rs"),
        "reindex prints a confirmation naming the file; got: {out:?}"
    );

    // The just-landed symbol is findable through the freshened index a later ground uses - the
    // reindex updated the on-disk structural store, proving the hybrid reindex fans out for real.
    let (out, err, ok) = run_rigger(root, &["ground", "teleport_player", "1"]);
    assert!(
        ok,
        "ground after the hybrid reindex must succeed; stderr: {err}"
    );
    assert!(
        out.lines()
            .next()
            .map(|l| l.starts_with("combat.rs:"))
            .unwrap_or(false),
        "after the reindex CLI freshens the hybrid index, the new symbol must ground to \
         combat.rs; got: {out:?}"
    );
}

/// `rigger reindex <file>` against the turbovec grounder UPDATES the persisted
/// grounding store incrementally: a term written into a file AFTER the index is first
/// built becomes findable via `rigger ground` once that file is reindexed - the CLI
/// surface the workflow calls after each unit lands. Gated to the turbovec lane (it
/// downloads the embedding model on first use, exactly like the grounder's own unit
/// test); the fixture is a single tiny file so the embed stays bounded.
/// Serialized cross-binary against the lib's model tests: this spawns a `rigger`
/// subprocess that builds an ort/CUDA session, and `serial_test`'s filesystem-lock
/// `file_serial` (same `turbovec_model` key the lib tests use) ensures no other model
/// construction - here or in the lib-test binary - runs concurrently on a GPU box.
#[cfg(feature = "turbovec")]
#[test]
#[serial_test::file_serial(turbovec_model)]
fn reindex_cli_updates_the_persisted_turbovec_store() {
    let dir = temp_project();
    let root = dir.path();
    write_grounder_workflow(root, "turbovec");
    std::fs::write(
        root.join("combat.rs"),
        "fn apply_damage(target: &mut Entity, amount: f32) {\n    target.health -= amount;\n}\n",
    )
    .unwrap();

    // First ground builds + persists the store (cold path). The persisted store dir
    // appears under .rigger/grounding/.
    let (_out, err, ok) = run_rigger(root, &["ground", "how is damage dealt", "1"]);
    assert!(ok, "the initial ground must build the index; stderr: {err}");
    assert!(
        root.join(".rigger")
            .join("grounding")
            .join("index.tvim")
            .exists(),
        "grounding the repo must persist the turbovec index to .rigger/grounding/"
    );

    // The change lands: combat.rs gains a teleport function (a term absent before).
    std::fs::write(
        root.join("combat.rs"),
        "fn apply_damage(target: &mut Entity, amount: f32) {\n    target.health -= amount;\n}\n\
         fn teleport_player(player: &mut Player, dest: Tile) {\n    player.position = dest;\n}\n",
    )
    .unwrap();

    // Reindex ONLY that file via the CLI - the incremental update the workflow runs.
    let (out, err, ok) = run_rigger(root, &["reindex", "combat.rs"]);
    assert!(ok, "reindex must succeed; stderr: {err}");
    assert!(
        out.contains("reindexed 1 file") && out.contains("combat.rs"),
        "reindex prints a confirmation naming the file; got: {out:?}"
    );

    // The just-landed term is now findable through the SAME store a later ground uses
    // - the reindex updated the persisted index, not just an in-process copy.
    let (out, err, ok) = run_rigger(
        root,
        &["ground", "teleport the player across the dungeon", "1"],
    );
    assert!(ok, "ground after reindex must succeed; stderr: {err}");
    assert!(
        out.lines().next().map(|l| l.starts_with("combat.rs:")).unwrap_or(false),
        "after the reindex CLI updates the store, the new term must ground to combat.rs; got: {out:?}"
    );
}

/// FINDING #1 (graceful ORT-dylib degradation): when the ONNX Runtime dylib `ort`
/// would `dlopen` cannot be loaded, grounder construction must DEGRADE GRACEFULLY - a
/// clean non-zero exit whose stderr names the runtime and an actionable remedy, NEVER
/// an unwind through `ort`'s internal `lib_handle()` panic that aborts the process.
///
/// This runs OUT OF PROCESS, spawning a fresh `rigger`, precisely because the check
/// cannot live in the lib-test binary: `ort` caches the first `libonnxruntime.so` it
/// dlopens in a process-global `OnceLock` that no env restore can undo, so an
/// in-process test that pointed `ORT_DYLIB_PATH` at a bad path and won the model lock
/// race POISONED that global and broke every later model-building test in the same
/// binary with `cannot open shared object file`. A subprocess gets its OWN ort globals,
/// so the bad path it loads dies with the child and cannot poison a sibling.
///
/// We pick the smallest grounder-constructing CLI path: `rigger ground "<q>" 1` with a
/// turbovec-pinned workflow resolves through `select_grounder` to `Turbovec::new(".")`,
/// which builds the model inside the `catch_unwind` under test. We set the CHILD's
/// `ORT_DYLIB_PATH` to a nonexistent path (`ensure_dylib_path` respects an already-set
/// value, so it will not "rescue" us with the discovered runtime), forcing the load to
/// fail. The `catch_unwind` must turn that into a clean `Err` that `main` prints and
/// exits 1 on - NOT a `panicked at` on stderr, NOT a signal death, NOT exit 101.
/// Serialized on the same `turbovec_model` key as the other model tests so no GPU
/// session construction overlaps this one cross-binary.
#[cfg(feature = "turbovec")]
#[test]
#[serial_test::file_serial(turbovec_model)]
fn ground_degrades_gracefully_when_the_ort_dylib_is_unresolvable() {
    let dir = temp_project();
    let root = dir.path();
    write_grounder_workflow(root, "turbovec");
    std::fs::write(
        root.join("combat.rs"),
        "fn apply_damage(target: &mut Entity, amount: f32) {\n    target.health -= amount;\n}\n",
    )
    .unwrap();

    // The child inherits a bad ORT_DYLIB_PATH: a path that does not exist, so `ort`'s
    // `dlopen` fails. It is set on the CHILD only (a fresh process with an uncached ort
    // runtime global), so the failing load poisons nothing outside this subprocess.
    let outcome = run_rigger_env(
        root,
        &["ground", "how is damage dealt", "1"],
        &[(
            "ORT_DYLIB_PATH",
            "/nonexistent/does-not-exist/libonnxruntime.so",
        )],
    );

    // 1. NOT aborted by a signal (SIGABRT would kill the process with no exit code).
    assert!(
        outcome.signal.is_none(),
        "grounder construction must not abort: the process was killed by signal {:?}; stderr: {}",
        outcome.signal,
        outcome.stderr
    );
    // 2. A CLEAN non-zero exit, and specifically NOT 101 (an escaped Rust panic's code).
    assert_eq!(
        outcome.code,
        Some(1),
        "an unresolvable ORT dylib must be a clean exit 1 (a surfaced Err), never a panic \
         (101) or abort; got code {:?}, signal {:?}; stderr: {}",
        outcome.code,
        outcome.signal,
        outcome.stderr
    );
    // 3. NO escaped panic on stderr - the failure was absorbed into a graceful Err, not
    //    a panic that unwound out of `construct`'s `catch_unwind`.
    assert!(
        !outcome.stderr.contains("panicked at"),
        "the ORT load failure must be a graceful Err, not an escaped panic; stderr: {}",
        outcome.stderr
    );
    // 4. The actionable error names the ONNX Runtime, that it could not be resolved for
    //    loading, and the grep opt-out remedy - the message `construct` returns.
    let err = &outcome.stderr;
    assert!(
        err.contains("ONNX Runtime")
            && err.contains("libonnxruntime.so")
            && err.contains("could not be resolved")
            && err.to_lowercase().contains("grep"),
        "the degrade message must name the runtime, that it could not be resolved, and the \
         grep opt-out remedy; got stderr: {err}"
    );
}

/// The ERROR-AFTER-EMBED exit path is crash-free (spec AC for the teardown fix): a run
/// that builds the GPU/CUDA embedding session and THEN fails - here, a persist that
/// cannot write because the store dir is read-only - must exit with a clean non-zero
/// code, NOT a SIGABRT from the ONNX Runtime / CUDA teardown heap corruption
/// (upstream pykeio/ort#564).
///
/// This is the path the old `libc::_exit(0)` dodge left exposed: it skipped the crashing
/// teardown only on the SUCCESS exit, so an error return still ran the racy `atexit`
/// destructors. The mitigation (`ort_teardown::release_ort_runtime`, called on BOTH paths
/// before `process::exit`) must make this path exit cleanly too.
///
/// We force the error deterministically: an EMPTY `.rigger/grounding/` dir at mode 000.
/// The cold `ground` still builds + runs the CUDA embed (proving the session was really
/// created - see the "CUDA execution provider available" stderr the child prints), then
/// the persist's store-lock open in that unwritable dir fails with EACCES, so `main`
/// takes its Err exit AFTER the embed - exactly the error-after-embed shape. We assert
/// the child was NOT killed by a signal and exited with a clean non-zero code.
///
/// Runs OUT OF PROCESS (like the degrade test) so the child owns its own ORT globals and
/// CUDA teardown; serialized on the shared `turbovec_model` key so no other GPU session
/// construction overlaps it cross-binary. Unix-only: it relies on POSIX dir permissions,
/// and the turbovec/CUDA path this guards is Linux-only regardless.
#[cfg(all(feature = "turbovec", unix))]
#[test]
#[serial_test::file_serial(turbovec_model)]
fn error_after_embed_exits_cleanly_without_a_teardown_abort() {
    use std::os::unix::fs::PermissionsExt;

    let dir = temp_project();
    let root = dir.path();
    write_grounder_workflow(root, "turbovec");
    std::fs::write(
        root.join("combat.rs"),
        "fn apply_damage(target: &mut Entity, amount: f32) {\n    target.health -= amount;\n}\n",
    )
    .unwrap();

    // Pre-create an EMPTY grounding dir and lock it to mode 000. There is no persisted
    // store to load, so the cold `ground` embeds the tree (building the CUDA session),
    // then fails when it tries to open the store lock / write the index into this
    // unwritable dir - an error that surfaces strictly AFTER the GPU embed.
    let grounding = root.join(".rigger").join("grounding");
    std::fs::create_dir_all(&grounding).unwrap();
    std::fs::set_permissions(&grounding, std::fs::Permissions::from_mode(0o000)).unwrap();

    // The whole test hinges on the mode-000 dir being genuinely UNWRITABLE by the child,
    // so its post-embed store-lock open fails with EACCES. Under root - or any principal
    // with CAP_DAC_OVERRIDE, common in CI containers - the kernel BYPASSES the mode bits,
    // so the child would write the store, ground succeeds, and the run exits 0. That is
    // not a teardown regression, but the `exit 1` assertion below would still fire and
    // report a misleading failure. Probe the actual enforcement with the same operation
    // the child does (create a file inside the dir); if WE can create it, the injection
    // cannot force the error, so skip cleanly rather than assert against a no-op fixture.
    // A filesystem probe (not a bare euid==0 check) is used deliberately: it also covers
    // the CAP_DAC_OVERRIDE-without-uid-0 and permissive-filesystem cases a euid test misses.
    let probe = grounding.join(".perm_probe");
    if std::fs::File::create(&probe).is_ok() {
        let _ = std::fs::remove_file(&probe);
        let _ = std::fs::set_permissions(&grounding, std::fs::Permissions::from_mode(0o755));
        eprintln!(
            "skipping error_after_embed_exits_cleanly_without_a_teardown_abort: the mode-000 \
             grounding dir is writable by this principal (root / CAP_DAC_OVERRIDE), so the \
             post-embed persist failure cannot be injected here"
        );
        return;
    }

    let outcome = run_rigger_env(root, &["ground", "how is damage dealt", "1"], &[]);

    // Restore permissions so the TempDir can be cleaned up on drop (a mode-000 subdir
    // would otherwise make the recursive remove fail).
    let _ = std::fs::set_permissions(&grounding, std::fs::Permissions::from_mode(0o755));

    // 1. The GPU/CUDA session really was built before the failure - otherwise this test
    //    would prove nothing about the TEARDOWN path. The grounder prints this line on
    //    stderr the moment the CUDA EP is selected, before it embeds and then fails.
    assert!(
        outcome.stderr.contains("CUDA execution provider available")
            || outcome.stderr.contains("embedding on"),
        "the child must have built the embedding session before failing (else the teardown \
         path is not exercised); stderr: {}",
        outcome.stderr
    );
    // 2. NOT killed by a signal: SIGABRT (signal 6) is exactly the teardown heap-corruption
    //    abort this fix eliminates, and it terminates the process with a signal and no code.
    assert!(
        outcome.signal.is_none(),
        "the error-after-embed exit must not abort in ORT/CUDA teardown: the process was \
         killed by signal {:?}; stderr: {}",
        outcome.signal,
        outcome.stderr
    );
    // 3. A CLEAN non-zero exit (the surfaced persist Err), specifically not 101 (panic).
    assert_eq!(
        outcome.code,
        Some(1),
        "the error-after-embed path must exit 1 cleanly (a surfaced Err), never a panic \
         (101) or a teardown abort; got code {:?}, signal {:?}; stderr: {}",
        outcome.code,
        outcome.signal,
        outcome.stderr
    );
    // 4. The failure was the post-embed persist, and it did not escape as a panic.
    assert!(
        !outcome.stderr.contains("panicked at"),
        "the persist failure must surface as a clean Err, not an escaped panic; stderr: {}",
        outcome.stderr
    );
}

/// Bad input to `rigger emit` is a clear error on stderr and a non-zero exit, never
/// a silent success - a workflow agent must be able to tell a malformed emit failed.
#[test]
fn emit_rejects_bad_json_with_a_nonzero_exit() {
    let dir = temp_project();
    let root = dir.path();

    // Not valid JSON at all.
    let (_out, err, ok) = run_rigger(root, &["emit", "DecisionMade", "{not json"]);
    assert!(!ok, "a malformed JSON payload must be a non-zero exit");
    assert!(
        err.contains("not valid JSON"),
        "the error must name the JSON problem; got: {err:?}"
    );

    // Valid JSON, but not an object (the emit data must be an object).
    let (_out, err, ok) = run_rigger(root, &["emit", "DecisionMade", "[1,2,3]"]);
    assert!(!ok, "a non-object JSON payload must be a non-zero exit");
    assert!(
        err.contains("must be a JSON object"),
        "the error must say the payload must be an object; got: {err:?}"
    );

    // A missing JSON argument is a clear usage error.
    let (_out, err, ok) = run_rigger(root, &["emit", "DecisionMade"]);
    assert!(!ok, "a missing JSON object must be a non-zero exit");
    assert!(
        err.contains("expected a JSON object"),
        "the error must explain the missing argument; got: {err:?}"
    );
}

/// The `main.rs` source text, read at test time from the crate manifest dir. `main.rs` is
/// a BINARY, not part of the `rigger` library, so its comments are not reachable through
/// the crate API - we assert on the file's bytes instead. `CARGO_MANIFEST_DIR` is stable
/// for both `cargo test` and the integration-test binary, so this resolves regardless of
/// the process cwd.
fn main_rs_source() -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("src")
        .join("main.rs");
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

/// Spec AC (u3-honest-doc): `src/main.rs`'s exit path must be HONESTLY documented -
/// whatever mitigation remains has to state what it does AND what it does not cover, with
/// NO overstated "removes the buggy code path entirely" framing.
///
/// The `ort_teardown::release_ort_runtime` mitigation deprives the upstream teardown bug
/// (pykeio/ort#564, still open) of its race; it does not delete the buggy ORT code, and
/// its guarantee is conditional on two invariants (drop-the-session-first, and the leaked
/// `G_ENV` static). This test locks in that the exit-path comment keeps saying so, so a
/// later edit cannot quietly regress the comment back into an absolute "this removes the
/// bug" claim that the behavior does not actually back.
///
/// It is a source-text assertion (not a behavior test - that is
/// `error_after_embed_exits_cleanly_without_a_teardown_abort`): the deliverable here is the
/// honesty of the DOCUMENTATION, and the guard has to be the words themselves.
#[test]
fn main_exit_path_is_honestly_documented() {
    let src = main_rs_source();

    // Isolate the exit-path teardown comment block: from the `release_ort_runtime` call's
    // documenting comment up to the call itself. Asserting on this slice (not the whole
    // file) keeps the test pointed at the exit path and immune to unrelated edits.
    let call = "rigger::ort_teardown::release_ort_runtime();";
    let call_at = src
        .find(call)
        .expect("main.rs must still call ort_teardown::release_ort_runtime() on the exit path");
    let block_start = src[..call_at]
        .rfind("// Tear the ONNX Runtime / CUDA runtime down EXPLICITLY")
        .expect("the exit-path teardown comment block must precede the release call");
    let block = &src[block_start..call_at];

    // 1. It must NOT overstate. The spec calls out the exact anti-pattern: a claim that the
    //    buggy path is gone. The comment may only use that phrase to DISCLAIM it (". . . NOT
    //    a claim that the buggy path has been removed"), so we reject the bare positive
    //    forms, not every occurrence of the words.
    for overstated in [
        "removes the buggy code path entirely",
        "removes the buggy path entirely",
        "eliminates the bug entirely",
        "the real fix for the intermittent teardown heap corruption",
    ] {
        assert!(
            !block.contains(overstated),
            "the exit-path comment overstates the fix ({overstated:?}); it must describe a \
             scoped mitigation of a live upstream bug, not claim the buggy path is gone"
        );
    }

    // 2. It must state what the mitigation does NOT cover / that the guarantee is bounded.
    //    An honest comment names the residual: the upstream bug is still open and the buggy
    //    code still ships; the win is depriving it of the race, not deleting it.
    assert!(
        block.contains("DOES *NOT*") || block.contains("does not remove"),
        "the exit-path comment must state what the mitigation does NOT cover, not just what \
         it fixes"
    );
    assert!(
        block.contains("remains open")
            || block.contains("still ships")
            || block.contains("live upstream bug"),
        "the exit-path comment must be honest that the upstream bug is not fixed by us - it \
         still exists; we only deprive it of the race"
    );

    // 3. It must state the coverage it DOES have (both exit paths + the no-session no-op) so
    //    "honest" cuts both ways: it neither overstates the fix nor undersells its real scope.
    assert!(
        block.contains("BOTH the success and the error path"),
        "the exit-path comment must state it covers BOTH exit paths (the win over the old \
         `_exit(0)` dodge)"
    );
    assert!(
        block.contains("no-op on any run that never built a GPU/CPU session"),
        "the exit-path comment must state it is a no-op when no session was built"
    );

    // 4. It must point the reader at the module that carries the full soundness argument and
    //    the invariants the conditional guarantee rests on, so the honesty is anchored to the
    //    evidence rather than free-floating.
    assert!(
        block.contains("ort_teardown"),
        "the exit-path comment must reference `ort_teardown` for the full rationale / invariants"
    );
}

/// Scaffold a project whose workflow has TWO independent stages (neither `needs` the
/// other, so both are ready in the first wave) that do no grounder work (`nop`) and
/// never merge (`on_pass: none`). This is the minimal shape that drives `rigger step`
/// into parking a disjoint two-unit wave, offline and deterministic (no model, no git
/// worktrees - the worker's `isolation: none`).
fn write_two_stage_workflow(root: &Path) {
    let rigger = root.join(".rigger");
    std::fs::create_dir_all(rigger.join("agents")).unwrap();
    std::fs::write(
        rigger.join("agents").join("worker.md"),
        "---\nid: worker\nmodel: sonnet\ntools: [Read, Edit]\nisolation: none\n---\nDo the unit.\n",
    )
    .unwrap();
    std::fs::write(
        rigger.join("workflow.yml"),
        r#"name: steptest
defaults:
  grounder: nop
  budget: 60
stages:
  a:
    agent: worker
    on_pass: none
  b:
    agent: worker
    on_pass: none
"#,
    )
    .unwrap();
}

/// Like [`write_two_stage_workflow`] but with a spawn budget of ONE: two independent units
/// are ready in the first wave, so exactly one implementer spawn is admitted and parked and
/// the other is refused - tripping the breaker so `rigger step` reports a halt (Gap 13).
fn write_budget_one_two_stage_workflow(root: &Path) {
    let rigger = root.join(".rigger");
    std::fs::create_dir_all(rigger.join("agents")).unwrap();
    std::fs::write(
        rigger.join("agents").join("worker.md"),
        "---\nid: worker\nmodel: sonnet\ntools: [Read, Edit]\nisolation: none\n---\nDo the unit.\n",
    )
    .unwrap();
    std::fs::write(
        rigger.join("workflow.yml"),
        r#"name: steptest
defaults:
  grounder: nop
  budget: 1
stages:
  a:
    agent: worker
    on_pass: none
  b:
    agent: worker
    on_pass: none
"#,
    )
    .unwrap();
}

/// `rigger step` advances the run one frontier and prints the newly parked spawn WAVE
/// plus a `done` flag as JSON. Two ready units with disjoint blast radii park their
/// spawns in the SAME wave (so fan-out falls out of the run structure); once a courier
/// records each spawn's result, the next step replays past them and reports `done`.
#[test]
fn step_prints_a_disjoint_two_spawn_wave_then_reports_done() {
    let dir = temp_project();
    let root = dir.path();
    write_two_stage_workflow(root);

    // Step 1: both independent units are ready in one wave, so both park their
    // implementer spawns together - a two-spawn wave, and the run is not done.
    let (out, err, ok) = run_rigger(root, &["step"]);
    assert!(ok, "step must succeed; stderr: {err}");
    let line = out.trim();
    assert!(
        line.contains(r#""id":"a/implementer#0""#) && line.contains(r#""id":"b/implementer#0""#),
        "the wave must carry BOTH disjoint units' implementer spawns; got: {line:?}"
    );
    assert_eq!(
        line.matches(r#""id":"#).count(),
        2,
        "exactly the two disjoint units park in one wave; got: {line:?}"
    );
    assert!(
        line.contains(r#""done":false"#),
        "with spawns still awaiting results the run is not done; got: {line:?}"
    );

    // A courier records each spawn's outcome - the `rigger result` channel, simulated
    // here by emitting the SpawnResult event `rigger result` would write to the run
    // stream (that command is a sibling unit).
    for id in ["a/implementer#0", "b/implementer#0"] {
        let (_o, err, ok) = run_rigger(
            root,
            &[
                "emit",
                "SpawnResult",
                &format!(r#"{{"id":"{id}","output":"did {id}"}}"#),
            ],
        );
        assert!(ok, "recording {id}'s result must succeed; stderr: {err}");
    }

    // Step 2: the recorded results replay, the conductor parks nothing new, and the
    // run has reached a fixpoint - an empty wave and done:true.
    let (out, err, ok) = run_rigger(root, &["step"]);
    assert!(ok, "the second step must succeed; stderr: {err}");
    let line = out.trim();
    assert!(
        line.contains(r#""wave":[]"#),
        "a step that parks nothing new prints an empty wave; got: {line:?}"
    );
    assert!(
        line.contains(r#""done":true"#),
        "every spawn now has a result, so the run is done; got: {line:?}"
    );
    // A converged run (budget not tripped) carries NO halt reason: the historical
    // `{"wave":[],"done":true}` wire shape is unchanged, so the driver reads a clean
    // completion, not a loud stop (Gap 13).
    assert!(
        !line.contains("halted"),
        "a converged step must omit the halted field; got: {line:?}"
    );
}

/// Gap 13: a spawn-budget HALT must be LOUD, not indistinguishable from convergence.
/// `rigger step` prints a `halted` reason (distinct from a clean `{"wave":[],"done":true}`)
/// when the breaker trips, so the thin driver stops loudly on a starved run instead of
/// reporting success. Budget 1 with two independent units: one implementer spawn is admitted
/// and parked, the second is refused - the breaker trips and records the halt.
#[test]
fn step_prints_a_budget_halt_reason_when_the_breaker_trips() {
    let dir = temp_project();
    let root = dir.path();
    write_budget_one_two_stage_workflow(root);

    let (out, err, ok) = run_rigger(root, &["step"]);
    // The step process itself SUCCEEDS - it prints its halt on stdout (a halt is a run
    // outcome carried in the JSON, not a process error): the driver reads `halted` and
    // stops loudly, rather than `rigger step` exiting non-zero with no JSON.
    assert!(
        ok,
        "a budget-halted step still prints its result and exits 0; stderr: {err}"
    );
    let line = out.trim();
    assert!(
        line.contains(r#""halted":"budget exhausted: 1/1 spawns""#),
        "a tripped budget must print a halt reason distinct from convergence; got: {line:?}"
    );
}

/// The single-stage liveness workflow the end-to-end tests drive: a per-role wall-clock
/// default so the parked implementer carries a `max_wall_clock` the sweep can time out
/// against, `isolation: none` (no worktree), and `on_pass: none` (no integrate).
fn write_liveness_workflow(root: &Path) {
    let rigger = root.join(".rigger");
    std::fs::create_dir_all(rigger.join("agents")).unwrap();
    std::fs::write(
        rigger.join("agents").join("worker.md"),
        "---\nid: worker\nmodel: sonnet\ntools: [Read, Edit]\nisolation: none\n---\nDo the unit.\n",
    )
    .unwrap();
    std::fs::write(
        rigger.join("workflow.yml"),
        "name: livetest\ndefaults:\n  grounder: nop\n  budget: 60\n  max_wall_clock: 60\nstages:\n  a:\n    agent: worker\n    on_pass: none\n",
    )
    .unwrap();
}

/// Plant a SYNTHETIC STALE MARKER at exactly `marker` (the path the wave carried), touched
/// an hour ago - far past the 60s bound. Backdating the mtime removes any dependence on the
/// test's own wall clock; the sweep reads that mtime.
fn plant_stale_marker(marker: &Path) {
    std::fs::create_dir_all(marker.parent().unwrap()).unwrap();
    std::fs::write(marker, b"heartbeat").unwrap();
    let stale = std::time::SystemTime::now() - std::time::Duration::from_secs(3600);
    std::fs::File::options()
        .write(true)
        .open(marker)
        .unwrap()
        .set_modified(stale)
        .unwrap();
}

/// Agent liveness end-to-end (spec 10, unit 3): a spawn carries a `max_wall_clock` bound;
/// when its per-spawn heartbeat marker goes STALE beyond that bound, `rigger step`
/// classifies it as an infrastructure fault (a HUNG agent) and SURFACES it as a loud halt -
/// so a hung agent can no longer stall the wave invisibly - while charging the unit NO
/// remediation attempt. The marker is planted at the EXACT path the wave carried (the
/// worker-write path == the sweep-read path, BLOCKER-1), and the test drives the no-charge
/// re-park across the step boundary AND the operator recovery (follow-up c).
#[test]
fn step_surfaces_a_hung_spawn_with_a_stale_marker_as_a_liveness_halt() {
    let dir = temp_project();
    let root = dir.path();
    write_liveness_workflow(root);

    // Step 1: the unit is ready, so its implementer parks in-flight (no result yet). The wave
    // carries the RESOLVED marker path the worker would touch - the single authority the sweep
    // also reads, so the test plants the marker exactly where the sweep will look.
    let (out, err, ok) = run_rigger(root, &["step"]);
    assert!(ok, "the first step must succeed; stderr: {err}");
    let line = out.trim();
    assert!(
        line.contains(r#""id":"a/implementer#0""#),
        "step 1 parks the implementer in-flight; got: {line:?}"
    );
    let marker_str =
        json_string_field(line, "marker_path").expect("the wave carries the resolved marker path");
    // Default scratch config: the marker resolves under the repo's own `.rigger/tmp`.
    assert!(
        marker_str.contains("/.rigger/tmp/agent-live/"),
        "the default marker path is under the repo scratch root's agent-live; got: {marker_str:?}"
    );
    let marker = std::path::Path::new(&marker_str);

    // Plant the SYNTHETIC STALE MARKER at the wire path (worker-write path == sweep-read path).
    plant_stale_marker(marker);

    // Step 2: the sweep finds the marker stale beyond the bound, classifies the spawn infra,
    // records the fault on its id, and surfaces it as a loud halt.
    let (out, err, ok) = run_rigger(root, &["step"]);
    assert!(
        ok,
        "a liveness-halted step still prints its result and exits 0; stderr: {err}"
    );
    let line = out.trim();
    assert!(
        line.contains(r#""halted":"#) && line.contains("a/implementer#0"),
        "the hung spawn must be surfaced as a halt naming it; got: {line:?}"
    );
    assert!(
        line.contains("infra") && line.contains("no remediation attempt"),
        "the halt must state infra classification and no-attempt-charged; got: {line:?}"
    );

    // Step 3: re-step WITHOUT recording a result. The hung spawn is already answered by the
    // liveness fault, so it is NOT re-parked/re-run (no dup-exec) - its id must NOT reappear as
    // a fresh wave item - and the halt RE-SURFACES so the stall stays visible every step.
    let (out, err, ok) = run_rigger(root, &["step"]);
    assert!(ok, "the re-step must succeed; stderr: {err}");
    let line = out.trim();
    assert!(
        line.contains(r#""halted":"#) && line.contains("a/implementer#0"),
        "the halt must re-surface on a later step, not silently drop; got: {line:?}"
    );
    assert!(
        json_string_field(line, "marker_path").is_none() && !line.contains(r#""wave":[{"#),
        "the answered hung spawn is not re-run (no fresh wave item / dup-exec); got: {line:?}"
    );

    // Step 4: the operator re-drives the now-healthy agent and records a REAL result. Being
    // last-write-wins, it supersedes the liveness fault.
    let (_o, err, ok) = run_rigger(
        root,
        &["result", "a/implementer#0", "recovered by operator"],
    );
    assert!(ok, "recording a real result must succeed; stderr: {err}");

    // Step 5: the halt CLEARS (no hung spawn remains) and the run converges - the unit proceeds.
    let (out, err, ok) = run_rigger(root, &["step"]);
    assert!(ok, "the recovery step must succeed; stderr: {err}");
    let line = out.trim();
    assert!(
        !line.contains(r#""halted":"#),
        "recording a real result clears the liveness halt; got: {line:?}"
    );
    assert!(
        line.contains(r#""done":true"#),
        "the recovered run converges to a clean fixpoint; got: {line:?}"
    );
}

/// BLOCKER-1 end-to-end: under a NON-default scratch config (`RIGGER_TMPDIR` pointing outside
/// the repo), the marker path the wave carries - the worker-WRITE path - must be the SAME path
/// the sweep READS. A driver that re-hardcoded a `${repo}/.rigger/tmp` root would diverge from
/// the sweep's `scratch_root_from_env` resolution and silently disable liveness. Here the wave's
/// marker path resolves under `RIGGER_TMPDIR`, and planting the stale marker THERE makes the
/// sweep - which resolves the same root - find it and halt, proving write-path == read-path off
/// the non-default root.
#[test]
fn the_liveness_marker_path_follows_a_non_default_scratch_root() {
    let dir = temp_project();
    let root = dir.path();
    write_liveness_workflow(root);
    // A scratch root OUTSIDE the repo - the non-default case the reject named.
    let scratch = tempfile::tempdir().unwrap();
    let scratch_path = scratch.path().to_str().unwrap().to_string();
    let envs: &[(&str, &str)] = &[("RIGGER_TMPDIR", scratch_path.as_str())];

    // Step 1: the wave carries a marker path resolved under RIGGER_TMPDIR, NOT the repo default.
    let (out, err, ok) = run_rigger_envs(root, &["step"], envs);
    assert!(ok, "the first step must succeed; stderr: {err}");
    let line = out.trim();
    let marker_str =
        json_string_field(line, "marker_path").expect("the wave carries the resolved marker path");
    assert!(
        marker_str.starts_with(&scratch_path) && marker_str.contains("/agent-live/"),
        "the marker path must follow RIGGER_TMPDIR, not a hardcoded repo root; got: {marker_str:?}"
    );
    assert!(
        !marker_str.contains("/.rigger/tmp/agent-live/"),
        "under RIGGER_TMPDIR the marker is not under the repo's .rigger/tmp; got: {marker_str:?}"
    );

    // Planting the stale marker at that wire path and re-stepping with the SAME env: the sweep
    // resolves the identical root, reads the marker, and halts - so the worker-write path the
    // wave advertised is exactly the sweep-read path.
    plant_stale_marker(std::path::Path::new(&marker_str));
    let (out, err, ok) = run_rigger_envs(root, &["step"], envs);
    assert!(ok, "the sweep step must succeed; stderr: {err}");
    let line = out.trim();
    assert!(
        line.contains(r#""halted":"#) && line.contains("a/implementer#0"),
        "a stale marker under RIGGER_TMPDIR halts loudly - write-path == read-path; got: {line:?}"
    );
}

/// Run scoping end-to-end (spec 06, unit 1 - Gap 11): a `rigger step` over a store that
/// still holds an UNANSWERED spawn from an OLDER run must never re-print that stale spawn
/// in this run's wave. The prior run's residue sits before this run's `RunStarted`
/// boundary, so scoping the wave to the current run's slice excludes it - the exact
/// zombie-resurrection this unit closes (a prior stepwise run re-parked implementers for
/// aborted runs' units).
#[test]
fn step_scopes_the_wave_to_the_current_run_and_ignores_prior_run_residue() {
    let dir = temp_project();
    let root = dir.path();
    write_two_stage_workflow(root);
    seed_store(root);

    // A prior campaign (DIFFERENT criteria) left an aborted, still-unanswered spawn in the
    // store: its `RunStarted` and a parked implementer with no result.
    let (_o, err, ok) = run_rigger(
        root,
        &[
            "emit",
            "RunStarted",
            r#"{"run":"r0","criteria":["an older spec"]}"#,
        ],
    );
    assert!(
        ok,
        "seeding the prior run's RunStarted must succeed; stderr: {err}"
    );
    let (_o, err, ok) = run_rigger(
        root,
        &[
            "emit",
            "SpawnRequested",
            r#"{"id":"zombie/implementer#0","unit":"zombie","stage":"zombie","prompt":"stale"}"#,
        ],
    );
    assert!(ok, "seeding the stale spawn must succeed; stderr: {err}");

    // This run has no spec criteria, so it is a NEW campaign vs the prior one: the step
    // begins a fresh run and its wave is only THIS run's units.
    let (out, err, ok) = run_rigger(root, &["step"]);
    assert!(ok, "step must succeed; stderr: {err}");
    let line = out.trim();
    assert!(
        line.contains(r#""id":"a/implementer#0""#) && line.contains(r#""id":"b/implementer#0""#),
        "the wave carries this run's two units; got: {line:?}"
    );
    assert!(
        !line.contains("zombie/implementer#0"),
        "the prior run's stale unanswered spawn must NOT reappear in this run's wave; got: {line:?}"
    );
    assert_eq!(
        line.matches(r#""id":"#).count(),
        2,
        "exactly this run's two spawns, never the zombie; got: {line:?}"
    );
}

/// `rigger stats` reports the LATEST run by default and `rigger stats --all` reports the
/// historical aggregate over every run (spec 06, unit 1). Two runs are seeded through the
/// real `rigger emit` courier: run 1 lands one clean unit, run 2 escalates one unit. The
/// default view sees only run 2 (1 of 1 escalated); `--all` sees both (1 of 2).
#[test]
fn stats_reports_the_latest_run_by_default_and_all_for_the_aggregate() {
    let dir = temp_project();
    let root = dir.path();
    seed_store(root);

    // Run 1: one clean unit (started + integrated, never failed).
    for (ty, body) in [
        ("RunStarted", r#"{"run":"r1","criteria":["spec one"]}"#),
        ("UnitStarted", r#"{"id":"u1","agent":"worker"}"#),
        ("UnitIntegrated", r#"{"id":"u1","commit":"aaa"}"#),
        // Run 2: one unit that escalates to a human.
        ("RunStarted", r#"{"run":"r2","criteria":["spec two"]}"#),
        ("UnitStarted", r#"{"id":"u2","agent":"worker"}"#),
        ("UnitEscalated", r#"{"id":"u2"}"#),
    ] {
        let (_o, err, ok) = run_rigger(root, &["emit", ty, body]);
        assert!(ok, "seeding {ty} must succeed; stderr: {err}");
    }

    // Default: only the latest run (run 2) - its single unit escalated.
    let (out, err, ok) = run_rigger(root, &["stats"]);
    assert!(ok, "stats must succeed; stderr: {err}");
    assert!(
        out.contains("(1/1 units escalated"),
        "the default view reports ONLY the latest run (1 of 1 escalated); got:\n{out}"
    );

    // --all: the historical aggregate across both runs - one of two units escalated.
    let (out_all, err, ok) = run_rigger(root, &["stats", "--all"]);
    assert!(ok, "stats --all must succeed; stderr: {err}");
    assert!(
        out_all.contains("(1/2 units escalated"),
        "the --all view aggregates every run (1 of 2 escalated); got:\n{out_all}"
    );

    // A stray argument is still rejected.
    let (_o, _e, ok) = run_rigger(root, &["stats", "--bogus"]);
    assert!(!ok, "an unknown stats argument must be rejected");
}

/// Every event the conductor emits carries the current run id in its metadata, and the
/// run opens with a `RunStarted` carrying a fresh run id (spec 06, unit 1). Drives a real
/// `rigger step`, then reads the store back and asserts the RunStarted, the parked spawn
/// requests, and the unit events all share one run id.
#[test]
fn a_step_stamps_the_run_id_on_the_run_started_and_every_event_it_emits() {
    use rigger::eventstore::sqlite::Store;
    use rigger::eventstore::{Direction, EventStore, Filter};

    let dir = temp_project();
    let root = dir.path();
    write_two_stage_workflow(root);

    let (_out, err, ok) = run_rigger(root, &["step"]);
    assert!(ok, "the step must succeed; stderr: {err}");

    let db_path = root.join(".rigger").join("events.db");
    let backend = Store::open(db_path.to_str().unwrap()).unwrap();
    let events = backend
        .read_all(0, Direction::Forward, &Filter::default())
        .unwrap();

    // Exactly one RunStarted, carrying a fresh run id in both its payload and its metadata.
    let starts: Vec<_> = events
        .iter()
        .filter(|e| e.type_ == rigger::run::TYPE_RUN_STARTED)
        .collect();
    assert_eq!(
        starts.len(),
        1,
        "the run begins with exactly one RunStarted"
    );
    let run_id = starts[0]
        .meta
        .get(rigger::run::META_RUN_ID)
        .expect("the RunStarted carries a run id in metadata")
        .clone();
    assert!(!run_id.is_empty(), "the run id is a fresh, non-empty id");

    // Every conductor-emitted unit event and every parked spawn request carries THAT run id.
    let scoped = ["UnitStarted", "SpawnRequested"];
    let mut checked = 0;
    for e in &events {
        if scoped.contains(&e.type_.as_str()) {
            assert_eq!(
                e.meta.get(rigger::run::META_RUN_ID).map(String::as_str),
                Some(run_id.as_str()),
                "the {} event must carry the current run id",
                e.type_
            );
            checked += 1;
        }
    }
    assert!(
        checked >= 3,
        "the step parked two spawns and started two units, all run-stamped; checked {checked}"
    );
}

/// End-to-end through the CLI seam (spec 05 line 52): a worker records its parked
/// implementer's result with `rigger result <id> --meta '{"resolved_model": ..}'`, and
/// the next `rigger step` replays that spawn and STAMPS the requested model alias plus the
/// worker-reported resolved id onto the unit events the conductor emits for that spawn.
/// Reads the run's `events.db` back through the library to confirm the metadata landed on
/// a real `green` UnitStatus event - not just that the `--meta` was parsed.
#[test]
fn step_result_meta_stamps_the_resolved_model_on_the_replayed_units_events() {
    use rigger::eventstore::sqlite::Store;
    use rigger::eventstore::{Direction, EventStore, Filter};

    let dir = temp_project();
    let root = dir.path();
    write_two_stage_workflow(root);

    // Step 1: both units park their implementer spawns.
    let (out, err, ok) = run_rigger(root, &["step"]);
    assert!(ok, "the first step must succeed; stderr: {err}");
    assert!(
        out.contains(r#""done":false"#),
        "spawns still pending; got: {out:?}"
    );

    // Each worker self-reports via the REAL `rigger result` command, carrying the concrete
    // model it ran as through `--meta` (the mechanism the criterion names).
    let resolved = [
        ("a/implementer#0", "claude-sonnet-4-5-20250101"),
        ("b/implementer#0", "claude-sonnet-4-5-20250929"),
    ];
    for (id, model) in resolved {
        let (_o, err, ok) = run_rigger(
            root,
            &[
                "result",
                id,
                &format!("did {id}"),
                "--meta",
                &format!(r#"{{"resolved_model":"{model}"}}"#),
            ],
        );
        assert!(
            ok,
            "`rigger result {id} --meta` must succeed; stderr: {err}"
        );
    }

    // Step 2: the recorded results replay to a fixpoint.
    let (out, err, ok) = run_rigger(root, &["step"]);
    assert!(ok, "the second step must succeed; stderr: {err}");
    assert!(
        out.contains(r#""done":true"#),
        "every spawn answered; got: {out:?}"
    );

    // Read the run stream back and confirm each unit's `green` event carries the requested
    // alias ("sonnet", from the worker agent) AND the resolved id the worker reported.
    let db_path = root.join(".rigger").join("events.db");
    let backend = Store::open(db_path.to_str().unwrap()).unwrap();
    let events = backend
        .read_all(0, Direction::Forward, &Filter::default())
        .unwrap();
    for (id, model) in resolved {
        let unit = id.split('/').next().unwrap();
        let green = events
            .iter()
            .find(|e| {
                e.type_ == rigger::ledger::TYPE_UNIT_STATUS && {
                    let body = String::from_utf8_lossy(&e.data);
                    body.contains(r#""status":"green""#)
                        && body.contains(&format!(r#""id":"{unit}""#))
                }
            })
            .unwrap_or_else(|| panic!("unit {unit} must have a green status event"));
        assert_eq!(
            green
                .meta
                .get(rigger::conductor::META_MODEL_ALIAS)
                .map(String::as_str),
            Some("sonnet"),
            "unit {unit}'s green event carries the requested alias"
        );
        assert_eq!(
            green
                .meta
                .get(rigger::conductor::META_MODEL_RESOLVED)
                .map(String::as_str),
            Some(model),
            "unit {unit}'s green event carries the worker-reported resolved model"
        );
    }
}

/// End-to-end through the CLI/step seam (spec 10 unit 4): an implementer agent declaring a
/// `model_ladder` parks on - and stamps - the cheap FIRST rung on its first attempt, so the
/// resolved rung is visible in the log the moment the spawn is parked. Reads the run's
/// `events.db` back through the library to confirm BOTH the parked `SpawnRequest`'s model and
/// the `UnitStarted` event's requested alias are the ladder's first rung (not a fixed model).
#[test]
fn step_resolves_the_model_ladders_first_rung_for_the_initial_attempt() {
    use rigger::eventstore::sqlite::Store;
    use rigger::eventstore::{Direction, EventStore, Filter};

    let dir = temp_project();
    let root = dir.path();
    let rigger = root.join(".rigger");
    std::fs::create_dir_all(rigger.join("agents")).unwrap();
    std::fs::write(
        rigger.join("agents").join("worker.md"),
        "---\nid: worker\nmodel_ladder: [haiku, sonnet, opus]\ntools: [Read, Edit]\nisolation: none\n---\nDo the unit.\n",
    )
    .unwrap();
    std::fs::write(
        rigger.join("workflow.yml"),
        "name: laddertest\ndefaults:\n  grounder: nop\n  budget: 60\nstages:\n  a:\n    agent: worker\n    on_pass: none\n",
    )
    .unwrap();

    // One step parks the implementer for unit `a` on its first (attempt-0) spawn.
    let (out, err, ok) = run_rigger(root, &["step"]);
    assert!(ok, "the step must succeed; stderr: {err}");
    assert!(
        out.contains(r#""done":false"#),
        "the implementer spawn is still pending; got: {out:?}"
    );

    let db_path = rigger.join("events.db");
    let backend = Store::open(db_path.to_str().unwrap()).unwrap();
    let events = backend
        .read_all(0, Direction::Forward, &Filter::default())
        .unwrap();

    // The UnitStarted event names the model the first attempt asks for - rung 0 (haiku), NOT
    // a fixed model. A ladder-less `model: haiku` would look identical here; the escalation is
    // pinned by the conductor's ladder-advance-on-retry test.
    let started = events
        .iter()
        .find(|e| e.type_ == rigger::ledger::TYPE_UNIT_STARTED)
        .expect("a UnitStarted must be recorded");
    assert_eq!(
        started
            .meta
            .get(rigger::conductor::META_MODEL_ALIAS)
            .map(String::as_str),
        Some("haiku"),
        "UnitStarted stamps the ladder's first rung as the requested alias"
    );

    // The parked implementer SpawnRequest runs on that same first rung - the model the driver
    // resolved for attempt 0, not the last rung or an empty default.
    let parked = rigger::spawn::recorded(&events).unwrap();
    let req = parked
        .get("a/implementer#0")
        .expect("the implementer spawn must be parked");
    assert_eq!(
        req.model, "haiku",
        "the parked spawn runs on the ladder's first rung"
    );
}

/// `rigger step` rejects an unknown flag with a clear, non-zero error rather than
/// silently running an unconstrained step.
#[test]
fn step_rejects_an_unknown_flag() {
    let dir = temp_project();
    let root = dir.path();
    write_two_stage_workflow(root);

    let (_out, err, ok) = run_rigger(root, &["step", "--nope"]);
    assert!(!ok, "an unknown flag must be a non-zero exit");
    assert!(
        err.contains("unknown flag"),
        "the error must name the unknown flag; got: {err:?}"
    );
}

/// `rigger step --base <ref>` anchors a NEW run branch: it creates the `rigger-run`
/// branch off the base ref and checks it out (so the conductor branches every unit
/// worktree off it), without disturbing the step's `{wave,done}` JSON on stdout.
#[test]
fn step_accepts_base_and_anchors_the_run_branch() {
    let dir = temp_git_project_with_commit();
    let root = dir.path();
    write_two_stage_workflow(root);
    let base_sha =
        git_out(root, &["rev-parse", "HEAD"]).expect("the seeded repo has a HEAD commit");

    let (out, err, ok) = run_rigger(root, &["step", "--base", "HEAD"]);
    assert!(ok, "step --base must succeed; stderr: {err}");

    // --base does not disturb the wave: both disjoint units still park, run not done.
    let line = out.trim();
    assert!(
        line.matches(r#""id":"#).count() == 2 && line.contains(r#""done":false"#),
        "the two-unit wave still parks with --base; got: {line:?}"
    );

    // The run branch was created off the base and checked out.
    assert_eq!(
        git_out(root, &["symbolic-ref", "--short", "-q", "HEAD"]).as_deref(),
        Some("rigger-run"),
        "rigger step --base must create and check out the run branch"
    );
    assert_eq!(
        git_out(root, &["rev-parse", "rigger-run"]).as_deref(),
        Some(base_sha.as_str()),
        "the run branch must be anchored on the --base commit"
    );
}

/// `rigger step --base` with no following ref is a clear, non-zero error, never a
/// silent unconstrained step - matching the `--spec` contract.
#[test]
fn step_rejects_base_without_a_value() {
    let dir = temp_project();
    let root = dir.path();
    write_two_stage_workflow(root);

    let (_out, err, ok) = run_rigger(root, &["step", "--base"]);
    assert!(!ok, "--base without a value must be a non-zero exit");
    assert!(
        err.contains("--base expects a ref"),
        "the error must explain --base needs a ref; got: {err:?}"
    );
}

/// BLOCKER regression: when the base ref does NOT resolve (a repo with no remote, a
/// `master`-default repo, or a pre-fetch clone - the common default `origin/main` case),
/// `rigger step` must still establish the run branch by creating it off HEAD and checking
/// it out, never silently proceed on the operator's own branch (which would let the
/// conductor branch and merge machine-generated units directly onto it). The step still
/// prints its `{wave,done}` JSON on stdout, and warns on stderr that it fell back to HEAD.
#[test]
fn step_creates_run_branch_off_head_when_base_unresolvable() {
    let dir = temp_git_project_with_commit();
    let root = dir.path();
    write_two_stage_workflow(root);
    let head_sha =
        git_out(root, &["rev-parse", "HEAD"]).expect("the seeded repo has a HEAD commit");
    let operator_branch = git_out(root, &["symbolic-ref", "--short", "-q", "HEAD"])
        .expect("the seeded repo is on a named branch");

    // The default-style base that does not exist here.
    let (out, err, ok) = run_rigger(root, &["step", "--base", "origin/does-not-exist"]);
    assert!(
        ok,
        "step must still succeed on an unresolvable base; stderr: {err}"
    );

    // The {wave,done} JSON is undisturbed on stdout.
    let line = out.trim();
    assert!(
        line.matches(r#""id":"#).count() == 2 && line.contains(r#""done":false"#),
        "the two-unit wave still parks despite the base fallback; got: {line:?}"
    );

    // The run branch was created off HEAD (not the operator's branch) and checked out.
    assert_ne!(
        operator_branch, "rigger-run",
        "guard: seed is not already on the run branch"
    );
    assert_eq!(
        git_out(root, &["symbolic-ref", "--short", "-q", "HEAD"]).as_deref(),
        Some("rigger-run"),
        "an unresolvable base must still create and check out the run branch, off HEAD"
    );
    assert_eq!(
        git_out(root, &["rev-parse", "rigger-run"]).as_deref(),
        Some(head_sha.as_str()),
        "the fallback run branch is anchored on the HEAD it was created from"
    );

    // The fallback is announced, not silent.
    assert!(
        err.contains("did not resolve") && err.contains("HEAD"),
        "stderr must announce the HEAD fallback; got: {err:?}"
    );
}

/// An existing run branch is the run's durable anchor: a second `rigger step` REUSES it
/// (never resets it), so an already-integrated commit on `rigger-run` survives, and an
/// EXPLICIT `--base` that would re-anchor it is ignored - with a stderr advisory, never
/// silently - because re-anchoring would orphan the integrated units.
#[test]
fn step_reuses_the_run_branch_and_warns_when_explicit_base_is_ignored() {
    let dir = temp_git_project_with_commit();
    let root = dir.path();
    write_two_stage_workflow(root);

    // First step creates + checks out rigger-run.
    let (_out, err, ok) = run_rigger(root, &["step", "--base", "HEAD"]);
    assert!(ok, "the first step must succeed; stderr: {err}");
    assert_eq!(
        git_out(root, &["symbolic-ref", "--short", "-q", "HEAD"]).as_deref(),
        Some("rigger-run"),
    );

    // Simulate a prior step integrating a unit onto the run branch.
    assert!(
        Command::new("git")
            .args(["commit", "--allow-empty", "-q", "-m", "integrated unit"])
            .current_dir(root)
            .status()
            .expect("git must run")
            .success(),
        "seeding an integrated commit must succeed"
    );
    let integrated_tip =
        git_out(root, &["rev-parse", "rigger-run"]).expect("the run branch has a tip");

    // A second step with an EXPLICIT base pointing elsewhere must reuse rigger-run,
    // preserve the integrated tip, and warn that --base was not applied.
    let (out, err, ok) = run_rigger(root, &["step", "--base", "origin/main"]);
    assert!(ok, "the second step must succeed; stderr: {err}");
    assert!(
        out.trim().contains(r#""wave""#),
        "the second step still prints its {{wave,done}} JSON; got: {out:?}"
    );
    assert_eq!(
        git_out(root, &["rev-parse", "rigger-run"]).as_deref(),
        Some(integrated_tip.as_str()),
        "reuse must NOT reset the run branch - the integrated commit is preserved"
    );
    assert!(
        err.contains("already exists and was reused") && err.contains("NOT applied"),
        "an ignored explicit --base must be announced on stderr; got: {err:?}"
    );
}

/// A throwaway project dir that is deliberately NOT a git repo (no `git init`), so
/// `git_repo()` resolves to empty and the conductor drives a REPO-LESS run. That is the
/// offline shape the stepwise driver's own unit tests use (`repo: String::new()`): with
/// no repo configured, `assert_isolated_cwd` is a no-op, so a reviewer spawn (the
/// adjudicator) parks with an empty working dir instead of being refused for "would run
/// in the main repo checkout". A repo-ful run would instead need real worktrees, and a
/// fabricated `SpawnResult` (no actual diff) would then fail the pre-gate commit with
/// "nothing to commit" - so repo-less is the faithful offline driver for this test.
/// `project_identity()` falls back to the dir basename, which is stable across the
/// step / emit / stats calls this test makes in the same dir.
fn temp_repoless_project() -> tempfile::TempDir {
    tempfile::tempdir().unwrap()
}

/// Scaffold a single-unit workflow whose unit runs a REAL inline gate and reviews itself
/// through an adjudicator - the two event kinds `rigger stats` reports as its gate and
/// review-verdict sections. It is offline and deterministic: the `nop` grounder does no
/// model work, the `check` gate is a trivial `true` shell command the [`ExecRunner`]
/// runs inline (recording a `GateVerdict`), the adjudicator's verdict is supplied via a
/// recorded `SpawnResult`, and `on_pass: none` means the verified+reviewed unit never
/// tries to merge (no git). The implementer and adjudicator spawns are parked by the
/// replay driver and drained by recorded results, exactly like `write_two_stage_workflow`.
fn write_gated_reviewed_workflow(root: &Path) {
    let rigger = root.join(".rigger");
    std::fs::create_dir_all(rigger.join("agents")).unwrap();
    std::fs::write(
        rigger.join("agents").join("worker.md"),
        "---\nid: worker\nmodel: sonnet\ntools: [Read, Edit]\nisolation: none\n---\nImplement the unit.\n",
    )
    .unwrap();
    std::fs::write(
        rigger.join("agents").join("judge.md"),
        "---\nid: judge\nmodel: sonnet\ntools: [Read]\nisolation: none\n---\nAdjudicate the unit.\n",
    )
    .unwrap();
    std::fs::write(
        rigger.join("workflow.yml"),
        r#"name: statstest
defaults:
  grounder: nop
  budget: 60
  review:
    adjudicator: judge
gates:
  check: { run: "true", kind: core }
stages:
  solo:
    agent: worker
    gates: [check]
    on_pass: none
"#,
    )
    .unwrap();
}

/// spec 04, criterion 49: a step-driven run recorded in the event log yields NON-EMPTY
/// gate and review-verdict sections in `rigger stats`. This is the capstone integration
/// proof that closes Gap 3 (the old JS driver under-emitted the vocabulary, blinding
/// `rigger stats`): driving the unit's whole lifecycle through the stepwise conductor -
/// `rigger step` to advance the frontier, `rigger emit SpawnResult` to drain each parked
/// spawn (the `rigger result` channel a courier uses), the inline gate running for real -
/// records the exact `GateVerdict` and `UnitStatus` events the metrics projection folds,
/// so the two sections that were empty under the thin driver are now populated.
#[test]
fn a_step_driven_run_yields_nonempty_gate_and_review_sections_in_stats() {
    let dir = temp_repoless_project();
    let root = dir.path();
    write_gated_reviewed_workflow(root);

    // Step 1: the unit is ready, so its implementer spawn parks at the frontier. The run
    // is not done while a spawn awaits a courier's result.
    let (out, err, ok) = run_rigger(root, &["step"]);
    assert!(ok, "the first step must succeed; stderr: {err}");
    assert!(
        out.contains(r#""id":"solo/implementer#0""#) && out.contains(r#""done":false"#),
        "step 1 parks the implementer and is not done; got: {out:?}"
    );

    // Drain the implementer via a recorded SpawnResult (the `rigger result` channel,
    // simulated here as its sibling command is not on this branch yet - the same
    // substitution `step_prints_a_disjoint_two_spawn_wave_then_reports_done` makes).
    let (_o, err, ok) = run_rigger(
        root,
        &[
            "emit",
            "SpawnResult",
            r#"{"id":"solo/implementer#0","output":"implemented the unit"}"#,
        ],
    );
    assert!(
        ok,
        "recording the implementer result must succeed; stderr: {err}"
    );

    // Step 2: the implementer REPLAYS from the log; the conductor commits (nothing, on the
    // repo-less path), runs the `check` gate inline (recording a passing GateVerdict),
    // emits `verified`, then the three-tier review parks the adjudicator spawn.
    let (out, err, ok) = run_rigger(root, &["step"]);
    assert!(ok, "the second step must succeed; stderr: {err}");
    assert!(
        out.contains(r#""id":"solo/adjudicator#0""#) && out.contains(r#""done":false"#),
        "step 2 replays the implementer, gates the unit, and parks the adjudicator; got: {out:?}"
    );

    // Drain the adjudicator with an APPROVE verdict (the last JSON line `verdict_approves`
    // reads), so the review resolves to an approve and the unit records `reviewed`.
    let (_o, err, ok) = run_rigger(
        root,
        &[
            "emit",
            "SpawnResult",
            r#"{"id":"solo/adjudicator#0","output":"{\"verdict\":\"approve\"}"}"#,
        ],
    );
    assert!(
        ok,
        "recording the adjudicator's approve must succeed; stderr: {err}"
    );

    // Step 3: everything replays - the implementer, the recorded gate verdict (never
    // re-run), and the adjudicator's approve - so the unit reaches `reviewed`. `on_pass:
    // none` means it does not merge, and no new spawn parks, so the run is done.
    let (out, err, ok) = run_rigger(root, &["step"]);
    assert!(ok, "the third step must succeed; stderr: {err}");
    assert!(
        out.contains(r#""wave":[]"#) && out.contains(r#""done":true"#),
        "step 3 replays to a fixpoint: an empty wave and done; got: {out:?}"
    );

    // `rigger stats` folds that recorded run and prints BOTH sections populated.
    let (stats, err, ok) = run_rigger(root, &["stats"]);
    assert!(
        ok,
        "stats over the step-driven run must succeed; stderr: {err}"
    );

    // The GATE section is non-empty: the inline `check` gate ran once and passed, so the
    // per-gate table appears (NOT the "no gate runs recorded" placeholder) and lists it.
    assert!(
        !stats.contains("no gate runs recorded"),
        "a step-driven run recorded a real gate, so the gate section must not be the empty \
         placeholder; got:\n{stats}"
    );
    assert!(
        stats.contains("per-gate runs"),
        "the gate section header must be present; got:\n{stats}"
    );
    let gate_line = stats
        .lines()
        .find(|l| l.contains("check"))
        .unwrap_or_else(|| {
            panic!("the `check` gate must appear in the gate section; got:\n{stats}")
        });
    assert!(
        gate_line.contains("1 pass") && gate_line.contains("1 total"),
        "the `check` gate must show its one passing inline run; got line: {gate_line:?}"
    );

    // The REVIEW-VERDICT section is non-empty: the adjudicator approved, so the review
    // line reports one real verdict (a genuine approve, not the zeroed default).
    let review_line = stats
        .lines()
        .find(|l| l.contains("review"))
        .unwrap_or_else(|| panic!("the review section must appear; got:\n{stats}"));
    assert!(
        review_line.contains("1 approved"),
        "the review-verdict section must record the adjudicator's approve; got line: {review_line:?}"
    );
}

// ---------------------------------------------------------------------------
// `rigger replay <run-id> --against <rev>` - trajectory replay / config eval (spec 13:2)
// ---------------------------------------------------------------------------

/// Drive the offline single-unit baseline run of `write_gated_reviewed_workflow` to
/// completion via the proven step/emit dance (implementer parks -> record it -> gates +
/// review park the adjudicator -> record an approve -> replays to done), recording a REAL
/// trajectory (SpawnResults + a passing GateVerdict + the unit lifecycle) in this project's
/// run stream. Shared by the replay tests, which then re-drive that trajectory.
fn drive_baseline_run(root: &Path) {
    let (out, err, ok) = run_rigger(root, &["step"]);
    assert!(
        ok && out.contains(r#""id":"solo/implementer#0""#),
        "baseline step 1 must park the implementer; stderr:{err} stdout:{out}"
    );
    let (_o, err, ok) = run_rigger(
        root,
        &[
            "emit",
            "SpawnResult",
            r#"{"id":"solo/implementer#0","output":"implemented the unit"}"#,
        ],
    );
    assert!(
        ok,
        "recording the implementer result must succeed; stderr:{err}"
    );
    let (out, err, ok) = run_rigger(root, &["step"]);
    assert!(
        ok && out.contains(r#""id":"solo/adjudicator#0""#),
        "baseline step 2 must park the adjudicator; stderr:{err} stdout:{out}"
    );
    let (_o, err, ok) = run_rigger(
        root,
        &[
            "emit",
            "SpawnResult",
            r#"{"id":"solo/adjudicator#0","output":"{\"verdict\":\"approve\"}"}"#,
        ],
    );
    assert!(
        ok,
        "recording the adjudicator approve must succeed; stderr:{err}"
    );
    let (out, err, ok) = run_rigger(root, &["step"]);
    assert!(
        ok && out.contains(r#""done":true"#),
        "baseline step 3 must replay to done; stderr:{err} stdout:{out}"
    );
}

/// The value columns of a `rigger replay` diff row whose label CONTAINS `needle`: the
/// whitespace-separated tokens after the two-word label, with any trailing `*` change flag
/// dropped. Lets a test assert a metric's baseline/candidate pair without pinning column
/// widths.
fn replay_diff_values(diff: &str, needle: &str) -> Vec<String> {
    let row = diff
        .lines()
        .find(|l| l.contains(needle))
        .unwrap_or_else(|| panic!("the replay diff must carry a {needle:?} row; got:\n{diff}"));
    row.split_whitespace()
        .filter(|t| *t != "*")
        .rev()
        .take(2)
        .map(|t| t.to_string())
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect()
}

/// spec 13, unit 2 done-when: `rigger replay <run-id|latest> --against <rev>` re-drives a
/// completed run's recorded trajectory under a candidate config in an ISOLATED namespace and
/// prints the stats diff against the recorded baseline, NEVER touching the real project
/// streams. Here the candidate rev is HEAD - the very config the run used - so a FAITHFUL
/// re-drive must reproduce the baseline (matching columns), which proves the re-drive
/// actually ran in isolation rather than echoing the baseline twice; the sibling test drives
/// a DIFFERENT config to show the candidate column move.
#[test]
fn replay_re_drives_the_trajectory_and_diffs_stats_without_touching_the_real_stream() {
    // Record the baseline in a REPO-LESS project first (the proven offline step/emit dance,
    // review included - a repo would force worktree isolation on the review agents), THEN
    // turn the dir into a git repo and commit the config, so `--against <rev>` resolves the
    // candidate config at a git rev while the re-drive itself stays repo-less and isolated.
    let dir = temp_repoless_project();
    let root = dir.path();
    write_gated_reviewed_workflow(root);

    drive_baseline_run(root);

    // Make the config readable at a git rev (HEAD == the config the run used). Add only the
    // config, never the runtime events.db, so the checkout `--against` loads is clean.
    git_ok(root, &["init", "-q"]);
    git_ok(root, &["config", "user.email", "t@example.com"]);
    git_ok(root, &["config", "user.name", "t"]);
    git_ok(root, &["add", ".rigger/workflow.yml", ".rigger/agents"]);
    git_ok(root, &["commit", "-q", "-m", "config"]);

    // Capture the recorded baseline stats, to prove the replay leaves them byte-identical.
    let (stats_before, err, ok) = run_rigger(root, &["stats"]);
    assert!(ok, "baseline stats must succeed; stderr:{err}");
    assert!(
        stats_before.contains("1 approved"),
        "the baseline recorded the adjudicator approve; got:\n{stats_before}"
    );

    let (diff, err, ok) = run_rigger(root, &["replay", "latest", "--against", "HEAD"]);
    assert!(
        ok,
        "rigger replay must succeed; stderr:\n{err}\nstdout:\n{diff}"
    );
    assert!(
        diff.contains("replay stats diff")
            && diff.contains("baseline")
            && diff.contains("candidate"),
        "the diff must print a header and both columns; got:\n{diff}"
    );
    // A faithful re-drive against the run's own config reproduces the review outcome AND the
    // gate run - baseline == candidate for both, so the re-drive genuinely re-folded the
    // trajectory under the candidate config (not a printed-baseline-twice no-op: the
    // candidate column is computed from the ISOLATED re-driven stream).
    assert_eq!(
        replay_diff_values(&diff, "review approved"),
        vec!["1".to_string(), "1".to_string()],
        "the faithful re-drive reproduces the one approve in both columns; got:\n{diff}"
    );
    assert_eq!(
        replay_diff_values(&diff, "gate runs"),
        vec!["1".to_string(), "1".to_string()],
        "the faithful re-drive replays the one gate verdict in both columns; got:\n{diff}"
    );
    // The fixture is `on_pass: none` (no git-merge boundary), so a faithful re-drive of the
    // run's OWN config must reproduce EVERY headline metric - not just the two rows spot-checked
    // above. Assert NO row is flagged with `*` (baseline == candidate across all six), pinning
    // the full-column fidelity the test headline claims (sdet-u13r-faithful-replay-spotchecks).
    let flagged: Vec<&str> = diff
        .lines()
        .filter(|l| l.trim_end().ends_with('*'))
        .collect();
    assert!(
        flagged.is_empty(),
        "a faithful HEAD re-drive must flag NO changed row (all six metrics equal); \
         flagged:\n{flagged:?}\nfull diff:\n{diff}"
    );

    // The real project stream is UNTOUCHED: stats after the replay are byte-identical.
    let (stats_after, err, ok) = run_rigger(root, &["stats"]);
    assert!(ok, "post-replay stats must succeed; stderr:{err}");
    assert_eq!(
        stats_after, stats_before,
        "rigger replay must never write the real project run stream"
    );
}

/// A candidate variant of `write_gated_reviewed_workflow` with the review panel REMOVED:
/// the `solo` unit still gates but no adjudicator reviews it. Re-driving the baseline
/// trajectory (which recorded a review approve) under THIS config must drop `review
/// approved` from 1 to 0 - the signal that a config edit changes the re-driven metrics.
fn write_gated_workflow_no_review(root: &Path) {
    std::fs::write(
        root.join(".rigger").join("workflow.yml"),
        r#"name: statstest
defaults:
  grounder: nop
  budget: 60
gates:
  check: { run: "true", kind: core }
stages:
  solo:
    agent: worker
    gates: [check]
    on_pass: none
"#,
    )
    .unwrap();
}

/// spec 13, unit 2: the candidate COLUMN reacts to the config - a config edit measurably
/// changes the re-driven metrics, which is the whole point of the eval ("did that change
/// regress the run?"). Re-driving the same recorded trajectory (a review approve) under a
/// candidate config with the review panel REMOVED drops `review approved` from the recorded
/// 1 to a re-driven 0, proving the candidate column is genuinely re-folded from the isolated
/// re-drive and not a copy of the baseline.
#[test]
fn replay_candidate_column_reacts_to_a_changed_config() {
    let dir = temp_repoless_project();
    let root = dir.path();
    write_gated_reviewed_workflow(root);
    drive_baseline_run(root);

    // Commit the reviewed config, then a review-less variant as HEAD (the candidate rev).
    git_ok(root, &["init", "-q"]);
    git_ok(root, &["config", "user.email", "t@example.com"]);
    git_ok(root, &["config", "user.name", "t"]);
    git_ok(root, &["add", ".rigger/workflow.yml", ".rigger/agents"]);
    git_ok(root, &["commit", "-q", "-m", "reviewed config"]);
    write_gated_workflow_no_review(root);
    git_ok(root, &["add", ".rigger/workflow.yml"]);
    git_ok(root, &["commit", "-q", "-m", "review removed"]);

    let (diff, err, ok) = run_rigger(root, &["replay", "latest", "--against", "HEAD"]);
    assert!(
        ok,
        "rigger replay must succeed; stderr:\n{err}\nstdout:\n{diff}"
    );
    // Baseline recorded one approve; the review-less candidate re-drives to zero approves.
    assert_eq!(
        replay_diff_values(&diff, "review approved"),
        vec!["1".to_string(), "0".to_string()],
        "removing review must move the candidate column from the baseline 1 to 0; got:\n{diff}"
    );
    // The changed row is flagged with the `*` marker so a reader spots the regression.
    let review_row = diff
        .lines()
        .find(|l| l.contains("review approved"))
        .unwrap();
    assert!(
        review_row.trim_end().ends_with('*'),
        "a changed metric row is flagged with `*`; got row: {review_row:?}"
    );
}

/// A candidate variant of `write_gated_reviewed_workflow` with the `check` GATE removed from
/// the `solo` stage (the review panel is kept). Re-driving the baseline trajectory (which
/// recorded one passing gate verdict) under THIS config must drop `gate runs` from 1 to 0 -
/// the re-drive's `run_gates` never iterates a gate the stage no longer lists, so its seeded
/// verdict is not reached.
fn write_reviewed_workflow_no_gate(root: &Path) {
    std::fs::write(
        root.join(".rigger").join("workflow.yml"),
        r#"name: statstest
defaults:
  grounder: nop
  budget: 60
  review:
    adjudicator: judge
stages:
  solo:
    agent: worker
    gates: []
    on_pass: none
"#,
    )
    .unwrap();
}

/// spec 13, unit 2 (adj u13 remediation #1): the candidate "gate runs" column must reflect the
/// CANDIDATE config, not echo the seeded baseline. Re-driving a trajectory that recorded ONE
/// passing gate under a candidate config that REMOVED that gate drops `gate runs` from the
/// recorded 1 to a re-driven 0 - proving the candidate column counts only the gates the
/// re-drive actually reaches, not the raw trajectory seed. Before the fix this row echoed the
/// baseline (candidate = 1 for a gate-less config), shipping a false contract.
#[test]
fn replay_removing_a_gate_lowers_the_candidate_gate_runs() {
    let dir = temp_repoless_project();
    let root = dir.path();
    write_gated_reviewed_workflow(root);
    drive_baseline_run(root);

    // Commit the gated config, then a gate-less variant as HEAD (the candidate rev).
    git_ok(root, &["init", "-q"]);
    git_ok(root, &["config", "user.email", "t@example.com"]);
    git_ok(root, &["config", "user.name", "t"]);
    git_ok(root, &["add", ".rigger/workflow.yml", ".rigger/agents"]);
    git_ok(root, &["commit", "-q", "-m", "gated config"]);
    write_reviewed_workflow_no_gate(root);
    git_ok(root, &["add", ".rigger/workflow.yml"]);
    git_ok(root, &["commit", "-q", "-m", "gate removed"]);

    let (diff, err, ok) = run_rigger(root, &["replay", "latest", "--against", "HEAD"]);
    assert!(
        ok,
        "rigger replay must succeed; stderr:\n{err}\nstdout:\n{diff}"
    );
    // The whole point: removing the gate lowers the candidate gate-runs column to 0.
    assert_eq!(
        replay_diff_values(&diff, "gate runs"),
        vec!["1".to_string(), "0".to_string()],
        "removing the gate must drop the candidate `gate runs` from the baseline 1 to 0, not \
         echo the seeded verdict; got:\n{diff}"
    );
    let gate_row = diff.lines().find(|l| l.contains("gate runs")).unwrap();
    assert!(
        gate_row.trim_end().ends_with('*'),
        "the changed gate-runs row is flagged with `*`; got row: {gate_row:?}"
    );
    // Only the gate column moved: the review panel is kept, so its approve stays 1 in BOTH
    // columns (the re-scoping drops the removed gate, never the rest of the candidate metrics).
    assert_eq!(
        replay_diff_values(&diff, "review approved"),
        vec!["1".to_string(), "1".to_string()],
        "removing only the gate must leave the kept review panel's approve unchanged; got:\n{diff}"
    );
}

/// A candidate variant that ADDS a gate (`extra`) the baseline trajectory never ran, alongside
/// the recorded `check`. The re-drive replays `check` from its seeded verdict but has NO
/// recorded verdict for `extra`, so `ReplayRunner` answers it FAIL-SAFE (never a fabricated
/// pass) - the `solo` unit's gates fail and it cannot integrate first-pass.
fn write_reviewed_workflow_added_gate(root: &Path) {
    std::fs::write(
        root.join(".rigger").join("workflow.yml"),
        r#"name: statstest
defaults:
  grounder: nop
  budget: 60
  review:
    adjudicator: judge
gates:
  check: { run: "true", kind: core }
  extra: { run: "true", kind: core }
stages:
  solo:
    agent: worker
    gates: [check, extra]
    on_pass: none
"#,
    )
    .unwrap();
}

/// spec 13, unit 2 (sdet-u13r-replayrunner-failsafe): a candidate config that ADDS a gate the
/// baseline trajectory never recorded must FAIL SAFE - `ReplayRunner` never fabricates a pass
/// for an unscored gate, so the unit does not proceed on a made-up green. Re-driving under a
/// config with an extra, never-recorded gate leaves the added gate RED, so the `solo` unit
/// never clears its gates, never reaches review, and its `review approved` drops from the
/// baseline 1 to a candidate 0. Mutating `ReplayRunner`'s `pass: false` to `true` would
/// fabricate the pass, let the unit reach review, and restore the approve to 1 - so this
/// assertion pins the fail-safe guard. The candidate also folds BOTH gates into `gate runs`
/// (2: the replayed `check` plus the fail-safe `extra`).
#[test]
fn replay_an_added_gate_fails_safe_and_never_fabricates_a_pass() {
    let dir = temp_repoless_project();
    let root = dir.path();
    write_gated_reviewed_workflow(root);
    drive_baseline_run(root);

    git_ok(root, &["init", "-q"]);
    git_ok(root, &["config", "user.email", "t@example.com"]);
    git_ok(root, &["config", "user.name", "t"]);
    git_ok(root, &["add", ".rigger/workflow.yml", ".rigger/agents"]);
    git_ok(root, &["commit", "-q", "-m", "gated config"]);
    write_reviewed_workflow_added_gate(root);
    git_ok(root, &["add", ".rigger/workflow.yml"]);
    git_ok(root, &["commit", "-q", "-m", "gate added"]);

    let (diff, err, ok) = run_rigger(root, &["replay", "latest", "--against", "HEAD"]);
    assert!(
        ok,
        "rigger replay must succeed (a fail-safe gate halts the unit, it does not error the \
         command); stderr:\n{err}\nstdout:\n{diff}"
    );
    // The baseline unit cleared its one gate and got its approve (review approved = 1). The
    // candidate's added `extra` gate is red (fail-safe), so the unit never clears its gates,
    // never reaches review, and the candidate approve collapses to 0 - NOT a fabricated pass.
    // (Were ReplayRunner to fabricate a pass, the unit would reach review and the approve would
    // stay 1, so this pins the guard.)
    assert_eq!(
        replay_diff_values(&diff, "review approved"),
        vec!["1".to_string(), "0".to_string()],
        "an added, never-recorded gate must fail safe and block the unit from review (approve \
         1 -> 0), never fabricate a pass; got:\n{diff}"
    );
    // Both gates are folded into the candidate `gate runs` (the replayed `check` + the fail-safe
    // `extra`), so the added gate is genuinely reached and scored, not silently skipped.
    assert_eq!(
        replay_diff_values(&diff, "gate runs"),
        vec!["1".to_string(), "2".to_string()],
        "the candidate folds both the replayed and the fail-safe added gate; got:\n{diff}"
    );
}

/// A candidate variant that adds a SECOND, independent stage (`probe`) whose implementer spawn
/// the baseline trajectory never recorded. The re-drive replays `solo` fully but PARKS `probe`
/// (no recorded result to answer it), so the candidate column is partial and honest.
fn write_reviewed_workflow_extra_stage(root: &Path) {
    std::fs::write(
        root.join(".rigger").join("workflow.yml"),
        r#"name: statstest
defaults:
  grounder: nop
  budget: 60
  review:
    adjudicator: judge
gates:
  check: { run: "true", kind: core }
stages:
  solo:
    agent: worker
    gates: [check]
    on_pass: none
  probe:
    agent: worker
    on_pass: none
"#,
    )
    .unwrap();
}

/// spec 13, unit 2 (sdet-u13r-incomplete-drive-honest-park): a candidate config that introduces
/// a spawn the trajectory never recorded PARKS honestly rather than fabricating a result - the
/// re-drive stops where the recorded behaviour runs out, and the diff still prints a partial,
/// honestly-labelled candidate column. Here the candidate adds an independent `probe` stage: the
/// baseline started ONE unit, the re-drive starts TWO (solo replays, probe parks), so the diff
/// prints with the candidate `units started` at 2 - the partial column the contract promises.
#[test]
fn replay_an_uncovered_candidate_spawn_parks_and_still_prints_a_partial_column() {
    let dir = temp_repoless_project();
    let root = dir.path();
    write_gated_reviewed_workflow(root);
    drive_baseline_run(root);

    git_ok(root, &["init", "-q"]);
    git_ok(root, &["config", "user.email", "t@example.com"]);
    git_ok(root, &["config", "user.name", "t"]);
    git_ok(root, &["add", ".rigger/workflow.yml", ".rigger/agents"]);
    git_ok(root, &["commit", "-q", "-m", "single-stage config"]);
    write_reviewed_workflow_extra_stage(root);
    git_ok(root, &["add", ".rigger/workflow.yml"]);
    git_ok(root, &["commit", "-q", "-m", "extra stage added"]);

    let (diff, err, ok) = run_rigger(root, &["replay", "latest", "--against", "HEAD"]);
    assert!(
        ok,
        "rigger replay must succeed even when a candidate spawn parks; stderr:\n{err}\nstdout:\n{diff}"
    );
    // The diff still prints a full header + both columns despite the uncovered `probe` parking.
    assert!(
        diff.contains("replay stats diff") && diff.contains("baseline") && diff.contains("candidate"),
        "the diff must still print when the candidate re-drive parks an uncovered spawn; got:\n{diff}"
    );
    // The baseline started one unit; the candidate started two (solo replayed, probe parked) -
    // the partial, honestly-labelled candidate column the honest-park contract promises.
    assert_eq!(
        replay_diff_values(&diff, "units started"),
        vec!["1".to_string(), "2".to_string()],
        "the candidate column reflects the uncovered `probe` stage starting (then parking); got:\n{diff}"
    );
}

/// Every file path under `dir`, recursively, as strings - so a test can assert the scratch root
/// carries no leaked sqlite artifact after a replay.
fn files_under(dir: &Path) -> Vec<String> {
    let mut out = Vec::new();
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                out.extend(files_under(&path));
            } else {
                out.push(path.to_string_lossy().into_owned());
            }
        }
    }
    out
}

/// spec 13, unit 2 (adv-u13r-replay-scratch-wal-shm-leak): `rigger replay` must leave NO sqlite
/// artifact under the scratch root. The isolated re-drive opens a WAL-mode sqlite, which keeps
/// `.db-wal` / `.db-shm` sidecars open beside the `.db`; the store is dropped (closed) and its
/// whole throwaway db subdir removed wholesale, so a replay leaks nothing that accumulates in
/// `.rigger/tmp` on every run. Before the fix only the `.db` was unlinked (while the store was
/// still open), leaking both sidecars.
#[test]
fn replay_leaves_no_sqlite_artifact_in_the_scratch_root() {
    let dir = temp_repoless_project();
    let root = dir.path();
    write_gated_reviewed_workflow(root);
    drive_baseline_run(root);

    git_ok(root, &["init", "-q"]);
    git_ok(root, &["config", "user.email", "t@example.com"]);
    git_ok(root, &["config", "user.name", "t"]);
    git_ok(root, &["add", ".rigger/workflow.yml", ".rigger/agents"]);
    git_ok(root, &["commit", "-q", "-m", "config"]);

    let (_diff, err, ok) = run_rigger(root, &["replay", "latest", "--against", "HEAD"]);
    assert!(ok, "rigger replay must succeed; stderr:\n{err}");

    // The scratch root is `<repo>/.rigger/tmp`. After the replay no sqlite file (the db or its
    // WAL/SHM sidecars) and no `rigger-replay-*` scratch dir may survive.
    let scratch = root.join(".rigger").join("tmp");
    let leaked: Vec<String> = files_under(&scratch)
        .into_iter()
        .filter(|p| {
            p.ends_with(".db")
                || p.ends_with(".db-wal")
                || p.ends_with(".db-shm")
                || p.contains("rigger-replay-")
        })
        .collect();
    assert!(
        leaked.is_empty(),
        "rigger replay must remove its whole scratch db subdir (db + WAL + SHM); leaked:\n{leaked:?}"
    );
}

// ---------------------------------------------------------------------------
// `rigger validate` install-drift + uncommitted-.rigger advisories (spec 05:55)
// ---------------------------------------------------------------------------

/// Clause (a) of spec 05:55: `rigger validate` WARNS (on stderr, without failing) when
/// the installed `.claude/workflows/rigger.js` has drifted from the binary's embedded
/// copy, and stays SILENT when the two are identical. A stale installed workflow (e.g.
/// after a `rigger` upgrade with no re-`setup`) is surfaced, not discovered by accident.
#[test]
fn validate_warns_when_the_installed_workflow_drifts_from_the_embedded_copy() {
    let dir = temp_project();
    let root = dir.path();

    // `rigger setup` scaffolds a valid config AND installs the workflow byte-identical
    // to the embedded copy. Stub npm so the shim's install is a no-op.
    let (_out, err, ok) = run_rigger_envs(root, &["setup"], &[("RIGGER_NPM", "true")]);
    assert!(ok, "rigger setup must succeed; stderr:\n{err}");

    // Identical installed vs embedded -> validate is drift-SILENT and succeeds.
    let (out, err, ok) = run_rigger(root, &["validate"]);
    assert!(
        ok,
        "validate must succeed on a clean project; stderr:\n{err}"
    );
    assert!(
        out.contains("config valid"),
        "validate must still print its config summary; stdout:\n{out}"
    );
    assert!(
        !err.to_lowercase().contains("drift"),
        "validate must NOT warn about drift when the installed workflow matches the \
         embedded copy; stderr:\n{err}"
    );

    // Drift the installed workflow, then validate must WARN on stderr but still exit 0.
    let installed = root.join(".claude").join("workflows").join("rigger.js");
    std::fs::write(&installed, "// drifted from the embedded workflow\n").unwrap();
    let (_out, err, ok) = run_rigger(root, &["validate"]);
    assert!(
        ok,
        "validate must still succeed (exit 0) when it only WARNS about drift; stderr:\n{err}"
    );
    assert!(
        err.to_lowercase().contains("drift") && err.contains(".claude/workflows/rigger.js"),
        "validate must warn on stderr that the installed workflow drifted from the \
         embedded copy, naming the workflow file; stderr:\n{err}"
    );
}

/// Clause (b) of spec 05:55: `rigger validate` FLAGS tracked `.rigger/` files that carry
/// uncommitted modifications (a stderr advisory, exit 0), and stays SILENT when the
/// tracked `.rigger/` state is clean.
#[test]
fn validate_flags_tracked_rigger_files_with_uncommitted_modifications() {
    let dir = temp_git_project_with_commit();
    let root = dir.path();

    // Scaffold a valid config (npm-free) and commit it so `.rigger/` is tracked+clean.
    let (_out, err, ok) = run_rigger(root, &["init"]);
    assert!(ok, "rigger init must succeed; stderr:\n{err}");
    git_ok(root, &["add", "-A"]);
    git_ok(root, &["commit", "-q", "-m", "scaffold"]);

    // Clean tracked `.rigger/` -> validate is SILENT on the uncommitted advisory.
    let (out, err, ok) = run_rigger(root, &["validate"]);
    assert!(ok, "validate must succeed; stderr:\n{err}");
    assert!(
        out.contains("config valid"),
        "validate must print its config summary; stdout:\n{out}"
    );
    assert!(
        !err.contains(".rigger/workflow.yml"),
        "validate must NOT flag a clean tracked `.rigger/` tree; stderr:\n{err}"
    );

    // Modify a TRACKED `.rigger/` file (a YAML comment keeps the config valid), leaving
    // it uncommitted -> validate must FLAG it on stderr but still exit 0.
    {
        use std::io::Write;
        let mut wf = std::fs::OpenOptions::new()
            .append(true)
            .open(root.join(".rigger").join("workflow.yml"))
            .unwrap();
        writeln!(wf, "# locally edited, not committed").unwrap();
    }
    let (_out, err, ok) = run_rigger(root, &["validate"]);
    assert!(
        ok,
        "validate must still succeed (exit 0) when it only FLAGS uncommitted `.rigger/` \
         changes; stderr:\n{err}"
    );
    assert!(
        err.contains(".rigger/workflow.yml") && err.to_lowercase().contains("uncommitted"),
        "validate must flag the tracked-but-modified `.rigger/workflow.yml` on stderr; \
         stderr:\n{err}"
    );
}

/// Spec 06 done-when line 60 (Gap 14d): `rigger validate` reports residue - scratch
/// worktrees with no live unit, orphaned build caches, shadow stores, and `rigger/u/*`
/// branches with no live unit - each with a size, as warnings that NEVER fail validation
/// and NEVER delete anything. Driving the real binary is the only way to prove the store +
/// git + filesystem read wiring; the pure scan is unit-tested in `src/main.rs`.
#[test]
fn validate_reports_scratch_residue_with_sizes_as_a_non_failing_warning() {
    let dir = temp_git_project_with_commit();
    let root = dir.path();

    // A valid, committed config so `.rigger/` is tracked+clean (no unrelated advisories),
    // and a seeded store so `validate` has a run stream to read the LIVE unit set from.
    let (_out, err, ok) = run_rigger(root, &["init"]);
    assert!(ok, "rigger init must succeed; stderr:\n{err}");
    git_ok(root, &["add", "-A"]);
    git_ok(root, &["commit", "-q", "-m", "scaffold"]);
    seed_store(root); // empty store -> zero live units -> leftovers read as residue

    // Point the scratch root at a dir we control, so the scan is hermetic.
    let scratch = root.join("scratchroot");
    let tmp = scratch.to_str().unwrap();

    // Clean scratch (no worktrees/caches/shadow stores) + no dead branches -> validate is
    // residue-SILENT and still succeeds.
    std::fs::create_dir_all(&scratch).unwrap();
    let (out, err, ok) = run_rigger_envs(root, &["validate"], &[("RIGGER_TMPDIR", tmp)]);
    assert!(
        ok,
        "validate must succeed on a clean scratch root; stderr:\n{err}"
    );
    assert!(
        out.contains("config valid"),
        "validate must still print its config summary; stdout:\n{out}"
    );
    assert!(
        !err.to_lowercase().contains("residue"),
        "validate must be residue-silent when the scratch root is clean; stderr:\n{err}"
    );

    // Now plant residue: a leftover unit worktree (with a shadow store inside it), an
    // orphaned build cache, a standalone shadow store, and a dead `rigger/u/*` branch.
    let ghost_wt = scratch.join("rigger-wt-unit-99-ghost-12345678");
    std::fs::create_dir_all(ghost_wt.join(".rigger")).unwrap();
    std::fs::write(ghost_wt.join("payload.bin"), [0u8; 4096]).unwrap();
    std::fs::write(ghost_wt.join(".rigger").join("events.db"), b"shadow").unwrap();
    std::fs::create_dir_all(scratch.join("cargo-target")).unwrap();
    std::fs::write(scratch.join("cargo-target").join("x.rlib"), [0u8; 2048]).unwrap();
    std::fs::create_dir_all(scratch.join("probe").join(".rigger")).unwrap();
    std::fs::write(
        scratch.join("probe").join(".rigger").join("events.db"),
        b"s2",
    )
    .unwrap();
    git_ok(root, &["branch", "rigger/u/unit-99-ghost"]);

    let (out, err, ok) = run_rigger_envs(root, &["validate"], &[("RIGGER_TMPDIR", tmp)]);
    assert!(
        ok,
        "validate must still exit 0 when it only WARNS about residue; stderr:\n{err}"
    );
    assert!(
        out.contains("config valid"),
        "validate must still print its config summary; stdout:\n{out}"
    );
    assert!(
        err.to_lowercase().contains("residue"),
        "validate must warn about residue on stderr; stderr:\n{err}"
    );
    assert!(
        err.contains("rigger-wt-unit-99-ghost-12345678"),
        "the leftover worktree must be named; stderr:\n{err}"
    );
    assert!(
        err.contains("cargo-target"),
        "the orphaned build cache must be named; stderr:\n{err}"
    );
    assert!(
        err.contains("probe/.rigger/events.db"),
        "the standalone shadow store must be named; stderr:\n{err}"
    );
    assert!(
        err.contains("rigger/u/unit-99-ghost"),
        "the dead `rigger/u/*` branch must be named; stderr:\n{err}"
    );
    // Sizes accompany the disk-bearing items (a parenthesized human size).
    assert!(
        err.contains("(4.0K)") || err.contains("(4.5K)"),
        "the leftover worktree must carry a size; stderr:\n{err}"
    );
}

/// Spec 06 done-when line 50 / unit desc line 30 (Gap 14d, CURRENT-run clause): residue is
/// scoped to the CURRENT run. A PRIOR run's abandoned, still-non-terminal unit - which an
/// UNSCOPED ledger fold reads as LIVE - must be surfaced as residue on BOTH sub-clauses (its
/// `rigger-wt-*` worktree AND its `rigger/u/*` branch), while THIS run's in-flight unit is
/// spared on both. This drives the real store + git + filesystem wiring end to end; reverting
/// the `runscope::current_run` scoping (so the fold spans every run) reddens it, because the
/// prior unit would then fold as live and its leftovers would be spared.
#[test]
fn validate_scopes_residue_to_the_current_run_flagging_a_prior_runs_abandoned_unit() {
    let dir = temp_git_project_with_commit();
    let root = dir.path();

    let (_out, err, ok) = run_rigger(root, &["init"]);
    assert!(ok, "rigger init must succeed; stderr:\n{err}");
    git_ok(root, &["add", "-A"]);
    git_ok(root, &["commit", "-q", "-m", "scaffold"]);
    seed_store(root);

    // Two runs recorded through the real courier: a PRIOR run whose `unit-old` never reached
    // a terminal state (abandoned mid-flight), then the CURRENT run with an in-flight
    // `unit-new`. `current_run` folds only the slice after the SECOND `RunStarted`, so
    // `unit-old` is not live in this run.
    for (ty, body) in [
        ("RunStarted", r#"{"run":"r0","criteria":["prior spec"]}"#),
        (
            "UnitStarted",
            r#"{"id":"unit-old","branch":"rigger/u/unit-old"}"#,
        ),
        ("RunStarted", r#"{"run":"r1","criteria":["current spec"]}"#),
        (
            "UnitStarted",
            r#"{"id":"unit-new","branch":"rigger/u/unit-new"}"#,
        ),
    ] {
        let (_o, e, ok) = run_rigger(root, &["emit", ty, body]);
        assert!(ok, "seeding {ty} must succeed; stderr:\n{e}");
    }

    // Hermetic scratch root: a deterministic worktree for EACH unit, plus a local branch for
    // each. Only the prior run's leftovers are residue.
    let scratch = root.join("scratchroot");
    let tmp = scratch.to_str().unwrap();
    for wt in ["rigger-wt-unit-old", "rigger-wt-unit-new"] {
        std::fs::create_dir_all(scratch.join(wt)).unwrap();
        std::fs::write(scratch.join(wt).join("payload.bin"), [0u8; 4096]).unwrap();
    }
    git_ok(root, &["branch", "rigger/u/unit-old"]);
    git_ok(root, &["branch", "rigger/u/unit-new"]);

    let (out, err, ok) = run_rigger_envs(root, &["validate"], &[("RIGGER_TMPDIR", tmp)]);
    assert!(
        ok,
        "validate only WARNS about residue, still exits 0; stderr:\n{err}"
    );
    assert!(
        out.contains("config valid"),
        "validate still prints its config summary; stdout:\n{out}"
    );

    // The PRIOR run's abandoned unit is residue on BOTH sub-clauses.
    assert!(
        err.contains("rigger-wt-unit-old"),
        "a prior run's abandoned worktree must be flagged as residue; stderr:\n{err}"
    );
    assert!(
        err.contains("rigger/u/unit-old"),
        "a prior run's abandoned branch must be flagged as residue; stderr:\n{err}"
    );
    // THIS run's in-flight unit is spared on BOTH sub-clauses (`rigger/u/unit-new` is not a
    // substring of `rigger/u/unit-old`, so these assertions are independent).
    assert!(
        !err.contains("rigger-wt-unit-new"),
        "the current run's live worktree must NOT be flagged; stderr:\n{err}"
    );
    assert!(
        !err.contains("rigger/u/unit-new"),
        "the current run's live branch must NOT be flagged; stderr:\n{err}"
    );
}

/// Spec 05 done-when line 57, clause 2: the empty-repo scaffold path must print a
/// pointer to the agency-agents collection AND the authoring-agents handbook chapter,
/// and that pointer must appear ONLY when the default fleet is actually scaffolded -
/// never on a re-run that keeps an existing fleet. Driving the real `rigger init`
/// binary is the only way to observe the printed pointer; no cargo gate can see it,
/// which is exactly why clause 2 was previously shipped unimplemented behind green
/// gates.
#[test]
fn empty_repo_scaffold_path_prints_the_agent_collection_pointer() {
    const COLLECTION_URL: &str = "github.com/msitarzewski/agency-agents";
    const HANDBOOK: &str = "docs/handbook/authoring-agents.md";

    let dir = temp_project();
    let root = dir.path();

    // First `init` on an empty repo actually scaffolds the default fleet, so the
    // scaffold path must point the user at where to get a real fleet and how to
    // author agents.
    let (out, err, ok) = run_rigger(root, &["init"]);
    assert!(
        ok,
        "rigger init must succeed on an empty repo; stderr:\n{err}"
    );
    assert!(
        out.contains(COLLECTION_URL),
        "the scaffold path must point at the agency-agents collection ({COLLECTION_URL}); got:\n{out}"
    );
    assert!(
        out.contains(HANDBOOK),
        "the scaffold path must point at the authoring-agents handbook chapter ({HANDBOOK}); got:\n{out}"
    );

    // A second `init` over the now-existing fleet keeps every agent file (scaffolds
    // nothing new), so the pointer must be ABSENT - it belongs to the empty-repo path
    // only. This is the discriminating half: a regression that always printed the
    // pointer would pass the first assertion but fail here.
    let (out2, err2, ok2) = run_rigger(root, &["init"]);
    assert!(ok2, "a re-run of rigger init must succeed; stderr:\n{err2}");
    assert!(
        !out2.contains(COLLECTION_URL),
        "the collection pointer must not print when scaffolding is skipped; got:\n{out2}"
    );
    assert!(
        !out2.contains(HANDBOOK),
        "the handbook pointer must not print when scaffolding is skipped; got:\n{out2}"
    );
}

/// Spec 08 item 3: `rigger init` reports a POSITIVE per-artifact summary of what it
/// scaffolded on the first run, then is a QUIET no-op on a rerun - it confirms the
/// already-initialized state without re-narrating any scaffold action it did not perform.
#[test]
fn init_reports_the_positive_summary_then_is_a_quiet_noop() {
    let dir = temp_project();
    let root = dir.path();

    // First init on an empty repo scaffolds the fleet and NARRATES what it wrote.
    let (out, err, ok) = run_rigger(root, &["init"]);
    assert!(
        ok,
        "rigger init must succeed on an empty repo; stderr:\n{err}"
    );
    assert!(
        out.contains("scaffolded .rigger/workflow.yml"),
        "the first init reports the workflow it scaffolded; got:\n{out}"
    );
    assert!(
        out.contains("scaffolded .rigger/agents/"),
        "the first init reports the agents it scaffolded; got:\n{out}"
    );

    // A rerun changes nothing: a quiet no-op that reports already-initialized and does
    // NOT re-narrate any scaffold action.
    let (out2, err2, ok2) = run_rigger(root, &["init"]);
    assert!(ok2, "a rerun of rigger init must succeed; stderr:\n{err2}");
    assert!(
        out2.contains("already initialized"),
        "a rerun reports the already-initialized no-op; got:\n{out2}"
    );
    assert!(
        !out2.contains("scaffolded"),
        "a rerun must NOT re-narrate any scaffold action; got:\n{out2}"
    );
}

/// Spec 08 item 3: a `--agents` import is a REQUESTED change and is REPORTED even on an
/// otherwise up-to-date repo - it runs before the silent-no-op check, so importing onto a
/// repo where the scaffold, workflow, and shim are all no-ops is never silently skipped.
#[test]
fn setup_agents_import_is_reported_even_when_nothing_else_drifted() {
    let dir = temp_project();
    let root = dir.path();

    // Bring the repo fully up to date: scaffold + install workflow + provision the shim
    // (npm stubbed to a no-op). Then mark the shim install COMPLETE so a re-run's
    // provision step is itself a no-op.
    let (_out, err, ok) = run_rigger_envs(root, &["setup"], &[("RIGGER_NPM", "true")]);
    assert!(ok, "the initial rigger setup must succeed; stderr:\n{err}");
    let marker = root
        .join(".rigger")
        .join("shim")
        .join("node_modules")
        .join(".package-lock.json");
    std::fs::create_dir_all(marker.parent().unwrap()).unwrap();
    std::fs::write(&marker, "{}").unwrap();

    // A local collection to import from (a foreign `name:` identity field).
    let src = root.join("collection");
    std::fs::create_dir_all(&src).unwrap();
    std::fs::write(
        src.join("researcher.md"),
        "---\nname: researcher\nmodel: sonnet\n---\nYou research prior art.\n",
    )
    .unwrap();

    // Re-run setup with --agents on the now up-to-date repo: scaffold/workflow/shim are all
    // no-ops, but the import must still be reported.
    let (out, err, ok) = run_rigger_envs(
        root,
        &["setup", "--agents", src.to_str().unwrap()],
        &[("RIGGER_NPM", "true")],
    );
    assert!(
        ok,
        "setup --agents must succeed on an up-to-date repo; stderr:\n{err}"
    );
    assert!(
        out.contains("imported") && out.contains("researcher.md"),
        "the --agents import must be reported even when nothing else drifted; got:\n{out}"
    );
    assert!(
        root.join(".rigger/agents/researcher.md").exists(),
        "the agent was actually imported into .rigger/agents/"
    );
}

/// `rigger result <id> --if-absent` records a died-worker outcome only when the spawn is
/// still unanswered: on a fresh run stream it writes the result and exits 0, so `rigger
/// reported <id>` then confirms the spawn is answered. The "records when absent" half of
/// the atomic guard the thin driver's death courier relies on (spec 05).
#[test]
fn result_if_absent_records_when_the_spawn_is_unanswered() {
    let dir = temp_project();
    let root = dir.path();
    // unit-9 weave: store-opening couriers refuse to fabricate a store, so the
    // project must hold one before `rigger result` can record into it.
    seed_store(root);

    let (out, err, ok) = run_rigger(
        root,
        &[
            "result",
            "u/implementer#0",
            "--if-absent",
            "--error",
            "died without reporting",
        ],
    );
    assert!(ok, "recording an absent result must succeed; stderr: {err}");
    assert!(
        out.contains("recorded error result for u/implementer#0"),
        "an unanswered spawn's --if-absent record must land; got: {out:?}"
    );

    // The spawn now reads as answered, as a FAILURE (the courier's --error).
    let (rout, _err, ok) = run_rigger(root, &["reported", "u/implementer#0"]);
    assert!(ok, "the recorded spawn must read as reported");
    assert!(
        rout.contains("failed"),
        "the recorded --error must read back as a failure; got: {rout:?}"
    );
}

/// The anti-clobber invariant end-to-end: once a worker self-reported a success, a later
/// `rigger result <id> --if-absent --error <why>` - the death courier's single atomic
/// command - records NOTHING, exits 0, and leaves the self-report standing. This is what
/// closes the TOCTOU window the old two-process `rigger reported <id> || rigger result
/// <id> --error` guard left open (spec 05).
#[test]
fn result_if_absent_never_clobbers_a_self_reported_success() {
    let dir = temp_project();
    let root = dir.path();
    // unit-9 weave: store-opening couriers refuse to fabricate a store, so the
    // project must hold one before `rigger result` can record into it.
    seed_store(root);

    // The worker self-reports a success first.
    let (_o, err, ok) = run_rigger(
        root,
        &["result", "u/implementer#0", "implemented and reported"],
    );
    assert!(ok, "the self-report must succeed; stderr: {err}");

    // The death courier, unaware the worker already reported, fires --if-absent --error.
    let (out, err, ok) = run_rigger(
        root,
        &[
            "result",
            "u/implementer#0",
            "--if-absent",
            "--error",
            "died without reporting",
        ],
    );
    assert!(ok, "the --if-absent no-op must still exit 0; stderr: {err}");
    assert!(
        out.contains("already has a result") && out.contains("left it untouched"),
        "a spawn with a result must be left untouched by --if-absent; got: {out:?}"
    );

    // The self-reported SUCCESS still stands - it was NOT force-failed by the courier.
    let (rout, _err, ok) = run_rigger(root, &["reported", "u/implementer#0"]);
    assert!(ok, "the self-reported spawn must read as reported");
    assert!(
        rout.contains("ok") && !rout.contains("failed"),
        "the self-reported success must survive un-clobbered; got: {rout:?}"
    );
}

/// Spec 09 (Gap 20): `rigger init` MINTS a durable `.rigger/project.id` when absent -
/// deterministically from the normalized origin URL - and REPORTS it in the summary, then a
/// rerun never re-mints (the file is left untouched, so the id is stable).
#[test]
fn init_mints_and_reports_the_durable_project_identity() {
    let dir = temp_project();
    let root = dir.path();
    // A remote so the minted id is the deterministic origin-hash form, not the random one.
    git_ok(
        root,
        &[
            "remote",
            "add",
            "origin",
            "https://github.com/acme/widgets.git",
        ],
    );

    let (out, err, ok) = run_rigger(root, &["init"]);
    assert!(ok, "rigger init must succeed; stderr:\n{err}");
    assert!(
        out.contains("minted the durable project identity") && out.contains(".rigger/project.id"),
        "init reports the minted identity in its summary; got:\n{out}"
    );
    let id = std::fs::read_to_string(root.join(".rigger/project.id")).unwrap();
    assert!(
        !id.trim().is_empty(),
        "project.id holds a non-empty id; got: {id:?}"
    );

    // A rerun never re-mints: the existing file is left untouched, so the id is stable.
    let (out2, _err2, ok2) = run_rigger(root, &["init"]);
    assert!(ok2, "a rerun of rigger init must succeed");
    assert!(
        !out2.contains("minted"),
        "a rerun must NOT re-mint the identity; got:\n{out2}"
    );
    let id2 = std::fs::read_to_string(root.join(".rigger/project.id")).unwrap();
    assert_eq!(id, id2, "the minted id is stable across reruns");
}

/// Spec 09 headline scenario (Gap 20): a project's history SURVIVES a directory rename
/// end-to-end, because identity resolves from the tracked `.rigger/project.id`, not the
/// volatile directory basename. Mint the id, record a decision under it, `mv` the checkout,
/// and read the SAME decision back from the renamed directory.
#[test]
fn project_identity_survives_a_directory_rename() {
    // A parent tempdir so the PROJECT subdir can be renamed cleanly (the TempDir handle owns
    // the parent, not the project path).
    let base = tempfile::tempdir().unwrap();
    let proj = base.path().join("original-name");
    std::fs::create_dir_all(&proj).unwrap();
    git_ok(&proj, &["init", "-q"]);
    git_ok(
        &proj,
        &[
            "remote",
            "add",
            "origin",
            "https://github.com/acme/widgets.git",
        ],
    );

    // Mint the durable identity, then establish the store and record a decision under it.
    let (_o, err, ok) = run_rigger(&proj, &["init"]);
    assert!(ok, "init must mint the identity; stderr:\n{err}");
    seed_store(&proj);
    let (_o, err, ok) = run_rigger(
        &proj,
        &[
            "emit",
            "DecisionMade",
            r#"{"id":"survivor","summary":"pre-rename history","governs":["src/foo.rs"]}"#,
        ],
    );
    assert!(
        ok,
        "emit must record under the minted identity; stderr:\n{err}"
    );

    // Before the rename, the decision reads back.
    let (out, _e, ok) = run_rigger(&proj, &["peers", "src/foo.rs"]);
    assert!(
        ok && out.contains("decision survivor"),
        "the decision must read back before the rename; got:\n{out}"
    );

    // Rename the checkout - the exact `mv` that used to orphan a project's history (Gap 20).
    let renamed = base.path().join("renamed-away");
    std::fs::rename(&proj, &renamed).unwrap();

    // From the renamed directory the SAME history reads back: identity came from the tracked
    // project.id, not the (now-changed) directory basename.
    let (out, err, ok) = run_rigger(&renamed, &["peers", "src/foo.rs"]);
    assert!(ok, "peers must succeed after the rename; stderr:\n{err}");
    assert!(
        out.contains("decision survivor") && out.contains("governs: src/foo.rs"),
        "history must survive the directory rename end-to-end (Gap 20); got:\n{out}"
    );
}

/// Spec 09 one-time migration: a store holding events ONLY under the legacy basename
/// namespace is migrated once to the minted identity when the run driver opens it - the
/// streams are renamed, the history reads back under the minted identity, and a re-open is a
/// no-op (idempotent).
#[test]
fn step_migrates_legacy_history_to_the_minted_identity() {
    let dir = temp_project();
    let root = dir.path();
    write_two_stage_workflow(root);
    seed_store(root);

    // Pre-spec-09 history: a DecisionMade recorded BEFORE any project.id exists lands under
    // the legacy basename namespace.
    let (_o, err, ok) = run_rigger(
        root,
        &[
            "emit",
            "DecisionMade",
            r#"{"id":"legacy-decision","summary":"pre-mint history","governs":["src/legacy.rs"]}"#,
        ],
    );
    assert!(ok, "seeding legacy history must succeed; stderr:\n{err}");

    // Mint a durable identity DISTINCT from the basename (written directly so the test is
    // deterministic, independent of the temp dir's random basename).
    std::fs::write(root.join(".rigger/project.id"), "durablemint\n").unwrap();

    // A step opens the store with the minted identity: it migrates the legacy history once
    // and says so on stderr.
    let (_out, err, ok) = run_rigger(root, &["step"]);
    assert!(ok, "the step must succeed; stderr:\n{err}");
    assert!(
        err.contains("migrated project identity") && err.contains("durablemint"),
        "the step reports the one-time identity migration on stderr; got:\n{err}"
    );

    // The legacy decision now reads back under the MINTED identity (peers resolves via
    // project.id): the history moved namespaces, it was not lost.
    let (out, err, ok) = run_rigger(root, &["peers", "src/legacy.rs"]);
    assert!(ok, "peers must succeed; stderr:\n{err}");
    assert!(
        out.contains("decision legacy-decision"),
        "the pre-mint history reads back under the minted identity after migration; got:\n{out}"
    );

    // A second step is idempotent: nothing is left under the legacy namespace, so it does
    // not migrate again.
    let (_out, err2, ok2) = run_rigger(root, &["step"]);
    assert!(ok2, "the second step must succeed; stderr:\n{err2}");
    assert!(
        !err2.contains("migrated project identity"),
        "the migration is one-time: a re-open does not migrate again; got:\n{err2}"
    );
}

/// Spec 09: `rigger validate` WARNS (stderr, exit 0) when `.rigger/project.id` is absent - a
/// rename away would orphan the history - and is SILENT about identity once the id is minted.
#[test]
fn validate_warns_when_the_project_id_is_absent_and_is_silent_after_minting() {
    let dir = temp_project();
    let root = dir.path();
    write_two_stage_workflow(root); // a loadable config so `rigger validate` reaches the advisories

    // No project.id yet: validate WARNS (still exit 0) that identity falls back to the basename.
    let (out, err, ok) = run_rigger(root, &["validate"]);
    assert!(
        ok,
        "validate must still succeed (warning only); stderr:\n{err}"
    );
    assert!(
        out.contains("config valid"),
        "validate prints its config summary; stdout:\n{out}"
    );
    assert!(
        err.contains(".rigger/project.id") && err.to_lowercase().contains("orphan"),
        "validate warns that a missing project.id lets a rename orphan history; stderr:\n{err}"
    );

    // Mint it, then validate is SILENT on the identity advisory.
    std::fs::write(root.join(".rigger/project.id"), "durable-xyz\n").unwrap();
    let (_out, err2, ok2) = run_rigger(root, &["validate"]);
    assert!(ok2, "validate must succeed; stderr:\n{err2}");
    assert!(
        !err2.contains("project.id"),
        "validate is silent about identity once project.id exists; stderr:\n{err2}"
    );
}

/// Overwrite the two-stage worker agent's PROMPT body in place (spec 13, unit 1), drifting
/// the on-disk definition from what a prior `rigger step` pinned - the mid-campaign prompt
/// edit that silently changes replay semantics, which pinning exists to catch.
fn edit_worker_prompt(root: &Path, new_body: &str) {
    std::fs::write(
        root.join(".rigger").join("agents").join("worker.md"),
        format!("---\nid: worker\nmodel: sonnet\ntools: [Read, Edit]\nisolation: none\n---\n{new_body}\n"),
    )
    .unwrap();
}

/// Definition pinning (spec 13, unit 1): a run pins its definition at start, and a LIVE-run
/// step under a definition drifted mid-campaign HALTS loudly naming the drift; the operator's
/// explicit `--rebase-definition` records the supersession and continues, after which plain
/// steps no longer halt.
#[test]
fn step_halts_on_definition_drift_and_rebase_definition_continues() {
    let dir = temp_project();
    let root = dir.path();
    write_two_stage_workflow(root);

    // Step 1 pins the run's definition (and parks the first wave). This is the pin-at-start.
    let (_out, err, ok) = run_rigger(root, &["step"]);
    assert!(
        ok,
        "the first step must succeed and pin the definition; stderr: {err}"
    );

    // A mid-campaign prompt edit drifts the on-disk definition from the pinned hash.
    edit_worker_prompt(root, "Do the unit, but differently now.");

    // Step 2 (no flag) must HALT loudly: a non-zero exit whose stderr names the drift, and
    // it must recommend the --rebase-definition escape. It must NOT print a wave (nothing ran).
    let (out, err, ok) = run_rigger(root, &["step"]);
    assert!(
        !ok,
        "a drifted live-run step must fail (halt), not succeed; stdout: {out:?}"
    );
    assert!(
        err.contains("definition drift"),
        "the halt must name the definition drift; stderr: {err}"
    );
    assert!(
        err.contains("--rebase-definition"),
        "the halt must point at the --rebase-definition escape; stderr: {err}"
    );
    assert!(
        !out.contains("\"wave\""),
        "a halted step must not drive the conductor / print a wave; stdout: {out:?}"
    );

    // Re-running the plain step STILL halts - drift is a pure read, so it re-surfaces every
    // step until it is resolved (never silently swallowed).
    let (_out, err, ok) = run_rigger(root, &["step"]);
    assert!(
        !ok,
        "the drift re-surfaces on every plain step; stderr: {err}"
    );
    assert!(err.contains("definition drift"));

    // `--rebase-definition` records the supersession and CONTINUES: the step succeeds and
    // reports the rebase on stderr.
    let (_out, err, ok) = run_rigger(root, &["step", "--rebase-definition"]);
    assert!(
        ok,
        "--rebase-definition must record the supersession and continue; stderr: {err}"
    );
    assert!(
        err.contains("supersession"),
        "the rebase must report the recorded supersession; stderr: {err}"
    );

    // After the rebase, a PLAIN step no longer halts: the effective pin advanced to the new
    // definition, so the campaign continues cleanly.
    let (_out, err, ok) = run_rigger(root, &["step"]);
    assert!(
        ok,
        "after --rebase-definition a plain step must no longer halt; stderr: {err}"
    );
    assert!(
        !err.contains("definition drift"),
        "the rebased definition is the pin now - no residual drift; stderr: {err}"
    );
}

/// Definition pinning, the new-run-is-free path (spec 13, unit 1): a FRESH run always pins the
/// CURRENT definition and never halts, even when the on-disk definition differs from what an
/// earlier run pinned - only a LIVE run pins, so a run boundary is always free to reconfigure.
#[test]
fn a_fresh_run_repins_the_current_definition_and_never_halts() {
    let dir = temp_project();
    let root = dir.path();
    write_two_stage_workflow(root);

    // A first run pins definition A.
    let (_out, err, ok) = run_rigger(root, &["step"]);
    assert!(ok, "the first step must pin definition A; stderr: {err}");

    // The definition drifts to B on disk. A plain step would halt (proven above)...
    edit_worker_prompt(root, "A brand new prompt body.");

    // ...but a FRESH run begins a new boundary pinning the CURRENT (B) definition and is free.
    let (_out, err, ok) = run_rigger(root, &["step", "--fresh"]);
    assert!(
        ok,
        "a --fresh run must pin the current definition and NOT halt on the prior pin; stderr: {err}"
    );
    assert!(
        err.contains("began a new run"),
        "the fresh run announces its new boundary; stderr: {err}"
    );
    assert!(
        !err.contains("definition drift"),
        "a fresh run is free - it never drifts against a prior run's pin; stderr: {err}"
    );

    // And the fresh run's pin is now B: a subsequent plain step is free on B but WOULD halt if
    // the definition drifted again - re-editing and stepping halts, confirming the fresh run
    // genuinely re-pinned (rather than disabling the check).
    let (_out, err, ok) = run_rigger(root, &["step"]);
    assert!(
        ok,
        "a plain step on the freshly-pinned definition is free; stderr: {err}"
    );
    edit_worker_prompt(root, "Yet another prompt body.");
    let (_out, err, ok) = run_rigger(root, &["step"]);
    assert!(
        !ok && err.contains("definition drift"),
        "the fresh run really re-pinned: a later drift against it halts; stderr: {err}"
    );
}

/// The canary namespace is DISTINCT from the run stream, so `rigger stats --canary`
/// reports the judge-the-judges scorecard from a project's canary stream without ever
/// touching its operator metrics (spec 13, unit 5). Seeds a canary run directly into the
/// namespaced canary stream (a real `rigger canary` would spawn the review panel, which
/// needs live agents), then drives the compiled binary and asserts the per-tier catch
/// rate, adjudicator correctness, and finding-order stability the reporter folds.
#[test]
fn stats_canary_reports_the_per_tier_scorecard_from_the_canary_stream() {
    use rigger::eventstore::namespace::Namespaced;
    use rigger::eventstore::sqlite::Store;
    use rigger::eventstore::{Event, EventStore, ExpectedRevision};

    // A plain (non-git) project with a pinned identity, so the binary's namespace and the
    // one we seed under agree exactly (no git-toplevel canonicalization in the way).
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let rigger = root.join(".rigger");
    std::fs::create_dir_all(&rigger).unwrap();
    std::fs::write(rigger.join("project.id"), "canary-proj\n").unwrap();

    // Seed one canary run: a batch marker + four scored outcomes (3 planted, 1 control).
    let backend = Store::open(rigger.join("events.db").to_str().unwrap()).unwrap();
    let store = Namespaced::new(&backend, "canary-proj");
    let ty = rigger::ledger::TYPE_UNIT_STATUS;
    let ev = |json: String| Event::new(ty, json.into_bytes());
    let marker = ev(r#"{"id":"batch-1","status":"canary-run"}"#.to_string());
    let outcome = |id: &str,
                   class: &str,
                   planted: bool,
                   expect_reject: bool,
                   caught: &str,
                   correct: bool,
                   stable: bool| {
        let approved = if correct {
            !expect_reject
        } else {
            expect_reject
        };
        ev(format!(
            r#"{{"id":"{id}","status":"canary","defect_class":"{class}","planted":{planted},"expected_reject":{expect_reject},"expected_tier":"","caught_by":[{caught}],"verdict_approved":{approved},"verdict_correct":{correct},"stable":{stable}}}"#
        ))
    };
    let events = [
        marker,
        outcome(
            "a",
            "off-by-one",
            true,
            true,
            r#""lens","adversary""#,
            true,
            true,
        ),
        outcome(
            "b",
            "resource-leak",
            true,
            true,
            r#""adversary""#,
            true,
            true,
        ),
        outcome(
            "c",
            "fail-open-guard",
            true,
            true,
            r#""adversary""#,
            true,
            false,
        ),
        outcome("d", "none", false, false, "", true, true),
    ];
    store
        .append("canary", ExpectedRevision::Any, &events)
        .unwrap();

    let (out, err, ok) = run_rigger(root, &["stats", "--canary"]);
    assert!(ok, "stats --canary must succeed; stderr: {err}");
    assert!(
        out.contains("items scored       4 (3 planted, 3 defect class(es) cataloged)"),
        "reports the corpus size and cataloged classes; got:\n{out}"
    );
    // Catch rate BY TIER: the adversary caught all 3 planted, the lens only 1.
    assert!(
        out.contains("adversary        3/3 (100.0%)"),
        "the adversary tier's catch rate; got:\n{out}"
    );
    assert!(
        out.contains("lens             1/3 (33.3%)"),
        "the lens tier's catch rate; got:\n{out}"
    );
    // Adjudicator correct on all 4; stable on 3 of 4 (item c flipped on order).
    assert!(
        out.contains("adjudicator        4/4 correct (100.0%)"),
        "adjudicator correctness; got:\n{out}"
    );
    assert!(
        out.contains("verdict stability  3/4 stable (75.0%)"),
        "finding-order stability; got:\n{out}"
    );
    // The run stream is untouched by a canary run, so plain `rigger stats` sees no runs.
    let (run_out, _e, run_ok) = run_rigger(root, &["stats"]);
    assert!(run_ok);
    assert!(
        run_out.contains("no runs recorded yet"),
        "a canary run never lands on the run stream; got:\n{run_out}"
    );
}

/// `rigger stats --canary` on a project that has never run a canary says so clearly,
/// rather than printing an empty/zero scorecard, and creates no false impression of a run.
#[test]
fn stats_canary_on_a_project_with_no_canary_run_says_so() {
    let dir = temp_project();
    let root = dir.path();
    seed_store(root);
    let (out, err, ok) = run_rigger(root, &["stats", "--canary"]);
    assert!(ok, "stats --canary must succeed; stderr: {err}");
    assert!(
        out.contains("no canary run recorded yet"),
        "an un-canaried project is told to run `rigger canary`; got:\n{out}"
    );
}

/// `rigger canary`'s CLI glue (arg parsing + corpus loading) is exercised through the
/// real binary on the paths that need no live review agent. The panel-spawning happy path
/// is covered end-to-end by the library runner test (`canary::run_canary`) with a scripted
/// driver; here we pin the binary's argument and corpus-loading contracts.
#[test]
fn canary_rejects_unknown_arguments_and_a_missing_corpus() {
    let dir = temp_project();
    let root = dir.path();

    // An unknown flag is refused (and does not silently no-op).
    let (_o, err, ok) = run_rigger(root, &["canary", "--bogus"]);
    assert!(!ok, "an unknown canary argument must be rejected");
    assert!(
        err.contains("unexpected argument"),
        "the error names the bad argument; stderr: {err}"
    );

    // A missing corpus directory fails loudly rather than scoring an empty corpus.
    let (_o, err, ok) = run_rigger(root, &["canary", "--corpus", "no-such-dir"]);
    assert!(!ok, "a missing corpus dir must fail");
    assert!(
        err.contains("canary"),
        "the error is a canary error; stderr: {err}"
    );

    // A present-but-empty corpus directory is also refused (not a silent zero-item run).
    let empty = root.join("empty-corpus");
    std::fs::create_dir_all(&empty).unwrap();
    let (_o, err, ok) = run_rigger(root, &["canary", "--corpus", "empty-corpus"]);
    assert!(!ok, "an empty corpus dir must fail");
    assert!(
        err.contains("no items"),
        "the error explains the corpus is empty; stderr: {err}"
    );
}

/// Seed `<root>/.rigger/events.db` under the pinned identity `project` with TWO runs on the
/// conductor's run stream, each stamping a tier's resolved model on a unit-lifecycle event
/// the way the conductor does (spec 05 line 52 / spec 13b unit 1): run `r1` resolves the
/// `opus` alias to `prev_model`, run `r2` resolves it to `curr_model`. Passing the same model
/// for both is the no-change control; a different `curr_model` seeds a silent alias re-point
/// the drift monitor must catch. Mirrors the real stamps (META_RUN_ID + META_MODEL_ALIAS +
/// META_MODEL_RESOLVED on a `green` status) so the binary folds them exactly as a live run's.
fn seed_two_runs_with_models(root: &Path, project: &str, prev_model: &str, curr_model: &str) {
    use rigger::eventstore::namespace::Namespaced;
    use rigger::eventstore::sqlite::Store;
    use rigger::eventstore::{Event, EventStore, ExpectedRevision};

    let rigger = root.join(".rigger");
    std::fs::create_dir_all(&rigger).unwrap();
    std::fs::write(rigger.join("project.id"), format!("{project}\n")).unwrap();

    let backend = Store::open(rigger.join("events.db").to_str().unwrap()).unwrap();
    let store = Namespaced::new(&backend, project);
    let run_id = rigger::run::META_RUN_ID;
    let alias = rigger::conductor::META_MODEL_ALIAS;
    let resolved = rigger::conductor::META_MODEL_RESOLVED;
    let started = |run: &str| {
        Event::new(
            rigger::run::TYPE_RUN_STARTED,
            format!(r#"{{"run":"{run}"}}"#).into_bytes(),
        )
        .with_meta(run_id, run)
    };
    let green = |run: &str, model: &str| {
        Event::new(
            rigger::ledger::TYPE_UNIT_STATUS,
            r#"{"id":"u","status":"green"}"#.as_bytes().to_vec(),
        )
        .with_meta(run_id, run)
        .with_meta(alias, "opus")
        .with_meta(resolved, model)
    };
    let events = [
        started("r1"),
        green("r1", prev_model),
        started("r2"),
        green("r2", curr_model),
    ];
    store
        .append(rigger::conductor::STREAM, ExpectedRevision::Any, &events)
        .unwrap();
}

/// Spec 13b, unit 1 (`rigger validate` clause): a tier whose resolved model id re-pointed
/// since the previous run makes `rigger validate` WARN on stderr (exit 0) and recommend the
/// drift-gated canary, while an unchanged model stays silent. The no-change control and the
/// seeded re-point are pinned side by side so the warning cannot fire on steady state.
#[test]
fn validate_warns_when_a_tier_resolved_model_repointed_between_runs() {
    // The no-change control: both runs resolve `opus` identically -> validate is drift-silent.
    let control = temp_project();
    let croot = control.path();
    let (_o, err, ok) = run_rigger(croot, &["init"]);
    assert!(
        ok,
        "rigger init must scaffold a valid config; stderr:\n{err}"
    );
    seed_two_runs_with_models(croot, "drift-control", "claude-opus-4-1", "claude-opus-4-1");
    let (out, err, ok) = run_rigger(croot, &["validate"]);
    assert!(
        ok,
        "validate must succeed on a steady model; stderr:\n{err}"
    );
    assert!(
        out.contains("config valid"),
        "validate still prints its config summary; stdout:\n{out}"
    );
    assert!(
        !err.to_lowercase().contains("resolved model id changed"),
        "an unchanged model must NOT warn about drift; stderr:\n{err}"
    );

    // The seeded re-point: `opus` resolves to a different concrete model in the second run.
    let drift = temp_project();
    let droot = drift.path();
    let (_o, err, ok) = run_rigger(droot, &["init"]);
    assert!(
        ok,
        "rigger init must scaffold a valid config; stderr:\n{err}"
    );
    seed_two_runs_with_models(droot, "drift-repoint", "claude-opus-4-1", "claude-opus-4-8");
    let (_out, err, ok) = run_rigger(droot, &["validate"]);
    assert!(
        ok,
        "validate WARNS but still exits 0 on model drift; stderr:\n{err}"
    );
    assert!(
        err.to_lowercase().contains("resolved model id changed")
            && err.contains("opus")
            && err.contains("claude-opus-4-1")
            && err.contains("claude-opus-4-8"),
        "the advisory names the re-pointed tier and both model ids; stderr:\n{err}"
    );
    assert!(
        err.contains("rigger canary --if-model-changed"),
        "the advisory recommends the drift-gated canary; stderr:\n{err}"
    );
}

/// Spec 13b, unit 1 (`rigger canary --if-model-changed` clause), the no-change control: an
/// unchanged resolved model runs NO canary. The gate precedes the corpus load, so the missing
/// `--corpus` is never even consulted - the command exits 0 having deliberately done nothing.
#[test]
fn canary_if_model_changed_skips_when_the_model_is_unchanged() {
    let dir = temp_project();
    let root = dir.path();
    seed_two_runs_with_models(root, "canary-steady", "claude-opus-4-1", "claude-opus-4-1");
    let (out, err, ok) = run_rigger(
        root,
        &["canary", "--if-model-changed", "--corpus", "no-such-dir"],
    );
    assert!(
        ok,
        "an unchanged model must exit 0 without running the panel; stderr:\n{err}"
    );
    assert!(
        out.contains("no resolved-model change") && out.contains("skipping"),
        "the skip is announced; stdout:\n{out}"
    );
    assert!(
        !out.contains("running the panel"),
        "no canary runs on an unchanged model; stdout:\n{out}"
    );
}

/// Spec 13b, unit 1 (`rigger canary --if-model-changed` clause), the seeded model change: a
/// re-pointed tier OPENS the gate so the canary runs. We point `--corpus` at a missing dir so
/// the command stops right after the gate (no live review panel is spawned in a CLI test); the
/// gate-open line on stdout proves the run was NOT skipped and reached corpus loading.
#[test]
fn canary_if_model_changed_runs_when_a_tier_resolved_model_repointed() {
    let dir = temp_project();
    let root = dir.path();
    seed_two_runs_with_models(root, "canary-repoint", "claude-opus-4-1", "claude-opus-4-8");
    let (out, err, ok) = run_rigger(
        root,
        &["canary", "--if-model-changed", "--corpus", "no-such-dir"],
    );
    assert!(
        out.contains("resolved model changed for opus") && out.contains("running the panel"),
        "a re-pointed model opens the gate; stdout:\n{out}"
    );
    assert!(
        !out.contains("skipping"),
        "a changed model is NOT skipped; stdout:\n{out}"
    );
    // Having opened the gate, the run proceeds into corpus loading (the missing `--corpus` is
    // now consulted and fails), which proves the gate let it through rather than short-circuiting.
    assert!(
        !ok && err.contains("canary"),
        "the gate opened and the run reached corpus loading; stderr:\n{err}"
    );
}
