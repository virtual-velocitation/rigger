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
