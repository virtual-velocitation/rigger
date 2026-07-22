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

/// The project identity the binary resolves for `root`, mirrored here for seeding: the
/// tracked `.rigger/project.id` at the git top-level when present, else the git top-level
/// basename, else `root`'s own basename (never empty) - the precedence
/// `project_identity_at` uses. A seed appended under this identity lands in the exact
/// `proj-<id>-run` stream the compiled binary reads back.
fn run_stream_identity(root: &Path) -> String {
    let toplevel = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["rev-parse", "--show-toplevel"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .filter(|s| !s.is_empty());
    let base = toplevel.as_deref().map(Path::new).unwrap_or(root);
    if let Ok(raw) = std::fs::read_to_string(base.join(".rigger").join("project.id")) {
        let id = raw.trim();
        if !id.is_empty() {
            return id.to_string();
        }
    }
    base.file_name()
        .and_then(|n| n.to_str())
        .filter(|s| !s.is_empty())
        .map(String::from)
        .unwrap_or_else(|| "rigger".to_string())
}

/// Seed run-lifecycle events (`RunStarted`, `SpawnRequested`, `SpawnResult`, `UnitStarted`,
/// `UnitIntegrated`, `UnitEscalated`, ...) directly into the namespaced run stream, standing
/// in for the conductor minting them (and for a courier's `rigger result` `SpawnResult`).
/// The `rigger emit` surface refuses these conductor-owned boundary types (spec 22), so a
/// test that must seed prior-run residue or a spawn's recorded outcome appends through the
/// store, not the guarded courier. Each event is byte-identical to what the pre-guard
/// `rigger emit <type> <json>` seed produced (same type, `data` bytes, and `run` stream,
/// no metadata), and it binds to the SAME identity the binary resolves for `root`, so every
/// downstream `rigger step` / `stats` / `validate` reads it back exactly as before.
fn seed_run_events(root: &Path, events: &[(&str, &str)]) {
    use rigger::eventstore::namespace::Namespaced;
    use rigger::eventstore::sqlite::Store;
    use rigger::eventstore::{Event, EventStore, ExpectedRevision};

    let rigger_dir = root.join(".rigger");
    std::fs::create_dir_all(&rigger_dir).unwrap();
    let backend = Store::open(rigger_dir.join("events.db").to_str().unwrap()).unwrap();
    let store = Namespaced::new(&backend, &run_stream_identity(root));
    for &(ty, body) in events {
        store
            .append(
                rigger::conductor::STREAM,
                ExpectedRevision::Any,
                &[Event::new(ty, body.as_bytes().to_vec())],
            )
            .unwrap();
    }
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
    // The step path auto-starts a persistent, detached run dashboard (spec 39, criterion 1);
    // opt out so these short-lived integration invocations never spawn a real dashboard
    // process that would outlive the test. Set before the caller's envs so a test could still
    // override it.
    cmd.env("RIGGER_NO_DASH", "1");
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

/// Append `line` (plus a newline) to the file at `path`, standing in for a hand edit that a
/// deterministic render would never produce - used to drive a committed file OUT of sync
/// with a fresh render.
fn append_line(path: &Path, line: &str) {
    use std::io::Write;
    let mut f = std::fs::OpenOptions::new()
        .append(true)
        .open(path)
        .unwrap_or_else(|e| panic!("open {} for append: {e}", path.display()));
    writeln!(f, "{line}").unwrap();
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
    // Opt out of the step path's auto-started dashboard (spec 39, criterion 1) so no real
    // dashboard process outlives the test; set before the caller's envs so it can override.
    cmd.env("RIGGER_NO_DASH", "1");
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

/// Spec 27, criterion 2 - the raw events stay RETRIEVABLE via `rigger peers` after
/// consolidation. The sleep-phase distiller folds OLDER-THAN-CURRENT-RUN
/// findings/decisions into a per-file digest pool, but it is a PROJECTION over the
/// append-only log: it introduces no event type and DELETES nothing, so the underlying
/// `DecisionMade`/`ReviewFinding` events survive untouched and a real `rigger peers <file>`
/// query still returns them. This is the load-bearing spec-27 decision - consolidation
/// summarizes by AGE, it never scopes grounding away from the consolidated raw items.
///
/// Proven through the REAL paths, not a fabricated fold: an old run A records a decision +
/// a finding ABOUT `combat.rs`, then the current run B starts, all seeded into the SAME
/// `conductor::STREAM` the compiled `rigger peers` binary reads. Consolidation runs via the
/// production `distiller::rebuild` entry over exactly that stream (writing the pool under
/// `.rigger/digests`, as production would), and only THEN is the COMPILED `rigger peers
/// combat.rs` asserted to still surface BOTH raw items - while the run itself is byte-for-byte
/// unchanged (consolidation appended and deleted nothing).
#[test]
fn distiller_consolidation_leaves_the_raw_events_retrievable_via_peers() {
    use rigger::eventstore::namespace::Namespaced;
    use rigger::eventstore::sqlite::Store;
    use rigger::eventstore::{Direction, EventStore};

    let dir = temp_project();
    let root = dir.path();

    // A prior run A recorded a decision + a finding ABOUT combat.rs; then the CURRENT run B
    // started. Run A's items are OLDER-THAN-CURRENT-RUN, so consolidation folds them. A
    // `RunStarted` is a lifecycle event the `rigger emit` allowlist refuses, so seed the
    // whole ordered run stream directly - byte-identical to what a real `rigger run` appends.
    seed_run_events(
        root,
        &[
            (rigger::run::TYPE_RUN_STARTED, r#"{"run":"A"}"#),
            (
                "DecisionMade",
                r#"{"id":"d-old","summary":"guard the checked add","governs":["combat.rs"]}"#,
            ),
            (
                "ReviewFinding",
                r#"{"id":"f-old","by":"tech-lens","summary":"misses the buffer bound","about":["combat.rs"]}"#,
            ),
            (rigger::run::TYPE_RUN_STARTED, r#"{"run":"B"}"#),
        ],
    );

    // Read the run stream exactly as production does (the same namespaced `conductor::STREAM`
    // `rigger peers` reads), scoped so the read connection drops before the peers subprocess.
    let read_run_stream = || -> Vec<rigger::eventstore::Event> {
        let backend =
            Store::open(root.join(".rigger").join("events.db").to_str().unwrap()).unwrap();
        let store = Namespaced::new(&backend, &run_stream_identity(root));
        store
            .read_stream(rigger::conductor::STREAM, 0, Direction::Forward)
            .unwrap()
    };
    let events = read_run_stream();

    // CONSOLIDATE via the production distiller entry (there is no CLI subcommand for it by
    // design - the distiller is a library projection). The pool lives under `.rigger/digests`,
    // exactly where production would write it.
    let pool_dir = root.join(".rigger").join(rigger::distiller::POOL_SUBDIR);
    let digests = rigger::distiller::rebuild(&events, &pool_dir).unwrap();

    // Consolidation ACTUALLY ran: run A's stale combat.rs items folded into ONE digest that
    // SUMMARIZES both, so the retrievability claim below is over consolidated content, not a
    // no-op. (Run B's items are current and would stay raw, but here B recorded none.)
    assert_eq!(
        digests.len(),
        1,
        "run A's stale combat.rs items consolidate into exactly one digest; got: {digests:?}"
    );
    let d = &digests[0];
    assert_eq!(d.file, "combat.rs", "the digest keys by the trigger file");
    assert!(
        d.summary.contains("guard the checked add")
            && d.summary.contains("misses the buffer bound"),
        "the digest summarizes BOTH the stale decision and the stale finding; got: {}",
        d.summary
    );
    assert!(
        pool_dir.join(format!("{}.md", d.id)).exists(),
        "the digest projection was written to the pool on disk"
    );

    // ...yet the RAW events are NOT deleted: a real `rigger peers combat.rs` still returns
    // BOTH the underlying decision and finding. peers replays the raw event log, which the
    // distiller never touches - so consolidation summarizes WITHOUT pruning the source, and
    // grounding can still retrieve the raw items (they are older-run, hence labeled HISTORICAL,
    // but still surfaced - the spec-27 guarantee that consolidation never scopes them away).
    let (out, err, ok) = run_rigger(root, &["peers", "combat.rs"]);
    assert!(ok, "peers must succeed after consolidation; stderr: {err}");
    assert!(
        out.contains("decision d-old") && out.contains("governs: combat.rs"),
        "the raw decision must STILL be retrievable via peers after consolidation; got: {out:?}"
    );
    assert!(
        out.contains("finding f-old")
            && out.contains("by tech-lens")
            && out.contains("about: combat.rs"),
        "the raw finding must STILL be retrievable via peers after consolidation; got: {out:?}"
    );

    // And the log itself is byte-for-byte unchanged: consolidation is a projection over the
    // append-only stream - it appended nothing and deleted nothing.
    let after = read_run_stream();
    assert_eq!(
        after.len(),
        events.len(),
        "consolidation must append and delete NO events in the run log"
    );
    assert!(
        after
            .iter()
            .zip(&events)
            .all(|(a, b)| a.type_ == b.type_ && a.data == b.data),
        "every raw event survives consolidation unchanged (type + payload)"
    );
    // The events the peers query returned are exactly the raw ones still present in the log.
    assert!(
        after.iter().any(|e| e.type_ == "DecisionMade"
            && e.data
                == br#"{"id":"d-old","summary":"guard the checked add","governs":["combat.rs"]}"#),
        "the raw d-old DecisionMade is still physically in events.db after consolidation"
    );
    assert!(
        after.iter().any(|e| e.type_ == "ReviewFinding"),
        "the raw f-old ReviewFinding is still physically in events.db after consolidation"
    );
}

/// Spec 25, criterion 1 - the DISCARD trigger, PROVEN through the REAL production result
/// path (`rigger result` -> `cmd_result` -> `spawn::record_result`), not a direct
/// `Projector::apply` on a hand-built event.
///
/// The wiring under test: `rigger emit ReviewFinding` folds two findings about the same file
/// into the PERSISTED `graph.db`; then `rigger result <adjudicator-id> <verdict>` records the
/// adjudicator's `SpawnResult` to the run log AND folds it into that same `graph.db` - exactly
/// as `rigger emit` folds an emitted event - so the verdict line's `discarded` array
/// invalidates that finding's `RAISED`/`ABOUT` edges (`valid_to` set, never deleted). Because
/// `rigger graph --around` reads the PERSISTED `graph.db` through `subgraph` (whose traversal
/// filters `valid_to IS NULL`), the discarded finding drops from the returned subgraph while
/// the un-disposed finding stays live. This is the observable the reject demanded: without the
/// `cmd_result` fold wiring the discarded finding is STILL returned here (the fold arm alone is
/// inert in production because nothing folds a `SpawnResult` into `graph.db`).
#[test]
fn rigger_result_folds_an_adjudicator_discard_into_the_persisted_graph() {
    let dir = temp_project();
    let root = dir.path();
    seed_store(root);

    let file = "combat.rs";
    let emit_finding = |id: &str, by: &str| {
        let payload = format!(r#"{{"id":"{id}","by":"{by}","summary":"x","about":["{file}"]}}"#);
        let (_o, err, ok) = run_rigger(root, &["emit", "ReviewFinding", &payload]);
        assert!(ok, "emit ReviewFinding {id} must succeed; stderr: {err}");
    };
    emit_finding("f-discard", "lens:tech");
    emit_finding("f-open", "lens:sdet");

    // Parse the `node <id> <kind>` lines of `rigger graph --around <file>` into the id set,
    // so the assertions are exact (never a substring match against an edge line).
    let graph_node_ids = |out: &str| -> Vec<String> {
        out.lines()
            .filter_map(|l| {
                l.trim_start()
                    .strip_prefix("node ")
                    .and_then(|rest| rest.split_whitespace().next())
                    .map(str::to_string)
            })
            .collect()
    };

    // Before any verdict, both findings are reachable from the file they are ABOUT.
    let (before, err, ok) = run_rigger(root, &["graph", "--around", file, "--depth", "2"]);
    assert!(ok, "graph must succeed; stderr: {err}");
    let before_ids = graph_node_ids(&before);
    for id in ["f-discard", "f-open"] {
        assert!(
            before_ids.iter().any(|n| n == id),
            "{id} must be in the graph before the verdict; got: {before_ids:?}"
        );
    }

    // The adjudicator records its verdict through the REAL result path. Its spawn id carries
    // the adjudicator role token so `SpawnResult::adjudication` recognizes it as a disposition;
    // the verdict line explicitly DISCARDS f-discard and does NOT name f-open (a discard keys
    // on the explicit `discarded` array, never the complement of `upheld`).
    let verdict =
        r#"{"verdict":"reject","upheld":[],"discarded":["f-discard"],"cause":"genuine-defect"}"#;
    let (_o, err, ok) = run_rigger(root, &["result", "u1/adjudicator#0", verdict]);
    assert!(ok, "rigger result must succeed; stderr: {err}");

    // After the recorded verdict folds into graph.db, the discarded finding's edges are
    // invalidated so `subgraph` (valid_to IS NULL) no longer reaches it; the un-disposed
    // finding is untouched and stays live.
    let (after, err, ok) = run_rigger(root, &["graph", "--around", file, "--depth", "2"]);
    assert!(ok, "graph must succeed after the verdict; stderr: {err}");
    let after_ids = graph_node_ids(&after);
    assert!(
        !after_ids.iter().any(|n| n == "f-discard"),
        "the explicitly discarded finding must drop from the live subgraph once `rigger result` \
         folds the adjudicator SpawnResult into graph.db; got: {after_ids:?}"
    );
    assert!(
        after_ids.iter().any(|n| n == "f-open"),
        "the un-disposed finding stays live after the verdict; got: {after_ids:?}"
    );
}

/// `rigger reset --runs` (spec 21, unit 2) drops every SUPERSEDED / dead run's decisions and
/// findings from the context graph while PRESERVING every `LessonLearned` and the ACTIVE
/// run's decisions and findings - the supported way to shed dead-run grounding noise without
/// wiping the store.
///
/// The fixture is two runs in one store (`r1` superseded, `r2` active) plus PRE-BOUNDARY
/// residue recorded before the first `RunStarted`. The `RunStarted` boundaries are seeded
/// directly through the store (`seed_run_events`) because `rigger emit` refuses to mint the
/// conductor-owned lifecycle types (spec 22); the decisions/findings/lessons go through
/// `rigger emit`, which both appends to the run stream AND folds them into `graph.db`. Every
/// provenance event governs or is about the SAME file, so `rigger graph --around <file>`
/// (which reads the persisted `graph.db` the prune mutates - unlike `rigger peers`, which
/// re-projects the stream) lists exactly which nodes survive. The drop set is proven against
/// `d21-preboundary-reset-drop`: a pre-boundary decision AND finding are DROPPED, a
/// pre-boundary lesson is KEPT, and - closing the cross-run id-reuse keep-invariant hazard - a
/// decision id `shared-d` recorded in BOTH the dead run and the active run is KEPT.
#[test]
fn reset_runs_prunes_dead_runs_from_the_graph_keeping_lessons_active_run_and_reused_ids() {
    let dir = temp_project();
    let root = dir.path();
    // A prior run created the store; the couriers below only append to it.
    seed_store(root);

    let file = "shared.rs";
    // A provenance event goes through `rigger emit`, which appends to the run stream AND folds
    // it into graph.db so the prune has a node to delete. Everything targets the SAME file so
    // one `rigger graph --around` reads the whole provenance set back.
    let emit = |typ: &str, json: &str| {
        let (_o, err, ok) = run_rigger(root, &["emit", typ, json]);
        assert!(ok, "emit {typ} must succeed; stderr: {err}");
    };

    // Pre-boundary (before any RunStarted): decision + finding DROP, lesson KEEPS.
    emit(
        "DecisionMade",
        &format!(r#"{{"id":"pre-d","summary":"pre decision","governs":["{file}"]}}"#),
    );
    emit(
        "ReviewFinding",
        &format!(r#"{{"id":"pre-f","by":"lens","summary":"pre finding","about":["{file}"]}}"#),
    );
    emit(
        "LessonLearned",
        &format!(r#"{{"id":"pre-lesson","summary":"pre lesson","about":["{file}"]}}"#),
    );

    // Boundary for the superseded run r1 (seeded directly - `rigger emit` refuses RunStarted).
    seed_run_events(
        root,
        &[("RunStarted", r#"{"run":"r1","criteria":["crit"]}"#)],
    );
    // Superseded run r1: decision + finding DROP, lesson KEEPS.
    emit(
        "DecisionMade",
        &format!(r#"{{"id":"r1-d","summary":"r1 decision","governs":["{file}"]}}"#),
    );
    emit(
        "ReviewFinding",
        &format!(r#"{{"id":"r1-f","by":"lens","summary":"r1 finding","about":["{file}"]}}"#),
    );
    emit(
        "LessonLearned",
        &format!(r#"{{"id":"r1-lesson","summary":"r1 lesson","about":["{file}"]}}"#),
    );
    // A decision id reused across runs, recorded first in the DEAD run r1.
    emit(
        "DecisionMade",
        &format!(r#"{{"id":"shared-d","summary":"shared (dead copy)","governs":["{file}"]}}"#),
    );

    // Boundary for the ACTIVE run r2.
    seed_run_events(
        root,
        &[("RunStarted", r#"{"run":"r2","criteria":["crit"]}"#)],
    );
    // Active run r2: decision + finding KEEP, lesson KEEPS.
    emit(
        "DecisionMade",
        &format!(r#"{{"id":"r2-d","summary":"r2 decision","governs":["{file}"]}}"#),
    );
    emit(
        "ReviewFinding",
        &format!(r#"{{"id":"r2-f","by":"lens","summary":"r2 finding","about":["{file}"]}}"#),
    );
    emit(
        "LessonLearned",
        &format!(r#"{{"id":"r2-lesson","summary":"r2 lesson","about":["{file}"]}}"#),
    );
    // The SAME reused id recorded again in the ACTIVE run r2: the node must survive the reset.
    emit(
        "DecisionMade",
        &format!(r#"{{"id":"shared-d","summary":"shared (active copy)","governs":["{file}"]}}"#),
    );

    // Parse the `node <id> <kind>` lines of `rigger graph --around <file>` into the id set.
    let graph_node_ids = |out: &str| -> Vec<String> {
        out.lines()
            .filter_map(|l| {
                l.trim_start()
                    .strip_prefix("node ")
                    .and_then(|rest| rest.split_whitespace().next())
                    .map(str::to_string)
            })
            .collect()
    };

    // Before the reset every provenance node is reachable from the shared file.
    let (before, err, ok) = run_rigger(root, &["graph", "--around", file, "--depth", "2"]);
    assert!(ok, "graph must succeed; stderr: {err}");
    let before_ids = graph_node_ids(&before);
    for id in [
        "pre-d",
        "pre-f",
        "pre-lesson",
        "r1-d",
        "r1-f",
        "r1-lesson",
        "shared-d",
        "r2-d",
        "r2-f",
        "r2-lesson",
    ] {
        assert!(
            before_ids.iter().any(|n| n == id),
            "{id} must be in the graph before reset; got: {before_ids:?}"
        );
    }

    // Reset: drop the dead-run noise. Exactly the four superseded/pre-boundary
    // decisions/findings (pre-d, pre-f, r1-d, r1-f) are pruned; `shared-d` is NOT (it is
    // reused by the active run r2), so the reported count is 4, not 5.
    let (out, err, ok) = run_rigger(root, &["reset", "--runs"]);
    assert!(ok, "reset --runs must succeed; stderr: {err}");
    assert!(
        out.contains("pruned 4"),
        "reset --runs must report pruning the 4 dead-run nodes; got: {out:?}"
    );

    // After the reset the superseded/pre-boundary decisions and findings are gone, but every
    // lesson (including the PRE-BOUNDARY one), the ACTIVE run's decision + finding, and the
    // cross-run-reused id all remain.
    let (after, err, ok) = run_rigger(root, &["graph", "--around", file, "--depth", "2"]);
    assert!(ok, "graph must succeed after reset; stderr: {err}");
    let after_ids = graph_node_ids(&after);
    for id in ["pre-d", "pre-f", "r1-d", "r1-f"] {
        assert!(
            !after_ids.iter().any(|n| n == id),
            "{id} must be pruned by reset --runs; got: {after_ids:?}"
        );
    }
    for id in [
        "r2-d",
        "r2-f",
        "shared-d",
        "pre-lesson",
        "r1-lesson",
        "r2-lesson",
        file,
    ] {
        assert!(
            after_ids.iter().any(|n| n == id),
            "{id} must survive reset --runs (active run + reused id + every lesson + the file); \
             got: {after_ids:?}"
        );
    }

    // The event log is UNTOUCHED - `rigger peers` (which re-projects the stream, not graph.db)
    // still surfaces the superseded decision. reset sheds GRAPH noise, it never wipes history.
    let (peers, err, ok) = run_rigger(root, &["peers", file]);
    assert!(ok, "peers must succeed after reset; stderr: {err}");
    assert!(
        peers.contains("decision r1-d"),
        "reset --runs must not wipe the event log - the superseded decision is still in the \
         stream; got: {peers:?}"
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

/// Spec 34, criterion 1 (per-spawn reclamation ON COMPLETION): rigger DELETES a spawn's
/// dedicated, rigger-assigned scratch dir under `.rigger/tmp` the MOMENT its result is
/// recorded - for EVERY outcome (a success, a reject verdict, an `--error`, and a
/// liveness/infra fault) - while a sibling spawn with NO recorded result keeps its scratch
/// untouched. All four outcomes reach the store through the SAME courier (`rigger result`,
/// [`cmd_result`]): the driver records even a liveness/infra fault as `--error` + `--meta
/// '{"liveness_class":"infra"}'` (see
/// `step_surfaces_a_hung_unbounded_spawn_recorded_as_a_liveness_fault_by_the_driver`), so
/// the reclaim keys off "a result was recorded", never the outcome TYPE. The scratch path is
/// the single authority `driver::replay::spawn_scratch_path`
/// (`<scratch_root>/agent-scratch/<run>/<sanitized id>`); the reclaim is `cmd_result`'s. The
/// "keeps its scratch" half falls out by construction: `cmd_result` only ever runs for the
/// spawn being reported, so a spawn with no result is never touched.
#[test]
fn a_spawns_scratch_is_reclaimed_the_moment_its_result_is_recorded_for_every_outcome() {
    // Each case is one terminal outcome in the exact courier shape the driver records it
    // with. A REJECT verdict is a plain (non-error) result whose text carries the verdict;
    // an `--error` is a charged failure; a liveness/infra fault is `--error` plus the
    // `liveness_class` meta that marks it no-charge.
    let cases: &[(&str, &[&str])] = &[
        ("success", &["result", "u/implementer#0", "did the work"]),
        (
            "reject-verdict",
            &["result", "u/implementer#0", r#"{"verdict":"reject"}"#],
        ),
        ("error", &["result", "u/implementer#0", "boom", "--error"]),
        (
            "liveness-fault",
            &[
                "result",
                "u/implementer#0",
                "worker hung past its wall clock",
                "--error",
                "--meta",
                r#"{"liveness_class":"infra"}"#,
            ],
        ),
    ];

    for (label, args) in cases {
        let dir = temp_project();
        let root = dir.path();
        seed_store(root);
        // A run is live, so the per-spawn scratch is run-scoped exactly as in production -
        // the reclaim recovers the run id from the store's latest `RunStarted`.
        seed_run_events(root, &[("RunStarted", r#"{"run":"r1","criteria":["c"]}"#)]);

        // The dedicated scratch rigger assigned each spawn, each populated with build debris.
        // The layout is the single authority `spawn_scratch_path`:
        // `<root>/.rigger/tmp/agent-scratch/<run>/<sanitized id>` (the `/` and `#` in a spawn
        // id collapse to `_`). One spawn will report (its scratch must be reclaimed); the
        // sibling never reports (its scratch must be untouched).
        let run_scratch = root
            .join(".rigger")
            .join("tmp")
            .join("agent-scratch")
            .join("r1");
        let done = run_scratch.join("u_implementer_0");
        let live = run_scratch.join("v_implementer_0");
        for d in [&done, &live] {
            std::fs::create_dir_all(d).unwrap();
            std::fs::write(d.join("cargo-target-debris.rlib"), [0u8; 64]).unwrap();
        }

        // Record the outcome for the DONE spawn through the real courier.
        let (out, err, ok) = run_rigger(root, args);
        assert!(
            ok,
            "[{label}] recording the result must succeed; stdout: {out:?} stderr: {err}"
        );

        // The just-completed spawn's scratch is GONE the moment its result landed...
        assert!(
            !done.exists(),
            "[{label}] a spawn's rigger-assigned scratch must be reclaimed the moment its \
             result is recorded; {} still exists",
            done.display()
        );
        // ...while the sibling with NO recorded result keeps its scratch untouched.
        assert!(
            live.exists() && live.join("cargo-target-debris.rlib").exists(),
            "[{label}] a spawn with no recorded result must keep its scratch; {} was wrongly \
             reclaimed",
            live.display()
        );
    }
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

/// `rigger emit --spawn <id>` stamps the emit with the EMITTING spawn's id
/// (`META_SPAWN`) at record time - the per-spawn correlation the runtime verdict-channel-
/// mismatch backstop (spec 18, unit 3) keys on so a NATIVE courier's approve is attributed
/// to its OWN spawn on replay, never a concurrent sibling's by shared-stream position. This
/// drives the REAL cli courier path (`cmd_emit`), not a pre-stamped store seed, so the flag
/// that PRODUCES the stamp the replay backstop matches is itself under test end to end.
#[test]
fn emit_spawn_flag_stamps_the_emitting_spawn_id() {
    use rigger::eventstore::sqlite::Store;
    use rigger::eventstore::{Direction, EventStore, Filter};

    let dir = temp_project();
    let root = dir.path();
    seed_store(root);

    // The exact shape a gating adjudicator's `rigger emit --spawn <id>` records out of process.
    let (out, err, ok) = run_rigger(
        root,
        &[
            "emit",
            "--spawn",
            "u/adjudicator#0",
            "DecisionMade",
            r#"{"id":"verdict","verdict":"approve"}"#,
        ],
    );
    assert!(ok, "a stamped emit must succeed; stderr: {err}");
    assert!(
        out.contains("emitted DecisionMade"),
        "the stamped emit prints the same confirmation; got: {out:?}"
    );

    // Read the recorded event back through the library: the --spawn id landed as META_SPAWN,
    // so `gating_spawn_emitted_approve` will correlate this approve to `u/adjudicator#0`
    // exactly (and to no sibling), on the replay driver the conductor folds it on.
    let db_path = root.join(".rigger").join("events.db");
    let backend = Store::open(db_path.to_str().unwrap()).unwrap();
    let events = backend
        .read_all(0, Direction::Forward, &Filter::default())
        .unwrap();
    let decision = events
        .iter()
        .find(|e| e.type_ == "DecisionMade")
        .expect("the stamped emit landed in the store");
    assert_eq!(
        decision.meta.get(rigger::conductor::META_SPAWN).map(String::as_str),
        Some("u/adjudicator#0"),
        "the --spawn id is recorded as META_SPAWN so the backstop correlates the approve by identity"
    );

    // A bare `--spawn` with no id is a clear usage error, never a silent unstamped emit.
    let (_out, err, ok) = run_rigger(root, &["emit", "--spawn"]);
    assert!(!ok, "a --spawn with no id must be a non-zero exit");
    assert!(
        err.contains("--spawn expects a spawn id"),
        "the error names the missing spawn id; got: {err:?}"
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

/// The `workflows/rigger.js` native-driver source, read at test time from the crate manifest
/// dir. The driver is embedded into the binary via `include_str!` (not reachable through the
/// crate API) and runs only under the workflow harness (top-level await, the injected
/// `agent`/`parallel`/`log` globals), so it cannot execute in the Rust test harness - we assert
/// on the file's bytes, the same convention the sibling driver fixtures use.
fn rigger_js_source() -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("workflows")
        .join("rigger.js");
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

/// Spec 19c, Unit 2 (a): the native driver must enforce an OUTER per-agent wall-clock so even an
/// UNBOUNDED-config spawn - one with no per-spawn `max_wall_clock`, which `rigger step`'s
/// liveness sweep can never time out - is abandoned-and-SURFACED after a bound instead of being
/// awaited forever, so a hung agent surfaces within a bounded time.
///
/// The driver runs only under the workflow harness and cannot execute here, so this is a source
/// fixture over the embedded driver (the campaign convention for driver-shaped proofs, like the
/// work-line render and workflow-drift fixtures). It pins the load-bearing structure, and does
/// so at a bar a no-op cannot pass: (1) an outer total-runtime cap constant and its racing
/// helper exist; (2) the cap is APPLIED and wired to the UNBOUNDED branch - the ELSE of the
/// bounded marker-staleness path, so a bounded spawn's precise watchdog (which deliberately
/// leaves a marker-fresh worker in-flight, spec 10 unit 3) is untouched; and (3) blowing the cap
/// abandons-and-surfaces the spawn as a no-charge infra LIVENESS fault recorded atomically
/// (`--if-absent`, never clobbering a self-report) so the next `rigger step` halts loudly. The
/// surfacing half is proven end-to-end in real Rust by
/// `step_surfaces_a_hung_unbounded_spawn_recorded_as_a_liveness_fault_by_the_driver`.
#[test]
fn native_driver_enforces_an_outer_wall_clock_that_surfaces_an_unbounded_spawn() {
    let src = rigger_js_source();

    // (1) The outer total-runtime ceiling constant and the helper that races against it exist.
    assert!(
        src.contains("OUTER_WALL_CLOCK_SEC"),
        "the driver must define an OUTER wall-clock ceiling constant"
    );
    assert!(
        src.contains("raceOuterWallClock"),
        "the driver must have a helper that races a spawn against the outer wall-clock"
    );

    // Isolate runWorker (its declaration up to the next top-level function) so the structural
    // assertions below stay pointed at the spawn-await logic and immune to unrelated edits.
    let rw_at = src
        .find("async function runWorker(")
        .expect("the driver must still define runWorker");
    let rw_end = src[rw_at..]
        .find("\nfunction stop(")
        .map(|off| rw_at + off)
        .expect("runWorker must be followed by the stop() helper");
    let run_worker = &src[rw_at..rw_end];

    // (2) The outer cap is actually APPLIED, and to the UNBOUNDED case: the bounded spawn rides
    // the marker-staleness watchdog and the outer cap is the ELSE branch, so the precise bounded
    // watchdog is left untouched (never contradicting spec 10 unit 3's marker-fresh semantics).
    let marker_at = run_worker
        .find("raceMarkerStaleness(ran, req.max_wall_clock")
        .expect("a BOUNDED spawn must still ride the marker-staleness watchdog");
    let outer_at = run_worker
        .find("raceOuterWallClock(ran, OUTER_WALL_CLOCK_SEC)")
        .expect("an UNBOUNDED spawn must ride the outer total-runtime wall-clock");
    assert!(
        marker_at < outer_at && run_worker[marker_at..outer_at].contains("} else {"),
        "the outer wall-clock must be the ELSE of the bounded marker-staleness branch, so it \
         applies to the unbounded-config spawn and not the bounded one"
    );

    // (3) Blowing the outer cap abandons-and-SURFACES the spawn as a no-charge infra LIVENESS
    // fault: the outer branch drives the SHARED fault courier with the `liveness_class:infra`
    // meta, and explains it fires because the spawn has no per-spawn bound (distinct from the
    // bounded marker-staleness `hung` path).
    let outer_branch_at = run_worker
        .find("outcome.kind === 'outer'")
        .expect("runWorker must handle the outer-wall-clock outcome");
    let hung_branch_at = run_worker[outer_branch_at..]
        .find("outcome.kind === 'hung'")
        .map(|off| outer_branch_at + off)
        .expect("the outer branch must precede the marker-staleness hung branch");
    let outer_branch = &run_worker[outer_branch_at..hung_branch_at];
    assert!(
        outer_branch.contains("recordFaultCourier(")
            && outer_branch.contains("liveness_class")
            && outer_branch.contains("no per-spawn max_wall_clock"),
        "the outer-wall-clock abandonment must record an infra LIVENESS fault via the shared \
         recordFaultCourier authority (stamping `liveness_class:infra`), explaining it fires \
         because the spawn has no per-spawn bound: {outer_branch}"
    );

    // (4) The fault recording is ONE authority, not a second parallel courier: recordFaultCourier
    // is the single place that records a fault atomically (`rigger result <id> --if-absent
    // --error`) and captures a courier that itself dies in the shared `fatal` sink - and BOTH
    // fault paths route through it (the outer-wall-clock `report-hung:` path and the dead-worker
    // `report-death:` path), so the concern is implemented once over the shared abstraction rather
    // than as the two near-verbatim couriers a naive port would duplicate.
    let helper_at = src
        .find("async function recordFaultCourier(")
        .expect("the driver must define a single shared fault-courier authority");
    let helper_end = src[helper_at..]
        .find("\nasync function runWorker(")
        .map(|off| helper_at + off)
        .expect("recordFaultCourier must be followed by runWorker");
    let helper = &src[helper_at..helper_end];
    assert!(
        helper.contains("rigger result ${req.id} --if-absent --error")
            && helper.contains("fatal.push("),
        "recordFaultCourier must record the fault atomically (`rigger result <id> --if-absent \
         --error`) and capture a courier that itself dies in the shared `fatal` sink: {helper}"
    );
    assert!(
        run_worker.contains("report-hung:") && run_worker.contains("report-death:"),
        "both the outer-wall-clock (report-hung) and the dead-worker (report-death) paths must \
         route through the shared fault courier, not a second parallel implementation"
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
    let dir = temp_git_project_with_commit();
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
        let body = format!(r#"{{"id":"{id}","output":"did {id}"}}"#);
        seed_run_events(root, &[("SpawnResult", body.as_str())]);
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
    // Nor does a clean convergence carry an escalated set (spec 19c, unit 1): both units are
    // `on_pass: none` terminal-by-design, never escalated, so the field is omitted and the
    // driver reads a clean completion - a wedge is surfaced ONLY when a unit escalated.
    assert!(
        !line.contains("escalated"),
        "a clean convergence (no escalated unit) must omit the escalated field; got: {line:?}"
    );
}

/// A single-unit workflow whose ONLY gate always FAILS (`bad: false`) with a remediation
/// bound of ONE (`defaults.max_retries: 1`), so the unit ESCALATES on its first failed gate
/// (`safety::remediate(0, 1)` escalates immediately). Repo-less and offline: the `nop`
/// grounder does no model work, the implementer is drained by a recorded `SpawnResult`, and
/// `isolation: none` keeps it off git. Drives spec 19c unit 1: a run that reaches a fixpoint
/// with an escalated unit.
fn write_failing_gate_escalating_workflow(root: &Path) {
    let rigger = root.join(".rigger");
    std::fs::create_dir_all(rigger.join("agents")).unwrap();
    std::fs::write(
        rigger.join("agents").join("worker.md"),
        "---\nid: worker\nmodel: sonnet\ntools: [Read, Edit]\nisolation: none\n---\nDo the unit.\n",
    )
    .unwrap();
    std::fs::write(
        rigger.join("workflow.yml"),
        r#"name: esctest
defaults:
  grounder: nop
  budget: 60
  max_retries: 1
gates:
  bad: { run: "false", kind: core }
stages:
  solo:
    agent: worker
    gates: [bad]
    on_pass: none
"#,
    )
    .unwrap();
}

/// A single-unit workflow whose gate is under `autonomy: manual`, so the stage PAUSES for human
/// review (§4.3) instead of running: `stage_paused_for_review` short-circuits `run_stage`, which
/// emits a `ManualReview` event and returns the unit PENDING without ever parking an implementer
/// spawn. The result is an empty pending frontier (no `SpawnRequested`) with NO hung spawn - so
/// the shared `terminal_and_no_live_worker` frontier+hung core reads TRUE - yet the run is
/// manual-review-pending, i.e. NOT converged and still advancing (a human will approve+integrate
/// on a later step). Drives spec 34 criterion 3's never-delete-live rail on a non-terminal pause.
fn write_manual_review_workflow(root: &Path) {
    let rigger = root.join(".rigger");
    std::fs::create_dir_all(rigger.join("agents")).unwrap();
    std::fs::write(
        rigger.join("agents").join("worker.md"),
        "---\nid: worker\nmodel: sonnet\ntools: [Read, Edit]\nisolation: none\n---\nDo the unit.\n",
    )
    .unwrap();
    std::fs::write(
        rigger.join("workflow.yml"),
        r#"name: manualtest
defaults:
  grounder: nop
  budget: 60
  autonomy: manual
gates:
  human: { run: "true", kind: core }
stages:
  solo:
    agent: worker
    gates: [human]
    on_pass: none
"#,
    )
    .unwrap();
}

/// Spec 19c, unit 1: a run that reaches a fixpoint with an ESCALATED unit must not
/// masquerade as a clean completion. `rigger step` carries the escalated/unintegrated set on
/// its printed `Step` (an `escalated` array, distinct from a clean `{"wave":[],"done":true}`)
/// so the thin driver stops LOUDLY on a wedged terminus - exactly as it already does for a
/// `halted` budget stop - naming the units instead of reporting success. The unit's only gate
/// always fails and `max_retries` is 1, so it escalates on its first failure and the run
/// converges around the terminal wedge.
#[test]
fn step_carries_the_escalated_set_when_a_fixpoint_is_reached_with_a_wedged_unit() {
    let dir = temp_repoless_project();
    let root = dir.path();
    write_failing_gate_escalating_workflow(root);

    // Step 1: the unit is ready, so its implementer parks in-flight; nothing has escalated
    // yet, so the escalated set is OMITTED from the wire.
    let (out, err, ok) = run_rigger(root, &["step"]);
    assert!(ok, "the first step must succeed; stderr: {err}");
    let line = out.trim();
    assert!(
        line.contains(r#""id":"solo/implementer#0""#) && line.contains(r#""done":false"#),
        "step 1 parks the implementer and is not done; got: {line:?}"
    );
    assert!(
        !line.contains("escalated"),
        "no unit has escalated yet, so the escalated field is omitted; got: {line:?}"
    );

    // Drain the implementer via a recorded SpawnResult (the `rigger result` channel).
    seed_run_events(
        root,
        &[(
            "SpawnResult",
            r#"{"id":"solo/implementer#0","output":"implemented the unit"}"#,
        )],
    );

    // Step 2: the implementer replays, the `bad` gate runs inline and FAILS, and with a
    // remediation bound of one the unit ESCALATES - it goes terminal and the run reaches a
    // fixpoint AROUND it. The step still exits 0 (an escalated terminus is a run outcome the
    // JSON carries, not a process error): the printed `Step` is `done:true` yet carries the
    // escalated set so the driver stops loudly rather than reading a wedge as convergence.
    let (out, err, ok) = run_rigger(root, &["step"]);
    assert!(
        ok,
        "a step that reaches an escalated fixpoint still prints its result and exits 0; stderr: {err}"
    );
    let line = out.trim();
    assert!(
        line.contains(r#""done":true"#),
        "every spawn now has a result, so the run has reached a fixpoint; got: {line:?}"
    );
    assert!(
        line.contains(r#""escalated":["solo"]"#),
        "a fixpoint reached with an escalated unit must carry it in the escalated set; got: {line:?}"
    );
}

/// Gap 13: a spawn-budget HALT must be LOUD, not indistinguishable from convergence.
/// `rigger step` prints a `halted` reason (distinct from a clean `{"wave":[],"done":true}`)
/// when the breaker trips, so the thin driver stops loudly on a starved run instead of
/// reporting success. Budget 1 with two independent units: one implementer spawn is admitted
/// and parked, the second is refused - the breaker trips and records the halt.
#[test]
fn step_prints_a_budget_halt_reason_when_the_breaker_trips() {
    let dir = temp_git_project_with_commit();
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
    let dir = temp_git_project_with_commit();
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

/// The single-stage liveness workflow with an UNBOUNDED default (`defaults.max_wall_clock`
/// absent = 0), so the parked implementer carries NO per-spawn `max_wall_clock` and thus no
/// marker on the wire - the exact spawn the sweep can never time out and the native driver's
/// OUTER wall-clock is the only backstop for (spec 19c, unit 2).
fn write_unbounded_liveness_workflow(root: &Path) {
    let rigger = root.join(".rigger");
    std::fs::create_dir_all(rigger.join("agents")).unwrap();
    std::fs::write(
        rigger.join("agents").join("worker.md"),
        "---\nid: worker\nmodel: sonnet\ntools: [Read, Edit]\nisolation: none\n---\nDo the unit.\n",
    )
    .unwrap();
    std::fs::write(
        rigger.join("workflow.yml"),
        "name: livetest\ndefaults:\n  grounder: nop\n  budget: 60\nstages:\n  a:\n    agent: worker\n    on_pass: none\n",
    )
    .unwrap();
}

/// Spec 19c, Unit 2 (a) - the SURFACING half, end-to-end in real Rust: a hung UNBOUNDED-config
/// spawn surfaces within a bounded time. Under an unbounded default the parked implementer
/// carries NO `max_wall_clock` (so no marker, and `rigger step`'s liveness SWEEP - which times
/// out only a positive bound - can never reach it). The native driver's OUTER wall-clock instead
/// records a LIVENESS fault on the spawn's behalf (`rigger result <id> --error ... --meta
/// '{"liveness_class":"infra"}'`); this test records exactly that fault via the CLI - the driver's
/// courier command shape - and proves the next `rigger step` SURFACES it as a loud halt (naming
/// the spawn, infra, no attempt charged), then re-surfaces it and never re-runs it, and that a
/// real result clears it. The DRIVER side (that the outer wall-clock records this fault) is the
/// source fixture `native_driver_enforces_an_outer_wall_clock_that_surfaces_an_unbounded_spawn`;
/// together they prove the criterion end to end without running the harness-only JS.
#[test]
fn step_surfaces_a_hung_unbounded_spawn_recorded_as_a_liveness_fault_by_the_driver() {
    let dir = temp_git_project_with_commit();
    let root = dir.path();
    write_unbounded_liveness_workflow(root);

    // Step 1: the implementer parks in-flight. Being UNBOUNDED it carries NO marker path - the
    // sweep has nothing to time out, which is exactly why the driver's outer wall-clock exists.
    let (out, err, ok) = run_rigger(root, &["step"]);
    assert!(ok, "the first step must succeed; stderr: {err}");
    let line = out.trim();
    assert!(
        line.contains(r#""id":"a/implementer#0""#),
        "step 1 parks the implementer in-flight; got: {line:?}"
    );
    assert!(
        json_string_field(line, "marker_path").is_none(),
        "an unbounded-config spawn carries no marker path (the sweep cannot time it out); got: {line:?}"
    );

    // The native driver's OUTER wall-clock fired: it records a LIVENESS fault on the spawn's
    // behalf, EXACTLY this CLI shape (`--error` + `--meta liveness_class:infra`). No sweep is
    // involved - the driver surfaces the hang itself.
    let (_o, err, ok) = run_rigger(
        root,
        &[
            "result",
            "a/implementer#0",
            "worker a/implementer#0 hung: ran past the outer wall-clock with no per-spawn max_wall_clock",
            "--error",
            "--meta",
            r#"{"liveness_class":"infra"}"#,
        ],
    );
    assert!(
        ok,
        "recording the driver's liveness fault must succeed; stderr: {err}"
    );

    // Step 2: `rigger step` reads that fault through `hung_spawns` and HALTS LOUDLY - so a hung
    // unbounded agent surfaces within a bounded time even though the sweep never touched it.
    let (out, err, ok) = run_rigger(root, &["step"]);
    assert!(
        ok,
        "a liveness-halted step still prints its result and exits 0; stderr: {err}"
    );
    let line = out.trim();
    assert!(
        line.contains(r#""halted":"#) && line.contains("a/implementer#0"),
        "the hung unbounded spawn must surface as a halt naming it; got: {line:?}"
    );
    assert!(
        line.contains("infra") && line.contains("no remediation attempt"),
        "the halt states infra classification and no-attempt-charged; got: {line:?}"
    );

    // Step 3: re-step without recording a real result - the fault ANSWERS the spawn, so it is
    // never re-run (no dup-exec) and the halt re-surfaces so the stall stays visible.
    let (out, err, ok) = run_rigger(root, &["step"]);
    assert!(ok, "the re-step must succeed; stderr: {err}");
    let line = out.trim();
    assert!(
        line.contains(r#""halted":"#) && line.contains("a/implementer#0"),
        "the halt re-surfaces on a later step; got: {line:?}"
    );
    assert!(
        json_string_field(line, "marker_path").is_none() && !line.contains(r#""wave":[{"#),
        "the answered hung spawn is not re-run (no fresh wave item / dup-exec); got: {line:?}"
    );

    // Step 4: recording a REAL result (last-write-wins) supersedes the fault and the run converges.
    let (_o, err, ok) = run_rigger(
        root,
        &["result", "a/implementer#0", "recovered by operator"],
    );
    assert!(ok, "recording a real result must succeed; stderr: {err}");
    let (out, err, ok) = run_rigger(root, &["step"]);
    assert!(ok, "the recovery step must succeed; stderr: {err}");
    let line = out.trim();
    assert!(
        !line.contains(r#""halted":"#) && line.contains(r#""done":true"#),
        "a real result clears the halt and the run converges; got: {line:?}"
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
    let dir = temp_git_project_with_commit();
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
    let dir = temp_git_project_with_commit();
    let root = dir.path();
    write_two_stage_workflow(root);
    seed_store(root);

    // A prior campaign (DIFFERENT criteria) left an aborted, still-unanswered spawn in the
    // store: its `RunStarted` and a parked implementer with no result.
    seed_run_events(
        root,
        &[
            ("RunStarted", r#"{"run":"r0","criteria":["an older spec"]}"#),
            (
                "SpawnRequested",
                r#"{"id":"zombie/implementer#0","unit":"zombie","stage":"zombie","prompt":"stale"}"#,
            ),
        ],
    );

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

/// Spec 34 (criterion 2), done-when line 65: the ORPHAN-SWEEP backstop. Driving the real
/// `rigger step` proves the end-to-end wiring - config load -> store -> scratch-root ->
/// reclaim - reclaims scratch under `.rigger/tmp` that no LIVE unit of the run this step
/// starts owns (a prior run's killed-process leftover worktree, and an ad-hoc
/// `cargo-target-<slug>` an agent wrote outside its assigned path - the unbounded per-agent
/// build-cache leak) while SPARING the shared `agent-scratch` area an in-flight worker is
/// still using. The liveness-keyed live-unit-vs-dead-unit sparing is unit-tested precisely in
/// `src/main.rs` (`reclaim_orphan_scratch_removes_non_live_owned_scratch_...`); this pins the
/// wiring through the compiled binary. A non-done first step never triggers the fixpoint
/// scratch reclaim, so agent-scratch survives only because the orphan-sweep spares it.
#[test]
fn step_reclaims_orphaned_scratch_while_sparing_the_live_worker_area() {
    let dir = temp_git_project_with_commit();
    let root = dir.path();
    write_two_stage_workflow(root);
    seed_store(root);

    // A controlled scratch root, so the reclaim is hermetic (mirrors the residue test).
    let scratch = root.join("scratchroot");
    let tmp = scratch.to_str().unwrap();

    // Non-live-owned scratch a prior/killed run stranded under the scratch root: an ad-hoc
    // per-agent build cache and a leftover unit worktree, neither owned by any live unit of
    // the fresh run this step begins.
    let orphan_cache = scratch.join("cargo-target-orphan-abc123");
    std::fs::create_dir_all(&orphan_cache).unwrap();
    std::fs::write(orphan_cache.join("junk.rlib"), [0u8; 64]).unwrap();
    let orphan_wt = scratch.join("rigger-wt-old-run-deadbeef");
    std::fs::create_dir_all(&orphan_wt).unwrap();
    std::fs::write(orphan_wt.join("leftover.txt"), [0u8; 32]).unwrap();

    // The live-shared worker area an in-flight spawn parks probe repos / builds under: MUST be
    // spared (a running spawn may still be writing into it) - the never-delete-live-owned rail.
    let worker_area = scratch.join("agent-scratch").join("probe");
    std::fs::create_dir_all(&worker_area).unwrap();
    std::fs::write(worker_area.join("Cargo.toml"), b"[package]").unwrap();

    let (out, err, ok) = run_rigger_envs(root, &["step"], &[("RIGGER_TMPDIR", tmp)]);
    assert!(ok, "the step must succeed; stderr:\n{err}");
    assert!(
        !out.trim().is_empty(),
        "a non-done first step still prints its wave; stdout:\n{out}"
    );

    assert!(
        !orphan_cache.exists(),
        "the orphan-sweep reclaims an ad-hoc cargo-target no live unit owns; it survived under {tmp}\nstderr:\n{err}"
    );
    assert!(
        !orphan_wt.exists(),
        "the orphan-sweep reclaims a prior run's leftover worktree; it survived under {tmp}\nstderr:\n{err}"
    );
    assert!(
        scratch.join("agent-scratch").exists(),
        "the orphan-sweep must SPARE the live worker area agent-scratch; it was wrongly reclaimed\nstderr:\n{err}"
    );
}

/// Plant the run-LEVEL shared scratch areas the terminal-state teardown (spec 34 c3) owns under
/// `scratch`: the SHARED build cache (`cargo-target` + `target` directly under the root - the
/// driver's `CARGO_TARGET_DIR`, the unbounded multi-GB leak), `agent-scratch` (probe repos +
/// verify builds a worker parks there), and `agent-live` (per-spawn liveness markers). These are
/// exactly what the orphan-sweep backstop (c2) deliberately SPARES while the run is stepping, so
/// only the run-level teardown ever reclaims them.
fn plant_run_level_scratch(scratch: &Path) {
    let cache = scratch.join("cargo-target");
    std::fs::create_dir_all(&cache).unwrap();
    std::fs::write(cache.join("incremental.bin"), [0u8; 64]).unwrap();
    let target = scratch.join("target");
    std::fs::create_dir_all(&target).unwrap();
    std::fs::write(target.join("debug.bin"), [0u8; 64]).unwrap();
    let probe = scratch.join("agent-scratch").join("probe");
    std::fs::create_dir_all(&probe).unwrap();
    std::fs::write(probe.join("Cargo.toml"), b"[package]").unwrap();
    let marker = scratch.join("agent-live").join("run").join("spawn");
    std::fs::create_dir_all(marker.parent().unwrap()).unwrap();
    std::fs::write(&marker, b"heartbeat").unwrap();
}

/// Assert the run-level shared scratch planted by [`plant_run_level_scratch`] is ALL still
/// present (the never-delete-live rail: while a spawn is live the teardown must not fire).
fn assert_run_level_scratch_spared(scratch: &Path, ctx: &str) {
    for area in ["cargo-target", "target", "agent-scratch", "agent-live"] {
        assert!(
            scratch.join(area).exists(),
            "{ctx}: a live-spawn step must SPARE the run-level {area}; it was wrongly reclaimed"
        );
    }
}

/// Assert the run-level shared scratch planted by [`plant_run_level_scratch`] is ALL reclaimed
/// (the terminal teardown fired). Includes the SHARED build cache the orphan-sweep spares.
fn assert_run_level_scratch_reclaimed(scratch: &Path, ctx: &str) {
    for area in ["cargo-target", "target", "agent-scratch", "agent-live"] {
        assert!(
            !scratch.join(area).exists(),
            "{ctx}: the terminal-state teardown must reclaim the run-level {area}; it survived under {}",
            scratch.display()
        );
    }
}

/// Spec 34 (criterion 3), done-when line 68: RUN TEARDOWN reclaims run-level scratch for a
/// WEDGE/ESCALATION terminal state. A run that reaches a fixpoint AROUND an escalated unit is
/// terminal (not a clean completion), and rigger must leave no shared build cache or agent
/// scratch behind - the leak that let wedged runs accumulate gigabytes of build debris. The
/// never-delete-live rail is proven too: while the implementer is still in flight (step 1, a
/// pending wave) the shared areas are SPARED - a live spawn may still be building into them -
/// and only the terminal step (step 2, the escalation fixpoint, no live spawn) reclaims them.
/// The SHARED build cache (`cargo-target`/`target`) is what the orphan-sweep deliberately
/// spares, so its reclamation here is uniquely this run-level teardown's job.
#[test]
fn run_teardown_reclaims_run_level_scratch_at_an_escalation_terminal_state() {
    let dir = temp_git_project_with_commit();
    let root = dir.path();
    write_failing_gate_escalating_workflow(root);

    let scratch = root.join("scratchroot");
    let tmp = scratch.to_str().unwrap();
    std::fs::create_dir_all(&scratch).unwrap();
    plant_run_level_scratch(&scratch);

    // Step 1: the unit's implementer parks in flight - a LIVE spawn (no recorded result yet),
    // so the run is NOT terminal and the teardown must NOT fire: every planted area is spared.
    let (out, err, ok) = run_rigger_envs(root, &["step"], &[("RIGGER_TMPDIR", tmp)]);
    assert!(ok, "the first step must succeed; stderr:\n{err}");
    assert!(
        out.contains(r#""done":false"#),
        "step 1 parks the implementer and is not done; got: {out:?}"
    );
    assert_run_level_scratch_spared(&scratch, "step 1 (implementer in flight)");

    // Drain the implementer via a recorded result, so the next step replays it, runs the
    // always-failing gate, and with max_retries 1 the unit ESCALATES into a terminal fixpoint.
    seed_run_events(
        root,
        &[(
            "SpawnResult",
            r#"{"id":"solo/implementer#0","output":"implemented the unit"}"#,
        )],
    );

    // Step 2: the run reaches a fixpoint AROUND the escalated unit - terminal, no live spawn.
    // The teardown reclaims every run-level shared area, including the SHARED build cache the
    // orphan-sweep spares.
    let (out, err, ok) = run_rigger_envs(root, &["step"], &[("RIGGER_TMPDIR", tmp)]);
    assert!(
        ok,
        "an escalation-fixpoint step still exits 0; stderr:\n{err}"
    );
    assert!(
        out.contains(r#""done":true"#) && out.contains(r#""escalated":["solo"]"#),
        "step 2 reaches a wedged (escalated) fixpoint; got: {out:?}"
    );
    assert_run_level_scratch_reclaimed(&scratch, "the escalation terminal state");
}

/// Spec 34 (criterion 3), done-when line 68: RUN TEARDOWN reclaims run-level scratch for a
/// BUDGET-HALT terminal state. A budget halt that leaves no spawn in flight (the breaker refused
/// the NEXT ready unit's implementer while every admitted spawn is already answered) is terminal:
/// rigger reclaims the run-level shared scratch - including the SHARED build cache - rather than
/// leaking it, exactly as it does on a clean fixpoint.
#[test]
fn run_teardown_reclaims_run_level_scratch_at_a_budget_halt_terminal_state() {
    let dir = temp_git_project_with_commit();
    let root = dir.path();
    write_budget_one_two_stage_workflow(root);

    let scratch = root.join("scratchroot");
    let tmp = scratch.to_str().unwrap();
    std::fs::create_dir_all(&scratch).unwrap();
    plant_run_level_scratch(&scratch);

    // Step 1: one unit's implementer is admitted and parks (a LIVE spawn); the other is refused
    // and the breaker trips. A pending wave means a live spawn: the areas are SPARED.
    let (out, err, ok) = run_rigger_envs(root, &["step"], &[("RIGGER_TMPDIR", tmp)]);
    assert!(ok, "the first step must succeed; stderr:\n{err}");
    let admitted = json_string_field(&out, "id")
        .filter(|id| id.ends_with("/implementer#0"))
        .unwrap_or_else(|| panic!("step 1 must park one implementer; got: {out:?}"));
    assert_run_level_scratch_spared(&scratch, "step 1 (an implementer in flight)");

    // Drain the admitted implementer. Step 2: it replays free (already recorded) and settles
    // terminal-by-design (`on_pass: none`); the OTHER unit's implementer would be the second
    // spawn against a budget of one, so it is REFUSED and the breaker halts the run with NO
    // spawn left in flight (an empty frontier - a genuine terminal state, no live worker).
    seed_run_events(
        root,
        &[(
            "SpawnResult",
            &format!(r#"{{"id":"{admitted}","output":"did the unit"}}"#),
        )],
    );
    let (out, err, ok) = run_rigger_envs(root, &["step"], &[("RIGGER_TMPDIR", tmp)]);
    assert!(ok, "a budget-halted step still exits 0; stderr:\n{err}");
    assert!(
        out.contains(r#""done":true"#) && out.contains(r#""halted":"#),
        "step 2 halts on budget with an empty frontier; got: {out:?}"
    );
    assert_run_level_scratch_reclaimed(&scratch, "the budget-halt terminal state");
}

/// Spec 34 (criterion 3), done-when line 68: RUN TEARDOWN reclaims run-level scratch for a
/// DEFINITION-DRIFT halt. A live run pins its definition at start; a mid-campaign prompt edit
/// drifts it and the next plain `rigger step` HALTS loudly (spec 13, unit 1). That halt is a
/// terminal state for the run process, so - when no spawn is still in flight - rigger reclaims
/// the run-level shared scratch before propagating the loud halt, leaving no build cache behind.
#[test]
fn run_teardown_reclaims_run_level_scratch_at_a_definition_drift_halt() {
    let dir = temp_git_project_with_commit();
    let root = dir.path();
    write_two_stage_workflow(root);

    let scratch = root.join("scratchroot");
    let tmp = scratch.to_str().unwrap();
    std::fs::create_dir_all(&scratch).unwrap();

    // Step 1 pins the run's definition and parks both units' implementers.
    let (_out, err, ok) = run_rigger_envs(root, &["step"], &[("RIGGER_TMPDIR", tmp)]);
    assert!(ok, "the first step must pin the definition; stderr:\n{err}");

    // Drain both implementers so the frontier is EMPTY, then step to a clean fixpoint (both units
    // are terminal-by-design). This clears any prior scratch via the clean-fixpoint teardown.
    seed_run_events(
        root,
        &[
            (
                "SpawnResult",
                r#"{"id":"a/implementer#0","output":"did a"}"#,
            ),
            (
                "SpawnResult",
                r#"{"id":"b/implementer#0","output":"did b"}"#,
            ),
        ],
    );
    let (_out, err, ok) = run_rigger_envs(root, &["step"], &[("RIGGER_TMPDIR", tmp)]);
    assert!(ok, "step 2 reaches a clean fixpoint; stderr:\n{err}");

    // Re-plant the run-level scratch, THEN drift the on-disk definition. The frontier is empty
    // (every spawn answered), so the drift halt is a genuine terminal state - no live spawn.
    plant_run_level_scratch(&scratch);
    edit_worker_prompt(root, "Do the unit, but differently now.");

    // Step 3 (no flag) HALTS on the drift (non-zero exit naming it). Before propagating the halt,
    // the terminal teardown reclaims the re-planted run-level scratch.
    let (out, err, ok) = run_rigger_envs(root, &["step"], &[("RIGGER_TMPDIR", tmp)]);
    assert!(
        !ok,
        "a drifted live-run step must HALT (non-zero exit); stdout: {out:?}"
    );
    assert!(
        err.contains("definition drift"),
        "the halt must name the definition drift; stderr:\n{err}"
    );
    assert_run_level_scratch_reclaimed(&scratch, "the definition-drift halt");
}

/// Spec 34 (criterion 3), the NEVER-DELETE-LIVE rail on the definition-drift teardown path. A
/// definition-drift halt reclaims run-level scratch ONLY when no worker is live - the SAME guard
/// the terminal fixpoint uses, so the two teardown sites can never diverge. The subtle live
/// worker is a HUNG-but-possibly-alive spawn: a marker-stale sweep recorded a liveness FAULT on
/// its id (an infra stall the worker never reported itself), which counts as "answered" so the
/// pending frontier is EMPTY (`done`) - yet the worker PROCESS may still be alive and writing
/// under the shared scratch, and the operator may yet recover it (record a real result, then
/// resume with `--rebase-definition`). So a drift halt while such a spawn exists must SPARE the
/// run-level scratch, exactly as the terminal fixpoint does (both gate on `hung.is_empty()`).
///
/// Regression guard for the drift path that once gated on the empty frontier ALONE: it would have
/// reclaimed the shared build cache and agent scratch out from under the hung-but-alive worker.
/// This case is what the earlier drift test (which drains every spawn to a CLEAN fixpoint, so no
/// hung spawn ever exists) could never exercise - the empty-frontier arm always fired there.
#[test]
fn run_teardown_spares_run_level_scratch_at_a_drift_halt_while_a_hung_spawn_may_be_alive() {
    let dir = temp_git_project_with_commit();
    let root = dir.path();
    write_two_stage_workflow(root);

    let scratch = root.join("scratchroot");
    let tmp = scratch.to_str().unwrap();
    std::fs::create_dir_all(&scratch).unwrap();

    // Step 1 pins the run's definition and parks both units' implementers (a/implementer#0,
    // b/implementer#0) as in-flight, recorded spawns.
    let (_out, err, ok) = run_rigger_envs(root, &["step"], &[("RIGGER_TMPDIR", tmp)]);
    assert!(ok, "the first step must pin the definition; stderr:\n{err}");

    // Answer BOTH parked spawns so the pending frontier is EMPTY (`done`) - but answer ONE with a
    // LIVENESS FAULT (the `meta.liveness_class` outcome a marker-stale sweep synthesizes for a
    // hung agent, spec 10 unit 3), not a worker-reported result. A fault counts as "answered" (so
    // the frontier is empty and the drift halt reads as terminal BY THE FRONTIER test alone), yet
    // `hung_spawns` still flags a/implementer#0 because its LATEST result is a liveness fault - the
    // exact asymmetry the never-delete-live guard exists for. b is answered with a real success.
    seed_run_events(
        root,
        &[
            (
                "SpawnResult",
                r#"{"id":"a/implementer#0","error":"a/implementer#0 hung past its max_wall_clock (no per-spawn heartbeat)","meta":{"liveness_class":"infra"}}"#,
            ),
            (
                "SpawnResult",
                r#"{"id":"b/implementer#0","output":"did b"}"#,
            ),
        ],
    );

    // Plant the run-level scratch a still-alive worker may be writing into, THEN drift the on-disk
    // definition. The frontier is empty, so the OLD (buggy) drift teardown - gated on the empty
    // frontier ALONE - would reclaim it; but the hung-but-alive spawn means a worker may still be
    // live, so the shared never-delete-live guard SPARES every run-level area.
    plant_run_level_scratch(&scratch);
    edit_worker_prompt(root, "Do the unit, but differently now.");

    // Step 3 (no flag) HALTS on the drift (non-zero exit naming it). Because a hung-but-alive
    // spawn is present, the terminal never-delete-live guard SPARES the run-level scratch.
    let (out, err, ok) = run_rigger_envs(root, &["step"], &[("RIGGER_TMPDIR", tmp)]);
    assert!(
        !ok,
        "a drifted live-run step must HALT (non-zero exit); stdout: {out:?}"
    );
    assert!(
        err.contains("definition drift"),
        "the halt must name the definition drift; stderr:\n{err}"
    );
    assert_run_level_scratch_spared(
        &scratch,
        "a definition-drift halt while a hung-but-possibly-alive spawn exists",
    );
}

/// Spec 34 (criterion 3), the NEVER-DELETE-LIVE rail on the terminal-fixpoint teardown for a
/// MANUAL-REVIEW pause. `autonomy: manual` on a gated stage is a first-class supported mode
/// (§4.3): the stage PAUSES awaiting a human, emitting a `ManualReview` event and returning the
/// unit pending WITHOUT ever parking an implementer spawn. So the pending frontier is EMPTY (no
/// `SpawnRequested` to answer) and no spawn is hung - the shared `terminal_and_no_live_worker`
/// frontier+hung core reads TRUE - yet the run is manual-review-pending: NOT converged, STILL
/// advancing, because a human will approve+integrate the unit on a later step. A still-advancing
/// run is exactly the case the teardown must SPARE: reclaiming the run-level shared scratch (the
/// multi-GB `cargo-target`/`target` build cache above all) out from under it would force a full
/// rebuild the instant the human resumes.
///
/// Regression guard for the terminal path that once gated on the frontier+hung core ALONE: a
/// manual-review pause slips past both (empty frontier, no hung fault) even though the run has
/// not reached any of criterion 3's enumerated terminal states (clean fixpoint / escalation /
/// definition-drift / budget halt). The manual-review exclusion is FOLDED INTO the shared
/// `terminal_and_no_live_worker` predicate (it projects the `manual_review` inbox from the scoped
/// events), so BOTH teardown sites inherit it - see
/// `run_teardown_spares_run_level_scratch_at_a_drift_halt_while_a_manual_review_is_pending` for the
/// drift-path twin. (The drift site reads the FULL stream, so a manual-review pause persisted on an
/// earlier step is pending there too, across the multi-step run - it is NOT exempt.)
#[test]
fn run_teardown_spares_run_level_scratch_at_a_manual_review_pause() {
    let dir = temp_git_project_with_commit();
    let root = dir.path();
    write_manual_review_workflow(root);

    let scratch = root.join("scratchroot");
    let tmp = scratch.to_str().unwrap();
    std::fs::create_dir_all(&scratch).unwrap();
    plant_run_level_scratch(&scratch);

    // One step: the manual-autonomy gate PAUSES the stage - a `ManualReview` is emitted and the
    // unit returns pending WITHOUT parking an implementer spawn, so the frontier is empty and no
    // spawn is hung. The run is manual-review-pending (not converged, still advancing), so the
    // terminal teardown must SPARE every run-level shared area including the build cache.
    let (out, err, ok) = run_rigger_envs(root, &["step"], &[("RIGGER_TMPDIR", tmp)]);
    assert!(
        ok,
        "a manual-review pause step still exits 0; stderr:\n{err}"
    );
    assert!(
        !out.contains(r#""id":"solo/implementer#0""#),
        "a manual-review pause parks NO implementer spawn; got: {out:?}"
    );
    assert_run_level_scratch_spared(
        &scratch,
        "a manual-review pause (the run is still advancing)",
    );
}

/// Spec 34 (criterion 3), the NEVER-DELETE-LIVE rail on the DEFINITION-DRIFT teardown path for a
/// MANUAL-REVIEW-pending run. This is the drift-path twin of the terminal-site manual-review case
/// above, and it exercises the arm the terminal-site test cannot: a manual-review pause that is
/// STILL pending when a later step HALTS on definition drift.
///
/// The reachability the whole case turns on: a manual-review pause emits a PERSISTED `ManualReview`
/// and leaves the unit pending WITHOUT parking a spawn, so the frontier stays empty and no spawn is
/// hung - the `terminal_and_no_live_worker` frontier+hung core reads TRUE. That pause persists in
/// the log ACROSS steps (`fold_manual_review_inbox` keeps the non-terminal unit in the inbox until
/// a human integrates it). So when the operator edits a prompt (definition drift) and the NEXT
/// plain `rigger step` HALTS in `enforce_definition_pin` BEFORE `conductor::run`, the drift
/// early-return teardown reads the SAME persisted `ManualReview` from the full stream it already
/// loads. The run is manual-review-pending = NOT converged = STILL ADVANCING, so its run-level
/// scratch (the multi-GB `cargo-target`/`target` build cache above all) MUST be spared - reclaiming
/// it would force a full rebuild the instant the human resumes.
///
/// Regression guard for the drift path that once gated on `terminal_and_no_live_worker` ALONE
/// (frontier+hung only): it omitted the manual-review exclusion the terminal site had, so a drift
/// halt at step N+1 wiped the build cache of a run paused at step N. The fix FOLDS the manual-review
/// inbox INTO `terminal_and_no_live_worker`, so BOTH teardown sites inherit the exclusion and can
/// never diverge - the false claim "no manual-review can be pending on the drift path" (true only
/// within one step process, false across the multi-step run the persisted log spans) is gone.
#[test]
fn run_teardown_spares_run_level_scratch_at_a_drift_halt_while_a_manual_review_is_pending() {
    let dir = temp_git_project_with_commit();
    let root = dir.path();
    write_manual_review_workflow(root);

    let scratch = root.join("scratchroot");
    let tmp = scratch.to_str().unwrap();
    std::fs::create_dir_all(&scratch).unwrap();
    plant_run_level_scratch(&scratch);

    // Step 1 pins the run's definition and PAUSES the solo stage for manual review: a `ManualReview`
    // is emitted and the unit returns pending WITHOUT parking an implementer spawn, so the frontier
    // is empty and no spawn is hung. The run is manual-review-pending, so the terminal teardown
    // spares every run-level area (proven by its own test above); the scratch survives step 1.
    let (out, err, ok) = run_rigger_envs(root, &["step"], &[("RIGGER_TMPDIR", tmp)]);
    assert!(
        ok,
        "the first step must pin the definition and pause for review; stderr:\n{err}"
    );
    assert!(
        !out.contains(r#""id":"solo/implementer#0""#),
        "a manual-review pause parks NO implementer spawn; got: {out:?}"
    );
    assert_run_level_scratch_spared(&scratch, "step 1 (a manual-review pause pins then pauses)");

    // Re-plant the run-level scratch, THEN drift the on-disk definition. The pause from step 1 is
    // still pending (no human has integrated the unit), and the frontier is empty (no spawn ever
    // parked), so the drift halt reads terminal BY THE FRONTIER+HUNG CORE - but the run is STILL
    // manual-review-pending, so the shared never-delete-live guard must SPARE every run-level area.
    plant_run_level_scratch(&scratch);
    edit_worker_prompt(root, "Do the unit, but differently now.");

    // Step 2 (no flag) HALTS on the drift (non-zero exit naming it). Because a manual-review pause
    // is still pending, the drift early-return teardown SPARES the re-planted run-level scratch -
    // exactly as the terminal fixpoint does (both now gate on the folded manual-review exclusion).
    let (out, err, ok) = run_rigger_envs(root, &["step"], &[("RIGGER_TMPDIR", tmp)]);
    assert!(
        !ok,
        "a drifted live-run step must HALT (non-zero exit); stdout: {out:?}"
    );
    assert!(
        err.contains("definition drift"),
        "the halt must name the definition drift; stderr:\n{err}"
    );
    assert_run_level_scratch_spared(
        &scratch,
        "a definition-drift halt while a manual-review pause is still pending",
    );
}

/// Spec 34 (criterion 3), the RECLAIM direction of the manual-review path: once a manual-review
/// pause is RESOLVED (the human approves and the unit integrates), the NEXT terminal step teardown
/// DOES reclaim the run-level scratch - the exclusion spares only a STILL-pending pause, never a
/// resolved one. This closes the sentinel arm the folded manual-review guard depends on:
/// `fold_manual_review_inbox` drops a terminal (integrated) unit from the inbox, so the projected
/// `manual_review` becomes empty and the teardown fires. A future change that stopped folding the
/// terminal exclusion would leak (spare forever); this test would catch it.
#[test]
fn run_teardown_reclaims_run_level_scratch_after_a_manual_review_is_integrated() {
    let dir = temp_git_project_with_commit();
    let root = dir.path();
    write_manual_review_workflow(root);

    let scratch = root.join("scratchroot");
    let tmp = scratch.to_str().unwrap();
    std::fs::create_dir_all(&scratch).unwrap();
    plant_run_level_scratch(&scratch);

    // Step 1: the solo stage pauses for manual review (a `ManualReview` is emitted, the unit stays
    // pending). The run is still advancing, so the teardown spares the run-level scratch.
    let (_out, err, ok) = run_rigger_envs(root, &["step"], &[("RIGGER_TMPDIR", tmp)]);
    assert!(
        ok,
        "the first step must pause the unit for manual review; stderr:\n{err}"
    );
    assert_run_level_scratch_spared(
        &scratch,
        "step 1 (the manual-review pause is still pending)",
    );

    // The human approves and integrates the paused unit: a `UnitIntegrated` lands it. This is the
    // action-needed inbox emptying - `fold_manual_review_inbox` drops the now-terminal unit, so the
    // projected `manual_review` becomes empty and the run reaches a clean, genuinely terminal
    // fixpoint on the next step.
    seed_run_events(
        root,
        &[("UnitIntegrated", r#"{"id":"solo","commit":"deadbeef"}"#)],
    );

    // Re-plant the run-level scratch, then step: the resolved unit is terminal (no re-pause), the
    // manual-review inbox is empty, and the run is genuinely done - so the terminal teardown
    // reclaims every run-level area, including the SHARED build cache.
    plant_run_level_scratch(&scratch);
    let (out, err, ok) = run_rigger_envs(root, &["step"], &[("RIGGER_TMPDIR", tmp)]);
    assert!(
        ok,
        "a step after the manual review is integrated still exits 0; stderr:\n{err}"
    );
    assert!(
        out.contains(r#""done":true"#),
        "with the sole unit integrated the run reaches a clean fixpoint; got: {out:?}"
    );
    assert_run_level_scratch_reclaimed(
        &scratch,
        "the terminal state after a manual-review pause is integrated",
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
    seed_run_events(
        root,
        &[
            ("RunStarted", r#"{"run":"r1","criteria":["spec one"]}"#),
            ("UnitStarted", r#"{"id":"u1","agent":"worker"}"#),
            ("UnitIntegrated", r#"{"id":"u1","commit":"aaa"}"#),
            // Run 2: one unit that escalates to a human.
            ("RunStarted", r#"{"run":"r2","criteria":["spec two"]}"#),
            ("UnitStarted", r#"{"id":"u2","agent":"worker"}"#),
            ("UnitEscalated", r#"{"id":"u2"}"#),
        ],
    );

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

    let dir = temp_git_project_with_commit();
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

    let dir = temp_git_project_with_commit();
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

    let dir = temp_git_project_with_commit();
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

/// Loop-readiness gate (spec 38, criterion 2): on a repo with NO reachable base at all - an
/// UNBORN HEAD (no commit to fall back to) AND an unresolvable base - `rigger step` must FAIL
/// LOUDLY rather than mint a run branch that branches from nowhere (an orphan history a pull
/// request cannot apply to). This is the deliberate contrast to
/// `step_creates_run_branch_off_head_when_base_unresolvable`: there a REAL HEAD is a reachable
/// base and the run PROCEEDS off it; here there is nothing to base on, so the run stops. The
/// refusal is side-effect-free - no run branch is created - so a corrected retry anchors fresh.
#[test]
fn step_refuses_when_there_is_no_reachable_base() {
    // `temp_project()` is a `git init` with NO commit: an unborn HEAD, nothing to branch from.
    let dir = temp_project();
    let root = dir.path();
    write_two_stage_workflow(root);
    let head_branch_before = git_out(root, &["symbolic-ref", "--short", "-q", "HEAD"]);

    // An unresolvable base + the unborn HEAD => no reachable base at all.
    let (out, err, ok) = run_rigger(root, &["step", "--base", "origin/does-not-exist"]);
    assert!(
        !ok,
        "a run with no reachable base must fail loudly; stdout: {out:?} stderr: {err:?}"
    );
    assert!(
        err.contains("no reachable base") && err.contains("--base"),
        "the refusal must name the missing base and point at --base; got: {err:?}"
    );

    // Side-effect-free: no run branch was minted, so HEAD is untouched (still the unborn
    // default branch, never rigger-run) and the corrected retry can anchor the run fresh.
    assert_ne!(
        git_out(root, &["symbolic-ref", "--short", "-q", "HEAD"]).as_deref(),
        Some("rigger-run"),
        "a refused run must NOT have created or checked out the run branch"
    );
    assert_eq!(
        git_out(root, &["symbolic-ref", "--short", "-q", "HEAD"]),
        head_branch_before,
        "the refused run leaves HEAD exactly where it was"
    );
}

/// Loop-readiness gate (spec 38, criterion 2), periphery wiring for `rigger run`: the same
/// no-reachable-base refusal `rigger step` enforces is wired into the default `cli` driver's
/// entry (`run_cli`), labelled `rigger run`. On a repo with an UNBORN HEAD (no commit to fall
/// back to) AND an unresolvable base, `rigger run` must FAIL LOUDLY instead of minting a run
/// branch that branches from nowhere. The gate is one shared function, but each entry point
/// calls it at its OWN site: a missing call here is an independent boundary bug the shared
/// unit test cannot catch, so this drives the built binary through `rigger run` and pins the
/// `rigger run` label to prove that this call-site - not another - fired.
#[test]
fn run_refuses_when_there_is_no_reachable_base() {
    // `temp_project()` is a `git init` with NO commit: an unborn HEAD, nothing to branch from.
    let dir = temp_project();
    let root = dir.path();
    write_two_stage_workflow(root);
    let head_branch_before = git_out(root, &["symbolic-ref", "--short", "-q", "HEAD"]);

    // An unresolvable base + the unborn HEAD => no reachable base at all.
    let (out, err, ok) = run_rigger(root, &["run", "--base", "origin/does-not-exist"]);
    assert!(
        !ok,
        "`rigger run` with no reachable base must fail loudly; stdout: {out:?} stderr: {err:?}"
    );
    assert!(
        err.contains("rigger run") && err.contains("no reachable base") && err.contains("--base"),
        "the refusal must carry the `rigger run` label, name the missing base, and point at \
         --base; got: {err:?}"
    );

    // Side-effect-free: no run branch was minted, so HEAD is untouched (still the unborn
    // default branch, never rigger-run) and the corrected retry can anchor the run fresh.
    assert_ne!(
        git_out(root, &["symbolic-ref", "--short", "-q", "HEAD"]).as_deref(),
        Some("rigger-run"),
        "a refused `rigger run` must NOT have created or checked out the run branch"
    );
    assert_eq!(
        git_out(root, &["symbolic-ref", "--short", "-q", "HEAD"]),
        head_branch_before,
        "the refused `rigger run` leaves HEAD exactly where it was"
    );
}

/// Loop-readiness gate (spec 38, criterion 2), periphery wiring for the workflow driver: the
/// `run_workflow` entry (reached by `rigger run --driver workflow`, the served-conductor path
/// `rigger workflow` funnels through) enforces the SAME no-reachable-base refusal, labelled
/// `rigger workflow`. The refusal fires BEFORE the workflow driver, store, or sidecar start,
/// so it is provable through the binary WITHOUT the Node driver. A missing call at this third
/// call-site is an independent boundary bug; this drives the binary through the workflow
/// driver and pins the `rigger workflow` label to prove that this call-site fired.
#[test]
fn run_workflow_refuses_when_there_is_no_reachable_base() {
    // `temp_project()` is a `git init` with NO commit: an unborn HEAD, nothing to branch from.
    let dir = temp_project();
    let root = dir.path();
    write_two_stage_workflow(root);
    let head_branch_before = git_out(root, &["symbolic-ref", "--short", "-q", "HEAD"]);

    // An unresolvable base + the unborn HEAD => no reachable base at all.
    let (out, err, ok) = run_rigger(
        root,
        &[
            "run",
            "--driver",
            "workflow",
            "--base",
            "origin/does-not-exist",
        ],
    );
    assert!(
        !ok,
        "`rigger run --driver workflow` with no reachable base must fail loudly; \
         stdout: {out:?} stderr: {err:?}"
    );
    assert!(
        err.contains("rigger workflow")
            && err.contains("no reachable base")
            && err.contains("--base"),
        "the refusal must carry the `rigger workflow` label, name the missing base, and point \
         at --base; got: {err:?}"
    );

    // Side-effect-free: no run branch was minted and the workflow driver never started, so
    // HEAD is untouched (never rigger-run) and the corrected retry anchors the run fresh.
    assert_ne!(
        git_out(root, &["symbolic-ref", "--short", "-q", "HEAD"]).as_deref(),
        Some("rigger-run"),
        "a refused workflow run must NOT have created or checked out the run branch"
    );
    assert_eq!(
        git_out(root, &["symbolic-ref", "--short", "-q", "HEAD"]),
        head_branch_before,
        "the refused workflow run leaves HEAD exactly where it was"
    );
}

/// `rigger workflow <spec> --base <ref>` ACCEPTS `--base` (spec 18, criterion 6): the
/// command an operator naturally reaches for no longer rejects the flag with "expected at
/// most one spec path". The spec and the flag both parse, so the command proceeds past
/// argument handling to launching the JS driver (which, un-provisioned in this throwaway
/// project, fails with the setup hint - a DIFFERENT, expected error, proving `--base` was
/// accepted rather than silently rejected).
#[test]
fn workflow_accepts_a_spec_and_a_base_flag() {
    let dir = temp_project();
    let root = dir.path();

    let (_out, err, ok) = run_rigger(root, &["workflow", "specs/18.md", "--base", "my-feature"]);
    assert!(!ok, "the un-provisioned shim still fails the command");
    assert!(
        !err.contains("expected at most one spec path"),
        "rigger workflow must ACCEPT --base alongside a spec, not reject it; got: {err:?}"
    );
    // It got PAST argument parsing to the driver-launch step (which is un-provisioned here).
    assert!(
        err.contains("not provisioned") || err.contains("rigger setup"),
        "the failure must be the un-provisioned-driver error, proving --base parsed; got: {err:?}"
    );
}

/// `rigger run <spec> --base <ref>` ACCEPTS `--base` (spec 18, criterion 6): it is no
/// longer rejected as an "unknown flag". In this config-less throwaway project the run
/// fails later (loading the workflow config), but NOT at argument parsing - proving the
/// flag was accepted and threaded on, not rejected up front.
#[test]
fn run_accepts_a_base_flag() {
    let dir = temp_project();
    let root = dir.path();

    let (_out, err, ok) = run_rigger(root, &["run", "--base", "my-feature"]);
    assert!(!ok, "a config-less run still fails, but not on the flag");
    assert!(
        !err.contains("unknown flag"),
        "rigger run must ACCEPT --base, not reject it as an unknown flag; got: {err:?}"
    );
}

/// Spec 18, criterion 7: before a run parks its first unit, a run whose spec criteria
/// reference ONLY paths ABSENT from the base ref is REFUSED - the error names a missing
/// path and suggests `--base` - and a run whose referenced paths ARE present in the base
/// proceeds past the check. Driven through `rigger step --spec ... --base HEAD` (the
/// courier entry that anchors the run branch, then runs this check before touching the
/// store), in a FRESH repo so the anchor is `CreatedFromBase` - exactly "before a run parks
/// its first unit".
#[test]
fn step_refuses_a_base_lacking_every_spec_path_and_proceeds_when_present() {
    // -- REFUSE: the spec's only path token is absent from the (empty) HEAD tree.
    let dir = temp_git_project_with_commit();
    let root = dir.path();
    write_two_stage_workflow(root);
    std::fs::write(
        root.join("absent-spec.md"),
        "# S\n\n## Done when\n\n- [ ] the file crates/foo/src/bar.rs exports Zed\n",
    )
    .unwrap();
    let (_out, err, ok) = run_rigger(
        root,
        &["step", "--spec", "absent-spec.md", "--base", "HEAD"],
    );
    assert!(
        !ok,
        "a base lacking every spec-referenced path must refuse; stderr: {err}"
    );
    assert!(
        err.contains("crates/foo/src/bar.rs"),
        "the refusal must name a missing path; got: {err:?}"
    );
    assert!(
        err.contains("--base"),
        "the refusal must suggest --base; got: {err:?}"
    );
    assert!(
        err.contains("NONE of them exist in the base ref"),
        "the refusal must explain the wrong-base signal; got: {err:?}"
    );
    // The refusal fires BEFORE the run branch is anchored, so a refused step leaves NO
    // rigger-run behind - otherwise the corrected --base retry would reuse the wrong-base
    // branch and self-disarm the check (spec 18, criterion 7).
    assert!(
        git_out(
            root,
            &["rev-parse", "--verify", "-q", "refs/heads/rigger-run"]
        )
        .is_none(),
        "a refused step must NOT create the run branch (it would self-disarm the retry)"
    );

    // -- PROCEED: a FRESH repo whose spec references a path PRESENT in the base gets past the
    // check and on into the conductor (which then fails LATER on this minimal, verifier-less
    // workflow - a DIFFERENT error), proving the base check did not refuse a correct base.
    let dir2 = temp_git_project_with_commit();
    let root2 = dir2.path();
    write_two_stage_workflow(root2);
    std::fs::create_dir_all(root2.join("src")).unwrap();
    std::fs::write(root2.join("src").join("lib.rs"), "pub fn f() {}\n").unwrap();
    git_ok(root2, &["add", "src/lib.rs"]);
    git_ok(root2, &["commit", "-q", "-m", "add lib"]);
    std::fs::write(
        root2.join("present-spec.md"),
        "# S\n\n## Done when\n\n- [ ] touches `src/lib.rs` to export a thing\n",
    )
    .unwrap();
    let (_o2, err2, ok2) = run_rigger(
        root2,
        &["step", "--spec", "present-spec.md", "--base", "HEAD"],
    );
    assert!(
        !ok2,
        "the minimal workflow still fails LATER (coverage), just not at the base check; stderr: {err2}"
    );
    assert!(
        !err2.contains("NONE of them exist in the base ref"),
        "a base that contains the spec's paths must NOT be refused; got: {err2:?}"
    );
    assert!(
        err2.contains("conductor"),
        "the run must proceed PAST the base check into the conductor; got: {err2:?}"
    );
}

/// Spec 18, criterion 7 - the INSTRUCTED recovery actually re-anchors. The refusal tells the
/// operator to "pass --base <your-branch>"; obeying it must land the run on the corrected base,
/// not leave it stuck on the wrong one. This pins the self-disarm fix: because the base check
/// runs BEFORE the run branch is anchored, a refused first step creates NO rigger-run, so the
/// retry with the correct base re-runs the check (now passing) and anchors the run branch where
/// the spec's paths actually exist.
#[test]
fn step_missing_files_refusal_recovery_anchors_on_the_corrected_base() {
    let dir = temp_git_project_with_commit();
    let root = dir.path();
    write_two_stage_workflow(root);

    // Two bases off the empty init commit: `wrong` lacks src/lib.rs; `right` has it.
    let init = git_out(root, &["rev-parse", "HEAD"]).expect("the init commit resolves");
    git_ok(root, &["branch", "wrong", &init]);
    git_ok(root, &["checkout", "-q", "-b", "right"]);
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(root.join("src").join("lib.rs"), "pub fn f() {}\n").unwrap();
    git_ok(root, &["add", "src/lib.rs"]);
    git_ok(root, &["commit", "-q", "-m", "add lib on right"]);
    // Stand on a NON-run branch so the first step is a fresh-from-base anchor.
    git_ok(root, &["checkout", "-q", "wrong"]);
    std::fs::write(
        root.join("spec.md"),
        "# S\n\n## Done when\n\n- [ ] touches `src/lib.rs` to export a thing\n",
    )
    .unwrap();

    // STEP 1: the WRONG base lacks src/lib.rs -> refuse, and leave NO run branch behind.
    let (_o1, err1, ok1) = run_rigger(root, &["step", "--spec", "spec.md", "--base", "wrong"]);
    assert!(!ok1, "the wrong base must refuse; stderr: {err1}");
    assert!(
        err1.contains("NONE of them exist in the base ref"),
        "the refusal must fire on the wrong base; got: {err1:?}"
    );
    assert!(
        git_out(
            root,
            &["rev-parse", "--verify", "-q", "refs/heads/rigger-run"]
        )
        .is_none(),
        "the refused step must NOT create rigger-run (else the retry self-disarms)"
    );

    // STEP 2: obey the refusal and retry with the CORRECT base. The check passes (src/lib.rs
    // is present on `right`), and the run branch is anchored on `right`, NOT `wrong`.
    let (_o2, err2, _ok2) = run_rigger(root, &["step", "--spec", "spec.md", "--base", "right"]);
    assert!(
        !err2.contains("NONE of them exist in the base ref"),
        "the corrected base must pass the check, not re-refuse; got: {err2:?}"
    );
    assert!(
        git_out(
            root,
            &["rev-parse", "--verify", "-q", "refs/heads/rigger-run"]
        )
        .is_some(),
        "the corrected retry must create the run branch; stderr: {err2:?}"
    );
    // The run branch is anchored where the spec's path exists: src/lib.rs is in its tree
    // (it is absent from `wrong`), so the run did NOT stay stuck on the wrong base.
    let run_has_lib = Command::new("git")
        .args(["cat-file", "-e", "rigger-run:src/lib.rs"])
        .current_dir(root)
        .status()
        .expect("git must run")
        .success();
    assert!(
        run_has_lib,
        "the run branch must be anchored on the corrected base `right` (which has src/lib.rs)"
    );
    let wrong_has_lib = Command::new("git")
        .args(["cat-file", "-e", "wrong:src/lib.rs"])
        .current_dir(root)
        .status()
        .expect("git must run")
        .success();
    assert!(
        !wrong_has_lib,
        "sanity: the wrong base must lack src/lib.rs, so anchoring on it would omit the file"
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
    seed_run_events(
        root,
        &[(
            "SpawnResult",
            r#"{"id":"solo/implementer#0","output":"implemented the unit"}"#,
        )],
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
    seed_run_events(
        root,
        &[(
            "SpawnResult",
            r#"{"id":"solo/adjudicator#0","output":"{\"verdict\":\"approve\"}"}"#,
        )],
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

/// Spec 18, unit 3 done-when (line 43) driven END TO END on the PRODUCTION native driver
/// (`rigger step` / ReplayDriver) through the REAL courier CLI - `cmd_step` and the
/// `rigger emit --spawn <id>` stamping path, not a pre-stamped store seed. A gating
/// adjudicator that recorded its approve-shaped verdict via `rigger emit --spawn` (the
/// native courier path) and reported a substantive result carrying NO verdict line must
/// make the next `rigger step` HARD-ERROR with the result-channel fix message - the
/// emit-only-approve persona the backstop exists to catch, correlated to THIS spawn by the
/// META_SPAWN stamp its own `rigger emit --spawn` recorded. This is the whole loop the
/// prior rejects turned on: the stamping the workflow prompt threads (`--spawn <id>`) and
/// the ReplayDriver attribution, exercised together over the real binary.
#[test]
fn a_native_courier_emit_only_approve_hard_errors_the_next_step() {
    let dir = temp_repoless_project();
    let root = dir.path();
    write_gated_reviewed_workflow(root);

    // Step 1: the implementer parks.
    let (out, err, ok) = run_rigger(root, &["step"]);
    assert!(
        ok && out.contains(r#""id":"solo/implementer#0""#),
        "step 1 parks the implementer; stderr:{err} stdout:{out}"
    );
    seed_run_events(
        root,
        &[(
            "SpawnResult",
            r#"{"id":"solo/implementer#0","output":"implemented the unit"}"#,
        )],
    );

    // Step 2: the implementer replays, the gate runs, and the adjudicator parks.
    let (out, err, ok) = run_rigger(root, &["step"]);
    assert!(
        ok && out.contains(r#""id":"solo/adjudicator#0""#),
        "step 2 gates the unit and parks the adjudicator; stderr:{err} stdout:{out}"
    );

    // The adjudicator (running out of process) records its approve-shaped verdict as an
    // EVENT via the REAL `rigger emit --spawn <id>` courier command - STAMPED with its own
    // spawn id at record time, exactly as the native workflow prompt threads it.
    let (_o, err, ok) = run_rigger(
        root,
        &[
            "emit",
            "--spawn",
            "solo/adjudicator#0",
            "DecisionMade",
            r#"{"id":"verdict","verdict":"approve"}"#,
        ],
    );
    assert!(
        ok,
        "the stamped emit-only approve must record; stderr:{err}"
    );

    // ...then reports a substantive result on the RESULT channel carrying NO verdict line -
    // the exact mismatch: the persona put its verdict in the event channel, not the gate's.
    seed_run_events(
        root,
        &[(
            "SpawnResult",
            r#"{"id":"solo/adjudicator#0","output":"I have reviewed the unit and it looks good to me."}"#,
        )],
    );

    // Step 3: replaying the adjudicator's recorded result, `rigger step` HARD-ERRORS - the
    // stamped approve is correlated to THIS spawn by its META_SPAWN and the empty verdict is
    // caught, rather than folded as a silent reject-and-remediate. A non-zero exit with the
    // spec-pinned result-channel fix message on stderr.
    let (_out, err, ok) = run_rigger(root, &["step"]);
    assert!(
        !ok,
        "an emit-only-approve gating persona must fail the step, not advance the run; stderr:{err}"
    );
    assert!(
        err.contains("the gate reads the result channel, not emitted events")
            && err.contains("end your output with the verdict line"),
        "the step fails with the result-channel fix message; got stderr: {err:?}"
    );
    // The internal recognition sentinel never leaks to the operator's terminal.
    assert!(
        !err.contains('\u{1}'),
        "the internal mismatch marker is stripped from the surfaced message; got: {err:?}"
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
    seed_run_events(
        root,
        &[(
            "SpawnResult",
            r#"{"id":"solo/implementer#0","output":"implemented the unit"}"#,
        )],
    );
    let (out, err, ok) = run_rigger(root, &["step"]);
    assert!(
        ok && out.contains(r#""id":"solo/adjudicator#0""#),
        "baseline step 2 must park the adjudicator; stderr:{err} stdout:{out}"
    );
    seed_run_events(
        root,
        &[(
            "SpawnResult",
            r#"{"id":"solo/adjudicator#0","output":"{\"verdict\":\"approve\"}"}"#,
        )],
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

/// Spec 19c Unit 3: `rigger validate` WARNS (on stderr, without failing) when
/// `defaults.max_wall_clock` is unbounded and a gating role carries no per-agent bound - so
/// a hung gating agent that the liveness sweep never times out is visible at author time -
/// and stays SILENT on that risk once a bound covers the gating roles. The scaffolded config
/// leaves `defaults.max_wall_clock` at its `0` (unbounded) default and its adjudicator (a
/// gating role) sets no per-agent bound, so a fresh project trips the advisory.
#[test]
fn validate_warns_when_an_unbounded_default_leaves_a_gating_role_unswept() {
    let dir = temp_project();
    let root = dir.path();

    // Scaffold a valid config: `defaults.max_wall_clock` unset (0 = unbounded) and the
    // gating adjudicator carries no per-agent bound.
    let (_out, err, ok) = run_rigger(root, &["init"]);
    assert!(ok, "rigger init must succeed; stderr:\n{err}");

    // Unbounded default over an unbounded gating role -> WARN on stderr, but still exit 0.
    let (out, err, ok) = run_rigger(root, &["validate"]);
    assert!(
        ok,
        "validate must still succeed (exit 0) when it only WARNS about an unbounded \
         wall-clock; stderr:\n{err}"
    );
    assert!(
        out.contains("config valid"),
        "validate must still print its config summary; stdout:\n{out}"
    );
    assert!(
        err.contains("max_wall_clock")
            && err.contains("\"adjudicator\"")
            && err.to_lowercase().contains("swept"),
        "validate must warn on stderr that an unbounded default leaves the gating adjudicator \
         unswept, naming the role and the fix knob; stderr:\n{err}"
    );

    // Bound the default -> the gating roles are swept, so the wall-clock advisory is gone
    // (other advisories may remain; only this risk must clear). Still exit 0.
    let workflow = root.join(".rigger").join("workflow.yml");
    let bounded = std::fs::read_to_string(&workflow)
        .unwrap()
        .replace("budget: 60", "budget: 60\n  max_wall_clock: 600");
    assert!(
        bounded.contains("max_wall_clock: 600"),
        "test setup: expected to inject a bounded default into the scaffolded workflow"
    );
    std::fs::write(&workflow, bounded).unwrap();
    let (_out, err, ok) = run_rigger(root, &["validate"]);
    assert!(
        ok,
        "validate must succeed with a bounded default; stderr:\n{err}"
    );
    assert!(
        !err.contains("is never swept"),
        "validate must NOT warn about an unswept gating agent once the default is bounded; \
         stderr:\n{err}"
    );
}

/// Spec 18 Unit 4: `rigger validate <spec>` emits a NAMED, non-failing advisory for a
/// multi-behavior checkbox, an indented sub-bullet-as-unit, and an over-long criterion -
/// each naming its rule and recommending "one observable behavior per criterion" - and
/// emits NONE for a clean single-behavior spec. Driving the real binary proves the
/// `cmd_validate` spec-arg wiring; the pure heuristics are unit-tested in `src/spec.rs`.
#[test]
fn validate_spec_flags_shape_defects_as_named_advisories_and_is_silent_on_a_clean_spec() {
    let dir = temp_project();
    let root = dir.path();

    // A valid config so `rigger validate` succeeds (exit 0) and prints its config summary.
    let (_out, err, ok) = run_rigger(root, &["init"]);
    assert!(ok, "rigger init must succeed; stderr:\n{err}");

    // A spec whose criteria carry all three shape defects (in order):
    //   1: clean single behavior (must stay silent)
    //   2: multi-behavior (two clause coordinators)
    //   3: a plain indented sub-bullet that reads as its own criterion
    //   4: over-long (a verbatim planner copy would be unreliable)
    let long = "the coverage gate confirms every acceptance criterion is exercised by a dedicated \
         regression test "
        .repeat(3);
    let bad_spec = format!(
        "# Widget\n\n## Design\n\nsome prose\n\n## Done when\n\n\
         - [ ] the store passes the contract suite\n\
         - [ ] the daemon starts on boot, and it writes a pidfile, and it rotates the log nightly\n\
         - [ ] the projector records a decision\n\
         \x20\x20- and it supersedes the prior decision\n\
         - [ ] {long}\n"
    );
    let bad_path = root.join("bad-spec.md");
    std::fs::write(&bad_path, &bad_spec).unwrap();

    let (out, err, ok) = run_rigger(root, &["validate", bad_path.to_str().unwrap()]);
    assert!(
        ok,
        "spec-shape advisories are heuristic warnings, never a hard failure; stderr:\n{err}"
    );
    assert!(
        out.contains("config valid"),
        "validate must still print its config summary; stdout:\n{out}"
    );
    for rule in ["multi-behavior", "sub-bullet-as-unit", "over-long"] {
        assert!(
            err.contains(rule),
            "validate <spec> must emit a named `{rule}` advisory on stderr; stderr:\n{err}"
        );
    }
    assert!(
        err.contains("one observable behavior per criterion"),
        "each advisory must recommend the fix; stderr:\n{err}"
    );
    assert!(
        err.contains("mode 0644") || err.contains("supersedes the prior decision"),
        "the sub-bullet advisory must name the offending bullet; stderr:\n{err}"
    );

    // A clean single-behavior spec: NO spec-shape advisory at all.
    let clean_spec = "# Widget\n\n## Done when\n\n\
         - [ ] the store passes the contract suite\n\
         - [ ] the graph projector supersedes an older decision\n\
         - [ ] the conductor integrates an approved unit\n";
    let clean_path = root.join("clean-spec.md");
    std::fs::write(&clean_path, clean_spec).unwrap();

    let (_out, err, ok) = run_rigger(root, &["validate", clean_path.to_str().unwrap()]);
    assert!(ok, "validate must succeed on a clean spec; stderr:\n{err}");
    assert!(
        !err.contains("warning: spec "),
        "a clean single-behavior spec must yield no spec-shape advisory; stderr:\n{err}"
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

/// Spec 23 (unit 2), done-when line 60: `rigger validate` reports, as a warning-only advisory
/// that NEVER fails validation, any process whose cwd is under the scratch root - naming its
/// pid - and reports none once nothing is rooted there. Driving the real binary proves the
/// config -> residue-scan -> `/proc`-scan -> stderr wiring end to end; the pure formatter is
/// unit-tested in `src/main.rs`. On a platform without `/proc` the scan is a graceful no-op
/// (the shared scanner returns empty), so this behaves like the no-process case, never an error.
#[test]
fn validate_warns_about_a_process_rooted_under_the_scratch_root() {
    let dir = temp_project();
    let root = dir.path();
    write_two_stage_workflow(root); // a loadable config so validate reaches the advisories

    // A controlled scratch root (hermetic, like the residue test) with a child process whose
    // cwd is strictly inside it - the exact leak spec 23 surfaces.
    let scratch = root.join("scratchroot");
    let probe = scratch.join("probe");
    std::fs::create_dir_all(&probe).unwrap();
    let tmp = scratch.to_str().unwrap();
    let mut child = Command::new("sleep")
        .arg("300")
        .current_dir(&probe)
        .spawn()
        .expect("spawn probe child");

    // Wait until the kernel reports the child rooted under the scratch root, so the scan
    // validate runs is guaranteed to see it before we assert.
    let appeared = (0..200).any(|_| {
        if rigger::reap::processes_rooted_under(&scratch)
            .iter()
            .any(|(pid, _)| *pid == child.id())
        {
            return true;
        }
        std::thread::sleep(std::time::Duration::from_millis(25));
        false
    });
    assert!(
        appeared,
        "precondition: the probe child is rooted under the scratch root"
    );

    let (out, err, ok) = run_rigger_envs(root, &["validate"], &[("RIGGER_TMPDIR", tmp)]);

    // Reap the child, then re-run: with nothing rooted under the scratch root the advisory is
    // gone and validate still succeeds.
    let _ = child.kill();
    let _ = child.wait();
    let (_out2, err2, ok2) = run_rigger_envs(root, &["validate"], &[("RIGGER_TMPDIR", tmp)]);

    assert!(
        ok,
        "validate is warning-only (exit 0) even with a leaked process; stderr:\n{err}"
    );
    assert!(
        out.contains("config valid"),
        "validate still prints its config summary; stdout:\n{out}"
    );
    assert!(
        err.contains(&format!("pid {}", child.id())) && err.contains("scratch root"),
        "validate warns, naming the process rooted under the scratch root; stderr:\n{err}"
    );
    assert!(
        ok2,
        "validate still succeeds once nothing is rooted under the scratch root; stderr:\n{err2}"
    );
    assert!(
        !err2.contains(&format!("pid {}", child.id())),
        "validate emits no leaked-process advisory once the process is gone; stderr:\n{err2}"
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
    seed_run_events(
        root,
        &[
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
        ],
    );

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

/// Spec 19a unit 2 (setup discoverability): a `rigger setup` that reports a change ends with
/// an orientation block that names the three ways to drive a run - the blessed native
/// `/rigger <spec>` path (chosen from `/workflows`), the dashboard (`rigger dash` at its
/// `127.0.0.1:<DEFAULT_PORT>` URL, the port single-sourced from `dash::DEFAULT_PORT` so
/// source and fixture cannot drift), and `rigger workflow` / `rigger run` labelled as the
/// headless twins. The block is placed AFTER the silent-no-op early return, so a fully
/// up-to-date rerun that changes nothing stays quiet and does NOT re-print it.
#[test]
fn setup_output_names_the_blessed_path_dashboard_url_and_headless_twins() {
    let dir = temp_project();
    let root = dir.path();

    // A first setup on an empty repo actually changes things (scaffold + workflow + shim),
    // so it takes the reported-change path and prints the orientation block.
    let (out, err, ok) = run_rigger_envs(root, &["setup"], &[("RIGGER_NPM", "true")]);
    assert!(ok, "rigger setup must succeed; stderr:\n{err}");

    // 1. The blessed native path: `/rigger <spec>`, discoverable in `/workflows`.
    assert!(
        out.contains("/rigger <spec>") && out.contains("/workflows"),
        "setup output must name the blessed native /rigger <spec> path (visible in \
         /workflows); got:\n{out}"
    );
    // 2. The dashboard URL, with the port single-sourced from dash::DEFAULT_PORT.
    let dashboard_url = format!("127.0.0.1:{}", rigger::dash::DEFAULT_PORT);
    assert!(
        out.contains("rigger dash") && out.contains(&dashboard_url),
        "setup output must name the dashboard (rigger dash) and its {dashboard_url} URL; \
         got:\n{out}"
    );
    // 3. The headless twins.
    assert!(
        out.contains("rigger workflow")
            && out.contains("rigger run")
            && out.contains("headless twins"),
        "setup output must label rigger workflow / rigger run as the headless twins; \
         got:\n{out}"
    );

    // The block lives AFTER the silent-no-op early return: bring the repo fully up to date
    // (mark the shim install COMPLETE so its provision step is a no-op too), then a rerun
    // with nothing drifted must be quiet and re-print NONE of the orientation anchors.
    let marker = root
        .join(".rigger")
        .join("shim")
        .join("node_modules")
        .join(".package-lock.json");
    std::fs::create_dir_all(marker.parent().unwrap()).unwrap();
    std::fs::write(&marker, "{}").unwrap();

    let (out2, err2, ok2) = run_rigger_envs(root, &["setup"], &[("RIGGER_NPM", "true")]);
    assert!(ok2, "a rerun of rigger setup must succeed; stderr:\n{err2}");
    assert!(
        !out2.contains("/workflows")
            && !out2.contains(&dashboard_url)
            && !out2.contains("headless twins"),
        "a fully up-to-date rerun must stay quiet and NOT re-print the orientation block; \
         got:\n{out2}"
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
    let dir = temp_git_project_with_commit();
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
    let dir = temp_git_project_with_commit();
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
    let dir = temp_git_project_with_commit();
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

/// Spec 18, criterion 8 (build provenance): `rigger version` and `rigger --version` must
/// each report the crate version AND a build-provenance identifier - a git commit/describe
/// id embedded at build time by `build.rs`. Without a self-serve version an agent cannot tell
/// whether the installed binary matches the source, which is what makes the workflow-drift
/// warning ambiguous.
///
/// The build script's `cargo:rustc-env` applies to this integration-test crate too, so the
/// test can pin the exact embedded values: the crate version (`CARGO_PKG_VERSION`, identical
/// across the binary and this crate in one build) and the provenance token
/// (`RIGGER_BUILD_PROVENANCE`, which `build.rs` guarantees non-empty). Both invocations must
/// print BOTH, and must agree byte-for-byte so the two entry points cannot drift.
#[test]
fn version_and_dash_dash_version_report_crate_version_and_build_provenance() {
    let dir = temp_project();
    let root = dir.path();

    let crate_version = env!("CARGO_PKG_VERSION");
    let provenance = env!("RIGGER_BUILD_PROVENANCE");
    assert!(
        !provenance.is_empty(),
        "build.rs must embed a non-empty build-provenance id"
    );

    for invocation in [vec!["version"], vec!["--version"]] {
        let (out, err, ok) = run_rigger(root, &invocation);
        assert!(ok, "`rigger {invocation:?}` must exit 0; stderr:\n{err}");
        assert!(
            out.contains(crate_version),
            "`rigger {invocation:?}` must report the crate version {crate_version}; stdout:\n{out}"
        );
        assert!(
            out.contains(provenance),
            "`rigger {invocation:?}` must report the embedded build-provenance id {provenance}; stdout:\n{out}"
        );
    }

    // Both entry points route through one authority, so they print identical output.
    let (version_out, _, _) = run_rigger(root, &["version"]);
    let (flag_out, _, _) = run_rigger(root, &["--version"]);
    assert_eq!(
        version_out, flag_out,
        "`rigger version` and `rigger --version` must print the same line"
    );
}

/// Scaffold a `.rigger/` under `root` whose default review panel gates on a single
/// adjudicator agent `judge` carrying `adjudicator_body`, so `rigger validate` exercises
/// the gating-persona verdict-line lint (spec 18, unit 1) over a real config on disk.
fn write_gating_lint_project(root: &Path, adjudicator_body: &str) {
    let rigger = root.join(".rigger");
    std::fs::create_dir_all(rigger.join("agents")).unwrap();
    std::fs::write(
        rigger.join("agents").join("judge.md"),
        format!(
            "---\nid: judge\nmodel: sonnet\ntools: [Read]\nisolation: none\n---\n{adjudicator_body}\n"
        ),
    )
    .unwrap();
    std::fs::write(
        rigger.join("workflow.yml"),
        "name: linttest\ndefaults:\n  grounder: nop\n  review:\n    adjudicator: judge\n",
    )
    .unwrap();
}

/// spec 18, unit 1 (done-when): `rigger validate` HARD-errors on a config whose gating
/// adjudicator persona records its verdict ONLY via `rigger_emit` - never on its result
/// output - with a message naming the fix, and PASSES on an otherwise-identical config
/// whose persona ends its output with the verdict line. The integration gate reads a
/// gating spawn's RESULT channel for `{"verdict":...}` and never emitted events, so an
/// emit-only verdict is a guaranteed stall that this lint refuses up front instead of
/// letting it ferment into an escalation loop.
#[test]
fn validate_hard_errors_on_a_gating_persona_that_only_emits_its_verdict() {
    // Non-compliant: the `{"verdict"...}` literal appears only as the rigger_emit payload.
    let emit_only = temp_project();
    write_gating_lint_project(
        emit_only.path(),
        "You are the Adjudicator. Weigh the lenses against the adversary and decide. Record your \
         verdict via the rigger_emit tool with type Verdict and data {\"verdict\":\"approve\"} to \
         approve or {\"verdict\":\"reject\"} to reject. Do not add anything after you emit.",
    );
    let (_out, err, ok) = run_rigger(emit_only.path(), &["validate"]);
    assert!(
        !ok,
        "validate must HARD-error on a gating adjudicator whose only verdict path is rigger_emit; \
         stderr:\n{err}"
    );
    assert!(
        err.contains("judge") && err.contains("gating role") && err.contains("verdict line"),
        "the error names the offending agent and the defect; stderr:\n{err}"
    );
    assert!(
        err.contains("rigger_emit will never gate"),
        "the error names why an emit-only verdict never gates; stderr:\n{err}"
    );

    // Compliant: otherwise-identical, but the persona ENDS ITS OUTPUT with the verdict line.
    let result_line = temp_project();
    write_gating_lint_project(
        result_line.path(),
        "You are the Adjudicator. Weigh the lenses against the adversary and decide. Record your \
         reasoning via the rigger_emit tool as you go. End your output with a single line: \
         {\"verdict\":\"approve\"} to approve or {\"verdict\":\"reject\"} to reject.",
    );
    let (out, err, ok) = run_rigger(result_line.path(), &["validate"]);
    assert!(
        ok,
        "validate must PASS the otherwise-identical config whose persona ends with the verdict \
         line; stderr:\n{err}"
    );
    assert!(
        out.contains("config valid"),
        "a passing validate reports the config is valid; stdout:\n{out}"
    );

    // False-positive freedom (spec 18 unit 1's one hard promise, Design L32 / done-when L111):
    // a persona that DOES present the verdict as output must PASS even when its verdict clause
    // avoids the output whitelist ("Finish with the JSON {...}") and - as rigger's own
    // communication discipline requires of every gating persona - a rigger_emit instruction
    // sits in a neighbouring sentence. This is the class the previous heuristic false-flagged.
    let compliant_non_whitelisted = temp_project();
    write_gating_lint_project(
        compliant_non_whitelisted.path(),
        "You are the Adjudicator. Weigh the lenses against the adversary. Record every decision \
         the moment you make it via the rigger_emit tool. Finish with the JSON \
         {\"verdict\":\"approve\"} to approve or {\"verdict\":\"reject\"} to reject.",
    );
    let (out, err, ok) = run_rigger(compliant_non_whitelisted.path(), &["validate"]);
    assert!(
        ok,
        "validate must NOT false-positive a compliant persona whose verdict clause avoids the \
         output whitelist while an emit instruction sits in a neighbouring sentence; stderr:\n{err}"
    );
    assert!(
        out.contains("config valid"),
        "a passing validate reports the config is valid; stdout:\n{out}"
    );

    // Residual class (adj-u18-1 REJECT / adv-u18-1-residual-false-positive-same-clause-emit): the
    // unrelated emit instruction shares the SAME sentence as the verdict-output clause - an emit
    // word ("emit a DecisionMade") governing a DIFFERENT target must not bind a verdict that is
    // independently presented as output. No output-whitelist word appears, so this pins the
    // emit-payload binding itself; the prior clause-scoped fix still flagged it.
    let same_sentence_emit = temp_project();
    write_gating_lint_project(
        same_sentence_emit.path(),
        "You are the Adjudicator. Weigh the lenses against the adversary. You must emit a \
         DecisionMade for each call and your verdict must be {\"verdict\":\"approve\"} to approve \
         or {\"verdict\":\"reject\"} to reject.",
    );
    let (out, err, ok) = run_rigger(same_sentence_emit.path(), &["validate"]);
    assert!(
        ok,
        "validate must NOT false-positive a compliant persona whose verdict-output clause shares \
         its sentence with an unrelated emit instruction; stderr:\n{err}"
    );
    assert!(
        out.contains("config valid"),
        "a passing validate reports the config is valid; stdout:\n{out}"
    );

    // Payload-slot residual class (adj-u18-1rr / adv-u18-1rr-residual-fp-defeats-your-verdict-
    // escape): a determiner-`verdict` presentation with a payload noun in its span, and a natural
    // output verb outside the fixed cue list whose object is a common noun, both present the verdict
    // AS OUTPUT and must PASS. The prior binary FLAGGED each of these (a payload common-noun after
    // the emit bound a non-payload literal); none carries an output-cue word, so they pin the fix
    // over the real binary, not a whitelist coincidence.
    for persona in [
        "You are the Adjudicator. Emit each decision via rigger_emit and your verdict value is \
         {\"verdict\":\"approve\"} to approve or {\"verdict\":\"reject\"} to reject.",
        "You are the Adjudicator. Emit each decision via rigger_emit and the verdict payload is \
         {\"verdict\":\"approve\"} to approve or {\"verdict\":\"reject\"} to reject.",
        "You are the Adjudicator. Emit your reasoning via rigger_emit, then report the value \
         {\"verdict\":\"approve\"} to approve or {\"verdict\":\"reject\"} to reject.",
    ] {
        let compliant = temp_project();
        write_gating_lint_project(compliant.path(), persona);
        let (out, err, ok) = run_rigger(compliant.path(), &["validate"]);
        assert!(
            ok,
            "validate must NOT false-positive a compliant persona whose verdict clause carries a \
             payload noun in its span or a natural output verb; persona:\n{persona}\nstderr:\n{err}"
        );
        assert!(
            out.contains("config valid"),
            "a passing validate reports the config is valid; stdout:\n{out}"
        );
    }

    // Unrelated-emit-EXAMPLE-brace class (adj-u18-1r3 REJECT, FP#1, CONFIRMED BY RUNNING the prior
    // binary via `rigger validate`): a determiner-`verdict` presentation is on the result channel
    // even when an UNRELATED emit-payload EXAMPLE brace (`... data {id} ...`) shares its clause
    // EARLIER, before the `verdict` word. The prior binary scanned every brace, so a different
    // literal's `data {id}` example FALSELY FLAGGED the determiner-verdict escape - including the
    // EXACT wording rigger's own communication discipline mandates of every gating persona. None of
    // these carries an output cue, so they pin the span-scoped fix over the real binary.
    for persona in [
        "You are the Adjudicator. Record each decision via rigger_emit with data {id}, and your \
         verdict is {\"verdict\":\"approve\"} to approve or {\"verdict\":\"reject\"} to reject.",
        "You are the Adjudicator. Record every decision via rigger_emit with type DecisionMade and \
         data {id,summary}, then your verdict is {\"verdict\":\"approve\"} to approve or \
         {\"verdict\":\"reject\"} to reject.",
    ] {
        let compliant = temp_project();
        write_gating_lint_project(compliant.path(), persona);
        let (out, err, ok) = run_rigger(compliant.path(), &["validate"]);
        assert!(
            ok,
            "validate must NOT false-positive a determiner-verdict presentation preceded by an \
             unrelated emit-payload example brace in the same clause; persona:\n{persona}\n\
             stderr:\n{err}"
        );
        assert!(
            out.contains("config valid"),
            "a passing validate reports the config is valid; stdout:\n{out}"
        );
    }
}

/// spec 18 unit 1 (adj-u18-1r3 REJECT, FP#2), over the real `rigger validate` binary: the conductor
/// builds the plan-critique / DAG-critique gate adjudicator's prompt via `build_dag_critique_prompt`,
/// which ALWAYS appends the result-channel verdict line, so an emit-only DAG-critique adjudicator is
/// a NON-stall the lint must not flag. Flagging it would REFUSE a legitimate run (unit 2 escalates
/// the lint to a run-start refusal). The per-unit review adjudicator (whose `build_prompt` injects
/// nothing) stays linted, so a config whose per-unit adjudicator is emit-only still HARD-errors.
#[test]
fn validate_excludes_the_conductor_injected_plan_critique_gate_adjudicator() {
    // A DEDICATED emit-only DAG-critique adjudicator wired ONLY at the plan-critique gate, plus a
    // compliant per-unit review adjudicator: validate must PASS (the gate's line is injected).
    let excluded = temp_project();
    let rigger = excluded.path().join(".rigger");
    std::fs::create_dir_all(rigger.join("agents")).unwrap();
    std::fs::write(
        rigger.join("agents").join("dag-critic.md"),
        "---\nid: dag-critic\nmodel: sonnet\ntools: [Read]\nisolation: none\n---\nYou are the \
         plan-critique gate. Review the DAG and record your verdict via the rigger_emit tool with \
         type Verdict and data {\"verdict\":\"approve\"} to approve or {\"verdict\":\"reject\"} to \
         reject.\n",
    )
    .unwrap();
    std::fs::write(
        rigger.join("agents").join("planner.md"),
        "---\nid: planner\nmodel: sonnet\ntools: [Read]\nisolation: none\n---\nDecompose the spec. \
         End your output with {\"verdict\":\"approve\"}.\n",
    )
    .unwrap();
    std::fs::write(
        rigger.join("agents").join("judge.md"),
        "---\nid: judge\nmodel: sonnet\ntools: [Read]\nisolation: none\n---\nWeigh the review. End \
         your output with the verdict line {\"verdict\":\"approve\"}.\n",
    )
    .unwrap();
    std::fs::write(
        rigger.join("workflow.yml"),
        "name: linttest\n\
         defaults:\n  grounder: nop\n  review:\n    adjudicator: judge\n\
         stages:\n  \
         plan:\n    agent: planner\n    produces: dag\n  \
         plan-critique:\n    needs: [plan]\n    adjudicator: dag-critic\n",
    )
    .unwrap();
    let (out, err, ok) = run_rigger(excluded.path(), &["validate"]);
    assert!(
        ok,
        "validate must NOT flag an emit-only plan-critique gate adjudicator whose verdict line the \
         conductor injects; stderr:\n{err}"
    );
    assert!(
        out.contains("config valid"),
        "a passing validate reports the config is valid; stdout:\n{out}"
    );

    // If that SAME emit-only persona ALSO gates per-unit review (build_prompt injects nothing), it
    // is a real stall and validate HARD-errors - the exclusion is scoped to the gate role.
    let flagged = temp_project();
    let rigger2 = flagged.path().join(".rigger");
    std::fs::create_dir_all(rigger2.join("agents")).unwrap();
    std::fs::write(
        rigger2.join("agents").join("dag-critic.md"),
        "---\nid: dag-critic\nmodel: sonnet\ntools: [Read]\nisolation: none\n---\nReview the DAG \
         and record your verdict via the rigger_emit tool with type Verdict and data \
         {\"verdict\":\"approve\"} to approve or {\"verdict\":\"reject\"} to reject.\n",
    )
    .unwrap();
    std::fs::write(
        rigger2.join("agents").join("planner.md"),
        "---\nid: planner\nmodel: sonnet\ntools: [Read]\nisolation: none\n---\nDecompose the spec. \
         End your output with {\"verdict\":\"approve\"}.\n",
    )
    .unwrap();
    std::fs::write(
        rigger2.join("workflow.yml"),
        "name: linttest\n\
         defaults:\n  grounder: nop\n  review:\n    adjudicator: dag-critic\n\
         stages:\n  \
         plan:\n    agent: planner\n    produces: dag\n  \
         plan-critique:\n    needs: [plan]\n    adjudicator: dag-critic\n",
    )
    .unwrap();
    let (_out, err, ok) = run_rigger(flagged.path(), &["validate"]);
    assert!(
        !ok,
        "validate must still HARD-error when an emit-only adjudicator ALSO gates per-unit review; \
         stderr:\n{err}"
    );
    assert!(
        err.contains("dag-critic") && err.contains("verdict line"),
        "the error names the offending per-unit adjudicator; stderr:\n{err}"
    );
}

/// spec 18, unit 2 (done-when): a run entry (`config::load`) REFUSES to start on the same
/// non-compliant gating persona unit 1 hard-errors in `rigger validate`, with the SAME fix
/// message, and STARTS on the compliant one - rather than beginning a doomed run that stalls
/// once the integration gate reads the result channel and finds no verdict. `rigger run` and
/// `rigger step` share the run-config load seam (`load_run_config`), so the refusal fires
/// identically at both entries; the refusal precedes any repo/store/anchor work, so it needs
/// no git repo and leaves nothing behind. The compliant twin differs ONLY in the persona's
/// verdict-line presentation and is proven to load by the unit-1 validate fixture, so the
/// ABSENCE of the lint message on it means the run got PAST the refusal and started.
#[test]
fn a_run_refuses_to_start_on_an_emit_only_gating_persona_and_starts_on_the_compliant_one() {
    // The exact emit-only / result-line adjudicator personas the unit-1 validate fixture pins.
    const EMIT_ONLY: &str = "You are the Adjudicator. Weigh the lenses against the adversary and \
         decide. Record your verdict via the rigger_emit tool with type Verdict and data \
         {\"verdict\":\"approve\"} to approve or {\"verdict\":\"reject\"} to reject. Do not add \
         anything after you emit.";
    const RESULT_LINE: &str =
        "You are the Adjudicator. Weigh the lenses against the adversary and \
         decide. Record your reasoning via the rigger_emit tool as you go. End your output with a \
         single line: {\"verdict\":\"approve\"} to approve or {\"verdict\":\"reject\"} to reject.";

    // Assert a refusal carries the SAME fix message unit 1's lint emits (agent id + defect +
    // why an emit-only verdict never gates), proving the run entry reuses that one lint.
    let assert_refuses_with_fix_message = |entry: &str, err: &str, ok: bool| {
        assert!(
            !ok,
            "`rigger {entry}` must REFUSE to start on an emit-only gating adjudicator; stderr:\n{err}"
        );
        assert!(
            err.contains("judge") && err.contains("gating role") && err.contains("verdict line"),
            "`rigger {entry}` refusal names the offending agent and the defect; stderr:\n{err}"
        );
        assert!(
            err.contains("rigger_emit will never gate"),
            "`rigger {entry}` refusal names why an emit-only verdict never gates; stderr:\n{err}"
        );
    };

    // -- REFUSE via `rigger step`: the same defect that hard-errors `rigger validate` refuses the
    // run at its config-load seam, before it parks any unit.
    let emit_only_step = temp_project();
    write_gating_lint_project(emit_only_step.path(), EMIT_ONLY);
    let (_out, err, ok) = run_rigger(emit_only_step.path(), &["step"]);
    assert_refuses_with_fix_message("step", &err, ok);

    // -- REFUSE via `rigger run`: the OTHER standalone run entry (`run_cli`) shares the same load
    // seam, so it refuses identically. Pins the run_cli wiring e2e, not only the step one.
    let emit_only_run = temp_project();
    write_gating_lint_project(emit_only_run.path(), EMIT_ONLY);
    let (_out, err, ok) = run_rigger(emit_only_run.path(), &["run"]);
    assert_refuses_with_fix_message("run", &err, ok);

    // -- START via `rigger step`: the otherwise-identical compliant persona (it ENDS its output
    // with the verdict line) is NOT refused - the run gets past the load seam and begins. The
    // config is proven loadable by the unit-1 validate fixture, so the absence of the lint's
    // "gating role"/"verdict line" phrasing means the run started rather than refusing.
    let compliant_step = temp_project();
    write_gating_lint_project(compliant_step.path(), RESULT_LINE);
    let (out, err, _ok) = run_rigger(compliant_step.path(), &["step"]);
    assert!(
        !err.contains("gating role") && !err.contains("rigger_emit will never gate"),
        "the compliant persona must NOT be refused at run start; stdout:\n{out}\nstderr:\n{err}"
    );
}

/// A currently-free loopback TCP port, found by binding an ephemeral port and immediately
/// releasing it, so the spawned `rigger dash` binds successfully (never colliding with a
/// parallel test or a real dash on `DEFAULT_PORT`) and is therefore a genuinely long-lived
/// child rather than a process that exits on a bind conflict.
fn free_loopback_port() -> u16 {
    std::net::TcpListener::bind(("127.0.0.1", 0))
        .expect("bind an ephemeral loopback port")
        .local_addr()
        .expect("read the bound port")
        .port()
}

/// Spec 19b, unit 3 (no orphaned processes): a standalone long-lived `rigger` child - a
/// `rigger dash` - wrapped in the supervised [`rigger::dash::ReapedChild`] guard is KILLED
/// and REAPED when the guard is dropped, so a finishing (or crashing) driver leaves no
/// orphaned `rigger` process. The dash's piped stdout is a race-free liveness probe: it
/// stays open (a blocked read) while the dash lives, and reaches EOF only once the child is
/// reaped. This is the criterion proof `d19b-c3-reaping-scope` names - a standalone
/// `rigger dash` wrapped in the guard, dropped, asserted dead.
#[test]
fn a_dropped_guard_reaps_a_standalone_rigger_dash() {
    use rigger::dash::ReapedChild;
    use std::io::Read;
    use std::process::{Command, Stdio};
    use std::time::Duration;

    // A repo-less/empty-store dir is enough: `rigger dash` reads an ABSENT events.db as an
    // empty run and serves anyway, so it is a genuine long-lived child with no run seeded.
    let proj = temp_project();
    let port = free_loopback_port();

    let mut child = Command::new(rigger_bin())
        .args(["dash", "--port", &port.to_string()])
        .current_dir(proj.path())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("failed to spawn `rigger dash`");
    let mut out = child.stdout.take().expect("dash stdout is piped");

    // Watch the piped stdout on a helper thread: a read that BLOCKS means the dash is still
    // alive (its write end is open); a read that yields 0 means it exited and its stdout
    // reached EOF - i.e. the guard reaped it. (The dash logs to stderr, so stdout stays
    // empty-and-open until the process dies.)
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut buf = [0u8; 1];
        let n = out.read(&mut buf).unwrap_or(0);
        let _ = tx.send(n);
    });

    let guard = ReapedChild::new(child);

    // The standalone dash is a genuinely long-lived child: it stays alive while its guard
    // holds it (the watcher stays blocked, nothing arrives). If it had exited on startup
    // this fails LOUD (the safe direction), never a false green.
    assert!(
        rx.recv_timeout(Duration::from_millis(500)).is_err(),
        "the `rigger dash` exited before its guard was dropped - not a long-lived child"
    );

    // Dropping the guard (a finishing driver) kills AND reaps the dash: its stdout closes,
    // so the watcher sees EOF - the process is no longer alive, no orphan is left behind.
    drop(guard);
    let n = rx
        .recv_timeout(Duration::from_secs(10))
        .expect("dropping the ReapedChild did not reap the `rigger dash` within 10s");
    assert_eq!(n, 0, "a reaped `rigger dash` should have its stdout at EOF");
}

/// A minimal HTTP GET of a `http://127.0.0.1:<port>/` URL over a raw TCP socket (the test
/// crate has no HTTP client), returning the response BODY on success. The dash answers with
/// `Connection: close`, so `read_to_string` reads to EOF and terminates. Used to prove an
/// auto-started dash is genuinely SERVING (not merely that a URL was recorded).
fn http_get(url: &str) -> Option<String> {
    use std::io::{Read, Write};
    use std::time::{Duration, Instant};
    let hostport = url.strip_prefix("http://")?.trim_end_matches('/');
    // The dash's URL breadcrumb is written the instant the child is spawned, which can be
    // BEFORE that child has bound its port - a connect during that startup window is refused.
    // Retry the connect on a bounded deadline (the same poll-on-deadline pattern the caller
    // already uses for the dash.url breadcrumb) so a transient connect-refused during startup
    // is retried, not fatal; a connect that never succeeds within the deadline still fails
    // LOUD (the safe direction), never a false green.
    let deadline = Instant::now() + Duration::from_secs(15);
    let mut stream = loop {
        match std::net::TcpStream::connect(hostport) {
            Ok(stream) => break stream,
            Err(_) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(50)),
            Err(_) => return None,
        }
    };
    stream.set_read_timeout(Some(Duration::from_secs(5))).ok()?;
    write!(
        stream,
        "GET / HTTP/1.1\r\nHost: {hostport}\r\nConnection: close\r\n\r\n"
    )
    .ok()?;
    let mut resp = String::new();
    stream.read_to_string(&mut resp).ok()?;
    let body_start = resp.find("\r\n\r\n").map(|i| i + 4).unwrap_or(0);
    Some(resp[body_start..].to_string())
}

/// Spec 19b, unit 1 (always-on dash + discoverability): whenever a driver has a run in
/// flight, a `rigger dash` is auto-started serving that run - with NO opt-in flag - its URL
/// printed at run start and shown in `rigger status`. Driven through the WORKFLOW driver
/// (`rigger serve`), whose MCP loop keeps the process a live run in flight while its stdin
/// is held open (the conductor parks the frontier and defers agent work to the shim, so no
/// real agent is needed); closing stdin ends the run, and its dash is reaped by unit 3's
/// guard (which this unit HOLDS but does not itself assert - this unit owns start +
/// discoverability, not reaping).
#[test]
fn a_run_driver_auto_starts_a_reachable_dash_with_a_url_shown_in_status() {
    use std::io::Read;
    use std::process::{Command, Stdio};
    use std::time::{Duration, Instant};

    // A compliant git project a driver can start a run on: `grounder: nop` and a persona
    // that ENDS with the verdict line (so the run is not refused at the gating-lint seam).
    let proj = temp_git_project_with_commit();
    let root = proj.path();
    write_gating_lint_project(
        root,
        "You are the Adjudicator. Weigh the lenses and decide. Record your reasoning via the \
         rigger_emit tool as you go. End your output with a single line: \
         {\"verdict\":\"approve\"} to approve or {\"verdict\":\"reject\"} to reject.",
    );

    // Start the workflow driver with its MCP stdin held OPEN, so the process stays a live run
    // in flight. NO opt-in flag is passed: the dash must come up regardless. `--base HEAD`
    // anchors the run branch off the repo's lone commit.
    let mut child = Command::new(rigger_bin())
        .args(["serve", "--base", "HEAD"])
        .current_dir(root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped()) // the MCP transport; piped so it never floods the test output
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn `rigger serve`");

    // The driver records the auto-started dash's URL in `.rigger/dash.url` for discoverability;
    // poll it until it appears (the dash comes up at run start). If the driver exited early,
    // surface its stderr so the failure is diagnosable rather than a bare timeout.
    let url_file = root.join(".rigger").join("dash.url");
    let deadline = Instant::now() + Duration::from_secs(15);
    let url = loop {
        if let Ok(s) = std::fs::read_to_string(&url_file) {
            let s = s.trim().to_string();
            if !s.is_empty() {
                break s;
            }
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let mut err = String::new();
            if let Some(mut e) = child.stderr.take() {
                let _ = e.read_to_string(&mut err);
            }
            panic!("the driver never recorded a dash URL in .rigger/dash.url; stderr:\n{err}");
        }
        std::thread::sleep(Duration::from_millis(50));
    };
    assert!(
        url.starts_with("http://127.0.0.1:"),
        "the auto-started dash serves on a loopback URL; got {url:?}"
    );

    // The dash is genuinely SERVING that run: an HTTP GET returns the read-only page.
    let body = http_get(&url).unwrap_or_else(|| {
        let _ = child.kill();
        panic!("the auto-started dash at {url} did not answer an HTTP GET");
    });
    assert!(
        body.contains("rigger dash"),
        "the auto-started dash served its page; body:\n{body}"
    );

    // `rigger status` (a SEPARATE process) surfaces the same URL - the discoverability the
    // criterion demands.
    let (out, _err, _ok) = run_rigger(root, &["status"]);
    assert!(
        out.contains(&url),
        "`rigger status` must show the auto-started dash URL {url}; stdout:\n{out}"
    );

    // Tear down: close MCP stdin so the driver finishes and reaps its dash.
    drop(child.stdin.take());
    let _ = child.wait();
}

/// spec 22, criterion 2 (the ACCEPT arm - sibling to the refuse arm proven directly in
/// `src/mcpserver.rs`): the shared `emit_event` core still ACCEPTS every agent-emittable
/// context event and appends it, so both the CLI (`rigger emit`) and the MCP
/// (`rigger_emit`) surfaces that share this one core keep working after the allowlist
/// guard. The allowlist is exactly the three context-graph events an agent records
/// (`DecisionMade`, `ReviewFinding`, `LessonLearned`) PLUS the planner's `UnitProposed`
/// refinement - dropping any one of them silently over-refuses a real producer, and
/// dropping `UnitProposed` breaks planning with no other test in this crate catching it,
/// so this test pins ALL FOUR as the over-refusal regression guard.
///
/// Driven DIRECTLY by calling `emit_event` over an in-memory `Store` (never a CLI `rigger
/// emit` from a walk-up-able cwd), so it exercises the exact shared core the guard lives
/// in and can never touch or walk up to a real store - the store corruption this spec
/// closes is unreproducible here by construction.
#[test]
fn emit_event_accepts_every_agent_context_event_and_appends_it() {
    use rigger::eventstore::sqlite::Store;
    use rigger::eventstore::{Direction, EventStore, Filter};
    use serde_json::json;

    // The complete agent-emittable allowlist, each referenced from the SAME defining
    // constant the production allowlist (`EMITTABLE_TYPES` in `src/mcpserver.rs`) is built
    // from, so the test's notion of "accepted" cannot drift from the producers'. The
    // constants are independent of the allowlist array, so dropping a type FROM
    // `EMITTABLE_TYPES` still turns this test RED (the type is emitted here and refused
    // there).
    let accepted = [
        rigger::contextgraph::TYPE_DECISION_MADE,
        rigger::contextgraph::TYPE_REVIEW_FINDING,
        rigger::contextgraph::TYPE_LESSON_LEARNED,
        rigger::conductor::TYPE_UNIT_PROPOSED,
    ];

    for typ in accepted {
        // A fresh isolated store per type: the read-back must see exactly the one event
        // this iteration emitted, nothing else.
        let store = Store::open(":memory:").unwrap();
        // A distinct payload per type, so the read-back proves THIS event actually landed.
        let data = json!({ "id": typ, "summary": format!("payload for {typ}") });
        let args = json!({ "type": typ, "data": data });

        rigger::mcpserver::emit_event(&store, "run", None, &args).unwrap_or_else(|e| {
            panic!("emit_event must ACCEPT the agent context type {typ:?}; refused with: {e}")
        });

        // Exactly one event landed on the `run` stream, carrying the emitted type and the
        // byte-identical payload the caller passed - proof the accept path really appended.
        let events = store
            .read_all(0, Direction::Forward, &Filter::default())
            .unwrap();
        assert_eq!(
            events.len(),
            1,
            "accepting {typ:?} must append exactly one event; found: {events:?}"
        );
        let ev = &events[0];
        assert_eq!(ev.type_, typ, "the appended event carries the emitted type");
        assert_eq!(ev.stream, "run", "the event lands on the target stream");
        assert_eq!(
            ev.data,
            serde_json::to_vec(&data).unwrap(),
            "the appended payload is byte-identical to what the caller emitted"
        );
    }
}

/// Spec 20, unit 1 (the render pipeline, end to end): `rigger docs` renders BOTH the
/// `using-rigger` skill and the handbook discipline chapter, from ONE code-derived
/// context, into their committed paths - and the known code facts (the default base ref
/// and the dashboard port const) appear VERBATIM in the output. Driving the real binary
/// proves the whole composition path (docs_context -> render -> write) produces the two
/// files an author commits and the drift check re-renders against.
#[test]
fn docs_renders_the_skill_and_handbook_with_code_facts_verbatim() {
    let proj = temp_project();
    let root = proj.path();

    let (stdout, stderr, ok) = run_rigger(root, &["docs"]);
    assert!(ok, "rigger docs must succeed; stderr: {stderr}");

    let skill_path = root.join("skills/using-rigger/SKILL.md");
    let handbook_path = root.join("docs/handbook/using-rigger.md");
    assert!(
        stdout.contains("skills/using-rigger/SKILL.md") && stdout.contains("using-rigger.md"),
        "rigger docs must report the rendered paths; got: {stdout}"
    );

    let skill = std::fs::read_to_string(&skill_path).expect("skill was rendered");
    let handbook = std::fs::read_to_string(&handbook_path).expect("handbook was rendered");

    // The skill is a distinct, loadable skill file (frontmatter), not the workflow.
    assert!(
        skill.starts_with("---\nname: using-rigger\n"),
        "the skill must open with its loadable frontmatter; got: {}",
        &skill[..skill.len().min(60)]
    );
    // Known code facts appear verbatim in BOTH outputs: the default base ref (origin/main)
    // and the dashboard port const (7420) are read from code, not hand-copied.
    for (label, out) in [("skill", &skill), ("handbook", &handbook)] {
        assert!(
            out.contains("origin/main"),
            "{label} must carry the base ref verbatim"
        );
        assert!(
            out.contains("7420"),
            "{label} must carry the dash port verbatim"
        );
        assert!(
            out.contains("verdict"),
            "{label} must carry the verdict-line discipline"
        );
    }

    // Byte-stable: a second render writes identical bytes (the drift check depends on it).
    let (_o2, _e2, ok2) = run_rigger(root, &["docs"]);
    assert!(ok2);
    assert_eq!(std::fs::read_to_string(&skill_path).unwrap(), skill);
    assert_eq!(std::fs::read_to_string(&handbook_path).unwrap(), handbook);
}

/// Spec 20, unit 2 (the drift GATE, end to end): `rigger validate` FAILS LOUDLY when the
/// committed `using-rigger` skill or the handbook discipline chapter has drifted from a
/// fresh render, and PASSES when they are in sync - this is what makes the discipline STAY
/// accurate rather than merely start accurate. Unlike the warning advisories, drift is a
/// HARD, non-zero exit (a changed const, a changed template, or a hand-edited skill is a
/// definition drift, not a soft nudge), and the failure names the drifted file plus the
/// one-command fix (`rigger docs`). Both committed outputs are gated, and the gate clears
/// once the docs are re-rendered - so it fails on real drift, not permanently.
#[test]
fn validate_fails_when_the_committed_using_rigger_docs_drift_and_passes_when_in_sync() {
    let dir = temp_project();
    let root = dir.path();

    // A valid config so validate reaches the drift gate (past config load + the hard lints).
    let (_o, err, ok) = run_rigger(root, &["init"]);
    assert!(ok, "rigger init must succeed; stderr:\n{err}");

    // Render the committed docs from code -> the skill and handbook are now IN SYNC.
    let (_o, err, ok) = run_rigger(root, &["docs"]);
    assert!(ok, "rigger docs must succeed; stderr:\n{err}");
    let skill_path = root.join("skills/using-rigger/SKILL.md");
    let handbook_path = root.join("docs/handbook/using-rigger.md");
    assert!(
        skill_path.exists() && handbook_path.exists(),
        "rigger docs must have written both committed outputs"
    );

    // IN SYNC -> validate PASSES (exit 0) and says nothing about docs drift.
    let (out, err, ok) = run_rigger(root, &["validate"]);
    assert!(
        ok,
        "validate must PASS when the committed docs match a fresh render; stderr:\n{err}"
    );
    assert!(
        out.contains("config valid"),
        "validate must still print its config summary when the docs are in sync; stdout:\n{out}"
    );
    assert!(
        !err.to_lowercase().contains("drift"),
        "validate must not report docs drift when the committed docs are in sync; stderr:\n{err}"
    );

    // DRIFT the skill with a hand edit the render never emits -> validate FAILS (non-zero),
    // naming the drifted skill file and the `rigger docs` fix.
    append_line(&skill_path, "hand-edited line the render never emits");
    let (_out, err, ok) = run_rigger(root, &["validate"]);
    assert!(
        !ok,
        "validate must FAIL (non-zero exit) when the committed skill drifts from a fresh \
         render; stderr:\n{err}"
    );
    assert!(
        err.contains("skills/using-rigger/SKILL.md") && err.contains("rigger docs"),
        "the drift failure must name the drifted skill file and the `rigger docs` fix; \
         stderr:\n{err}"
    );

    // Re-render restores sync -> validate PASSES again (the gate is not stuck failing).
    let (_o, _e, ok) = run_rigger(root, &["docs"]);
    assert!(ok, "re-rendering the docs must succeed");
    let (_out, err, ok) = run_rigger(root, &["validate"]);
    assert!(
        ok,
        "validate must PASS again once the drifted docs are re-rendered; stderr:\n{err}"
    );

    // DRIFT the handbook chapter -> validate FAILS naming the handbook, proving BOTH
    // committed outputs are gated (not just the skill).
    append_line(
        &handbook_path,
        "hand-edited handbook line the render never emits",
    );
    let (_out, err, ok) = run_rigger(root, &["validate"]);
    assert!(
        !ok,
        "validate must FAIL when the committed handbook discipline chapter drifts; stderr:\n{err}"
    );
    assert!(
        err.contains("docs/handbook/using-rigger.md") && err.contains("rigger docs"),
        "the drift failure must name the drifted handbook chapter and the fix; stderr:\n{err}"
    );
}

/// Spec 20, unit 3 (setup install + project overlay, end to end): `rigger setup` installs
/// the rendered `using-rigger` skill as a file DISTINCT from the `/rigger` workflow, and a
/// project overlay adds this repo's specifics (base branch, specs location) into the
/// installed skill WITHOUT editing the shared discipline source. Driving the real binary
/// proves the whole install path (overlay read -> merge onto docs_context -> render ->
/// write) lands a loadable skill carrying the repo's own facts.
#[test]
fn setup_installs_the_using_rigger_skill_with_project_overlay() {
    let proj = temp_project();
    let root = proj.path();

    // This repo declares its specifics in the overlay: a non-default base branch and a
    // non-default specs directory. `rigger docs` never sees this - it is the setup-time
    // project overlay, merged into the installed skill only.
    std::fs::create_dir_all(root.join(".rigger")).unwrap();
    std::fs::write(
        root.join(".rigger").join("docs-overlay.yml"),
        "base_ref: origin/trunk\nspecs_location: product-specs/\n",
    )
    .unwrap();

    // npm is stubbed to a no-op so the shim provision step does not need a real npm.
    let (out, err, ok) = run_rigger_envs(root, &["setup"], &[("RIGGER_NPM", "true")]);
    assert!(ok, "rigger setup must succeed; stderr:\n{err}");

    let skill_path = root.join(".claude/skills/using-rigger/SKILL.md");
    let workflow_path = root.join(".claude/workflows/rigger.js");
    assert!(
        skill_path.exists(),
        "setup must install the using-rigger skill at .claude/skills/using-rigger/SKILL.md"
    );
    // The skill is a file DISTINCT from the /rigger workflow (both are installed, at
    // different paths, and the skill is not the workflow).
    assert!(
        workflow_path.exists(),
        "the /rigger workflow is also installed"
    );
    assert_ne!(
        skill_path, workflow_path,
        "the installed skill and the /rigger workflow are distinct files"
    );

    let skill = std::fs::read_to_string(&skill_path).expect("the skill was installed");
    assert!(
        skill.starts_with("---\nname: using-rigger\n"),
        "the installed skill is a loadable skill (frontmatter); got: {}",
        &skill[..skill.len().min(60)]
    );
    // The project overlay's repo specifics flow into the installed skill...
    assert!(
        skill.contains("origin/trunk"),
        "the overlay base branch must appear in the installed skill; got:\n{skill}"
    );
    assert!(
        skill.contains("product-specs/"),
        "the overlay specs location must appear in the installed skill; got:\n{skill}"
    );
    // ...and setup reports installing the skill.
    assert!(
        out.contains("using-rigger skill") && out.contains(".claude/skills/using-rigger/SKILL.md"),
        "setup must report installing the using-rigger skill; got:\n{out}"
    );

    // The shared discipline source is NOT edited by the overlay: `rigger docs` renders the
    // committed source with the DEFAULT base ref, not the overlay's.
    let (_o, _e, ok2) = run_rigger(root, &["docs"]);
    assert!(ok2, "rigger docs must succeed");
    let committed = std::fs::read_to_string(root.join("skills/using-rigger/SKILL.md")).unwrap();
    assert!(
        committed.contains("origin/main") && !committed.contains("origin/trunk"),
        "the committed shared source keeps the default base ref; the overlay only \
         customized the install"
    );
}

/// Stage a `rigger` shim (a tiny sh script that execs the freshly built binary) in a
/// `shim-bin/` under `root` and return a `PATH` value with that dir prepended, so a `git
/// commit` run with this `PATH` finds `rigger` BY NAME - the pre-commit hook invokes `rigger`
/// unqualified (spec 24), and pinning it to the built binary keeps the test off whatever old
/// `rigger` happens to be installed in the ambient `PATH`.
fn stage_rigger_shim(root: &Path) -> String {
    let bindir = root.join("shim-bin");
    std::fs::create_dir_all(&bindir).unwrap();
    let shim = bindir.join("rigger");
    std::fs::write(
        &shim,
        format!("#!/bin/sh\nexec \"{}\" \"$@\"\n", rigger_bin()),
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&shim, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    let orig_path = std::env::var("PATH").unwrap_or_default();
    format!("{}:{}", bindir.display(), orig_path)
}

/// Spec 24, crit 1 (install + regenerate-then-stage-into-the-commit, end to end): in a repo
/// that ALREADY TRACKS the rendered docs (rigger's own self-hosting repo), `rigger setup`
/// installs a git pre-commit hook that, on `git commit`, runs `rigger docs` and stages the
/// changed rendered outputs (the `using-rigger` skill + the handbook discipline chapter) into
/// that SAME commit - so a commit that changes a documented code fact carries its freshly
/// rendered docs. Drives the REAL `rigger` binary and REAL git, with a `rigger` shim on PATH
/// at commit time (the hook invokes `rigger` by name, per spec 24).
#[test]
fn setup_precommit_hook_regenerates_and_stages_docs_when_the_repo_tracks_them() {
    let proj = temp_git_project_with_commit();
    let root = proj.path();

    // `rigger setup` installs the pre-commit hook (npm stubbed so the shim step needs no npm).
    let (out, err, ok) = run_rigger_envs(root, &["setup"], &[("RIGGER_NPM", "true")]);
    assert!(ok, "rigger setup must succeed; stderr:\n{err}");
    assert!(
        out.contains("pre-commit hook"),
        "setup must report installing the pre-commit hook; got:\n{out}"
    );

    // The hook is installed, executable, and carries rigger's docs-regenerating block.
    let hook_path = root.join(".git/hooks/pre-commit");
    assert!(
        hook_path.exists(),
        "setup must install .git/hooks/pre-commit"
    );
    let hook = std::fs::read_to_string(&hook_path).unwrap();
    assert!(
        hook.contains("rigger docs") && hook.contains("skills/using-rigger/SKILL.md"),
        "the hook regenerates the docs and stages the rendered outputs; got:\n{hook}"
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(&hook_path).unwrap().permissions().mode();
        assert!(
            mode & 0o111 != 0,
            "the hook must be executable so git runs it; mode {mode:o}"
        );
    }

    // Make this a rigger SELF-HOSTING repo: TRACK stale committed copies of both rendered
    // outputs so the hook has a real, tracked change to freshen. Commit the seed with
    // `--no-verify` so the just-installed hook does NOT fire here - the seed must stay
    // genuinely STALE, otherwise the final "not STALE" assertion could pass vacuously (the
    // docs were already fresh) instead of proving the FINAL commit's hook regenerated them.
    const STALE: &str = "STALE DOC - not a real render\n";
    for rel in [
        "skills/using-rigger/SKILL.md",
        "docs/handbook/using-rigger.md",
    ] {
        let p = root.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(&p, STALE).unwrap();
    }
    git_ok(
        root,
        &[
            "add",
            "skills/using-rigger/SKILL.md",
            "docs/handbook/using-rigger.md",
        ],
    );
    git_ok(
        root,
        &["commit", "-q", "--no-verify", "-m", "seed stale docs"],
    );

    // Guard against a vacuous pass: HEAD must carry the STALE bytes right after the seed, so a
    // later "not STALE" assertion can only hold if the FINAL commit's hook did the regenerate.
    let seeded_skill =
        git_out(root, &["show", "HEAD:skills/using-rigger/SKILL.md"]).unwrap_or_default();
    assert!(
        seeded_skill.contains("STALE DOC"),
        "the --no-verify seed must commit the STALE docs unchanged so the discrimination is \
         real, not vacuous; got:\n{seeded_skill}"
    );

    // A `rigger` shim on PATH at commit time - the hook invokes `rigger` BY NAME.
    let commit_path = stage_rigger_shim(root);

    // Make an UNRELATED tracked change and commit it. The pre-commit hook regenerates the
    // docs and stages them into THIS commit, alongside the unrelated change.
    std::fs::write(root.join("code.txt"), "a documented code fact changed\n").unwrap();
    git_ok(root, &["add", "code.txt"]);
    let commit_ok = Command::new("git")
        .args(["commit", "-q", "-m", "change a documented fact"])
        .current_dir(root)
        .env("PATH", &commit_path)
        .status()
        .expect("git must be runnable")
        .success();
    assert!(
        commit_ok,
        "the commit must succeed - the hook must never block it"
    );

    // The freshly regenerated docs RODE the same commit: HEAD carries both rendered outputs
    // AND the unrelated change, and the committed docs are the FRESH render (not the stale
    // seed the hook replaced), proving the regenerated docs ride the commit that changed a
    // documented fact.
    let tree = git_out(root, &["ls-tree", "-r", "--name-only", "HEAD"]).unwrap_or_default();
    assert!(
        tree.contains("code.txt")
            && tree.contains("skills/using-rigger/SKILL.md")
            && tree.contains("docs/handbook/using-rigger.md"),
        "the commit must carry the unrelated change AND both regenerated docs; tree:\n{tree}"
    );
    let committed_skill =
        git_out(root, &["show", "HEAD:skills/using-rigger/SKILL.md"]).unwrap_or_default();
    assert!(
        committed_skill.contains("name: using-rigger") && !committed_skill.contains("STALE DOC"),
        "the commit must carry the FRESH render of the skill, not the stale seed; got:\n{committed_skill}"
    );
    let committed_handbook =
        git_out(root, &["show", "HEAD:docs/handbook/using-rigger.md"]).unwrap_or_default();
    assert!(
        !committed_handbook.is_empty() && !committed_handbook.contains("STALE DOC"),
        "the commit must carry the FRESH render of the handbook, not the stale seed; got:\n{committed_handbook}"
    );
}

/// Spec 24, crit 1 (operator repo is NOT polluted, end to end): the docs pre-commit hook is
/// installed the SAME way everywhere, but it regenerates+stages ONLY where the repo already
/// TRACKS rigger's rendered docs. In an OPERATOR project - one driving the operator's own code
/// that never carries these committed docs (spec 20's drift check treats their absence as
/// "nothing to drift") - the hook stays INERT even with `rigger` on PATH: an ordinary operator
/// commit carries none of rigger's internal discipline docs and the operator's worktree is not
/// polluted with them. Guards the operator-scoping (adj-u24-1 / d24-10) against regression.
#[test]
fn setup_precommit_hook_stays_inert_in_an_operator_repo() {
    // An OPERATOR repo: it drives the operator's OWN code and never tracks rigger's committed
    // `using-rigger` docs.
    let proj = temp_git_project_with_commit();
    let root = proj.path();
    std::fs::write(root.join("README.md"), "operator project\n").unwrap();
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(root.join("src/app.rs"), "fn main() {}\n").unwrap();
    git_ok(root, &["add", "README.md", "src/app.rs"]);
    git_ok(
        root,
        &["commit", "-q", "--no-verify", "-m", "operator code"],
    );

    // `rigger setup` installs the SAME hook here (it cannot know at install time whether the
    // repo tracks the docs).
    let (_out, err, ok) = run_rigger_envs(root, &["setup"], &[("RIGGER_NPM", "true")]);
    assert!(ok, "rigger setup must succeed; stderr:\n{err}");
    assert!(
        root.join(".git/hooks/pre-commit").exists(),
        "the hook is installed in an operator repo too"
    );

    // `rigger` IS on PATH at commit time, so the hook DOES run - the ONLY thing keeping it
    // inert is that this repo does not track the docs.
    let commit_path = stage_rigger_shim(root);

    // An ordinary operator commit of the operator's OWN code.
    std::fs::write(root.join("src/app.rs"), "fn main() { let _ = 1; }\n").unwrap();
    git_ok(root, &["add", "src/app.rs"]);
    let commit_ok = Command::new("git")
        .args(["commit", "-q", "-m", "operator changes their own code"])
        .current_dir(root)
        .env("PATH", &commit_path)
        .status()
        .expect("git must be runnable")
        .success();
    assert!(
        commit_ok,
        "the commit must succeed - the hook must never block it"
    );

    // The hook stayed INERT: the operator's commit carries their OWN change but NONE of
    // rigger's internal docs.
    let tree = git_out(root, &["ls-tree", "-r", "--name-only", "HEAD"]).unwrap_or_default();
    assert!(
        tree.contains("src/app.rs"),
        "the operator's own change is committed; tree:\n{tree}"
    );
    assert!(
        !tree.contains("skills/using-rigger/SKILL.md")
            && !tree.contains("docs/handbook/using-rigger.md"),
        "an operator commit must NOT be forced to carry rigger's internal discipline docs; \
         tree:\n{tree}"
    );

    // And the worktree is not polluted with them either: the inert hook never ran `rigger
    // docs`, so it created no files the operator did not ask for.
    assert!(
        !root.join("skills/using-rigger/SKILL.md").exists()
            && !root.join("docs/handbook/using-rigger.md").exists(),
        "the hook must not create rigger's committed docs in an operator worktree"
    );
}

/// Turn `root` into a rigger SELF-HOSTING repo for the pre-commit-hook SAFETY fixtures (spec 24,
/// crit 2): run `rigger setup` (npm stubbed) so the hook is installed, then TRACK stale committed
/// copies of both rendered docs, committed with `--no-verify` so the just-installed hook does NOT
/// fire and the seed stays genuinely STALE. After this the installed hook has real, tracked work
/// to do on the next ordinary commit, so any later "not STALE" / "still STALE" assertion actually
/// discriminates whether that commit's hook regenerated the docs.
fn setup_selfhosting_repo_with_stale_docs(root: &Path) {
    const STALE: &str = "STALE DOC - not a real render\n";
    let (out, err, ok) = run_rigger_envs(root, &["setup"], &[("RIGGER_NPM", "true")]);
    assert!(ok, "rigger setup must succeed; stderr:\n{err}");
    assert!(
        out.contains("pre-commit hook"),
        "setup must install the pre-commit hook; got:\n{out}"
    );
    for rel in [
        "skills/using-rigger/SKILL.md",
        "docs/handbook/using-rigger.md",
    ] {
        let p = root.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(&p, STALE).unwrap();
    }
    git_ok(
        root,
        &[
            "add",
            "skills/using-rigger/SKILL.md",
            "docs/handbook/using-rigger.md",
        ],
    );
    git_ok(
        root,
        &["commit", "-q", "--no-verify", "-m", "seed stale docs"],
    );
    let seeded = git_out(root, &["show", "HEAD:skills/using-rigger/SKILL.md"]).unwrap_or_default();
    assert!(
        seeded.contains("STALE DOC"),
        "the --no-verify seed must commit the STALE docs so discrimination is real; got:\n{seeded}"
    );
}

/// A `PATH` built from the ambient `PATH` with every directory that contains a `rigger` binary
/// removed, so the pre-commit hook's `command -v rigger` fails DETERMINISTICALLY (the
/// graceful-degrade "rigger unavailable" path) regardless of whatever `rigger` happens to sit in
/// the developer's ambient `PATH`. Keeps `git` and the coreutils the commit needs.
fn path_without_rigger() -> String {
    std::env::var("PATH")
        .unwrap_or_default()
        .split(':')
        .filter(|dir| !dir.is_empty() && !Path::new(dir).join("rigger").exists())
        .collect::<Vec<_>>()
        .join(":")
}

/// Stage a `rigger` shim on `PATH` that is PRESENT (so `command -v rigger` succeeds) but makes
/// `rigger docs` FAIL (exit 1), delegating every other subcommand to the real built binary. Drives
/// the graceful-degrade "rigger docs errors" path: the hook must WARN and let the commit proceed.
/// Returns a `PATH` with the shim dir prepended.
fn stage_failing_docs_rigger_shim(root: &Path) -> String {
    let bindir = root.join("shim-bin");
    std::fs::create_dir_all(&bindir).unwrap();
    let shim = bindir.join("rigger");
    std::fs::write(
        &shim,
        format!(
            "#!/bin/sh\nif [ \"$1\" = docs ]; then\n  echo 'boom: rigger docs failed' 1>&2\n  \
             exit 1\nfi\nexec \"{}\" \"$@\"\n",
            rigger_bin()
        ),
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&shim, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    let orig_path = std::env::var("PATH").unwrap_or_default();
    format!("{}:{}", bindir.display(), orig_path)
}

/// Spec 24, crit 2 (idempotency, end to end): the hook is SAFE to live in everyone's
/// `.git/hooks` - re-running `rigger setup` does NOT duplicate it. The installed hook is
/// byte-identical after a second setup, still carries exactly one managed block, and the rerun
/// does not re-report installing the hook (it is a true no-op).
#[test]
fn setup_precommit_hook_is_idempotent_no_duplicate_block_on_rerun() {
    const BEGIN: &str = "# >>> BEGIN rigger docs pre-commit (managed - do not edit) >>>";
    let proj = temp_git_project_with_commit();
    let root = proj.path();

    let (_o, err, ok) = run_rigger_envs(root, &["setup"], &[("RIGGER_NPM", "true")]);
    assert!(ok, "the first setup must succeed; stderr:\n{err}");
    let hook_path = root.join(".git/hooks/pre-commit");
    let first = std::fs::read_to_string(&hook_path).unwrap();
    assert_eq!(
        first.matches(BEGIN).count(),
        1,
        "one managed block after the first setup; got:\n{first}"
    );

    let (out2, err2, ok2) = run_rigger_envs(root, &["setup"], &[("RIGGER_NPM", "true")]);
    assert!(ok2, "the second setup must succeed; stderr:\n{err2}");
    let second = std::fs::read_to_string(&hook_path).unwrap();
    assert_eq!(
        first, second,
        "re-running setup does not rewrite the hook (a true no-op)"
    );
    assert_eq!(
        second.matches(BEGIN).count(),
        1,
        "no duplicate managed block on a rerun; got:\n{second}"
    );
    assert!(
        !out2.contains("pre-commit hook"),
        "an up-to-date rerun must not re-report installing the hook; got:\n{out2}"
    );
}

/// Spec 24, crit 2 (non-clobbering chaining defeats a TERMINAL existing hook, end to end): the
/// modal hand-written / sample pre-commit hook ends in a terminal `exit 0`. rigger chains its
/// block onto it WITHOUT clobbering it, and - crucially - rigger's block still RUNS: it is
/// inserted BEFORE the existing hook body (which ends in `exit 0`), so a `git commit` runs BOTH
/// the pre-existing hook AND rigger's docs regeneration. Regression-guards
/// adv-u24-1r-chained-terminal-hook-shadows-rigger-block-silently (d24-11): appending rigger's
/// block after such a hook would let the `exit 0` silently shadow it.
#[test]
fn setup_precommit_hook_chains_after_a_terminal_exit_hook_and_still_runs() {
    let proj = temp_git_project_with_commit();
    let root = proj.path();

    // A pre-existing pre-commit hook that does work and ends in a TERMINAL `exit 0`.
    let hooks = root.join(".git/hooks");
    std::fs::create_dir_all(&hooks).unwrap();
    let user_hook = hooks.join("pre-commit");
    std::fs::write(&user_hook, "#!/bin/sh\ntouch USER_HOOK_RAN\nexit 0\n").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&user_hook, std::fs::Permissions::from_mode(0o755)).unwrap();
    }

    // `rigger setup` chains its block onto the pre-existing hook; then make it self-hosting with
    // stale tracked docs so the final commit's hook has real work.
    setup_selfhosting_repo_with_stale_docs(root);

    // The chained hook carries BOTH the user hook's command and rigger's block.
    let hook = std::fs::read_to_string(&user_hook).unwrap();
    assert!(
        hook.contains("touch USER_HOOK_RAN") && hook.contains("rigger docs"),
        "the chained hook must preserve the user hook AND carry rigger's block; got:\n{hook}"
    );

    let commit_path = stage_rigger_shim(root);
    std::fs::write(root.join("code.txt"), "a documented fact changed\n").unwrap();
    git_ok(root, &["add", "code.txt"]);
    let commit_ok = Command::new("git")
        .args(["commit", "-q", "-m", "change a documented fact"])
        .current_dir(root)
        .env("PATH", &commit_path)
        .status()
        .expect("git must be runnable")
        .success();
    assert!(
        commit_ok,
        "the commit must succeed - the hook must never block it"
    );

    // The PRE-EXISTING hook still ran (its side effect is present)...
    assert!(
        root.join("USER_HOOK_RAN").exists(),
        "the pre-existing hook must still run when chained"
    );
    // ...AND rigger's block ALSO ran despite the existing hook's terminal `exit 0`: HEAD carries
    // the FRESH render, not the stale seed. A terminal-shadow bug (append-after) would break this.
    let committed =
        git_out(root, &["show", "HEAD:skills/using-rigger/SKILL.md"]).unwrap_or_default();
    assert!(
        committed.contains("name: using-rigger") && !committed.contains("STALE DOC"),
        "rigger's block must still regenerate the docs even though the existing hook ends in \
         `exit 0`; got:\n{committed}"
    );
}

/// Spec 24, crit 2 (staging scope, end to end): the hook stages ONLY the two rendered doc
/// outputs, never any other working-tree file. With an UNTRACKED junk file and an UNSTAGED edit
/// to an unrelated tracked file both present in the worktree, an ordinary commit rides the two
/// regenerated docs plus only the change the operator staged - the junk file is never committed
/// and the unstaged edit does not ride the commit; both are left untouched in the worktree.
#[test]
fn setup_precommit_hook_stages_only_the_rendered_docs() {
    let proj = temp_git_project_with_commit();
    let root = proj.path();

    // A tracked file whose later UNSTAGED modification must NOT ride the commit.
    std::fs::write(root.join("other.txt"), "original\n").unwrap();
    git_ok(root, &["add", "other.txt"]);
    git_ok(
        root,
        &["commit", "-q", "--no-verify", "-m", "add other.txt"],
    );

    setup_selfhosting_repo_with_stale_docs(root);
    let commit_path = stage_rigger_shim(root);

    // Working-tree noise the hook must NOT stage: an UNTRACKED junk file and an UNSTAGED edit to
    // a tracked file.
    std::fs::write(root.join("junk.txt"), "not for the commit\n").unwrap();
    std::fs::write(root.join("other.txt"), "MODIFIED but not staged\n").unwrap();

    // Stage ONE unrelated change and commit; the hook stages the two docs on top of it.
    std::fs::write(root.join("trigger.txt"), "trigger\n").unwrap();
    git_ok(root, &["add", "trigger.txt"]);
    let commit_ok = Command::new("git")
        .args(["commit", "-q", "-m", "trigger"])
        .current_dir(root)
        .env("PATH", &commit_path)
        .status()
        .expect("git must be runnable")
        .success();
    assert!(commit_ok, "the commit must succeed");

    let tree = git_out(root, &["ls-tree", "-r", "--name-only", "HEAD"]).unwrap_or_default();
    assert!(
        tree.contains("trigger.txt")
            && tree.contains("skills/using-rigger/SKILL.md")
            && tree.contains("docs/handbook/using-rigger.md"),
        "the commit must carry the staged change AND both regenerated docs; tree:\n{tree}"
    );
    assert!(
        !tree.contains("junk.txt"),
        "the hook must NOT stage an unrelated untracked file; tree:\n{tree}"
    );
    let committed_other = git_out(root, &["show", "HEAD:other.txt"]).unwrap_or_default();
    assert_eq!(
        committed_other, "original",
        "the hook must not stage an unrelated tracked file's unstaged modification; got:\n{committed_other}"
    );
    // The worktree noise is left untouched.
    assert!(
        root.join("junk.txt").exists(),
        "the untracked file is left in the worktree, not deleted or staged"
    );
    assert_eq!(
        std::fs::read_to_string(root.join("other.txt")).unwrap(),
        "MODIFIED but not staged\n",
        "the tracked file's unstaged modification is left in the worktree"
    );
}

/// Spec 24, crit 2 (graceful degrade when rigger is UNAVAILABLE, end to end): with `rigger`
/// removed from `PATH`, the hook WARNS and lets the commit PROCEED - it never blocks a commit.
/// The docs are not regenerated (the spec-20 drift check is the backstop, not the hook), so HEAD
/// keeps the stale seed.
#[test]
fn setup_precommit_hook_warns_and_proceeds_when_rigger_is_unavailable() {
    let proj = temp_git_project_with_commit();
    let root = proj.path();
    setup_selfhosting_repo_with_stale_docs(root);

    let path = path_without_rigger();
    std::fs::write(root.join("code.txt"), "changed\n").unwrap();
    git_ok(root, &["add", "code.txt"]);
    let out = Command::new("git")
        .args(["commit", "-q", "-m", "change with rigger off PATH"])
        .current_dir(root)
        .env("PATH", &path)
        .output()
        .expect("git must be runnable");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        out.status.success(),
        "the commit must succeed - the hook must never block it; stderr:\n{stderr}"
    );
    assert!(
        stderr.contains("rigger not on PATH"),
        "the hook must WARN that rigger is unavailable; stderr:\n{stderr}"
    );
    let committed =
        git_out(root, &["show", "HEAD:skills/using-rigger/SKILL.md"]).unwrap_or_default();
    assert!(
        committed.contains("STALE DOC"),
        "with rigger unavailable the hook regenerates nothing; got:\n{committed}"
    );
}

/// Spec 24, crit 2 (graceful degrade when `rigger docs` ERRORS, end to end): `rigger` is on
/// `PATH` (so `command -v rigger` succeeds) but `rigger docs` fails. The hook WARNS and lets the
/// commit PROCEED - a transient generator failure degrades to "caught later" by the drift check,
/// never "cannot commit". HEAD keeps the stale seed.
#[test]
fn setup_precommit_hook_warns_and_proceeds_when_rigger_docs_errors() {
    let proj = temp_git_project_with_commit();
    let root = proj.path();
    setup_selfhosting_repo_with_stale_docs(root);

    let path = stage_failing_docs_rigger_shim(root);
    std::fs::write(root.join("code.txt"), "changed\n").unwrap();
    git_ok(root, &["add", "code.txt"]);
    let out = Command::new("git")
        .args(["commit", "-q", "-m", "change while rigger docs errors"])
        .current_dir(root)
        .env("PATH", &path)
        .output()
        .expect("git must be runnable");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        out.status.success(),
        "the commit must succeed - a failing `rigger docs` must never block it; stderr:\n{stderr}"
    );
    assert!(
        stderr.contains("rigger docs failed"),
        "the hook must WARN that `rigger docs` failed; stderr:\n{stderr}"
    );
    let committed =
        git_out(root, &["show", "HEAD:skills/using-rigger/SKILL.md"]).unwrap_or_default();
    assert!(
        committed.contains("STALE DOC"),
        "a failing `rigger docs` regenerates nothing; got:\n{committed}"
    );
}

/// Spec 24, crit 2 (staging scope, the all-tracked gate, end to end): a repo in the degenerate
/// PARTIAL-tracking state - it tracks ONE rendered doc but not the other - must stay INERT, not
/// half-run. The hook gates `rigger docs` on EVERY rendered output already being tracked, so it
/// never runs here: it neither regenerates the one tracked doc NOR writes the untracked one as a
/// stray working-tree file the operator did not ask for (d24-2-all-tracked-gate-no-stray /
/// sdet-u24-1r-any-tracked-gate-vs-regenerate-both). This DISCRIMINATES the all-tracked gate: a
/// regression to an any-tracked gate would run `rigger docs`, regenerate the tracked skill AND
/// create the stray untracked handbook - both asserted against here.
#[test]
fn setup_precommit_hook_stays_inert_when_only_one_doc_is_tracked() {
    const STALE: &str = "STALE DOC - not a real render\n";
    const SKILL_REL: &str = "skills/using-rigger/SKILL.md";
    const HANDBOOK_REL: &str = "docs/handbook/using-rigger.md";
    let proj = temp_git_project_with_commit();
    let root = proj.path();

    // Install the hook the SAME way everywhere (it cannot know at install time what the repo
    // will track).
    let (_out, err, ok) = run_rigger_envs(root, &["setup"], &[("RIGGER_NPM", "true")]);
    assert!(ok, "rigger setup must succeed; stderr:\n{err}");

    // Track ONLY the skill (a stale committed copy), NOT the handbook - the degenerate
    // partial-tracking state. Committed with `--no-verify` so the just-installed hook does not
    // fire and the seed stays genuinely STALE.
    let skill = root.join(SKILL_REL);
    std::fs::create_dir_all(skill.parent().unwrap()).unwrap();
    std::fs::write(&skill, STALE).unwrap();
    git_ok(root, &["add", SKILL_REL]);
    git_ok(
        root,
        &["commit", "-q", "--no-verify", "-m", "seed only the skill"],
    );

    // `rigger` IS on PATH at commit time, so the ONLY thing keeping the hook inert is the
    // all-tracked gate (the handbook is untracked).
    let commit_path = stage_rigger_shim(root);
    std::fs::write(root.join("code.txt"), "a documented fact changed\n").unwrap();
    git_ok(root, &["add", "code.txt"]);
    let commit_ok = Command::new("git")
        .args(["commit", "-q", "-m", "change with only the skill tracked"])
        .current_dir(root)
        .env("PATH", &commit_path)
        .status()
        .expect("git must be runnable")
        .success();
    assert!(
        commit_ok,
        "the commit must succeed - the hook must never block it"
    );

    // The hook stayed INERT: the tracked skill was NOT regenerated (HEAD keeps the stale seed).
    let committed = git_out(root, &["show", &format!("HEAD:{SKILL_REL}")]).unwrap_or_default();
    assert!(
        committed.contains("STALE DOC") && !committed.contains("name: using-rigger"),
        "with only one doc tracked the hook must NOT regenerate the tracked doc; got:\n{committed}"
    );
    // And it did NOT write the untracked handbook as a stray working-tree file the operator did
    // not ask for.
    assert!(
        !root.join(HANDBOOK_REL).exists(),
        "the hook must not create the untracked handbook as a stray file in the worktree"
    );
    // Nor does the untracked handbook ride the commit.
    let tree = git_out(root, &["ls-tree", "-r", "--name-only", "HEAD"]).unwrap_or_default();
    assert!(
        !tree.contains(HANDBOOK_REL),
        "the untracked handbook must never ride the commit; tree:\n{tree}"
    );
}

// ---------------------------------------------------------------------------
// Spec 38, criterion 3 - the ready-to-release handoff, PROVEN THROUGH THE BINARY.
//
// The inside-out unit tests (`ledger::release_ready_surfaces_only_a_done_run`,
// `main::release_ready_lines_surface_only_on_a_done_run`, `dash::release_ready_is_surfaced_
// on_the_dash_only_for_a_done_run`) call the pure projection and render seams IN-PROCESS.
// They never drive the compiled `rigger` binary against a REAL namespaced event store, so
// they cannot prove that `cmd_status` / `cmd_dash` actually WIRE the handoff onto the
// operator-facing surfaces, that the run-branch/base resolution (`resolve_run_base`, the
// `RIGGER_BASE` override, the `origin/` -> branch strip) reaches the printed PR command
// end-to-end, or that an unfinished / deferred-gate-failed run surfaces NOTHING through the
// same seam. These periphery tests drive the binary against a seeded store to guard exactly
// that boundary.
// ---------------------------------------------------------------------------

/// The handoff (`ReleaseReady::lines`) surfaces on `rigger status` for a DONE run: the run
/// branch, the release-target base, the integrated-unit count, and the EXACT `gh pr create`
/// command, resolved through the whole store -> project -> release_ready -> render path the
/// binary walks. The `RIGGER_BASE` override then proves the resolved base (with the
/// `origin/` remote prefix stripped) reaches the surfaced PR command, which no in-process
/// unit test exercises (they call `release_ready` with a fixed base directly).
#[test]
fn release_ready_handoff_surfaces_on_status_for_a_done_run() {
    let dir = temp_project();
    let root = dir.path();
    seed_store(root);
    // A done run: one unit started and integrated, no failed deferred gate.
    seed_run_events(
        root,
        &[
            ("RunStarted", r#"{"run":"r1","criteria":["spec 38"]}"#),
            ("UnitStarted", r#"{"id":"u1","agent":"worker"}"#),
            ("UnitIntegrated", r#"{"id":"u1","commit":"abc"}"#),
        ],
    );

    // Default base (`origin/main` -> `main`): `rigger status` names all four facts.
    let (out, err, ok) = run_rigger(root, &["status"]);
    assert!(
        ok,
        "rigger status must succeed on a done run; stderr:\n{err}"
    );
    assert!(
        out.contains("release-ready:"),
        "a done run surfaces the release-ready handoff on status; got:\n{out}"
    );
    assert!(
        out.contains("rigger-run"),
        "the handoff names the run branch (the PR head); got:\n{out}"
    );
    assert!(
        out.contains("1 unit integrated"),
        "the handoff names the integrated-unit count; got:\n{out}"
    );
    assert!(
        out.contains("gh pr create --base main --head rigger-run"),
        "the handoff names the exact PR command, with `origin/main` stripped to `main`; \
         got:\n{out}"
    );

    // The `RIGGER_BASE` override flows through `resolve_run_base` into the surfaced PR
    // command, and its `origin/` remote prefix is stripped to the release-target branch -
    // proven end-to-end through the binary, not just the in-process projection.
    let (out, err, ok) =
        run_rigger_envs(root, &["status"], &[("RIGGER_BASE", "origin/release-2.0")]);
    assert!(ok, "rigger status honors RIGGER_BASE; stderr:\n{err}");
    assert!(
        out.contains("gh pr create --base release-2.0 --head rigger-run"),
        "the RIGGER_BASE override reaches the PR command with `origin/` stripped; got:\n{out}"
    );
}

/// The handoff is SILENT through `rigger status` for any run that is not done: a
/// still-un-integrated unit, and (the load-bearing guard) a run whose every unit integrated
/// but whose deferred phase-boundary gate FAILED - which must never be advertised as a
/// finished, releasable run. No in-process test proves the CLI seam honors either negative.
#[test]
fn release_ready_is_silent_on_status_for_an_unfinished_run() {
    // A run with a still-un-integrated unit surfaces no release-ready signal.
    let dir = temp_project();
    let root = dir.path();
    seed_store(root);
    seed_run_events(
        root,
        &[
            ("RunStarted", r#"{"run":"r1","criteria":["spec 38"]}"#),
            ("UnitStarted", r#"{"id":"u1","agent":"worker"}"#),
            ("UnitIntegrated", r#"{"id":"u1","commit":"abc"}"#),
            ("UnitStarted", r#"{"id":"u2","agent":"worker"}"#),
        ],
    );
    let (out, err, ok) = run_rigger(root, &["status"]);
    assert!(ok, "rigger status must succeed; stderr:\n{err}");
    assert!(
        !out.contains("release-ready:") && !out.contains("gh pr create"),
        "an unfinished run surfaces NO release-ready handoff on status; got:\n{out}"
    );

    // Every unit integrated, but a deferred phase-boundary gate FAILED: not done, so the
    // handoff must stay silent - the run is not releasable.
    let dir2 = temp_project();
    let root2 = dir2.path();
    seed_store(root2);
    seed_run_events(
        root2,
        &[
            ("RunStarted", r#"{"run":"r2","criteria":["spec 38"]}"#),
            ("UnitStarted", r#"{"id":"u1","agent":"worker"}"#),
            ("UnitIntegrated", r#"{"id":"u1","commit":"abc"}"#),
            ("DeferredGateFailed", r#"{"gate":"itest"}"#),
        ],
    );
    let (out, err, ok) = run_rigger(root2, &["status"]);
    assert!(ok, "rigger status must succeed; stderr:\n{err}");
    assert!(
        !out.contains("release-ready:") && !out.contains("gh pr create"),
        "a failed deferred phase-boundary gate is never advertised as releasable; got:\n{out}"
    );
}

/// `rigger dash --export` threads the run branch and the resolved release base into
/// `render_export`, so the exported static snapshot carries the SAME handoff on a done run -
/// and omits it for an unfinished run. This is the ONLY periphery coverage of the `cmd_dash`
/// export seam (no in-process test drives `cmd_dash`), and it proves the exported artifact -
/// the file an operator opens - carries the exact PR command.
#[test]
fn release_ready_handoff_reaches_the_dash_export_snapshot() {
    // A done run: the exported HTML carries the exact PR command.
    let dir = temp_project();
    let root = dir.path();
    seed_store(root);
    seed_run_events(
        root,
        &[
            ("RunStarted", r#"{"run":"r1","criteria":["spec 38"]}"#),
            ("UnitStarted", r#"{"id":"u1","agent":"worker"}"#),
            ("UnitIntegrated", r#"{"id":"u1","commit":"abc"}"#),
        ],
    );
    let (out, err, ok) = run_rigger(root, &["dash", "--export", "snapshot.html"]);
    assert!(ok, "rigger dash --export must succeed; stderr:\n{err}");
    assert!(
        out.contains("wrote dash snapshot"),
        "the export confirms it wrote the snapshot; got:\n{out}"
    );
    let html = std::fs::read_to_string(root.join("snapshot.html"))
        .expect("the export writes the snapshot file");
    assert!(
        html.contains("gh pr create --base main --head rigger-run"),
        "the exported snapshot carries the handoff's exact PR command"
    );

    // An unfinished run: the exported snapshot carries NO release-ready handoff.
    let dir2 = temp_project();
    let root2 = dir2.path();
    seed_store(root2);
    seed_run_events(
        root2,
        &[
            ("RunStarted", r#"{"run":"r2","criteria":["spec 38"]}"#),
            ("UnitStarted", r#"{"id":"u1","agent":"worker"}"#),
            ("UnitIntegrated", r#"{"id":"u1","commit":"abc"}"#),
            ("UnitStarted", r#"{"id":"u2","agent":"worker"}"#),
        ],
    );
    let (_out, err, ok) = run_rigger(root2, &["dash", "--export", "snapshot.html"]);
    assert!(ok, "rigger dash --export must succeed; stderr:\n{err}");
    let html = std::fs::read_to_string(root2.join("snapshot.html"))
        .expect("the export writes the snapshot file");
    assert!(
        !html.contains("gh pr create"),
        "an unfinished run's exported snapshot carries no release-ready handoff"
    );
}

/// The handoff PLURALIZES the integrated-unit count on `rigger status` for a run that
/// integrated MORE THAN ONE unit. Every other release-ready test seeds exactly ONE
/// integrated unit, so `integrated_units` is only ever asserted `== 1` and only the
/// singular branch of `ReleaseReady::lines` runs; the count-of-two and the plural
/// (`unit` -> `units`) arm ship unexercised, so a miscount or a wrong pluralization would
/// stay green. This drives the binary against a two-integrated-unit done run and asserts
/// the plural render reaches the operator's terminal.
#[test]
fn release_ready_pluralizes_the_unit_count_on_status_for_a_multi_unit_run() {
    let dir = temp_project();
    let root = dir.path();
    seed_store(root);
    // A done run with TWO integrated units (no failed deferred gate, no spec defect).
    seed_run_events(
        root,
        &[
            ("RunStarted", r#"{"run":"r1","criteria":["spec 38"]}"#),
            ("UnitStarted", r#"{"id":"u1","agent":"worker"}"#),
            ("UnitIntegrated", r#"{"id":"u1","commit":"abc"}"#),
            ("UnitStarted", r#"{"id":"u2","agent":"worker"}"#),
            ("UnitIntegrated", r#"{"id":"u2","commit":"def"}"#),
        ],
    );
    let (out, err, ok) = run_rigger(root, &["status"]);
    assert!(
        ok,
        "rigger status must succeed on a done run; stderr:\n{err}"
    );
    assert!(
        out.contains("release-ready:"),
        "a done multi-unit run surfaces the release-ready handoff on status; got:\n{out}"
    );
    assert!(
        out.contains("2 units integrated"),
        "the handoff pluralizes the count for more than one integrated unit (the plural \
         arm of ReleaseReady::lines), naming BOTH the count 2 and the plural noun; got:\n{out}"
    );
    assert!(
        !out.contains("1 unit integrated"),
        "a two-integrated-unit run must not render the singular count; got:\n{out}"
    );
}

/// `rigger status` names the run's PERSISTED base (spec 38, criterion 3): the base is read
/// from the run's `RunStarted` `META_BASE` metadata via `runscope::current_run_base`, so the
/// surfaced PR command targets the branch the run ACTUALLY anchored on - even though status
/// runs without the run's `--base` flag on its argv. This is the outside-in guard for the
/// base-asymmetry boundary: the persisted base must WIN over the live env re-resolution, so
/// the test seeds `META_BASE = origin/release-9.9` AND passes a DECOY `RIGGER_BASE` the
/// re-resolution would otherwise pick; a status that re-resolved (the pre-fix behavior) would
/// name the decoy. `current_run_base` / `META_BASE` are new public API exercised end-to-end
/// through the compiled binary here, which no in-process unit test does.
#[test]
fn release_ready_names_the_runs_persisted_base_on_status_over_a_re_resolution() {
    let dir = temp_project();
    let root = dir.path();
    seed_store(root);
    // A done run whose RunStarted persists its resolved run-branch base in META_BASE, the way
    // `runscope::start_fresh` stamps the resolved `--base` at mint.
    seed_done_run_with_persisted_base(root, "origin/release-9.9");

    // No `--base` on the status argv, and a DECOY `RIGGER_BASE` the fallback re-resolution
    // would pick: the persisted base must win, so the PR command names `release-9.9` (with the
    // `origin/` remote prefix stripped), never the decoy `main-decoy`.
    let (out, err, ok) =
        run_rigger_envs(root, &["status"], &[("RIGGER_BASE", "origin/main-decoy")]);
    assert!(ok, "rigger status must succeed; stderr:\n{err}");
    assert!(
        out.contains("gh pr create --base release-9.9 --head rigger-run"),
        "status names the run's PERSISTED base (origin/release-9.9 -> release-9.9), read from \
         META_BASE, not a re-resolution off the decoy RIGGER_BASE; got:\n{out}"
    );
    assert!(
        !out.contains("main-decoy"),
        "the decoy RIGGER_BASE must never reach the PR command once a base is persisted; \
         got:\n{out}"
    );
}

/// The handoff is SILENT through `rigger status` for a run that HALTED on a coverage gap - a
/// flagged `SpecDefect` - even though the one unit it did plan integrated (so `done()` alone
/// is true). Release-ready gates on the full-done predicate (`!done() || spec_defect`): a
/// spec-defective run has NOT finished the job, so it must advertise no release PR. This
/// drives the exact boundary a prior review found gate-invisible (no seeded spec-defect run),
/// proving the spec-defect conjunct of the release_ready gate holds through the binary.
#[test]
fn release_ready_is_silent_on_status_for_a_spec_defective_run() {
    let dir = temp_project();
    let root = dir.path();
    seed_store(root);
    // Every planned unit integrated, but the coverage gate flagged a SpecDefect: done() is
    // true, yet the run halted on an uncovered criterion, so it is not releasable.
    seed_run_events(
        root,
        &[
            ("RunStarted", r#"{"run":"r1","criteria":["spec 38"]}"#),
            ("UnitStarted", r#"{"id":"u1","agent":"worker"}"#),
            ("UnitIntegrated", r#"{"id":"u1","commit":"abc"}"#),
            ("SpecDefect", r#"{"criterion":"c2"}"#),
        ],
    );
    let (out, err, ok) = run_rigger(root, &["status"]);
    assert!(ok, "rigger status must succeed; stderr:\n{err}");
    assert!(
        !out.contains("release-ready:") && !out.contains("gh pr create"),
        "a run halted on a coverage gap (SpecDefect) is never advertised as releasable; \
         got:\n{out}"
    );
}

/// Seed a done run whose `RunStarted` carries a PERSISTED release base in `META_BASE`
/// metadata (spec 38, criterion 3), the way `runscope::start_fresh` stamps the resolved
/// `--base` at mint - so a later `rigger status`, which runs without the run's `--base` on
/// its argv, reads the run's ACTUAL base from the log via `runscope::current_run_base`
/// instead of re-resolving it from the environment. One unit is started and integrated so the
/// run is done and the handoff surfaces.
fn seed_done_run_with_persisted_base(root: &Path, base: &str) {
    use rigger::eventstore::namespace::Namespaced;
    use rigger::eventstore::sqlite::Store;
    use rigger::eventstore::{Event, EventStore, ExpectedRevision};

    let rigger_dir = root.join(".rigger");
    std::fs::create_dir_all(&rigger_dir).unwrap();
    let backend = Store::open(rigger_dir.join("events.db").to_str().unwrap()).unwrap();
    let store = Namespaced::new(&backend, &run_stream_identity(root));
    let events = [
        Event::new(
            rigger::run::TYPE_RUN_STARTED,
            br#"{"run":"r1","criteria":["spec 38"]}"#.to_vec(),
        )
        .with_meta(rigger::run::META_BASE, base),
        Event::new(
            rigger::ledger::TYPE_UNIT_STARTED,
            br#"{"id":"u1","agent":"worker"}"#.to_vec(),
        ),
        Event::new(
            rigger::ledger::TYPE_UNIT_INTEGRATED,
            br#"{"id":"u1","commit":"abc"}"#.to_vec(),
        ),
    ];
    store
        .append(rigger::conductor::STREAM, ExpectedRevision::Any, &events)
        .unwrap();
}

// --- Spec 39, criterion 1: idempotent always-on dash start on the native `rigger step` path.
// These periphery tests drive the BUILT binary end-to-end - the layer the dash.rs/main.rs unit
// tests (which inject the serving-check and the spawn) are structurally blind to: the real
// `cmd_step` -> `ensure_run_dashboard` -> a real, detached `rigger dash` wiring, the on-disk
// `.rigger/dash.marker` round-trip ACROSS two separate step processes, and the RIGGER_NO_DASH
// opt-out honored by the actual binary.

/// Read the per-project dash marker `.rigger/dash.marker` under `root` as its `(port, pid)`,
/// or `None` when it is absent or malformed - the test-side reader of the `port\npid` record
/// the step path writes (spec 39, criterion 1).
fn read_dash_marker(root: &Path) -> Option<(u16, u32)> {
    let s = std::fs::read_to_string(root.join(".rigger").join("dash.marker")).ok()?;
    let mut lines = s.lines();
    let port = lines.next()?.trim().parse().ok()?;
    let pid = lines.next()?.trim().parse().ok()?;
    Some((port, pid))
}

/// Best-effort kill+reap of a process by pid, so a test that drove the step path into starting
/// a real, DETACHED `rigger dash` never leaves it orphaned. Ignores every error: the pid may
/// already be gone, which is exactly the state we want.
fn reap_pid(pid: u32) {
    let _ = Command::new("kill").arg("-9").arg(pid.to_string()).status();
}

/// Run `rigger step` in `root` with the always-on step dash ENABLED - the RIGGER_NO_DASH
/// opt-out explicitly REMOVED from the environment (so an ambient opt-out in CI cannot mask the
/// behavior under test) - returning (stdout, stderr). Used only by the spec-39 idempotent-start
/// test, which reaps any dash it starts.
fn run_step_dash_enabled(root: &Path) -> (String, String) {
    let out = Command::new(rigger_bin())
        .args(["step"])
        .current_dir(root)
        .env_remove("RIGGER_NO_DASH")
        .output()
        .expect("failed to spawn the rigger binary");
    (
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

/// Spec 39, criterion 1 end-to-end, through the BUILT binary: the FIRST `rigger step` of a run
/// starts ONE persistent, detached run dashboard and records its port+pid in
/// `.rigger/dash.marker`; every LATER step of the same run finds that live marker and starts
/// NONE - never a second dash or a port fight. The unit tests prove the idempotency DECISION
/// with an injected spawn; only driving the real binary proves the wiring, the on-disk marker
/// round-trip across two separate step processes, and the `pid_is_alive` short-circuit against
/// a genuinely-serving child.
///
/// The started dash is a real, long-lived process, so this test REAPS it by pid BEFORE its
/// idempotency assertions - a failed assertion never leaks a dashboard (the reap discipline the
/// `rigger serve` dash tests already follow).
#[test]
fn step_auto_starts_one_persistent_dash_and_a_second_step_starts_none() {
    let proj = temp_git_project_with_commit();
    let root = proj.path();
    write_two_stage_workflow(root);

    // First step: no dash is recorded, so it must start one and record its marker.
    let (out1, err1) = run_step_dash_enabled(root);
    assert!(
        out1.contains(r#""wave":"#),
        "the first step must run to completion (a printed wave), reaching the dash-start seam; \
         stdout: {out1:?} stderr: {err1:?}"
    );
    let (port1, pid1) = read_dash_marker(root).unwrap_or_else(|| {
        panic!("the first step must record a dash marker at .rigger/dash.marker; stderr:\n{err1}")
    });
    // A real dash is now alive; every exit path below must reap pid1.
    assert!(
        err1.contains("serving this run"),
        "the first step announces the dash it started; stderr:\n{err1}"
    );

    // The recorded dash is a GENUINE serving process, not merely a written marker: an HTTP GET
    // of its loopback URL returns the read-only page. Reap before failing so nothing leaks.
    let url = format!("http://127.0.0.1:{port1}/");
    if !matches!(http_get(&url), Some(body) if body.contains("rigger dash")) {
        reap_pid(pid1);
        panic!("the auto-started step dash at {url} did not serve its page");
    }

    // Second step of the SAME run: it must find the live marker and start NO second dash.
    let (_out2, err2) = run_step_dash_enabled(root);
    let marker2 = read_dash_marker(root);

    // Reap every dash this test could have started BEFORE asserting, so a failed assertion
    // never leaves an orphaned dashboard behind.
    reap_pid(pid1);
    if let Some((_, pid2)) = marker2 {
        if pid2 != pid1 {
            reap_pid(pid2);
        }
    }

    assert_eq!(
        marker2,
        Some((port1, pid1)),
        "the second step must leave the marker UNCHANGED - the idempotent no-op that starts no \
         second dash"
    );
    assert!(
        !err2.contains("serving this run"),
        "the second step must announce no newly-started dash (it found the first still serving); \
         stderr:\n{err2}"
    );
}

/// Spec 39, criterion 1: the RIGGER_NO_DASH opt-out is honored by the BUILT binary on the step
/// path - a step run under it reaches and passes the dash-start seam (it prints its wave) yet
/// records NO `.rigger/dash.marker`, so a short-lived CI run or the crate's own integration
/// harness never leaks a real dashboard. The companion
/// `step_auto_starts_one_persistent_dash_and_a_second_step_starts_none` proves the SAME step
/// path DOES record a marker WITHOUT the opt-out, so this absence is the opt-out at work, not a
/// dead code path that never starts a dash at all.
#[test]
fn step_honors_the_rigger_no_dash_opt_out() {
    let proj = temp_git_project_with_commit();
    let root = proj.path();
    write_two_stage_workflow(root);

    // `run_rigger` sets RIGGER_NO_DASH=1 for exactly this reason.
    let (out, err, ok) = run_rigger(root, &["step"]);
    assert!(ok, "the step must succeed; stderr: {err}");
    assert!(
        out.contains(r#""wave":"#),
        "the step runs to completion (a printed wave), reaching the dash-start seam; stdout: {out:?}"
    );
    assert!(
        !root.join(".rigger").join("dash.marker").exists(),
        "under RIGGER_NO_DASH the step must record NO dash marker; one was written"
    );
    assert!(
        !err.contains("serving this run"),
        "under RIGGER_NO_DASH the step announces no dash; stderr:\n{err}"
    );
}
