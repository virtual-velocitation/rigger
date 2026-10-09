//! PERIPHERY (contract / API / integration) tests for `rigger reset --derived`, the one-time
//! migration of the event log, driven through the built binary and read back from the files an
//! operator can open.
//!
//! What this file holds, each a property the migration must keep at the command's surface:
//!
//!   1. **The namespace boundary.** The migration reaches the run stream of the project it was
//!      run in and no other: a store shared with a project whose identity differs from this one
//!      by a character SQL reads as a wildcard is left byte-for-byte, and the bare menu previews
//!      that one stream's derived events and no neighbour's, as `rigger validate`'s log-bloat
//!      advisory warns of that one stream's alone.
//!   2. **The flag registry.** Each mode is named at most once and the two compose in either
//!      order, each reporting its own work.
//!   3. **The shipped operator-facing artifacts.** The usage the binary prints and the committed
//!      documents say what the migration does, and the documents are the current render.
//!   4. **Two accumulations, two modes.** `--runs` changes the graph and leaves the log alone,
//!      `--derived` converts the log and writes nothing to the graph, and composing them does
//!      exactly both.
//!   5. **The store the command walked up to.** Run from a nested worktree it migrates the
//!      project's store and reads each file under the tree that store belongs to, never under
//!      the working directory's own top level.
//!   6. **The number.** The bytes the command says it reclaimed are the bytes the log lost on
//!      disk.

mod common;
use common::git::run_git;

use common::cli::migrated_lines;
use common::cli::pre_ledger_batch;
use common::cli::reclaimed_line;
use common::cli::reported_reclaimed_bytes;
use common::cli::rigger_file;
use common::cli::run_rigger;
use common::cli::run_stream_identity;
use common::cli::temp_rigger_project;
use common::cli::{LOG_LEFT_AS_IT_STANDS_LINE, NOTHING_TO_SHED_LINE};
use common::fixtures::{file_len, meta_replay_key, plant_free_pages, pragma_i64};
use common::repo::repo_text;
use rigger::contextgraph::sqlite::Projector;
use rigger::eventstore::namespace::Namespaced;
use rigger::eventstore::sqlite::Store;
use rigger::eventstore::{Direction, Event, EventStore, ExpectedRevision};
use std::collections::BTreeSet;
use std::path::Path;
use std::time::{Duration, UNIX_EPOCH};

// ---------------------------------------------------------------------------------------
// Harness
// ---------------------------------------------------------------------------------------

/// One row of the event log as the table holds it: position, stream, type, id, payload bytes,
/// metadata, and per-stream revision. Comparing these tuples is what "untouched" MEANS - a row
/// that kept its bytes but was renumbered, or moved to another position, is not untouched.
type Row = (i64, String, String, String, Vec<u8>, String, i64);

fn raw_rows(db: &Path) -> Vec<Row> {
    let conn = rusqlite::Connection::open(db).expect("open the event log");
    let mut stmt = conn
        .prepare(
            "SELECT position, stream, type, id, data, meta, revision FROM events ORDER BY position",
        )
        .unwrap();
    let out = stmt
        .query_map([], |r| {
            Ok((
                r.get(0)?,
                r.get(1)?,
                r.get(2)?,
                r.get(3)?,
                r.get(4)?,
                r.get(5)?,
                r.get(6)?,
            ))
        })
        .unwrap()
        .map(|r| r.unwrap())
        .collect();
    out
}

fn rows_in(rows: &[Row], prefix: &str) -> Vec<Row> {
    rows.iter()
        .filter(|r| r.1.starts_with(prefix))
        .cloned()
        .collect()
}

/// Seed `project`'s namespace inside `backend` with `rounds` recordings of one derived batch of
/// two events - a log recorded before the ledger - preceded by one non-derived event the
/// migration must leave alone. Written THROUGH `Namespaced::new`, so the streams it creates are
/// named by the very code path the published `Namespaced::prefix_for` claims to speak for, over
/// the pre-ledger store of the file `db`, since a store refuses a derived event.
fn seed_namespace(db: &Path, project: &str, rounds: u64) {
    let backend = Store::open(db.to_str().unwrap()).expect("open the event log");
    let pre_ledger = common::fixtures::PreLedgerStore {
        db,
        inner: &backend,
    };
    let store = Namespaced::new(&pre_ledger, project);
    let mut events = vec![Event::new(
        "RunStarted",
        format!(r#"{{"run":"{project}","criteria":["c"]}}"#).into_bytes(),
    )
    .with_valid_from(UNIX_EPOCH + Duration::from_secs(10))];
    for r in 0..rounds {
        events.extend(pre_ledger_batch(project, 1_000 + r));
    }
    store
        .append(rigger::conductor::STREAM, ExpectedRevision::Any, &events)
        .expect("seed the namespace");
}

fn seed_project(root: &Path, rounds: u64) {
    seed_namespace(
        &rigger_file(root, "events.db"),
        &run_stream_identity(root),
        rounds,
    );
}

/// The ledger entry the migration converts the seeded batch into: the generation `h1` of
/// `src/a.rs`, a batch of two events, recording `blob`.
fn seeded_entry(blob: &str) -> Event {
    common::fixtures::generation_ingested("gc", "src/a.rs", "h1", blob, false).event(2)
}

/// The rows of `rows` holding a ledger entry, each as `(position, payload, replay key)`.
fn entries(rows: &[Row]) -> Vec<(i64, Vec<u8>, Option<String>)> {
    rows.iter()
        .filter(|r| r.2 == rigger::retention::TYPE_GENERATION_INGESTED)
        .map(|r| (r.0, r.4.clone(), meta_replay_key(&r.5)))
        .collect()
}

/// `(position, payload, replay key)` of [`seeded_entry`] standing at `position`.
fn entry_at(position: i64, blob: &str) -> (i64, Vec<u8>, Option<String>) {
    let entry = seeded_entry(blob);
    (
        position,
        entry.data.clone(),
        Some("gc/src/a.rs@h1#2".to_string()),
    )
}

/// What `rigger reset --derived` printed of the migration: `out` from its first own line.
fn derived_report(out: &str) -> &str {
    let at = out
        .find("reset --derived: ")
        .unwrap_or_else(|| panic!("the command must report the migration; got {out:?}"));
    &out[at..]
}

// ---------------------------------------------------------------------------------------
// 1. The namespace boundary: the migration reaches EXACTLY one project's run stream
// ---------------------------------------------------------------------------------------

#[test]
fn the_prune_reaches_only_the_namespace_it_was_handed_and_matches_that_prefix_literally() {
    // One store file holding three projects - the shape `Namespaced` is documented to serve.
    // `my_repo` is the project the command runs in and its identity carries a SQL wildcard
    // (`_`); `myXrepo` is the namespace a `LIKE`-based match would sweep in with it; `other` is
    // an ordinary unrelated neighbour.
    const TARGET: &str = "my_repo";
    const WILDCARD_NEIGHBOUR: &str = "myXrepo";
    const NEIGHBOUR: &str = "other";
    const ROUNDS: u64 = 6;
    let dir = project_pinned_to(TARGET);
    let root = dir.path();
    assert_eq!(run_stream_identity(root), TARGET);
    let db = rigger_file(root, "events.db");
    for project in [TARGET, WILDCARD_NEIGHBOUR, NEIGHBOUR] {
        seed_namespace(&db, project, ROUNDS);
    }
    let before = raw_rows(&db);
    let target_prefix = Namespaced::prefix_for(TARGET);
    let seeded_target = rows_in(&before, &target_prefix);
    assert_eq!(seeded_target.len(), 1 + 2 * ROUNDS as usize);

    let (out, err, ok) = run_rigger(root, &["reset", "--derived"]);
    assert!(ok, "reset --derived must succeed; stderr: {err}\n{out}");
    assert_eq!(
        out,
        migrated_lines(1, 2 * ROUNDS as usize, 0) + LOG_LEFT_AS_IT_STANDS_LINE,
        "the migration converts the target's one batch and sheds its twelve derived events"
    );

    // Inside the target: the run's own event as it stood, then the entry at the first row of the
    // last recording of the batch, and nothing else.
    let after = raw_rows(&db);
    let migrated = rows_in(&after, &target_prefix);
    let last_batch = &seeded_target[seeded_target.len() - 2];
    assert_eq!(migrated.len(), 2);
    assert_eq!(migrated[0], seeded_target[0]);
    assert_eq!(
        entries(&migrated),
        vec![entry_at(last_batch.0, "")],
        "the entry stands where the latest batch's first row stood, recording no blob for a \
         file the tree does not hold"
    );
    assert_eq!(
        (migrated[1].3.as_str(), migrated[1].6),
        (last_batch.3.as_str(), last_batch.6),
        "the rewritten row keeps its id and its revision"
    );

    // AND NOWHERE ELSE. Every other namespace is byte-for-byte identical, INCLUDING each row's
    // global position and per-stream revision.
    for project in [WILDCARD_NEIGHBOUR, NEIGHBOUR] {
        let prefix = Namespaced::prefix_for(project);
        let expected = rows_in(&before, &prefix);
        assert_eq!(expected.len(), 1 + 2 * ROUNDS as usize);
        assert_eq!(
            rows_in(&after, &prefix),
            expected,
            "reset --derived on {TARGET} must leave {project}'s streams byte-for-byte untouched"
        );
    }
    assert_eq!(before.len() - after.len(), 2 * ROUNDS as usize - 1);
}

/// The three projects [`store_shared_by_three_projects`] seeds into one store file, each with the
/// recordings of its derived batch: the project the command runs in, a neighbour whose identity
/// differs from it by a character SQL reads as a wildcard, and an unrelated one.
const SHARING: [(&str, u64); 3] = [("my_repo", 6), ("myXrepo", 2), ("other", 1)];

/// A project pinned to the first identity of [`SHARING`] whose store file holds all three: its
/// own run's event and twelve derived events, the wildcard neighbour's and four, the unrelated
/// one's and two.
fn store_shared_by_three_projects() -> tempfile::TempDir {
    let dir = project_pinned_to(SHARING[0].0);
    let root = dir.path();
    assert_eq!(run_stream_identity(root), SHARING[0].0);
    let db = rigger_file(root, "events.db");
    for (project, rounds) in SHARING {
        seed_namespace(&db, project, rounds);
    }
    assert_eq!(
        rows_held_by_each_sharing_project(&raw_rows(&db)),
        [13, 5, 3],
        "premise: each namespace holds its run's own event and two derived events a recording"
    );
    dir
}

/// How many of `rows` each project of [`SHARING`] holds, in its order.
fn rows_held_by_each_sharing_project(rows: &[Row]) -> [usize; 3] {
    SHARING.map(|(project, _)| rows_in(rows, &Namespaced::prefix_for(project)).len())
}

/// Given one store file holding three projects, each with a different number of recordings of
/// its derived batch ([`store_shared_by_three_projects`]) - the project the command runs in
/// (twelve derived events), a neighbour whose
/// identity differs from it by a character SQL reads as a wildcard (four) and an unrelated one
/// (two) - when the operator runs bare `rigger reset`, then its `--derived` line names the twelve
/// events of the one file identity this project's run stream holds and no neighbour's, and every
/// row of the file stands as it stood; when `rigger reset --derived` then sheds exactly those
/// twelve, bare `rigger reset` says none is left to shed while each neighbour still holds its own.
#[test]
fn bare_reset_names_the_derived_events_of_its_own_namespace_alone_on_a_store_it_shares() {
    use common::cli::{derived_menu_line_naming, derived_menu_lines, NOTHING_TO_SHED_MENU_LINE};

    let dir = store_shared_by_three_projects();
    let root = dir.path();
    let db = rigger_file(root, "events.db");
    let before = raw_rows(&db);
    let held = rows_held_by_each_sharing_project;

    let (menu, menu_err, menu_ok) = run_rigger(root, &["reset"]);
    assert_eq!(
        (menu_ok, derived_menu_lines(&menu)),
        (true, vec![derived_menu_line_naming(12, 1).as_str()]),
        "the menu names this project's twelve derived events and none of the six its neighbours \
         hold; its stderr: {menu_err}"
    );
    assert_eq!(
        raw_rows(&db),
        before,
        "the menu changes no row of any namespace"
    );

    let (shed, shed_err, shed_ok) = run_rigger(root, &["reset", "--derived"]);
    assert_eq!(
        (shed_ok, shed),
        (true, migrated_lines(1, 12, 0) + LOG_LEFT_AS_IT_STANDS_LINE),
        "the derived reset sheds the twelve events the menu named; its stderr: {shed_err}"
    );
    let migrated = raw_rows(&db);
    assert_eq!(
        held(&migrated),
        [2, 5, 3],
        "the target keeps its run's own event and the one entry, each neighbour every row"
    );

    let (after, after_err, after_ok) = run_rigger(root, &["reset"]);
    assert_eq!(
        (after_ok, derived_menu_lines(&after)),
        (true, vec![NOTHING_TO_SHED_MENU_LINE]),
        "the menu says none is left to shed, the neighbours' six derived events being no part of \
         what this project's derived reset sheds; its stderr: {after_err}"
    );
    assert_eq!(
        raw_rows(&db),
        migrated,
        "the menu changes no row once migrated"
    );
}

/// Given one store file holding three projects ([`store_shared_by_three_projects`]), when the
/// operator runs `rigger validate` in the first, then it exits 0 and warns once naming the twelve
/// derived events of the one file identity this project's run stream holds and no neighbour's,
/// and every row of the file stands as it stood; and once `rigger reset --derived` has migrated
/// this project, `rigger validate` prints no such warning while each neighbour still holds its
/// own derived events.
#[test]
fn validate_warns_of_the_derived_events_of_its_own_namespace_alone_on_a_store_it_shares() {
    use common::cli::{bloat_advisory_naming, bloat_lines};

    let dir = store_shared_by_three_projects();
    let root = dir.path();
    let db = rigger_file(root, "events.db");
    let (_, init_err, inited) = run_rigger(root, &["init"]);
    assert!(inited, "premise: init scaffolds the project: {init_err}");
    assert_eq!(
        run_stream_identity(root),
        SHARING[0].0,
        "premise: init keeps the pinned identity"
    );
    let before = raw_rows(&db);
    assert_eq!(
        rows_held_by_each_sharing_project(&before),
        [13, 5, 3],
        "premise: init adds no row to any namespace"
    );

    let (_, err, ok) = run_rigger(root, &["validate"]);
    assert_eq!(
        (ok, bloat_lines(&err)),
        (true, vec![bloat_advisory_naming(12, 1).as_str()]),
        "validate names this project's twelve derived events and none of the six its neighbours \
         hold; its stderr: {err}"
    );
    assert_eq!(
        raw_rows(&db),
        before,
        "the advisory changes no row of any namespace"
    );

    let (shed, shed_err, shed_ok) = run_rigger(root, &["reset", "--derived"]);
    assert_eq!(
        (shed_ok, shed),
        (true, migrated_lines(1, 12, 0) + LOG_LEFT_AS_IT_STANDS_LINE),
        "premise: the derived reset sheds the twelve events the advisory named; its stderr: \
         {shed_err}"
    );
    let migrated = raw_rows(&db);
    assert_eq!(
        rows_held_by_each_sharing_project(&migrated),
        [2, 5, 3],
        "premise: the target keeps its run's own event and the one entry, each neighbour every row"
    );

    let (_, after_err, after_ok) = run_rigger(root, &["validate"]);
    assert_eq!(
        (after_ok, bloat_lines(&after_err)),
        (true, vec![""; 0]),
        "a migrated project draws no warning, the neighbours' six derived events being no part \
         of what its derived reset sheds; its stderr: {after_err}"
    );
    assert_eq!(
        raw_rows(&db),
        migrated,
        "the advisory changes no row once migrated"
    );
}

// ---------------------------------------------------------------------------------------
// 5. The command's flag registry, at the edges the second mode opened
// ---------------------------------------------------------------------------------------

#[test]
fn reset_accepts_each_mode_at_most_once_and_composes_the_two_in_either_order() {
    let dir = temp_rigger_project();
    let root = dir.path();
    seed_project(root, 3);

    // A repeated mode is a typed mistake, not an instruction to run it twice: it is refused, and
    // the refusal names the flag that was repeated so the operator can see which one it was.
    for flag in ["--derived", "--runs"] {
        let (out, err, ok) = run_rigger(root, &["reset", flag, flag]);
        assert!(!ok, "reset {flag} {flag} must be refused; stdout: {out}");
        let said = format!("{err}{out}");
        assert!(
            said.contains(flag) && said.contains("more than once"),
            "the refusal must name the repeated flag; got {said:?}"
        );
    }

    // The modes are named, not positional: composing them the other way round does the same two
    // things. Each still reports its own work, so neither silently swallows the other.
    let (out, err, ok) = run_rigger(root, &["reset", "--derived", "--runs"]);
    assert!(
        ok,
        "reset --derived --runs must succeed; stderr: {err}\n{out}"
    );
    assert!(
        out.contains("reset --runs:") && out.contains("reset --derived:"),
        "a composed reset must report BOTH modes whichever order they were named in; got {out:?}"
    );
    assert!(
        out.starts_with("reset --runs: "),
        "the graph's mode reports first whichever order the two were named in; got {out:?}"
    );
    assert_eq!(
        derived_report(&out),
        migrated_lines(1, 6, 0) + LOG_LEFT_AS_IT_STANDS_LINE,
        "the composed --derived migration reports exactly what it reports alone"
    );
}

// ---------------------------------------------------------------------------------------
// 7. The shipped operator-facing artifacts: the usage registry and the committed documents
// ---------------------------------------------------------------------------------------

/// The `reset` modes the binary's usage registry ADVERTISES, read out of the help it actually
/// prints. Deriving the set from the running binary rather than naming it here is what makes a
/// third mode covered by the assertions below without anyone editing this test.
fn advertised_reset_modes(help: &str) -> Vec<String> {
    let mut modes = BTreeSet::new();
    for line in help.lines() {
        if let Some(rest) = line.trim_start().strip_prefix("rigger reset ") {
            if let Some(mode) = rest.split_whitespace().next() {
                if let Some(stripped) = mode.strip_prefix("--") {
                    modes.insert(format!("--{stripped}"));
                }
            }
        }
    }
    modes.into_iter().collect()
}

/// The registry entry for one `rigger reset <mode>`: its own line and the wrapped continuation
/// lines under it, up to whatever entry comes next.
fn registry_entry(help: &str, mode: &str) -> String {
    let opener = format!("rigger reset {mode}");
    let mut out: Vec<&str> = Vec::new();
    for line in help.lines() {
        let trimmed = line.trim_start();
        if out.is_empty() {
            if trimmed.starts_with(&opener) {
                out.push(trimmed);
            }
            continue;
        }
        if trimmed.starts_with("rigger ") {
            break;
        }
        out.push(trimmed);
    }
    assert!(
        !out.is_empty(),
        "the usage registry must carry an entry for `{opener}`; got {help:?}"
    );
    out.join(" ")
}

/// The usage registry is the only description of `reset --derived` an operator gets from the
/// binary itself.
///
/// Two directions, both driven against the built binary:
///
///   - Every mode the registry ADVERTISES is a mode the parser accepts and actually runs. A
///     registry that documents a flag the binary refuses is worse than no documentation, because
///     it is the operator's evidence that they typed the right thing.
///   - A mode the registry does NOT advertise is refused, and the refusal names every advertised
///     mode - so the parser's own error text and the registry cannot drift into disagreeing about
///     what this command takes.
///
/// The `--derived` entry is additionally held to the two facts that decide whether an operator
/// reaches for it at all: WHICH store it migrates (the event log, not the graph) and that it
/// COMPOSES with `--runs` rather than replacing it.
#[test]
fn the_usage_registry_advertises_the_derived_prune_and_every_mode_it_advertises_is_real() {
    let dir = temp_rigger_project();
    let (out, err, ok) = run_rigger(dir.path(), &["--help"]);
    assert!(ok, "rigger --help must succeed; stderr: {err}\n{out}");
    let help = format!("{err}{out}");

    let modes = advertised_reset_modes(&help);
    assert!(
        modes.iter().any(|m| m == "--derived"),
        "the usage registry must advertise the migration; it advertises {modes:?}"
    );

    let derived = registry_entry(&help, "--derived");
    assert!(
        derived.contains("EVENT LOG"),
        "the --derived entry must say WHICH store it migrates, since --runs prunes a different \
         one; got {derived:?}"
    );
    assert!(
        derived.contains("--runs"),
        "the --derived entry must say it composes with the mode that was already there; got \
         {derived:?}"
    );

    // EVERY ADVERTISED MODE IS REAL. Each runs in its own freshly seeded project so one mode's
    // work cannot be what makes the next one look like it worked.
    for mode in &modes {
        let project = temp_rigger_project();
        seed_project(project.path(), 3);
        let (out, err, ok) = run_rigger(project.path(), &["reset", mode]);
        let said = format!("{err}{out}");
        assert!(
            ok,
            "the registry advertises `rigger reset {mode}`, so the binary must accept it; got \
             {said:?}"
        );
        assert!(
            !said.contains("expected --runs and/or --derived"),
            "`rigger reset {mode}` is advertised, so it must not be refused as unrecognized; got \
             {said:?}"
        );
    }

    // AND NOTHING ELSE IS. An unadvertised mode is refused, and the refusal enumerates exactly the
    // modes the registry advertises, so a mode added to one and not the other cannot go unnoticed.
    let project = temp_rigger_project();
    seed_project(project.path(), 3);
    let (out, err, ok) = run_rigger(project.path(), &["reset", "--everything"]);
    let said = format!("{err}{out}");
    assert!(
        !ok,
        "an unadvertised reset mode must be refused; got {said:?}"
    );
    for mode in &modes {
        assert!(
            said.contains(mode.as_str()),
            "the refusal must name every mode the registry advertises ({modes:?}), so the two \
             cannot disagree about what reset takes; got {said:?}"
        );
    }
}

/// The two documents carrying the `--derived` guidance, at the path they ship from.
const SHIPPED_DOCS: [&str; 2] = [
    "skills/using-rigger/SKILL.md",
    "docs/handbook/using-rigger.md",
];

/// The COMMITTED operator guidance, asserted on the bytes on disk with no render in the loop.
///
/// The render test in `crates/rigger-domain/src/docs.rs` renders `discipline_body` from a
/// SENTINEL context; these two files are rendered from the REAL one. "The sentinel render
/// carries the paragraph" and "the committed file equals a fresh real render" together still do
/// not give "the committed file carries the paragraph", so the shipped bytes are read directly
/// and held to the things an operator must know before running the migration: WHAT IT CONVERTS,
/// what it deletes, what it leaves alone, what it reports, WHAT IT COSTS, and that the two modes
/// compose.
///
/// AND THEN TO THE RENDERER'S OWN TEXT, WORD FOR WORD, which is the assertion that goes red on
/// staleness: a uniformly stale render satisfies every substring, and so does a stale render
/// that agrees with its equally stale sibling. It doubles as the proof that this paragraph is
/// not context-conditional: it is compared against a render from a context that shares no value
/// with the real one.
#[test]
fn the_committed_operator_documents_ship_the_derived_prunes_guidance() {
    let mut paragraphs: Vec<(String, String)> = Vec::new();

    for rel in SHIPPED_DOCS {
        let shipped = repo_text(rel);

        for (fact, needle) in [
            ("name the command", "rigger reset --derived"),
            ("say which store it migrates", "EVENT LOG"),
            ("say it runs once", "ONE-TIME MIGRATION"),
            (
                "say what it CONVERTS",
                "rewrites the first row of the file's LATEST derived batch into that \
                 generation's ledger entry, in place",
            ),
            (
                "say what it deletes",
                "deletes every other derived event, so no derived event is left behind",
            ),
            ("say what it costs everything else", "byte-for-byte"),
            ("say the file actually shrinks", "shrinks on disk"),
            (
                "say it writes nothing to the graph",
                "writes nothing to graph.db",
            ),
            (
                "say the unkeyed count has its own line",
                "on its own line, how many of those named no file",
            ),
            (
                "say what a second run reports",
                "there is no derived event to shed",
            ),
            (
                "say where the compaction stages its copy of the log",
                "in the process's MEMORY while it does, never in a temporary directory",
            ),
            (
                "say what the rewrite takes on the log's own partition",
                "the partition holding .rigger/ needs about the compacted size of the log free",
            ),
            (
                "say a run with nothing to shed does not rewrite the file",
                "leaves the file exactly as it found it",
            ),
            (
                "show the two modes composing",
                "rigger reset --runs --derived",
            ),
        ] {
            assert!(
                shipped.contains(needle),
                "the committed {rel} must {fact} ({needle:?}); an operator reads this file, not a \
                 fresh render of it"
            );
        }

        // The document was rendered from the REAL context, not the sentinel one the render test
        // uses: the dashboard address it quotes is the port the code actually defaults to.
        assert!(
            shipped.contains(&format!("127.0.0.1:{}", rigger::dash::DEFAULT_PORT)),
            "the committed {rel} must be a render of the real context (dash port {})",
            rigger::dash::DEFAULT_PORT
        );

        paragraphs.push((rel.to_string(), derived_paragraph(rel, &shipped)));
    }

    // ONE body, two consumers: the skill and the handbook chapter render from the same
    // `discipline_body`, so the paragraph an operator reads must be the same one whichever
    // document they opened.
    let (first_rel, first) = &paragraphs[0];
    for (rel, paragraph) in &paragraphs[1..] {
        assert_eq!(
            paragraph, first,
            "{rel} and {first_rel} render from one shared discipline body, so their --derived \
             guidance must not have drifted apart"
        );
    }

    // AND IT IS THE CURRENT PARAGRAPH, not merely a paragraph the two files agree on.
    let rendered = derived_paragraph(
        "<discipline_body>",
        &rigger::docs::render_handbook_discipline(&docs_context_for_paragraph_comparison()),
    );
    for (rel, paragraph) in &paragraphs {
        assert_eq!(
            paragraph, &rendered,
            "the committed {rel} carries a STALE `--derived` paragraph: it is not what \
             `rigger::docs` renders today. Re-render it with a `rigger` built from THIS tree \
             (`cargo build` and run that binary's `docs`, or `cargo install --path .` first) and \
             commit the result"
        );
    }
}

/// The ONE `rigger reset --derived` paragraph in a rendered or shipped discipline document.
///
/// Exactly one, asserted rather than assumed: taking the first of several would let a second
/// mention elsewhere in the document silently become what the comparison above is about.
fn derived_paragraph(what: &str, text: &str) -> String {
    let mut found: Vec<&str> = text
        .lines()
        .filter(|l| l.contains("rigger reset --derived"))
        .collect();
    assert_eq!(
        found.len(),
        1,
        "{what} must carry exactly one `rigger reset --derived` paragraph; got {}: {found:?}",
        found.len()
    );
    found.remove(0).to_string()
}

/// A docs context that shares NO value with the real one the shipped documents were rendered from.
///
/// That is the point of it. The paragraph this test compares must be the same text whatever the
/// context says, so rendering it from a context whose every field is deliberately unlike the real
/// one turns "these two paragraphs are equal" into a proof that the paragraph is unconditional -
/// which is exactly the hole a comparison against the real context would leave open.
fn docs_context_for_paragraph_comparison() -> rigger::docs::DocsContext {
    rigger::docs::DocsContext {
        base_ref: "not-the-real-base-ref".into(),
        dash_port: 65531,
        max_retries: 999,
        verdict_approve: "not-the-real-verdict".into(),
        spec_shape_rules: vec!["not-a-real-rule".into()],
        spec_shape_recommendation: "not the real recommendation".into(),
        subcommands: vec!["not-a-real-subcommand".into()],
        specs_location: "not/the/real/specs".into(),
        watch_signals: [
            rigger::docs::WatchSignalFact {
                name: "not-a-real-signal-1".into(),
                response: "not-a-real-response-1".into(),
            },
            rigger::docs::WatchSignalFact {
                name: "not-a-real-signal-2".into(),
                response: "not-a-real-response-2".into(),
            },
            rigger::docs::WatchSignalFact {
                name: "not-a-real-signal-3".into(),
                response: "not-a-real-response-3".into(),
            },
            rigger::docs::WatchSignalFact {
                name: "not-a-real-signal-4".into(),
                response: "not-a-real-response-4".into(),
            },
            rigger::docs::WatchSignalFact {
                name: "not-a-real-signal-5".into(),
                response: "not-a-real-response-5".into(),
            },
        ],
        watch_poll_interval_secs: 999_999,
        reject_recurrence_diagnose_threshold: 999,
        grep_guard_message: "not the real grep-guard message".into(),
    }
}

// ---------------------------------------------------------------------------------------
// 8. Two piles, two modes: each sheds ONLY its own, and composing them does exactly both
// ---------------------------------------------------------------------------------------

/// The identity every fixture in this section is pinned to, so projects living under different
/// temp directories resolve to the SAME namespace and their stores compare row for row.
const PINNED_ID: &str = "compaction-fixture";

/// The decision the DEAD run recorded. It is the graph's half of the fixture: `--runs` must drop
/// its node, which is what makes "the graph changed" a fact rather than an assumption.
const DEAD_DECISION: &str = "d-dead-run";

/// A temp project whose identity is PINNED in `.rigger/project.id` - the first rung the binary's
/// identity resolution reads, and the one [`run_stream_identity`] mirrors. Without it each fixture
/// would take its identity from its own temp directory name, so two identically-seeded projects
/// would write their events under two different stream names and could not be compared.
fn pinned_project() -> tempfile::TempDir {
    project_pinned_to(PINNED_ID)
}

/// A temp project whose identity is pinned to `id` in `.rigger/project.id`.
fn project_pinned_to(id: &str) -> tempfile::TempDir {
    let dir = temp_rigger_project();
    std::fs::write(dir.path().join(".rigger").join("project.id"), id)
        .expect("pin the project identity");
    dir
}

/// The context graph's LIVE content: its nodes, and the edges that have not been retired. This is
/// what "the live graph is unchanged" has to mean - the file itself is rebuilt by the `--runs` vacuum,
/// so bytes on disk would answer the wrong question.
fn graph_rows(db: &Path) -> (Vec<String>, Vec<String>) {
    let conn = rusqlite::Connection::open(db).expect("open the context graph");
    let mut nodes: Vec<String> = conn
        .prepare("SELECT id, kind, COALESCE(attrs,''), project FROM nodes")
        .unwrap()
        .query_map([], |r| {
            Ok(format!(
                "{}|{}|{}|{}",
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?
            ))
        })
        .unwrap()
        .map(|r| r.unwrap())
        .collect();
    nodes.sort();
    let mut edges: Vec<String> = conn
        .prepare("SELECT from_id, to_id, rel, project, tier FROM edges WHERE valid_to IS NULL")
        .unwrap()
        .query_map([], |r| {
            Ok(format!(
                "{}|{}|{}|{}|{}",
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, String>(4)?
            ))
        })
        .unwrap()
        .map(|r| r.unwrap())
        .collect();
    edges.sort();
    (nodes, edges)
}

/// A row with its event id dropped. Ids are minted per event, so two identically-seeded projects
/// agree on every column BUT that one; comparing their logs is a comparison of this shape.
fn shape(rows: &[Row]) -> Vec<(i64, String, String, Vec<u8>, String, i64)> {
    rows.iter()
        .map(|r| (r.0, r.1.clone(), r.2.clone(), r.4.clone(), r.5.clone(), r.6))
        .collect()
}

/// Where two row lists first part company, named compactly: `position/type/revision` on each side.
/// The comparisons below stay EXACT (whole rows, payload bytes included) - this only decides what a
/// failure prints, because a raw dump of two logs is unreadable in a gate log.
fn first_difference<T: PartialEq + std::fmt::Debug>(left: &[T], right: &[T]) -> String {
    for (i, (l, r)) in left.iter().zip(right.iter()).enumerate() {
        if l != r {
            return format!("index {i}: {l:?} vs {r:?}");
        }
    }
    format!("lengths {} vs {}", left.len(), right.len())
}

/// The compact form of a row a failure names: position, type, and per-stream revision.
fn row_marks(rows: &[Row]) -> Vec<String> {
    rows.iter()
        .map(|r| format!("{}/{}/{}", r.0, r.2, r.6))
        .collect()
}

/// Seed BOTH of the project's stores from ONE trajectory, the way a real project accumulates them:
/// a dead run that recorded a decision, the active run that superseded it, and `rounds`
/// re-recordings of two derived keys - then fold the log as it was WRITTEN into `.rigger/graph.db`.
///
/// Each mode therefore has its own pile waiting: the graph holds a dead run's node for `--runs`,
/// the log holds the derived index for `--derived`. That is the precondition for separating
/// "this mode did nothing to the other store" from "there was nothing to do".
fn seed_both_stores(root: &Path, rounds: u64) {
    let id = run_stream_identity(root);
    let mut events = vec![
        Event::new("RunStarted", br#"{"run":"dead","criteria":["c"]}"#.to_vec())
            .with_valid_from(UNIX_EPOCH + Duration::from_secs(10)),
    ];
    events.push(
        Event::new(
            "DecisionMade",
            format!(
                r#"{{"id":"{DEAD_DECISION}","summary":"s","governs":["src/a.rs"],"supersedes":""}}"#
            )
            .into_bytes(),
        )
        .with_valid_from(UNIX_EPOCH + Duration::from_secs(11)),
    );
    events.push(
        Event::new("RunStarted", br#"{"run":"live","criteria":["c"]}"#.to_vec())
            .with_valid_from(UNIX_EPOCH + Duration::from_secs(20)),
    );
    for r in 0..rounds {
        events.extend(pre_ledger_batch("alpha", 1_000 + r));
    }

    let events_db = rigger_file(root, "events.db");
    let backend = Store::open(events_db.to_str().unwrap()).unwrap();
    let store = Namespaced::new(&backend, &id);
    Namespaced::new(
        &common::fixtures::PreLedgerStore {
            db: &events_db,
            inner: &backend,
        },
        &id,
    )
    .append(rigger::conductor::STREAM, ExpectedRevision::Any, &events)
    .expect("seed the event log");

    // Fold the log AS WRITTEN - each event carrying the position the store gave it - so the graph
    // is the projection of this log rather than of a pre-append copy of it.
    let written = store
        .read_stream(rigger::conductor::STREAM, 0, Direction::Forward)
        .expect("read the seeded log back");
    let graph = Projector::open(rigger_file(root, "graph.db").to_str().unwrap(), &id)
        .expect("open the context graph");
    common::fixtures::folds(&graph, &written);
}

/// `rigger reset` drives TWO modes over TWO stores, and each one tells the operator it left the
/// other alone: `reset --runs` prints that it deletes no event from the log, and the shipped
/// `--derived` guidance says it writes nothing to graph.db. Both claims are load-bearing
/// precisely BECAUSE the modes compose - an operator who runs them together has no way to
/// attribute a loss to one of them, so each has to be safe for the other's store on its own.
///
/// Both stores are populated with a pile for EACH mode - a dead run's node for `--runs`, the
/// derived index for `--derived` - so "it did not touch the other store" is separated from
/// "there was nothing to do". The composition then has exactly one honest outcome - the two
/// effects, neither more nor less - asserted against the two single-mode results.
#[test]
fn each_reset_mode_sheds_only_its_own_accumulation_and_composing_them_does_exactly_both() {
    const ROUNDS: u64 = 5;

    let runs_only = pinned_project();
    let derived_only = pinned_project();
    let composed = pinned_project();
    for project in [&runs_only, &derived_only, &composed] {
        assert_eq!(
            run_stream_identity(project.path()),
            PINNED_ID,
            "the fixtures must all resolve to one identity, or their stores are not comparable"
        );
        seed_both_stores(project.path(), ROUNDS);
    }

    let seed_log = raw_rows(&rigger_file(runs_only.path(), "events.db"));
    let seed_graph = graph_rows(&rigger_file(runs_only.path(), "graph.db"));
    assert!(
        !seed_log.is_empty() && !seed_graph.0.is_empty() && !seed_graph.1.is_empty(),
        "the seed must populate BOTH stores, or nothing below proves anything"
    );
    assert!(
        shape(&raw_rows(&rigger_file(derived_only.path(), "events.db"))) == shape(&seed_log),
        "the three fixtures must start from an identical log; they differ at {}",
        first_difference(
            &row_marks(&raw_rows(&rigger_file(derived_only.path(), "events.db"))),
            &row_marks(&seed_log)
        )
    );
    assert_eq!(
        graph_rows(&rigger_file(derived_only.path(), "graph.db")),
        seed_graph,
        "the three fixtures must start from an identical graph"
    );

    // `--runs` PRUNES THE GRAPH AND ONLY THE GRAPH. Its own report promises it deletes no event,
    // so every row keeps its bytes AND its numbering: a row that survived but was renumbered or
    // repositioned is not an untouched log. This project is already under its minted identity, so
    // the identity migration `cmd_reset` runs first is a no-op here and the log really is
    // byte-for-byte identical; the one store class where it is NOT is pinned by
    // reset_runs_alone_migrates_a_legacy_store_and_its_report_says_what_that_wrote.
    let (out, err, ok) = run_rigger(runs_only.path(), &["reset", "--runs"]);
    assert!(ok, "reset --runs must succeed; stderr: {err}\n{out}");
    let after_runs_log = raw_rows(&rigger_file(runs_only.path(), "events.db"));
    assert!(
        after_runs_log == seed_log,
        "reset --runs reports that it deletes no event, so every row must survive \
         byte-for-byte, position and revision included; the log differs at {}, and the command \
         said: {out:?}",
        first_difference(&row_marks(&after_runs_log), &row_marks(&seed_log))
    );
    let after_runs_graph = graph_rows(&rigger_file(runs_only.path(), "graph.db"));
    let dropped: Vec<&String> = seed_graph
        .0
        .iter()
        .filter(|n| !after_runs_graph.0.contains(n))
        .collect();
    assert!(
        dropped.iter().any(|n| n.contains(DEAD_DECISION)),
        "the --runs prune must actually drop the dead run's decision node, or 'the log survived \
         it' is a claim about a mode that did nothing; it dropped {dropped:?}"
    );

    // `--derived` MIGRATES THE LOG AND ONLY THE LOG. The shipped guidance tells an operator it
    // writes nothing to graph.db, so the live graph is unchanged.
    let (out, err, ok) = run_rigger(derived_only.path(), &["reset", "--derived"]);
    assert!(ok, "reset --derived must succeed; stderr: {err}\n{out}");
    assert_eq!(
        out,
        migrated_lines(1, 2 * ROUNDS as usize, 0) + LOG_LEFT_AS_IT_STANDS_LINE
    );
    assert_eq!(
        graph_rows(&rigger_file(derived_only.path(), "graph.db")),
        seed_graph,
        "the shipped guidance says --derived writes nothing to graph.db, so its live content \
         must be identical; the command said: {out:?}"
    );
    let after_derived_log = raw_rows(&rigger_file(derived_only.path(), "events.db"));
    assert_eq!(
        seed_log.len() - after_derived_log.len(),
        2 * ROUNDS as usize - 1,
        "the migration keeps one row of the derived index, rewritten into its entry, and sheds \
         the rest; it said: {out:?}"
    );
    assert_eq!(
        entries(&after_derived_log),
        vec![entry_at(seed_log[seed_log.len() - 2].0, "")],
        "the entry stands at the first row of the latest recording of the batch"
    );

    // COMPOSED: exactly the two effects. The log is what `--derived` alone leaves and the graph is
    // what `--runs` alone leaves - so neither mode sheds more in company, and neither one's read
    // is disturbed by the other's writes.
    let (out, err, ok) = run_rigger(composed.path(), &["reset", "--runs", "--derived"]);
    assert!(
        ok,
        "reset --runs --derived must succeed; stderr: {err}\n{out}"
    );
    let composed_log = raw_rows(&rigger_file(composed.path(), "events.db"));
    assert!(
        shape(&composed_log) == shape(&after_derived_log),
        "the composed reset must leave exactly the log --derived alone leaves; it differs at {}, \
         and the command said: {out:?}",
        first_difference(&row_marks(&composed_log), &row_marks(&after_derived_log))
    );
    assert_eq!(
        graph_rows(&rigger_file(composed.path(), "graph.db")),
        after_runs_graph,
        "the composed reset must leave exactly the graph --runs alone leaves; it said: {out:?}"
    );
}

/// Run `git <args...>` inside the throwaway repo, and fail the test with git's own message if it
/// does not succeed - a silently skipped `commit` would leave `--against HEAD` with no rev to
/// check out, and the test would then be asserting on an error message.
fn git_in(root: &Path, args: &[&str]) {
    let out = run_git(root, args);
    assert!(
        out.status.success(),
        "git {args:?} must succeed in the throwaway repo: {}",
        String::from_utf8_lossy(&out.stderr).trim()
    );
}

// ---------------------------------------------------------------------------------------
// 16. The store the command migrates is the store it walked up to.
//
// `cmd_reset` resolves the store by walking up from the working directory, runs the project
// identity migration against that store, and only then the mode. From a worktree nested inside
// the project the walk reaches the project's store, so everything the command reads - the
// identity, the run stream and the tree each file is read from - is the store's own, never the
// working directory's.
// ---------------------------------------------------------------------------------------

/// The durable identity the fixtures below mint. A fixed string rather than a derivation of the
/// basename it replaces, for the reason spelled out where it is written.
const MINTED_ID: &str = "compaction-fixture-9f2c1a";

/// A project whose event log was written under the LEGACY basename namespace and which only
/// afterwards minted a durable identity - the shape a store that needs migrating actually has.
/// Returns `(legacy identity, minted identity)`.
fn seed_project_under_the_legacy_namespace(root: &Path, rounds: u64) -> (String, String) {
    git_in(root, &["init", "-q"]);
    git_in(root, &["config", "user.email", "fixture@example.invalid"]);
    git_in(root, &["config", "user.name", "fixture"]);
    git_in(root, &["commit", "-q", "--allow-empty", "-m", "seed"]);
    std::fs::create_dir_all(root.join(".rigger")).expect("create .rigger");

    // Seeded BEFORE the mint, so the history is filed under the basename namespace exactly as a
    // pre-identity store's is. A fixture that minted first would prove nothing about the migration.
    let legacy = run_stream_identity(root);
    seed_namespace(&rigger_file(root, "events.db"), &legacy, rounds);

    // The minted id shares NO prefix with the basename it replaces. `proj-<id>-` is a separator-free
    // string prefix, so an id of the form `<legacy>-<suffix>` would leave every migrated stream
    // still matching the LEGACY prefix and the assertions below would be reading that ambiguity
    // rather than whether the history moved.
    let minted = MINTED_ID.to_string();
    std::fs::write(root.join(".rigger").join("project.id"), &minted).expect("mint the identity");
    assert!(
        !minted.starts_with(&legacy) && !legacy.starts_with(&minted),
        "neither identity may be a string prefix of the other, or the namespace assertions below \
         cannot tell a migrated stream from an unmigrated one ({legacy:?} vs {minted:?})"
    );
    assert_ne!(
        legacy,
        run_stream_identity(root),
        "the mint must produce an identity distinct from the basename, or this fixture does not \
         reproduce the shape it exists for"
    );
    (legacy, minted)
}

/// What the project's tree holds at `src/a.rs`, and what the nested worktree holds at the same
/// path: two different contents, so the blob the entry records says which tree was read.
const TREE_BODY: &[u8] = b"pub fn alpha() {}\n";
const WORKTREE_BODY: &[u8] = b"pub fn alpha() { let _nested = 1; }\n";

#[test]
fn a_reset_from_a_nested_worktree_migrates_and_compacts_the_store_it_walked_up_to() {
    const ROUNDS: u64 = 4;

    // Two identically seeded projects. One is reset from its own root, the other from a worktree
    // nested inside it: the SAME store, reached by the SAME walk, from two different working
    // directories. Whatever the command does, it must do the same thing in both.
    let from_root = tempfile::tempdir().expect("create a temp project");
    let from_worktree = tempfile::tempdir().expect("create a temp project");
    let (legacy_a, minted_a) = seed_project_under_the_legacy_namespace(from_root.path(), ROUNDS);
    let (legacy_b, minted_b) =
        seed_project_under_the_legacy_namespace(from_worktree.path(), ROUNDS);
    for root in [from_root.path(), from_worktree.path()] {
        std::fs::create_dir_all(root.join("src")).expect("create src");
        std::fs::write(root.join("src").join("a.rs"), TREE_BODY).expect("write the tree's file");
    }

    let nested = from_worktree.path().join("wt");
    git_in(
        from_worktree.path(),
        &["worktree", "add", "-q", "--detach", "wt"],
    );
    assert!(
        !nested.join(".rigger").exists(),
        "the nested worktree must carry no store of its own, or the walk would stop at a shadow \
         instead of reaching the project's store"
    );
    std::fs::create_dir_all(nested.join("src")).expect("create the worktree's src");
    std::fs::write(nested.join("src").join("a.rs"), WORKTREE_BODY)
        .expect("write the worktree's file");
    let tree_blob = rigger::worktree::hash_blob(from_worktree.path(), TREE_BODY)
        .expect("hash the tree's bytes");
    assert_ne!(
        tree_blob,
        rigger::worktree::hash_blob(&nested, WORKTREE_BODY).expect("hash the worktree's bytes"),
        "the two trees must hold different bytes, or the blob cannot say which was read"
    );

    let before_a = raw_rows(&rigger_file(from_root.path(), "events.db"));
    let before_b = raw_rows(&rigger_file(from_worktree.path(), "events.db"));
    assert_eq!(before_a.len(), 1 + 2 * ROUNDS as usize);
    assert_eq!(before_b.len(), before_a.len());

    let (out_a, err_a, ok_a) = run_rigger(from_root.path(), &["reset", "--derived"]);
    assert!(
        ok_a,
        "reset --derived at the root must succeed: {err_a}\n{out_a}"
    );
    let (out_b, err_b, ok_b) = run_rigger(&nested, &["reset", "--derived"]);
    assert!(
        ok_b,
        "reset --derived from a nested worktree must succeed - the store walk reaches the \
         project's store from there: {err_b}\n{out_b}"
    );

    // THE MIGRATION DID THE WORK, from the worktree exactly as from the root. A report of
    // nothing to shed here is the failure this test exists for: it is what an identity
    // migration anchored at the process's working directory produces.
    for (where_, out) in [("the root", &out_a), ("a nested worktree", &out_b)] {
        assert_eq!(
            derived_report(out),
            migrated_lines(1, 2 * ROUNDS as usize, 0) + LOG_LEFT_AS_IT_STANDS_LINE,
            "reset --derived run from {where_} must migrate the legacy-namespaced history it \
             walked up to; got {out:?}"
        );
    }

    // THE IDENTITY MIGRATION MOVED THE HISTORY rather than leaving it stranded, and THE BLOB IS
    // THE TREE'S: each entry records the object id of the bytes under the tree the store belongs
    // to, which from the nested worktree is not the working directory's own top level.
    for (where_, root, legacy, minted, before) in [
        (
            "the root",
            from_root.path(),
            &legacy_a,
            &minted_a,
            &before_a,
        ),
        (
            "a nested worktree",
            from_worktree.path(),
            &legacy_b,
            &minted_b,
            &before_b,
        ),
    ] {
        let after = raw_rows(&rigger_file(root, "events.db"));
        assert!(
            rows_in(&after, &Namespaced::prefix_for(legacy)).is_empty(),
            "no row may be left behind under the legacy namespace after a reset from {where_}"
        );
        assert_eq!(
            rows_in(&after, &Namespaced::prefix_for(minted)).len(),
            after.len(),
            "every surviving row must live under the minted namespace after a reset from {where_}"
        );
        assert_eq!(
            entries(&after),
            vec![entry_at(before[before.len() - 2].0, &tree_blob)],
            "the entry a reset from {where_} writes stands at the first row of the latest \
             recording and records the blob of the file under the store's own tree"
        );
    }

    // The two invocations are the SAME operation: one store, one authority, two cwds.
    assert_eq!(
        shape(&raw_rows(&rigger_file(from_root.path(), "events.db")))
            .iter()
            .map(|r| (r.0, r.2.clone(), r.5))
            .collect::<Vec<_>>(),
        shape(&raw_rows(&rigger_file(from_worktree.path(), "events.db")))
            .iter()
            .map(|r| (r.0, r.2.clone(), r.5))
            .collect::<Vec<_>>(),
        "a reset from a nested worktree must leave the log in the state a reset from the root \
         leaves it in"
    );
}

///
/// The migration is wired into `cmd_reset` for every sqlite invocation rather than into
/// `--derived` alone, and it must be: `reset_runs` reads through the project's MINTED namespace,
/// so on a store still filed under the legacy basename it would read an empty stream and report a
/// confident prune of zero dead-run nodes. But that makes `--runs` a command that writes the
/// event log on exactly the old-store class it is most likely to be pointed at, and the report
/// used to end "the event log is untouched". This pins the corrected claim against the store the
/// claim is about, both halves at once: NOTHING IS DELETED (every seeded row survives with its
/// position, type, id, payload bytes and revision intact, only its stream re-namespaced) and
/// EXACTLY ONE EVENT IS APPENDED (the migration's own `DecisionMade`) - and the printed line says
/// both rather than promising an untouched file.
#[test]
fn reset_runs_alone_migrates_a_legacy_store_and_its_report_says_what_that_wrote() {
    const ROUNDS: u64 = 3;
    let project = tempfile::tempdir().expect("create a temp project");
    let (legacy, minted) = seed_project_under_the_legacy_namespace(project.path(), ROUNDS);
    let legacy_ns = Namespaced::prefix_for(&legacy);
    let minted_ns = Namespaced::prefix_for(&minted);

    let before = raw_rows(&rigger_file(project.path(), "events.db"));
    assert!(
        !rows_in(&before, &legacy_ns).is_empty() && rows_in(&before, &minted_ns).is_empty(),
        "the premise: this store's whole history is under the LEGACY namespace, which is the only \
         shape on which reset writes the log at all"
    );

    let (out, err, ok) = run_rigger(project.path(), &["reset", "--runs"]);
    assert!(ok, "reset --runs must succeed; stderr: {err}\n{out}");

    // NOTHING WAS DELETED, and nothing was renumbered or re-dated: each seeded row is still there,
    // in order, with only its stream moved into the minted namespace. That is the half of the
    // claim an operator cannot check afterwards, so it is checked here column by column.
    let after = raw_rows(&rigger_file(project.path(), "events.db"));
    let carried: Vec<Row> = before
        .iter()
        .map(|r| {
            let suffix =
                r.1.strip_prefix(&legacy_ns)
                    .expect("every seeded row is under the legacy namespace");
            (
                r.0,
                format!("{minted_ns}{suffix}"),
                r.2.clone(),
                r.3.clone(),
                r.4.clone(),
                r.5.clone(),
                r.6,
            )
        })
        .collect();
    assert!(
        after.starts_with(&carried),
        "reset --runs deletes no event: every row must survive with its position, type, id, \
         payload and revision, renamed into the minted namespace and nothing more. It differs at \
         {}; the command said: {out:?}",
        first_difference(&row_marks(&after), &row_marks(&carried))
    );

    // AND EXACTLY ONE EVENT WAS APPENDED: the migration's record of itself, in the minted
    // namespace. One, not zero (the migration really did fire on this store) and not two (nothing
    // else about `--runs` writes the log).
    let appended = &after[carried.len()..];
    assert_eq!(
        appended.len(),
        1,
        "the migration records itself with ONE event and the graph prune writes none, so a \
         legacy-store `reset --runs` appends exactly one row; it appended {:?}",
        appended.iter().map(|r| (&r.1, &r.2)).collect::<Vec<_>>()
    );
    assert_eq!(
        appended[0].2,
        rigger::contextgraph::TYPE_DECISION_MADE,
        "and it is the existing DecisionMade the migration is recorded with - no event type is \
         minted for it; got {:?}",
        appended[0].2
    );
    assert!(
        appended[0].1.starts_with(&minted_ns),
        "recorded under the identity the history was migrated TO, or the audit trail of the \
         migration is filed where the migration moved the history away from; got {:?}",
        appended[0].1
    );
    assert!(
        rows_in(&after, &legacy_ns).is_empty(),
        "and the legacy namespace is empty afterwards, or the migration did not complete"
    );

    // THE REPORT SAYS SO. The old sentence promised an untouched event log two lines under a
    // command that had just renamed every stream in it.
    assert!(
        !out.contains("the event log is untouched"),
        "the report must not promise an untouched log on the one store class where reset writes \
         it; got {out:?}"
    );
    for (fact, needle) in [
        ("say the prune itself deletes nothing", "deletes no event"),
        ("name the one thing reset does write", "identity migration"),
        ("name what that migration records", "DecisionMade"),
        (
            "tell the operator how to see it happen",
            "prints its own line",
        ),
    ] {
        assert!(
            out.contains(needle),
            "the reset --runs report must {fact} ({needle:?}); got {out:?}"
        );
    }
}

// ---------------------------------------------------------------------------------------
// 23. THE NUMBER: what the command says it reclaimed, against what the file actually lost.
//
// The page-count delta is the LOGICAL size the database shrank by; it becomes bytes on disk only
// once the truncating checkpoint folds the write-ahead log back into the file, and a report that
// skipped that step would print a number the operator's own `ls` contradicts. So the number is
// held to two things at once: it equals the bytes the log lost on disk, and the file on disk
// really is the size its pages say afterwards.
// ---------------------------------------------------------------------------------------

#[test]
fn the_reclamation_the_command_reports_is_the_space_the_file_actually_lost() {
    let dir = temp_rigger_project();
    let root = dir.path();
    seed_project(root, 4);
    let db = rigger_file(root, "events.db");
    // ROOM TO RECLAIM. The seeded duplication is a few small rows, which can free no whole page
    // at all - a run that honestly reports zero would leave this test asserting nothing. Planted
    // free pages make the reclamation a definite figure without changing what it means: the
    // vacuum reclaims the pages the file is not using, however they came to be free.
    plant_free_pages(&db, 3_000);

    let page_size = pragma_i64(&db, "page_size");
    let pages_before = pragma_i64(&db, "page_count");
    let wal = db.with_extension("db-wal");
    // WHAT AN OPERATOR MEASURES: the bytes the log occupies on disk before the command runs -
    // the main file plus whatever its write-ahead log is holding. This is deliberately NOT the
    // page-count arithmetic the implementation could do internally: a page count is the LOGICAL
    // size of the database, it includes pages that live only in an un-checkpointed `-wal`, and a
    // report computed from it can name a reclamation while the file on disk grew.
    let on_disk_before = file_len(&db) + file_len(&wal);

    let (out, err, ok) = run_rigger(root, &["reset", "--derived"]);
    assert!(
        ok,
        "the migration must succeed; stdout {out:?}, stderr {err:?}"
    );

    let pages_after = pragma_i64(&db, "page_count");
    let on_disk_after = file_len(&db) + file_len(&wal);
    let reclaimed = reported_reclaimed_bytes(&out, "reclaimed ")
        .unwrap_or_else(|| panic!("the report must carry a measured reclamation; got {out:?}"));
    assert_eq!(
        out,
        migrated_lines(1, 8, 0) + &reclaimed_line(reclaimed),
        "the migration's counts, then the one line of the measured reclamation"
    );
    assert!(
        pages_after < pages_before,
        "the compaction must actually shrink the log, or the figure below is a claim about \
         nothing: {pages_before} page(s) before, {pages_after} after. Report was {out:?}"
    );
    assert_eq!(
        reclaimed,
        on_disk_before - on_disk_after,
        "the reported reclamation must be the bytes the log actually lost on disk - \
         {on_disk_before} before the command and {on_disk_after} after it, main file plus \
         write-ahead log. A number that is not this one is a claim an operator can disprove with \
         `du`. Report was {out:?}"
    );

    // AND IT LANDED IN THE MAIN FILE. The number above is a delta over main-plus-`-wal`; this is
    // what says the freed space really left the pair rather than moving between them, which it
    // does only because the truncating checkpoint folded the write-ahead log back into the file.
    // If the frames were still in the `-wal`, the file would not be the size the pages say it is
    // - the case the command reports as unmeasured instead.
    assert_eq!(
        file_len(&db),
        pages_after as u64 * page_size as u64,
        "the log on disk must be exactly the pages it now holds, or the reclamation was reported \
         as landed while the freed frames were still in the write-ahead log"
    );
    assert_eq!(
        file_len(&wal),
        0,
        "and nothing may be left in the write-ahead log the reclamation already counted as \
         reclaimed"
    );
}

/// Run again on the migrated log there is nothing to shed, and the log is left as it stands.
#[test]
fn a_second_reset_of_a_migrated_log_sheds_nothing_and_leaves_the_file_as_it_stands() {
    let dir = temp_rigger_project();
    let root = dir.path();
    seed_project(root, 4);
    let (first, err, ok) = run_rigger(root, &["reset", "--derived"]);
    assert!(ok, "the migration must succeed; stderr: {err}\n{first}");
    let migrated = raw_rows(&rigger_file(root, "events.db"));

    let (again, err, ok) = run_rigger(root, &["reset", "--derived"]);
    assert!(ok, "the second run must succeed; stderr: {err}\n{again}");
    assert_eq!(
        again,
        NOTHING_TO_SHED_LINE.to_string() + LOG_LEFT_AS_IT_STANDS_LINE
    );
    assert_eq!(raw_rows(&rigger_file(root, "events.db")), migrated);
}
