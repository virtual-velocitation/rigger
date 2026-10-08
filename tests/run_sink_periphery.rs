//! Periphery (contract / API / integration) tests for spec 107 criterion 10, THE RUN'S SINK
//! RECORDS PERCEPTION AS A LEDGER ENTRY. These run OUTSIDE the crates, over the root crate's
//! public surface, and guard what the in-crate sink tests are structurally blind to.
//!
//! The in-crate tests hand batches to the sink over a `:memory:` store and a `:memory:` graph and
//! a hash function they invent. Here a whole `conductor::run` walks a committed tree into an
//! `events.db` file and a `graph.db` file, under the one hash function the binary binds
//! (`worktree::hash_blob`), and every assertion is on what those files hold afterwards: the
//! entries the log carries, the `applied` row each one left, the generation a later open of the
//! graph answers, and the facts a re-recording did not touch.
//!
//! SINK OUTCOMES row 13 is the one row only an older store reaches: an identity whose derived
//! rows were recorded before the group stamp answers no generation at the group lookup while
//! `graph.db` already holds the generation those rows folded. Its first walk records one entry,
//! and that entry folds as a re-recording - its `applied` row and no fact.

#![cfg(feature = "symbols")]

mod common;

use std::path::Path;
use std::sync::Mutex;

use common::cli::applied_positions;
use common::fixtures::{
    agent, entry_records, git_commit_all, git_hash_object, live_edges, minted_events,
    temp_git_project_with_commit, walked_entry_events, write_text, NoopDriver, DOCUMENT_BODY,
    DOCUMENT_PATH, SOURCE_BODY, SOURCE_PATH, TEST_MODULE_BODY, TEST_MODULE_PATH,
};
use rigger::conductor::{run, Deps, STREAM};
use rigger::config::{Config, Stage};
use rigger::contextgraph::sqlite::Projector;
use rigger::contextgraph::{Fold, Projection};
use rigger::eventstore::sqlite::Store;
use rigger::eventstore::{Direction, Event, EventStore, ExpectedRevision, META_GROUP};
use rigger::gate::ExecRunner;
use rigger::ingest::{
    derived_key_parts, folding_into, is_derived_index_type, keyed_derived_event, latest_generation,
};
use rigger::retention::{GenerationIngested, TYPE_GENERATION_INGESTED};

/// A committed git project holding a source file, the out-of-line test module it declares and a
/// design document.
fn committed_tree() -> tempfile::TempDir {
    let dir = temp_git_project_with_commit();
    for (path, body) in [
        (SOURCE_PATH, SOURCE_BODY),
        (TEST_MODULE_PATH, TEST_MODULE_BODY),
        (DOCUMENT_PATH, DOCUMENT_BODY),
    ] {
        write_text(dir.path(), path, body);
    }
    git_commit_all(dir.path(), "tree");
    dir
}

/// One entry a run records, as [`entry_records`] answers it.
type Recorded = (GenerationIngested, String, String);

/// What a run records for the tree at `root` as it stands and nothing recorded, each entry's
/// blob the id `git hash-object` gives the bytes the tree holds.
fn walked(root: &Path) -> Vec<Recorded> {
    entry_records(&walked_entry_events(root, |file| {
        git_hash_object(root, file, false)
    }))
}

/// The identity each of `recorded` is grouped under.
fn identities(recorded: &[Recorded]) -> Vec<&str> {
    recorded
        .iter()
        .map(|(_, group, _)| group.as_str())
        .collect()
}

/// The generation each of `recorded` names, as a lookup answers it.
fn generations(recorded: &[Recorded]) -> Vec<Option<String>> {
    recorded
        .iter()
        .map(|(entry, ..)| Some(entry.generation.clone()))
        .collect()
}

/// The sqlite files one project's runs record into, and what each run said of a lost fold.
struct Files {
    dir: tempfile::TempDir,
    said: Mutex<Vec<String>>,
}

impl Files {
    fn new() -> Self {
        Files {
            dir: tempfile::tempdir().unwrap(),
            said: Mutex::new(Vec::new()),
        }
    }

    fn graph_db(&self) -> std::path::PathBuf {
        self.dir.path().join("graph.db")
    }

    fn store(&self) -> Store {
        Store::open(self.dir.path().join("events.db").to_str().unwrap()).unwrap()
    }

    fn graph(&self) -> Projector {
        Projector::open(self.graph_db().to_str().unwrap(), "test").unwrap()
    }

    /// One whole run over the tree at `root`, a fresh campaign named by `criterion`, each port a
    /// fresh open of the files and the hash function the one the binary binds. It must succeed.
    fn run_over(&self, root: &Path, criterion: &str) {
        let mut cfg = Config::default();
        cfg.agents.insert("a".into(), agent("a"));
        cfg.workflow.stages.insert(
            "s".into(),
            Stage {
                name: "s".into(),
                agent: "a".into(),
                coverage: criterion.into(),
                ..Default::default()
            },
        );
        let (store, graph) = (self.store(), self.graph());
        let log = |line: &str| self.said.lock().unwrap().push(line.to_string());
        let hash_blob = |bytes: &[u8]| rigger::worktree::hash_blob(root, bytes);
        let deps = Deps {
            store: &store,
            driver: &NoopDriver,
            gates: &ExecRunner,
            repo: root.to_str().unwrap().to_string(),
            grounder: None,
            graph: Some(&graph),
            criteria: vec![criterion.to_string()],
            log: &log,
            hash_blob: &hash_blob,
        };
        run(&cfg, &deps).unwrap();
    }

    /// The run stream, oldest first.
    fn log(&self) -> Vec<Event> {
        self.store()
            .read_stream(STREAM, 0, Direction::Forward)
            .unwrap()
    }

    /// The generation the log's group lookup answers for each of `identities`, in order.
    fn logged(&self, identities: &[&str]) -> Vec<Option<String>> {
        let store = self.store();
        identities
            .iter()
            .map(|identity| latest_generation(&store, STREAM, identity).unwrap())
            .collect()
    }

    /// The generation a fresh open of `graph.db` holds for each of `identities`, in order.
    fn held(&self, identities: &[&str]) -> Vec<Option<String>> {
        let graph = self.graph();
        identities
            .iter()
            .map(|identity| graph.current_generation(identity).unwrap())
            .collect()
    }

    /// The position of each ledger entry the log carries, in log order.
    fn entry_positions(&self) -> Vec<u64> {
        self.log()
            .iter()
            .filter(|e| e.type_ == TYPE_GENERATION_INGESTED)
            .map(|e| e.position)
            .collect()
    }

    /// Each of `positions` as many times as `graph.db`'s `applied` table holds it.
    fn applied_among(&self, positions: &[u64]) -> Vec<u64> {
        applied_positions(&self.graph_db())
            .into_iter()
            .filter(|position| positions.contains(position))
            .collect()
    }
}

/// How many derived index events `events` carry.
fn derived_count(events: &[Event]) -> usize {
    events
        .iter()
        .filter(|e| is_derived_index_type(&e.type_))
        .count()
}

/// GIVEN a committed tree holding a source file, the out-of-line test module it declares and a
/// design document, and an empty `events.db` and `graph.db`,
/// WHEN a run walks the tree,
/// THEN the log holds one ledger entry per batch the walk extracts and no derived event - each
/// entry's generation its batch's, its blob the id `git hash-object` gives the file's bytes, its
/// flag the walk's (set for the test module's code batch alone), under its identity as the group
/// and its generation and event count in the replay key; `graph.db` holds exactly one `applied`
/// row per entry and, opened again, each identity's generation; and no fold was lost.
/// AND WHEN a second run, a fresh campaign, walks the unchanged tree, THEN it records no entry.
#[test]
fn a_run_records_one_ledger_entry_per_batch_and_one_applied_row_per_entry_in_the_files() {
    let tree = committed_tree();
    let root = tree.path();
    let walked = walked(root);
    assert_eq!(
        walked
            .iter()
            .map(|(entry, group, _)| (group.as_str(), entry.excluded))
            .collect::<Vec<_>>(),
        vec![
            ("gc/src/checks.rs", true),
            ("gc/src/lib.rs", false),
            ("gd/docs/architecture.md", false),
            ("gd/src/checks.rs", false),
            ("gd/src/lib.rs", false),
        ],
        "sanity: the batches the walk extracts, the test module's code batch alone flagged"
    );
    let files = Files::new();

    files.run_over(root, "first criterion");

    let log = files.log();
    assert_eq!(entry_records(&log), walked);
    assert_eq!(derived_count(&log), 0);
    let positions = files.entry_positions();
    assert_eq!(positions.len(), walked.len());
    assert_eq!(files.applied_among(&positions), positions);
    assert_eq!(files.held(&identities(&walked)), generations(&walked));
    assert_eq!(files.logged(&identities(&walked)), generations(&walked));
    assert_eq!(*files.said.lock().unwrap(), Vec::<String>::new());

    files.run_over(root, "second criterion");

    let log = files.log();
    assert_eq!(
        log.iter().filter(|e| e.type_ == "RunStarted").count(),
        2,
        "sanity: the second run is a campaign of its own"
    );
    assert_eq!(entry_records(&log), walked);
    assert_eq!(files.entry_positions(), positions);
    assert_eq!(derived_count(&log), 0);
}

/// SINK OUTCOMES rows 9 then 13.
///
/// GIVEN a store recorded before the ledger and before the group stamp: every batch of the tree
/// as it stands recorded as derived rows that carry their replay key and no group, each folded
/// into `graph.db` - so the group lookup answers no generation for any identity while the graph
/// holds each one's current generation,
/// WHEN a run walks the tree for the first time,
/// THEN the run succeeds and records one entry per identity, at the generation the graph already
/// holds, from the bytes it read; each entry folds as a RE-RECORDING: `graph.db` takes its one
/// `applied` row and no fact - every live edge still names the derived row that asserted it - and
/// no fold is reported lost; the group lookup then answers every identity from its entry,
/// AND a second run records nothing.
#[test]
fn an_identity_whose_pre_ledger_rows_carry_no_group_records_an_entry_that_folds_as_a_re_recording()
{
    let tree = committed_tree();
    let root = tree.path();
    let walked = walked(root);
    let files = Files::new();
    {
        let (store, graph) = (files.store(), files.graph());
        let folding = folding_into(&store, Some(&graph as &dyn Projection), &|_| {});
        let minted = minted_events(root);
        let identity_of = |key: &str| derived_key_parts(key).unwrap().0.to_string();
        for batch in minted.chunk_by(|(a, _), (b, _)| identity_of(a) == identity_of(b)) {
            let rows: Vec<Event> = batch
                .iter()
                .map(|(key, event)| {
                    let mut row = keyed_derived_event(event.clone(), key);
                    assert_eq!(row.meta.remove(META_GROUP), Some(identity_of(key)));
                    row
                })
                .collect();
            let done = folding
                .append_and_fold(STREAM, ExpectedRevision::Any, &rows)
                .unwrap();
            assert_eq!(done.fold, Fold::Folded);
        }
    }
    let pre_ledger = files.log();
    let last_pre_ledger = pre_ledger.last().unwrap().position;
    assert_eq!(derived_count(&pre_ledger), pre_ledger.len());
    assert_eq!(
        files.logged(&identities(&walked)),
        vec![None; walked.len()],
        "rows that carry no group answer no generation at the group lookup"
    );
    assert_eq!(
        files.held(&identities(&walked)),
        generations(&walked),
        "the graph holds the generation the pre-ledger rows folded"
    );
    let facts = live_edges(&files.graph());
    assert_ne!(
        facts,
        Vec::new(),
        "sanity: the pre-ledger rows folded facts"
    );

    files.run_over(root, "first criterion");

    let log = files.log();
    assert_eq!(entry_records(&log), walked);
    assert_eq!(derived_count(&log), pre_ledger.len());
    let positions = files.entry_positions();
    assert_eq!(positions.len(), walked.len());
    assert!(
        positions.iter().all(|position| *position > last_pre_ledger),
        "sanity: every entry follows the pre-ledger rows"
    );
    assert_eq!(files.applied_among(&positions), positions);
    assert_eq!(
        live_edges(&files.graph()),
        facts,
        "a re-recording writes no fact: the live edges are the ones the rows asserted, each \
         still naming its row"
    );
    assert_eq!(files.held(&identities(&walked)), generations(&walked));
    assert_eq!(files.logged(&identities(&walked)), generations(&walked));
    assert_eq!(*files.said.lock().unwrap(), Vec::<String>::new());

    files.run_over(root, "second criterion");

    assert_eq!(files.entry_positions(), positions);
}
