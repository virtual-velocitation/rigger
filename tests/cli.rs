//! Integration tests for the `rigger ground` / `rigger emit` / `rigger peers` CLI
//! subcommands - the surface native-workflow agents (which have Bash, not the MCP
//! tools) use to reach rigger's grounder, event store, and context graph. They run
//! the COMPILED `rigger` binary against a throwaway project so they exercise the
//! same composition path (`Store::open(.rigger/events.db)` namespaced, the
//! `graph.db` projector, `conductor::STREAM`) the `serve` path uses.

use std::path::Path;
use std::process::Command;

// The compiled `rigger` binary under test is located at RUNTIME by the shared authority in
// `tests/common`: a path baked in at compile time goes stale the moment the target dir moves,
// and every suite that spawns the product then dies with a bare NotFound.
mod common;
use common::rigger_bin;

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
    let mut cmd = common::rigger_courier();
    cmd.args(args).current_dir(cwd);
    // The step path auto-starts a persistent, detached run dashboard (spec 39, criterion 1);
    // opt out so these short-lived integration invocations never spawn a real dashboard
    // process that would outlive the test. Set before the caller's envs so a test could still
    // override it.
    cmd.env("RIGGER_NO_DASH", "1");
    // The step/run/serve paths register this instance in the machine-global registry under
    // XDG_STATE_HOME (spec 50, criterion 2). Default it to a per-invocation temp dir so the
    // many tests that drive those paths never seed a phantom into the operator's real
    // ~/.local/state/rigger/instances - a live discovery entry, rooted at a since-deleted test
    // tempdir, that a running dash would otherwise pick up. Bound to `state` so the dir lives
    // until after the command runs; set before the caller's envs so the registry tests that pass
    // an explicit XDG_STATE_HOME (to read the registry back) still override it.
    let state = tempfile::tempdir().expect("create a temp XDG_STATE_HOME for the rigger run");
    cmd.env("XDG_STATE_HOME", state.path());
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

/// `rigger reset --runs` reclaims a SUPERSEDED edge retired before the retention boundary
/// (spec 41 criterion 1) end-to-end through the COMPILED binary - the CLI seam the library-level
/// periphery test (`tests/graph_superseded_prune.rs`) cannot reach. That test drives
/// `Projector::prune` directly; this proves the composition root in `cmd_reset` wires the seam:
/// it derives the retention boundary from the ACTIVE run's `RunStarted` (`superseded_edge_boundary`),
/// hands it to the extended prune, and reports the reclaimed-edge count on the reset line - a count
/// the pre-spec-41 message never carried and the existing node-prune test only ever sees as 0.
///
/// The superseded edge is created by DECISION supersession, which spec 41 names as one of the three
/// accumulation sources and which the reclamation SQL matches relationship-agnostically (any
/// `valid_to IS NOT NULL` row before the boundary). It is reachable through the `rigger emit`
/// courier in EITHER feature lane - unlike a structural CONTAINS edge, whose extraction is
/// symbols-gated - so this integration test drives the real binary in both lanes.
///
/// Fixture: `keep-d` (dead run r1) governs `old.rs`, then `super-d` (also r1) supersedes it - which
/// retires `keep-d`'s GOVERNS(old.rs) edge with `valid_to` = super-d's fold time, strictly BEFORE
/// r2's start. `keep-d` is re-recorded in the ACTIVE run r2 governing `new.rs`, so its NODE survives
/// the reset as a reused id (the same keep-invariant the spec-21 node prune honors) - leaving the
/// superseded-EDGE reclamation, not the node drop, as the only thing that can remove the retired
/// GOVERNS(old.rs) row. GOVERNS(new.rs) is a LIVE edge the reclamation must never touch.
#[test]
fn reset_runs_reclaims_superseded_edges_retired_before_the_active_run_and_reports_the_count() {
    let dir = temp_project();
    let root = dir.path();
    seed_store(root);

    let emit = |typ: &str, json: &str| {
        let (_o, err, ok) = run_rigger(root, &["emit", typ, json]);
        assert!(ok, "emit {typ} must succeed; stderr: {err}");
    };

    // Dead run r1's boundary (seeded directly - `rigger emit` refuses RunStarted, spec 22).
    seed_run_events(
        root,
        &[("RunStarted", r#"{"run":"r1","criteria":["crit"]}"#)],
    );
    // r1: `keep-d` governs old.rs (a live GOVERNS edge), then `super-d` supersedes keep-d - the fold
    // retires keep-d's GOVERNS(old.rs) with valid_to = super-d's fold time, strictly before r2.
    emit(
        "DecisionMade",
        r#"{"id":"keep-d","summary":"keep-d r1","governs":["old.rs"]}"#,
    );
    emit(
        "DecisionMade",
        r#"{"id":"super-d","summary":"supersedes keep-d","governs":[],"supersedes":"keep-d"}"#,
    );

    // The ACTIVE run r2's boundary - later in wall-clock than every r1 emit, so the retired edge's
    // valid_to is strictly below it (reclaimable) while r2's own edges are recent history the window
    // keeps. This is the SAME boundary `superseded_edge_boundary` derives for the reclamation.
    seed_run_events(
        root,
        &[("RunStarted", r#"{"run":"r2","criteria":["crit"]}"#)],
    );
    // r2: re-record `keep-d` governing new.rs. Its NODE now survives the reset (reused across runs),
    // and GOVERNS(new.rs) is a LIVE edge (valid_to IS NULL) the reclamation must leave untouched.
    emit(
        "DecisionMade",
        r#"{"id":"keep-d","summary":"keep-d r2","governs":["new.rs"]}"#,
    );

    // Drive the real reset. The composition root derives the boundary from r2's RunStarted, drops
    // super-d (a dead-run decision node), and reclaims exactly the ONE superseded GOVERNS(keep-d ->
    // old.rs) edge retired before that boundary - a count the pre-spec-41 reset line never reported.
    let (out, err, ok) = run_rigger(root, &["reset", "--runs"]);
    assert!(ok, "reset --runs must succeed; stderr: {err}");
    assert!(
        out.contains("reclaimed 1 superseded edge(s)"),
        "reset --runs must reclaim the one superseded edge retired before the active run; got: {out:?}"
    );
    assert!(
        out.contains("pruned 1 dead-run node(s)"),
        "reset --runs must drop the dead-run super-d node; got: {out:?}"
    );

    // LIVE is sacrosanct: keep-d's ACTIVE-run GOVERNS(new.rs) survived, so the live graph a grounding
    // consumer reads still shows keep-d governing new.rs after the reclamation removed only history.
    let (graph, err, ok) = run_rigger(root, &["graph", "--around", "new.rs", "--depth", "2"]);
    assert!(ok, "graph must succeed after reset; stderr: {err}");
    assert!(
        graph.contains("keep-d"),
        "the active-run live edge keep-d -> new.rs must survive the reclamation; got: {graph:?}"
    );

    // Idempotent at the binary boundary: a second reset at the same active-run boundary reclaims
    // NOTHING - the pre-boundary superseded row is already gone and no live/recent row is touched,
    // so the historical tail is bounded, not re-scanned into further removals.
    let (again, err, ok) = run_rigger(root, &["reset", "--runs"]);
    assert!(ok, "second reset --runs must succeed; stderr: {err}");
    assert!(
        again.contains("reclaimed 0 superseded edge(s)"),
        "a second reset reclaims nothing - the reclamation is a stable, bounded set operation; got: {again:?}"
    );
}

/// `rigger reset --runs` COMPACTS the on-disk graph after reclaiming (spec 46, criterion 3).
/// The documented pre-run hygiene command must reclaim DISK, not just rows: the prune's DELETE
/// only frees pages inside the file (SQLite keeps them for reuse), so without a VACUUM the graph
/// file stays as LARGE on disk as before even after a prune (VACUUM reclaims disk only - it changes
/// no query result and gives no query or fold speedup). This drives the COMPILED binary end to end
/// and proves the file actually SHRINKS: seed a bloated `graph.db`
/// (thousands of SUPERSEDED structural edges retired before the active run's boundary, plus ONE
/// LIVE edge), run `rigger reset --runs`, then assert the on-disk `graph.db` is STRICTLY smaller,
/// the live edge survived, and the event log is byte-for-byte the same length. Seeded directly
/// through the graph's own edge table (thousands of `rigger emit`s would be far slower) after
/// `Projector::open` lays down the canonical, fully-migrated schema so the binary re-opens it
/// with every migration a no-op. Owns the reset COMPACTION (it does NOT own the operator
/// guidance, criterion 2, nor the reclaimed-row semantics the spec-41 test already pins).
#[test]
fn reset_runs_compacts_the_on_disk_graph_after_reclaiming_superseded_rows() {
    use rigger::contextgraph::sqlite::Projector;
    use rigger::eventstore::namespace::Namespaced;
    use rigger::eventstore::sqlite::Store;
    use rigger::eventstore::{Direction, EventStore};

    let dir = temp_project();
    let root = dir.path();
    seed_store(root);
    let id = run_stream_identity(root);

    // The ACTIVE run's boundary. `superseded_edge_boundary` derives the retention cutoff from this
    // RunStarted's wall-clock `valid_from` (nanoseconds since the epoch, ~1.7e18 for 2026), so every
    // edge seeded below at `valid_to = 1` is strictly before it and thus reclaimable.
    seed_run_events(root, &[("RunStarted", r#"{"run":"r1","criteria":["c"]}"#)]);

    let graph_path = root.join(".rigger").join("graph.db");

    // Lay down the canonical, fully-migrated schema exactly as the binary opens it, so the binary's
    // own `Projector::open` re-runs every migration as a no-op and never rewrites the file itself.
    {
        let _p = Projector::open(graph_path.to_str().unwrap(), &id).unwrap();
    }

    // Bloat `graph.db` directly: a big pile of SUPERSEDED structural edges (`valid_to = 1`, strictly
    // before the boundary) the reclamation deletes, plus ONE LIVE edge (`valid_to` NULL) it must
    // preserve. One transaction, then a TRUNCATE checkpoint so the whole pile folds into the MAIN db
    // file (not the WAL side-file) before we measure its on-disk size.
    const SUPERSEDED: usize = 8_000;
    {
        let conn = rusqlite::Connection::open(&graph_path).unwrap();
        conn.execute_batch("BEGIN").unwrap();
        {
            let mut stmt = conn
                .prepare(
                    "INSERT INTO edges (from_id, to_id, rel, valid_from, valid_to, source, project, tier)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'extracted')",
                )
                .unwrap();
            for i in 0..SUPERSEDED {
                stmt.execute(rusqlite::params![
                    format!("dead-from-{i}"),
                    format!("dead-to-{i}"),
                    "GOVERNS",
                    1i64,
                    1i64, // retired at t=1, strictly before the active-run boundary
                    i as i64,
                    id,
                ])
                .unwrap();
            }
            // The one LIVE edge (valid_to NULL) the reclamation must never touch.
            stmt.execute(rusqlite::params![
                "keep-from",
                "keep-to",
                "GOVERNS",
                2i64,
                None::<i64>,
                999i64,
                id,
            ])
            .unwrap();
        }
        conn.execute_batch("COMMIT").unwrap();
        // Fold the WAL into the main db file so the measured on-disk size reflects the whole pile.
        let _: (i64, i64, i64) = conn
            .query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?))
            })
            .unwrap();
    }

    let before = std::fs::metadata(&graph_path).unwrap().len();

    // Drive the real reset: it reclaims every pre-boundary superseded edge, then VACUUMs so the file
    // shrinks to match, reporting BOTH on the reset line.
    let (out, err, ok) = run_rigger(root, &["reset", "--runs"]);
    assert!(
        ok,
        "reset --runs must succeed; stderr: {err}\nstdout: {out}"
    );
    assert!(
        out.contains(&format!("reclaimed {SUPERSEDED} superseded edge(s)")),
        "reset --runs must reclaim all {SUPERSEDED} seeded superseded edges; got: {out:?}"
    );
    assert!(
        out.contains("compact"),
        "reset --runs must report the graph-file compaction alongside the reclaimed count; got: {out:?}"
    );

    // The on-disk graph file is STRICTLY smaller: a VACUUM ran. Without the compaction the DELETE
    // only frees internal pages and the file stays exactly `before` bytes.
    let after = std::fs::metadata(&graph_path).unwrap().len();
    assert!(
        after < before,
        "reset --runs must COMPACT the on-disk graph (a VACUUM ran): file went from {before} to {after} bytes"
    );

    // LIVE is sacrosanct and every superseded edge is gone: the reclamation removed only history.
    let conn = rusqlite::Connection::open(&graph_path).unwrap();
    let live: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM edges WHERE from_id = 'keep-from' AND to_id = 'keep-to' \
             AND valid_to IS NULL AND project = ?1",
            rusqlite::params![id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        live, 1,
        "the one LIVE edge must survive the compacting prune"
    );
    let dead: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM edges WHERE valid_to IS NOT NULL AND project = ?1",
            rusqlite::params![id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        dead, 0,
        "every superseded edge retired before the boundary must be reclaimed"
    );
    drop(conn);

    // The event log is UNTOUCHED - the compaction rewrites only the graph projection file, never the
    // store: the seeded RunStarted is still the one and only event.
    let backend = Store::open(root.join(".rigger").join("events.db").to_str().unwrap()).unwrap();
    let store = Namespaced::new(&backend, &id);
    let log = store
        .read_stream(rigger::conductor::STREAM, 0, Direction::Forward)
        .unwrap();
    assert_eq!(
        log.len(),
        1,
        "reset --runs must not touch the event log - the seeded RunStarted is still the only event"
    );
}

/// Extract the compaction's REPORTED reclaimed byte count from a `rigger reset --runs` line
/// (spec 46, criterion 3). The line reads `... then compacted the graph file (reclaimed N
/// byte(s) on disk) ...`; this parses `N`. Returns `None` when the phrase is absent, so a
/// caller can distinguish "compaction was reported" from "no compaction phrase at all".
fn reported_reclaimed_bytes(reset_line: &str) -> Option<u64> {
    let marker = "compacted the graph file (reclaimed ";
    let start = reset_line.find(marker)? + marker.len();
    let rest = &reset_line[start..];
    let end = rest.find(" byte(s)")?;
    rest[..end].trim().parse().ok()
}

/// `rigger reset --runs` REPORTS the bytes its compaction reclaimed, and a SECOND pass is an
/// idempotent no-op (spec 46, criterion 3). Two operator-facing boundaries the happy-path
/// compaction test does not reach, both driven through the COMPILED binary: (1) the reset line
/// actually REPORTS the compaction with a PARSEABLE, non-zero reclaimed-byte count on a real
/// prune - guarding the `cmd_reset` report seam that formats `Projector::compact`'s return value
/// (the happy-path test only checks the line CONTAINS "compact", which a `reclaimed 0` report
/// also satisfies). The checkpoint that makes the on-disk shrink SYNCHRONOUS is guarded
/// separately and precisely by `projector_compact_returns_the_on_disk_bytes_reclaimed_and_never_changes_a_query`,
/// which reads the file in-process; here the count is `page_count`-based and correct regardless.
/// (2) Running the documented hygiene command TWICE must be safe: the second pass finds nothing
/// retired before the boundary, so it prunes 0 edges, reclaims 0 bytes (the "nothing was freed
/// reclaims 0" contract), never GROWS the file, keeps the one live edge, and never touches the
/// event log. Seeds the bloated `graph.db` directly through the edge table (thousands of `rigger
/// emit`s would be far slower) after `Projector::open` lays the canonical migrated schema, so the
/// binary re-opens it with every migration a no-op.
#[test]
fn reset_runs_reports_nonzero_bytes_reclaimed_then_a_second_pass_is_an_idempotent_no_op() {
    use rigger::contextgraph::sqlite::Projector;
    use rigger::eventstore::namespace::Namespaced;
    use rigger::eventstore::sqlite::Store;
    use rigger::eventstore::{Direction, EventStore};

    let dir = temp_project();
    let root = dir.path();
    seed_store(root);
    let id = run_stream_identity(root);

    // The ACTIVE run's boundary; every edge seeded at `valid_to = 1` is strictly before it and
    // thus reclaimable on the FIRST pass, while nothing seeded after it survives to a second pass.
    seed_run_events(root, &[("RunStarted", r#"{"run":"r1","criteria":["c"]}"#)]);

    let graph_path = root.join(".rigger").join("graph.db");

    // Lay down the canonical, fully-migrated schema exactly as the binary opens it, so the binary's
    // own `Projector::open` re-runs every migration as a no-op and never rewrites the file itself.
    {
        let _p = Projector::open(graph_path.to_str().unwrap(), &id).unwrap();
    }

    // Bloat `graph.db`: a big pile of SUPERSEDED structural edges the first reset reclaims, plus ONE
    // LIVE edge (`valid_to` NULL) it must preserve across BOTH passes. One transaction, then a
    // TRUNCATE checkpoint so the whole pile lands in the MAIN file before it is measured.
    const SUPERSEDED: usize = 8_000;
    {
        let conn = rusqlite::Connection::open(&graph_path).unwrap();
        conn.execute_batch("BEGIN").unwrap();
        {
            let mut stmt = conn
                .prepare(
                    "INSERT INTO edges (from_id, to_id, rel, valid_from, valid_to, source, project, tier)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'extracted')",
                )
                .unwrap();
            for i in 0..SUPERSEDED {
                stmt.execute(rusqlite::params![
                    format!("dead-from-{i}"),
                    format!("dead-to-{i}"),
                    "GOVERNS",
                    1i64,
                    1i64, // retired at t=1, strictly before the active-run boundary
                    i as i64,
                    id,
                ])
                .unwrap();
            }
            stmt.execute(rusqlite::params![
                "keep-from",
                "keep-to",
                "GOVERNS",
                2i64,
                None::<i64>,
                999i64,
                id,
            ])
            .unwrap();
        }
        conn.execute_batch("COMMIT").unwrap();
        let _: (i64, i64, i64) = conn
            .query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?))
            })
            .unwrap();
    }

    let before = std::fs::metadata(&graph_path).unwrap().len();

    // FIRST pass: reclaims every pre-boundary superseded edge and reports a NON-ZERO reclamation.
    let (out1, err1, ok1) = run_rigger(root, &["reset", "--runs"]);
    assert!(
        ok1,
        "first reset --runs must succeed; stderr: {err1}\n{out1}"
    );
    assert!(
        out1.contains(&format!("reclaimed {SUPERSEDED} superseded edge(s)")),
        "first reset --runs must reclaim all {SUPERSEDED} superseded edges; got: {out1:?}"
    );
    let reclaimed1 = reported_reclaimed_bytes(&out1)
        .unwrap_or_else(|| panic!("first reset --runs must REPORT a compaction; got: {out1:?}"));
    // A real prune must format a PARSEABLE, non-zero reclaimed-byte count through the `cmd_reset`
    // report seam. That count is `page_count`-based (measured BEFORE the VACUUM), so it is non-zero
    // whenever the VACUUM actually frees pages, independent of the WAL checkpoint - this assertion
    // guards the report seam, NOT the checkpoint. The checkpoint's own guarantee, that the on-disk
    // file shrinks SYNCHRONOUSLY while the connection is still open, is guarded precisely by
    // `projector_compact_returns_the_on_disk_bytes_reclaimed_and_never_changes_a_query`.
    assert!(
        reclaimed1 > 0,
        "first reset --runs must REPORT a non-zero reclaimed-byte count on a real prune; got {reclaimed1} from {out1:?}"
    );
    let after1 = std::fs::metadata(&graph_path).unwrap().len();
    assert!(
        after1 < before,
        "first reset --runs must shrink the on-disk graph: {before} -> {after1} bytes"
    );

    // SECOND pass: nothing is retired before the boundary anymore, so it is a provable no-op.
    let (out2, err2, ok2) = run_rigger(root, &["reset", "--runs"]);
    assert!(
        ok2,
        "second reset --runs must succeed; stderr: {err2}\n{out2}"
    );
    assert!(
        out2.contains("reclaimed 0 superseded edge(s)"),
        "second reset --runs must prune nothing (all cruft already gone); got: {out2:?}"
    );
    let reclaimed2 = reported_reclaimed_bytes(&out2)
        .unwrap_or_else(|| panic!("second reset --runs must REPORT a compaction; got: {out2:?}"));
    assert_eq!(
        reclaimed2, 0,
        "second reset --runs compacts an already-compact file, so nothing was freed and it reclaims 0 bytes; got {reclaimed2}"
    );
    let after2 = std::fs::metadata(&graph_path).unwrap().len();
    assert!(
        after2 <= after1,
        "an idempotent second reset --runs must never GROW the on-disk graph: {after1} -> {after2} bytes"
    );

    // The one LIVE edge is still there after both passes, and no superseded edge lingers.
    let conn = rusqlite::Connection::open(&graph_path).unwrap();
    let live: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM edges WHERE from_id = 'keep-from' AND to_id = 'keep-to' \
             AND valid_to IS NULL AND project = ?1",
            rusqlite::params![id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(live, 1, "the LIVE edge must survive both reset passes");
    drop(conn);

    // The event log is byte-for-byte the same length after both passes: compaction rewrites only
    // the projection file, never the store (the seeded RunStarted is still the one and only event).
    let backend = Store::open(root.join(".rigger").join("events.db").to_str().unwrap()).unwrap();
    let store = Namespaced::new(&backend, &id);
    let log = store
        .read_stream(rigger::conductor::STREAM, 0, Direction::Forward)
        .unwrap();
    assert_eq!(
        log.len(),
        1,
        "two reset --runs passes must not touch the event log - the seeded RunStarted is still the only event"
    );
}

/// `Projector::compact()` is the new PUBLIC graph-mutation API (spec 46, criterion 3); this drives
/// it DIRECTLY (the CLI tests reach it only through `rigger reset --runs`) to pin its return-value
/// contract at the port edge: (1) after a `prune` frees pages onto the freelist, `compact()` returns
/// a POSITIVE byte count that EXACTLY equals the on-disk file-size drop it caused - the reported
/// reclamation is the truth on disk, not an estimate (this also fails closed if the internal
/// `wal_checkpoint(TRUNCATE)` is removed: with the connection still open the main file has not
/// shrunk, so `reclaimed > 0` and `reclaimed == before - after` cannot both hold); (2) it never
/// changes a QUERY result - the whole live projection is byte-identical before and after, only the
/// file size moves; and (3) a second `compact()` on the now-compact file freed nothing, so it
/// reclaims exactly 0 bytes and does not shrink the file further. Seeds directly through the edge
/// table for speed, then prunes and compacts through the real `Projector` API on ONE connection.
#[test]
fn projector_compact_returns_the_on_disk_bytes_reclaimed_and_never_changes_a_query() {
    use rigger::contextgraph::sqlite::Projector;

    let dir = temp_project();
    let root = dir.path();
    let id = run_stream_identity(root);
    let rigger_dir = root.join(".rigger");
    std::fs::create_dir_all(&rigger_dir).unwrap();
    let graph_path = rigger_dir.join("graph.db");

    // Lay the canonical migrated schema, then bloat the file directly and fold the WAL into the MAIN
    // file, so the pre-compaction on-disk size is authoritative before the Projector under test opens.
    {
        let _p = Projector::open(graph_path.to_str().unwrap(), &id).unwrap();
    }
    const SUPERSEDED: usize = 8_000;
    {
        let conn = rusqlite::Connection::open(&graph_path).unwrap();
        conn.execute_batch("BEGIN").unwrap();
        {
            let mut stmt = conn
                .prepare(
                    "INSERT INTO edges (from_id, to_id, rel, valid_from, valid_to, source, project, tier)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'extracted')",
                )
                .unwrap();
            for i in 0..SUPERSEDED {
                stmt.execute(rusqlite::params![
                    format!("dead-from-{i}"),
                    format!("dead-to-{i}"),
                    "GOVERNS",
                    1i64,
                    1i64, // superseded, retired before any boundary -> reclaimable
                    i as i64,
                    id,
                ])
                .unwrap();
            }
            // Three LIVE edges that whole() returns; the query result must be invariant across compact.
            for (f, t, vf, src) in [
                ("live-a", "live-b", 10i64, 1_000i64),
                ("live-c", "live-d", 20i64, 1_001i64),
                ("live-e", "live-f", 30i64, 1_002i64),
            ] {
                stmt.execute(rusqlite::params![f, t, "GOVERNS", vf, None::<i64>, src, id])
                    .unwrap();
            }
        }
        conn.execute_batch("COMMIT").unwrap();
        let _: (i64, i64, i64) = conn
            .query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?))
            })
            .unwrap();
    }

    // Comparable projection of the WHOLE live graph (source Position aside, which whole() derives):
    // the identity of every live edge the compaction must preserve exactly.
    let project = |p: &Projector| -> Vec<(String, String, String, i64, Option<i64>, String)> {
        let g = p.whole().unwrap();
        let mut edges: Vec<_> = g
            .edges
            .iter()
            .map(|e| {
                (
                    e.from.clone(),
                    e.to.clone(),
                    e.rel.clone(),
                    e.valid_from,
                    e.valid_to,
                    e.tier.clone(),
                )
            })
            .collect();
        edges.sort();
        edges
    };

    let graph = Projector::open(graph_path.to_str().unwrap(), &id).unwrap();

    // Free the superseded pile onto the freelist. `valid_to = 1 < 2`, so a boundary of 2 reclaims
    // every superseded edge; the live edges (`valid_to` NULL) are never matched. The DELETE frees
    // internal pages WITHOUT shrinking the main file - exactly the state compact() exists to fix.
    let pruned = graph.prune(&[], Some(2)).unwrap();
    assert_eq!(
        pruned.superseded_edges, SUPERSEDED,
        "prune must free all {SUPERSEDED} superseded edges onto the freelist before compaction"
    );

    let before = std::fs::metadata(&graph_path).unwrap().len();
    let query_before = project(&graph);
    assert_eq!(
        query_before.len(),
        3,
        "the three LIVE edges are the query result compaction must preserve"
    );

    // The API under test: compact and read back the reclaimed byte count.
    let reclaimed = graph.compact().unwrap();
    let after = std::fs::metadata(&graph_path).unwrap().len();

    assert!(
        reclaimed > 0,
        "compact() must reclaim the freed pages after a prune; got {reclaimed} bytes"
    );
    assert!(
        after < before,
        "compact() must shrink the on-disk file: {before} -> {after} bytes"
    );
    // The reported reclamation IS the on-disk truth (and fails closed without the WAL checkpoint,
    // since the still-open connection would leave `after == before`).
    assert_eq!(
        reclaimed,
        before - after,
        "compact() must return EXACTLY the on-disk bytes it reclaimed: reported {reclaimed}, file dropped {}",
        before - after
    );

    // Only the file size moved: the whole live projection is byte-identical.
    let query_after = project(&graph);
    assert_eq!(
        query_before, query_after,
        "compact() must never change a query result - the live projection must be identical after the VACUUM"
    );

    // A second compaction freed nothing: it reclaims exactly 0 bytes and does not shrink further.
    let reclaimed_again = graph.compact().unwrap();
    let after_again = std::fs::metadata(&graph_path).unwrap().len();
    assert_eq!(
        reclaimed_again, 0,
        "compacting an already-compact file frees nothing, so it must reclaim 0 bytes; got {reclaimed_again}"
    );
    assert_eq!(
        after_again, after,
        "a no-op compaction must not change the on-disk file size: {after} -> {after_again}"
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
/// explicitly: the structural `symbols` grounder is the default now, so an unconfigured
/// project would resolve to it (a tree-sitter symbol index, not grep's exact-line /
/// no-match-empty / k-cap contract). Pinning grep keeps the test focused on the literal
/// grounder's behavior.
fn write_grounder_workflow(root: &Path, grounder: &str) {
    let rigger = root.join(".rigger");
    // The agents/ dir must exist for `config::load` to succeed; without it the load
    // fails and `cmd_ground` falls back to the UNSET grounder (which resolves to
    // symbols), so the pinned `grounder` would never take effect.
    std::fs::create_dir_all(rigger.join("agents")).unwrap();
    std::fs::write(
        rigger.join("workflow.yml"),
        format!("name: t\ndefaults:\n  grounder: {grounder}\n"),
    )
    .unwrap();
}

/// `rigger ground "<query>"` returns repo references (`file:line: <text>`) from the
/// project's configured grounder over a small temp repo. This pins the LITERAL grep
/// grounder (its exact-line / empty-on-no-match / k-cap contract); the structural
/// `symbols` grounder, the default, is exercised by its own unit tests.
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

/// The count of `CodeEntityExtracted` events the cold-checkout `graph build` recorded into the
/// run stream, read back through the same namespaced store the binary writes. Used to prove the
/// incremental refresh: a re-build over an unchanged tree re-ingests NOTHING (the content key of
/// an unchanged file is already recorded), so this count is stable across a second build.
#[cfg(feature = "symbols")]
fn code_entity_event_count(root: &Path) -> usize {
    use rigger::eventstore::namespace::Namespaced;
    use rigger::eventstore::sqlite::Store;
    use rigger::eventstore::{Direction, EventStore};

    let backend = Store::open(root.join(".rigger").join("events.db").to_str().unwrap()).unwrap();
    let store = Namespaced::new(&backend, &run_stream_identity(root));
    store
        .read_stream(rigger::conductor::STREAM, 0, Direction::Forward)
        .unwrap()
        .iter()
        .filter(|e| e.type_ == rigger::contextgraph::TYPE_CODE_ENTITY_EXTRACTED)
        .count()
}

/// Cold-checkout build (spec 45, criterion 3): `rigger graph build` folds the project's source
/// into `.rigger/graph.db` with NO run - no `RunStarted`, no event beyond the code-ingest events
/// the fold already emits - so the graph is populated from source alone, on a repo the tool has
/// merely cloned. Proven end-to-end through the shipped surface: `graph build` creates the store,
/// then `graph --around` reads back the code-entity nodes and the `CALLS` edge the fold emits.
/// The second build proves the incremental refresh - an unchanged tree re-ingests nothing.
#[cfg(feature = "symbols")]
#[test]
fn graph_build_folds_source_into_the_graph_with_no_run() {
    let dir = temp_project();
    let root = dir.path();
    // A callee and a caller in one file: the fold emits `combat.rs::helper` / `combat.rs::caller`
    // code-entity nodes and a `combat.rs::caller --CALLS--> combat.rs::helper` edge.
    std::fs::write(
        root.join("combat.rs"),
        "fn helper() {}\nfn caller() { helper(); }\n",
    )
    .unwrap();

    let (out, err, ok) = run_rigger(root, &["graph", "build"]);
    assert!(ok, "graph build must succeed; stderr: {err}; stdout: {out}");
    assert!(
        root.join(".rigger").join("graph.db").exists(),
        "graph build must create .rigger/graph.db from a cold checkout"
    );

    // Read it back through the shipped inspector command: the code-entity nodes AND the CALLS edge
    // were folded from source alone (no run seeded `graph_seeds`, no conductor ingest ran).
    let (g, gerr, gok) = run_rigger(root, &["graph", "--around", "combat.rs::helper"]);
    assert!(
        gok,
        "graph --around must succeed after a build; stderr: {gerr}"
    );
    assert!(
        g.contains("caller") && g.contains("helper"),
        "the code-entity nodes must be folded from source; got:\n{g}"
    );
    assert!(
        g.contains("-CALLS->"),
        "the CALLS edge the fold emits must be present in the built graph; got:\n{g}"
    );

    // Incremental refresh: a second build over the byte-identical tree re-ingests NOTHING (the
    // content-keyed dedup, seeded from the log) and still exits clean.
    let before = code_entity_event_count(root);
    assert!(
        before > 0,
        "the first build must have recorded code-ingest events"
    );
    let (_o2, e2, ok2) = run_rigger(root, &["graph", "build"]);
    assert!(ok2, "a second graph build must succeed; stderr: {e2}");
    assert_eq!(
        before,
        code_entity_event_count(root),
        "a re-build over an unchanged tree must not re-ingest (content-keyed incremental refresh)"
    );
}

/// Cold-checkout build degrades cleanly in BOTH feature lanes (spec 45 global constraint): with
/// the extraction pass off (`--no-default-features`) `graph build` has nothing to walk, so it
/// produces an EMPTY graph - it still opens/creates the store and exits 0, never an error. Run
/// without a `#[cfg]` so it exercises whichever lane the test binary is built for; in the symbols
/// lane the empty tree likewise yields an empty graph, so the clean-exit contract holds in both.
#[test]
fn graph_build_exits_clean_and_creates_the_store_in_both_lanes() {
    let dir = temp_project();
    let root = dir.path();
    let (out, err, ok) = run_rigger(root, &["graph", "build"]);
    assert!(
        ok,
        "graph build must exit clean with nothing to ingest, in both lanes; stderr: {err}; stdout: {out}"
    );
    assert!(
        root.join(".rigger").join("graph.db").exists(),
        "graph build must create .rigger/graph.db even when there is nothing to ingest"
    );
}

/// API contract of the shared ingest authority (spec 45): `rigger::ingest::ingest_project` is
/// the ONE walk-and-content-key entry BOTH the live run (`conductor`) and the cold `graph build`
/// (`main`) call, so the content key an event is deduped under can never fork between them.
/// Proven at the API edge rather than only end-to-end through one caller: the function is a
/// DETERMINISTIC function of the tree - two calls over one unchanged tree emit the identical SET
/// of keys - and every key carries the documented `<prefix>/<file>@<hash>#<idx>` shape (`gc` for
/// the code half, `gd` for the design half). Because the keys are a pure function of the tree,
/// any two callers passing the same root necessarily agree; that IS the "cannot drift between
/// the two ingest entries" contract. The incremental refresh (an unchanged file re-ingests
/// nothing) rests on this same identical-keys property.
#[cfg(feature = "symbols")]
#[test]
fn ingest_project_emits_deterministic_content_keys() {
    use std::collections::BTreeSet;

    let dir = temp_project();
    let root = dir.path();
    // A `.rigger/` for the symbols grounder to persist its index under, mirroring the store
    // `graph build` creates before it walks.
    std::fs::create_dir_all(root.join(".rigger")).unwrap();
    std::fs::write(
        root.join("combat.rs"),
        "fn helper() {}\nfn caller() { helper(); }\n",
    )
    .unwrap();
    let root_str = root.to_str().unwrap();

    let collect_keys = || {
        let mut keys: BTreeSet<String> = BTreeSet::new();
        rigger::ingest::ingest_project(root_str, |key, _ev| {
            keys.insert(key.to_string());
        });
        keys
    };

    let first = collect_keys();
    let second = collect_keys();

    assert!(
        !first.is_empty(),
        "the symbols-lane walk must emit content keys for a .rs source file"
    );
    assert_eq!(
        first, second,
        "the shared ingest authority must key an unchanged tree identically across calls - the \
         property that keeps the run and a cold `graph build` from forking the dedup key"
    );
    // Every key is `<prefix>/<file>@<hash>#<idx>` with the prefix in {gc, gd}.
    for key in &first {
        let (prefix, rest) = key
            .split_once('/')
            .unwrap_or_else(|| panic!("key {key:?} must be `<prefix>/<file>@<hash>#<idx>`"));
        assert!(
            prefix == "gc" || prefix == "gd",
            "key {key:?} must carry the `gc` (code) or `gd` (design) prefix"
        );
        let (file_hash, idx) = rest
            .rsplit_once('#')
            .unwrap_or_else(|| panic!("key {key:?} must end with `#<idx>`"));
        assert!(
            !idx.is_empty() && idx.chars().all(|c| c.is_ascii_digit()),
            "key {key:?} must end with a numeric event index"
        );
        assert!(
            file_hash.contains('@'),
            "key {key:?} must carry the `@<hash>` content fingerprint"
        );
    }
    assert!(
        first.iter().any(|k| k.starts_with("gc/")),
        "the code half (spec 29a) must emit a `gc/` key for combat.rs; got: {first:?}"
    );
}

/// Cross-module scope seam of the shared walk (spec 49, section 3): the ingest walk is scoped to the
/// project's OWN sources, and because EVERY ingest half rides the ONE shared
/// `grounder::walk_guarded` skeleton, the SHIPPED `rigger::ingest::ingest_project` authority scopes
/// BOTH halves at once - the code half (`gc`) AND the design half (`gd`). Proven at the shipped API
/// edge over a real git repo (the way a live run and a cold `graph build` actually ingest). The
/// fixture seeds an in-root source carrying BOTH a code entity and inline `// WHY:` rationale (so one
/// file yields a `gc` and a `gd` key), alongside four never-source locations each seeded with the
/// SAME shape so a leak surfaces in either half: a directory the repo's OWN committed `.gitignore`
/// excludes (`build/`); the VCS metadata dir `.git` and rigger's runtime dir `.rigger` (the
/// always-excluded dotdirs); and a symlink escaping the root (root confinement - the link is never
/// followed). The DESIGN half is what the implementer's `.rs`-only ingest unit test does not
/// exercise (its in-root source carries no rationale, so no `gd` batch is produced); driving the
/// shipped authority proves the shared walk scopes the design half too. A leak from any excluded
/// path into EITHER half - or a regression that hand-rolled a second, unscoped walk for one
/// consumer - fails here.
#[cfg(all(unix, feature = "symbols"))]
#[test]
fn ingest_project_scopes_the_walk_to_the_project_across_both_halves() {
    use std::collections::BTreeSet;

    let dir = temp_project();
    let root = dir.path();
    // The symbols grounder persists its index under `.rigger/`, exactly as `graph build`/a run do.
    std::fs::create_dir_all(root.join(".rigger")).unwrap();

    // (a) The one in-root source the walk MUST ingest, carrying BOTH a code entity (a `gc` batch)
    // and an inline `// WHY:` rationale (a `gd` batch), so a single file proves both halves.
    std::fs::write(
        root.join("keep.rs"),
        "// WHY: this in-root source is first-party; the scoped walk must ingest it\nfn kept() {}\n",
    )
    .unwrap();

    // (b) A build tree the project declares not-source via its OWN committed `.gitignore`.
    std::fs::write(root.join(".gitignore"), "build/\n").unwrap();
    // Every excluded location carries the SAME `code + rationale` shape as `keep.rs`, so a leak
    // would surface as either a `gc/<path>` or a `gd/<path>` key.
    let decoy = "// WHY: excluded - must never be ingested\nfn decoy() {}\n";
    for excluded in ["build", ".git", ".rigger"] {
        std::fs::create_dir_all(root.join(excluded)).unwrap();
        std::fs::write(root.join(excluded).join("decoy.rs"), decoy).unwrap();
    }

    // (c) A symlink escaping the root: the file beyond it must never be reached through the link.
    let outside = tempfile::tempdir().unwrap();
    std::fs::write(outside.path().join("decoy.rs"), decoy).unwrap();
    std::os::unix::fs::symlink(outside.path(), root.join("outsider")).unwrap();

    // Drive the SHIPPED ingest authority (the entry `graph build` and the live run share) and
    // collect the FILE each emitted content key names, split by half.
    let mut gc_files: BTreeSet<String> = BTreeSet::new();
    let mut gd_files: BTreeSet<String> = BTreeSet::new();
    rigger::ingest::ingest_project(root.to_str().unwrap(), |key, _ev| {
        let (prefix, rest) = key
            .split_once('/')
            .unwrap_or_else(|| panic!("key {key:?} must be `<prefix>/<file>@<hash>#<idx>`"));
        let file = rest.split('@').next().unwrap().to_string();
        match prefix {
            "gc" => {
                gc_files.insert(file);
            }
            "gd" => {
                gd_files.insert(file);
            }
            other => panic!("unexpected key prefix {other:?} in {key:?}"),
        }
    });

    // Both halves ingested the in-root source: the code half emitted a `gc` key for it ...
    assert!(
        gc_files.contains("keep.rs"),
        "the code half must ingest the in-root source; got gc={gc_files:?}"
    );
    // ... and the design half emitted a `gd` key for its `// WHY:` rationale (the half the
    // implementer's `.rs`-only ingest unit test does not reach).
    assert!(
        gd_files.contains("keep.rs"),
        "the design half must ingest the in-root source's rationale; got gd={gd_files:?}"
    );

    // No excluded path leaked into EITHER half - not the gitignored build tree, the VCS metadata,
    // rigger's runtime dir, nor anything reached by escaping the root through the symlink.
    for file in gc_files.iter().chain(gd_files.iter()) {
        assert!(
            !file.starts_with("build/")
                && !file.starts_with(".git")
                && !file.starts_with(".rigger")
                && !file.starts_with("outsider"),
            "an excluded path leaked into the ingest: {file:?} (gc={gc_files:?}, gd={gd_files:?})"
        );
    }
    // Concretely: across BOTH halves, the walk ingested EXACTLY the one in-root source, nothing else.
    assert_eq!(
        &gc_files | &gd_files,
        BTreeSet::from(["keep.rs".to_string()]),
        "the scoped walk must ingest only the project's own in-root source; \
         gc={gc_files:?}, gd={gd_files:?}"
    );
}

/// Light-lane contract of the shared ingest authority (spec 45 global constraint): with the
/// extraction pass off (`--no-default-features`), `rigger::ingest::ingest_project` is a genuine
/// NO-OP - it emits NOTHING even over a tree that carries real `.rs` source, which is why
/// `graph build` there degrades to an empty graph rather than erroring. This asserts strictly
/// more than the empty-tree CLI test can: real source is present, yet the light-lane authority
/// still yields zero events, so the no-op is the lane's behavior and not merely an empty input.
#[cfg(not(feature = "symbols"))]
#[test]
fn ingest_project_is_a_noop_in_the_light_lane() {
    let dir = temp_project();
    let root = dir.path();
    std::fs::create_dir_all(root.join(".rigger")).unwrap();
    std::fs::write(
        root.join("combat.rs"),
        "fn helper() {}\nfn caller() { helper(); }\n",
    )
    .unwrap();

    let mut emits = 0usize;
    rigger::ingest::ingest_project(root.to_str().unwrap(), |_key, _ev| {
        emits += 1;
    });
    assert_eq!(
        emits, 0,
        "the light lane compiles no extraction pass, so the ingest authority must emit nothing"
    );
}

/// Cold-checkout build resolves the whole-project root, not the cwd (spec 45): a `graph build`
/// launched from a SUBDIRECTORY still folds the ENTIRE project - `cmd_graph_build` walks the git
/// top-level (the same root a run's `deps.repo` carries), not the directory it was invoked from.
/// The implementer's build test runs from the project root, so this guards the root-resolution
/// seam it leaves open: a root-level source file is folded even though the build ran from a
/// nested dir, read back through the shipped `graph --around` inspector run in that same
/// subdirectory (so the build and the read share one store, isolating the tree-root property).
#[cfg(feature = "symbols")]
#[test]
fn graph_build_from_a_subdirectory_ingests_the_whole_project() {
    let dir = temp_project();
    let root = dir.path();
    std::fs::write(
        root.join("combat.rs"),
        "fn helper() {}\nfn caller() { helper(); }\n",
    )
    .unwrap();
    let sub = root.join("engine").join("nested");
    std::fs::create_dir_all(&sub).unwrap();

    let (out, err, ok) = run_rigger(&sub, &["graph", "build"]);
    assert!(
        ok,
        "graph build from a subdirectory must succeed; stderr: {err}; stdout: {out}"
    );

    // Read back from the SAME subdirectory (the store the subdir build wrote): the root-level
    // combat.rs entities were folded, proving the walk rooted at the git top-level, not the cwd.
    let (g, gerr, gok) = run_rigger(&sub, &["graph", "--around", "combat.rs::helper"]);
    assert!(
        gok,
        "graph --around must succeed after a subdirectory build; stderr: {gerr}"
    );
    assert!(
        g.contains("caller") && g.contains("helper"),
        "a root-level source file must be folded by a build launched from a subdirectory; got:\n{g}"
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

/// PERIPHERY (integration): a RETIRED grounder (`turbovec` / `vector` / `hybrid`, spec 57)
/// configured in `defaults.grounder` must make the shipped `rigger ground` binary FAIL
/// LOUDLY with the migration message, never silently degrade to grep. This drives the whole
/// operator seam the in-module resolver test cannot see: `.rigger/workflow.yml` ->
/// `config::load` -> `select_grounder` -> `grounder::grounder_for` -> the process's stderr +
/// exit code. It is feature-INDEPENDENT: `select_grounder` only intercepts the unset /
/// `symbols` default for the real grounder, so every retired name falls through to
/// `grounder_for` in BOTH lanes - hence ungated. The NON-ZERO exit is the discriminating
/// signal: a silent grep degrade would exit 0, so `!ok` is what keeps this test non-vacuous.
#[test]
fn ground_rejects_a_retired_grounder_with_the_migration_error() {
    let dir = temp_project();
    let root = dir.path();
    for name in ["turbovec", "vector", "hybrid"] {
        write_grounder_workflow(root, name);
        let (out, err, ok) = run_rigger(root, &["ground", "apply_damage"]);
        assert!(
            !ok,
            "a retired grounder {name:?} must FAIL the process, never silently ground via grep; \
             stdout: {out:?}, stderr: {err:?}"
        );
        let low = err.to_lowercase();
        assert!(
            low.contains("retire") && low.contains("symbols"),
            "the migration error must name the retirement and the symbols default; \
             grounder {name:?} stderr: {err}"
        );
        assert!(
            !low.contains("feature"),
            "a retired name is gone for good - not a missing feature to rebuild; \
             grounder {name:?} stderr: {err}"
        );
        assert!(
            !err.contains("unknown grounder"),
            "a retired name must not read as a typo; grounder {name:?} stderr: {err}"
        );
    }
}

/// PERIPHERY (integration): an UNKNOWN grounder name makes `rigger ground` fail with a
/// message advertising the EXACT accepted set (`symbols` (default) / `grep` / `nop`) and
/// nothing else - not a retired engine, not a silent grep degrade. Same operator seam as
/// the retired-name test; feature-independent (unknown names route through `grounder_for`
/// in both lanes), so ungated.
#[test]
fn ground_rejects_an_unknown_grounder_naming_only_the_accepted_set() {
    let dir = temp_project();
    let root = dir.path();
    write_grounder_workflow(root, "semantic-embed");
    let (out, err, ok) = run_rigger(root, &["ground", "apply_damage"]);
    assert!(
        !ok,
        "an unknown grounder must fail the process, never silently ground; stdout: {out:?}"
    );
    assert!(
        err.contains("unknown grounder"),
        "an unknown name must hit the generic unknown-grounder arm; stderr: {err}"
    );
    assert!(
        err.contains("symbols") && err.contains("grep") && err.contains("nop"),
        "the unknown-name message must advertise the accepted set; stderr: {err}"
    );
    assert!(
        !err.to_lowercase().contains("turbovec"),
        "a typo must not advertise a retired engine as a choice; stderr: {err}"
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

/// Spec 51, criterion 5 (SWEEP-BEFORE-ADD ORDERING): within one `rigger step`, no worktree ADD
/// begins until the terminal-worktree SWEEP has completed, and both worktree mutations happen
/// UNDER the step's serialization (the step lock). This pins the lifecycle SEAM where `cmd_step`'s
/// terminal sweep hands off to the conductor - the first code on the step path that adds a unit
/// worktree - so a crash mid-either can never leave a half-removed `.git/worktrees` admin entry
/// that a later `git worktree add` trips over (the permanent wedge spec 51 hardens), and a future
/// edit cannot silently reorder the sweep after an add or lift either mutation out from under the
/// one-step-at-a-time lock.
///
/// It is a SOURCE-TEXT assertion, not a behavior test: the terminal sweep
/// (`worktree::sweep_terminal`) is called only from `cmd_step` in the BINARY (`main.rs`), whose
/// step-lifecycle body is not reachable through the crate API, and the "add" side is deep inside
/// `conductor::run` (`stage_worktree` / `review_only_worktree` -> `Worktree::create`). So we assert
/// on the file's bytes - the convention the sibling `main_exit_path_is_honestly_documented` fixture
/// uses - at a bar a no-op cannot pass: the three load-bearing calls exist in `cmd_step` and appear
/// in the order lock -> sweep -> add.
#[test]
fn worktree_sweep_completes_before_any_add_within_one_step() {
    let src = main_rs_source();

    // Isolate cmd_step's body (its declaration up to the next top-level `fn`) so the ordering
    // assertions stay pointed at the step lifecycle and are immune to the OTHER
    // `conductor::run(&cfg, &deps)` call sites elsewhere in main.rs (`cmd_run` and the workflow
    // entry), which are not the stepwise seam under test.
    let step_at = src
        .find("fn cmd_step(args: &[String]) -> Res {")
        .expect("main.rs must still define cmd_step, the `rigger step` lifecycle");
    let step_end = src[step_at..]
        .find("\nfn ")
        .map(|off| step_at + off)
        .expect("cmd_step must be followed by another top-level fn");
    let cmd_step = &src[step_at..step_end];

    // The three load-bearing worktree-lifecycle anchors, by BYTE OFFSET within cmd_step:
    //   (1) the step lock - the serialization that makes "one step at a time" hold, so the sweep
    //       and the add can never interleave with another step's worktree mutations;
    //   (2) the terminal-worktree sweep (`worktree::sweep_terminal`) - the REMOVE half;
    //   (3) the conductor run - the first code on the step path that ADDS a unit worktree.
    let lock_at = cmd_step
        .find("acquire_step_lock(")
        .expect("cmd_step must acquire the step lock (the worktree-mutation serialization)");
    let sweep_at = cmd_step
        .find("worktree::sweep_terminal(")
        .expect("cmd_step must sweep the terminal worktrees before driving the conductor");
    let add_at = cmd_step
        .find("conductor::run(&cfg, &deps)")
        .expect("cmd_step must drive the conductor, which adds the unit worktrees");

    // The sweep is the ONLY terminal sweep on the step path: a second, out-of-order sweep must not
    // be introducible without this pin noticing.
    assert_eq!(
        cmd_step.matches("worktree::sweep_terminal(").count(),
        1,
        "cmd_step must call the terminal sweep exactly once, at the lifecycle seam"
    );

    // SERIALIZATION: both mutations happen AFTER the step lock is taken, so no worktree sweep or
    // add runs outside the one-step-at-a-time guard.
    assert!(
        lock_at < sweep_at && lock_at < add_at,
        "the terminal sweep and the conductor's worktree adds must both run UNDER the step lock \
         (acquired first), so worktree mutations are serialized one step at a time"
    );

    // ORDERING: the sweep completes before the conductor begins adding worktrees - the whole of
    // criterion 5. Were the conductor driven first (or the sweep moved below it), a terminal
    // worktree's removal would race the next unit's add on the shared `.git/worktrees` admin area,
    // which is exactly the corruption spec 51 forbids.
    assert!(
        sweep_at < add_at,
        "the terminal-worktree sweep must complete BEFORE the conductor adds any unit worktree; \
         found the conductor run at offset {add_at} before the sweep at {sweep_at} within cmd_step"
    );
}

/// Spec 64, criterion 4 (`src/worktree.rs::sweep_terminal` learns liveness; its caller
/// `cmd_step` in `src/main.rs` now folds `current_run_units(events).live_branches` and
/// hands it in): the step-start terminal-worktree sweep must SPARE a unit that is still
/// LIVE in the current run even though its branch tip is - trivially - an ancestor of the
/// run branch (the empty-diff shape: nothing has been committed into the unit branch
/// yet), while it must still RECLAIM an unrelated worktree in the IDENTICAL git shape
/// whose branch belongs to no live unit of this run. The merged-only ancestry rule the
/// sweep used before this criterion cannot tell these two apart on its own - both pass
/// `merge-base --is-ancestor`.
///
/// Drives the REAL compiled binary (`rigger step`) twice with no courier result recorded
/// for the unit in between, so the very seam this criterion changed (`cmd_step`'s
/// step-start sweep call) runs a second time while the unit is still parked, waiting on
/// its implementer's result. An UNCOMMITTED canary file written into each worktree after
/// step 1 is the differentiator a bare "the dir still exists" check cannot give:
/// `stage_worktree`'s own adopt-or-create machinery (`Worktree::create`) also runs at the
/// top of every step for every in-flight unit and would silently RE-CREATE a
/// wrongly-swept worktree from the unit branch's last COMMIT before this test's own
/// assertions ever ran - masking exactly the bug this test exists to catch. A freshly
/// re-created checkout carries no uncommitted file, so the canary surviving is proof the
/// worktree was never removed at all, not proof it was removed and then rebuilt.
#[test]
fn step_start_sweep_spares_a_live_units_empty_diff_worktree_but_reclaims_a_dead_ancestor_leftover()
{
    let dir = temp_git_project_with_commit();
    let root = dir.path();
    write_reviewless_git_unit_workflow(root);

    let scratch = root.join("scratchroot");
    let tmp = scratch.to_str().unwrap();

    // Step 1: the "solo" unit's implementer parks - a real, git-backed unit worktree is
    // created now, checked out on `rigger/u/solo` at whatever `rigger-run` currently
    // points to. Nothing has landed yet, so the branch tip trivially equals the run tip -
    // the empty-diff shape this criterion targets.
    let (out, err, ok) = run_rigger_envs(root, &["step"], &[("RIGGER_TMPDIR", tmp)]);
    assert!(ok, "the first step must succeed; stderr:\n{err}");
    assert!(
        out.contains(r#""id":"solo/implementer#0""#) && out.contains(r#""done":false"#),
        "step 1 must park the implementer; got: {out:?}"
    );

    let live_wt = scratch.join("rigger-wt-solo");
    assert!(
        live_wt.exists(),
        "premise: a parked implementer must already have its unit worktree on disk: {}",
        live_wt.display()
    );
    let live_canary = live_wt.join("canary-live.txt");
    std::fs::write(&live_canary, "spared\n").unwrap();

    // Plant an UNRELATED worktree in the exact same git shape: a branch whose tip is
    // (trivially) an ancestor of `rigger-run`, registered under the same scratch root -
    // but that belongs to NO unit this run ever started. From `sweep_terminal`'s own
    // git-only view this is indistinguishable from the live unit's worktree above except
    // for the one thing this criterion adds: it is absent from the current run's
    // `live_branches`. Stands in for a crashed process's leftover registration without
    // needing to actually kill a subprocess mid-flight to construct one.
    let dead_wt = scratch.join("rigger-wt-leftover-orphan");
    git_ok(
        root,
        &[
            "worktree",
            "add",
            dead_wt.to_str().unwrap(),
            "-b",
            "rigger/u/leftover-orphan",
            "rigger-run",
        ],
    );
    let dead_canary = dead_wt.join("canary-dead.txt");
    std::fs::write(&dead_canary, "reclaimed\n").unwrap();

    // Step 2: no courier result was recorded for "solo/implementer#0", so it is still the
    // very same outstanding spawn - and the run's `current_run_units` fold still reads it
    // as LIVE. This step's OWN step-start sweep is the one under test.
    let (out, err, ok) = run_rigger_envs(root, &["step"], &[("RIGGER_TMPDIR", tmp)]);
    assert!(ok, "the second step must succeed; stderr:\n{err}");
    assert!(
        out.contains(r#""id":"solo/implementer#0""#) && out.contains(r#""done":false"#),
        "step 2 must still be waiting on the very same outstanding implementer spawn, or \
         this test proves nothing about the step-start sweep seeing it live; got: {out:?}"
    );

    // The LIVE unit's worktree survives - the canary proves it was never removed, not
    // merely that a fresh one now happens to exist at the same path.
    assert!(
        live_canary.exists(),
        "the step-start sweep must SPARE a live unit's worktree even though its branch \
         tip is an ancestor of the run branch (the empty-diff shape); the canary is gone, \
         so it was removed (and possibly silently rebuilt) despite the unit still being \
         live; stderr:\n{err}"
    );

    // The UNRELATED, not-live worktree in the identical git shape is reclaimed.
    assert!(
        !dead_wt.exists(),
        "the step-start sweep must still reclaim a worktree whose branch belongs to no \
         live unit of this run, even in the identical empty-diff shape as the live one \
         above; it survived under {}\nstderr:\n{err}",
        dead_wt.display()
    );
    let list = git_out(root, &["worktree", "list", "--porcelain"]).unwrap_or_default();
    assert!(
        !list.contains("rigger/u/leftover-orphan"),
        "the reclaimed worktree must also be DEREGISTERED from git, not just directory-\
         deleted: {list}"
    );
    assert!(
        list.contains("rigger/u/solo"),
        "the live unit's worktree must still be registered with git: {list}"
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

/// Scaffold a project with ONE standalone review-only stage (`agents: [lens]`, no
/// singular `agent`) - the `is_fan_out` shape `run_fan_out_stage` drives, the call site
/// spec 64 criterion 1 closes the review-worktree park-teardown gap for. No `needs`, so
/// it is ready in the very first wave. Deliberately carries NO `isolation: none`: a
/// standalone review's throwaway worktree is minted unconditionally whenever a repo is
/// configured (`review_only_worktree` checks only `self.deps.repo`, never the lens
/// agent's own isolation setting), so the default (real, git-backed) isolation this
/// test needs is just the field's absence.
fn write_standalone_review_workflow(root: &Path) {
    let rigger = root.join(".rigger");
    std::fs::create_dir_all(rigger.join("agents")).unwrap();
    std::fs::write(
        rigger.join("agents").join("lens.md"),
        "---\nid: lens\nmodel: sonnet\ntools: [Read]\n---\nReview it.\n",
    )
    .unwrap();
    std::fs::write(
        rigger.join("workflow.yml"),
        r#"name: reviewparktest
defaults:
  grounder: nop
  budget: 60
stages:
  review:
    agents: [lens]
"#,
    )
    .unwrap();
}

/// Spec 64, criterion 1 (the review-worktree half of the split this unit OWNS): a
/// standalone review stage's lens spawn PARKS - `rigger step` never runs an agent
/// in-process, it is by construction the parked/stepwise driver - and the stage must
/// KEEP its throwaway `rigger-review-*` worktree and branch on disk, not tear them
/// down as `run_fan_out_stage` did unconditionally before this criterion's fix.
///
/// This is the TRUE PERIPHERY of that guarantee, not a restatement of it: the
/// implementer's own tests (`conductor.rs`'s `mod tests`) call the library's `run()`
/// directly, in ONE test process, against an internal `Stub` driver - they can prove
/// the internal state machine keeps the worktree alive across the function call, but
/// they cannot observe whether the guarantee holds at the boundary the spec exists
/// for: "the parked agent runs BETWEEN conductor processes." This test drives the
/// actual COMPILED BINARY as a real subprocess against a real git repo and inspects
/// the result the only way an out-of-process agent (or an operator) could: reading
/// the filesystem and asking `git` what is registered - nothing in a live Rust call
/// stack is silently keeping state alive across the process boundary here. It is also
/// the first CLI-level park test in this file to use REAL isolation at all: every
/// other `rigger step` park scenario here deliberately sets `isolation: none` to stay
/// offline and worktree-free, so none of them could have caught a regression here.
#[test]
fn step_parks_a_standalone_review_spawn_and_keeps_its_review_worktree() {
    let dir = temp_git_project_with_commit();
    let root = dir.path();
    write_standalone_review_workflow(root);

    let (out, err, ok) = run_rigger(root, &["step"]);
    assert!(ok, "step must succeed; stderr: {err}");
    let line = out.trim();
    assert!(
        line.contains(r#""id":"review/lens:lens#0""#),
        "the lens spawn must actually have parked in the wave, or this test proves \
         nothing about a park; got: {line:?}"
    );
    assert!(
        line.contains(r#""done":false"#),
        "a spawn still awaiting a result is not a done run; got: {line:?}"
    );

    // The deterministic review-worktree naming (`conductor.rs`'s `review_worktree_dir`
    // / `review_branch`, spec 06): `<scratch-root>/rigger-review-<stage>-<attempt>` on
    // branch `rigger/review/<stage>-<attempt>`, scratch root defaulting to
    // `<repo>/.rigger/tmp` (no `RIGGER_TMPDIR` set, no `defaults.workdir` configured).
    let wt_dir = root
        .join(".rigger")
        .join("tmp")
        .join("rigger-review-review-0");
    assert!(
        wt_dir.exists(),
        "a parked standalone-review stage must KEEP its throwaway review worktree on \
         disk: {}",
        wt_dir.display()
    );

    // Registered with git, not just a leftover directory: a bare surviving dir whose
    // `.git/worktrees` admin entry is gone is exactly the half-fixed state this
    // criterion rules out (the SAME failure class `worktree_registered_on` guards
    // against in the implementer's in-process tests, checked here from outside).
    let list = git_out(root, &["worktree", "list", "--porcelain"])
        .expect("git worktree list must succeed in the seeded repo");
    assert!(
        list.contains("rigger/review/review-0"),
        "the kept review worktree must stay REGISTERED with git, checked out on its \
         throwaway branch: {list}"
    );
}

/// Spec 64, criterion 1 rounds 2-3 (adv-u1c1-r2-park-swap-swallows-concurrent-genuine-error):
/// the concurrent-chunk masking defect the round-2/round-3 fixes close in
/// `run_review_agents_concurrently`. The single-lens test above cannot see this at all - it has
/// no sibling to race against. The implementer's own regression
/// (`a_parked_lens_keeps_the_review_worktree_beside_a_sibling_degenerate_halt`, `conductor.rs`'s
/// `mod tests`) proves the fix's ALGORITHM correct - which of two `Result`s collected into a
/// `Vec` wins - with a synchronous, single-process `Stub` driver that decides the outcome from a
/// fixed id set before `run()` is even called. It can never exercise the mechanism this
/// criterion actually gates: [`rigger::driver::replay::ReplayDriver`] parks a spawn as a durable
/// event, then REPLAYS it only once a LATER, SEPARATE process finds its result already in the
/// log - so a real mixed chunk (one sibling genuinely dead, another still open) can only arise
/// from an out-of-process courier resolving one before the other BETWEEN conductor invocations.
///
/// A single `rigger result --error` cannot reproduce a GENUINE terminal error here by itself: a
/// replayed error on a review-tier spawn is spec 51's territory first - `run_reviewer` reads it
/// as an infrastructure fault (an externally-killed reviewer, not a verdict) and RE-PARKS a
/// fresh attempt rather than propagating it, bounded by `REVIEWER_RESPAWN_BOUND`. Exhausting
/// every one of a lens's spawns this way converges on the SAME halt as an exhausted
/// degenerate-reviewer (Gap 18) - `run_wave`'s dedicated `is_degenerate_reviewer` arm, the
/// second genuine-error shape round 3 names - which is exactly the boundary condition this test
/// drives to, deterministically, through the real binary: a courier that posts nothing but
/// failures for lens "a"'s spawn and every one of its respawns, while lens "b" is left
/// unanswered throughout and so re-parks in the SAME concurrent chunk on every step, including
/// the final one that halts. Two requirements from that one real halt: it must surface LOUDLY
/// (non-zero exit, the dead reviewer named on stderr) - round 2's swap-to-front bug could mask a
/// co-chunked genuine error behind a sibling's park - AND the shared review worktree must
/// survive anyway - round 1's original bug tore it down whenever the propagated result was not
/// itself a park.
#[test]
fn step_halts_on_an_exhausted_reviewer_beside_a_parked_sibling_and_keeps_the_review_worktree() {
    let dir = temp_git_project_with_commit();
    let root = dir.path();
    let rigger = root.join(".rigger");
    std::fs::create_dir_all(rigger.join("agents")).unwrap();
    for id in ["a", "b"] {
        std::fs::write(
            rigger.join("agents").join(format!("{id}.md")),
            format!("---\nid: {id}\nmodel: sonnet\ntools: [Read]\n---\nReview it.\n"),
        )
        .unwrap();
    }
    std::fs::write(
        rigger.join("workflow.yml"),
        r#"name: reviewracetest
defaults:
  grounder: nop
  budget: 60
stages:
  review:
    agents: [a, b]
"#,
    )
    .unwrap();

    // Step 1: neither lens has a recorded result yet, so the concurrent chunk parks BOTH -
    // the real fan-out shape a two-lens review panel takes on nearly every unit.
    let (out1, err1, ok1) = run_rigger(root, &["step"]);
    assert!(ok1, "the first step must succeed; stderr: {err1}");
    assert!(
        out1.contains(r#""id":"review/lens:a#0""#) && out1.contains(r#""id":"review/lens:b#0""#),
        "both lenses must actually have parked in the same wave, or this test proves nothing \
         about a mixed chunk; got: {out1:?}"
    );

    // An out-of-process courier posts nothing but failures for "a" - its original spawn, then
    // each deterministic `~retryN` respawn `run_reviewer` re-parks in turn (spec 51's
    // reviewer-error-re-park) - while "b" is left unanswered throughout. `spawn_retry_id`'s
    // exact naming (retry 0 is the plain id, retry N>0 appends `~retryN`) is asserted by
    // `rigger::spawn`'s own doc tests; this loop reconstructs it rather than hardcoding the
    // respawn bound, so it tracks whatever that bound is. Capped well above any real bound so a
    // genuine infinite-repark regression fails LOUDLY here instead of hanging the suite.
    let mut ok_step = true;
    let mut last_err = String::new();
    let mut retry = 0u32;
    while ok_step {
        assert!(
            retry <= 8,
            "the reviewer-error-repark loop did not halt within a sane number of rounds - \
             either the respawn bound regressed or this test's premise is wrong"
        );
        let id = if retry == 0 {
            "review/lens:a#0".to_string()
        } else {
            format!("review/lens:a#0~retry{retry}")
        };
        let (_o, err_r, ok_r) = run_rigger(
            root,
            &["result", &id, "boundary-genuine-crash-marker", "--error"],
        );
        assert!(
            ok_r,
            "recording a failure for {id:?} must succeed; stderr: {err_r}"
        );
        let (_out, err_s, ok_s) = run_rigger(root, &["step"]);
        ok_step = ok_s;
        last_err = err_s;
        retry += 1;
    }

    // The exhausted reviewer must halt LOUDLY, naming the dead reviewer - not be silently
    // masked by "b"'s park in the same final concurrent chunk (the round-2 defect).
    assert!(
        last_err.contains("\"review\"") && last_err.contains("\"a\"") && last_err.contains("lens"),
        "the halt must name the exhausted reviewer (stage, tier, agent): {last_err}"
    );

    // The shared review worktree survives anyway - "b" genuinely parked in this same final
    // chunk and resumes in exactly this tree from a later conductor process.
    let wt_dir = root
        .join(".rigger")
        .join("tmp")
        .join("rigger-review-review-0");
    assert!(
        wt_dir.exists(),
        "a PARKED sibling must keep the shared review worktree even when a co-chunked lens \
         exhausted into a loud halt, across a REAL process boundary: {}",
        wt_dir.display()
    );
    let list = git_out(root, &["worktree", "list", "--porcelain"])
        .expect("git worktree list must succeed in the seeded repo");
    assert!(
        list.contains("rigger/review/review-0"),
        "the kept review worktree must stay REGISTERED with git: {list}"
    );
}

/// Scaffold a project with ONE full unit stage - `agent: worker` (so a REAL, gated
/// implementer runs and its OWN durable worktree is created), gated by a trivial inline
/// `ok` gate, then reviewed by TWO lenses in its per-unit review panel. Unlike
/// [`write_standalone_review_workflow`] (no `agent:`, `run_fan_out_stage`'s throwaway
/// `rigger-review-*` worktree), this is the OTHER worktree kind spec 64 criterion 1
/// covers: the unit's own durable `rigger-wt-<unit>` worktree on its `rigger/u/<unit>`
/// branch, torn down (or not) by `run_stage`'s `parked_unwind` gate rather than
/// `run_fan_out_stage`'s. No `isolation: none` on any agent, for the same reason
/// [`write_standalone_review_workflow`] omits it: this test needs the real, git-backed
/// worktree the round-4 regression tears down, not the offline no-worktree shape most
/// other `step` fixtures deliberately choose.
fn write_unit_review_lenses_workflow(root: &Path) {
    let rigger = root.join(".rigger");
    std::fs::create_dir_all(rigger.join("agents")).unwrap();
    std::fs::write(
        rigger.join("agents").join("worker.md"),
        "---\nid: worker\nmodel: sonnet\ntools: [Read, Edit]\n---\nDo the unit.\n",
    )
    .unwrap();
    for id in ["a", "b"] {
        std::fs::write(
            rigger.join("agents").join(format!("{id}.md")),
            format!("---\nid: {id}\nmodel: sonnet\ntools: [Read]\n---\nReview it.\n"),
        )
        .unwrap();
    }
    std::fs::write(
        rigger.join("workflow.yml"),
        r#"name: unitworktreeparktest
defaults:
  grounder: nop
  budget: 60
gates:
  ok: { run: "true", kind: core }
stages:
  solo:
    agent: worker
    gates: [ok]
    on_pass: none
    review:
      lenses: [a, b]
"#,
    )
    .unwrap();
}

/// Spec 64, criterion 1, round 4 (sdet-u1c1-r3-unit-worktree-torn-down-beside-genuine-park,
/// upheld live by adv2-u1c1-r4-severe-finding-upheld-live): the MIRROR IMAGE, at the real
/// binary boundary, of `step_halts_on_an_exhausted_reviewer_beside_a_parked_sibling_and_
/// keeps_the_review_worktree` above - for the UNIT's own durable worktree rather than a
/// standalone review's throwaway one.
///
/// Round 3 wired `run_review_agents_concurrently`'s out-of-band `any_parked` signal through
/// to `run_fan_out_stage` (the review-worktree call site) but not through `review_unit` to
/// `run_stage`'s own pre-existing `parked_unwind` gate (the unit-worktree call site) -
/// `review_unit` passed the shared function a throwaway `AtomicBool` it never read, so
/// `run_stage`'s `parked_unwind` derived solely from the single `Result`
/// `run_single_stage` propagated. Round 3's own swap (prioritize a genuine terminal error
/// over a co-chunked park, so the error is what a caller's `?` propagates) then had a side
/// effect nobody had measured on THIS path: a lens that genuinely crashes correctly
/// propagates its error, but the UNIT's own worktree was torn down out from under a
/// SIBLING lens that is genuinely parked and will resume in that exact worktree from a
/// later conductor process - the identical defect class spec 64 c1 exists to close, just
/// recurring on the other worktree kind.
///
/// This is the TRUE PERIPHERY of round 4's fix, not a restatement of it: the implementer's
/// own regression (`a_parked_lens_keeps_the_unit_worktree_beside_a_genuine_sibling_crash`,
/// `conductor.rs`'s `mod tests`) proves the fix's ALGORITHM correct with a synchronous,
/// single-process `Stub` driver that decides both lenses' outcomes from a fixed id set
/// before `run()` is even called. It cannot exercise the mechanism this criterion actually
/// gates: [`rigger::driver::replay::ReplayDriver`] parks a spawn as a durable event, then
/// REPLAYS it only once a LATER, SEPARATE process finds its result already in the log - so
/// a real mixed chunk (one sibling genuinely dead, another still open) can only arise from
/// an out-of-process courier resolving one before the other BETWEEN conductor invocations.
/// It also drives a REAL implementer through a REAL gate first, so the worktree under test
/// is the unit's own durable checkpoint (`rigger-wt-solo` on `rigger/u/solo`), not a
/// synthetic one a Stub driver never actually created on disk.
///
/// Lens "a" PARKS and is left unanswered every step (re-parking each time); lens "b" is
/// exhausted via the same reviewer-error re-park mechanism (spec 51) round 2's test uses -
/// a plain recorded error re-parks a fresh `~retryN` attempt (bounded by
/// `REVIEWER_RESPAWN_BOUND`) until it converges on a genuine `is_degenerate_reviewer` halt.
/// Two requirements from that one real halt: "b"'s exhaustion must surface LOUDLY (spec
/// 19c), not be masked by "a"'s park in the same final concurrent chunk; and the UNIT's
/// OWN worktree must survive anyway, because "a" genuinely parked and will resume in
/// exactly this tree from a later conductor process.
#[test]
fn step_halts_on_an_exhausted_lens_beside_a_parked_sibling_and_keeps_the_unit_worktree() {
    let dir = temp_git_project_with_commit();
    let root = dir.path();
    write_unit_review_lenses_workflow(root);

    // Step 1: the unit is ready, so its implementer spawn parks - the real, git-backed
    // isolation this fixture deliberately keeps means the unit's own durable worktree is
    // created right here, before the implementer has even produced a diff.
    let (out, err, ok) = run_rigger(root, &["step"]);
    assert!(ok, "the first step must succeed; stderr: {err}");
    assert!(
        out.contains(r#""id":"solo/implementer#0""#) && out.contains(r#""done":false"#),
        "step 1 parks the implementer; got: {out:?}"
    );

    let wt_dir = root.join(".rigger").join("tmp").join("rigger-wt-solo");
    assert!(
        wt_dir.exists(),
        "a parked implementer must already have its unit worktree on disk (round 1's own \
         criterion, load-bearing for the rest of this test): {}",
        wt_dir.display()
    );

    // Write the implementer's "diff" directly into the worktree it was already handed -
    // the shape a real out-of-process agent takes (it edits files in its assigned tree,
    // then reports done), never a CLI-supplied patch.
    std::fs::write(wt_dir.join("work.rs"), "pub fn work() {}\n").unwrap();
    let (_o, err, ok) = run_rigger(
        root,
        &["result", "solo/implementer#0", "implemented the unit"],
    );
    assert!(
        ok,
        "recording the implementer result must succeed; stderr: {err}"
    );

    // Step 2: the implementer replays, the pre-gate commit lands the written file, the `ok`
    // gate passes, and the per-unit review parks BOTH lenses in one concurrent chunk - the
    // real fan-out shape a two-lens panel takes on nearly every unit.
    let (out, err, ok) = run_rigger(root, &["step"]);
    assert!(ok, "the second step must succeed; stderr: {err}");
    assert!(
        out.contains(r#""id":"solo/lens:a#0""#) && out.contains(r#""id":"solo/lens:b#0""#),
        "both lenses must actually have parked in the same wave, or this test proves nothing \
         about a mixed chunk; got: {out:?}"
    );

    // An out-of-process courier posts nothing but failures for "b" - its original spawn,
    // then each deterministic `~retryN` respawn `run_reviewer` re-parks in turn (spec 51's
    // reviewer-error-re-park) - while "a" is left unanswered throughout, so it re-parks in
    // the same chunk on every step including the final one that halts. Capped well above
    // any real bound so a genuine infinite-repark regression fails LOUDLY here instead of
    // hanging the suite.
    let mut ok_step = true;
    let mut last_err = String::new();
    let mut retry = 0u32;
    while ok_step {
        assert!(
            retry <= 8,
            "the reviewer-error-repark loop did not halt within a sane number of rounds - \
             either the respawn bound regressed or this test's premise is wrong"
        );
        let id = if retry == 0 {
            "solo/lens:b#0".to_string()
        } else {
            format!("solo/lens:b#0~retry{retry}")
        };
        let (_o, err_r, ok_r) = run_rigger(
            root,
            &["result", &id, "boundary-genuine-crash-marker", "--error"],
        );
        assert!(
            ok_r,
            "recording a failure for {id:?} must succeed; stderr: {err_r}"
        );
        let (_out, err_s, ok_s) = run_rigger(root, &["step"]);
        ok_step = ok_s;
        last_err = err_s;
        retry += 1;
    }

    // The exhausted lens must halt LOUDLY, naming the dead reviewer - not be silently
    // masked by "a"'s park in the same final concurrent chunk (the round-2 defect class,
    // now proven closed on the unit-worktree call site round 4 fixes).
    assert!(
        last_err.contains("\"solo\"") && last_err.contains("\"b\"") && last_err.contains("lens"),
        "the halt must name the exhausted reviewer (stage, tier, agent): {last_err}"
    );

    // The UNIT's OWN worktree survives anyway - "a" genuinely parked in this same final
    // chunk and resumes in exactly this tree from a later conductor process. This is the
    // round-4 regression: pre-round-4, "b"'s genuine halt correctly propagated but tore
    // this SAME worktree down out from under "a"'s park.
    assert!(
        wt_dir.exists(),
        "a PARKED sibling must keep the UNIT's own worktree even when a co-chunked lens \
         exhausted into a loud halt, across a REAL process boundary: {}",
        wt_dir.display()
    );
    let list = git_out(root, &["worktree", "list", "--porcelain"])
        .expect("git worktree list must succeed in the seeded repo");
    assert!(
        list.contains("rigger/u/solo"),
        "the kept unit worktree must stay REGISTERED with git, checked out on its durable \
         unit branch: {list}"
    );
}

/// Scaffold a project with ONE full unit stage - `agent: worker`, gated by a trivial
/// always-passing inline gate, `on_pass: merge`, and NO review panel at all
/// (`review.is_empty()`, "the historical implement-then-integrate behavior" `ReviewPanel`
/// itself documents) - the minimal shape that reaches a genuine `Ok(true)` integrate. Real
/// git isolation (no `isolation: none`, same reasoning as
/// [`write_unit_review_lenses_workflow`]) so the unit's own durable `rigger-wt-<unit>`
/// worktree on `rigger/u/<unit>` is the SAME checkpoint kind `run_stage`'s
/// terminal-teardown gate covers.
fn write_reviewless_git_unit_workflow(root: &Path) {
    let rigger = root.join(".rigger");
    std::fs::create_dir_all(rigger.join("agents")).unwrap();
    std::fs::write(
        rigger.join("agents").join("worker.md"),
        "---\nid: worker\nmodel: sonnet\ntools: [Read, Edit]\n---\nDo the unit.\n",
    )
    .unwrap();
    std::fs::write(
        rigger.join("workflow.yml"),
        r#"name: terminalintegratetest
defaults:
  grounder: nop
  budget: 60
gates:
  ok: { run: "true", kind: core }
stages:
  solo:
    agent: worker
    gates: [ok]
    on_pass: merge
"#,
    )
    .unwrap();
}

/// The escalating twin of [`write_reviewless_git_unit_workflow`]: identical shape, but
/// `defaults.max_retries: 1` means `safety::remediate(0, 1)` escalates on the FIRST failed
/// attempt (`bounded_then_escalates` in `src/safety.rs` pins that arithmetic), so a single
/// crashed implementer spawn - never a park - is enough to drive the unit terminal without
/// ever integrating.
fn write_reviewless_git_escalating_unit_workflow(root: &Path) {
    let rigger = root.join(".rigger");
    std::fs::create_dir_all(rigger.join("agents")).unwrap();
    std::fs::write(
        rigger.join("agents").join("worker.md"),
        "---\nid: worker\nmodel: sonnet\ntools: [Read, Edit]\n---\nDo the unit.\n",
    )
    .unwrap();
    std::fs::write(
        rigger.join("workflow.yml"),
        r#"name: terminalescalatetest
defaults:
  grounder: nop
  budget: 60
  max_retries: 1
gates:
  ok: { run: "true", kind: core }
stages:
  solo:
    agent: worker
    gates: [ok]
    on_pass: merge
"#,
    )
    .unwrap();
}

/// Spec 64, criterion 2 (TERMINAL TEARDOWN IS UNCHANGED), the "in both drivers" half no
/// existing test measures at the real binary boundary. The implementer's own regression
/// (`a_units_worktree_cache_and_branch_are_all_reclaimed_on_a_successful_integrate`,
/// `conductor.rs`'s `mod tests`) proves the guarantee with a synchronous, single-process
/// `Stub` driver whose spawn NEVER parks - `any_parked` starts false and stays false for the
/// whole call, so that test cannot exercise the shape every REAL unit actually takes:
/// [`rigger::driver::replay::ReplayDriver`] parks the implementer as a durable event on the
/// FIRST `rigger step`, and only REPLAYS its result (posted by an out-of-process courier,
/// here `rigger result`) on a LATER, SEPARATE process. This drives that real two-process
/// lifecycle - park, then terminate - through the compiled binary against a real git repo,
/// and inspects the result the only way an out-of-process operator could: the filesystem
/// and `git`, never an internal helper.
#[test]
fn step_reclaims_the_units_worktree_and_deletes_its_branch_on_a_clean_integrate() {
    let dir = temp_git_project_with_commit();
    let root = dir.path();
    write_reviewless_git_unit_workflow(root);

    // Step 1: the unit is ready, so its implementer parks - the real, git-backed isolation
    // means the unit's own durable worktree is created right here, before the implementer
    // has produced any diff.
    let (out, err, ok) = run_rigger(root, &["step"]);
    assert!(ok, "the first step must succeed; stderr: {err}");
    assert!(
        out.contains(r#""id":"solo/implementer#0""#) && out.contains(r#""done":false"#),
        "step 1 parks the implementer; got: {out:?}"
    );

    let wt_dir = root.join(".rigger").join("tmp").join("rigger-wt-solo");
    assert!(
        wt_dir.exists(),
        "a parked implementer must already have its unit worktree on disk (load-bearing for \
         the rest of this test): {}",
        wt_dir.display()
    );

    // Write the implementer's "diff" directly into the worktree it was already handed - the
    // shape a real out-of-process agent takes - then record its result.
    std::fs::write(wt_dir.join("work.rs"), "pub fn work() {}\n").unwrap();
    let (_o, err, ok) = run_rigger(
        root,
        &["result", "solo/implementer#0", "implemented the unit"],
    );
    assert!(
        ok,
        "recording the implementer result must succeed; stderr: {err}"
    );

    // Step 2: the implementer replays, the pre-gate commit lands the written file, the `ok`
    // gate passes inline, there is no review panel to run, and with `on_pass: merge` the
    // stage reaches a genuine TERMINAL `Ok(true)` in THIS process - through the real replay
    // driver, across the process boundary step 1 opened.
    let (out, err, ok) = run_rigger(root, &["step"]);
    assert!(ok, "the second step must succeed; stderr: {err}");
    assert!(
        out.contains(r#""done":true"#),
        "every spawn now has a result and the stage integrates, so the run converges; got: {out:?}"
    );
    assert!(
        !out.contains("escalated"),
        "a clean integrate must not carry an escalated unit; got: {out:?}"
    );

    // The unit's own worktree is gone...
    assert!(
        !wt_dir.exists(),
        "the unit's worktree must be reclaimed after a clean integrate: {}",
        wt_dir.display()
    );
    let list = git_out(root, &["worktree", "list", "--porcelain"]).unwrap_or_default();
    assert!(
        !list.contains("rigger-wt-solo"),
        "the reclaimed worktree must be fully DEREGISTERED with git, not a stale admin entry \
         pointing at a removed dir: {list}"
    );

    // ...and its durable branch is DELETED - the checkpoint already served its purpose (the
    // merged work now lives on the run branch) - the one half of "identical to today's
    // behavior" the implementer's own tests, calling `run()` directly in one process, never
    // observe.
    assert!(
        git_out(
            root,
            &["rev-parse", "--verify", "-q", "refs/heads/rigger/u/solo"]
        )
        .is_none(),
        "a successfully integrated unit's branch must be deleted, not left on disk"
    );
}

/// Spec 64, criterion 2's other half at the real binary boundary - the mirror image of
/// [`step_reclaims_the_units_worktree_and_deletes_its_branch_on_a_clean_integrate`] above,
/// and of the implementer's own
/// `a_units_worktree_is_reclaimed_but_its_branch_survives_a_terminal_escalation`
/// (`conductor.rs`'s `mod tests`), which proves the same guarantee with a synchronous,
/// single-process `Stub` driver that never parks at all. Here the implementer's crash is a
/// REPLAYED `SpawnResult` a LATER process reads back (never a park), so remediation exhausts
/// into `UnitEscalated` only once this second real process folds it - the shape the
/// implementer's own comment notes no existing test could drive through a real repo. Only a
/// successful `Ok(true)` integrate deletes the branch (`run_stage`, `src/conductor.rs`), and
/// this path never reaches one, so the worktree must still be reclaimed while the branch
/// survives as the human's evidence.
#[test]
fn step_reclaims_the_units_worktree_but_keeps_its_branch_on_a_terminal_escalation() {
    let dir = temp_git_project_with_commit();
    let root = dir.path();
    write_reviewless_git_escalating_unit_workflow(root);

    let (out, err, ok) = run_rigger(root, &["step"]);
    assert!(ok, "the first step must succeed; stderr: {err}");
    assert!(
        out.contains(r#""id":"solo/implementer#0""#) && out.contains(r#""done":false"#),
        "step 1 parks the implementer; got: {out:?}"
    );

    let wt_dir = root.join(".rigger").join("tmp").join("rigger-wt-solo");
    assert!(
        wt_dir.exists(),
        "a parked implementer must already have its unit worktree on disk (load-bearing for \
         the rest of this test): {}",
        wt_dir.display()
    );

    // The out-of-process courier reports a genuine crash, never a verdict.
    let (_o, err, ok) = run_rigger(
        root,
        &[
            "result",
            "solo/implementer#0",
            "boundary-genuine-crash-marker",
            "--error",
        ],
    );
    assert!(ok, "recording the crash must succeed; stderr: {err}");

    // Step 2: the crash replays, `max_retries: 1` escalates on this FIRST failed attempt (no
    // second implementer spawn), and the run reaches a fixpoint AROUND the escalated unit -
    // terminal, never parked.
    let (out, err, ok) = run_rigger(root, &["step"]);
    assert!(
        ok,
        "an escalation-fixpoint step still exits 0; stderr: {err}"
    );
    assert!(
        out.contains(r#""done":true"#) && out.contains(r#""escalated":["solo"]"#),
        "the crash must exhaust remediation into an escalated fixpoint; got: {out:?}"
    );

    // The unit's worktree is reclaimed exactly as a clean integrate's...
    assert!(
        !wt_dir.exists(),
        "the unit's worktree must be reclaimed after a terminal escalation: {}",
        wt_dir.display()
    );
    let list = git_out(root, &["worktree", "list", "--porcelain"]).unwrap_or_default();
    assert!(
        !list.contains("rigger-wt-solo"),
        "the reclaimed worktree must be fully DEREGISTERED with git, not a stale admin entry \
         pointing at a removed dir: {list}"
    );

    // ...but its branch SURVIVES as the human's evidence - only a successful integrate
    // deletes it, and this unit never integrated.
    assert!(
        git_out(
            root,
            &["rev-parse", "--verify", "-q", "refs/heads/rigger/u/solo"]
        )
        .is_some(),
        "an escalated unit's branch must be RETAINED, not deleted alongside its worktree"
    );
}

/// Spec 64, criterion 3 (ensure-on-park, defense in depth: `Worktree::ensure_present` in
/// `src/worktree.rs`, called from `run_single_stage` in `src/conductor.rs` immediately
/// before `review_unit`): the conductor's next hand-off restores a unit worktree an
/// out-of-band actor deleted, before the review tier's agent consumes it.
///
/// The implementer's own regression
/// (`review_unit_restores_a_worktree_a_gate_deleted_out_of_band`, `conductor.rs`'s `mod
/// tests`) proves the call site is reached, using a synchronous, single-process `Stub`
/// driver and a `RecordingRunner` whose `run` fabricates the deletion entirely in memory -
/// it never runs a real gate subprocess, never creates a real git-registered worktree, and
/// so never exercises the REAL `Worktree::create` adopt-or-create machinery
/// `ensure_present` actually calls to restore one. This test drives a REAL implementer
/// into a REAL, git-backed unit worktree, then a REAL `sh -c` gate whose OWN command
/// deletes that worktree directory wholesale (leaving its `.git/worktrees/<id>` admin
/// entry behind - the "dir gone, admin entry stale" shape the historical fault took, per
/// this spec's own Goal section) before reporting PASS, and checks purely from outside -
/// filesystem existence, `git worktree list --porcelain`, and `git rev-parse` - that the
/// lens spawn parked immediately afterward finds the worktree restored, registered, and
/// checked out at the unit branch's current tip.
///
/// Round 2 also reads the real event store this run wrote to and checks the `verified`
/// `UnitStatus`'s stamped `worktree_sha`, closing adjudication round 1's UPHELD reject
/// (`sdet-u3c3-verified-sha-stamped-before-restore`: the sha was stamped BEFORE the
/// restore, silently empty in exactly this scenario) at the same real-binary boundary,
/// not just through the implementer's in-crate `Stub`-driven regression.
#[test]
fn step_restores_the_unit_worktree_a_gate_deletes_before_the_review_spawn() {
    use rigger::eventstore::namespace::Namespaced;
    use rigger::eventstore::sqlite::Store;
    use rigger::eventstore::{Direction, EventStore};

    let dir = temp_git_project_with_commit();
    let root = dir.path();
    let rigger = root.join(".rigger");
    std::fs::create_dir_all(rigger.join("agents")).unwrap();
    std::fs::write(
        rigger.join("agents").join("worker.md"),
        "---\nid: worker\nmodel: sonnet\ntools: [Read, Edit]\n---\nDo the unit.\n",
    )
    .unwrap();
    std::fs::write(
        rigger.join("agents").join("a.md"),
        "---\nid: a\nmodel: sonnet\ntools: [Read]\n---\nReview it.\n",
    )
    .unwrap();

    // A marker OUTSIDE the worktree (the scratch root itself, `.rigger/tmp`, which the
    // deletion below never touches) self-reports whether the gate's own `rm -rf` really
    // removed the directory it ran in - the non-vacuity check a single opaque subprocess
    // call otherwise denies an outside observer.
    let marker = rigger.join("tmp").join("gate-deleted-marker.txt");
    let marker_str = marker.to_str().unwrap();
    std::fs::write(
        rigger.join("workflow.yml"),
        format!(
            r#"name: ensureonparktest
defaults:
  grounder: nop
  budget: 60
gates:
  ok:
    run: 'd=$(pwd); cd / && rm -rf "$d"; ( [ -d "$d" ] && echo present || echo absent ) > "{marker}"'
    kind: core
stages:
  solo:
    agent: worker
    gates: [ok]
    on_pass: none
    review:
      lenses: [a]
"#,
            marker = marker_str
        ),
    )
    .unwrap();

    // Step 1: the unit is ready, so its implementer spawn parks - the real, git-backed
    // isolation this fixture deliberately keeps means the unit's own durable worktree is
    // created right here, before the implementer has produced anything.
    let (out, err, ok) = run_rigger(root, &["step"]);
    assert!(ok, "the first step must succeed; stderr: {err}");
    assert!(
        out.contains(r#""id":"solo/implementer#0""#) && out.contains(r#""done":false"#),
        "step 1 parks the implementer; got: {out:?}"
    );

    let wt_dir = root.join(".rigger").join("tmp").join("rigger-wt-solo");
    assert!(
        wt_dir.exists(),
        "premise: a parked implementer must already have its unit worktree on disk: {}",
        wt_dir.display()
    );

    // Write the implementer's "diff" directly into the worktree it was already handed,
    // the shape a real out-of-process agent takes, never a CLI-supplied patch.
    std::fs::write(wt_dir.join("work.rs"), "pub fn work() {}\n").unwrap();

    // Commit it immediately, ahead of `run_single_stage`'s own pre-gate commit (which
    // only runs once step 2 below reaches this unit again). This is NOT modeling the
    // implementer (a real agent never commits); it defends this test's premise against
    // an UNRELATED, already out-of-scope hazard: the step-start sweep on this branch
    // (`Worktree::sweep_terminal`) has no liveness conjunct yet (spec 64 criterion 4, a
    // sibling unit not merged here) and force-removes any worktree whose branch has not
    // yet diverged from the run branch - exactly this window, between `rigger result`
    // and this unit's own first commit. A previously-recorded hazard in this same
    // codebase names the identical fault and the identical mitigation (advance the
    // branch past the run branch immediately, so `merge-base --is-ancestor` is false and
    // the sweep skips it) - applied here so this test exercises ONLY criterion 3's own
    // surface, never criterion 4's still-open gap.
    for args in [&["add", "-A"][..], &["commit", "-q", "-m", "wip"]] {
        let ok = Command::new("git")
            .args(args)
            .current_dir(&wt_dir)
            .status()
            .expect("git must be runnable")
            .success();
        assert!(
            ok,
            "git {args:?} must succeed committing the test's setup diff"
        );
    }

    let (_o, err, ok) = run_rigger(
        root,
        &["result", "solo/implementer#0", "implemented the unit"],
    );
    assert!(
        ok,
        "recording the implementer result must succeed; stderr: {err}"
    );

    // Step 2: the implementer replays, the pre-gate commit lands the file, and the `ok`
    // gate runs - as ITS OWN side effect it deletes the worktree wholesale, then still
    // reports PASS (a real `rm -rf` exits 0). If `run_single_stage` did not restore the
    // worktree via `Worktree::ensure_present` before spawning the review tier, the
    // lens's assigned dir would be handed out already gone.
    let (out, err, ok) = run_rigger(root, &["step"]);
    assert!(ok, "the second step must succeed; stderr: {err}");
    assert!(
        out.contains(r#""id":"solo/lens:a#0""#) && out.contains(r#""done":false"#),
        "step 2 must reach and park the review tier's lens spawn, or the gate never \
         passed and this test proves nothing about ensure-on-park; got: {out:?}\n\
         stderr: {err}"
    );

    // Non-vacuity: the gate's own command really did remove the worktree wholesale
    // before the assertions below run, self-reported from a marker location the
    // deletion itself never touches.
    let marker_content = std::fs::read_to_string(&marker).unwrap_or_default();
    assert_eq!(
        marker_content.trim(),
        "absent",
        "premise: the gate's own rm -rf must actually have removed the worktree \
         wholesale, or this test proves nothing about a restore: {marker_content:?}"
    );

    // The review tier's lens spawn found the worktree RESTORED, not the gone dir a
    // gate-side deletion would otherwise hand out.
    assert!(
        wt_dir.exists(),
        "the review tier's lens spawn must find the unit worktree restored: {}",
        wt_dir.display()
    );
    let list = git_out(root, &["worktree", "list", "--porcelain"])
        .expect("git worktree list must succeed in the seeded repo");
    assert!(
        list.contains("rigger/u/solo"),
        "the restored worktree must be REGISTERED with git again, not just a leftover \
         dir nobody re-added: {list}"
    );

    // Checked out at the unit branch's CURRENT tip (the implementer's own committed
    // file), never rewound or re-created from an older point.
    assert!(
        wt_dir.join("work.rs").exists(),
        "the restored worktree must be checked out at the implementer's own committed \
         tip, containing its landed file: {}",
        wt_dir.display()
    );
    let branch_tip = git_out(root, &["rev-parse", "rigger/u/solo"])
        .expect("the unit branch must resolve a tip after the pre-gate commit");
    let head_after = git_out(&wt_dir, &["rev-parse", "HEAD"]).expect(
        "the restored worktree must resolve its own HEAD - a bare leftover dir with no \
         `.git` admin link would fail this",
    );
    assert_eq!(
        head_after, branch_tip,
        "the restored worktree must be checked out at the SAME tip the durable unit \
         branch carries - never rewound or re-created from an older point"
    );

    // Round 2 (adjudication reject sdet-u3c3-verified-sha-stamped-before-restore, UPHELD):
    // the FIRST attempt stamped the `verified` event's `worktree_sha` BEFORE
    // `ensure_present` restored the gate-deleted worktree, so `head_sha_of` silently read
    // an absent directory and stamped an empty sha. The fix reordered the restore ahead of
    // the stamp. Prove that reorder holds through the REAL binary, not just the
    // implementer's own in-crate `Stub`-driven regression
    // (`verified_worktree_sha_is_stamped_after_a_gate_side_deletion_is_restored`,
    // `conductor.rs`'s `mod tests`): read the `verified` `UnitStatus` this same run just
    // recorded, from the real sqlite-backed store this binary wrote to, and check its
    // stamped `worktree_sha` is a real 40-hex sha agreeing with the restored tree's HEAD -
    // never empty, never a stale snapshot taken during the deletion window.
    let backend = Store::open(root.join(".rigger").join("events.db").to_str().unwrap()).unwrap();
    let store = Namespaced::new(&backend, &run_stream_identity(root));
    let events = store
        .read_stream(rigger::conductor::STREAM, 0, Direction::Forward)
        .unwrap();
    let verified = events
        .iter()
        .find(|e| {
            e.type_ == rigger::ledger::TYPE_UNIT_STATUS
                && String::from_utf8_lossy(&e.data).contains(r#""status":"verified"#)
        })
        .expect("a verified status must have been recorded for the unit before review");
    let verified_sha = verified
        .meta
        .get(rigger::conductor::META_WORKTREE_SHA)
        .cloned()
        .unwrap_or_default();
    assert_eq!(
        verified_sha.len(),
        40,
        "the verified event's worktree_sha must be a real 40-hex sha, not empty - it must \
         be stamped AFTER the gate-deleted worktree is restored, not before: {verified_sha:?}"
    );
    assert!(
        verified_sha.chars().all(|c| c.is_ascii_hexdigit()),
        "the stamped sha must be real hex: {verified_sha:?}"
    );
    assert_eq!(
        verified_sha, head_after,
        "the stamped sha must be the RESTORED tree's actual HEAD - the one the review tier \
         is about to judge, not a snapshot taken during the deletion window"
    );
}

/// Spec 64, criterion 3, adjudication round 2 (`adv-u3c3-ensure-present-covers-only-one-of-
/// three-same-function-windows`, UPHELD): the round-1 fix's single `ensure_present` call
/// site (guarded above) protects only the window before the review tier spawns. The SAME
/// `run_single_stage` function reaches `integrate_and_emit` again, after a SECOND real
/// gate run, on two more doors with no re-assert in between - the main loop's post-approval
/// EXHAUSTIVE gate door being the one reachable end-to-end through the CLI without seeding
/// events directly. Round 2 centralized the fix INSIDE `integrate_and_emit` itself (`src/
/// conductor.rs`, right before `wt.changed_since_base()`), a single call shared by every
/// dir-touching integrate door.
///
/// The implementer's own regression
/// (`a_live_approved_unit_restores_a_worktree_the_integrate_door_exhaustive_gate_deleted`,
/// `conductor.rs`'s `mod tests`) proves this using the in-process `Stub` driver and
/// `RecordingRunner::deleting_worktree`, which fabricates the deletion in memory and never
/// exercises the real `Worktree::create` adopt-or-create machinery or a real git merge.
/// This test drives a REAL implementer into a REAL git-backed unit worktree, gets a REAL
/// adjudicator approval, and only THEN lets a REAL `sh -c` gate - scoped with `inputs`
/// that never match the (grounder-less, always-empty) blast radius, so it is skipped by
/// every narrowed inner-loop run and fires for the FIRST time only at the exhaustive
/// integrate door - delete the worktree wholesale before reporting PASS. If
/// `integrate_and_emit` did not restore it first, `wt.changed_since_base()` would shell
/// into a directory that no longer exists and the whole wave would collapse to a hard
/// error (independently reproduced empirically by the adversary: `git -C <missing-dir>`
/// exits 128, unrecognized by any of `run_stage`'s named sentinel arms) - strictly worse
/// than the review-tier-only window the round-1 fix alone covers. Checked purely from
/// outside: the step call must still finish cleanly (no halt, no collapsed wave), the unit
/// must reach `integrated`, and the approved file must land in the base repo's working
/// tree.
#[test]
fn step_integrates_after_the_exhaustive_gate_deletes_the_worktree_post_approval() {
    use rigger::eventstore::namespace::Namespaced;
    use rigger::eventstore::sqlite::Store;
    use rigger::eventstore::{Direction, EventStore};

    let dir = temp_git_project_with_commit();
    let root = dir.path();
    let rigger = root.join(".rigger");
    std::fs::create_dir_all(rigger.join("agents")).unwrap();
    std::fs::write(
        rigger.join("agents").join("worker.md"),
        "---\nid: worker\nmodel: sonnet\ntools: [Read, Edit]\n---\nDo the unit.\n",
    )
    .unwrap();
    std::fs::write(
        rigger.join("agents").join("judge.md"),
        "---\nid: judge\nmodel: sonnet\ntools: [Read]\n---\nAdjudicate it.\n",
    )
    .unwrap();

    // A marker OUTSIDE the worktree self-reports whether the door gate's own `rm -rf`
    // really ran (and when) - the non-vacuity check a single opaque subprocess call
    // otherwise denies an outside observer.
    let marker = rigger.join("tmp").join("door-gate-deleted-marker.txt");
    let marker_str = marker.to_str().unwrap();
    std::fs::write(
        rigger.join("workflow.yml"),
        format!(
            r#"name: ensureonparkintegratetest
defaults:
  grounder: nop
  budget: 60
gates:
  door:
    run: 'd=$(pwd); cd / && rm -rf "$d"; ( [ -d "$d" ] && echo present || echo absent ) > "{marker}"'
    kind: core
    inputs: [never-matches/**]
stages:
  solo:
    agent: worker
    gates: [door]
    review:
      adjudicator: judge
"#,
            marker = marker_str
        ),
    )
    .unwrap();

    // Step 1: the implementer parks; its real, git-backed unit worktree is created now.
    let (out, err, ok) = run_rigger(root, &["step"]);
    assert!(ok, "the first step must succeed; stderr: {err}");
    assert!(
        out.contains(r#""id":"solo/implementer#0""#) && out.contains(r#""done":false"#),
        "step 1 parks the implementer; got: {out:?}"
    );

    let wt_dir = root.join(".rigger").join("tmp").join("rigger-wt-solo");
    assert!(
        wt_dir.exists(),
        "premise: a parked implementer must already have its unit worktree on disk: {}",
        wt_dir.display()
    );

    // Write the implementer's "diff" directly into the worktree it was already handed, and
    // commit it immediately - the same premise-defending mitigation the sibling test above
    // documents in full: it advances the unit branch past the run branch before the
    // step-start sweep (spec 64 criterion 4, unmerged here) can see an undiverged tip.
    std::fs::write(wt_dir.join("work.rs"), "pub fn work() {}\n").unwrap();
    for args in [&["add", "-A"][..], &["commit", "-q", "-m", "wip"]] {
        let ok = Command::new("git")
            .args(args)
            .current_dir(&wt_dir)
            .status()
            .expect("git must be runnable")
            .success();
        assert!(
            ok,
            "git {args:?} must succeed committing the test's setup diff"
        );
    }

    let (_o, err, ok) = run_rigger(
        root,
        &["result", "solo/implementer#0", "implemented the unit"],
    );
    assert!(
        ok,
        "recording the implementer result must succeed; stderr: {err}"
    );

    // Step 2: the implementer replays and the pre-gate commit lands the file. The `door`
    // gate is scoped with `inputs` that never intersect the unit's blast radius (no
    // grounder is configured, so the radius is always empty) - the narrowed inner loop
    // therefore SKIPS it entirely, and review proceeds straight to parking the adjudicator.
    let (out, err, ok) = run_rigger(root, &["step"]);
    assert!(ok, "the second step must succeed; stderr: {err}");
    assert!(
        out.contains(r#""id":"solo/adjudicator#0""#) && out.contains(r#""done":false"#),
        "step 2 must gate (skipping the scoped door gate) and park the adjudicator; got: \
         {out:?}\nstderr: {err}"
    );
    assert!(
        !marker.exists(),
        "premise: the door gate must NOT have run yet - it is scoped away from the empty \
         blast radius in the narrowed inner loop, so this step must never have executed it"
    );

    // A real approve verdict.
    let (_o, err, ok) = run_rigger(
        root,
        &["result", "solo/adjudicator#0", r#"{"verdict":"approve"}"#],
    );
    assert!(
        ok,
        "recording the adjudicator's approve must succeed; stderr: {err}"
    );

    // Step 3: the approve folds through. The main loop's post-approval path now runs the
    // EXHAUSTIVE gate suite - the door gate's first and only real run - which deletes the
    // worktree wholesale as its own side effect, then still reports PASS (a real `rm -rf`
    // exits 0). `integrate_and_emit` must restore it via `Worktree::ensure_present` before
    // `changed_since_base` reads it, or the whole wave collapses to a hard error instead of
    // completing.
    let (out, err, ok) = run_rigger(root, &["step"]);
    assert!(
        ok,
        "the integrate step must succeed - a missing self-heal collapses the wave to a \
         hard error instead; stderr: {err}\nstdout: {out:?}"
    );
    assert!(
        out.contains(r#""done":true"#) && !out.contains(r#""halted":"#),
        "the approved unit must reach a clean, non-halted fixpoint; got: {out:?}"
    );

    // Non-vacuity: the door gate's own command really did remove the worktree wholesale.
    let marker_content = std::fs::read_to_string(&marker).unwrap_or_default();
    assert_eq!(
        marker_content.trim(),
        "absent",
        "premise: the exhaustive door gate's own rm -rf must actually have removed the \
         worktree wholesale, or this test proves nothing about a restore: {marker_content:?}"
    );

    // The unit reached `integrated`, not stuck or failed.
    let backend = Store::open(root.join(".rigger").join("events.db").to_str().unwrap()).unwrap();
    let store = Namespaced::new(&backend, &run_stream_identity(root));
    let events = store
        .read_stream(rigger::conductor::STREAM, 0, Direction::Forward)
        .unwrap();
    assert!(
        events
            .iter()
            .any(|e| e.type_ == rigger::ledger::TYPE_UNIT_INTEGRATED),
        "the unit must self-heal the exhaustive-gate-deleted worktree inside \
         integrate_and_emit and reach `integrated`, not collapse the wave"
    );
    assert!(
        !events
            .iter()
            .any(|e| e.type_ == rigger::ledger::TYPE_UNIT_FAILED
                || e.type_ == rigger::ledger::TYPE_UNIT_ESCALATED),
        "the self-heal must land cleanly - no failed or escalated unit"
    );

    // The approved work actually landed in the base repo's working tree.
    assert!(
        root.join("work.rs").exists(),
        "the approved work must land in the base after integrate: {}",
        root.display()
    );
}

/// Spec 64, criterion 3, adjudication round 3 finding
/// `adv-u3c3r3-reviewed-and-failed-sha-empty-sentinel-inversion` (UPHELD, approve-arm half,
/// closed by round 4): round 2 fixed the empty-sha bug for the `verified` stamp
/// (`step_restores_the_unit_worktree_a_gate_deletes_before_the_review_spawn` above), but
/// left the SAME bug open at the `reviewed` stamp's own `head_sha_of` read. This proves the
/// round-4 fix at the real-binary boundary, extending sha-stamp coverage from `verified` to
/// `reviewed`, over a full three-tier panel with a between-`rigger-step` deletion repeated
/// at every hand-off, so each restore is independently checked (dir existence, git
/// registration, and HEAD-vs-branch-tip agreement) rather than trusted from one probe.
///
/// This does NOT discriminate round 4's specific NEW mechanism (the re-assert moved from
/// one call before `review_unit` to one call per tier inside `run_reviewer`): every
/// deletion here happens BETWEEN two `rigger step` processes, and `stage_worktree`'s
/// pre-existing, already real-binary-tested adopt-or-create restore (round 1, see
/// `step_restores_the_unit_worktree_a_gate_deletes_before_the_review_spawn`) runs once at
/// the top of EVERY step invocation - healing any between-step deletion before review even
/// begins, regardless of round 4. Verified empirically, not assumed: mutating OUT round 4's
/// per-tier re-assert left this test GREEN (see the progress log and
/// `run_end_to_end_restores_a_worktree_a_reviewer_agent_deletes_mid_review` below, which
/// closes that specific gap through the synchronous CLI driver instead - the only path a
/// deletion WITHIN one process, between two tiers, can occur through a real agent spawn).
#[test]
fn step_stamps_a_real_reviewed_sha_after_repeated_between_step_deletions() {
    use rigger::eventstore::namespace::Namespaced;
    use rigger::eventstore::sqlite::Store;
    use rigger::eventstore::{Direction, EventStore};

    let dir = temp_git_project_with_commit();
    let root = dir.path();
    let rigger = root.join(".rigger");
    std::fs::create_dir_all(rigger.join("agents")).unwrap();
    std::fs::write(
        rigger.join("agents").join("worker.md"),
        "---\nid: worker\nmodel: sonnet\ntools: [Read, Edit]\n---\nDo the unit.\n",
    )
    .unwrap();
    for (id, body) in [
        ("a", "Review it."),
        ("adv", "Try to break it."),
        ("judge", "Adjudicate it."),
    ] {
        std::fs::write(
            rigger.join("agents").join(format!("{id}.md")),
            format!("---\nid: {id}\nmodel: sonnet\ntools: [Read]\n---\n{body}\n"),
        )
        .unwrap();
    }
    std::fs::write(
        rigger.join("workflow.yml"),
        r#"name: ensureonparktierstest
defaults:
  grounder: nop
  budget: 60
gates:
  ok: { run: "true", kind: core }
stages:
  solo:
    agent: worker
    gates: [ok]
    on_pass: none
    review:
      lenses: [a]
      adversary: adv
      adjudicator: judge
"#,
    )
    .unwrap();

    let wt_dir = root.join(".rigger").join("tmp").join("rigger-wt-solo");

    // Step 1: the implementer parks; its real, git-backed unit worktree is created now.
    let (out, err, ok) = run_rigger(root, &["step"]);
    assert!(ok, "the first step must succeed; stderr: {err}");
    assert!(
        out.contains(r#""id":"solo/implementer#0""#) && out.contains(r#""done":false"#),
        "step 1 parks the implementer; got: {out:?}"
    );
    assert!(
        wt_dir.exists(),
        "premise: the unit worktree must exist after step 1: {}",
        wt_dir.display()
    );

    // Write the implementer's "diff" directly into the worktree it was already handed, and
    // commit it immediately - the same premise-defending mitigation the sibling tests above
    // document in full (advances the unit branch past the run branch before the step-start
    // sweep, which has no liveness conjunct yet, can see an undiverged tip).
    std::fs::write(wt_dir.join("work.rs"), "pub fn work() {}\n").unwrap();
    for args in [&["add", "-A"][..], &["commit", "-q", "-m", "wip"]] {
        let ok = Command::new("git")
            .args(args)
            .current_dir(&wt_dir)
            .status()
            .expect("git must be runnable")
            .success();
        assert!(
            ok,
            "git {args:?} must succeed committing the test's setup diff"
        );
    }
    let (_o, err, ok) = run_rigger(
        root,
        &["result", "solo/implementer#0", "implemented the unit"],
    );
    assert!(
        ok,
        "recording the implementer result must succeed; stderr: {err}"
    );

    // Step 2: the `ok` gate passes cleanly (no deletion of its own), and the review panel
    // parks its FIRST tier, the lens.
    let (out, err, ok) = run_rigger(root, &["step"]);
    assert!(ok, "the second step must succeed; stderr: {err}");
    assert!(
        out.contains(r#""id":"solo/lens:a#0""#) && out.contains(r#""done":false"#),
        "step 2 must park the lens; got: {out:?}\nstderr: {err}"
    );
    let (_o, err, ok) = run_rigger(root, &["result", "solo/lens:a#0", "reviewed: no blocker"]);
    assert!(ok, "recording the lens result must succeed; stderr: {err}");

    // OUT-OF-BAND DELETION 1: mimic an out-of-band actor removing the worktree in the real
    // wall-clock gap between the lens tier resolving and the adversary tier's own spawn -
    // exactly the window round 2's single before-`review_unit` re-assert left unprotected,
    // and round 4's per-tier re-assert (moved into `run_reviewer`) now covers.
    assert!(
        wt_dir.exists(),
        "premise: the worktree must exist before this deletion"
    );
    std::fs::remove_dir_all(&wt_dir).unwrap();
    assert!(
        !wt_dir.exists(),
        "premise: the out-of-band deletion must actually have removed it"
    );

    // Step 3: the adversary tier's spawn must find the worktree RESTORED, not gone.
    let (out, err, ok) = run_rigger(root, &["step"]);
    assert!(ok, "the third step must succeed; stderr: {err}");
    assert!(
        out.contains(r#""id":"solo/adversary#0""#) && out.contains(r#""done":false"#),
        "step 3 must park the adversary, finding the restored worktree, not collapse; got: \
         {out:?}\nstderr: {err}"
    );
    assert!(
        wt_dir.exists(),
        "the adversary tier's spawn must find the unit worktree restored: {}",
        wt_dir.display()
    );
    let list = git_out(root, &["worktree", "list", "--porcelain"])
        .expect("git worktree list must succeed in the seeded repo");
    assert!(
        list.contains("rigger/u/solo"),
        "the restored worktree must be REGISTERED with git again, not just a leftover dir: {list}"
    );
    let branch_tip = git_out(root, &["rev-parse", "rigger/u/solo"])
        .expect("the unit branch must resolve a tip after the pre-gate commit");
    let head_at_adversary = git_out(&wt_dir, &["rev-parse", "HEAD"])
        .expect("the restored worktree must resolve its own HEAD");
    assert_eq!(
        head_at_adversary, branch_tip,
        "the restored worktree must be checked out at the durable branch's actual tip"
    );

    let (_o, err, ok) = run_rigger(root, &["result", "solo/adversary#0", "tried and failed"]);
    assert!(
        ok,
        "recording the adversary result must succeed; stderr: {err}"
    );

    // OUT-OF-BAND DELETION 2: the SAME window, one tier later - the adversary-to-adjudicator
    // hand-off. Round 4's fix is a per-tier re-assert inside the ONE shared `run_reviewer`
    // authority, so this must self-heal exactly like deletion 1 did, not just the second tier.
    std::fs::remove_dir_all(&wt_dir).unwrap();
    assert!(
        !wt_dir.exists(),
        "premise: the second out-of-band deletion must have removed it"
    );

    // Step 4: the adjudicator tier's spawn must ALSO find the worktree restored.
    let (out, err, ok) = run_rigger(root, &["step"]);
    assert!(ok, "the fourth step must succeed; stderr: {err}");
    assert!(
        out.contains(r#""id":"solo/adjudicator#0""#) && out.contains(r#""done":false"#),
        "step 4 must park the adjudicator, finding the restored worktree; got: {out:?}\nstderr: \
         {err}"
    );
    assert!(
        wt_dir.exists(),
        "the adjudicator tier's spawn must ALSO find the unit worktree restored, proving the \
         re-assert runs before EVERY tier's spawn, not only the one right after the lens: {}",
        wt_dir.display()
    );
    let list = git_out(root, &["worktree", "list", "--porcelain"])
        .expect("git worktree list must succeed in the seeded repo");
    assert!(
        list.contains("rigger/u/solo"),
        "the twice-restored worktree must still be REGISTERED with git: {list}"
    );

    let (_o, err, ok) = run_rigger(
        root,
        &["result", "solo/adjudicator#0", r#"{"verdict":"approve"}"#],
    );
    assert!(
        ok,
        "recording the adjudicator's approve must succeed; stderr: {err}"
    );

    // OUT-OF-BAND DELETION 3: closes the sibling finding's approve-arm half - the window
    // between the adjudicator's own spawn returning approved and the `reviewed` event's
    // `worktree_sha` stamp being read, which round 2 already fixed for the sibling
    // `verified` stamp but round 3 found unfixed here.
    std::fs::remove_dir_all(&wt_dir).unwrap();
    assert!(
        !wt_dir.exists(),
        "premise: the third out-of-band deletion must have removed it"
    );

    // Step 5: the approve folds through (`on_pass: none`, so the unit reaches `reviewed`
    // and stops - no further gate/integrate door to cross).
    let (out, err, ok) = run_rigger(root, &["step"]);
    assert!(
        ok,
        "the fifth step must succeed - a missing self-heal here would surface as a hard \
         error reading the deleted tree's HEAD; stderr: {err}\nstdout: {out:?}"
    );
    assert!(
        out.contains(r#""done":true"#),
        "the reviewed, unmerged unit must reach a clean fixpoint; got: {out:?}"
    );
    // Note: `wt_dir` is gone again by now - a genuinely TERMINAL (non-parked) return tears
    // the worktree down as ordinary end-of-stage cleanup (`run_stage`, unconditional on any
    // non-parked result), unrelated to ensure-on-park. What this step must have done BEFORE
    // that teardown is restore the tree long enough to stamp a real `reviewed_sha` - checked
    // below via the recorded event, the only outside-observable evidence of that window.

    let backend = Store::open(root.join(".rigger").join("events.db").to_str().unwrap()).unwrap();
    let store = Namespaced::new(&backend, &run_stream_identity(root));
    let events = store
        .read_stream(rigger::conductor::STREAM, 0, Direction::Forward)
        .unwrap();
    let reviewed = events
        .iter()
        .find(|e| {
            e.type_ == rigger::ledger::TYPE_UNIT_STATUS
                && String::from_utf8_lossy(&e.data).contains(r#""status":"reviewed"#)
        })
        .expect("a reviewed status must have been recorded for the unit");
    let reviewed_sha = reviewed
        .meta
        .get(rigger::conductor::META_WORKTREE_SHA)
        .cloned()
        .unwrap_or_default();
    assert_eq!(
        reviewed_sha.len(),
        40,
        "the reviewed event's worktree_sha must be a real 40-hex sha, not empty - it must be \
         stamped AFTER a re-assert restores whatever the adjudicator's own spawn (or an \
         out-of-band actor) deleted, not a snapshot taken during the deletion window: \
         {reviewed_sha:?}"
    );
    assert!(
        reviewed_sha.chars().all(|c| c.is_ascii_hexdigit()),
        "the stamped sha must be real hex: {reviewed_sha:?}"
    );
    assert_eq!(
        reviewed_sha, branch_tip,
        "the stamped sha must be the durable branch's actual tip - the RESTORED tree's real \
         HEAD, not a snapshot taken during the deletion window"
    );
}

/// Mirrors `step_stamps_a_real_reviewed_sha_after_repeated_between_step_deletions` above for
/// the REJECT arm (`run_single_stage`'s `failed_sha` stamp on the review-reject
/// `UnitFailed`, `adv-u3c3r3-reviewed-and-failed-sha-empty-sentinel-inversion`'s second
/// sibling site, closed by round 4): the same empty-sha bug survives on a reject exactly as
/// on an approve - the fold this field feeds (spec 11 unit 1's flip-flop detection) needs it
/// real on EITHER verdict. Same scope note as the sibling test: this proves the STAMP is
/// correct after a between-step restore (extending real-binary sha coverage to
/// `failed_sha`), not round 4's specific per-tier mechanism - see that test's doc comment
/// for why a between-step deletion cannot discriminate the two, and
/// `run_end_to_end_restores_a_worktree_a_reviewer_agent_deletes_mid_review` for the test
/// that does. The implementer's own regression
/// (`failed_worktree_sha_is_stamped_after_the_adjudicators_own_deletion_is_restored`,
/// `conductor.rs`'s `mod tests`) proves the algorithm in-process; this drives a REAL
/// adjudicator to a REJECT verdict through the real binary, deletes the worktree directly
/// (out-of-band) between the recorded reject and the next step, and proves the resulting
/// `UnitFailed` event's
/// `worktree_sha` is a real 40-hex sha of the restored tree, never the empty sentinel a
/// pre-round-4 binary would have stamped.
#[test]
fn step_stamps_a_real_failed_sha_after_a_deletion_before_the_reject_stamp() {
    use rigger::eventstore::namespace::Namespaced;
    use rigger::eventstore::sqlite::Store;
    use rigger::eventstore::{Direction, EventStore};

    let dir = temp_git_project_with_commit();
    let root = dir.path();
    let rigger = root.join(".rigger");
    std::fs::create_dir_all(rigger.join("agents")).unwrap();
    std::fs::write(
        rigger.join("agents").join("worker.md"),
        "---\nid: worker\nmodel: sonnet\ntools: [Read, Edit]\n---\nDo the unit.\n",
    )
    .unwrap();
    std::fs::write(
        rigger.join("agents").join("judge.md"),
        "---\nid: judge\nmodel: sonnet\ntools: [Read]\n---\nAdjudicate it.\n",
    )
    .unwrap();
    std::fs::write(
        rigger.join("workflow.yml"),
        r#"name: ensureonparkfailedshatest
defaults:
  grounder: nop
  budget: 60
gates:
  ok: { run: "true", kind: core }
stages:
  solo:
    agent: worker
    gates: [ok]
    on_pass: none
    review:
      adjudicator: judge
"#,
    )
    .unwrap();

    let wt_dir = root.join(".rigger").join("tmp").join("rigger-wt-solo");

    let (out, err, ok) = run_rigger(root, &["step"]);
    assert!(ok, "the first step must succeed; stderr: {err}");
    assert!(
        out.contains(r#""id":"solo/implementer#0""#) && out.contains(r#""done":false"#),
        "step 1 parks the implementer; got: {out:?}"
    );

    std::fs::write(wt_dir.join("work.rs"), "pub fn work() {}\n").unwrap();
    for args in [&["add", "-A"][..], &["commit", "-q", "-m", "wip"]] {
        let ok = Command::new("git")
            .args(args)
            .current_dir(&wt_dir)
            .status()
            .expect("git must be runnable")
            .success();
        assert!(
            ok,
            "git {args:?} must succeed committing the test's setup diff"
        );
    }
    let (_o, err, ok) = run_rigger(
        root,
        &["result", "solo/implementer#0", "implemented the unit"],
    );
    assert!(
        ok,
        "recording the implementer result must succeed; stderr: {err}"
    );

    let (out, err, ok) = run_rigger(root, &["step"]);
    assert!(ok, "the second step must succeed; stderr: {err}");
    assert!(
        out.contains(r#""id":"solo/adjudicator#0""#) && out.contains(r#""done":false"#),
        "step 2 must park the adjudicator; got: {out:?}\nstderr: {err}"
    );

    let (_o, err, ok) = run_rigger(
        root,
        &["result", "solo/adjudicator#0", r#"{"verdict":"reject"}"#],
    );
    assert!(
        ok,
        "recording the adjudicator's reject must succeed; stderr: {err}"
    );

    // OUT-OF-BAND DELETION: the window between the adjudicator's own spawn returning
    // reject and the `failed_sha` read in `run_single_stage`'s remediation fall-through.
    assert!(
        wt_dir.exists(),
        "premise: the worktree must exist before this deletion"
    );
    std::fs::remove_dir_all(&wt_dir).unwrap();
    assert!(
        !wt_dir.exists(),
        "premise: the out-of-band deletion must actually have removed it"
    );

    // Step 3: the reject folds through remediation (a fresh implementer attempt parks) -
    // either way this step must not collapse, and the UnitFailed event it records along the
    // way must carry a real sha.
    let (out, err, ok) = run_rigger(root, &["step"]);
    assert!(
        ok,
        "the third step must succeed - a missing self-heal here would surface as a hard \
         error reading the deleted tree's HEAD; stderr: {err}\nstdout: {out:?}"
    );

    // The restored tree's own HEAD is the independent, outside-git ground truth for what
    // the failed_sha stamp should read, read immediately after step 3 returns (before any
    // further attempt has a chance to write to the same dir).
    let head_after = git_out(&wt_dir, &["rev-parse", "HEAD"])
        .expect("the restored worktree must resolve its own HEAD after step 3");

    let backend = Store::open(root.join(".rigger").join("events.db").to_str().unwrap()).unwrap();
    let store = Namespaced::new(&backend, &run_stream_identity(root));
    let events = store
        .read_stream(rigger::conductor::STREAM, 0, Direction::Forward)
        .unwrap();
    let failed = events
        .iter()
        .find(|e| e.type_ == rigger::ledger::TYPE_UNIT_FAILED)
        .expect("a review-reject UnitFailed must have been recorded");
    let failed_sha = failed
        .meta
        .get(rigger::conductor::META_WORKTREE_SHA)
        .cloned()
        .unwrap_or_default();
    assert_eq!(
        failed_sha.len(),
        40,
        "the failed event's worktree_sha must be a real 40-hex sha, not empty - it must be \
         stamped AFTER a re-assert restores the out-of-band-deleted tree, not a snapshot \
         taken during the deletion window: {failed_sha:?}"
    );
    assert!(
        failed_sha.chars().all(|c| c.is_ascii_hexdigit()),
        "the stamped sha must be real hex: {failed_sha:?}"
    );
    assert_eq!(
        failed_sha, head_after,
        "the stamped sha must be the restored tree's actual HEAD, not a snapshot taken \
         during the deletion window"
    );
}

/// Spec 64, criterion 3, adjudication round 3 finding
/// `adv-u3c3r3-ensure-present-covers-only-the-first-tier` (UPHELD, closed by round 4's
/// `impl-u3c3-r4-reassert-centralized-in-run-reviewer`): the TRUE periphery of round 4's
/// fix, not a restatement of it.
///
/// The window round 4 actually closes exists WITHIN one process, between two REAL
/// synchronous agent spawns. The implementer's own in-crate regression
/// (`review_tier_boundary_restores_a_worktree_a_prior_tier_deleted`, `conductor.rs`'s `mod
/// tests`) proves the algorithm with a synchronous, single-process `Stub` driver whose OWN
/// `spawn` call deletes the directory as a side effect. The stepwise/replay driver `rigger
/// step` drives (every OTHER periphery test in this file) cannot reach this window: a
/// genuinely NEW spawn always PARKS without touching the worktree, so every tier-to-tier
/// hand-off crosses a real PROCESS boundary, and `stage_worktree`'s pre-existing,
/// already-real-binary-tested adopt-or-create restore (round 1) runs once at the top of
/// EVERY `rigger step` invocation - healing any out-of-band deletion BEFORE review even
/// begins, regardless of round 4's fix. Verified empirically, not assumed: the sibling
/// tests above document that they went GREEN under a mutation that disabled round 4's
/// per-tier re-assert, before this test was written to close the actual gap.
///
/// This drives the REAL `cli::Driver` instead - the synchronous, subprocess-per-spawn path
/// `rigger run` uses - with a fake `claude` executable substituted onto `PATH` (the same
/// shimming technique `src/driver/cli.rs`'s own `spawn_shells_out_and_bridges_the_agents_
/// emits` unit test uses for the driver alone, extended here through the whole compiled
/// binary and a real git-backed unit worktree). The fake agent plays four roles, selected
/// by a marker embedded in each agent's own persona (which `build_system_prompt` forwards
/// verbatim into `--system-prompt`): the worker writes a file; the LENS - the review
/// panel's own FIRST tier, standing in for the "review agents doing unprompted forensic
/// self-repair" this spec's Goal section names as the motivating harm - deletes its own
/// `$PWD` (the unit worktree) wholesale as a side effect of running, the identical shape
/// `RecordingRunner::deleting_worktree` proves at the gate boundary but here at the
/// review-TIER spawn boundary instead; the adversary and adjudicator behave normally. If
/// `run_reviewer`'s per-tier re-assert does not run immediately before the ADVERSARY's
/// spawn - the very next tier after the lens, in the SAME process - `Command::current_dir`
/// on the now-missing directory fails at the OS boundary with a real ENOENT, which no
/// sentinel arm in `run_stage` recognizes, and the whole `rigger run` process exits
/// non-zero instead of completing.
#[cfg(unix)]
#[test]
fn run_end_to_end_restores_a_worktree_a_reviewer_agent_deletes_mid_review() {
    use std::os::unix::fs::PermissionsExt;

    use rigger::eventstore::namespace::Namespaced;
    use rigger::eventstore::sqlite::Store;
    use rigger::eventstore::{Direction, EventStore};

    let dir = temp_git_project_with_commit();
    let root = dir.path();
    let rigger = root.join(".rigger");
    std::fs::create_dir_all(rigger.join("agents")).unwrap();
    std::fs::write(
        rigger.join("agents").join("worker.md"),
        "---\nid: worker\nmodel: sonnet\ntools: [Read, Edit]\n---\nRIGGERTEST_WORKER: do the \
         unit.\n",
    )
    .unwrap();
    std::fs::write(
        rigger.join("agents").join("a.md"),
        "---\nid: a\nmodel: sonnet\ntools: [Read]\n---\nRIGGERTEST_LENS_DELETE: review it.\n",
    )
    .unwrap();
    std::fs::write(
        rigger.join("agents").join("adv.md"),
        "---\nid: adv\nmodel: sonnet\ntools: [Read]\n---\nRIGGERTEST_ADVERSARY: try to break \
         it.\n",
    )
    .unwrap();
    std::fs::write(
        rigger.join("agents").join("judge.md"),
        "---\nid: judge\nmodel: sonnet\ntools: [Read]\n---\nRIGGERTEST_ADJUDICATOR: adjudicate \
         it.\n",
    )
    .unwrap();
    std::fs::write(
        rigger.join("workflow.yml"),
        r#"name: ensureonparkendtoendtest
defaults:
  grounder: nop
  budget: 60
gates:
  ok: { run: "true", kind: core }
stages:
  solo:
    agent: worker
    gates: [ok]
    on_pass: none
    review:
      lenses: [a]
      adversary: adv
      adjudicator: judge
"#,
    )
    .unwrap();

    // A fake `claude` executable, substituted onto PATH ahead of the real system PATH. Its
    // behavior is selected by a marker embedded in each agent's own persona (above), which
    // the driver forwards verbatim into `--system-prompt`; the lens's own branch deletes
    // its `$PWD` wholesale (mirroring a real gate's `rm -rf`, per the sibling gate-based
    // tests above) before reporting, self-describing the deletion to `$RIGGERTEST_MARKER`
    // (a location OUTSIDE the worktree the deletion itself never touches).
    let fakebin = tempfile::tempdir().unwrap();
    let claude_path = fakebin.path().join("claude");
    std::fs::write(
        &claude_path,
        r#"#!/bin/sh
sp=""
next=0
for a in "$@"; do
  if [ "$next" = "1" ]; then
    sp="$a"
    next=0
  fi
  if [ "$a" = "--system-prompt" ]; then
    next=1
  fi
done
case "$sp" in
  *RIGGERTEST_LENS_DELETE*)
    d="$(pwd)"
    cd / || exit 1
    rm -rf "$d"
    if [ -d "$d" ]; then echo present > "$RIGGERTEST_MARKER"; else echo absent > "$RIGGERTEST_MARKER"; fi
    echo "reviewed: no blocker"
    ;;
  *RIGGERTEST_ADVERSARY*)
    echo "tried and failed"
    ;;
  *RIGGERTEST_ADJUDICATOR*)
    echo '{"verdict":"approve"}'
    ;;
  *RIGGERTEST_WORKER*)
    echo "pub fn work() {}" > work.rs
    ;;
  *)
    echo "fake-claude: unrecognized system prompt: $sp" 1>&2
    exit 1
    ;;
esac
"#,
    )
    .unwrap();
    let mut perms = std::fs::metadata(&claude_path).unwrap().permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(&claude_path, perms).unwrap();

    let marker = root.join("lens-deleted-marker.txt");
    let path_env = format!(
        "{}:{}",
        fakebin.path().display(),
        std::env::var("PATH").unwrap_or_default()
    );

    let (out, err, ok) = run_rigger_envs(
        root,
        &["run"],
        &[
            ("PATH", &path_env),
            ("RIGGERTEST_MARKER", marker.to_str().unwrap()),
        ],
    );
    assert!(
        ok,
        "the end-to-end run must succeed - a missing per-tier self-heal surfaces as a real \
         ENOENT spawning the adversary in the now-deleted worktree instead; stderr: {err}\n\
         stdout: {out}"
    );

    // Non-vacuity: the lens's own fake-agent process really did delete the worktree
    // wholesale, self-reported from a location outside the worktree the deletion itself
    // never touches.
    let marker_content = std::fs::read_to_string(&marker).unwrap_or_default();
    assert_eq!(
        marker_content.trim(),
        "absent",
        "premise: the lens's own process must actually have removed the worktree wholesale, \
         or this test proves nothing about a restore: {marker_content:?}"
    );

    // The unit reached `reviewed`, not stuck, failed, or escalated - so the adversary and
    // adjudicator tiers both really ran to completion in the SAME worktree the lens
    // deleted, in the SAME process.
    let backend = Store::open(root.join(".rigger").join("events.db").to_str().unwrap()).unwrap();
    let store = Namespaced::new(&backend, &run_stream_identity(root));
    let events = store
        .read_stream(rigger::conductor::STREAM, 0, Direction::Forward)
        .unwrap();
    assert!(
        events.iter().any(|e| {
            e.type_ == rigger::ledger::TYPE_UNIT_STATUS
                && String::from_utf8_lossy(&e.data).contains(r#""status":"reviewed"#)
        }),
        "the unit must self-heal the lens-deleted worktree before the adversary's own real \
         subprocess spawn and reach `reviewed`, not collapse the run; events: {events:?}"
    );
    assert!(
        !events
            .iter()
            .any(|e| e.type_ == rigger::ledger::TYPE_UNIT_FAILED
                || e.type_ == rigger::ledger::TYPE_UNIT_ESCALATED),
        "the self-heal must land cleanly - no failed or escalated unit"
    );
}

/// Spec 64 criterion 3, adjudication round 4 (`adv-u3c3r4-concurrent-lens-ensure-present-races-
/// worktree-create`, `sdet-u3c3r4-concurrent-lenses-race-ensure-present-on-the-same-worktree`,
/// `arch-u3c3r4-speculation-winner-sha-unguarded`, all UPHELD; fixed round 5 with a per-
/// `Worktree` `reassert_mu` mutex and a pre-read `ensure_present()` call in
/// `emit_speculation_winner_status`).
///
/// The implementer's own regressions
/// (`worktree::tests::concurrent_ensure_present_on_a_deleted_worktree_never_races_create`,
/// `conductor::tests::speculation_lenses_restore_a_gate_deleted_worktree_without_racing_and_stamp_the_winner_sha`)
/// prove both fixes purely in-process: N real threads sharing a bare `Worktree`, and a
/// `RecordingRunner`/`Stub` driver standing in for the gates and the review agents - never an
/// actual `git worktree add` race between two REAL OS PROCESSES, nor the real `rigger run` ->
/// `run_speculation` -> `emit_speculation_winner_status` call path through the compiled binary.
///
/// This drives it end to end: a `speculation_width: 2` unit with an UNSCOPED gate (`ok`, no
/// `inputs`, so it runs at both the narrowed AND exhaustive gate doors - spec 64 c3 round 2's
/// own technique) whose own `rm -rf` deletes candidate 0's worktree wholesale every time it
/// runs, reviewed by a two-lens panel (`a`, `b`) that fans out as REAL concurrent OS threads,
/// each spawning its own real `claude` subprocess (a fake shim substituted onto PATH, role-
/// selected by a marker in each agent's own persona, mirroring the sibling end-to-end test
/// above). Before round 5's fix, two real threads racing `Worktree::create`'s `git worktree
/// add`/adopt path against the SAME dir is a genuine git race (not fabricated); and
/// `emit_speculation_winner_status`'s `winner_sha` read - right after the exhaustive gate's
/// SECOND deletion - had no re-assert guard of its own.
#[test]
fn run_speculation_restores_a_gate_deleted_worktree_across_concurrent_lenses_and_stamps_a_real_winner_sha(
) {
    use std::os::unix::fs::PermissionsExt;

    use rigger::eventstore::namespace::Namespaced;
    use rigger::eventstore::sqlite::Store;
    use rigger::eventstore::{Direction, EventStore};

    let dir = temp_git_project_with_commit();
    let root = dir.path();
    let rigger = root.join(".rigger");
    std::fs::create_dir_all(rigger.join("agents")).unwrap();
    std::fs::write(
        rigger.join("agents").join("worker.md"),
        "---\nid: worker\nmodel: sonnet\ntools: [Read, Edit]\n---\nRIGGERTEST_WORKER: do the \
         unit.\n",
    )
    .unwrap();
    std::fs::write(
        rigger.join("agents").join("a.md"),
        "---\nid: a\nmodel: sonnet\ntools: [Read]\n---\nRIGGERTEST_LENS_A: review it.\n",
    )
    .unwrap();
    std::fs::write(
        rigger.join("agents").join("b.md"),
        "---\nid: b\nmodel: sonnet\ntools: [Read]\n---\nRIGGERTEST_LENS_B: review it.\n",
    )
    .unwrap();
    std::fs::write(
        rigger.join("agents").join("judge.md"),
        "---\nid: judge\nmodel: sonnet\ntools: [Read]\n---\nRIGGERTEST_ADJUDICATOR: adjudicate \
         it.\n",
    )
    .unwrap();

    // Two markers OUTSIDE the worktree self-report each gate's own `rm -rf` really running
    // (and really removing the dir): `ok` is UNSCOPED (no `inputs`) so it runs for real in
    // the NARROWED pass, before the lens fan-out - but a gate verdict replays from the SAME
    // `(unit, attempt, gate)` key regardless of selection (spec 12, unit 1), so re-listing it
    // would only REPLAY, not re-run, at the exhaustive pass. `door` is scoped with `inputs`
    // that never intersect the always-empty (no grounder) blast radius (round 3's own
    // technique, `step_integrates_after_the_exhaustive_gate_deletes_the_worktree_post_approval`
    // above) - SKIPPED (not run, not cached) at the narrowed pass, so its FIRST real run lands
    // at the exhaustive pass, right before the winner-sha read.
    let narrowed_marker = rigger
        .join("tmp")
        .join("speculation-narrowed-deleted-marker.txt");
    let exhaustive_marker = rigger
        .join("tmp")
        .join("speculation-exhaustive-deleted-marker.txt");
    std::fs::create_dir_all(narrowed_marker.parent().unwrap()).unwrap();
    std::fs::write(
        rigger.join("workflow.yml"),
        format!(
            r#"name: ensureonparkspeculationwinnertest
defaults:
  grounder: nop
  budget: 60
gates:
  ok:
    run: 'd=$(pwd); cd / && rm -rf "$d"; ( [ -d "$d" ] && echo present || echo absent ) >> "{narrowed}"'
  door:
    run: 'd=$(pwd); cd / && rm -rf "$d"; ( [ -d "$d" ] && echo present || echo absent ) >> "{exhaustive}"'
    inputs: [never-matches/**]
stages:
  solo:
    agent: worker
    gates: [ok, door]
    on_pass: none
    speculation_width: 2
    review:
      lenses: [a, b]
      adjudicator: judge
"#,
            narrowed = narrowed_marker.to_str().unwrap(),
            exhaustive = exhaustive_marker.to_str().unwrap(),
        ),
    )
    .unwrap();

    let fakebin = tempfile::tempdir().unwrap();
    let claude_path = fakebin.path().join("claude");
    std::fs::write(
        &claude_path,
        r#"#!/bin/sh
sp=""
next=0
for a in "$@"; do
  if [ "$next" = "1" ]; then
    sp="$a"
    next=0
  fi
  if [ "$a" = "--system-prompt" ]; then
    next=1
  fi
done
case "$sp" in
  *RIGGERTEST_LENS_A*)
    echo lensA >> "$RIGGERTEST_LENS_MARKER"
    echo "reviewed: no blocker"
    ;;
  *RIGGERTEST_LENS_B*)
    echo lensB >> "$RIGGERTEST_LENS_MARKER"
    echo "reviewed: no blocker"
    ;;
  *RIGGERTEST_ADJUDICATOR*)
    echo '{"verdict":"approve"}'
    ;;
  *RIGGERTEST_WORKER*)
    echo "pub fn work() {}" > work.rs
    ;;
  *)
    echo "fake-claude: unrecognized system prompt: $sp" 1>&2
    exit 1
    ;;
esac
"#,
    )
    .unwrap();
    let mut perms = std::fs::metadata(&claude_path).unwrap().permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(&claude_path, perms).unwrap();

    // Both lenses self-report having run into ONE shared file, OUTSIDE the worktree the
    // gate's own deletion never touches - the non-vacuity check that they really are two
    // independent real subprocesses, not one call standing in for both.
    let lens_marker = root.join("lens-ran-marker.txt");
    let path_env = format!(
        "{}:{}",
        fakebin.path().display(),
        std::env::var("PATH").unwrap_or_default()
    );

    let (out, err, ok) = run_rigger_envs(
        root,
        &["run"],
        &[
            ("PATH", &path_env),
            ("RIGGERTEST_LENS_MARKER", lens_marker.to_str().unwrap()),
        ],
    );
    assert!(
        ok,
        "the speculation run must succeed - a missing concurrent-lens serialization surfaces \
         as a real git race, and a missing pre-read re-assert surfaces as an empty-sha stamp \
         or a hard ENOENT instead; stderr: {err}\nstdout: {out}"
    );

    // Non-vacuity: both lenses really ran as their own real subprocess, concurrently sharing
    // the SAME candidate worktree the gate deleted.
    let lens_report = std::fs::read_to_string(&lens_marker).unwrap_or_default();
    assert!(
        lens_report.contains("lensA") && lens_report.contains("lensB"),
        "premise: both lenses must actually have run as real concurrent processes against the \
         same shared worktree, or this test proves nothing about the race: {lens_report:?}"
    );

    // Non-vacuity: BOTH doors really deleted the worktree wholesale - the unscoped `ok` gate
    // for real at the narrowed pass (opening the concurrent-lens race window), and the
    // `inputs`-scoped `door` gate for the FIRST time at the exhaustive pass (opening the
    // winner-sha race window) - two DISTINCT real deletions, not one gate replayed twice.
    let narrowed_report = std::fs::read_to_string(&narrowed_marker).unwrap_or_default();
    assert_eq!(
        narrowed_report.trim(),
        "absent",
        "premise: the unscoped `ok` gate must have deleted the worktree wholesale in the \
         narrowed pass, before the lens fan-out, or this test proves nothing about the race: \
         {narrowed_report:?}"
    );
    let exhaustive_report = std::fs::read_to_string(&exhaustive_marker).unwrap_or_default();
    assert_eq!(
        exhaustive_report.trim(),
        "absent",
        "premise: the `door` gate, skipped in the narrowed pass, must run for the first time \
         in the exhaustive pass and delete the worktree wholesale right before the winner-sha \
         read, or this test proves nothing about that guard: {exhaustive_report:?}"
    );

    // The candidate's worktree is restored and REGISTERED with git (not a leftover dir) after
    // the run, checked out on its own unit branch.
    let wt_dir = root.join(".rigger").join("tmp").join("rigger-wt-solo");
    assert!(
        wt_dir.is_dir(),
        "the candidate worktree must be restored after the run: {}",
        wt_dir.display()
    );
    let list = git_out(root, &["worktree", "list", "--porcelain"])
        .expect("git worktree list must succeed in the seeded repo");
    assert!(
        list.contains(wt_dir.to_str().unwrap()),
        "the restored candidate worktree must be REGISTERED with git, not a leftover dir: {list}"
    );

    // The deferred winner status carries a real 40-hex worktree_sha - the RESTORED tree's
    // actual HEAD, not an empty sentinel snapshotted during the deletion window.
    let backend = Store::open(root.join(".rigger").join("events.db").to_str().unwrap()).unwrap();
    let store = Namespaced::new(&backend, &run_stream_identity(root));
    let events = store
        .read_stream(rigger::conductor::STREAM, 0, Direction::Forward)
        .unwrap();
    let verified = events
        .iter()
        .find(|e| {
            e.type_ == rigger::ledger::TYPE_UNIT_STATUS
                && String::from_utf8_lossy(&e.data).contains(r#""status":"verified"#)
        })
        .expect("the speculation winner's deferred verified status must have been recorded");
    let winner_sha = verified
        .meta
        .get(rigger::conductor::META_WORKTREE_SHA)
        .cloned()
        .unwrap_or_default();
    assert_eq!(
        winner_sha.len(),
        40,
        "the speculation winner's verified event must carry a real 40-hex worktree_sha, not \
         empty - it must be stamped AFTER a re-assert restores whatever the exhaustive gate \
         just deleted, not a snapshot taken during the deletion window: {winner_sha:?}"
    );
    assert!(
        winner_sha.chars().all(|c| c.is_ascii_hexdigit()),
        "the stamped sha must be real hex: {winner_sha:?}"
    );
    let head_after = git_out(&wt_dir, &["rev-parse", "HEAD"])
        .expect("the restored worktree must resolve its own HEAD");
    assert_eq!(
        winner_sha, head_after,
        "the stamped sha must be the RESTORED tree's actual HEAD"
    );

    assert!(
        !events
            .iter()
            .any(|e| e.type_ == rigger::ledger::TYPE_UNIT_FAILED
                || e.type_ == rigger::ledger::TYPE_UNIT_ESCALATED),
        "the self-heal must land cleanly - no failed or escalated unit"
    );
}

/// Spec 64 criterion 3, adjudication round 4 (`arch-u3c3r4-speculation-reject-sha-unguarded`,
/// UPHELD; fixed round 5 with a pre-read `ensure_present()` call in
/// `record_speculation_reject`). Mirrors
/// `run_end_to_end_restores_a_worktree_a_reviewer_agent_deletes_mid_review` above for the
/// SPECULATION candidate's review-reject arm: the implementer's own regression
/// (`conductor::tests::speculation_reject_worktree_sha_is_stamped_after_the_adjudicators_own_deletion_is_restored`)
/// proves it purely in-process against a `Stub` driver; this drives it through the real
/// compiled binary instead.
///
/// A `speculation_width: 2` unit with NO lenses (isolating this from the concurrent-lens
/// mechanism the sibling test above already covers) whose adjudicator ALWAYS rejects AND
/// deletes its own `$PWD` wholesale as a side effect before reporting - a real subprocess
/// spawn deleting the worktree BETWEEN `review_unit`'s pre-spawn re-assert and
/// `record_speculation_reject`'s `head_sha_of` read, the exact window round 5 closes. Both
/// candidates lose (the same adjudicator persona rejects lane 0 AND lane 1), so the unit
/// escalates - a legitimate terminal fixpoint, not a run failure (mirrors
/// `step_carries_the_escalated_set_when_a_fixpoint_is_reached_with_a_wedged_unit`'s own
/// exit-0-on-escalation contract).
#[test]
fn speculation_reject_worktree_sha_is_stamped_after_the_adjudicators_own_deletion_is_restored_end_to_end(
) {
    use std::os::unix::fs::PermissionsExt;

    use rigger::eventstore::namespace::Namespaced;
    use rigger::eventstore::sqlite::Store;
    use rigger::eventstore::{Direction, EventStore};

    let dir = temp_git_project_with_commit();
    let root = dir.path();
    let rigger = root.join(".rigger");
    std::fs::create_dir_all(rigger.join("agents")).unwrap();
    std::fs::write(
        rigger.join("agents").join("worker.md"),
        "---\nid: worker\nmodel: sonnet\ntools: [Read, Edit]\n---\nRIGGERTEST_WORKER: do the \
         unit.\n",
    )
    .unwrap();
    std::fs::write(
        rigger.join("agents").join("judge.md"),
        "---\nid: judge\nmodel: sonnet\ntools: [Read]\n---\nRIGGERTEST_ADJUDICATOR_REJECT_DELETE: \
         adjudicate it.\n",
    )
    .unwrap();
    std::fs::write(
        rigger.join("workflow.yml"),
        r#"name: ensureonparkspeculationrejecttest
defaults:
  grounder: nop
  budget: 60
gates:
  ok: { run: "true" }
stages:
  solo:
    agent: worker
    gates: [ok]
    on_pass: merge
    speculation_width: 2
    review:
      adjudicator: judge
"#,
    )
    .unwrap();

    let fakebin = tempfile::tempdir().unwrap();
    let claude_path = fakebin.path().join("claude");
    std::fs::write(
        &claude_path,
        r#"#!/bin/sh
sp=""
next=0
for a in "$@"; do
  if [ "$next" = "1" ]; then
    sp="$a"
    next=0
  fi
  if [ "$a" = "--system-prompt" ]; then
    next=1
  fi
done
case "$sp" in
  *RIGGERTEST_ADJUDICATOR_REJECT_DELETE*)
    d="$(pwd)"
    cd / || exit 1
    rm -rf "$d"
    if [ -d "$d" ]; then echo present >> "$RIGGERTEST_MARKER"; else echo absent >> "$RIGGERTEST_MARKER"; fi
    echo '{"verdict":"reject"}'
    ;;
  *RIGGERTEST_WORKER*)
    echo "pub fn work() {}" > work.rs
    ;;
  *)
    echo "fake-claude: unrecognized system prompt: $sp" 1>&2
    exit 1
    ;;
esac
"#,
    )
    .unwrap();
    let mut perms = std::fs::metadata(&claude_path).unwrap().permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(&claude_path, perms).unwrap();

    let marker = root.join("adjudicator-deleted-marker.txt");
    let path_env = format!(
        "{}:{}",
        fakebin.path().display(),
        std::env::var("PATH").unwrap_or_default()
    );

    let (out, err, ok) = run_rigger_envs(
        root,
        &["run"],
        &[
            ("PATH", &path_env),
            ("RIGGERTEST_MARKER", marker.to_str().unwrap()),
        ],
    );
    assert!(
        ok,
        "an all-candidates-rejected speculation group must still reach a clean escalated \
         fixpoint (exit 0), not a hard failure; stderr: {err}\nstdout: {out}"
    );

    // Non-vacuity: the adjudicator's own real subprocess really did delete its candidate's
    // worktree wholesale, self-reported from a location outside the worktree the deletion
    // itself never touches - once per candidate (both lane 0 and lane 1 reject).
    let marker_content = std::fs::read_to_string(&marker).unwrap_or_default();
    let deletions: Vec<&str> = marker_content.lines().collect();
    assert!(
        deletions.len() >= 2 && deletions.iter().all(|l| l.trim() == "absent"),
        "premise: the adjudicator's own process must actually have removed each candidate's \
         worktree wholesale (one per lane), or this test proves nothing about a restore: \
         {marker_content:?}"
    );

    let backend = Store::open(root.join(".rigger").join("events.db").to_str().unwrap()).unwrap();
    let store = Namespaced::new(&backend, &run_stream_identity(root));
    let events = store
        .read_stream(rigger::conductor::STREAM, 0, Direction::Forward)
        .unwrap();

    // The group really escalated (both candidates lost) - the terminal state this test's
    // premise depends on, not a run that silently found some other way to "succeed".
    assert!(
        events
            .iter()
            .any(|e| e.type_ == rigger::ledger::TYPE_UNIT_ESCALATED),
        "premise: an always-rejecting adjudicator across both speculation candidates must \
         escalate the unit, or this test proves nothing about the reject arm; events: {events:?}"
    );

    // At least one speculation-candidate reject carries a real 40-hex worktree sha - stamped
    // AFTER a re-assert restored what the adjudicator's own spawn had just deleted, never a
    // snapshot taken during the deletion window.
    let status_key = format!(
        "\"status\":\"{}\"",
        rigger::conductor::STATUS_SPECULATION_REJECTED
    );
    let reject_with_sha = events.iter().find(|e| {
        e.type_ == rigger::ledger::TYPE_UNIT_STATUS
            && String::from_utf8_lossy(&e.data).contains(&status_key)
            && e.meta
                .get(rigger::conductor::META_WORKTREE_SHA)
                .is_some_and(|s| s.len() == 40 && s.chars().all(|c| c.is_ascii_hexdigit()))
    });
    assert!(
        reject_with_sha.is_some(),
        "a speculation-candidate review-reject must carry a real 40-hex worktree sha even \
         when the adjudicator's own spawn deleted the tree as its side effect - it must be \
         stamped AFTER a re-assert restores it, not a snapshot taken during the deletion \
         window; events: {events:?}"
    );
}

/// Spec 64, criterion 3, adjudication round 5 finding
/// `sdet-u3c3r5-resumed-reviewed-gate-failure-failed-sha-still-empty-sentinel` (UPHELD; fixed
/// round 6 with the identical one-line guard already used at six sibling sites - `if let
/// Some(w) = wt { w.ensure_present()?; }` - immediately before the `failed_sha` read at
/// `run_single_stage`'s `ResumePhase::Reviewed` exhaustive-gate-FAILURE arm).
///
/// A unit whose PRIOR window recorded `reviewed` (the adjudicator already approved it; only
/// the merge was interrupted) resumes straight to the integrate door on the VERY NEXT step,
/// skipping implement and review entirely - so this arm is reached with NO agent spawn at all
/// in this process, purely from a real git-backed unit branch (the prior window's durable
/// checkpoint) plus a seeded `UnitStatus{"status":"reviewed"}` event standing in for the
/// interrupted window's own recorded verdict. A throwaway first step BOOTSTRAPS the run (mints
/// the real `RunStarted` every fold scopes through, and creates the unit's real worktree/branch
/// at their deterministic path) - the seed then lands AFTER that boundary, in the SAME slice
/// `resume_phase` folds, exactly like a real interrupted window's residue. Its resumed
/// exhaustive re-gate (spec 12, unit 3: "done" is measured against the exhaustive suite even
/// on a resumed approve) is real wall-clock time and the last thing that touches the worktree
/// before the read this round guards - its own `sh -c` command deletes the worktree wholesale
/// as a side effect, then reports FAIL (a real `rm -rf` followed by `exit 1`). Before round 6,
/// `head_sha_of` read the now-missing dir and silently stamped an empty sentinel, the same
/// class every sibling in this unit was rejected over.
#[test]
fn resumed_reviewed_unit_stamps_a_real_failed_sha_after_the_exhaustive_gates_own_deletion_is_restored(
) {
    use rigger::eventstore::namespace::Namespaced;
    use rigger::eventstore::sqlite::Store;
    use rigger::eventstore::{Direction, EventStore};

    let dir = temp_git_project_with_commit();
    let root = dir.path();
    let rigger = root.join(".rigger");
    std::fs::create_dir_all(rigger.join("agents")).unwrap();
    std::fs::write(
        rigger.join("agents").join("worker.md"),
        "---\nid: worker\nmodel: sonnet\ntools: [Read, Edit]\n---\nDo the unit.\n",
    )
    .unwrap();

    // A marker OUTSIDE the worktree self-reports whether the gate's own `rm -rf` really ran -
    // the non-vacuity check a single opaque subprocess call otherwise denies an outside
    // observer.
    let marker = rigger
        .join("tmp")
        .join("resumed-gate-fail-deleted-marker.txt");
    let marker_str = marker.to_str().unwrap();
    std::fs::write(
        rigger.join("workflow.yml"),
        format!(
            r#"name: ensureonparkresumedgatefailtest
defaults:
  grounder: nop
  budget: 60
gates:
  ok:
    run: 'd=$(pwd); cd / && rm -rf "$d"; ( [ -d "$d" ] && echo present || echo absent ) > "{marker}"; exit 1'
stages:
  solo:
    agent: worker
    gates: [ok]
"#,
            marker = marker_str
        ),
    )
    .unwrap();

    // Step 1 (bootstrap): mints the run's `RunStarted` and creates the unit's real,
    // git-backed worktree/branch at their deterministic path. Its own parked implementer is
    // never resulted - it is simply abandoned, standing in for the prior window this test's
    // premise depends on (a window whose OWN later steps carried it to `reviewed` and then
    // died before the merge).
    let (out, err, ok) = run_rigger(root, &["step"]);
    assert!(ok, "the bootstrap step must succeed; stderr: {err}");
    assert!(
        out.contains(r#""id":"solo/implementer#0""#),
        "the bootstrap step must park the implementer, creating the unit's worktree; got: \
         {out:?}"
    );
    let wt_dir = root.join(".rigger").join("tmp").join("rigger-wt-solo");
    assert!(
        wt_dir.exists(),
        "premise: the bootstrap step must already have created the unit's worktree: {}",
        wt_dir.display()
    );

    // The prior window's own committed work, written directly into the ALREADY-CREATED
    // worktree and committed - the durable checkpoint a real interrupted window leaves on the
    // unit's branch.
    std::fs::write(wt_dir.join("work.rs"), "pub fn work() {}\n").unwrap();
    git_ok(&wt_dir, &["add", "-A"]);
    git_ok(
        &wt_dir,
        &[
            "commit",
            "-q",
            "-m",
            "prior window: implemented and reviewed",
        ],
    );
    // Captured NOW, while the worktree is known to exist: `run_stage`'s own caller removes a
    // unit's worktree DIR (never its branch) on any TERMINAL, non-parked return - including
    // the `UnitFailed` this test drives - so the dir this test's own commit landed in will
    // itself be gone again by the time the step below returns. The branch is the durable
    // checkpoint; this sha is what a re-`ensure_present` checks the SAME branch back out to.
    let expected_sha = git_out(&wt_dir, &["rev-parse", "HEAD"])
        .expect("the committed worktree must resolve its own HEAD");

    // The prior window's own recorded verdict: the unit is `reviewed`, only the merge is
    // outstanding. Seeded AFTER the bootstrap step's `RunStarted`, so it lands in the SAME
    // slice `resume_phase` folds (`current_run` scopes to the suffix from the latest
    // `RunStarted` onward) - together with the committed branch above (`branch_has_work`),
    // this is everything `resume_phase` needs to route the very next step straight into
    // `ResumePhase::Reviewed`, with no implementer or review spawn in that process at all.
    seed_run_events(
        root,
        &[("UnitStatus", r#"{"id":"solo","status":"reviewed"}"#)],
    );

    let (out, err, ok) = run_rigger(root, &["step"]);
    assert!(
        ok,
        "a resumed unit whose exhaustive re-gate goes red must still complete the step \
         cleanly (a recorded UnitFailed under bounded remediation - never a hard process \
         failure); stderr: {err}\nstdout: {out}"
    );

    // Non-vacuity: the gate's own command really did remove the worktree wholesale.
    let marker_content = std::fs::read_to_string(&marker).unwrap_or_default();
    assert_eq!(
        marker_content.trim(),
        "absent",
        "premise: the exhaustive gate's own rm -rf must actually have removed the worktree \
         wholesale, or this test proves nothing about a restore: {marker_content:?}"
    );

    let backend = Store::open(root.join(".rigger").join("events.db").to_str().unwrap()).unwrap();
    let store = Namespaced::new(&backend, &run_stream_identity(root));
    let events = store
        .read_stream(rigger::conductor::STREAM, 0, Direction::Forward)
        .unwrap();
    let failed = events
        .iter()
        .find(|e| e.type_ == rigger::ledger::TYPE_UNIT_FAILED)
        .expect("the resumed unit's red exhaustive re-gate must record a UnitFailed");
    let failed_sha = failed
        .meta
        .get(rigger::conductor::META_WORKTREE_SHA)
        .cloned()
        .unwrap_or_default();
    assert_eq!(
        failed_sha.len(),
        40,
        "the failed event's worktree_sha must be a real 40-hex sha, not the empty sentinel a \
         read taken during the deletion window would silently stamp: {failed_sha:?}"
    );
    assert!(
        failed_sha.chars().all(|c| c.is_ascii_hexdigit()),
        "the stamped sha must be real hex: {failed_sha:?}"
    );
    assert_eq!(
        failed_sha, expected_sha,
        "the stamped sha must be the RESTORED tree's actual HEAD, not a snapshot taken during \
         the deletion window"
    );
    assert!(
        !events
            .iter()
            .any(|e| e.type_ == rigger::ledger::TYPE_UNIT_INTEGRATED),
        "a red exhaustive re-gate on resume must never integrate"
    );
}

/// Spec 64, criterion 3, adjudication round 5 finding
/// `adv-u3c3r5-two-more-unguarded-empty-sha-siblings` (UPHELD; fixed round 6 with the
/// identical guard at `run_single_stage`'s `ResumePhase::Reviewed` `integration.blocked` arm).
///
/// The mirror image of the sibling test above: this time the resumed unit's exhaustive
/// re-gate PASSES, so `run_single_stage` reaches `integrate_and_emit`, which merges the
/// branch into the base repo and then re-gates the MERGED tree (spec 12, unit 5,
/// `GateSelection::PostMerge`, run against `self.deps.repo` - a DIFFERENT directory than the
/// unit worktree). An out-of-band actor deleting the unit worktree during that real wall-clock
/// window is invisible to `integrate_and_emit`'s own internal re-assert (which ran BEFORE the
/// post-merge re-gate, over the pre-merge tree) - so nothing protects the `failed_sha` read on
/// this specific `integration.blocked` arm without round 6's fix. The gate command here is
/// unscoped (no `inputs`), so it genuinely runs twice at two distinct verdict keys - once
/// against the worktree (the exhaustive check, passes) and once against the merged base repo
/// (the post-merge re-gate, fails and deletes the worktree as its side effect) - never a
/// fabricated in-memory deletion. The base repo gets an unrelated commit of its own between
/// the bootstrap and the resume, so the merge is a genuine three-way merge rather than a
/// fast-forward - a fast-forward's merged tree is byte-identical to the worktree's own
/// pre-merge tree, which would CACHE-HIT the exhaustive check's green verdict (spec 12, unit
/// 1's content-address cache) and never run the post-merge command at all.
#[test]
fn resumed_reviewed_unit_stamps_a_real_failed_sha_after_the_post_merge_re_gates_own_deletion_is_restored(
) {
    use rigger::eventstore::namespace::Namespaced;
    use rigger::eventstore::sqlite::Store;
    use rigger::eventstore::{Direction, EventStore};

    let dir = temp_git_project_with_commit();
    let root = dir.path();
    let rigger = root.join(".rigger");
    std::fs::create_dir_all(rigger.join("agents")).unwrap();
    std::fs::write(
        rigger.join("agents").join("worker.md"),
        "---\nid: worker\nmodel: sonnet\ntools: [Read, Edit]\n---\nDo the unit.\n",
    )
    .unwrap();

    // The unit's own deterministic worktree dir, computed the SAME way `rigger step` itself
    // computes it - known up front so the gate script below can target it by an absolute
    // path, exactly the "an out-of-band actor deletes the worktree" shape this arm guards
    // against (never the gate deleting its OWN cwd, since the post-merge re-gate's cwd is the
    // base repo, a different directory entirely).
    let wt_dir = root.join(".rigger").join("tmp").join("rigger-wt-solo");
    let wt_dir_str = wt_dir.to_str().unwrap();
    // A flag OUTSIDE the worktree that survives its deletion: the gate's first real
    // invocation (the pre-merge exhaustive check, run in the worktree) passes and sets it;
    // its second real invocation (the post-merge re-gate, run in the base repo - a distinct
    // verdict key, never cache-answered) finds it set, deletes the worktree, and fails.
    let flag = rigger.join("tmp").join("postmerge-resumed-flag");
    let flag_str = flag.to_str().unwrap();
    let marker = rigger
        .join("tmp")
        .join("postmerge-resumed-deleted-marker.txt");
    let marker_str = marker.to_str().unwrap();
    std::fs::write(
        rigger.join("workflow.yml"),
        format!(
            r#"name: ensureonparkresumedpostmergetest
defaults:
  grounder: nop
  budget: 60
gates:
  ok:
    run: 'if [ -f "{flag}" ]; then rm -rf "{wtdir}"; ( [ -d "{wtdir}" ] && echo present || echo absent ) > "{marker}"; exit 1; else touch "{flag}"; fi'
stages:
  solo:
    agent: worker
    gates: [ok]
"#,
            flag = flag_str,
            wtdir = wt_dir_str,
            marker = marker_str
        ),
    )
    .unwrap();

    // Step 1 (bootstrap): mints the run's `RunStarted` and creates the unit's real,
    // git-backed worktree/branch at their deterministic path. Its own parked implementer is
    // never resulted - it is simply abandoned, standing in for the prior window this test's
    // premise depends on (a window whose OWN later steps carried it to `reviewed` and then
    // died before the merge).
    let (out, err, ok) = run_rigger(root, &["step"]);
    assert!(ok, "the bootstrap step must succeed; stderr: {err}");
    assert!(
        out.contains(r#""id":"solo/implementer#0""#),
        "the bootstrap step must park the implementer, creating the unit's worktree; got: \
         {out:?}"
    );
    assert!(
        wt_dir.exists(),
        "premise: the bootstrap step must already have created the unit's worktree: {}",
        wt_dir.display()
    );

    // The prior window's own committed work, written directly into the ALREADY-CREATED
    // worktree and committed - the durable checkpoint a real interrupted window leaves on the
    // unit's branch.
    std::fs::write(wt_dir.join("work.rs"), "pub fn work() {}\n").unwrap();
    git_ok(&wt_dir, &["add", "-A"]);
    git_ok(
        &wt_dir,
        &[
            "commit",
            "-q",
            "-m",
            "prior window: implemented and reviewed",
        ],
    );
    // Captured NOW, while the worktree is known to exist: `run_stage`'s own caller removes a
    // unit's worktree DIR (never its branch) on any TERMINAL, non-parked return - including
    // the `UnitFailed` this test drives - so the dir this test's own commit landed in will
    // itself be gone again by the time the step below returns. The branch is the durable
    // checkpoint; this sha is what a re-`ensure_present` checks the SAME branch back out to.
    let expected_sha = git_out(&wt_dir, &["rev-parse", "HEAD"])
        .expect("the committed worktree must resolve its own HEAD");

    // Advance the BASE repo's own checkout with an unrelated commit, so the unit's merge is a
    // genuine three-way merge (both sides added a distinct file since their common ancestor)
    // rather than a fast-forward. This is load-bearing for the content-address cache (spec 12,
    // unit 1): a fast-forward merge's tree is byte-identical to the worktree's own pre-merge
    // tree, so the post-merge re-gate would CACHE-HIT the exhaustive check's green verdict
    // (same command, same tree digest) and never run the command a second time at all - never
    // exercising the arm this test targets. A real divergence gives the merged tree its own
    // distinct digest, forcing the post-merge re-gate to run for real.
    std::fs::write(root.join("base-advanced.txt"), "unrelated base commit\n").unwrap();
    // Add ONLY the new file, never `-A`: the unit's own worktree is a nested git checkout
    // under `.rigger/tmp/`, and a broad `add -A` at the base repo's root would stage it as an
    // embedded repository (a gitlink), corrupting the very tree this test drives a merge over.
    git_ok(root, &["add", "base-advanced.txt"]);
    git_ok(root, &["commit", "-q", "-m", "unrelated base advance"]);

    // The prior window's own recorded verdict: the unit is `reviewed`, only the merge is
    // outstanding. Seeded AFTER the bootstrap step's `RunStarted`, so it lands in the SAME
    // slice `resume_phase` folds.
    seed_run_events(
        root,
        &[("UnitStatus", r#"{"id":"solo","status":"reviewed"}"#)],
    );

    let (out, err, ok) = run_rigger(root, &["step"]);
    assert!(
        ok,
        "a resumed unit whose post-merge re-gate goes red must still complete the step \
         cleanly (a rolled-back merge and a recorded UnitFailed - never a hard process \
         failure); stderr: {err}\nstdout: {out}"
    );

    // Non-vacuity: the post-merge re-gate's own command really did remove the worktree
    // wholesale.
    let marker_content = std::fs::read_to_string(&marker).unwrap_or_default();
    assert_eq!(
        marker_content.trim(),
        "absent",
        "premise: the post-merge re-gate's own rm -rf must actually have removed the \
         worktree wholesale, or this test proves nothing about a restore: {marker_content:?}"
    );

    // The merge was rolled back (spec 12, unit 5): the base repo's own working tree must NOT
    // carry the file a landed integration would have.
    assert!(
        !root.join("work.rs").exists(),
        "a RED post-merge re-gate must roll the merge back - the file must never land in the \
         base repo's working tree"
    );

    let backend = Store::open(root.join(".rigger").join("events.db").to_str().unwrap()).unwrap();
    let store = Namespaced::new(&backend, &run_stream_identity(root));
    let events = store
        .read_stream(rigger::conductor::STREAM, 0, Direction::Forward)
        .unwrap();
    let failed = events
        .iter()
        .find(|e| e.type_ == rigger::ledger::TYPE_UNIT_FAILED)
        .expect("the resumed unit's red post-merge re-gate must record a UnitFailed");
    let failed_sha = failed
        .meta
        .get(rigger::conductor::META_WORKTREE_SHA)
        .cloned()
        .unwrap_or_default();
    assert_eq!(
        failed_sha.len(),
        40,
        "the failed event's worktree_sha must be a real 40-hex sha, not the empty sentinel a \
         read taken during the deletion window would silently stamp: {failed_sha:?}"
    );
    assert!(
        failed_sha.chars().all(|c| c.is_ascii_hexdigit()),
        "the stamped sha must be real hex: {failed_sha:?}"
    );
    assert_eq!(
        failed_sha, expected_sha,
        "the stamped sha must be the RESTORED tree's actual HEAD, not a snapshot taken during \
         the deletion window"
    );
    assert!(
        !events
            .iter()
            .any(|e| e.type_ == rigger::ledger::TYPE_UNIT_INTEGRATED),
        "a rolled-back post-merge re-gate on resume must never integrate"
    );
}

/// Spec 64, criterion 3, adjudication round 5 finding
/// `adv-u3c3r5-two-more-unguarded-empty-sha-siblings` (UPHELD; fixed round 6 with
/// `candidates[i].wt.ensure_present()` immediately before the `failed_sha` read at
/// `run_speculation`'s own `integration.blocked` arm).
///
/// The THIRD sibling round 6 closes, on the SPECULATION surface this time: a
/// `speculation_width: 2` unit whose winning candidate's post-merge re-gate goes red. The
/// exhaustive post-merge re-gate that produces `blocked` here runs against `self.deps.repo`
/// (the base repo), never re-touching `candidates[i].wt.dir` - so `integrate_and_emit`'s own
/// internal re-assert (which ran BEFORE that re-gate, over the pre-merge worktree) cannot
/// cover this read either. The unscoped `ok` gate's FIRST real invocation (candidate 0's
/// pre-merge narrowed check) passes and arms a flag; every real invocation after that -
/// candidate 0's own post-merge re-gate, and (a candidate 1 that also reaches its own
/// post-merge door) candidate 1's - finds the flag armed, deletes candidate 0's worktree, and
/// fails, so this drives the target arm at least once with no candidate ever winning: the
/// group exhausts both candidates and ESCALATES, a legitimate terminal fixpoint (mirrors
/// `speculation_reject_worktree_sha_is_stamped_after_the_adjudicators_own_deletion_is_restored_end_to_end`'s
/// own exit-0-on-escalation contract above), driven through the real, subprocess-per-spawn
/// `rigger run` so both candidates' implementer and adjudicator spawns are real, synchronous
/// subprocesses in the SAME process - never a fabricated in-memory deletion.
#[test]
fn run_speculation_stamps_a_real_failed_sha_after_the_post_merge_re_gates_own_deletion_is_restored()
{
    use std::os::unix::fs::PermissionsExt;

    use rigger::eventstore::namespace::Namespaced;
    use rigger::eventstore::sqlite::Store;
    use rigger::eventstore::{Direction, EventStore};

    let dir = temp_git_project_with_commit();
    let root = dir.path();
    let rigger = root.join(".rigger");
    std::fs::create_dir_all(rigger.join("agents")).unwrap();
    std::fs::write(
        rigger.join("agents").join("worker.md"),
        "---\nid: worker\nmodel: sonnet\ntools: [Read, Edit]\n---\nRIGGERTEST_WORKER: do the \
         unit.\n",
    )
    .unwrap();
    std::fs::write(
        rigger.join("agents").join("judge.md"),
        "---\nid: judge\nmodel: sonnet\ntools: [Read]\n---\nRIGGERTEST_ADJUDICATOR: adjudicate \
         it.\n",
    )
    .unwrap();

    // Candidate 0 uses the unit's CANONICAL deterministic worktree/branch (the same dir a
    // single-lane unit would use) - known up front so the gate script can target it directly,
    // the "out-of-band actor deletes the worktree" shape this arm guards against.
    let wt_dir0 = root.join(".rigger").join("tmp").join("rigger-wt-solo");
    let wt_dir0_str = wt_dir0.to_str().unwrap();
    let flag = rigger.join("tmp").join("spec-postmerge-flag");
    let flag_str = flag.to_str().unwrap();
    let marker = root.join("spec-postmerge-deleted-marker.txt");
    let marker_str = marker.to_str().unwrap();
    // Candidate 1's own implementer (see the fake `claude` below) advances the BASE repo with
    // an unrelated commit of its own as a side effect, once candidate 0's already ran - this
    // is load-bearing for the content-address cache (spec 12, unit 1): candidate 0's merge
    // would otherwise be a clean FAST-FORWARD (nothing else ever touches the base repo), whose
    // merged tree is byte-identical to its own pre-merge worktree tree - a digest match that
    // would CACHE-HIT the narrowed check's green verdict and never run the post-merge command
    // at all. Phase A completes both candidates' implementer spawns before Phase B evaluates
    // either one, so this advance is already in place by the time candidate 0 reaches its
    // merge.
    let root_str = root.to_str().unwrap();
    let lane0_marker = rigger.join("tmp").join("spec-lane0-implemented-marker");
    let lane0_marker_str = lane0_marker.to_str().unwrap();
    std::fs::write(
        rigger.join("workflow.yml"),
        format!(
            r#"name: ensureonparkspeculationpostmergetest
defaults:
  grounder: nop
  budget: 60
gates:
  ok:
    run: 'if [ -f "{flag}" ]; then rm -rf "{wtdir0}"; ( [ -d "{wtdir0}" ] && echo present || echo absent ) > "{marker}"; exit 1; else touch "{flag}"; fi'
stages:
  solo:
    agent: worker
    gates: [ok]
    speculation_width: 2
    review:
      adjudicator: judge
"#,
            flag = flag_str,
            wtdir0 = wt_dir0_str,
            marker = marker_str
        ),
    )
    .unwrap();

    let fakebin = tempfile::tempdir().unwrap();
    let claude_path = fakebin.path().join("claude");
    std::fs::write(
        &claude_path,
        format!(
            r#"#!/bin/sh
sp=""
next=0
for a in "$@"; do
  if [ "$next" = "1" ]; then
    sp="$a"
    next=0
  fi
  if [ "$a" = "--system-prompt" ]; then
    next=1
  fi
done
case "$sp" in
  *RIGGERTEST_ADJUDICATOR*)
    echo '{{"verdict":"approve"}}'
    ;;
  *RIGGERTEST_WORKER*)
    echo "pub fn work() {{}}" > work.rs
    if [ -f "{lane0_marker}" ]; then
      echo "unrelated base commit" > "{root_dir}/base-advanced.txt"
      git -C "{root_dir}" add base-advanced.txt
      git -C "{root_dir}" commit -q -m "unrelated base advance (lane 1 side effect)"
    else
      touch "{lane0_marker}"
    fi
    ;;
  *)
    echo "fake-claude: unrecognized system prompt: $sp" 1>&2
    exit 1
    ;;
esac
"#,
            lane0_marker = lane0_marker_str,
            root_dir = root_str,
        ),
    )
    .unwrap();
    let mut perms = std::fs::metadata(&claude_path).unwrap().permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(&claude_path, perms).unwrap();

    let path_env = format!(
        "{}:{}",
        fakebin.path().display(),
        std::env::var("PATH").unwrap_or_default()
    );

    let (out, err, ok) = run_rigger_envs(root, &["run"], &[("PATH", &path_env)]);
    assert!(
        ok,
        "a speculation group whose only winnable merges break post-merge must still reach a \
         clean escalated fixpoint (exit 0), not a hard failure; stderr: {err}\nstdout: {out}"
    );

    // Non-vacuity: the post-merge re-gate's own command really did remove candidate 0's
    // worktree wholesale at least once.
    let marker_content = std::fs::read_to_string(&marker).unwrap_or_default();
    assert_eq!(
        marker_content.trim(),
        "absent",
        "premise: the post-merge re-gate's own rm -rf must actually have removed the \
         candidate worktree wholesale, or this test proves nothing about a restore: \
         {marker_content:?}"
    );

    let backend = Store::open(root.join(".rigger").join("events.db").to_str().unwrap()).unwrap();
    let store = Namespaced::new(&backend, &run_stream_identity(root));
    let events = store
        .read_stream(rigger::conductor::STREAM, 0, Direction::Forward)
        .unwrap();

    // The group really escalated (no candidate's merge survived its post-merge re-gate) - the
    // terminal state this test's premise depends on.
    assert!(
        events
            .iter()
            .any(|e| e.type_ == rigger::ledger::TYPE_UNIT_ESCALATED),
        "premise: a speculation group whose every winnable merge breaks post-merge must \
         escalate the unit, or this test proves nothing about the blocked arm; events: \
         {events:?}"
    );
    assert!(
        !events
            .iter()
            .any(|e| e.type_ == rigger::ledger::TYPE_UNIT_INTEGRATED),
        "no candidate may integrate when every merge broke post-merge"
    );

    // Every UnitFailed this run recorded (candidate 0's post-merge block, and candidate 1's
    // own if it also reached the same door) carries a real 40-hex worktree sha - stamped
    // AFTER a re-assert restores what the post-merge re-gate's own spawn just deleted, never
    // a snapshot taken during the deletion window. At least candidate 0's own (attempts:1)
    // must be present.
    let failed: Vec<_> = events
        .iter()
        .filter(|e| e.type_ == rigger::ledger::TYPE_UNIT_FAILED)
        .collect();
    assert!(
        !failed.is_empty(),
        "the blocked post-merge merge(s) must record at least one UnitFailed; events: \
         {events:?}"
    );
    assert!(
        failed
            .iter()
            .any(|e| String::from_utf8_lossy(&e.data).contains(r#""attempts":1"#)),
        "candidate 0's own post-merge block must record a UnitFailed at attempts:1; events: \
         {events:?}"
    );
    for e in &failed {
        let sha = e
            .meta
            .get(rigger::conductor::META_WORKTREE_SHA)
            .cloned()
            .unwrap_or_default();
        assert_eq!(
            sha.len(),
            40,
            "every post-merge-blocked UnitFailed's worktree_sha must be a real 40-hex sha, \
             not the empty sentinel a read taken during the deletion window would silently \
             stamp: {sha:?} event={e:?}"
        );
        assert!(
            sha.chars().all(|c| c.is_ascii_hexdigit()),
            "the stamped sha must be real hex: {sha:?}"
        );
    }
}

/// Spec 50, criterion 2 (the REGISTRY lifecycle): `rigger step` REGISTERS this instance in the
/// machine-global state directory - the project root plus a CREDENTIAL-FREE store identity, with a
/// live heartbeat - so a machine-level dash can DISCOVER it without a coordination protocol. The
/// state dir is redirected into a temp `XDG_STATE_HOME` so the test never touches the real
/// `~/.local/state`; the entry is read back through the `rigger::registry` reader (the same reader
/// the dash uses) to assert the local store identity, a live heartbeat, and no credential on disk.
/// A second step refreshes the SAME entry in place - the registry never piles up duplicates.
#[test]
fn step_registers_the_instance_in_the_machine_global_registry() {
    use rigger::registry;

    let dir = temp_git_project_with_commit();
    let root = dir.path();
    write_two_stage_workflow(root);

    // Redirect the machine-global state dir into a temp home, so the registry lands under the
    // test's own tree instead of the operator's real ~/.local/state.
    let state = tempfile::tempdir().unwrap();
    let xdg = state.path().to_str().unwrap();

    let (_out, err, ok) = run_rigger_envs(root, &["step"], &[("XDG_STATE_HOME", xdg)]);
    assert!(ok, "step must succeed; stderr: {err}");

    // The dash's own reader returns exactly this instance (a fresh heartbeat, so nothing prunes).
    let regdir = registry::instances_dir(state.path());
    let live = registry::read_live(&regdir, registry::now_ms(), registry::DEFAULT_IDLE_MS);
    assert_eq!(
        live.len(),
        1,
        "exactly one instance is registered; got {live:?}"
    );
    let inst = &live[0];

    // The registered store identity is the LOCAL sqlite log - credential-free by construction.
    match &inst.store {
        registry::StoreIdentity::Local { path } => assert!(
            path.ends_with(".rigger/events.db"),
            "the local store identity is the project's events.db; got {path}"
        ),
        other => panic!("a local run registers a Local store identity; got {other:?}"),
    }
    assert!(!inst.root.is_empty(), "the project root is recorded");
    assert!(inst.heartbeat_ms > 0, "a live heartbeat is stamped");

    // The on-disk entry carries NO connection string at all (the secrets-discipline invariant for
    // a local run), and its file name is the deterministic id.
    let entry = regdir.join(format!("{}.json", inst.id()));
    let body = std::fs::read_to_string(&entry).unwrap();
    assert!(
        !body.contains("://"),
        "a local registry entry holds no connection credential; got {body}"
    );

    // A second step refreshes the SAME entry rather than accumulating a duplicate.
    let (_o, err2, ok2) = run_rigger_envs(root, &["step"], &[("XDG_STATE_HOME", xdg)]);
    assert!(ok2, "the second step must succeed; stderr: {err2}");
    let again = registry::read_live(&regdir, registry::now_ms(), registry::DEFAULT_IDLE_MS);
    assert_eq!(
        again.len(),
        1,
        "a re-step refreshes one entry in place; got {again:?}"
    );
}

/// Spec 50, criterion 2 on the IN-PROCESS run drivers + the SECRETS invariant on the SERVER arm: a
/// native `rigger run` drives the WHOLE run in-process (a single `conductor::run`), NOT through the
/// stepwise loop, so it must register its instance at its OWN call site (`run_cli`) - a missing call
/// here is an independent boundary bug the `rigger step` test cannot catch. And when the run reports
/// to a SHARED server, the persisted store identity must be CREDENTIAL-FREE: the exact end-to-end
/// Server-arm coverage the module unit tests cannot give (they never drive the binary's wiring).
///
/// Drives `rigger run` against a well-formed but UNREACHABLE server URL whose userinfo AND query
/// hide a credential (nothing listens on this loopback port, so the eager connect is refused fast).
/// `--base HEAD` resolves in the committed repo, so the run clears its base/anchor gates and reaches
/// the register + store-open seam; it then FAILS at connect - AFTER `run_cli` has registered. The
/// registry is redirected into a temp `XDG_STATE_HOME` and read back through the dash's own reader:
/// exactly one Shared entry, its endpoint the bare `scheme://host:port`, with NO credential fragment
/// anywhere on disk. A regression that registered only from `rigger step` finds zero entries here; a
/// regression that persisted the raw connection string finds `admin`/`hunter2` on disk.
#[test]
fn run_registers_a_credential_free_shared_instance() {
    use rigger::registry;

    let dir = temp_git_project_with_commit();
    let root = dir.path();
    write_two_stage_workflow(root);

    // Redirect the machine-global state dir into a temp home (never the operator's ~/.local/state).
    let state = tempfile::tempdir().unwrap();
    let xdg = state.path().to_str().unwrap();

    // A credential in the userinfo, plus a query, on an unreachable loopback port so the eager
    // connect is refused fast (the port pattern the crate's own tests use for "nothing listens").
    let conn = "kurrentdb://admin:hunter2@127.0.0.1:65533?tls=false";
    let (_out, err, ok) = run_rigger_envs(
        root,
        &["run", "--base", "HEAD", "--conn", conn],
        &[("XDG_STATE_HOME", xdg)],
    );
    // The run itself fails at store-open (the server is unreachable) - expected. The assertion is on
    // the registration side effect that fired BEFORE that failure, and on stderr never leaking the
    // credential (the store-open error redacts the conn through the same single authority).
    assert!(
        !ok,
        "the unreachable server makes the run fail at store-open (expected); stderr: {err}"
    );
    assert!(
        !err.contains("admin") && !err.contains("hunter2"),
        "the store-open error must redact the credential, never echo it; stderr: {err}"
    );

    // The dash's own reader returns exactly this instance (a fresh heartbeat, so nothing prunes).
    let regdir = registry::instances_dir(state.path());
    let live = registry::read_live(&regdir, registry::now_ms(), registry::DEFAULT_IDLE_MS);
    assert_eq!(
        live.len(),
        1,
        "a native `rigger run` registers its instance too (not only `rigger step`); got {live:?}"
    );
    let inst = &live[0];
    assert!(inst.heartbeat_ms > 0, "a live heartbeat is stamped");

    // The Server arm persisted the CREDENTIAL-FREE endpoint: scheme + host:port only, no userinfo,
    // no query - the single redaction authority (`eventstore::endpoint_label`) ran over the conn.
    match &inst.store {
        registry::StoreIdentity::Shared { endpoint } => assert_eq!(
            endpoint, "kurrentdb://127.0.0.1:65533",
            "the shared store identity is the bare scheme://host:port"
        ),
        other => panic!("a --conn run registers a Shared store identity; got {other:?}"),
    }

    // The secrets invariant, end to end: NO credential or query fragment reaches the on-disk entry.
    let entry = regdir.join(format!("{}.json", inst.id()));
    let body = std::fs::read_to_string(&entry).unwrap();
    for secret in ["admin", "hunter2", "tls=false"] {
        assert!(
            !body.contains(secret),
            "no credential/query fragment ({secret:?}) may reach the registry entry; got {body}"
        );
    }
}

/// Spec 50, criterion 2 on the THIRD registration path - the in-process SERVED conductor
/// (`rigger serve` / `rigger run --driver workflow`, i.e. `run_workflow`) - plus the SECRETS
/// invariant on its Server arm. `run_workflow` drives the whole run in-process on a background
/// thread while it serves the MCP bridge, so - exactly like `run_cli` and unlike the stepwise
/// loop - it must register its instance at its OWN call site. That call site is DISTINCT from the
/// two the sibling tests cover (`cmd_step`, `run_cli`): a regression that dropped
/// `register_run_instance` from `run_workflow` alone (keeping it in `run_cli`) stays green in
/// `run_registers_a_credential_free_shared_instance` yet finds ZERO entries here. This closes the
/// "wire ALL THREE paths" seam, the last of which no other test drives.
///
/// Drives `rigger run --driver workflow` against a well-formed but UNREACHABLE server URL whose
/// userinfo AND query hide a credential. `run_workflow` registers BEFORE it opens the store, so the
/// registration side effect fires and THEN the eager connect is refused fast (nothing listens on
/// this loopback port) - the run returns the store-open error before ever reaching its MCP serve
/// loop, so the invocation terminates without a live server. The registry is redirected into a temp
/// `XDG_STATE_HOME` and read back through the dash's own reader: exactly one Shared entry, its
/// endpoint the bare `scheme://host:port`, with NO credential fragment anywhere on disk.
#[test]
fn run_driver_workflow_registers_a_credential_free_shared_instance() {
    use rigger::registry;

    let dir = temp_git_project_with_commit();
    let root = dir.path();
    write_two_stage_workflow(root);

    // Redirect the machine-global state dir into a temp home (never the operator's ~/.local/state).
    let state = tempfile::tempdir().unwrap();
    let xdg = state.path().to_str().unwrap();

    // A credential in the userinfo, plus a query, on an unreachable loopback port so the eager
    // connect is refused fast. A distinct port from the sibling test's, purely for readability
    // (both merely connect to a dead port, so they could never collide).
    let conn = "kurrentdb://admin:hunter2@127.0.0.1:65532?tls=false";
    let (_out, err, ok) = run_rigger_envs(
        root,
        &[
            "run", "--driver", "workflow", "--base", "HEAD", "--conn", conn,
        ],
        &[("XDG_STATE_HOME", xdg)],
    );
    // The served path fails at store-open (the server is unreachable) - expected. The assertion is
    // on the registration side effect that fired BEFORE that failure, and on stderr never leaking
    // the credential (the store-open error redacts the conn through the same single authority).
    assert!(
        !ok,
        "the unreachable server makes the served run fail at store-open (expected); stderr: {err}"
    );
    assert!(
        !err.contains("admin") && !err.contains("hunter2"),
        "the store-open error must redact the credential, never echo it; stderr: {err}"
    );

    // The dash's own reader returns exactly this instance (a fresh heartbeat, so nothing prunes).
    let regdir = registry::instances_dir(state.path());
    let live = registry::read_live(&regdir, registry::now_ms(), registry::DEFAULT_IDLE_MS);
    assert_eq!(
        live.len(),
        1,
        "the served path (`rigger run --driver workflow`) registers its instance too, at its own \
         call site distinct from `rigger step` and plain `rigger run`; got {live:?}"
    );
    let inst = &live[0];
    assert!(inst.heartbeat_ms > 0, "a live heartbeat is stamped");

    // The Server arm persisted the CREDENTIAL-FREE endpoint: scheme + host:port only, no userinfo,
    // no query - the single redaction authority (`eventstore::endpoint_label`) ran over the conn.
    match &inst.store {
        registry::StoreIdentity::Shared { endpoint } => assert_eq!(
            endpoint, "kurrentdb://127.0.0.1:65532",
            "the shared store identity is the bare scheme://host:port"
        ),
        other => panic!("a --conn served run registers a Shared store identity; got {other:?}"),
    }

    // The secrets invariant, end to end: NO credential or query fragment reaches the on-disk entry.
    let entry = regdir.join(format!("{}.json", inst.id()));
    let body = std::fs::read_to_string(&entry).unwrap();
    for secret in ["admin", "hunter2", "tls=false"] {
        assert!(
            !body.contains(secret),
            "no credential/query fragment ({secret:?}) may reach the registry entry; got {body}"
        );
    }
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

/// Spec 47 - KurrentDB is always available (the CLI/binary edge). Before spec 47 the
/// DEFAULT build answered `rigger run --eventstore kurrentdb` with a recompile-required
/// dead end ("requires the `kurrentdb` cargo feature"); the adapter was gated behind a
/// build-time flag. Now it is compiled into EVERY build, so the SAME command in the
/// default binary reaches the real adapter and fails only for the reason a server-backed
/// store legitimately can: no connection string. This drives the COMPILED binary (the
/// consumer's exact command) to prove a RUNTIME flag - not a recompile - selects the
/// shared backend. It is the outside-in proof the binary's internal `open_store` unit
/// test cannot give: that the `--eventstore kurrentdb` flag is wired to the adapter and
/// surfaces the right error. Ungated, so it runs in both feature lanes' `cargo test`.
#[test]
fn run_eventstore_kurrentdb_reaches_the_adapter_not_a_missing_feature_dead_end() {
    let dir = temp_git_project_with_commit();
    let root = dir.path();
    write_two_stage_workflow(root);

    // Drive `rigger run --eventstore kurrentdb` with NO connection string. `--base HEAD`
    // resolves in the committed repo, so the run clears its base/anchor gates and reaches
    // the store-open seam (`open_store`). KURRENTDB_CONN is REMOVED from the child so the
    // missing-connection guard - not an eager connect attempt against a leaked url - is the
    // path under test. RIGGER_NO_DASH keeps the step path from spawning a real dashboard
    // (spec 39); the failure fires before the dashboard would start anyway.
    let out = common::rigger_courier()
        .args(["run", "--base", "HEAD", "--eventstore", "kurrentdb"])
        .current_dir(root)
        // Redirect the machine-global registry (spec 50, criterion 2) into the test's own temp
        // tree so any registration side effect lands under `root/rigger`, never the operator's
        // real ~/.local/state/rigger/instances. Every direct spawn of a registering command
        // (run/serve/step) that cannot go through the sandboxed `run_rigger_envs` sets this.
        .env("XDG_STATE_HOME", root)
        .env("RIGGER_NO_DASH", "1")
        .env_remove("KURRENTDB_CONN")
        .output()
        .expect("failed to spawn the rigger binary");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    let combined = format!("{stdout}{stderr}");

    assert!(
        !out.status.success(),
        "kurrentdb with no connection string must fail; stdout: {stdout:?} stderr: {stderr:?}"
    );
    // It reaches the REAL adapter's missing-connection guard, which names the
    // --conn / KURRENTDB_CONN channel - proving the compiled binary reached the adapter and
    // the runtime flag selected it (not a config/base failure short of the store seam).
    assert!(
        combined.contains("--conn") || combined.contains("KURRENTDB_CONN"),
        "the failure must be the missing-connection error, proving the default binary reached the \
         adapter via the runtime flag; got stdout: {stdout:?} stderr: {stderr:?}"
    );
    // And NEVER the retired recompile-required dead end: the adapter is always compiled in,
    // so no "requires the cargo feature" / "-F kurrentdb" message can occur in any build.
    assert!(
        !combined.contains("feature"),
        "the retired missing-feature dead end must be gone (spec 47): the default binary must not \
         tell a consumer to recompile with a cargo feature; got stdout: {stdout:?} stderr: {stderr:?}"
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

/// Spec 51, criterion 1 (REVIEWER ERROR RE-PARK) at the BINARY boundary: a REVIEW-stage
/// spawn whose RECORDED result is a PLAIN error - NOT a liveness fault - (an externally-killed
/// reviewer: quota exhaustion, a crash, whose error the death courier recorded on its id via
/// `rigger result <id> --error`) is an INFRASTRUCTURE fault, not a verdict. An error is not a
/// verdict: review must COMPLETE. So the next `rigger step` must NOT adopt the error as the
/// review outcome and must NOT halt; it charges the unit NO attempt (the work was never judged)
/// and RE-PARKS a FRESH attempt of the SAME review (`~retry1`) in the printed wave. A completed
/// REAL verdict on the re-parked spawn then folds through the NORMAL adjudication path to
/// `reviewed`. Driven end to end through the built binary (`rigger step` / `rigger result`) -
/// the SEAM the implementer's in-process ReplayDriver unit test cannot reach: this proves the
/// re-park is observable at the true external boundary, and (with its liveness-fault sibling
/// below) that ONLY a plain recorded error re-parks.
#[test]
fn a_plain_error_on_a_review_spawn_re_parks_a_fresh_attempt_then_a_real_verdict_folds() {
    use rigger::eventstore::namespace::Namespaced;
    use rigger::eventstore::sqlite::Store;
    use rigger::eventstore::{Direction, EventStore};

    let dir = temp_repoless_project();
    let root = dir.path();
    write_gated_reviewed_workflow(root);

    // Read the run stream exactly as production does, to assert the unit was charged no attempt.
    let read_run_stream = || -> Vec<rigger::eventstore::Event> {
        let backend =
            Store::open(root.join(".rigger").join("events.db").to_str().unwrap()).unwrap();
        let store = Namespaced::new(&backend, &run_stream_identity(root));
        store
            .read_stream(rigger::conductor::STREAM, 0, Direction::Forward)
            .unwrap()
    };

    // Step 1: the implementer parks; drain it with a real success through the courier CLI.
    let (out, err, ok) = run_rigger(root, &["step"]);
    assert!(ok, "the first step must succeed; stderr: {err}");
    assert!(
        out.contains(r#""id":"solo/implementer#0""#) && out.contains(r#""done":false"#),
        "step 1 parks the implementer; got: {out:?}"
    );
    let (_o, err, ok) = run_rigger(
        root,
        &["result", "solo/implementer#0", "implemented the unit"],
    );
    assert!(
        ok,
        "recording the implementer result must succeed; stderr: {err}"
    );

    // Step 2: the implementer replays, the unit gates green, and the review parks the
    // ADJUDICATOR (this workflow's only review-tier spawn).
    let (out, err, ok) = run_rigger(root, &["step"]);
    assert!(ok, "the second step must succeed; stderr: {err}");
    assert!(
        out.contains(r#""id":"solo/adjudicator#0""#) && out.contains(r#""done":false"#),
        "step 2 gates the unit and parks the adjudicator; got: {out:?}"
    );

    // The death courier records a PLAIN error on the adjudicator (a reviewer killed mid-run:
    // usage-limit exhaustion) via `rigger result <id> --error` with NO liveness meta - the exact
    // signal `review_spawn_errored` keys on: a recorded error that is NOT a liveness fault.
    let (_o, err, ok) = run_rigger(
        root,
        &[
            "result",
            "solo/adjudicator#0",
            "agent killed mid-run: usage limit reached",
            "--error",
        ],
    );
    assert!(
        ok,
        "recording the reviewer's plain error must succeed; stderr: {err}"
    );

    // Step 3: the errored adjudicator RE-PARKS a FRESH `~retry1` attempt instead of halting or
    // adopting the error as a verdict - and charges the unit NO attempt.
    let (out, err, ok) = run_rigger(root, &["step"]);
    assert!(ok, "the re-park step must succeed; stderr: {err}");
    assert!(
        out.contains(r#""id":"solo/adjudicator#0~retry1""#),
        "an errored review re-parks a FRESH ~retry1 attempt in the wave; got: {out:?}"
    );
    assert!(
        !out.contains(r#""halted":"#),
        "a plain review error is an infra fault, not a halt: review must COMPLETE; got: {out:?}"
    );
    let events = read_run_stream();
    assert!(
        !events
            .iter()
            .any(|e| e.type_ == rigger::ledger::TYPE_UNIT_FAILED),
        "an errored review charges the unit no remediation attempt (no UnitFailed)"
    );
    assert!(
        !events
            .iter()
            .any(|e| e.type_ == rigger::ledger::TYPE_UNIT_ESCALATED),
        "an errored review never escalates the unit (no UnitEscalated)"
    );

    // A COMPLETED real verdict on the re-parked spawn folds through the NORMAL adjudication path.
    let (_o, err, ok) = run_rigger(
        root,
        &[
            "result",
            "solo/adjudicator#0~retry1",
            r#"{"verdict":"approve"}"#,
        ],
    );
    assert!(
        ok,
        "recording the re-parked adjudicator's approve must succeed; stderr: {err}"
    );

    // Step 4: everything replays to a fixpoint - the re-parked approve resolves the review, the
    // unit reaches `reviewed`, and (on_pass: none) the run converges. Still no attempt charged.
    let (out, err, ok) = run_rigger(root, &["step"]);
    assert!(ok, "the fold-through step must succeed; stderr: {err}");
    assert!(
        out.contains(r#""wave":[]"#)
            && out.contains(r#""done":true"#)
            && !out.contains(r#""halted":"#),
        "the real verdict on the re-parked spawn folds to a clean fixpoint; got: {out:?}"
    );
    let events = read_run_stream();
    assert!(
        events
            .iter()
            .any(|e| e.type_ == rigger::ledger::TYPE_UNIT_STATUS
                && String::from_utf8_lossy(&e.data).contains("reviewed")),
        "the re-parked real verdict folds through to a `reviewed` unit status"
    );
    assert!(
        !events
            .iter()
            .any(|e| e.type_ == rigger::ledger::TYPE_UNIT_FAILED),
        "recovery still charges the unit no remediation attempt"
    );

    // `rigger stats` confirms the review-verdict section folded exactly the re-parked approve
    // (1 approved) - the errored first attempt contributed no verdict, as an infra fault should.
    let (stats, err, ok) = run_rigger(root, &["stats"]);
    assert!(
        ok,
        "stats over the recovered run must succeed; stderr: {err}"
    );
    let review_line = stats
        .lines()
        .find(|l| l.contains("review"))
        .unwrap_or_else(|| panic!("the review section must appear; got:\n{stats}"));
    assert!(
        review_line.contains("1 approved"),
        "the review section records exactly the re-parked adjudicator's approve; got line: {review_line:?}"
    );
}

/// Spec 51, criterion 1's EXCLUSION at the binary boundary: a REVIEW-stage spawn whose recorded
/// result is a LIVENESS fault (a HUNG reviewer - `rigger result <id> --error --meta
/// '{"liveness_class":"infra"}'`, the shape the driver/sweep records for a stalled agent) is NOT
/// re-parked by the reviewer-error path. A hung reviewer has its OWN re-park-then-loud-halt path
/// (the replay driver re-parks its id; `rigger step` surfaces it via `hung_spawns`), so the
/// criterion-1 re-park must never ALSO swallow it. This proves the `!is_liveness_fault()`
/// exclusion (and the `is_parked` short-circuit that precedes the re-park branch) holds for a
/// review-tier spawn: a hung adjudicator HALTS LOUDLY, it does not silently re-park a fresh
/// `~retry1`. Only a PLAIN recorded error re-parks (its sibling above); a liveness fault stays a
/// loud halt - the two together prove the re-park is scoped to exactly the plain-error signal.
#[test]
fn a_liveness_fault_on_a_review_spawn_halts_instead_of_re_parking() {
    let dir = temp_repoless_project();
    let root = dir.path();
    write_gated_reviewed_workflow(root);

    // Steps 1-2: drive the unit through its implementer to park the ADJUDICATOR review spawn.
    let (out, err, ok) = run_rigger(root, &["step"]);
    assert!(
        ok && out.contains(r#""id":"solo/implementer#0""#),
        "step 1 parks the implementer; stderr: {err}; got: {out:?}"
    );
    let (_o, err, ok) = run_rigger(
        root,
        &["result", "solo/implementer#0", "implemented the unit"],
    );
    assert!(
        ok,
        "recording the implementer result must succeed; stderr: {err}"
    );
    let (out, err, ok) = run_rigger(root, &["step"]);
    assert!(
        ok && out.contains(r#""id":"solo/adjudicator#0""#),
        "step 2 parks the adjudicator; stderr: {err}; got: {out:?}"
    );

    // The adjudicator HANGS: the driver/sweep records a LIVENESS fault on its id (infra), NOT a
    // plain error - exactly the shape `step_surfaces_a_hung_unbounded_spawn...` records for a
    // stalled agent. This is the case the criterion-1 re-park path must EXCLUDE.
    let (_o, err, ok) = run_rigger(
        root,
        &[
            "result",
            "solo/adjudicator#0",
            "reviewer solo/adjudicator#0 hung: heartbeat marker went stale past its bound",
            "--error",
            "--meta",
            r#"{"liveness_class":"infra"}"#,
        ],
    );
    assert!(
        ok,
        "recording the reviewer's liveness fault must succeed; stderr: {err}"
    );

    // Step 3: `rigger step` HALTS LOUDLY on the hung reviewer - it is NOT re-parked as a fresh
    // `~retry1` attempt. The halt names the spawn, states infra, and charges no attempt.
    let (out, err, ok) = run_rigger(root, &["step"]);
    assert!(
        ok,
        "a liveness-halted step still prints its result and exits 0; stderr: {err}"
    );
    assert!(
        out.contains(r#""halted":"#) && out.contains("solo/adjudicator#0"),
        "a hung reviewer surfaces as a loud halt naming it; got: {out:?}"
    );
    assert!(
        out.contains("infra") && out.contains("no remediation attempt"),
        "the halt states infra classification and no-attempt-charged; got: {out:?}"
    );
    assert!(
        !out.contains("~retry1"),
        "a liveness fault is EXCLUDED from the criterion-1 re-park: no fresh ~retry1 review \
         attempt is parked for a hung reviewer; got: {out:?}"
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

// ---------------------------------------------------------------------------
// `rigger validate` build.wrapper resolution (spec 65 unit 2, NO SILENT DEGRADE)
// ---------------------------------------------------------------------------

/// Set a test's `build:` block on a scaffolded project's `.rigger/workflow.yml`, REPLACING
/// any `build:` the scaffold already wrote rather than blindly appending a second one:
/// `rigger init` (spec 65 unit 5) now scaffolds `build:\n  wrapper: auto\n` on every fresh
/// project, so a bare append would leave two top-level `build:` keys - a YAML parse error
/// ("duplicate field `build`"), not the single resolved block each test below means to
/// exercise.
fn append_build_block(root: &Path, block: &str) {
    let path = root.join(".rigger").join("workflow.yml");
    let existing = std::fs::read_to_string(&path).unwrap();
    let mut without_build = String::new();
    let mut in_build = false;
    for line in existing.lines() {
        if line == "build:" {
            in_build = true;
            continue;
        }
        if in_build {
            if line.trim().is_empty() || line.starts_with(' ') || line.starts_with('\t') {
                continue; // still inside the scaffolded build: block (or its trailing blank)
            }
            in_build = false;
        }
        without_build.push_str(line);
        without_build.push('\n');
    }
    std::fs::write(&path, without_build).unwrap();

    use std::io::Write;
    let mut wf = std::fs::OpenOptions::new()
        .append(true)
        .open(&path)
        .unwrap();
    writeln!(wf, "{block}").unwrap();
}

/// The single directory that provides `bin` on the REAL `PATH`, panicking if `bin` cannot
/// be found anywhere on it (a precondition of the tests below, not something they mean to
/// exercise).
fn real_path_dir_of(bin: &str) -> String {
    std::env::var("PATH")
        .unwrap_or_default()
        .split(':')
        .find(|dir| !dir.is_empty() && Path::new(dir).join(bin).exists())
        .map(|s| s.to_string())
        .unwrap_or_else(|| panic!("{bin} must be resolvable on the real PATH for this test"))
}

/// A minimal synthetic `PATH` carrying only what `rigger validate` itself needs (`git`, for
/// its drift/residue advisories) - an ALLOWLIST, not a denylist, so it can never
/// accidentally strip a directory `rigger validate` needs while still guaranteeing NEITHER
/// known build-cache wrapper (`sccache`/`ccache`) is reachable, regardless of what the real
/// machine running this test happens to have installed (some systems co-locate `ccache`
/// with `git` in the same `/usr/bin`, which a directory-denylist filter could not tell
/// apart).
fn path_with_no_known_wrapper() -> String {
    real_path_dir_of("git")
}

/// [`path_with_no_known_wrapper`] with a fake `name` executable staged in a fresh bin dir
/// under `root` and prepended, so `name` resolves unambiguously as the ONLY wrapper-shaped
/// binary on this synthetic `PATH`.
fn path_with_fake_wrapper(root: &Path, name: &str) -> String {
    let bindir = root.join("fake-wrapper-bin");
    std::fs::create_dir_all(&bindir).unwrap();
    let bin = bindir.join(name);
    std::fs::write(&bin, "#!/bin/sh\nexit 0\n").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    format!("{}:{}", bindir.display(), path_with_no_known_wrapper())
}

/// A CONFIGURED (non-auto, non-off) `build.wrapper` absent from PATH fails `rigger
/// validate` at run start (`config::load`'s `Config::validate` call), naming both the
/// missing binary and the `build.wrapper` config key - a configured-explicit failure,
/// never a silent degrade. Uses the real ambient PATH (the fake name is virtually certain
/// to be absent from it), so no synthetic PATH is needed for this direction.
#[test]
fn validate_fails_at_run_start_when_a_named_build_wrapper_is_absent_from_path() {
    let dir = temp_project();
    let root = dir.path();
    let (_out, err, ok) = run_rigger(root, &["init"]);
    assert!(ok, "rigger init must succeed; stderr:\n{err}");
    append_build_block(
        root,
        "build:\n  wrapper: definitely-not-a-real-wrapper-rigger-cli-test\n",
    );

    let (out, err, ok) = run_rigger(root, &["validate"]);
    assert!(
        !ok,
        "a named-but-absent build.wrapper must fail validate (run start); \
         stdout:\n{out}\nstderr:\n{err}"
    );
    assert!(
        err.contains("definitely-not-a-real-wrapper-rigger-cli-test"),
        "the failure must name the missing binary; stderr:\n{err}"
    );
    assert!(
        err.contains("build.wrapper"),
        "the failure must name the config key; stderr:\n{err}"
    );
}

/// `build.wrapper: auto` finding NO known wrapper on PATH must never fail validate (a
/// discovered-implicit degrade, not a configured-explicit failure) and must report "none"
/// through `rigger validate`'s output - so a silently-skipped cache layer is SEEN, not
/// invisible.
#[test]
fn validate_reports_none_when_auto_finds_no_known_wrapper_on_path() {
    let dir = temp_project();
    let root = dir.path();
    let (_out, err, ok) = run_rigger(root, &["init"]);
    assert!(ok, "rigger init must succeed; stderr:\n{err}");
    append_build_block(root, "build:\n  wrapper: auto\n");

    let path = path_with_no_known_wrapper();
    let (out, err, ok) = run_rigger_envs(root, &["validate"], &[("PATH", &path)]);
    assert!(
        ok,
        "auto finding nothing must never fail validate; stdout:\n{out}\nstderr:\n{err}"
    );
    assert!(
        out.lines().any(|l| l == "build wrapper: none"),
        "auto with no known wrapper on PATH must report none through validate; \
         stdout:\n{out}"
    );
}

/// `build.wrapper: auto` finding a known wrapper on PATH resolves and reports its name
/// through `rigger validate`'s output.
#[test]
fn validate_reports_the_resolved_wrapper_when_auto_finds_a_known_wrapper_on_path() {
    let dir = temp_project();
    let root = dir.path();
    let (_out, err, ok) = run_rigger(root, &["init"]);
    assert!(ok, "rigger init must succeed; stderr:\n{err}");
    append_build_block(root, "build:\n  wrapper: auto\n");

    let path = path_with_fake_wrapper(root, "sccache");
    let (out, err, ok) = run_rigger_envs(root, &["validate"], &[("PATH", &path)]);
    assert!(
        ok,
        "a found wrapper must not fail validate; stdout:\n{out}\nstderr:\n{err}"
    );
    assert!(
        out.lines().any(|l| l == "build wrapper: sccache"),
        "auto finding sccache on PATH must report it through validate; stdout:\n{out}"
    );
}

/// Spec 65 unit 5 (HONEST SURFACES), end to end through the real CLI: with a wrapper
/// active AND a custom `max_concurrent`, `rigger validate` reports the wrapper, the cache
/// dir it resolved to, AND the resolved budget - all three, not just the wrapper the
/// earlier (spec 65 unit 2) test above already covers.
#[test]
fn validate_reports_cache_dir_and_budget_alongside_the_wrapper() {
    let dir = temp_project();
    let root = dir.path();
    let (_out, err, ok) = run_rigger(root, &["init"]);
    assert!(ok, "rigger init must succeed; stderr:\n{err}");
    let cache_dir = root.join("my-cache");
    append_build_block(
        root,
        &format!(
            "build:\n  wrapper: auto\n  cache_dir: {}\n  max_concurrent: 7\n",
            cache_dir.display()
        ),
    );

    let path = path_with_fake_wrapper(root, "sccache");
    let (out, err, ok) = run_rigger_envs(root, &["validate"], &[("PATH", &path)]);
    assert!(
        ok,
        "a found wrapper with a custom budget must not fail validate; \
         stdout:\n{out}\nstderr:\n{err}"
    );
    assert!(
        out.lines()
            .any(|l| l == format!("build cache dir: {}", cache_dir.display())),
        "the resolved cache dir must be reported; stdout:\n{out}"
    );
    assert!(
        out.lines().any(|l| l == "build budget: 7"),
        "the resolved max_concurrent budget must be reported; stdout:\n{out}"
    );
}

/// With the wrapper layer off, `rigger validate` still reports the budget (it gates every
/// compiler invocation regardless of the wrapper) but NO cache dir line - an inactive
/// layer touches no cache dir, so a claimed one would be fabricated.
#[test]
fn validate_reports_budget_but_no_cache_dir_when_the_wrapper_is_off() {
    let dir = temp_project();
    let root = dir.path();
    let (_out, err, ok) = run_rigger(root, &["init"]);
    assert!(ok, "rigger init must succeed; stderr:\n{err}");
    append_build_block(root, "build:\n  wrapper: off\n  max_concurrent: 2\n");

    let (out, err, ok) = run_rigger(root, &["validate"]);
    assert!(ok, "wrapper: off must not fail validate; stderr:\n{err}");
    assert!(
        out.lines().any(|l| l == "build wrapper: none"),
        "an off wrapper reports none; stdout:\n{out}"
    );
    assert!(
        out.lines().any(|l| l == "build budget: 2"),
        "the budget is still reported with the wrapper off; stdout:\n{out}"
    );
    assert!(
        !out.lines().any(|l| l.starts_with("build cache dir:")),
        "an off wrapper must report no cache dir line at all; stdout:\n{out}"
    );
}

/// SDET periphery (spec 65 unit 5, HONEST SURFACES): every wrapper-resolution test above
/// OVERWRITES the scaffolded `build:` block with its own hand-typed one (via
/// `append_build_block`) before running `validate` - none of them proves the real seam
/// between `rigger init` WRITING the scaffold and `rigger validate` READING it. This test
/// leaves a genuinely fresh `rigger init` output - `build:\n  wrapper: auto\n`, and
/// nothing else - completely untouched, so it is the literal bytes the scaffold constant
/// puts on disk (not a hand-retyped equivalent that could silently drift from it) driving
/// resolution. It also pins the two defaults an unmodified `build:` section leaves
/// implicit and that no other CLI test asserts a value for: the cache dir line reads
/// `<state home>/rigger/build-cache` (`BuildConfig::cache_dir`'s own doc comment - empty
/// resolves to this, not a fabricated or blank value) and the budget line reads `4`
/// (`BuildConfig::max_concurrent`'s own doc comment - omitted resolves to 4, not the
/// in-memory `Default::default()` zero).
#[test]
fn a_fresh_scaffolded_init_resolves_and_reports_through_validate_untouched() {
    let dir = temp_project();
    let root = dir.path();
    let state_home = tempfile::tempdir().unwrap();
    let state_home_str = state_home.path().to_str().unwrap();

    let (_out, err, ok) = run_rigger_envs(root, &["init"], &[("XDG_STATE_HOME", state_home_str)]);
    assert!(ok, "rigger init must succeed; stderr:\n{err}");

    let workflow_path = root.join(".rigger").join("workflow.yml");
    let workflow = std::fs::read_to_string(&workflow_path).unwrap();
    assert!(
        workflow.contains("build:\n  wrapper: auto\n"),
        "a fresh scaffold must declare build:\\n  wrapper: auto verbatim, byte for byte, \
         with no other key implied; workflow.yml:\n{workflow}"
    );

    let path = path_with_fake_wrapper(root, "sccache");
    let (out, err, ok) = run_rigger_envs(
        root,
        &["validate"],
        &[("PATH", &path), ("XDG_STATE_HOME", state_home_str)],
    );
    assert!(
        ok,
        "an unmodified fresh scaffold must validate cleanly; stdout:\n{out}\nstderr:\n{err}"
    );
    assert!(
        out.lines().any(|l| l == "build wrapper: sccache"),
        "the untouched scaffold's auto wrapper must resolve against PATH and report the \
         name it found; stdout:\n{out}"
    );
    let expected_cache_dir = state_home.path().join("rigger").join("build-cache");
    assert!(
        out.lines()
            .any(|l| l == format!("build cache dir: {}", expected_cache_dir.display())),
        "the untouched scaffold's empty cache_dir must report the documented default \
         <state home>/rigger/build-cache; stdout:\n{out}"
    );
    assert!(
        out.lines().any(|l| l == "build budget: 4"),
        "an unmodified build: section (bare wrapper key only, max_concurrent omitted) \
         must report the documented default of 4, not the in-memory zero; stdout:\n{out}"
    );
}

/// A cache-dir path guaranteed to be uncreatable: `<root>/blocker` is a plain FILE, so
/// `create_dir_all("<root>/blocker/nested/cache")` fails because a path COMPONENT already
/// exists as a non-directory - deterministic on every OS/user (no root/permission tricks a
/// privileged test runner could bypass).
fn uncreatable_cache_dir(root: &Path) -> std::path::PathBuf {
    let blocker = root.join("blocker");
    std::fs::write(&blocker, "not a directory").unwrap();
    blocker.join("nested").join("cache")
}

/// A NAMED (non-auto) `build.wrapper` present on PATH but whose `build.cache_dir` cannot be
/// created is also a configured-explicit failure (specs/65:26-28 decides both failure
/// directions in the SAME Design sentence as the absent-binary case above) - `rigger
/// validate` must fail at run start naming the dir and the `build.cache_dir` config key,
/// never silently proceed with a cache that never actually writes anything.
#[test]
fn validate_fails_at_run_start_when_a_named_wrappers_cache_dir_cannot_be_created() {
    let dir = temp_project();
    let root = dir.path();
    let (_out, err, ok) = run_rigger(root, &["init"]);
    assert!(ok, "rigger init must succeed; stderr:\n{err}");
    let cache_dir = uncreatable_cache_dir(root);
    append_build_block(
        root,
        &format!(
            "build:\n  wrapper: sccache\n  cache_dir: {}\n",
            cache_dir.display()
        ),
    );

    let path = path_with_fake_wrapper(root, "sccache");
    let (out, err, ok) = run_rigger_envs(root, &["validate"], &[("PATH", &path)]);
    assert!(
        !ok,
        "a named wrapper's uncreatable cache dir must fail validate (run start); \
         stdout:\n{out}\nstderr:\n{err}"
    );
    assert!(
        err.contains(&cache_dir.to_string_lossy().into_owned()),
        "the failure must name the cache dir; stderr:\n{err}"
    );
    assert!(
        err.contains("build.cache_dir"),
        "the failure must name the config key; stderr:\n{err}"
    );
}

/// `build.wrapper: auto` finding a known wrapper on PATH but whose cache dir cannot be
/// created must never fail validate - a DISCOVERED-IMPLICIT degrade, mirroring auto finding
/// no wrapper binary at all - and must report "none" (the whole layer skipped), so an
/// operator SEES the cache is not actually live rather than trusting a resolved name that
/// silently never worked.
#[test]
fn validate_reports_none_when_autos_discovered_wrapper_has_an_uncreatable_cache_dir() {
    let dir = temp_project();
    let root = dir.path();
    let (_out, err, ok) = run_rigger(root, &["init"]);
    assert!(ok, "rigger init must succeed; stderr:\n{err}");
    let cache_dir = uncreatable_cache_dir(root);
    append_build_block(
        root,
        &format!(
            "build:\n  wrapper: auto\n  cache_dir: {}\n",
            cache_dir.display()
        ),
    );

    let path = path_with_fake_wrapper(root, "sccache");
    let (out, err, ok) = run_rigger_envs(root, &["validate"], &[("PATH", &path)]);
    assert!(
        ok,
        "auto's uncreatable cache dir must never fail validate; stdout:\n{out}\nstderr:\n{err}"
    );
    assert!(
        out.lines().any(|l| l == "build wrapper: none"),
        "auto with an uncreatable cache dir must report none (the whole layer skipped) \
         through validate; stdout:\n{out}"
    );
}

/// A cache-dir path guaranteed to EXIST but be UNWRITABLE: created first, then chmod'd
/// read+execute-only (0o555) - `create_dir_all` against it succeeds (a no-op against an
/// already-existing dir, regardless of write permission), but writing INTO it fails with
/// `PermissionDenied`. This is the realistic steady state for a persisted, shared cache dir
/// (the machine-wide `default_cache_dir` every project reuses after the first one creates
/// it) - unlike `uncreatable_cache_dir` above (a blocked path component), this is the only
/// way to make a directory that EXISTS yet cannot be written into. It is exactly the
/// sub-case `gate::ensure_cache_dir_writable` added on top of `uncreatable_cache_dir`'s
/// bare-`create_dir_all` check. Unix-only: the mode bits are a POSIX concept.
#[cfg(unix)]
fn preexisting_unwritable_cache_dir(root: &Path) -> std::path::PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let dir = root.join("preexisting-cache");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o555)).unwrap();
    dir
}

/// A NAMED (non-auto) `build.wrapper` present on PATH whose `build.cache_dir` ALREADY
/// EXISTS but is not WRITABLE is also a configured-explicit failure (spec 65 unit 2, NO
/// SILENT DEGRADE) - mirrors
/// `validate_fails_at_run_start_when_a_named_wrappers_cache_dir_cannot_be_created` above for
/// the writability rather than creatability failure mode: a pre-existing dir makes
/// `create_dir_all` alone a no-op success regardless of permission, so only a real write
/// probe catches this, and nothing before this test proved that probe's failure reaches the
/// real compiled binary's exit code. `rigger validate` must fail at run start naming the
/// dir and the `build.cache_dir` config key, never silently proceed with a cache that turns
/// out to never actually write anything.
#[cfg(unix)]
#[test]
fn validate_fails_at_run_start_when_a_named_wrappers_cache_dir_is_preexisting_but_unwritable() {
    let dir = temp_project();
    let root = dir.path();
    let (_out, err, ok) = run_rigger(root, &["init"]);
    assert!(ok, "rigger init must succeed; stderr:\n{err}");
    let cache_dir = preexisting_unwritable_cache_dir(root);
    append_build_block(
        root,
        &format!(
            "build:\n  wrapper: sccache\n  cache_dir: {}\n",
            cache_dir.display()
        ),
    );

    let path = path_with_fake_wrapper(root, "sccache");
    let (out, err, ok) = run_rigger_envs(root, &["validate"], &[("PATH", &path)]);
    assert!(
        !ok,
        "a named wrapper's pre-existing-but-unwritable cache dir must fail validate (run \
         start); stdout:\n{out}\nstderr:\n{err}"
    );
    assert!(
        err.contains(&cache_dir.to_string_lossy().into_owned()),
        "the failure must name the cache dir; stderr:\n{err}"
    );
    assert!(
        err.contains("build.cache_dir"),
        "the failure must name the config key; stderr:\n{err}"
    );
}

/// `build.wrapper: auto` finding a known wrapper on PATH but whose cache dir ALREADY EXISTS
/// yet is not WRITABLE must never fail validate - a DISCOVERED-IMPLICIT degrade, mirroring
/// `validate_reports_none_when_autos_discovered_wrapper_has_an_uncreatable_cache_dir` above
/// for the writability rather than creatability failure mode - and must report "none" (the
/// whole layer skipped), so an operator SEES the cache is not actually live rather than
/// trusting a resolved name that silently never worked.
#[cfg(unix)]
#[test]
fn validate_reports_none_when_autos_discovered_wrapper_has_a_preexisting_unwritable_cache_dir() {
    let dir = temp_project();
    let root = dir.path();
    let (_out, err, ok) = run_rigger(root, &["init"]);
    assert!(ok, "rigger init must succeed; stderr:\n{err}");
    let cache_dir = preexisting_unwritable_cache_dir(root);
    append_build_block(
        root,
        &format!(
            "build:\n  wrapper: auto\n  cache_dir: {}\n",
            cache_dir.display()
        ),
    );

    let path = path_with_fake_wrapper(root, "sccache");
    let (out, err, ok) = run_rigger_envs(root, &["validate"], &[("PATH", &path)]);
    assert!(
        ok,
        "auto's pre-existing-but-unwritable cache dir must never fail validate; \
         stdout:\n{out}\nstderr:\n{err}"
    );
    assert!(
        out.lines().any(|l| l == "build wrapper: none"),
        "auto with a pre-existing-but-unwritable cache dir must report none (the whole \
         layer skipped) through validate; stdout:\n{out}"
    );
}

// ---------------------------------------------------------------------------
// `rigger validate` build.mutation resolution (spec 73, ENABLED-BUT-ABSENT FAILS LOUD)
// ---------------------------------------------------------------------------

/// [`path_with_no_known_wrapper`] additionally VERIFIED to lack the mutation-efficacy
/// binary too: unlike the wrapper case (an operator-chosen name, so a nonsense string is
/// used against the real ambient PATH instead), `cargo-mutants`'s name is fixed and this
/// repo's own development environment genuinely has it installed - so the git-only
/// directory is the only way to exercise the absent-binary direction, and this asserts
/// (rather than merely reasons in a doc comment) that it really is absent, so a
/// coincidental co-location could never silently turn this into a false pass.
fn path_with_no_cargo_mutants() -> String {
    let dir = path_with_no_known_wrapper();
    assert!(
        !Path::new(&dir).join("cargo-mutants").exists(),
        "the git-only directory {dir:?} unexpectedly also carries a cargo-mutants binary; \
         this test needs a PATH that genuinely lacks the tool"
    );
    dir
}

/// A CONFIGURED `build.mutation: on` with no `cargo-mutants` resolvable on PATH fails
/// `rigger validate` at run start (`config::load`'s `Config::validate` call), naming both
/// the missing binary and the `build.mutation` config key - a configured-explicit failure,
/// never a silent skip (spec 73). Mirrors
/// `validate_fails_at_run_start_when_a_named_build_wrapper_is_absent_from_path` above.
#[test]
fn validate_fails_at_run_start_when_mutation_is_on_and_cargo_mutants_is_absent_from_path() {
    let dir = temp_project();
    let root = dir.path();
    let (_out, err, ok) = run_rigger(root, &["init"]);
    assert!(ok, "rigger init must succeed; stderr:\n{err}");
    append_build_block(root, "build:\n  mutation: on\n");

    let path = path_with_no_cargo_mutants();
    let (out, err, ok) = run_rigger_envs(root, &["validate"], &[("PATH", &path)]);
    assert!(
        !ok,
        "build.mutation: on with no cargo-mutants on PATH must fail validate (run start); \
         stdout:\n{out}\nstderr:\n{err}"
    );
    assert!(
        err.contains("cargo-mutants"),
        "the failure must name the missing binary; stderr:\n{err}"
    );
    assert!(
        err.contains("build.mutation"),
        "the failure must name the config key; stderr:\n{err}"
    );
}

/// `build.mutation: on` with `cargo-mutants` resolvable on PATH must not fail validate, and
/// `rigger validate` must report the resolved setting through its output.
#[test]
fn validate_reports_mutation_on_when_cargo_mutants_is_resolvable() {
    let dir = temp_project();
    let root = dir.path();
    let (_out, err, ok) = run_rigger(root, &["init"]);
    assert!(ok, "rigger init must succeed; stderr:\n{err}");
    append_build_block(root, "build:\n  mutation: on\n");

    let path = path_with_fake_wrapper(root, "cargo-mutants");
    let (out, err, ok) = run_rigger_envs(root, &["validate"], &[("PATH", &path)]);
    assert!(
        ok,
        "a resolvable cargo-mutants must not fail validate; stdout:\n{out}\nstderr:\n{err}"
    );
    assert!(
        out.lines().any(|l| l == "build mutation: on"),
        "a resolved-enabled mutation step must report on through validate; stdout:\n{out}"
    );
}

/// An EXPLICIT `build.mutation: off` must validate successfully and report "off" even with
/// `cargo-mutants` entirely absent from PATH - off never even probes PATH, so its absence
/// can never surface as a failure (spec 73).
#[test]
fn validate_reports_mutation_off_without_probing_path_when_configured_off() {
    let dir = temp_project();
    let root = dir.path();
    let (_out, err, ok) = run_rigger(root, &["init"]);
    assert!(ok, "rigger init must succeed; stderr:\n{err}");
    append_build_block(root, "build:\n  mutation: off\n");

    let path = path_with_no_cargo_mutants();
    let (out, err, ok) = run_rigger_envs(root, &["validate"], &[("PATH", &path)]);
    assert!(
        ok,
        "build.mutation: off must never fail validate regardless of PATH; \
         stdout:\n{out}\nstderr:\n{err}"
    );
    assert!(
        out.lines().any(|l| l == "build mutation: off"),
        "an explicitly-off mutation step must report off through validate; stdout:\n{out}"
    );
}

/// A fresh `rigger init` scaffold sets no `build.mutation` key at all, so it defaults to
/// off - back-compat with every workflow committed before this key existed - and `rigger
/// validate` reports that default through its output.
#[test]
fn validate_reports_mutation_off_by_default_on_a_fresh_scaffold() {
    let dir = temp_project();
    let root = dir.path();
    let (_out, err, ok) = run_rigger(root, &["init"]);
    assert!(ok, "rigger init must succeed; stderr:\n{err}");

    let (out, err, ok) = run_rigger(root, &["validate"]);
    assert!(
        ok,
        "a fresh scaffold must validate; stdout:\n{out}\nstderr:\n{err}"
    );
    assert!(
        out.lines().any(|l| l == "build mutation: off"),
        "an unconfigured mutation step must default to off, reported through validate; \
         stdout:\n{out}"
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

/// Spec 48 criterion 1 (single authority), applied to the `rigger validate` residue scan:
/// "a command invoked in a project configured for the server-backed store resolves THAT
/// store." The residue scan reads the CURRENT run's live-unit set to decide which
/// `rigger-wt-*`/`rigger/u/*` leftovers are LIVE (spared) versus residue (flagged), and that
/// read (`read_run_units`) walks the DURABLE real run stream - so it must resolve the
/// configured backend through the one authority, never hardcode local sqlite. This drives the
/// real binary from OUTSIDE, container-free, and DISCRIMINATES the two backends over the SAME
/// seeded state:
///
///   * a LOCAL sqlite run marks `liveunit` LIVE, and its worktree/branch exist on disk;
///   * UNCONFIGURED (sqlite default): the scan reads that local run, sees `liveunit` live, and
///     SPARES its leftovers - the control proving the local run IS read when sqlite is selected;
///   * SERVER-configured (`KURRENTDB_CONN` set, unreachable): the scan must resolve the SERVER,
///     NOT the local sqlite - the server holds no such run (and is down), so `liveunit` is not
///     live and its leftovers are FLAGGED as residue.
///
/// A regression that pins `StoreSelection::Sqlite` in `read_run_units` reads the LOCAL run in
/// BOTH cases, so the server case would wrongly spare `rigger-wt-liveunit` and this test's
/// server-arm assertion reddens. That pin is the exact anti-pattern spec 48 eradicates - a
/// command that ignores the configured store - surviving inside the residue scan.
#[test]
fn validate_residue_scan_resolves_the_configured_server_not_the_local_sqlite_run() {
    let dir = temp_git_project_with_commit();
    let root = dir.path();

    let (_out, err, ok) = run_rigger(root, &["init"]);
    assert!(ok, "rigger init must succeed; stderr:\n{err}");
    git_ok(root, &["add", "-A"]);
    git_ok(root, &["commit", "-q", "-m", "scaffold"]);
    seed_store(root);

    // A LOCAL sqlite run whose `liveunit` is in-flight (non-terminal) - the exact shape that,
    // when READ, spares its leftovers. The whole point is that a server-configured scan must
    // NOT read this.
    seed_run_events(
        root,
        &[
            ("RunStarted", r#"{"run":"r1","criteria":["current spec"]}"#),
            (
                "UnitStarted",
                r#"{"id":"liveunit","branch":"rigger/u/liveunit"}"#,
            ),
        ],
    );

    // The unit's deterministic worktree + local branch. Live => spared; not-live => residue.
    let scratch = root.join("scratchroot");
    let tmp = scratch.to_str().unwrap();
    std::fs::create_dir_all(scratch.join("rigger-wt-liveunit")).unwrap();
    std::fs::write(
        scratch.join("rigger-wt-liveunit").join("payload.bin"),
        [0u8; 4096],
    )
    .unwrap();
    git_ok(root, &["branch", "rigger/u/liveunit"]);

    // CONTROL - nothing configured, so the single authority defaults to the LOCAL sqlite log.
    // The scan reads the seeded run, sees `liveunit` live, and SPARES its leftovers. This proves
    // the local run genuinely IS read on the sqlite path, so the server-case difference below is
    // attributable to the store SELECTION, not to some unrelated reason the unit stays unflagged.
    let (_out, err, ok) = run_rigger_envs(root, &["validate"], &[("RIGGER_TMPDIR", tmp)]);
    assert!(
        ok,
        "validate only warns about residue, still exits 0; stderr:\n{err}"
    );
    assert!(
        !err.contains("rigger-wt-liveunit"),
        "with sqlite selected, the local run's live unit must be read and its worktree SPARED; \
         stderr:\n{err}"
    );
    assert!(
        !err.contains("rigger/u/liveunit"),
        "with sqlite selected, the local run's live branch must be SPARED; stderr:\n{err}"
    );

    // SERVER-configured via KURRENTDB_CONN (well-formed but unreachable: nothing listens on this
    // loopback port, so the eager connect is refused fast - we prove WHICH store the scan
    // resolved, not that a server is up). The residue scan must resolve the SERVER, which holds
    // no run, so `liveunit` is NOT live and its worktree/branch are FLAGGED. A hardcoded-sqlite
    // regression reads the LOCAL run here too and would wrongly spare them.
    let (_out, err, ok) = run_rigger_envs(
        root,
        &["validate"],
        &[
            ("RIGGER_TMPDIR", tmp),
            ("KURRENTDB_CONN", "kurrentdb://127.0.0.1:65533?tls=false"),
        ],
    );
    assert!(
        ok,
        "validate's residue scan is best-effort: an unreachable configured store degrades to no \
         live units, it never fails validate; stderr:\n{err}"
    );
    assert!(
        err.contains("rigger-wt-liveunit"),
        "server-configured: the scan must resolve the SERVER (not the local sqlite run), so the \
         local run's `liveunit` is NOT seen as live and its worktree is FLAGGED as residue; \
         stderr:\n{err}"
    );
    assert!(
        err.contains("rigger/u/liveunit"),
        "server-configured: the local run's branch must likewise be FLAGGED, proving the pinned \
         sqlite read is gone; stderr:\n{err}"
    );
}

/// Spec 48 "one resolution authority" loud-selection + spec 19c loud-failure-surfacing, applied to
/// the `rigger validate` residue scan on the different-user / permission edge §48 explicitly
/// contemplates: an unreadable `.rigger/store.conn` (a server-pinning box whose per-machine secret
/// file the invoking user cannot read) makes the ONE store-selection authority return an ERROR, not
/// a selection. The residue scan reads the CURRENT run's live-unit set THROUGH that authority
/// (`read_run_units`), so a genuine selection FAILURE off a PRESENT source must SURFACE loudly - it
/// must NOT silently degrade to the local sqlite default, which (finding zero live units) would
/// misreport every LIVE `rigger-wt-*` worktree and `rigger/u/*` branch as removable residue: a
/// destructive misdiagnosis on the exact edge §48 guards and `store_conn_file`'s own doc calls out.
///
/// This drives the OBSERVER path (`rigger validate`) end to end - the coverage the courier `?`-path
/// SDET periphery tests never reach - and DISCRIMINATES the fix from the silent-degrade regression:
///
///   * a regression that keeps `store_selection(None, None).unwrap_or(StoreSelection::Sqlite)` reads
///     the empty local sqlite, sees zero live units, FLAGS the on-disk `rigger-wt-liveunit` /
///     `rigger/u/liveunit` as residue, and exits 0 with no store-connection error - silent
///     wrong-store, the exact class `adj-u2r-precedence-reject` gated on the courier path;
///   * the fix propagates the selection error, so validate FAILS loudly naming the store-connection
///     read failure and never emits the false residue advisory.
///
/// The unreadable file is a DIRECTORY where `store.conn` goes (an IO error distinct from NotFound),
/// mirroring the SDET periphery's portable sentinel - more portable than `chmod 000`, which a test
/// running as root would bypass (root reads any file), silently defeating the guard under test.
#[test]
fn validate_residue_scan_surfaces_an_unreadable_store_conn_never_misreporting_live_worktrees() {
    let dir = temp_git_project_with_commit();
    let root = dir.path();

    let (_out, err, ok) = run_rigger(root, &["init"]);
    assert!(ok, "rigger init must succeed; stderr:\n{err}");
    git_ok(root, &["add", "-A"]);
    git_ok(root, &["commit", "-q", "-m", "scaffold"]);
    // An empty LOCAL sqlite store: the store a silent degrade would wrongly read (zero live units),
    // so the regression path FLAGS the live leftovers below as residue.
    seed_store(root);

    // A LIVE unit's on-disk leftovers: its deterministic scratch worktree and its `rigger/u/*`
    // branch. On a server-pinned box these are LIVE (the run lives on the server the unreadable
    // secret file names); they must never be misreported as removable residue because the scan
    // silently fell back to an empty local store.
    let scratch = root.join("scratchroot");
    let tmp = scratch.to_str().unwrap();
    std::fs::create_dir_all(scratch.join("rigger-wt-liveunit")).unwrap();
    std::fs::write(
        scratch.join("rigger-wt-liveunit").join("payload.bin"),
        [0u8; 4096],
    )
    .unwrap();
    git_ok(root, &["branch", "rigger/u/liveunit"]);

    // Make `.rigger/store.conn` PRESENT but UNREADABLE (a directory where the file goes): the
    // per-machine secret file this user cannot read. The store-selection authority now returns an
    // ERROR at the secret-file rung, not a selection - with no `--eventstore`, no `--conn`, and no
    // `KURRENTDB_CONN`, this rung is the one that decides.
    std::fs::create_dir(root.join(".rigger").join("store.conn"))
        .expect("place a directory where store.conn goes");

    let (out, err, ok) = run_rigger_envs(root, &["validate"], &[("RIGGER_TMPDIR", tmp)]);

    // 1. The selection error SURFACES: validate FAILS loudly (not the silent exit-0 degrade), and
    //    names the store-connection read failure - the loud-failure contract (spec 19c).
    assert!(
        !ok,
        "an unreadable store.conn must make validate's residue scan FAIL loudly, never silently \
         degrade to the local sqlite default; stdout:\n{out}\nstderr:\n{err}"
    );
    assert!(
        err.contains("store connection file"),
        "the failure must name the store-connection-file read error, proving the residue scan \
         surfaced the selection error rather than swallowing it into a wrong-store read; \
         stderr:\n{err}"
    );

    // 2. It NEVER misreports the LIVE unit's worktree/branch as residue: having aborted on the
    //    unreadable source, it must not have scanned an empty local store and flagged the live
    //    leftovers as removable - the destructive misdiagnosis this guards.
    assert!(
        !err.contains("rigger-wt-liveunit"),
        "a live worktree must NOT be flagged as residue off a silently-degraded empty local store; \
         stderr:\n{err}"
    );
    assert!(
        !err.contains("rigger/u/liveunit"),
        "a live branch must NOT be flagged as residue off a silently-degraded empty local store; \
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

/// Spec 44, criterion 1 - PERIPHERY (setup -> disk seam): the step-courier guarantee must
/// survive to the artifact Claude Code actually loads. The implementer's unit test asserts
/// the foreground/honest prompt over the IN-MEMORY `RIGGER_WORKFLOW` constant, and a separate
/// unit test asserts the installed file is BYTE-IDENTICAL to that constant - but byte-identity
/// says nothing about the constant's CONTENT (a regressed prompt would still install
/// byte-for-byte), and neither drives the real `rigger setup` subcommand end-to-end. This test
/// closes that boundary: it runs the built binary's `setup` (arg dispatch -> cmd_setup ->
/// install_workflow -> file write), then reads the on-disk `.claude/workflows/rigger.js` - the
/// exact file the harness auto-discovers and runs - and pins that its COURIER prompt still
/// runs `rigger step` as one foreground, blocking call (never backgrounded, never Monitor-
/// watched) and reports only an honest error, never a fabricated placeholder token.
#[test]
fn installed_workflow_courier_prompt_is_foreground_and_honest() {
    let dir = temp_project();
    let root = dir.path();

    // Drive the REAL `rigger setup` subcommand (RIGGER_NPM stubs npm so the shim step needs
    // no network); it writes the native /rigger workflow to disk.
    let (_out, err, ok) = run_rigger_envs(root, &["setup"], &[("RIGGER_NPM", "true")]);
    assert!(ok, "rigger setup must succeed; stderr:\n{err}");

    // The user-facing artifact: the file Claude Code auto-discovers and runs.
    let installed = root.join(".claude").join("workflows").join("rigger.js");
    let workflow = std::fs::read_to_string(&installed).unwrap_or_else(|e| {
        panic!(
            "rigger setup must install the workflow at {}; read failed: {e}",
            installed.display()
        )
    });

    // We are asserting on the COURIER agent's prompt specifically - anchor on it so the guard
    // pins the right agent's instructions, not some other prompt that happens to share a word.
    assert!(
        workflow.contains("You are a rigger COURIER"),
        "the installed workflow must define the step-courier agent prompt"
    );

    // 1. FOREGROUND, BLOCKING: the courier runs the step as one blocking Bash call - a
    //    foreground call blocks until the step prints its single JSON line, the exact line the
    //    courier relays back to the driver.
    assert!(
        workflow.contains("FOREGROUND, BLOCKING Bash"),
        "the installed courier prompt must instruct running `rigger step` as one FOREGROUND, \
         BLOCKING Bash call; got:\n{workflow}"
    );

    // 2. NOT backgrounded, NOT polled: the exact shape the defect ran the step in - a
    //    `run_in_background` step watched by a Monitor, returning a fabricated error before the
    //    step produced anything - is explicitly forbidden in the on-disk prompt.
    assert!(
        workflow.contains("NOT run_in_background"),
        "the installed courier prompt must explicitly forbid `run_in_background`; got:\n{workflow}"
    );
    assert!(
        workflow.contains("NOT via a Monitor"),
        "the installed courier prompt must explicitly forbid watching the step via a Monitor / \
         poll loop; got:\n{workflow}"
    );

    // 3. HONEST error: when the courier must report a failure, `error` is the ACTUAL stderr or
    //    the one fixed no-completion phrase - never an invented placeholder token.
    assert!(
        workflow.contains("step did not complete within my attempts"),
        "the installed courier prompt must allow the fixed no-completion phrase in `error`; \
         got:\n{workflow}"
    );
    assert!(
        workflow.contains("NEVER an invented placeholder"),
        "the installed courier prompt must forbid a fabricated placeholder token in `error`; \
         got:\n{workflow}"
    );

    // 4. Regression guard at the artifact: the exact fabricated token the defect returned must
    //    not appear ANYWHERE in the installed workflow - a courier that returns it lies that
    //    the step failed after zero waves.
    assert!(
        !workflow.contains("PLACEHOLDER_DO_NOT_USE"),
        "the fabricated placeholder token `PLACEHOLDER_DO_NOT_USE` must never reach the \
         installed workflow; got:\n{workflow}"
    );
}

/// Spec 51, criterion 3 - PERIPHERY (setup -> disk seam for THIS unit's amendment): the
/// courier's auto-background WAIT rule must survive to the artifact Claude Code actually loads.
/// The implementer's unit test asserts the amendment over the IN-MEMORY `RIGGER_WORKFLOW`
/// constant (comment-stripped), and a sibling unit test asserts the installed file is
/// BYTE-IDENTICAL to that constant - but byte-identity says nothing about the constant's CONTENT
/// (a regressed prompt would still install byte-for-byte), and neither drives the real `rigger
/// setup` subcommand end-to-end. This test closes THAT boundary for the new rule: it runs the
/// built binary's `setup` (arg dispatch -> cmd_setup -> install_workflow -> file write), then
/// reads the on-disk `.claude/workflows/rigger.js` - the exact file the harness auto-discovers -
/// and pins that the courier prompt carries the ONE sanctioned exception spec 51 grants: when the
/// DRIVING HARNESS (not the courier) auto-backgrounds the foreground step because it outran the
/// foreground cap, the courier WAITS on that background task's output file for the step's JSON
/// line and returns it verbatim, falls back to the re-run rule if it cannot, and NEVER returns a
/// placeholder. It is scoped to this unit: it asserts nothing about the reviewer-error re-park or
/// the worktree self-heal / sweep-ordering. Complements
/// `installed_workflow_courier_prompt_is_foreground_and_honest`, which pins the unchanged
/// foreground/honest half of the SAME on-disk courier prompt.
#[test]
fn installed_workflow_courier_waits_on_an_auto_backgrounded_step() {
    let dir = temp_project();
    let root = dir.path();

    // Drive the REAL `rigger setup` subcommand (RIGGER_NPM stubs npm so the shim step needs no
    // network); it writes the native /rigger workflow to disk.
    let (_out, err, ok) = run_rigger_envs(root, &["setup"], &[("RIGGER_NPM", "true")]);
    assert!(ok, "rigger setup must succeed; stderr:\n{err}");

    // The user-facing artifact: the file Claude Code auto-discovers and runs.
    let installed = root.join(".claude").join("workflows").join("rigger.js");
    let workflow = std::fs::read_to_string(&installed).unwrap_or_else(|e| {
        panic!(
            "rigger setup must install the workflow at {}; read failed: {e}",
            installed.display()
        )
    });

    // Anchor on the COURIER agent's prompt so the guard pins the right agent's instructions.
    assert!(
        workflow.contains("You are a rigger COURIER"),
        "the installed workflow must define the step-courier agent prompt"
    );

    // 1. The exception is scoped to a HARNESS-initiated conversion, not a courier choice: the
    //    on-disk prompt names it a sanctioned exception where the harness turns the foreground
    //    call into a background task the courier did not choose - so a courier reading the
    //    artifact cannot use it to justify backgrounding the step itself.
    assert!(
        workflow.contains("sanctioned")
            && workflow.contains("background task")
            && workflow.contains("did not choose"),
        "the installed courier prompt must scope the wait to a HARNESS-initiated conversion of \
         the foreground call into a background task the courier did not choose; got:\n{workflow}"
    );

    // 2. The SANCTIONED WAIT reached the artifact: on that path the courier polls the background
    //    task's OUTPUT FILE for the step's single JSON line and returns it verbatim - the exact
    //    wait spec 51 grants a courier otherwise forbidden from monitors and unable to wait.
    assert!(
        workflow.contains("output file")
            && workflow.contains("polling")
            && workflow.contains("verbatim"),
        "the installed courier prompt must instruct WAITING by polling the auto-backgrounded \
         step's OUTPUT FILE for the JSON line and returning it verbatim; got:\n{workflow}"
    );

    // 3. FALL BACK to the existing re-run rule when the JSON still cannot be obtained: the step's
    //    gate results are recorded durably, so a re-run resumes past finished work - the on-disk
    //    prompt must route to that rule, never to a fabricated result.
    assert!(
        workflow.contains("re-run") && workflow.contains("resumes past"),
        "the installed courier prompt must fall back to re-running the FOREGROUND step (a re-run \
         resumes past durably recorded work) when the JSON cannot be obtained; got:\n{workflow}"
    );

    // 4. The PLACEHOLDER PROHIBITION still holds on THIS path in the artifact - returning a
    //    sentinel / placeholder for an auto-backgrounded step is exactly the defect spec 51
    //    closes - and the fabricated token from the original defect never reaches the file.
    assert!(
        workflow.contains("return a placeholder"),
        "the installed courier prompt must keep forbidding a placeholder on the auto-background \
         path; got:\n{workflow}"
    );
    assert!(
        !workflow.contains("PLACEHOLDER_DO_NOT_USE"),
        "the fabricated placeholder token must never reach the installed workflow; got:\n{workflow}"
    );

    // 5. The amendment ADDS an exception; it does not relax the default. The unchanged
    //    foreground/honest rule (owned by the sibling test) still stands in the SAME artifact,
    //    so the auto-background wait cannot be read as a general license to background the step.
    assert!(
        workflow.contains("FOREGROUND, BLOCKING Bash")
            && workflow.contains("NOT run_in_background"),
        "the installed courier prompt must keep the normal FOREGROUND, BLOCKING rule that forbids \
         the courier from backgrounding the step itself; got:\n{workflow}"
    );
}

/// Spec 46, criterion 1 - PERIPHERY (setup -> gitignore-on-disk seam): the always-on dash
/// writes two runtime breadcrumbs under `.rigger/` - `.rigger/dash.url` and
/// `.rigger/dash.marker`. Left untracked-and-not-ignored in a consumer's repo they get swept
/// into a unit worktree's commit by `git add`, then collide with the live dash's rewrites when
/// the conductor merges the unit ("untracked working tree files would be overwritten"). The
/// implementer's unit tests call `init_project` IN-PROCESS and assert the `.gitignore` CONTENT
/// and the returned report - but neither drives the real `rigger setup` subcommand end-to-end
/// (arg dispatch -> cmd_setup -> init_project -> file write), and neither proves that a real
/// git actually HONORS the written lines. This test closes that boundary: it runs the built
/// binary's `setup`, reads the on-disk `.gitignore` the consumer keeps, and then proves the
/// actual collision-preventing behavior - a real `git check-ignore` treats both breadcrumbs as
/// ignored, so a later `git add` never sweeps them into a unit commit.
#[test]
fn setup_gitignores_the_dash_breadcrumbs_and_git_honors_them_end_to_end() {
    let dir = temp_project();
    let root = dir.path();

    // Drive the REAL `rigger setup` subcommand (RIGGER_NPM stubs npm so the shim step needs no
    // network; run_rigger_envs sets RIGGER_NO_DASH so no live dashboard starts - the breadcrumbs
    // are created by hand below to model the dash having written them).
    let (_out, err, ok) = run_rigger_envs(root, &["setup"], &[("RIGGER_NPM", "true")]);
    assert!(ok, "rigger setup must succeed; stderr:\n{err}");

    // 1. CLI -> init_project -> disk wiring: the consumer's on-disk `.gitignore` ignores BOTH
    //    dash breadcrumbs, exactly as it does for the other machine-local installs.
    let gitignore = std::fs::read_to_string(root.join(".gitignore"))
        .expect("rigger setup must write a .gitignore at the project root");
    assert!(
        gitignore.lines().any(|l| l.trim() == ".rigger/dash.url"),
        "the installed .gitignore must ignore the dash url breadcrumb; got:\n{gitignore}"
    );
    assert!(
        gitignore.lines().any(|l| l.trim() == ".rigger/dash.marker"),
        "the installed .gitignore must ignore the dash marker breadcrumb; got:\n{gitignore}"
    );

    // 2. The actual collision-preventing behavior, end to end: create the two breadcrumbs the
    //    live dash would write, then prove a REAL git treats each as ignored. `git check-ignore
    //    -q` exits 0 only for an ignored path, so a subsequent `git add -A` (which the conductor
    //    runs before committing a unit) never sweeps them in, and the "untracked working tree
    //    files would be overwritten" merge collision cannot arise.
    std::fs::create_dir_all(root.join(".rigger")).unwrap();
    std::fs::write(
        root.join(".rigger").join("dash.url"),
        "http://127.0.0.1:7420/\n",
    )
    .unwrap();
    std::fs::write(root.join(".rigger").join("dash.marker"), "7420\n1234\n").unwrap();
    for breadcrumb in [".rigger/dash.url", ".rigger/dash.marker"] {
        let ignored = Command::new("git")
            .args(["check-ignore", "-q", breadcrumb])
            .current_dir(root)
            .status()
            .expect("git must be runnable")
            .success();
        assert!(
            ignored,
            "a real git must treat {breadcrumb} as ignored after rigger setup, so `git add` \
             never sweeps it into a unit commit"
        );
    }
}

/// Spec 46, criterion 1 - PERIPHERY (setup -> gitignore under a HOSTILE global git config):
/// the committed `.gitignore` setup writes must be SELF-CONTAINED and portable, never
/// contingent on the setup-runner's machine-local git configuration. `git`'s full ignore
/// resolution consults global sources (`core.excludesFile`, `~/.config/git/ignore`,
/// `.git/info/exclude`); if setup let those decide what to append, an operator whose global
/// excludes already cover `.claude/` and `.rigger/` would ship a `.gitignore` MISSING the
/// dash-breadcrumb lines - and a teammate or CI cloning with a clean HOME would then let
/// `git add` sweep `.rigger/dash.url` / `.rigger/dash.marker` into a unit commit, the exact
/// "untracked working tree files would be overwritten" collision criterion 1 exists to
/// prevent. This test runs the real `rigger setup` under a `GIT_CONFIG_GLOBAL` whose
/// `core.excludesFile` already ignores `.claude/` and `.rigger/`, and asserts the committed
/// `.gitignore` STILL carries every required line - so the shipped artifact is machine
/// independent. It is a regression guard against re-introducing a machine-local ignore lookup
/// (e.g. `git check-ignore`) that would silently omit the lines.
#[test]
fn setup_writes_a_machine_independent_gitignore_under_a_hostile_global_config() {
    let dir = temp_project();
    let root = dir.path();

    // A hostile global git config: its excludes list already ignores `.claude/` and the whole
    // `.rigger/` runtime dir (a common configuration - so a full `git check-ignore` would report
    // every setup-written pattern already ignored).
    let global_ignore = root.join("hostile_global_ignore");
    std::fs::write(&global_ignore, ".claude/\n.rigger/\n").unwrap();
    let global_config = root.join("hostile_global_config");
    std::fs::write(
        &global_config,
        format!(
            "[core]\n\texcludesFile = {}\n",
            global_ignore.to_str().unwrap()
        ),
    )
    .unwrap();
    let global_config_str = global_config.to_str().unwrap();

    // Sanity: prove the global config is genuinely HOSTILE - a full `git check-ignore` (which is
    // what a machine-local lookup would use) reports the dash breadcrumb already ignored via the
    // global rule, so this config WOULD have suppressed the append under a check-ignore skip.
    let would_be_suppressed = Command::new("git")
        .args(["check-ignore", "-q", ".rigger/dash.url"])
        .current_dir(root)
        .env("GIT_CONFIG_GLOBAL", global_config_str)
        .status()
        .expect("git must be runnable")
        .success();
    assert!(
        would_be_suppressed,
        "the test's global config must actually ignore .rigger/dash.url (else the regression \
         guard is inconclusive)"
    );

    // Drive the REAL `rigger setup` under that hostile global config.
    let (_out, err, ok) = run_rigger_envs(
        root,
        &["setup"],
        &[
            ("RIGGER_NPM", "true"),
            ("GIT_CONFIG_GLOBAL", global_config_str),
        ],
    );
    assert!(ok, "rigger setup must succeed; stderr:\n{err}");

    // The committed `.gitignore` carries EVERY setup-written pattern despite the hostile global
    // excludes - the artifact is self-contained and portable to a clean-HOME teammate/CI. The
    // patterns are written in their normalized form (a trailing slash is stripped before the
    // line is appended), so `.claude/` lands as `.claude` and `.rigger/shim/` as `.rigger/shim`.
    let gitignore = std::fs::read_to_string(root.join(".gitignore"))
        .expect("rigger setup must write a .gitignore at the project root");
    for pattern in [
        ".claude",
        ".rigger/shim",
        ".rigger/dash.url",
        ".rigger/dash.marker",
    ] {
        assert!(
            gitignore.lines().any(|l| l.trim() == pattern),
            "the committed .gitignore must contain `{pattern}` even under a global config that \
             already ignores it, so the shipped file is machine independent; got:\n{gitignore}"
        );
    }
}

/// Spec 44, criterion 2 - PERIPHERY (setup -> disk seam): the driver's null-step guard must
/// survive to the artifact the harness actually loads and runs. The implementer's unit test
/// asserts the guard structurally over the IN-MEMORY `RIGGER_WORKFLOW` constant, and a
/// separate unit test asserts the installed file is byte-identical to that constant - but
/// byte-identity says nothing about the constant's CONTENT (a regressed driver would still
/// install byte-for-byte), and neither drives the real `rigger setup` subcommand end-to-end.
/// This test closes that boundary: it runs the built binary's `setup` (arg dispatch ->
/// cmd_setup -> install_workflow -> file write), then reads the on-disk
/// `.claude/workflows/rigger.js` - the exact driver the harness auto-discovers and runs - and
/// pins that its loop STILL guards a null step before dereferencing it. `agent()` can RESOLVE
/// to null (not reject) when the step-courier dies on a terminal error, so an unguarded
/// `step.error` read would crash the driver uncaught; the guard turns that into a clean, loud,
/// resumable stop. Both the guard's PRESENCE and its ORDERING (guard before the dereference)
/// and its loud `stop()` and its cause-naming, resumable diagnostic must reach the installed
/// file, or the fix never protects a real run.
#[test]
fn installed_workflow_driver_guards_a_null_step() {
    let dir = temp_project();
    let root = dir.path();

    // Drive the REAL `rigger setup` subcommand (RIGGER_NPM stubs npm so the shim step needs
    // no network); it writes the native driver workflow to disk.
    let (_out, err, ok) = run_rigger_envs(root, &["setup"], &[("RIGGER_NPM", "true")]);
    assert!(ok, "rigger setup must succeed; stderr:\n{err}");

    // The user-facing artifact: the driver file the harness auto-discovers and runs.
    let installed = root.join(".claude").join("workflows").join("rigger.js");
    let workflow = std::fs::read_to_string(&installed).unwrap_or_else(|e| {
        panic!(
            "rigger setup must install the workflow at {}; read failed: {e}",
            installed.display()
        )
    });

    // 1. The guard EXISTS in the installed driver: it tests `!step` (agent() resolved to null)
    //    before touching the step's fields.
    assert!(
        workflow.contains("if (!step)"),
        "the installed driver must guard a null step with `if (!step)` before dereferencing it; \
         got:\n{workflow}"
    );

    // 2. The guard PRECEDES the dereference in the installed driver. Anchor the dereference on
    //    the code conditional `if (step.error)` (not a bare `step.error`, which also appears in
    //    the explanatory comment): a null step reaching `if (step.error)` before the guard runs
    //    would crash on the very read the guard exists to prevent. Both tokens are code and each
    //    appears once, so `find` positions order them unambiguously.
    let guard = workflow
        .find("if (!step)")
        .expect("the installed driver must guard a null step");
    let deref = workflow
        .find("if (step.error)")
        .expect("the installed driver must read step.error after the guard");
    assert!(
        guard < deref,
        "the `if (!step)` guard must precede the `if (step.error)` dereference in the installed \
         driver, or a null step (agent() resolved to null) would still crash before the guard \
         runs; got:\n{workflow}"
    );

    // 3. The guard stops CLEANLY and LOUDLY: it routes the null step through the throwing
    //    `stop()`, not a silent fall-through, and that stop lives BETWEEN the guard and the
    //    dereference.
    assert!(
        workflow[guard..deref].contains("stop("),
        "the installed null-step guard must stop loudly via `stop(...)` before the dereference, \
         not fall through; got:\n{workflow}"
    );

    // 4. The diagnostic names the LIKELY CAUSE (the courier agent died on a terminal error - an
    //    expired login / an exhausted quota - so agent() RESOLVED TO NULL) and that the run is
    //    RESUMABLE, the two things spec 44 requires the message to carry so the operator knows
    //    why it stopped and that a re-run continues from this frontier.
    assert!(
        workflow.contains("resolved to null"),
        "the installed null-step diagnostic must name the cause: agent() RESOLVED TO NULL rather \
         than rejecting; got:\n{workflow}"
    );
    assert!(
        workflow.contains("expired login") && workflow.contains("quota"),
        "the installed null-step diagnostic must name the likely terminal cause (an expired \
         login or an exhausted API quota); got:\n{workflow}"
    );
    assert!(
        workflow.contains("RESUMABLE"),
        "the installed null-step diagnostic must tell the operator the run is RESUMABLE (a \
         re-run continues from this frontier); got:\n{workflow}"
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

/// Seed `<root>/.rigger/events.db` with a stream whose position order and revision order
/// DISAGREE (spec 71's signature `rigger validate` must detect) by inserting rows directly -
/// bypassing the store's own revision assignment, the only way to reach this shape (a
/// correctly functioning append always assigns `MAX(revision) + 1`, so it can never produce
/// this on its own). Three rows land in stream `run`, in this insertion (position) order:
/// revision 5, then revision 1, then revision 2 - each value is DISTINCT so
/// `UNIQUE(stream, revision)` is satisfied (this is the actual on-disk shape a write that
/// lands in a compaction-opened revision hole leaves: the row it targets is a hole, never a
/// duplicate), but positions 2 and 3 both carry a revision at or below the stream's already-
/// recorded maximum (5) - the two out-of-order rows the test asserts on.
fn seed_order_signature(root: &Path, project: &str) {
    let rigger = root.join(".rigger");
    std::fs::create_dir_all(&rigger).unwrap();
    std::fs::write(rigger.join("project.id"), format!("{project}\n")).unwrap();
    let db = rigger.join("events.db");
    // Open through the real store first, so the schema is laid down exactly as the binary
    // itself would lay it down.
    rigger::eventstore::sqlite::Store::open(db.to_str().unwrap()).unwrap();
    let stream = format!(
        "{}run",
        rigger::eventstore::namespace::Namespaced::prefix_for(project)
    );
    let conn = rusqlite::Connection::open(&db).unwrap();
    for revision in [5i64, 1, 2] {
        conn.execute(
            "INSERT INTO events (stream, type, id, data, meta, valid_from, recorded_at, revision)
             VALUES (?1, 'Seed', ?2, X'7b7d', '{}', 0, 0, ?3)",
            rusqlite::params![stream, format!("seed-{revision}"), revision],
        )
        .unwrap();
    }
}

/// Spec 71 (`rigger validate` clause, VALIDATE DETECTS THE SIGNATURE): a stream whose
/// position order and revision order disagree draws an advisory naming the stream, the
/// out-of-order row count, and the repair doc, while the exit status stays unchanged
/// (report-only - validate never repairs anything). A clean store draws nothing.
#[test]
fn validate_detects_a_stream_whose_position_order_and_revision_order_disagree() {
    // The clean control first: an ordinary project draws no order-signature advisory.
    let clean = temp_project();
    let croot = clean.path();
    let (_o, err, ok) = run_rigger(croot, &["init"]);
    assert!(
        ok,
        "rigger init must scaffold a valid config; stderr:\n{err}"
    );
    let (out, err, ok) = run_rigger(croot, &["validate"]);
    assert!(ok, "validate must succeed on a clean store; stderr:\n{err}");
    assert!(
        out.contains("config valid"),
        "validate still prints its config summary; stdout:\n{out}"
    );
    assert!(
        !err.to_lowercase()
            .contains("position order and revision order"),
        "a clean store must NOT draw the order-signature advisory; stderr:\n{err}"
    );

    // The seeded disagreement.
    let dirty = temp_project();
    let droot = dirty.path();
    let (_o, err, ok) = run_rigger(droot, &["init"]);
    assert!(
        ok,
        "rigger init must scaffold a valid config; stderr:\n{err}"
    );
    seed_order_signature(droot, "order-signature-project");
    let (_out, err, ok) = run_rigger(droot, &["validate"]);
    assert!(
        ok,
        "validate WARNS but still exits 0 on an order signature (report-only); stderr:\n{err}"
    );
    // Pinned to the exact reported values, not a loose digit match: `seed_order_signature`
    // inserts revision 5 at position 1 (sets the running max), then revision 1 at position 2
    // and revision 2 at position 3 - both out of order against the max of 5 - so the advisory
    // MUST report exactly 2 row(s) spanning positions 2..=3 for stream `run`. A bare
    // `err.contains('2')` would still pass on a wrong count or a shifted range (there are
    // other digits in validate's output, e.g. the config summary and other advisories), so it
    // proves nothing about the count/range the Done-when criterion actually requires.
    assert!(
        err.contains("stream run has 2 row(s)"),
        "the advisory must name the stream and the exact out-of-order row count together; \
         stderr:\n{err}"
    );
    assert!(
        err.contains("positions 2..=3"),
        "the advisory must name the exact affected position range; stderr:\n{err}"
    );
    assert!(
        err.to_lowercase()
            .contains("position order and revision order"),
        "the advisory names the disagreement it detected; stderr:\n{err}"
    );
    assert!(
        err.contains("architecture.md"),
        "the advisory names the repair doc; stderr:\n{err}"
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

/// Every loopback port THIS TEST BINARY has already handed out, in the order it handed them
/// out. A TCP port is a process-global resource, so the record of which ones are already
/// spoken for is process-global too - there is no caller to inject it into, because the
/// resource being rationed is not owned by any caller.
///
/// It exists because a port a test has been HANDED is not a port the OS considers taken: a
/// test reads an ephemeral port's number, releases the listener, and only then spawns the
/// child that binds it, so between those two moments the OS is free to offer the very same
/// port to a test running on another thread. Both then bind servers on one port: one wins,
/// and the loser probes the WINNER's server. When the winner is one of this suite's
/// deliberately silent holders, the loser's probe blocks to its read timeout and reports the
/// server it spawned "never came up" - a red on an unchanged tree. The ledger closes exactly
/// that window: a port handed out is never handed out again while its owner is still starting.
static HANDED_OUT_LOOPBACK_PORTS: std::sync::Mutex<Vec<u16>> = std::sync::Mutex::new(Vec::new());

/// How many ports [`reserved_loopback_listener`] will look at before giving up. Generous: a
/// probe is only rejected when the OS offers a port this process ALREADY holds a reservation
/// for, and needing more than this many fresh offers means the ephemeral range is exhausted -
/// a machine condition a test must report, never quietly hand back a colliding port for.
const LOOPBACK_PROBE_ATTEMPTS: usize = 128;

/// The reservation decision itself, with the OS held at arm's length: keep asking `probe` for
/// a port until it offers one absent from `handed_out`, record that one, and answer with it -
/// or `None` when `attempts` offers were all already reserved.
///
/// `probe` yields `(port, holder)`: the port, and the live listener still HOLDING it, so a
/// rejected offer is dropped (releasing the port) while the accepted one is handed on still
/// bound. Pure with respect to the network - the caller supplies the binding - so the choice
/// can be driven with an OS that offers the same port twice, which is the whole failure this
/// exists to prevent and which no test can provoke from a real one on demand.
fn reserve_first_unheld<H>(
    handed_out: &mut Vec<u16>,
    attempts: usize,
    mut probe: impl FnMut() -> (u16, H),
) -> Option<(u16, H)> {
    (0..attempts).find_map(|_| {
        let (port, holder) = probe();
        (!handed_out.contains(&port)).then(|| {
            handed_out.push(port);
            (port, holder)
        })
    })
}

/// A bound loopback listener on a port no other test in this binary has been handed - the ONE
/// authority every ephemeral bind in this file goes through, so no two tests are ever pointed
/// at the same port (see [`HANDED_OUT_LOOPBACK_PORTS`]). Callers that need the port HELD (a
/// deliberate non-dash holder) keep the returned listener; [`free_loopback_port`] drops it.
fn reserved_loopback_listener() -> std::net::TcpListener {
    // Poisoning carries no meaning here: the ledger is a plain list of numbers, consistent at
    // every point a panic could unwind through, and a poisoned lock must not turn one test's
    // failure into a cascade of unrelated ones.
    let mut handed_out = HANDED_OUT_LOOPBACK_PORTS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let (_, listener) = reserve_first_unheld(&mut handed_out, LOOPBACK_PROBE_ATTEMPTS, || {
        let listener =
            std::net::TcpListener::bind(("127.0.0.1", 0)).expect("bind an ephemeral loopback port");
        let port = listener.local_addr().expect("read the bound port").port();
        (port, listener)
    })
    .unwrap_or_else(|| {
        panic!(
            "no loopback port free of this binary's own {} reservation(s) in \
             {LOOPBACK_PROBE_ATTEMPTS} attempts - the ephemeral range is exhausted",
            handed_out.len()
        )
    });
    listener
}

/// A currently-free loopback TCP port, reserved for the caller and then released, so the
/// server the caller spawns binds successfully (never colliding with a parallel test or a real
/// dash on `DEFAULT_PORT`) and is therefore a genuinely long-lived child rather than a process
/// that exits on a bind conflict.
fn free_loopback_port() -> u16 {
    reserved_loopback_listener()
        .local_addr()
        .expect("read the bound port")
        .port()
}

/// AN OS THAT OFFERS ONE PORT TWICE STILL HANDS TWO TESTS TWO PORTS.
///
/// The window this closes is not observable from the real network on demand - it needs the OS
/// to re-offer a port between a test reading its number and the child binding it - so the
/// choice is driven here against an OS that does exactly that, every time. Without the ledger
/// both callers are handed `40001`, which is the shape that fails a green suite on an
/// unchanged tree.
#[test]
fn a_loopback_port_already_handed_out_is_never_handed_out_a_second_time() {
    let offers = [40001u16, 40001, 40002];
    let mut offered = offers.iter().copied();
    let mut probe = move || (offered.next().expect("the fixture offers enough ports"), ());

    let mut handed_out = Vec::new();
    let first = reserve_first_unheld(&mut handed_out, 8, &mut probe).expect("a first port");
    let second = reserve_first_unheld(&mut handed_out, 8, &mut probe).expect("a second port");

    assert_eq!(first.0, 40001, "the first caller takes the first offer");
    assert_eq!(
        second.0, 40002,
        "the second caller must SKIP the re-offered {} and take the next free port",
        first.0
    );
    assert_eq!(
        handed_out,
        vec![40001, 40002],
        "both reservations are recorded, so a third caller skips them too"
    );
}

/// A RESERVATION THAT CANNOT BE MADE IS REPORTED, NEVER FAKED.
///
/// Handing back a port the process already reserved would reintroduce the exact collision this
/// authority exists to prevent, silently - so an exhausted probe answers `None` and the caller
/// fails loudly instead.
#[test]
fn an_exhausted_probe_reserves_nothing_rather_than_re_handing_a_reserved_port() {
    let mut handed_out = vec![40001u16];
    let taken = reserve_first_unheld(&mut handed_out, 4, || (40001, ()));
    assert!(
        taken.is_none(),
        "an OS with only an already-reserved port to offer must yield no reservation"
    );
    assert_eq!(handed_out, vec![40001], "and must record nothing new");
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
    use std::process::Stdio;
    use std::time::Duration;

    // A repo-less/empty-store dir is enough: `rigger dash` reads an ABSENT events.db as an
    // empty run and serves anyway, so it is a genuine long-lived child with no run seeded.
    let proj = temp_project();
    let port = free_loopback_port();

    let mut child = common::rigger_courier()
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

/// How long a probe keeps trying to get a complete response out of a just-spawned server
/// before it gives up and reports that the server never came up.
const SERVER_READY_SECS: u64 = 15;

/// How long ONE attempt waits for the server to answer. Shorter than
/// [`SERVER_READY_SECS`] on purpose: an attempt that stalls must leave the deadline room
/// for further attempts, because "answered nothing yet" is what a server still coming up
/// looks like.
const ATTEMPT_READ_SECS: u64 = 3;

/// ONE complete HTTP GET attempt of `path` at `hostport` over a raw TCP socket (the test
/// crate has no HTTP client): connect, send, read to EOF, and answer with the WHOLE response
/// (status line + headers + body). `None` means this ATTEMPT produced no complete response.
///
/// Every way an attempt can come up short is the same one answer - a refused connect, a
/// failed write, a read that timed out, a peer that closed with nothing, or a reply with no
/// header terminator - because to a caller probing a server that is still coming up they are
/// all the same fact, and only [`http_probe`] decides when that stops being acceptable.
fn http_attempt(hostport: &str, path: &str) -> Option<String> {
    use std::io::{Read, Write};
    use std::time::Duration;
    let mut stream = std::net::TcpStream::connect(hostport).ok()?;
    stream
        .set_read_timeout(Some(Duration::from_secs(ATTEMPT_READ_SECS)))
        .ok()?;
    write!(
        stream,
        "GET {path} HTTP/1.1\r\nHost: {hostport}\r\nConnection: close\r\n\r\n"
    )
    .ok()?;
    let mut resp = String::new();
    stream.read_to_string(&mut resp).ok()?;
    // A response without a header terminator is not a response: the peer accepted the
    // connection and closed (or dribbled) without answering, which is a server that has not
    // finished coming up, NOT a server that answered something unexpected.
    resp.contains("\r\n\r\n").then_some(resp)
}

/// Poll `hostport` for `path` until a WHOLE response arrives, on a bounded deadline.
///
/// Bringing a server up is not one instant but a sequence - the child is spawned, then binds,
/// then accepts, then answers - and a probe fired anywhere before the end of it comes back
/// empty-handed for a reason that says nothing about whether the server works. Retrying only
/// the CONNECT covers just the first of those steps: the kernel completes a handshake into the
/// backlog the moment the port is bound, so a connect starts succeeding while the server is
/// still too busy to answer, and a probe that gave up there reported a healthy server as dead
/// on a loaded machine. So the whole exchange is what gets retried. A server that never
/// answers within the deadline still fails LOUD (the safe direction), never a false green.
fn http_probe(hostport: &str, path: &str) -> Option<String> {
    use std::time::{Duration, Instant};
    let deadline = Instant::now() + Duration::from_secs(SERVER_READY_SECS);
    loop {
        if let Some(resp) = http_attempt(hostport, path) {
            return Some(resp);
        }
        if Instant::now() >= deadline {
            return None;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// A minimal HTTP GET of a `http://127.0.0.1:<port>/` URL, returning the response BODY on
/// success. The dash answers with `Connection: close`, so the read runs to EOF and terminates.
/// Used to prove an auto-started dash is genuinely SERVING (not merely that a URL was
/// recorded). `None` only when the server never answered within [`http_probe`]'s deadline.
fn http_get(url: &str) -> Option<String> {
    let hostport = url.strip_prefix("http://")?.trim_end_matches('/');
    let resp = http_probe(hostport, "/")?;
    let body_start = resp.find("\r\n\r\n").map(|i| i + 4).unwrap_or(0);
    Some(resp[body_start..].to_string())
}

/// A minimal HTTP GET of an arbitrary `path` (e.g. `/api/instances`, `/api/state?instance=<id>`)
/// on a loopback dash at `port`, returning the whole response (status line + headers + body) so a
/// caller can assert BOTH the `200` status and the body content. Polled on the same bounded
/// startup deadline as [`http_get`]; `None` only when the dash never came up within it.
fn http_get_path(port: u16, path: &str) -> Option<String> {
    http_probe(&format!("127.0.0.1:{port}"), path)
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
    use std::process::Stdio;
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
    let mut child = common::rigger_courier()
        .args(["serve", "--base", "HEAD"])
        .current_dir(root)
        // Redirect the machine-global registry (spec 50, criterion 2) into the test's own temp
        // tree so this served run registers under `root/rigger`, never the operator's real
        // ~/.local/state/rigger/instances.
        .env("XDG_STATE_HOME", root)
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

/// Spec 68, criterion 1 (the render pipeline covers the WHOLE registry, end to end): `rigger
/// docs` renders EVERY entry in `rigger::docs::skill_registry` - not only the pre-existing
/// `using-rigger` skill the sibling test above drives, but the second, generalized entry
/// `planning-a-spec` too - to its own committed path, carrying its own loadable frontmatter,
/// its authoring recipe, AND the operator-binary prohibition every registry skill is stamped
/// with structurally. Driving the real binary proves the registry generalization
/// (`write_docs` looping over `skill_registry()`) actually reaches a SECOND skill, not just
/// the one the pre-generalization implementation already handled - the class of regression an
/// in-process unit test that only checks set membership (names/paths match) cannot catch: a
/// loop that silently renders the first entry twice, or a path bug specific to a
/// two-or-more-entry registry, would still satisfy a length/name check but ship the wrong
/// bytes here.
#[test]
fn docs_renders_every_registry_skill_including_planning_a_spec() {
    let proj = temp_project();
    let root = proj.path();

    let (stdout, stderr, ok) = run_rigger(root, &["docs"]);
    assert!(ok, "rigger docs must succeed; stderr: {stderr}");
    assert!(
        stdout.contains("skills/planning-a-spec/SKILL.md"),
        "rigger docs must report rendering the SECOND registry entry too; got: {stdout}"
    );

    let planning_path = root.join("skills/planning-a-spec/SKILL.md");
    let planning = std::fs::read_to_string(&planning_path)
        .expect("rigger docs must have written the planning-a-spec skill to disk");

    assert!(
        planning.starts_with("---\nname: planning-a-spec\n"),
        "the second registry entry must open with its own loadable frontmatter; got: {}",
        &planning[..planning.len().min(60)]
    );
    assert!(
        planning.contains("**1. Ground the Goal in evidence.**")
            && planning.contains("**7. Preflight, then launch.**"),
        "the rendered planning-a-spec skill must carry its authoring recipe; got:\n{planning}"
    );
    // The operator-binary prohibition (spec 68, Design) is stamped on EVERY registry entry
    // structurally, including a skill whose body carries no code-derived facts to interpolate.
    assert!(
        planning.contains("## Operator binary boundary")
            && planning.contains("never installs, replaces, or modifies the operator's"),
        "the rendered planning-a-spec skill must carry the operator-binary prohibition; \
         got:\n{planning}"
    );

    // Byte-stable across runs (the drift check depends on it).
    let (_o2, _e2, ok2) = run_rigger(root, &["docs"]);
    assert!(ok2);
    assert_eq!(
        std::fs::read_to_string(&planning_path).unwrap(),
        planning,
        "a second render of the second registry entry must be byte-identical"
    );
}

/// Spec 46, criterion 2 (the pre-run graph-hygiene guidance ships to CONSUMERS, end to
/// end): the discipline is not merely present in an in-process render - it must survive the
/// whole composition path (docs_context -> render -> write) that the real `rigger docs`
/// binary drives, so it reaches the two consumer-facing files an author commits and ships.
/// Driving the built binary and reading the WRITTEN skill and handbook proves the section
/// actually LANDS in what consumers get, and that the shipped text carries the truthful WHY:
/// graph.db is a PERSISTENT incremental projection (a step never re-folds the whole
/// history), so across runs it accumulates dead-run rows no live query reads, which `rigger
/// reset --runs` prunes to reclaim the disk they held. The implementer's in-process
/// `discipline_carries_graph_hygiene_pre_run_reset` unit test pins the render output; this
/// periphery layer pins that the write path in the built binary carries it all the way to
/// the consumer's files - something an in-process render assertion cannot prove.
#[test]
fn docs_ships_graph_hygiene_guidance_to_consumers() {
    let proj = temp_project();
    let root = proj.path();

    let (stdout, stderr, ok) = run_rigger(root, &["docs"]);
    assert!(ok, "rigger docs must succeed; stderr: {stderr}");
    assert!(
        stdout.contains("skills/using-rigger/SKILL.md") && stdout.contains("using-rigger.md"),
        "rigger docs must report writing both consumer-facing paths; got: {stdout}"
    );

    let skill = std::fs::read_to_string(root.join("skills/using-rigger/SKILL.md"))
        .expect("the skill was rendered to disk");
    let handbook = std::fs::read_to_string(root.join("docs/handbook/using-rigger.md"))
        .expect("the handbook chapter was rendered to disk");

    // BOTH consumer-facing outputs, as WRITTEN by the built binary, carry the graph-hygiene
    // section, name the pre-run command, and frame the truthful WHY (a persistent
    // incremental projection whose dead-run accumulation `rigger reset --runs` prunes to
    // reclaim the disk it held) - not a per-step whole-stream re-fold, and not a fold-speed
    // claim. Both render from the single `discipline_body` authority, so the skill and the
    // handbook chapter ship the guidance identically.
    for (label, out) in [("skill", &skill), ("handbook", &handbook)] {
        assert!(
            out.contains("## Graph hygiene"),
            "{label} shipped by `rigger docs` must carry the graph-hygiene section"
        );
        assert!(
            out.contains("rigger reset --runs"),
            "{label} shipped by `rigger docs` must name `rigger reset --runs` as the pre-run \
             hygiene step"
        );
        assert!(
            out.contains("persistent projection"),
            "{label} shipped by `rigger docs` must frame graph.db as a persistent incremental \
             projection (the corrected WHY, not a per-step whole-stream re-fold)"
        );
        assert!(
            out.contains("reclaims the disk"),
            "{label} shipped by `rigger docs` must explain reset --runs reclaims the disk the \
             dead-run rows held (bounded growth, not a fold-speed claim)"
        );
        // NEGATIVE regression guard (spec 46 c2): the DISCREDITED fold-speed framing that
        // rejected this unit's first attempt (graph.db re-folded whole-history each step, the
        // fold slow in proportion to graph size, a prune speeding it up) must never reach the
        // consumer's files. graph.db is a PERSISTENT incremental projection; a prune reclaims
        // DISK, it speeds no fold. Pin those phrases OUT (case-insensitively) so a future edit
        // resurrecting the false mechanism fails LOUDLY here instead of shipping to consumers.
        let lower = out.to_lowercase();
        for banned in [
            "re-folded each step",
            "fold stays slow",
            "proportional to graph size",
            "faster fold",
        ] {
            assert!(
                !lower.contains(banned),
                "{label} shipped by `rigger docs` must NOT resurrect the discredited fold-speed \
                 framing (found {banned:?}); a prune reclaims disk, it does not speed a fold"
            );
        }
    }
}

/// Spec 58, criterion 3 (the habit half ships to CONSUMERS, end to end): the three-verb lookup
/// guidance is not merely present in an in-process render - it must survive the whole composition
/// path (docs_context -> render -> write) the real `rigger docs` binary drives, so it reaches the two
/// consumer-facing files an author commits and ships. Driving the built binary and reading the
/// WRITTEN skill and handbook proves the "Looking things up" guidance actually LANDS in what
/// consumers get: the knowledge graph is the lookup surface, its three verbs each carry their
/// one-line job (`rigger graph --around` structure, `rigger graph --show` text, `rigger peers`
/// memory), and grep over the project's sources is a fallback worth REPORTING via a `grep-fallback:`
/// progress line. The implementer's in-process `discipline_carries_three_verb_lookup_guidance` unit
/// test pins the render output; this periphery layer pins that the write path in the built binary
/// carries it all the way to the consumer's files - something an in-process render assertion cannot
/// prove.
#[test]
fn docs_ships_three_verb_lookup_guidance_to_consumers() {
    let proj = temp_project();
    let root = proj.path();

    let (stdout, stderr, ok) = run_rigger(root, &["docs"]);
    assert!(ok, "rigger docs must succeed; stderr: {stderr}");
    assert!(
        stdout.contains("skills/using-rigger/SKILL.md") && stdout.contains("using-rigger.md"),
        "rigger docs must report writing both consumer-facing paths; got: {stdout}"
    );

    let skill = std::fs::read_to_string(root.join("skills/using-rigger/SKILL.md"))
        .expect("the skill was rendered to disk");
    let handbook = std::fs::read_to_string(root.join("docs/handbook/using-rigger.md"))
        .expect("the handbook chapter was rendered to disk");

    // BOTH consumer-facing outputs, as WRITTEN by the built binary, carry the lookup guidance: all
    // three verbs with their one-line jobs, and the grep-fallback reporting instruction. Both render
    // from the single `discipline_body` authority, so the skill and the handbook chapter ship the
    // guidance identically.
    for (label, out) in [("skill", &skill), ("handbook", &handbook)] {
        assert!(
            out.contains("rigger graph --around"),
            "{label} shipped by `rigger docs` must name the STRUCTURE verb `rigger graph --around`"
        );
        assert!(
            out.contains("rigger graph --show"),
            "{label} shipped by `rigger docs` must name the TEXT verb `rigger graph --show`"
        );
        assert!(
            out.contains("rigger peers"),
            "{label} shipped by `rigger docs` must name the MEMORY verb `rigger peers`"
        );
        assert!(
            out.contains("structure") && out.contains("text") && out.contains("memory"),
            "{label} shipped by `rigger docs` must name each lookup verb's job \
             (structure/text/memory)"
        );
        assert!(
            out.contains("grep-fallback:") && out.contains("rigger progress"),
            "{label} shipped by `rigger docs` must carry the grep-fallback reporting instruction"
        );
    }
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

/// Spec 68, criterion 1 (the docs-drift GATE covers the WHOLE registry, end to end): `rigger
/// validate` FAILS when the committed `planning-a-spec` skill - the second, generalized
/// registry entry - drifts from a fresh render, even while the pre-existing `using-rigger`
/// skill and the handbook stay perfectly in sync; and it stays SILENT about the untouched
/// original entry. The sibling
/// `validate_fails_when_the_committed_using_rigger_docs_drift_and_passes_when_in_sync` test
/// proves the ORIGINAL entry is still gated; this one proves the gate was not merely widened
/// to accept a second file without actually CHECKING it - a regression the implementer's own
/// in-process `install_and_docs_each_cover_exactly_the_registry_no_more_no_less` unit test
/// (which only proves `docs_drift`'s CALLER loops over the registry's names, not that each
/// iteration's byte comparison is wired to the right file) would not catch.
#[test]
fn validate_docs_drift_gate_covers_the_second_registry_entry() {
    let dir = temp_project();
    let root = dir.path();

    let (_o, err, ok) = run_rigger(root, &["init"]);
    assert!(ok, "rigger init must succeed; stderr:\n{err}");
    let (_o, err, ok) = run_rigger(root, &["docs"]);
    assert!(ok, "rigger docs must succeed; stderr:\n{err}");

    let planning_path = root.join("skills/planning-a-spec/SKILL.md");
    let using_rigger_path = root.join("skills/using-rigger/SKILL.md");
    assert!(
        planning_path.exists(),
        "rigger docs must have written the second registry entry"
    );

    // IN SYNC -> validate passes.
    let (_out, err, ok) = run_rigger(root, &["validate"]);
    assert!(
        ok,
        "validate must pass when every registry entry is in sync; stderr:\n{err}"
    );

    // Drift ONLY the second registry entry; the original entry and the handbook stay fresh.
    append_line(&planning_path, "hand-edited line the render never emits");
    let (_out, err, ok) = run_rigger(root, &["validate"]);
    assert!(
        !ok,
        "validate must FAIL when the SECOND registry entry drifts, even though the original \
         entry is untouched; stderr:\n{err}"
    );
    assert!(
        err.contains("skills/planning-a-spec/SKILL.md") && err.contains("rigger docs"),
        "the drift failure must name the drifted second entry and the `rigger docs` fix; \
         stderr:\n{err}"
    );
    assert!(
        !err.contains("skills/using-rigger/SKILL.md"),
        "the untouched original entry must NOT be reported as drifted; stderr:\n{err}"
    );

    // Re-render restores sync -> validate passes again (the gate is not stuck failing).
    let (_o, _e, ok) = run_rigger(root, &["docs"]);
    assert!(ok, "re-rendering the docs must succeed");
    let (_out, err, ok) = run_rigger(root, &["validate"]);
    assert!(
        ok,
        "validate must pass again once the second entry's drift is re-rendered; stderr:\n{err}"
    );
    assert!(
        using_rigger_path.exists(),
        "the original entry was never touched and must still exist"
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

/// Spec 68, criterion 1 (the INSTALL seam covers the WHOLE registry, end to end): `rigger
/// setup` installs EVERY entry in the registry into the consumer project, not only the
/// pre-existing `using-rigger` skill - `planning-a-spec` lands at its own installed path,
/// carrying the operator-binary prohibition, and setup REPORTS installing it. A no-op rerun
/// leaves the second entry untouched (no report line, no moved mtime), exactly like the
/// original one. Driving the real binary proves the install seam (`install_skills` looping
/// over `skill_registry()`) was actually generalized, not just the render/drift seams the
/// sibling tests above cover - install is a THIRD, independent consumer of the registry (its
/// own function, its own loop over `(name, InstallOutcome)` pairs), so a bug specific to that
/// loop (installing only the first entry, or reusing one `InstallOutcome` across every entry)
/// would pass every other test in this file and only show up here.
#[test]
fn setup_installs_every_registry_skill_into_the_consumer_project() {
    let proj = temp_project();
    let root = proj.path();

    let (out, err, ok) = run_rigger_envs(root, &["setup"], &[("RIGGER_NPM", "true")]);
    assert!(ok, "rigger setup must succeed; stderr:\n{err}");

    let using_rigger_installed = root.join(".claude/skills/using-rigger/SKILL.md");
    let planning_installed = root.join(".claude/skills/planning-a-spec/SKILL.md");
    assert!(
        using_rigger_installed.exists(),
        "setup must still install the original registry entry"
    );
    assert!(
        planning_installed.exists(),
        "setup must ALSO install the second, generalized registry entry at \
         .claude/skills/planning-a-spec/SKILL.md"
    );
    assert!(
        out.contains("planning-a-spec skill")
            && out.contains(".claude/skills/planning-a-spec/SKILL.md"),
        "setup must report installing the second registry entry too; got:\n{out}"
    );

    let installed = std::fs::read_to_string(&planning_installed)
        .expect("the second registry entry was installed");
    assert!(
        installed.starts_with("---\nname: planning-a-spec\n"),
        "the installed second entry is a loadable skill; got: {}",
        &installed[..installed.len().min(60)]
    );
    assert!(
        installed.contains("## Operator binary boundary"),
        "the installed second entry must carry the operator-binary prohibition too; \
         got:\n{installed}"
    );

    // Re-running setup on an up-to-date project is a true no-op for BOTH entries, not just the
    // original one - no file's mtime moves and no install/refresh line is printed for it.
    let before = std::fs::metadata(&planning_installed)
        .unwrap()
        .modified()
        .unwrap();
    std::thread::sleep(std::time::Duration::from_millis(20));
    let (out2, err2, ok2) = run_rigger_envs(root, &["setup"], &[("RIGGER_NPM", "true")]);
    assert!(ok2, "a no-op setup rerun must succeed; stderr:\n{err2}");
    assert!(
        !out2.contains("installed the planning-a-spec skill")
            && !out2.contains("refreshed the drifted planning-a-spec skill"),
        "an already-current second entry must not be reported as installed/refreshed; \
         got:\n{out2}"
    );
    let after = std::fs::metadata(&planning_installed)
        .unwrap()
        .modified()
        .unwrap();
    assert_eq!(
        before, after,
        "an up-to-date second entry must not even move its mtime"
    );
}

/// The five per-operation skill names spec 68, criterion 2 adds to the registry (rigger-docs,
/// rigger-validate, and rigger-setup already loop over `skill_registry()` generically as of
/// spec 68, criterion 1 - shared by the three tests below).
const PER_OPERATION_SKILL_NAMES: [&str; 5] = [
    "rigger-reset-store",
    "rigger-build-graph",
    "rigger-reindex",
    "rigger-resume-a-run",
    "rigger-handle-an-escalation",
];

/// Spec 68, criterion 2 (the render pipeline reaches all FIVE new registry entries, end to
/// end): `rigger docs` renders every per-operation skill spec 68, criterion 2 adds - not just
/// the pre-existing `using-rigger`/`planning-a-spec` pair the sibling tests above drive - each
/// to its own committed `skills/<name>/SKILL.md` path, with its own loadable frontmatter and
/// the structurally-stamped operator-binary prohibition. The sibling
/// `docs_renders_every_registry_skill_including_planning_a_spec` test proved the render
/// pipeline reaches a SECOND entry; it never drove the five entries THIS unit adds, so a
/// path-wiring bug specific to one of them (skill_source_rel mapping a name to the wrong
/// directory, or a registry entry silently dropped from the loop) would satisfy every existing
/// binary-driving test and only show up here. The implementer's own in-process
/// `write_docs_writes_every_registry_skill_plus_the_handbook` and
/// `install_and_docs_each_cover_exactly_the_registry_no_more_no_less` tests (src/main.rs) call
/// `write_docs`/`install_skills` directly in-process; this drives the actual COMPILED binary
/// and reads back what it wrote to disk, which an in-process call cannot prove.
#[test]
fn docs_renders_every_per_operation_skill_through_the_compiled_binary() {
    let proj = temp_project();
    let root = proj.path();

    let (stdout, stderr, ok) = run_rigger(root, &["docs"]);
    assert!(ok, "rigger docs must succeed; stderr: {stderr}");

    for name in PER_OPERATION_SKILL_NAMES {
        let rel = format!("skills/{name}/SKILL.md");
        assert!(
            stdout.contains(&rel),
            "rigger docs must report rendering {name} at {rel}; got: {stdout}"
        );

        let path = root.join(&rel);
        let rendered = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("rigger docs must have written {rel}: {e}"));
        assert!(
            rendered.starts_with(&format!("---\nname: {name}\n")),
            "{name}: must open with its own loadable frontmatter; got: {}",
            &rendered[..rendered.len().min(60)]
        );
        assert!(
            rendered.contains("## Procedure") && rendered.contains("## Anti-move"),
            "{name}: rendered skill must carry its Procedure and Anti-move sections; \
             got:\n{rendered}"
        );
        assert!(
            rendered.contains("## Operator binary boundary")
                && rendered.contains("never installs, replaces, or modifies the operator's"),
            "{name}: rendered skill must carry the structurally-stamped operator-binary \
             prohibition; got:\n{rendered}"
        );

        // Byte-stable across runs (the drift check the next test relies on depends on this).
        let (_o2, _e2, ok2) = run_rigger(root, &["docs"]);
        assert!(ok2, "a second `rigger docs` run must succeed");
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            rendered,
            "{name}: a second render must be byte-identical"
        );
    }
}

/// Spec 68, criterion 2 (the docs-drift GATE covers all FIVE new entries individually, end to
/// end): `rigger validate` fails when exactly ONE per-operation skill has drifted, names that
/// skill (and no other registry member), and passes again once it is re-rendered - proven for
/// EACH of the five in turn, not just one representative. The sibling
/// `validate_docs_drift_gate_covers_the_second_registry_entry` test proved the gate was
/// genuinely wired (not just widened to accept a second file without checking it) for
/// `planning-a-spec`; that same class of per-file wiring bug could affect any ONE of these
/// five committed paths independently (`docs_drift` builds its check list by mapping each
/// registry name through `skill_source_rel`, so a copy-paste mistake in that mapping for a
/// single entry would only be caught by exercising that entry's own path, not by exercising
/// any other).
#[test]
fn validate_docs_drift_gate_covers_each_per_operation_skill() {
    let dir = temp_project();
    let root = dir.path();

    let (_o, err, ok) = run_rigger(root, &["init"]);
    assert!(ok, "rigger init must succeed; stderr:\n{err}");
    let (_o, err, ok) = run_rigger(root, &["docs"]);
    assert!(ok, "rigger docs must succeed; stderr:\n{err}");

    // Baseline: every committed doc, including all five new entries, starts in sync.
    let (_out, err, ok) = run_rigger(root, &["validate"]);
    assert!(
        ok,
        "validate must pass when every registry entry (including the five new ones) is in \
         sync; stderr:\n{err}"
    );

    for name in PER_OPERATION_SKILL_NAMES {
        let path = root.join(format!("skills/{name}/SKILL.md"));
        assert!(
            path.exists(),
            "rigger docs must have written {name}'s skill"
        );

        append_line(&path, "hand-edited line the render never emits");
        let (_out, err, ok) = run_rigger(root, &["validate"]);
        assert!(!ok, "validate must FAIL when {name} drifts; stderr:\n{err}");
        assert!(
            err.contains(&format!("skills/{name}/SKILL.md")) && err.contains("rigger docs"),
            "the drift failure must name the drifted {name} skill and the `rigger docs` fix; \
             stderr:\n{err}"
        );
        for other in PER_OPERATION_SKILL_NAMES
            .iter()
            .filter(|other| **other != name)
        {
            assert!(
                !err.contains(&format!("skills/{other}/SKILL.md")),
                "{name} alone drifted, but the failure also names untouched {other}; \
                 stderr:\n{err}"
            );
        }
        assert!(
            !err.contains("skills/using-rigger/SKILL.md")
                && !err.contains("skills/planning-a-spec/SKILL.md"),
            "{name} alone drifted, but the failure also names an untouched pre-existing \
             registry entry; stderr:\n{err}"
        );

        // Re-render restores sync for every entry -> validate passes again before the next
        // iteration drifts a different one.
        let (_o, _e, ok) = run_rigger(root, &["docs"]);
        assert!(ok, "re-rendering the docs must succeed");
        let (_out, err, ok) = run_rigger(root, &["validate"]);
        assert!(
            ok,
            "validate must pass again once {name}'s drift is re-rendered; stderr:\n{err}"
        );
    }
}

/// Spec 68, criterion 2 (the INSTALL seam reaches all FIVE new entries, end to end): `rigger
/// setup` installs every per-operation skill into the consumer project at its own
/// `.claude/skills/<name>/SKILL.md` path, carrying the operator-binary prohibition, and
/// reports installing it; a no-op rerun leaves every one of them untouched (no report line, no
/// moved mtime). The sibling `setup_installs_every_registry_skill_into_the_consumer_project`
/// test proved the install seam (`install_skills` looping over `skill_registry()`) reaches a
/// second entry; install is its own function with its own loop, independent from the
/// docs/render and validate/drift seams the two tests above cover, so a bug specific to that
/// loop (skipping an entry, or reusing one `InstallOutcome`/one rendered body across several
/// entries) would pass every other test in this file and only show up by checking each
/// installed file's own name and content here.
#[test]
fn setup_installs_every_per_operation_skill_into_the_consumer_project() {
    let proj = temp_project();
    let root = proj.path();

    let (out, err, ok) = run_rigger_envs(root, &["setup"], &[("RIGGER_NPM", "true")]);
    assert!(ok, "rigger setup must succeed; stderr:\n{err}");

    let mut installed_before = Vec::new();
    for name in PER_OPERATION_SKILL_NAMES {
        let installed_path = root.join(format!(".claude/skills/{name}/SKILL.md"));
        assert!(
            installed_path.exists(),
            "setup must install {name} at .claude/skills/{name}/SKILL.md"
        );
        assert!(
            out.contains(&format!("installed the {name} skill"))
                && out.contains(&format!(".claude/skills/{name}/SKILL.md")),
            "setup must report installing {name}; got:\n{out}"
        );

        let installed = std::fs::read_to_string(&installed_path)
            .unwrap_or_else(|e| panic!("{name} was installed: {e}"));
        assert!(
            installed.starts_with(&format!("---\nname: {name}\n")),
            "the installed {name} skill must be loadable; got: {}",
            &installed[..installed.len().min(60)]
        );
        assert!(
            installed.contains("## Operator binary boundary"),
            "the installed {name} skill must carry the operator-binary prohibition too; \
             got:\n{installed}"
        );

        let mtime = std::fs::metadata(&installed_path)
            .unwrap()
            .modified()
            .unwrap();
        installed_before.push((name, installed_path, mtime));
    }

    // A no-op rerun leaves every one of the five untouched: no install/refresh report line,
    // and not even a moved mtime.
    std::thread::sleep(std::time::Duration::from_millis(20));
    let (out2, err2, ok2) = run_rigger_envs(root, &["setup"], &[("RIGGER_NPM", "true")]);
    assert!(ok2, "a no-op setup rerun must succeed; stderr:\n{err2}");
    for (name, installed_path, before) in installed_before {
        assert!(
            !out2.contains(&format!("installed the {name} skill"))
                && !out2.contains(&format!("refreshed the drifted {name} skill")),
            "an already-current {name} must not be reported as installed/refreshed; \
             got:\n{out2}"
        );
        let after = std::fs::metadata(&installed_path)
            .unwrap()
            .modified()
            .unwrap();
        assert_eq!(
            before, after,
            "an up-to-date {name} must not even move its mtime"
        );
    }
}

/// Spec 46, criterion 2 (the pre-run graph-hygiene guidance ships to CONSUMERS through the
/// INSTALL seam): a consumer never edits rigger's own repo copies - they run `rigger setup`,
/// which renders and INSTALLS the `using-rigger` skill into THEIR project at
/// `.claude/skills/using-rigger/SKILL.md`. That install path (`render_installed_skill`:
/// docs_context -> overlay merge -> render -> write) is DISTINCT from the `rigger docs`
/// repo-copy path the sibling `docs_ships_graph_hygiene_guidance_to_consumers` test drives,
/// so a regression in the install/overlay composition could ship a consumer skill missing
/// the guidance while the repo copies stay fine. This periphery test drives the real
/// `rigger setup` binary and reads the INSTALLED skill to prove the graph-hygiene section
/// reaches the consumer's project with its truthful WHY: graph.db is a PERSISTENT
/// incremental projection (a step never re-folds the whole history), so across runs it
/// accumulates dead-run rows no live query reads, which `rigger reset --runs` prunes to
/// reclaim the disk they held - and NOT the discredited fold-speed framing.
#[test]
fn setup_installs_graph_hygiene_guidance_into_consumer_skill() {
    let proj = temp_project();
    let root = proj.path();

    // npm is stubbed to a no-op so the shim provision step does not need a real npm; this
    // mirrors the sibling setup-install test.
    let (out, err, ok) = run_rigger_envs(root, &["setup"], &[("RIGGER_NPM", "true")]);
    assert!(ok, "rigger setup must succeed; stderr:\n{err}");
    assert!(
        out.contains(".claude/skills/using-rigger/SKILL.md"),
        "setup must report installing the using-rigger skill into the consumer project; got:\n{out}"
    );

    let installed = std::fs::read_to_string(root.join(".claude/skills/using-rigger/SKILL.md"))
        .expect("setup installed the consumer skill");

    // The INSTALLED consumer skill carries the graph-hygiene section, names the pre-run
    // command, and frames the truthful WHY (a persistent incremental projection whose
    // dead-run accumulation `rigger reset --runs` prunes to reclaim the disk it held) - the
    // guidance ships all the way into the consumer's own project, not just rigger's repo.
    assert!(
        installed.contains("## Graph hygiene"),
        "the installed consumer skill must carry the graph-hygiene section; got:\n{installed}"
    );
    assert!(
        installed.contains("rigger reset --runs"),
        "the installed consumer skill must name `rigger reset --runs` as the pre-run hygiene step"
    );
    assert!(
        installed.contains("persistent projection"),
        "the installed consumer skill must frame graph.db as a persistent incremental \
         projection (the corrected WHY, not a per-step whole-stream re-fold)"
    );
    assert!(
        installed.contains("reclaims the disk"),
        "the installed consumer skill must explain reset --runs reclaims the disk the dead-run \
         rows held (bounded growth, not a fold-speed claim)"
    );

    // NEGATIVE regression guard (spec 46 c2): the DISCREDITED fold-speed framing that
    // rejected this unit's first attempt (graph.db re-folded whole-history each step, the
    // fold slow in proportion to graph size, a prune speeding it up) must never reach the
    // consumer's installed skill. graph.db is a PERSISTENT incremental projection; a prune
    // reclaims DISK, it speeds no fold. Pin those phrases OUT (case-insensitively) so a
    // future edit resurrecting the false mechanism fails LOUDLY here instead of installing
    // it into consumer projects.
    let lower = installed.to_lowercase();
    for banned in [
        "re-folded each step",
        "fold stays slow",
        "proportional to graph size",
        "faster fold",
    ] {
        assert!(
            !lower.contains(banned),
            "the installed consumer skill must NOT resurrect the discredited fold-speed framing \
             (found {banned:?}); a prune reclaims disk, it does not speed a fold"
        );
    }
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
        format!("#!/bin/sh\nexec \"{}\" \"$@\"\n", rigger_bin().display()),
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

/// Seed genuinely STALE tracked copies of both rendered docs under `root`, committed with
/// `--no-verify` so a just-installed hook does NOT fire on the seed itself. Shared by the
/// spec 70 fixtures below that need real, tracked drift for the hook to detect.
fn seed_stale_tracked_docs(root: &Path) {
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
    // Guard against a vacuous pass in every caller: HEAD must carry the STALE bytes right after
    // the seed, so a later "the hook detected drift" assertion can only hold for real.
    let seeded_skill =
        git_out(root, &["show", "HEAD:skills/using-rigger/SKILL.md"]).unwrap_or_default();
    assert!(
        seeded_skill.contains("STALE DOC"),
        "the --no-verify seed must commit the STALE docs unchanged so the discrimination is \
         real, not vacuous; got:\n{seeded_skill}"
    );
}

/// Turn `root` into a rigger SELF-HOSTING repo whose tracked docs are already the TRUE fresh
/// render: `rigger setup` (npm stubbed) installs the hook, then a real `rigger docs` run seeds
/// both outputs, committed with `--no-verify` so the just-installed hook does not fire on its
/// own seed. Because the seed comes from the SAME compiled binary the hook later runs (via
/// `stage_rigger_shim`), the hook's own re-render at commit time is byte-identical to what is
/// already staged - the "matching render" fixture spec 70 crit 1 needs.
fn setup_selfhosting_repo_with_fresh_docs(root: &Path) {
    let (out, err, ok) = run_rigger_envs(root, &["setup"], &[("RIGGER_NPM", "true")]);
    assert!(ok, "rigger setup must succeed; stderr:\n{err}");
    assert!(
        out.contains("pre-commit hook"),
        "setup must install the pre-commit hook; got:\n{out}"
    );
    let (_out, err, ok) = run_rigger(root, &["docs"]);
    assert!(
        ok,
        "rigger docs must succeed while seeding a fresh render; stderr:\n{err}"
    );
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
        &["commit", "-q", "--no-verify", "-m", "seed fresh docs"],
    );
}

/// Spec 70, crit 1 (THE HOOK REFUSES INSTEAD OF REWRITING, end to end): OWNS the hook behavior.
/// In a self-hosting repo whose tracked docs have drifted from a fresh render, the managed
/// pre-commit hook must REFUSE the commit - naming the drifted files, the rendering binary's
/// path AND its build provenance, and the two remedies - rather than silently staging its own
/// re-render over them. This is the exact defect that cost three rejected attempts on one unit
/// (a binary older than the tree re-rendering committed docs to the OLD text and silently
/// staging them, stripping a branch's rendered changes from every later commit). Drives the
/// REAL `rigger` binary (via a `rigger` shim on PATH, spec 24) and REAL git.
#[test]
fn setup_precommit_hook_refuses_when_the_staged_render_has_drifted() {
    let proj = temp_git_project_with_commit();
    let root = proj.path();

    let (out, err, ok) = run_rigger_envs(root, &["setup"], &[("RIGGER_NPM", "true")]);
    assert!(ok, "rigger setup must succeed; stderr:\n{err}");
    assert!(
        out.contains("pre-commit hook"),
        "setup must report installing the pre-commit hook; got:\n{out}"
    );
    let hook_path = root.join(".git/hooks/pre-commit");
    assert!(
        hook_path.exists(),
        "setup must install .git/hooks/pre-commit"
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

    // Make this a rigger SELF-HOSTING repo with genuinely STALE tracked docs.
    seed_stale_tracked_docs(root);

    // A `rigger` shim on PATH at commit time - the hook invokes `rigger` BY NAME.
    let commit_path = stage_rigger_shim(root);

    // Make an UNRELATED tracked change and attempt to commit it.
    std::fs::write(root.join("code.txt"), "a documented code fact changed\n").unwrap();
    git_ok(root, &["add", "code.txt"]);
    let out = Command::new("git")
        .args(["commit", "-q", "-m", "change a documented fact"])
        .current_dir(root)
        .env("PATH", &commit_path)
        .output()
        .expect("git must be runnable");
    let stderr = String::from_utf8_lossy(&out.stderr);

    assert!(
        !out.status.success(),
        "a drifted render must REFUSE the commit, not silently let it through; stderr:\n{stderr}"
    );
    assert!(
        stderr.contains("skills/using-rigger/SKILL.md")
            && stderr.contains("docs/handbook/using-rigger.md"),
        "the refusal must name BOTH drifted files; stderr:\n{stderr}"
    );
    let shim_rigger = root.join("shim-bin").join("rigger");
    assert!(
        stderr.contains(shim_rigger.to_str().unwrap()),
        "the refusal must name the rendering binary's PATH; stderr:\n{stderr}"
    );
    assert!(
        stderr.contains("build "),
        "the refusal must name the rendering binary's BUILD PROVENANCE (`rigger version`); \
         stderr:\n{stderr}"
    );
    assert!(
        stderr.to_lowercase().contains("reinstall") && stderr.contains("tree-built"),
        "the refusal must name the two remedies - re-render with the tree-built binary, or \
         reinstall; stderr:\n{stderr}"
    );

    // Nothing landed: HEAD still carries the STALE seed, not a silently-substituted re-render.
    let committed =
        git_out(root, &["show", "HEAD:skills/using-rigger/SKILL.md"]).unwrap_or_default();
    assert!(
        committed.contains("STALE DOC"),
        "a refused commit must not land - HEAD must still carry the stale seed; got:\n{committed}"
    );
    // And nothing was re-staged: the hook never ran `git add`, so the freshly re-rendered
    // working-tree copy (written by the hook's own `rigger docs`) is only an unstaged edit.
    let staged = git_out(root, &["diff", "--cached", "--name-only"]).unwrap_or_default();
    assert!(
        !staged.contains("SKILL.md") && !staged.contains("using-rigger.md"),
        "the hook must never stage its own re-render; staged files:\n{staged}"
    );
}

/// Spec 70, crit 1 (a MATCHING render passes silently, end to end): the flip side of refusing
/// instead of rewriting. When the committed docs are ALREADY the fresh render, the hook must
/// change nothing and let the commit through exactly as before this fix - no warning, no
/// refusal, no touched doc content. Drives the REAL `rigger` binary and REAL git.
#[test]
fn setup_precommit_hook_passes_untouched_when_the_render_matches() {
    let proj = temp_git_project_with_commit();
    let root = proj.path();
    setup_selfhosting_repo_with_fresh_docs(root);
    let fresh_skill_before =
        git_out(root, &["show", "HEAD:skills/using-rigger/SKILL.md"]).unwrap_or_default();
    assert!(
        fresh_skill_before.contains("name: using-rigger"),
        "the seed must be a real fresh render, not a stub; got:\n{fresh_skill_before}"
    );

    let commit_path = stage_rigger_shim(root);
    std::fs::write(root.join("code.txt"), "an unrelated change\n").unwrap();
    git_ok(root, &["add", "code.txt"]);
    let out = Command::new("git")
        .args(["commit", "-q", "-m", "unrelated change"])
        .current_dir(root)
        .env("PATH", &commit_path)
        .output()
        .expect("git must be runnable");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        out.status.success(),
        "a matching render must pass the commit through untouched; stderr:\n{stderr}"
    );
    assert!(
        !stderr.contains("refusing"),
        "a matching render must never print a refusal; stderr:\n{stderr}"
    );

    let tree = git_out(root, &["ls-tree", "-r", "--name-only", "HEAD"]).unwrap_or_default();
    assert!(
        tree.contains("code.txt"),
        "the unrelated change must ride the commit; tree:\n{tree}"
    );
    let committed_after =
        git_out(root, &["show", "HEAD:skills/using-rigger/SKILL.md"]).unwrap_or_default();
    assert_eq!(
        committed_after, fresh_skill_before,
        "the already-fresh doc must land byte-identical - the hook must not touch it"
    );
}

/// Spec 68, criterion 1 (the fast pre-commit hook's scope is a DELIBERATE non-generalization,
/// end to end): the commit-time hook still compares ONLY the `using-rigger` skill and the
/// handbook chapter - spec 70's pre-existing, hand-enumerated scope - and was NOT widened to
/// walk the whole registry (`rigger validate` is the gate that covers every registry entry;
/// see `validate_docs_drift_gate_covers_the_second_registry_entry` above). This matters MORE,
/// not less, now that `write_docs` was generalized: the hook's own internal `rigger docs` call
/// (it re-renders before comparing) now rewrites EVERY registry file on disk as a side effect,
/// including `planning-a-spec` - so this test also re-pins spec 70's "never stages anything
/// itself" invariant across that wider write: an untracked, hand-edited `planning-a-spec`
/// working-tree copy must survive the hook's internal re-render UNABSORBED by git (never
/// silently added to an unrelated commit), and the commit must never be blocked or slowed by
/// it, exactly as if the hook had never touched it at all.
#[test]
fn setup_precommit_hook_never_drift_checks_or_stages_a_registry_entry_outside_its_scope() {
    let proj = temp_git_project_with_commit();
    let root = proj.path();
    setup_selfhosting_repo_with_fresh_docs(root);

    // `setup_selfhosting_repo_with_fresh_docs` only tracks the two spec-70 files; the second
    // registry entry that `rigger docs` also wrote during the fixture is left UNTRACKED - the
    // exact state an operator mid-edit on a new skill would be in.
    let planning_path = root.join("skills/planning-a-spec/SKILL.md");
    assert!(
        planning_path.exists(),
        "the fixture's `rigger docs` call must have written the second registry entry too"
    );
    let status_before = git_out(
        root,
        &["status", "--porcelain", "skills/planning-a-spec/SKILL.md"],
    )
    .unwrap_or_default();
    assert!(
        status_before.starts_with("??"),
        "the second registry entry must start UNTRACKED (never staged by the fixture); \
         got:\n{status_before}"
    );

    // Simulate an operator's in-progress, unstaged hand-edit to it.
    std::fs::write(&planning_path, "WIP hand-edit, not a render\n").unwrap();

    let commit_path = stage_rigger_shim(root);
    std::fs::write(root.join("code.txt"), "an unrelated change\n").unwrap();
    git_ok(root, &["add", "code.txt"]);
    let out = Command::new("git")
        .args(["commit", "-q", "-m", "unrelated change"])
        .current_dir(root)
        .env("PATH", &commit_path)
        .output()
        .expect("git must be runnable");
    let stderr = String::from_utf8_lossy(&out.stderr);

    assert!(
        out.status.success(),
        "the commit must succeed - the hook must never drift-check or block on a registry \
         entry outside its fixed scope; stderr:\n{stderr}"
    );
    assert!(
        !stderr.to_lowercase().contains("refusing"),
        "the hook must never refuse over a registry entry it does not scope; stderr:\n{stderr}"
    );

    let tree = git_out(root, &["ls-tree", "-r", "--name-only", "HEAD"]).unwrap_or_default();
    assert!(
        tree.contains("code.txt"),
        "the unrelated change must ride the commit; tree:\n{tree}"
    );
    assert!(
        !tree.contains("planning-a-spec"),
        "the out-of-scope registry entry must NEVER be staged/committed by the hook's internal \
         re-render, even though that render rewrote its bytes on disk; tree:\n{tree}"
    );
    // It stays exactly as untracked as before - the hook's internal `rigger docs` call may
    // have rewritten its BYTES, but git's view of it (untracked) is unchanged.
    let status_after = git_out(
        root,
        &["status", "--porcelain", "skills/planning-a-spec/SKILL.md"],
    )
    .unwrap_or_default();
    assert!(
        status_after.starts_with("??"),
        "the out-of-scope entry must still be untracked after the commit, unabsorbed by it; \
         got:\n{status_after}"
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
    let (out, err, ok) = run_rigger_envs(root, &["setup"], &[("RIGGER_NPM", "true")]);
    assert!(ok, "rigger setup must succeed; stderr:\n{err}");
    assert!(
        out.contains("pre-commit hook"),
        "setup must install the pre-commit hook; got:\n{out}"
    );
    seed_stale_tracked_docs(root);
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
            rigger_bin().display()
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
/// the pre-existing hook AND rigger's docs check. Regression-guards
/// adv-u24-1r-chained-terminal-hook-shadows-rigger-block-silently (d24-11): appending rigger's
/// block after such a hook would let the `exit 0` silently shadow it. Uses the MATCHING-render
/// fixture (spec 70, crit 1) rather than a stale one, since a REFUSED commit never reaches the
/// chained hook at all by design - this test isolates the chaining/ordering property alone.
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
    // ALREADY-FRESH tracked docs so the final commit's hook finds no drift and falls through.
    setup_selfhosting_repo_with_fresh_docs(root);
    let fresh_skill_before =
        git_out(root, &["show", "HEAD:skills/using-rigger/SKILL.md"]).unwrap_or_default();

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
        "a matching render must let the commit through - the hook must never block it"
    );

    // The PRE-EXISTING hook still ran (its side effect is present)...
    assert!(
        root.join("USER_HOOK_RAN").exists(),
        "the pre-existing hook must still run when chained"
    );
    // ...AND rigger's block ALSO ran despite the existing hook's terminal `exit 0`: it checked
    // the docs (no drift found) and fell through, landing the SAME fresh bytes unchanged. A
    // terminal-shadow bug (append-after) would have skipped rigger's block entirely, which this
    // cannot distinguish from "ran and found nothing to do" - the reachability is proven by the
    // block-position assertion above; this proves it did not somehow corrupt what it read.
    let committed =
        git_out(root, &["show", "HEAD:skills/using-rigger/SKILL.md"]).unwrap_or_default();
    assert_eq!(
        committed, fresh_skill_before,
        "rigger's block must leave an already-fresh doc byte-identical; got:\n{committed}"
    );
}

/// Spec 70, crit 1 (comparison scope, end to end): the hook reads and compares ONLY the two
/// rendered doc outputs; it never touches any other working-tree file. Since the hook no longer
/// stages anything at all (it only ever refuses or falls through), an UNTRACKED junk file and an
/// UNSTAGED edit to an unrelated tracked file both stay exactly as the operator left them across
/// a commit whose docs are already the fresh render.
#[test]
fn setup_precommit_hook_never_touches_unrelated_files() {
    let proj = temp_git_project_with_commit();
    let root = proj.path();

    // A tracked file whose later UNSTAGED modification must NOT ride the commit.
    std::fs::write(root.join("other.txt"), "original\n").unwrap();
    git_ok(root, &["add", "other.txt"]);
    git_ok(
        root,
        &["commit", "-q", "--no-verify", "-m", "add other.txt"],
    );

    setup_selfhosting_repo_with_fresh_docs(root);
    let commit_path = stage_rigger_shim(root);

    // Working-tree noise the hook must NOT touch: an UNTRACKED junk file and an UNSTAGED edit to
    // a tracked file.
    std::fs::write(root.join("junk.txt"), "not for the commit\n").unwrap();
    std::fs::write(root.join("other.txt"), "MODIFIED but not staged\n").unwrap();

    // Stage ONE unrelated change and commit; the docs are already fresh so nothing else happens.
    std::fs::write(root.join("trigger.txt"), "trigger\n").unwrap();
    git_ok(root, &["add", "trigger.txt"]);
    let commit_ok = Command::new("git")
        .args(["commit", "-q", "-m", "trigger"])
        .current_dir(root)
        .env("PATH", &commit_path)
        .status()
        .expect("git must be runnable")
        .success();
    assert!(commit_ok, "a matching render must let the commit through");

    let tree = git_out(root, &["ls-tree", "-r", "--name-only", "HEAD"]).unwrap_or_default();
    assert!(
        tree.contains("trigger.txt"),
        "the staged change must ride the commit; tree:\n{tree}"
    );
    assert!(
        !tree.contains("junk.txt"),
        "the hook must never stage an unrelated untracked file; tree:\n{tree}"
    );
    let committed_other = git_out(root, &["show", "HEAD:other.txt"]).unwrap_or_default();
    assert_eq!(
        committed_other, "original",
        "the hook must never stage an unrelated tracked file's unstaged modification; got:\n{committed_other}"
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

/// Spec 70, crit 1 (a REFUSAL aborts the WHOLE hook, including anything chained after it, end
/// to end): rigger's block is inserted BEFORE a pre-existing hook's body (spec 24 chaining), and
/// its `exit 1` on a detected drift is deliberately NOT cooperative - `precommit_block`'s own doc
/// comment states it "must abort the whole hook (including anything chained after it), because a
/// commit that is about to be refused should not go on to run further gates." No existing test
/// drives this: the chaining test
/// (`setup_precommit_hook_chains_after_a_terminal_exit_hook_and_still_runs`) deliberately swaps
/// to a MATCHING-render fixture specifically to stay off the refusal path, so this exact safety
/// claim has zero coverage until this test. Drives the refusal path with a pre-existing chained
/// hook present and proves its body never runs.
#[test]
fn setup_precommit_hook_refusal_aborts_a_chained_hook_body() {
    let proj = temp_git_project_with_commit();
    let root = proj.path();

    // A pre-existing pre-commit hook that leaves a detectable side effect if it runs.
    let hooks = root.join(".git/hooks");
    std::fs::create_dir_all(&hooks).unwrap();
    let user_hook = hooks.join("pre-commit");
    std::fs::write(&user_hook, "#!/bin/sh\ntouch USER_HOOK_RAN\nexit 0\n").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&user_hook, std::fs::Permissions::from_mode(0o755)).unwrap();
    }

    // `rigger setup` chains its block BEFORE the existing hook body (spec 24); seed genuinely
    // STALE tracked docs so the final commit's hook has real drift to refuse.
    let (setup_out, setup_err, setup_ok) =
        run_rigger_envs(root, &["setup"], &[("RIGGER_NPM", "true")]);
    assert!(setup_ok, "rigger setup must succeed; stderr:\n{setup_err}");
    assert!(
        setup_out.contains("pre-commit hook"),
        "setup must install the pre-commit hook; got:\n{setup_out}"
    );
    seed_stale_tracked_docs(root);

    let hook = std::fs::read_to_string(&user_hook).unwrap();
    assert!(
        hook.contains("touch USER_HOOK_RAN") && hook.contains("rigger docs"),
        "the chained hook must preserve the user hook AND carry rigger's block; got:\n{hook}"
    );

    let commit_path = stage_rigger_shim(root);
    std::fs::write(root.join("code.txt"), "a documented fact changed\n").unwrap();
    git_ok(root, &["add", "code.txt"]);
    let out = Command::new("git")
        .args(["commit", "-q", "-m", "change a documented fact"])
        .current_dir(root)
        .env("PATH", &commit_path)
        .output()
        .expect("git must be runnable");

    assert!(
        !out.status.success(),
        "a drifted render must refuse the commit even with a chained hook present"
    );
    assert!(
        !root.join("USER_HOOK_RAN").exists(),
        "a refusal must abort the WHOLE hook script before reaching the chained body that \
         follows rigger's block - the pre-existing hook's side effect must never appear"
    );
}

/// Spec 70, crit 1 (a refusal names ONLY the file that actually drifted, end to end): the hook
/// judges each tracked doc independently (`git diff --quiet -- "$doc"` runs per file), and only a
/// file that differs is added to the refusal's file list. Every existing refusal fixture drifts
/// BOTH docs at once, so none can discriminate "names every tracked file" from "names only the
/// drifted ones". This fixture drifts ONLY the handbook, leaving the skill genuinely fresh, and
/// proves the refusal names the handbook alone.
#[test]
fn setup_precommit_hook_refusal_names_only_the_drifted_file() {
    const HANDBOOK_REL: &str = "docs/handbook/using-rigger.md";
    let proj = temp_git_project_with_commit();
    let root = proj.path();
    setup_selfhosting_repo_with_fresh_docs(root);
    let fresh_skill =
        git_out(root, &["show", "HEAD:skills/using-rigger/SKILL.md"]).unwrap_or_default();

    // Drift ONLY the handbook back to a stale committed copy; the skill stays the true fresh
    // render committed by `setup_selfhosting_repo_with_fresh_docs`.
    std::fs::write(root.join(HANDBOOK_REL), "STALE DOC - not a real render\n").unwrap();
    git_ok(root, &["add", HANDBOOK_REL]);
    git_ok(
        root,
        &[
            "commit",
            "-q",
            "--no-verify",
            "-m",
            "drift only the handbook",
        ],
    );
    let seeded_handbook =
        git_out(root, &["show", &format!("HEAD:{HANDBOOK_REL}")]).unwrap_or_default();
    assert!(
        seeded_handbook.contains("STALE DOC") && !fresh_skill.contains("STALE DOC"),
        "the fixture must leave exactly one doc stale and the other genuinely fresh"
    );

    let commit_path = stage_rigger_shim(root);
    std::fs::write(root.join("code.txt"), "a documented fact changed\n").unwrap();
    git_ok(root, &["add", "code.txt"]);
    let out = Command::new("git")
        .args(["commit", "-q", "-m", "change a documented fact"])
        .current_dir(root)
        .env("PATH", &commit_path)
        .output()
        .expect("git must be runnable");
    let stderr = String::from_utf8_lossy(&out.stderr);

    assert!(
        !out.status.success(),
        "a single drifted doc must still refuse the commit; stderr:\n{stderr}"
    );
    assert!(
        stderr.contains(HANDBOOK_REL),
        "the refusal must name the drifted handbook; stderr:\n{stderr}"
    );
    assert!(
        !stderr.contains("skills/using-rigger/SKILL.md"),
        "the refusal must NOT name the still-fresh skill - only the actually drifted file; \
         stderr:\n{stderr}"
    );
}

/// Spec 70, crit 1 (the refused working tree already carries the fresh render, end to end): the
/// hook runs `rigger docs` BEFORE it compares, so by the time it refuses, the fresh render is
/// already sitting in the working tree as an unstaged edit - the printed remedy ("re-render with
/// the tree-built binary ... then git add the result") only makes sense if that render is
/// already there. No existing test reads the raw working-tree file after a refusal (only HEAD
/// and the index are checked); this proves the operator does not need to re-run `rigger docs`
/// themselves to recover.
#[test]
fn setup_precommit_hook_refusal_leaves_the_fresh_render_in_the_working_tree() {
    let proj = temp_git_project_with_commit();
    let root = proj.path();
    let (out, err, ok) = run_rigger_envs(root, &["setup"], &[("RIGGER_NPM", "true")]);
    assert!(ok, "rigger setup must succeed; stderr:\n{err}");
    assert!(
        out.contains("pre-commit hook"),
        "setup must install the pre-commit hook; got:\n{out}"
    );
    seed_stale_tracked_docs(root);

    let commit_path = stage_rigger_shim(root);
    std::fs::write(root.join("code.txt"), "a documented fact changed\n").unwrap();
    git_ok(root, &["add", "code.txt"]);
    let commit_out = Command::new("git")
        .args(["commit", "-q", "-m", "change a documented fact"])
        .current_dir(root)
        .env("PATH", &commit_path)
        .output()
        .expect("git must be runnable");
    assert!(
        !commit_out.status.success(),
        "the drifted render must refuse the commit"
    );

    // The raw working-tree file (NOT `git show HEAD:...`, NOT the index) must already hold the
    // fresh render the hook's own `rigger docs` call wrote before comparing - not the stale seed.
    let working_tree_skill =
        std::fs::read_to_string(root.join("skills/using-rigger/SKILL.md")).unwrap();
    assert!(
        working_tree_skill.contains("name: using-rigger")
            && !working_tree_skill.contains("STALE DOC"),
        "the working tree must already carry the fresh render after a refusal, so the printed \
         remedy's `git add` step has real fresh content to stage; got:\n{working_tree_skill}"
    );
    // And it is genuinely UNSTAGED - the hook never ran `git add` on its own re-render.
    let unstaged = git_out(root, &["diff", "--name-only"]).unwrap_or_default();
    assert!(
        unstaged.contains("skills/using-rigger/SKILL.md"),
        "the fresh render in the working tree must be an unstaged edit, not already staged; \
         diff --name-only:\n{unstaged}"
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
/// behavior under test) - returning (stdout, stderr). Used by the spec-39/50 step-path dash
/// tests, which reap any dash they start.
///
/// The dash's ensure port is pinned to `dash_port` via `RIGGER_DASH_PORT` (the same seam the
/// production singleton resolves through, defaulting to `dash::DEFAULT_PORT`). Passing an ephemeral
/// `free_loopback_port` here is what keeps these tests HERMETIC: they exercise the real ensure path
/// WITHOUT binding the machine-fixed default, so they never fight a genuine always-on dash already
/// serving 7420 on the self-hosting box - exactly as the direct-`rigger dash` singleton test injects
/// an ephemeral port. Two calls with the SAME `dash_port` model the same run's successive steps
/// (they must find the one dash the first step started); distinct ports model independent runs.
fn run_step_dash_enabled(root: &Path, dash_port: u16) -> (String, String) {
    let out = common::rigger_courier()
        .args(["step"])
        .current_dir(root)
        // Redirect the machine-global registry (spec 50, criterion 2) into the test's own temp
        // tree so this step registers under `root/rigger`, never the operator's real
        // ~/.local/state/rigger/instances.
        .env("XDG_STATE_HOME", root)
        // Pin the ensure port to the test's own ephemeral loopback port so the step-path dash never
        // binds the machine-fixed default and never collides with a real always-on dash on 7420.
        .env("RIGGER_DASH_PORT", dash_port.to_string())
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
// Hermetic against a real machine dash: each of the three step-path dash tests spawns a real
// `rigger step` whose always-on ensure binds the singleton's default address (spec 50, criterion 4)
// - which, on the self-hosting box, a genuine always-on dash already holds on the fixed 7420. Each
// test therefore pins the ensure port to its OWN ephemeral `free_loopback_port` via `RIGGER_DASH_PORT`
// (the same seam production resolves through, defaulting to `dash::DEFAULT_PORT`), so it exercises the
// real ensure path WITHOUT fighting that machine dash - exactly as the direct-`rigger dash` singleton
// test injects an ephemeral port. Distinct private ports per test mean they no longer share any port,
// so no `serial` key is needed (the same reason the direct-`rigger dash` port tests need none).
#[test]
fn step_auto_starts_one_persistent_dash_and_a_second_step_starts_none() {
    let proj = temp_git_project_with_commit();
    let root = proj.path();
    write_two_stage_workflow(root);
    // This run's ephemeral singleton port: BOTH steps below share it (they model one run's
    // successive steps, which must find the one dash the first started), and it is never 7420.
    let dash_port = free_loopback_port();

    // First step: no dash is recorded, so it must start one and record its marker.
    let (out1, err1) = run_step_dash_enabled(root, dash_port);
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
    let (_out2, err2) = run_step_dash_enabled(root, dash_port);
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

/// Spec 50, criterion 4 (stable fixed address) end-to-end at the BUILT binary: the
/// `RIGGER_DASH_PORT` seam the step-path ensure resolves through actually reaches the bind. The
/// pure unit test `dash_ensure_port_defaults_to_the_fixed_address_and_only_a_valid_override_relocates_it`
/// proves the RESOLUTION table (an unset override binds `dash::DEFAULT_PORT`; a valid `u16`
/// relocates it); only driving a real `rigger step` proves the WIRING - that the resolved port
/// reaches `spawn_run_dashboard_detached`'s actual bind. A step under `RIGGER_DASH_PORT=<port>`
/// must start its detached dash at EXACTLY that port: the recorded `.rigger/dash.marker` names it
/// AND a live process is genuinely serving there.
///
/// This is the periphery assertion the other step-path dash tests deliberately do NOT make: they
/// pass the override only to stay hermetic and then read back whatever port the marker names, so a
/// regression that IGNORED the env and always bound `dash::DEFAULT_PORT` would still pass them on a
/// clean machine (none assert the recorded port equals the injected one) and surface only as
/// flakiness on a box where 7420 is already held. Asserting marker-port == injected-port closes
/// that gap. The complementary unset -> `dash::DEFAULT_PORT` fallback is left to the pure unit test
/// on purpose: asserting it end-to-end is exactly the fixed-7420 machine-dash fight this unit
/// removed, so re-introducing it here would re-introduce the flakiness.
///
/// The started dash is a real, long-lived detached process, so this test REAPS it by pid BEFORE
/// its assertions - a failed assertion never leaks a dashboard.
// Hermetic against a real machine dash: pins the ensure port to its OWN ephemeral
// `free_loopback_port` (never the fixed 7420 a genuine always-on dash holds on the self-hosting
// box), so it drives the real ensure/bind path without fighting that machine dash - the same
// reason the other step-path dash tests need no `serial` key.
#[test]
fn step_dash_binds_exactly_the_rigger_dash_port_override() {
    let proj = temp_git_project_with_commit();
    let root = proj.path();
    write_two_stage_workflow(root);
    // The override the real binary must honor at the bind: an ephemeral loopback port, never 7420.
    let dash_port = free_loopback_port();

    let (out, err) = run_step_dash_enabled(root, dash_port);
    assert!(
        out.contains(r#""wave":"#),
        "the step must run to completion (a printed wave), reaching the dash-start seam; \
         stdout: {out:?} stderr: {err:?}"
    );
    let (marker_port, pid) = read_dash_marker(root).unwrap_or_else(|| {
        panic!("the step must record a dash marker at .rigger/dash.marker; stderr:\n{err}")
    });

    // Prove a GENUINE process bound exactly the injected port, not merely that the marker names it:
    // an HTTP GET of that port's loopback URL returns the read-only page. Capture the result BEFORE
    // reaping so the assertion never depends on the child still being alive.
    let url = format!("http://127.0.0.1:{dash_port}/");
    let served_at_override = matches!(http_get(&url), Some(body) if body.contains("rigger dash"));

    // Reap the real detached dash BEFORE any assertion, so a failed assertion never leaks it.
    reap_pid(pid);

    assert_eq!(
        marker_port, dash_port,
        "`rigger step` under RIGGER_DASH_PORT={dash_port} must bind its step-path dash at EXACTLY \
         that port and record it in .rigger/dash.marker (proving the override reaches the real \
         bind, not the fixed dash::DEFAULT_PORT); the marker instead recorded {marker_port}"
    );
    assert!(
        served_at_override,
        "a real detached dash must be genuinely serving at the injected RIGGER_DASH_PORT={dash_port} \
         - the override must reach the actual bind, not just the recorded marker value"
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

/// Spec 50, criterion 4 (opt-out): the CONFIG opt-out `dash: off` in workflow.yml suppresses the
/// always-on ensure exactly like the env opt-out - a step under it runs to completion (prints its
/// wave) yet binds NO dash and records NO `.rigger/dash.marker`, so a headless/CI run configured
/// `dash: off` proceeds with no dash and no port bind. The env opt-out is REMOVED here so the
/// config key is the ONLY thing suppressing the dash; the companion
/// `step_honors_the_rigger_no_dash_opt_out` proves the ENV path, and
/// `step_auto_starts_one_persistent_dash_and_a_second_step_starts_none` proves the SAME step path
/// DOES start one with NO opt-out, so this absence is the config opt-out at work, not a dead path.
/// The run still REGISTERS its instance (criterion 2) under the redirected state dir - "the run
/// proceeds normally" - so the opt-out drops only the dash, never the run.
// Hermetic against a real machine dash: were the config opt-out to regress, the ensure would bind
// a dash - but the ensure port is pinned to this test's own ephemeral `free_loopback_port` (never
// the fixed 7420 a genuine always-on dash holds on the self-hosting box), so even the regression
// path binds a private port and the `no marker` assertion still catches it. No `serial` key is
// needed: the test no longer touches the shared default port on any path.
#[test]
fn step_honors_the_config_dash_off_opt_out() {
    let proj = temp_git_project_with_commit();
    let root = proj.path();
    write_two_stage_workflow(root);
    // Opt out via the config key alone: append `dash: off` at the top level of workflow.yml.
    append_line(&root.join(".rigger").join("workflow.yml"), "dash: off");

    // RIGGER_NO_DASH REMOVED so ONLY `dash: off` can suppress the dash; the state dir is redirected
    // so the run's registration lands in the test's own tree, never the operator's real one; and the
    // ensure port is pinned to an ephemeral loopback port so a regression never binds the fixed 7420.
    let out = common::rigger_courier()
        .args(["step"])
        .current_dir(root)
        .env("XDG_STATE_HOME", root)
        .env("RIGGER_DASH_PORT", free_loopback_port().to_string())
        .env_remove("RIGGER_NO_DASH")
        .output()
        .expect("failed to spawn the rigger binary");
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();

    // If the opt-out regressed, a real dash started at the fixed port and recorded a marker: reap
    // it BEFORE asserting so a failing assertion never leaks a dashboard.
    if let Some((_, pid)) = read_dash_marker(root) {
        reap_pid(pid);
    }

    assert!(
        stdout.contains(r#""wave":"#),
        "the step runs to completion (a printed wave) with `dash: off`; stdout: {stdout:?} \
         stderr: {stderr:?}"
    );
    assert!(
        !root.join(".rigger").join("dash.marker").exists(),
        "under `dash: off` the step must record NO dash marker (no dash was started); one was written"
    );
    assert!(
        !stderr.contains("serving this run"),
        "under `dash: off` the step announces no dash; stderr:\n{stderr}"
    );
    // The run still proceeded normally: it registered its instance (criterion 2) even with the
    // dash opted out, proving the opt-out drops only the dash, not the run.
    let instances = root.join("rigger").join("instances");
    let registered = std::fs::read_dir(&instances)
        .map(|d| d.flatten().next().is_some())
        .unwrap_or(false);
    assert!(
        registered,
        "with `dash: off` the run still registers its instance under {}; none found",
        instances.display()
    );
}

/// Spec 50, criterion 4 (opt-out) - the CONTRACT and BACK-COMPAT of the config opt-out at the
/// PUBLIC config-load boundary. The step-path opt-out reads `Workflow::dash_enabled()` off the
/// workflow the binary loads through the public `rigger::config::load`; this drives that REAL load
/// path - parse AND validate a FULL, valid `workflow.yml` on disk (agents + stages + defaults), the
/// exact fixture `rigger step` loads - rather than a bare `serde_yaml::from_str` of a one-key
/// document. It therefore guards the whole outside-in surface an in-crate unit test cannot reach.
/// First, the new `dash` key is PUBLIC and honored end-to-end through `config::load`. Second,
/// BACK-COMPAT: a pre-existing `workflow.yml` that says nothing about the dash still loads and keeps
/// the always-on promise (`dash_enabled()` true), so old configs are never broken. Third, the
/// `dash` field composes inside a complete, VALIDATED workflow, not a one-key document. The
/// documented bare `dash: off` / `dash: on` and the quoted `false`/`no`/`true`/`OFF`/` off `
/// synonyms (case- and whitespace-insensitive) resolve as the opt-out spec names. This is the
/// config-side contract that the CLI test `step_honors_the_config_dash_off_opt_out` then proves
/// end-to-end at the binary; this side is pure `config::load`, binds no port, and spawns no process.
#[test]
fn config_load_dash_enabled_is_the_public_opt_out_contract_and_back_compat() {
    // The SAME full, valid project the real loader accepts that `rigger step` drives: an agents dir
    // plus a two-stage `workflow.yml`, so this exercises the exact production `config::load` path.
    let proj = temp_git_project_with_commit();
    let root = proj.path();
    let dir = root.to_str().expect("utf-8 project path");
    let wf_path = root.join(".rigger").join("workflow.yml");

    // BACK-COMPAT: the fixture's `workflow.yml` says NOTHING about the dash - exactly as every
    // config authored before this key did. It still loads AND keeps the always-on promise.
    write_two_stage_workflow(root);
    let cfg = rigger::config::load(dir).expect("a workflow that omits `dash` still loads");
    assert!(
        cfg.workflow.dash_enabled(),
        "an omitted `dash` key keeps the always-on dash ON (back-compat)"
    );

    // Every opt-out value resolves the dash OFF through the real load path. Each case rewrites the
    // fixture fresh (never appending a SECOND `dash:` onto a prior one), so the result is
    // order-insensitive and no duplicate-key parse error can mask a regression. `dash: off` is the
    // BARE documented form; the rest are quoted to pin the case- and whitespace-insensitive match.
    let off_forms: &[&str] = &[
        "dash: off",
        r#"dash: "OFF""#,
        r#"dash: " off ""#,
        r#"dash: "Off""#,
        r#"dash: "false""#,
        r#"dash: "no""#,
    ];
    for form in off_forms {
        write_two_stage_workflow(root);
        append_line(&wf_path, form);
        let cfg = rigger::config::load(dir).expect("a workflow with `dash` set still loads");
        assert!(
            !cfg.workflow.dash_enabled(),
            "`{form}` must resolve the opt-out OFF through config::load"
        );
    }

    // A truthy / empty value keeps the always-on dash ON through the same path. `dash: on` is the
    // BARE documented truthy counterpart to `dash: off`; `true` and the empty string are quoted.
    let on_forms: &[&str] = &["dash: on", r#"dash: "true""#, r#"dash: """#];
    for form in on_forms {
        write_two_stage_workflow(root);
        append_line(&wf_path, form);
        let cfg = rigger::config::load(dir).expect("a workflow with `dash` set still loads");
        assert!(
            cfg.workflow.dash_enabled(),
            "`{form}` keeps the always-on dash ON through config::load"
        );
    }
}

// --- Spec 39, criterion 2: the step-started dash is DETACHED - it persists across the run's
// many short-lived `step` processes. Criterion 1 (above) owns start-once idempotency; THIS
// criterion owns PERSISTENCE: the dash a `step` starts is still serving after that step process
// has exited, and stays the same live process across a LATER step process, because it is NOT
// bound to a `ReapedChild` in any step process (a guard-bound dash would be reaped the instant
// the step's `main` returned, and nothing would be alive or serving afterwards).

/// Spec 39, criterion 2 end-to-end, through the BUILT binary: a run dashboard started by one
/// `rigger step` OUTLIVES that step process and keeps serving, and is still the SAME live
/// process after a LATER step of the same run has itself started and exited - proving the dash
/// is DETACHED, not bound to a per-step [`rigger::dash::ReapedChild`]. `run_step_dash_enabled`
/// waits on the step process before returning, so every observation below happens strictly
/// AFTER that process is gone; were the dash guard-bound, its `ReapedChild::drop` would have
/// killed+reaped it as the step's `main` returned and neither `pid_is_alive` nor an HTTP GET
/// would hold here. The main.rs/dash.rs unit tests inject the spawn and never fork a real step
/// process, so this persistence-across-a-process-boundary is the layer they are structurally
/// blind to.
///
/// This test OWNS persistence, NOT the idempotent no-op (criterion 1's): it asserts only that
/// the ORIGINAL dash lives and serves across step-process boundaries, and never that a second
/// step started no second dash. It reaps every dash it could have started BEFORE any assertion,
/// so a failed assertion never leaks a dashboard.
// Hermetic against a real machine dash: pins the ensure port to its own ephemeral
// `free_loopback_port` (never the fixed 7420 a genuine always-on dash holds on the self-hosting
// box), so it exercises the real detached-dash persistence path without fighting that machine dash
// (see `step_auto_starts_one_persistent_dash_and_a_second_step_starts_none` for the full rationale).
#[test]
fn a_step_started_dash_is_detached_and_outlives_its_step_process() {
    let proj = temp_git_project_with_commit();
    let root = proj.path();
    write_two_stage_workflow(root);
    // Both steps of this one run share the same ephemeral singleton port (never 7420).
    let dash_port = free_loopback_port();

    // A first, now-EXITED step process starts the detached dash and records its (port, pid).
    let (out1, err1) = run_step_dash_enabled(root, dash_port);
    assert!(
        out1.contains(r#""wave":"#),
        "the first step must run to completion (a printed wave), reaching the dash-start seam; \
         stdout: {out1:?} stderr: {err1:?}"
    );
    let (port, pid) = read_dash_marker(root).unwrap_or_else(|| {
        panic!("the first step must record a dash marker at .rigger/dash.marker; stderr:\n{err1}")
    });
    let url = format!("http://127.0.0.1:{port}/");

    // The step process has already been waited on, so it is GONE. A `ReapedChild`-bound dash
    // would have been reaped on that process's return; a detached one is still alive AND serving.
    // Probe both liveness (the pid) and reachability (the served page).
    let alive_after_step1 = rigger::dash::pid_is_alive(pid);
    let served_after_step1 = matches!(http_get(&url), Some(body) if body.contains("rigger dash"));

    // Drive a SECOND, independent step process to completion and let it too exit. The dash must
    // persist ACROSS this step boundary as the very same live process (persistence, NOT
    // idempotency: we assert the ORIGINAL pid still lives and serves, never that no second dash
    // was started - that no-op is criterion 1's).
    let (out2, _err2) = run_step_dash_enabled(root, dash_port);
    // Reap defensively any dash a (buggy) second start could have spawned, without asserting on
    // it - keeping this test off criterion 1's idempotency ground while never leaking a dash.
    if let Some((_, pid2)) = read_dash_marker(root) {
        if pid2 != pid {
            reap_pid(pid2);
        }
    }
    let alive_after_step2 = rigger::dash::pid_is_alive(pid);
    let served_after_step2 = matches!(http_get(&url), Some(body) if body.contains("rigger dash"));

    // Reap the original detached dash BEFORE asserting, so a failure never leaves it orphaned.
    reap_pid(pid);

    assert!(
        out2.contains(r#""wave":"#),
        "the second step must also run to completion (a printed wave); stdout: {out2:?}"
    );
    assert!(
        alive_after_step1,
        "the detached dash (pid {pid}) must stay ALIVE after the step process that started it \
         exited - a `ReapedChild`-bound dash would be reaped on the step's `main` return"
    );
    assert!(
        served_after_step1,
        "the detached dash must still SERVE {url} after its step process exited"
    );
    assert!(
        alive_after_step2,
        "the SAME detached dash (pid {pid}) must still be alive after a LATER step process ran \
         and exited - it persists across the run's many step invocations"
    );
    assert!(
        served_after_step2,
        "the SAME detached dash must still SERVE {url} across a later step-process boundary"
    );
}

// --- Spec 50, criterion 5: the machine-level SINGLETON dash SELF-REAPS only when NOTHING is
// registered-and-alive for the idle window. This RETARGETS spec 39's per-run liveness trigger ("my
// run went idle") at the singleton: the dash serves every registered instance and outlives any
// single run, so its reap is driven by the machine-global INSTANCE REGISTRY, not one project's
// `agent-live` heartbeat. The dash.rs unit test proves the pure decision (`should_reap_singleton`);
// only driving the real `rigger dash --reap-on-idle` binary against a live registry proves the
// watcher WIRING - that a genuinely-serving detached singleton keeps serving while any instance
// heartbeats and exits ITSELF once the registry empties, with no other process killing it.

/// Write (or refresh) one live instance into the registry `regdir` with a heartbeat stamped NOW.
/// Stands in for a run driver's `register_run_instance` heartbeat without spawning a real run - the
/// registry is the singleton watcher's ONLY liveness signal, so the test controls it directly. A
/// distinct `project`+`root` pair produces a distinct entry (the registry's id keys on them), so
/// two calls with different projects register two independent live instances.
fn write_live_instance(regdir: &std::path::Path, project: &str, root: &str) {
    use rigger::registry::{Instance, StoreIdentity};
    let inst = Instance {
        project: project.to_string(),
        root: root.to_string(),
        store: StoreIdentity::Local {
            path: format!("{root}/.rigger/events.db"),
        },
        heartbeat_ms: rigger::registry::now_ms(),
    };
    rigger::registry::write(regdir, &inst).expect("write a live registry instance");
}

/// Spec 50, criterion 5 end-to-end, through the BUILT binary: a detached `rigger dash --reap-on-idle`
/// (the exact flag the ensure path passes) keeps serving WHILE a registered instance heartbeats, and
/// SELF-REAPS once the registry empties - leaving no orphaned dash on a quiet machine. Nothing in
/// this test ever kills the dash, so its exit is proof of SELF-reap driven by the REGISTRY, not by a
/// `step` process or a `ReapedChild` guard.
///
/// The instance is a real entry under a temp `XDG_STATE_HOME` the dash and the test share; a
/// background thread REFRESHES its heartbeat to simulate a live run, so the registry stays non-empty
/// until the test lets it go idle. The piped stdout is a race-free liveness probe exactly as the
/// guard-reaping test uses: a blocked read means the dash lives; EOF means it exited.
#[test]
fn a_reap_on_idle_singleton_serves_while_an_instance_heartbeats_then_reaps_when_the_registry_empties(
) {
    use std::io::Read;
    use std::process::Stdio;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{mpsc, Arc};
    use std::time::Duration;

    // Redirect the machine-global registry into a temp state home the dash and the test share, so no
    // process-global `~/.local/state` write happens and the dash reads exactly what the test writes.
    let state = tempfile::tempdir().unwrap();
    let xdg = state.path().to_str().unwrap().to_string();
    let regdir = rigger::registry::instances_dir(state.path());

    // One live instance, kept fresh by a background thread - a live run's registry heartbeat.
    write_live_instance(&regdir, "proj-a", "/home/dev/proj-a");
    let stop = Arc::new(AtomicBool::new(false));
    let hb_dir = regdir.clone();
    let hb_stop = stop.clone();
    let heartbeat = std::thread::spawn(move || {
        while !hb_stop.load(Ordering::Relaxed) {
            write_live_instance(&hb_dir, "proj-a", "/home/dev/proj-a");
            std::thread::sleep(Duration::from_millis(150));
        }
    });

    let port = free_loopback_port();
    let mut child = common::rigger_courier()
        .args(["dash", "--port", &port.to_string(), "--reap-on-idle"])
        .env("XDG_STATE_HOME", &xdg)
        // Poll fast and treat an instance heartbeat older than 2s as idle, so the self-reap is
        // observable within the test rather than on the shipped multi-minute cadence.
        .env("RIGGER_DASH_REAP_POLL_MS", "150")
        .env("RIGGER_DASH_REAP_STALE_SECS", "2")
        .env_remove("RIGGER_NO_DASH")
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("failed to spawn `rigger dash --reap-on-idle`");
    let mut out = child.stdout.take().expect("dash stdout is piped");

    // Watch the piped stdout: a blocked read means the dash is alive; a 0-byte read means it exited
    // and stdout hit EOF (the dash logs to stderr, so stdout stays empty-and-open until it dies).
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let mut buf = [0u8; 1];
        let n = out.read(&mut buf).unwrap_or(0);
        let _ = tx.send(n);
    });

    // The singleton is genuinely SERVING its read-only page. Reap on failure so nothing leaks.
    if !matches!(http_get(&format!("http://127.0.0.1:{port}/")), Some(body) if body.contains("rigger dash"))
    {
        stop.store(true, Ordering::Relaxed);
        let _ = heartbeat.join();
        let _ = child.kill();
        let _ = child.wait();
        panic!("the `rigger dash --reap-on-idle` never served its page");
    }

    // While the instance heartbeats, the singleton must NOT reap - a live run keeps it serving. A
    // read arriving here would mean it reaped a live machine (a premature self-reap): fail LOUD.
    if rx.recv_timeout(Duration::from_millis(1200)).is_ok() {
        stop.store(true, Ordering::Relaxed);
        let _ = heartbeat.join();
        let _ = child.kill();
        let _ = child.wait();
        panic!("the singleton self-reaped while an instance was still live - premature reap");
    }

    // Let the registry go idle: stop heartbeating. The one entry ages past the 2s window, the
    // watcher's `read_live` prunes it, and with zero live instances the singleton exits ITSELF.
    stop.store(true, Ordering::Relaxed);
    heartbeat.join().expect("heartbeat thread joins");

    let reaped = rx.recv_timeout(Duration::from_secs(12));
    // Reap defensively before asserting so a failure never leaves the dash orphaned; on the success
    // path the process has already exited, so this is a no-op wait that collects the exited child.
    let _ = child.kill();
    let _ = child.wait();
    let n = reaped.expect(
        "the singleton did not SELF-REAP within 12s after its registry went idle - a machine-idle \
         dash must not leak",
    );
    assert_eq!(
        n, 0,
        "a self-reaped dash should have its stdout at EOF (it exited on its own, un-killed)"
    );
}

/// Spec 50, criterion 5 - THE headline through the BUILT binary: the singleton SURVIVES one project's
/// run ending while ANOTHER project's run is still live, then reaps once BOTH are idle. Two registered
/// instances heartbeat; when project A's heartbeat stops (its entry ages out and `read_live` prunes
/// it) the dash must KEEP serving because B is still live; only when B's heartbeat also stops does the
/// registry empty and the singleton self-reap. This is the exact multi-instance lifecycle the
/// machine-level singleton exists for - a per-run dash keyed on one run's liveness would wrongly die
/// when A ended, which is the regression this test locks out.
#[test]
fn a_reap_on_idle_singleton_survives_one_run_ending_while_another_project_is_live() {
    use std::io::Read;
    use std::process::Stdio;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{mpsc, Arc};
    use std::time::Duration;

    let state = tempfile::tempdir().unwrap();
    let xdg = state.path().to_str().unwrap().to_string();
    let regdir = rigger::registry::instances_dir(state.path());

    // TWO live instances (two projects), each refreshed by its own independently-stoppable thread.
    write_live_instance(&regdir, "proj-a", "/home/dev/proj-a");
    write_live_instance(&regdir, "proj-b", "/home/dev/proj-b");
    let stop_a = Arc::new(AtomicBool::new(false));
    let stop_b = Arc::new(AtomicBool::new(false));
    let spawn_heartbeat = |project: &'static str, root: &'static str, stop: Arc<AtomicBool>| {
        let dir = regdir.clone();
        std::thread::spawn(move || {
            while !stop.load(Ordering::Relaxed) {
                write_live_instance(&dir, project, root);
                std::thread::sleep(Duration::from_millis(150));
            }
        })
    };
    let hb_a = spawn_heartbeat("proj-a", "/home/dev/proj-a", stop_a.clone());
    let hb_b = spawn_heartbeat("proj-b", "/home/dev/proj-b", stop_b.clone());

    let port = free_loopback_port();
    let mut child = common::rigger_courier()
        .args(["dash", "--port", &port.to_string(), "--reap-on-idle"])
        .env("XDG_STATE_HOME", &xdg)
        .env("RIGGER_DASH_REAP_POLL_MS", "150")
        .env("RIGGER_DASH_REAP_STALE_SECS", "2")
        .env_remove("RIGGER_NO_DASH")
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("failed to spawn `rigger dash --reap-on-idle`");
    let mut out = child.stdout.take().expect("dash stdout is piped");

    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let mut buf = [0u8; 1];
        let n = out.read(&mut buf).unwrap_or(0);
        let _ = tx.send(n);
    });

    let stop_all = |a: &Arc<AtomicBool>, b: &Arc<AtomicBool>| {
        a.store(true, Ordering::Relaxed);
        b.store(true, Ordering::Relaxed);
    };

    if !matches!(http_get(&format!("http://127.0.0.1:{port}/")), Some(body) if body.contains("rigger dash"))
    {
        stop_all(&stop_a, &stop_b);
        let _ = hb_a.join();
        let _ = hb_b.join();
        let _ = child.kill();
        let _ = child.wait();
        panic!("the singleton never served its page");
    }

    // Project A's run ENDS: stop refreshing A. Its entry ages past the 2s window and is pruned, but B
    // keeps heartbeating - so at least one instance stays live and the singleton must KEEP serving.
    // Wait well past the window (and several polls) with B alive: a read here is the exact wrong
    // behavior - a machine-level dash dying because ONE of several runs ended.
    stop_a.store(true, Ordering::Relaxed);
    hb_a.join().expect("A heartbeat thread joins");
    if rx.recv_timeout(Duration::from_millis(3500)).is_ok() {
        stop_b.store(true, Ordering::Relaxed);
        let _ = hb_b.join();
        let _ = child.kill();
        let _ = child.wait();
        panic!(
            "the singleton reaped when project A's run ended while project B was still live - it \
             must survive one run ending as long as another instance heartbeats"
        );
    }
    // It is still genuinely serving B's machine (not merely a blocked-but-dead pipe).
    assert!(
        matches!(http_get(&format!("http://127.0.0.1:{port}/")), Some(body) if body.contains("rigger dash")),
        "the singleton must still serve while project B is live"
    );

    // Now project B ends too: with the registry empty, the singleton self-reaps.
    stop_b.store(true, Ordering::Relaxed);
    hb_b.join().expect("B heartbeat thread joins");
    let reaped = rx.recv_timeout(Duration::from_secs(12));
    let _ = child.kill();
    let _ = child.wait();
    let n = reaped.expect(
        "the singleton did not SELF-REAP within 12s after BOTH runs ended - a quiet machine's dash \
         must not leak",
    );
    assert_eq!(n, 0, "a self-reaped dash should have its stdout at EOF");
}

/// Spec 50, criterion 5 - the STARTUP-RACE guard through the BUILT binary: a singleton the ensure
/// path just started reads an EMPTY registry until its ensuring run writes its entry, and must NOT
/// reap on those first empty polls (the `ever_seen_live` latch, the analogue of spec 39's
/// `run_started`). The dash boots against an empty registry and must keep serving across many poll
/// intervals; only after an instance registers and then goes idle does it reap - proving the guard
/// delays the reap to first-sight, not that the watcher is simply absent (the no-flag test below
/// proves absence separately).
#[test]
fn a_reap_on_idle_singleton_does_not_reap_before_any_instance_has_registered() {
    use std::io::Read;
    use std::process::Stdio;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{mpsc, Arc};
    use std::time::Duration;

    // An EMPTY registry at boot: no instance has registered yet (the ensure-then-register gap).
    let state = tempfile::tempdir().unwrap();
    let xdg = state.path().to_str().unwrap().to_string();
    let regdir = rigger::registry::instances_dir(state.path());

    let port = free_loopback_port();
    let mut child = common::rigger_courier()
        .args(["dash", "--port", &port.to_string(), "--reap-on-idle"])
        .env("XDG_STATE_HOME", &xdg)
        // A tiny window so that WITHOUT the guard the empty registry would reap almost immediately -
        // making the ABSENCE of an early reap a strong signal the `ever_seen_live` latch holds.
        .env("RIGGER_DASH_REAP_POLL_MS", "100")
        .env("RIGGER_DASH_REAP_STALE_SECS", "1")
        .env_remove("RIGGER_NO_DASH")
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("failed to spawn `rigger dash --reap-on-idle`");
    let mut out = child.stdout.take().expect("dash stdout is piped");

    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let mut buf = [0u8; 1];
        let n = out.read(&mut buf).unwrap_or(0);
        let _ = tx.send(n);
    });

    if !matches!(http_get(&format!("http://127.0.0.1:{port}/")), Some(body) if body.contains("rigger dash"))
    {
        let _ = child.kill();
        let _ = child.wait();
        panic!("the singleton never served its page against an empty registry");
    }

    // Across MANY windows with a registry that has never held a live instance, the singleton must
    // NOT reap: it has never seen a live instance, so the safe direction is to keep serving. A read
    // here is a premature reap the `ever_seen_live` guard exists to prevent.
    if rx.recv_timeout(Duration::from_millis(2500)).is_ok() {
        let _ = child.kill();
        let _ = child.wait();
        panic!(
            "the singleton self-reaped before ANY instance registered - the startup-race guard must \
             hold the reap until a live instance has been seen"
        );
    }

    // Now an instance registers and heartbeats: the dash keeps serving, and after it goes idle the
    // singleton self-reaps - proving the watcher was running all along and the guard, not its
    // absence, delayed the reap.
    let stop = Arc::new(AtomicBool::new(false));
    let hb_dir = regdir.clone();
    let hb_stop = stop.clone();
    let heartbeat = std::thread::spawn(move || {
        while !hb_stop.load(Ordering::Relaxed) {
            write_live_instance(&hb_dir, "proj-a", "/home/dev/proj-a");
            std::thread::sleep(Duration::from_millis(150));
        }
    });
    // Let a few polls observe the live instance (flip `ever_seen_live`), still serving.
    if rx.recv_timeout(Duration::from_millis(800)).is_ok() {
        stop.store(true, Ordering::Relaxed);
        let _ = heartbeat.join();
        let _ = child.kill();
        let _ = child.wait();
        panic!("the singleton reaped while a fresh instance was live");
    }

    stop.store(true, Ordering::Relaxed);
    heartbeat.join().expect("heartbeat thread joins");
    let reaped = rx.recv_timeout(Duration::from_secs(12));
    let _ = child.kill();
    let _ = child.wait();
    let n = reaped.expect(
        "the singleton did not SELF-REAP within 12s after its one instance went idle - once a live \
         instance has been seen, a return to an empty registry must reap",
    );
    assert_eq!(n, 0, "a self-reaped dash should have its stdout at EOF");
}

/// Spec 50, criterion 5 - the flag GATE through the BUILT binary: a `rigger dash` WITHOUT
/// `--reap-on-idle` starts NO watcher, so it never self-reaps even on a quiet machine with an EMPTY
/// registry. The guard-bound `rigger run` / `run_workflow` dash relies on this: its `ReapedChild`
/// owns its lifecycle, and a stray watcher exiting out from under it would race that teardown.
#[test]
fn a_dash_without_reap_on_idle_never_self_reaps_on_a_quiet_machine() {
    use std::io::Read;
    use std::process::Stdio;
    use std::sync::mpsc;
    use std::time::Duration;

    // An empty registry - the exact machine-idle state that drives the FLAGGED singleton to reap. An
    // unflagged dash must ignore it entirely and keep serving.
    let state = tempfile::tempdir().unwrap();
    let xdg = state.path().to_str().unwrap().to_string();

    let port = free_loopback_port();
    // NOTE: no `--reap-on-idle`. A fast poll and a tiny window are set so that IF a watcher ran at
    // all it would reap almost immediately - making the ABSENCE of a reap a strong signal the flag
    // genuinely gates the watcher off.
    let mut child = common::rigger_courier()
        .args(["dash", "--port", &port.to_string()])
        .env("XDG_STATE_HOME", &xdg)
        .env("RIGGER_DASH_REAP_POLL_MS", "100")
        .env("RIGGER_DASH_REAP_STALE_SECS", "1")
        .env_remove("RIGGER_NO_DASH")
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("failed to spawn `rigger dash`");
    let mut out = child.stdout.take().expect("dash stdout is piped");

    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let mut buf = [0u8; 1];
        let n = out.read(&mut buf).unwrap_or(0);
        let _ = tx.send(n);
    });

    if !matches!(http_get(&format!("http://127.0.0.1:{port}/")), Some(body) if body.contains("rigger dash"))
    {
        let _ = child.kill();
        let _ = child.wait();
        panic!("the `rigger dash` (no --reap-on-idle) never served its page");
    }

    // Well past many poll intervals, the unflagged dash must STILL be up: no watcher was ever
    // started, so an empty registry does not make it exit. A read here is a self-reap the flag was
    // supposed to gate off.
    let reaped = rx.recv_timeout(Duration::from_secs(2));
    let still_serving = reaped.is_err();
    // Nothing owns this dash in the test (in production its parent's `ReapedChild` would), so reap
    // it here regardless of outcome.
    let _ = child.kill();
    let _ = child.wait();
    assert!(
        still_serving,
        "a `rigger dash` WITHOUT --reap-on-idle self-reaped on a quiet machine - the flag must gate \
         the watcher so the guard-bound run dash never exits out from under its ReapedChild"
    );
}

/// Spec 50, criterion 5 - the HOMELESS-ENVIRONMENT guard through the BUILT binary: a
/// `rigger dash --reap-on-idle` in an environment with NO resolvable state home (neither
/// `XDG_STATE_HOME` nor `HOME`) has no machine-global instance registry to poll, so the
/// `reap_on_idle.then(registry::default_dir).flatten()` seam yields `None`, starts NO watcher, and
/// the singleton simply SERVES - it never self-reaps. This is the distinct boundary the `.flatten()`
/// guard adds, and it is the one input none of the criterion tests drive: the flag-gate test proves
/// flag-ABSENT, while every serve/reap/survive test runs with a redirected `XDG_STATE_HOME` (a
/// resolvable home), so only THIS test covers flag-PRESENT-but-HOMELESS. A regression that dropped
/// the `.flatten()` (unwrapping the `None` dir) or fell back to a bogus registry dir would either
/// crash the dash or reap it against an empty registry - both caught here, because the tiny poll and
/// 1s window would drive a real watcher to reap almost immediately, so the ABSENCE of a reap is a
/// strong signal the homeless seam gated the watcher off rather than merely a long window.
#[test]
fn a_reap_on_idle_singleton_in_a_homeless_environment_serves_without_a_watcher() {
    use std::io::Read;
    use std::process::Stdio;
    use std::sync::mpsc;
    use std::time::Duration;

    // A real repo the dash can serve, created by THIS test (which HAS a home); only the DASH
    // subprocess is made homeless below, so `registry::default_dir` resolves to `None` inside it
    // while the served project is a normal, well-formed run root.
    let proj = temp_git_project_with_commit();
    let root = proj.path();

    let port = free_loopback_port();
    // `--reap-on-idle` IS set, but BOTH state-home variables are removed, so the dash's
    // `state_home()` - and thus `default_dir()` - returns `None`: no registry, no watcher. The fast
    // poll and 1s window mean that IF a watcher had started against ANY (necessarily empty) registry
    // it would self-reap within ~1s, so the ABSENCE of a reap is a strong signal the homeless seam
    // gated the watcher off, not that the window was simply too long.
    let mut child = common::rigger_courier()
        .args(["dash", "--port", &port.to_string(), "--reap-on-idle"])
        .current_dir(root)
        .env_remove("XDG_STATE_HOME")
        .env_remove("HOME")
        .env("RIGGER_DASH_REAP_POLL_MS", "100")
        .env("RIGGER_DASH_REAP_STALE_SECS", "1")
        .env_remove("RIGGER_NO_DASH")
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("failed to spawn `rigger dash --reap-on-idle`");
    let mut out = child.stdout.take().expect("dash stdout is piped");

    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let mut buf = [0u8; 1];
        let n = out.read(&mut buf).unwrap_or(0);
        let _ = tx.send(n);
    });

    // It genuinely SERVES its read-only page: this proves the homeless startup did not crash (a
    // crash would EOF stdout and masquerade as a reap below). Reap on failure so nothing leaks.
    if !matches!(http_get(&format!("http://127.0.0.1:{port}/")), Some(body) if body.contains("rigger dash"))
    {
        let _ = child.kill();
        let _ = child.wait();
        panic!("the homeless `rigger dash --reap-on-idle` never served its page");
    }

    // Well past many poll intervals and the 1s window, the dash must STILL be up: with no state home
    // resolvable, `.flatten()` yielded `None` and no watcher was ever started. A read here is a
    // self-reap the homeless guard exists to prevent (an unwrapped `None`, or a bogus-dir fallback
    // reaping against an empty registry), and it fails LOUD.
    let reaped = rx.recv_timeout(Duration::from_secs(2));
    let still_serving = reaped.is_err();
    // Nothing owns this dash in the test (in production its parent's `ReapedChild` would), so reap
    // it here regardless of outcome.
    let _ = child.kill();
    let _ = child.wait();
    assert!(
        still_serving,
        "a `rigger dash --reap-on-idle` in a homeless environment (no XDG_STATE_HOME, no HOME) \
         self-reaped - with no resolvable state home there is no registry to poll, so the \
         `.flatten()` seam must start NO watcher and the singleton must simply serve"
    );
}

/// Spec 50, criterion 5 - the PUBLIC contract of the new decision function at the CRATE BOUNDARY.
/// `dash::should_reap_singleton` is the exported domain core the singleton's watcher polls; the
/// dash.rs unit test proves it white-box (inside the module), and the integration tests above drive
/// it end-to-end through the built binary. This pins its contract at the PUBLIC `rigger::dash`
/// boundary an EXTERNAL caller sees - that the function is exported and its truth table holds -
/// independent of any watcher wiring: reap IFF a live instance has EVER been seen AND none remains,
/// so a positive live count never reaps and a not-yet-seen (startup) empty registry keeps serving.
#[test]
fn should_reap_singleton_public_contract_holds_at_the_crate_boundary() {
    use rigger::dash::should_reap_singleton;

    // Startup guard: no live instance has EVER been seen yet, so keep serving regardless of the
    // current count - a just-ensured singleton must not reap before its ensuring run registers.
    assert!(
        !should_reap_singleton(0, false),
        "a never-seen empty registry must keep serving (the startup-race guard)"
    );
    assert!(
        !should_reap_singleton(1, false),
        "a positive live count never reaps, whatever the seen flag"
    );

    // A live instance keeps the singleton serving once one has been seen (this project's run or any
    // other's - a positive count means at least one run needs the dash).
    assert!(
        !should_reap_singleton(1, true),
        "one live instance keeps the singleton serving"
    );
    assert!(
        !should_reap_singleton(5, true),
        "several live instances keep the singleton serving"
    );

    // Machine idle: at least one live instance was seen and none remains live -> reap.
    assert!(
        should_reap_singleton(0, true),
        "seen-then-empty is a genuinely quiet machine: the singleton reaps"
    );
}

// --- Spec 44, criterion 3: the always-on step dash is SESSION-DETACHED from the `rigger step`
// command's PROCESS GROUP. Spec 39 criterion 2 (above) proves the dash outlives the step PROCESS
// (it holds no `ReapedChild` guard); THIS criterion owns the distinct failure spec 44 fixes: when
// the workflow courier runs `rigger step` as a FOREGROUND command, the harness tears down that
// command's whole process GROUP on return - a merely-"detached" but group-INHERITING dash shares
// that group and is reaped with it, so the spec-39 always-on dash dies the instant every step
// returns. The fix puts the spawned dash in its OWN process group. The main.rs unit tests check
// the process group of an isolated `sleep` child and of `spawn_run_dashboard_detached` called
// DIRECTLY; only driving the real `rigger step` binary proves the production wiring end-to-end:
// that the dash the actual step binary spawns lands OUTSIDE the step command's process group.

/// Read the process-group id (`pgrp`) of `pid` from `/proc/<pid>/stat` - pure std, no signal
/// delivery, so it is reliable and race-free (a not-yet-reaped process, even a zombie, still has
/// a readable `stat`). `/proc/<pid>/stat` is `pid (comm) state ppid pgrp ...`; `comm` may itself
/// contain spaces and parens, so split AFTER the last `)` and take the third whitespace token.
#[cfg(target_os = "linux")]
fn proc_pgid_of(pid: u32) -> u32 {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat"))
        .unwrap_or_else(|e| panic!("read /proc/{pid}/stat: {e}"));
    let after_comm = stat
        .rsplit_once(')')
        .expect("/proc stat has a parenthesised comm field")
        .1;
    after_comm
        .split_whitespace()
        .nth(2)
        .expect("/proc stat has a pgrp field after comm")
        .parse()
        .expect("pgrp is a base-10 integer")
}

/// Spec 44, criterion 3 end-to-end, through the BUILT binary: a `rigger step` run as its OWN
/// process-group leader (mirroring the courier running `rigger step` as a foreground command in
/// its own group) spawns the always-on dash into a DIFFERENT process group - the dash's own
/// group (its PGID equals its PID), never the step command's group. That out-of-group placement
/// is exactly what lets a later teardown of the step command's process group leave the dash
/// serving (spec 44). The proof is by direct PGID OBSERVATION via `/proc` - no signal is sent, so
/// it is deterministic and fail-closed in any environment: were the dash still group-inheriting
/// (the pre-spec-44 regression), the real step binary would spawn it INTO the step command's
/// group and `dash_pgid == step_pgid` would fail this test RED.
///
/// The dash is a real, long-lived detached process; this test reaps it by pid BEFORE its
/// assertions, so a failed assertion never leaks a dashboard.
#[cfg(target_os = "linux")]
// Hermetic against a real machine dash: pins the ensure port to its own ephemeral
// `free_loopback_port` (never the fixed 7420 a genuine always-on dash holds on the self-hosting
// box), so it exercises the real session-detachment path without fighting that machine dash (see
// `step_auto_starts_one_persistent_dash_and_a_second_step_starts_none` for the full rationale).
#[test]
fn a_real_rigger_step_session_detaches_the_dash_from_the_step_command_process_group() {
    use std::os::unix::process::CommandExt;
    use std::process::Stdio;

    let proj = temp_git_project_with_commit();
    let root = proj.path();
    write_two_stage_workflow(root);

    // Run `rigger step` as its OWN process-group leader: `process_group(0)` makes the step a group
    // leader whose PGID equals its PID - the exact shape of a foreground command the courier's
    // harness later tears down by group. RIGGER_NO_DASH is removed so the always-on dash starts.
    let mut step = common::rigger_courier()
        .args(["step"])
        .current_dir(root)
        // Redirect the machine-global registry (spec 50, criterion 2) into the test's own temp
        // tree so this step registers under `root/rigger`, never the operator's real
        // ~/.local/state/rigger/instances.
        .env("XDG_STATE_HOME", root)
        // Pin the ensure port to an ephemeral loopback port so this step-path dash never binds the
        // machine-fixed default and never collides with a real always-on dash on 7420.
        .env("RIGGER_DASH_PORT", free_loopback_port().to_string())
        .env_remove("RIGGER_NO_DASH")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .process_group(0)
        .spawn()
        .expect("failed to spawn `rigger step`");
    // The step is its own group leader, so its process-group id IS its pid - the group whose
    // teardown must NOT reach the dash.
    let step_pgid = step.id();
    step.wait().expect("wait on the step process");

    // The now-exited step recorded its detached dash. Read its (port, pid).
    let (port, dash_pid) =
        read_dash_marker(root).expect("the step must record a dash marker at .rigger/dash.marker");
    let url = format!("http://127.0.0.1:{port}/");

    // The dash is a GENUINE serving process (not a stale marker or a recycled pid): confirm it
    // serves its page before observing its group. Reap on failure so nothing leaks.
    if !matches!(http_get(&url), Some(body) if body.contains("rigger dash")) {
        reap_pid(dash_pid);
        panic!("the step-started dash at {url} did not serve its page");
    }

    // Observe the dash's process group directly from `/proc` - no signal sent.
    let dash_pgid = proc_pgid_of(dash_pid);

    // Reap the detached dash BEFORE asserting, so a failed assertion never leaves it orphaned.
    reap_pid(dash_pid);

    assert_eq!(
        dash_pgid, dash_pid,
        "the step-spawned dash must be its OWN process-group leader (PGID == its PID) - the \
         session-detachment spec 44 requires"
    );
    assert_ne!(
        dash_pgid, step_pgid,
        "the step-spawned dash must NOT be in the `rigger step` command's process group (pgid \
         {step_pgid}) - it is the out-of-group placement that lets a teardown of the step \
         command's group leave the always-on dash serving (spec 44 c3); a group-inheriting dash \
         would carry the step command's pgid here"
    );
}

/// Spec 50, criterion 1 end-to-end, through the BUILT binary: `rigger dash` binds a FIXED
/// address and is a SINGLETON. The first invocation binds the (here ephemeral, standing in for
/// the fixed default) address and serves it; a SECOND invocation on that SAME address while the
/// first is still serving does NOT bind a second port and does NOT enter a serve loop - it
/// recognizes the already-serving dash (by the `X-Rigger-Dash` header it probes), reports the
/// EXISTING address, and exits 0. The `dash.rs` unit tests prove `bind_singleton`'s branch
/// decisions in-process; only driving the real binary proves the wiring across two separate
/// `rigger dash` processes: the header recognition over the real socket and the clean exit-0
/// report instead of an `Address already in use` failure.
///
/// The first dash is a real, long-lived process; this test REAPS it before its assertions so a
/// failed assertion never leaks a dashboard.
#[test]
fn dash_is_a_fixed_address_singleton_a_second_invocation_reports_and_exits_clean() {
    use std::io::Read;
    use std::process::Stdio;
    use std::time::{Duration, Instant};

    // A repo-less/empty-store dir is enough: `rigger dash` serves an absent store as an empty
    // run. An ephemeral port stands in for the fixed default so the test never fights a real dash.
    let proj = temp_project();
    let root = proj.path();
    let port = free_loopback_port();
    let url = format!("http://127.0.0.1:{port}/");

    // FIRST dash: bind the address and wait until it genuinely serves its page.
    let mut first = common::rigger_courier()
        .args(["dash", "--port", &port.to_string()])
        .current_dir(root)
        .env_remove("RIGGER_NO_DASH")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("failed to spawn the first `rigger dash`");

    if !matches!(http_get(&url), Some(body) if body.contains("rigger dash")) {
        let _ = first.kill();
        let _ = first.wait();
        panic!("the first `rigger dash` never served its page at {url}");
    }

    // SECOND invocation on the SAME address: it must NOT enter a serve loop (so it exits on its
    // own) and must NOT bind a second port. It reports the existing address and exits 0. Poll
    // `try_wait` on a bounded deadline so a regression that DID enter the serve loop fails LOUD
    // (a hang caught by the deadline) rather than hanging the whole suite.
    let mut second = common::rigger_courier()
        .args(["dash", "--port", &port.to_string()])
        .current_dir(root)
        .env_remove("RIGGER_NO_DASH")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn the second `rigger dash`");

    let deadline = Instant::now() + Duration::from_secs(15);
    let exited = loop {
        match second.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(50)),
            Ok(None) => break None,
            Err(_) => break None,
        }
    };

    // Collect the second invocation's output (its pipes are at EOF once it has exited).
    let mut out = String::new();
    if let Some(mut so) = second.stdout.take() {
        let _ = so.read_to_string(&mut out);
    }
    let mut err = String::new();
    if let Some(mut se) = second.stderr.take() {
        let _ = se.read_to_string(&mut err);
    }

    // Reap the first (still the ONLY serving process) and, if the second hung, kill it too -
    // BEFORE any assertion, so a failure never leaks a dashboard.
    let _ = first.kill();
    let _ = first.wait();
    if exited.is_none() {
        let _ = second.kill();
        let _ = second.wait();
        panic!(
            "the second `rigger dash` never exited within the deadline - a singleton invocation \
             must recognize the serving dash and NOT enter a serve loop"
        );
    }
    let status = exited.unwrap();

    assert!(
        status.success(),
        "the second `rigger dash` must exit 0 (the singleton is the point), not fail on a port \
         conflict; stdout: {out:?} stderr: {err:?}"
    );
    assert!(
        out.contains(&format!("127.0.0.1:{port}")),
        "the second `rigger dash` must report the EXISTING address ({url}) it found serving; \
         stdout: {out:?} stderr: {err:?}"
    );
}

/// Spec 50, criterion 3 end-to-end, through the BUILT binary: the dash's LANDING view lists every
/// registered instance, and selecting one ATTACHES the run + knowledge-graph views to THAT
/// instance's stores, read-only, through per-request store opens - including an instance with no
/// active run (its graph still serves; its run view degrades to an empty state, never an error).
///
/// Two independent projects register into ONE shared machine-global registry (this criterion
/// CONSUMES the registry criterion 2 owns, so the test writes the entries directly through the
/// registry API rather than driving a run): instance A has an ACTIVE run (a distinctive unit
/// `u-alpha`) and a graph node (`src/alpha.rs`); instance B has NO active run - only a graph node
/// (`src/beta.rs`) folded with an empty run stream. A third, NEUTRAL project is the dash's own cwd,
/// so its default (no-selector) state is empty - proving that what `?instance=` serves comes from
/// the SELECTED instance's stores, not the dash's own project. The dash is a real, long-lived
/// process the test REAPS before its assertions so a failure never leaks a dashboard.
#[test]
fn dash_landing_lists_instances_and_attach_serves_each_instance_store() {
    use rigger::registry;
    use std::process::Stdio;

    // One shared registry both instances register into and the dash discovers.
    let state = tempfile::tempdir().unwrap();
    let xdg = state.path().to_str().unwrap();
    let regdir = registry::instances_dir(state.path());

    // Instance A: an ACTIVE run (RunStarted + a distinctive unit) plus a knowledge-graph node.
    let a = temp_git_project_with_commit();
    let a_root = a.path();
    seed_run_events(
        a_root,
        &[
            ("RunStarted", r#"{"run_id":"run-a","specs":["s"]}"#),
            (
                "UnitStarted",
                r#"{"id":"u-alpha","spec_criterion":"alpha work"}"#,
            ),
        ],
    );
    // `rigger emit` appends the decision AND folds it into A's graph.db (src/alpha.rs node + edge).
    let (_o, e, ok) = run_rigger(
        a_root,
        &[
            "emit",
            "DecisionMade",
            r#"{"id":"d-alpha","summary":"alpha decision","governs":["src/alpha.rs"]}"#,
        ],
    );
    assert!(ok, "seeding instance A's graph must succeed; stderr: {e}");

    // Instance B: NO active run - a graph node folded over an empty run stream (no RunStarted). Its
    // run view must degrade to empty; its graph must still serve.
    let b = temp_git_project_with_commit();
    let b_root = b.path();
    seed_store(b_root);
    // B governs a file under a DISTINCT top-level dir (`docs/`) so its clustered graph overview is
    // told apart from A's (`src/`) - the whole-graph overview folds nodes by module directory.
    let (_o, e, ok) = run_rigger(
        b_root,
        &[
            "emit",
            "DecisionMade",
            r#"{"id":"d-beta","summary":"beta decision","governs":["docs/beta.md"]}"#,
        ],
    );
    assert!(ok, "seeding instance B's graph must succeed; stderr: {e}");

    // Register both instances directly (criterion 3 consumes the registry; it does not own writes).
    let entry = |root: &Path| registry::Instance {
        project: run_stream_identity(root),
        root: root.to_string_lossy().into_owned(),
        store: registry::StoreIdentity::Local {
            path: root
                .join(".rigger")
                .join("events.db")
                .to_string_lossy()
                .into_owned(),
        },
        heartbeat_ms: registry::now_ms(),
    };
    let a_inst = entry(a_root);
    let b_inst = entry(b_root);
    registry::write(&regdir, &a_inst).unwrap();
    registry::write(&regdir, &b_inst).unwrap();
    let a_id = a_inst.id();
    let b_id = b_inst.id();

    // The dash runs in a NEUTRAL project so its own default state is empty - the contrast that
    // proves attach reads the SELECTED store, not the dash's cwd.
    let neutral = temp_project();
    let port = free_loopback_port();
    let mut dash = common::rigger_courier()
        .args(["dash", "--port", &port.to_string()])
        .current_dir(neutral.path())
        .env("XDG_STATE_HOME", xdg)
        .env_remove("RIGGER_NO_DASH")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("failed to spawn `rigger dash`");

    // Drive every read, then REAP the dash before asserting so a failure never leaks a process.
    let landing = http_get_path(port, "/api/instances");
    let default_state = http_get_path(port, "/api/state");
    let a_state = http_get_path(port, &format!("/api/state?instance={a_id}"));
    let b_state = http_get_path(port, &format!("/api/state?instance={b_id}"));
    let a_graph = http_get_path(port, &format!("/api/graph?instance={a_id}"));
    let b_graph = http_get_path(port, &format!("/api/graph?instance={b_id}"));
    let _ = dash.kill();
    let _ = dash.wait();

    let landing = landing.expect("the dash never served /api/instances");
    let default_state = default_state.expect("the dash never served /api/state");
    let a_state = a_state.expect("the dash never served A's attached state");
    let b_state = b_state.expect("the dash never served B's attached state");
    let a_graph = a_graph.expect("the dash never served A's attached graph");
    let b_graph = b_graph.expect("the dash never served B's attached graph");

    // Clause 1 - the LANDING lists BOTH registered instances, each with its selectable id.
    assert!(
        landing.contains("HTTP/1.1 200"),
        "the landing endpoint answers 200: {landing}"
    );
    for (project, id) in [
        (run_stream_identity(a_root), &a_id),
        (run_stream_identity(b_root), &b_id),
    ] {
        assert!(
            landing.contains(&project) && landing.contains(id.as_str()),
            "the landing must list instance {project} with its attach id {id}; got: {landing}"
        );
    }

    // The dash's OWN default (no selector) state is empty - it has neither A's nor B's content.
    assert!(
        default_state.contains("HTTP/1.1 200") && !default_state.contains("u-alpha"),
        "the neutral-cwd dash's default state carries no attached instance's run: {default_state}"
    );

    // Clause 2 - selecting A serves A's RUN (its unit) and A's GRAPH (its `src/` cluster), read-only.
    assert!(
        a_state.contains("HTTP/1.1 200") && a_state.contains("u-alpha"),
        "attaching to A must serve A's own run (unit u-alpha): {a_state}"
    );
    assert!(
        a_graph.contains("HTTP/1.1 200") && a_graph.contains("\"key\":\"src\""),
        "attaching to A must serve A's own knowledge graph (a `src/` cluster): {a_graph}"
    );
    // And attach is genuinely per-instance: A's views carry NONE of B's content.
    assert!(
        !a_state.contains("d-beta") && !a_graph.contains("\"key\":\"docs\""),
        "A's attached views must not bleed B's content: state={a_state} graph={a_graph}"
    );

    // Clause 3 - an instance with NO active run: its graph still serves (its `docs/` cluster), and
    // its run view degrades to an EMPTY state (no u-alpha, no error), never a 500.
    assert!(
        b_graph.contains("HTTP/1.1 200") && b_graph.contains("\"key\":\"docs\""),
        "attaching to B (no active run) must still serve its knowledge graph: {b_graph}"
    );
    assert!(
        !b_graph.contains("\"key\":\"src\""),
        "B's attached graph must be B's own, not A's `src/` cluster: {b_graph}"
    );
    assert!(
        b_state.contains("HTTP/1.1 200")
            && !b_state.contains("HTTP/1.1 500")
            && !b_state.contains("u-alpha"),
        "attaching to B must degrade its empty run to an empty state, never an error: {b_state}"
    );
    // The empty-run view is genuinely empty (no units), proving the empty-state degrade: the
    // `run.units` array serializes empty (`"units":[]`) when no `UnitStarted` was folded.
    let b_body = b_state.split("\r\n\r\n").nth(1).unwrap_or("");
    assert!(
        b_body.contains("\"units\":[]"),
        "B has no active run, so its attached run view has no units: {b_body}"
    );
}

/// Spec 50, criterion 3, the ATTACH RESOLVER's SAFETY branch through the BUILT binary: an
/// UNKNOWN or since-gone `?instance=<id>` selector must degrade to an EMPTY state - NEVER the
/// dash's OWN local run, and never a 500. This is the boundary the happy-path landing test cannot
/// reach: its neutral-cwd dash has no local run, so an unknown selector returning empty is
/// indistinguishable from returning the (already empty) local project. Here the dash HAS a
/// distinctive local run AND a registered live instance also has one, so an unknown selector that
/// returns empty is provably neither - it did not silently fall back to the local project (the
/// bug this guards: a since-gone selection quietly showing the operator's own run under a stale
/// bookmark) and did not attach to some other instance.
///
/// Three-way contrast, all through per-request store opens on ONE long-lived dash process (reaped
/// before the assertions so a failure never leaks a dashboard): with no selector the dash serves
/// its OWN local run (`u-ownrun`, backward compatible); `?instance=<live id>` serves that
/// instance's run (`u-gamma`) and NOT the local run; and `?instance=<bogus>` serves an EMPTY run
/// (a 200 with `"units":[]`, never a 500) that is neither the local run nor the instance's.
#[test]
fn dash_attach_unknown_or_since_gone_instance_serves_empty_not_the_local_run() {
    use rigger::registry;
    use std::process::Stdio;

    // One sandboxed registry both the dash and the registered instance share. Sandboxing
    // `XDG_STATE_HOME` is mandatory: without it the dash reads/writes the operator's REAL
    // machine-global registry, and the test pollutes production state.
    let state = tempfile::tempdir().unwrap();
    let xdg = state.path().to_str().unwrap();
    let regdir = registry::instances_dir(state.path());

    // The dash's OWN project, with a distinctive LOCAL run so the no-selector default is non-empty
    // - the contrast that makes "unknown selector did NOT show the local run" meaningful.
    let own = temp_git_project_with_commit();
    let own_root = own.path();
    seed_run_events(
        own_root,
        &[
            ("RunStarted", r#"{"run_id":"run-own","specs":["s"]}"#),
            (
                "UnitStarted",
                r#"{"id":"u-ownrun","spec_criterion":"the dash's own local run"}"#,
            ),
        ],
    );

    // A registered LIVE instance with its OWN distinctive run, so a KNOWN selector genuinely
    // attaches away from the local project (sanity that the resolver's live arm still works).
    let gamma = temp_git_project_with_commit();
    let gamma_root = gamma.path();
    seed_run_events(
        gamma_root,
        &[
            ("RunStarted", r#"{"run_id":"run-gamma","specs":["s"]}"#),
            (
                "UnitStarted",
                r#"{"id":"u-gamma","spec_criterion":"the registered instance's run"}"#,
            ),
        ],
    );
    let gamma_inst = registry::Instance {
        project: run_stream_identity(gamma_root),
        root: gamma_root.to_string_lossy().into_owned(),
        store: registry::StoreIdentity::Local {
            path: gamma_root
                .join(".rigger")
                .join("events.db")
                .to_string_lossy()
                .into_owned(),
        },
        heartbeat_ms: registry::now_ms(),
    };
    registry::write(&regdir, &gamma_inst).unwrap();
    let gamma_id = gamma_inst.id();

    // The dash serves its OWN project (cwd = own_root); the registry it discovers is the sandbox.
    let port = free_loopback_port();
    let mut dash = common::rigger_courier()
        .args(["dash", "--port", &port.to_string()])
        .current_dir(own_root)
        .env("XDG_STATE_HOME", xdg)
        .env_remove("RIGGER_NO_DASH")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("failed to spawn `rigger dash`");

    let default_state = http_get_path(port, "/api/state");
    let gamma_state = http_get_path(port, &format!("/api/state?instance={gamma_id}"));
    // A selector that never named a registry entry - the since-gone / stale-bookmark case.
    let bogus_state = http_get_path(port, "/api/state?instance=this-id-was-never-registered");
    let _ = dash.kill();
    let _ = dash.wait();

    let default_state = default_state.expect("the dash never served /api/state");
    let gamma_state = gamma_state.expect("the dash never served the attached state");
    let bogus_state = bogus_state.expect("the dash never served the unknown-selector state");

    // No selector -> the dash's OWN local run (backward compatible).
    assert!(
        default_state.contains("HTTP/1.1 200") && default_state.contains("u-ownrun"),
        "with no selector the dash serves its own local run: {default_state}"
    );

    // A KNOWN selector attaches to THAT instance's run, and away from the local project.
    assert!(
        gamma_state.contains("HTTP/1.1 200")
            && gamma_state.contains("u-gamma")
            && !gamma_state.contains("u-ownrun"),
        "a known selector serves the instance's run, not the dash's local one: {gamma_state}"
    );

    // The SAFETY boundary: an unknown / since-gone selector degrades to an EMPTY run - it is
    // NEITHER the dash's local run NOR the registered instance's, and it is a 200 with no units,
    // never a 500 and never a silent fall-back to the local project.
    assert!(
        bogus_state.contains("HTTP/1.1 200") && !bogus_state.contains("HTTP/1.1 500"),
        "an unknown selector degrades to an empty state, never an error: {bogus_state}"
    );
    assert!(
        !bogus_state.contains("u-ownrun"),
        "an unknown selector must NOT silently show the dash's own local run: {bogus_state}"
    );
    assert!(
        !bogus_state.contains("u-gamma"),
        "an unknown selector must not leak an unrelated instance's run: {bogus_state}"
    );
    let bogus_body = bogus_state.split("\r\n\r\n").nth(1).unwrap_or("");
    assert!(
        bogus_body.contains("\"units\":[]"),
        "the unknown-selector run view is genuinely empty (no units): {bogus_body}"
    );
}

/// Spec 50, criterion 3, the STALE-PRUNE reaching the periphery AND the landing's WIRE CONTRACT,
/// through the BUILT binary. `registry::read_live` prunes any entry whose heartbeat has gone stale
/// past the idle window, and BOTH the `/api/instances` landing provider and the attach resolver
/// consume it - so a stale (dead) instance must not appear in the landing, and selecting a stale
/// id must degrade to empty. The happy-path test registers only fresh entries, so neither the
/// prune nor the exact serialized field names the page's JS reads are exercised here.
///
/// A LIVE instance (fresh heartbeat) and a STALE one (heartbeat well past `DEFAULT_IDLE_MS`) are
/// registered into one sandboxed registry. On the built dash, `/api/instances` lists the LIVE
/// instance and NOT the stale one (pruned by the read) and carries every one of the six
/// `InstanceView` wire keys the landing page reads; and `?instance=<stale id>` degrades to an
/// empty run (the stale entry was pruned before resolve).
#[test]
fn dash_landing_prunes_stale_instances_and_pins_the_wire_contract() {
    use rigger::registry;
    use std::process::Stdio;

    let state = tempfile::tempdir().unwrap();
    let xdg = state.path().to_str().unwrap();
    let regdir = registry::instances_dir(state.path());

    let now = registry::now_ms();
    let live_root = temp_git_project_with_commit();
    let stale_root = temp_git_project_with_commit();
    let mk = |root: &Path, hb: u64| registry::Instance {
        project: run_stream_identity(root),
        root: root.to_string_lossy().into_owned(),
        store: registry::StoreIdentity::Local {
            path: root
                .join(".rigger")
                .join("events.db")
                .to_string_lossy()
                .into_owned(),
        },
        heartbeat_ms: hb,
    };
    // Live: heartbeat now. Stale: a full idle window plus a minute in the past, so `is_stale`
    // is unambiguously true regardless of the small drift between this stamp and the dash's read.
    let live_inst = mk(live_root.path(), now);
    let stale_inst = mk(stale_root.path(), now - registry::DEFAULT_IDLE_MS - 60_000);
    registry::write(&regdir, &live_inst).unwrap();
    registry::write(&regdir, &stale_inst).unwrap();
    let live_project = run_stream_identity(live_root.path());
    let stale_project = run_stream_identity(stale_root.path());
    let live_id = live_inst.id();
    let stale_id = stale_inst.id();

    let neutral = temp_project();
    let port = free_loopback_port();
    let mut dash = common::rigger_courier()
        .args(["dash", "--port", &port.to_string()])
        .current_dir(neutral.path())
        .env("XDG_STATE_HOME", xdg)
        .env_remove("RIGGER_NO_DASH")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("failed to spawn `rigger dash`");

    let landing = http_get_path(port, "/api/instances");
    let stale_state = http_get_path(port, &format!("/api/state?instance={stale_id}"));
    let _ = dash.kill();
    let _ = dash.wait();

    let landing = landing.expect("the dash never served /api/instances");
    let stale_state = stale_state.expect("the dash never served the stale-selector state");

    assert!(
        landing.contains("HTTP/1.1 200"),
        "the landing endpoint answers 200: {landing}"
    );
    // The LIVE instance is listed with its attach id; the STALE one is pruned, absent from both
    // the landing's project labels and its selectable ids.
    assert!(
        landing.contains(&live_project) && landing.contains(live_id.as_str()),
        "the landing lists the live instance {live_project} with its attach id: {landing}"
    );
    assert!(
        !landing.contains(&stale_project) && !landing.contains(stale_id.as_str()),
        "a stale instance is pruned from the landing, never shown as attachable: {landing}"
    );

    // The WIRE CONTRACT: the landing body carries every field name the page's JS reads to label
    // and select a row. A rename would silently break the picker; pin all six here.
    let landing_body = landing.split("\r\n\r\n").nth(1).unwrap_or("");
    for key in [
        "\"instances\"",
        "\"id\"",
        "\"project\"",
        "\"root\"",
        "\"kind\"",
        "\"store\"",
        "\"age_secs\"",
    ] {
        assert!(
            landing_body.contains(key),
            "the landing wire contract must carry {key}: {landing_body}"
        );
    }

    // Selecting the stale id resolves against the pruned registry, so it degrades to an empty run
    // (a 200 with no units), never the stale instance's content and never a 500.
    assert!(
        stale_state.contains("HTTP/1.1 200") && !stale_state.contains("HTTP/1.1 500"),
        "a stale selector degrades to an empty state, never an error: {stale_state}"
    );
    let stale_body = stale_state.split("\r\n\r\n").nth(1).unwrap_or("");
    assert!(
        stale_body.contains("\"units\":[]"),
        "the stale-selector run view is genuinely empty (no units): {stale_body}"
    );
}

/// Spec 50, criterion 3, the READ-ONLY GLOBAL CONSTRAINT through the BUILT binary: attaching to a
/// SHARED instance whose own `.rigger` no longer resolves a server (a SQLITE DEGRADE) must NOT
/// create a store file under that instance's project. A dash attach is a read-only projection, and
/// `Store::open` CREATES `events.db` AND its schema - so the Shared arm must guard existence exactly
/// like the Local arm rather than open-creating a phantom store under a foreign root
/// (adv-u50c3-shared-attach-creates-phantom-store). This is the CREATION angle - the testable, and
/// gating, sibling of the env-precedence finding: the Shared arm resolves through the ATTACHED
/// instance's own config with NO ambient environment, so the dash process's `KURRENTDB_CONN` can
/// neither redirect a foreign read nor (its write-path sibling) open-create a store here.
///
/// The instance's `.rigger` exists (as it does for any real instance) but carries no store config
/// and no `events.db`, so its own resolution degrades to the sqlite default; the dash runs with
/// `KURRENTDB_CONN` REMOVED so the pre-fix Shared arm's `env_conn()` rung is also empty and it
/// reaches the exact `Store::open` that wrote the phantom file. After one attach GET, the file must
/// still be absent.
#[test]
fn dash_attach_to_shared_instance_never_creates_a_store_under_its_root() {
    use rigger::registry;
    use std::process::Stdio;

    let state = tempfile::tempdir().unwrap();
    let xdg = state.path().to_str().unwrap();
    let regdir = registry::instances_dir(state.path());

    // A SHARED-registered instance whose `.rigger` exists (the registry/graph live there for any
    // real instance) but carries NO store config (`store.conn`/`workflow.yml`) and NO `events.db`,
    // so its OWN store resolution degrades to the sqlite default.
    let shared_root = temp_git_project_with_commit();
    let rigger_dir = shared_root.path().join(".rigger");
    std::fs::create_dir_all(&rigger_dir).unwrap();
    let events_db = rigger_dir.join("events.db");
    assert!(
        !events_db.exists(),
        "precondition: the shared instance starts with no events.db"
    );

    let inst = registry::Instance {
        project: run_stream_identity(shared_root.path()),
        root: shared_root.path().to_string_lossy().into_owned(),
        store: registry::StoreIdentity::Shared {
            endpoint: "kurrentdb://localhost:2113".to_string(),
        },
        heartbeat_ms: registry::now_ms(),
    };
    registry::write(&regdir, &inst).unwrap();
    let id = inst.id();

    let neutral = temp_project();
    let port = free_loopback_port();
    let mut dash = common::rigger_courier()
        .args(["dash", "--port", &port.to_string()])
        .current_dir(neutral.path())
        .env("XDG_STATE_HOME", xdg)
        // No ambient server address: the Shared arm degrades to sqlite (rung 5) and MUST hit the
        // existence guard - never resolve the dash's own `KURRENTDB_CONN` for a foreign instance.
        .env_remove("KURRENTDB_CONN")
        .env_remove("RIGGER_NO_DASH")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("failed to spawn `rigger dash`");

    let attached = http_get_path(port, &format!("/api/state?instance={id}"));
    let _ = dash.kill();
    let _ = dash.wait();

    let attached = attached.expect("the dash never served the shared-attached state");
    // The attach degrades to an EMPTY run - a 200, never a 500 - because the store is absent.
    assert!(
        attached.contains("HTTP/1.1 200") && !attached.contains("HTTP/1.1 500"),
        "attaching to a store-less shared instance degrades to an empty state, never an error: {attached}"
    );
    let body = attached.split("\r\n\r\n").nth(1).unwrap_or("");
    assert!(
        body.contains("\"units\":[]"),
        "the shared-attach run view is genuinely empty (no units): {body}"
    );
    // THE CONSTRAINT: a read-only attach created NO store file under the instance's root. The
    // pre-fix Shared arm called `Store::open` on the sqlite degrade, writing this phantom events.db.
    assert!(
        !events_db.exists(),
        "a read-only dash attach must never create a store under a foreign project, but {} was created",
        events_db.display()
    );
}

/// Spec 50, criterion 3, the ENV-PRECEDENCE branch of the Shared attach arm through the BUILT
/// binary (adv-u50c3-uphold-sdet-env-precedence): the dash process's OWN `KURRENTDB_CONN`
/// addresses a DIFFERENT project's store, so when it attaches to a registered SHARED instance the
/// ATTACHED instance's own `.rigger` config - never the dash's ambient environment - is
/// authoritative for the read. The Shared arm proves this by resolving through
/// `store_selection_at(None, ..)`: passing `None` for env instead of `env_conn()`, so the dash's
/// `KURRENTDB_CONN` can NOT win the precedence and redirect the foreign read to the wrong store.
///
/// The sibling no-phantom-store test cannot reach THIS boundary: it REMOVES `KURRENTDB_CONN`, so a
/// regression that swapped `None` back to `env_conn()` would stay green there (an empty env resolves
/// to the same sqlite degrade). Here the env is SET and the instance's own store is POPULATED, so
/// the two behaviors DIVERGE observably: the correct `None` path reads the instance's own sqlite
/// (its distinctive run shows through), while an `env_conn()` regression would resolve the dash's
/// `KURRENTDB_CONN` server, fail to reach it, and degrade to an EMPTY run - the instance's own run
/// would vanish. Asserting the instance's own unit shows through is therefore RED exactly on the
/// env-redirect regression and GREEN on the shipped env-agnostic read.
#[test]
fn dash_attach_to_shared_instance_reads_its_own_store_not_the_dash_process_kurrentdb_conn() {
    use rigger::registry;
    use std::process::Stdio;

    let state = tempfile::tempdir().unwrap();
    let xdg = state.path().to_str().unwrap();
    let regdir = registry::instances_dir(state.path());

    // A SHARED-registered instance whose `.rigger` carries NO store config (so its OWN resolution
    // degrades to the sqlite default) but DOES have a POPULATED `events.db` - a distinctive run
    // (unit `u-envprec`) seeded into its own local sqlite exactly as the compiled binary reads it
    // back. This is the Shared-arm Sqlite DEGRADE with real content to read.
    let shared_root = temp_git_project_with_commit();
    seed_run_events(
        shared_root.path(),
        &[
            ("RunStarted", r#"{"run_id":"run-envprec","specs":["s"]}"#),
            (
                "UnitStarted",
                r#"{"id":"u-envprec","spec_criterion":"env precedence work"}"#,
            ),
        ],
    );

    let inst = registry::Instance {
        project: run_stream_identity(shared_root.path()),
        root: shared_root.path().to_string_lossy().into_owned(),
        store: registry::StoreIdentity::Shared {
            endpoint: "kurrentdb://localhost:2113".to_string(),
        },
        heartbeat_ms: registry::now_ms(),
    };
    registry::write(&regdir, &inst).unwrap();
    let id = inst.id();

    // The dash process's OWN ambient server address - a well-formed but UNREACHABLE endpoint (a
    // free loopback port nothing listens on, so a connection is refused fast). If the Shared arm
    // wrongly resolved THIS `env_conn()` for the foreign instance it would select the server, fail
    // to reach it, and the attached run would come back empty. The shipped arm ignores it.
    let dead_port = free_loopback_port();
    let dead_conn = format!("kurrentdb://127.0.0.1:{dead_port}");

    let neutral = temp_project();
    let port = free_loopback_port();
    let mut dash = common::rigger_courier()
        .args(["dash", "--port", &port.to_string()])
        .current_dir(neutral.path())
        .env("XDG_STATE_HOME", xdg)
        // The dash process HOLDS a `KURRENTDB_CONN`, pointed at a store that is NOT this instance's.
        // The attach must ignore it and read the instance's OWN sqlite (rungs 3-5 at the instance's
        // `.rigger`), never let the dash env (rung 2) redirect the foreign read.
        .env("KURRENTDB_CONN", &dead_conn)
        .env_remove("RIGGER_NO_DASH")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("failed to spawn `rigger dash`");

    let attached = http_get_path(port, &format!("/api/state?instance={id}"));
    let _ = dash.kill();
    let _ = dash.wait();

    let attached = attached.expect("the dash never served the shared-attached state");
    // A 200 (never a 500): the read is best-effort and the instance's own sqlite is present.
    assert!(
        attached.contains("HTTP/1.1 200") && !attached.contains("HTTP/1.1 500"),
        "attaching to the shared instance answers 200 from its own store: {attached}"
    );
    // THE CONSTRAINT: the attach served the ATTACHED INSTANCE's OWN run (unit `u-envprec`), proving
    // the read went through the instance's `.rigger` and the dash process's `KURRENTDB_CONN` did NOT
    // redirect it. A regression to `env_conn()` would resolve the dead server and serve an empty run.
    assert!(
        attached.contains("u-envprec"),
        "the shared attach must read the instance's OWN store (its run unit u-envprec), never the \
         dash process's KURRENTDB_CONN: {attached}"
    );
}

/// Spec 50, criterion 1, the CONFLICT branch through the BUILT binary: when the dash's requested
/// address is held by an UNRELATED (non-dash) process, `rigger dash --port <held>` must FAIL
/// LOUD - it surfaces the address-in-use conflict and exits NON-ZERO, the deliberate opposite of
/// the retired free-port search. It must NEVER drift to another port and NEVER mistake the holder
/// for a serving singleton (no "already serving" defer): only a genuine rigger dash, recognized
/// by its `X-Rigger-Dash` header, earns the clean exit-0 defer the sibling singleton test covers.
/// This is the negative half of the fixed-address policy - the address in, or a loud conflict,
/// never a silent drift - which the singleton (success) test cannot reach. Bounded so a
/// regression that DID drift-and-serve fails LOUD (a caught hang) instead of wedging the suite.
#[test]
fn dash_on_a_port_held_by_a_non_dash_process_fails_loud_and_never_drifts() {
    use std::io::Read;
    use std::process::Stdio;
    use std::time::{Duration, Instant};

    let proj = temp_project();
    let root = proj.path();

    // Hold the port with a plain listener that is NOT a dash and answers no probe - any unrelated
    // process squatting the address. Keeping the bound listener in scope holds the port for the
    // whole `rigger dash` run; the singleton probe connects into its backlog, reads nothing, and
    // times out - so the holder is correctly NOT recognized as a dash.
    let holder = reserved_loopback_listener();
    let port = holder.local_addr().unwrap().port();

    let mut dash = common::rigger_courier()
        .args(["dash", "--port", &port.to_string()])
        .current_dir(root)
        .env_remove("RIGGER_NO_DASH")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn `rigger dash` against a held port");

    // The conflict path exits immediately; poll `try_wait` on a bounded deadline so a regression
    // that drifted to a free port and entered the serve loop is caught LOUD (as a hang past the
    // deadline) rather than blocking the whole suite forever.
    let deadline = Instant::now() + Duration::from_secs(15);
    let exited = loop {
        match dash.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(50)),
            Ok(None) => break None,
            Err(_) => break None,
        }
    };

    let mut out = String::new();
    if let Some(mut so) = dash.stdout.take() {
        let _ = so.read_to_string(&mut out);
    }
    let mut err = String::new();
    if let Some(mut se) = dash.stderr.take() {
        let _ = se.read_to_string(&mut err);
    }

    if exited.is_none() {
        let _ = dash.kill();
        let _ = dash.wait();
        panic!(
            "`rigger dash` never exited on a conflicting port - a non-dash holder must be a LOUD \
             conflict, never a drift-and-serve; stdout: {out:?} stderr: {err:?}"
        );
    }
    let status = exited.unwrap();

    assert!(
        !status.success(),
        "`rigger dash` on a port held by a non-dash process must FAIL (non-zero), never drift or \
         defer; stdout: {out:?} stderr: {err:?}"
    );
    assert!(
        err.to_lowercase().contains("in use"),
        "the conflict must surface as an address-in-use error on stderr; stdout: {out:?} \
         stderr: {err:?}"
    );
    assert!(
        !out.contains("serving on") && !out.contains("already serving"),
        "a non-dash holder must NOT be reported as a serving/already-serving dash (no false \
         singleton defer, no drift to another port); stdout: {out:?} stderr: {err:?}"
    );

    drop(holder);
}

/// Spec 50, criterion 1, the SINGLETON under CONCURRENCY: with one genuine `rigger dash` already
/// serving the fixed address, MANY claimants racing that SAME address at once must EACH defer
/// cleanly - `bind_singleton` returns `AlreadyServing` for every one of them, never a second
/// `Bound` and never a spurious conflict error. The unit and CLI tests each serialize a SINGLE
/// claimant behind a fully-served winner; only racing N claimants simultaneously proves the
/// singleton holds under the real contention it exists to resolve (a manual `rigger dash` while
/// the step path is also ensuring one, or two runs starting together). Driving the public
/// `bind_singleton` API from the integration crate against a real serving process is the
/// outside-in view the in-crate single-threaded unit tests are structurally blind to.
#[test]
fn many_claimants_racing_a_live_serving_singleton_all_defer_cleanly() {
    use rigger::dash::{bind_singleton, SingletonBind};
    use std::net::SocketAddr;
    use std::process::Stdio;
    use std::sync::{Arc, Barrier};

    let proj = temp_project();
    let root = proj.path();
    let port = free_loopback_port();
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    let url = format!("http://127.0.0.1:{port}/");

    // The WINNER: a real, serving `rigger dash` process holding the address.
    let mut winner = common::rigger_courier()
        .args(["dash", "--port", &port.to_string()])
        .current_dir(root)
        .env_remove("RIGGER_NO_DASH")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("failed to spawn the serving `rigger dash`");

    if !matches!(http_get(&url), Some(body) if body.contains("rigger dash")) {
        let _ = winner.kill();
        let _ = winner.wait();
        panic!("the serving `rigger dash` never came up at {url}");
    }

    // N claimants race the SAME address concurrently; a start barrier maximizes the overlap so
    // they genuinely contend rather than run one-after-another.
    let n = 8usize;
    let barrier = Arc::new(Barrier::new(n));
    let mut handles = Vec::with_capacity(n);
    for _ in 0..n {
        let b = barrier.clone();
        handles.push(std::thread::spawn(move || {
            b.wait();
            bind_singleton(addr)
        }));
    }
    let results: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();

    // Reap the winner (the only serving process) BEFORE asserting so a failure never leaks a dash.
    let _ = winner.kill();
    let _ = winner.wait();

    for (i, r) in results.iter().enumerate() {
        match r {
            Ok(SingletonBind::AlreadyServing(reported)) => assert_eq!(
                *reported, addr,
                "claimant {i} must defer to the fixed address the singleton serves, not a drifted one"
            ),
            other => panic!(
                "claimant {i} racing a live serving singleton must defer (AlreadyServing), never \
                 bind a second port or error; got {other:?}"
            ),
        }
    }
}

/// Spec 50, criterion 1, the ATOMIC single-winner guarantee: when MANY claimants race the SAME
/// (initially free) address at once, EXACTLY ONE binds it and NO OTHER ever becomes a second
/// `Bound` - the singleton is atomic, not a check-then-bind two racers could both pass. Because
/// `bind_singleton` never searches upward, a loser never drifts to a different port; it either
/// recognizes an already-serving dash or surfaces the conflict, but it is NEVER a second holder
/// of the address. This is the cold-race complement to the live-serving defer test, and the
/// concurrency dimension the serialized unit tests cannot reach. Every result is collected before
/// any is dropped, so the winner holds the address for the whole race.
#[test]
fn a_concurrent_cold_race_yields_exactly_one_binder_never_two() {
    use rigger::dash::{bind_singleton, SingletonBind};
    use std::net::SocketAddr;
    use std::sync::{Arc, Barrier};

    let port = free_loopback_port();
    let addr = SocketAddr::from(([127, 0, 0, 1], port));

    let n = 8usize;
    let barrier = Arc::new(Barrier::new(n));
    let mut handles = Vec::with_capacity(n);
    for _ in 0..n {
        let b = barrier.clone();
        handles.push(std::thread::spawn(move || {
            b.wait();
            bind_singleton(addr)
        }));
    }
    // Collect ALL results first (keeping every `Bound` listener alive) so the winner holds the
    // address for the whole race - no result is dropped until the count below is settled.
    let results: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();

    let mut bound = 0usize;
    for r in &results {
        match r {
            Ok(SingletonBind::Bound(listener)) => {
                bound += 1;
                assert_eq!(
                    listener.local_addr().unwrap().port(),
                    port,
                    "the single winner must bind the EXACT raced address, never a drifted one"
                );
            }
            // A loser: it either recognized a serving dash or saw the raw conflict. Both are valid
            // loser outcomes; the one thing forbidden - a second `Bound`, or a `Bound` on a
            // drifted port - is caught by the count below and the port assertion above.
            Ok(SingletonBind::AlreadyServing(_)) | Err(_) => {}
        }
    }
    assert_eq!(
        bound, 1,
        "a concurrent race for one free address must produce EXACTLY ONE binder (the singleton is \
         atomic); got {bound}. results: {results:?}"
    );
}

/// Spec 50, criterion 1, the RECOGNITION CONTRACT through the BUILT binary: every `rigger dash`
/// response carries the `X-Rigger-Dash` header whose NAME is the public `dash::DASH_HEADER`
/// constant the singleton probe looks for. That header IS the mechanism by which a second
/// invocation's `dash_serving_on` probe recognizes an already-serving dash and defers cleanly -
/// the sibling singleton test proves the deferral only by CONSEQUENCE. Asserting the header
/// DIRECTLY on a real response localizes a regression: a dropped or renamed header would
/// otherwise surface only as a confusing "the second invocation refused to defer" failure
/// elsewhere. The header rides the root page every dash already serves, so the dash stays
/// read-only and gains no endpoint. The dash is a real, long-lived process; it is reaped BEFORE
/// the assertion so a failed assertion never leaks a dashboard.
#[test]
fn every_dash_response_carries_the_rigger_dash_recognition_header() {
    use std::io::{Read, Write};
    use std::process::Stdio;
    use std::time::{Duration, Instant};

    let proj = temp_project();
    let root = proj.path();
    let port = free_loopback_port();
    let url = format!("http://127.0.0.1:{port}/");

    let mut dash = common::rigger_courier()
        .args(["dash", "--port", &port.to_string()])
        .current_dir(root)
        .env_remove("RIGGER_NO_DASH")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("failed to spawn `rigger dash`");

    // Wait until the dash genuinely serves its page (the body helper polls the startup window).
    if !matches!(http_get(&url), Some(body) if body.contains("rigger dash")) {
        let _ = dash.kill();
        let _ = dash.wait();
        panic!("`rigger dash` never served its page at {url}");
    }

    // A raw GET that keeps the RESPONSE HEAD (`http_get` strips it): read the status line +
    // headers so the recognition header can be asserted directly.
    let hostport = format!("127.0.0.1:{port}");
    let head = {
        let deadline = Instant::now() + Duration::from_secs(15);
        let mut stream = loop {
            match std::net::TcpStream::connect(&hostport) {
                Ok(s) => break s,
                Err(_) if Instant::now() < deadline => {
                    std::thread::sleep(Duration::from_millis(50))
                }
                Err(_) => {
                    let _ = dash.kill();
                    let _ = dash.wait();
                    panic!("could not connect to the serving dash at {url}");
                }
            }
        };
        let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
        if stream
            .write_all(b"GET / HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
            .is_err()
        {
            let _ = dash.kill();
            let _ = dash.wait();
            panic!("could not write the probe request to the serving dash at {url}");
        }
        let mut resp = String::new();
        let _ = stream.read_to_string(&mut resp);
        // Keep only the head (status line + headers) up to the blank end-of-headers line.
        let end = resp.find("\r\n\r\n").unwrap_or(resp.len());
        resp[..end].to_string()
    };

    // Reap the dash (the ONLY serving process) BEFORE asserting so a failure never leaks it.
    let _ = dash.kill();
    let _ = dash.wait();

    // The recognition contract: a header line whose NAME (case-insensitively) is the public
    // `DASH_HEADER` constant. Line-anchored - the same discipline `dash_serving_on` uses - so the
    // marker cannot be satisfied by the same text appearing inside another header's value.
    let needle = format!("{}:", rigger::dash::DASH_HEADER).to_ascii_lowercase();
    let carries = head
        .lines()
        .any(|line| line.to_ascii_lowercase().starts_with(&needle));
    assert!(
        carries,
        "every dash response must carry the `{}` recognition header the singleton probe looks \
         for; response head was:\n{head}",
        rigger::dash::DASH_HEADER
    );
}

/// Spec 50, criterion 1, the singleton probe's BOUNDEDNESS end-to-end through the BUILT binary:
/// when the fixed address is squatted by a HOSTILE holder that ACCEPTS the probe connection and
/// then DRIBBLES bytes forever - one byte slower than any per-read timeout, NEVER a newline -
/// `rigger dash` must still terminate on a HARD bound and surface the address-in-use conflict,
/// never hang. The `dash.rs` unit test proves `dash_serving_on` itself is bounded in-process;
/// only driving the whole binary proves the FULL `cmd_dash -> bind_singleton -> dash_serving_on`
/// chain stays bounded in the shipped path - the outside-in view the single-process unit test is
/// structurally blind to. It guards exactly the boundary the whole-head-read bound hardens: a
/// regression to a per-read-only timeout would reset forever on the dribble and spin, so this
/// test's bounded `try_wait` deadline turns that regression into a LOUD caught hang, never a
/// silent wedge. The sibling non-dash-holder test drives a SILENT holder (bounded by the read
/// timeout); this drives an ACTIVE dribbler (bounded only by the overall head-read deadline).
#[test]
fn the_dash_singleton_probe_stays_bounded_against_a_dribbling_holder() {
    use std::io::{Read, Write};
    use std::process::Stdio;
    use std::time::{Duration, Instant};

    let proj = temp_project();
    let root = proj.path();

    // Bind the port and hold it for the whole run, so `rigger dash`'s bind is a genuine conflict.
    let holder = reserved_loopback_listener();
    let port = holder.local_addr().unwrap().port();

    // A worker that ACCEPTS the one singleton-probe connection (bounded so it never blocks
    // forever) and then dribbles a single byte every 120ms with NO newline - defeating any
    // per-read-only timeout so ONLY a bound on the TOTAL head-read can stop the probe. It never
    // sends the recognition header, so it must be treated as a conflict, never a serving dash.
    let dribbler = std::thread::spawn(move || {
        holder
            .set_nonblocking(true)
            .expect("make the holder non-blocking");
        let accept_deadline = Instant::now() + Duration::from_secs(20);
        let sock = loop {
            match holder.accept() {
                Ok((s, _)) => break Some(s),
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    if Instant::now() >= accept_deadline {
                        break None;
                    }
                    std::thread::sleep(Duration::from_millis(20));
                }
                Err(_) => break None,
            }
        };
        if let Some(mut s) = sock {
            let _ = s.set_nonblocking(false);
            // Drain whatever the probe wrote (best-effort) so a full receive buffer never stalls
            // it, then dribble until the probe gives up (its bounded read returns and it drops the
            // stream, so the next write fails).
            let _ = s.set_read_timeout(Some(Duration::from_millis(50)));
            let mut scratch = [0u8; 256];
            let _ = s.read(&mut scratch);
            while s.write_all(b"X").is_ok() && s.flush().is_ok() {
                std::thread::sleep(Duration::from_millis(120));
            }
        }
        // `holder` drops here, releasing the port - only after the probe has already concluded.
    });

    let mut dash = common::rigger_courier()
        .args(["dash", "--port", &port.to_string()])
        .current_dir(root)
        .env_remove("RIGGER_NO_DASH")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn `rigger dash` against a dribbling holder");

    // A bounded probe exits in ~1s; poll `try_wait` on a deadline comfortably above that so a
    // regression to an UNBOUNDED head-read (spinning on the dribble) is caught LOUD as a hang.
    let deadline = Instant::now() + Duration::from_secs(20);
    let exited = loop {
        match dash.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(50)),
            Ok(None) => break None,
            Err(_) => break None,
        }
    };

    let mut out = String::new();
    if let Some(mut so) = dash.stdout.take() {
        let _ = so.read_to_string(&mut out);
    }
    let mut err = String::new();
    if let Some(mut se) = dash.stderr.take() {
        let _ = se.read_to_string(&mut err);
    }

    // Reap a hung dash (if any) BEFORE asserting, then let the dribbler wind down.
    if exited.is_none() {
        let _ = dash.kill();
        let _ = dash.wait();
    }
    let _ = dribbler.join();

    let status = exited.unwrap_or_else(|| {
        panic!(
            "`rigger dash` never exited within the deadline against a dribbling holder - the \
             singleton probe's head-read must be time-bounded, never spin on a slow byte dribble; \
             stdout: {out:?} stderr: {err:?}"
        )
    });
    assert!(
        !status.success(),
        "a dribbling non-dash holder must be a genuine conflict (non-zero exit), never a serving \
         singleton to defer to; stdout: {out:?} stderr: {err:?}"
    );
    assert!(
        err.to_lowercase().contains("in use"),
        "the conflict must surface as an address-in-use error on stderr; stdout: {out:?} \
         stderr: {err:?}"
    );
    assert!(
        !out.contains("already serving"),
        "a dribbling non-dash holder must NOT be mistaken for an already-serving dash (no false \
         singleton defer); stdout: {out:?} stderr: {err:?}"
    );
}

/// Spec 50, criterion 1, the public `dash_serving_on` two-sided CONTRACT, driven by name from the
/// integration crate (as the sibling tests drive `bind_singleton`): it returns TRUE for a REAL
/// serving `rigger dash` (recognized by its `X-Rigger-Dash` header) and FALSE for an unrelated
/// process that merely holds the port and never answers. This is the precise input/output edge of
/// the probe that the singleton short-circuit hinges on (a real dash defers, a non-dash
/// conflicts), stated directly rather than only by consequence of the CLI behavior tests. The real
/// dash is reaped BEFORE the assertion so a failure never leaks a dashboard.
#[test]
fn dash_serving_on_recognizes_a_real_dash_and_rejects_a_non_dash_holder() {
    use rigger::dash::dash_serving_on;
    use std::process::Stdio;

    // Case 1 - a REAL serving `rigger dash`: `dash_serving_on` must recognize it (TRUE).
    let proj = temp_project();
    let root = proj.path();
    let dash_port = free_loopback_port();
    let url = format!("http://127.0.0.1:{dash_port}/");
    let mut dash = common::rigger_courier()
        .args(["dash", "--port", &dash_port.to_string()])
        .current_dir(root)
        .env_remove("RIGGER_NO_DASH")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("failed to spawn a serving `rigger dash`");
    if !matches!(http_get(&url), Some(body) if body.contains("rigger dash")) {
        let _ = dash.kill();
        let _ = dash.wait();
        panic!("the serving `rigger dash` never came up at {url}");
    }
    let recognized_real = dash_serving_on(dash_port);

    // Case 2 - a NON-dash holder that never answers the probe: `dash_serving_on` must reject it
    // (FALSE), bounded by its own read timeouts (the silent-holder half of the boundedness the
    // dribble test drives). Keeping the listener in scope holds the port for the whole probe.
    let non_dash = reserved_loopback_listener();
    let non_dash_port = non_dash.local_addr().unwrap().port();
    let recognized_non_dash = dash_serving_on(non_dash_port);

    // Reap the serving dash BEFORE asserting so a failure never leaks a dashboard.
    let _ = dash.kill();
    let _ = dash.wait();
    drop(non_dash);

    assert!(
        recognized_real,
        "dash_serving_on must recognize a REAL serving `rigger dash` by its `{}` header",
        rigger::dash::DASH_HEADER
    );
    assert!(
        !recognized_non_dash,
        "dash_serving_on must NOT recognize a non-dash holder that never sends the recognition \
         header - only a genuine dash earns the singleton defer"
    );
}
