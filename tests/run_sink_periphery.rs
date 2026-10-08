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

#![cfg(feature = "symbols")]

mod common;

use std::path::Path;
use std::sync::Mutex;

use common::cli::applied_positions;
use common::fixtures::{
    agent, arm_read_fault, entry_records, generation_ingested, git_commit_all, git_hash_object,
    live_edges, minted_events, temp_git_project_with_commit, walked_entry_events, wire_owned,
    write_text, Handed, NoopDriver, DOCUMENT_BODY, DOCUMENT_PATH, SOURCE_BODY, SOURCE_PATH,
    TEST_MODULE_BODY, TEST_MODULE_PATH,
};
use rigger::conductor::{run, Deps, STREAM};
use rigger::config::{Config, Stage};
use rigger::contextgraph::sqlite::Projector;
use rigger::contextgraph::{wired, EntryFold, Fold, Projection, REBUILD_OWED};
use rigger::eventstore::sqlite::Store;
use rigger::eventstore::{Direction, Event, EventStore, ExpectedRevision, META_GROUP};
use rigger::gate::ExecRunner;
use rigger::ingest::{
    batch_is_current, derived_key_parts, entry_of_batch, folding_into, ingest_files_batched,
    is_derived_index_type, keyed_derived_event, latest_generation, EntryFailure, GraphSide,
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

/// The identity of the source file's code batch, the one identity [`SOURCE_MOVED`] moves.
const SOURCE: &str = "gc/src/lib.rs";

/// [`SOURCE_BODY`] with its helper renamed: a second body of the source file that keeps its
/// rationale line and its test-module declaration, so only the file's code batch moves.
fn source_moved() -> String {
    SOURCE_BODY.replace("helper", "assistant")
}

/// The one entry of `recorded` under `identity`.
fn entry_under(recorded: &[Recorded], identity: &str) -> Recorded {
    let under: Vec<&Recorded> = recorded
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
    let first_walk = walked(root);
    let a = entry_under(&first_walk, SOURCE);

    files.run_over(root, "first criterion");
    let facts_a = facts(&files.graph());

    write_text(root, SOURCE_PATH, &source_moved());
    let b = entry_under(&walked(root), SOURCE);
    let keys = |recorded: &[Recorded]| -> Vec<String> {
        recorded.iter().map(|(.., key)| key.clone()).collect()
    };
    assert_eq!(
        keys(&walked(root))
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
    assert_eq!(files.held(&[SOURCE]), vec![Some(b.0.generation.clone())]);

    write_text(root, SOURCE_PATH, SOURCE_BODY);
    assert_eq!(walked(root), first_walk, "sanity: the tree is back at A");
    files.run_over(root, "third criterion");

    let recorded = entry_records(&files.log());
    assert_eq!(
        recorded,
        [first_walk.clone(), vec![b.clone(), a.clone()]].concat()
    );
    let positions = files.entry_positions();
    assert_eq!(positions.len(), first_walk.len() + 2);
    assert_eq!(files.applied_among(&positions), positions);
    assert_eq!(files.held(&[SOURCE]), vec![Some(a.0.generation.clone())]);
    assert_eq!(files.logged(&[SOURCE]), vec![Some(a.0.generation.clone())]);
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
            files.logged(&[SOURCE]).remove(0),
            files.held(&[SOURCE]).remove(0),
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
    {
        // The public fold refuses a ledger entry and marks the graph owing.
        let graph = files.graph();
        let mut stray = generation_ingested("gc", "src/stray.rs", "h0", "", false).event(1);
        stray.position = 1;
        assert_ne!(
            Fold::of(wired(Some(&graph as &dyn Projection)), &stray),
            Fold::Folded
        );
        assert!(graph.rebuild_owed().unwrap());
    }
    let owed_applied = applied_positions(&files.graph_db());
    let first_walk = walked(root);
    let lost = format!(
        "rigger: recorded 1 run event(s); not folded into the context graph: graph: {REBUILD_OWED}"
    );

    files.run_over(root, "first criterion");

    assert_eq!(entry_records(&files.log()), first_walk);
    let positions = files.entry_positions();
    assert_eq!(applied_positions(&files.graph_db()), owed_applied);
    assert_eq!(
        files.held(&identities(&first_walk)),
        vec![None; first_walk.len()]
    );
    assert!(files.graph().rebuild_owed().unwrap());
    let said = || files.said.lock().unwrap().len();
    let said_first = said();

    files.run_over(root, "second criterion");

    assert_eq!(entry_records(&files.log()), first_walk);
    assert_eq!(files.entry_positions(), positions);
    let said_without_an_entry = said() - said_first;

    write_text(root, SOURCE_PATH, &source_moved());
    let moved = entry_under(&walked(root), SOURCE);
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
