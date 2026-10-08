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
//!
//! Beyond those two walks: a revert A, B, A recorded by three whole runs over the files; a run
//! over a `graph.db` that owes its rebuild; and the public sink answer, `ingest::entry_of_batch`,
//! asked by an outside caller over the store's group lookup, a graph file and the real hash -
//! closed through the entry's constructor and the ledger form until the batch is current, and
//! for the rows that record no blob or fail the emit.
//!
//! Criterion 11, THE RUN'S SINK MEMOIZES THE LOG SIDE, is guarded here at the one seam an outside
//! caller reaches the memo through. The in-crate tests hand a batch to the private sink twice; a
//! whole `conductor::run` hands an identity's batch to its sink again when its whole-tree walk is
//! followed by the reindex of an integration naming the file. The tests at the end of this file
//! count the group lookups such a run makes at the store it was handed
//! (`CountedRead::LatestInGroup`): one per walked identity and none for a reindex, over an empty
//! store, a recorded one and row 13's fixture, a revert between two reindexes still recorded; and
//! an identity a rebuild of `graph.db` left behind mid-run recorded again at the next reindex
//! with no group lookup.

#![cfg(feature = "symbols")]

mod common;

use std::path::Path;
use std::sync::Mutex;

use common::cli::applied_positions;
use common::fixtures::{
    agent, arm_read_fault, derived_count, entry_records, gate_def, generation_ingested,
    git_commit_all, git_hash_object, held_generations, live_edges, logged_generations,
    one_lookup_each, owe_a_rebuild, rebuild_from_the_tree, seed_pre_ledger_rows_without_a_group,
    source_with, temp_git_project_with_commit, walked_git_entry_records, wire_owned, write_text,
    CountedRead, EntryRecord, Handed, NoopDriver, ReadCountingStore, DOCUMENT_BODY, DOCUMENT_PATH,
    MOVED, REWORDED, SOURCE, SOURCE_BODY, SOURCE_PATH, TEST_MODULE_BODY, TEST_MODULE_PATH,
};
use rigger::conductor::{run, AgentDriver, AgentResult, Deps, Error, SpawnOpts, STREAM};
use rigger::config::{AgentDef, Config, Stage};
use rigger::contextgraph::sqlite::Projector;
use rigger::contextgraph::{EntryFold, Fold, Projection, REBUILD_OWED};
use rigger::eventstore::sqlite::Store;
use rigger::eventstore::{Direction, Event, EventStore};
use rigger::gate::ExecRunner;
use rigger::ingest::{
    batch_is_current, entry_of_batch, folding_into, ingest_files_batched, latest_generation,
    EntryFailure, GraphSide,
};
use rigger::ledger::{RunState, Status};
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

/// The identity each of `recorded` is grouped under.
fn identities(recorded: &[EntryRecord]) -> Vec<&str> {
    recorded
        .iter()
        .map(|(_, group, _)| group.as_str())
        .collect()
}

/// The generation each of `recorded` names, as a lookup answers it.
fn generations(recorded: &[EntryRecord]) -> Vec<Option<String>> {
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
        self.run_campaign(root, criterion, &self.store());
    }

    /// [`run_over`](Files::run_over) with the run's log read and written through `store`, a
    /// view of this project's `events.db`.
    fn run_campaign(&self, root: &Path, criterion: &str, store: &dyn EventStore) {
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
        self.run_through(root, &cfg, &[criterion], &NoopDriver, store);
    }

    /// One whole run of `cfg` over the tree at `root`, its agents spawned through `driver` and
    /// its log read and written through `store` - a view of this project's `events.db` - with a
    /// fresh open of `graph.db` and the hash function the binary binds. It must succeed.
    fn run_through(
        &self,
        root: &Path,
        cfg: &Config,
        criteria: &[&str],
        driver: &dyn AgentDriver,
        store: &dyn EventStore,
    ) -> RunState {
        let graph = self.graph();
        let log = |line: &str| self.said.lock().unwrap().push(line.to_string());
        let hash_blob = |bytes: &[u8]| rigger::worktree::hash_blob(root, bytes);
        let deps = Deps {
            store,
            driver,
            gates: &ExecRunner,
            repo: root.to_str().unwrap().to_string(),
            grounder: None,
            graph: Some(&graph),
            criteria: criteria.iter().map(|c| c.to_string()).collect(),
            log: &log,
            hash_blob: &hash_blob,
        };
        run(cfg, &deps).unwrap()
    }

    /// The run stream, oldest first.
    fn log(&self) -> Vec<Event> {
        self.store()
            .read_stream(STREAM, 0, Direction::Forward)
            .unwrap()
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
    let walked = walked_git_entry_records(root);
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
    assert_eq!(
        held_generations(&files.graph(), &identities(&walked)),
        generations(&walked)
    );
    assert_eq!(
        logged_generations(&files.store(), STREAM, &identities(&walked)),
        generations(&walked)
    );
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
    let walked = walked_git_entry_records(root);
    let files = Files::new();
    seed_pre_ledger_rows_without_a_group(root, &files.store(), &files.graph());
    let pre_ledger = files.log();
    let last_pre_ledger = pre_ledger.last().unwrap().position;
    assert_eq!(derived_count(&pre_ledger), pre_ledger.len());
    assert_eq!(
        logged_generations(&files.store(), STREAM, &identities(&walked)),
        vec![None; walked.len()],
        "rows that carry no group answer no generation at the group lookup"
    );
    assert_eq!(
        held_generations(&files.graph(), &identities(&walked)),
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
    assert_eq!(
        held_generations(&files.graph(), &identities(&walked)),
        generations(&walked)
    );
    assert_eq!(
        logged_generations(&files.store(), STREAM, &identities(&walked)),
        generations(&walked)
    );
    assert_eq!(*files.said.lock().unwrap(), Vec::<String>::new());

    files.run_over(root, "second criterion");

    assert_eq!(files.entry_positions(), positions);
}

/// The one entry of `recorded` under `identity`.
fn entry_under(recorded: &[EntryRecord], identity: &str) -> EntryRecord {
    let under: Vec<&EntryRecord> = recorded
        .iter()
        .filter(|(_, group, _)| group == identity)
        .collect();
    assert_eq!(under.len(), 1, "one entry under {identity}");
    under[0].clone()
}

/// The live edges of `graph` with the position and valid-time each was asserted at left out: the
/// facts themselves.
fn facts(graph: &Projector) -> Vec<(String, String, String)> {
    live_edges(graph)
        .into_iter()
        .map(|(from, rel, to, ..)| (from, rel, to))
        .collect()
}

/// A revert A, B, A, each generation recorded by a whole run of its own over the files.
///
/// GIVEN a committed tree a first run recorded at body A of its source file,
/// WHEN the file is rewritten to body B and a second run walks the tree, then rewritten to body A
/// and a third run walks it - each run a fresh open of `events.db` and `graph.db` under the hash
/// function the binary binds,
/// THEN the log holds three entries under the file's code identity, in order, at generations
/// A, B, A with the blobs `git hash-object` gives bodies A, B, A - the third recorded although the
/// log already holds its generation - and no other entry moved; `graph.db` holds one `applied`
/// row per entry, the identity's generation A, and exactly the facts the first run left, which
/// are not the facts body B left; no fold was lost; and a fourth run records nothing.
#[test]
fn a_revert_a_b_a_across_three_runs_records_three_entries_with_their_blobs_and_leaves_as_facts() {
    let tree = committed_tree();
    let root = tree.path();
    let files = Files::new();
    let first_walk = walked_git_entry_records(root);
    let a = entry_under(&first_walk, SOURCE);

    files.run_over(root, "first criterion");
    let facts_a = facts(&files.graph());

    write_text(root, SOURCE_PATH, &source_with(MOVED));
    let b = entry_under(&walked_git_entry_records(root), SOURCE);
    let keys = |recorded: &[EntryRecord]| -> Vec<String> {
        recorded.iter().map(|(.., key)| key.clone()).collect()
    };
    assert_eq!(
        keys(&walked_git_entry_records(root))
            .into_iter()
            .filter(|key| !keys(&first_walk).contains(key))
            .collect::<Vec<_>>(),
        vec![b.2.clone()],
        "sanity: the second body moves the file's code batch and no other"
    );
    assert_ne!(a.0.generation, b.0.generation);
    assert_ne!(a.0.blob, b.0.blob);
    files.run_over(root, "second criterion");
    let facts_b = facts(&files.graph());
    assert_eq!(
        held_generations(&files.graph(), &[SOURCE]),
        vec![Some(b.0.generation.clone())]
    );

    write_text(root, SOURCE_PATH, SOURCE_BODY);
    assert_eq!(
        walked_git_entry_records(root),
        first_walk,
        "sanity: the tree is back at A"
    );
    files.run_over(root, "third criterion");

    let recorded = entry_records(&files.log());
    assert_eq!(
        recorded,
        [first_walk.clone(), vec![b.clone(), a.clone()]].concat()
    );
    let positions = files.entry_positions();
    assert_eq!(positions.len(), first_walk.len() + 2);
    assert_eq!(files.applied_among(&positions), positions);
    assert_eq!(
        held_generations(&files.graph(), &[SOURCE]),
        vec![Some(a.0.generation.clone())]
    );
    assert_eq!(
        logged_generations(&files.store(), STREAM, &[SOURCE]),
        vec![Some(a.0.generation.clone())]
    );
    assert_ne!(facts_b, facts_a, "sanity: body B left facts of its own");
    assert_eq!(facts(&files.graph()), facts_a);
    assert_eq!(derived_count(&files.log()), 0);
    assert_eq!(*files.said.lock().unwrap(), Vec::<String>::new());

    files.run_over(root, "fourth criterion");

    assert_eq!(files.entry_positions(), positions);
}

/// A batch's events as a test compares them: each its type and payload text.
type Typed = Vec<(String, String)>;

/// The batch an integration reindex of `root` naming the source file hands for its code
/// identity.
fn reindexed(root: &Path) -> Handed {
    Handed::by(
        |sink| {
            ingest_files_batched(root.to_str().unwrap(), &[SOURCE_PATH.to_string()], sink);
        },
        SOURCE,
    )
}

impl Handed {
    /// The batch's events, each as its type and payload.
    fn events(&self) -> Typed {
        let events: Vec<Event> = self.keyed.iter().map(|(_, event)| event.clone()).collect();
        wire_owned(&events)
    }

    /// What the public sink answer records for this batch over `files` under `hash`: the entry
    /// and the extraction's events, nothing, or the failure's own wording.
    fn answer(
        &self,
        root: &Path,
        files: &Files,
        hash: &rigger::ingest::HashBlob,
    ) -> Result<Option<(GenerationIngested, Typed)>, String> {
        let (store, graph) = (files.store(), files.graph());
        entry_of_batch(
            root,
            &self.as_keyed(),
            self.excluded,
            |identity| latest_generation(&store, STREAM, identity),
            &graph,
            hash,
        )
        .map(|recorded| recorded.map(|of| (of.entry, wire_owned(&of.batch))))
        .map_err(|failure: EntryFailure| failure.to_string())
    }
}

/// A hash function that fails the test when asked.
fn unasked(_bytes: &[u8]) -> Result<String, rigger::worktree::Error> {
    panic!("the hash is asked only for an extraction the sink records from bytes")
}

/// The public sink answer, closed through the entry's constructor and the ledger form.
///
/// GIVEN a committed tree, an empty `events.db` and `graph.db`, and the batch an integration
/// reindex hands for the source file,
/// WHEN an outside caller asks `ingest::entry_of_batch` what to record, over the store's group
/// lookup, the graph file and `worktree::hash_blob`,
/// THEN it answers the entry of the file's bytes - the walk's generation, the blob
/// `git hash-object` gives, the walk's flag - and the walk's own batch, and the pure predicate
/// answers not current over the two sides read back;
/// AND WHEN that entry's constructor event is appended and folded through the ledger form,
/// THEN the fold is made, asking for the batch; both sides hold the generation; the predicate
/// answers current; and the same batch asked again records nothing and asks no hash.
#[test]
fn the_public_sink_answer_records_through_the_constructor_and_the_ledger_form_until_current() {
    let tree = committed_tree();
    let root = tree.path();
    let files = Files::new();
    let handed = reindexed(root);
    let generation = handed.generation();
    let hash = |bytes: &[u8]| rigger::worktree::hash_blob(root, bytes);
    let sides = || {
        (
            logged_generations(&files.store(), STREAM, &[SOURCE]).remove(0),
            held_generations(&files.graph(), &[SOURCE]).remove(0),
        )
    };
    let current = |(logged, held): (Option<String>, Option<String>)| {
        batch_is_current(
            logged.as_deref(),
            GraphSide::Holds(held.as_deref()),
            &generation,
        )
    };
    assert_eq!(sides(), (None, None));
    assert!(!current(sides()));

    let (entry, batch) = handed.answer(root, &files, &hash).unwrap().unwrap();

    assert_eq!(
        entry,
        generation_ingested(
            "gc",
            SOURCE_PATH,
            &generation,
            &git_hash_object(root, SOURCE_PATH, false),
            false
        )
    );
    assert_eq!(entry.blob.len(), 40);
    assert_eq!(batch, handed.events());
    assert_ne!(batch, Vec::new(), "sanity: the file extracts to a batch");

    let (store, graph) = (files.store(), files.graph());
    let folding = folding_into(&store, Some(&graph as &dyn Projection), &|_| {});
    let events: Vec<Event> = handed.keyed.iter().map(|(_, e)| e.clone()).collect();
    let done = folding
        .append_entry_and_fold(STREAM, &entry.event(events.len()), events)
        .unwrap();

    assert_eq!(
        (done.fold, done.outcome),
        (Fold::Folded, Some(EntryFold::BatchAsked))
    );
    assert_eq!(
        entry_records(&files.log()),
        vec![(
            entry.clone(),
            SOURCE.to_string(),
            format!("{SOURCE}@{generation}#{}", batch.len())
        )]
    );
    assert_eq!(
        sides(),
        (Some(generation.clone()), Some(generation.clone()))
    );
    assert!(current(sides()));
    assert_eq!(handed.answer(root, &files, &unasked), Ok(None));
}

/// SINK OUTCOMES rows 10 and 11 at the public sink answer.
///
/// GIVEN a tree whose source file was deleted, and one whose committed `.gitignore` names a
/// source file it still holds,
/// WHEN an outside caller asks `ingest::entry_of_batch` what to record for the batch an
/// integration reindex naming the file hands,
/// THEN each answers the entry of the code half's batch for no bytes - one generation for both,
/// which is not the file's - with no blob and the batch the reindex handed, and the hash is never
/// asked.
#[test]
fn the_public_sink_answer_for_a_deleted_file_and_one_out_of_scope_names_no_blob_and_asks_no_hash() {
    let deleted = committed_tree();
    let with_bytes = reindexed(deleted.path()).generation();
    std::fs::remove_file(deleted.path().join(SOURCE_PATH)).unwrap();
    let ignored = committed_tree();
    write_text(ignored.path(), ".gitignore", "src/lib.rs\n");
    git_commit_all(ignored.path(), "ignore the source file");
    assert_eq!(
        std::fs::read_to_string(ignored.path().join(SOURCE_PATH)).unwrap(),
        SOURCE_BODY,
        "sanity: the out-of-scope path still holds its readable file"
    );

    let answers = [deleted.path(), ignored.path()].map(|root| {
        let handed = reindexed(root);
        let answered = handed.answer(root, &Files::new(), &unasked);
        (answered, handed.generation(), handed.events())
    });

    let no_bytes = answers[0].1.clone();
    assert_ne!(no_bytes, with_bytes);
    for (answered, generation, events) in answers {
        assert_eq!(generation, no_bytes);
        assert_eq!(
            answered,
            Ok(Some((
                generation_ingested("gc", SOURCE_PATH, &no_bytes, "", false),
                events
            )))
        );
    }
}

/// SINK OUTCOMES rows 1 and 8 at the public sink answer, the failure worded as a caller prints it.
///
/// GIVEN a committed tree and an empty `events.db` and `graph.db`,
/// WHEN an outside caller asks `ingest::entry_of_batch` about a batch whose key has no key
/// shape, one whose key names a prefix no half extracts, and no batch at all,
/// THEN each fails naming the key and asks no hash;
/// AND WHEN it asks about the reindexed source batch under `worktree::hash_blob` bound to a root
/// that does not exist, so the hash process cannot start,
/// THEN it fails naming the identity and the hash's own failure - the command and the root.
#[test]
fn the_public_sink_answer_fails_naming_a_key_with_no_identity_and_a_hash_that_cannot_start() {
    let tree = committed_tree();
    let root = tree.path();
    let files = Files::new();
    let handed = reindexed(root);
    let rekeyed = |key: &str| Handed {
        keyed: vec![(key.to_string(), handed.keyed[0].1.clone())],
        excluded: false,
    };
    let none = Handed {
        keyed: Vec::new(),
        excluded: false,
    };

    assert_eq!(
        [
            rekeyed("unshaped#0").answer(root, &files, &unasked),
            rekeyed("zz/src/lib.rs@0123456789abcdef#0").answer(root, &files, &unasked),
            none.answer(root, &files, &unasked),
        ],
        [
            Err("the batch key \"unshaped#0\" names no identity".to_string()),
            Err("the batch key \"zz/src/lib.rs@0123456789abcdef#0\" names no identity".to_string()),
            Err("the batch key \"\" names no identity".to_string()),
        ]
    );

    let nowhere = root.join("no-such-root");
    let cannot_start = |bytes: &[u8]| rigger::worktree::hash_blob(&nowhere, bytes);
    let why = cannot_start(b"").unwrap_err();
    assert!(
        why.0.starts_with(&format!(
            "git hash-object --stdin in {}: ",
            nowhere.display()
        )),
        "sanity: the hash names its command and root: {why}"
    );

    assert_eq!(
        handed.answer(root, &files, &cannot_start),
        Err(format!("the bytes of {SOURCE} could not be hashed: {why}"))
    );
    assert_eq!(entry_records(&files.log()), Vec::new());
}

/// SINK OUTCOMES row 5 at the public sink answer: THE READ FAULT.
///
/// GIVEN a committed tree whose source file an integration reindex lowered and the caller's uid
/// can then no longer read,
/// WHEN an outside caller asks `ingest::entry_of_batch` what to record for that batch,
/// THEN it fails naming the file and the read's own error, and asks no hash: an unreadable file
/// is never recorded as a file that is gone.
#[test]
fn the_public_sink_answer_fails_naming_a_read_that_fails_for_a_reason_other_than_absence() {
    let tree = committed_tree();
    let root = tree.path();
    let handed = reindexed(root);
    let file = root.join(SOURCE_PATH);
    if !arm_read_fault(&file) {
        return;
    }
    let refused = std::fs::read(&file).unwrap_err();
    assert_ne!(refused.kind(), std::io::ErrorKind::NotFound);

    assert_eq!(
        handed.answer(root, &Files::new(), &unasked),
        Err(format!("{} could not be read: {refused}", file.display()))
    );
}

/// SINK OUTCOMES rows 14 and 4 over the files.
///
/// GIVEN a committed tree and a `graph.db` that owes its rebuild,
/// WHEN a run walks the tree,
/// THEN the run succeeds and records one ledger entry per batch the walk extracts, from the
/// bytes it read; it says each entry's lost fold once, naming the debt, through its log; `graph.db`
/// takes no `applied` row and no generation for any entry and still owes its rebuild;
/// AND WHEN a second run, a fresh campaign, walks the unchanged tree, THEN it records nothing -
/// the log side alone answers for an owed graph;
/// AND WHEN the source file moves and a third run walks the tree, THEN it records exactly the
/// moved batch's entry: one entry per generation.
#[test]
fn a_run_over_a_graph_db_that_owes_its_rebuild_records_one_entry_per_generation_and_says_the_debt()
{
    let tree = committed_tree();
    let root = tree.path();
    let files = Files::new();
    owe_a_rebuild(&files.graph());
    let owed_applied = applied_positions(&files.graph_db());
    let first_walk = walked_git_entry_records(root);
    let lost = format!(
        "rigger: recorded 1 run event(s); not folded into the context graph: graph: {REBUILD_OWED}"
    );

    files.run_over(root, "first criterion");

    assert_eq!(entry_records(&files.log()), first_walk);
    let positions = files.entry_positions();
    assert_eq!(applied_positions(&files.graph_db()), owed_applied);
    assert_eq!(
        held_generations(&files.graph(), &identities(&first_walk)),
        vec![None; first_walk.len()]
    );
    assert!(files.graph().rebuild_owed().unwrap());
    let said = || files.said.lock().unwrap().len();
    let said_first = said();

    files.run_over(root, "second criterion");

    assert_eq!(entry_records(&files.log()), first_walk);
    assert_eq!(files.entry_positions(), positions);
    let said_without_an_entry = said() - said_first;

    write_text(root, SOURCE_PATH, &source_with(MOVED));
    let moved = entry_under(&walked_git_entry_records(root), SOURCE);
    assert_ne!(moved, entry_under(&first_walk, SOURCE));
    files.run_over(root, "third criterion");

    assert_eq!(
        entry_records(&files.log()),
        [first_walk, vec![moved]].concat()
    );
    assert_eq!(applied_positions(&files.graph_db()), owed_applied);
    // Every append of a run over an owed graph is a lost fold the run says; the second run
    // recorded no entry, so what it said is what a run says without one.
    let said_third = said() - said_first - said_without_an_entry;
    assert_eq!(
        [
            said_first - said_without_an_entry,
            said_third - said_without_an_entry
        ],
        [positions.len(), 1],
        "each entry's lost fold is said once, through the run's log"
    );
    assert_eq!(
        *files.said.lock().unwrap(),
        vec![lost; said_first + said_without_an_entry + said_third]
    );
}

// THE RUN'S SINK MEMOIZES THE LOG SIDE (spec 107 criterion 11), at the one seam an outside caller
// reaches it: a whole `conductor::run` hands an identity's batch to its sink more than once in one
// process when its whole-tree walk is followed by the reindex of an integration naming the file.

/// A driver whose every agent writes the source file's body it is listed with into its worktree.
struct SourceWriters(Vec<(&'static str, String)>);

impl AgentDriver for SourceWriters {
    fn spawn(
        &self,
        agent: &AgentDef,
        _prompt: &str,
        opts: &SpawnOpts,
        _emit: &dyn Fn(&str, serde_json::Value) -> Result<(), Error>,
    ) -> Result<AgentResult, Error> {
        let (_, body) = self
            .0
            .iter()
            .find(|(id, _)| *id == agent.id)
            .expect("every spawned agent is a listed writer");
        write_text(Path::new(&opts.dir), SOURCE_PATH, body);
        Ok(AgentResult::default())
    }
}

/// A workflow over the repository at `root` of one unit per `(name, needs)`, in order: each run by
/// the agent of its own name after the unit it needs, under a gate that passes, and merged when
/// it passes - so each lands and its integration reindexes the files it changed.
fn landing(root: &Path, units: &[(&str, Option<&str>)]) -> Config {
    let mut cfg = Config::default();
    cfg.workflow.defaults.workdir = common::isolated_workdir(root);
    cfg.workflow.gates.insert("ok".into(), gate_def("true"));
    for (name, needs) in units {
        cfg.agents.insert((*name).into(), agent(name));
        cfg.workflow.stages.insert(
            (*name).into(),
            Stage {
                name: (*name).into(),
                agent: (*name).into(),
                needs: needs.iter().map(|unit| unit.to_string()).collect(),
                coverage: "core".into(),
                gates: vec!["ok".into()],
                on_pass: "merge".into(),
                ..Default::default()
            },
        );
    }
    cfg
}

/// Every group lookup among `reads`, in call order.
fn group_lookups(reads: &[CountedRead]) -> Vec<CountedRead> {
    reads
        .iter()
        .filter(|read| matches!(read, CountedRead::LatestInGroup { .. }))
        .cloned()
        .collect()
}

/// ONE process walks the tree, lands the source file at its moved body and lands it back.
///
/// WHEN a single run over `files` and the committed tree at `root` walks the whole tree, then
/// integrates a unit that rewrites the source file to its moved body and a second unit that
/// rewrites it back - each integration reindexing the file, so the file's code identity and its
/// design identity are handed to the run's sink three times in the one process,
/// THEN the store the run was handed is asked one group lookup per walked identity, in walk
/// order, and no other: both reindexes are answered from the process's memo. The log holds the
/// entries it held before, then `walk_records`, then the moved body's entry and the first
/// body's again under the code identity - the revert recorded because the memo took the moved
/// entry's generation - and no second entry under the design identity, whose generation never
/// moved; both sides hold the first body's generation.
/// AND WHEN a second run, a fresh process's memo, walks the tree, THEN it asks one group lookup
/// per identity again and records nothing.
fn one_run_walks_then_lands_the_moved_body_and_its_revert(
    files: &Files,
    root: &Path,
    walk_records: Vec<EntryRecord>,
) {
    let first_walk = walked_git_entry_records(root);
    let a = entry_under(&first_walk, SOURCE);
    let b = {
        let moved = committed_tree();
        write_text(moved.path(), SOURCE_PATH, &source_with(MOVED));
        entry_under(&walked_git_entry_records(moved.path()), SOURCE)
    };
    assert_ne!(a.0.generation, b.0.generation);
    let before = entry_records(&files.log());
    let cfg = landing(root, &[("moves", None), ("reverts", Some("moves"))]);
    let driver = SourceWriters(vec![
        ("moves", source_with(MOVED)),
        ("reverts", SOURCE_BODY.to_string()),
    ]);
    let store = files.store();
    let counted = ReadCountingStore::new(&store);

    let state = files.run_through(root, &cfg, &[], &driver, &counted);

    assert_eq!(
        [&state.units["moves"].status, &state.units["reverts"].status],
        [&Status::Integrated, &Status::Integrated],
        "sanity: both units landed, so both reindexes ran"
    );
    assert_eq!(
        std::fs::read_to_string(root.join(SOURCE_PATH)).unwrap(),
        SOURCE_BODY,
        "sanity: the tree is back at its first body"
    );
    let asked = one_lookup_each(STREAM, &identities(&first_walk));
    assert_eq!(group_lookups(&counted.reads()), asked);
    let recorded = [before, walk_records, vec![b, a]].concat();
    assert_eq!(entry_records(&files.log()), recorded);
    let walked_generations = generations(&first_walk);
    assert_eq!(
        held_generations(&files.graph(), &identities(&first_walk)),
        walked_generations
    );
    assert_eq!(
        logged_generations(&files.store(), STREAM, &identities(&first_walk)),
        walked_generations
    );
    assert_eq!(*files.said.lock().unwrap(), Vec::<String>::new());

    let store = files.store();
    let counted = ReadCountingStore::new(&store);

    files.run_campaign(root, "later criterion", &counted);

    assert_eq!(group_lookups(&counted.reads()), asked);
    assert_eq!(entry_records(&files.log()), recorded);
}

/// GIVEN a committed tree and an empty `events.db` and `graph.db`, so every group lookup of the
/// walk answers no generation and the memo takes each recorded entry's.
#[test]
fn one_run_asks_the_group_lookup_once_per_identity_across_its_walk_and_two_reindexes() {
    let tree = committed_tree();
    let root = tree.path();
    one_run_walks_then_lands_the_moved_body_and_its_revert(
        &Files::new(),
        root,
        walked_git_entry_records(root),
    );
}

/// GIVEN a committed tree an earlier run, another process, already recorded, so every group
/// lookup of the walk answers the generation both sides hold, the walk records nothing and the
/// memo holds only what the lookups answered.
#[test]
fn one_run_over_a_recorded_tree_answers_both_reindexes_from_what_its_walk_looked_up() {
    let tree = committed_tree();
    let root = tree.path();
    let files = Files::new();
    files.run_over(root, "first criterion");
    assert_eq!(entry_records(&files.log()), walked_git_entry_records(root));

    one_run_walks_then_lands_the_moved_body_and_its_revert(&files, root, Vec::new());
}

/// SINK OUTCOMES row 13's fixture in one run.
///
/// GIVEN a store recorded before the ledger and before the group stamp, so the group lookup
/// answers no generation for any identity while `graph.db` holds each one's: the memo takes each
/// entry's generation in place of the lookup's empty answer.
#[test]
fn one_run_over_pre_ledger_rows_asks_the_group_lookup_once_per_identity_across_its_reindexes() {
    let tree = committed_tree();
    let files = Files::new();
    seed_pre_ledger_rows_without_a_group(tree.path(), &files.store(), &files.graph());
    assert_eq!(
        logged_generations(
            &files.store(),
            STREAM,
            &identities(&walked_git_entry_records(tree.path()))
        ),
        vec![None; 5],
        "premise: the group lookup answers no generation"
    );

    one_run_walks_then_lands_the_moved_body_and_its_revert(
        &files,
        tree.path(),
        walked_git_entry_records(tree.path()),
    );
}

/// A driver whose one agent, before it writes [`REWORDED`] into its worktree, rebuilds the
/// run's `graph.db` from the log as it stands, re-extracting each entry from the tree at
/// `unresolving` with no object database to ask, and keeps the generation the rebuilt graph
/// holds for each of `identities`.
struct RebuildsThenRewords<'a> {
    files: &'a Files,
    unresolving: &'a Path,
    identities: Vec<String>,
    held_after_rebuild: Mutex<Vec<Vec<Option<String>>>>,
}

impl AgentDriver for RebuildsThenRewords<'_> {
    fn spawn(
        &self,
        _agent: &AgentDef,
        _prompt: &str,
        opts: &SpawnOpts,
        _emit: &dyn Fn(&str, serde_json::Value) -> Result<(), Error>,
    ) -> Result<AgentResult, Error> {
        rebuild_from_the_tree(&self.files.graph_db(), &self.files.log(), self.unresolving);
        let identities: Vec<&str> = self.identities.iter().map(String::as_str).collect();
        self.held_after_rebuild
            .lock()
            .unwrap()
            .push(held_generations(&self.files.graph(), &identities));
        write_text(Path::new(&opts.dir), SOURCE_PATH, &source_with(REWORDED));
        Ok(AgentResult::default())
    }
}

/// A LONG-LIVED RUN RESTORES AN IDENTITY A REBUILD LEFT BEHIND, at the run's own seam.
///
/// GIVEN one run whose whole-tree walk recorded every file, and a rebuild of `graph.db` made
/// while that run is alive that could resolve no source for the source file's code entry - the
/// tree it read held another body and it had no object database to ask - and so left that one
/// identity behind,
/// WHEN the same run then integrates a unit that rewords the file's rationale line, so the
/// reindex names the file and hands its code batch at the generation the run already recorded,
/// THEN the store is asked no group lookup past the walk's one per identity - the memo answers
/// the log side - and the run still records the code identity's entry again, at the same
/// generation and the reworded bytes' blob, because the graph's side is read on every batch;
/// the design identity's moved batch records its entry after it; and `graph.db` holds every
/// identity's generation again.
#[test]
fn a_run_restores_an_identity_a_rebuild_left_behind_at_its_next_reindex_with_no_group_lookup() {
    let tree = committed_tree();
    let root = tree.path();
    let first_walk = walked_git_entry_records(root);
    let all = identities(&first_walk);
    let a = entry_under(&first_walk, SOURCE);
    let moved = committed_tree();
    write_text(moved.path(), SOURCE_PATH, &source_with(MOVED));
    let reworded = committed_tree();
    write_text(reworded.path(), SOURCE_PATH, &source_with(REWORDED));
    let reindexed: Vec<EntryRecord> = walked_git_entry_records(reworded.path())
        .into_iter()
        .filter(|(entry, ..)| entry.file == SOURCE_PATH)
        .collect();
    assert_eq!(identities(&reindexed), vec![SOURCE, "gd/src/lib.rs"]);
    assert_eq!(
        (
            reindexed[0].0.generation == a.0.generation,
            reindexed[0].0.blob == a.0.blob,
            reindexed[1] == entry_under(&first_walk, "gd/src/lib.rs"),
        ),
        (true, false, false),
        "premise: the reworded body keeps the code generation under other bytes and moves the \
         design batch"
    );
    let files = Files::new();
    let cfg = landing(root, &[("rewords", None)]);
    let driver = RebuildsThenRewords {
        files: &files,
        unresolving: moved.path(),
        identities: all.iter().map(|identity| identity.to_string()).collect(),
        held_after_rebuild: Mutex::new(Vec::new()),
    };
    let store = files.store();
    let counted = ReadCountingStore::new(&store);

    let state = files.run_through(root, &cfg, &[], &driver, &counted);

    assert_eq!(state.units["rewords"].status, Status::Integrated);
    let left_behind: Vec<Option<String>> = first_walk
        .iter()
        .map(|(entry, group, _)| Some(entry.generation.clone()).filter(|_| group != SOURCE))
        .collect();
    assert_eq!(
        *driver.held_after_rebuild.lock().unwrap(),
        vec![left_behind],
        "premise: the one rebuild left the source file's code identity behind, and no other"
    );
    assert_eq!(
        group_lookups(&counted.reads()),
        one_lookup_each(STREAM, &all)
    );
    assert_eq!(
        entry_records(&files.log()),
        [first_walk.clone(), reindexed.clone()].concat()
    );
    assert_eq!(
        held_generations(&files.graph(), &all),
        generations(&walked_git_entry_records(reworded.path())),
        "the graph holds the left-behind identity's generation again, and the moved design one"
    );
    assert_eq!(
        held_generations(&files.graph(), &[SOURCE]),
        vec![Some(a.0.generation)]
    );
}
