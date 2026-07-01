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

/// Run `rigger <args...>` in `cwd` and return (stdout, stderr, success).
fn run_rigger(cwd: &Path, args: &[&str]) -> (String, String, bool) {
    let out = Command::new(rigger_bin())
        .args(args)
        .current_dir(cwd)
        .output()
        .expect("failed to spawn the rigger binary");
    (
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
        out.status.success(),
    )
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

    // The store and graph databases now exist under .rigger/.
    assert!(
        root.join(".rigger").join("events.db").exists(),
        "emit must create the namespaced event store"
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
