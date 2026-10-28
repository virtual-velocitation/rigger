//! PERIPHERY (contract / API / CLI) tests for the generation rule a derived batch folds under, at
//! the seams between the event log, the `graph.db` folded from it and the commands that rebuild or
//! reset either:
//!
//!   1. **The `ContentIdentity::with_key_parts` / `key_parts` contract.** A policy answers a key's
//!      batch identity and generation only through the parser it declared.
//!   2. **The fold's generation rule at its seams.** `ingest::derived_generation` names a
//!      generation only for a keyed derived-index event, and one project's generations never
//!      supersede another's in a shared file.
//!   3. **A `graph.db` folded before the generation rule.** It owes a rebuild from the log, which
//!      `rigger setup` pays once; until then the fold-dependent commands refuse, `rigger reset
//!      --derived` among them, and the read-only surfaces say the rebuild is owed - and `rigger
//!      validate` alone, of a `graph.db` that passed events over, how many by cause.
//!   4. **The shadow rebuild's seams.** Its sources (the live selection among them), its tail, an
//!      emit that meets it, a fold that is lost, the prune it makes, what it passes over and the
//!      counts read back by cause, `graph.db.lock`, and a run live beside it: `rigger setup`
//!      refuses a new rebuild - one whose pay starts a fresh shadow and folds the log from its
//!      start: a cause owed beside no shadow, a tail standing or not, or a standing shadow that is
//!      not resumable, an older-rule binary's among them - while a `rigger step` holds
//!      `.rigger/step.lock`, an unanswered spawn's liveness marker on the log setup selects is
//!      inside its bound or a driver registration of the store heartbeats inside the idle window,
//!      leaving the rebuild owed and its standing state as it stood, and beside a held step lock
//!      resumes a resumable standing shadow - one folded under the current rule with the run
//!      attribution it gathered - and finishes a tail no cause is owed beside.
//!   5. **The catch-up cursor's seeding.** `rigger setup` creates the missing `graph.db` beside an
//!      absent event log with its `caught_up` cursor at 0, reading the absent log as an empty one
//!      and never creating it, and gives a file folded without one its cursor, seeded or set by the
//!      rebuild the file owes: a hole below an applied event is young or old by the store stamp the
//!      seeding reads for it against the catch-up horizon; a `graph.db` removed beside a non-empty
//!      log owes the rebuild of a lost fold, as does the log, removed or cut below the highest
//!      position the file applied, beside a file without a cursor at the current fold rule; a file
//!      that folded the log at an older fold rule owes that rule's rebuild, and the lost fold beside
//!      it only for an old hole, never for the removed log; and a seeding that meets another
//!      writer's write lock is skipped, setup exiting zero, and the next setup starts it.
//!   6. **The catch-up from the cursor.** `rigger setup` folds a file carrying its cursor forward
//!      from it to the log's head, one page of the log per call, and says how many events it folded:
//!      over a young hole below an applied event, over more events than a page holds and over what
//!      another writer of a server log appended. It rebuilds a file whose cursor stands above the
//!      head of a log replaced by a shorter one, and fails, naming the position its catch-up
//!      stopped at, when a fold meets a storage error, the next setup folding on once the storage
//!      recovers.

mod common;

use common::cli::append_run_events;
use common::cli::applied;
use common::cli::applied_each;
use common::cli::assert_stopped_at_the_grounder;
use common::cli::backdate_the_log;
use common::cli::backdate_the_log_by;
use common::cli::decision_lines;
use common::cli::escalate_solo_unit;
use common::cli::exists;
use common::cli::fold_state;
use common::cli::graph_files;
use common::cli::graph_identity;
use common::cli::hold_step_lock;
use common::cli::hold_the_rebuild;
use common::cli::idle_window_secs;
use common::cli::insert_node;
use common::cli::keyed;
use common::cli::log_and_graph_files;
use common::cli::migrated_lines;
use common::cli::named_marker_age;
use common::cli::no_progress;
use common::cli::one_unanswered_spawn;
use common::cli::read_run_events;
use common::cli::refusal_line;
use common::cli::refused_derived_reset;
use common::cli::remove_the_store_file;
use common::cli::rigger_command;
use common::cli::rigger_db;
use common::cli::rigger_file;
use common::cli::run_log;
use common::cli::run_log_head;
use common::cli::run_rigger;
use common::cli::run_rigger_envs;
use common::cli::run_stream_identity;
use common::cli::seed_owned;
use common::cli::stopping_at_the_grounder;
use common::cli::temp_store_project;
use common::cli::write_workflow_fixture;
use common::cli::DriverTraces;
use common::cli::REVIEWLESS_GIT_ESCALATING_UNIT_WORKFLOW;
use common::cli::SPAWN;
use common::cli::{pruned_from_the_rebuilt_graph, rebuilt_through_the_head};
use common::cli::{rebuild_owed, setup_ok, user_version, version_and_owed};
use common::cli::{rebuilt_for, rebuilt_for_a_lost_fold};
use common::cli::{BOTH_CAUSES_REBUILD_LINE, OLDER_RULE_REBUILD_LINE};
use common::cli::{EMIT_FOLDED_ENDING, LOST_FOLD_REBUILD_LINE, REBUILT_LINE, SEEDING_SKIPPED_LINE};
use common::cli::{LOG_LEFT_AS_IT_STANDS_LINE, NOTHING_TO_SHED_LINE};
use common::fixtures::caught_up_at;
use common::fixtures::caught_up_line;
use common::fixtures::cleanup;
use common::fixtures::decision_json;
use common::fixtures::drop_the_cursor;
use common::fixtures::f_rs_batch_def;
use common::fixtures::files_open_by;
use common::fixtures::fresh_test_reference;
use common::fixtures::nanos;
use common::fixtures::owned_ids;
use common::fixtures::past_the_horizon;
use common::fixtures::rebuild_owed_note;
use common::fixtures::wait_until_for;
use common::fixtures::with_graph_locked;
use common::fixtures::DISK_GAVE_OUT;
use common::fixtures::STEP_LOCK_REASON;
use common::fixtures::{catch_up_failed_line, the_disk_gives_out_at_d_boom, the_disk_recovers};
use common::fixtures::{graph_error, lost_fold, not_folded, DATABASE_IS_LOCKED, NOT_A_DATABASE};
use common::fixtures::{source_interrupted, SOURCE_INTERRUPTED};
use common::git::temp_git_project_with_commit;
use rigger::contextgraph::sqlite::{
    PruneStats, RebuildSink, Rebuilt, CATCH_UP_HORIZON, FOLD_BATCH, PROJECTION_VERSION,
};
use rigger::contextgraph::Fold;
use rigger::contextgraph::{
    TYPE_CODE_ENTITY_EXTRACTED, TYPE_DOC_CONCEPT_EXTRACTED, TYPE_DOC_LINK_EXTRACTED,
    TYPE_EDGE_INFERRED,
};
use rigger::eventstore::namespace::Namespaced;
use rigger::eventstore::sqlite::Store;
use rigger::eventstore::{ContentIdentity, Event, EventStore, ExpectedRevision};
use std::path::Path;

// ---------------------------------------------------------------------------------------
// Harness
// ---------------------------------------------------------------------------------------

const PROJECT: &str = "proj-gen";

/// A `CodeEntityExtracted` payload naming `name` at `line`.
fn entity(name: &str, line: u32) -> Vec<u8> {
    serde_json::to_vec(&serde_json::json!({
        "file": "src/f.rs", "name": name, "kind": "function", "line": line, "lang": "rust",
    }))
    .unwrap()
}

/// A `DocLinkExtracted` payload: one `SPECIFIES` design fact from `docs/f.md` to `to`.
fn link(to: &str) -> Vec<u8> {
    serde_json::to_vec(&serde_json::json!({
        "from": "docs/f.md", "to": to, "rel": rigger::contextgraph::REL_SPECIFIES,
    }))
    .unwrap()
}

/// A file-backed store in `dir` with `events` appended, in order, to `stream` of [`PROJECT`].
fn store_with(dir: &Path, batches: &[(&str, Vec<Event>)]) -> (Store, String) {
    let db = dir.join("events.db");
    let backend = Store::open(db.to_str().unwrap()).unwrap();
    {
        // The batches hold derived events, which a store refuses: they land as the rows of a
        // store recorded before the ledger.
        let pre_ledger = common::fixtures::PreLedgerStore {
            db: &db,
            inner: &backend,
        };
        let store = Namespaced::new(&pre_ledger, PROJECT);
        for (stream, events) in batches {
            store.append(stream, ExpectedRevision::Any, events).unwrap();
        }
    }
    (backend, db.to_str().unwrap().to_string())
}

/// The shipped policy's meta key, covered types and valid-time partition, with NO key parser.
fn identity_without_key_parts() -> ContentIdentity {
    let shipped = rigger::ingest::derived_index_identity();
    ContentIdentity::new(shipped.meta_key().to_string(), shipped.types().to_vec())
        .with_reasserting_types(shipped.reasserting().unwrap().to_vec())
}

/// `project`'s run stream in `store`, as recorded (positions included).
fn run_events(store: &Store, project: &str) -> Vec<Event> {
    Namespaced::new(store, project)
        .read_stream(
            rigger::conductor::STREAM,
            0,
            rigger::eventstore::Direction::Forward,
        )
        .unwrap()
}

/// Fold `batches` into `graph_db` for `project`, REOPENING the projector for every batch (the way
/// successive runs fold one live `graph.db`), and return the whole live projection's wire form.
fn fold_in_batches(graph_db: &Path, project: &str, batches: &[Vec<Event>]) -> String {
    use rigger::contextgraph::sqlite::Projector;

    let open = || Projector::open(graph_db.to_str().unwrap(), project).unwrap();
    for batch in batches {
        common::fixtures::folds(&open(), batch);
    }
    common::fixtures::whole_json(&open())
}

/// Events folded into a graph AFTER the state under test is reached, so a divergence latent in its
/// fold state surfaces: a later file defining every name the fixtures drop, and a decision naming
/// the dropped entity, which brings a retired node - and whatever detached from it - back.
fn later_events() -> Vec<Event> {
    let mut events: Vec<Event> = ["gone", "alpha", "mid"]
        .into_iter()
        .enumerate()
        .map(|(i, name)| {
            keyed(
                TYPE_CODE_ENTITY_EXTRACTED,
                def_in("src/late.rs", name, 1, i == 0),
                &format!("gc/src/late.rs@z#{i}"),
                90,
            )
        })
        .collect();
    events.push(
        Event::new(
            "DecisionMade",
            serde_json::to_vec(&serde_json::json!({
                "id": "d-late", "summary": "s", "governs": ["src/f.rs::gone"], "supersedes": "",
            }))
            .unwrap(),
        )
        .with_valid_from(std::time::UNIX_EPOCH + std::time::Duration::from_secs(91)),
    );
    events
        .into_iter()
        .zip(1_000_000u64..)
        .map(|(mut e, position)| {
            e.position = position;
            e
        })
        .collect()
}

/// The [`graph_identity`] of `graph_db` under [`PROJECT`].
fn identity_of(graph_db: &Path) -> (String, Vec<String>) {
    graph_identity(graph_db, PROJECT)
}

/// A `CodeEntityExtracted` payload defining `name` at `line` in `file`, heading its batch when
/// `fresh`.
fn def_in(file: &str, name: &str, line: u32, fresh: bool) -> Vec<u8> {
    serde_json::to_vec(&serde_json::json!({
        "file": file, "name": name, "kind": "function", "line": line, "lang": "rust",
        "fresh": fresh,
    }))
    .unwrap()
}

/// The live `(from, to, rel, tier)` of every edge `graph` holds into `to`.
fn tiers_into<'g>(
    graph: &'g rigger::contextgraph::Graph,
    to: &str,
) -> Vec<(&'g str, &'g str, &'g str)> {
    graph
        .edges
        .iter()
        .filter(|e| e.to == to)
        .map(|e| (e.from.as_str(), e.rel.as_str(), e.tier.as_str()))
        .collect()
}

/// A graph-derived attachment payload: `node` in `community` (`CommunityAssigned`), heading the
/// pass.
fn community_of(node: &str) -> Vec<u8> {
    serde_json::to_vec(&serde_json::json!({
        "node": node, "community": "community/1/c0", "resolution": 1.0, "hash": "h",
        "fresh": true,
    }))
    .unwrap()
}

// ---------------------------------------------------------------------------------------
// 1. The key-parser contract
// ---------------------------------------------------------------------------------------

#[test]
fn key_parts_answers_only_through_the_parser_the_policy_declared() {
    let shipped = rigger::ingest::derived_index_identity();
    assert_eq!(
        shipped.key_parts("gc/src/a.rs@h1#0"),
        Some(("gc/src/a.rs", "h1")),
        "the shipped policy cuts a derived key into its batch identity and generation"
    );
    assert_eq!(
        shipped.key_parts("gc/src/a.rs@h1"),
        None,
        "a key the shipped parser rejects has no parts"
    );
    assert_eq!(
        identity_without_key_parts().key_parts("gc/src/a.rs@h1#0"),
        None,
        "a policy that declared no parser answers None even for a well-formed derived key"
    );
}

// ---------------------------------------------------------------------------------------
// 2. The fold's generation rule at its seams
// ---------------------------------------------------------------------------------------

/// A `(batch identity, generation)` pair as the key parser answers it.
type Parts<'k> = (&'k str, &'k str);

/// `ingest::derived_generation` is the one place the fold learns who asserted a fact: the
/// `(<prefix>/<file>, generation)` of a derived-index event's replay key, and nothing for any other
/// event, for a derived event with no key, or for a key that is not the derived shape.
#[test]
fn derived_generation_names_only_a_keyed_derived_events_identity_and_generation() {
    let cases: Vec<(Event, Option<Parts>, &str)> = vec![
        (
            keyed(
                TYPE_CODE_ENTITY_EXTRACTED,
                entity("a", 1),
                "gc/src/f.rs@h1#0",
                1,
            ),
            Some(("gc/src/f.rs", "h1")),
            "a code entity",
        ),
        (
            keyed(TYPE_EDGE_INFERRED, entity("a", 1), "gc/src/f.rs@h2#3", 1),
            Some(("gc/src/f.rs", "h2")),
            "an inferred edge",
        ),
        (
            keyed(
                TYPE_DOC_CONCEPT_EXTRACTED,
                link("x"),
                "gd/docs/f.md@h3#0",
                1,
            ),
            Some(("gd/docs/f.md", "h3")),
            "a doc concept",
        ),
        (
            keyed(
                TYPE_DOC_LINK_EXTRACTED,
                link("x"),
                "gd/docs/a@b#c.md@h4#12",
                1,
            ),
            Some(("gd/docs/a@b#c.md", "h4")),
            "a doc link whose file holds `@` and `#`",
        ),
        (
            keyed("DecisionMade", link("x"), "gc/src/f.rs@h1#0", 1),
            None,
            "a non-derived event carrying a derived-shaped key",
        ),
        (
            Event::new(TYPE_CODE_ENTITY_EXTRACTED, entity("a", 1)),
            None,
            "a derived event with no replay key",
        ),
        (
            keyed(
                TYPE_CODE_ENTITY_EXTRACTED,
                entity("a", 1),
                "gc/src/f.rs@h1",
                1,
            ),
            None,
            "a derived event whose key has no `#<index>` tail",
        ),
    ];
    for (event, expected, what) in &cases {
        assert_eq!(
            rigger::ingest::derived_generation(event),
            *expected,
            "derived_generation of {what}"
        );
    }
}

/// Two projects share one `graph.db`: each one's generations supersede only its own. Folded
/// interleaved - A's h1, B's h1, B's h9, A's h2 - each project's projection is exactly the one it
/// folds alone, so A's h2 retires A's `gone` and B's h9 never touched A.
#[test]
fn one_projects_generations_never_supersede_anothers_in_a_shared_graph() {
    const OTHER: &str = "proj-gen-other";
    let dir = tempfile::tempdir().unwrap();
    let (backend, events_db) = store_with(
        dir.path(),
        &[(
            rigger::conductor::STREAM,
            vec![
                f_rs_batch_def(0, "h1", 0, ("alpha", "function", 1), None, 10),
                keyed(
                    TYPE_CODE_ENTITY_EXTRACTED,
                    entity("gone", 9),
                    "gc/src/f.rs@h1#1",
                    10,
                ),
                f_rs_batch_def(0, "h2", 0, ("alpha", "function", 2), None, 20),
            ],
        )],
    );
    let pre_ledger = common::fixtures::PreLedgerStore {
        db: Path::new(&events_db),
        inner: &backend,
    };
    Namespaced::new(&pre_ledger, OTHER)
        .append(
            rigger::conductor::STREAM,
            ExpectedRevision::Any,
            &[
                f_rs_batch_def(0, "h1", 0, ("beta", "function", 1), None, 11),
                keyed(
                    TYPE_CODE_ENTITY_EXTRACTED,
                    entity("kept", 3),
                    "gc/src/f.rs@h9#1",
                    12,
                ),
                f_rs_batch_def(0, "h9", 0, ("beta", "function", 2), None, 12),
            ],
        )
        .unwrap();
    let a = run_events(&backend, PROJECT);
    let b = run_events(&backend, OTHER);
    let a_alone = fold_in_batches(&dir.path().join("a.db"), PROJECT, std::slice::from_ref(&a));
    let b_alone = fold_in_batches(&dir.path().join("b.db"), OTHER, std::slice::from_ref(&b));

    let shared = dir.path().join("shared.db");
    fold_in_batches(&shared, PROJECT, &[a[..2].to_vec()]);
    fold_in_batches(&shared, OTHER, &[b[..1].to_vec(), b[1..].to_vec()]);
    let a_shared = fold_in_batches(&shared, PROJECT, &[a[2..].to_vec()]);
    let b_shared = fold_in_batches(&shared, OTHER, &[]);

    assert_eq!(a_shared, a_alone, "A's projection is the one A folds alone");
    assert_eq!(b_shared, b_alone, "B's projection is the one B folds alone");
    let ids = |json: &str| -> Vec<String> {
        let graph: rigger::contextgraph::Graph = serde_json::from_str(json).unwrap();
        graph.nodes.into_iter().map(|n| n.id).collect()
    };
    assert_eq!(
        ids(&a_alone),
        vec!["src/f.rs", "src/f.rs::alpha"],
        "A's h2 retired the `gone` its h1 defined"
    );
    assert_eq!(
        ids(&b_alone),
        vec!["src/f.rs", "src/f.rs::beta", "src/f.rs::kept"],
        "B's h9 holds `beta` and `kept`"
    );
}

// ---------------------------------------------------------------------------------------
// 3. A graph.db folded before the generation rule is rebuilt once, by `rigger setup`
// ---------------------------------------------------------------------------------------

/// `src/f.rs` drops `gone` and `docs/f.md` drops its link to `src/old.rs` between h1 and h2: the
/// facts a graph folded before the generation rule keeps and a rebuild does not.
fn two_generations_dropping_facts() -> Vec<Event> {
    vec![
        f_rs_batch_def(0, "h1", 0, ("alpha", "function", 1), None, 10),
        keyed(
            TYPE_CODE_ENTITY_EXTRACTED,
            entity("gone", 9),
            "gc/src/f.rs@h1#1",
            10,
        ),
        keyed(
            TYPE_DOC_LINK_EXTRACTED,
            link("src/old.rs"),
            "gd/docs/f.md@h1#0",
            10,
        ),
        f_rs_batch_def(0, "h2", 0, ("alpha", "function", 2), None, 20),
        keyed(
            TYPE_DOC_LINK_EXTRACTED,
            link("src/a.rs"),
            "gd/docs/f.md@h2#0",
            20,
        ),
    ]
}

/// A `graph.db` at `db` holding `events` folded and then put in the shape a file folded before the
/// generation rule has: `ledgers` (SQL) rewrites its generation ledgers and its projection version
/// is cleared.
fn a_pre_rule_graph_db(db: &Path, events: &[Event], ledgers: &str) {
    use rigger::contextgraph::sqlite::Projector;

    common::fixtures::folds(
        &Projector::open(db.to_str().unwrap(), PROJECT).unwrap(),
        events,
    );
    let conn = rusqlite::Connection::open(db).unwrap();
    conn.execute_batch(&format!(
        "DROP VIEW live_node_assertions; DROP VIEW live_edge_assertions;
         DROP TABLE generations; DROP TABLE node_assertions; DROP TABLE edge_assertions;
         DROP TABLE retired_nodes; DROP TABLE detached_attachments; DROP TABLE relabel_owed;
         {ledgers}
         PRAGMA user_version = 0;"
    ))
    .unwrap();
}

/// A rebuild source over `log`: every call records the position it was asked to read after, and
/// hands the events past it in batches of `batch`, each with the log's last position.
fn source_over<'a>(
    log: &'a [Event],
    batch: usize,
    afters: &'a std::cell::RefCell<Vec<u64>>,
) -> impl FnMut(u64, &mut RebuildSink) -> Result<(), rigger::contextgraph::Error> + 'a {
    move |after, sink| {
        afters.borrow_mut().push(after);
        rigger::contextgraph::sqlite::stream_past(log, after, batch, sink)
    }
}

/// A rebuild source over `backend`'s live selection of `project`'s run stream, in batches of
/// `batch` - the source `rigger setup` hands a rebuild.
fn live_selection_of<'a>(
    backend: &'a Store,
    project: &'a str,
    batch: usize,
) -> impl FnMut(u64, &mut RebuildSink) -> Result<(), rigger::contextgraph::Error> + 'a {
    let identity = rigger::ingest::derived_index_identity();
    move |after, sink| {
        backend
            .read_live_selection(
                &Namespaced::prefix_for(project),
                rigger::conductor::STREAM,
                &identity,
                after,
                batch,
                &mut |events, head| {
                    sink(events, head).map_err(|e| rigger::eventstore::Error::Backend(e.0))
                },
            )
            .map_err(|e| rigger::contextgraph::Error(e.to_string()))
    }
}

/// Rebuild the `graph.db` at `db` from `source`, reporting nothing: whether there was one to pay,
/// and what it did ([`rebuild_under`] the test project, owing no cause).
fn rebuild(
    db: &Path,
    source: &mut rigger::contextgraph::sqlite::RebuildSource,
) -> Result<Option<Rebuilt>, rigger::contextgraph::Error> {
    rebuild_under(db, PROJECT, false, source)
}

/// Rebuild the `graph.db` at `db`, folded under `project`, from `source`, owed when `owed` says
/// so, reporting nothing: whether there was one to pay, and what it did.
fn rebuild_under(
    db: &Path,
    project: &str,
    owed: bool,
    source: &mut rigger::contextgraph::sqlite::RebuildSource,
) -> Result<Option<Rebuilt>, rigger::contextgraph::Error> {
    use rigger::contextgraph::sqlite::Projector;
    Projector::open(db.to_str().unwrap(), project)
        .unwrap()
        .rebuild(
            &Projector::lock_rebuild(db.to_str().unwrap()).unwrap(),
            owed,
            source,
            &mut |_| Ok(None),
            &mut no_progress,
        )
}

/// A `graph.db` folded before the generation rule - with no ledgers at all, or with them in an
/// older shape - owes one rebuild: nothing folds into it until then, the rebuild reaches exactly
/// what a fresh fold of the log reaches, reporting each batch it folds, and it happens once - a
/// paid rebuild never reads the log again.
fn a_pre_rule_graph_db_is_rebuilt_from_the_log_once(ledgers: &str) {
    use rigger::contextgraph::sqlite::{Projector, RebuildProgress};
    use rigger::contextgraph::Projection;
    let dir = tempfile::tempdir().unwrap();
    let (backend, _) = store_with(
        dir.path(),
        &[(rigger::conductor::STREAM, two_generations_dropping_facts())],
    );
    let log = run_events(&backend, PROJECT);
    let (old_db, fresh_db) = (dir.path().join("old.db"), dir.path().join("fresh.db"));
    a_pre_rule_graph_db(&old_db, &log[..3], ledgers);

    let graph = Projector::open(old_db.to_str().unwrap(), PROJECT).unwrap();
    assert!(
        graph.rebuild_owed().unwrap(),
        "a pre-rule graph.db owes a rebuild"
    );
    assert_eq!(
        common::fixtures::fold_of(&graph, &log[3..]),
        common::fixtures::owed_refusal(),
        "nothing folds incrementally into a pre-rule graph.db"
    );
    drop(graph);
    let afters = std::cell::RefCell::new(Vec::new());
    let mut reported = Vec::new();
    let ran = Projector::open(old_db.to_str().unwrap(), PROJECT)
        .unwrap()
        .rebuild(
            &Projector::lock_rebuild(old_db.to_str().unwrap()).unwrap(),
            false,
            &mut source_over(&log, 2, &afters),
            &mut |_| Ok(None),
            &mut |at| reported.push(at),
        )
        .unwrap();
    assert_eq!(ran, Some(Rebuilt::default()), "the owed rebuild runs");
    let (p, head) = (|i: usize| log[i].position, log[4].position);
    let at = |through, folded| RebuildProgress {
        start: 0,
        through,
        head,
        folded,
    };
    assert_eq!(
        reported,
        vec![at(p(1), 2), at(p(3), 4), at(p(4), 5)],
        "the rebuild reports each batch it folds"
    );
    assert_eq!(
        *afters.borrow(),
        vec![0, p(4)],
        "the log is read whole into the shadow, then past it for the tail"
    );
    assert_eq!(
        (
            dir.path().join("old.db.rebuild").exists(),
            dir.path().join("old.db.rebuild-journal").exists()
        ),
        (false, false),
        "the shadow and its journal are removed once copied in"
    );

    fold_in_batches(&fresh_db, PROJECT, std::slice::from_ref(&log));
    assert_eq!(
        identity_of(&old_db),
        identity_of(&fresh_db),
        "the rebuilt graph.db is the log's, fold state included"
    );
    let reopened = Projector::open(old_db.to_str().unwrap(), PROJECT).unwrap();
    assert!(
        !reopened.rebuild_owed().unwrap(),
        "the rebuild happens once"
    );
    drop(reopened);
    let again = rebuild(&old_db, &mut |_, _| {
        panic!("a paid rebuild never reads the log")
    })
    .unwrap();
    assert_eq!(again, None, "a paid rebuild does not run again");
    common::fixtures::folds(
        &Projector::open(old_db.to_str().unwrap(), PROJECT).unwrap(),
        &later_events(),
    );
}

/// Given a log with no live selection - a server-backed log, whose live selection is its run stream
/// as it stands - and a `graph.db` that misses one of its events below the last one it applied,
/// when setup seeds the graph's cursor against it - handed no stamp, as setup is for the
/// server-backed log, whose stamps are taken on another host's clock, so the hole reads as old -
/// and pays the rebuild, then the seeding reads the stream's positions alone in one ordered pass,
/// materializing no event, and the rebuild reads each event once: its fold reads the stream, and
/// its tail reads only the event the stream gained meanwhile.
#[test]
fn a_log_with_no_live_selection_is_read_for_positions_alone_and_rebuilt_reading_each_event_once() {
    use common::fixtures::{CountedRead, ReadCountingStore};
    use rigger::contextgraph::sqlite::{stream_positions, stream_source, Projector};
    let dir = tempfile::tempdir().unwrap();
    let (backend, _) = store_with(
        dir.path(),
        &[(rigger::conductor::STREAM, two_generations_dropping_facts())],
    );
    let log = run_events(&backend, PROJECT);
    let n = log.len();
    let graph_db = dir.path().join("graph.db");
    fold_in_batches(
        &graph_db,
        PROJECT,
        &[log[..n - 2].to_vec(), log[n - 1..].to_vec()],
    );
    let late = Event::new(
        rigger::contextgraph::TYPE_DECISION_MADE,
        br#"{"id":"d-late","summary":"s","governs":["src/f.rs"],"supersedes":""}"#.to_vec(),
    );
    let store = Namespaced::new(&backend, PROJECT);
    let counting =
        ReadCountingStore::new(&store).interleaving(1, rigger::conductor::STREAM, vec![late]);
    let run = || rigger::conductor::STREAM.to_string();

    let owed = common::fixtures::owed_once_seeded(
        &Projector::open(graph_db.to_str().unwrap(), PROJECT).unwrap(),
        &mut stream_positions(&counting, rigger::conductor::STREAM, 2),
        &mut |_| Ok(None),
        store.log_head().unwrap(),
    );
    let rebuilt = Projector::open(graph_db.to_str().unwrap(), PROJECT)
        .unwrap()
        .rebuild(
            &Projector::lock_rebuild(graph_db.to_str().unwrap()).unwrap(),
            !owed.is_empty(),
            &mut stream_source(&counting, rigger::conductor::STREAM, 2),
            &mut |_| Ok(None),
            &mut no_progress,
        )
        .unwrap();
    assert_eq!(
        (owed, rebuilt, counting.reads()),
        (
            vec![rigger::contextgraph::OWED_LOST_FOLD],
            Some(Rebuilt::default()),
            vec![
                CountedRead::StreamPositions {
                    stream: run(),
                    handed: n,
                },
                CountedRead::StreamBatched {
                    stream: run(),
                    from: 0,
                    batches: [vec![2; n / 2], vec![1; n % 2]].concat(),
                },
                CountedRead::StreamBatched {
                    stream: run(),
                    from: n as i64,
                    batches: vec![1],
                },
            ]
        ),
        "positions alone, then each event read once across the fold and its tail, no read \
         handing more than the batch bound of two"
    );
    assert_eq!(
        serde_json::to_string(
            &Projector::open(graph_db.to_str().unwrap(), PROJECT)
                .unwrap()
                .whole()
                .unwrap()
        )
        .unwrap(),
        fold_in_batches(
            &dir.path().join("clean.db"),
            PROJECT,
            &[run_events(&backend, PROJECT)]
        ),
        "the rebuilt graph is a fresh fold of the whole log, the late event included"
    );
}

rigger::test_cases! {
    /// Folded before the ledgers existed at all.
    a_graph_db_without_ledgers_is_rebuilt_from_the_log_once:
        a_pre_rule_graph_db_is_rebuilt_from_the_log_once("");
    /// Folded with the ledgers in an older shape.
    a_graph_db_with_older_ledgers_is_rebuilt_from_the_log_once:
        a_pre_rule_graph_db_is_rebuilt_from_the_log_once(
            "CREATE TABLE generations (project TEXT, identity TEXT, generation TEXT, prior TEXT);
             CREATE TABLE edge_assertions (
               project TEXT, identity TEXT, generation TEXT, edge_id INTEGER);",
        );
}

/// A rebuild folds the log one event at a time, as the live fold does: an event whose fold fails
/// (a malformed payload the log holds) is skipped, never failing the rebuild, and leaves the rest
/// of the log folded exactly as without it.
#[test]
fn a_rebuild_skips_an_event_whose_fold_fails() {
    let malformed = Event::new(
        "DecisionMade",
        br#"{"id":"bad","summary":"s","governs":"src/f.rs","supersedes":""}"#.to_vec(),
    );
    let dir = tempfile::tempdir().unwrap();
    let mut events = two_generations_dropping_facts();
    events.insert(3, malformed);
    let (backend, _) = store_with(dir.path(), &[(rigger::conductor::STREAM, events)]);
    let log = run_events(&backend, PROJECT);
    let (rebuilt, clean) = (dir.path().join("rebuilt.db"), dir.path().join("clean.db"));
    a_pre_rule_graph_db(&rebuilt, &log[..1], "");
    let afters = std::cell::RefCell::new(Vec::new());
    let ran = rebuild(&rebuilt, &mut source_over(&log, 10, &afters)).unwrap();
    assert_eq!(
        (
            ran.map(|r| r.pruned),
            common::fixtures::passed_over_rows(rebuilt.to_str().unwrap())
        ),
        (
            Some(PruneStats::default()),
            vec![common::fixtures::rejected_row(&log[3])]
        ),
        "the owed rebuild runs, passing over the malformed event with its cause"
    );
    let without: Vec<Event> = log
        .iter()
        .filter(|e| e.position != log[3].position)
        .cloned()
        .collect();
    fold_in_batches(&clean, PROJECT, &[without]);
    assert_eq!(
        identity_of(&rebuilt),
        identity_of(&clean),
        "the malformed event is skipped and every other event folds"
    );
}

/// An event appended while the rebuild ran - by an emit that found the old file still owing and so
/// did not fold - is on the log when the shadow is swapped in: the rebuild then reads the log past
/// the last position it folded, and only past it, and folds what it gained into the new file, so
/// the rebuilt graph is the whole log's.
#[test]
fn a_rebuild_folds_what_the_log_gained_while_it_ran() {
    let dir = tempfile::tempdir().unwrap();
    let (backend, _) = store_with(
        dir.path(),
        &[(rigger::conductor::STREAM, two_generations_dropping_facts())],
    );
    let log = run_events(&backend, PROJECT);
    let (rebuilt, fresh) = (dir.path().join("rebuilt.db"), dir.path().join("fresh.db"));
    a_pre_rule_graph_db(&rebuilt, &log[..3], "");
    let (afters, before) = (
        std::cell::RefCell::new(Vec::new()),
        std::cell::RefCell::new(Vec::new()),
    );
    let (mut before_run, mut whole) = (
        source_over(&log[..3], 2, &before),
        source_over(&log, 2, &afters),
    );
    let mut reads = 0;
    let ran = rebuild(&rebuilt, &mut |after, sink| {
        reads += 1;
        if reads == 1 {
            before_run(after, sink)
        } else {
            whole(after, sink)
        }
    })
    .unwrap();
    assert_eq!(
        (ran, before.borrow().clone(), afters.borrow().clone()),
        (Some(Rebuilt::default()), vec![0], vec![log[2].position]),
        "the tail is read once, past the last position the shadow folded"
    );
    fold_in_batches(&fresh, PROJECT, std::slice::from_ref(&log));
    assert_eq!(
        identity_of(&rebuilt),
        identity_of(&fresh),
        "the events appended during the rebuild are folded after it"
    );
}

/// A rebuild interrupted part way leaves `graph.db` byte for byte as it was, still owing, with its
/// committed batches in the shadow; the next rebuild resumes past the last of them, folding only
/// what they did not, and reaches exactly the whole log's graph.
#[test]
fn an_interrupted_rebuild_leaves_graph_db_untouched_and_resumes_from_its_last_committed_batch() {
    let dir = tempfile::tempdir().unwrap();
    let (backend, _) = store_with(
        dir.path(),
        &[(rigger::conductor::STREAM, two_generations_dropping_facts())],
    );
    let log = run_events(&backend, PROJECT);
    let (db, fresh) = (dir.path().join("graph.db"), dir.path().join("fresh.db"));
    a_pre_rule_graph_db(&db, &log[..3], "");
    let before = std::fs::read(&db).unwrap();
    let interrupted = rebuild(&db, &mut |_, sink| {
        sink(&log[..2], log[4].position)?;
        source_interrupted()
    });
    assert_eq!(
        interrupted.unwrap_err().0,
        SOURCE_INTERRUPTED,
        "the interruption fails the rebuild"
    );
    assert!(
        std::fs::read(&db).unwrap() == before && rebuild_owed(&db, PROJECT),
        "graph.db is untouched and still owes the rebuild"
    );
    assert!(
        dir.path().join("graph.db.rebuild").exists(),
        "the committed batch is kept in the shadow"
    );

    let (afters, handed) = (
        std::cell::RefCell::new(Vec::new()),
        std::cell::RefCell::new(Vec::new()),
    );
    let mut source = source_over(&log, 2, &afters);
    let ran = rebuild(&db, &mut |after, sink| {
        source(after, &mut |events, head| {
            handed
                .borrow_mut()
                .extend(crate::common::fixtures::positions_of(events));
            sink(events, head)
        })
    })
    .unwrap();
    assert_eq!(
        (ran, afters.borrow().clone(), handed.borrow().clone()),
        (
            Some(Rebuilt::default()),
            vec![log[1].position, log[4].position],
            vec![log[2].position, log[3].position, log[4].position]
        ),
        "the rerun resumes past the committed batch and never folds it again"
    );
    fold_in_batches(&fresh, PROJECT, std::slice::from_ref(&log));
    assert_eq!(
        identity_of(&db),
        identity_of(&fresh),
        "the resumed rebuild is the whole log's"
    );
}

/// A rebuild interrupted after its swap, while folding the tail the log gained, leaves a `graph.db`
/// that no longer owes the rebuild but still owes that tail: the next rebuild folds only the tail,
/// past the last position folded, and the one after it has nothing to do.
#[test]
fn a_rebuild_interrupted_in_its_tail_finishes_the_tail_on_the_next_call() {
    let dir = tempfile::tempdir().unwrap();
    let (backend, _) = store_with(
        dir.path(),
        &[(rigger::conductor::STREAM, two_generations_dropping_facts())],
    );
    let log = run_events(&backend, PROJECT);
    let (db, fresh) = (dir.path().join("graph.db"), dir.path().join("fresh.db"));
    a_pre_rule_graph_db(&db, &log[..3], "");
    let mut reads = 0;
    let interrupted = rebuild(&db, &mut |_, sink| {
        reads += 1;
        match reads {
            1 => sink(&log[..3], log[2].position),
            _ => source_interrupted(),
        }
    });
    assert_eq!(interrupted.unwrap_err().0, SOURCE_INTERRUPTED);
    assert_eq!(
        version_and_owed(&db, PROJECT),
        (PROJECTION_VERSION, false),
        "the rebuilt file was swapped in"
    );

    let afters = std::cell::RefCell::new(Vec::new());
    let ran = rebuild(&db, &mut source_over(&log, 2, &afters)).unwrap();
    assert_eq!(
        (ran, afters.borrow().clone()),
        (Some(Rebuilt::default()), vec![log[2].position]),
        "the next rebuild folds only the tail"
    );
    let again = rebuild(&db, &mut |_, _| panic!("a finished rebuild reads nothing")).unwrap();
    assert_eq!(again, None, "and the one after it has nothing to do");
    fold_in_batches(&fresh, PROJECT, &[log]);
    assert_eq!(identity_of(&db), identity_of(&fresh));
}

/// The last position the rebuild whose cursor the graph file at `db` holds folded, as a connection
/// of its own reads it committed.
fn rebuild_cursor_of(db: &Path) -> u64 {
    rusqlite::Connection::open(db)
        .unwrap()
        .query_row("SELECT position FROM rebuild_cursor", [], |r| r.get(0))
        .unwrap()
}

/// Spec 122, ONE BATCHING RULE and THE SEAM at the rebuild's public surface: given a log that
/// gained, while a rebuild folded its shadow, a tail of more than `FOLD_BATCH` events holding two
/// ledger entries, when that rebuild folds the tail into the swapped-in `graph.db` from two read
/// batches, and another projector folds an event of the second batch between them, then each entry
/// is extracted with every event before it committed and none from it on, the writer's fold lands
/// between the read batches with the cursor at the first one's end, and the `applied` ledger's
/// positions are the log's.
#[test]
fn a_rebuild_tail_commits_before_each_entry_and_a_writer_lands_between_its_read_batches() {
    use common::cli::applied_positions;
    use common::fixtures::{
        entry_at, folds, positions_of, run_started, temp_graph_db, two_read_batches,
    };
    use rigger::contextgraph::sqlite::Projector;
    let (dir, path) = temp_graph_db();
    let db = Path::new(&path);
    let mut events = two_generations_dropping_facts();
    let swapped = events.len();
    events.extend((0..600).map(|i| match i {
        300 => entry_at("gc", "src/tail.rs", "g1", 0, 30),
        450 => entry_at("gc", "src/tail.rs", "g2", 0, 40),
        _ => run_started(&format!("r{i}"), &[]),
    }));
    let (backend, _) = store_with(dir.path(), &[(rigger::conductor::STREAM, events)]);
    let log = run_events(&backend, PROJECT);
    fold_in_batches(db, PROJECT, &[log[..swapped].to_vec()]);
    let at = |i: usize| log[i].position;
    let (split, writer) = (swapped + 500, swapped + 550);
    // The cursor and every position `applied` holds, as a connection of its own reads them.
    let committed = || (rebuild_cursor_of(db), applied_positions(db));
    let (mut reads, mut seam, mut extracted) = (0, Vec::new(), Vec::new());

    let rebuilt = Projector::open(&path, PROJECT)
        .unwrap()
        .rebuild(
            &Projector::lock_rebuild(&path).unwrap(),
            true,
            &mut |after, sink| {
                reads += 1;
                if reads == 1 {
                    return sink(&log[..swapped], at(swapped - 1));
                }
                two_read_batches(&log, split, after, sink, || {
                    folds(
                        &Projector::open(&path, PROJECT).unwrap(),
                        std::slice::from_ref(&log[writer]),
                    );
                    seam.push(committed());
                })
            },
            &mut |e| {
                extracted.push((e.position, committed()));
                Ok(None)
            },
            &mut no_progress,
        )
        .unwrap();

    let before = |i: usize| (at(i - 1), positions_of(&log[..i]));
    assert_eq!(
        (rebuilt, reads),
        (Some(Rebuilt::default()), 2),
        "the shadow folds the log as it stood, then the tail is read once"
    );
    assert_eq!(
        extracted,
        vec![
            (at(swapped + 300), before(swapped + 300)),
            (at(swapped + 450), before(swapped + 450))
        ],
        "each entry is extracted with every event before it committed and none from it on"
    );
    assert_eq!(
        seam,
        vec![(
            at(split - 1),
            [positions_of(&log[..split]), vec![at(writer)]].concat()
        )],
        "the writer's fold lands between the read batches, the cursor at the first one's end"
    );
    assert_eq!(
        applied_positions(db),
        positions_of(&log),
        "the applied ledger holds exactly the log's positions"
    );
}

/// `rigger reset --derived` refuses to migrate while the project's `graph.db` still owes its
/// rebuild, and says how to pay it; `rigger setup` rebuilds it from the log, after which the
/// migration runs.
#[test]
fn reset_derived_refuses_until_a_pre_rule_graph_db_is_rebuilt() {
    let dir = temp_store_project();
    let root = dir.path();
    {
        let events_db = rigger_file(root, "events.db");
        let backend = Store::open(events_db.to_str().unwrap()).unwrap();
        let store = Namespaced::new(&backend, &run_stream_identity(root));
        Namespaced::new(
            &common::fixtures::PreLedgerStore {
                db: &events_db,
                inner: &backend,
            },
            &run_stream_identity(root),
        )
        .append(
            rigger::conductor::STREAM,
            ExpectedRevision::Any,
            &two_generations_dropping_facts(),
        )
        .unwrap();
        let graph_db = rigger_file(root, "graph.db");
        let log = store
            .read_stream(
                rigger::conductor::STREAM,
                0,
                rigger::eventstore::Direction::Forward,
            )
            .unwrap();
        use rigger::contextgraph::sqlite::Projector;

        common::fixtures::folds(
            &Projector::open(graph_db.to_str().unwrap(), &run_stream_identity(root)).unwrap(),
            &log[..3],
        );
        common::fixtures::stamp_user_version(&graph_db, 0);
    }

    let (out, err, ok) = run_rigger(root, &["reset", "--derived"]);
    assert!(!ok, "reset --derived must refuse; stdout: {out}");
    assert!(
        err.contains(&format!(
            "reset --derived: {}",
            rigger::contextgraph::REBUILD_OWED
        )),
        "the refusal is the one owed refusal, naming the command that rebuilds it; stderr: {err}"
    );
    setup_ok(root, &[("RIGGER_NPM", "true")]);
    let (out, err, ok) = run_rigger(root, &["reset", "--derived"]);
    assert_eq!(
        (ok, out),
        (
            true,
            format!("{}{LOG_LEFT_AS_IT_STANDS_LINE}", migrated_lines(2, 5, 0))
        ),
        "once the graph is rebuilt the migration converts each file's h2 batch into its entry \
         and sheds all five derived recordings; stderr: {err}"
    );
}

/// Pruning superseded edges (`rigger reset --runs`) keeps what the fold may still bring back: an
/// attachment detached from a retired node and a design link its identity's prior generation
/// asserted. Only history nothing will revive is reclaimed, and the preview counts exactly that.
#[test]
fn pruning_superseded_edges_keeps_what_the_fold_may_revive() {
    use rigger::contextgraph::sqlite::Projector;

    let dir = tempfile::tempdir().unwrap();
    let at = std::time::UNIX_EPOCH + std::time::Duration::from_secs(11);
    let (backend, _) = store_with(
        dir.path(),
        &[(
            rigger::conductor::STREAM,
            vec![
                f_rs_batch_def(0, "h1", 0, ("alpha", "function", 1), None, 10),
                keyed(
                    TYPE_CODE_ENTITY_EXTRACTED,
                    entity("gone", 9),
                    "gc/src/f.rs@h1#1",
                    10,
                ),
                keyed(
                    TYPE_DOC_LINK_EXTRACTED,
                    link("src/old.rs"),
                    "gd/docs/f.md@h1#0",
                    10,
                ),
                Event::new(
                    rigger::contextgraph::TYPE_COMMUNITY_ASSIGNED,
                    community_of("src/f.rs::gone"),
                )
                .with_valid_from(at),
                f_rs_batch_def(0, "h2", 0, ("alpha", "function", 2), None, 20),
                keyed(
                    TYPE_DOC_LINK_EXTRACTED,
                    link("src/a.rs"),
                    "gd/docs/f.md@h2#0",
                    20,
                ),
            ],
        )],
    );
    let db = dir.path().join("graph.db");
    fold_in_batches(&db, PROJECT, &[run_events(&backend, PROJECT)]);
    let graph = Projector::open(db.to_str().unwrap(), PROJECT).unwrap();
    let boundary = Some(nanos(100));
    let preview = graph.count_prunable(&[], boundary).unwrap();
    let pruned = graph.prune(&[], boundary).unwrap();
    assert_eq!(
        (preview.superseded_edges, pruned.superseded_edges),
        (2, 2),
        "only the retired CONTAINS edges of h1 (alpha's and gone's) are reclaimable"
    );
    common::fixtures::folds(&graph, &later_events());
    assert_eq!(
        tiers_into(&graph.whole().unwrap(), "community/1/c0"),
        vec![("src/f.rs::gone", "IN_COMMUNITY", "inferred")],
        "the detached membership survives the prune and revives when its node returns"
    );
    let edge_rows: i64 = rusqlite::Connection::open(&db)
        .unwrap()
        .query_row(
            "SELECT COUNT(*) FROM edges WHERE to_id = 'src/old.rs'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        edge_rows, 1,
        "the prior generation's retired link stays revivable"
    );
}

/// Opening a pre-rule `graph.db` never pays its rebuild and never writes to it: every open until
/// [`Projector::rebuild`] owes it again and leaves the file byte for byte as it was, so a command
/// that opens and refuses cannot let the next one fold onto it or undo a rebuild. A file at the
/// old rule that never folded anything holds nothing a rebuild could change: its open owes
/// nothing and records the current rule.
#[test]
fn an_open_owes_the_rebuild_until_it_is_paid_writes_nothing_and_an_unfolded_file_owes_none() {
    let dir = tempfile::tempdir().unwrap();
    let (backend, _) = store_with(
        dir.path(),
        &[(rigger::conductor::STREAM, two_generations_dropping_facts())],
    );
    let log = run_events(&backend, PROJECT);
    let folded = dir.path().join("folded.db");
    a_pre_rule_graph_db(&folded, &log[..3], "");
    let before = std::fs::read(&folded).unwrap();
    assert_eq!(
        version_and_owed(&folded, PROJECT),
        (0, true),
        "the first open owes it"
    );
    assert_eq!(
        version_and_owed(&folded, PROJECT),
        (0, true),
        "an open that did not rebuild leaves the rebuild owed"
    );
    assert!(
        std::fs::read(&folded).unwrap() == before,
        "opening a file that owes its rebuild writes nothing to it"
    );

    let unfolded = dir.path().join("unfolded.db");
    a_pre_rule_graph_db(&unfolded, &[], "");
    assert_eq!(
        version_and_owed(&unfolded, PROJECT),
        (PROJECTION_VERSION, false),
        "a file that folded nothing owes nothing and records the current rule"
    );
    assert_eq!(
        version_and_owed(&unfolded, PROJECT),
        (PROJECTION_VERSION, false),
        "and stays current"
    );
}

/// The `graph.db` schema of the release before the generation rule: no ledgers, no `proofs`, and
/// the `pending_proof` table that rule replaced.
const RELEASE_ERA_SCHEMA: &str = "
PRAGMA journal_mode = WAL;
CREATE TABLE nodes (
  id TEXT NOT NULL, kind TEXT NOT NULL, attrs TEXT,
  project TEXT NOT NULL DEFAULT '',
  PRIMARY KEY (id, project)
);
CREATE TABLE edges (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  from_id TEXT NOT NULL, to_id TEXT NOT NULL, rel TEXT NOT NULL,
  valid_from INTEGER NOT NULL, valid_to INTEGER, source INTEGER NOT NULL,
  project TEXT NOT NULL DEFAULT '',
  tier TEXT NOT NULL DEFAULT 'extracted'
);
CREATE INDEX idx_edges_from ON edges(from_id);
CREATE INDEX idx_edges_to ON edges(to_id);
CREATE TABLE aliases (alias TEXT PRIMARY KEY, canonical_id TEXT NOT NULL);
CREATE TABLE applied (position INTEGER PRIMARY KEY);
CREATE TABLE pending_proof (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  project TEXT NOT NULL DEFAULT '',
  name TEXT NOT NULL,
  evidence TEXT NOT NULL
);
CREATE INDEX idx_pending_proof_name ON pending_proof(project, name);
";

/// An operator store whose `graph.db` a release at an older fold rule folded. [`Self::new`] and
/// [`Self::with_identity`] give the one the release before the generation rule folded: its log
/// holds [`two_generations_dropping_facts`] plus a test's proof, and its `graph.db` still holds
/// the definition and the design link a superseded generation asserted, and the proof waiting in
/// `pending_proof`. [`Self::folded_at_rule_1`] gives the one the release before the shared-node
/// rule folded.
struct ReleaseEraStore {
    dir: tempfile::TempDir,
    events_db: std::path::PathBuf,
    graph_db: std::path::PathBuf,
}

impl ReleaseEraStore {
    /// The store with its durable identity already minted, so `rigger setup` keeps its namespace.
    fn new() -> Self {
        Self::with_identity(true)
    }

    /// The store, with its durable identity `minted` already or still named after its directory
    /// (which `rigger setup` then mints, moving the log to it).
    fn with_identity(minted: bool) -> Self {
        let mut log = two_generations_dropping_facts();
        log.push(fresh_test_reference("alpha", 3, 30));
        let store = Self::logging(minted, &log);
        let (project, appended) = (store.project(), store.log());
        let graph_db = &store.graph_db;
        let conn = rusqlite::Connection::open(graph_db).unwrap();
        conn.execute_batch(RELEASE_ERA_SCHEMA).unwrap();
        for e in &appended {
            conn.execute("INSERT INTO applied (position) VALUES (?1)", [e.position])
                .unwrap();
        }
        for (id, kind) in [
            ("src/f.rs", "file"),
            ("src/f.rs::alpha", "function"),
            ("src/f.rs::gone", "function"),
            ("docs/f.md", "artifact"),
            ("src/old.rs", "artifact"),
        ] {
            insert_node(&conn, &project, id, kind, None);
        }
        conn.execute(
            "INSERT INTO edges (from_id, to_id, rel, valid_from, valid_to, source, project, tier)
             VALUES ('docs/f.md', 'src/old.rs', ?1, 10000000000, NULL, 3, ?2, 'extracted')",
            [rigger::contextgraph::REL_SPECIFIES, project.as_str()],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO pending_proof (project, name, evidence) VALUES (?1, 'gone', 'tests/t.rs:3')",
            [project.as_str()],
        )
        .unwrap();
        drop(conn);
        store
    }

    /// The store whose log holds one generation of `src/f.rs` defining the leaf `x` twice, a fn
    /// at line 3 and then a field at line 7, and whose `graph.db` a release at fold rule 1
    /// folded: its ledgers are the current ones, it records rule 1, and the node the two
    /// definitions share, with its file's assertion of it, holds the one folded last, the field.
    fn folded_at_rule_1() -> Self {
        use rigger::contextgraph::sqlite::Projector;
        let store = Self::logging(
            true,
            &[
                f_rs_batch_def(1, "h1", 0, ("x", "function", 3), None, 10),
                f_rs_batch_def(2, "h1", 1, ("x", "field", 7), None, 10),
            ],
        );
        common::fixtures::folds(
            &Projector::open(store.graph_db.to_str().unwrap(), &store.project()).unwrap(),
            &store.log(),
        );
        Self::as_rule_1_left(&store.graph_db, 1);
        store
    }

    /// [`Self::folded_at_rule_1`]'s store in the state `stand` leaves it, which holds the rebuild
    /// state `standing` names ([`Self::rebuild_state`]).
    fn folded_at_rule_1_standing(stand: fn(&Self), standing: (bool, bool)) -> Self {
        let store = Self::folded_at_rule_1();
        stand(&store);
        assert_eq!(
            store.rebuild_state(),
            standing,
            "the rebuild state the case stands"
        );
        store
    }

    /// Stop a rebuild of this store's `graph.db` at its `read`th read of the log
    /// ([`Self::interrupt_a_rebuild_at_read`]): `1` leaves its shadow, holding the whole log,
    /// standing beside the untouched file, and `2` leaves the swapped-in file owing its tail.
    fn stop_a_rebuild_at_read(&self, read: usize) {
        self.interrupt_a_rebuild_at_read(read).unwrap_err();
    }

    /// Put the folded file at `db` in the state fold rule 1 left one holding
    /// [`Self::folded_at_rule_1`]'s log: the node the two definitions of `x` share, with its
    /// file's assertion of it, holds the one folded last, the field, and the file records
    /// `version` as its fold rule.
    fn as_rule_1_left(db: &Path, version: i64) {
        rusqlite::Connection::open(db)
            .unwrap()
            .execute_batch(&format!(
                "UPDATE nodes SET attrs = json_set(attrs, '$.kind', 'field', '$.line', '7')
                  WHERE id = 'src/f.rs::x';
                 UPDATE node_assertions
                    SET attrs = json_set(attrs, '$.kind', 'field', '$.line', '7')
                  WHERE node_id = 'src/f.rs::x';
                 PRAGMA user_version = {version};"
            ))
            .unwrap();
    }

    /// A rebuild of this store's `graph.db`, owed by its caller, that its source stops at its
    /// `read`th read of the log, each read handing the log's events past where the rebuild asks
    /// from, in one batch, before the `read`th fails: with no shadow standing the owed rebuild
    /// starts a fresh one, whether or not the file owes a cause of its own, and the first read is
    /// the shadow's fold, so `1` leaves the shadow standing beside the untouched file, and the
    /// second is the tail past the swap, so `2` leaves the swapped-in file owing its tail.
    fn interrupt_a_rebuild_at_read(
        &self,
        read: usize,
    ) -> Result<Option<Rebuilt>, rigger::contextgraph::Error> {
        let log = self.log();
        let mut reads = 0;
        rebuild_under(
            &self.graph_db,
            &self.project(),
            true,
            &mut |after, sink: &mut RebuildSink| {
                reads += 1;
                rigger::contextgraph::sqlite::stream_past(&log, after, log.len(), sink)?;
                if reads == read {
                    return source_interrupted();
                }
                Ok(())
            },
        )
    }

    /// The store whose log holds `log` and that has no `graph.db` yet, its durable identity
    /// `minted` already or still named after its directory.
    fn logging(minted: bool, log: &[Event]) -> Self {
        let dir = temp_store_project();
        let root = dir.path();
        if minted {
            std::fs::write(root.join(".rigger").join("project.id"), "proj-era\n").unwrap();
        }
        let events_db = rigger_file(root, "events.db");
        let graph_db = rigger_file(root, "graph.db");
        common::cli::seed_pre_ledger_events(root, log);
        let _ = std::fs::remove_file(&graph_db);
        ReleaseEraStore {
            dir,
            events_db,
            graph_db,
        }
    }

    fn root(&self) -> &Path {
        self.dir.path()
    }

    /// The project's identity as the binary resolves it now.
    fn project(&self) -> String {
        run_stream_identity(self.root())
    }

    /// The project's run stream as the log now records it.
    fn log(&self) -> Vec<Event> {
        run_events(
            &Store::open(self.events_db.to_str().unwrap()).unwrap(),
            &self.project(),
        )
    }

    /// The `graph.db` file's bytes: equal before and after a command that wrote nothing to it.
    fn graph_bytes(&self) -> Vec<u8> {
        std::fs::read(&self.graph_db).unwrap()
    }

    /// The shadow a rebuild of this store's `graph.db` folds into, beside the file.
    fn shadow(&self) -> std::path::PathBuf {
        rebuild_shadow(self.root())
    }

    /// The bytes of the shadow beside this store's `graph.db`, or `None` when none can be read, as
    /// when none stands: equal before and after a command that wrote nothing to it.
    fn shadow_bytes(&self) -> Option<Vec<u8>> {
        std::fs::read(self.shadow()).ok()
    }

    /// The rebuild state standing for this store's `graph.db`: whether a rebuild's shadow stands
    /// beside the file, and whether the file holds the cursor a swapped-in rebuild's tail is owed
    /// past.
    fn rebuild_state(&self) -> (bool, bool) {
        (
            self.shadow().exists(),
            holds_table(&self.graph_db, "rebuild_cursor"),
        )
    }

    /// The live projection and fold state of the project's `graph.db`, beside those of a fresh
    /// fold of the whole log.
    fn graph_and_a_fresh_fold_of_the_log(&self) -> ((String, Vec<String>), (String, Vec<String>)) {
        let fresh = self.root().join("fresh.db");
        (
            graph_identity(&self.graph_db, &self.project()),
            (
                fold_in_batches(&fresh, &self.project(), &[self.log()]),
                fold_state(&fresh),
            ),
        )
    }
}

/// The shadow a rebuild of the `graph.db` of the store at `root` folds into, beside the file.
fn rebuild_shadow(root: &Path) -> std::path::PathBuf {
    rigger_file(root, "graph.db.rebuild")
}

/// Leave beside the `graph.db` of the store at `root` the shadow a rebuild stopped as it opened
/// it leaves, before it opened its rebuild state there: a file recording the current fold rule
/// that holds no run attribution to resume from.
fn leave_a_shadow_with_no_rebuild_state(root: &Path) {
    common::fixtures::stamp_user_version(&rebuild_shadow(root), PROJECTION_VERSION);
}

/// Given an operator store whose `graph.db` the release before the generation rule folded, when
/// the operator runs `rigger setup`, then that `graph.db` is rebuilt cold from the log - setup says
/// it is rebuilding and reports how far along it is - the superseded facts are gone, the old table
/// is dropped, the file records the current rule, and it holds exactly the graph a fresh fold of
/// the whole log reaches, and the rebuild's report reads the keyed derived rows of the log as the
/// recordings they are. A second `rigger setup` owes and reports no rebuild.
#[test]
fn rigger_setup_rebuilds_a_release_era_graph_db_from_the_log_and_stamps_the_rule() {
    use rigger::contextgraph::sqlite::Projector;
    use rigger::contextgraph::Projection;
    let store = ReleaseEraStore::new();
    let out = setup_ok(store.root(), &[("RIGGER_NPM", "true")]);
    let rebuild: Vec<&str> = out
        .lines()
        .filter(|l| l.contains("graph.db") || l.starts_with("rebuilt "))
        .collect();
    let head = store.log().last().unwrap().position;
    assert_eq!(
        rebuild,
        vec![
            OLDER_RULE_REBUILD_LINE.to_string(),
            rebuilt_through_the_head(3, head),
            REBUILT_LINE.to_string(),
            "identities the tree holds a file for whose generation in graph.db is not their \
             latest recording's: 0"
                .to_string(),
        ],
        "setup says it is rebuilding and how far along it is, folding only the live selection - \
         h1's three superseded recordings are never folded - and reports no identity the next \
         ingest records again; stdout: {out}"
    );
    assert!(
        !store
            .root()
            .join(".rigger")
            .join("graph.db.rebuild")
            .exists(),
        "the shadow is removed once copied in"
    );

    let pending_proof_tables: i64 = rusqlite::Connection::open(&store.graph_db)
        .unwrap()
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE name = 'pending_proof'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        (user_version(&store.graph_db), pending_proof_tables),
        (PROJECTION_VERSION, 0),
        "the rebuilt file records the current rule and no longer holds the old table"
    );
    let rebuilt = Projector::open(store.graph_db.to_str().unwrap(), &store.project()).unwrap();
    assert!(!rebuilt.rebuild_owed().unwrap(), "the rebuild was paid");
    let whole = rebuilt.whole().unwrap();
    drop(rebuilt);
    assert_eq!(
        (
            whole.nodes.iter().any(|n| n.id == "src/f.rs::gone"),
            whole.edges.iter().any(|e| e.to == "src/old.rs"),
        ),
        (false, false),
        "the superseded generation's definition and design link are gone"
    );
    let (graph, fresh) = store.graph_and_a_fresh_fold_of_the_log();
    assert_eq!(
        graph, fresh,
        "the rebuilt graph.db is the whole log's, fold state included"
    );

    let out = setup_ok(store.root(), &[("RIGGER_NPM", "true")]);
    assert!(
        !out.contains("graph.db"),
        "a paid rebuild is not owed or reported again; stdout: {out}"
    );
}

/// Given a `graph.db` that owes its rebuild for both causes at once - the release before the
/// generation rule folded it, and it misses an event the log holds - when the operator runs `rigger setup`,
/// then setup names both causes in order, pays both with its one rebuild (the lost fold's mark is
/// gone and the file records the current rule), and a second `rigger setup` owes nothing.
#[test]
fn rigger_setup_names_both_causes_a_graph_db_owes_its_rebuild_for_and_pays_both() {
    let store = ReleaseEraStore::new();
    let mark = rigger_file(store.root(), "graph.db.owed");
    std::fs::write(&mark, b"").unwrap();
    let out = setup_ok(store.root(), &[("RIGGER_NPM", "true")]);
    assert_eq!(
        out.lines()
            .filter(|l| l.starts_with("rebuilding graph.db"))
            .collect::<Vec<_>>(),
        vec![BOTH_CAUSES_REBUILD_LINE],
        "setup names the older rule first and the lost fold second; stdout: {out}"
    );
    assert_eq!(
        (mark.exists(), user_version(&store.graph_db)),
        (false, PROJECTION_VERSION),
        "the one rebuild pays both: the mark is dropped and the file records the current rule"
    );
    let (graph, fresh) = store.graph_and_a_fresh_fold_of_the_log();
    assert_eq!(graph, fresh, "the rebuilt graph.db is the whole log's");

    let out = setup_ok(store.root(), &[("RIGGER_NPM", "true")]);
    assert!(
        !out.contains("graph.db"),
        "a paid rebuild is not owed or reported again; stdout: {out}"
    );
}

/// Given a release-era store whose durable identity `rigger setup` has yet to mint, when the
/// operator runs `rigger setup`, then the log moves to the minted identity before the rebuild reads
/// it, and the rebuilt `graph.db` is the whole log's under that identity - never an empty graph
/// folded from a namespace the history has not reached yet.
#[test]
fn rigger_setup_rebuilds_from_the_log_under_the_identity_it_mints() {
    let store = ReleaseEraStore::with_identity(false);
    let legacy = store.project();
    setup_ok(store.root(), &[("RIGGER_NPM", "true")]);
    assert_ne!(store.project(), legacy, "setup minted the durable identity");
    let (graph, fresh) = store.graph_and_a_fresh_fold_of_the_log();
    assert!(
        graph.0.contains("src/f.rs::alpha"),
        "the rebuild read the moved history; graph: {}",
        graph.0
    );
    assert_eq!(graph, fresh, "the rebuilt graph.db is the whole log's");
}

/// Given an operator store whose `graph.db` a release at fold rule 1 folded, so the node two
/// definitions of one leaf share holds the one folded last, when the operator opens it, then the
/// open owes the rebuild and writes nothing; and when the operator runs `rigger setup`, then setup
/// names the older rule, rebuilds from the log, the file records the current rule and the node
/// holds its precedence winner, the fn at the fn's line - exactly the graph a fresh fold of the
/// log reaches. A second `rigger setup` owes and reports no rebuild.
#[test]
fn rigger_setup_rebuilds_a_rule_1_graph_db_and_its_shared_node_takes_its_precedence_winner() {
    use common::fixtures::{attrs_of, definition};
    use rigger::contextgraph::sqlite::Projector;
    const X: &str = "src/f.rs::x";
    let store = ReleaseEraStore::folded_at_rule_1();
    let x = || {
        let graph = Projector::open(store.graph_db.to_str().unwrap(), &store.project()).unwrap();
        attrs_of(&graph, X)
    };
    let before = store.graph_bytes();
    assert_eq!(
        version_and_owed(&store.graph_db, PROJECT),
        (1, true),
        "a file that folded at rule 1 owes its rebuild"
    );
    assert_eq!(
        x(),
        definition("x", "field", 7),
        "until the rebuild the node is as rule 1 left it, the last folded"
    );
    assert!(
        store.graph_bytes() == before,
        "opening a file that owes its rebuild writes nothing to it"
    );

    let out = setup_ok(store.root(), &[("RIGGER_NPM", "true")]);
    let head = store.log().last().unwrap().position;
    assert_eq!(
        rebuilding_and_rebuilt_lines(&out),
        vec![
            OLDER_RULE_REBUILD_LINE.to_string(),
            rebuilt_through_the_head(2, head),
            REBUILT_LINE.to_string(),
        ],
        "setup names the older rule and refolds both definitions; stdout: {out}"
    );
    assert_eq!(
        (user_version(&store.graph_db), x()),
        (PROJECTION_VERSION, definition("x", "function", 3)),
        "the rebuilt file records the current rule and the node holds the fn at the fn's line"
    );
    assert_eq!(
        version_and_owed(&store.graph_db, PROJECT),
        (PROJECTION_VERSION, false),
        "the rebuild was paid"
    );
    let (graph, fresh) = store.graph_and_a_fresh_fold_of_the_log();
    assert_eq!(
        graph, fresh,
        "the rebuilt graph.db is the whole log's, fold state included"
    );

    let out = setup_ok(store.root(), &[("RIGGER_NPM", "true")]);
    assert!(
        !out.contains("graph.db"),
        "a paid rebuild is not owed or reported again; stdout: {out}"
    );
}

/// Given an operator store whose `graph.db` a release at fold rule 1 folded, and beside it the
/// shadow of a rebuild that release began and was interrupted in - past both definitions of the
/// shared leaf, the node holding the one folded last, and no fold rule recorded, as a rule-1
/// binary left one - when the operator runs `rigger setup`, then the shadow is never resumed:
/// setup folds the log again from its start, both events, the node holds its precedence winner,
/// the fn at the fn's line, the file records the current rule and is exactly the graph a fresh
/// fold of the log reaches, and the shadow is gone.
#[test]
fn rigger_setup_discards_the_rebuild_shadow_a_rule_1_binary_left_and_folds_the_log_again() {
    use common::fixtures::{attrs_of, definition};
    use rigger::contextgraph::sqlite::Projector;
    let store = ReleaseEraStore::folded_at_rule_1();
    let (log, shadow) = (store.log(), rigger_file(store.root(), "graph.db.rebuild"));
    let head = log.last().unwrap().position;
    let interrupted = store.interrupt_a_rebuild_at_read(1);
    assert_eq!(
        (interrupted.unwrap_err().0.as_str(), shadow.exists()),
        (SOURCE_INTERRUPTED, true),
        "the interrupted rebuild leaves its shadow holding both definitions"
    );
    ReleaseEraStore::as_rule_1_left(&shadow, 0);

    let out = setup_ok(store.root(), &[("RIGGER_NPM", "true")]);
    assert_eq!(
        common::cli::lines_where(&out, |l| l.starts_with("rebuilt ")),
        vec![rebuilt_through_the_head(2, head), REBUILT_LINE.to_string(),],
        "setup folds both definitions again, resuming nothing; stdout: {out}"
    );
    let graph = Projector::open(store.graph_db.to_str().unwrap(), &store.project()).unwrap();
    assert_eq!(
        (
            attrs_of(&graph, "src/f.rs::x"),
            user_version(&store.graph_db),
            shadow.exists()
        ),
        (definition("x", "function", 3), PROJECTION_VERSION, false),
        "the node holds the fn at the fn's line, under the current rule, and the shadow is gone"
    );
    drop(graph);
    let (graph, fresh) = store.graph_and_a_fresh_fold_of_the_log();
    assert_eq!(
        graph, fresh,
        "the rebuilt graph.db is the whole log's, fold state included"
    );
}

/// The run a live-run refusal case leaves [`SPAWN`] unanswered in.
const RUN: &str = "r1";

/// The wall-clock bound, in seconds, of the spawn a live-run refusal case leaves unanswered.
const SPAWN_BOUND_SECS: u64 = 300;

/// The reasons a refusal's stderr `err` names a run live by, joined as the live-writer probe joins
/// them: the held step lock when `step`, [`SPAWN`]'s marker when `spawn` - at the age `err` names
/// it with, which lies inside its [`SPAWN_BOUND_SECS`] bound since the marker was touched just
/// before the binary started - and the store's one driver registration, heartbeating inside the
/// idle window, when `registered`.
fn live_reasons(err: &str, step: bool, spawn: bool, registered: bool) -> String {
    let mut reasons = Vec::new();
    if step {
        reasons.push(STEP_LOCK_REASON.to_string());
    }
    if spawn {
        let age = named_marker_age(err);
        assert!(
            age < SPAWN_BOUND_SECS,
            "the marker touched just now is {age}s old, inside its {SPAWN_BOUND_SECS}s bound; \
             stderr: {err}"
        );
        reasons.push(format!(
            "1 in-flight spawn(s) in the current run touched their liveness marker within their \
             wall-clock bound: {SPAWN} (marker touched {age}s ago, bound {SPAWN_BOUND_SECS}s)"
        ));
    }
    if registered {
        reasons.push(format!(
            "1 driver registration(s) for this project's store in the machine-global instance \
             registry (spec 50) heartbeat within the last {}s - a `run`/`serve`/`step` may be \
             advancing this run elsewhere on this machine",
            idle_window_secs()
        ));
    }
    reasons.join("; ")
}

/// The line `rigger setup` ends its stderr with when it refuses a new rebuild beside a run live
/// for `reasons`.
fn live_run_refusal_line(reasons: &str) -> String {
    refusal_line(&format!(
        "rigger setup: refusing to rebuild graph.db while the run is live - {reasons}; \
         stop the run's driver, run rigger setup, then relaunch the run by adoption"
    ))
}

/// The traces a run's driver leaves for the store at `root`: [`SPAWN`]'s marker in [`RUN`]
/// touched just now when `spawn`, else a second past its bound, and the store's driver
/// registration heard from just now when `registered`, else a minute past the idle window.
fn driver_traces(root: &Path, spawn: bool, registered: bool) -> DriverTraces {
    let marker_secs_ago = if spawn { 0 } else { SPAWN_BOUND_SECS + 1 };
    let heartbeat_ms_ago = if registered {
        0
    } else {
        rigger::registry::DEFAULT_IDLE_MS + 60_000
    };
    DriverTraces::left(root, &[(RUN, marker_secs_ago)], Some(heartbeat_ms_ago))
}

/// Given an operator store whose `graph.db` a release at fold rule 1 folded, standing in the state
/// `stand` leaves it - its rebuild state, a shadow beside the file and a rebuild cursor in it, as
/// `standing` says - whose rebuild pay is NEW, starting a fresh shadow and folding the log from
/// its start, for the cause the line `cause` names, or for none the file owes when `cause` is
/// none, and whose run [`RUN`] left [`SPAWN`] unanswered, when the operator runs `rigger setup`
/// while the run is live by the signals a case names - a `rigger step` holding
/// `.rigger/step.lock` when `step`, the spawn's liveness marker touched just now when `spawn` and
/// the store's driver registration heard from just now when `registered`, each other trace
/// [`driver_traces`] leaves dead - then setup refuses the rebuild beside the live run: it exits
/// non-zero with the refusal naming exactly the live signals and the way past it, says nothing of
/// rebuilding, and writes nothing to `graph.db` or to the shadow beside it, the rebuild state
/// standing as it stood; and once the step releases the lock and every trace the driver left is
/// dead, `rigger setup` pays the rebuild, printing the `cause` line, when there is one, before it
/// says it rebuilt, and the file is exactly the graph a fresh fold of the log reaches.
fn rigger_setup_refuses_a_new_rebuild_while_the_run_is_live(
    stand: fn(&ReleaseEraStore),
    standing: (bool, bool),
    cause: Option<&str>,
    step: bool,
    spawn: bool,
    registered: bool,
) {
    let store = ReleaseEraStore::folded_at_rule_1_standing(stand, standing);
    seed_owned(
        store.root(),
        &one_unanswered_spawn(RUN, Some(SPAWN_BOUND_SECS)),
    );
    let live = driver_traces(store.root(), spawn, registered);
    let (before, step_lock) = (
        (store.graph_bytes(), store.shadow_bytes()),
        step.then(|| hold_step_lock(store.root())),
    );
    let (out, err, ok) = run_rigger_envs(
        store.root(),
        &["setup"],
        &live.envs(&[("RIGGER_NPM", "true")]),
    );
    assert_eq!(
        (
            ok,
            err.lines().last(),
            rebuilding_and_rebuilt_lines(&out),
            (store.graph_bytes(), store.shadow_bytes()) == before,
            store.rebuild_state(),
        ),
        (
            false,
            Some(live_run_refusal_line(&live_reasons(&err, step, spawn, registered)).as_str()),
            vec![],
            true,
            standing
        ),
        "setup refuses the new rebuild beside the live run and writes nothing to graph.db or its \
         shadow; stdout: {out} stderr: {err}"
    );

    drop(step_lock);
    let dead = driver_traces(store.root(), false, false);
    let out = setup_ok(store.root(), &dead.envs(&[("RIGGER_NPM", "true")]));
    assert_eq!(
        common::cli::rebuild_lines(&out),
        cause.map_or_else(|| vec![REBUILT_LINE.to_string()], common::cli::rebuilt_for),
        "once the run's driver is dead, setup pays the rebuild; stdout: {out}"
    );
    let (graph, fresh) = store.graph_and_a_fresh_fold_of_the_log();
    assert_eq!(graph, fresh, "the rebuilt graph.db is the whole log's");
}

rigger::test_cases! {
    /// A `rigger step` holds its lock, the spawn's marker and the registration dead: the refusal
    /// names the held lock alone.
    rigger_setup_refuses_a_new_rebuild_while_a_step_holds_its_lock_and_pays_it_once_released:
        rigger_setup_refuses_a_new_rebuild_while_the_run_is_live(
            |_| {},
            (false, false),
            Some(OLDER_RULE_REBUILD_LINE),
            true,
            false,
            false,
        );
    /// The spawn's worker touched its marker just now, under the scratch root `RIGGER_TMPDIR`
    /// names, with the lock free and the registration dead: the refusal names the spawn alone,
    /// read from the store's own run stream.
    rigger_setup_refuses_a_new_rebuild_beside_a_fresh_spawn_marker_and_pays_it_once_stale:
        rigger_setup_refuses_a_new_rebuild_while_the_run_is_live(
            |_| {},
            (false, false),
            Some(OLDER_RULE_REBUILD_LINE),
            false,
            true,
            false,
        );
    /// A driver registration of the store heartbeats just now in the registry `XDG_STATE_HOME`
    /// holds, with the lock free and the spawn's marker dead: the refusal names the registration
    /// alone.
    rigger_setup_refuses_a_new_rebuild_beside_a_driver_registration_and_pays_it_once_idle:
        rigger_setup_refuses_a_new_rebuild_while_the_run_is_live(
            |_| {},
            (false, false),
            Some(OLDER_RULE_REBUILD_LINE),
            false,
            false,
            true,
        );
    /// Every signal live at once: the refusal names all three, the lock, the spawn and the
    /// registration, in that order.
    rigger_setup_refuses_a_new_rebuild_naming_every_live_signal_and_pays_it_once_all_are_dead:
        rigger_setup_refuses_a_new_rebuild_while_the_run_is_live(
            |_| {},
            (false, false),
            Some(OLDER_RULE_REBUILD_LINE),
            true,
            true,
            true,
        );
    /// A rebuild stopped after its swap left its tail standing, and the swapped-in file records
    /// the fold rule before the current one, so it owes that rule's rebuild: the pay starts a
    /// fresh shadow rather than finishing the tail, and with a `rigger step` holding its lock the
    /// refusal names the held lock alone.
    rigger_setup_refuses_an_older_rule_cause_over_a_standing_tail_while_a_step_holds_its_lock:
        rigger_setup_refuses_a_new_rebuild_while_the_run_is_live(
            |store| {
                store.stop_a_rebuild_at_read(2);
                common::fixtures::stamp_user_version(&store.graph_db, PROJECTION_VERSION - 1);
            },
            (false, true),
            Some(OLDER_RULE_REBUILD_LINE),
            true,
            false,
            false,
        );
    /// A rebuild stopped before its swap left its shadow standing as a rule-1 binary leaves one,
    /// recording no fold rule, so the pay discards it and starts a fresh shadow, and with a
    /// `rigger step` holding its lock the refusal names the held lock alone.
    rigger_setup_refuses_to_refold_a_shadow_an_older_rule_binary_left_while_a_step_holds_its_lock:
        rigger_setup_refuses_a_new_rebuild_while_the_run_is_live(
            |store| {
                store.stop_a_rebuild_at_read(1);
                ReleaseEraStore::as_rule_1_left(&store.shadow(), 0);
            },
            (true, false),
            Some(OLDER_RULE_REBUILD_LINE),
            true,
            false,
            false,
        );
    /// A rebuild stopped as it opened its shadow, before it opened its rebuild state there, left
    /// the shadow recording the current fold rule with no run attribution to resume from, beside
    /// the file that owes the older rule's rebuild: the pay discards it and starts a fresh shadow,
    /// and with a `rigger step` holding its lock the refusal names the held lock alone.
    rigger_setup_refuses_to_refold_a_shadow_holding_no_rebuild_state_while_a_step_holds_its_lock:
        rigger_setup_refuses_a_new_rebuild_while_the_run_is_live(
            |store| leave_a_shadow_with_no_rebuild_state(store.root()),
            (true, false),
            Some(OLDER_RULE_REBUILD_LINE),
            true,
            false,
            false,
        );
    /// An earlier setup rebuilt the file, so it owes nothing and the pay names no cause, and a
    /// rebuild a binary of a newer fold rule stopped before its swap left its shadow standing, its
    /// rebuild state whole: the shadow was not folded under the current rule, so the pay discards
    /// it and starts a fresh shadow, and with a `rigger step` holding its lock the refusal names
    /// the held lock alone.
    rigger_setup_refuses_to_refold_a_newer_rule_shadow_beside_a_file_owing_nothing_while_a_step_holds_its_lock:
        rigger_setup_refuses_a_new_rebuild_while_the_run_is_live(
            |store| {
                setup_ok(store.root(), &[("RIGGER_NPM", "true")]);
                store.stop_a_rebuild_at_read(1);
                common::fixtures::stamp_user_version(&store.shadow(), PROJECTION_VERSION + 1);
            },
            (true, false),
            None,
            true,
            false,
            false,
        );
}

/// Given an operator store whose owning root has no UTF-8 path, so the scratch root its runs write
/// their liveness markers under cannot be resolved, whose `graph.db` a setup made and beside which
/// a rebuild stopped as it opened its shadow left it standing with no rebuild state
/// ([`leave_a_shadow_with_no_rebuild_state`]), so its pay is NEW, when the operator runs `rigger
/// setup`, then setup refuses the rebuild rather than reading the unresolved root as no live
/// spawn: it exits non-zero naming the unresolvable owning root, in words that name no command,
/// as its last line of stderr, says nothing of rebuilding and writes nothing to `graph.db` or to
/// the shadow beside it.
#[cfg(unix)]
#[test]
fn rigger_setup_refuses_a_new_rebuild_in_a_store_whose_owning_root_has_no_utf8_path() {
    let (_parent, root) = common::cli::store_at_a_non_utf8_root();
    setup_ok(&root, &[("RIGGER_NPM", "true")]);
    leave_a_shadow_with_no_rebuild_state(&root);
    let before = graph_files(&root);
    let (out, err, ok) = run_rigger_envs(&root, &["setup"], &[("RIGGER_NPM", "true")]);
    assert_eq!(
        (
            ok,
            err.lines().last(),
            rebuilding_and_rebuilt_lines(&out),
            graph_files(&root) == before,
        ),
        (
            false,
            Some(common::cli::unresolved_scratch_root_refusal_line(&root).as_str()),
            vec![],
            true,
        ),
        "setup refuses the new rebuild over the unresolvable scratch root and writes nothing to \
         graph.db or its shadow; stdout: {out} stderr: {err}"
    );
}

/// Given an operator store whose `graph.db` a release at fold rule 1 folded, in the state `stand`
/// leaves it - its rebuild state, a shadow beside the file and a rebuild cursor in it, as
/// `standing` says, standing work its pay finishes without starting a fresh shadow, or none -
/// when the operator runs `rigger setup` while a `rigger step` holds `.rigger/step.lock`, then
/// setup is not refused: it says it rebuilds and that it rebuilt exactly
/// as `lines` says, and leaves the file holding no rebuild state, under the current fold rule,
/// its `caught_up` cursor at the log's head and exactly the graph a fresh fold of the log reaches.
fn rigger_setup_runs_beside_a_held_step_lock(
    stand: fn(&ReleaseEraStore),
    standing: (bool, bool),
    lines: &[&str],
) {
    let store = ReleaseEraStore::folded_at_rule_1_standing(stand, standing);
    let _step = hold_step_lock(store.root());
    let out = setup_ok(store.root(), &[("RIGGER_NPM", "true")]);
    assert_eq!(
        rebuilding_and_rebuilt_lines(&out),
        lines,
        "setup runs beside the held step lock; stdout: {out}"
    );
    let head = store.log().last().unwrap().position;
    assert_eq!(
        (
            store.rebuild_state(),
            user_version(&store.graph_db),
            caught_up_at(&store.graph_db)
        ),
        ((false, false), PROJECTION_VERSION, Some(head)),
        "the file holds no rebuild state, records the current rule and is caught up to the head"
    );
    let (graph, fresh) = store.graph_and_a_fresh_fold_of_the_log();
    assert_eq!(graph, fresh, "the graph.db is the whole log's");
}

rigger::test_cases! {
    /// A rebuild stopped before its swap: its shadow, folded under the current rule with the run
    /// attribution it gathered, stands beside the file that still owes it, so setup names the
    /// older rule and resumes the shadow rather than starting a fresh one, the shadow already
    /// holding the whole log.
    rigger_setup_finishes_a_standing_shadow_beside_a_held_step_lock:
        rigger_setup_runs_beside_a_held_step_lock(
            |store| store.stop_a_rebuild_at_read(1),
            (true, false),
            &[OLDER_RULE_REBUILD_LINE, REBUILT_LINE],
        );
    /// A rebuild stopped after its swap: the swapped-in file owes nothing and its tail stands, so
    /// setup names no cause and finishes the tail.
    rigger_setup_finishes_a_standing_tail_beside_a_held_step_lock:
        rigger_setup_runs_beside_a_held_step_lock(
            |store| store.stop_a_rebuild_at_read(2),
            (false, true),
            &[REBUILT_LINE],
        );
    /// A file an earlier setup rebuilt: it owes nothing and no rebuild stands, so setup rebuilds
    /// nothing.
    rigger_setup_owing_no_rebuild_runs_beside_a_held_step_lock:
        rigger_setup_runs_beside_a_held_step_lock(
            |store| {
                setup_ok(store.root(), &[("RIGGER_NPM", "true")]);
            },
            (false, false),
            &[],
        );
}

/// Given a project whose `graph.db` setup created beside the empty log the store selects - the
/// local `.rigger/events.db`, or the KurrentDB server at `conn` - whose run [`RUN`] then left
/// [`SPAWN`] unanswered on that log, and whose `graph.db` stands as a fold made it before the
/// cursor existed, its empty ledger beside the log's events owing the rebuild of a lost fold, when
/// the operator runs `rigger setup` while the spawn's liveness marker is fresh, then setup reads
/// the spawn from the log it selected and refuses the rebuild: it exits non-zero with the refusal
/// naming the spawn, says nothing of rebuilding and writes nothing to `graph.db`; and once the
/// marker is stale the file still owes the lost fold, which setup pays.
fn rigger_setup_refuses_a_lost_fold_s_rebuild_beside_a_live_spawn_on_the_log_it_selects(
    conn: Option<&str>,
) {
    let dir = common::cli::temp_project();
    let root = dir.path();
    let mut lane = vec![("RIGGER_NPM", "true")];
    lane.extend(conn.map(|conn| ("KURRENTDB_CONN", conn)));
    setup_ok(root, &lane);
    let spawn = one_unanswered_spawn(RUN, Some(SPAWN_BOUND_SECS));
    match conn {
        None => seed_owned(root, &spawn),
        Some(conn) => append_run_events(
            &Namespaced::new(
                &rigger::eventstore::kurrentdb::Store::open(conn).unwrap(),
                &run_stream_identity(root),
            ),
            &spawn,
        ),
    }
    let graph_db = rigger_file(root, "graph.db");
    drop_the_cursor(&graph_db);
    let before = std::fs::read(&graph_db).unwrap();

    let live = driver_traces(root, true, false);
    let (out, err, ok) = run_rigger_envs(root, &["setup"], &live.envs(&lane));
    assert_eq!(
        (
            ok,
            err.lines().last(),
            rebuilding_and_rebuilt_lines(&out),
            std::fs::read(&graph_db).unwrap() == before,
        ),
        (
            false,
            Some(live_run_refusal_line(&live_reasons(&err, false, true, false)).as_str()),
            vec![],
            true
        ),
        "setup refuses the lost fold's rebuild beside the spawn its log holds and writes nothing; \
         stdout: {out} stderr: {err}"
    );

    let dead = driver_traces(root, false, false);
    let out = setup_ok(root, &dead.envs(&lane));
    assert_eq!(
        common::cli::rebuild_lines(&out),
        common::cli::rebuilt_for_a_lost_fold(),
        "the refused setup left the lost fold owed, and once the marker is stale setup pays it; \
         stdout: {out}"
    );
}

rigger::test_cases! {
    /// The local `.rigger/events.db`: setup reads the spawn from the store's own namespace of it.
    rigger_setup_refuses_a_lost_fold_s_rebuild_beside_a_live_spawn_on_the_sqlite_log:
        rigger_setup_refuses_a_lost_fold_s_rebuild_beside_a_live_spawn_on_the_log_it_selects(None);
    /// The KurrentDB server `KURRENTDB_CONN` names: setup reads the spawn from the store's own
    /// namespace of the server's log. Gracefully skipped when no container runtime is reachable.
    rigger_setup_refuses_a_lost_fold_s_rebuild_beside_a_live_spawn_on_the_server_log:
        common::fixtures::with_kurrentdb(|conn| {
            rigger_setup_refuses_a_lost_fold_s_rebuild_beside_a_live_spawn_on_the_log_it_selects(
                Some(conn),
            )
        });
}

/// Given a cold start - a project with neither `graph.db` nor `.rigger/events.db`, its event log
/// absent - when the operator runs `rigger setup` twice, then neither setup creates the event log,
/// the seeding reading the absent log as an empty one, and the first creates `graph.db` with its
/// `caught_up` cursor at 0, which the second leaves where it stands; neither reports a rebuild or
/// says anything of `graph.db`.
#[test]
fn rigger_setup_on_a_project_without_a_graph_db_creates_it_and_seeds_its_cursor_at_zero() {
    let dir = common::cli::temp_project();
    let root = dir.path();
    for _ in 0..2 {
        let out = setup_ok(root, &[("RIGGER_NPM", "true")]);
        assert_eq!(
            (
                rigger_file(root, "events.db").exists(),
                caught_up_at(&rigger_file(root, "graph.db")),
                out.contains("graph.db")
            ),
            (false, Some(0), false),
            "setup creates graph.db with its cursor at 0 and never the log; stdout: {out}"
        );
    }
}

/// Given a `graph.db` on a project whose identity is minted, so no identity migration waits for
/// the next setup, that `stand` leaves as the route under test finds it beside the log's newest
/// event, folded or not as `folded_while_locked` says, when the operator runs `rigger setup` while
/// another writer holds the file's write lock past the busy timeout, then setup exits zero, says
/// of `graph.db` only that the catch-up was not started and the next `rigger setup` starts it, and
/// leaves that event as folded or missing as it found it and the cursor where it stood; the next
/// `rigger setup`, the lock released, says `said` of `graph.db` and leaves that event folded and
/// the cursor at it.
fn a_seeding_that_meets_another_writer_s_lock_is_skipped_and_the_next_setup_folds_on(
    stand: fn(&Path, &Path),
    folded_while_locked: bool,
    said: Vec<String>,
) {
    let dir = temp_store_project();
    let root = dir.path();
    emit_the_first_decision_after_setup(root);
    let graph_db = rigger_file(root, "graph.db");
    stand(root, &graph_db);
    let (cursor, newest) = (caught_up_at(&graph_db), run_log_head(root));
    // What setup's stdout `out` says of `graph.db`, whether the log's newest event is folded, and
    // where the cursor stands.
    let standing = |out: &str| {
        (
            common::cli::lines_where(out, |l| l.contains("graph.db"))
                .into_iter()
                .map(str::to_string)
                .collect::<Vec<_>>(),
            applied(&graph_db, newest),
            caught_up_at(&graph_db),
        )
    };

    let out = with_graph_locked(&graph_db, || setup_ok(root, &[("RIGGER_NPM", "true")]));
    assert_eq!(
        standing(&out),
        (
            vec![SEEDING_SKIPPED_LINE.to_string()],
            folded_while_locked,
            cursor
        ),
        "the seeding meets the lock: setup exits zero, says so, folds nothing and moves no cursor; \
         stdout: {out}"
    );
    let out = setup_ok(root, &[("RIGGER_NPM", "true")]);
    assert_eq!(
        standing(&out),
        (said, true, Some(newest)),
        "the next setup leaves the log's newest event folded and the cursor at it; stdout: {out}"
    );
}

rigger::test_cases! {
    /// The file as a fold made it before the cursor existed ([`drop_the_cursor`]), its first
    /// seeding reading the log's positions against its ledger: the next setup seeds the cursor at
    /// the highest applied position, the log's newest event, and says nothing of `graph.db`.
    a_seeding_that_meets_another_writer_s_lock_is_skipped_and_the_next_setup_seeds_the_cursor:
        a_seeding_that_meets_another_writer_s_lock_is_skipped_and_the_next_setup_folds_on(
            |_, graph_db| drop_the_cursor(graph_db),
            true,
            vec![],
        );
    /// The file carries its cursor and misses a decision appended behind every fold: the seeding
    /// takes the write lock before it reads the cursor, so the catch-up is not started either, and
    /// the next setup folds the decision forward from the cursor and says it caught the file up.
    a_seeding_of_a_cursored_graph_db_that_meets_another_writer_s_lock_starts_no_catch_up:
        a_seeding_that_meets_another_writer_s_lock_is_skipped_and_the_next_setup_folds_on(
            |root, _| {
                append_unfolded_decision_named(root, "d-lost");
            },
            false,
            vec![caught_up_line(1)],
        );
}

/// Given an embedded event log whose run stream holds three events, each appended by a store
/// opened for it, when the seeding of a graph's cursor reads a position's age through
/// `Store::recorded_at`, then each position the log holds answers the store stamp its event reads
/// back with, and position 0 and the position past the head, where it holds none, answer none
/// (spec 124, THE CLOCKS).
#[test]
fn the_store_answers_each_held_position_s_stamp_and_none_where_it_holds_no_event() {
    let dir = temp_store_project();
    let root = dir.path();
    for _ in 0..3 {
        append_unfolded_decision(root, b"{}");
    }
    let head = run_log_head(root);
    let expected: Vec<_> = std::iter::once((0, None))
        .chain(
            read_run_events(root)
                .into_iter()
                .map(|e| (e.position, Some(e.recorded_at))),
        )
        .chain([(head + 1, None)])
        .collect();
    let store = Store::open(rigger_file(root, "events.db").to_str().unwrap()).unwrap();
    assert_eq!(
        expected
            .iter()
            .map(|&(at, _)| (at, store.recorded_at(at).unwrap()))
            .collect::<Vec<_>>(),
        expected,
        "each held position answers its event's stamp; an empty one answers none"
    );
}

/// Given a `graph.db` without the `caught_up` cursor, on a project whose identity is minted,
/// recording fold rule `rule` and missing the event at log position `lost` below an applied one,
/// when the operator runs `rigger setup` with every event of the log recorded `age` earlier than it
/// was, then setup reads the missing event's age from its store stamp, prints `said` of
/// `graph.db`, and leaves `lost` folded and the cursor at the log's head. At the current rule a
/// hole within the catch-up horizon is young, so setup rebuilds nothing: it seeds the cursor just
/// below the hole and folds the log forward from there, saying it caught the file up with the one
/// event it missed; a hole past the horizon is old, so setup rebuilds the file and the rebuild's
/// drop sets the cursor at the log's head, leaving the catch-up nothing to fold. A file at an older
/// rule is rebuilt for that rule whatever the hole's age, owing the lost fold beside it only for an
/// old hole. Either way a later `rigger setup`, every event by then older than the horizon, reads
/// the cursor the file carries and nothing else, rebuilding nothing and leaving the cursor where it
/// stands.
fn setup_reads_the_age_of_a_hole_below_an_applied_event(
    rule: i64,
    age: std::time::Duration,
    said: Vec<String>,
) {
    let dir = temp_store_project();
    let root = dir.path();
    emit_the_first_decision_after_setup(root);
    let graph_db = rigger_file(root, "graph.db");
    drop_the_cursor(&graph_db);
    let lost = append_unfolded_decision_named(root, "d-hole");
    emit_folding(root, "d-above");
    common::fixtures::stamp_user_version(&graph_db, rule);
    backdate_the_log_by(root, age);

    let out = setup_ok(root, &[("RIGGER_NPM", "true")]);
    let cursor = Some(run_log_head(root));
    assert_eq!(
        (
            common::cli::rebuild_lines(&out),
            applied(&graph_db, lost),
            caught_up_at(&graph_db)
        ),
        (said, true, cursor),
        "setup reads the hole's age from its stamp; stdout: {out}"
    );
    backdate_the_log(root);
    let out = setup_ok(root, &[("RIGGER_NPM", "true")]);
    assert_eq!(
        (
            out.contains("graph.db"),
            applied(&graph_db, lost),
            caught_up_at(&graph_db)
        ),
        (false, true, cursor),
        "a file carrying its cursor is never seeded again; stdout: {out}"
    );
}

rigger::test_cases! {
    /// An event recorded just now.
    a_hole_recorded_just_now_is_young_and_setup_catches_up_over_it:
        setup_reads_the_age_of_a_hole_below_an_applied_event(
            PROJECTION_VERSION,
            std::time::Duration::ZERO,
            vec![caught_up_line(1)],
        );
    /// An event older than the fold-lock ceiling (600 s) by 10 s, 50 s inside the horizon.
    a_hole_past_the_fold_lock_ceiling_within_the_horizon_is_still_young:
        setup_reads_the_age_of_a_hole_below_an_applied_event(
            PROJECTION_VERSION,
            CATCH_UP_HORIZON - std::time::Duration::from_secs(50),
            vec![caught_up_line(1)],
        );
    /// An event older than the catch-up horizon.
    a_hole_past_the_catch_up_horizon_is_old_and_setup_rebuilds_over_it:
        setup_reads_the_age_of_a_hole_below_an_applied_event(
            PROJECTION_VERSION,
            past_the_horizon(),
            rebuilt_for_a_lost_fold(),
        );
    /// An event recorded just now, in a file at the fold rule before the current one.
    a_young_hole_in_a_file_at_an_older_fold_rule_owes_the_older_rule_alone:
        setup_reads_the_age_of_a_hole_below_an_applied_event(
            PROJECTION_VERSION - 1,
            std::time::Duration::ZERO,
            rebuilt_for(OLDER_RULE_REBUILD_LINE),
        );
    /// An event older than the catch-up horizon, in a file at the fold rule before the current one.
    an_old_hole_in_a_file_at_an_older_fold_rule_owes_both_causes:
        setup_reads_the_age_of_a_hole_below_an_applied_event(
            PROJECTION_VERSION - 1,
            past_the_horizon(),
            rebuilt_for(BOTH_CAUSES_REBUILD_LINE),
        );
}

/// Given a project whose identity is minted and whose `graph.db` folded the log, when `lose` loses
/// a fold of the file and the operator runs `rigger setup`, then setup finds the file's ledger
/// beside the log's events a lost fold and rebuilds it from the log into the graph it held before
/// the loss, the rebuild's drop setting its cursor at the log's head; the next `rigger setup` says
/// nothing of `graph.db`.
fn setup_rebuilds_a_lost_fold_into_the_graph_the_log_holds(lose: fn(&Path)) {
    let dir = temp_store_project();
    let root = dir.path();
    emit_the_first_decision_after_setup(root);
    emit_folding(root, "d-second");
    let before = whole_graph(root);
    lose(root);

    let out = setup_ok(root, &[("RIGGER_NPM", "true")]);
    assert_eq!(
        (
            common::cli::rebuild_lines(&out),
            whole_graph(root),
            caught_up_at(&rigger_file(root, "graph.db"))
        ),
        (rebuilt_for_a_lost_fold(), before, Some(run_log_head(root))),
        "setup rebuilds the file from the log; stdout: {out}"
    );
    let out = setup_ok(root, &[("RIGGER_NPM", "true")]);
    assert!(
        !out.contains("graph.db"),
        "the rebuilt file owes nothing; stdout: {out}"
    );
}

/// Fold the decision `d-third` in `root` ([`emit_folding`]), drop the `caught_up` cursor of its
/// `graph.db` and delete that decision's event from `.rigger/events.db`: a file without a cursor
/// whose highest applied position stands above the log's head, as when the log is replaced under
/// a standing graph file.
fn cut_the_log_below_a_cursorless_graph_db(root: &Path) {
    emit_folding(root, "d-third");
    drop_the_cursor(&rigger_file(root, "graph.db"));
    rigger_db(root, "events.db")
        .execute(
            "DELETE FROM events WHERE position = ?1",
            [run_log_head(root) as i64],
        )
        .unwrap();
}

/// Copy `root`'s `.rigger/events.db`, fold the decision `d-third` ([`emit_folding`]) and run
/// `rigger setup` so the `caught_up` cursor of its `graph.db` reaches the log's head, then replace
/// the log by the copy: a file whose cursor stands above the head of a log the operator replaced
/// under it by a shorter one, whose positions issued end below the cursor.
fn replace_the_log_below_a_cursored_graph_db(root: &Path) {
    let shorter = rigger_file(root, "events.db.shorter");
    rigger_db(root, "events.db")
        .execute("VACUUM INTO ?1", [shorter.to_str().unwrap()])
        .unwrap();
    emit_folding(root, "d-third");
    setup_ok(root, &[("RIGGER_NPM", "true")]);
    remove_the_store_file(root, "events.db");
    std::fs::rename(shorter, rigger_file(root, "events.db")).unwrap();
}

rigger::test_cases! {
    /// The operator removes `graph.db`: setup creates the file afresh, its empty ledger beside the
    /// log's events.
    setup_rebuilds_a_graph_db_removed_beside_a_non_empty_log_into_the_graph_it_held:
        setup_rebuilds_a_lost_fold_into_the_graph_the_log_holds(|root| {
            remove_the_store_file(root, "graph.db")
        });
    /// The log loses the newest event a file without a cursor folded, the file's ledger standing
    /// above the log's head.
    setup_rebuilds_a_cursorless_graph_db_that_applied_past_the_log_s_head_into_the_graph_the_log_holds:
        setup_rebuilds_a_lost_fold_into_the_graph_the_log_holds(
            cut_the_log_below_a_cursorless_graph_db
        );
    /// The operator replaces the log under a file carrying its cursor by a shorter one, the cursor
    /// standing above the new log's head.
    setup_rebuilds_a_graph_db_whose_cursor_stands_past_a_shorter_log_s_head_into_the_graph_it_holds:
        setup_rebuilds_a_lost_fold_into_the_graph_the_log_holds(
            replace_the_log_below_a_cursored_graph_db
        );
}

/// Given a `graph.db` carrying its `caught_up` cursor that misses more events than one page of the
/// log holds ([`FOLD_BATCH`]) - appended behind every fold, as another writer of the log leaves
/// them - when the operator runs `rigger setup`, then setup folds every one forward from the
/// cursor, one page per catch-up call, says it caught up with all of them, and leaves the cursor at
/// the log's head, rebuilding nothing.
#[test]
fn setup_folds_a_cursored_graph_db_forward_over_a_gap_longer_than_a_page() {
    let dir = temp_store_project();
    let root = dir.path();
    emit_the_first_decision_after_setup(root);
    let missed: Vec<u64> = (0..=FOLD_BATCH)
        .map(|i| append_unfolded_decision_named(root, &format!("d-gap-{i}")))
        .collect();
    let graph_db = rigger_file(root, "graph.db");

    let out = setup_ok(root, &[("RIGGER_NPM", "true")]);
    assert_eq!(
        (
            common::cli::rebuild_lines(&out),
            applied_each(&graph_db, &missed),
            caught_up_at(&graph_db)
        ),
        (
            vec![caught_up_line(missed.len())],
            vec![true; missed.len()],
            Some(run_log_head(root))
        ),
        "setup folds each missed event forward from the cursor, over more than one page; stdout: \
         {out}"
    );
}

/// Given a `graph.db` carrying its `caught_up` cursor that misses an event appended behind every
/// fold, when the operator runs `rigger setup` while the file's storage refuses that event's node
/// ([`the_disk_gives_out_at_d_boom`]), then setup's catch-up stops at the cursor and setup fails
/// naming that position and the storage error, leaving the event missing and the cursor where it
/// stood; once the storage recovers, the next `rigger setup` folds the event forward from the
/// cursor and moves the cursor to it.
#[test]
fn setup_whose_catch_up_meets_a_storage_error_fails_at_the_cursor_and_the_next_setup_folds_on() {
    let dir = temp_store_project();
    let root = dir.path();
    emit_the_first_decision_after_setup(root);
    let graph_db = rigger_file(root, "graph.db");
    let cursor = caught_up_at(&graph_db).unwrap();
    let lost = append_unfolded_decision(root, D_BOOM);
    the_disk_gives_out_at_d_boom(&graph_db);

    let (out, err, ok) = run_rigger_envs(root, &["setup"], &[("RIGGER_NPM", "true")]);
    let refused = refusal_line(&catch_up_failed_line(cursor, DISK_GAVE_OUT));
    assert_eq!(
        (
            ok,
            err.lines().last(),
            common::cli::rebuild_lines(&out),
            applied(&graph_db, lost),
            caught_up_at(&graph_db)
        ),
        (
            false,
            Some(refused.as_str()),
            Vec::<String>::new(),
            false,
            Some(cursor)
        ),
        "setup fails naming where its catch-up stopped and why, and leaves the event missing and \
         the cursor where it stood; stdout: {out} stderr: {err}"
    );

    the_disk_recovers(&graph_db);
    let out = setup_ok(root, &[("RIGGER_NPM", "true")]);
    assert_eq!(
        (
            common::cli::rebuild_lines(&out),
            applied(&graph_db, lost),
            caught_up_at(&graph_db)
        ),
        (vec![caught_up_line(1)], true, Some(lost)),
        "once the storage recovers, setup folds the event forward from the cursor; stdout: {out}"
    );
}

/// Given a `graph.db` without the `caught_up` cursor that folded the log, on a project whose
/// identity is minted, recording fold rule `rule`, when the operator removes `.rigger/events.db`
/// and runs `rigger setup`, then setup reads the absent log as an empty one without creating it,
/// prints `rebuilt` of `graph.db` and rebuilds the file from that log, leaving it empty with its
/// cursor at 0. At the current rule the file's ledger beside the empty log is a lost fold; a file
/// at an older rule owes that rule alone, its seeding finding no missing position below an applied
/// one, since the empty log holds none.
fn setup_reads_a_cursorless_graph_db_whose_event_log_was_removed(rule: i64, rebuilt: Vec<String>) {
    let dir = temp_store_project();
    let root = dir.path();
    emit_the_first_decision_after_setup(root);
    drop_the_cursor(&rigger_file(root, "graph.db"));
    common::fixtures::stamp_user_version(&rigger_file(root, "graph.db"), rule);
    remove_the_store_file(root, "events.db");

    let out = setup_ok(root, &[("RIGGER_NPM", "true")]);
    assert_eq!(
        (
            common::cli::rebuild_lines(&out),
            rigger_file(root, "events.db").exists(),
            whole_graph(root),
            caught_up_at(&rigger_file(root, "graph.db"))
        ),
        (
            rebuilt,
            false,
            serde_json::to_string(&rigger::contextgraph::Graph::default()).unwrap(),
            Some(0)
        ),
        "setup rebuilds the file from the absent log and creates no log; stdout: {out}"
    );
}

rigger::test_cases! {
    /// A file at the current fold rule.
    setup_rebuilds_a_cursorless_graph_db_whose_event_log_was_removed_and_creates_no_log:
        setup_reads_a_cursorless_graph_db_whose_event_log_was_removed(
            PROJECTION_VERSION,
            rebuilt_for_a_lost_fold(),
        );
    /// A file at the fold rule before the current one.
    setup_rebuilds_a_cursorless_graph_db_at_an_older_fold_rule_whose_log_was_removed_for_that_rule:
        setup_reads_a_cursorless_graph_db_whose_event_log_was_removed(
            PROJECTION_VERSION - 1,
            rebuilt_for(OLDER_RULE_REBUILD_LINE),
        );
}

/// Given a `graph.db` still at the old fold rule, when an agent runs `rigger emit`, then the event
/// is appended to the log and the emit says it was not folded, naming `rigger setup`, and
/// `graph.db` is left byte for byte as it was; `rigger setup` then folds the event with the rest.
#[test]
fn an_emit_at_the_old_rule_appends_says_it_skipped_the_fold_and_leaves_graph_db_unchanged() {
    let store = ReleaseEraStore::new();
    let before = store.graph_bytes();
    let (out, err, ok) = run_rigger(
        store.root(),
        &[
            "emit",
            "DecisionMade",
            r#"{"id":"d-up","summary":"s","governs":["src/f.rs::alpha"],"supersedes":""}"#,
        ],
    );
    assert!(ok, "the emit must succeed; stdout: {out} stderr: {err}");
    let log = store.log();
    let last = log.last().unwrap();
    assert_eq!(
        (log.len(), last.type_.as_str()),
        (7, "DecisionMade"),
        "the decision is appended to the log"
    );
    assert_eq!(
        out,
        format!(
            "emitted DecisionMade (position {}); {}\n",
            last.position,
            not_folded(&graph_error(rigger::contextgraph::REBUILD_OWED))
        ),
        "the emit says it skipped the fold and names the rebuild"
    );
    assert!(
        store.graph_bytes() == before,
        "an emit at the old rule writes nothing to graph.db"
    );

    setup_ok(store.root(), &[("RIGGER_NPM", "true")]);
    let (graph, fresh) = store.graph_and_a_fresh_fold_of_the_log();
    assert_eq!(
        graph, fresh,
        "the rebuild folds the emitted decision with the rest of the log"
    );
}

/// Given a `graph.db` still at the old fold rule, a command whose answer depends on the fold
/// (`rigger graph build`) refuses at once naming `rigger setup` and writes nothing, while a
/// read-only inspection (`rigger graph --around`) answers from the graph as it stands, says the
/// rebuild is owed, and writes nothing either.
#[test]
fn a_fold_dependent_command_refuses_at_the_old_rule_and_an_inspection_answers_as_it_stands() {
    let store = ReleaseEraStore::new();
    let before = store.graph_bytes();
    let (out, err, ok) = run_rigger(store.root(), &["graph", "build"]);
    assert!(!ok, "graph build must refuse; stdout: {out}");
    assert!(
        err.contains(&format!(
            "graph build: {}",
            rigger::contextgraph::REBUILD_OWED
        )),
        "the refusal names `rigger setup`; stderr: {err}"
    );
    assert!(store.graph_bytes() == before, "the refusal writes nothing");

    let (out, err, ok) = run_rigger(store.root(), &["graph", "--around", "docs/f.md"]);
    assert!(ok, "an inspection still answers; stderr: {err}");
    assert!(
        err.contains(&rebuild_owed_note()),
        "the inspection says the rebuild is owed; stderr: {err}"
    );
    assert!(
        out.contains("src/old.rs"),
        "it answers from the graph as it stands; stdout: {out}"
    );
    assert!(
        store.graph_bytes() == before,
        "the inspection writes nothing"
    );
}

/// While `rigger setup`'s rebuild folds its shadow, a read-only open answers from the old file
/// without waiting and changes nothing, a command that depends on the fold refuses at once naming
/// `rigger setup` rather than failing on a lock, an emit appends without folding, and a second
/// rebuild is refused rather than folding into the same shadow; when the
/// rebuild has swapped the shadow in, its ledgers are intact and it has folded the emitted event
/// too - the graph is exactly the whole log's.
#[test]
fn opens_racing_the_rebuild_neither_wait_nor_undo_it() {
    use rigger::contextgraph::sqlite::Projector;
    use rigger::contextgraph::Projection;
    use std::sync::mpsc::channel;
    use std::time::{Duration, Instant};
    let store = ReleaseEraStore::new();
    let (paused_tx, paused) = channel();
    let (resume, resume_rx) = channel::<()>();
    let rebuilder = {
        let (events_db, graph_db) = (store.events_db.clone(), store.graph_db.clone());
        let project = store.project();
        std::thread::spawn(move || {
            let backend = Store::open(events_db.to_str().unwrap()).unwrap();
            let mut batches = 0;
            let mut source = live_selection_of(&backend, &project, 1);
            let ran = Projector::open(graph_db.to_str().unwrap(), &project)
                .unwrap()
                .rebuild(
                    &Projector::lock_rebuild(graph_db.to_str().unwrap()).unwrap(),
                    false,
                    &mut source,
                    &mut |_| Ok(None),
                    &mut |_| {
                        batches += 1;
                        if batches == 1 {
                            paused_tx.send(()).unwrap();
                            resume_rx.recv().unwrap();
                        }
                    },
                )
                .unwrap();
            ran
        })
    };
    paused.recv().unwrap();

    let started = Instant::now();
    let reader = Projector::open(store.graph_db.to_str().unwrap(), &store.project()).unwrap();
    let (owed, whole) = (reader.rebuild_owed().unwrap(), reader.whole().unwrap());
    drop(reader);
    assert_eq!(
        (owed, whole.edges.iter().any(|e| e.to == "src/old.rs")),
        (true, true),
        "a read during the rebuild sees the old file as it stands"
    );
    let (_, err, ok) = run_rigger(store.root(), &["graph", "build"]);
    assert!(
        !ok && err.contains(&format!(
            "graph build: {}",
            rigger::contextgraph::REBUILD_OWED
        )),
        "a folding command during the rebuild refuses naming `rigger setup`; stderr: {err}"
    );
    let (out, err, ok) = run_rigger(
        store.root(),
        &[
            "emit",
            "DecisionMade",
            r#"{"id":"d-race","summary":"s","governs":["src/f.rs::alpha"],"supersedes":""}"#,
        ],
    );
    assert!(
        ok && out.contains("not folded into the context graph"),
        "an emit during the rebuild appends without folding; stdout: {out} stderr: {err}"
    );
    assert!(
        started.elapsed() < Duration::from_secs(4),
        "none of them waited on the rebuild's lock (the busy timeout is 5s)"
    );
    let second = Projector::lock_rebuild(store.graph_db.to_str().unwrap())
        .map(drop)
        .map_err(|e| e.0);
    assert_eq!(
        second,
        Err(rigger::contextgraph::sqlite::REBUILD_IN_PROGRESS.to_string()),
        "a second rebuild racing this one is refused at its lock, never interleaved in its shadow"
    );

    resume.send(()).unwrap();
    assert_eq!(
        rebuilder.join().unwrap(),
        Some(Rebuilt::default()),
        "the rebuild ran"
    );
    assert_eq!(
        user_version(&store.graph_db),
        PROJECTION_VERSION,
        "and recorded the current rule"
    );
    let (graph, fresh) = store.graph_and_a_fresh_fold_of_the_log();
    assert_eq!(
        graph, fresh,
        "the rebuilt ledgers are intact and the raced emit is folded"
    );
}

/// The read-only surfaces over `graph.db`, each as the operator runs it: `rigger graph --show`,
/// `rigger validate` and `rigger dash --export`.
const READ_ONLY_SURFACES: [&[&str]; 3] = [
    &["graph", "--show", "src/f.rs::gone"],
    &["validate"],
    &common::cli::DASH_SNAPSHOT_ARGS,
];

/// Run the read-only surface `args` ([`READ_ONLY_SURFACES`]) in `root`: its stdout, its stderr,
/// whether it succeeded, and the notes it printed on its stderr in the order printed - every line
/// opening `note: `, as the one each surface prints while `graph.db` owes its rebuild and the one
/// `rigger validate` prints while the file counts events passed over, by cause, do.
fn run_read_only(root: &Path, args: &[&str]) -> (String, String, bool, Vec<String>) {
    let (out, err, ok) = run_rigger(root, args);
    let notes = common::cli::lines_where(&err, |line| line.starts_with("note: "))
        .into_iter()
        .map(str::to_string)
        .collect();
    (out, err, ok, notes)
}

/// Given a `graph.db` still at the old fold rule, every read-only surface - `rigger graph --show`,
/// `rigger validate` and `rigger dash --export` - says once that the rebuild is owed, naming
/// `rigger setup`, and writes nothing to the file; once `rigger setup` has paid it, none of them
/// says it again.
#[test]
fn the_read_only_surfaces_say_the_rebuild_is_owed_write_nothing_and_stop_once_setup_pays_it() {
    let store = ReleaseEraStore::new();
    common::cli::run_rigger_ok(store.root(), &["init"]);
    let surfaces = READ_ONLY_SURFACES;
    let note = rebuild_owed_note();
    let before = store.graph_bytes();
    for args in surfaces {
        let (out, err, ok, notes) = run_read_only(store.root(), args);
        assert!(
            ok,
            "rigger {} answers as it stands; stdout: {out} stderr: {err}",
            args.join(" ")
        );
        assert_eq!(
            err.matches(note.as_str()).count(),
            1,
            "rigger {} says once that the rebuild is owed; stdout: {out} stderr: {err}",
            args.join(" ")
        );
        assert_eq!(
            notes,
            vec![note.clone()],
            "rigger {} says no more of a file without the passed_over table; stderr: {err}",
            args.join(" ")
        );
        assert!(
            store.graph_bytes() == before,
            "rigger {} writes nothing to a graph.db owing its rebuild",
            args.join(" ")
        );
    }
    let (out, err, ok) = run_rigger(store.root(), surfaces[0]);
    assert!(
        ok && out.contains("src/f.rs::gone"),
        "graph --show answers from the graph as it stands; stdout: {out} stderr: {err}"
    );

    setup_ok(store.root(), &[("RIGGER_NPM", "true")]);
    for args in surfaces {
        let (_, err, _) = run_rigger(store.root(), args);
        assert!(
            !err.contains(rigger::contextgraph::REBUILD_OWED),
            "rigger {} says nothing once the rebuild is paid; stderr: {err}",
            args.join(" ")
        );
    }
}

/// Given a `graph.db` still at the old fold rule, an agent's `rigger mcp` session starts and
/// serves it as it stands: `rigger_graph` (both selectors) and `rigger_ground` refuse at once
/// naming `rigger setup`, while `rigger_emit` appends and writes nothing to `graph.db`. When
/// `rigger setup` pays the rebuild while that session runs, its next `rigger_graph` answers from
/// the rebuilt graph - the superseded link gone - and its next emit folds live, so the graph stays
/// exactly the whole log's.
#[test]
fn an_mcp_session_refuses_the_fold_dependent_tools_until_setup_pays_the_rebuild_it_serves() {
    let store = ReleaseEraStore::new();
    let before = store.graph_bytes();
    let mut mcp =
        common::mcp::McpSession::start_with(store.root(), &["mcp", "--spawn", "u/implementer#0"]);
    for (tool, arguments) in [
        ("rigger_graph", serde_json::json!({"around": "docs/f.md"})),
        ("rigger_graph", serde_json::json!({"show": "alpha"})),
        ("rigger_ground", serde_json::json!({"query": "alpha"})),
    ] {
        let answer = mcp.tool_call(tool, arguments.clone());
        assert_eq!(
            answer["error"]["message"],
            format!("{tool}: {}", rigger::contextgraph::REBUILD_OWED),
            "{tool} {arguments} refuses naming the rebuild; got: {answer}"
        );
    }
    let decision = |id: &str| {
        serde_json::json!({
            "type": "DecisionMade",
            "data": {"id": id, "summary": "s", "governs": ["src/f.rs::alpha"], "supersedes": ""},
        })
    };
    let emitted = mcp.tool_call("rigger_emit", decision("d-owed"));
    let log = store.log();
    assert_eq!(
        (log.len(), log.last().unwrap().type_.as_str()),
        (7, "DecisionMade"),
        "the emit appended to the log"
    );
    assert_eq!(
        emitted["result"]["structuredContent"],
        common::fixtures::emit_answer(
            log.last().unwrap().position,
            Some(&common::fixtures::graph_error(
                rigger::contextgraph::REBUILD_OWED
            ))
        ),
        "the emit succeeds and says it was not folded, naming the rebuild; got: {emitted}"
    );
    assert!(
        store.graph_bytes() == before,
        "neither the session nor its emit wrote to a graph.db owing its rebuild"
    );

    setup_ok(store.root(), &[("RIGGER_NPM", "true")]);
    let around = mcp.tool_call("rigger_graph", serde_json::json!({"around": "docs/f.md"}));
    let answer = around["result"].to_string();
    assert!(
        around.get("error").is_none()
            && answer.contains("src/a.rs")
            && !answer.contains("src/old.rs"),
        "the same session answers from the rebuilt graph; got: {around}"
    );
    let emitted = mcp.tool_call("rigger_emit", decision("d-paid"));
    assert_eq!(
        emitted["result"]["structuredContent"],
        common::fixtures::emit_answer(store.log().last().unwrap().position, None),
        "the emit succeeds and says it folded; got: {emitted}"
    );
    mcp.finish_cleanly();
    let (graph, fresh) = store.graph_and_a_fresh_fold_of_the_log();
    assert_eq!(
        graph, fresh,
        "the rebuild folded the first emit and the session folded the second live"
    );
}

// ---------------------------------------------------------------------------------------
// 4. The shadow rebuild's seams: its sources, its tail, and an emit that meets it
// ---------------------------------------------------------------------------------------

/// The positions of each batch `stream_past` hands for `log` past `after`, in batches of `batch`,
/// with the head handed beside each.
fn streamed_past(log: &[Event], after: u64, batch: usize) -> Vec<(Vec<u64>, u64)> {
    let mut handed = Vec::new();
    rigger::contextgraph::sqlite::stream_past(log, after, batch, &mut |events, head| {
        handed.push((crate::common::fixtures::positions_of(events), head));
        Ok(())
    })
    .unwrap();
    handed
}

/// `stream_past` hands exactly the events past `after` - an event AT `after` is already folded -
/// in batches of at most `batch`, each with the log's last position, and hands nothing (never an
/// empty batch) when nothing lies past `after` or the log is empty.
#[test]
fn stream_past_hands_only_what_lies_past_the_position_in_batches_with_the_head() {
    let dir = tempfile::tempdir().unwrap();
    let (backend, _) = store_with(
        dir.path(),
        &[(rigger::conductor::STREAM, two_generations_dropping_facts())],
    );
    let log = run_events(&backend, PROJECT);
    let p: Vec<u64> = crate::common::fixtures::positions_of(&log);
    assert_eq!(
        streamed_past(&log, 0, 2),
        vec![
            (vec![p[0], p[1]], p[4]),
            (vec![p[2], p[3]], p[4]),
            (vec![p[4]], p[4])
        ],
        "the whole log, in batches of two, each with the head"
    );
    assert_eq!(
        streamed_past(&log, p[1], 3),
        vec![(vec![p[2], p[3], p[4]], p[4])],
        "an event at the position is passed over; a batch that fits exactly is handed once"
    );
    assert_eq!(
        streamed_past(&log, p[4], 2),
        Vec::<(Vec<u64>, u64)>::new(),
        "nothing past the head hands nothing"
    );
    assert_eq!(
        streamed_past(&[], 7, 2),
        Vec::<(Vec<u64>, u64)>::new(),
        "an empty log hands nothing"
    );
    let failed = rigger::contextgraph::sqlite::stream_past(&log, 0, 2, &mut |_, _| {
        Err(rigger::contextgraph::Error("sink refused".to_string()))
    });
    assert_eq!(
        failed.unwrap_err().0,
        "sink refused",
        "a sink's failure stops the stream and is the stream's"
    );
}

/// The batches `backend`'s live selection of `project` hands past `after` in batches of `batch`,
/// as positions, with the head handed beside each.
fn selected_past(backend: &Store, project: &str, after: u64, batch: usize) -> Vec<(Vec<u64>, u64)> {
    let mut handed = Vec::new();
    backend
        .read_live_selection(
            &Namespaced::prefix_for(project),
            rigger::conductor::STREAM,
            &rigger::ingest::derived_index_identity(),
            after,
            batch,
            &mut |events, head| {
                handed.push((crate::common::fixtures::positions_of(events), head));
                Ok(())
            },
        )
        .unwrap();
    handed
}

/// The live selection is one project's run stream alone: another project's events interleaved in
/// the same file are never handed, the head is this stream's last position rather than the file's,
/// a selection that fills its last batch exactly is never followed by an empty one, and a project
/// with no run stream hands nothing.
#[test]
fn the_live_selection_is_one_projects_stream_with_that_streams_head() {
    let dir = tempfile::tempdir().unwrap();
    let (backend, db) = store_with(dir.path(), &[]);
    let pre_ledger = common::fixtures::PreLedgerStore {
        db: Path::new(&db),
        inner: &backend,
    };
    let other = "proj-other";
    let generation = |key: &str, line| {
        vec![common::fixtures::keyed_event(
            0,
            TYPE_CODE_ENTITY_EXTRACTED,
            common::fixtures::def_json("src/f.rs", "alpha", line, true),
            key,
            10,
        )]
    };
    for (project, key, line) in [
        (PROJECT, "gc/src/f.rs@h1#0", 1),
        (other, "gc/src/f.rs@h1#0", 1),
        (PROJECT, "gc/src/f.rs@h2#0", 2),
        (other, "gc/src/f.rs@h2#0", 2),
        (PROJECT, "gc/src/g.rs@h1#0", 3),
        (other, "gc/src/g.rs@h1#0", 3),
    ] {
        Namespaced::new(&pre_ledger, project)
            .append(
                rigger::conductor::STREAM,
                ExpectedRevision::Any,
                &generation(key, line),
            )
            .unwrap();
    }
    let mine = run_events(&backend, PROJECT);
    let theirs = run_events(&backend, other);
    let (m, t) = (
        crate::common::fixtures::positions_of(&mine),
        crate::common::fixtures::positions_of(&theirs),
    );
    assert!(
        t[2] > m[2],
        "the other project's last event lies past this one's"
    );
    assert_eq!(
        selected_past(&backend, PROJECT, 0, 2),
        vec![(vec![m[1], m[2]], m[2])],
        "h1 is shed, the two latest generations fill one batch exactly, and the head is this \
         stream's"
    );
    assert_eq!(
        selected_past(&backend, other, 0, 1),
        vec![(vec![t[1]], t[2]), (vec![t[2]], t[2])],
        "the other project's selection is its own"
    );
    assert_eq!(
        selected_past(&backend, "proj-absent", 0, 2),
        Vec::<(Vec<u64>, u64)>::new(),
        "a project with no run stream hands nothing"
    );
    assert_eq!(
        (
            live_positions(&backend, PROJECT, 2),
            live_positions(&backend, other, 1),
            live_positions(&backend, "proj-absent", 2)
        ),
        (vec![vec![m[1], m[2]]], vec![vec![t[1]], vec![t[2]]], vec![]),
        "the positions alone are the same selection, batch for batch"
    );
}

/// The batches of positions `backend`'s live selection of `project` hands in batches of `batch`.
fn live_positions(backend: &Store, project: &str, batch: usize) -> Vec<Vec<u64>> {
    let mut handed = Vec::new();
    backend
        .read_live_positions(
            &Namespaced::prefix_for(project),
            rigger::conductor::STREAM,
            &rigger::ingest::derived_index_identity(),
            batch,
            &mut |positions| {
                handed.push(positions.to_vec());
                Ok(())
            },
        )
        .unwrap();
    handed
}

/// The live selection is refused, before a single event is handed, under a policy that has not
/// declared which of its types re-assert a fact: a selection that cannot say which valid-time a
/// kept row carries would silently re-date facts in the graph it rebuilds.
#[test]
fn the_live_selection_refuses_a_policy_without_a_reasserting_partition_and_hands_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let (backend, _) = store_with(
        dir.path(),
        &[(rigger::conductor::STREAM, two_generations_dropping_facts())],
    );
    let shipped = rigger::ingest::derived_index_identity();
    let undeclared = ContentIdentity::new(shipped.meta_key().to_string(), shipped.types().to_vec());
    let mut handed = 0;
    let refused = backend.read_live_selection(
        &Namespaced::prefix_for(PROJECT),
        rigger::conductor::STREAM,
        &undeclared,
        0,
        2,
        &mut |events, _| {
            handed += events.len();
            Ok(())
        },
    );
    let message = refused.unwrap_err().to_string();
    assert!(
        message.contains(&format!(
            "the content-identity policy for {:?} has not declared which of its types re-assert \
             a fact in place",
            shipped.types()
        )),
        "the refusal names the undeclared partition; got: {message}"
    );
    assert_eq!(handed, 0, "nothing is handed before the refusal");
}

/// A rebuild interrupted part way, then resumed after the log gained a NEW generation of a file
/// whose superseded generation the shadow had already folded, reaches exactly the whole log's
/// graph: the resumed selection is the log's live selection NOW, and the generation fold retires
/// what the newer generation drops, as it does live.
fn a_resumed_rebuild_after_the_log_gained_a_generation_is_the_whole_logs(gained: Vec<Event>) {
    let dir = tempfile::tempdir().unwrap();
    let (backend, events_db) = store_with(
        dir.path(),
        &[(rigger::conductor::STREAM, two_generations_dropping_facts())],
    );
    let log = run_events(&backend, PROJECT);
    let (db, fresh) = (dir.path().join("graph.db"), dir.path().join("fresh.db"));
    a_pre_rule_graph_db(&db, &log[..1], "");
    let mut first = live_selection_of(&backend, PROJECT, 1);
    let mut batches = 0;
    let interrupted = rebuild(&db, &mut |after, sink| {
        first(after, &mut |events, head| {
            batches += 1;
            if batches == 2 {
                return source_interrupted();
            }
            sink(events, head)
        })
    });
    assert!(interrupted.is_err(), "the rebuild is interrupted");
    assert!(
        dir.path().join("graph.db.rebuild").exists(),
        "its first batch - h2's `alpha` - is committed in the shadow"
    );

    let pre_ledger = common::fixtures::PreLedgerStore {
        db: Path::new(&events_db),
        inner: &backend,
    };
    Namespaced::new(&pre_ledger, PROJECT)
        .append(rigger::conductor::STREAM, ExpectedRevision::Any, &gained)
        .unwrap();
    assert_eq!(
        rebuild(&db, &mut live_selection_of(&backend, PROJECT, 2)).unwrap(),
        Some(Rebuilt::default())
    );
    assert!(
        !dir.path().join("graph.db.rebuild").exists(),
        "the shadow is removed once copied in"
    );
    fold_in_batches(
        &fresh,
        PROJECT,
        std::slice::from_ref(&run_events(&backend, PROJECT)),
    );
    assert_eq!(
        identity_of(&db),
        identity_of(&fresh),
        "the resumed rebuild is the whole log's graph"
    );
}

rigger::test_cases! {
    /// `src/f.rs` moves on to a third content that adds `delta`.
    a_resumed_rebuild_folds_a_generation_the_log_gained_as_the_whole_log_does:
        a_resumed_rebuild_after_the_log_gained_a_generation_is_the_whole_logs(vec![
            f_rs_batch_def(0, "h3", 0, ("alpha", "function", 3), None, 30),
            keyed(TYPE_CODE_ENTITY_EXTRACTED, entity("delta", 5), "gc/src/f.rs@h3#1", 30),
        ]);
    /// `src/f.rs` returns to h1, bringing `gone` back.
    a_resumed_rebuild_folds_a_revert_the_log_gained_as_the_whole_log_does:
        a_resumed_rebuild_after_the_log_gained_a_generation_is_the_whole_logs(vec![
            f_rs_batch_def(0, "h1", 0, ("alpha", "function", 1), None, 30),
            keyed(TYPE_CODE_ENTITY_EXTRACTED, entity("gone", 9), "gc/src/f.rs@h1#1", 30),
        ]);
}

/// Whether `db` holds a table named `table`: `rebuild_cursor` while a rebuild's tail is not yet
/// folded; `lost_fold`, a record of a lost fold beside the ledger, never.
fn holds_table(db: &Path, table: &str) -> bool {
    exists(
        db,
        "SELECT EXISTS (SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1)",
        table,
    )
}

/// Given `rigger setup`'s rebuild has swapped its shadow in and is about to fold the tail the log
/// gained, when an agent runs `rigger emit`, then the emit neither waits on the rebuild nor skips
/// the fold: the file is current, so its event is folded at once; the tail then meets it exactly
/// once, drops its cursor, and the graph is exactly the whole log's.
#[test]
fn an_emit_while_the_rebuild_folds_its_tail_is_folded_at_once_and_met_exactly_once() {
    use rigger::contextgraph::sqlite::Projector;
    use std::sync::mpsc::channel;
    use std::time::{Duration, Instant};
    let store = ReleaseEraStore::new();
    let head_before = store.log().last().unwrap().position;
    let (paused_tx, paused) = channel();
    let (resume, resume_rx) = channel::<()>();
    let rebuilder = {
        let (events_db, graph_db) = (store.events_db.clone(), store.graph_db.clone());
        let project = store.project();
        std::thread::spawn(move || {
            let backend = Store::open(events_db.to_str().unwrap()).unwrap();
            let mut source = live_selection_of(&backend, &project, 1);
            let mut reads = 0;
            Projector::open(graph_db.to_str().unwrap(), &project)
                .unwrap()
                .rebuild(
                    &Projector::lock_rebuild(graph_db.to_str().unwrap()).unwrap(),
                    false,
                    &mut |after, sink| {
                        reads += 1;
                        if reads == 2 {
                            paused_tx.send(after).unwrap();
                            resume_rx.recv().unwrap();
                        }
                        source(after, sink)
                    },
                    &mut |_| Ok(None),
                    &mut |_| {},
                )
                .unwrap()
        })
    };
    let tail_after = paused.recv().unwrap();
    assert_eq!(
        tail_after, head_before,
        "the tail reads past the last position the shadow folded"
    );
    assert_eq!(
        (
            user_version(&store.graph_db),
            holds_table(&store.graph_db, "rebuild_cursor")
        ),
        (PROJECTION_VERSION, true),
        "the rebuilt file is swapped in and still owes its tail"
    );

    let started = Instant::now();
    let (out, err, ok) = run_rigger(
        store.root(),
        &[
            "emit",
            "DecisionMade",
            r#"{"id":"d-tail","summary":"s","governs":["src/f.rs::alpha"],"supersedes":""}"#,
        ],
    );
    let emitted = store.log().last().unwrap().position;
    assert_eq!(
        (ok, out.as_str()),
        (
            true,
            format!(
                "emitted DecisionMade (position {emitted}) and folded it into the context graph\n"
            )
            .as_str()
        ),
        "an emit during the tail folds its event; stderr: {err}"
    );
    assert!(
        started.elapsed() < Duration::from_secs(4),
        "the emit did not wait on the rebuild (the busy timeout is 5s)"
    );
    assert!(
        applied(&store.graph_db, emitted),
        "the emitted event is folded before the tail runs"
    );

    resume.send(()).unwrap();
    assert_eq!(
        rebuilder.join().unwrap(),
        Some(Rebuilt::default()),
        "the rebuild ran"
    );
    assert!(
        !holds_table(&store.graph_db, "rebuild_cursor"),
        "the tail is folded and its cursor dropped"
    );
    let (graph, fresh) = store.graph_and_a_fresh_fold_of_the_log();
    assert_eq!(
        graph, fresh,
        "the tail met the emitted event exactly once: the graph is the whole log's"
    );
}

/// Run `rigger emit DecisionMade` in `root` for a decision `id` governing `src/f.rs`, returning
/// its stdout, its stderr and whether it exited successfully.
fn emit_decision(root: &Path, id: &str) -> (String, String, bool) {
    let args = format!(r#"{{"id":"{id}","summary":"s","governs":["src/f.rs"],"supersedes":""}}"#);
    run_rigger(root, &["emit", "DecisionMade", &args])
}

/// Emit the decision `id` in `root` ([`emit_decision`]), which must succeed and fold it into
/// `graph.db` ([`EMIT_FOLDED_ENDING`]).
fn emit_folding(root: &Path, id: &str) {
    let (out, err, ok) = emit_decision(root, id);
    assert!(
        ok && out.ends_with(EMIT_FOLDED_ENDING),
        "the emit of {id} folds into graph.db; stdout: {out} stderr: {err}"
    );
}

/// Emit the decision `d-first` in `root` ([`emit_decision`]), which must succeed: a test's first
/// emit, creating its `graph.db` when no setup has.
fn emit_the_first_decision(root: &Path) {
    let (_, err, ok) = emit_decision(root, "d-first");
    assert!(ok, "the first emit succeeds; stderr: {err}");
}

/// Run a cold-start `rigger setup` in `root` - minting the project's identity and creating
/// `graph.db` with its `caught_up` cursor at 0 beside the empty log, so no identity migration
/// waits for a later setup - then emit the first decision into it ([`emit_the_first_decision`]).
fn emit_the_first_decision_after_setup(root: &Path) {
    setup_ok(root, &[("RIGGER_NPM", "true")]);
    emit_the_first_decision(root);
}

/// Given a current `graph.db` another writer holds locked past the busy timeout, when an agent
/// runs `rigger emit`, then the event is on the log and the emit reports its position, but it does
/// not claim to have folded an event its fold failed to write: the graph is current, so no
/// rebuild will ever re-derive it, and a claimed fold would hide a permanent divergence.
#[test]
fn an_emit_whose_fold_fails_does_not_claim_it_folded() {
    let dir = temp_store_project();
    let root = dir.path();
    let (out, err, ok) = emit_decision(root, "d-first");
    assert!(
        ok && out.ends_with("and folded it into the context graph\n"),
        "an emit into an unlocked graph folds; stdout: {out} stderr: {err}"
    );
    let graph_db = rigger_file(root, "graph.db");
    let (out, err, ok) = with_graph_locked(&graph_db, || emit_decision(root, "d-locked"));
    let log = read_run_events(root);
    let last = log.last().unwrap();
    assert_eq!(
        (log.len(), last.type_.as_str()),
        (2, "DecisionMade"),
        "the event is on the log"
    );
    assert!(
        !applied(&graph_db, last.position),
        "the locked fold wrote nothing"
    );
    assert_eq!(
        (ok, out),
        (
            true,
            format!(
                "emitted DecisionMade (position {}); {}\n",
                last.position,
                not_folded(&lost_fold(&graph_error(DATABASE_IS_LOCKED)))
            )
        ),
        "the emit reports the position and the fold it could not make, with the reason; \
         stderr: {err}"
    );
}

/// Given a current `graph.db` another writer holds locked past the busy timeout, when an agent's
/// `rigger mcp` session calls `rigger_emit`, then the event is on the log and the tool answers its
/// position and that it was not folded, with the reason: the graph is current, so no rebuild will
/// ever re-derive the event, and an answer that looked like a fold would hide the divergence.
#[test]
fn an_mcp_emit_whose_fold_fails_answers_not_folded_with_the_reason() {
    let dir = temp_store_project();
    let root = dir.path();
    emit_the_first_decision(root);
    let decision = |id: &str| {
        serde_json::json!({
            "type": "DecisionMade",
            "data": {"id": id, "summary": "s", "governs": ["src/f.rs"], "supersedes": ""},
        })
    };
    let mut mcp = common::mcp::McpSession::start_with(root, &["mcp", "--spawn", "u/implementer#0"]);
    let free = mcp.tool_call("rigger_emit", decision("d-free"));
    let free_at = run_log_head(root);
    assert_eq!(
        free["result"]["structuredContent"],
        common::fixtures::emit_answer(free_at, None),
        "an emit into an unlocked graph folds; got: {free}"
    );

    let graph_db = rigger_file(root, "graph.db");
    let locked = with_graph_locked(&graph_db, || {
        mcp.tool_call("rigger_emit", decision("d-locked"))
    });
    mcp.finish_cleanly();

    let log = read_run_events(root);
    let last = log.last().unwrap();
    assert_eq!(
        (log.len(), last.type_.as_str()),
        (3, "DecisionMade"),
        "the locked emit's event is on the log"
    );
    assert_eq!(
        locked["result"]["structuredContent"],
        common::fixtures::emit_answer(
            last.position,
            Some(&common::fixtures::lost_fold(&common::fixtures::graph_error(
                common::fixtures::DATABASE_IS_LOCKED
            )))
        ),
        "the tool answers the position and the fold it could not make, with the reason; got: {locked}"
    );
    assert_eq!(
        (
            applied(&graph_db, free_at),
            applied(&graph_db, last.position)
        ),
        (true, false),
        "the free emit folded and the locked fold wrote nothing"
    );
}

/// Given a `graph.db` that cannot be opened as a graph (its bytes are not a database), when an
/// agent runs `rigger emit`, then the emit still succeeds with its event on the log and reports
/// the fold it could not make, with the reason, and leaves the unreadable file exactly as it was:
/// an event already durably appended is never reported as a failed emit.
#[test]
fn an_emit_into_a_graph_it_cannot_open_is_on_the_log_and_reported_not_folded() {
    let dir = temp_store_project();
    let root = dir.path();
    emit_the_first_decision(root);
    let graph_db = rigger_file(root, "graph.db");
    let garbage = b"this file is not a sqlite database, only text standing in for one".repeat(64);
    std::fs::write(&graph_db, &garbage).unwrap();

    let (out, err, ok) = emit_decision(root, "d-unopenable");
    let log = read_run_events(root);
    let last = log.last().unwrap();
    assert_eq!(
        (log.len(), last.type_.as_str()),
        (2, "DecisionMade"),
        "the event is on the log"
    );
    assert_eq!(
        (ok, out),
        (
            true,
            format!(
                "emitted DecisionMade (position {}); {}\n",
                last.position,
                not_folded(&graph_error(NOT_A_DATABASE))
            )
        ),
        "the emit succeeds and reports the fold it could not make, with the reason; stderr: {err}"
    );
    assert_eq!(
        std::fs::read(&graph_db).unwrap(),
        garbage,
        "the unopenable graph.db is left exactly as it was"
    );
}

/// Given history recorded before the project minted its durable identity, when a command that
/// opens the store (a bare `rigger reset`) runs the one-time identity migration, then the
/// migration's own `DecisionMade` goes through the one emit core into the graph it was wired with:
/// it is on the log under the minted identity AND folded into the current `graph.db`, where it
/// resolves - no rebuild will ever re-derive an event a current graph missed.
#[test]
fn the_identity_migrations_decision_is_folded_into_the_graph_it_migrates() {
    use rigger::contextgraph::Projection;

    let dir = temp_store_project();
    let root = dir.path();
    let (legacy, minted) = legacy_history_then_minted_identity(root);

    let (_, err, ok) = run_rigger(root, &["reset"]);
    let log = read_run_events(root);
    let migration = format!("identity-migration-{minted}");
    let last = log.last().unwrap();
    assert!(
        ok && err.contains(&migration_report(
            &legacy,
            &minted,
            last.position,
            " and folded it into the context graph"
        )),
        "the bare reset migrates the one legacy stream and reports its decision's fold; \
         stderr: {err}"
    );
    assert_eq!(
        (
            log.len(),
            last.type_.as_str(),
            serde_json::from_slice::<serde_json::Value>(&last.data).unwrap()["id"].clone()
        ),
        (2, "DecisionMade", serde_json::json!(migration)),
        "the migration's decision follows the legacy one on the minted run stream"
    );
    let graph_db = rigger_file(root, "graph.db");
    assert_eq!(
        (
            applied(&graph_db, log[0].position),
            applied(&graph_db, last.position)
        ),
        (true, true),
        "both the legacy emit and the migration's decision are folded"
    );
    assert_eq!(
        common::cli::open_graph(root).resolve(&migration).unwrap(),
        Some(migration.clone()),
        "the migration's decision resolves in the graph under the minted identity"
    );
}

/// Record a decision under the legacy basename namespace of the project at `root`, creating its
/// store and graph, then mint its durable identity with `rigger init`, leaving that history for
/// the one-time identity migration: the legacy and the minted identity.
fn legacy_history_then_minted_identity(root: &Path) -> (String, String) {
    let (_, err, ok) = emit_decision(root, "d-legacy");
    assert!(
        ok,
        "the legacy emit creates the store and graph; stderr: {err}"
    );
    let legacy = run_stream_identity(root);
    let (_, err, ok) = run_rigger(root, &["init"]);
    assert!(ok, "rigger init mints the identity; stderr: {err}");
    let minted = run_stream_identity(root);
    assert_ne!(minted, legacy, "init minted a distinct identity");
    (legacy, minted)
}

/// The stderr line a bare `rigger reset` prints when it migrates one legacy stream `legacy` ->
/// `minted` and records its decision at `position`, ending in what became of that decision's fold.
fn migration_report(legacy: &str, minted: &str, position: u64, fold: &str) -> String {
    format!(
        "rigger: migrated project identity - renamed 1 stream(s) from the legacy namespace \
         {legacy:?} to the minted identity {minted:?} (.rigger/project.id); recorded its \
         decision (position {position}){fold}\n"
    )
}

/// Given a `graph.db` still at the old fold rule, when a courier runs `rigger result`, then the
/// result is on the log and the courier exits successfully, but it says the result was not folded,
/// naming `rigger setup`, and `graph.db` is left byte for byte as it was: a command whose job is
/// to append skips the fold while the rebuild is owed and says so.
#[test]
fn a_result_at_the_old_rule_is_on_the_log_and_says_it_skipped_the_fold() {
    let store = ReleaseEraStore::new();
    let before = store.graph_bytes();
    let (out, err, ok) = run_rigger(store.root(), &["result", "u/implementer#0", "did the work"]);
    let log = store.log();
    let last = log.last().unwrap();
    assert_eq!(
        (ok, log.len(), last.type_.as_str()),
        (true, 7, "SpawnResult"),
        "the result is appended to the log and the courier succeeds; stderr: {err}"
    );
    assert_eq!(
        out,
        format!(
            "recorded result for u/implementer#0 (position {})\n{}\n",
            last.position,
            not_folded(&graph_error(rigger::contextgraph::REBUILD_OWED))
        ),
        "the result says it skipped the fold and names the rebuild"
    );
    assert!(
        store.graph_bytes() == before,
        "a result at the old rule writes nothing to graph.db"
    );
}

/// Given a current `graph.db` another writer holds locked past the busy timeout, when a courier
/// runs `rigger result`, then the result is on the log and the courier exits successfully, but it
/// says the result was not folded, with the reason: the graph is current, so no rebuild will ever
/// re-derive it, and a silent miss would keep an adjudicator's discarded findings in grounding.
#[test]
fn a_result_whose_fold_fails_says_it_was_not_folded() {
    let dir = temp_store_project();
    let root = dir.path();
    let (out, err, ok) = emit_decision(root, "d-first");
    assert!(
        ok && out.ends_with("and folded it into the context graph\n"),
        "an emit into an unlocked graph folds; stdout: {out} stderr: {err}"
    );
    let (out, err, ok) = run_rigger(root, &["result", "u/implementer#0", "first"]);
    let free = run_log_head(root);
    assert_eq!(
        (ok, out),
        (
            true,
            format!("recorded result for u/implementer#0 (position {free})\n")
        ),
        "a result into an unlocked graph folds silently; stderr: {err}"
    );
    let graph_db = rigger_file(root, "graph.db");
    let (out, err, ok) = with_graph_locked(&graph_db, || {
        run_rigger(root, &["result", "v/implementer#0", "second"])
    });
    let log = read_run_events(root);
    let last = log.last().unwrap();
    assert_eq!(
        (log.len(), last.type_.as_str()),
        (3, "SpawnResult"),
        "the locked result is on the log"
    );
    assert_eq!(
        (applied(&graph_db, free), applied(&graph_db, last.position)),
        (true, false),
        "the free result folded and the locked fold wrote nothing"
    );
    assert_eq!(
        (ok, out),
        (
            true,
            format!(
                "recorded result for v/implementer#0 (position {})\n{}\n",
                last.position,
                not_folded(&lost_fold(&graph_error(DATABASE_IS_LOCKED)))
            )
        ),
        "the result reports the fold it could not make, with the reason; stderr: {err}"
    );
}

/// Given a store still named after its directory whose `graph.db` is at the old fold rule, when
/// the operator mints the identity and runs a bare `rigger reset`, then the migration renames the
/// legacy stream and records its decision, but says that decision was not folded, naming the
/// rebuild it owes, and `graph.db` does not hold it until `rigger setup` rebuilds from the log.
#[test]
fn an_identity_migration_into_a_graph_that_owes_its_rebuild_says_its_decision_was_not_folded() {
    let store = ReleaseEraStore::with_identity(false);
    let legacy = store.project();
    let (_, err, ok) = run_rigger(store.root(), &["init"]);
    assert!(ok, "rigger init mints the identity; stderr: {err}");
    let minted = store.project();
    assert_ne!(minted, legacy, "init minted a distinct identity");

    let (out, err, ok) = run_rigger(store.root(), &["reset"]);
    let log = store.log();
    let last = log.last().unwrap();
    assert_eq!(
        out.lines().next(),
        Some(format!("--runs: {}", rigger::contextgraph::REBUILD_OWED).as_str()),
        "the menu counts nothing prunable from a graph that owes its rebuild, and says so; \
         stderr: {err}"
    );
    assert_eq!(
        (
            ok,
            log.len(),
            last.type_.as_str(),
            serde_json::from_slice::<serde_json::Value>(&last.data).unwrap()["id"].clone()
        ),
        (
            true,
            7,
            "DecisionMade",
            serde_json::json!(format!("identity-migration-{minted}"))
        ),
        "the migration's decision follows the six moved events on the minted run stream; \
         stderr: {err}"
    );
    assert!(
        err.contains(&migration_report(
            &legacy,
            &minted,
            last.position,
            &format!(
                "; {}",
                not_folded(&graph_error(rigger::contextgraph::REBUILD_OWED))
            )
        )),
        "the migration reports that its decision skipped the owed graph, with the reason; \
         stderr: {err}"
    );
    assert!(
        !applied(&store.graph_db, last.position),
        "the owed graph.db does not hold the migration's decision"
    );

    setup_ok(store.root(), &[("RIGGER_NPM", "true")]);
    assert!(
        applied(&store.graph_db, last.position),
        "setup's rebuild folds the migration's decision from the log"
    );
}

/// Given a current `graph.db` whose fold of an emit was lost to another writer's lock, when the
/// agent emits again, then the graph owes its rebuild: the next emit appends and says it was not
/// folded because the rebuild is owed, a read-only inspection says the rebuild is owed, and once
/// `rigger setup` has paid it every event of the log is folded - the lost one included - and an
/// emit folds again.
#[test]
fn a_fold_lost_to_a_lock_marks_the_graph_owed_until_setup_rebuilds_it() {
    let dir = temp_store_project();
    let root = dir.path();
    emit_the_first_decision(root);
    let graph_db = rigger_file(root, "graph.db");
    let (out, err, _) = with_graph_locked(&graph_db, || emit_decision(root, "d-locked"));
    assert!(
        out.ends_with(&format!(
            "; {}\n",
            not_folded(&lost_fold(&graph_error(DATABASE_IS_LOCKED)))
        )),
        "the locked emit's fold is lost; stdout: {out} stderr: {err}"
    );
    let lost = run_log_head(root);

    let (out, err, ok) = emit_decision(root, "d-after");
    let after = run_log_head(root);
    assert_eq!(
        (ok, out),
        (
            true,
            format!(
                "emitted DecisionMade (position {after}); {}\n",
                not_folded(&graph_error(rigger::contextgraph::REBUILD_OWED))
            )
        ),
        "the graph now owes its rebuild, so the next emit says so; stderr: {err}"
    );
    let (_, err, ok) = run_rigger(root, &["graph", "--around", "src/f.rs"]);
    assert!(
        ok && err.contains(&rebuild_owed_note()),
        "a read-only inspection says the rebuild is owed; stderr: {err}"
    );
    let owed_log = read_run_events(root).len();
    let (out, err, ok) = run_rigger(root, &["reset", "--derived"]);
    assert_eq!(
        (ok, read_run_events(root).len()),
        (false, owed_log),
        "the migration refuses a graph a lost fold left owed and sheds nothing; stdout: {out}"
    );
    assert!(
        err.contains(&rigger::contextgraph::rebuild_owed_refusal(
            "reset --derived"
        )),
        "the migration's refusal names `rigger setup`; stderr: {err}"
    );

    let out = setup_ok(root, &[("RIGGER_NPM", "true")]);
    assert_eq!(
        out.lines()
            .filter(|l| l.starts_with("rebuilding graph.db"))
            .collect::<Vec<_>>(),
        vec![LOST_FOLD_REBUILD_LINE],
        "setup names the cause the graph owes its rebuild for - the lost fold, not an older \
         rule; stdout: {out}"
    );
    assert_eq!(
        (applied(&graph_db, lost), applied(&graph_db, after)),
        (true, true),
        "setup's rebuild folds the lost event and the one emitted while owed"
    );
    emit_folding(root, "d-rebuilt");
    let (out, err, ok) = run_rigger(root, &["reset", "--derived"]);
    assert_eq!(
        (ok, out),
        (
            true,
            format!("{NOTHING_TO_SHED_LINE}{LOG_LEFT_AS_IT_STANDS_LINE}")
        ),
        "once setup has paid the rebuild the migration runs, over a log holding no derived \
         event; stderr: {err}"
    );
}

/// Given a `graph.db` a lost fold left owed, on a project whose identity is minted, so no identity
/// migration waits for the next setup, when the operator removes the graph file and runs `rigger
/// setup`, then setup drops the mark the removed file left behind and rebuilds a fresh graph from
/// the log - its own open creates the file, an empty ledger beside a non-empty log - the lost event
/// folded with the rest, so the next emit folds into it rather than inheriting a debt that file no
/// longer carries.
#[test]
fn an_owed_mark_whose_graph_is_removed_is_dropped_by_setup_and_the_next_emit_folds() {
    let dir = temp_store_project();
    let root = dir.path();
    emit_the_first_decision_after_setup(root);
    let graph_db = rigger_file(root, "graph.db");
    let mark = rigger_file(root, "graph.db.owed");
    with_graph_locked(&graph_db, || emit_decision(root, "d-locked"));
    let locked = run_log_head(root);
    assert!(mark.exists(), "the lost fold marks the graph owed");
    remove_the_store_file(root, "graph.db");

    setup_ok(root, &[("RIGGER_NPM", "true")]);
    assert_eq!(
        (mark.exists(), applied(&graph_db, locked)),
        (false, true),
        "setup drops the mark of the removed graph file and rebuilds a fresh one from the log"
    );
    emit_folding(root, "d-fresh");
}

/// Append a `DecisionMade` whose payload is `data` to `root`'s run stream straight through the
/// store, behind every fold - the shape of an event whose process died before its fold, or one
/// that never met the emit surface's shape check - and answer its log position.
fn append_unfolded_decision(root: &Path, data: &[u8]) -> u64 {
    let backend = Store::open(rigger_file(root, "events.db").to_str().unwrap()).unwrap();
    Namespaced::new(&backend, &run_stream_identity(root))
        .append(
            rigger::conductor::STREAM,
            ExpectedRevision::Any,
            &[Event::new("DecisionMade", data.to_vec())],
        )
        .unwrap()
        .one("the unfolded decision")
        .unwrap()
}

/// Append the decision `id`, governing `src/f.rs`, to `root`'s run stream behind every fold
/// ([`append_unfolded_decision`]) and answer its log position.
fn append_unfolded_decision_named(root: &Path, id: &str) -> u64 {
    let decision = decision_json(id, "s", &["src/f.rs"], "");
    append_unfolded_decision(root, decision.to_string().as_bytes())
}

/// The `DecisionMade` payload of the decision `d-boom`, whose node a graph file refuses while
/// [`the_disk_gives_out_at_d_boom`] stands in it.
const D_BOOM: &[u8] = br#"{"id":"d-boom","summary":"s","governs":["src/f.rs"],"supersedes":""}"#;

/// Fold a decision into `root`'s `graph.db` above every event it misses, then backdate the log past
/// the catch-up horizon ([`backdate_the_log`]): each missing event is then an old hole below an
/// applied position, which the next `rigger setup`'s seeding of a file without `caught_up` finds a
/// lost fold (spec 124, HORIZON IN TESTS).
fn bury_below_an_applied_event(root: &Path) {
    emit_decision(root, "d-above");
    backdate_the_log(root);
}

/// Given `root`'s `graph.db`, without `caught_up`, misses the event at log position `lost` below an
/// applied position and older than the catch-up horizon ([`bury_below_an_applied_event`]), when the
/// operator runs `rigger setup` in a fresh process, then setup names the missing event as the
/// cause, its rebuild folds it, the next emit folds, and a second `rigger setup` owes nothing - the
/// rebuild's drop set the file's cursor, so that setup's seeding reads nothing.
fn the_next_setup_pays_the_missing_event(root: &Path, lost: u64) {
    let graph_db = rigger_file(root, "graph.db");
    let out = setup_ok(root, &[("RIGGER_NPM", "true")]);
    assert_eq!(
        (
            out.lines()
                .filter(|l| l.starts_with("rebuilding graph.db"))
                .collect::<Vec<_>>(),
            applied(&graph_db, lost)
        ),
        (vec![LOST_FOLD_REBUILD_LINE], true),
        "setup finds the event missing from the ledger, says so, and its rebuild folds it; \
         stdout: {out}"
    );
    assert_eq!(
        out.lines()
            .filter(|l| l.starts_with("passed over "))
            .collect::<Vec<_>>(),
        Vec::<&str>::new(),
        "a rebuild whose every payload folds passes over nothing; stdout: {out}"
    );
    emit_folding(root, "d-rebuilt");
    let (out, err, ok) = run_rigger_envs(root, &["setup"], &[("RIGGER_NPM", "true")]);
    assert!(
        ok && !out.contains("graph.db"),
        "a paid rebuild is not owed or reported again; stdout: {out} stderr: {err}"
    );
}

/// Given a current `graph.db`, when an agent's `rigger emit` loses its fold to another writer's
/// lock while the directory the owed mark lives in is not writable, then the emit appends and
/// says it did not fold, naming the mark it could not write, and records nothing else - no mark,
/// no table in the file - yet the loss is not forgotten with the process: once a later event folds
/// above it and it is older than the catch-up horizon, the next `rigger setup` finds the event
/// missing from the graph's ledger and pays it.
#[test]
fn a_fold_lost_where_its_mark_cannot_be_written_is_paid_by_the_next_setup_from_the_ledger() {
    use std::os::unix::fs::PermissionsExt;
    let dir = temp_store_project();
    let root = dir.path();
    emit_the_first_decision(root);
    let graph_db = rigger_file(root, "graph.db");
    let mark = rigger_file(root, "graph.db.owed");
    let rigger_dir = graph_db.parent().unwrap().to_path_buf();
    // Both files' write-ahead files stay in place while these are open, so the append and the
    // lock need no new file in the directory the mark cannot be written to.
    let keepers = [rigger_file(root, "events.db"), graph_db.clone()].map(|db| {
        let keeper = rusqlite::Connection::open(db).unwrap();
        keeper
            .query_row("SELECT COUNT(*) FROM sqlite_master", [], |r| {
                r.get::<_, i64>(0)
            })
            .unwrap();
        keeper
    });
    std::fs::set_permissions(&rigger_dir, std::fs::Permissions::from_mode(0o555)).unwrap();
    let (out, err, ok) = with_graph_locked(&graph_db, || emit_decision(root, "d-lost"));
    std::fs::set_permissions(&rigger_dir, std::fs::Permissions::from_mode(0o755)).unwrap();
    drop(keepers);
    let lost = run_log_head(root);
    let prefix = format!(
        "emitted DecisionMade (position {lost}); {}",
        not_folded(&format!(
            "{}; the mark that graph.db owes its rebuild was not written (",
            graph_error(DATABASE_IS_LOCKED)
        ))
    );
    let suffix = lost_fold("graph.db.owed: Permission denied (os error 13))") + "\n";
    assert_eq!(
        (
            ok,
            out.starts_with(&prefix) && out.ends_with(&suffix),
            mark.exists(),
            holds_table(&graph_db, "lost_fold"),
            applied(&graph_db, lost)
        ),
        (true, true, false, false, false),
        "the emit appends, says it did not fold and names the unwritten mark, and records the \
         loss nowhere but in the ledger it misses; stdout: {out} stderr: {err}"
    );
    bury_below_an_applied_event(root);
    the_next_setup_pays_the_missing_event(root, lost);
}

/// Given an event on the log that no fold ever reached - its process died between the append and
/// the fold, so nothing ran to record the loss - once a later event folds above it and it is older
/// than the catch-up horizon, when the operator runs `rigger setup`, then setup finds it missing
/// from the graph's ledger and pays it.
#[test]
fn an_event_appended_but_never_folded_is_paid_by_the_next_setup_from_the_ledger() {
    let dir = temp_store_project();
    let root = dir.path();
    emit_the_first_decision(root);
    let lost = append_unfolded_decision(
        root,
        br#"{"id":"d-crashed","summary":"s","governs":["src/f.rs"],"supersedes":""}"#,
    );
    assert!(
        !applied(&rigger_file(root, "graph.db"), lost),
        "the event is on the log and not in the graph"
    );
    bury_below_an_applied_event(root);
    the_next_setup_pays_the_missing_event(root, lost);
}

/// Given a project whose event log is a KurrentDB server, the first `rigger setup` creates
/// `graph.db` and seeds its `caught_up` cursor at 0 beside the empty stream. Given a `graph.db` on
/// this machine without `caught_up` - as a fold made it before the cursor existed - that misses an
/// event another writer of that server appended (a second machine sharing the log) below one this
/// machine folded, when the operator runs `rigger setup`, then setup's seeding reads the server's
/// positions against the graph's ledger - the server's stamps are taken on another host's clock,
/// so setup hands the seeding none and the missing event counts as older than the catch-up
/// horizon - names it as the cause, rebuilds `graph.db` from the server's whole run stream (a
/// server-backed log has no compaction plan, so its live selection is the stream as it stands),
/// reports its progress and its prune, leaves the graph a fold of that stream with its cursor at
/// the stream's head, and a second setup owes nothing. When that writer appends one more event,
/// the next setup folds it forward from the cursor (spec 124, ONE PROCEDURE), says it caught up
/// with the one event and moves the cursor to it, and a setup after it owes nothing. Gracefully
/// skipped when no container runtime is reachable.
#[test]
fn setup_rebuilds_a_graph_db_missing_an_event_another_writer_appended_to_the_server_log() {
    common::fixtures::with_kurrentdb(|conn| {
        let dir = common::cli::temp_project();
        let root = dir.path();
        let server = [("KURRENTDB_CONN", conn), ("RIGGER_NPM", "true")];
        let setup = || run_rigger_envs(root, &["setup"], &server);
        let decision = |id: &str| {
            format!(r#"{{"id":"{id}","summary":"s","governs":["src/f.rs"],"supersedes":""}}"#)
        };
        let rebuild_lines = |out: &str| -> Vec<String> {
            out.lines()
                .filter(|l| {
                    ["rebuil", "pruned ", "passed over "]
                        .iter()
                        .any(|p| l.starts_with(p))
                })
                .map(str::to_string)
                .collect()
        };
        let owes_nothing = |why: &str| {
            let (out, err, ok) = setup();
            assert_eq!(
                (ok, out.contains("graph.db"), rebuild_lines(&out)),
                (true, false, Vec::<String>::new()),
                "{why}; stdout: {out} stderr: {err}"
            );
        };
        let (out, err, ok) = setup();
        assert_eq!(
            (ok, rebuild_lines(&out), caught_up_at(&rigger_file(root, "graph.db"))),
            (true, Vec::<String>::new(), Some(0)),
            "setup mints the identity, creates graph.db and seeds its cursor at 0 beside the empty \
             stream, rebuilding nothing; stdout: {out} stderr: {err}"
        );
        let project = run_stream_identity(root);
        let backend = rigger::eventstore::kurrentdb::Store::open(conn).unwrap();
        let log = Namespaced::new(&backend, &project);
        let other_writer_appends = |id: &str| {
            log.append(
                rigger::conductor::STREAM,
                ExpectedRevision::Any,
                &[Event::new("DecisionMade", decision(id).into_bytes())],
            )
            .unwrap()
            .one("the other writer's decision")
            .unwrap()
        };
        let lost = other_writer_appends("d-other");
        let graph_db = rigger_file(root, "graph.db");
        assert!(
            !applied(&graph_db, lost),
            "the other writer's event is on the server and not in this machine's graph"
        );

        let (out, err, ok) = run_rigger_envs(
            root,
            &["emit", "DecisionMade", &decision("d-first")],
            &server,
        );
        assert!(
            ok && out.ends_with(EMIT_FOLDED_ENDING),
            "the emit appends to the server and folds into the local graph.db; stdout: {out} \
             stderr: {err}"
        );

        // The file as a fold made it before the cursor existed: its first seeding reads the
        // server's positions against its ledger.
        drop_the_cursor(&graph_db);
        let (out, err, ok) = setup();
        let stream = log
            .read_stream(
                rigger::conductor::STREAM,
                0,
                rigger::eventstore::Direction::Forward,
            )
            .unwrap();
        let head = stream.last().unwrap().position;
        let scratch = tempfile::tempdir().unwrap();
        let folded = fold_in_batches(
            &scratch.path().join("graph.db"),
            &project,
            std::slice::from_ref(&stream),
        );
        assert_eq!(
            (
                ok,
                rebuild_lines(&out),
                stream.len(),
                applied(&graph_db, lost),
                whole_graph(root),
                caught_up_at(&graph_db)
            ),
            (
                true,
                vec![
                    LOST_FOLD_REBUILD_LINE.to_string(),
                    rebuilt_through_the_head(2, head),
                    REBUILT_LINE.to_string(),
                    pruned_from_the_rebuilt_graph(0, 0),
                ],
                2,
                true,
                folded,
                Some(head)
            ),
            "setup finds the other writer's event missing from the ledger, rebuilds from the \
             server's whole run stream in one batch, prunes nothing before a run starts, and the \
             graph is the fold of that stream with its cursor at the head; stdout: {out} \
             stderr: {err}"
        );
        owes_nothing(
            "the rebuilt ledger holds every position the server hands, so the rebuild is paid",
        );

        // The rebuilt file carries its cursor: what the other writer appends next is folded
        // forward from it, with no rebuild.
        let later = other_writer_appends("d-later");
        let (out, err, ok) = setup();
        assert_eq!(
            (
                ok,
                common::cli::rebuild_lines(&out),
                applied(&graph_db, later),
                caught_up_at(&graph_db)
            ),
            (true, vec![caught_up_line(1)], true, Some(later)),
            "setup folds the other writer's later event forward from the cursor and moves the \
             cursor to it, rebuilding nothing; stdout: {out} stderr: {err}"
        );
        owes_nothing("the cursor stands at the server's head, so setup folds nothing more");
    });
}

/// Given events on the log whose payload the fold rejects - appended without the emit surface's
/// shape check - below an event the graph folded and older than the catch-up horizon
/// ([`bury_below_an_applied_event`]), when the operator runs `rigger setup`, then setup finds them
/// missing from the graph's ledger, rebuilds, and says it passed them over, recording each as
/// folded with its cause so the next setup owes nothing for them and only says again what the file
/// passed over.
#[test]
fn setup_says_how_many_events_its_rebuild_passed_over_for_a_payload_the_fold_rejects() {
    let dir = temp_store_project();
    let root = dir.path();
    emit_the_first_decision(root);
    let rejected = [
        append_unfolded_decision(root, common::fixtures::POISON),
        append_unfolded_decision(
            root,
            br#"{"id":"d-trailing","summary":"s","governs":["src/t.rs"],"supersedes":""} x"#,
        ),
    ];
    bury_below_an_applied_event(root);

    let out = setup_ok(root, &[("RIGGER_NPM", "true")]);
    assert_eq!(
        (
            common::cli::rebuild_lines(&out),
            common::cli::applied_each(&rigger_file(root, "graph.db"), &rejected),
        ),
        (common::cli::rebuilt_passing_over(2, 0, 0), vec![true, true]),
        "setup rebuilds, and says the two events it passed over - a malformed payload and one \
         with trailing bytes; stdout: {out}"
    );
    let (out, err, ok) = run_rigger_envs(root, &["setup"], &[("RIGGER_NPM", "true")]);
    assert_eq!(
        (ok, common::cli::rebuild_lines(&out)),
        (true, vec![common::cli::passed_over_line(2, 0, 0)]),
        "the passed-over events are not owed again, and setup says again what the file passed \
         over; stdout: {out} stderr: {err}"
    );
}

/// Given a `graph.db` that passed nothing over, no read-only surface ([`READ_ONLY_SURFACES`])
/// prints a note. Given one whose rebuild passed two events over - payloads the fold rejects, below
/// an event it folded and older than the catch-up horizon ([`bury_below_an_applied_event`]) - when
/// the operator runs each read-only surface, then each answers and writes nothing to the
/// file, `rigger validate` says once how many events the file passed over, by cause, and `rigger
/// graph --show` and `rigger dash --export` say nothing of them; once that file also owes its
/// rebuild, every surface says so first, and only `rigger validate` then says what it passed over.
#[test]
fn validate_alone_says_what_graph_db_passed_over_and_no_read_only_surface_writes_to_it() {
    let dir = temp_store_project();
    let root = dir.path();
    common::cli::run_rigger_ok(root, &["init"]);
    emit_the_first_decision(root);
    let graph_db = rigger_file(root, "graph.db");
    // The notes each surface of READ_ONLY_SURFACES prints, in its order: graph --show, validate,
    // dash --export.
    let each_surface_notes = |elsewhere: &[String], validate: &[String], why: &str| {
        for (args, expected) in READ_ONLY_SURFACES
            .into_iter()
            .zip([elsewhere, validate, elsewhere])
        {
            let before = std::fs::read(&graph_db).unwrap();
            let (out, err, ok, notes) = run_read_only(root, args);
            assert_eq!(
                (ok, notes, std::fs::read(&graph_db).unwrap() == before),
                (true, expected.to_vec(), true),
                "rigger {} {why}, and writes nothing to graph.db; stdout: {out} stderr: {err}",
                args.join(" ")
            );
        }
    };
    each_surface_notes(
        &[],
        &[],
        "prints no note for a graph.db that passed nothing over",
    );

    for _ in 0..2 {
        append_unfolded_decision(root, common::fixtures::POISON);
    }
    bury_below_an_applied_event(root);
    setup_ok(root, &[("RIGGER_NPM", "true")]);
    let passed_over = common::fixtures::passed_over_note(2, 0, 0);
    each_surface_notes(
        &[],
        std::slice::from_ref(&passed_over),
        "says what graph.db passed over, by cause, only when it is validate",
    );

    common::fixtures::stamp_user_version(&graph_db, 0);
    let owed = rebuild_owed_note();
    each_surface_notes(
        std::slice::from_ref(&owed),
        &[owed.clone(), passed_over],
        "says first that the rebuild is owed, then what graph.db passed over only when it is \
         validate",
    );
}

/// `Projector::passed_over` counts the `passed_over` rows a rebuild wrote by the class of each
/// cause - one event whose payload the judge rejects, two ledger entries resolving to nothing and
/// four whose re-extracted batch it rejects - and their total, and counts a row of any other class
/// under none of them.
#[test]
fn passed_over_counts_each_class_a_rebuild_wrote_by_cause_and_no_other_class() {
    use common::fixtures::{poisoned_decision_at, rejected_batch_entries, unresolved_entry_at};
    use rigger::contextgraph::sqlite::{PassedOver, Projector};
    let (_dir, graph_db) = common::fixtures::temp_graph_db();
    let log = [
        vec![
            poisoned_decision_at(1),
            unresolved_entry_at(2),
            unresolved_entry_at(3),
        ],
        rejected_batch_entries(4..=7),
    ]
    .concat();
    common::fixtures::rebuild_from_the_log(Path::new(&graph_db), &log, 10, &mut |entry| {
        Ok((entry.position > 3).then(common::fixtures::poisoned_batch))
    });
    let counted = || {
        let passed = Projector::open(&graph_db, "test")
            .unwrap()
            .passed_over()
            .unwrap();
        (passed, passed.total())
    };
    let by_class = PassedOver {
        rejected: 1,
        failed: 4,
        unresolved: 2,
    };
    assert_eq!(
        counted(),
        (by_class, 7),
        "each class the rebuild wrote is counted under its own cause"
    );

    rusqlite::Connection::open(&graph_db)
        .unwrap()
        .execute(
            "INSERT INTO passed_over (position, cause) VALUES (8, 'another class: its text')",
            [],
        )
        .unwrap();
    assert_eq!(
        counted(),
        (by_class, 7),
        "a cause of no class a fold pass writes is counted under none"
    );
}

/// Given a `graph.db` that misses a well-formed event the log holds below an event it folded, older
/// than the catch-up horizon ([`bury_below_an_applied_event`]), beside the shadow a rebuild of it
/// left, stopped before it folded an event, when the operator runs `rigger setup` and the rebuild
/// it resumes meets a storage error folding that event - the store's failure,
/// not a payload the fold rejects - then setup fails naming the error, passes nothing over, and
/// leaves `graph.db` untouched with the event still missing from its ledger; once the storage
/// recovers, the next `rigger setup` resumes the rebuild and folds the event.
#[test]
fn setup_whose_rebuild_meets_a_storage_error_fails_and_the_next_setup_folds_the_event() {
    let dir = temp_store_project();
    let root = dir.path();
    emit_the_first_decision(root);
    let first = run_log_head(root);
    let lost = append_unfolded_decision(root, D_BOOM);
    bury_below_an_applied_event(root);
    let graph_db = rigger_file(root, "graph.db");
    let shadow = rigger_file(root, "graph.db.rebuild");
    // The rebuild setup owes, stopped at its first read before it folded an event: the shadow it
    // leaves holds its rebuild state, which the next rebuild resumes, and no node yet.
    rebuild_under(&graph_db, &run_stream_identity(root), true, &mut |_, _| {
        source_interrupted()
    })
    .unwrap_err();
    // A rebuild owns its shadow alone, so the fixture holds no connection to it while `rigger
    // setup` rebuilds.
    the_disk_gives_out_at_d_boom(&shadow);

    let (out, err, ok) = run_rigger_envs(root, &["setup"], &[("RIGGER_NPM", "true")]);
    let refused = common::cli::refusal_line(&graph_error(&format!("event store: {DISK_GAVE_OUT}")));
    assert_eq!(
        (
            ok,
            err.lines().last(),
            out.lines()
                .filter(|l| l.starts_with("rebuilt graph.db") || l.starts_with("passed over "))
                .collect::<Vec<_>>(),
            applied(&graph_db, first),
            applied(&graph_db, lost),
        ),
        (
            false,
            Some(refused.as_str()),
            Vec::<&str>::new(),
            true,
            false
        ),
        "setup fails naming the storage error, passes nothing over, and leaves graph.db as it \
         was, the event still missing; stdout: {out} stderr: {err}"
    );

    the_disk_recovers(&shadow);
    the_next_setup_pays_the_missing_event(root, lost);
}

/// Given a project whose run stream grew only through rigger's own verbs - `rigger step` minting
/// the run and driving its unit to an escalation, and `rigger resume-unit` granting it another
/// attempt - when the operator runs `rigger setup`, then setup rebuilds nothing: every event a verb
/// appended is folded, so the graph's ledger holds every position the log does, and nothing was
/// lost for setup to find missing.
#[test]
fn every_event_rigger_s_own_verbs_append_is_folded_so_setup_owes_no_rebuild() {
    let dir = temp_git_project_with_commit();
    let root = dir.path();
    escalate_solo_unit(root);
    let (_, err, ok) = run_rigger(root, &["resume-unit", "solo", "--attempts", "1"]);
    assert!(
        ok,
        "resume-unit on the escalated unit must succeed; stderr: {err}"
    );
    let graph_db = rigger_file(root, "graph.db");
    assert_eq!(
        read_run_events(root)
            .into_iter()
            .filter(|e| !applied(&graph_db, e.position))
            .map(|e| (e.position, e.type_))
            .collect::<Vec<_>>(),
        Vec::<(u64, String)>::new(),
        "every event the verbs appended is in the graph's ledger"
    );
    let out = setup_ok(root, &[("RIGGER_NPM", "true")]);
    assert_eq!(
        rebuilding_and_rebuilt_lines(&out),
        Vec::<&str>::new(),
        "setup owes no rebuild for a log its verbs folded; stdout: {out}"
    );
}

/// Given an escalated unit and a `graph.db` whose bytes are not a database, when the operator runs
/// `rigger resume-unit`, then the resume succeeds with its `UnitResumed` on the log and says the fold
/// it could not make, with the reason: a verb whose job is to append never opens `graph.db` before
/// its append, so an unopenable graph never costs the operator's recovery verb its record.
#[test]
fn resume_unit_into_a_graph_it_cannot_open_is_on_the_log_and_reported_not_folded() {
    let dir = temp_git_project_with_commit();
    let root = dir.path();
    escalate_solo_unit(root);
    let garbage = b"this file is not a sqlite database, only text standing in for one".repeat(64);
    std::fs::write(rigger_file(root, "graph.db"), &garbage).unwrap();

    let (out, err, ok) = run_rigger(root, &["resume-unit", "solo", "--attempts", "1"]);
    let log = run_log(root);
    assert_eq!(
        (ok, log.last().map(|(_, t)| t.as_str())),
        (true, Some("UnitResumed")),
        "the resume succeeds and is on the log; stdout: {out} stderr: {err}"
    );
    assert_eq!(
        err,
        common::fixtures::recorded_not_folded(1, common::fixtures::NOT_A_DATABASE) + "\n",
        "the fold it could not make is said, with the reason"
    );
}

/// A git project scaffolded with the escalating one-unit workflow, stopping at the grounder
/// ([`stopping_at_the_grounder`]): a `--fresh` step or run mints its boundary, re-pins the
/// definition and then fails at the grounder, before it drives anything or serves stdin.
fn fresh_run_project() -> tempfile::TempDir {
    let dir = temp_git_project_with_commit();
    write_workflow_fixture(dir.path(), &REVIEWLESS_GIT_ESCALATING_UNIT_WORKFLOW);
    let body = stopping_at_the_grounder(REVIEWLESS_GIT_ESCALATING_UNIT_WORKFLOW.body);
    std::fs::write(rigger_file(dir.path(), "workflow.yml"), body).unwrap();
    dir
}

/// Given a project, when the operator begins a new run with `args` (`rigger step --fresh`, or
/// `rigger run --fresh` on either driver), then the boundary it mints and every other event it
/// appends before it drives are folded: the graph's ledger holds every position the log does.
fn a_fresh_runs_mint_is_folded(args: &[&str]) {
    let dir = fresh_run_project();
    let root = dir.path();
    assert_stopped_at_the_grounder(&run_rigger(root, args), &format!("rigger {args:?}"));
    let graph_db = rigger_file(root, "graph.db");
    let log = run_log(root);
    assert_eq!(
        (
            log.iter().filter(|(_, t)| t == "RunStarted").count(),
            log.iter()
                .filter(|(p, _)| !applied(&graph_db, *p))
                .cloned()
                .collect::<Vec<_>>(),
        ),
        (1, Vec::<(u64, String)>::new()),
        "the fresh boundary is on the log and every appended event is in the graph's ledger; \
         log: {log:?}"
    );
}

/// Given a project whose `graph.db` owes its rebuild (a fold lost to another writer's lock), when
/// the operator begins a new run with `args`, then it refuses naming `rigger setup` before it
/// appends anything: no boundary is minted that the graph would miss, and neither the log nor
/// `graph.db` changes.
fn a_fresh_run_on_a_graph_that_owes_its_rebuild_refuses_before_it_mints(
    args: &[&str],
    command: &str,
) {
    let dir = fresh_run_project();
    let root = dir.path();
    common::cli::seed_store(root);
    emit_the_first_decision(root);
    let graph_db = rigger_file(root, "graph.db");
    let (out, err, _) = with_graph_locked(&graph_db, || emit_decision(root, "d-locked"));
    assert!(
        out.ends_with(&format!(
            "; {}\n",
            not_folded(&lost_fold(&graph_error(DATABASE_IS_LOCKED)))
        )),
        "the locked emit's fold is lost, so the graph owes its rebuild; stdout: {out} stderr: {err}"
    );
    let (log, graph) = (run_log(root), std::fs::read(&graph_db).unwrap());

    let (out, err, ok) = run_rigger(root, args);
    assert_eq!(
        (
            ok,
            err.trim_end()
                .ends_with(&rigger::contextgraph::rebuild_owed_refusal(command))
        ),
        (false, true),
        "the fresh run refuses naming `rigger setup`; stdout: {out} stderr: {err}"
    );
    assert_eq!(
        (run_log(root), std::fs::read(&graph_db).unwrap() == graph),
        (log, true),
        "the refusal mints no boundary and leaves graph.db as it was"
    );
}

rigger::test_cases! {
    /// `rigger step --fresh` mints its boundary through the step's folding store.
    a_fresh_steps_mint_is_folded: a_fresh_runs_mint_is_folded(&["step", "--fresh"]);
    /// `rigger run --driver cli --fresh` mints its boundary through a folding store.
    a_fresh_cli_runs_mint_is_folded:
        a_fresh_runs_mint_is_folded(&["run", "--driver", "cli", "--base", "HEAD", "--fresh"]);
    /// `rigger run --driver workflow --fresh` mints its boundary through a folding store.
    a_fresh_workflow_runs_mint_is_folded:
        a_fresh_runs_mint_is_folded(&["run", "--driver", "workflow", "--base", "HEAD", "--fresh"]);
    /// `rigger step --fresh` opens the graph before it mints.
    a_fresh_step_on_an_owed_graph_refuses_before_it_mints:
        a_fresh_run_on_a_graph_that_owes_its_rebuild_refuses_before_it_mints(
            &["step", "--fresh"],
            "step",
        );
    /// `rigger run --driver cli --fresh` opens the graph before it mints.
    a_fresh_cli_run_on_an_owed_graph_refuses_before_it_mints:
        a_fresh_run_on_a_graph_that_owes_its_rebuild_refuses_before_it_mints(
            &["run", "--driver", "cli", "--base", "HEAD", "--fresh"],
            "run",
        );
    /// `rigger run --driver workflow --fresh` opens the graph before it mints.
    a_fresh_workflow_run_on_an_owed_graph_refuses_before_it_mints:
        a_fresh_run_on_a_graph_that_owes_its_rebuild_refuses_before_it_mints(
            &["run", "--driver", "workflow", "--base", "HEAD", "--fresh"],
            "run",
        );
}

/// Given a `graph.db` still at the old fold rule, when the operator runs `rigger reset --runs`,
/// then it refuses at once naming `rigger setup` - it neither prunes nor compacts a graph that
/// owes its rebuild, nor folds into it - and leaves both the file and the log exactly as they were.
#[test]
fn reset_runs_on_a_graph_that_owes_its_rebuild_refuses_naming_setup_and_writes_nothing() {
    let store = ReleaseEraStore::new();
    let (graph, log) = (store.graph_bytes(), store.log().len());
    let (out, err, ok) = run_rigger(store.root(), &["reset", "--runs"]);
    assert!(!ok, "reset --runs must refuse; stdout: {out}");
    assert!(
        err.contains(&format!(
            "reset --runs: {}",
            rigger::contextgraph::REBUILD_OWED
        )),
        "the refusal names `rigger setup`; stderr: {err}"
    );
    assert_eq!(
        (store.graph_bytes() == graph, store.log().len()),
        (true, log),
        "the refusal writes nothing to graph.db or the log"
    );
}

/// Given a current `graph.db` another writer holds locked, when the operator runs `rigger graph
/// build` over a project with two source files, then the build records both files' ledger
/// entries, counts the batch events of both - a lost fold and a refused one each count - and says
/// they were not folded, naming the first fold it lost (the lock) rather than the owed refusal
/// the second entry met because of it; the graph now owes its rebuild, so the next build refuses
/// naming `rigger setup`.
#[cfg(feature = "symbols")]
#[test]
fn a_graph_build_whose_fold_is_lost_to_a_lock_says_so_and_the_next_build_refuses() {
    let dir = temp_store_project();
    let root = dir.path();
    emit_the_first_decision(root);
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(root.join("src").join("lib.rs"), "pub fn alpha() {}\n").unwrap();
    std::fs::write(
        root.join("src").join("more.rs"),
        "pub fn beta() {}\npub fn gamma() { beta(); }\n",
    )
    .unwrap();
    let graph_db = rigger_file(root, "graph.db");
    let before = read_run_events(root).len();

    let (out, err, ok) = with_graph_locked(&graph_db, || run_rigger(root, &["graph", "build"]));
    let recorded = common::fixtures::recorded_entry_keys(&read_run_events(root)[before..]);
    assert_eq!(
        (ok, out),
        (
            true,
            common::cli::graph_build_line(
                6,
                &format!(
                    "; {}",
                    not_folded(&lost_fold(&graph_error(DATABASE_IS_LOCKED)))
                )
            )
        ),
        "the build counts both entries' batch events and names the first fold it lost - the \
         lock - never the owed refusal the later entry then met; stderr: {err}"
    );
    assert_eq!(
        (recorded.clone(), read_run_events(root).len() - before),
        (common::fixtures::walked_entry_keys(root), 2),
        "both files' entries are on the log, one after the other, and nothing else"
    );
    assert_eq!(
        recorded
            .iter()
            .map(|key| {
                let (identity, _, events) = common::fixtures::entry_key_parts(key);
                (identity, events)
            })
            .collect::<Vec<_>>(),
        vec![
            ("gc/src/lib.rs".to_string(), 2),
            ("gc/src/more.rs".to_string(), 4)
        ],
        "sanity: the two entries stand for two and four batch events"
    );

    let (out, err, ok) = run_rigger(root, &["graph", "build"]);
    assert!(
        !ok && err.contains(&format!(
            "graph build: {}",
            rigger::contextgraph::REBUILD_OWED
        )),
        "the next build refuses naming `rigger setup`; stdout: {out} stderr: {err}"
    );
}

/// Given a run whose `graph.db` another writer holds locked while `rigger step` records the run's
/// events, when the step runs, then it still advances the run and says on stderr that the events
/// it recorded were not folded, with the reason; the graph now owes its rebuild, so the next step
/// refuses naming `rigger setup` instead of advancing the run over a graph that lost them.
#[test]
fn a_step_whose_fold_is_lost_to_a_lock_says_so_and_the_next_step_refuses() {
    let dir = common::cli::temp_repoless_project();
    let root = dir.path();
    common::cli::seed_store(root);
    common::cli::write_workflow(root, "");
    let (out, err, ok) = run_rigger(root, &["step"]);
    assert!(
        ok && !err.contains("run event(s)"),
        "the first step parks the stage's spawn and, its events folded, says nothing of the \
         fold; stdout: {out} stderr: {err}"
    );
    let (_, err, ok) = run_rigger(root, &["result", "a/implementer#0", "done"]);
    assert!(ok, "the spawn's result is recorded; stderr: {err}");

    let graph_db = rigger_file(root, "graph.db");
    let (out, err, ok) = with_graph_locked(&graph_db, || run_rigger(root, &["step"]));
    assert_eq!(
        (ok, out.as_str()),
        (true, "{\"wave\":[],\"done\":true}\n"),
        "the step replays the result and finishes the run; stderr: {err}"
    );
    assert!(
        err.contains(&format!(
            "; {}\n",
            not_folded(&lost_fold(&graph_error(DATABASE_IS_LOCKED)))
        )),
        "the step says the events it recorded were not folded; stderr: {err}"
    );

    let (out, err, ok) = run_rigger(root, &["step"]);
    assert!(
        !ok && err.contains(&format!("step: {}", rigger::contextgraph::REBUILD_OWED)),
        "the next step refuses naming `rigger setup`; stdout: {out} stderr: {err}"
    );
}

/// Cut `branch` from the run branch of the repository at `root`, commit on it, and fast-forward
/// the run branch onto it: a unit's work landed by hand, as `rigger reset --runs` closes it.
/// Returns the landed tip.
fn land_on_the_run_branch(root: &Path, branch: &str) -> String {
    use common::git::{git_ok, git_ok_with_identity, git_out};
    git_ok(root, &["branch", branch, "rigger-run"]);
    git_ok(root, &["checkout", "-q", branch]);
    git_ok_with_identity(root, &["commit", "-q", "--allow-empty", "-m", branch]);
    git_ok(root, &["checkout", "-q", "rigger-run"]);
    git_ok(root, &["merge", "-q", "--ff-only", branch]);
    git_out(root, &["rev-parse", branch])
}

/// The line `rigger reset --runs` prints for closing unit `unit` at landed tip `tip` of run `r1`,
/// before whatever it adds about the fold.
fn closed_unit_line(unit: &str, tip: &str) -> String {
    format!(
        "reset --runs: closed unit {unit:?} of run r1: no driver is alive and its branch tip {tip} \
         is landed on rigger-run, so its UnitIntegrated is recorded (by operator)"
    )
}

/// The position of the last `UnitIntegrated` on `root`'s run stream.
fn last_integration(root: &Path) -> u64 {
    read_run_events(root)
        .iter()
        .rev()
        .find(|e| e.type_ == "UnitIntegrated")
        .expect("a UnitIntegrated is on the log")
        .position
}

/// Given a run no driver drives whose unit's branch is landed on the run branch, when the operator
/// runs `rigger reset --runs`, then it records the unit's `UnitIntegrated`, folds it, and its
/// closing line claims nothing more; when another writer holds `graph.db` locked while it closes
/// the next landed unit, the closing line says that unit's integration was not folded, with the
/// reason, and the graph now owes its rebuild, so the next `rigger reset --runs` refuses naming
/// `rigger setup`.
#[test]
fn reset_runs_says_whether_the_integration_it_recorded_for_a_landed_unit_was_folded() {
    use common::git::{git_ok, init_repo};
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    init_repo(root);
    git_ok(root, &["checkout", "-q", "-b", "rigger-run"]);
    common::cli::seed_run_events(
        root,
        &[
            ("RunStarted", r#"{"run":"r1","spec":"s.md"}"#),
            ("UnitStarted", r#"{"id":"u1","branch":"rigger/u1"}"#),
        ],
    );
    let tip = land_on_the_run_branch(root, "rigger/u1");

    let (out, err, ok) = run_rigger(root, &["reset", "--runs"]);
    assert_eq!(
        (ok, out.lines().next().unwrap_or_default().to_string()),
        (true, closed_unit_line("u1", &tip)),
        "the folded integration's line claims nothing more; stderr: {err}"
    );
    let graph_db = rigger_file(root, "graph.db");
    assert!(
        applied(&graph_db, last_integration(root)),
        "the recorded integration is folded"
    );

    common::cli::seed_run_events(
        root,
        &[("UnitStarted", r#"{"id":"u2","branch":"rigger/u2"}"#)],
    );
    let tip = land_on_the_run_branch(root, "rigger/u2");
    let (out, err, _) = with_graph_locked(&graph_db, || run_rigger(root, &["reset", "--runs"]));
    assert_eq!(
        out.lines().next().unwrap_or_default(),
        format!(
            "{}; {}",
            closed_unit_line("u2", &tip),
            not_folded(&lost_fold(&graph_error(DATABASE_IS_LOCKED)))
        ),
        "the lost integration's line says it was not folded, with the reason; stderr: {err}"
    );
    assert!(
        !applied(&graph_db, last_integration(root)),
        "the integration is on the log but not in the graph"
    );

    let (out, err, ok) = run_rigger(root, &["reset", "--runs"]);
    assert!(
        !ok && err.contains(&rigger::contextgraph::rebuild_owed_refusal("reset --runs")),
        "the next reset --runs refuses naming `rigger setup`; stdout: {out} stderr: {err}"
    );
}

/// Given an in-memory graph, which no rebuild outlives, when a batch fails to fold into it, then
/// the failure is reported but marks nothing owed - no file beside it is written - and the next
/// batch folds.
#[test]
fn a_failed_fold_into_an_in_memory_graph_marks_nothing_owed_and_the_next_batch_folds() {
    use rigger::contextgraph::sqlite::Projector;
    use rigger::contextgraph::Projection;
    let mark = std::env::current_dir().unwrap().join(":memory:.owed");
    let mut poison = Event::new("DecisionMade", common::fixtures::POISON.to_vec());
    poison.position = 1;
    let graph = Projector::open(":memory:", PROJECT).unwrap();
    let failed = common::fixtures::fold_of(&graph, &[poison]);
    assert!(
        matches!(&failed, Fold::NotFolded(why) if why.starts_with(&graph_error(""))),
        "the failed fold is reported: {failed:?}"
    );
    assert_eq!(
        (graph.rebuild_owed().unwrap(), mark.exists()),
        (false, false),
        "an in-memory graph owes nothing and writes no mark"
    );
    let mut good = Event::new(
        "DecisionMade",
        br#"{"id":"d","summary":"s","governs":["a.rs"],"supersedes":""}"#.to_vec(),
    );
    good.position = 2;
    common::fixtures::folds(&graph, &[good]);
}

/// The decision, finding and lesson node ids the `graph.db` of `root` holds, sorted.
fn provenance_nodes(root: &Path) -> Vec<String> {
    let graph = rigger::contextgraph::sqlite::Projector::open(
        rigger_file(root, "graph.db").to_str().unwrap(),
        &run_stream_identity(root),
    )
    .unwrap();
    let mut ids: Vec<String> = graph
        .whole()
        .unwrap()
        .nodes
        .into_iter()
        .filter(|n| ["decision", "finding", "lesson"].contains(&n.kind.as_str()))
        .map(|n| n.id)
        .collect();
    ids.sort();
    ids
}

/// Given a log holding a closed run's decision and finding, a lesson, a closed-run decision that
/// supersedes one whose id the active run reuses (retiring its governing edge before the active
/// run began), and that active run, whose closed-run nodes and superseded edges `rigger reset
/// --runs` pruned from the live graph, when the operator's `graph.db` is replaced by an empty one
/// and `rigger setup` cold-rebuilds it from the log, then the rebuilt graph holds exactly the
/// decision, finding and lesson nodes the live graph held - never the pruned ones - and is the
/// live graph, every node and edge.
#[test]
fn a_cold_rebuild_of_a_log_whose_closed_runs_were_pruned_yields_the_live_graph() {
    let dir = temp_store_project();
    let root = dir.path();
    closed_run_store(root);
    let before = menus_runs_prune(root);
    let (out, err, ok) = run_rigger(root, &["reset", "--runs"]);
    assert!(
        ok,
        "reset --runs prunes the closed run; stdout: {out} stderr: {err}"
    );
    let live = provenance_nodes(root);
    let live_whole = whole_graph(root);
    let pruned = menus_runs_prune(root);

    lose_graph(root);
    let out = setup_ok(root, &[("RIGGER_NPM", "true")]);
    assert_eq!(
        (live, provenance_nodes(root)),
        (owned_ids(&CLOSED_RUN_KEPT), owned_ids(&CLOSED_RUN_KEPT)),
        "the rebuilt graph holds the live graph's decision, finding and lesson nodes, never the \
         pruned ones; stdout: {out}"
    );
    assert_eq!(
        (before, pruned, menus_runs_prune(root)),
        (
            runs_prunable(3, 1),
            runs_prunable(0, 0),
            runs_prunable(0, 0)
        ),
        "the closed run left nodes and a superseded edge to prune; once pruned, and once rebuilt, \
         nothing is left - the rebuild dropped the superseded edge reset --runs dropped"
    );
    assert_eq!(
        whole_graph(root),
        live_whole,
        "the rebuilt graph is the live pruned graph, every node and edge"
    );
}

/// The provenance nodes [`closed_run_store`]'s graph keeps once `rigger reset --runs` prunes it,
/// and so the nodes a rebuild of its log holds: the active run's `d-live` and `shared` and the
/// lesson `l1`.
const CLOSED_RUN_KEPT: [&str; 3] = ["d-live", "l1", "shared"];

/// Set up the project at `root` and record, as a run records it and folded into its live graph as
/// it lands, a lesson, a closed run `r1` (a decision, a finding, and a decision `d-sup` that
/// supersedes `shared`, retiring its governing edge before the active run began) and the active run
/// `r2`, which records `d-live` and reuses the id `shared`: `rigger reset --runs` prunes 3 dead-run
/// nodes (`d-dead`, `f-dead`, `d-sup`) and 1 superseded edge from it.
fn closed_run_store(root: &Path) {
    let decision = |id: &str| {
        format!(r#"{{"id":"{id}","summary":"s","governs":["src/f.rs"],"supersedes":""}}"#)
    };
    let (d_dead, shared, d_live) = (decision("d-dead"), decision("shared"), decision("d-live"));
    setup_ok(root, &[("RIGGER_NPM", "true")]);
    let (out, err, ok) = run_rigger(
        root,
        &[
            "emit",
            "LessonLearned",
            r#"{"id":"l1","summary":"s","about":["src/f.rs"]}"#,
        ],
    );
    assert!(
        ok,
        "the first emit creates the store and graph; stdout: {out} stderr: {err}"
    );
    fold_run_events(
        root,
        &[
            ("RunStarted", r#"{"run":"r1","spec":"s.md"}"#),
            ("DecisionMade", &d_dead),
            ("ReviewFinding", r#"{"id":"f-dead","about":["src/f.rs"]}"#),
            ("DecisionMade", &shared),
            (
                "DecisionMade",
                r#"{"id":"d-sup","summary":"s","governs":["src/f.rs"],"supersedes":"shared"}"#,
            ),
            ("RunStarted", r#"{"run":"r2","spec":"s.md"}"#),
            ("DecisionMade", &d_live),
            (
                "DecisionMade",
                r#"{"id":"shared","summary":"s","governs":["src/g.rs"],"supersedes":""}"#,
            ),
        ],
    );
}

/// Replace the `graph.db` of `root` with an empty one, which owes its rebuild from the log.
fn lose_graph(root: &Path) {
    let graph_db = rigger_file(root, "graph.db");
    std::fs::remove_file(&graph_db).unwrap();
    drop(
        rigger::contextgraph::sqlite::Projector::open(
            graph_db.to_str().unwrap(),
            &run_stream_identity(root),
        )
        .unwrap(),
    );
}

/// What `rigger setup` printed of `out` right after it said it rebuilt `graph.db`, if it did.
fn after_rebuilt(out: &str) -> Option<String> {
    out.lines()
        .skip_while(|l| *l != REBUILT_LINE)
        .nth(1)
        .map(str::to_string)
}

/// The opening of the report `rigger reset --runs` prints in `out`: what it pruned, up to where it
/// names the graph it pruned.
fn reset_runs_pruned(out: &str) -> Option<String> {
    out.lines()
        .find(|l| l.starts_with("reset --runs: "))
        .and_then(|l| l.split(" from the context graph").next())
        .map(str::to_string)
}

/// Given a log whose closed run the live graph still holds, when `rigger reset --runs` prunes it
/// and then `graph.db` is lost and `rigger setup` rebuilds it from the same log, then setup reports,
/// right after its rebuilt line, the prune its rebuild made in the words `reset --runs` reports its
/// own - the same dead-run node and superseded edge counts.
#[test]
fn setup_reports_the_prune_its_rebuild_made_as_reset_runs_reports_it() {
    let dir = temp_store_project();
    let root = dir.path();
    closed_run_store(root);
    let (reset, err, ok) = run_rigger(root, &["reset", "--runs"]);
    assert!(ok, "reset --runs prunes; stdout: {reset} stderr: {err}");
    lose_graph(root);
    let (out, err, ok) = run_rigger_envs(root, &["setup"], &[("RIGGER_NPM", "true")]);
    assert_eq!(
        (ok, reset_runs_pruned(&reset), after_rebuilt(&out)),
        (
            true,
            Some(
                "reset --runs: pruned 3 dead-run node(s) and reclaimed 1 superseded edge(s)"
                    .to_string()
            ),
            Some(pruned_from_the_rebuilt_graph(3, 1)),
        ),
        "setup reports the prune reset --runs reports, in its words; stdout: {out} stderr: {err}"
    );
}

/// Given a `rigger setup` whose rebuild stopped after its prune and its swap - the owed mark it
/// must then drop is a directory it cannot remove - when that is cleared and setup runs again,
/// then the rerun finishes the rebuild and reports the counts that prune made, never zero, and the
/// graph holds what `reset --runs` leaves.
#[test]
fn a_setup_rerun_after_its_rebuild_stopped_past_its_prune_reports_the_same_counts() {
    let dir = temp_store_project();
    let root = dir.path();
    closed_run_store(root);
    lose_graph(root);
    let (stopped, stopped_ok) = stop_setup_past_its_prune(root, |_| {});
    let (out, err, ok) = run_rigger_envs(root, &["setup"], &[("RIGGER_NPM", "true")]);
    assert_eq!(
        (
            stopped_ok,
            after_rebuilt(&stopped),
            ok,
            after_rebuilt(&out),
            provenance_nodes(root),
        ),
        (
            false,
            None,
            true,
            Some(pruned_from_the_rebuilt_graph(3, 1)),
            owned_ids(&CLOSED_RUN_KEPT),
        ),
        "the stopped setup fails before its rebuilt line, and the rerun reports the prune it \
         made; stdout: {out} stderr: {err}"
    );
}

/// Run a `rigger setup` in `root` whose rebuild stops right after its prune and its swap - the owed
/// mark it must then drop is a directory it cannot remove, and its failure names that mark - then
/// `meanwhile`, while that mark still says `graph.db` owes its rebuild, and clear the mark: what
/// the stopped setup printed, and whether it exited successfully.
fn stop_setup_past_its_prune(root: &Path, meanwhile: fn(&Path)) -> (String, bool) {
    let mark = rigger_file(root, "graph.db.owed");
    std::fs::create_dir(&mark).unwrap();
    let (stopped, err, stopped_ok) = run_rigger_envs(root, &["setup"], &[("RIGGER_NPM", "true")]);
    assert_eq!(
        err.lines().last(),
        Some(
            common::cli::refusal_line(&common::fixtures::directory_refusal(
                Path::new(".rigger").join("graph.db.owed").display()
            ))
            .as_str()
        ),
        "the stopped setup names the mark it could not remove; stdout: {stopped} stderr: {err}"
    );
    meanwhile(root);
    std::fs::remove_dir(&mark).unwrap();
    (stopped, stopped_ok)
}

/// Given a `rigger setup` whose rebuild stopped past its prune and its swap, when the log gains
/// events before setup runs again - a decision the active run records with `rigger emit`, which
/// the owed graph does not fold, a run another writer of the log starts, closing the run that was
/// active, or a closed run's decision id the active run records again about another file - or a
/// swap cut short left its private pruned copy behind, then the rerun names no cause - the shadow
/// is the rebuild's own unfinished work, which the swapped-in ledger owes nothing for - and resumes
/// its unpruned shadow, folding only what the log gained past it - its one progress line counts
/// exactly those events, and a rerun the log gained nothing for folds none - and prunes a fresh
/// copy of it from the whole run attribution it gathered, the first pass's included, never reading
/// a stale copy and leaving none behind: it keeps the active run's decision, prunes the run that
/// closed, keeps the id the active run re-recorded governing both files its two recordings name,
/// folds every event the log gained, keeps no rebuild state and leaves `rigger reset --runs`
/// nothing to prune; and a cold rebuild of the same log yields the same graph, every node and edge,
/// and the same report - the one prune of the whole log, never the sum of two: when the run that
/// closed is the one that was active, `shared` is dropped with the retired edge it owns, 5 nodes
/// and no edge left to reclaim.
#[test]
fn a_setup_resumed_past_its_prune_keeps_the_active_runs_gains_and_prunes_the_run_that_closed() {
    let active_run_decides: fn(&Path) = |root| {
        let (out, err, ok) = emit_decision(root, "d-window");
        assert!(ok, "the emit appends; stdout: {out} stderr: {err}");
    };
    let another_writer_starts_a_run: fn(&Path) = |root| {
        common::cli::with_run_store(root, |store| {
            store
                .append(
                    rigger::conductor::STREAM,
                    ExpectedRevision::Any,
                    &[
                        Event::new("RunStarted", br#"{"run":"r3","spec":"s.md"}"#.to_vec()),
                        Event::new(
                            "DecisionMade",
                            br#"{"id":"d-r3","summary":"s","governs":["src/f.rs"],"supersedes":""}"#
                                .to_vec(),
                        ),
                    ],
                )
                .unwrap();
        });
    };
    let active_run_re_records_a_closed_runs_decision: fn(&Path) = |root| {
        let (out, err, ok) = run_rigger(
            root,
            &[
                "emit",
                "DecisionMade",
                r#"{"id":"d-dead","summary":"s","governs":["src/h.rs"],"supersedes":""}"#,
            ],
        );
        assert!(ok, "the emit appends; stdout: {out} stderr: {err}");
    };
    let a_swap_cut_short_left_its_copy: fn(&Path) = |root| {
        std::fs::write(
            rigger_file(root, "graph.db.pruned"),
            b"left by a swap cut short",
        )
        .unwrap();
    };
    for (meanwhile, gains, kept, d_dead_governs, report) in [
        (
            active_run_decides,
            1,
            vec!["d-live", "d-window", "l1", "shared"],
            vec![],
            pruned_from_the_rebuilt_graph(3, 1),
        ),
        (
            another_writer_starts_a_run,
            2,
            vec!["d-r3", "l1"],
            vec![],
            pruned_from_the_rebuilt_graph(5, 0),
        ),
        (
            active_run_re_records_a_closed_runs_decision,
            1,
            vec!["d-dead", "d-live", "l1", "shared"],
            vec!["src/f.rs", "src/h.rs"],
            pruned_from_the_rebuilt_graph(2, 1),
        ),
        (
            a_swap_cut_short_left_its_copy,
            0,
            CLOSED_RUN_KEPT.to_vec(),
            vec![],
            pruned_from_the_rebuilt_graph(3, 1),
        ),
    ] {
        let dir = temp_store_project();
        let root = dir.path();
        closed_run_store(root);
        lose_graph(root);
        let held = read_run_events(root).len();
        let (stopped, stopped_ok) = stop_setup_past_its_prune(root, meanwhile);
        let graph_db = rigger_file(root, "graph.db");
        let log = read_run_events(root);
        let head = log.last().unwrap().position;
        let gained: Vec<u64> = crate::common::fixtures::positions_of(&log[held..]);
        // Whether the graph's applied ledger records each event the log gained.
        let folded = || -> Vec<bool> { gained.iter().map(|&p| applied(&graph_db, p)).collect() };
        let folded_before = folded();

        let (out, err, ok) = run_rigger_envs(root, &["setup"], &[("RIGGER_NPM", "true")]);
        let resumed = (
            ok,
            out.lines()
                .filter(|l| l.starts_with("rebuilding graph.db"))
                .map(str::to_string)
                .collect::<Vec<_>>(),
            rebuild_progress(&out),
            after_rebuilt(&out),
            provenance_nodes(root),
            live_governs(root, "d-dead"),
            folded(),
            [
                holds_table(&graph_db, "rebuild_cursor"),
                holds_table(&graph_db, "rebuild_run_closure"),
                rigger_file(root, "graph.db.rebuild").exists(),
                rigger_file(root, "graph.db.pruned").exists(),
            ],
            menus_runs_prune(root),
        );
        let resumed_graph = whole_graph(root);
        lose_graph(root);
        let (cold, cold_err, cold_ok) =
            run_rigger_envs(root, &["setup"], &[("RIGGER_NPM", "true")]);
        assert_eq!(
            (
                stopped_ok,
                after_rebuilt(&stopped),
                gained.len(),
                folded_before,
                resumed,
                (cold_ok, after_rebuilt(&cold), whole_graph(root)),
            ),
            (
                false,
                None,
                gains,
                vec![false; gains],
                (
                    true,
                    Vec::<String>::new(),
                    (gains > 0)
                        .then(|| {
                            rebuilt_through_the_head(gains, head)
                        })
                        .into_iter()
                        .collect::<Vec<_>>(),
                    Some(report.to_string()),
                    owned_ids(&kept),
                    owned_ids(&d_dead_governs),
                    vec![true; gains],
                    [false, false, false, false],
                    runs_prunable(0, 0),
                ),
                (true, Some(report.to_string()), resumed_graph),
            ),
            "the rerun folds only what the log gained, keeps what the active run gained and prunes \
             what closed, from the whole gathered attribution, with the report a cold rebuild \
             makes; rerun stdout: {out} stderr: {err}; cold stdout: {cold} stderr: {cold_err}"
        );
    }
}

/// The lines of `rigger setup`'s stdout `out` that say it rebuilds `graph.db` and how far it got:
/// the cause it rebuilds for, each progress line and that it rebuilt, in the order printed.
fn rebuilding_and_rebuilt_lines(out: &str) -> Vec<&str> {
    common::cli::lines_where(out, |l| {
        l.starts_with("rebuilding graph.db") || l.starts_with("rebuilt ")
    })
}

/// The progress lines a `rigger setup` rebuild printed in `out`, in order: how many events each
/// batch carried it through.
fn rebuild_progress(out: &str) -> Vec<String> {
    out.lines()
        .filter(|l| l.starts_with("rebuilt ") && l.ends_with("%)"))
        .map(str::to_string)
        .collect()
}

/// The files the node `id` in `root`'s `graph.db` governs through a live edge, sorted.
fn live_governs(root: &Path, id: &str) -> Vec<String> {
    let graph = rigger::contextgraph::sqlite::Projector::open(
        rigger_file(root, "graph.db").to_str().unwrap(),
        &run_stream_identity(root),
    )
    .unwrap();
    let mut files: Vec<String> = graph
        .whole()
        .unwrap()
        .edges
        .into_iter()
        .filter(|e| {
            e.from == id && e.rel == rigger::contextgraph::REL_GOVERNS && e.valid_to.is_none()
        })
        .map(|e| e.to)
        .collect();
    files.sort();
    files
}

/// Given a `rigger setup` whose rebuild swapped its pruned graph in and stopped before it folded the
/// tail - the state it leaves once the owed mark and the shadow are gone, the next two steps its
/// swap takes: `graph.db` holding the rebuild's cursor, no shadow beside it - when the log gains an
/// event no fold made (another writer's) and an agent's `rigger emit`, which folds at once into the
/// current file, then the next `rigger setup` names no cause, because the ledger owes the tail to
/// the swapped-in cursor rather than to a lost fold, folds exactly the tail - its one progress line
/// counts the two events past the cursor, through the log's last position - and meets the emitted
/// event once, reports the prune stamped with the swap, drops the rebuild state, and leaves the
/// graph a cold rebuild of the same log yields; a setup after it owes nothing.
#[test]
fn a_setup_stopped_in_its_tail_is_finished_by_the_next_setup_folding_exactly_the_tail() {
    let dir = temp_store_project();
    let root = dir.path();
    closed_run_store(root);
    lose_graph(root);
    let (stopped, stopped_ok) = stop_setup_past_its_prune(root, |root| {
        std::fs::remove_file(rigger_file(root, "graph.db.rebuild")).unwrap();
    });
    let graph_db = rigger_file(root, "graph.db");
    let swapped_cursor = holds_table(&graph_db, "rebuild_cursor");
    let unfolded = append_unfolded_decision(
        root,
        br#"{"id":"d-tail","summary":"s","governs":["src/t.rs"],"supersedes":""}"#,
    );
    let (emitted, emit_err, emit_ok) = emit_decision(root, "d-emitted");
    let emitted_at = run_log_head(root);
    let before = (applied(&graph_db, unfolded), applied(&graph_db, emitted_at));

    let (out, err, ok) = run_rigger_envs(root, &["setup"], &[("RIGGER_NPM", "true")]);
    let finished = (
        ok,
        out.lines()
            .filter(|l| l.starts_with("rebuilding graph.db"))
            .collect::<Vec<_>>(),
        rebuild_progress(&out),
        after_rebuilt(&out),
        (applied(&graph_db, unfolded), applied(&graph_db, emitted_at)),
        provenance_nodes(root),
        [
            holds_table(&graph_db, "rebuild_cursor"),
            holds_table(&graph_db, "rebuild_run_closure"),
            rigger_file(root, "graph.db.rebuild").exists(),
        ],
    );
    let (again, again_err, again_ok) = run_rigger_envs(root, &["setup"], &[("RIGGER_NPM", "true")]);
    let finished_graph = whole_graph(root);
    lose_graph(root);
    let (cold, cold_err, cold_ok) = run_rigger_envs(root, &["setup"], &[("RIGGER_NPM", "true")]);
    assert_eq!(
        (
            stopped_ok,
            after_rebuilt(&stopped),
            swapped_cursor,
            emit_ok && emitted.ends_with(EMIT_FOLDED_ENDING),
            before,
            finished,
            again_ok && !again.contains("graph.db"),
            (cold_ok, finished_graph),
        ),
        (
            false,
            None,
            true,
            true,
            (false, true),
            (
                true,
                Vec::<&str>::new(),
                vec![rebuilt_through_the_head(2, emitted_at)],
                Some(pruned_from_the_rebuilt_graph(3, 1)),
                (true, true),
                vec![
                    "d-emitted".to_string(),
                    "d-live".to_string(),
                    "d-tail".to_string(),
                    "l1".to_string(),
                    "shared".to_string(),
                ],
                [false, false, false],
            ),
            true,
            (true, whole_graph(root)),
        ),
        "the next setup folds exactly the tail and reaches the cold graph; emit stdout: {emitted} \
         stderr: {emit_err}; setup stdout: {out} stderr: {err}; second setup stdout: {again} \
         stderr: {again_err}; cold stdout: {cold} stderr: {cold_err}"
    );
}

/// Given a `graph.db` that owes its rebuild and a stale `graph.db.pruned` beside it with no shadow
/// - the copy a swap cut short left, whose rebuild is gone - when the operator runs `rigger reset
/// --runs`, then it removes the copy first and says so, and still refuses the prune naming `rigger
/// setup`, leaving `graph.db` and the log exactly as they were and creating no shadow.
#[test]
fn reset_runs_on_a_graph_that_owes_its_rebuild_removes_a_stale_pruned_copy_before_it_refuses() {
    let store = ReleaseEraStore::new();
    let root = store.root();
    let copy = rigger_file(root, "graph.db.pruned");
    std::fs::write(&copy, b"left by a swap cut short").unwrap();
    let (graph, log) = (store.graph_bytes(), store.log().len());
    let (out, err, ok) = run_rigger(root, &["reset", "--runs"]);
    assert_eq!(
        (
            ok,
            out.lines().collect::<Vec<_>>(),
            err.lines().last(),
            copy.exists(),
            rigger_file(root, "graph.db.rebuild").exists(),
            store.graph_bytes() == graph,
            store.log().len(),
        ),
        (
            false,
            vec![
                "reset --runs: removed graph.db.pruned, the pruned copy a rebuild's stopped swap \
                 left beside graph.db"
            ],
            Some(
                common::cli::refusal_line(&format!(
                    "reset --runs: {}",
                    rigger::contextgraph::REBUILD_OWED
                ))
                .as_str()
            ),
            false,
            false,
            true,
            log,
        ),
        "the stale copy is removed before the owed graph is refused; stdout: {out} stderr: {err}"
    );
}

/// Given a project that recorded a decision with `rigger emit` before any run started, when an
/// agent asks `rigger_peers` in a live `rigger mcp` session and the operator runs `rigger peers`,
/// then both label it LIVE - with no run there is no closed run for it to belong to; when another
/// writer of the log then starts a run and that run records a decision, then the same session's
/// next `rigger_peers` and `rigger peers` both label the earlier decision HISTORICAL and the run's
/// own LIVE: one rule, one answer on both surfaces.
#[test]
fn a_decision_recorded_before_any_run_is_live_on_both_peers_surfaces_until_a_run_starts() {
    let dir = temp_store_project();
    let root = dir.path();
    setup_ok(root, &[("RIGGER_NPM", "true")]);
    let (out, err, ok) = emit_decision(root, "d-early");
    assert!(ok, "the emit appends; stdout: {out} stderr: {err}");
    let mut mcp = common::mcp::McpSession::start(root);
    // `(id, live)` of each decision a `rigger_peers` reply holds, and the decision lines of a
    // `rigger peers` read, in order.
    let mut both_surfaces = || {
        let reply = mcp.peers();
        let tool: Vec<(String, bool)> = reply["result"]["structuredContent"]["decisions"]
            .as_array()
            .unwrap_or_else(|| panic!("a decisions array: {reply}"))
            .iter()
            .map(|d| {
                (
                    d["id"].as_str().unwrap().to_string(),
                    d["live"].as_bool().unwrap(),
                )
            })
            .collect();
        let (out, err, ok) = run_rigger(root, &["peers", "src/f.rs"]);
        assert!(ok, "rigger peers answers; stdout: {out} stderr: {err}");
        let cli = owned_ids(&decision_lines(&out));
        (tool, cli)
    };
    let before_any_run = both_surfaces();
    common::cli::seed_run_events(root, &[("RunStarted", r#"{"run":"r1","spec":"s.md"}"#)]);
    let (out, err, ok) = emit_decision(root, "d-run");
    assert!(ok, "the emit appends; stdout: {out} stderr: {err}");
    let once_a_run_started = both_surfaces();
    let exit = mcp.finish();
    assert_eq!(
        (before_any_run, once_a_run_started, exit.status.success()),
        (
            (
                vec![("d-early".to_string(), true)],
                vec!["decision d-early | LIVE | s | governs: src/f.rs".to_string()],
            ),
            (
                vec![("d-early".to_string(), false), ("d-run".to_string(), true)],
                vec![
                    "decision d-early | HISTORICAL | s | governs: src/f.rs".to_string(),
                    "decision d-run | LIVE | s | governs: src/f.rs".to_string(),
                ],
            ),
            true,
        ),
        "both surfaces label a decision recorded before any run live, and historical once a run \
         started"
    );
}

/// Given a sqlite log with a stream subscription or an all-streams subscription live over it, each
/// having delivered the event the log held, when the log's events table is dropped under it by
/// another connection, then the subscription ends and reports the database's own error exactly as
/// the database spells it, never re-spelled as the port's error: the one forward read the stream
/// subscription polls through spells a failure as its caller does, so both kinds report the same
/// words.
#[test]
fn a_sqlite_subscription_whose_log_fails_reports_the_databases_own_error() {
    type Subscribe = fn(&Store) -> rigger::eventstore::Subscription;
    let to_the_stream: Subscribe = |store| store.subscribe_stream("s", 0).unwrap();
    let to_every_stream: Subscribe = |store| {
        store
            .subscribe_all(
                0,
                &rigger::eventstore::Filter {
                    stream_prefix: None,
                },
            )
            .unwrap()
    };
    let ended = [to_the_stream, to_every_stream].map(|subscribe| {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("events.db");
        let store = Store::open(db.to_str().unwrap()).unwrap();
        store
            .append(
                "s",
                ExpectedRevision::Any,
                &[Event::new("E", b"{}".to_vec())],
            )
            .unwrap();
        let subscription = subscribe(&store);
        let delivered = subscription
            .recv_timeout(std::time::Duration::from_secs(10))
            .map(|e| (e.stream, e.revision));
        let other = rusqlite::Connection::open(&db).unwrap();
        other
            .busy_timeout(std::time::Duration::from_secs(10))
            .unwrap();
        other.execute_batch("DROP TABLE events").unwrap();
        let after = subscription
            .recv_timeout(std::time::Duration::from_secs(10))
            .map(|e| e.revision);
        (delivered, after, subscription.err())
    });
    assert_eq!(
        ended,
        [
            (
                Some(("s".to_string(), 0)),
                None,
                Some("no such table: events".to_string())
            ),
            (
                Some(("s".to_string(), 0)),
                None,
                Some("no such table: events".to_string())
            ),
        ],
        "each subscription delivers what the log held, then ends reporting the database's own error"
    );
}

/// Given a project that recorded decisions, a finding and a lesson with `rigger emit` before it
/// ever started a run, when its `graph.db` is lost and `rigger setup` rebuilds it, then setup
/// reports it pruned nothing and the rebuilt graph is the live one, every node and edge; and
/// `rigger reset --runs` over that log prunes nothing either, as the bare `rigger reset` menu
/// previews it over the live graph that holds every one of those nodes; and `rigger peers` labels
/// each of those decisions live, the one answer the prune gives.
#[test]
fn a_log_that_never_started_a_run_is_rebuilt_and_reset_without_pruning_anything() {
    let dir = temp_store_project();
    let root = dir.path();
    setup_ok(root, &[("RIGGER_NPM", "true")]);
    for (ty, body) in [
        (
            "LessonLearned",
            r#"{"id":"l1","summary":"s","about":["src/f.rs"]}"#,
        ),
        (
            "DecisionMade",
            r#"{"id":"d-old","summary":"s","governs":["src/f.rs"],"supersedes":""}"#,
        ),
        (
            "DecisionMade",
            r#"{"id":"d-new","summary":"s","governs":["src/f.rs"],"supersedes":"d-old"}"#,
        ),
        (
            "ReviewFinding",
            r#"{"id":"f1","by":"lens","summary":"s","about":["src/f.rs"]}"#,
        ),
    ] {
        let (out, err, ok) = run_rigger(root, &["emit", ty, body]);
        assert!(ok, "emit {ty}; stdout: {out} stderr: {err}");
    }
    let live = whole_graph(root);
    let menu = menus_runs_prune(root);
    lose_graph(root);
    let (out, err, ok) = run_rigger_envs(root, &["setup"], &[("RIGGER_NPM", "true")]);
    let rebuilt = whole_graph(root);
    let (reset, reset_err, reset_ok) = run_rigger(root, &["reset", "--runs"]);
    let (peers, peers_err, peers_ok) = run_rigger(root, &["peers", "src/f.rs"]);
    assert_eq!(
        (
            ok,
            after_rebuilt(&out),
            provenance_nodes(root),
            rebuilt,
            menu,
            reset_ok,
            reset_runs_pruned(&reset),
            whole_graph(root),
            peers_ok,
            decision_lines(&peers),
        ),
        (
            true,
            Some(pruned_from_the_rebuilt_graph(0, 0)),
            vec![
                "d-new".to_string(),
                "d-old".to_string(),
                "f1".to_string(),
                "l1".to_string()
            ],
            live.clone(),
            runs_prunable(0, 0),
            true,
            Some(
                "reset --runs: pruned 0 dead-run node(s) and reclaimed 0 superseded edge(s)"
                    .to_string()
            ),
            live,
            true,
            vec![
                "decision d-old | LIVE | s | governs: src/f.rs",
                "decision d-new | LIVE | s | governs: src/f.rs",
            ],
        ),
        "nothing is dead before a run starts; setup stdout: {out} stderr: {err}; reset stderr: \
         {reset_err}; peers stderr: {peers_err}"
    );
}

/// Given a stale `graph.db.pruned` beside the graph, when `rigger reset --runs` runs while a
/// rebuild holds `graph.db.lock` in its swap - its shadow open, its private copy beside it - then
/// reset keeps the copy and names the lock, held by another `rigger setup` or `rigger reset`; once
/// the lock is free, `rigger reset --runs` removes the copy and says so, and never touches the
/// shadow a rebuild resumes from.
#[test]
fn reset_runs_removes_a_stale_pruned_copy_unless_a_rebuild_holds_graph_db_lock() {
    let dir = temp_store_project();
    let root = dir.path();
    closed_run_store(root);
    let copy = rigger_file(root, "graph.db.pruned");
    let shadow = rigger_file(root, "graph.db.rebuild");
    std::fs::write(&copy, b"the copy a live swap is using").unwrap();
    let rebuilding = hold_the_rebuild(root);
    let rebuilding_shadow = hold_shadow(root);
    let (held, held_err, held_ok) = run_rigger(root, &["reset", "--runs"]);
    let kept = std::fs::read(&copy).ok();
    drop((rebuilding, rebuilding_shadow));
    let (out, err, ok) = run_rigger(root, &["reset", "--runs"]);
    assert_eq!(
        (
            held_ok,
            lines_naming(&held, "graph.db.pruned"),
            kept,
            ok,
            lines_naming(&out, "graph.db.pruned"),
            copy.exists(),
            shadow.exists(),
        ),
        (
            true,
            vec![format!(
                "reset --runs: kept graph.db.pruned: {}",
                rigger::contextgraph::sqlite::REBUILD_IN_PROGRESS
            )],
            Some(b"the copy a live swap is using".to_vec()),
            true,
            vec![
                "reset --runs: removed graph.db.pruned, the pruned copy a rebuild's stopped swap \
                 left beside graph.db"
                    .to_string()
            ],
            false,
            true,
        ),
        "the copy is kept naming the lock, held by another setup or reset, while it is held, and \
         removed once it is free; held stderr: {held_err}; stdout: {out} stderr: {err}"
    );
}

/// Where a rebuild in progress stands when a second `rigger setup` starts.
#[derive(Debug, Clone, Copy)]
enum RebuildPhase {
    /// It holds `graph.db.lock` and has written nothing yet.
    Locked,
    /// It folds its shadow, open mid-batch: the fixture's EXCLUSIVE connection ([`hold_shadow`])
    /// stands in for it, the test's detector for a setup that reads the shadow - not a lock a
    /// rebuild holds through its fold: a rebuild holds only `graph.db.lock`.
    Folding,
    /// It swaps: its shadow is open and its private pruned copy stands beside it.
    Swapping,
    /// Its swap has ended - no shadow, `graph.db` holding its cursor - and it folds the tail.
    FoldingTheTail,
}

/// Every line of `out` that names `needle`, in order.
fn lines_naming(out: &str, needle: &str) -> Vec<String> {
    out.lines()
        .filter(|l| l.contains(needle))
        .map(str::to_string)
        .collect()
}

/// Given a rebuild in progress holding `graph.db.lock` - before its first write, in its fold, in its
/// swap and in its tail - when a second `rigger setup` starts, then it is refused at once with the
/// one refusal text naming the lock, held by another setup or reset, printing nothing else about
/// `graph.db`, never opening the shadow and leaving every `graph.db` file exactly as it found it -
/// no shadow, copy or mark it did not find; and once the rebuild lets go, the next `rigger setup`
/// pays what it left and reaches the graph a cold rebuild of the same log yields, with no rebuild
/// state, shadow or copy.
#[test]
fn a_second_setup_while_a_rebuild_holds_graph_db_lock_is_refused_at_once_and_touches_nothing() {
    use RebuildPhase::*;
    for phase in [Locked, Folding, Swapping, FoldingTheTail] {
        let dir = temp_store_project();
        let root = dir.path();
        closed_run_store(root);
        lose_graph(root);
        if let FoldingTheTail = phase {
            // The state a rebuild leaves once its swap has ended: the cursor it carried into
            // graph.db, and no shadow.
            stop_setup_past_its_prune(root, |root| {
                std::fs::remove_file(rigger_file(root, "graph.db.rebuild")).unwrap();
            });
        }
        let graph_db = rigger_file(root, "graph.db");
        let shadow = rigger_file(&root.canonicalize().unwrap(), "graph.db.rebuild");
        let holder = hold_the_rebuild(root);
        let held_shadow = matches!(phase, Folding | Swapping).then(|| hold_shadow(root));
        if let Swapping = phase {
            std::fs::write(
                rigger_file(root, "graph.db.pruned"),
                b"the copy the swap is using",
            )
            .unwrap();
        }
        let found = graph_files(root);
        let state = tempfile::tempdir().unwrap();
        let mut second = rigger_command(root, &["setup"], &[("RIGGER_NPM", "true")], state.path())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        // How many descriptors the second setup ever held on the shadow while it ran: a setup
        // that read the held shadow would wait on the fixture's exclusive lock, long enough to be
        // seen.
        let pid = second.id();
        let mut shadow_opened = 0;
        let ended = wait_until_for(4800, || {
            shadow_opened = shadow_opened.max(files_open_by(pid, &shadow));
            !matches!(second.try_wait(), Ok(None))
        });
        if !ended {
            cleanup(&mut second);
            panic!("the second setup ({phase:?}) never ended in 120s");
        }
        let refused = second.wait_with_output().unwrap();
        let (out, err) = (
            String::from_utf8_lossy(&refused.stdout).into_owned(),
            String::from_utf8_lossy(&refused.stderr).into_owned(),
        );
        let refused = (
            refused.status.success(),
            lines_naming(&out, "graph.db"),
            err.lines().last().map(str::to_string),
            shadow_opened,
            graph_files(root) == found,
        );
        drop((holder, held_shadow));
        let (paid, paid_err, paid_ok) =
            run_rigger_envs(root, &["setup"], &[("RIGGER_NPM", "true")]);
        let paid_state = (
            paid_ok,
            after_rebuilt(&paid),
            holds_table(&graph_db, "rebuild_cursor"),
            shadow.exists(),
            rigger_file(root, "graph.db.pruned").exists(),
        );
        let paid_graph = whole_graph(root);
        lose_graph(root);
        let (cold, cold_err, cold_ok) =
            run_rigger_envs(root, &["setup"], &[("RIGGER_NPM", "true")]);
        assert_eq!(
            (refused, paid_state, (cold_ok, paid_graph)),
            (
                (
                    false,
                    Vec::<String>::new(),
                    Some(common::cli::rebuild_in_progress_line()),
                    0,
                    true,
                ),
                (
                    true,
                    Some(pruned_from_the_rebuilt_graph(3, 1)),
                    false,
                    false,
                    false,
                ),
                (true, whole_graph(root)),
            ),
            "the second setup ({phase:?}) is refused at once and touches nothing, and the next \
             setup pays the rebuild; refused stdout: {out} stderr: {err}; next stdout: {paid} \
             stderr: {paid_err}; cold stdout: {cold} stderr: {cold_err}"
        );
    }
}

/// Given a `rigger setup` whose rebuild committed its first batches into its shadow and whose
/// process is then gone - killed while it still held `graph.db.lock` - when the next `rigger setup`
/// runs, then the lock is free: that setup takes it, resumes the shadow from its last committed
/// batch - its one progress line counts only the events past it - and reaches the graph a cold
/// rebuild of the same log yields; while the process lived, a setup was refused naming the
/// lock, held by another setup or reset.
#[test]
fn a_rebuild_whose_process_is_gone_leaves_no_lock_and_the_next_setup_resumes_its_shadow() {
    use rigger::contextgraph::sqlite::Projector;
    let dir = temp_store_project();
    let root = dir.path();
    closed_run_store(root);
    lose_graph(root);
    let graph_db = rigger_file(root, "graph.db");
    let project = run_stream_identity(root);
    let log = read_run_events(root);
    let head = log.last().unwrap().position;
    // The rebuild's first two batches of two events are committed into its shadow; it stops.
    let mut batches = 0;
    let backend = Store::open(rigger_file(root, "events.db").to_str().unwrap()).unwrap();
    let mut live = live_selection_of(&backend, &project, 2);
    let stopped = Projector::open(graph_db.to_str().unwrap(), &project)
        .unwrap()
        .rebuild(
            &Projector::lock_rebuild(graph_db.to_str().unwrap()).unwrap(),
            true,
            &mut |after, sink| {
                live(after, &mut |events, head| {
                    batches += 1;
                    if batches > 2 {
                        return Err(rigger::contextgraph::Error("stopped".to_string()));
                    }
                    sink(events, head)
                })
            },
            &mut |_| Ok(None),
            &mut |_| {},
        )
        .map_err(|e| e.to_string());
    drop(live);
    let cursor = rebuild_cursor_of(&rigger_file(root, "graph.db.rebuild"));
    // Its process holds the lock until it is gone.
    let lock = rigger_file(root, "graph.db.lock").canonicalize().unwrap();
    let mut rebuilding = common::fixtures::lock_holder(&lock);
    let held = (
        files_open_by(rebuilding.id(), &lock),
        run_rigger_envs(root, &["setup"], &[("RIGGER_NPM", "true")]),
    );
    cleanup(&mut rebuilding);
    let (out, err, ok) = run_rigger_envs(root, &["setup"], &[("RIGGER_NPM", "true")]);
    let past_the_cursor = log.iter().filter(|e| e.position > cursor).count();
    let resumed_graph = whole_graph(root);
    lose_graph(root);
    let (cold, cold_err, cold_ok) = run_rigger_envs(root, &["setup"], &[("RIGGER_NPM", "true")]);
    assert_eq!(
        (
            stopped,
            (
                held.0,
                held.1 .2,
                held.1 .1.lines().last().map(str::to_string)
            ),
            ok,
            rebuild_progress(&out),
            after_rebuilt(&out),
            rigger_file(root, "graph.db.rebuild").exists(),
            (cold_ok, resumed_graph),
        ),
        (
            Err(common::fixtures::graph_error("event store: stopped")),
            (1, false, Some(common::cli::rebuild_in_progress_line())),
            true,
            vec![rebuilt_through_the_head(past_the_cursor, head)],
            Some(pruned_from_the_rebuilt_graph(3, 1)),
            false,
            (true, whole_graph(root)),
        ),
        "the lock of a gone process is free and the next setup resumes its shadow; stdout: {out} \
         stderr: {err}; cold stdout: {cold} stderr: {cold_err}"
    );
}

/// Given a project whose `graph.db` a `rigger setup` rebuilt, then `graph.db.lock` stands beside it,
/// zero bytes, and no `rigger reset` verb removes it: the bare menu, `--runs`, `--derived`,
/// `--build-cache` and `--scratch-orphans` each leave it where it is.
#[test]
fn graph_db_lock_stands_beside_graph_db_after_setup_and_no_reset_verb_removes_it() {
    let dir = temp_store_project();
    let root = dir.path();
    closed_run_store(root);
    lose_graph(root);
    setup_ok(root, &[("RIGGER_NPM", "true")]);
    let lock = rigger_file(root, "graph.db.lock");
    let stands = || std::fs::metadata(&lock).map(|m| m.len()).ok();
    let mut after = vec![("setup".to_string(), true, stands())];
    // The cache-home reclaim verbs act on a private cache home, never the operator's.
    let cache = tempfile::tempdir().unwrap();
    let cache_home = [("XDG_CACHE_HOME", cache.path().to_str().unwrap())];
    for verb in [
        vec!["reset"],
        vec!["reset", "--runs"],
        vec!["reset", "--derived"],
        vec!["reset", "--build-cache"],
        vec!["reset", "--scratch-orphans"],
    ] {
        let (out, err, ok) = run_rigger_envs(root, &verb, &cache_home);
        assert!(ok, "{verb:?} runs; stdout: {out} stderr: {err}");
        after.push((verb.join(" "), ok, stands()));
    }
    assert_eq!(
        after,
        [
            "setup",
            "reset",
            "reset --runs",
            "reset --derived",
            "reset --build-cache",
            "reset --scratch-orphans"
        ]
        .map(|verb| (verb.to_string(), true, Some(0)))
        .to_vec(),
        "graph.db.lock stands, zero bytes, through every reset verb"
    );
}

/// Given history recorded before the project minted its durable identity, and a rebuild in
/// progress holding `graph.db.lock`, when `rigger setup` runs, then it is refused before its
/// identity migration, in the operator's words naming the lock another setup or reset holds: it
/// renames no stream and records no migration decision, prints nothing about `graph.db`, and
/// leaves every `graph.db` file as it found them; once the rebuild lets go, the next `rigger
/// setup` migrates the history and records the migration's decision on the minted stream.
#[test]
fn a_setup_refused_by_a_rebuild_in_progress_migrates_nothing_and_names_it_in_its_own_words() {
    let dir = temp_store_project();
    let root = dir.path();
    let (legacy, minted) = legacy_history_then_minted_identity(root);
    let streams = || (ids_under(root, &legacy), ids_under(root, &minted));
    let migrated = |err: &str| {
        err.lines()
            .filter(|l| l.starts_with("rigger: migrated project identity"))
            .count()
    };

    let holder = hold_the_rebuild(root);
    let found = graph_files(root);
    let (out, err, ok) = run_rigger_envs(root, &["setup"], &[("RIGGER_NPM", "true")]);
    let refused = (
        ok,
        lines_naming(&out, "graph.db"),
        migrated(&err),
        err.lines().last().map(str::to_string),
        streams(),
        graph_files(root) == found,
    );
    drop(holder);
    let (paid, paid_err, paid_ok) = run_rigger_envs(root, &["setup"], &[("RIGGER_NPM", "true")]);
    assert_eq!(
        (refused, (paid_ok, migrated(&paid_err), streams())),
        (
            (
                false,
                Vec::<String>::new(),
                0,
                Some(common::cli::rebuild_in_progress_line()),
                (vec!["d-legacy".to_string()], Vec::<String>::new()),
                true,
            ),
            (
                true,
                1,
                (
                    Vec::<String>::new(),
                    vec![
                        "d-legacy".to_string(),
                        format!("identity-migration-{minted}")
                    ]
                )
            ),
        ),
        "the refused setup migrates nothing and names the lock another setup or reset holds, and \
         the next setup migrates; refused stdout: {out} stderr: {err}; next stdout: {paid} \
         stderr: {paid_err}"
    );
}

/// The ids the run stream of `root` holds under the namespace `identity`, in log order.
fn ids_under(root: &Path, identity: &str) -> Vec<String> {
    let backend = Store::open(rigger_file(root, "events.db").to_str().unwrap()).unwrap();
    Namespaced::new(&backend, identity)
        .read_stream(
            rigger::conductor::STREAM,
            0,
            rigger::eventstore::Direction::Forward,
        )
        .unwrap()
        .iter()
        .map(|e| {
            serde_json::from_slice::<serde_json::Value>(&e.data).unwrap()["id"]
                .as_str()
                .unwrap()
                .to_string()
        })
        .collect()
}

/// Given a `graph.db.lock` that cannot be opened - a directory stands at its path - when `rigger
/// setup` runs over the project's `graph.db`, then it fails naming `.rigger/graph.db.lock`, prints
/// nothing about `graph.db` and leaves every `graph.db` file as it found them - the graph and the
/// directory at the lock's path, nothing else; when `rigger reset --runs` runs with no stale copy
/// beside the graph, it never tries the lock and prunes as ever; and with a stale
/// `graph.db.pruned` beside it, `rigger reset --runs` fails naming the lock, and beside the graph
/// stand only the lock's directory and the copy, byte for byte. `rigger --help` says the copy is
/// kept while another `rigger setup` or `rigger reset` holds `graph.db.lock`.
#[test]
fn a_graph_db_lock_that_cannot_be_opened_is_named_by_setup_and_by_reset_runs_over_a_stale_copy() {
    let dir = temp_store_project();
    let root = dir.path();
    closed_run_store(root);
    let lock = rigger_file(root, "graph.db.lock");
    // The cold-start setup took the rebuild lock and made its zero-byte file: a directory takes
    // its place.
    std::fs::remove_file(&lock).unwrap();
    std::fs::create_dir(&lock).unwrap();
    let copy = rigger_file(root, "graph.db.pruned");

    let found = graph_files(root);
    // What setup found, by name and whether it is a file: the snapshot it must leave as it was.
    let found_kinds: Vec<(String, bool)> = found
        .iter()
        .map(|(name, bytes)| (name.clone(), bytes.is_some()))
        .collect();
    let (out, err, ok) = run_rigger_envs(root, &["setup"], &[("RIGGER_NPM", "true")]);
    let setup = (
        ok,
        lines_naming(&out, "graph.db"),
        err.lines().last().map(str::to_string),
        found_kinds,
        graph_files(root) == found,
    );
    let (no_copy, no_copy_err, no_copy_ok) = run_rigger(root, &["reset", "--runs"]);
    let no_copy = (
        no_copy_ok,
        lines_naming(&no_copy, "graph.db"),
        no_copy
            .lines()
            .find(|l| l.starts_with("reset --runs: pruned"))
            .and_then(|l| l.split(", then").next())
            .map(str::to_string),
    );
    std::fs::write(&copy, b"left by a swap that stopped").unwrap();
    let (over_copy, over_copy_err, over_copy_ok) = run_rigger(root, &["reset", "--runs"]);
    let over_copy = (
        over_copy_ok,
        lines_naming(&over_copy, "graph.db"),
        over_copy_err.lines().last().map(str::to_string),
        // Every `graph.db` file but the graph itself, with its bytes.
        graph_files(root)
            .into_iter()
            .filter(|(name, _)| name != "graph.db")
            .collect::<Vec<_>>(),
    );
    // The usage goes to stderr.
    let (_, help, help_ok) = run_rigger(root, &["--help"]);
    let help = (
        help_ok,
        help.lines()
            .skip_while(|l| !l.trim_start().starts_with("rigger reset --runs"))
            .take_while(|l| !l.trim_start().starts_with("rigger reset --derived"))
            .map(str::trim)
            .collect::<Vec<_>>()
            .split_last()
            .map(|(last, _)| last.to_string()),
    );
    let named = |lock: &Path| {
        common::cli::refusal_line(&common::fixtures::directory_refusal(lock.display()))
    };
    assert_eq!(
        (setup, no_copy, over_copy, help),
        (
            (
                false,
                Vec::<String>::new(),
                // Setup names the lock file relative to the project root it runs in.
                Some(named(&rigger_file(Path::new(""), "graph.db.lock"))),
                vec![
                    ("graph.db".to_string(), true),
                    ("graph.db.lock".to_string(), false),
                ],
                true
            ),
            (
                true,
                Vec::<String>::new(),
                Some(
                    "reset --runs: pruned 3 dead-run node(s) and reclaimed 1 superseded edge(s) \
                     from the context graph"
                        .to_string()
                )
            ),
            (
                false,
                Vec::<String>::new(),
                Some(named(&lock.canonicalize().unwrap())),
                vec![
                    ("graph.db.lock".to_string(), None),
                    (
                        "graph.db.pruned".to_string(),
                        Some(b"left by a swap that stopped".to_vec())
                    ),
                ]
            ),
            (
                true,
                Some(
                    "left, unless another `rigger setup` or `rigger reset` holds graph.db.lock"
                        .to_string()
                )
            ),
        ),
        "setup and reset --runs over a stale copy name the lock file they cannot open, and reset \
         --runs with no copy never tries it; setup stdout: {out} stderr: {err}; no-copy stderr: \
         {no_copy_err}; over-copy stderr: {over_copy_err}"
    );
}

/// The shadow of the `graph.db` of `root`, held open mid-batch as a rebuild in its fold holds it,
/// until the connection is dropped. The connection is EXCLUSIVE as the test's detector for a setup
/// that reads the shadow - that setup would wait on it - not as a lock a rebuild holds through its
/// fold: a second setup is refused at `graph.db.lock` ([`hold_the_rebuild`]).
fn hold_shadow(root: &Path) -> rusqlite::Connection {
    let rebuilding = rigger_db(root, "graph.db.rebuild");
    rebuilding
        .execute_batch("PRAGMA locking_mode = EXCLUSIVE; CREATE TABLE folding (x);")
        .unwrap();
    rebuilding
}

/// Append each of `events` to the run stream of `root` and fold it into its `graph.db` as it lands,
/// as the conductor records a run: the live graph, never one a rebuild produced.
fn fold_run_events(root: &Path, events: &[(&str, &str)]) {
    let graph = rigger::contextgraph::sqlite::Projector::open(
        rigger_file(root, "graph.db").to_str().unwrap(),
        &run_stream_identity(root),
    )
    .unwrap();
    common::cli::with_run_store(root, |store| {
        let folding = rigger::ingest::folding_into(store, Some(&graph), &|_| {});
        for &(ty, body) in events {
            let done = folding
                .append_and_fold(
                    rigger::conductor::STREAM,
                    ExpectedRevision::Any,
                    &[Event::new(ty, body.as_bytes().to_vec())],
                )
                .unwrap();
            assert_eq!(done.fold, Fold::Folded, "the {ty} folds as it lands");
        }
    });
}

/// The reset menu's `--runs` line counting `nodes` dead-run nodes and `edges` superseded edges.
fn runs_prunable(nodes: usize, edges: usize) -> String {
    format!(
        "--runs: {nodes} dead-run node(s) and {edges} superseded edge(s) prunable from the context \
         graph; rerun `rigger reset --runs` to reclaim them"
    )
}

/// The `--runs` line of the `rigger reset` menu of `root`: what the context graph holds to prune.
fn menus_runs_prune(root: &Path) -> String {
    let (out, err, ok) = run_rigger(root, &["reset"]);
    assert!(ok, "the reset menu answers; stdout: {out} stderr: {err}");
    out.lines()
        .find(|l| l.starts_with("--runs:"))
        .unwrap_or_else(|| panic!("the menu names --runs; stdout: {out}"))
        .to_string()
}

/// The whole graph the `graph.db` of `root` holds - every node and edge - as JSON.
fn whole_graph(root: &Path) -> String {
    let graph = rigger::contextgraph::sqlite::Projector::open(
        rigger_file(root, "graph.db").to_str().unwrap(),
        &run_stream_identity(root),
    )
    .unwrap();
    common::fixtures::whole_json(&graph)
}

/// Given a `rigger setup` whose rebuild swapped its pruned graph in and stopped before it folded
/// the tail - `graph.db` standing, holding the rebuild's cursor, no shadow beside it and no holder
/// of the rebuild lock - over a log holding derived recordings the derived reset would shed, when
/// `rigger reset --derived` runs, then it is refused for the unfinished rebuild, naming `rigger
/// setup`, printing nothing else and leaving the event log and every `graph.db` file byte for
/// byte; a graph that owes its rebuild as well is refused in the same words, the unfinished
/// rebuild being asked before the debt; and once `rigger setup` has finished the rebuild and
/// dropped the cursor the same command migrates them.
#[test]
fn reset_derived_refuses_a_rebuild_stopped_in_its_tail_until_setup_finishes_it() {
    let dir = temp_store_project();
    let root = dir.path();
    closed_run_store(root);
    common::cli::seed_derived_duplicates(root);
    lose_graph(root);
    let (stopped, stopped_ok) = stop_setup_past_its_prune(root, |root| {
        std::fs::remove_file(rigger_file(root, "graph.db.rebuild")).unwrap();
    });
    let graph_db = rigger_file(root, "graph.db");
    // The refused command, over the log and the graph files as it found them.
    let refused = || refused_derived_reset(root, &log_and_graph_files(root));
    let unfinished = || {
        (
            false,
            String::new(),
            common::cli::unfinished_rebuild_refusal() + "\n",
            true,
        )
    };

    let stopped_in_its_tail = (
        stopped_ok,
        holds_table(&graph_db, "rebuild_cursor"),
        rigger_file(root, "graph.db.rebuild").exists(),
    );
    let in_its_tail = refused();
    common::fixtures::owe_a_rebuild(&common::cli::open_graph(root));
    let owing_too = refused();
    let (out, err, finished_ok) = run_rigger_envs(root, &["setup"], &[("RIGGER_NPM", "true")]);
    let finished = (finished_ok, holds_table(&graph_db, "rebuild_cursor"));
    let (shed, shed_err, shed_ok) = run_rigger(root, &["reset", "--derived"]);
    assert_eq!(
        (
            stopped_in_its_tail,
            in_its_tail,
            owing_too,
            finished,
            (shed_ok, shed),
        ),
        (
            (false, true, false),
            unfinished(),
            unfinished(),
            (true, false),
            (
                true,
                format!("{}{LOG_LEFT_AS_IT_STANDS_LINE}", migrated_lines(1, 3, 0)),
            ),
        ),
        "a graph file holding the rebuild's cursor refuses the derived reset, before its debt is \
         asked, until setup has finished the rebuild; stopped setup stdout: {stopped}; finishing \
         setup stdout: {out} stderr: {err}; reset stderr: {shed_err}"
    );
}
